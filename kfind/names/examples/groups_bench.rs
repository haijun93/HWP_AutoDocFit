//! QMS2PIPE1: 합성 색인(결정적)에서 1묶음 `Searcher::search`(서비스가 쓰던 경로) 와 `search_groups`(1묶음) 시간 비교, 그리고 2~3묶음 교집합 시간.
//! `cargo run --release --example groups_bench [항목수]`
use kfind_names::query::parse_groups;
use kfind_names::{Builder, Options, Searcher, parse};
use std::time::Instant;

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(2_000_000);
    let mut b = Builder::new();
    let v = b.add_volume("C:", 5);
    let words = ["예산", "집행", "계획", "보고서", "회의", "결과", "report", "plan", "draft", "공사", "계약", "자료"];
    let mut x: u64 = 88172645463325252;
    for d in 0..2000u64 {
        b.push(v, 100 + d, 5, &format!("폴더{d}"), 0x10);
    }
    for i in 0..n {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let w1 = words[(x % 12) as usize];
        let w2 = words[((x >> 8) % 12) as usize];
        let ext = ["hwp", "pdf", "txt", "xlsx"][((x >> 16) % 4) as usize];
        b.push(v, 10_000 + i, 100 + (x >> 24) % 2000, &format!("{w1}_{w2}_{i}.{ext}"), 0);
    }
    let names = b.finish();
    let view = names.view();
    let o = Options { limit: 100, ..Default::default() };
    let med = |f: &mut dyn FnMut() -> usize| {
        let mut t: Vec<f64> = (0..15).map(|_| { let s = Instant::now(); f(); s.elapsed().as_secs_f64() * 1000.0 }).collect();
        t.sort_by(|a, b| a.partial_cmp(b).unwrap());
        t[t.len() / 2]
    };
    println!("entries={}", names.len());
    for text in ["보고서", "예산 집행", "report ext:pdf", "자료 -draft"] {
        let q = parse(text);
        let g = parse_groups(text);
        let a = med(&mut || { let mut s = Searcher::new(); s.search(&view, names.generation(), &q, &o).unwrap().total });
        let b2 = med(&mut || { let mut s = Searcher::new(); s.search_groups(&view, names.generation(), &g, &o).unwrap().total });
        println!("1묶음 {text:<16} Searcher::search {a:8.3} ms   search_groups {b2:8.3} ms");
    }
    for text in ["보고서; 예산", "report; plan; ext:pdf", "공사 계약; 자료; ext:hwp"] {
        let g = parse_groups(text);
        let t = med(&mut || { let mut s = Searcher::new(); s.search_groups(&view, names.generation(), &g, &o).unwrap().total });
        let total = Searcher::new().search_groups(&view, names.generation(), &g, &o).unwrap().total;
        println!("{}묶음 {text:<26} {t:8.3} ms  total={total}", g.len());
    }
}
