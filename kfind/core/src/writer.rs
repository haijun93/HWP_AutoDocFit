//! 쓰기 쪽: 문서 추가·수정·삭제를 새 조각과 "지운 문장" 비트맵으로 반영하고, 조각 합치기를 배경에서 할 수 있게 나눈다.
//!
//! 쓰기 상태(한 폴더에 쓰는 쪽은 하나, `LOCK` 파일로 보장):
//! - 대기(pending): `add_document`·`delete_document`가 메모리에만 반영한 상태. 읽는 쪽에는 보이지 않는다.
//! - 커밋(commit): 대기 문서를 새 조각 파일로 쓰고 목록 파일을 원자적으로 교체. 이때부터 읽는 쪽에 보인다.
//! - 합치기: `plan_merge`(목록 스냅숏) → `MergeJob::run`(다른 스레드, 쓰기 잠금 없이) → `apply_merge`(커밋).
//!   합치는 동안 들어온 삭제는 `apply_merge`가 새 조각 번호로 옮겨 적는다. 합치기는 한 번에 하나만.
use crate::docmap::{DocumentMeta, detect_sentence_role, extract_folder};
use crate::fold::fold_into;
use crate::index::load_views;
use crate::manifest::{MANIFEST_NAME, MANIFEST_TMP_PREFIX, Manifest, SegEntry};
use crate::segment::{MergedSource, PendingDoc, SegMeta, Segment, merge_segments, seg_file_name, write_segment};
use crate::topic::{TopicTable, TOPICS_TMP_PREFIX, extract_body_terms, extract_path_terms, extract_title_terms};
use crate::{Error, Result, corrupt, guard};
use roaring::RoaringBitmap;
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const LOCK_NAME: &str = "LOCK";

/// 색인에 넣을 문장 하나. 문장 나누기는 호출하는 쪽이 한다.
#[derive(Clone, Copy, Debug)]
pub struct Sentence<'a> {
    /// 문단 번호(원문 위치를 찾는 데 쓴다).
    pub para: u32,
    /// 위계 순위(작을수록 높은 위계).
    pub level: u8,
    pub text: &'a str,
}

#[derive(Clone, Debug)]
pub struct WriterOptions {
    /// 대기 문장이 이 수를 넘으면 `add_document`가 스스로 커밋한다(메모리 상한).
    pub max_pending_sentences: usize,
}

impl Default for WriterOptions {
    fn default() -> Self {
        WriterOptions { max_pending_sentences: 200_000 }
    }
}

#[derive(Clone, Debug)]
pub struct MergePolicy {
    /// 조각 수가 이보다 많으면 합친다.
    pub max_segments: usize,
    /// 지운 문장 비율이 이보다 큰 조각은 다시 써서 줄인다.
    pub deleted_ratio: f32,
}

impl Default for MergePolicy {
    fn default() -> Self {
        MergePolicy { max_segments: 8, deleted_ratio: 0.3 }
    }
}

/// 합칠 조각의 스냅숏. `run`은 쓰기 쪽을 잡지 않으므로 다른 스레드에서 돌릴 수 있다.
pub struct MergeJob {
    dir: PathBuf,
    new_id: u64,
    sources: Vec<(Arc<Segment>, RoaringBitmap)>,
}

/// 합치기 결과. `Writer::apply_merge`에 넘긴다.
pub struct MergeOutput {
    new_id: u64,
    /// 새 조각과 (작으면) 그 메모리 사본.
    meta: Option<(SegMeta, Option<Vec<u8>>)>,
    sources: Vec<MergedSource>,
}

impl MergeJob {
    /// 합칠 조각 번호.
    pub fn source_ids(&self) -> Vec<u64> {
        self.sources.iter().map(|(s, _)| s.id).collect()
    }

    /// 새 조각 파일을 쓴다(목록 파일은 건드리지 않는다).
    pub fn run(self) -> Result<MergeOutput> {
        guard(|| {
            let (meta, sources) = merge_segments(&self.dir, self.new_id, &self.sources)?;
            Ok(MergeOutput { new_id: self.new_id, meta, sources })
        })
    }
}

pub struct Writer {
    dir: PathBuf,
    _lock: File,
    opts: WriterOptions,
    manifest: Manifest,
    /// manifest.segs와 같은 순서.
    segs: Vec<Arc<Segment>>,
    /// 커밋된 문서 → (조각 번호, 조각 안 문서 번호).
    docs: HashMap<u64, (u64, u32)>,
    pending: Vec<Option<PendingDoc>>,
    pending_ix: HashMap<u64, usize>,
    pending_sentences: usize,
    /// 목록에 아직 쓰지 않은 삭제가 있다.
    dirty: bool,
    /// 진행 중인 합치기의 새 조각 번호.
    merging: Option<u64>,
    /// 주제 빈도 순위표 (SEMI3).
    pub topics: TopicTable,
}

/// 쓰는 쪽이 방금 쓴 조각을 연다: 메모리 사본이 있으면 그것으로(새 파일을 디스크에서 다시 열고 매핑하는 비용이
/// 문서 1개 커밋의 절반 이상이었다, KB3), 없으면(큰 조각) 파일을 매핑한다. 읽는 쪽은 언제나 파일을 매핑한다.
fn open_written(dir: &Path, id: u64, copy: Option<Vec<u8>>) -> Result<Segment> {
    let path = dir.join(seg_file_name(id));
    match copy {
        Some(bytes) => {
            warm_file(path.clone());
            Segment::from_bytes(bytes, &path)
        }
        None => Segment::open(&path),
    }
}

/// 방금 제 이름을 얻은 조각 파일을 배경 스레드에서 한 번 열고 닫는다(KB4). 이 PC에서 새로 쓴 파일의 첫 열기는
/// 누가 열든 한 번 약 15ms가 들고(열기 호출 자체, 백신 검사로 추정) 두 번째부터는 0.2ms 안팎이다. 메모리 사본으로
/// 연 조각은 쓰는 쪽이 파일을 열지 않아 그 비용을 읽는 쪽 `reload`가 냈다. 커밋의 나머지(목록 저장)와 겹쳐 미리 낸다.
/// 실패는 무시한다(읽는 쪽이 열 때 같은 오류를 다시 만난다).
fn warm_file(path: PathBuf) {
    let _ = std::thread::Builder::new().name("kfind-warm".into()).spawn(move || {
        let _ = File::open(&path);
    });
}

fn index_docs(manifest: &Manifest, segs: &[Arc<Segment>]) -> Result<HashMap<u64, (u64, u32)>> {
    let mut docs = HashMap::new();
    for (e, seg) in manifest.segs.iter().zip(segs) {
        for local in 0..seg.n_docs {
            let rec = seg.doc(local)?;
            if rec.n_sent > 0 && !e.deleted.contains(rec.first_sid) && docs.insert(rec.doc_id, (seg.id, local)).is_some() {
                return corrupt(format!("document {} is live in two segments", rec.doc_id));
            }
        }
    }
    Ok(docs)
}

impl Writer {
    /// 색인 폴더를 쓰기용으로 연다(없으면 만든다). 다른 쓰기 쪽이 있으면 `Error::Locked`,
    /// 형식·규칙 버전이 다르면 `Error::FormatVersion`·`Error::RuleVersion`(이때는 `Writer::create`로 새로 만든다).
    pub fn open(dir: &Path, opts: WriterOptions) -> Result<Writer> {
        guard(|| {
            let lock = Self::lock(dir)?;
            Self::open_locked(dir, lock, opts)
        })
    }

    /// 있던 색인을 지우고 빈 색인으로 연다(규칙 버전이 바뀌어 다시 색인할 때).
    pub fn create(dir: &Path, opts: WriterOptions) -> Result<Writer> {
        guard(|| {
            let lock = Self::lock(dir)?;
            // 목록 파일부터 지운다: 읽는 쪽은 이후 빈 색인으로 본다.
            match std::fs::remove_file(dir.join(MANIFEST_NAME)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            Self::open_locked(dir, lock, opts)
        })
    }

    fn lock(dir: &Path) -> Result<File> {
        std::fs::create_dir_all(dir)?;
        let lock = File::options().create(true).truncate(false).write(true).open(dir.join(LOCK_NAME))?;
        match lock.try_lock() {
            Ok(()) => Ok(lock),
            Err(std::fs::TryLockError::WouldBlock) => Err(Error::Locked),
            Err(std::fs::TryLockError::Error(e)) => Err(e.into()),
        }
    }

    fn open_locked(dir: &Path, lock: File, opts: WriterOptions) -> Result<Writer> {
        let (manifest, segs) = load_views(dir, &[])?;
        let docs = index_docs(&manifest, &segs)?;
        let topics = TopicTable::load(dir).unwrap_or_default();
        let w = Writer {
            dir: dir.to_path_buf(),
            _lock: lock,
            opts,
            manifest,
            segs,
            docs,
            pending: Vec::new(),
            pending_ix: HashMap::new(),
            pending_sentences: 0,
            dirty: false,
            merging: None,
            topics,
        };
        w.collect_garbage();
        Ok(w)
    }

    /// 마지막으로 커밋한 세대.
    pub fn generation(&self) -> u64 {
        self.manifest.generation
    }

    pub fn num_segments(&self) -> usize {
        self.manifest.segs.len()
    }

    /// 대기 중인(아직 커밋하지 않은) 문장 수.
    pub fn pending_sentences(&self) -> usize {
        self.pending_sentences
    }

    /// 커밋된 조각이나 대기 중에서 문서를 지운다. 있었으면 true.
    fn remove_existing(&mut self, doc_id: u64) -> Result<bool> {
        let mut existed = false;
        if let Some(i) = self.pending_ix.remove(&doc_id) {
            if let Some(d) = self.pending[i].take() {
                self.pending_sentences -= d.sent_end.len();
            }
            existed = true;
        }
        if let Some((seg_id, local)) = self.docs.remove(&doc_id) {
            let pos = self.manifest.segs.iter().position(|e| e.id == seg_id).ok_or_else(|| Error::Corrupt(format!("document {doc_id} points at a missing segment")))?;
            let rec = self.segs[pos].doc(local)?;
            self.manifest.segs[pos].deleted.insert_range(rec.first_sid..rec.first_sid + rec.n_sent);
            self.manifest.segs[pos].doc_deleted.insert(local);
            self.dirty = true;
            existed = true;
        }
        if existed {
            self.topics.remove_document(doc_id);
        }
        Ok(existed)
    }

    /// 문서를 넣는다. 같은 ID의 문서가 있으면 바꾼다. 문장이 없으면 삭제와 같다. 커밋해야 읽는 쪽에 보인다.
    /// `date`는 순위의 날짜 비교에 쓰는 값(클수록 최신).
    pub fn add_document(&mut self, doc_id: u64, date: i64, sentences: &[Sentence<'_>]) -> Result<()> {
        self.add_document_with_meta(doc_id, date, &DocumentMeta::default(), sentences)
    }

    /// 메타데이터(경로, 파일 형식 등)와 함께 문서를 넣는다.
    pub fn add_document_with_meta(&mut self, doc_id: u64, date: i64, meta: &DocumentMeta, sentences: &[Sentence<'_>]) -> Result<()> {
        // 비교형 만들기(외부 글을 다루는 부분)를 패닉 경계 안에서 먼저 끝내고, 성공했을 때만 상태를 바꾼다.
        let doc = guard(|| {
            let mut d = PendingDoc {
                doc_id,
                date,
                path: meta.path.clone(),
                file_type: meta.file_type.clone(),
                text: String::new(),
                sent_end: Vec::with_capacity(sentences.len()),
                tok_start: Vec::new(),
                para: Vec::with_capacity(sentences.len()),
                level: Vec::with_capacity(sentences.len()),
                roles: Vec::with_capacity(sentences.len()),
            };
            for s in sentences {
                for tok in s.text.split(char::is_whitespace) {
                    let before = d.text.len();
                    fold_into(tok, &mut d.text);
                    if d.text.len() > before {
                        d.tok_start.push(before as u32);
                    }
                }
                if d.text.len() >= u32::MAX as usize {
                    return Err(Error::Invalid(format!("document {doc_id} is too large")));
                }
                d.sent_end.push(d.text.len() as u32);
                d.para.push(s.para);
                d.level.push(s.level);
                d.roles.push(detect_sentence_role(s.level, s.para, s.text) as u8);
            }
            Ok(d)
        })?;

        // 주제 낱말(파일명 + 제목/소제목) 및 본문 낱말 추출 (SEMI3)
        let mut title_terms = Vec::new();
        if let Some(path) = &meta.path {
            title_terms.extend(extract_path_terms(path));
        }
        let mut body_terms = Vec::new();
        for s in sentences {
            let role = detect_sentence_role(s.level, s.para, s.text);
            if s.level <= 1 || role == crate::docmap::SentenceRole::Title {
                title_terms.extend(extract_title_terms(s.text));
            } else {
                body_terms.extend(extract_body_terms(s.text));
            }
        }
        let folder = meta.path.as_deref().map(extract_folder).unwrap_or_default();

        guard(|| {
            self.remove_existing(doc_id)?;
            self.topics.add_document(doc_id, date, folder, title_terms, body_terms);
            if !doc.sent_end.is_empty() {
                self.pending_sentences += doc.sent_end.len();
                self.pending_ix.insert(doc_id, self.pending.len());
                self.pending.push(Some(doc));
            }
            if self.pending_sentences >= self.opts.max_pending_sentences {
                self.commit_inner()?;
            }
            Ok(())
        })
    }

    /// 문서를 지운다. 있었으면 true. 커밋해야 읽는 쪽에 보인다.
    pub fn delete_document(&mut self, doc_id: u64) -> Result<bool> {
        guard(|| self.remove_existing(doc_id))
    }

    /// 대기 중인 추가·삭제를 디스크에 반영하고 새 세대 번호를 돌려준다. 바뀐 것이 없으면 아무것도 쓰지 않는다.
    /// 실패하면 이 Writer는 버리고 다시 열어야 한다(디스크의 이전 세대는 그대로 유효하다).
    pub fn commit(&mut self) -> Result<u64> {
        guard(|| self.commit_inner())
    }

    fn commit_inner(&mut self) -> Result<u64> {
        let has_docs = self.pending.iter().any(|d| d.is_some());
        if !has_docs && !self.dirty {
            self.pending.clear();
            self.pending_ix.clear();
            return Ok(self.manifest.generation);
        }
        if has_docs {
            let id = self.manifest.next_seg;
            self.manifest.next_seg += 1;
            let (meta, copy) = write_segment(&self.dir, id, self.pending.iter().flatten())?;
            let seg = Arc::new(open_written(&self.dir, id, copy)?);
            let mut local = 0u32;
            for d in self.pending.iter().flatten() {
                self.docs.insert(d.doc_id, (id, local));
                local += 1;
            }
            self.manifest.segs.push(SegEntry { id, n_sent: meta.n_sent, n_docs: meta.n_docs, body_hash: meta.body_hash, deleted: RoaringBitmap::new(), doc_deleted: RoaringBitmap::new() });
            self.segs.push(seg);
        }
        self.pending.clear();
        self.pending_ix.clear();
        self.pending_sentences = 0;
        // 문장이 모두 지워진 조각은 목록에서 뺀다.
        let mut i = 0;
        while i < self.manifest.segs.len() {
            if self.manifest.segs[i].deleted.len() == self.manifest.segs[i].n_sent as u64 && self.merging.is_none() {
                self.manifest.segs.remove(i);
                self.segs.remove(i);
            } else {
                i += 1;
            }
        }
        self.manifest.generation += 1;
        self.manifest.save(&self.dir)?;
        self.topics.save(&self.dir)?;
        self.dirty = false;
        self.collect_garbage();
        Ok(self.manifest.generation)
    }

    /// 목록에 없는 조각 파일과 남은 임시 파일을 지운다. 다른 프로세스가 매핑 중이라 못 지우면 다음 커밋 때 다시 시도한다.
    fn collect_garbage(&self) {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return };
        let live: Vec<String> = self.manifest.segs.iter().map(|e| seg_file_name(e.id)).collect();
        let merging = self.merging.map(seg_file_name);
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let stale = if name.starts_with(MANIFEST_TMP_PREFIX) || name.starts_with(TOPICS_TMP_PREFIX) {
                true
            } else if name.starts_with("seg-") {
                let base = name.strip_suffix(".tmp").unwrap_or(&name);
                base.ends_with(".kfs") && !live.iter().any(|l| l == base) && merging.as_deref() != Some(base)
            } else {
                false
            };
            if stale {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    /// 합칠 조각을 정한다. 합칠 것이 없거나 이미 합치는 중이면 None.
    pub fn plan_merge(&mut self, policy: &MergePolicy) -> Option<MergeJob> {
        if self.merging.is_some() {
            return None;
        }
        let entries = &self.manifest.segs;
        let mut pick: Vec<usize> = Vec::new();
        if entries.len() > policy.max_segments.max(1) {
            // 가장 큰 조각이 나머지 합의 4배보다 크면 그것은 두고 작은 것들만 합친다.
            let live = |e: &SegEntry| e.n_sent as u64 - e.deleted.len();
            let total: u64 = entries.iter().map(live).sum();
            let (big, big_live) = entries.iter().enumerate().map(|(i, e)| (i, live(e))).max_by_key(|x| x.1).unwrap_or((0, 0));
            let skip_big = big_live > 4 * (total - big_live) && entries.len() > 2;
            pick = (0..entries.len()).filter(|&i| !(skip_big && i == big)).collect();
        }
        for (i, e) in entries.iter().enumerate() {
            if e.n_sent > 0 && e.deleted.len() as f32 / e.n_sent as f32 > policy.deleted_ratio && !pick.contains(&i) {
                pick.push(i);
            }
        }
        if pick.is_empty() {
            return None;
        }
        pick.sort_unstable();
        Some(self.start_merge(&pick))
    }

    /// 모든 조각을 하나로 합치는 작업. 조각이 하나뿐이고 지운 문장이 없으면 None.
    pub fn plan_merge_all(&mut self) -> Option<MergeJob> {
        if self.merging.is_some() || self.manifest.segs.is_empty() {
            return None;
        }
        if self.manifest.segs.len() == 1 && self.manifest.segs[0].deleted.is_empty() {
            return None;
        }
        let all: Vec<usize> = (0..self.manifest.segs.len()).collect();
        Some(self.start_merge(&all))
    }

    fn start_merge(&mut self, pick: &[usize]) -> MergeJob {
        let new_id = self.manifest.next_seg;
        self.manifest.next_seg += 1;
        self.merging = Some(new_id);
        MergeJob { dir: self.dir.clone(), new_id, sources: pick.iter().map(|&i| (self.segs[i].clone(), self.manifest.segs[i].deleted.clone())).collect() }
    }

    /// 실패한 합치기를 정리한다(`MergeJob::run`이 오류를 냈을 때).
    pub fn abort_merge(&mut self) {
        self.merging = None;
        self.collect_garbage();
    }

    /// 합치기 결과를 목록에 반영하고 커밋한다(대기 중인 추가·삭제도 함께 커밋된다). 새 세대 번호를 돌려준다.
    pub fn apply_merge(&mut self, out: MergeOutput) -> Result<u64> {
        guard(|| {
            if self.merging != Some(out.new_id) {
                return Err(Error::Invalid("merge output does not belong to the merge in progress".into()));
            }
            // 합치는 동안 새로 지워진 문장을 새 조각의 번호로 옮긴다.
            let mut deleted = RoaringBitmap::new();
            let mut positions = Vec::with_capacity(out.sources.len());
            for src in &out.sources {
                let pos = self.manifest.segs.iter().position(|e| e.id == src.id).ok_or_else(|| Error::Corrupt(format!("merged segment {} is no longer listed", src.id)))?;
                positions.push(pos);
                let newly = &self.manifest.segs[pos].deleted - &src.deleted;
                for sid in newly {
                    // 합칠 때 살아 있던 문장 중 sid보다 앞선 것의 수 = sid - (지워져 있던 것 중 sid 이하의 수).
                    deleted.insert(src.base + (sid - src.deleted.rank(sid) as u32));
                }
            }
            let opened = match out.meta {
                Some((meta, copy)) => Some((SegEntry { id: meta.id, n_sent: meta.n_sent, n_docs: meta.n_docs, body_hash: meta.body_hash, deleted, doc_deleted: RoaringBitmap::new() }, Arc::new(open_written(&self.dir, meta.id, copy)?))),
                None => None,
            };
            let at = positions.iter().copied().min().unwrap_or(0);
            positions.sort_unstable_by(|a, b| b.cmp(a));
            for pos in positions {
                self.manifest.segs.remove(pos);
                self.segs.remove(pos);
            }
            if let Some((entry, seg)) = opened {
                let at = at.min(self.manifest.segs.len());
                self.manifest.segs.insert(at, entry);
                self.segs.insert(at, seg);
            }
            self.merging = None;
            self.docs = index_docs(&self.manifest, &self.segs)?;
            self.dirty = true;
            self.commit_inner()
        })
    }

    /// 정책에 따라 지금 스레드에서 합친다. 합쳤으면 true.
    pub fn merge(&mut self, policy: &MergePolicy) -> Result<bool> {
        match self.plan_merge(policy) {
            None => Ok(false),
            Some(job) => self.finish_job(job),
        }
    }

    /// 모든 조각을 지금 스레드에서 하나로 합친다. 합쳤으면 true.
    pub fn merge_all(&mut self) -> Result<bool> {
        match self.plan_merge_all() {
            None => Ok(false),
            Some(job) => self.finish_job(job),
        }
    }

    fn finish_job(&mut self, job: MergeJob) -> Result<bool> {
        match job.run() {
            Ok(out) => {
                self.apply_merge(out)?;
                Ok(true)
            }
            Err(e) => {
                self.abort_merge();
                Err(e)
            }
        }
    }
}

#[allow(dead_code)]
fn _assert_send() {
    fn is_send<T: Send>() {}
    is_send::<MergeJob>();
    is_send::<MergeOutput>();
    is_send::<Writer>();
    is_send::<crate::Index>();
}
