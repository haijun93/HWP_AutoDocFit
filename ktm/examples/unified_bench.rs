// PolyForm Strict License 1.0.0
// https://polyformproject.org/licenses/strict/1.0.0
//! G-UNIFIED 몽땅검색 통합 검색 지연 측정 (fn1_bench와 같은 질의 23개).
//!
//! 파일명(서비스 파이프) + 위계·본문(kfind-core mmap) 세 질의 동시 시작 후 merge_tiers 병합까지의
//! 단계별 및 전체 지연 시간(콜드, 웜 p50/p95)을 측정한다.
//!
//! cargo run --release --example unified_bench -- [--index-root PATH] [--pipe-name NAME] [--runs 30]

use ktm::backend::{Backend, Row, UnifiedContentAnswer, pipe_names_total};
use ktm::tm_state::merge_tiers;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;

const QUERIES: &[(&str, &str)] = &[
    ("1글자", "a"),
    ("1글자", "e"),
    ("1글자", "s"),
    ("1글자", "보"),
    ("2글자", "do"),
    ("2글자", "in"),
    ("2글자", "보고"),
    ("2글자", "문서"),
    ("3글자", "doc"),
    ("3글자", "win"),
    ("3글자", "보고서"),
    ("긴 낱말", "microsoft"),
    ("없는 말", "zqxjv"),
    ("여러 단어", "win sys"),
    ("여러 단어", "program files"),
    ("여러 단어", "microsoft office 16"),
    ("여러 단어", "보고 2026"),
    ("확장자", "ext:pdf"),
    ("확장자", "ext:hwp;hwpx"),
    ("확장자+낱말", "보고 ext:hwp"),
    ("확장자+낱말", "setup ext:exe"),
    ("제외", "report -pdf"),
    ("폴더만", "folder: data"),
];

fn pct(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if v.is_empty() {
        return f64::NAN;
    }
    let k = ((v.len() as f64 - 1.0) * p).round() as usize;
    v[k]
}

fn arg(args: &[String], k: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == k).map(|w| w[1].clone())
}

#[derive(Clone, Debug, Default)]
struct BenchResult {
    total_ms: f64,
    names_ms: f64,
    content_ms: f64,
    count: usize,
}

fn bench_unified_query(
    backend: &mut Backend,
    pipe_name: &str,
    query: &str,
) -> BenchResult {
    let start = Instant::now();
    let (tx, rx) = mpsc::channel();

    let tx_names = tx.clone();
    let pipe_str = pipe_name.to_string();
    let q_str = query.to_string();
    let cancel_names = Arc::new(AtomicBool::new(false));
    let t_names_spawn = Instant::now();

    let h_names = std::thread::spawn(move || {
        let t0 = Instant::now();
        let res = pipe_names_total(&pipe_str, &q_str, "relevance");
        let elapsed = t0.elapsed().as_secs_f64() * 1000.0;
        let rows = res.map(|(r, _)| r).unwrap_or_default();
        let _ = tx_names.send((true, rows, elapsed));
    });

    let t_content_start = Instant::now();
    let content_res = backend.search_unified(query, None);
    let content_elapsed = t_content_start.elapsed().as_secs_f64() * 1000.0;
    let content_ans = content_res.unwrap_or_default();

    let mut names_rows = Vec::new();
    let mut names_elapsed = 0.0;

    let _ = h_names.join();
    while let Ok((is_names, rows, ms)) = rx.try_recv() {
        if is_names {
            names_rows = rows;
            names_elapsed = ms;
        }
    }

    let merged = merge_tiers(names_rows, content_ans.hier_rows, content_ans.body_rows);
    let total_elapsed = start.elapsed().as_secs_f64() * 1000.0;

    BenchResult {
        total_ms: total_elapsed,
        names_ms: names_elapsed,
        content_ms: content_elapsed,
        count: merged.len(),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let runs: usize = arg(&args, "--runs")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let pipe_name = arg(&args, "--pipe-name").unwrap_or_else(|| ktm::backend::PIPE_NAME.into());
    let root = match arg(&args, "--index-root") {
        Some(p) => PathBuf::from(p),
        None => ktm::paths::default_index().unwrap_or_else(|_| PathBuf::from("index")),
    };

    println!("# G-UNIFIED 몽땅검색 통합 검색 지연 측정");
    println!("- 색인 경로: {}", root.display());
    println!("- 파이프: {}", pipe_name);
    println!("- 반복 횟수: {}", runs);
    println!("- 질의 수: {}", QUERIES.len());
    println!();

    let mut backend = match Backend::open(&root) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("경고: 로컬 색인을 열 수 없습니다 ({}). 가상 측정 모드로 진행합니다.", e);
            return;
        }
    };

    let _ = backend.warm();

    println!("| 범주 | 질의 | 콜드(ms) | 웜 p50(ms) | 웜 p95(ms) | 파일명 p50(ms) | 본문·위계 p50(ms) | 결과수 |");
    println!("|---|---|---|---|---|---|---|---|");

    for (cat, q) in QUERIES {
        // 콜드 1회
        let cold = bench_unified_query(&mut backend, &pipe_name, q);

        // 웜 runs 회
        let mut total_times = Vec::with_capacity(runs);
        let mut names_times = Vec::with_capacity(runs);
        let mut content_times = Vec::with_capacity(runs);
        let mut count = cold.count;

        for _ in 0..runs {
            let res = bench_unified_query(&mut backend, &pipe_name, q);
            total_times.push(res.total_ms);
            names_times.push(res.names_ms);
            content_times.push(res.content_ms);
            count = res.count;
        }

        let p50 = pct(&mut total_times, 0.50);
        let p95 = pct(&mut total_times, 0.95);
        let n_p50 = pct(&mut names_times, 0.50);
        let c_p50 = pct(&mut content_times, 0.50);

        println!(
            "| {} | `{}` | {:.2} | **{:.2}** | {:.2} | {:.2} | {:.2} | {} |",
            cat, q, cold.total_ms, p50, p95, n_p50, c_p50, count
        );
    }
}
