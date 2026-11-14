//! 읽기 쪽: 목록 파일의 한 세대를 열어 질의한다. 쓰는 쪽이 목록을 바꿔도 이미 연 세대는 그대로 유효하다.
use crate::docmap::{
    DateCount, DateInterval, FolderCount, SearchScope, SentenceRole, normalize_date, ym_to_key,
    ymd_to_key,
};
use crate::manifest::Manifest;
use crate::qcache::{Plan, QueryCache, QueryCacheOptions, QueryCacheStats, SegKey, SegSets, query_key};
use crate::query::{MIN_WORD_CHARS, ParsedQuery, QueryWord, SplitMode, add_phrases, char_bounds, is_word_or_stem, parse_groups, parse_query, split_bigram, split_dict};
use crate::segment::{Segment, seg_file_name, word_key};
use crate::{Error, Result, corrupt, guard};
use roaring::{RoaringBitmap, RoaringTreemap};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::time::{Duration, Instant};

pub(crate) struct SegView {
    pub seg: Arc<Segment>,
    pub deleted: RoaringBitmap,
    pub doc_deleted: RoaringBitmap,
}

/// 목록 파일을 읽고 조각을 연다. 읽는 사이 쓰는 쪽이 조각을 합쳐 지웠으면 새 목록으로 다시 시도한다.
pub(crate) fn load_views(dir: &Path, reuse: &[Arc<Segment>]) -> Result<(Manifest, Vec<Arc<Segment>>)> {
    let mut last: Option<Error> = None;
    for _ in 0..5 {
        let manifest = match Manifest::load(dir)? {
            Some(m) => m,
            None => return Ok((Manifest::default(), Vec::new())),
        };
        let mut segs = Vec::with_capacity(manifest.segs.len());
        let mut missing = None;
        for e in &manifest.segs {
            if let Some(s) = reuse.iter().find(|s| s.id == e.id && s.body_hash == e.body_hash) {
                segs.push(s.clone());
                continue;
            }
            match Segment::open(&dir.join(seg_file_name(e.id))) {
                Ok(s) => {
                    if s.id != e.id || s.body_hash != e.body_hash || s.n_sent != e.n_sent || s.n_docs != e.n_docs {
                        return corrupt(format!("segment {} does not match the manifest", e.id));
                    }
                    segs.push(Arc::new(s));
                }
                Err(Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {
                    missing = Some(Error::Io(err));
                    break;
                }
                Err(err) => return Err(err),
            }
        }
        match missing {
            None => return Ok((manifest, segs)),
            Some(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| Error::Corrupt("segment listed in the manifest is missing".into())))
}

#[derive(Clone, Debug)]
pub struct SearchOptions {
    /// 돌려줄 문서 수(상위 N). 0이면 전부.
    pub top: usize,
    /// 붙여 쓴 검색어 나누기 방법.
    pub split: SplitMode,
    /// 질의 시간 한도. 넘으면 `Error::Timeout`.
    pub time_limit: Option<Duration>,
    /// true가 되면 질의를 멈추고 `Error::Cancelled`(입력마다 앞 검색 취소).
    pub cancel: Option<Arc<AtomicBool>>,
    /// 문서마다 돌려줄 일치 문장 수(미리보기용).
    pub refs_per_doc: usize,
    /// 문서 범위(허용 집합, 문서 ID). 있으면 이 안의 문서만 결과에 낸다(조사 폴더·상자·업무자료함 범위, 날짜·확장자
    /// 거르기). 비어 있으면 결과도 비어 있다. None이면 거르지 않는다.
    pub allow: Option<Arc<RoaringTreemap>>,
    /// 뺄 문서(문서 ID). `allow`보다 나중에 적용한다(둘 다 있으면 허용 ∖ 제외). 제외어는 `Index::doc_set`으로 만든다.
    pub deny: Option<Arc<RoaringTreemap>>,
    /// 구문(KFC2): 나누지 않고 붙어 있는 그대로만 찾는 낱말(검색어 낱말과 AND). 비교형으로 바꾸므로 "예산 집행"은
    /// "예산집행"이 붙어 있는 문장을 찾는다. 같은 낱말이 검색어에 있으면 그 낱말의 나누기를 끈다.
    pub phrases: Vec<String>,
    /// 검색 범위(Sentence, Document, Folder). 기본값은 Sentence.
    pub scope: SearchScope,
    /// 특정 폴더 안에서만 검색.
    pub folder: Option<String>,
    /// 기간 검색: 시작 날짜 (YYYYMMDD 또는 Unix 타임스탬프).
    pub start_date: Option<i64>,
    /// 기간 검색: 끝 날짜.
    pub end_date: Option<i64>,
    /// 특정 파일 형식(확장자)만 검색 (예: ["hwp", "hwpx", "pdf"]).
    pub file_types: Option<Vec<String>>,
    /// 제목에만 검색.
    pub title_only: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        SearchOptions {
            top: 20,
            split: SplitMode::DictLongest,
            time_limit: Some(Duration::from_secs(2)),
            cancel: None,
            refs_per_doc: 3,
            allow: None,
            deny: None,
            phrases: Vec::new(),
            scope: SearchScope::Sentence,
            folder: None,
            start_date: None,
            end_date: None,
            file_types: None,
            title_only: false,
        }
    }
}

impl SearchOptions {
    /// 문서 범위 거르기가 있는지.
    fn scoped(&self) -> bool {
        self.allow.is_some() || self.deny.is_some()
    }

    /// 이 문서가 범위 안인지(허용 집합 안이고 제외 집합 밖).
    #[inline]
    fn admits(&self, doc_id: u64) -> bool {
        self.allow.as_ref().is_none_or(|a| a.contains(doc_id)) && !self.deny.as_ref().is_some_and(|d| d.contains(doc_id))
    }
}

/// 일치 문장 하나의 위치.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentenceRef {
    /// 문서 안 문장 순번(색인에 넣은 순서, 0부터).
    pub ord: u32,
    pub para: u32,
    pub level: u8,
    /// 모든 검색 낱말이 붙어 있는 그대로 들어 있는 문장인지.
    pub adjacent: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentenceMatch {
    pub doc_id: u64,
    pub sentence: SentenceRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocHit {
    pub doc_id: u64,
    /// 붙어 있는 일치 문장이 하나라도 있는 문서(순위 맨 앞 묶음).
    pub adjacent: bool,
    /// 일치 문장 중 가장 높은 위계(작을수록 높음).
    pub level: u8,
    /// 일치 문장 수.
    pub count: u32,
    pub date: i64,
    /// 앞에서부터 `refs_per_doc`개의 일치 문장.
    pub sentences: Vec<SentenceRef>,
}

#[derive(Clone, Debug, Default)]
pub struct SearchResult {
    /// 해석한 검색 낱말(나눈 조각 포함).
    pub words: Vec<QueryWord>,
    /// 2글자 미만이라 뺀 낱말.
    pub dropped: Vec<String>,
    pub total_docs: usize,
    pub total_sentences: u64,
    /// 붙어 있는 일치가 있는 문서 수.
    pub adjacent_docs: usize,
    /// 순위 순서(붙어 있는 일치 > 위계 > 일치 문장 수 > 날짜 최신 > 문서 ID).
    pub hits: Vec<DocHit>,
    /// 답한 색인 세대.
    pub generation: u64,
}

/// 개수만 낸 결과(`Index::count`). 값은 같은 검색어·방법의 `search` 합계와 같다.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchCounts {
    pub words: Vec<QueryWord>,
    pub dropped: Vec<String>,
    pub total_sentences: u64,
    pub total_docs: usize,
    pub generation: u64,
}

/// 시간 한도·취소 확인.
struct Budget<'a> {
    deadline: Option<Instant>,
    cancel: Option<&'a AtomicBool>,
    ticks: u32,
}

impl<'a> Budget<'a> {
    fn new(opts: &'a SearchOptions) -> Budget<'a> {
        Budget { deadline: opts.time_limit.map(|d| Instant::now() + d), cancel: opts.cancel.as_deref(), ticks: 0 }
    }

    fn check(&self) -> Result<()> {
        if self.cancel.is_some_and(|c| c.load(AtomicOrdering::Relaxed)) {
            return Err(Error::Cancelled);
        }
        if self.deadline.is_some_and(|d| Instant::now() >= d) {
            return Err(Error::Timeout);
        }
        Ok(())
    }

    /// 반복문 안에서 부른다. 1,024번에 한 번만 시계를 본다.
    #[inline]
    fn tick(&mut self) -> Result<()> {
        self.ticks = self.ticks.wrapping_add(1);
        if self.ticks & 1023 == 0 { self.check() } else { Ok(()) }
    }
}

/// 4글자 이상 낱말을 덮는 3글자 조각의 시작 위치: 0, 3, 6, … 과 맨 끝(n-3).
fn cover_offsets(n: usize) -> Vec<usize> {
    let mut v: Vec<usize> = (0..=n - 3).step_by(3).collect();
    if v.last() != Some(&(n - 3)) {
        v.push(n - 3);
    }
    v
}

fn date_range_bitmap(seg: &Segment, start: Option<i64>, end: Option<i64>) -> Result<Option<RoaringBitmap>> {
    if start.is_none() && end.is_none() {
        return Ok(None);
    }
    let s_date = start.unwrap_or(0);
    let e_date = end.unwrap_or(21001231);
    let (sy, sm, sd) = normalize_date(s_date);
    let (ey, em, ed) = normalize_date(e_date);
    let start_ymd = ymd_to_key(sy, sm, sd);
    let end_ymd = ymd_to_key(ey, em, ed);

    let mut out = RoaringBitmap::new();
    for y in sy..=ey {
        let y_start_ymd = ymd_to_key(y, 1, 1);
        let y_end_ymd = ymd_to_key(y, 12, 31);
        if y_start_ymd >= start_ymd && y_end_ymd <= end_ymd {
            if let Some(bm) = seg.year_posting(y)? {
                out |= bm;
            }
            continue;
        }
        let sm_for_y = if y == sy { sm } else { 1 };
        let em_for_y = if y == ey { em } else { 12 };
        for m in sm_for_y..=em_for_y {
            let m_start_ymd = ymd_to_key(y, m, 1);
            let m_end_ymd = ymd_to_key(y, m, 31);
            if m_start_ymd >= start_ymd && m_end_ymd <= end_ymd {
                if let Some(bm) = seg.month_posting(ym_to_key(y, m))? {
                    out |= bm;
                }
                continue;
            }
            let sd_for_m = if y == sy && m == sm { sd } else { 1 };
            let ed_for_m = if y == ey && m == em { ed } else { 31 };
            for d in sd_for_m..=ed_for_m {
                let ymd = ymd_to_key(y, m, d);
                if ymd >= start_ymd && ymd <= end_ymd {
                    if let Some(bm) = seg.date_posting(ymd)? {
                        out |= bm;
                    }
                }
            }
        }
    }
    Ok(Some(out))
}

fn candidate_docs_for_word(seg: &Segment, w: &str) -> Result<Option<RoaringBitmap>> {
    let b = char_bounds(w);
    let n = b.len() - 1;
    if n < 2 {
        return Ok(None);
    }
    let mut acc: Option<RoaringBitmap> = None;
    for i in 0..n - 1 {
        let g2 = &w.as_bytes()[b[i]..b[i + 2]];
        match seg.doc_posting(g2)? {
            Some(bm) => {
                acc = Some(match acc {
                    None => bm,
                    Some(mut a) => {
                        a &= bm;
                        a
                    }
                });
                if acc.as_ref().is_some_and(|a| a.is_empty()) {
                    return Ok(Some(RoaringBitmap::new()));
                }
            }
            None => return Ok(Some(RoaringBitmap::new())),
        }
    }
    Ok(acc)
}

fn candidate_docs_for_qword(seg: &Segment, w: &QueryWord) -> Result<Option<RoaringBitmap>> {
    let s = candidate_docs_for_word(seg, &w.text)?;
    match &w.pieces {
        None => Ok(s),
        Some(pieces) => {
            let mut parts_acc: Option<RoaringBitmap> = None;
            for p in pieces {
                if let Some(p_bm) = candidate_docs_for_word(seg, p)? {
                    parts_acc = Some(match parts_acc {
                        None => p_bm,
                        Some(mut a) => {
                            a &= p_bm;
                            a
                        }
                    });
                }
            }
            match (s, parts_acc) {
                (Some(mut a), Some(b)) => {
                    a |= b;
                    Ok(Some(a))
                }
                (Some(a), None) => Ok(Some(a)),
                (None, Some(b)) => Ok(Some(b)),
                (None, None) => Ok(None),
            }
        }
    }
}

pub(crate) fn candidate_docs_for_query(view: &SegView, words: &[QueryWord], opts: &SearchOptions) -> Result<Option<RoaringBitmap>> {
    let seg = &view.seg;
    let mut cand: Option<RoaringBitmap> = None;
    for w in words {
        if let Some(w_bm) = candidate_docs_for_qword(seg, w)? {
            cand = Some(match cand {
                None => w_bm,
                Some(mut a) => {
                    a &= w_bm;
                    a
                }
            });
            if cand.as_ref().is_some_and(|c| c.is_empty()) {
                return Ok(Some(RoaringBitmap::new()));
            }
        }
    }

    // 1등급 비트맵 ①: tombstone (doc_deleted) 즉시 제외
    if !view.doc_deleted.is_empty() {
        if let Some(c) = cand.as_mut() {
            *c -= &view.doc_deleted;
        } else {
            let mut all = RoaringBitmap::from_sorted_iter(0..seg.n_docs).unwrap_or_default();
            all -= &view.doc_deleted;
            cand = Some(all);
        }
    }

    // 폴더 단위 거르기 (요구사항 5)
    if let Some(folder) = &opts.folder {
        if let Some(f_bm) = seg.folder_posting(folder)? {
            cand = Some(match cand {
                None => f_bm,
                Some(mut c) => {
                    c &= f_bm;
                    c
                }
            });
        } else {
            return Ok(Some(RoaringBitmap::new()));
        }
    }

    // 파일 형식 거르기 (1등급 비트맵 ②, 요구사항 7)
    if let Some(types) = &opts.file_types {
        let mut t_bm = RoaringBitmap::new();
        for t in types {
            if let Some(b) = seg.file_type_posting(t)? {
                t_bm |= b;
            }
        }
        cand = Some(match cand {
            None => t_bm,
            Some(mut c) => {
                c &= t_bm;
                c
            }
        });
    }

    // 시간 단위 거르기 (요구사항 6)
    if opts.start_date.is_some() || opts.end_date.is_some() {
        if let Some(d_bm) = date_range_bitmap(seg, opts.start_date, opts.end_date)? {
            cand = Some(match cand {
                None => d_bm,
                Some(mut c) => {
                    c &= d_bm;
                    c
                }
            });
        }
    }

    // allow / deny 범위 거르기
    if opts.scoped() {
        if let Some(c) = cand {
            let tables = seg.tables();
            let mut filtered = RoaringBitmap::new();
            for local in c {
                if let Some(rec) = tables.doc(local) {
                    if opts.admits(rec.doc_id) {
                        filtered.insert(local);
                    }
                }
            }
            cand = Some(filtered);
        }
    }

    Ok(cand)
}

/// 낱말 하나가 붙어 있는 그대로 들어 있는 문장. `restrict`가 있으면 그 안에서만.
fn strict(seg: &Segment, w: &str, restrict: Option<&RoaringBitmap>, budget: &mut Budget) -> Result<RoaringBitmap> {
    if restrict.is_some_and(|r| r.is_empty()) {
        return Ok(RoaringBitmap::new());
    }
    let b = char_bounds(w);
    let n = b.len() - 1;
    if n <= 3 {
        // 2·3글자 낱말은 조각 비트맵이 곧 정답.
        let mut bm = seg.posting(w.as_bytes())?.unwrap_or_default();
        if let Some(r) = restrict {
            bm &= r;
        }
        return Ok(bm);
    }
    let mut parts: Vec<RoaringBitmap> = Vec::new();
    for o in cover_offsets(n) {
        match seg.posting(&w.as_bytes()[b[o]..b[o + 3]])? {
            Some(bm) => parts.push(bm),
            None => return Ok(RoaringBitmap::new()),
        }
    }
    parts.sort_by_key(|p| p.len());
    let mut parts = parts.into_iter();
    let mut cand = parts.next().unwrap_or_default();
    if let Some(r) = restrict {
        cand &= r;
    }
    for p in parts {
        cand &= p;
    }
    budget.check()?;
    // 후보 문장의 비교형 글에서 실제로 이어져 있는지 확인. 글은 조각 글자 표로 부호화돼 있어 낱말도 같은 부호로 바꾼다
    // (자기 동기 부호라 바이트 찾기가 글자 경계에서만 맞는다). 조각에 없는 글자가 있으면 어느 문장에도 없다.
    let Some(code) = seg.encode_word(w) else { return Ok(RoaringBitmap::new()) };
    let finder = memchr::memmem::Finder::new(&code);
    let mut out = RoaringBitmap::new();
    for sid in cand {
        budget.tick()?;
        if finder.find(seg.text(sid)?).is_some() {
            let _ = out.try_push(sid);
        }
    }
    Ok(out)
}

/// 조각 하나에서 (일치 문장, 붙어 있는 일치 문장)을 지운 문장을 빼기 전 상태로. 두 번째가 None이면 첫 번째와 같다.
/// `restrict`가 있으면 그 안에서만 판정한다: 정답 일치가 `restrict` 안에 있을 때(질의 이력 캐시의 좁히기) 결과는
/// 제한 없이 판정한 것과 같다(모든 단계가 교집합·합집합이고 붙어 있는 일치는 일치의 부분집합이다).
fn eval_seg(seg: &Segment, words: &[QueryWord], restrict: Option<&RoaringBitmap>, budget: &mut Budget) -> Result<SegSets> {
    let is_short = |w: &QueryWord| w.pieces.is_none() && w.text.chars().count() <= 3;
    let mut acc: Option<RoaringBitmap> = restrict.cloned();
    // 1. 2·3글자 낱말: 작은 비트맵부터 교집합.
    let mut short: Vec<RoaringBitmap> = Vec::new();
    for w in words.iter().filter(|w| is_short(w)) {
        match seg.posting(w.text.as_bytes())? {
            Some(bm) => short.push(bm),
            None => return Ok((RoaringBitmap::new(), None)),
        }
    }
    short.sort_by_key(|b| b.len());
    for bm in short {
        acc = Some(match acc {
            None => bm,
            Some(mut a) => {
                a &= bm;
                a
            }
        });
    }
    // 2. 4글자 이상 낱말: 지금까지의 교집합 안에서만 확인.
    let mut strict_acc: Option<RoaringBitmap> = None;
    for w in words.iter().filter(|w| !is_short(w)) {
        budget.check()?;
        let s = strict(seg, &w.text, acc.as_ref(), budget)?;
        match &w.pieces {
            None => {
                if let Some(sa) = strict_acc.as_mut() {
                    *sa &= &s;
                }
                acc = Some(s);
            }
            Some(pieces) => {
                // 넓은 일치: 붙어 있거나, 조각이 모두 같은 문장에 있거나.
                let mut ordered: Vec<&String> = pieces.iter().collect();
                ordered.sort_by_key(|p| p.len());
                let mut all: Option<RoaringBitmap> = acc.clone();
                for p in ordered {
                    all = Some(strict(seg, p, all.as_ref(), budget)?);
                }
                let mut broad = all.unwrap_or_default();
                broad |= &s;
                strict_acc = Some(match strict_acc.take() {
                    None => s,
                    Some(sa) => sa & s,
                });
                acc = Some(broad);
            }
        }
    }
    Ok((acc.unwrap_or_default(), strict_acc))
}

/// 조각 판정에서 그 세대의 지운 문장을 뺀다.
fn remove_deleted(view: &SegView, mut sets: SegSets) -> SegSets {
    if !view.deleted.is_empty() {
        sets.0 -= &view.deleted;
        if let Some(sa) = sets.1.as_mut() {
            *sa -= &view.deleted;
        }
    }
    sets
}

/// 캐시에 담긴 판정에서 지운 문장을 뺀 것. 지운 문장이 없으면 복사하지 않고 같이 쓴다.
fn live_share(view: &SegView, sets: &Arc<SegSets>) -> Arc<SegSets> {
    if view.deleted.is_empty() {
        return sets.clone();
    }
    Arc::new((&sets.0 - &view.deleted, sets.1.as_ref().map(|sa| sa - &view.deleted)))
}

/// 문서 범위 거르기(KFC1): 범위 밖 문서의 문장을 일치·붙어 있는 일치에서 뺀다. 판정(과 질의 이력 캐시)은 범위와
/// 무관하게 하고 여기서만 거르므로, 같은 질의를 범위만 바꿔 물어도 캐시를 그대로 쓴다. 문서의 문장은 조각 안에서
/// 이어져 있어 일치 문서마다 한 번만 확인한다. 범위 밖 문서가 없으면 복사하지 않는다.
fn scope_sets(view: &SegView, sets: Arc<SegSets>, opts: &SearchOptions, budget: &mut Budget) -> Result<Arc<SegSets>> {
    if !opts.scoped() || sets.0.is_empty() {
        return Ok(sets);
    }
    let tables = view.seg.tables();
    let mut outside = RoaringBitmap::new();
    let mut it = sets.0.iter();
    while let Some(sid) = it.next() {
        budget.tick()?;
        let Some(rec) = tables.doc_of(sid).and_then(|local| tables.doc(local)) else { return corrupt("document out of range") };
        if sid < rec.first_sid || sid - rec.first_sid >= rec.n_sent {
            return corrupt("sentence outside its document range");
        }
        let end = rec.first_sid + rec.n_sent;
        if !opts.admits(rec.doc_id) {
            outside.insert_range(rec.first_sid..end);
        }
        it.advance_to(end);
    }
    if outside.is_empty() {
        return Ok(sets);
    }
    Ok(Arc::new((&sets.0 - &outside, sets.1.as_ref().map(|a| a - &outside))))
}

struct Agg {
    /// 순위 열쇠(작을수록 앞): 붙어 있는 일치 없음(1비트) | 위계 | 일치 문장 수의 보수.
    key: u64,
    doc_id: u64,
    date: i64,
    view: u32,
    /// 조각 안 문서 번호.
    local: u32,
}

const ADJACENT_BIT: u64 = 1 << 40;

/// 결과 순서(MG2). `Relevance`는 `search`·`rank`의 기본 순서, `Hierarchy`는 위계(작은 값) 먼저이고 같은 위계 안에서는
/// 기본 순서(붙어 있는 일치 > 일치 문장 수 > 날짜 최신 > 문서 ID) — 기본 순서 결과를 위계로 안정 정렬한 것과 같다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Order {
    #[default]
    Relevance,
    Hierarchy,
}

/// 위계 순 열쇠: 위계(8비트)를 맨 위로 올리고 붙어 있는 일치 비트·일치 문장 수 보수는 그대로 둔다(작을수록 앞).
#[inline]
fn hierarchy_key(key: u64) -> u64 {
    (((key >> 32) & 0xff) << 41) | (key & ADJACENT_BIT) | (key & 0xffff_ffff)
}

/// `hierarchy_key`의 역.
#[inline]
fn relevance_key(hkey: u64) -> u64 {
    (hkey & ADJACENT_BIT) | (((hkey >> 41) & 0xff) << 32) | (hkey & 0xffff_ffff)
}

impl Agg {
    /// 순위 열쇠를 적는다. 일치 문장 수는 아래 32비트에 보수로 넣는다(많을수록 열쇠가 작다).
    #[inline]
    fn set(&mut self, adjacent: bool, level: u8, count: u32) {
        let tier = if adjacent { 0 } else { ADJACENT_BIT };
        self.key = tier | ((level as u64) << 32) | (u32::MAX - count) as u64;
    }
    fn adjacent(&self) -> bool {
        self.key & ADJACENT_BIT == 0
    }
    fn level(&self) -> u8 {
        (self.key >> 32) as u8
    }
    fn count(&self) -> u32 {
        u32::MAX - self.key as u32
    }
}

/// 문서의 고정 순위표: 모든 조각의 모든 문서를 (날짜 최신, 문서 ID, 조각, 조각 안 번호) 순서로 줄 세운 번호.
/// 질의마다 달라지지 않는 순위 뒷부분(날짜 > 문서 ID)을 32비트 하나로 바꿔, 전체 순위를 정수 하나의 정렬로 만든다.
/// 색인 세대마다 처음 전체 순위를 낼 때 한 번 만든다(문서 수 D에 O(D log D)).
struct StaticRank {
    /// [조각][조각 안 문서 번호] → 순위.
    per_view: Vec<Vec<u32>>,
    /// 순위 → (조각, 조각 안 문서 번호).
    inverse: Vec<(u32, u32)>,
}

/// 전체 순위 정렬 열쇠: 위 = 질의 순위 열쇠(41비트), 아래 32비트 = 고정 순위. 작을수록 앞.
#[inline]
fn full_key(key: u64, static_rank: u32) -> u128 {
    ((key as u128) << 32) | static_rank as u128
}

/// 문서 순위 모으기. `top`이 0이면 전부 모으고(고정 순위로 묶은 정렬 열쇠 하나씩), 아니면 가장 앞선 `top`개만
/// 순위 순서로 쥐고 있는다(문서 전체를 쌓았다가 고르지 않는다 — 상위 N건을 먼저 낼 때의 비용을 줄인다).
struct Collector<'r> {
    top: usize,
    order: Order,
    best: Vec<Agg>,
    all: Vec<u128>,
    ranks: &'r [Vec<u32>],
    total_docs: usize,
    adjacent_docs: usize,
}

impl Collector<'_> {
    #[inline]
    fn push(&mut self, a: Agg) -> Result<()> {
        self.total_docs += 1;
        self.adjacent_docs += a.adjacent() as usize;
        if self.top == 0 {
            let Some(&r) = self.ranks.get(a.view as usize).and_then(|v| v.get(a.local as usize)) else { return corrupt("document missing from the static rank") };
            self.all.push(full_key(sort_key(&a, self.order), r));
            return Ok(());
        }
        if self.best.len() == self.top {
            match self.best.last() {
                Some(worst) if order(&a, worst, self.order) == Ordering::Less => {
                    self.best.pop();
                }
                _ => return Ok(()),
            }
        }
        let pos = self.best.partition_point(|b| order(b, &a, self.order) == Ordering::Less);
        self.best.insert(pos, a);
        Ok(())
    }
}

/// 순위: 붙어 있는 일치 > 위계(작은 값) > 일치 문장 수 > 날짜 최신 > 문서 ID.
/// `Order::Hierarchy`면 위계를 앞에 둔 열쇠로 같은 비교를 한다.
#[inline]
fn sort_key(a: &Agg, o: Order) -> u64 {
    match o {
        Order::Relevance => a.key,
        Order::Hierarchy => hierarchy_key(a.key),
    }
}

fn order(a: &Agg, b: &Agg, o: Order) -> Ordering {
    sort_key(a, o).cmp(&sort_key(b, o)).then_with(|| b.date.cmp(&a.date)).then_with(|| a.doc_id.cmp(&b.doc_id))
}

/// 미리보기 문장 없이 순위만 낸 결과. 미리보기는 화면에 보일 문서만 `sentences`로 만든다.
pub struct Ranking {
    /// `hits`의 `sentences`는 비어 있다.
    pub result: SearchResult,
    /// hits와 같은 순서의 (조각, 조각 안 문서 번호).
    slots: Vec<(u32, u32)>,
    /// 조각별 (일치 문장, 붙어 있는 일치 문장).
    sets: Vec<Arc<SegSets>>,
    /// 그룹 검색일 때 각 묶음별 조각별 (일치 문장, 붙어 있는 일치 문장).
    group_sets: Option<Vec<Vec<Arc<SegSets>>>>,
}

impl Ranking {
    /// `hit`번째 문서의 앞에서부터 `n`개 일치 문장. `index`는 이 순위를 낸 색인(같은 세대)이어야 한다.
    /// 그룹 검색으로 만든 순위면 각 묶음의 최고 문장을 묶음 순서대로 최대 `n`개 돌려준다.
    pub fn sentences(&self, index: &Index, hit: usize, n: usize) -> Result<Vec<SentenceRef>> {
        if index.generation != self.result.generation {
            return Err(Error::Invalid("ranking belongs to another index generation".into()));
        }
        let Some(&(view, local)) = self.slots.get(hit) else { return Err(Error::Invalid(format!("hit {hit} out of range"))) };
        if let Some(groups) = &self.group_sets {
            guard(|| {
                let mut out = Vec::new();
                for g_sets in groups {
                    if out.len() >= n {
                        break;
                    }
                    if let Some(s) = index.doc_best_sentence(g_sets, view, local)? {
                        out.push(s);
                    }
                }
                Ok(out)
            })
        } else {
            if index.views.len() != self.sets.len() {
                return Err(Error::Invalid("ranking belongs to another index generation".into()));
            }
            guard(|| index.doc_sentences(&self.sets, view, local, n))
        }
    }
}

/// 열려 있는 색인의 한 세대.
pub struct Index {
    dir: PathBuf,
    generation: u64,
    views: Vec<SegView>,
    ranks: OnceLock<StaticRank>,
    /// 질의 이력 캐시(켜면). 세대가 바뀌어도(`reload`) 같은 것을 쓴다.
    cache: Option<Arc<Mutex<QueryCache>>>,
}

/// 캐시 잠금. 잠근 채 패닉이 났더라도 캐시 상태는 늘 일관되므로(담기·버리기가 끝난 뒤에만 바뀐다) 그대로 쓴다.
fn lock_cache(c: &Mutex<QueryCache>) -> std::sync::MutexGuard<'_, QueryCache> {
    c.lock().unwrap_or_else(|e| e.into_inner())
}

impl Index {
    /// Explicit one-character search for an Enter-submitted UI query. This scans
    /// the mapped comparison text; normal indexed queries remain unchanged.
    pub fn search_character(&self, query: &str, opts: &SearchOptions) -> Result<SearchResult> {
        guard(|| {
            let word = crate::fold(query);
            if word.chars().count() != 1 {
                return Err(Error::Invalid("character search requires one comparison character".into()));
            }
            if !opts.phrases.is_empty() {
                return Err(Error::Invalid("character search does not accept phrase filters".into()));
            }
            let mut budget = Budget::new(opts);
            budget.check()?;
            let mut docs = std::collections::BTreeMap::<u64, DocHit>::new();
            let mut total_sentences = 0;
            for view in &self.views {
                let seg = &view.seg;
                let Some(code) = seg.encode_word(&word) else { continue };
                let finder = memchr::memmem::Finder::new(&code);
                for sid in 0..seg.n_sent {
                    budget.tick()?;
                    if view.deleted.contains(sid) || finder.find(seg.text(sid)?).is_none() { continue; }
                    let rec = seg.doc(seg.doc_of(sid)?)?;
                    if !opts.admits(rec.doc_id) { continue; }
                    let level = seg.level(sid)?;
                    let hit = docs.entry(rec.doc_id).or_insert_with(|| DocHit {
                        doc_id: rec.doc_id, adjacent: true, level, count: 0,
                        date: rec.date, sentences: Vec::new(),
                    });
                    hit.level = hit.level.min(level);
                    hit.count += 1;
                    total_sentences += 1;
                    if hit.sentences.len() < opts.refs_per_doc {
                        hit.sentences.push(SentenceRef { ord: sid - rec.first_sid,
                            para: seg.para(sid)?, level, adjacent: true });
                    }
                }
            }
            budget.check()?;
            let total_docs = docs.len();
            let mut hits: Vec<_> = docs.into_values().collect();
            hits.sort_unstable_by(|a, b| a.level.cmp(&b.level)
                .then_with(|| b.count.cmp(&a.count)).then_with(|| b.date.cmp(&a.date))
                .then_with(|| a.doc_id.cmp(&b.doc_id)));
            if opts.top != 0 { hits.truncate(opts.top); }
            Ok(SearchResult { total_docs, total_sentences, adjacent_docs: total_docs,
                generation: self.generation, hits, ..SearchResult::default() })
        })
    }

    /// 색인 폴더를 연다. 목록 파일이 없으면 빈 색인(세대 0).
    pub fn open(dir: &Path) -> Result<Index> {
        guard(|| {
            let (manifest, segs) = load_views(dir, &[])?;
            Ok(Index::from_parts(dir, manifest, segs))
        })
    }

    fn from_parts(dir: &Path, manifest: Manifest, segs: Vec<Arc<Segment>>) -> Index {
        let views = segs.into_iter().zip(manifest.segs).map(|(seg, e)| SegView { seg, deleted: e.deleted, doc_deleted: e.doc_deleted }).collect();
        Index { dir: dir.to_path_buf(), generation: manifest.generation, views, ranks: OnceLock::new(), cache: None }
    }

    /// 문서 고정 순위표(처음 부를 때 만든다).
    fn static_rank(&self) -> Result<&StaticRank> {
        if let Some(r) = self.ranks.get() {
            return Ok(r);
        }
        let mut all: Vec<(i64, u64, u32, u32)> = Vec::with_capacity(self.views.iter().map(|v| v.seg.n_docs as usize).sum());
        for (vi, v) in self.views.iter().enumerate() {
            let t = v.seg.tables();
            for local in 0..v.seg.n_docs {
                let Some(rec) = t.doc(local) else { return corrupt("document out of range") };
                all.push((rec.date, rec.doc_id, vi as u32, local));
            }
        }
        if all.len() > u32::MAX as usize {
            return Err(Error::Invalid("too many documents for one ranking".into()));
        }
        // 날짜 최신 > 문서 ID > 조각 > 번호(살아 있는 문서의 ID는 겹치지 않아 뒤 둘은 지운 문서끼리만 가른다).
        all.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)).then(a.3.cmp(&b.3)));
        let mut per_view: Vec<Vec<u32>> = self.views.iter().map(|v| vec![0; v.seg.n_docs as usize]).collect();
        let mut inverse = Vec::with_capacity(all.len());
        for (r, &(_, _, view, local)) in all.iter().enumerate() {
            per_view[view as usize][local as usize] = r as u32;
            inverse.push((view, local));
        }
        let _ = self.ranks.set(StaticRank { per_view, inverse });
        Ok(self.ranks.get().expect("static rank was just set"))
    }

    /// 문서 하나의 앞에서부터 `n`개 일치 문장.
    fn doc_sentences(&self, sets: &[Arc<SegSets>], view: u32, local: u32, n: usize) -> Result<Vec<SentenceRef>> {
        let mut out = Vec::new();
        if n == 0 {
            return Ok(out);
        }
        let (matched, adjacent) = &*sets[view as usize];
        let seg = &*self.views[view as usize].seg;
        let rec = seg.doc(local)?;
        for sid in matched.range(rec.first_sid..rec.first_sid + rec.n_sent).take(n) {
            out.push(SentenceRef { ord: sid - rec.first_sid, para: seg.para(sid)?, level: seg.level(sid)?, adjacent: adjacent.as_ref().is_none_or(|b| b.contains(sid)) });
        }
        Ok(out)
    }

    /// 문서 하나의 일치 문장 중 가장 높은 위계의 최고 문장 1개.
    /// 위계(작은 level) 우선, 같으면 붙어 있는 일치(adjacent) 우선, 같으면 앞선 문장(ord).
    fn doc_best_sentence(&self, sets: &[Arc<SegSets>], view: u32, local: u32) -> Result<Option<SentenceRef>> {
        let (matched, adjacent) = &*sets[view as usize];
        let seg = &*self.views[view as usize].seg;
        let rec = seg.doc(local)?;
        let mut best: Option<SentenceRef> = None;
        for sid in matched.range(rec.first_sid..rec.first_sid + rec.n_sent) {
            let is_adj = adjacent.as_ref().is_none_or(|b| b.contains(sid));
            let level = seg.level(sid)?;
            let ord = sid - rec.first_sid;
            let cand = SentenceRef {
                ord,
                para: seg.para(sid)?,
                level,
                adjacent: is_adj,
            };
            match &best {
                None => best = Some(cand),
                Some(cur) => {
                    let better = cand.level < cur.level
                        || (cand.level == cur.level && cand.adjacent && !cur.adjacent)
                        || (cand.level == cur.level && cand.adjacent == cur.adjacent && cand.ord < cur.ord);
                    if better {
                        best = Some(cand);
                    }
                }
            }
        }
        Ok(best)
    }

    /// 목록 파일이 바뀌었으면 새 세대로 바꾼다(이미 연 조각은 다시 열지 않는다). 바뀌었으면 true.
    pub fn reload(&mut self) -> Result<bool> {
        let reuse: Vec<Arc<Segment>> = self.views.iter().map(|v| v.seg.clone()).collect();
        let dir = self.dir.clone();
        // Windows에서는 쓰는 쪽이 파일을 바꿔 끼우는 순간 읽기가 잠깐 '액세스 거부'(OS 5)를 받을 수 있다.
        // 실패해도 지금 색인(self)은 그대로이므로 짧게 몇 번 다시 시도한다(다중 프로세스 시험 4회 중 1회 관찰, 2026-11-14).
        let mut attempt: u64 = 0;
        let (manifest, segs) = loop {
            match guard(|| load_views(&dir, &reuse)) {
                Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::PermissionDenied && attempt < 5 => {
                    attempt += 1;
                    std::thread::sleep(std::time::Duration::from_millis(5 * attempt));
                }
                other => break other?,
            }
        };
        if manifest.generation == self.generation {
            return Ok(false);
        }
        let cache = self.cache.take();
        *self = Index::from_parts(&dir, manifest, segs);
        if let Some(c) = &cache {
            let live: Vec<SegKey> = self.views.iter().map(|v| (v.seg.id, v.seg.body_hash)).collect();
            lock_cache(c).retain_segments(&live);
        }
        self.cache = cache;
        Ok(true)
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn num_segments(&self) -> usize {
        self.views.len()
    }

    /// 살아 있는 문장 수.
    pub fn num_sentences(&self) -> u64 {
        self.views.iter().map(|v| v.seg.n_sent as u64 - v.deleted.len()).sum()
    }

    /// 조각 파일 본문 해시를 모두 다시 계산해 확인한다(진단용, 느림).
    pub fn verify(&self) -> Result<()> {
        guard(|| self.views.iter().try_for_each(|v| v.seg.verify()))
    }

    /// 살아 있는 문서에 이 낱말(어절, 비교형)이 있는지. 낱말 비트맵은 조각 안 문서 번호이고(형식 v3), 문서는 통째로
    /// 지워지므로 첫 문장이 지워지지 않은 문서가 하나라도 있으면 살아 있다.
    fn has_word(&self, w: &str) -> Result<bool> {
        let key = word_key(w);
        for v in &self.views {
            if let Some(bm) = v.seg.posting(&key)? {
                if v.deleted.is_empty() {
                    if !bm.is_empty() {
                        return Ok(true);
                    }
                    continue;
                }
                let tables = v.seg.tables();
                for local in &bm {
                    let Some(rec) = tables.doc(local) else { return corrupt("word posting: document out of range") };
                    if !v.deleted.contains(rec.first_sid) {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    /// 색인 낱말로 가장 길게 나눈 뒤, 조각이 또 색인 낱말들로 나뉘면 더 나눈다(더 나뉘지 않는 낱말까지).
    /// "예산집행계획" → "예산집행" + "계획" → "예산" + "집행" + "계획": 띄어 쓴 문장도 모두 찾게 된다.
    fn split_deep(&self, w: &str) -> Result<Option<Vec<String>>> {
        if w.chars().count() < 2 * MIN_WORD_CHARS {
            return Ok(None);
        }
        let mut failed: Option<Error> = None;
        // 조각은 색인 어절이거나, 조사·어미가 붙은 꼴로만 나온 줄기(KB3: "예산"이 "예산을"로만 나와도 나눈다).
        let first = split_dict(w, |p| {
            is_word_or_stem(p, |x| match self.has_word(x) {
                Ok(found) => found,
                Err(e) => {
                    failed.get_or_insert(e);
                    false
                }
            })
        });
        if let Some(e) = failed {
            return Err(e);
        }
        let Some(first) = first else { return Ok(None) };
        let mut out = Vec::with_capacity(first.len());
        for p in first {
            match self.split_deep(&p)? {
                Some(sub) => out.extend(sub),
                None => out.push(p),
            }
        }
        Ok(Some(out))
    }

    /// 검색어를 낱말로 나누고, 붙여 쓴 낱말은 `split` 방법으로 조각을 정한다.
    pub fn resolve(&self, query: &str, split: SplitMode) -> Result<ParsedQuery> {
        guard(|| self.resolve_inner(query, split))
    }

    fn resolve_inner(&self, query: &str, split: SplitMode) -> Result<ParsedQuery> {
        let mut parsed = parse_query(query);
        for w in &mut parsed.words {
            w.pieces = match split {
                SplitMode::Off => None,
                SplitMode::Bigram => split_bigram(&w.text),
                SplitMode::DictLongest => self.split_deep(&w.text)?,
            };
        }
        Ok(parsed)
    }

    /// 질의 옵션까지 반영한 해석: `opts.split`으로 나누고 `opts.phrases`(구문)를 나누지 않는 낱말로 더한다.
    /// `search`·`rank`·`count`·`match_sentences`가 쓰는 해석과 같다.
    pub fn resolve_with(&self, query: &str, opts: &SearchOptions) -> Result<ParsedQuery> {
        guard(|| self.resolve_opts(query, opts))
    }

    fn resolve_opts(&self, query: &str, opts: &SearchOptions) -> Result<ParsedQuery> {
        let mut parsed = self.resolve_inner(query, opts.split)?;
        add_phrases(&mut parsed, &opts.phrases);
        Ok(parsed)
    }

    /// 제외어용 문서 집합(KFC2): 비교형 낱말이 붙어 있는 그대로(나누지 않음) 들어 있는 문장이 하나라도 있는 살아 있는
    /// 문서의 ID. 여러 낱말("예산 집행")은 비교형을 이어 붙인 하나로 본다. 결과를 `SearchOptions::deny`에 합쳐 쓴다.
    /// 비교형이 2글자 미만이면 `Error::Invalid`(본문 검색 낱말과 같은 하한).
    pub fn doc_set(&self, word: &str) -> Result<RoaringTreemap> {
        guard(|| {
            let w = crate::fold(word);
            if w.chars().count() < MIN_WORD_CHARS {
                return Err(Error::Invalid(format!("exclude word shorter than {MIN_WORD_CHARS} chars")));
            }
            let opts = SearchOptions { time_limit: None, ..SearchOptions::default() };
            let mut budget = Budget::new(&opts);
            let mut out = RoaringTreemap::new();
            for v in &self.views {
                let mut hit = strict(&v.seg, &w, None, &mut budget)?;
                hit -= &v.deleted;
                let tables = v.seg.tables();
                let mut it = hit.iter();
                while let Some(sid) = it.next() {
                    let Some(rec) = tables.doc_of(sid).and_then(|local| tables.doc(local)) else { return corrupt("document out of range") };
                    out.insert(rec.doc_id);
                    it.advance_to(rec.first_sid + rec.n_sent);
                }
            }
            Ok(out)
        })
    }

    /// 조각마다 (일치 문장, 붙어 있는 일치 문장): 지운 문장과 범위 밖 문서(`opts.allow`·`deny`)를 뺀 것.
    /// `rank`·`count`·`match_sentences`가 모두 이 결과를 쓰므로 세 결과의 범위가 늘 같다.
    fn eval_all(&self, parsed: &ParsedQuery, opts: &SearchOptions, budget: &mut Budget) -> Result<Vec<Arc<SegSets>>> {
        let sets = self.eval_unscoped(parsed, budget)?;
        let mut out = Vec::with_capacity(self.views.len());

        for (v, s) in self.views.iter().zip(sets) {
            budget.check()?;
            let candidate_docs = candidate_docs_for_query(v, &parsed.words, opts)?;
            if candidate_docs.as_ref().is_some_and(|c| c.is_empty()) {
                out.push(Arc::new((RoaringBitmap::new(), None)));
                continue;
            }

            let mut matched = s.0.clone();
            let mut adjacent = s.1.clone();

            if let Some(c_docs) = &candidate_docs {
                let tables = v.seg.tables();
                let mut filtered_matched = RoaringBitmap::new();
                for sid in &matched {
                    if let Some(d) = tables.doc_of(sid) {
                        if c_docs.contains(d) {
                            filtered_matched.insert(sid);
                        }
                    }
                }
                adjacent = adjacent.map(|adj| {
                    let mut a = RoaringBitmap::new();
                    for sid in adj {
                        if filtered_matched.contains(sid) {
                            a.insert(sid);
                        }
                    }
                    a
                });
                matched = filtered_matched;
            }

            if opts.title_only {
                if let Some(role_title) = v.seg.role_posting(SentenceRole::Title as u8)? {
                    matched &= &role_title;
                    if let Some(adj) = adjacent.as_mut() {
                        *adj &= &role_title;
                    }
                } else {
                    matched.clear();
                    if let Some(adj) = adjacent.as_mut() {
                        adj.clear();
                    }
                }
            }

            out.push(Arc::new((matched, adjacent)));
        }

        if !opts.scoped() {
            return Ok(out);
        }
        self.views.iter().zip(out).map(|(v, s)| scope_sets(v, s, opts, budget)).collect()
    }

    fn eval_unscoped(&self, parsed: &ParsedQuery, budget: &mut Budget) -> Result<Vec<Arc<SegSets>>> {
        if parsed.words.is_empty() {
            budget.check()?;
            return Ok(self.views.iter().map(|_| Arc::new((RoaringBitmap::new(), None))).collect());
        }
        let Some(cache) = &self.cache else {
            let mut out = Vec::with_capacity(self.views.len());
            for v in &self.views {
                budget.check()?;
                out.push(Arc::new(remove_deleted(v, eval_seg(&v.seg, &parsed.words, None, budget)?)));
            }
            return Ok(out);
        };
        // 질의 이력 캐시: 판정 방법은 잠근 채 정하고, 판정은 잠금 없이 하고, 새 판정은 다 끝난 뒤에만 담는다
        // (시간 초과·취소로 멈춘 질의는 아무것도 담지 않는다).
        let key = query_key(&parsed.words);
        let seg_keys: Vec<SegKey> = self.views.iter().map(|v| (v.seg.id, v.seg.body_hash)).collect();
        let plans = lock_cache(cache).plan(&key, &parsed.words, &seg_keys);
        let mut out = Vec::with_capacity(self.views.len());
        let mut fresh = Vec::new();
        for ((v, plan), sk) in self.views.iter().zip(plans).zip(&seg_keys) {
            budget.check()?;
            let raw = match plan {
                Plan::Reuse(s) => {
                    out.push(live_share(v, &s));
                    continue;
                }
                Plan::Narrow(prev) => eval_seg(&v.seg, &parsed.words, Some(&prev.0), budget)?,
                Plan::Fresh => eval_seg(&v.seg, &parsed.words, None, budget)?,
            };
            let raw = Arc::new(raw);
            out.push(live_share(v, &raw));
            fresh.push((*sk, raw));
        }
        lock_cache(cache).store(&key, &parsed.words, fresh);
        Ok(out)
    }

    /// 질의 이력 캐시를 켜거나(`Some`) 끈다(`None`). 켜면 같은 질의·좁히는 질의·새 세대의 같은 질의가 남은 조각의
    /// 판정을 다시 쓴다(결과는 캐시 없이 낸 것과 같다). `reload`해도 유지되고, 목록에서 빠진 조각의 판정은 버린다.
    /// 기본은 꺼짐.
    pub fn set_query_cache(&mut self, opts: Option<QueryCacheOptions>) {
        self.cache = opts.map(|o| Arc::new(Mutex::new(QueryCache::new(o))));
    }

    /// 질의 이력 캐시 상태(꺼져 있으면 None).
    pub fn query_cache_stats(&self) -> Option<QueryCacheStats> {
        self.cache.as_ref().map(|c| lock_cache(c).stats())
    }

    /// 문장 단위 검색. 같은 문장에 모든 낱말이 있는 문장을 찾아 문서별로 모으고 순위를 매겨 상위 `opts.top`건을 돌려준다.
    /// 검색 낱말이 하나도 없으면(모두 2글자 미만) 빈 결과.
    pub fn search(&self, query: &str, opts: &SearchOptions) -> Result<SearchResult> {
        guard(|| self.search_inner(query, opts))
    }

    /// `search`와 같되 결과 순서를 고른다. `Order::Hierarchy`는 위계 순 상위 `opts.top`건만 쥐고 모으므로(전체를 모아
    /// 정렬하지 않음) 위계 칸의 재검색이 기본 순서 상위 N건과 같은 비용이다.
    pub fn search_by(&self, query: &str, opts: &SearchOptions, order: Order) -> Result<SearchResult> {
        guard(|| self.search_inner_by(query, opts, order))
    }

    /// `rank`와 같되 결과 순서를 고른다.
    pub fn rank_by(&self, query: &str, opts: &SearchOptions, order: Order) -> Result<Ranking> {
        guard(|| self.rank_inner(query, opts, order))
    }

    fn search_inner(&self, query: &str, opts: &SearchOptions) -> Result<SearchResult> {
        self.search_inner_by(query, opts, Order::Relevance)
    }

    fn search_inner_by(&self, query: &str, opts: &SearchOptions, order: Order) -> Result<SearchResult> {
        let ranking = self.rank_inner(query, opts, order)?;
        let mut result = ranking.result;
        if opts.refs_per_doc > 0 {
            for (hit, &(view, local)) in result.hits.iter_mut().zip(&ranking.slots) {
                hit.sentences = self.doc_sentences(&ranking.sets, view, local, opts.refs_per_doc)?;
            }
        }
        Ok(result)
    }

    /// `search`와 같은 순위를 내되 미리보기 문장(`DocHit::sentences`)은 만들지 않는다. 전체 순위(`opts.top` 0)를
    /// 낼 때 화면에 보일 문서만 `Ranking::sentences`로 미리보기를 만들면 문서 수만큼의 미리보기 비용이 들지 않는다.
    /// `opts.refs_per_doc`은 쓰지 않는다.
    pub fn rank(&self, query: &str, opts: &SearchOptions) -> Result<Ranking> {
        guard(|| self.rank_inner(query, opts, Order::Relevance))
    }

    fn rank_inner(&self, query: &str, opts: &SearchOptions, order: Order) -> Result<Ranking> {
        if opts.scope == SearchScope::Document {
            return self.rank_document_scope_inner(query, opts, order);
        }
        let mut budget = Budget::new(opts);
        budget.check()?;
        let parsed = self.resolve_opts(query, opts)?;
        let sets = self.eval_all(&parsed, opts, &mut budget)?;
        let static_rank = if opts.top == 0 && sets.iter().any(|s| !s.0.is_empty()) { Some(self.static_rank()?) } else { None };
        let mut found = Collector { top: opts.top, order, best: Vec::new(), all: Vec::new(), ranks: static_rank.map_or(&[], |r| &r.per_view), total_docs: 0, adjacent_docs: 0 };
        let mut total_sentences = 0u64;
        for (vi, (view, sets_v)) in self.views.iter().zip(&sets).enumerate() {
            let (matched, adjacent) = &**sets_v;
            if matched.is_empty() {
                continue;
            }
            total_sentences += matched.len();
            let tables = view.seg.tables();
            if opts.top == 0 {
                found.all.reserve((matched.len().min(view.seg.n_docs as u64)) as usize);
            }
            // 붙어 있는 일치 문장은 일치 문장의 부분집합이고 둘 다 오름차순이라 나란히 훑는다.
            let mut adj = adjacent.as_ref().map(|b| b.iter().peekable());
            // 한 문서의 문장은 조각 안에서 이어져 있고 문장 번호는 오름차순으로 나온다: 지금 문서의 끝 번호를
            // 넘을 때만 문서 표를 찾는다. 일치 문장 수와 가장 높은 위계는 지역 변수에 모았다가 문서가 바뀔 때 적는다.
            let (mut doc_end, mut count, mut best) = (0u32, 0u32, u8::MAX);
            let mut cur: Option<Agg> = None;
            let mut finish = |cur: &mut Option<Agg>, doc_end: u32, count: u32, best: u8| -> Result<()> {
                if let Some(mut a) = cur.take() {
                    let mut adjacent_hit = adj.is_none();
                    if let Some(it) = adj.as_mut() {
                        while it.next_if(|s| *s < doc_end).is_some() {
                            adjacent_hit = true;
                        }
                    }
                    a.set(adjacent_hit, best, count);
                    found.push(a)?;
                }
                Ok(())
            };
            for sid in matched {
                let Some(lv) = tables.level(sid) else { return corrupt("matched sentence out of range") };
                if sid < doc_end {
                    count += 1;
                    best = best.min(lv);
                    continue;
                }
                budget.tick()?;
                finish(&mut cur, doc_end, count, best)?;
                let Some((local, rec)) = tables.doc_of(sid).and_then(|local| tables.doc(local).map(|r| (local, r))) else { return corrupt("document out of range") };
                if sid < rec.first_sid || sid - rec.first_sid >= rec.n_sent {
                    return corrupt("sentence outside its document range");
                }
                doc_end = rec.first_sid + rec.n_sent;
                (count, best) = (1, lv);
                cur = Some(Agg { key: 0, doc_id: rec.doc_id, date: rec.date, view: vi as u32, local });
            }
            finish(&mut cur, doc_end, count, best)?;
        }
        budget.check()?;
        let (total_docs, adjacent_docs) = (found.total_docs, found.adjacent_docs);
        let (mut hits, mut slots) = (Vec::new(), Vec::new());
        if let Some(sr) = static_rank {
            // 전체 순위: 정수 하나(질의 열쇠 + 고정 순위)로 정렬하고, 고정 순위에서 문서를 되찾는다.
            let mut all = found.all;
            all.sort_unstable();
            hits.reserve(all.len());
            slots.reserve(all.len());
            for k in all {
                let Some(&(view, local)) = sr.inverse.get(k as u32 as usize) else { return corrupt("static rank out of range") };
                let Some(rec) = self.views.get(view as usize).and_then(|v| v.seg.tables().doc(local)) else { return corrupt("document out of range") };
                let skey = (k >> 32) as u64;
                let key = if order == Order::Hierarchy { relevance_key(skey) } else { skey };
                let a = Agg { key, doc_id: rec.doc_id, date: rec.date, view, local };
                hits.push(DocHit { doc_id: a.doc_id, adjacent: a.adjacent(), level: a.level(), count: a.count(), date: a.date, sentences: Vec::new() });
                slots.push((view, local));
            }
        } else {
            for a in &found.best {
                hits.push(DocHit { doc_id: a.doc_id, adjacent: a.adjacent(), level: a.level(), count: a.count(), date: a.date, sentences: Vec::new() });
                slots.push((a.view, a.local));
            }
        }
        let result = SearchResult { words: parsed.words, dropped: parsed.dropped, total_docs, total_sentences, adjacent_docs, hits, generation: self.generation };
        Ok(Ranking { result, slots, sets, group_sets: None })
    }

    fn rank_document_scope_inner(&self, query: &str, opts: &SearchOptions, order: Order) -> Result<Ranking> {
        let mut budget = Budget::new(opts);
        budget.check()?;
        let parsed = self.resolve_opts(query, opts)?;
        if parsed.words.is_empty() {
            let result = SearchResult {
                words: parsed.words,
                dropped: parsed.dropped,
                generation: self.generation,
                ..Default::default()
            };
            return Ok(Ranking {
                result,
                slots: Vec::new(),
                sets: Vec::new(),
                group_sets: None,
            });
        }

        let static_rank = if opts.top == 0 { Some(self.static_rank()?) } else { None };
        let mut found = Collector {
            top: opts.top,
            order,
            best: Vec::new(),
            all: Vec::new(),
            ranks: static_rank.map_or(&[], |r| &r.per_view),
            total_docs: 0,
            adjacent_docs: 0,
        };
        let mut total_sentences = 0u64;
        let mut all_sets = Vec::with_capacity(self.views.len());

        for (vi, view) in self.views.iter().enumerate() {
            budget.check()?;
            let cand = candidate_docs_for_query(view, &parsed.words, opts)?;
            if cand.as_ref().is_some_and(|c| c.is_empty()) {
                all_sets.push(Arc::new((RoaringBitmap::new(), None)));
                continue;
            }

            let mut word_doc_sets: Vec<RoaringBitmap> = Vec::with_capacity(parsed.words.len());
            let mut word_sentence_sets: Vec<RoaringBitmap> = Vec::with_capacity(parsed.words.len());
            let tables = view.seg.tables();

            for w in &parsed.words {
                let sents = eval_seg(&view.seg, std::slice::from_ref(w), None, &mut budget)?.0;
                let sents = &sents - &view.deleted;
                let mut docs = RoaringBitmap::new();
                for sid in &sents {
                    if let Some(d) = tables.doc_of(sid) {
                        docs.insert(d);
                    }
                }
                word_doc_sets.push(docs);
                word_sentence_sets.push(sents);
            }

            let mut doc_matches = word_doc_sets.first().cloned().unwrap_or_default();
            for ds in &word_doc_sets[1..] {
                doc_matches &= ds;
            }
            if let Some(c) = cand {
                doc_matches &= c;
            }

            let mut combined_sents = RoaringBitmap::new();
            for ws in &word_sentence_sets {
                combined_sents |= ws;
            }

            let mut adjacent_sents = word_sentence_sets.first().cloned().unwrap_or_default();
            for ws in &word_sentence_sets[1..] {
                adjacent_sents &= ws;
            }

            for local in &doc_matches {
                let Some(rec) = tables.doc(local) else { continue };
                let mut doc_sent_count = 0u32;
                let mut best_level = u8::MAX;
                let mut doc_adjacent = false;

                for sid in rec.first_sid..rec.first_sid + rec.n_sent {
                    if combined_sents.contains(sid) {
                        doc_sent_count += 1;
                        if let Some(lv) = tables.level(sid) {
                            best_level = best_level.min(lv);
                        }
                    }
                    if adjacent_sents.contains(sid) {
                        doc_adjacent = true;
                    }
                }

                if doc_sent_count > 0 {
                    total_sentences += doc_sent_count as u64;
                    let mut a = Agg {
                        key: 0,
                        doc_id: rec.doc_id,
                        date: rec.date,
                        view: vi as u32,
                        local,
                    };
                    a.set(doc_adjacent, best_level, doc_sent_count);
                    found.push(a)?;
                }
            }

            all_sets.push(Arc::new((combined_sents, Some(adjacent_sents))));
        }

        let (total_docs, adjacent_docs) = (found.total_docs, found.adjacent_docs);
        let (mut hits, mut slots) = (Vec::new(), Vec::new());
        if let Some(sr) = static_rank {
            let mut all = found.all;
            all.sort_unstable();
            hits.reserve(all.len());
            slots.reserve(all.len());
            for k in all {
                let Some(&(view, local)) = sr.inverse.get(k as u32 as usize) else { return corrupt("static rank out of range") };
                let Some(rec) = self.views.get(view as usize).and_then(|v| v.seg.tables().doc(local)) else { return corrupt("document out of range") };
                let skey = (k >> 32) as u64;
                let key = if order == Order::Hierarchy { relevance_key(skey) } else { skey };
                let a = Agg { key, doc_id: rec.doc_id, date: rec.date, view, local };
                hits.push(DocHit { doc_id: a.doc_id, adjacent: a.adjacent(), level: a.level(), count: a.count(), date: a.date, sentences: Vec::new() });
                slots.push((view, local));
            }
        } else {
            for a in &found.best {
                hits.push(DocHit { doc_id: a.doc_id, adjacent: a.adjacent(), level: a.level(), count: a.count(), date: a.date, sentences: Vec::new() });
                slots.push((a.view, a.local));
            }
        }
        let result = SearchResult { words: parsed.words, dropped: parsed.dropped, total_docs, total_sentences, adjacent_docs, hits, generation: self.generation };
        Ok(Ranking { result, slots, sets: all_sets, group_sets: None })
    }

    /// 개수만: 일치 문장 수와 문서 수. 문서별 모으기·순위·미리보기를 하지 않는다(입력 도중 개수 표시용,
    /// 상위 N건은 입력이 멈춘 뒤 `search`로). `opts.top`·`refs_per_doc`은 쓰지 않는다.
    pub fn count(&self, query: &str, opts: &SearchOptions) -> Result<SearchCounts> {
        guard(|| self.count_inner(query, opts))
    }

    fn count_inner(&self, query: &str, opts: &SearchOptions) -> Result<SearchCounts> {
        let mut budget = Budget::new(opts);
        budget.check()?;
        let parsed = self.resolve_opts(query, opts)?;
        let sets = self.eval_all(&parsed, opts, &mut budget)?;
        let (mut total_sentences, mut total_docs) = (0u64, 0usize);
        for (view, sets_v) in self.views.iter().zip(&sets) {
            let matched = &sets_v.0;
            if matched.is_empty() {
                continue;
            }
            total_sentences += matched.len();
            let tables = view.seg.tables();
            // 문서의 문장은 이어져 있다: 문서마다 첫 일치 문장에서 문서 끝으로 건너뛴다.
            let mut it = matched.iter();
            while let Some(sid) = it.next() {
                budget.tick()?;
                let Some(rec) = tables.doc_of(sid).and_then(|local| tables.doc(local)) else { return corrupt("document out of range") };
                if sid < rec.first_sid || sid - rec.first_sid >= rec.n_sent {
                    return corrupt("sentence outside its document range");
                }
                total_docs += 1;
                it.advance_to(rec.first_sid + rec.n_sent);
            }
        }
        budget.check()?;
        Ok(SearchCounts { words: parsed.words, dropped: parsed.dropped, total_sentences, total_docs, generation: self.generation })
    }

    /// 일치 문장 전부(문서 ID, 문장 순번 오름차순). `opts.top`·`refs_per_doc`은 쓰지 않는다.
    pub fn match_sentences(&self, query: &str, opts: &SearchOptions) -> Result<Vec<SentenceMatch>> {
        guard(|| {
            let mut budget = Budget::new(opts);
            let parsed = self.resolve_opts(query, opts)?;
            let sets = self.eval_all(&parsed, opts, &mut budget)?;
            let mut out = Vec::new();
            for (view, sets_v) in self.views.iter().zip(&sets) {
                let (matched, adjacent) = &**sets_v;
                let seg = &*view.seg;
                for sid in matched {
                    budget.tick()?;
                    let rec = seg.doc(seg.doc_of(sid)?)?;
                    out.push(SentenceMatch {
                        doc_id: rec.doc_id,
                        sentence: SentenceRef { ord: sid - rec.first_sid, para: seg.para(sid)?, level: seg.level(sid)?, adjacent: adjacent.as_ref().is_none_or(|b| b.contains(sid)) },
                    });
                }
            }
            out.sort_unstable_by_key(|m| (m.doc_id, m.sentence.ord));
            Ok(out)
        })
    }

    /// 연관 검색어(`;`) 묶음 검색. 각 묶음마다 문장 단위 검색을 하고 문서 단위 교집합을 낸다.
    ///
    /// - `;`가 없으면 `search`와 결과가 완전히 같다(결과 차이 0).
    /// - 순위: 묶음 중 최고 위계 > 묶음별 일치 문장 수 합 > 날짜 최신 > 문서 ID.
    /// - 미리보기: 묶음마다 최고 문장 1개.
    /// - 제외어(`SearchOptions::deny`)는 문서 전체 기준으로 적용된다.
    pub fn search_groups(&self, query: &str, opts: &SearchOptions) -> Result<SearchResult> {
        guard(|| self.search_groups_inner_by(query, opts, Order::Relevance))
    }

    /// `search_groups`와 같되 결과 순서를 고른다.
    pub fn search_groups_by(&self, query: &str, opts: &SearchOptions, order: Order) -> Result<SearchResult> {
        guard(|| self.search_groups_inner_by(query, opts, order))
    }

    /// `search_groups`와 같은 순위를 내되 미리보기 문장은 만들지 않는다.
    pub fn rank_groups(&self, query: &str, opts: &SearchOptions) -> Result<Ranking> {
        guard(|| self.rank_groups_inner(query, opts, Order::Relevance))
    }

    /// `rank_groups`와 같되 결과 순서를 고른다.
    pub fn rank_groups_by(&self, query: &str, opts: &SearchOptions, order: Order) -> Result<Ranking> {
        guard(|| self.rank_groups_inner(query, opts, order))
    }

    /// 연관 검색어(`;`) 개수만: 교집합 문서 수와 일치 문장 수 총합.
    pub fn count_groups(&self, query: &str, opts: &SearchOptions) -> Result<SearchCounts> {
        guard(|| self.count_groups_inner(query, opts))
    }

    fn search_groups_inner_by(&self, query: &str, opts: &SearchOptions, order: Order) -> Result<SearchResult> {
        let groups = parse_groups(query);
        if groups.len() <= 1 {
            let single = groups.first().copied().unwrap_or(query);
            return self.search_inner_by(single, opts, order);
        }

        let ranking = self.rank_groups_inner(query, opts, order)?;
        let mut result = ranking.result;

        if opts.refs_per_doc > 0 {
            if let Some(group_sets) = &ranking.group_sets {
                for (hit, &(view, local)) in result.hits.iter_mut().zip(&ranking.slots) {
                    let mut sentences = Vec::with_capacity(group_sets.len());
                    for g_sets in group_sets {
                        if sentences.len() >= opts.refs_per_doc {
                            break;
                        }
                        if let Some(s) = self.doc_best_sentence(g_sets, view, local)? {
                            sentences.push(s);
                        }
                    }
                    hit.sentences = sentences;
                }
            }
        }

        Ok(result)
    }

    fn rank_groups_inner(&self, query: &str, opts: &SearchOptions, order: Order) -> Result<Ranking> {
        let groups = parse_groups(query);
        if groups.len() <= 1 {
            let single = groups.first().copied().unwrap_or(query);
            return self.rank_inner(single, opts, order);
        }

        // 2단계 최적화: 문서 비트맵으로 후보 문서 교집합 사전 검사
        // (scope == SearchScope::Folder일 때는 문서 단위 교집합이 아니라 폴더 단위이므로 건너뛴다)
        if opts.scope != SearchScope::Folder {
            let mut any_common = false;
            for view in &self.views {
                let mut seg_cand: Option<RoaringBitmap> = None;
                for g in &groups {
                    if let Ok(parsed) = self.resolve_opts(g, opts) {
                        if let Ok(Some(cand)) = candidate_docs_for_query(view, &parsed.words, opts) {
                            seg_cand = Some(match seg_cand {
                                None => cand,
                                Some(mut sc) => {
                                    sc &= cand;
                                    sc
                                }
                            });
                        }
                    }
                }
                if seg_cand.as_ref().is_none_or(|c| !c.is_empty()) {
                    any_common = true;
                    break;
                }
            }
            if !any_common && !self.views.is_empty() {
                let mut words = Vec::new();
                let mut dropped = Vec::new();
                for grp in &groups {
                    if let Ok(parsed) = self.resolve_opts(grp, opts) {
                        for w in parsed.words {
                            if !words.iter().any(|existing: &QueryWord| existing.text == w.text) {
                                words.push(w);
                            }
                        }
                        for d in parsed.dropped {
                            if !dropped.contains(&d) {
                                dropped.push(d);
                            }
                        }
                    }
                }
                let result = SearchResult {
                    words,
                    dropped,
                    total_docs: 0,
                    total_sentences: 0,
                    adjacent_docs: 0,
                    hits: Vec::new(),
                    generation: self.generation,
                };
                return Ok(Ranking {
                    result,
                    slots: Vec::new(),
                    sets: Vec::new(),
                    group_sets: None,
                });
            }
        }

        let opts_all = SearchOptions { top: 0, refs_per_doc: 0, ..opts.clone() };
        let mut group_rankings = Vec::with_capacity(groups.len());
        for g in &groups {
            let ranking = self.rank_inner(g, &opts_all, order)?;
            if ranking.result.hits.is_empty() {
                let mut words = Vec::new();
                let mut dropped = Vec::new();
                for grp in &groups {
                    if let Ok(parsed) = self.resolve_opts(grp, opts) {
                        for w in parsed.words {
                            if !words.iter().any(|existing: &QueryWord| existing.text == w.text) {
                                words.push(w);
                            }
                        }
                        for d in parsed.dropped {
                            if !dropped.contains(&d) {
                                dropped.push(d);
                            }
                        }
                    }
                }
                let result = SearchResult {
                    words,
                    dropped,
                    total_docs: 0,
                    total_sentences: 0,
                    adjacent_docs: 0,
                    hits: Vec::new(),
                    generation: self.generation,
                };
                return Ok(Ranking {
                    result,
                    slots: Vec::new(),
                    sets: Vec::new(),
                    group_sets: None,
                });
            }
            group_rankings.push(ranking);
        }

        struct DocAcc {
            view: u32,
            local: u32,
            date: i64,
            best_level: u8,
            sum_count: u32,
            all_adjacent: bool,
            match_count: usize,
        }

        let mut acc: std::collections::HashMap<u64, DocAcc> = std::collections::HashMap::new();
        let n_groups = groups.len();

        if opts.scope == SearchScope::Folder {
            // 폴더 단위 교집합 (scope=folder, 요구사항 5)
            let mut group_folders: Vec<HashMap<String, Vec<(usize, u64)>>> = Vec::new();
            for ranking in &group_rankings {
                let mut folders_for_g: HashMap<String, Vec<(usize, u64)>> = HashMap::new();
                for (hit_idx, (hit, &(view_idx, local))) in ranking.result.hits.iter().zip(&ranking.slots).enumerate() {
                    if let Some(view) = self.views.get(view_idx as usize) {
                        if let Ok(folders) = view.seg.folders() {
                            for f in folders {
                                if let Ok(Some(f_bm)) = view.seg.folder_posting(&f) {
                                    if f_bm.contains(local) {
                                        folders_for_g.entry(f).or_default().push((hit_idx, hit.doc_id));
                                    }
                                }
                            }
                        }
                    }
                }
                group_folders.push(folders_for_g);
            }

            if let Some(first_folders) = group_folders.first() {
                let mut common_folders: std::collections::HashSet<String> = first_folders.keys().cloned().collect();
                for other in &group_folders[1..] {
                    common_folders.retain(|f| other.contains_key(f));
                }

                if !common_folders.is_empty() {
                    let mut admitted_doc_ids: std::collections::HashSet<u64> = std::collections::HashSet::new();
                    for f in &common_folders {
                        for g_map in &group_folders {
                            if let Some(hits) = g_map.get(f) {
                                for &(_, doc_id) in hits {
                                    admitted_doc_ids.insert(doc_id);
                                }
                            }
                        }
                    }

                    for ranking in &group_rankings {
                        for (hit, &(view, local)) in ranking.result.hits.iter().zip(&ranking.slots) {
                            if admitted_doc_ids.contains(&hit.doc_id) {
                                let entry = acc.entry(hit.doc_id).or_insert(DocAcc {
                                    view,
                                    local,
                                    date: hit.date,
                                    best_level: hit.level,
                                    sum_count: 0,
                                    all_adjacent: hit.adjacent,
                                    match_count: n_groups,
                                });
                                entry.best_level = entry.best_level.min(hit.level);
                                entry.sum_count += hit.count;
                                entry.all_adjacent &= hit.adjacent;
                            }
                        }
                    }
                }
            }
        } else {
            let first = &group_rankings[0];
            for (hit, &(view, local)) in first.result.hits.iter().zip(&first.slots) {
                acc.insert(hit.doc_id, DocAcc {
                    view,
                    local,
                    date: hit.date,
                    best_level: hit.level,
                    sum_count: hit.count,
                    all_adjacent: hit.adjacent,
                    match_count: 1,
                });
            }

            for ranking in &group_rankings[1..] {
                for hit in &ranking.result.hits {
                    if let Some(entry) = acc.get_mut(&hit.doc_id) {
                        entry.best_level = entry.best_level.min(hit.level);
                        entry.sum_count += hit.count;
                        entry.all_adjacent &= hit.adjacent;
                        entry.match_count += 1;
                    }
                }
            }

            acc.retain(|_, v| v.match_count == n_groups);
        }

        let total_docs = acc.len();
        let mut adjacent_docs = 0usize;
        let mut total_sentences = 0u64;

        struct IntersectedDoc {
            key: u64,
            doc_id: u64,
            date: i64,
            view: u32,
            local: u32,
            best_level: u8,
            sum_count: u32,
            adjacent: bool,
        }

        let mut candidates: Vec<IntersectedDoc> = Vec::with_capacity(total_docs);
        for (&doc_id, entry) in &acc {
            total_sentences += entry.sum_count as u64;
            if entry.all_adjacent {
                adjacent_docs += 1;
            }
            let key = ((entry.best_level as u64) << 32) | (u32::MAX - entry.sum_count) as u64;
            candidates.push(IntersectedDoc {
                key,
                doc_id,
                date: entry.date,
                view: entry.view,
                local: entry.local,
                best_level: entry.best_level,
                sum_count: entry.sum_count,
                adjacent: entry.all_adjacent,
            });
        }

        // 순위: 묶음 중 최고 위계 > 묶음별 일치 문장 수 합 > 날짜 > 문서 ID
        candidates.sort_unstable_by(|a, b| {
            a.key.cmp(&b.key)
                .then_with(|| b.date.cmp(&a.date))
                .then_with(|| a.doc_id.cmp(&b.doc_id))
        });

        if opts.top != 0 && candidates.len() > opts.top {
            candidates.truncate(opts.top);
        }

        let mut hits = Vec::with_capacity(candidates.len());
        let mut slots = Vec::with_capacity(candidates.len());
        for c in candidates {
            hits.push(DocHit {
                doc_id: c.doc_id,
                adjacent: c.adjacent,
                level: c.best_level,
                count: c.sum_count,
                date: c.date,
                sentences: Vec::new(),
            });
            slots.push((c.view, c.local));
        }

        let mut words = Vec::new();
        let mut dropped = Vec::new();
        for r in &group_rankings {
            for w in &r.result.words {
                if !words.iter().any(|existing: &QueryWord| existing.text == w.text) {
                    words.push(w.clone());
                }
            }
            for d in &r.result.dropped {
                if !dropped.contains(d) {
                    dropped.push(d.clone());
                }
            }
        }

        let group_sets = Some(group_rankings.into_iter().map(|r| r.sets).collect());

        let result = SearchResult {
            words,
            dropped,
            total_docs,
            total_sentences,
            adjacent_docs,
            hits,
            generation: self.generation,
        };

        Ok(Ranking {
            result,
            slots,
            sets: Vec::new(),
            group_sets,
        })
    }

    fn count_groups_inner(&self, query: &str, opts: &SearchOptions) -> Result<SearchCounts> {
        let groups = parse_groups(query);
        if groups.len() <= 1 {
            let single = groups.first().copied().unwrap_or(query);
            return self.count_inner(single, opts);
        }

        let ranking = self.rank_groups_inner(query, opts, Order::Relevance)?;
        Ok(SearchCounts {
            words: ranking.result.words,
            dropped: ranking.result.dropped,
            total_sentences: ranking.result.total_sentences,
            total_docs: ranking.result.total_docs,
            generation: ranking.result.generation,
        })
    }

    /// 폴더별 결과 수 집계 API (요구사항 5).
    pub fn count_by_folder(&self, query: &str, opts: &SearchOptions) -> Result<Vec<FolderCount>> {
        guard(|| {
            let ranking = self.rank_groups_inner(query, &SearchOptions { top: 0, refs_per_doc: 0, ..opts.clone() }, Order::Relevance)?;
            let mut seg_hits: HashMap<u32, RoaringBitmap> = HashMap::new();
            let mut hit_counts: HashMap<(u32, u32), u32> = HashMap::new();

            for (hit, &(view, local)) in ranking.result.hits.iter().zip(&ranking.slots) {
                seg_hits.entry(view).or_default().insert(local);
                hit_counts.insert((view, local), hit.count);
            }

            let mut folder_counts: HashMap<String, (u32, u64)> = HashMap::new();

            for (&view_idx, hit_locals) in &seg_hits {
                if let Some(view) = self.views.get(view_idx as usize) {
                    let folders = view.seg.folders()?;
                    for f in folders {
                        if let Some(f_bm) = view.seg.folder_posting(&f)? {
                            let common = hit_locals & &f_bm;
                            if !common.is_empty() {
                                let doc_c = common.len() as u32;
                                let mut sent_c = 0u64;
                                for local in &common {
                                    if let Some(&c) = hit_counts.get(&(view_idx, local)) {
                                        sent_c += c as u64;
                                    }
                                }
                                let entry = folder_counts.entry(f).or_insert((0, 0));
                                entry.0 += doc_c;
                                entry.1 += sent_c;
                            }
                        }
                    }
                }
            }

            let mut out: Vec<FolderCount> = folder_counts.into_iter().map(|(folder, (doc_count, sentence_count))| {
                FolderCount { folder, doc_count, sentence_count }
            }).collect();

            out.sort_unstable_by(|a, b| b.doc_count.cmp(&a.doc_count).then_with(|| a.folder.cmp(&b.folder)));
            Ok(out)
        })
    }

    /// 날짜별 결과 수 집계 API (요구사항 6, 막대그래프용).
    pub fn count_by_date_histogram(&self, query: &str, opts: &SearchOptions, interval: DateInterval) -> Result<Vec<DateCount>> {
        guard(|| {
            let ranking = self.rank_groups_inner(query, &SearchOptions { top: 0, refs_per_doc: 0, ..opts.clone() }, Order::Relevance)?;
            let mut buckets: BTreeMap<u32, (String, u32, u64)> = BTreeMap::new();

            for hit in &ranking.result.hits {
                let (y, m, d) = normalize_date(hit.date);
                let (key, label) = match interval {
                    DateInterval::Year => (y, format!("{y}")),
                    DateInterval::Month => (ym_to_key(y, m), format!("{y}-{m:02}")),
                    DateInterval::Week => {
                        let doy = (m - 1) * 30 + d;
                        let w = (doy / 7) + 1;
                        (y * 100 + w, format!("{y}-W{w:02}"))
                    }
                    DateInterval::Day => (ymd_to_key(y, m, d), format!("{y}-{m:02}-{d:02}")),
                };
                let entry = buckets.entry(key).or_insert((label, 0, 0));
                entry.1 += 1;
                entry.2 += hit.count as u64;
            }

            let out = buckets.into_iter().map(|(date_key, (label, doc_count, sentence_count))| {
                DateCount { date_key, label, doc_count, sentence_count }
            }).collect();
            Ok(out)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover() {
        assert_eq!(cover_offsets(4), [0, 1]);
        assert_eq!(cover_offsets(6), [0, 3]);
        assert_eq!(cover_offsets(7), [0, 3, 4]);
    }

    #[test]
    fn hierarchy_key_round_trip_and_order() {
        let mk = |adj: bool, level: u8, count: u32| {
            let mut a = Agg { key: 0, doc_id: 0, date: 0, view: 0, local: 0 };
            a.set(adj, level, count);
            a.key
        };
        for k in [mk(true, 0, 1), mk(false, 255, u32::MAX), mk(false, 3, 7), mk(true, 9, 0)] {
            assert_eq!(relevance_key(hierarchy_key(k)), k);
        }
        // 위계가 붙어 있는 일치보다 앞선다.
        assert!(hierarchy_key(mk(false, 1, 1)) < hierarchy_key(mk(true, 2, 99)));
        assert!(hierarchy_key(mk(true, 1, 1)) < hierarchy_key(mk(false, 1, 99)));
        assert!(hierarchy_key(mk(true, 1, 5)) < hierarchy_key(mk(true, 1, 4)));
    }
}
