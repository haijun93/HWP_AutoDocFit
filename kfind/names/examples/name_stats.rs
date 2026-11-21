//! Read-only filename length statistics. Emits aggregate numbers only.
//! cargo run --release --example name_stats
use std::{collections::VecDeque, fs, path::PathBuf, time::{Duration, Instant}};

const LIMIT: usize = 3_000_000;
const TIME_LIMIT: Duration = Duration::from_secs(60);

fn main() {
    let mut roots = vec![PathBuf::from(r"C:\Windows"), PathBuf::from(r"C:\Program Files")];
    if let Ok(users) = fs::read_dir(r"C:\Users") {
        for e in users.flatten() {
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) { roots.push(e.path()); }
        }
    }
    let started = Instant::now();
    let mut queue: VecDeque<(PathBuf, usize)> = roots.into_iter().map(|p| (p, 0)).collect();
    let mut lengths = Vec::new();
    let (mut dirs, mut files, mut errors, mut depth_sum, mut depth_n, mut hangul) = (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    while let Some((dir, depth)) = queue.pop_front() {
        if started.elapsed() >= TIME_LIMIT || dirs + files >= LIMIT { break; }
        let entries = match fs::read_dir(&dir) { Ok(v) => v, Err(_) => { errors += 1; continue; } };
        for item in entries {
            if started.elapsed() >= TIME_LIMIT || dirs + files >= LIMIT { break; }
            let e = match item { Ok(v) => v, Err(_) => { errors += 1; continue; } };
            let name = e.file_name();
            let text = name.to_string_lossy();
            lengths.push(text.as_bytes().len());
            if text.chars().any(|c| matches!(c as u32, 0xAC00..=0xD7A3 | 0x1100..=0x11FF | 0x3130..=0x318F)) { hangul += 1; }
            match e.file_type() {
                Ok(t) if t.is_dir() => { dirs += 1; depth_sum += depth + 1; depth_n += 1; queue.push_back((e.path(), depth + 1)); }
                Ok(_) => files += 1,
                Err(_) => errors += 1,
            }
        }
    }
    lengths.sort_unstable();
    let n = lengths.len();
    let percentile = |p: f64| lengths[((n as f64 * p).ceil() as usize).saturating_sub(1).min(n.saturating_sub(1))];
    let total: usize = lengths.iter().sum();
    println!("entries={} directories={} files={} errors={} elapsed_seconds={:.2} capped={}", n, dirs, files, errors, started.elapsed().as_secs_f64(), n >= LIMIT || started.elapsed() >= TIME_LIMIT);
    if n > 0 { println!("name_utf8_bytes_mean={:.3} median={} p90={} max={} hangul_name_percent={:.3} mean_directory_depth={:.3} folder_file_ratio={:.6}", total as f64 / n as f64, percentile(0.5), percentile(0.9), lengths[n-1], hangul as f64 * 100.0 / n as f64, if depth_n > 0 { depth_sum as f64 / depth_n as f64 } else { 0.0 }, dirs as f64 / files.max(1) as f64); }
}
