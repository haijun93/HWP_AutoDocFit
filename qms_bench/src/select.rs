//! Adapter selection logic for `--adapter everything`.
//! Priority order: IPC -> HTTP -> es.exe -> fallback to kfind standalone.

use crate::{
    detect_everything, detect_everything_http_port, find_es_exe_path, measure_es_baseline,
    EverythingDetect, EverythingEsAdapter, EverythingHttpAdapter, EverythingIpcAdapter,
    KfindPipeAdapter, PortDetectResult, QueryAdapter,
};
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EverythingMeta {
    pub method: String, // "ipc", "http", "es_exe", "none"
    pub status: String, // "available", "service_only", "not_running", "not_installed"
    pub guidance: String,
    pub unavailable_reasons: Vec<String>,
    pub includes_process_spawn: bool,
    pub baseline_ms: Option<f64>,
}

pub struct EverythingSelection {
    pub adapter: Box<dyn QueryAdapter>,
    pub meta: EverythingMeta,
    pub is_fallback_kfind: bool,
}

/// `--adapter everything` 요청 시 IPC -> HTTP -> es.exe 순으로 사용 가능한 방식을 선택.
/// 하나도 안 되면 몽땅검색(KfindPipeAdapter)으로 폴백.
pub fn select_everything_adapter(explicit_http_port: Option<u16>) -> EverythingSelection {
    let mut unavailable_reasons = Vec::new();
    let detect = detect_everything();

    // 1. IPC 어댑터 시도
    #[cfg(windows)]
    {
        if let Ok(ipc) = EverythingIpcAdapter::new() {
            if ipc.target_hwnd.is_some() {
                return EverythingSelection {
                    adapter: Box::new(ipc),
                    meta: EverythingMeta {
                        method: "ipc".into(),
                        status: "available".into(),
                        guidance: "Everything GUI 클라이언트(IPC)를 통해 측정합니다.".into(),
                        unavailable_reasons,
                        includes_process_spawn: false,
                        baseline_ms: None,
                    },
                    is_fallback_kfind: false,
                };
            } else {
                unavailable_reasons.push(
                    "ipc: Everything message window not found in current session (GUI client not running)".into(),
                );
            }
        } else {
            unavailable_reasons.push("ipc: failed to initialize IPC reply window".into());
        }
    }
    #[cfg(not(windows))]
    {
        unavailable_reasons.push("ipc: IPC is only supported on Windows".into());
    }

    // 2. HTTP 어댑터 시도
    let http_ports = if let Some(p) = explicit_http_port {
        vec![p]
    } else {
        vec![80, 8080, 8888]
    };
    match detect_everything_http_port("127.0.0.1", &http_ports, Duration::from_millis(250)) {
        PortDetectResult::Available(p) => {
            return EverythingSelection {
                adapter: Box::new(EverythingHttpAdapter::new(p)),
                meta: EverythingMeta {
                    method: "http".into(),
                    status: "available".into(),
                    guidance: format!("Everything HTTP 서버(포트 {p})를 통해 측정합니다."),
                    unavailable_reasons,
                    includes_process_spawn: false,
                    baseline_ms: None,
                },
                is_fallback_kfind: false,
            };
        }
        PortDetectResult::AuthRequired(p) => {
            unavailable_reasons.push(format!(
                "http: Everything HTTP server on port {p} requires authentication (password handling prohibited)"
            ));
        }
        PortDetectResult::NotFound => {
            unavailable_reasons.push(format!(
                "http: Everything HTTP server not responding on tested ports ({:?})",
                http_ports
            ));
        }
    }

    // 3. es.exe 어댑터 시도
    if let Some(es_path) = find_es_exe_path() {
        match measure_es_baseline(&es_path, Duration::from_secs(3)) {
            Ok(baseline_ms) => {
                return EverythingSelection {
                    adapter: Box::new(EverythingEsAdapter::new(es_path, baseline_ms)),
                    meta: EverythingMeta {
                        method: "es_exe".into(),
                        status: "available".into(),
                        guidance: "es.exe 명령줄 도구를 통해 측정합니다(프로세스 시작 비용 포함)."
                            .into(),
                        unavailable_reasons,
                        includes_process_spawn: true,
                        baseline_ms: Some(baseline_ms),
                    },
                    is_fallback_kfind: false,
                };
            }
            Err(e) => {
                unavailable_reasons.push(format!("es.exe: baseline measurement failed: {e}"));
            }
        }
    } else {
        unavailable_reasons.push(
            "es.exe: executable not found in PATH or standard installation directories".into(),
        );
    }

    // 4. 전부 불가능할 때 -> 몽땅검색(kfind) 단독 측정으로 폴백
    EverythingSelection {
        adapter: Box::new(KfindPipeAdapter::default()),
        meta: EverythingMeta {
            method: "none".into(),
            status: detect.status,
            guidance: detect.guidance,
            unavailable_reasons,
            includes_process_spawn: false,
            baseline_ms: None,
        },
        is_fallback_kfind: true,
    }
}

/// 모의 입력으로 선택 순서 및 unavailable_reasons 생성을 결정론적으로 검증할 수 있는 순수 함수
pub fn select_order_decision(
    ipc_available: bool,
    http_result: PortDetectResult,
    es_available: bool,
    detect: &EverythingDetect,
) -> (String, Vec<String>, bool) {
    let mut unavailable_reasons = Vec::new();

    // 1. IPC
    if ipc_available {
        return ("ipc".into(), unavailable_reasons, false);
    } else {
        unavailable_reasons.push("ipc: window not found".into());
    }

    // 2. HTTP
    match http_result {
        PortDetectResult::Available(_) => {
            return ("http".into(), unavailable_reasons, false);
        }
        PortDetectResult::AuthRequired(p) => {
            unavailable_reasons.push(format!("http: port {p} auth required"));
        }
        PortDetectResult::NotFound => {
            unavailable_reasons.push("http: server not found".into());
        }
    }

    // 3. es.exe
    if es_available {
        return ("es_exe".into(), unavailable_reasons, false);
    } else {
        unavailable_reasons.push("es.exe: not found".into());
    }

    // 4. Fallback
    let _ = detect;
    ("none".into(), unavailable_reasons, true)
}
