//! 합성 레코드로 엔진을 단순 전수 비교 기준 구현과 대조한다(이 PC 경로·사용자 파일을 쓰지 않음).
use std::collections::{BTreeMap, HashSet};

use kfind_names::query::{Kind, hits, search};
use kfind_names::source::{ChangeKind, attr};
use kfind_names::{Builder, Change, Names, NamesSeg, Options, Query, Searcher, Sort, View, flag, parse, seg};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const WORDS: &[&str] = &[
    "예산", "집행", "계획", "보고서", "회의", "결과", "Report", "plan", "draft", "Final", "사업", "2026", "2025", "자료",
    "민원", "공문", "budget", "a", "ab", "x", "카페", "Straße", "ＡＢＣ", "e\u{301}clair", "data", "img",
];
const EXTS: &[&str] = &["hwp", "hwpx", "pdf", "xlsx", "txt", "PNG", "", "tar.gz", "jpg"];

/// 합성 모델: (볼륨, 레코드) → (부모 레코드, 이름, 속성).
#[derive(Clone)]
struct Model {
    vols: Vec<(String, u64)>,
    e: BTreeMap<(u16, u64), (u64, String, u32)>,
}

impl Model {
    fn make(seed: u64, n: usize) -> Model {
        let mut r = Rng(seed);
        let vols = vec![("C:".to_string(), 5u64), ("D:".to_string(), 5u64)];
        let mut e = BTreeMap::new();
        let mut dirs: Vec<Vec<u64>> = vec![vec![5], vec![5]];
        let mut seen: HashSet<(u16, u64, String)> = HashSet::new();
        // 레코드 번호를 흩어 놓는다(열거 순서 ≠ 트리 순서).
        let mut recs: Vec<u64> = (16..16 + n as u64 * 2).collect();
        for i in (1..recs.len()).rev() {
            let j = r.below(i + 1);
            recs.swap(i, j);
        }
        for (k, &rec) in recs.iter().take(n).enumerate() {
            let vol = (r.below(5) == 0) as u16;
            let parent = dirs[vol as usize][r.below(dirs[vol as usize].len())];
            let is_dir = r.below(4) == 0;
            let mut name = String::new();
            for w in 0..1 + r.below(3) {
                if w > 0 {
                    name.push(['_', ' ', '-', '.'][r.below(4)]);
                }
                name.push_str(WORDS[r.below(WORDS.len())]);
            }
            if r.below(7) == 0 {
                name = format!("{name}{k}");
            }
            let mut attrs = if is_dir { attr::DIRECTORY } else { 0 };
            if !is_dir {
                let ext = EXTS[r.below(EXTS.len())];
                if !ext.is_empty() {
                    name = format!("{name}.{ext}");
                }
            }
            if r.below(13) == 0 {
                attrs |= attr::HIDDEN;
            }
            // 같은 폴더 안 같은 이름은 NTFS에서 생기지 않으므로 피한다(대소문자 무시).
            while !seen.insert((vol, parent, name.to_lowercase())) {
                name = format!("{name}~{k}");
            }
            if is_dir {
                dirs[vol as usize].push(rec);
            }
            e.insert((vol, rec), (parent, name, attrs));
        }
        Model { vols, e }
    }

    fn build(&self) -> Names {
        let mut b = Builder::new();
        for (n, r) in &self.vols {
            b.add_volume(n, *r);
        }
        for (&(v, rec), (p, n, a)) in &self.e {
            b.push(v, rec, *p, n, *a);
        }
        b.finish()
    }

    fn chain(&self, v: u16, rec: u64) -> Vec<(u64, String)> {
        // 조상(가까운 것부터), 뿌리 포함.
        let mut out = Vec::new();
        let mut cur = self.e.get(&(v, rec)).map(|x| x.0);
        let mut guard = 0;
        while let Some(p) = cur {
            if p == self.vols[v as usize].1 || guard > 500 {
                break;
            }
            match self.e.get(&(v, p)) {
                Some((pp, n, _)) => {
                    out.push((p, n.clone()));
                    cur = Some(*pp);
                }
                None => break,
            }
            guard += 1;
        }
        out.push((self.vols[v as usize].1, self.vols[v as usize].0.clone()));
        out
    }

    fn full_path(&self, v: u16, rec: u64) -> String {
        let mut c: Vec<String> = self.chain(v, rec).into_iter().map(|x| x.1).collect();
        c.reverse();
        c.push(self.e[&(v, rec)].1.clone());
        c.join("\\")
    }
}

fn fold(s: &str) -> Vec<u8> {
    kfind_core::fold(s).into_bytes()
}

fn contains(h: &[u8], n: &[u8]) -> bool {
    n.is_empty() || h.windows(n.len()).any(|w| w == n)
}

fn ext_of(name: &str, dir: bool) -> Option<Vec<u8>> {
    if dir {
        return None;
    }
    let p = name.rfind('.')?;
    if p == 0 {
        return None;
    }
    let e = &name[p + 1..];
    if e.is_empty() || e.chars().count() > 16 {
        return None;
    }
    let f = fold(e);
    if f.is_empty() { None } else { Some(f) }
}

/// 기준 구현: (전체 경로 집합, 정렬 튜플 목록).
fn oracle(m: &Model, q: &Query, deny: &[String], sort: Sort) -> (HashSet<String>, Vec<(u8, u32, Vec<u8>)>) {
    let mut set = HashSet::new();
    let mut tuples = Vec::new();
    // 볼륨 뿌리(`C:`)도 항목이다(깊이 0, 조상 없음).
    let roots = m.vols.iter().enumerate().map(|(vi, (n, r))| ((vi as u16, *r), n.clone()));
    let items = roots
        .map(|(k, n)| (k, n, attr::DIRECTORY, true))
        .chain(m.e.iter().map(|(&k, (_, n, a))| (k, n.clone(), *a, false)));
    for ((v, rec), name, a, is_root) in items {
        let name = &name;
        let f = fold(name);
        let dir = a & attr::DIRECTORY != 0;
        if !q.words.iter().all(|w| contains(&f, w)) || q.not.iter().any(|w| contains(&f, w)) {
            continue;
        }
        match q.kind {
            Kind::File if dir => continue,
            Kind::Dir if !dir => continue,
            _ => {}
        }
        if !q.exts.is_empty() && !ext_of(name, dir).is_some_and(|e| q.exts.contains(&e)) {
            continue;
        }
        let chain = if is_root { Vec::new() } else { m.chain(v, rec) };
        if !q.path.iter().all(|w| chain.iter().any(|(_, n)| contains(&fold(n), w))) {
            continue;
        }
        let full = if is_root { name.clone() } else { m.full_path(v, rec) };
        if deny.iter().any(|d| full == *d || full.starts_with(&format!("{d}\\"))) {
            continue;
        }
        let depth = (chain.len() as u32).min(63);
        let grade = match q.words.first() {
            Some(w) if &f == w => 0,
            Some(w) if f.starts_with(w) => 1,
            _ => 2,
        };
        tuples.push(match sort {
            Sort::Relevance => (grade, depth, f),
            Sort::Name => (0, 0, f),
            Sort::Path => (0, 0, Vec::new()),
        });
        set.insert(full);
    }
    tuples.sort();
    (set, tuples)
}

fn engine_tuple(v: &View, q: &Query, id: u32, sort: Sort) -> (u8, u32, Vec<u8>) {
    let f = v.fold_of(id).to_vec();
    let depth = (v.ancestors(id).count() as u32).min(63);
    let grade = match q.words.first() {
        Some(w) if &f == w => 0,
        Some(w) if f.starts_with(w) => 1,
        _ => 2,
    };
    match sort {
        Sort::Relevance => (grade, depth, f),
        Sort::Name => (0, 0, f),
        Sort::Path => (0, 0, Vec::new()),
    }
}

fn check(m: &Model, v: &View, text: &str, deny_paths: &[String], sort: Sort, serial_bytes: usize) {
    let q = parse(text);
    let deny: Vec<u32> = deny_paths.iter().map(|p| v.find_dir(p).expect("deny dir")).collect();
    let (want_set, want_t) = oracle(m, &q, deny_paths, sort);
    let all = Options { limit: usize::MAX, sort, deny: deny.clone(), serial_bytes, ..Default::default() };
    let r = search(v, &q, &all).unwrap();
    assert_eq!(r.total, want_t.len(), "total for {text:?}");
    let got: HashSet<String> = r.top.iter().map(|&i| v.full_path(i)).collect();
    assert_eq!(got, want_set, "set for {text:?}");
    if sort != Sort::Path {
        let top = Options { limit: 30, sort, deny, serial_bytes, ..Default::default() };
        let r = search(v, &q, &top).unwrap();
        let t: Vec<_> = r.top.iter().map(|&i| engine_tuple(v, &q, i, sort)).collect();
        assert_eq!(t, want_t.iter().take(30).cloned().collect::<Vec<_>>(), "order for {text:?}");
    } else {
        // 경로 순 = 번호 순.
        assert!(r.top.windows(2).all(|w| w[0] < w[1]));
    }
}

const QUERIES: &[&str] = &[
    "a", "예", "예산", "예산집행", "예산 집행", "보고서", "report", "REPORT", "x", "2026", "ab", "e\u{301}", "é", "abc", "strasse",
    "예산 -집행", "plan ext:pdf", "ext:hwp;hwpx", "ext:gz", "file: 회의", "folder: 자료", "path:자료 a", "path:예산 path:2026",
    "자료\\보고", "!a", "-a -예", "\"final report\"", "", "zzzz", "1", "data img",
];

#[test]
fn matches_oracle_all_sorts_serial_and_parallel() {
    let m = Model::make(0x9E37_79B9_7F4A_7C15, 30_000);
    let n = m.build();
    let v = n.view();
    for &q in QUERIES {
        for sort in [Sort::Relevance, Sort::Name, Sort::Path] {
            check(&m, &v, q, &[], sort, usize::MAX);
            check(&m, &v, q, &[], sort, 0);
        }
    }
}

#[test]
fn deny_folders_and_hidden() {
    let m = Model::make(7, 20_000);
    let n = m.build();
    let v = n.view();
    // 하위가 많은 폴더 두 개를 제외 대상으로.
    let mut dirs: Vec<(usize, String)> = m
        .e
        .iter()
        .filter(|(_, (_, _, a))| a & attr::DIRECTORY != 0)
        .map(|(&(vv, r), _)| {
            let p = m.full_path(vv, r);
            (m.e.keys().filter(|&&(v2, r2)| m.full_path(v2, r2).starts_with(&format!("{p}\\"))).count(), p)
        })
        .take(400)
        .collect();
    dirs.sort();
    let deny: Vec<String> = dirs.iter().rev().take(2).map(|d| d.1.clone()).collect();
    for &q in &["a", "예산", "path:자료 a", "ext:pdf", ""] {
        check(&m, &v, q, &deny, Sort::Relevance, 0);
        check(&m, &v, q, &deny, Sort::Name, usize::MAX);
    }
    let hidden = Options { hidden: false, limit: usize::MAX, ..Default::default() };
    let r = search(&v, &parse("a"), &hidden).unwrap();
    assert!(r.top.iter().all(|&i| v.flags[i as usize] & flag::HIDDEN == 0));
}

#[test]
fn narrowing_equals_fresh_search() {
    let m = Model::make(42, 30_000);
    let n = m.build();
    let v = n.view();
    let seqs: &[&[&str]] = &[
        &["예", "예산", "예산집", "예산집행", "예산집행 2026", "예산집행 2026 -a"],
        &["a", "ab", "abc", "ab"],
        &["r", "re", "rep", "repo", "report ext:pdf", "report ext:pdf"],
        &["path:자료 a", "path:자료 ab", "path:자료 ab -x"],
        &["d", "da", "dat", "data", "data i", "data im"],
    ];
    for seq in seqs {
        for sort in [Sort::Relevance, Sort::Name] {
            let opts = Options { limit: 50, sort, serial_bytes: 0, ..Default::default() };
            let mut s = Searcher::new();
            let mut narrowed = 0;
            for &t in *seq {
                let q = parse(t);
                let a = s.search(&v, n.generation(), &q, &opts).unwrap();
                let b = search(&v, &q, &opts).unwrap();
                assert_eq!((a.total, &a.top), (b.total, &b.top), "{t:?}");
                narrowed += a.narrowed as usize;
            }
            assert!(narrowed >= seq.len() - 3, "narrowing used {narrowed}");
        }
    }
}

fn apply_both(m: &mut Model, n: &mut Names, c: Change) {
    match c.kind {
        ChangeKind::Upsert => {
            m.e.insert((c.vol, c.rec), (c.parent, c.name.clone(), c.attrs));
        }
        ChangeKind::Remove => {
            m.e.remove(&(c.vol, c.rec));
        }
        ChangeKind::Ignore => {}
    }
    n.apply(&c);
}

#[test]
fn usn_changes_keep_results_exact() {
    let mut m = Model::make(99, 15_000);
    let mut n = m.build();
    let mut r = Rng(5);
    let g0 = n.generation();
    let dir_recs: Vec<(u16, u64)> = m.e.iter().filter(|(_, x)| x.2 & attr::DIRECTORY != 0).map(|(k, _)| *k).collect();
    for k in 0..600 {
        let keys: Vec<(u16, u64)> = m.e.keys().copied().collect();
        let (vol, rec) = keys[r.below(keys.len())];
        let c = match r.below(4) {
            // 새 파일
            0 => {
                let pd = dir_recs[r.below(dir_recs.len())];
                Change { vol: pd.0, rec: 1_000_000 + k, parent: pd.1, name: format!("새예산_{k}.hwp"), attrs: 0, kind: ChangeKind::Upsert }
            }
            // 이름 바꾸기(같은 부모)
            1 => {
                let (p, nm, a) = m.e[&(vol, rec)].clone();
                Change { vol, rec, parent: p, name: format!("바뀜_{nm}"), attrs: a, kind: ChangeKind::Upsert }
            }
            // 옮기기(폴더 포함, 고리가 생기지 않게 다른 볼륨 뿌리 아래로만)
            2 => {
                let (_, nm, a) = m.e[&(vol, rec)].clone();
                Change { vol, rec, parent: 5, name: nm, attrs: a, kind: ChangeKind::Upsert }
            }
            // 지우기(폴더는 빈 경우만 — NTFS와 같게)
            _ => {
                let has_child = m.e.iter().any(|(&(v2, _), x)| v2 == vol && x.0 == rec);
                if has_child {
                    continue;
                }
                Change { vol, rec, parent: 0, name: String::new(), attrs: 0, kind: ChangeKind::Remove }
            }
        };
        apply_both(&mut m, &mut n, c);
    }
    assert!(n.generation() > g0);
    assert!(n.delta_count() > 0);
    let v = n.view();
    // 관련도 순의 깊이는 옮긴 폴더 아래에서 다시 만들기 전까지 옛 값이라(설계 문서 §3.5) 집합·이름 순만 본다.
    for &q in &["새예산", "바뀜", "예산", "a", "path:자료 a", "ext:hwp 바뀜", "folder: 바뀜", ""] {
        check(&m, &v, q, &[], Sort::Path, 0);
        check(&m, &v, q, &[], Sort::Name, usize::MAX);
        check(&m, &v, q, &[], Sort::Name, 0);
    }
    // 다시 만든 기본판도 같은 결과.
    let c = n.compact();
    assert!(!c.has_delta());
    let cv = c.view();
    for &q in &["새예산", "바뀜", "path:자료 a", ""] {
        check(&m, &cv, q, &[], Sort::Relevance, 0);
    }
}

#[test]
fn segment_roundtrip_and_corruption() {
    let m = Model::make(3, 8_000);
    let n = m.build();
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("names.seg");
    let size = seg::write(&n, &p).unwrap();
    let s = NamesSeg::open(&p, true).unwrap();
    assert_eq!(s.file_len() as u64, size);
    assert_eq!(s.len(), n.len());
    let (a, b) = (n.view(), s.view());
    for &q in QUERIES {
        let q = parse(q);
        let o = Options { limit: 40, ..Default::default() };
        let ra = search(&a, &q, &o).unwrap();
        let rb = search(&b, &q, &o).unwrap();
        assert_eq!(ra, rb);
        assert_eq!(hits(&a, &ra.top), hits(&b, &rb.top));
    }
    // 소유판으로 되읽기 → 갱신 가능.
    let mut back = s.to_names();
    assert!(back.upsert(0, 999_999, 5, "되읽기.txt", 0));
    assert_eq!(search(&back.view(), &parse("되읽기"), &Options::default()).unwrap().total, 1);
    // 손상: 잘라내기·바이트 뒤집기 → 오류(패닉 없음).
    let bytes = std::fs::read(&p).unwrap();
    let mut r = Rng(11);
    let bad = dir.path().join("bad.seg");
    for k in 0..400 {
        let mut b2 = bytes.clone();
        if k % 4 == 0 {
            b2.truncate(r.below(b2.len()));
        } else {
            for _ in 0..1 + r.below(4) {
                let i = r.below(b2.len());
                b2[i] ^= 1 << r.below(8);
            }
        }
        std::fs::write(&bad, &b2).unwrap();
        let res = std::panic::catch_unwind(|| NamesSeg::open(&bad, true).map(|_| ()));
        assert!(res.is_ok(), "panic on corruption {k}");
        assert!(res.unwrap().is_err(), "accepted corrupted file {k}");
    }
    // 본문 CRC 검사를 끈 채로(빠른 열기) 구조만 뒤틀린 파일도 패닉 없이 거르거나, 열리면 질의가 패닉하지 않아야 한다.
    for k in 0..400 {
        let mut b2 = bytes.clone();
        let i = 64 + r.below(b2.len() - 64);
        b2[i] ^= 1 << r.below(8);
        std::fs::write(&bad, &b2).unwrap();
        let res = std::panic::catch_unwind(|| {
            if let Ok(s) = NamesSeg::open(&bad, false) {
                let v = s.view();
                for q in ["a", "예산", "path:자료 a", "ext:pdf", ""] {
                    let r = search(&v, &parse(q), &Options { limit: 20, ..Default::default() }).unwrap();
                    let _ = hits(&v, &r.top);
                }
            }
        });
        assert!(res.is_ok(), "panic on unverified corruption {k}");
    }
}
