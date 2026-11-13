//! 가상 관계형 자료함 엔진 `klib.exe` 진입점.
//! 상주하지 않으며 요청 후 60초 유휴 시 자동 종료.

#![windows_subsystem = "windows"]

mod db;
mod json;
mod pipe;
mod util;
mod search;
mod content;
mod reindex;
mod notebook;
mod nb_store;
mod nb_split_check;
mod nb_transform;
mod scan;
mod extract;
mod index_content;
mod unify_marker;
mod classify_levels;
mod legacy_records;
mod rename;
mod tm_scan;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError};
use windows_sys::Win32::System::Threading::CreateMutexW;

use crate::db::Database;
use crate::util::{data_dir, now_ms, wide};

static SEARCH_INDEX: std::sync::OnceLock<Result<search::SearchIndex, String>> = std::sync::OnceLock::new();

pub(crate) fn search_index() -> Result<&'static search::SearchIndex, String> {
    SEARCH_INDEX.get_or_init(|| {
        let dir = std::env::var("KLIB_TANTIVY_DIR").unwrap_or_else(|_| format!("{}\\tantivy", data_dir()));
        search::SearchIndex::open_or_create(dir).map_err(|e| e.to_string())
    }).as_ref().map_err(Clone::clone)
}

fn send_pipe_cmd(cmd_json: &str) -> Option<String> {
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, ReadFile, WriteFile, FILE_GENERIC_READ, FILE_GENERIC_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;

    let pipe_path = wide(&pipe::pipe_name());
    let handle = unsafe {
        CreateFileW(
            pipe_path.as_ptr(),
            FILE_GENERIC_READ | FILE_GENERIC_WRITE,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut data = cmd_json.as_bytes().to_vec();
    data.push(b'\n');
    let mut written = 0u32;
    unsafe { WriteFile(handle, data.as_ptr(), data.len() as u32, &mut written, std::ptr::null_mut()) };

    let mut buf = [0u8; 4096];
    let mut read_bytes = 0u32;
    let ok = unsafe { ReadFile(handle, buf.as_mut_ptr(), buf.len() as u32, &mut read_bytes, std::ptr::null_mut()) };
    unsafe { CloseHandle(handle) };
    if ok != 0 && read_bytes > 0 {
        String::from_utf8(buf[..read_bytes as usize].to_vec()).ok()
    } else {
        None
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "tm-scan") {
        let Some(root) = args.get(1) else { eprintln!("usage: klib tm-scan <root> [request-json]"); std::process::exit(2); };
        let request = args.get(2).and_then(|s| crate::json::parse(s)).unwrap_or_else(|| crate::json::Value::obj(vec![]));
        match tm_scan::run(root, &request) { Ok(value) => println!("{}", crate::json::to_string(&value)), Err(e) => { eprintln!("{e}"); std::process::exit(1); } }
        return;
    }
    let has = |flag: &str| args.iter().any(|a| a == flag);

    if args.first().is_some_and(|a| a == "nb-split-compare") {
        // 업무자료함 색인 분리 비교(사본 폴더 전용): 옮기기 전후 표·검색 결과 차이.
        let Some(dir) = args.get(1) else { eprintln!("usage: klib nb-split-compare <library-folder-copy>"); std::process::exit(2); };
        match nb_split_check::run(std::path::Path::new(dir)) { Ok(v) => println!("{}", json::to_string(&v)), Err(e) => { eprintln!("{e}"); std::process::exit(1); } }
        return;
    }

    if args.first().is_some_and(|a| a == "legacy-records") {
        let Some(path) = args.get(1) else { eprintln!("legacy-records requires a path"); std::process::exit(2); };
        match legacy_records::run(std::path::Path::new(path)) { Ok(rows) => println!("{}", json::to_string(&json::Value::Arr(rows))), Err(e) => { eprintln!("{e}"); std::process::exit(1); } }
        return;
    }

    if args.first().is_some_and(|a| a == "unify-marker") {
        use std::io::{self, Read};
        let mut input = String::new();
        if let Err(error) = io::stdin().read_to_string(&mut input) { eprintln!("{error}"); std::process::exit(1); }
        let texts: Vec<String> = match json::parse(input.trim()) {
            Some(json::Value::Arr(items)) => items.into_iter().filter_map(|v| match v { json::Value::Str(s) => Some(s), _ => None }).collect(),
            _ => input.lines().map(str::to_owned).collect(),
        };
        let values: Vec<_> = texts.iter().map(|text| unify_marker::unify_marker(text)).map(|(group, role, marker)|
            json::Value::Arr(vec![group.into(), role.into(), marker.into()])).collect();
        println!("{}", json::to_string(&json::Value::Arr(values)));
        return;
    }

    if args.first().is_some_and(|a| a == "classify-levels") {
        use std::io::{self, Read};
        let stage = args.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(7).clamp(0, 7);
        let mut input = String::new();
        if let Err(error) = io::stdin().read_to_string(&mut input) { eprintln!("{error}"); std::process::exit(1); }
        if let Err(error) = classify_levels::run(&input, stage) { eprintln!("{error}"); std::process::exit(1); }
        return;
    }

    if args.first().is_some_and(|a| a == "index-content") {
        if args.len() < 3 { eprintln!("usage: klib index-content <db> <file-or-folder>..."); std::process::exit(2); }
        if let Err(error) = index_content::run(&args[1], &args[2..]) { eprintln!("{error}"); std::process::exit(1); }
        return;
    }
    if args.first().is_some_and(|a| a == "extract") {
        let Some(path) = args.get(1) else { eprintln!("extract requires a path"); std::process::exit(2); };
        match extract::extract(std::path::Path::new(path)) {
            Ok(paragraphs) => {
                let records = paragraphs.iter().map(extract::Paragraph::to_json).collect();
                println!("{}", json::to_string(&json::Value::obj(vec![("paragraphs", json::Value::Arr(records))])));
            }
            Err(error) => { eprintln!("{error}"); std::process::exit(1); }
        }
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--rename-preview-json") {
        if let Some(request) = args.get(pos + 1) {
            let request = if let Some(path) = request.strip_prefix('@') {
                std::fs::read_to_string(path).unwrap_or_else(|_| std::process::exit(2))
            } else { request.clone() };
            let Some(value) = json::parse(&request) else { std::process::exit(2) };
            println!("{}", json::to_string(&rename::preview(&value)));
            return;
        }
        std::process::exit(2);
    }
    let has = |flag: &str| args.iter().any(|a| a == flag);

    if let Some(pos) = args.iter().position(|a| a == "--request-json") {
        if let Some(request) = args.get(pos + 1) {
            let dir = std::env::var("KLIB_DATA").unwrap_or_else(|_| data_dir());
            let _ = std::fs::create_dir_all(&dir);
            let db = Database::open(&std::path::Path::new(&dir).join("library.db").to_string_lossy()).expect("open library database");
            let running = AtomicBool::new(true);
            let db = Mutex::new(db);
            let reply = pipe::handle_request(request, &db, &running);
            // 폴더 스캔은 백그라운드 스레드라, 한 번 실행 방식에서는 끝날 때까지 기다린 뒤 최종 상태를 출력한다.
            let is_scan = request.contains("\"box_scan\"");
            if is_scan && reply.contains("\"started\":true") {
                while scan::is_running() { std::thread::sleep(std::time::Duration::from_millis(10)); }
                println!("{}", pipe::handle_request(r#"{"cmd":"box_scan_status"}"#, &db, &running));
                reindex::wait();
                return;
            }
            println!("{reply}");
            // 색인을 다시 만드는 중이면 끝까지 기다린다(여기서 끝내면 다음 실행이 처음부터 다시 해야 한다).
            reindex::wait();
            return;
        }
        eprintln!("--request-json requires one JSON request argument");
        std::process::exit(2);
    }

    let mutex_name = format!("Local\\HwpAutoDocFit.klib{}", util::instance_suffix().unwrap_or_default());
    let instance_mutex = unsafe { CreateMutexW(std::ptr::null(), 0, wide(&mutex_name).as_ptr()) };
    if instance_mutex.is_null() {
        std::process::exit(1);
    }

    unsafe {
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(instance_mutex);
            if has("--quit") {
                let _ = send_pipe_cmd(r#"{"cmd":"quit"}"#);
            }
            return;
        }
        if has("--quit") {
            CloseHandle(instance_mutex);
            return;
        }
    }

    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let db_path = format!("{dir}\\library.db");
    let db = match Database::open(&db_path) {
        Ok(d) => Arc::new(Mutex::new(d)),
        Err(_) => {
            unsafe { CloseHandle(instance_mutex) };
            std::process::exit(2);
        }
    };

    let running = Arc::new(AtomicBool::new(true));
    let last_activity = Arc::new(AtomicU64::new(now_ms()));

    // 유휴 자동 종료 제한 시간(초) - 기본 60초, KLIB_IDLE_SECS 환경변수 지원
    let idle_limit_secs = std::env::var("KLIB_IDLE_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(60);

    // Watchdog 스레드: 60초(또는 지정값) 유휴 시 running을 false로 설정하여 정상 정리 종료 유도
    let r_clone = running.clone();
    let act_clone = last_activity.clone();
    std::thread::Builder::new()
        .name("klib-watchdog".into())
        .spawn(move || {
            while r_clone.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(500));
                if scan::is_running() || reindex::is_running() { act_clone.store(now_ms(), Ordering::Relaxed); } // 스캔·색인 다시 만들기 중에는 유휴 종료하지 않음
                let elapsed = (now_ms().saturating_sub(act_clone.load(Ordering::Relaxed))) / 1000;
                if elapsed >= idle_limit_secs {
                    r_clone.store(false, Ordering::Relaxed);
                    // 서버 루프가 대기에서 깨어나 정리 후 종료될 수 있도록 여유 대기 후 안전 종료
                    std::thread::sleep(Duration::from_millis(1000));
                    std::process::exit(0);
                }
            }
        })
        .ok();

    // Pipe 서버 구동 (메인 스레드 대기)
    pipe::serve(db, running, last_activity);

    unsafe {
        CloseHandle(instance_mutex);
    }
}
