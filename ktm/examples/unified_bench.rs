//! One isolated synthetic measurement run; optional named-pipe fixture for the GUI probe.
//! unified_bench NEW_CORPUS_DIR [ISOLATED_PIPE_NAME]
mod unified_support;
use ktm::backend::{MAX_SEARCH_RESULTS, unified};
use ktm::tm_state::{self, ResultKind};
use serde_json::json;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

fn stats(values: &[f64]) -> serde_json::Value {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    json!({"samples_ms": values, "p50_ms": sorted[sorted.len()/2], "max_ms": sorted[sorted.len()-1]})
}

fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    let corpus = unified_support::Corpus::create(std::path::Path::new(
        args.get(1).ok_or("NEW_CORPUS_DIR [ISOLATED_PIPE_NAME]")?,
    ))?;
    let mut backend = corpus.backend()?;
    backend.warm()?;
    let mut results = Vec::new();
    for query in ["예산집행", "2026; 공사 계약"] {
        let mut legacy_names = Vec::new();
        let mut legacy_hierarchy = Vec::new();
        let mut legacy_body = Vec::new();
        let mut legacy_full = Vec::new();
        let mut first = Vec::new();
        let mut full = Vec::new();
        let mut unified_count = 0;
        let mut legacy_count = 0;
        for _ in 0..7 {
            let start = Instant::now();
            let mut batches = Vec::new();
            for (kind, top) in unified::TIERS {
                let part = Instant::now();
                let answer = corpus.fetch(&mut backend, query, kind, top)?;
                let elapsed = part.elapsed().as_secs_f64() * 1000.0;
                match kind {
                    ResultKind::Names => legacy_names.push(elapsed),
                    ResultKind::Hierarchy => legacy_hierarchy.push(elapsed),
                    ResultKind::Body => legacy_body.push(elapsed),
                }
                batches.push((kind, answer.rows));
                legacy_count =
                    tm_state::combine_results(&batches, |r| unified::path_key(&r.path)).len();
            }
            legacy_full.push(start.elapsed().as_secs_f64() * 1000.0);
            let start = Instant::now();
            unified::stream(
                80,
                &AtomicBool::new(false),
                |kind, top| {
                    assert_eq!(top, MAX_SEARCH_RESULTS);
                    corpus.fetch(&mut backend, query, kind, top)
                },
                |snapshot| {
                    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                    if snapshot.completed == ResultKind::Names {
                        first.push(elapsed);
                    }
                    if snapshot.final_part {
                        full.push(elapsed);
                        unified_count = snapshot.answer.rows.len();
                    }
                },
            )?;
        }
        results.push(json!({"query":query,"legacy_names":stats(&legacy_names),"legacy_hierarchy":stats(&legacy_hierarchy),
            "legacy_body":stats(&legacy_body),"legacy_full":stats(&legacy_full),"unified_first":stats(&first),
            "unified_full":stats(&full),"legacy_count":legacy_count,"unified_count":unified_count}));
    }
    let report = json!({"seed":unified_support::SEED,"documents":unified_support::COUNT,"samples_per_query":7,
        "includes_transport_or_paint":false,"queries":results});
    std::fs::write(
        corpus.root.join("backend-bench.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    println!("{report}");
    if let Some(pipe) = args.get(2) {
        unified_support::serve(corpus, pipe)?;
    }
    Ok(())
}
