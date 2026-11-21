//! Build a capped, read-only directory sample with the production Builder and emit numbers only.
//! cargo run --release --example name_index_probe -- [entry-limit]
#[cfg(windows)]
fn main() {
    use kfind_names::query::{Options, Sort, search};
    use kfind_names::{Builder, parse};
    use std::collections::VecDeque;
    use std::fs;
    use std::os::windows::fs::MetadataExt;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    fn private_bytes() -> usize {
        let mut c: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
        c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        let ok = unsafe {
            K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c as *mut _ as *mut _, c.cb)
        };
        assert_ne!(ok, 0, "GetProcessMemoryInfo failed");
        c.PrivateUsage
    }

    let limit = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000_000usize);
    let started = Instant::now();
    let mut roots = vec![
        PathBuf::from(r"C:\Windows"),
        PathBuf::from(r"C:\Program Files"),
    ];
    if let Ok(entries) = fs::read_dir(r"C:\Users") {
        for e in entries.flatten() {
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                roots.push(e.path());
            }
        }
    }
    println!("baseline_private_bytes={}", private_bytes());
    let mut b = Builder::new();
    let mut queue = VecDeque::new();
    let mut next_rec = Vec::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        let vol = b.add_volume("sample", 0);
        queue.push_back((root, vol, 0u64, 1usize));
        next_rec.push(1u64);
    }
    let mut entries = 0usize;
    let (mut dirs, mut files, mut errors) = (0usize, 0usize, 0usize);
    while entries < limit && started.elapsed() < Duration::from_secs(75) {
        let Some((dir, vol, parent, depth)) = queue.pop_front() else {
            break;
        };
        let children = match fs::read_dir(dir) {
            Ok(v) => v,
            Err(_) => {
                errors += 1;
                continue;
            }
        };
        for item in children {
            if entries >= limit || started.elapsed() >= Duration::from_secs(75) {
                break;
            }
            let e = match item {
                Ok(v) => v,
                Err(_) => {
                    errors += 1;
                    continue;
                }
            };
            let ty = match e.file_type() {
                Ok(v) => v,
                Err(_) => {
                    errors += 1;
                    continue;
                }
            };
            let md = match e.metadata() {
                Ok(v) => v,
                Err(_) => {
                    errors += 1;
                    continue;
                }
            };
            let rec = next_rec[vol as usize];
            next_rec[vol as usize] += 1;
            let attrs = md.file_attributes();
            let name = e.file_name().to_string_lossy().into_owned();
            // Match ntfs::index_volume: it calls Builder::push (unknown size/time).
            b.push(vol, rec, parent, &name, attrs);
            entries += 1;
            if ty.is_dir() {
                dirs += 1;
                // Avoid following reparse points, as the production walker does.
                if attrs & 0x400 == 0 {
                    queue.push_back((e.path(), vol, rec, depth + 1));
                }
            } else {
                files += 1;
            }
        }
    }
    let walk_ms = started.elapsed().as_secs_f64() * 1000.0;
    println!(
        "walk entries={} dirs={} files={} errors={} capped={} private_bytes={} elapsed_ms={:.1}",
        entries,
        dirs,
        files,
        errors,
        entries >= limit,
        private_bytes(),
        walk_ms
    );
    let finish_started = Instant::now();
    let mut names = b.finish();
    let heap_before = names.heap_bytes();
    let breakdown = names.heap_breakdown();
    let built_private = private_bytes();
    println!(
        "built entries={} heap_bytes={} bytes_per_entry={:.3} private_bytes={} private_per_entry={:.3} finish_ms={:.1}",
        names.len(),
        heap_before,
        heap_before as f64 / names.len().max(1) as f64,
        built_private,
        built_private as f64 / names.len().max(1) as f64,
        finish_started.elapsed().as_secs_f64() * 1000.0
    );
    println!("heap_breakdown={breakdown:?}");
    let queries: Vec<String> = (0..100)
        .map(|i| {
            [
                "a", "e", "pdf", "doc", "2026", "system", "setup", "readme", "dll", "txt",
            ][i % 10]
                .to_string()
        })
        .collect();
    let options = Options {
        limit: 1000,
        sort: Sort::Relevance,
        ..Default::default()
    };
    let run_queries = |n: &kfind_names::Names| {
        let view = n.view();
        let mut times = Vec::with_capacity(100);
        let mut checksum = 0usize;
        let mut results = Vec::with_capacity(100);
        for text in &queries {
            let q = parse(text);
            let t = Instant::now();
            let r = search(&view, &q, &options).expect("query");
            times.push(t.elapsed().as_secs_f64() * 1000.0);
            checksum = checksum.wrapping_add(r.total).wrapping_add(r.top.len());
            results.push((r.total, r.top));
        }
        times.sort_by(f64::total_cmp);
        (times[49], checksum, results)
    };
    let (before_p50, before_sum, before_results) = run_queries(&names);
    names.shrink_to_fit();
    let heap_after = names.heap_bytes();
    let private_after = private_bytes();
    let (after_p50, after_sum, after_results) = run_queries(&names);
    println!(
        "after_shrink heap_bytes={} bytes_per_entry={:.3} private_bytes={} private_per_entry={:.3} heap_reclaimed_bytes={} private_delta_bytes={}",
        heap_after,
        heap_after as f64 / names.len().max(1) as f64,
        private_after,
        private_after as f64 / names.len().max(1) as f64,
        heap_before.saturating_sub(heap_after),
        private_after as i64 - built_private as i64
    );
    println!(
        "queries=100 before_p50_ms={:.3} after_p50_ms={:.3} before_checksum={} after_checksum={} equal={}",
        before_p50,
        after_p50,
        before_sum,
        after_sum,
        before_results == after_results
    );
}

#[cfg(not(windows))]
fn main() {
    eprintln!("name_index_probe is Windows-only");
}
