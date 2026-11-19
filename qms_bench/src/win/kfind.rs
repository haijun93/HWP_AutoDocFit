use crate::{sha256, AdapterResult, Query, QueryAdapter};
use serde_json::{json, Value};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

pub const DEFAULT_KFIND_PIPE_NAME: &str = r"\\.\pipe\HwpAutoDocFit.Index";
const MAX_PIPE_RESPONSE_SIZE: usize = 8 * 1024 * 1024; // 8MB

#[derive(Debug, Clone)]
pub struct KfindPipeAdapter {
    pub name: String,
    pub pipe_name: String,
    next_id: std::sync::Arc<AtomicU64>,
}

impl Default for KfindPipeAdapter {
    fn default() -> Self {
        Self::new(DEFAULT_KFIND_PIPE_NAME)
    }
}

impl KfindPipeAdapter {
    pub fn new(pipe_name: impl Into<String>) -> Self {
        Self {
            name: "kfind".into(),
            pipe_name: pipe_name.into(),
            next_id: std::sync::Arc::new(AtomicU64::new(1)),
        }
    }
}

impl QueryAdapter for KfindPipeAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn query(&mut self, query: &Query, top_n: usize, time_limit: Duration) -> AdapterResult {
        let req_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = json!({
            "command": "names_query",
            "id": req_id,
            "text": query.text,
            "limit": top_n,
            "sort": "relevance",
        });

        let started = Instant::now();
        let pipe_name = self.pipe_name.clone();

        let (tx, rx) = mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("kfind-pipe-call".into())
            .spawn(move || {
                let res = call_pipe_blocking(&pipe_name, &request);
                let _ = tx.send(res);
            });

        if let Err(e) = handle {
            return AdapterResult {
                latency_ms: started.elapsed().as_secs_f64() * 1000.0,
                result_count: 0,
                top_ids: vec![],
                error: Some(format!("thread spawn failed: {e}")),
                timed_out: false,
            };
        }

        match rx.recv_timeout(time_limit) {
            Ok(Ok(bytes)) => {
                let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
                match parse_kfind_response(&bytes, top_n) {
                    Ok((total, top_ids)) => AdapterResult {
                        latency_ms,
                        result_count: total,
                        top_ids,
                        error: None,
                        timed_out: false,
                    },
                    Err(e) => AdapterResult {
                        latency_ms,
                        result_count: 0,
                        top_ids: vec![],
                        error: Some(e),
                        timed_out: false,
                    },
                }
            }
            Ok(Err(e)) => AdapterResult {
                latency_ms: started.elapsed().as_secs_f64() * 1000.0,
                result_count: 0,
                top_ids: vec![],
                error: Some(e),
                timed_out: false,
            },
            Err(mpsc::RecvTimeoutError::Timeout) => AdapterResult {
                latency_ms: started.elapsed().as_secs_f64() * 1000.0,
                result_count: 0,
                top_ids: vec![],
                error: Some("kfind pipe call timed out".into()),
                timed_out: true,
            },
            Err(mpsc::RecvTimeoutError::Disconnected) => AdapterResult {
                latency_ms: started.elapsed().as_secs_f64() * 1000.0,
                result_count: 0,
                top_ids: vec![],
                error: Some("kfind pipe worker disconnected".into()),
                timed_out: false,
            },
        }
    }
}

fn call_pipe_blocking(name: &str, request: &Value) -> Result<Vec<u8>, String> {
    let req_bytes = serde_json::to_vec(request).map_err(|e| e.to_string())?;
    let mut pipe = OpenOptions::new()
        .read(true)
        .write(true)
        .open(name)
        .map_err(|e| format!("pipe open failed: {e}"))?;

    let len = req_bytes.len() as u32;
    pipe.write_all(&len.to_le_bytes())
        .and_then(|_| pipe.write_all(&req_bytes))
        .map_err(|e| format!("pipe write failed: {e}"))?;

    let mut prefix = [0u8; 4];
    pipe.read_exact(&mut prefix)
        .map_err(|e| format!("pipe read prefix failed: {e}"))?;
    let resp_len = u32::from_le_bytes(prefix) as usize;
    if resp_len > MAX_PIPE_RESPONSE_SIZE {
        return Err(format!(
            "response exceeds max size ({} > {})",
            resp_len, MAX_PIPE_RESPONSE_SIZE
        ));
    }

    let mut buf = vec![0u8; resp_len];
    pipe.read_exact(&mut buf)
        .map_err(|e| format!("pipe read body failed: {e}"))?;
    Ok(buf)
}

pub fn parse_kfind_response(bytes: &[u8], top_n: usize) -> Result<(u64, Vec<String>), String> {
    let v: Value = serde_json::from_slice(bytes).map_err(|e| format!("invalid json: {e}"))?;
    if v["ok"] != true {
        let err = v["error"]
            .as_str()
            .unwrap_or("unknown kfind index error")
            .to_string();
        return Err(err);
    }
    let res = &v["result"];
    let items = res["items"]
        .as_array()
        .ok_or_else(|| "missing items array in result".to_string())?;

    let total = res["total"].as_u64().unwrap_or(items.len() as u64);

    let top_ids = items
        .iter()
        .take(top_n)
        .map(|item| {
            let name = item["name"].as_str().unwrap_or_default();
            let dir = item["dir"].as_str().unwrap_or_default();
            let full_path = if dir.is_empty() {
                name.to_string()
            } else if dir.ends_with('\\') {
                format!("{dir}{name}")
            } else {
                format!("{dir}\\{name}")
            };
            sha256(full_path.as_bytes())
        })
        .collect();

    Ok((total, top_ids))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KfindIndexCheck {
    Ready,
    NotReady { reason: String, guidance: String },
    Unavailable(String),
}

/// 몽땅검색 `names_status` 응답 JSON 검증
pub fn validate_kfind_names_status_json(val: &Value) -> Result<(), (String, String)> {
    let guidance = "몽땅검색 파일명 색인이 아직 준비되지 않았습니다. 색인 구축이 완료된 후 다시 실행해 주세요.".to_string();

    if val.get("ok").and_then(Value::as_bool) != Some(true) {
        let err_str = val.get("error").and_then(Value::as_str).unwrap_or("unknown error");
        return Err((format!("kfind status error: {err_str}"), guidance));
    }

    let result = match val.get("result") {
        Some(r) => r,
        None => return Err(("missing result in names_status response".into(), guidance)),
    };

    let state = result.get("state").and_then(Value::as_str).unwrap_or("empty");
    let entries = result.get("entries").and_then(Value::as_u64).unwrap_or(0);

    if state == "building" {
        return Err(("names index building".into(), guidance));
    }

    if state == "failed" {
        let failed_detail = result.get("failed").and_then(Value::as_str).unwrap_or("");
        let reason = if failed_detail.is_empty() {
            "names index failed".into()
        } else {
            format!("names index failed: {failed_detail}")
        };
        return Err((reason, guidance));
    }

    if state == "empty" || entries == 0 {
        return Err(("names index unavailable (empty)".into(), guidance));
    }

    if state != "ready" {
        return Err((format!("names index {state}"), guidance));
    }

    Ok(())
}

/// kfind 명명 파이프에 읽기 전용 `names_status` 명령을 전송하여 색인 준비 상태 확인
pub fn check_kfind_names_index(pipe_name: &str, timeout: Duration) -> KfindIndexCheck {
    let request = json!({
        "command": "names_status",
    });

    let (tx, rx) = mpsc::channel();
    let p_name = pipe_name.to_string();
    let handle = std::thread::Builder::new()
        .name("kfind-status-check".into())
        .spawn(move || {
            let res = call_pipe_blocking(&p_name, &request);
            let _ = tx.send(res);
        });

    if let Err(e) = handle {
        return KfindIndexCheck::Unavailable(format!("thread spawn failed: {e}"));
    }

    match rx.recv_timeout(timeout) {
        Ok(Ok(bytes)) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(val) => match validate_kfind_names_status_json(&val) {
                Ok(()) => KfindIndexCheck::Ready,
                Err((reason, guidance)) => KfindIndexCheck::NotReady { reason, guidance },
            },
            Err(e) => KfindIndexCheck::Unavailable(format!("invalid JSON in status response: {e}")),
        },
        Ok(Err(e)) => KfindIndexCheck::Unavailable(e),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            KfindIndexCheck::Unavailable("kfind status check timed out".into())
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            KfindIndexCheck::Unavailable("kfind status worker disconnected".into())
        }
    }
}

