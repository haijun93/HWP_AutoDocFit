//! Everything command line interface (`es.exe`) adapter.
//! Spawns `es.exe -n <N> <query>` and measures execution latency including process spawn cost.

use crate::{sha256, AdapterResult, Query, QueryAdapter};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct EverythingEsAdapter {
    pub name: String,
    pub es_path: PathBuf,
    pub baseline_ms: f64,
    pub includes_process_spawn: bool,
}

impl EverythingEsAdapter {
    pub fn new(es_path: PathBuf, baseline_ms: f64) -> Self {
        Self {
            name: "everything_es".into(),
            es_path,
            baseline_ms,
            includes_process_spawn: true,
        }
    }

    /// 기준선 측정(빈 질의 실행 시간)을 수행하며 어댑터를 생성
    pub fn discover_and_create(
        custom_path: Option<&Path>,
        timeout: Duration,
    ) -> Result<Self, String> {
        let es_path = match custom_path {
            Some(p) if p.is_file() => p.to_path_buf(),
            _ => find_es_exe_path()
                .ok_or_else(|| "es.exe not found in PATH or standard installation directories".to_string())?,
        };

        let baseline_ms = measure_es_baseline(&es_path, timeout)?;
        Ok(Self::new(es_path, baseline_ms))
    }
}

impl QueryAdapter for EverythingEsAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn query(&mut self, query: &Query, top_n: usize, time_limit: Duration) -> AdapterResult {
        let started = Instant::now();

        // es.exe -n <top_n> <query>
        let mut cmd = Command::new(&self.es_path);
        cmd.arg("-n").arg(top_n.to_string()).arg(&query.text);

        // 프로세스 실행 및 출력 획득
        let output = match run_command_with_timeout(&mut cmd, time_limit) {
            Ok(out) => out,
            Err(e) => {
                let timed_out = e.contains("timed out");
                return AdapterResult {
                    latency_ms: started.elapsed().as_secs_f64() * 1000.0,
                    result_count: 0,
                    top_ids: vec![],
                    error: Some(e),
                    timed_out,
                };
            }
        };

        let latency_ms = started.elapsed().as_secs_f64() * 1000.0;

        if !output.status.success() {
            let stderr_str = String::from_utf8_lossy(&output.stderr);
            return AdapterResult {
                latency_ms,
                result_count: 0,
                top_ids: vec![],
                error: Some(format!(
                    "es.exe exited with code {:?}: {}",
                    output.status.code(),
                    stderr_str.trim()
                )),
                timed_out: false,
            };
        }

        let (total, top_ids) = parse_es_output(&output.stdout, top_n);

        AdapterResult {
            latency_ms,
            result_count: total,
            top_ids,
            error: None,
            timed_out: false,
        }
    }
}

/// 빈 질의 실행으로 프로세스 시작 및 종료 기준선 측정 (ms)
pub fn measure_es_baseline(es_path: &Path, timeout: Duration) -> Result<f64, String> {
    let started = Instant::now();
    let mut cmd = Command::new(es_path);
    cmd.arg("-n").arg("0").arg("");

    let output = run_command_with_timeout(&mut cmd, timeout)
        .map_err(|e| format!("es.exe baseline measurement failed: {e}"))?;

    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("es.exe baseline returned non-zero: {}", err.trim()));
    }

    Ok(elapsed_ms)
}

/// es.exe stdout 라인 파싱 -> (결과 수, 상위 N개 해시 목록)
pub fn parse_es_output(stdout_bytes: &[u8], top_n: usize) -> (u64, Vec<String>) {
    let stdout_str = String::from_utf8_lossy(stdout_bytes);
    let lines: Vec<&str> = stdout_str
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    let total = lines.len() as u64;
    let top_ids = lines
        .into_iter()
        .take(top_n)
        .map(|path| sha256(path.as_bytes()))
        .collect();

    (total, top_ids)
}

/// es.exe 경로 검색 (PATH 및 표준 설치 위치)
pub fn find_es_exe_path() -> Option<PathBuf> {
    // 1. PATH 탐색
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("es.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
            #[cfg(not(windows))]
            {
                let candidate_unix = dir.join("es");
                if candidate_unix.is_file() {
                    return Some(candidate_unix);
                }
            }
        }
    }

    // 2. 표준 Everything 설치 폴더
    for candidate in get_standard_es_paths() {
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

fn get_standard_es_paths() -> Vec<PathBuf> {
    let mut list = Vec::new();
    if let Ok(pf) = std::env::var("ProgramFiles") {
        list.push(PathBuf::from(pf).join("Everything").join("es.exe"));
    }
    if let Ok(pfx86) = std::env::var("ProgramFiles(x86)") {
        list.push(PathBuf::from(pfx86).join("Everything").join("es.exe"));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        list.push(
            PathBuf::from(local)
                .join("Programs")
                .join("Everything")
                .join("es.exe"),
        );
    }
    list
}

/// 타임아웃을 지원하는 커맨드 실행 (워커 스레드 사용)
fn run_command_with_timeout(
    cmd: &mut Command,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn process: {e}"))?;

    let child_handle = std::thread::spawn(move || {
        let res = child.wait_with_output();
        let _ = tx.send(res);
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => {
            let _ = child_handle.join();
            Ok(output)
        }
        Ok(Err(e)) => {
            let _ = child_handle.join();
            Err(format!("process execution error: {e}"))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            // 시간 초과
            Err("process execution timed out".into())
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err("process worker thread disconnected".into())
        }
    }
}
