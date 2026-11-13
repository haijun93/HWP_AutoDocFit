//! KFC1 측정: 문서 범위(허용 집합) 거르기의 상위 20 지연.
//!
//! kfc1_scope_bench <색인 폴더(비어 있어야 함)> <문서 수> <허용 문서 수> <반복 수>
//!
//! 합성 말뭉치(결정적 난수, 문서마다 문장 12개)로 색인을 만들고, 검색어마다 거르기 없음 / 허용 집합(흩어진 문서) /
//! 허용 집합(이어진 문서 ID) / 허용 집합 + 제외 집합의 상위 20 지연 중앙값·최대를 잰다. JSON 한 덩어리로 출력한다.
use kfind_core::{Index, RoaringTreemap, SearchOptions, Sentence, SplitMode, Writer, WriterOptions};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

const WORDS: &[&str] = &[
    "예산", "집행", "계획", "추진", "사업", "주민", "안전", "관리", "점검", "보고", "축제", "도로", "공사", "협의", "지원", "결과",
    "현황", "개선", "청소", "복지", "문화", "교육", "환경", "시설", "주민센터", "프로그램", "행정", "민원", "공원", "주차",
];
const TAILS: &[&str] = &["을", "를", "이", "가", "은", "는", "의", "에", "으로", "", "", ""];
const QUERIES: &[&str] = &["예산", "예산 집행", "주민센터 프로그램", "예산집행", "도로 공사 협의", "민원 처리", "공원"];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn pick<'a>(&mut self, v: &[&'a str]) -> &'a str {
        v[(self.next() % v.len() as u64) as usize]
    }
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 {
        eprintln!("usage: kfc1_scope_bench <index dir> <docs> <allow docs> <repeat>");
        std::process::exit(2);
    }
    let dir = Path::new(&args[1]);
    let (n_docs, n_allow, repeat): (u64, u64, usize) = (args[2].parse().unwrap(), args[3].parse().unwrap(), args[4].parse().unwrap());
    let mut rng = Rng(0x9e3779b97f4a7c15);
    let t = Instant::now();
    let mut w = Writer::open(dir, WriterOptions::default()).expect("writer");
    let mut texts: Vec<String> = Vec::new();
    for doc in 0..n_docs {
        texts.clear();
        for _ in 0..12 {
            let n = 4 + (rng.next() % 6) as usize;
            let s: Vec<String> = (0..n).map(|_| format!("{}{}", rng.pick(WORDS), rng.pick(TAILS))).collect();
            texts.push(s.join(" ") + ".");
        }
        let sents: Vec<Sentence> = texts.iter().enumerate().map(|(i, t)| Sentence { para: i as u32 / 2, level: [1u8, 3, 8, 8][i % 4], text: t }).collect();
        w.add_document(doc, 20_200_000 + (rng.next() % 70_000) as i64, &sents).expect("add");
    }
    w.commit().expect("commit");
    w.merge_all().expect("merge");
    let build_ms = ms(t);
    let index = Index::open(dir).expect("open");

    let scattered: RoaringTreemap = (0..n_docs).filter(|d| d % (n_docs / n_allow).max(1) == 0).take(n_allow as usize).collect();
    let contiguous: RoaringTreemap = (0..n_allow.min(n_docs)).collect();
    let deny: RoaringTreemap = scattered.iter().step_by(10).collect();
    let base = SearchOptions { top: 20, split: SplitMode::DictLongest, time_limit: None, refs_per_doc: 3, ..SearchOptions::default() };
    let cases: Vec<(&str, SearchOptions)> = vec![
        ("none", base.clone()),
        ("allow_scattered", SearchOptions { allow: Some(Arc::new(scattered.clone())), ..base.clone() }),
        ("allow_contiguous", SearchOptions { allow: Some(Arc::new(contiguous)), ..base.clone() }),
        ("allow_scattered_deny", SearchOptions { allow: Some(Arc::new(scattered.clone())), deny: Some(Arc::new(deny)), ..base.clone() }),
    ];
    let mut out = Vec::new();
    for q in QUERIES {
        let mut row = vec![format!("\"query\":\"{q}\"")];
        for (name, o) in &cases {
            let first = index.search(q, o).expect("search");
            let mut times: Vec<f64> = (0..repeat)
                .map(|_| {
                    let t = Instant::now();
                    let r = index.search(q, o).expect("search");
                    std::hint::black_box(r.hits.len());
                    ms(t)
                })
                .collect();
            times.sort_by(f64::total_cmp);
            row.push(format!(
                "\"{name}\":{{\"docs\":{},\"sentences\":{},\"median_ms\":{:.3},\"max_ms\":{:.3}}}",
                first.total_docs,
                first.total_sentences,
                times[times.len() / 2],
                times[times.len() - 1]
            ));
        }
        out.push(format!("{{{}}}", row.join(",")));
    }
    println!(
        "{{\"docs\":{n_docs},\"sentences\":{},\"allow_docs\":{},\"repeat\":{repeat},\"build_ms\":{build_ms:.0},\"queries\":[{}]}}",
        index.num_sentences(),
        scattered.len(),
        out.join(",")
    );
}
