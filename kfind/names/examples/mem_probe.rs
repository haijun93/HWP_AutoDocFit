//! Repeatable large synthetic FN1 memory and query probe.
//! Run once per revision: cargo run --release --example mem_probe -- [entry-count]
use kfind_names::query::{Options, Sort, search};
use kfind_names::{Builder, parse};
use std::time::Instant;

const DEFAULT_ENTRIES: usize = 3_343_333;
const DIRECTORY: u32 = 0x10;

fn name(i: usize) -> String {
    match i % 5 {
        0 => format!("보고서_2026_{i:07}.docx"),
        1 => format!("업무계획-{i:07}.hwp"),
        2 => format!("memo_{i:07}.txt"),
        3 => format!("예산 집행 결과 {i:07}.pdf"),
        _ => format!("Archive_{i:07}.zip"),
    }
}

fn pct(sorted: &[f64], p: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

fn main() {
    let target = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_ENTRIES);
    let folder_count = (target / 12).max(1);
    let mut b = Builder::with_capacity(target, target * 28);
    b.add_volume("C:", 5);
    // A 32-way directory tree gives a representative multi-level path distribution.
    for d in 0..folder_count {
        let rec = 6 + d as u64;
        let parent = if d < 32 { 5 } else { 6 + ((d - 1) / 32) as u64 };
        b.push(0, rec, parent, &format!("자료_{d:06}"), DIRECTORY);
    }
    let file_count = target.saturating_sub(folder_count);
    for i in 0..file_count {
        let folder = i % folder_count;
        let rec = 6 + folder_count as u64 + i as u64;
        let parent = 6 + folder as u64;
        let n = name(i);
        let attrs = if i % 97 == 0 { 2 } else { 0 };
        b.push_meta(
            0,
            rec,
            parent,
            &n,
            attrs,
            (i % 4_000_000_000) as u64,
            i as i64,
        );
    }
    eprintln!("building {target} entries...");
    let t = Instant::now();
    let names = b.finish();
    let build_ms = t.elapsed().as_secs_f64() * 1000.0;
    let heap = names.heap_breakdown();
    println!(
        "entries={} heap_bytes={} bytes_per_entry={:.3} build_ms={:.1}",
        names.len(),
        names.heap_bytes(),
        names.heap_bytes() as f64 / names.len() as f64,
        build_ms
    );
    macro_rules! row {
        ($label:literal, $value:expr) => {
            println!(
                "memory.{:<20} bytes={} bytes_per_entry={:.3}",
                $label,
                $value,
                $value as f64 / names.len() as f64
            );
        };
    }
    row!("parent", heap.parent);
    row!("fold_offsets", heap.fold_offsets);
    row!("name_offsets", heap.name_offsets);
    row!("sort_keys", heap.sort_keys);
    row!("extensions", heap.extensions);
    row!("flags", heap.flags);
    row!("size", heap.size);
    row!("mtime", heap.mtime);
    row!("subtree_ends", heap.subtree_ends);
    row!("folded_names", heap.folded_names);
    row!("names", heap.names);
    row!("volume_record_maps", heap.volume_record_maps);
    row!("other", heap.other);

    let v = names.view();
    let queries: Vec<String> = (0..100)
        .map(|i| match i % 5 {
            0 => format!("보고서_{:07}", i * 41),
            1 => format!("업무계획 {:07}", i * 37),
            2 => format!("memo_{:07}", i * 29),
            3 => format!("집행결과 {:07}", i * 31),
            _ => format!("archive_{:07}", i * 43),
        })
        .collect();
    let options = Options {
        limit: 1000,
        sort: Sort::Relevance,
        serial_bytes: usize::MAX,
        ..Default::default()
    };
    let mut ms = Vec::with_capacity(queries.len());
    let mut result_checksum = 0usize;
    for text in &queries {
        let q = parse(text);
        let t = Instant::now();
        let r = search(&v, &q, &options).expect("query");
        ms.push(t.elapsed().as_secs_f64() * 1000.0);
        result_checksum = result_checksum
            .wrapping_add(r.total)
            .wrapping_add(r.top.len());
    }
    ms.sort_by(f64::total_cmp);
    println!(
        "query_count={} p50_ms={:.3} p95_ms={:.3} checksum={}",
        ms.len(),
        pct(&ms, 0.50),
        pct(&ms, 0.95),
        result_checksum
    );
}
