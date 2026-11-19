use super::*;
use std::io::Read;
use std::time::Duration;
fn query(text: &str) -> Query {
    Query {
        id: "q1".into(),
        kind: "sample".into(),
        text: text.into(),
        filter: None,
        comparability: "comparable".into(),
        expected_scope: "filename".into(),
    }
}
#[test]
fn stats_empty() {
    assert_eq!(statistics(&[]).count, 0);
    assert_eq!(statistics(&[]).mean, None);
}
#[test]
fn stats_single() {
    let s = statistics(&[7.]);
    assert_eq!(s.p50, Some(7.));
    assert_eq!(s.stddev, Some(0.));
    assert_eq!(s.min, Some(7.));
}
#[test]
fn stats_exact_values() {
    let s = statistics(&[1., 2., 3., 4., 5.]);
    assert_eq!(s.p50, Some(3.));
    assert_eq!(s.mean, Some(3.));
    assert!((s.stddev.unwrap() - 2f64.sqrt()).abs() < 1e-12);
}
#[test]
fn stats_percentiles() {
    let s = statistics(&[1., 2., 3., 4., 5.]);
    assert_eq!(s.p90, Some(4.6));
    assert_eq!(s.p95, Some(4.8));
    assert_eq!(s.p99, Some(4.96));
}
#[test]
fn outliers_flagged_not_removed() {
    let s = statistics(&[1., 1., 1., 1., 100.]);
    assert_eq!(s.outlier_count, 1);
    assert_eq!(s.count, 5);
}
#[test]
fn low_sample_warning() {
    assert!(statistics(&[1.; 9]).low_sample_warning);
    assert!(!statistics(&[1.; 10]).low_sample_warning);
}
#[test]
fn sha256_known() {
    assert_eq!(
        sha256(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
#[test]
fn digest_hides_text() {
    let d = digest(&query("private"), false);
    assert_eq!(d.query_text, None);
    assert_eq!(d.query_length, 7);
}
#[test]
fn digest_opt_in() {
    assert_eq!(digest(&query("abc"), true).query_text, Some("abc".into()));
}
#[test]
fn query_validation() {
    let q = QuerySet {
        schema_version: 1,
        set_version: "v1".into(),
        seed: None,
        queries: vec![query("hi")],
    };
    assert!(validate_query_set(&q).is_ok());
}
#[test]
fn query_reject_schema() {
    let mut q = QuerySet {
        schema_version: 2,
        set_version: "v1".into(),
        seed: None,
        queries: vec![],
    };
    assert!(validate_query_set(&q).is_err());
    q.schema_version = 1;
}
#[test]
fn query_reject_duplicate_id() {
    let q = query("a");
    let mut r = query("b");
    r.id = q.id.clone();
    assert!(validate_query_set(&QuerySet {
        schema_version: 1,
        set_version: "v".into(),
        seed: None,
        queries: vec![q, r]
    })
    .is_err());
}
#[test]
fn query_reject_bad_kind_class() {
    let mut q = query("x");
    q.comparability = "maybe".into();
    assert!(validate_query_set(&QuerySet {
        schema_version: 1,
        set_version: "v".into(),
        seed: None,
        queries: vec![q]
    })
    .is_err());
}
#[test]
fn deterministic_sampler() {
    let n = vec![
        "/a/alpha.txt".into(),
        "/b/beta.pdf".into(),
        "/c/gamma.doc".into(),
    ];
    assert_eq!(sample_queries(&n, 44, 2), sample_queries(&n, 44, 2));
}
#[test]
fn sampler_respects_count() {
    let n = vec!["a.txt".into(), "b.txt".into(), "c.txt".into()];
    assert_eq!(sample_queries(&n, 1, 2).queries.len(), 2);
}
#[test]
fn sampler_dedupes() {
    let n = vec!["/a/same.txt".into(), "/b/same.pdf".into()];
    assert_eq!(sample_queries(&n, 1, 5).queries.len(), 1);
}
#[test]
fn sampler_stores_seed() {
    assert_eq!(sample_queries(&[], 9, 5).seed, Some(9));
}
#[test]
fn fake_top_n_and_zero_count() {
    let mut a = FakeAdapter {
        name: "fake".into(),
        results: vec![],
        latency_ms: 2.,
        error: None,
        timed_out: false,
    };
    let r = a.query(&query("z"), 3, Duration::from_secs(1));
    assert_eq!(r.result_count, 0);
    assert!(r.top_ids.is_empty());
}
#[test]
fn fake_adapter_top_n_hashes() {
    let mut a = FakeAdapter {
        name: "fake".into(),
        results: vec!["a".into(), "b".into()],
        latency_ms: 2.,
        error: None,
        timed_out: false,
    };
    let r = a.query(&query("z"), 1, Duration::from_secs(1));
    assert_eq!(r.top_ids, vec![sha256(b"a")]);
}
struct FakeClock(u64);
impl Clock for FakeClock {
    fn now_ms(&mut self) -> u64 {
        self.0 += 5;
        self.0
    }
}
#[test]
fn measure_warmup_separated() {
    let mut a = FakeAdapter {
        name: "f".into(),
        results: vec![],
        latency_ms: 1.,
        error: None,
        timed_out: false,
    };
    let mut c = FakeClock(0);
    let r = measure(
        &mut a,
        &mut c,
        &NeverCancel,
        &[query("a")],
        2,
        3,
        0,
        4,
        Duration::from_secs(1),
    );
    assert_eq!(
        r.observations
            .iter()
            .filter(|x| x.phase == "warmup")
            .count(),
        2
    );
    assert_eq!(
        r.observations.iter().filter(|x| x.phase == "warm").count(),
        3
    );
    assert!(r.observations.iter().all(|x| x.phase != "cold"));
}
#[test]
fn measure_timeout_status() {
    let mut a = FakeAdapter {
        name: "f".into(),
        results: vec![],
        latency_ms: 12.,
        error: None,
        timed_out: true,
    };
    let r = measure(
        &mut a,
        &mut FakeClock(0),
        &NeverCancel,
        &[query("a")],
        0,
        1,
        0,
        4,
        Duration::from_millis(2),
    );
    assert_eq!(r.observations[0].status, "timeout");
}
#[test]
fn measure_error_status() {
    let mut a = FakeAdapter {
        name: "f".into(),
        results: vec![],
        latency_ms: 1.,
        error: Some("nope".into()),
        timed_out: false,
    };
    let r = measure(
        &mut a,
        &mut FakeClock(0),
        &NeverCancel,
        &[query("a")],
        0,
        1,
        0,
        4,
        Duration::from_secs(1),
    );
    assert_eq!(r.observations[0].status, "error");
}
struct CancelAt(usize);
impl Cancellation for CancelAt {
    fn is_cancelled(&self) -> bool {
        self.0 == 1
    }
}
#[test]
fn measure_cancel_signal() {
    let mut a = FakeAdapter {
        name: "f".into(),
        results: vec![],
        latency_ms: 1.,
        error: None,
        timed_out: false,
    };
    let r = measure(
        &mut a,
        &mut FakeClock(0),
        &CancelAt(1),
        &[query("a")],
        0,
        3,
        0,
        4,
        Duration::from_secs(1),
    );
    assert!(r.cancelled);
    assert!(r.observations.is_empty());
}
#[test]
fn measure_keeps_result_count() {
    let mut a = FakeAdapter {
        name: "f".into(),
        results: vec!["x".into(), "y".into()],
        latency_ms: 1.,
        error: None,
        timed_out: false,
    };
    let r = measure(
        &mut a,
        &mut FakeClock(0),
        &NeverCancel,
        &[query("a")],
        0,
        1,
        0,
        4,
        Duration::from_secs(1),
    );
    assert_eq!(r.observations[0].result_count, 2);
}
fn sample(t: u64, ws: u64, pr: u64, cpu: u64, io: u64) -> ResourceSample {
    ResourceSample {
        at_ms: t,
        working_set_bytes: Some(ws),
        private_bytes: Some(pr),
        cpu_time_ms: Some(cpu),
        read_bytes: Some(io),
        write_bytes: Some(io * 2),
        read_ops: Some(io / 2),
        write_ops: Some(io / 3),
        handles: Some(io),
        threads: Some(2),
    }
}
#[test]
fn resource_delta_and_peaks() {
    let s = summarize_samples(&[sample(0, 10, 20, 4, 3), sample(1, 15, 29, 9, 8)], false);
    assert_eq!(s.working_set_peak_bytes, Some(15));
    assert_eq!(s.private_delta_bytes, Some(9));
    assert_eq!(s.cpu_delta_ms, Some(5));
    assert_eq!(s.read_bytes_delta, Some(5));
    assert_eq!(s.write_bytes_delta, Some(10));
}
#[test]
fn idle_flag_separate() {
    let s = summarize_samples(&[sample(0, 1, 2, 3, 4)], true);
    assert!(s.idle_baseline);
}
#[test]
fn resource_empty_reason() {
    let s = summarize_samples(&[], false);
    assert_eq!(s.status, "unavailable");
    assert_eq!(s.unavailable_reason.as_deref(), Some("no_samples"));
}
#[test]
fn result_schema_required_fields() {
    let v =
        serde_json::json!({"schema_version":1,"run":{},"queries":[],"resources":{},"summary":{}});
    assert!(validate_result(&v).is_ok());
}
#[test]
fn result_schema_missing_field() {
    assert!(validate_result(&serde_json::json!({"schema_version":1})).is_err());
}
#[test]
fn csv_redacts_query() {
    let d = digest(&query("secret text"), false);
    let csv = csv_summary("r", &[(d, statistics(&[]), 0, 0, 0)]);
    assert!(!csv.contains("secret text"));
    assert!(csv.contains("query_hash"));
}

// ============================================================================
// BENCH2C 추가 시험: HTTP 어댑터, 상태 분류, 선택 순서, 색인 준비 확인
// ============================================================================

#[test]
fn http_url_encoding_test() {
    assert_eq!(url_encode("hello world"), "hello%20world");
    assert_eq!(url_encode("보고서_2026.pdf"), "%EB%B3%B4%EA%B3%A0%EC%84%9C_2026.pdf");
    assert_eq!(url_encode("a-b_c.d~e"), "a-b_c.d~e");
}

#[test]
fn http_adapter_fake_server_json_ok() {
    use std::io::Write;
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().unwrap().port();

    let server_handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);

        let body = serde_json::json!({
            "totalResults": 2,
            "results": [
                {"name": "test1.hwp", "path": "C:\\docs"},
                {"name": "test2.pdf", "path": "D:\\data"}
            ]
        })
        .to_string();

        let resp = format!(
            "HTTP/1.1 200 OK\r\n\
             Server: Everything\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n\
             {}",
            body.len(),
            body
        );
        stream.write_all(resp.as_bytes()).unwrap();
    });

    let mut adapter = EverythingHttpAdapter::new(port);
    let q = query("test");
    let res = adapter.query(&q, 10, Duration::from_secs(2));

    server_handle.join().unwrap();

    assert!(res.error.is_none(), "unexpected error: {:?}", res.error);
    assert!(!res.timed_out);
    assert_eq!(res.result_count, 2);
    assert_eq!(res.top_ids.len(), 2);
    assert_eq!(res.top_ids[0], sha256(b"C:\\docs\\test1.hwp"));
    assert_eq!(res.top_ids[1], sha256(b"D:\\data\\test2.pdf"));
}

#[test]
fn http_adapter_fake_server_chunked_ok() {
    use std::io::Write;
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().unwrap().port();

    let server_handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);

        let part1 = "{\"totalResults\": 1, \"results\": [";
        let part2 = "{\"name\": \"chunked.txt\", \"path\": \"C:\\\\tmp\"}]}";

        let header = "HTTP/1.1 200 OK\r\n\
                      Server: Everything\r\n\
                      Transfer-Encoding: chunked\r\n\
                      Connection: close\r\n\
                      \r\n";
        stream.write_all(header.as_bytes()).unwrap();

        // chunk 1
        let c1 = format!("{:X}\r\n{}\r\n", part1.len(), part1);
        stream.write_all(c1.as_bytes()).unwrap();

        // chunk 2
        let c2 = format!("{:X}\r\n{}\r\n", part2.len(), part2);
        stream.write_all(c2.as_bytes()).unwrap();

        // end chunk
        stream.write_all(b"0\r\n\r\n").unwrap();
    });

    let mut adapter = EverythingHttpAdapter::new(port);
    let q = query("chunked");
    let res = adapter.query(&q, 10, Duration::from_secs(2));

    server_handle.join().unwrap();

    assert!(res.error.is_none());
    assert_eq!(res.result_count, 1);
    assert_eq!(res.top_ids.len(), 1);
    assert_eq!(res.top_ids[0], sha256(b"C:\\tmp\\chunked.txt"));
}

#[test]
fn http_adapter_fake_server_error() {
    use std::io::Write;
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let server_handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);

        let resp = "HTTP/1.1 500 Internal Server Error\r\n\
                    Content-Length: 0\r\n\
                    Connection: close\r\n\
                    \r\n";
        stream.write_all(resp.as_bytes()).unwrap();
    });

    let mut adapter = EverythingHttpAdapter::new(port);
    let q = query("err");
    let res = adapter.query(&q, 10, Duration::from_secs(2));

    server_handle.join().unwrap();

    assert!(res.error.is_some());
    assert!(res.error.unwrap().contains("500 Internal Server Error"));
}

#[test]
fn http_adapter_fake_server_auth_required() {
    use std::io::Write;
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let server_handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);

        let resp = "HTTP/1.1 401 Unauthorized\r\n\
                    WWW-Authenticate: Basic realm=\"Everything\"\r\n\
                    Content-Length: 0\r\n\
                    Connection: close\r\n\
                    \r\n";
        stream.write_all(resp.as_bytes()).unwrap();
    });

    let mut adapter = EverythingHttpAdapter::new(port);
    let q = query("auth");
    let res = adapter.query(&q, 10, Duration::from_secs(2));

    server_handle.join().unwrap();

    assert!(res.error.is_some());
    let err = res.error.unwrap();
    assert!(err.contains("인증 필요"), "expected auth required, got: {err}");
}

#[test]
fn http_adapter_fake_server_timeout() {
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let server_handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        // 읽기만 하고 응답 없이 클라이언트 타임아웃보다 오래 수면
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);
        std::thread::sleep(Duration::from_millis(500));
    });

    let mut adapter = EverythingHttpAdapter::new(port);
    let q = query("timeout");
    let res = adapter.query(&q, 10, Duration::from_millis(100));

    server_handle.join().unwrap();

    assert!(res.timed_out || res.error.is_some());
}

#[test]
fn http_port_detection_tests() {
    use std::io::Write;
    use std::net::TcpListener;

    // 1. Available 감지
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let server_handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);
        let resp = "HTTP/1.1 200 OK\r\n\
                    Server: Everything 1.4\r\n\
                    Content-Length: 18\r\n\
                    Connection: close\r\n\
                    \r\n\
                    {\"totalResults\":0}";
        stream.write_all(resp.as_bytes()).unwrap();
    });

    let det = detect_everything_http_port("127.0.0.1", &[port], Duration::from_millis(300));
    server_handle.join().unwrap();
    assert_eq!(det, PortDetectResult::Available(port));

    // 2. AuthRequired 감지
    let listener2 = TcpListener::bind("127.0.0.1:0").unwrap();
    let port2 = listener2.local_addr().unwrap().port();

    let server_handle2 = std::thread::spawn(move || {
        let (mut stream, _) = listener2.accept().unwrap();
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);
        let resp = "HTTP/1.1 401 Unauthorized\r\n\
                    WWW-Authenticate: Basic realm=\"Everything\"\r\n\
                    Content-Length: 0\r\n\
                    Connection: close\r\n\
                    \r\n";
        stream.write_all(resp.as_bytes()).unwrap();
    });

    let det2 = detect_everything_http_port("127.0.0.1", &[port2], Duration::from_millis(300));
    server_handle2.join().unwrap();
    assert_eq!(det2, PortDetectResult::AuthRequired(port2));
}

#[test]
fn everything_status_classification_and_guidance_test() {
    // 1. gui_client_running: 창 있음
    let (st1, g1) = classify_everything_status(true, true, true);
    assert_eq!(st1, "gui_client_running");
    assert!(g1.contains("GUI 클라이언트가 실행 중입니다"));

    // 2. service_only: 창 없으나 프로세스 실행 중
    let (st2, g2) = classify_everything_status(false, true, true);
    assert_eq!(st2, "service_only");
    assert!(g2.contains("Everything 앱(트레이)을 실행하면 측정할 수 있습니다"));
    assert!(g2.contains("HTTP 서버를 켜세요"));

    // 3. not_running: 설치되어 있으나 프로세스 없음
    let (st3, g3) = classify_everything_status(false, false, true);
    assert_eq!(st3, "not_running");
    assert!(g3.contains("Everything이 설치되어 있으나 실행되지 않았습니다"));

    // 4. not_installed: 설치 안 됨
    let (st4, g4) = classify_everything_status(false, false, false);
    assert_eq!(st4, "not_installed");
    assert!(g4.contains("Everything이 설치되어 있지 않습니다"));
}

#[test]
fn everything_selection_order_decision_test() {
    let dummy_detect = EverythingDetect {
        installed: true,
        running: true,
        status: "service_only".into(),
        guidance: "service only guidance".into(),
        pids: vec![123],
        path: None,
        version: None,
        db_loaded: None,
    };

    // 케이스 1: IPC 가용 -> ipc 선택
    let (m1, u1, fb1) = select_order_decision(
        true,
        PortDetectResult::NotFound,
        true,
        &dummy_detect,
    );
    assert_eq!(m1, "ipc");
    assert!(u1.is_empty());
    assert!(!fb1);

    // 케이스 2: IPC 불가, HTTP 가용 -> http 선택
    let (m2, u2, fb2) = select_order_decision(
        false,
        PortDetectResult::Available(80),
        true,
        &dummy_detect,
    );
    assert_eq!(m2, "http");
    assert_eq!(u2.len(), 1);
    assert!(u2[0].contains("ipc:"));
    assert!(!fb2);

    // 케이스 3: IPC 불가, HTTP 인증필요, es.exe 가용 -> es_exe 선택
    let (m3, u3, fb3) = select_order_decision(
        false,
        PortDetectResult::AuthRequired(8080),
        true,
        &dummy_detect,
    );
    assert_eq!(m3, "es_exe");
    assert_eq!(u3.len(), 2);
    assert!(u3[0].contains("ipc:"));
    assert!(u3[1].contains("http:"));
    assert!(!fb3);

    // 케이스 4: IPC 불가, HTTP 불가, es.exe 불가 -> fallback kfind (method: none)
    let (m4, u4, fb4) = select_order_decision(
        false,
        PortDetectResult::NotFound,
        false,
        &dummy_detect,
    );
    assert_eq!(m4, "none");
    assert_eq!(u4.len(), 3);
    assert!(fb4);
}

#[test]
fn kfind_names_index_readiness_validation_test() {
    // 1. Ready & entries > 0 -> Ok
    let val_ok = serde_json::json!({
        "ok": true,
        "result": {
            "state": "ready",
            "entries": 54321
        }
    });
    assert!(validate_kfind_names_status_json(&val_ok).is_ok());

    // 2. empty 상태 -> not_ready
    let val_empty = serde_json::json!({
        "ok": true,
        "result": {
            "state": "empty",
            "entries": 0
        }
    });
    let err_empty = validate_kfind_names_status_json(&val_empty).unwrap_err();
    assert_eq!(err_empty.0, "names index unavailable (empty)");
    assert!(err_empty.1.contains("색인 구축이 완료된 후 다시 실행"));

    // 3. entries == 0 인 경우 -> not_ready (empty)
    let val_zero_entries = serde_json::json!({
        "ok": true,
        "result": {
            "state": "ready",
            "entries": 0
        }
    });
    let err_zero = validate_kfind_names_status_json(&val_zero_entries).unwrap_err();
    assert_eq!(err_zero.0, "names index unavailable (empty)");

    // 4. building 상태 -> not_ready
    let val_building = serde_json::json!({
        "ok": true,
        "result": {
            "state": "building",
            "entries": 100
        }
    });
    let err_building = validate_kfind_names_status_json(&val_building).unwrap_err();
    assert_eq!(err_building.0, "names index building");

    // 5. failed 상태 -> not_ready
    let val_failed = serde_json::json!({
        "ok": true,
        "result": {
            "state": "failed",
            "entries": 0
        }
    });
    let err_failed = validate_kfind_names_status_json(&val_failed).unwrap_err();
    assert!(err_failed.0.contains("names index failed"));
}

