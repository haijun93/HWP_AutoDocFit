//! IS-S1 본문 검색 엔진 비교 시제품. 앱 코드가 아니며 `bench.py`가 부른다.
//!
//! kfind-engine-bench build-a <corpus_dir> <index_dir> [heap_mb] [threads]
//! kfind-engine-bench build-b <corpus_dir> <index_dir>
//! kfind-engine-bench query <a|b> <index_dir> <plan.json> <reps>
//! 결과는 표준 출력 JSON 한 줄.
mod common;
mod engine_a;
mod engine_b;

use common::{dir_size, memory, parse_query, rank, rank_hash, stats, fnv, DocHit, Meta};
use roaring::RoaringBitmap;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

enum Eng { A(engine_a::EngineA), B(engine_b::EngineB) }
enum Sids { V(Vec<u32>), B(RoaringBitmap) }

impl Sids {
    fn iter(&self) -> Box<dyn Iterator<Item = u32> + '_> {
        match self { Sids::V(v) => Box::new(v.iter().copied()), Sids::B(b) => Box::new(b.iter()) }
    }
    fn len(&self) -> u64 { match self { Sids::V(v) => v.len() as u64, Sids::B(b) => b.len() } }
}

impl Eng {
    fn meta(&self) -> &Meta { match self { Eng::A(e) => &e.meta, Eng::B(e) => &e.meta } }
    fn query(&self, words: &[String]) -> Sids {
        match self { Eng::A(e) => Sids::V(e.query(words)), Eng::B(e) => Sids::B(e.query(words)) }
    }
    fn word(&self, w: &str, restrict: Option<&RoaringBitmap>) -> RoaringBitmap {
        match self { Eng::A(e) => e.word(w, restrict), Eng::B(e) => e.word(w, restrict) }
    }
    /// 후보 제한이 계산량을 줄이는 낱말인지(B의 4글자 이상 확인 단계). 그 결과는 낱말 캐시에 넣지 않는다.
    fn restrict_helps(&self, w: &str) -> bool { matches!(self, Eng::B(_)) && w.chars().count() >= 4 }
}

/// 입력 흉내용 앞 결과 재사용(IS-QCACHE 설계): ① 앞 질의의 낱말이 새 질의의 같은 자리 낱말에 모두 포함되면
/// (글자 덧붙임·낱말 추가 = 단조 축소) 앞 결과 비트맵 안에서만 바뀐 낱말을 판정 ② 낱말별 일치 비트맵 캐시.
#[derive(Default)]
struct Session { prev_words: Vec<String>, prev: Option<RoaringBitmap>, cache: HashMap<String, RoaringBitmap>, narrowed: u32, cache_hits: u32 }

impl Session {
    fn step(&mut self, eng: &Eng, words: Vec<String>) -> RoaringBitmap {
        let narrow = self.prev.is_some() && self.prev_words.len() <= words.len()
            && self.prev_words.iter().zip(&words).all(|(p, n)| n.contains(p.as_str()));
        let mut acc: Option<RoaringBitmap> = if narrow { self.narrowed += 1; self.prev.clone() } else { None };
        let todo: Vec<&String> = words.iter().enumerate()
            .filter(|(i, w)| !narrow || self.prev_words.get(*i) != Some(*w)).map(|(_, w)| w).collect();
        let mut pending: Vec<&String> = Vec::new();
        for w in todo {
            if let Some(bm) = self.cache.get(w.as_str()) {
                self.cache_hits += 1;
                acc = Some(match acc { None => bm.clone(), Some(mut a) => { a &= bm; a } });
            } else { pending.push(w); }
        }
        for w in pending {
            if acc.is_some() && eng.restrict_helps(w) {
                acc = Some(eng.word(w, acc.as_ref()));
            } else {
                let bm = eng.word(w, None);
                acc = Some(match acc { None => bm.clone(), Some(mut a) => { a &= &bm; a } });
                self.cache.insert(w.clone(), bm);
            }
        }
        let out = acc.unwrap_or_default();
        self.prev_words = words;
        self.prev = Some(out.clone());
        out
    }
}

fn ms(t: Instant) -> f64 { t.elapsed().as_secs_f64() * 1000.0 }
fn mem_json() -> Value { let (ws, pc, cur) = memory(); json!({"peak_working_set_mb": ws as f64 / 1048576.0, "peak_private_mb": pc as f64 / 1048576.0, "private_now_mb": cur as f64 / 1048576.0}) }
fn hits_json(h: &[DocHit]) -> Value { json!(h.iter().map(|x| [x.0, x.1 as u32, x.2]).collect::<Vec<_>>()) }

fn run_query(eng: &Eng, plan: &Value, reps: usize, open_ms: f64) -> Value {
    let meta = eng.meta();
    let mut first_ms = None;
    let mut out_q = Vec::new();
    for q in plan["queries"].as_array().unwrap() {
        let q = q.as_str().unwrap();
        let words = parse_query(q);
        let t = Instant::now();
        let s = eng.query(&words);
        let (top, _) = rank(meta, s.iter(), Some(20));
        let first = ms(t);
        first_ms.get_or_insert(first);
        // 같은 질의의 첫 '전체' 계산(DFB2: 첫 회와 반복 중앙값을 상위 20·전체 모두 구분해 보고).
        let t = Instant::now();
        let s = eng.query(&words);
        let (_, _) = rank(meta, s.iter(), None);
        let _: Vec<u32> = s.iter().collect();
        let first_full = ms(t);
        let (mut top_s, mut full_s) = (Vec::new(), Vec::new());
        let mut result = Value::Null;
        for r in 0..reps {
            let t = Instant::now();
            let s = eng.query(&words);
            let (top_r, _) = rank(meta, s.iter(), Some(20));
            top_s.push(ms(t));
            assert_eq!(top_r, top);
            let t = Instant::now();
            let s = eng.query(&words);
            let (all, ndocs) = rank(meta, s.iter(), None);
            let sids: Vec<u32> = s.iter().collect();
            full_s.push(ms(t));
            if r == 0 {
                result = json!({"sentences": sids.len(), "docs": ndocs, "sid_hash": format!("{:016x}", fnv(sids.iter().copied())),
                    "rank_hash": format!("{:016x}", rank_hash(&all)), "top20": hits_json(&top)});
            }
        }
        out_q.push(json!({"q": q, "words": words, "first_ms": first, "first_full_ms": first_full, "top20": stats(&top_s), "full": stats(&full_s), "result": result}));
    }
    let mut out_t = Vec::new();
    for seq in plan["typing"].as_array().unwrap() {
        let steps: Vec<&str> = seq["steps"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        let mut fresh: Vec<Vec<f64>> = vec![Vec::new(); steps.len()];
        let mut reuse: Vec<Vec<f64>> = vec![Vec::new(); steps.len()];
        let mut info: Vec<Value> = vec![Value::Null; steps.len()];
        let (mut fresh_tot, mut reuse_tot) = (Vec::new(), Vec::new());
        let (mut narrowed, mut hits) = (0, 0);
        for r in 0..reps {
            let mut sum = 0.0;
            let mut fresh_hash = Vec::new();
            for (i, q) in steps.iter().enumerate() {
                let words = parse_query(q);
                if words.is_empty() { fresh_hash.push(None); continue; }
                let t = Instant::now();
                let s = eng.query(&words);
                let (top, _) = rank(meta, s.iter(), Some(20));
                let e = ms(t);
                fresh[i].push(e); sum += e;
                fresh_hash.push(Some((s.len(), fnv(s.iter()), top)));
            }
            fresh_tot.push(sum);
            let mut sess = Session::default();
            let mut sum = 0.0;
            for (i, q) in steps.iter().enumerate() {
                let words = parse_query(q);
                if words.is_empty() { sess = Session::default(); info[i] = json!({"q": q, "skipped": "2글자 미만(Q8)"}); continue; }
                let t = Instant::now();
                let bm = sess.step(eng, words);
                let (top, _) = rank(meta, bm.iter(), Some(20));
                let e = ms(t);
                reuse[i].push(e); sum += e;
                if r == 0 {
                    let (n, h, ftop) = fresh_hash[i].clone().unwrap();
                    let same = n == bm.len() && h == fnv(bm.iter()) && ftop == top;
                    info[i] = json!({"q": q, "sentences": n, "sid_hash": format!("{h:016x}"), "reuse_equals_fresh": same});
                }
            }
            reuse_tot.push(sum);
            narrowed = sess.narrowed; hits = sess.cache_hits;
        }
        let steps_json: Vec<Value> = info.into_iter().enumerate().map(|(i, mut v)| {
            if !fresh[i].is_empty() { v["fresh"] = stats(&fresh[i]); v["reuse"] = stats(&reuse[i]); }
            v
        }).collect();
        out_t.push(json!({"name": seq["name"], "steps": steps_json, "fresh_total": stats(&fresh_tot), "reuse_total": stats(&reuse_tot),
            "narrowed_steps_last_rep": narrowed, "word_cache_hits_last_rep": hits}));
    }
    json!({"open_ms": open_ms, "first_query_in_process_ms": first_ms, "queries": out_q, "typing": out_t, "memory": mem_json()})
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let cmd = a.get(1).map(String::as_str).unwrap_or("");
    let result = match cmd {
        "build-a" | "build-b" => {
            let (corpus, out) = (Path::new(&a[2]), Path::new(&a[3]));
            if out.exists() { std::fs::remove_dir_all(out).expect("clear index dir"); }
            let t = Instant::now();
            let extra = if cmd == "build-a" {
                let heap = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(200usize) * 1_000_000;
                let threads = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(4usize);
                engine_a::build(corpus, out, heap, threads).expect("build a")
            } else { engine_b::build(corpus, out).expect("build b") };
            json!({"cmd": cmd, "build_s": t.elapsed().as_secs_f64(), "index_bytes": dir_size(out), "extra": extra, "memory": mem_json()})
        }
        "query" => {
            let dir = Path::new(&a[3]);
            let plan: Value = serde_json::from_slice(&std::fs::read(&a[4]).expect("plan")).expect("plan json");
            let reps: usize = a[5].parse().expect("reps");
            let t = Instant::now();
            let eng = if a[2] == "a" { Eng::A(engine_a::EngineA::open(dir).expect("open a")) } else { Eng::B(engine_b::EngineB::open(dir).expect("open b")) };
            let open = ms(t);
            let mut v = run_query(&eng, &plan, reps, open);
            v["engine"] = json!(a[2]);
            v
        }
        _ => { eprintln!("usage: build-a|build-b|query (see source header)"); std::process::exit(2); }
    };
    println!("{}", result);
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::{fold, read_corpus};

    /// 작은 말뭉치에서 두 엔진·재사용 세션이 단순 전수 비교와 같은 문장 집합을 내는지.
    #[test]
    fn engines_match_brute_force() {
        let base = std::env::temp_dir().join(format!("kfind-engine-bench-test-{}", std::process::id()));
        let corpus = base.join("corpus");
        std::fs::create_dir_all(&corpus).unwrap();
        let lines = ["0\t8\t예산 집행 계획을 세운다.", "0\t3\t예산집행은 늦다.", "1\t8\t집행 예산 사업.", "1\t8\t사업 계획 주민.",
            "2\t8\t예산을 집 행사에.", "2\t5\t집행계획 예산 예산 예산.", "3\t8\tABC 예산-집행 계획서."];
        std::fs::write(corpus.join("corpus.tsv"), lines.join("\n") + "\n").unwrap();
        engine_a::build(&corpus, &base.join("a"), 15_000_000, 1).unwrap();
        engine_b::build(&corpus, &base.join("b")).unwrap();
        let mut folds = Vec::new();
        read_corpus(&corpus.join("corpus.tsv"), |_, _, _, t| folds.push(fold(t))).unwrap();
        let ea = Eng::A(engine_a::EngineA::open(&base.join("a")).unwrap());
        let eb = Eng::B(engine_b::EngineB::open(&base.join("b")).unwrap());
        let queries = ["예산", "예산 집행", "예산집행", "예산 집행 계획", "집행계획", "산집", "예산집행계획", "abc 예산집행계획서", "없는말", "계획 주민 사업"];
        for q in queries {
            let words = parse_query(q);
            let expect: Vec<u32> = (0..folds.len() as u32).filter(|&i| words.iter().all(|w| folds[i as usize].contains(w.as_str()))).collect();
            for e in [&ea, &eb] {
                assert_eq!(e.query(&words).iter().collect::<Vec<_>>(), expect, "query {q}");
            }
        }
        // 입력 흉내: 재사용 결과가 매번 새로 찾은 결과와 같아야 한다.
        for e in [&ea, &eb] {
            let mut s = Session::default();
            for q in ["예산", "예산 집", "예산 집행", "예산 집행 계", "예산 집행 계획", "예산 집행 계획서", "예산집", "예산집행"] {
                let words = parse_query(q);
                let fresh: Vec<u32> = e.query(&words).iter().collect();
                assert_eq!(s.step(e, words).iter().collect::<Vec<_>>(), fresh, "typing {q}");
            }
        }
        drop((ea, eb));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cover_offsets_cover_word() {
        assert_eq!(engine_a::cover_offsets(4), vec![0, 1]);
        assert_eq!(engine_a::cover_offsets(6), vec![0, 3]);
        assert_eq!(engine_a::cover_offsets(7), vec![0, 3, 4]);
    }
}
