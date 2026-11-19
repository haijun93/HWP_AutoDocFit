//! Platform-independent search benchmark primitives.
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path, time::Duration};

#[cfg(windows)]
pub mod win;
#[cfg(windows)]
pub use win::*;

#[cfg(not(windows))]
pub mod non_win {
    use crate::{AdapterResult, CounterReader, Query, QueryAdapter, ResourceSample};
    use serde::{Deserialize, Serialize};
    use std::time::Duration;

    #[derive(Debug, Clone)]
    pub struct KfindPipeAdapter {
        pub name: String,
    }
    impl KfindPipeAdapter {
        pub fn new(_: impl Into<String>) -> Self {
            Self {
                name: "kfind".into(),
            }
        }
    }
    impl QueryAdapter for KfindPipeAdapter {
        fn name(&self) -> &str {
            &self.name
        }
        fn query(&mut self, _: &Query, _: usize, _: Duration) -> AdapterResult {
            AdapterResult {
                latency_ms: 0.0,
                result_count: 0,
                top_ids: vec![],
                error: Some("kfind pipe adapter is only supported on Windows".into()),
                timed_out: false,
            }
        }
    }

    pub struct EverythingIpcAdapter {
        pub name: String,
    }
    impl EverythingIpcAdapter {
        pub fn new() -> Result<Self, String> {
            Ok(Self {
                name: "everything".into(),
            })
        }
    }
    impl QueryAdapter for EverythingIpcAdapter {
        fn name(&self) -> &str {
            &self.name
        }
        fn query(&mut self, _: &Query, _: usize, _: Duration) -> AdapterResult {
            AdapterResult {
                latency_ms: 0.0,
                result_count: 0,
                top_ids: vec![],
                error: Some("Everything IPC adapter is only supported on Windows".into()),
                timed_out: false,
            }
        }
    }

    pub struct WindowsProcessCounter;
    impl WindowsProcessCounter {
        pub fn for_pid(_: u32) -> Self {
            Self
        }
        pub fn for_process_names(_: &[&str]) -> Result<Self, String> {
            Err("WindowsProcessCounter is only supported on Windows".into())
        }
    }
    impl CounterReader for WindowsProcessCounter {
        fn read(&mut self) -> Result<ResourceSample, String> {
            Err("WindowsProcessCounter is only supported on Windows".into())
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct DetectReport {
        pub ok: bool,
        pub command: String,
        pub error: Option<String>,
    }
    pub fn detect_all() -> DetectReport {
        DetectReport {
            ok: false,
            command: "detect".into(),
            error: Some("detect is only supported on Windows".into()),
        }
    }
}
#[cfg(not(windows))]
pub use non_win::*;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Query {
    pub id: String,
    pub kind: String,
    pub text: String,
    #[serde(default)]
    pub filter: Option<String>,
    pub comparability: String,
    #[serde(default)]
    pub expected_scope: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuerySet {
    pub schema_version: u32,
    pub set_version: String,
    #[serde(default)]
    pub seed: Option<u64>,
    pub queries: Vec<Query>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryDigest {
    pub id: String,
    pub kind: String,
    pub query_length: usize,
    pub query_hash: String,
    pub query_text: Option<String>,
    pub comparability: String,
}

pub fn validate_query_set(qs: &QuerySet) -> Result<(), String> {
    if qs.schema_version != 1 {
        return Err("unsupported schema_version".into());
    }
    if qs.set_version.trim().is_empty() {
        return Err("set_version is required".into());
    }
    let mut ids = std::collections::HashSet::new();
    for q in &qs.queries {
        if q.id.trim().is_empty() || !ids.insert(&q.id) {
            return Err("query ids must be non-empty and unique".into());
        }
        if q.kind.trim().is_empty() {
            return Err(format!("{}: kind is required", q.id));
        }
        if q.text.is_empty() {
            return Err(format!("{}: query text is required", q.id));
        }
        if !["comparable", "standalone"].contains(&q.comparability.as_str()) {
            return Err(format!("{}: invalid comparability", q.id));
        }
    }
    Ok(())
}
pub fn load_query_set(path: impl AsRef<Path>) -> Result<QuerySet, String> {
    let b = fs::read(path).map_err(|e| e.to_string())?;
    let slice = b.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&b);
    let q: QuerySet = serde_json::from_slice(slice).map_err(|e| e.to_string())?;
    validate_query_set(&q)?;
    Ok(q)
}
pub fn digest(q: &Query, include_text: bool) -> QueryDigest {
    QueryDigest {
        id: q.id.clone(),
        kind: q.kind.clone(),
        query_length: q.text.chars().count(),
        query_hash: format!("sha256:{}", sha256(q.text.as_bytes())),
        query_text: include_text.then(|| q.text.clone()),
        comparability: q.comparability.clone(),
    }
}

/// Deterministic sample selection. Uses a seeded Fisher-Yates shuffle, never filesystem access.
pub fn sample_queries(file_names: &[String], seed: u64, count: usize) -> QuerySet {
    let mut unique: Vec<String> = file_names
        .iter()
        .filter_map(|f| {
            std::path::Path::new(f)
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_owned)
        })
        .filter(|s| !s.trim().is_empty())
        .collect();
    unique.sort();
    unique.dedup();
    let mut state = seed;
    for i in (1..unique.len()).rev() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let j = (state as usize) % (i + 1);
        unique.swap(i, j)
    }
    let queries = unique
        .into_iter()
        .take(count)
        .enumerate()
        .map(|(i, text)| Query {
            id: format!("q{:03}", i + 1),
            kind: match text.chars().count() {
                2 => "korean_2char",
                3 => "korean_3char",
                _ => "sampled_filename",
            }
            .into(),
            text,
            filter: None,
            comparability: "comparable".into(),
            expected_scope: "filename".into(),
        })
        .collect();
    QuerySet {
        schema_version: 1,
        set_version: "sampled-v1".into(),
        seed: Some(seed),
        queries,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdapterResult {
    pub latency_ms: f64,
    pub result_count: u64,
    pub top_ids: Vec<String>,
    pub error: Option<String>,
    pub timed_out: bool,
}
pub trait QueryAdapter {
    fn name(&self) -> &str;
    fn query(&mut self, query: &Query, top_n: usize, time_limit: Duration) -> AdapterResult;
}
#[derive(Debug, Clone)]
pub struct FakeAdapter {
    pub name: String,
    pub results: Vec<String>,
    pub latency_ms: f64,
    pub error: Option<String>,
    pub timed_out: bool,
}
impl QueryAdapter for FakeAdapter {
    fn name(&self) -> &str {
        &self.name
    }
    fn query(&mut self, _: &Query, top_n: usize, _: Duration) -> AdapterResult {
        AdapterResult {
            latency_ms: self.latency_ms,
            result_count: self.results.len() as u64,
            top_ids: self
                .results
                .iter()
                .take(top_n)
                .map(|s| sha256(s.as_bytes()))
                .collect(),
            error: self.error.clone(),
            timed_out: self.timed_out,
        }
    }
}

pub trait Clock {
    fn now_ms(&mut self) -> u64;
}
#[derive(Default)]
pub struct SystemClock {
    start: Option<std::time::Instant>,
}
impl Clock for SystemClock {
    fn now_ms(&mut self) -> u64 {
        let s = self.start.get_or_insert_with(std::time::Instant::now);
        s.elapsed().as_millis() as u64
    }
}
pub trait Cancellation {
    fn is_cancelled(&self) -> bool;
}
#[derive(Default)]
pub struct NeverCancel;
impl Cancellation for NeverCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Observation {
    pub latency_ms: f64,
    pub result_count: u64,
    pub top_ids: Vec<String>,
    pub status: String,
    pub error: Option<String>,
    pub phase: String,
    pub repeat: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunOutput {
    pub adapter: String,
    pub observations: Vec<Observation>,
    pub cancelled: bool,
}
pub fn measure<A: QueryAdapter, C: Clock, K: Cancellation>(
    adapter: &mut A,
    clock: &mut C,
    cancel: &K,
    queries: &[Query],
    warmups: usize,
    repeats: usize,
    cold_repeats: usize,
    top_n: usize,
    limit: Duration,
) -> RunOutput {
    let mut out = RunOutput {
        adapter: adapter.name().into(),
        observations: vec![],
        cancelled: false,
    };
    for q in queries {
        for phase in ["warmup", "warm", "cold"] {
            let n = if phase == "warmup" {
                warmups
            } else if phase == "warm" {
                repeats
            } else {
                cold_repeats
            };
            for i in 0..n {
                if cancel.is_cancelled() {
                    out.cancelled = true;
                    return out;
                }
                let start = clock.now_ms();
                let r = adapter.query(q, top_n, limit);
                let elapsed = clock.now_ms().saturating_sub(start) as f64;
                out.observations.push(observation(r, elapsed, phase, i + 1));
            }
        }
    }
    out
}
fn observation(r: AdapterResult, elapsed: f64, phase: &str, repeat: usize) -> Observation {
    let status = if r.timed_out {
        "timeout"
    } else if r.error.is_some() {
        "error"
    } else {
        "ok"
    };
    Observation {
        latency_ms: if r.timed_out || r.error.is_some() {
            elapsed.max(r.latency_ms)
        } else {
            elapsed.max(r.latency_ms)
        },
        result_count: r.result_count,
        top_ids: r.top_ids,
        status: status.into(),
        error: r.error,
        phase: phase.into(),
        repeat,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Statistics {
    pub count: usize,
    pub p50: Option<f64>,
    pub p90: Option<f64>,
    pub p95: Option<f64>,
    pub p99: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub mean: Option<f64>,
    pub stddev: Option<f64>,
    pub outlier_bounds: Option<(f64, f64)>,
    pub outlier_count: usize,
    pub low_sample_warning: bool,
}
pub fn statistics(values: &[f64]) -> Statistics {
    if values.is_empty() {
        return Statistics {
            count: 0,
            p50: None,
            p90: None,
            p95: None,
            p99: None,
            min: None,
            max: None,
            mean: None,
            stddev: None,
            outlier_bounds: None,
            outlier_count: 0,
            low_sample_warning: true,
        };
    }
    let mut s = values.to_vec();
    s.sort_by(f64::total_cmp);
    let mean = s.iter().sum::<f64>() / s.len() as f64;
    let sd = (s.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / s.len() as f64).sqrt();
    let q1 = percentile(&s, 0.25).unwrap();
    let q3 = percentile(&s, 0.75).unwrap();
    let bounds = (q1 - 1.5 * (q3 - q1), q3 + 1.5 * (q3 - q1));
    Statistics {
        count: s.len(),
        p50: percentile(&s, 0.5),
        p90: percentile(&s, 0.9),
        p95: percentile(&s, 0.95),
        p99: percentile(&s, 0.99),
        min: s.first().copied(),
        max: s.last().copied(),
        mean: Some(mean),
        stddev: Some(sd),
        outlier_bounds: Some(bounds),
        outlier_count: s.iter().filter(|&&x| x < bounds.0 || x > bounds.1).count(),
        low_sample_warning: s.len() < 10,
    }
}
fn percentile(s: &[f64], p: f64) -> Option<f64> {
    if s.is_empty() {
        None
    } else {
        let rank = p * (s.len() - 1) as f64;
        let lo = rank.floor() as usize;
        let hi = rank.ceil() as usize;
        Some(s[lo] + (s[hi] - s[lo]) * (rank - lo as f64))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ResourceSample {
    pub at_ms: u64,
    pub working_set_bytes: Option<u64>,
    pub private_bytes: Option<u64>,
    pub cpu_time_ms: Option<u64>,
    pub read_bytes: Option<u64>,
    pub write_bytes: Option<u64>,
    pub read_ops: Option<u64>,
    pub write_ops: Option<u64>,
    pub handles: Option<u64>,
    pub threads: Option<u64>,
}
pub trait CounterReader {
    fn read(&mut self) -> Result<ResourceSample, String>;
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceSummary {
    pub status: String,
    pub unavailable_reason: Option<String>,
    pub samples: usize,
    pub working_set_peak_bytes: Option<u64>,
    pub working_set_avg_bytes: Option<f64>,
    pub private_peak_bytes: Option<u64>,
    pub private_delta_bytes: Option<i64>,
    pub cpu_delta_ms: Option<u64>,
    pub read_bytes_delta: Option<u64>,
    pub write_bytes_delta: Option<u64>,
    pub read_ops_delta: Option<u64>,
    pub write_ops_delta: Option<u64>,
    pub handles_peak: Option<u64>,
    pub threads_peak: Option<u64>,
    pub idle_baseline: bool,
}
pub fn summarize_samples(samples: &[ResourceSample], idle_baseline: bool) -> ResourceSummary {
    if samples.is_empty() {
        return ResourceSummary {
            status: "unavailable".into(),
            unavailable_reason: Some("no_samples".into()),
            samples: 0,
            working_set_peak_bytes: None,
            working_set_avg_bytes: None,
            private_peak_bytes: None,
            private_delta_bytes: None,
            cpu_delta_ms: None,
            read_bytes_delta: None,
            write_bytes_delta: None,
            read_ops_delta: None,
            write_ops_delta: None,
            handles_peak: None,
            threads_peak: None,
            idle_baseline,
        };
    }
    let vals =
        |f: fn(&ResourceSample) -> Option<u64>| samples.iter().filter_map(f).collect::<Vec<_>>();
    let delta = |f: fn(&ResourceSample) -> Option<u64>| {
        let v = vals(f);
        if v.len() < 2 {
            None
        } else {
            Some((*v.last()?).saturating_sub(*v.first()?))
        }
    };
    let ws = vals(|s| s.working_set_bytes);
    let pr = vals(|s| s.private_bytes);
    ResourceSummary {
        status: "available".into(),
        unavailable_reason: None,
        samples: samples.len(),
        working_set_peak_bytes: ws.iter().copied().max(),
        working_set_avg_bytes: if ws.is_empty() {
            None
        } else {
            Some(ws.iter().sum::<u64>() as f64 / ws.len() as f64)
        },
        private_peak_bytes: pr.iter().copied().max(),
        private_delta_bytes: if pr.len() < 2 {
            None
        } else {
            Some(pr.last().unwrap().saturating_sub(pr[0]) as i64)
        },
        cpu_delta_ms: delta(|s| s.cpu_time_ms),
        read_bytes_delta: delta(|s| s.read_bytes),
        write_bytes_delta: delta(|s| s.write_bytes),
        read_ops_delta: delta(|s| s.read_ops),
        write_ops_delta: delta(|s| s.write_ops),
        handles_peak: vals(|s| s.handles).iter().copied().max(),
        threads_peak: vals(|s| s.threads).iter().copied().max(),
        idle_baseline,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchRun {
    pub schema_version: u32,
    pub run: serde_json::Value,
    pub queries: Vec<serde_json::Value>,
    pub resources: serde_json::Value,
    pub summary: serde_json::Value,
}
pub fn validate_result(v: &serde_json::Value) -> Result<(), String> {
    for f in ["schema_version", "run", "queries", "resources", "summary"] {
        if v.get(f).is_none() {
            return Err(format!("missing required field: {f}"));
        }
    }
    if v["schema_version"].as_u64() != Some(1) {
        return Err("unsupported schema_version".into());
    }
    Ok(())
}
pub fn write_results(
    json_path: impl AsRef<Path>,
    csv_path: impl AsRef<Path>,
    json: &serde_json::Value,
    csv: &str,
) -> Result<(), String> {
    validate_result(json)?;
    atomic_write(
        json_path.as_ref(),
        serde_json::to_vec_pretty(json)
            .map_err(|e| e.to_string())?
            .as_slice(),
    )
    .map_err(|e| e.to_string())?;
    atomic_write(csv_path.as_ref(), csv.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}
fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(format!(".tmp-{}", std::process::id()));
    let tmp = std::path::PathBuf::from(tmp);
    fs::write(&tmp, bytes)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}
pub fn csv_summary(run_id: &str, rows: &[(QueryDigest, Statistics, u64, usize, usize)]) -> String {
    let mut s=String::from("run_id,query_id,kind,query_length,query_hash,comparability,p50_ms,p90_ms,p95_ms,p99_ms,min_ms,max_ms,mean_ms,stddev_ms,result_count,timeouts,errors\n");
    for (d, st, count, to, err) in rows {
        s.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            run_id,
            d.id,
            d.kind,
            d.query_length,
            d.query_hash,
            d.comparability,
            cell(st.p50),
            cell(st.p90),
            cell(st.p95),
            cell(st.p99),
            cell(st.min),
            cell(st.max),
            cell(st.mean),
            cell(st.stddev),
            count,
            to,
            err
        ))
    }
    s
}
fn cell(x: Option<f64>) -> String {
    x.map(|v| v.to_string()).unwrap_or_default()
}

// Small dependency-free SHA-256 implementation for privacy-preserving stable identifiers.
pub fn sha256(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut m = data.to_vec();
    let bits = (m.len() as u64) * 8;
    m.push(0x80);
    while m.len() % 64 != 56 {
        m.push(0)
    }
    m.extend_from_slice(&bits.to_be_bytes());
    for c in m.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, b) in c.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes(b.try_into().unwrap())
        }
        for i in 16..64 {
            let a = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let b = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(a)
                .wrapping_add(w[i - 7])
                .wrapping_add(b)
        }
        let (mut a, mut b, mut c0, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c0) ^ (b & c0);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c0;
            c0 = b;
            b = a;
            a = t1.wrapping_add(t2)
        }
        for (i, x) in [a, b, c0, d, e, f, g, hh].iter().enumerate() {
            h[i] = h[i].wrapping_add(*x)
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

#[cfg(test)]
mod tests;
