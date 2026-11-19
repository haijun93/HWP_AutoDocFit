use crate::{sha256, AdapterResult, Query, QueryAdapter};
use std::{
    collections::HashMap,
    ffi::c_void,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex, Once,
    },
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{GetLastError, HWND, LPARAM, LRESULT, WPARAM},
    System::{DataExchange::COPYDATASTRUCT, LibraryLoader::GetModuleHandleW},
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowW,
        GetWindowThreadProcessId, PeekMessageW, RegisterClassW, SendMessageW, TranslateMessage,
        HWND_MESSAGE, MSG, PM_REMOVE, WM_COPYDATA, WM_USER, WNDCLASSW,
    },
};

// ============================================================================
// Everything IPC 프로토콜 상수 및 미확인 표기
// ============================================================================

/// [확인] Everything 기본 메시지 창 클래스명
pub const EVERYTHING_IPC_WNDCLASS: &str = "EVERYTHING";

/// [확인] Everything 새 버전/작업 표시줄 알림 메시지 창 클래스명
pub const EVERYTHING_IPC_WNDCLASS_TASKBAR: &str = "EVERYTHING_TASKBAR_NOTIFICATION";

/// [확인] dwData: EVERYTHING_IPC_COPYDATA_QUERY2W = 18 (UTF-16 와이드 쿼리 v2)
pub const EVERYTHING_IPC_COPYDATA_QUERY2W: usize = 18;

/// [확인] Everything 1.4+ DB 로드 상태 확인 WM_USER 메시지
pub const EVERYTHING_IPC_IS_DB_LOADED: usize = 401;

/// [확인] Everything 버전 확인 WM_USER 메시지 (0: 메이저, 1: 마이너, 2: 빌드, 3: 리비전)
pub const EVERYTHING_IPC_GET_MAJOR_VERSION: usize = 0;
pub const EVERYTHING_IPC_GET_MINOR_VERSION: usize = 1;
pub const EVERYTHING_IPC_GET_BUILD_NUMBER: usize = 2;
pub const EVERYTHING_IPC_GET_REVISION: usize = 3;

/// [확인] 회신 식별 메시지 ID 기본값 (임의 고유 번호)
pub const EVERYTHING_IPC_DEFAULT_REPLY_ID: u32 = 0x514D_5342; // "QMSB"

/// [확인] Everything IPC 요청 플래그 (request_flags)
pub const EVERYTHING_IPC_FLAG_NAME: u32 = 0x00000001;
pub const EVERYTHING_IPC_FLAG_PATH: u32 = 0x00000002;
pub const EVERYTHING_IPC_FLAG_FULL_PATH_AND_NAME: u32 = 0x00000004;

/// [확인] Everything IPC 정렬 플래그 (sort_type)
pub const EVERYTHING_IPC_SORT_NAME_ASCENDING: u32 = 1;

/// [미확인] Everything IPC search_flags 세부 비트 플래그 (0 = 기본 대소문자 미구분, regex 없음)
pub const EVERYTHING_IPC_SEARCH_FLAG_DEFAULT: u32 = 0; // [미확인] Everything 기본 검색 옵션 따름
pub const EVERYTHING_IPC_SEARCH_FLAG_MATCH_CASE: u32 = 0x00000001; // [미확인]
pub const EVERYTHING_IPC_SEARCH_FLAG_MATCH_PATH: u32 = 0x00000004; // [미확인]

/// [미확인] QUERY2W 요청 헤더의 확장 필드 사용 여부 (7 DWORD vs 5 DWORD)
/// Everything SDK 1.4+에서는 [reply_hwnd, reply_copydata_message, search_flags, offset, max_results, request_flags, sort_type] 7 DWORD를 권장함.
pub const EVERYTHING_IPC_USE_7_DWORD_HEADER: bool = true;

// ============================================================================
// 전역 회신 수신 맵
// ============================================================================

static INIT_WNDCLASS: Once = Once::new();
static NEXT_REPLY_ID: AtomicU32 = AtomicU32::new(EVERYTHING_IPC_DEFAULT_REPLY_ID);
static PENDING_REPLIES: Mutex<Option<HashMap<u32, Vec<u8>>>> = Mutex::new(None);

fn ensure_reply_map() {
    let mut guard = PENDING_REPLIES.lock().unwrap();
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
}

pub fn wide_str(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

unsafe extern "system" fn reply_wndproc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if m == WM_COPYDATA {
        let cds = unsafe { &*(l as *const COPYDATASTRUCT) };
        let reply_id = cds.dwData as u32;
        if cds.cbData > 0 && !cds.lpData.is_null() {
            let slice =
                unsafe { std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize) };
            let mut guard = PENDING_REPLIES.lock().unwrap();
            if let Some(map) = guard.as_mut() {
                map.insert(reply_id, slice.to_vec());
            }
            return 1;
        }
    }
    unsafe { DefWindowProcW(h, m, w, l) }
}

fn register_reply_window_class(class_name: &str) -> Result<(), String> {
    let mut res = Ok(());
    INIT_WNDCLASS.call_once(|| {
        let cname = wide_str(class_name);
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(reply_wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: unsafe { GetModuleHandleW(std::ptr::null()) },
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: cname.as_ptr(),
        };
        let atom = unsafe { RegisterClassW(&wc) };
        if atom == 0 {
            let err = unsafe { GetLastError() };
            // 1410 = ERROR_CLASS_ALREADY_EXISTS
            if err != 1410 {
                res = Err(format!("RegisterClassW failed: {err}"));
            }
        }
    });
    res
}

pub struct ReplyWindow {
    pub hwnd: HWND,
}

impl ReplyWindow {
    pub fn new() -> Result<Self, String> {
        let class_name = "QmsBenchReplyWindow";
        register_reply_window_class(class_name)?;
        ensure_reply_map();

        let cname = wide_str(class_name);
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                cname.as_ptr(),
                cname.as_ptr(),
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

        if hwnd.is_null() {
            return Err(format!(
                "CreateWindowExW failed for reply window: {}",
                unsafe { GetLastError() }
            ));
        }

        Ok(Self { hwnd })
    }
}

impl Drop for ReplyWindow {
    fn drop(&mut self) {
        if !self.hwnd.is_null() {
            unsafe {
                DestroyWindow(self.hwnd);
            }
        }
    }
}

// ============================================================================
// Everything 창 탐지 및 정보 확인
// ============================================================================

pub fn find_everything_window() -> Option<(HWND, u32)> {
    unsafe {
        let cls1 = wide_str(EVERYTHING_IPC_WNDCLASS);
        let mut hwnd = FindWindowW(cls1.as_ptr(), std::ptr::null());
        if hwnd.is_null() {
            let cls2 = wide_str(EVERYTHING_IPC_WNDCLASS_TASKBAR);
            hwnd = FindWindowW(cls2.as_ptr(), std::ptr::null());
        }
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        Some((hwnd, pid))
    }
}

pub fn get_everything_version(ev_hwnd: HWND) -> String {
    let get_part = |code: usize| unsafe { SendMessageW(ev_hwnd, WM_USER, code, 0) as u32 };
    format!(
        "{}.{}.{}.{}",
        get_part(EVERYTHING_IPC_GET_MAJOR_VERSION),
        get_part(EVERYTHING_IPC_GET_MINOR_VERSION),
        get_part(EVERYTHING_IPC_GET_BUILD_NUMBER),
        get_part(EVERYTHING_IPC_GET_REVISION)
    )
}

pub fn is_everything_db_loaded(ev_hwnd: HWND) -> bool {
    unsafe { SendMessageW(ev_hwnd, WM_USER, EVERYTHING_IPC_IS_DB_LOADED, 0) != 0 }
}

// ============================================================================
// EverythingIpcAdapter
// ============================================================================

pub struct EverythingIpcAdapter {
    pub name: String,
    pub target_hwnd: Option<HWND>,
    reply_window: ReplyWindow,
}

impl EverythingIpcAdapter {
    pub fn new() -> Result<Self, String> {
        let reply_window = ReplyWindow::new()?;
        let target_hwnd = find_everything_window().map(|(h, _)| h);
        Ok(Self {
            name: "everything".into(),
            target_hwnd,
            reply_window,
        })
    }

    pub fn with_target(target_hwnd: HWND) -> Result<Self, String> {
        let reply_window = ReplyWindow::new()?;
        Ok(Self {
            name: "everything".into(),
            target_hwnd: Some(target_hwnd),
            reply_window,
        })
    }
}

impl QueryAdapter for EverythingIpcAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn query(&mut self, query: &Query, top_n: usize, time_limit: Duration) -> AdapterResult {
        let started = Instant::now();

        let target_hwnd = match self.target_hwnd {
            Some(h) => h,
            None => match find_everything_window() {
                Some((h, _)) => {
                    self.target_hwnd = Some(h);
                    h
                }
                None => {
                    return AdapterResult {
                        latency_ms: started.elapsed().as_secs_f64() * 1000.0,
                        result_count: 0,
                        top_ids: vec![],
                        error: Some("Everything window not found (not running)".into()),
                        timed_out: false,
                    };
                }
            },
        };

        let reply_id = NEXT_REPLY_ID.fetch_add(1, Ordering::Relaxed);
        let req_bytes = build_query2w_request(
            self.reply_window.hwnd,
            reply_id,
            &query.text,
            top_n as u32,
            EVERYTHING_IPC_FLAG_NAME | EVERYTHING_IPC_FLAG_PATH,
            EVERYTHING_IPC_SORT_NAME_ASCENDING,
        );

        let cds = COPYDATASTRUCT {
            dwData: EVERYTHING_IPC_COPYDATA_QUERY2W,
            cbData: req_bytes.len() as u32,
            lpData: req_bytes.as_ptr() as *mut c_void,
        };

        // 기존 대기 중인 회신 제거
        {
            let mut guard = PENDING_REPLIES.lock().unwrap();
            if let Some(map) = guard.as_mut() {
                map.remove(&reply_id);
            }
        }

        let send_res = unsafe {
            SendMessageW(
                target_hwnd,
                WM_COPYDATA,
                self.reply_window.hwnd as usize,
                &cds as *const _ as isize,
            )
        };

        if send_res == 0 {
            // 메시지 전송 실패 (상대 창 닫힘 또는 미응답)
            return AdapterResult {
                latency_ms: started.elapsed().as_secs_f64() * 1000.0,
                result_count: 0,
                top_ids: vec![],
                error: Some("SendMessageW(WM_COPYDATA) returned 0 or failed".into()),
                timed_out: false,
            };
        }

        // 메시지 루프로 응답 대기
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        loop {
            while unsafe { PeekMessageW(&mut msg, self.reply_window.hwnd, 0, 0, PM_REMOVE) } != 0 {
                unsafe {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }

            {
                let mut guard = PENDING_REPLIES.lock().unwrap();
                if let Some(map) = guard.as_mut() {
                    if let Some(resp_bytes) = map.remove(&reply_id) {
                        let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
                        match parse_everything_list2(&resp_bytes, top_n) {
                            Ok((total, top_ids)) => {
                                return AdapterResult {
                                    latency_ms,
                                    result_count: total,
                                    top_ids,
                                    error: None,
                                    timed_out: false,
                                };
                            }
                            Err(e) => {
                                return AdapterResult {
                                    latency_ms,
                                    result_count: 0,
                                    top_ids: vec![],
                                    error: Some(e),
                                    timed_out: false,
                                };
                            }
                        }
                    }
                }
            }

            if started.elapsed() >= time_limit {
                return AdapterResult {
                    latency_ms: started.elapsed().as_secs_f64() * 1000.0,
                    result_count: 0,
                    top_ids: vec![],
                    error: Some("Everything IPC query timed out".into()),
                    timed_out: true,
                };
            }

            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

// ============================================================================
// 요청 생성 및 응답 파싱 유틸리티
// ============================================================================

/// EVERYTHING_IPC_QUERY2W 버퍼 생성
pub fn build_query2w_request(
    reply_hwnd: HWND,
    reply_id: u32,
    query_text: &str,
    max_results: u32,
    request_flags: u32,
    sort_type: u32,
) -> Vec<u8> {
    let mut b = Vec::new();
    let fields = [
        reply_hwnd as usize as u32,
        reply_id,
        EVERYTHING_IPC_SEARCH_FLAG_DEFAULT,
        0u32, // offset
        max_results,
        request_flags,
        sort_type,
    ];
    for v in fields {
        b.extend_from_slice(&v.to_le_bytes());
    }
    for c in query_text.encode_utf16().chain([0]) {
        b.extend_from_slice(&c.to_le_bytes());
    }
    b
}

/// EVERYTHING_IPC_LIST2 응답 버퍼 해석
pub fn parse_everything_list2(bytes: &[u8], top_n: usize) -> Result<(u64, Vec<String>), String> {
    if bytes.len() < 20 {
        return Err(format!(
            "EVERYTHING_IPC_LIST2 header too short ({} < 20)",
            bytes.len()
        ));
    }

    let totitems = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as u64;
    let numitems = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let _offset = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    let _request_flags = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    let _sort_type = u32::from_le_bytes(bytes[16..20].try_into().unwrap());

    let mut top_ids = Vec::new();
    let take_count = numitems.min(top_n);

    // 각 아이템은 [flags: u32, data_offset: u32] (8바이트)
    let item_header_base = 20;
    for i in 0..take_count {
        let item_start = item_header_base + i * 8;
        if item_start + 8 > bytes.len() {
            break;
        }
        let _flags = u32::from_le_bytes(bytes[item_start..item_start + 4].try_into().unwrap());
        let data_offset =
            u32::from_le_bytes(bytes[item_start + 4..item_start + 8].try_into().unwrap()) as usize;

        if data_offset < bytes.len() {
            // data_offset 위치부터 UTF-16 NUL-terminated 문자열 읽기
            let mut u16_chars = Vec::new();
            let mut curr = data_offset;
            while curr + 1 < bytes.len() {
                let code = u16::from_le_bytes([bytes[curr], bytes[curr + 1]]);
                if code == 0 {
                    break;
                }
                u16_chars.push(code);
                curr += 2;
            }
            let item_str = String::from_utf16_lossy(&u16_chars);
            top_ids.push(sha256(item_str.as_bytes()));
        }
    }

    Ok((totitems, top_ids))
}
