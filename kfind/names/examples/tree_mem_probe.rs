//! Read-only directory walk and in-process private-byte probe.
//! Usage: cargo run --release --example tree_mem_probe -- <directory>
#[cfg(windows)]
fn main() {
    use std::time::Instant;
    use kfind_names::{Builder, Options, Searcher, Sort, parse, walk};
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    fn private_bytes() -> usize {
        let mut c: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
        c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c as *mut _ as *mut _, c.cb) };
        c.PrivateUsage
    }

    let root = std::env::args().nth(1).expect("directory argument");
    println!("baseline_private_bytes={}", private_bytes());
    let mut b = Builder::new();
    let walk_started = Instant::now();
    let stats = walk::index_tree(&mut b, &root);
    println!(
        "walk entries={} dirs={} files={} errors={} private_bytes={} elapsed_ms={:.1}",
        b.len() + 1,
        stats.dirs,
        stats.files,
        stats.errors,
        private_bytes(),
        walk_started.elapsed().as_secs_f64() * 1000.0
    );
    let finish_started = Instant::now();
    let names = b.finish();
    let heap = names.heap_breakdown();
    println!(
        "built entries={} heap_bytes={} private_bytes={} ratio_private_to_heap={:.3} finish_ms={:.1}",
        names.len(),
        names.heap_bytes(),
        private_bytes(),
        private_bytes() as f64 / names.heap_bytes().max(1) as f64,
        finish_started.elapsed().as_secs_f64() * 1000.0
    );
    println!("heap_breakdown={heap:?}");
    let view = names.view();
    let mut searcher = Searcher::new();
    let options = Options { limit: 1000, sort: Sort::Relevance, ..Default::default() };
    let mut times = Vec::with_capacity(100);
    let mut checksum = 0usize;
    for i in 0..100 {
        let query = parse(&format!("probe_{i:03}"));
        let started = Instant::now();
        let result = searcher.search(&view, names.generation(), &query, &options).expect("query");
        times.push(started.elapsed().as_secs_f64() * 1000.0);
        checksum = checksum.wrapping_add(result.total).wrapping_add(result.top.len());
    }
    times.sort_by(f64::total_cmp);
    println!(
        "queries=100 p50_ms={:.3} checksum={} post_query_private_bytes={}",
        times[49], checksum, private_bytes()
    );
}

#[cfg(not(windows))]
fn main() {
    eprintln!("tree_mem_probe is Windows-only");
}
