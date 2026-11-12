//! 읽기 쪽: 목록 파일의 한 세대를 열어 질의한다. 쓰는 쪽이 목록을 바꿔도 이미 연 세대는 그대로 유효하다.
use crate::manifest::Manifest;
use crate::query::{MIN_WORD_CHARS, ParsedQuery, QueryWord, SplitMode, char_bounds, is_word_or_stem, parse_query, split_bigram, split_dict};
use crate::segment::{Segment, seg_file_name, word_key};
use crate::{Error, Result, corrupt, guard};
use roaring::RoaringBitmap;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::time::{Duration, Instant};

pub(crate) struct SegView {
    pub seg: Arc<Segment>,
    pub deleted: RoaringBitmap,
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
}

impl Default for SearchOptions {
    fn default() -> Self {
        SearchOptions { top: 20, split: SplitMode::DictLongest, time_limit: Some(Duration::from_secs(2)), cancel: None, refs_per_doc: 3 }
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

/// 조각 하나에서 (일치 문장, 붙어 있는 일치 문장). 두 번째가 None이면 첫 번째와 같다.
fn eval(view: &SegView, words: &[QueryWord], budget: &mut Budget) -> Result<(RoaringBitmap, Option<RoaringBitmap>)> {
    let seg = &*view.seg;
    let is_short = |w: &QueryWord| w.pieces.is_none() && w.text.chars().count() <= 3;
    let mut acc: Option<RoaringBitmap> = None;
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
    let mut matched = acc.unwrap_or_default();
    if !view.deleted.is_empty() {
        matched -= &view.deleted;
        if let Some(sa) = strict_acc.as_mut() {
            *sa -= &view.deleted;
        }
    }
    Ok((matched, strict_acc))
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
            self.all.push(full_key(a.key, r));
            return Ok(());
        }
        if self.best.len() == self.top {
            match self.best.last() {
                Some(worst) if order(&a, worst) == Ordering::Less => {
                    self.best.pop();
                }
                _ => return Ok(()),
            }
        }
        let pos = self.best.partition_point(|b| order(b, &a) == Ordering::Less);
        self.best.insert(pos, a);
        Ok(())
    }
}

/// 순위: 붙어 있는 일치 > 위계(작은 값) > 일치 문장 수 > 날짜 최신 > 문서 ID.
fn order(a: &Agg, b: &Agg) -> Ordering {
    a.key.cmp(&b.key).then_with(|| b.date.cmp(&a.date)).then_with(|| a.doc_id.cmp(&b.doc_id))
}

/// 미리보기 문장 없이 순위만 낸 결과. 미리보기는 화면에 보일 문서만 `sentences`로 만든다.
pub struct Ranking {
    /// `hits`의 `sentences`는 비어 있다.
    pub result: SearchResult,
    /// hits와 같은 순서의 (조각, 조각 안 문서 번호).
    slots: Vec<(u32, u32)>,
    /// 조각별 (일치 문장, 붙어 있는 일치 문장).
    sets: Vec<(RoaringBitmap, Option<RoaringBitmap>)>,
}

impl Ranking {
    /// `hit`번째 문서의 앞에서부터 `n`개 일치 문장. `index`는 이 순위를 낸 색인(같은 세대)이어야 한다.
    pub fn sentences(&self, index: &Index, hit: usize, n: usize) -> Result<Vec<SentenceRef>> {
        if index.generation != self.result.generation || index.views.len() != self.sets.len() {
            return Err(Error::Invalid("ranking belongs to another index generation".into()));
        }
        let Some(&(view, local)) = self.slots.get(hit) else { return Err(Error::Invalid(format!("hit {hit} out of range"))) };
        guard(|| index.doc_sentences(&self.sets, view, local, n))
    }
}

/// 열려 있는 색인의 한 세대.
pub struct Index {
    dir: PathBuf,
    generation: u64,
    views: Vec<SegView>,
    ranks: OnceLock<StaticRank>,
}

impl Index {
    /// 색인 폴더를 연다. 목록 파일이 없으면 빈 색인(세대 0).
    pub fn open(dir: &Path) -> Result<Index> {
        guard(|| {
            let (manifest, segs) = load_views(dir, &[])?;
            Ok(Index::from_parts(dir, manifest, segs))
        })
    }

    fn from_parts(dir: &Path, manifest: Manifest, segs: Vec<Arc<Segment>>) -> Index {
        let views = segs.into_iter().zip(manifest.segs).map(|(seg, e)| SegView { seg, deleted: e.deleted }).collect();
        Index { dir: dir.to_path_buf(), generation: manifest.generation, views, ranks: OnceLock::new() }
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
    fn doc_sentences(&self, sets: &[(RoaringBitmap, Option<RoaringBitmap>)], view: u32, local: u32, n: usize) -> Result<Vec<SentenceRef>> {
        let mut out = Vec::new();
        if n == 0 {
            return Ok(out);
        }
        let (matched, adjacent) = &sets[view as usize];
        let seg = &*self.views[view as usize].seg;
        let rec = seg.doc(local)?;
        for sid in matched.range(rec.first_sid..rec.first_sid + rec.n_sent).take(n) {
            out.push(SentenceRef { ord: sid - rec.first_sid, para: seg.para(sid)?, level: seg.level(sid)?, adjacent: adjacent.as_ref().is_none_or(|b| b.contains(sid)) });
        }
        Ok(out)
    }

    /// 목록 파일이 바뀌었으면 새 세대로 바꾼다(이미 연 조각은 다시 열지 않는다). 바뀌었으면 true.
    pub fn reload(&mut self) -> Result<bool> {
        let reuse: Vec<Arc<Segment>> = self.views.iter().map(|v| v.seg.clone()).collect();
        let dir = self.dir.clone();
        let (manifest, segs) = guard(|| load_views(&dir, &reuse))?;
        if manifest.generation == self.generation {
            return Ok(false);
        }
        *self = Index::from_parts(&dir, manifest, segs);
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

    fn eval_all(&self, parsed: &ParsedQuery, budget: &mut Budget) -> Result<Vec<(RoaringBitmap, Option<RoaringBitmap>)>> {
        let mut out = Vec::with_capacity(self.views.len());
        for v in &self.views {
            budget.check()?;
            out.push(if parsed.words.is_empty() { (RoaringBitmap::new(), None) } else { eval(v, &parsed.words, budget)? });
        }
        Ok(out)
    }

    /// 문장 단위 검색. 같은 문장에 모든 낱말이 있는 문장을 찾아 문서별로 모으고 순위를 매겨 상위 `opts.top`건을 돌려준다.
    /// 검색 낱말이 하나도 없으면(모두 2글자 미만) 빈 결과.
    pub fn search(&self, query: &str, opts: &SearchOptions) -> Result<SearchResult> {
        guard(|| self.search_inner(query, opts))
    }

    fn search_inner(&self, query: &str, opts: &SearchOptions) -> Result<SearchResult> {
        let ranking = self.rank_inner(query, opts)?;
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
        guard(|| self.rank_inner(query, opts))
    }

    fn rank_inner(&self, query: &str, opts: &SearchOptions) -> Result<Ranking> {
        let mut budget = Budget::new(opts);
        budget.check()?;
        let parsed = self.resolve_inner(query, opts.split)?;
        let sets = self.eval_all(&parsed, &mut budget)?;
        let static_rank = if opts.top == 0 && sets.iter().any(|(m, _)| !m.is_empty()) { Some(self.static_rank()?) } else { None };
        let mut found = Collector { top: opts.top, best: Vec::new(), all: Vec::new(), ranks: static_rank.map_or(&[], |r| &r.per_view), total_docs: 0, adjacent_docs: 0 };
        let mut total_sentences = 0u64;
        for (vi, (view, (matched, adjacent))) in self.views.iter().zip(&sets).enumerate() {
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
                let a = Agg { key: (k >> 32) as u64, doc_id: rec.doc_id, date: rec.date, view, local };
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
        Ok(Ranking { result, slots, sets })
    }

    /// 개수만: 일치 문장 수와 문서 수. 문서별 모으기·순위·미리보기를 하지 않는다(입력 도중 개수 표시용,
    /// 상위 N건은 입력이 멈춘 뒤 `search`로). `opts.top`·`refs_per_doc`은 쓰지 않는다.
    pub fn count(&self, query: &str, opts: &SearchOptions) -> Result<SearchCounts> {
        guard(|| self.count_inner(query, opts))
    }

    fn count_inner(&self, query: &str, opts: &SearchOptions) -> Result<SearchCounts> {
        let mut budget = Budget::new(opts);
        budget.check()?;
        let parsed = self.resolve_inner(query, opts.split)?;
        let sets = self.eval_all(&parsed, &mut budget)?;
        let (mut total_sentences, mut total_docs) = (0u64, 0usize);
        for (view, (matched, _)) in self.views.iter().zip(&sets) {
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
            let parsed = self.resolve_inner(query, opts.split)?;
            let sets = self.eval_all(&parsed, &mut budget)?;
            let mut out = Vec::new();
            for (view, (matched, adjacent)) in self.views.iter().zip(&sets) {
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
}
