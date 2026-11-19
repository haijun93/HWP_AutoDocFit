use super::*;
use crate::{CounterReader, Query, QueryAdapter};
use std::{
    ffi::c_void,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::{DataExchange::COPYDATASTRUCT, LibraryLoader::GetModuleHandleW},
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, PeekMessageW,
        PostQuitMessage, RegisterClassW, SendMessageW, TranslateMessage, HWND_MESSAGE, MSG,
        PM_REMOVE, WM_COPYDATA, WM_DESTROY, WNDCLASSW,
    },
};

// ============================================================================
// 1. 가짜 Everything 서버 (테스트용 숨은 창)
// ============================================================================

unsafe extern "system" fn fake_server_wndproc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if m == WM_COPYDATA {
        let cds = unsafe { &*(l as *const COPYDATASTRUCT) };
        if cds.dwData == EVERYTHING_IPC_COPYDATA_QUERY2W && cds.cbData >= 28 {
            let req =
                unsafe { std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize) };
            let reply_hwnd = u32::from_le_bytes(req[0..4].try_into().unwrap()) as usize as HWND;
            let reply_id = u32::from_le_bytes(req[4..8].try_into().unwrap());
            let max_results = u32::from_le_bytes(req[16..20].try_into().unwrap());

            // 가짜 결과 2개 생성
            let fake_items = ["C:\\Docs\\Report.pdf", "C:\\Docs\\Summary.docx"];
            let numitems = if max_results == 0 {
                0
            } else {
                (fake_items.len() as u32).min(max_results)
            };
            let totitems = 42u32; // 가짜 총 건수

            // EVERYTHING_IPC_LIST2 버퍼 조립
            let mut resp = Vec::new();
            resp.extend_from_slice(&totitems.to_le_bytes());
            resp.extend_from_slice(&numitems.to_le_bytes());
            resp.extend_from_slice(&0u32.to_le_bytes()); // offset
            resp.extend_from_slice(&EVERYTHING_IPC_FLAG_NAME.to_le_bytes());
            resp.extend_from_slice(&EVERYTHING_IPC_SORT_NAME_ASCENDING.to_le_bytes());

            // 아이템 헤더 (20 + numitems * 8) 바이트 뒤에 문자열 데이터 배치
            let string_data_start = 20 + (numitems as usize) * 8;
            let mut string_bytes = Vec::new();
            let mut offsets = Vec::new();

            for &item_path in fake_items.iter().take(numitems as usize) {
                let curr_offset = string_data_start + string_bytes.len();
                offsets.push(curr_offset as u32);
                for c in item_path.encode_utf16().chain([0]) {
                    string_bytes.extend_from_slice(&c.to_le_bytes());
                }
            }

            for &offset in &offsets {
                resp.extend_from_slice(&0u32.to_le_bytes()); // flags
                resp.extend_from_slice(&offset.to_le_bytes()); // data_offset
            }
            resp.extend_from_slice(&string_bytes);

            let reply_cds = COPYDATASTRUCT {
                dwData: reply_id as usize,
                cbData: resp.len() as u32,
                lpData: resp.as_ptr() as *mut c_void,
            };

            unsafe {
                SendMessageW(
                    reply_hwnd,
                    WM_COPYDATA,
                    h as usize,
                    &reply_cds as *const _ as isize,
                );
            }
            return 1;
        }
    } else if m == WM_DESTROY {
        unsafe {
            PostQuitMessage(0);
        }
        return 0;
    }
    unsafe { DefWindowProcW(h, m, w, l) }
}

struct FakeServerHandle {
    hwnd: HWND,
    stopped: Arc<AtomicBool>,
    thread_handle: Option<std::thread::JoinHandle<()>>,
}

impl FakeServerHandle {
    fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let stopped_clone = Arc::clone(&stopped);

        let thread_handle = std::thread::spawn(move || {
            let class_name = wide_str("FakeEverythingServerClass");
            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(fake_server_wndproc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: unsafe { GetModuleHandleW(std::ptr::null()) },
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: class_name.as_ptr(),
            };
            unsafe {
                RegisterClassW(&wc);
            }

            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class_name.as_ptr(),
                    class_name.as_ptr(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE,
                    std::ptr::null_mut(),
                    GetModuleHandleW(std::ptr::null()),
                    std::ptr::null(),
                )
            };

            let _ = tx.send(hwnd as usize);

            let mut msg: MSG = unsafe { std::mem::zeroed() };
            while !stopped_clone.load(Ordering::Relaxed) {
                while unsafe { PeekMessageW(&mut msg, hwnd, 0, 0, PM_REMOVE) } != 0 {
                    unsafe {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            if !hwnd.is_null() {
                unsafe {
                    DestroyWindow(hwnd);
                }
            }
        });

        let hwnd_raw = rx
            .recv_timeout(Duration::from_secs(3))
            .expect("fake server started");
        Self {
            hwnd: hwnd_raw as HWND,
            stopped,
            thread_handle: Some(thread_handle),
        }
    }
}

impl Drop for FakeServerHandle {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Some(h) = self.thread_handle.take() {
            let _ = h.join();
        }
    }
}

// ============================================================================
// 테스트 함수들
// ============================================================================

#[test]
fn fake_everything_server_roundtrip() {
    let server = FakeServerHandle::start();
    let mut adapter = EverythingIpcAdapter::with_target(server.hwnd).unwrap();

    let q = Query {
        id: "q_test".into(),
        kind: "test".into(),
        text: "*.pdf".into(),
        filter: None,
        comparability: "comparable".into(),
        expected_scope: "filename".into(),
    };

    // 1) 상위 10건 질의
    let res = adapter.query(&q, 10, Duration::from_secs(2));
    assert!(res.error.is_none(), "error: {:?}", res.error);
    assert!(!res.timed_out);
    assert_eq!(res.result_count, 42); // totitems
    assert_eq!(res.top_ids.len(), 2);
    assert!(res.latency_ms >= 0.0);

    // 2) 건수만 요청 (top_n = 0)
    let count_res = adapter.query(&q, 0, Duration::from_secs(2));
    assert!(count_res.error.is_none());
    assert_eq!(count_res.result_count, 42);
    assert_eq!(count_res.top_ids.len(), 0);
}

#[test]
fn everything_list2_parsing_edge_cases() {
    // 1) 헤더보다 짧은 바이트
    assert!(parse_everything_list2(&[0u8; 10], 10).is_err());

    // 2) 정상 빈 결과
    let mut empty_header = vec![0u8; 20];
    empty_header[0..4].copy_from_slice(&0u32.to_le_bytes()); // totitems = 0
    empty_header[4..8].copy_from_slice(&0u32.to_le_bytes()); // numitems = 0
    let (total, ids) = parse_everything_list2(&empty_header, 10).unwrap();
    assert_eq!(total, 0);
    assert_eq!(ids.len(), 0);
}

#[test]
fn kfind_parse_response_ok() {
    let json_bytes = br#"{
        "ok": true,
        "result": {
            "total": 128,
            "ms": 1.25,
            "items": [
                {"name": "test1.hwp", "dir": "C:\\Work", "size": 1024, "mtime": 1000},
                {"name": "test2.hwp", "dir": "C:\\Work\\", "size": 2048, "mtime": 2000}
            ]
        }
    }"#;

    let (tot, ids) = parse_kfind_response(json_bytes, 10).unwrap();
    assert_eq!(tot, 128);
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], crate::sha256(b"C:\\Work\\test1.hwp"));
    assert_eq!(ids[1], crate::sha256(b"C:\\Work\\test2.hwp"));
}

#[test]
fn kfind_parse_response_err() {
    let json_bytes = br#"{"ok": false, "error": "names index unavailable"}"#;
    let res = parse_kfind_response(json_bytes, 10);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "names index unavailable");
}

#[test]
fn process_counter_self_pid() {
    let my_pid = std::process::id();
    let mut counter = WindowsProcessCounter::for_pid(my_pid);

    let sample = counter.read().expect("self pid should be readable");
    assert!(sample.working_set_bytes.unwrap() > 0);
    assert!(sample.private_bytes.unwrap() > 0);
    assert!(sample.handles.unwrap() > 0);
    assert!(sample.threads.unwrap() >= 1);
    assert!(sample.cpu_time_ms.is_some());
    assert!(sample.read_bytes.is_some());
    assert!(sample.at_ms > 0);
}

#[test]
fn process_counter_nonexistent_process() {
    let res = WindowsProcessCounter::for_process_names(&["NonExistent_Process_Fake_12345.exe"]);
    assert_eq!(res.err(), Some("process_not_found".into()));
}

#[test]
fn process_counter_invalid_pid() {
    let mut counter = WindowsProcessCounter::for_pid(999_999_999);
    let res = counter.read();
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        err.contains("process_not_found")
            || err.contains("open_process_failed")
            || err.contains("access_denied")
    );
}

#[test]
fn detect_smoke_test() {
    let report = detect_all();
    assert!(report.ok);
    assert_eq!(report.command, "detect");
    // Everything 실행 여부 및 kfind 파이프 여부가 bool로 반환되는지 확인
    println!("Detect report: {:?}", report);
}

// ============================================================================
// 실제 Everything 확인 시험 (#[ignore])
// QMS_BENCH_LIVE=1 환경변수일 때 실행 중인 실제 Everything에 읽기 전용 질의 전송
// ============================================================================

#[test]
#[ignore]
fn live_everything_real_instance_test() {
    if std::env::var("QMS_BENCH_LIVE").unwrap_or_default() != "1" {
        eprintln!("Skipping live_everything_real_instance_test because QMS_BENCH_LIVE != 1");
        return;
    }

    println!("=== LIVE EVERYTHING REAL INSTANCE TEST START ===");
    let window_info = find_everything_window();
    println!("Everything window found: {:?}", window_info);
    assert!(
        window_info.is_some(),
        "LIVE TEST FAILED: Everything window not found (Everything.exe must be running)"
    );

    let (ev_hwnd, ev_pid) = window_info.unwrap();
    let ver = get_everything_version(ev_hwnd);
    let db_loaded = is_everything_db_loaded(ev_hwnd);
    println!("Everything version: {ver}, DB loaded: {db_loaded}, PID: {ev_pid}");

    let mut adapter = EverythingIpcAdapter::new().expect("EverythingIpcAdapter creation failed");

    // 질의 1: 건수만 확인 (*.pdf, top_n = 0)
    let q1 = Query {
        id: "live_q1".into(),
        kind: "live".into(),
        text: "*.pdf".into(),
        filter: None,
        comparability: "comparable".into(),
        expected_scope: "filename".into(),
    };

    let start1 = std::time::Instant::now();
    let res1 = adapter.query(&q1, 0, Duration::from_secs(5));
    let elapsed1 = start1.elapsed();

    println!("Query 1 (*.pdf, top_n=0) Result:");
    println!(
        "  Latency: {:.2} ms (roundtrip: {:.2} ms)",
        res1.latency_ms,
        elapsed1.as_secs_f64() * 1000.0
    );
    println!("  Total Result Count: {}", res1.result_count);
    println!("  Top IDs Count: {}", res1.top_ids.len());
    println!("  Error: {:?}", res1.error);
    println!("  Timed Out: {}", res1.timed_out);

    assert!(
        res1.error.is_none(),
        "Live query 1 failed with error: {:?}",
        res1.error
    );
    assert!(!res1.timed_out, "Live query 1 timed out");
    assert!(
        res1.result_count > 0,
        "Expected non-zero result count for *.pdf"
    );
    assert_eq!(
        res1.top_ids.len(),
        0,
        "top_n=0 should return 0 items in top_ids"
    );

    // 질의 2: 상위 5건 확인 ("*.txt", top_n = 5)
    let q2 = Query {
        id: "live_q2".into(),
        kind: "live".into(),
        text: "*.txt".into(),
        filter: None,
        comparability: "comparable".into(),
        expected_scope: "filename".into(),
    };

    let start2 = std::time::Instant::now();
    let res2 = adapter.query(&q2, 5, Duration::from_secs(5));
    let elapsed2 = start2.elapsed();

    println!("Query 2 (*.txt, top_n=5) Result:");
    println!(
        "  Latency: {:.2} ms (roundtrip: {:.2} ms)",
        res2.latency_ms,
        elapsed2.as_secs_f64() * 1000.0
    );
    println!("  Total Result Count: {}", res2.result_count);
    println!("  Top IDs Count: {}", res2.top_ids.len());
    println!("  Top IDs: {:?}", res2.top_ids);
    println!("  Error: {:?}", res2.error);
    println!("  Timed Out: {}", res2.timed_out);

    assert!(
        res2.error.is_none(),
        "Live query 2 failed with error: {:?}",
        res2.error
    );
    assert!(!res2.timed_out, "Live query 2 timed out");
    assert!(
        res2.result_count > 0,
        "Expected non-zero result count for *.txt"
    );
    assert!(res2.top_ids.len() <= 5, "Expected <= 5 top IDs");
    println!("=== LIVE EVERYTHING REAL INSTANCE TEST SUCCESS ===");
}
