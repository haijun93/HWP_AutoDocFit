//! 파일명 질의: 해석(`parse`) → 실행(`search`, 다중 스레드 SIMD 부분 문자열 찾기) → 결과(`Hit`).
//!
//! 문법(Everything과 비슷하게):
//! - 낱말은 공백으로 나누고 모두 들어 있어야 한다(AND, 순서 무관). 비교형(`kfind_core::fold`)으로 맞춘다:
//!   대소문자·NFC 무시, 글자·숫자가 아닌 것(공백·`.`·`_`·`-`)은 빼고 비교 → "예산집행"이 `예산_집행.hwp`를 찾는다.
//! - `"…"`: 따옴표 안을 한 낱말로. `-낱말`·`!낱말`: 그 낱말이 이름에 없어야 한다.
//! - `ext:hwp;pdf`: 확장자 중 하나. `path:낱말`: 조상 폴더 이름 중 하나가 그 낱말을 품음. `a\b`: `a`는 경로 조건, `b`는 이름 낱말.
//! - `file:`·`folder:`(`dir:`): 종류.
//! - Q-F3(사용자 결정 2026-11-14): 구분 기호를 빼고 비교하는 넓은 일치는 그대로 두고, 관련도 순에서는 입력한 글자
//!   그대로(대소문자만 무시) 이름에 들어 있는 항목을 앞에 둔다("예산_집행" → `예산_집행.hwp`가 `예산집행.hwp`보다 앞).
//! - Q-F2: 크기 순(큰 것 먼저)·날짜 순(최근 먼저). 모르는 값(폴더 크기, MFT 열거 원천)은 맨 뒤.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use memchr::memmem::Finder;
use rayon::prelude::*;

use crate::index::{EXT_OTHER, SIZE_UNKNOWN, TIME_UNKNOWN, View, ext_fold, flag};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Any,
    File,
    Dir,
}

/// 해석한 질의(모든 낱말은 비교형 바이트).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Query {
    /// 이름에 모두 들어 있어야 하는 낱말(입력 순서).
    pub words: Vec<Vec<u8>>,
    /// 이름에 없어야 하는 낱말.
    pub not: Vec<Vec<u8>>,
    /// 확장자(하나라도).
    pub exts: Vec<Vec<u8>>,
    /// 조상 폴더 이름에 들어 있어야 하는 낱말(각각 어떤 조상이든).
    pub path: Vec<Vec<u8>>,
    pub kind: Kind,
    /// 글자 그대로 비교할 낱말(소문자). 비교형과 다른 낱말만(구분 기호 등이 있어 넓은 일치와 결과가 갈릴 때).
    pub literal: Vec<String>,
}

impl Query {
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
            && self.not.is_empty()
            && self.exts.is_empty()
            && self.path.is_empty()
            && self.kind == Kind::Any
    }
}

fn fold_b(s: &str) -> Vec<u8> {
    kfind_core::fold(s).into_bytes()
}

fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in text.chars() {
        if c == '"' {
            quoted = !quoted;
            continue;
        }
        if c.is_whitespace() && !quoted {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            continue;
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn strip_prefix_ci<'a>(s: &'a str, p: &str) -> Option<&'a str> {
    if s.len() >= p.len() && s.is_char_boundary(p.len()) && s[..p.len()].eq_ignore_ascii_case(p) {
        Some(&s[p.len()..])
    } else {
        None
    }
}

/// 질의 글 → `Query`.
pub fn parse(text: &str) -> Query {
    let mut q = Query::default();
    for t in tokenize(text) {
        if let Some(r) = strip_prefix_ci(&t, "ext:") {
            for e in r.split([';', ',', '|']) {
                let f = fold_b(e.trim_start_matches('.'));
                if !f.is_empty() && !q.exts.contains(&f) {
                    q.exts.push(f);
                }
            }
            continue;
        }
        if let Some(r) = strip_prefix_ci(&t, "path:") {
            for c in r.split(['\\', '/']) {
                let f = fold_b(c);
                if !f.is_empty() {
                    q.path.push(f);
                }
            }
            continue;
        }
        if t.eq_ignore_ascii_case("file:") {
            q.kind = Kind::File;
            continue;
        }
        if t.eq_ignore_ascii_case("folder:") || t.eq_ignore_ascii_case("dir:") {
            q.kind = Kind::Dir;
            continue;
        }
        if (t.starts_with('-') || t.starts_with('!')) && t.chars().count() > 1 {
            let f = fold_b(&t[1..]);
            if !f.is_empty() {
                q.not.push(f);
            }
            continue;
        }
        if t.contains(['\\', '/']) {
            let parts: Vec<&str> = t.split(['\\', '/']).collect();
            let (last, dirs) = parts.split_last().unwrap();
            for d in dirs {
                let f = fold_b(d);
                if !f.is_empty() {
                    q.path.push(f);
                }
            }
            let f = fold_b(last);
            if !f.is_empty() {
                q.words.push(f);
            }
            continue;
        }
        let f = fold_b(&t);
        if !f.is_empty() {
            let raw = t.to_lowercase();
            if raw.as_bytes() != f.as_slice() {
                q.literal.push(raw);
            }
            q.words.push(f);
        }
    }
    q
}

/// 결과 순서.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    /// 이름 완전 일치 > 앞부분 일치 > 그 밖, 같은 등급은 얕은 경로 → 이름 순(IS1 Q12).
    #[default]
    Relevance,
    /// 비교형 이름 순(Everything 기본 정렬과 같은 꼴).
    Name,
    /// 경로 순(DFS 번호 순).
    Path,
    /// 크기 순(큰 것 먼저, 모르는 크기는 맨 뒤). Q-F2.
    Size,
    /// 고친 날짜 순(최근 먼저, 모르는 날짜는 맨 뒤). Q-F2.
    Date,
}

impl Sort {
    /// 파이프·설정 글 → 정렬. 모르는 글은 None.
    pub fn parse(s: &str) -> Option<Sort> {
        Some(match s {
            "relevance" => Sort::Relevance,
            "name" => Sort::Name,
            "path" => Sort::Path,
            "size" => Sort::Size,
            "date" => Sort::Date,
            _ => return None,
        })
    }
}

/// 대소문자 무시 포함(ASCII는 할당 없이).
fn contains_ci(hay: &str, needle_lower: &str) -> bool {
    if hay.is_ascii() && needle_lower.is_ascii() {
        let (h, n) = (hay.as_bytes(), needle_lower.as_bytes());
        n.is_empty() || h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
    } else {
        hay.to_lowercase().contains(needle_lower)
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    /// 돌려줄 상위 건수.
    pub limit: usize,
    pub sort: Sort,
    /// 결과에서 뺄 폴더(그 폴더와 하위 전부) 번호.
    pub deny: Vec<u32>,
    /// 숨김·시스템 항목 포함(기본: 포함, Everything과 같음).
    pub hidden: bool,
    pub cancel: Option<Arc<AtomicBool>>,
    /// 병렬로 나누지 않는 최대 바이트(시험·측정용 조절).
    pub serial_bytes: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            limit: 100,
            sort: Sort::Relevance,
            deny: Vec::new(),
            hidden: true,
            cancel: None,
            serial_bytes: 256 * 1024,
        }
    }
}

/// 질의 결과: 전체 개수와 상위 번호.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Results {
    pub total: usize,
    pub top: Vec<u32>,
    /// 앞 결과 안에서만 걸렀는가(측정·시험용).
    pub narrowed: bool,
}

/// 화면에 보낼 한 줄.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub id: u32,
    pub name: String,
    pub dir: String,
    pub is_dir: bool,
    /// 크기(바이트). 모르면 None.
    pub size: Option<u64>,
    /// 고친 시각(FILETIME 100ns). 모르면 None.
    pub mtime: Option<i64>,
}

pub fn hits(v: &View, top: &[u32]) -> Vec<Hit> {
    top.iter()
        .map(|&id| Hit {
            id,
            name: v.name_of(id).to_string(),
            dir: v.dir_path(id),
            is_dir: v.flags[id as usize] & flag::DIR != 0,
            size: Some(v.size[id as usize]).filter(|&s| s != SIZE_UNKNOWN),
            mtime: Some(v.mtime[id as usize]).filter(|&t| t != TIME_UNKNOWN),
        })
        .collect()
}

// ───────────── 구간 계산 ─────────────

type Ranges = Vec<(u32, u32)>;

fn normalize(mut r: Ranges) -> Ranges {
    r.retain(|&(a, b)| a < b);
    r.sort_unstable();
    let mut out: Ranges = Vec::with_capacity(r.len());
    for (a, b) in r {
        if let Some(last) = out.last_mut() {
            if a <= last.1 {
                last.1 = last.1.max(b);
                continue;
            }
        }
        out.push((a, b));
    }
    out
}

fn intersect(x: &Ranges, y: &Ranges) -> Ranges {
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < x.len() && j < y.len() {
        let a = x[i].0.max(y[j].0);
        let b = x[i].1.min(y[j].1);
        if a < b {
            out.push((a, b));
        }
        if x[i].1 < y[j].1 { i += 1 } else { j += 1 }
    }
    out
}

fn subtract(x: &Ranges, y: &Ranges) -> Ranges {
    let mut out = Vec::new();
    let mut j = 0;
    for &(mut a, b) in x {
        while j < y.len() && y[j].1 <= a {
            j += 1;
        }
        let mut k = j;
        while k < y.len() && y[k].0 < b {
            if y[k].0 > a {
                out.push((a, y[k].0));
            }
            a = a.max(y[k].1);
            k += 1;
        }
        if a < b {
            out.push((a, b));
        }
    }
    out
}

#[cfg(test)]
fn in_ranges(r: &Ranges, i: u32) -> bool {
    let k = r.partition_point(|&(a, _)| a <= i);
    k > 0 && i < r[k - 1].1
}

// ───────────── 실행 ─────────────

struct Plan<'q> {
    primary: Option<Finder<'q>>,
    /// 등급을 매길 첫 낱말.
    first: Option<&'q [u8]>,
    others: Vec<Finder<'q>>,
    not: Vec<Finder<'q>>,
    path: Vec<Finder<'q>>,
    ext_ok: Option<Vec<bool>>,
    ext_words: &'q [Vec<u8>],
    literal: &'q [String],
    kind: Kind,
    hidden: bool,
    sort: Sort,
    limit: usize,
    /// 기본판에서 훑을 구간(경로 조건·제외 폴더 반영).
    allowed: Ranges,
    /// 덧붙인 항목·옮긴 폴더 아래 항목에 부모 사슬 확인이 필요한가.
    walk: bool,
    walk_all: bool,
    deny: Vec<u32>,
}

#[derive(Default)]
struct Out {
    count: usize,
    top: Vec<(u64, u32)>,
    ids: Vec<u32>,
}

#[inline]
fn cmp_cand(v: &View, a: (u64, u32), b: (u64, u32), sort: Sort) -> std::cmp::Ordering {
    a.0.cmp(&b.0)
        .then_with(|| {
            if sort == Sort::Path {
                std::cmp::Ordering::Equal
            } else {
                v.fold_of(a.1).cmp(v.fold_of(b.1))
            }
        })
        .then(a.1.cmp(&b.1))
}

impl Out {
    #[inline]
    fn offer(&mut self, v: &View, limit: usize, sort: Sort, c: (u64, u32)) {
        if limit == 0 {
            return;
        }
        if self.top.len() == limit {
            let last = *self.top.last().unwrap();
            if c.0 > last.0 || cmp_cand(v, c, last, sort) != std::cmp::Ordering::Less {
                return;
            }
            self.top.pop();
        }
        let pos = self
            .top
            .partition_point(|&x| cmp_cand(v, x, c, sort) == std::cmp::Ordering::Less);
        self.top.insert(pos, c);
    }
}

impl<'q> Plan<'q> {
    #[inline]
    fn key(&self, v: &View, i: u32) -> u64 {
        match self.sort {
            Sort::Path => i as u64,
            Sort::Name => v.key[i as usize] & ((1u64 << 56) - 1),
            Sort::Size => match v.size[i as usize] {
                SIZE_UNKNOWN => u64::MAX,
                s => u64::MAX - 1 - s,
            },
            Sort::Date => match v.mtime[i as usize] {
                TIME_UNKNOWN => u64::MAX,
                t => (i64::MAX - t.max(0)) as u64,
            },
            Sort::Relevance => {
                let base = v.key[i as usize] & ((1u64 << 62) - 1);
                // 글자 그대로 일치가 아니면 맨 위 비트(Q-F3). 등급은 그다음 2비트, 깊이·이름 앞부분은 한 비트 줄여
                // 아래에 둔다(같은 열쇠끼리는 비교형 전체로 다시 가르므로 순서는 그대로).
                let nonliteral = !self.literal.is_empty()
                    && !self.literal.iter().all(|w| contains_ci(v.name_of(i), w));
                let g = match self.first {
                    Some(w) => {
                        let f = v.fold_of(i);
                        if f == w {
                            0
                        } else if f.starts_with(w) {
                            1
                        } else {
                            2
                        }
                    }
                    None => 2,
                };
                ((nonliteral as u64) << 63) | (g << 61) | (base >> 1)
            }
        }
    }

    /// 1차 낱말 말고 나머지 조건(표지·종류·확장자·낱말·제외 낱말·경로).
    #[inline]
    fn accept(&self, v: &View, i: u32, check_primary: bool, check_path: bool) -> bool {
        let fl = v.flags[i as usize];
        if fl & flag::DELETED != 0 {
            return false;
        }
        match self.kind {
            Kind::Any => {}
            Kind::File => {
                if fl & flag::DIR != 0 {
                    return false;
                }
            }
            Kind::Dir => {
                if fl & flag::DIR == 0 {
                    return false;
                }
            }
        }
        if !self.hidden && fl & (flag::HIDDEN | flag::SYSTEM) != 0 {
            return false;
        }
        if let Some(ok) = &self.ext_ok {
            let e = v.ext[i as usize];
            let good = if e == EXT_OTHER {
                ext_fold(v.name_of(i), fl & flag::DIR != 0)
                    .map(|f| self.ext_words.iter().any(|w| w.as_slice() == f.as_bytes()))
                    .unwrap_or(false)
            } else {
                ok.get(e as usize).copied().unwrap_or(false)
            };
            if !good {
                return false;
            }
        }
        if !self.others.is_empty() || !self.not.is_empty() || check_primary {
            let f = v.fold_of(i);
            if check_primary {
                if let Some(p) = &self.primary {
                    if p.find(f).is_none() {
                        return false;
                    }
                }
            }
            for o in &self.others {
                if o.find(f).is_none() {
                    return false;
                }
            }
            for o in &self.not {
                if o.find(f).is_some() {
                    return false;
                }
            }
        }
        if check_path && self.walk && (self.walk_all || i >= v.base_len) && !self.walk_ok(v, i) {
            return false;
        }
        true
    }

    /// 부모 사슬로 경로 조건·제외 폴더 확인.
    fn walk_ok(&self, v: &View, i: u32) -> bool {
        if self.deny.contains(&i) {
            return false;
        }
        let mut need: u64 = if self.path.len() >= 64 {
            u64::MAX
        } else {
            (1u64 << self.path.len()) - 1
        };
        for a in v.ancestors(i) {
            if self.deny.contains(&a) {
                return false;
            }
            if need != 0 {
                let f = v.fold_of(a);
                for (k, p) in self.path.iter().enumerate().take(64) {
                    if need & (1 << k) != 0 && p.find(f).is_some() {
                        need &= !(1 << k);
                    }
                }
            }
        }
        need == 0
    }

    #[inline]
    fn take(&self, v: &View, i: u32, out: &mut Out, collect: bool) {
        out.count += 1;
        if collect {
            out.ids.push(i);
        }
        let k = self.key(v, i);
        out.offer(v, self.limit, self.sort, (k, i));
    }

    /// 항목 구간 [a,b)를 훑는다.
    fn scan(
        &self,
        v: &View,
        a: u32,
        b: u32,
        collect: bool,
        cancel: Option<&AtomicBool>,
    ) -> Result<Out> {
        let mut out = Out::default();
        let check_path = true;
        let Some(fin) = &self.primary else {
            for i in a..b {
                if (i & 0xFFFF) == 0 && cancelled(cancel) {
                    return Err(Error::Cancelled);
                }
                if self.accept(v, i, false, check_path) {
                    self.take(v, i, &mut out, collect);
                }
            }
            return Ok(out);
        };
        let off = v.fold_off;
        let base = off[a as usize] as usize;
        let hay = &v.fold[base..off[b as usize] as usize];
        let mut pos = 0usize;
        let mut cur = a as usize;
        let b = b as usize;
        let mut n = 0u32;
        while pos < hay.len() {
            let Some(r) = fin.find(&hay[pos..]) else {
                break;
            };
            let p = (base + pos + r) as u32;
            // 질주 탐색: off[cur] <= p < off[cur+1]인 cur.
            let mut step = 1usize;
            while cur + step < b && off[cur + step] <= p {
                step <<= 1;
            }
            let lo = cur + (step >> 1);
            let hi = (cur + step).min(b);
            cur = lo + off[lo..hi].partition_point(|&o| o <= p) - 1;
            let i = cur as u32;
            if self.accept(v, i, false, check_path) {
                self.take(v, i, &mut out, collect);
            }
            cur += 1;
            pos = off[cur] as usize - base;
            n += 1;
            if (n & 0x3FFF) == 0 && cancelled(cancel) {
                return Err(Error::Cancelled);
            }
        }
        Ok(out)
    }

    /// 앞 결과(후보 번호)만 확인한다. 후보는 경로·제외·종류·확장자 조건을 이미 지난 것.
    fn verify(&self, v: &View, cands: &[u32], collect: bool) -> Out {
        let mut out = Out::default();
        for &i in cands {
            if self.accept(v, i, true, false) {
                self.take(v, i, &mut out, collect);
            }
        }
        out
    }
}

#[inline]
fn cancelled(c: Option<&AtomicBool>) -> bool {
    c.map(|c| c.load(Ordering::Relaxed)).unwrap_or(false)
}

/// 낱말 목록에서 다른 낱말에 들어 있는 것을 뺀다(같은 뜻, 확인 줄이기).
fn reduce_words(words: &[Vec<u8>]) -> Vec<&[u8]> {
    let mut out: Vec<&[u8]> = Vec::new();
    for (i, w) in words.iter().enumerate() {
        let covered = words.iter().enumerate().any(|(j, o)| {
            j != i
                && o.len() >= w.len()
                && memchr::memmem::find(o, w).is_some()
                && (o.len() > w.len() || j < i)
        });
        if !covered {
            out.push(w.as_slice());
        }
    }
    out
}

fn path_ranges(v: &View, w: &[u8], threads_ok: bool) -> Ranges {
    // 이 낱말을 품은 살아 있는 기본판 폴더의 하위 구간(폴더 자신은 빼고).
    let fin = Finder::new(w);
    let n = v.base_len as usize;
    if n == 0 {
        return Vec::new();
    }
    let chunks = chunk_ranges(
        v,
        &[(0, n as u32)],
        if threads_ok {
            rayon::current_num_threads() * 4
        } else {
            1
        },
        64 * 1024,
    );
    let parts: Vec<Ranges> = chunks
        .par_iter()
        .map(|&(a, b)| {
            let mut r = Vec::new();
            let off = v.fold_off;
            let base = off[a as usize] as usize;
            let hay = &v.fold[base..off[b as usize] as usize];
            let mut pos = 0;
            let mut cur = a as usize;
            while pos < hay.len() {
                let Some(x) = fin.find(&hay[pos..]) else {
                    break;
                };
                let p = (base + pos + x) as u32;
                cur += off[cur..b as usize].partition_point(|&o| o <= p) - 1;
                let fl = v.flags[cur];
                if fl & flag::DIR != 0 && fl & flag::DELETED == 0 {
                    r.push((cur as u32 + 1, v.end[cur]));
                }
                cur += 1;
                pos = off[cur] as usize - base;
            }
            r
        })
        .collect();
    normalize(parts.into_iter().flatten().collect())
}

/// 구간들을 비슷한 바이트 크기의 조각(항목 경계)으로 나눈다.
fn chunk_ranges(
    v: &View,
    regions: &[(u32, u32)],
    parts: usize,
    min_bytes: usize,
) -> Vec<(u32, u32)> {
    let bytes = |a: u32, b: u32| (v.fold_off[b as usize] - v.fold_off[a as usize]) as usize;
    let total: usize = regions.iter().map(|&(a, b)| bytes(a, b)).sum();
    let target = (total / parts.max(1)).max(min_bytes).max(1);
    let mut out = Vec::new();
    for &(a, b) in regions {
        let mut s = a;
        while s < b {
            let want = v.fold_off[s as usize] as usize + target;
            let mut e = s + v.fold_off[s as usize..=b as usize]
                .partition_point(|&o| (o as usize) < want) as u32;
            e = e.clamp(s + 1, b);
            out.push((s, e));
            s = e;
        }
    }
    out
}

fn merge(
    v: &View,
    outs: Vec<Out>,
    limit: usize,
    sort: Sort,
    collect: bool,
) -> (usize, Vec<u32>, Vec<u32>) {
    let total = outs.iter().map(|o| o.count).sum();
    let mut all: Vec<(u64, u32)> = Vec::with_capacity(outs.iter().map(|o| o.top.len()).sum());
    let mut ids = Vec::new();
    if collect {
        ids.reserve(total);
    }
    for o in outs {
        all.extend(o.top);
        if collect {
            ids.extend(o.ids);
        }
    }
    all.sort_unstable_by(|&a, &b| cmp_cand(v, a, b, sort));
    all.truncate(limit);
    (total, all.into_iter().map(|c| c.1).collect(), ids)
}

fn plan<'q>(v: &View, q: &'q Query, opts: &Options, reduced: &[&'q [u8]]) -> Plan<'q> {
    // 1차 낱말: 가장 긴 것(대개 가장 드묾).
    let pi = (0..reduced.len()).max_by_key(|&k| (reduced[k].len(), std::cmp::Reverse(k)));
    let primary = pi.map(|k| Finder::new(reduced[k]));
    let others = reduced
        .iter()
        .enumerate()
        .filter(|(k, _)| Some(*k) != pi)
        .map(|(_, w)| Finder::new(*w))
        .collect();
    let ext_ok = if q.exts.is_empty() {
        None
    } else {
        let mut ok = vec![false; v.exts.len()];
        for e in &q.exts {
            if let Some(id) = v.exts.find(e) {
                ok[id as usize] = true;
            }
        }
        Some(ok)
    };
    let has_cond = !q.path.is_empty() || !opts.deny.is_empty();
    let mut allowed: Ranges = vec![(0, v.base_len)];
    let walk_all = v.dirty && has_cond;
    if has_cond && !walk_all {
        let multi = v.fold.len() > opts.serial_bytes;
        for w in &q.path {
            let r = path_ranges(v, w, multi);
            allowed = intersect(&allowed, &r);
        }
        let deny: Ranges = normalize(
            opts.deny
                .iter()
                .filter(|&&d| d < v.base_len)
                .map(|&d| (d, v.end[d as usize]))
                .collect(),
        );
        allowed = subtract(&allowed, &deny);
    }
    Plan {
        primary,
        first: q.words.first().map(|w| w.as_slice()),
        others,
        not: q.not.iter().map(|w| Finder::new(w.as_slice())).collect(),
        path: q.path.iter().map(|w| Finder::new(w.as_slice())).collect(),
        ext_ok,
        ext_words: &q.exts,
        literal: &q.literal,
        kind: q.kind,
        hidden: opts.hidden,
        sort: opts.sort,
        limit: opts.limit,
        allowed,
        walk: has_cond,
        walk_all,
        deny: opts.deny.clone(),
    }
}

fn run(
    v: &View,
    q: &Query,
    opts: &Options,
    cands: Option<&[u32]>,
    collect: bool,
) -> Result<(usize, Vec<u32>, Vec<u32>)> {
    let reduced = reduce_words(&q.words);
    let p = plan(v, q, opts, &reduced);
    let cancel = opts.cancel.as_deref();
    let threads = rayon::current_num_threads().max(1);
    if let Some(c) = cands {
        let parts = if c.len() < 50_000 { 1 } else { threads * 4 };
        let size = c.len().div_ceil(parts).max(1);
        let outs: Vec<Out> = if parts == 1 {
            vec![p.verify(v, c, collect)]
        } else {
            c.par_chunks(size)
                .map(|ch| p.verify(v, ch, collect))
                .collect()
        };
        if cancelled(cancel) {
            return Err(Error::Cancelled);
        }
        return Ok(merge(v, outs, opts.limit, opts.sort, collect));
    }
    let mut regions: Vec<(u32, u32)> = p.allowed.clone();
    if (v.len() as u32) > v.base_len {
        regions.push((v.base_len, v.len() as u32));
    }
    let bytes: usize = regions
        .iter()
        .map(|&(a, b)| (v.fold_off[b as usize] - v.fold_off[a as usize]) as usize)
        .sum();
    let outs: Vec<Out> = if bytes <= opts.serial_bytes {
        let mut o = Vec::with_capacity(regions.len());
        for &(a, b) in &regions {
            o.push(p.scan(v, a, b, collect, cancel)?);
        }
        o
    } else {
        let chunks = chunk_ranges(v, &regions, threads * 4, 32 * 1024);
        chunks
            .par_iter()
            .map(|&(a, b)| p.scan(v, a, b, collect, cancel))
            .collect::<Result<Vec<Out>>>()?
    };
    Ok(merge(v, outs, opts.limit, opts.sort, collect))
}

/// 한 번 질의(앞 결과를 쓰지 않음).
pub fn search(v: &View, q: &Query, opts: &Options) -> Result<Results> {
    let (total, top, _) = run(v, q, opts, None, false)?;
    Ok(Results {
        total,
        top,
        narrowed: false,
    })
}

/// 같은 범위 조건 아래, 새 질의의 일치가 늘 앞 질의의 일치 안에 드는가.
fn refines(old: &Query, new: &Query) -> bool {
    old.kind == new.kind
        && old.exts == new.exts
        && old.path == new.path
        && old.not.iter().all(|w| new.not.contains(w))
        && old.words.iter().all(|w| {
            new.words
                .iter()
                .any(|n| memchr::memmem::find(n, w).is_some())
        })
}

struct Last {
    generation: u64,
    len: usize,
    q: Query,
    deny: Vec<u32>,
    hidden: bool,
    ids: Vec<u32>,
}

/// 입력할 때마다 부르는 질의기. 새 질의가 앞 질의의 연장(낱말이 길어지거나 늘어남)이면 앞 일치 안에서만 거른다.
#[derive(Default)]
pub struct Searcher {
    last: Option<Last>,
    /// 앞 일치를 기억하는 최대 개수.
    pub max_keep: usize,
}

impl Searcher {
    pub fn new() -> Self {
        Searcher {
            last: None,
            max_keep: 1 << 24,
        }
    }

    pub fn clear(&mut self) {
        self.last = None;
    }

    /// `generation`: 색인 세대(바뀌면 앞 결과를 버린다).
    pub fn search(
        &mut self,
        v: &View,
        generation: u64,
        q: &Query,
        opts: &Options,
    ) -> Result<Results> {
        let last = self.last.take();
        let cands = last
            .filter(|l| {
                l.generation == generation
                    && l.len == v.len()
                    && l.deny == opts.deny
                    && l.hidden == opts.hidden
                    && refines(&l.q, q)
            })
            .map(|l| l.ids);
        let narrowed = cands.is_some();
        let r = run(v, q, opts, cands.as_deref(), true);
        let (total, top, ids) = match r {
            Ok(x) => x,
            Err(e) => {
                self.last = None;
                return Err(e);
            }
        };
        self.last = if ids.len() <= self.max_keep {
            Some(Last {
                generation,
                len: v.len(),
                q: q.clone(),
                deny: opts.deny.clone(),
                hidden: opts.hidden,
                ids,
            })
        } else {
            None
        };
        Ok(Results {
            total,
            top,
            narrowed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic() {
        let q = parse("예산 집행 -초안 ext:hwp;.PDF path:문서 file:");
        assert_eq!(
            q.words,
            vec!["예산".as_bytes().to_vec(), "집행".as_bytes().to_vec()]
        );
        assert_eq!(q.not, vec!["초안".as_bytes().to_vec()]);
        assert_eq!(q.exts, vec![b"hwp".to_vec(), b"pdf".to_vec()]);
        assert_eq!(q.path, vec!["문서".as_bytes().to_vec()]);
        assert_eq!(q.kind, Kind::File);
        let q = parse("\"Annual Report\" docs\\plan");
        assert_eq!(q.words, vec![b"annualreport".to_vec(), b"plan".to_vec()]);
        assert_eq!(q.path, vec![b"docs".to_vec()]);
        let q = parse("folder: !tmp - .");
        assert_eq!(q.kind, Kind::Dir);
        assert_eq!(q.not, vec![b"tmp".to_vec()]);
        assert!(q.words.is_empty());
    }

    #[test]
    fn ranges_ops() {
        let a = vec![(0, 10), (20, 30)];
        let b = vec![(5, 25)];
        assert_eq!(intersect(&a, &b), vec![(5, 10), (20, 25)]);
        assert_eq!(subtract(&a, &b), vec![(0, 5), (25, 30)]);
        assert_eq!(
            subtract(&a, &vec![(0, 10), (21, 22)]),
            vec![(20, 21), (22, 30)]
        );
        assert_eq!(
            normalize(vec![(3, 5), (0, 4), (6, 6), (5, 7)]),
            vec![(0, 7)]
        );
        assert!(in_ranges(&a, 25) && !in_ranges(&a, 10) && in_ranges(&a, 0));
    }

    #[test]
    fn refine_rules() {
        assert!(refines(&parse("보"), &parse("보고")));
        assert!(refines(&parse("보고"), &parse("보고 2026")));
        assert!(refines(&parse("a"), &parse("ba")));
        assert!(!refines(&parse("보고"), &parse("보")));
        assert!(!refines(&parse("a ext:pdf"), &parse("ab")));
        assert!(refines(&parse("a -x"), &parse("ab -x -y")));
        assert!(!refines(&parse("a -x"), &parse("ab")));
    }

    #[test]
    fn reduce() {
        let w = vec![
            b"ab".to_vec(),
            b"abc".to_vec(),
            b"x".to_vec(),
            b"ab".to_vec(),
        ];
        assert_eq!(reduce_words(&w), vec![b"abc".as_slice(), b"x".as_slice()]);
    }
}
