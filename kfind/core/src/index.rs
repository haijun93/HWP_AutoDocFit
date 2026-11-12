//! 읽기 쪽: 목록 파일의 한 세대를 열어 질의한다. 쓰는 쪽이 목록을 바꿔도 이미 연 세대는 그대로 유효하다.
use crate::manifest::Manifest;
use crate::query::{MIN_WORD_CHARS, ParsedQuery, QueryWord, SplitMode, char_bounds, parse_query, split_bigram, split_dict};
use crate::segment::{Segment, seg_file_name, word_key};
use crate::{Error, Result, corrupt, guard};
use roaring::RoaringBitmap;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
    // 후보 문장의 비교형 글에서 실제로 이어져 있는지 확인.
    let finder = memchr::memmem::Finder::new(w.as_bytes());
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

/// 문서 순위 모으기. `top`이 0이면 전부 모으고, 아니면 가장 앞선 `top`개만 순위 순서로 쥐고 있는다
/// (문서 전체를 쌓았다가 고르지 않는다 — 상위 N건을 먼저 낼 때의 비용을 줄인다).
struct Collector {
    top: usize,
    best: Vec<Agg>,
    total_docs: usize,
    adjacent_docs: usize,
}

impl Collector {
    #[inline]
    fn push(&mut self, a: Agg) {
        self.total_docs += 1;
        self.adjacent_docs += a.adjacent() as usize;
        if self.top == 0 {
            self.best.push(a);
            return;
        }
        if self.best.len() == self.top {
            match self.best.last() {
                Some(worst) if order(&a, worst) == Ordering::Less => {
                    self.best.pop();
                }
                _ => return,
            }
        }
        let pos = self.best.partition_point(|b| order(b, &a) == Ordering::Less);
        self.best.insert(pos, a);
    }
}

/// 순위: 붙어 있는 일치 > 위계(작은 값) > 일치 문장 수 > 날짜 최신 > 문서 ID.
fn order(a: &Agg, b: &Agg) -> Ordering {
    a.key.cmp(&b.key).then(b.date.cmp(&a.date)).then(a.doc_id.cmp(&b.doc_id))
}

/// 열려 있는 색인의 한 세대.
pub struct Index {
    dir: PathBuf,
    generation: u64,
    views: Vec<SegView>,
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
        Index { dir: dir.to_path_buf(), generation: manifest.generation, views }
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

    /// 살아 있는 문장에 이 낱말(어절, 비교형)이 있는지.
    fn has_word(&self, w: &str) -> Result<bool> {
        let key = word_key(w);
        for v in &self.views {
            if let Some(bm) = v.seg.posting(&key)? {
                if v.deleted.is_empty() || !bm.is_subset(&v.deleted) {
                    return Ok(true);
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
        let first = split_dict(w, |p| match self.has_word(p) {
            Ok(found) => found,
            Err(e) => {
                failed.get_or_insert(e);
                false
            }
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
        let mut budget = Budget::new(opts);
        budget.check()?;
        let parsed = self.resolve_inner(query, opts.split)?;
        let sets = self.eval_all(&parsed, &mut budget)?;
        let mut found = Collector { top: opts.top, best: Vec::new(), total_docs: 0, adjacent_docs: 0 };
        let mut total_sentences = 0u64;
        for (vi, (view, (matched, adjacent))) in self.views.iter().zip(&sets).enumerate() {
            if matched.is_empty() {
                continue;
            }
            total_sentences += matched.len();
            let tables = view.seg.tables();
            if opts.top == 0 {
                found.best.reserve((matched.len().min(view.seg.n_docs as u64)) as usize);
            }
            // 붙어 있는 일치 문장은 일치 문장의 부분집합이고 둘 다 오름차순이라 나란히 훑는다.
            let mut adj = adjacent.as_ref().map(|b| b.iter().peekable());
            // 한 문서의 문장은 조각 안에서 이어져 있고 문장 번호는 오름차순으로 나온다: 지금 문서의 끝 번호를
            // 넘을 때만 문서 표를 찾는다. 일치 문장 수와 가장 높은 위계는 지역 변수에 모았다가 문서가 바뀔 때 적는다.
            let (mut doc_end, mut count, mut best) = (0u32, 0u32, u8::MAX);
            let mut cur: Option<Agg> = None;
            let mut finish = |cur: &mut Option<Agg>, doc_end: u32, count: u32, best: u8| {
                if let Some(mut a) = cur.take() {
                    let mut adjacent_hit = adj.is_none();
                    if let Some(it) = adj.as_mut() {
                        while it.next_if(|s| *s < doc_end).is_some() {
                            adjacent_hit = true;
                        }
                    }
                    a.set(adjacent_hit, best, count);
                    found.push(a);
                }
            };
            for sid in matched {
                let Some(lv) = tables.level(sid) else { return corrupt("matched sentence out of range") };
                if sid < doc_end {
                    count += 1;
                    best = best.min(lv);
                    continue;
                }
                budget.tick()?;
                finish(&mut cur, doc_end, count, best);
                let Some((local, rec)) = tables.doc_of(sid).and_then(|local| tables.doc(local).map(|r| (local, r))) else { return corrupt("document out of range") };
                if sid < rec.first_sid || sid - rec.first_sid >= rec.n_sent {
                    return corrupt("sentence outside its document range");
                }
                doc_end = rec.first_sid + rec.n_sent;
                (count, best) = (1, lv);
                cur = Some(Agg { key: 0, doc_id: rec.doc_id, date: rec.date, view: vi as u32, local });
            }
            finish(&mut cur, doc_end, count, best);
        }
        budget.check()?;
        let (total_docs, adjacent_docs) = (found.total_docs, found.adjacent_docs);
        let mut aggs = found.best;
        if opts.top == 0 {
            aggs.sort_unstable_by(order);
        }
        let mut hits = Vec::with_capacity(aggs.len());
        for a in &aggs {
            let mut sentences = Vec::new();
            if opts.refs_per_doc > 0 {
                let (matched, adjacent) = &sets[a.view as usize];
                let seg = &*self.views[a.view as usize].seg;
                let rec = seg.doc(a.local)?;
                for sid in matched.range(rec.first_sid..rec.first_sid + rec.n_sent).take(opts.refs_per_doc) {
                    sentences.push(SentenceRef { ord: sid - rec.first_sid, para: seg.para(sid)?, level: seg.level(sid)?, adjacent: adjacent.as_ref().is_none_or(|b| b.contains(sid)) });
                }
            }
            hits.push(DocHit { doc_id: a.doc_id, adjacent: a.adjacent(), level: a.level(), count: a.count(), date: a.date, sentences });
        }
        Ok(SearchResult { words: parsed.words, dropped: parsed.dropped, total_docs, total_sentences, adjacent_docs, hits, generation: self.generation })
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
