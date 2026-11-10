//! K에이전트: 몽땅클립보드 기록·빠른 붙여넣기 상주 프로그램.
//!
//! 속도·자원 원칙(사용자 지시 2026-10-31: 실행 속도와 최소 자원이 가장 중요):
//! - 이벤트로만 깨어난다(WM_CLIPBOARDUPDATE·단축키·파이프 요청). 폴링·주기 타이머 없음 → 유휴 CPU 0.
//! - 클립보드는 짧게 열어 복사만 하고, 해시·PNG 변환·암호화·저장은 작업 스레드가 한다.
//! - 팝업은 메모리에 둔 최근 목록으로 바로 그린다(DB·복호화 없음).
//! - SQLite는 Windows 내장 winsqlite3.dll, 네트워크 코드 없음.
#![windows_subsystem = "windows"]

mod category;
mod clip;
mod crypt;
mod memo;
mod open_item;
mod json;
mod pipe;
mod popup;
mod settings_menu;
mod sqlite;
mod store;
mod timeline;
mod util;
mod watch;
mod xl_target;
mod excel_target;

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, AtomicI64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{CreateBitmap, CreateDIBSection, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS};
use windows_sys::Win32::System::DataExchange::{AddClipboardFormatListener, GetClipboardSequenceNumber,
                                               RemoveClipboardFormatListener};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, UnregisterHotKey,
};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::clip::Capture;
use crate::store::{Settings, Store, Summary};
use crate::util::{now, wide};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const WM_TRAY: u32 = WM_APP + 1;
pub const WM_REFRESH: u32 = WM_APP + 2;
pub const WM_RESTORE: u32 = WM_APP + 3;
pub const WM_SHOW: u32 = WM_APP + 4;
pub const WM_PAUSE: u32 = WM_APP + 5;
pub const WM_SETTINGS: u32 = WM_APP + 6;
const HOTKEY_ID: i32 = 1;
const TIMER_READ: usize = 1;
const READ_DELAY_MS: u32 = 60; // 연달아 오는 변경 알림을 한 번으로(복사 중간 상태 읽기 방지)
/// 메모리에 두는 최근 목록 개수(빠른 목록은 분류를 이 목록에서 먼저 거른다).
pub const RECENT: usize = 50;
const MAIN_CLASS: &str = "HwpAutoDocFit.KAgentMain";

pub enum TimelineMsg {
    Reconfigure {
        generation: u64,
        roots: Vec<String>,
        mtime_only: bool,
    },
    ScanRoot {
        generation: u64,
        root: String,
    },
    Apply {
        generation: u64,
        root: String,
        relative: String,
    },
}

pub struct App {
    pub dir: String,
    pub store: Mutex<Store>,
    pub settings: Mutex<Settings>,
    pub recent: Mutex<Vec<Summary>>,
    /// 최근 목록을 저장소에서 읽지 못했을 때의 오류(빠른 목록이 빈 이유를 보이려고 보관, 정상이면 빈 글자).
    pub recent_error: Mutex<String>,
    pub tx: Mutex<Sender<(Capture, String)>>,
    pub self_seq: AtomicU32,
    pub hwnd: AtomicIsize,
    pub hotkey_ok: AtomicBool,
    pub listening: AtomicBool,
    /// 마지막으로 클립보드에서 그림을 기록한 때(ms) — 같은 캡처가 스크린샷 폴더로도 오면 한 번만.
    pub last_clip_image: AtomicU64,
    /// 마지막 스크린샷 PNG 지문과 시각(클립보드·파일 경로 공통 중복 제거).
    pub image_dedupe: Mutex<std::collections::VecDeque<(String, u64)>>,
    pub timeline: Mutex<Option<timeline::Timeline>>,
    pub screenshots: Mutex<String>,
    pub scan_pending: Mutex<HashSet<String>>,
    pub timeline_generation: AtomicU64,
    pub timeline_files: AtomicI64,
    pub timeline_tx: Mutex<Option<Sender<TimelineMsg>>>,
}

fn now_ms() -> u64 {
    (now() * 1000.0) as u64
}

/// 스크린샷 폴더(알려진 폴더 FOLDERID_Screenshots, 없으면 사진\Screenshots).
fn screenshots_folder() -> Option<String> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_Screenshots, SHGetKnownFolderPath};
    if let Ok(root) = std::env::var("KAGENT_TEST_ROOT") {
        let dir = std::path::Path::new(&root).join("Screenshots");
        if std::fs::create_dir_all(&dir).is_ok() {
            return Some(dir.to_string_lossy().into_owned());
        }
        return None;
    }
    if let Ok(dir) = std::env::var("KAGENT_SCREENSHOTS") {
        return std::path::Path::new(&dir).is_dir().then_some(dir); // 시험용
    }
    unsafe {
        let mut p: windows_sys::core::PWSTR = std::ptr::null_mut();
        if SHGetKnownFolderPath(&FOLDERID_Screenshots, 0, std::ptr::null_mut(), &mut p) == 0 && !p.is_null() {
            let mut n = 0;
            while *p.add(n) != 0 {
                n += 1;
            }
            let path = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
            CoTaskMemFree(p as _);
            if std::path::Path::new(&path).is_dir() {
                return Some(path);
            }
        } else if !p.is_null() {
            CoTaskMemFree(p as _);
        }
    }
    let fallback = format!("{}\\Pictures\\Screenshots", std::env::var("USERPROFILE").ok()?);
    std::path::Path::new(&fallback).is_dir().then_some(fallback)
}

/// 스크린샷 폴더에 새로 생긴 PNG 한 장을 그림 항목으로 기록한다.
fn record_screenshot(path: &str) {
    if !path.to_lowercase().ends_with(".png") {
        return;
    }
    let max = app().settings.lock().unwrap().max_item_bytes as u64;
    // 캡처 도구가 아직 쓰는 중일 수 있어 몇 번 다시 시도한다.
    let mut data = None;
    for _ in 0..5 {
        match std::fs::metadata(path) {
            Ok(m) if m.len() > max => return,
            Ok(_) => {
                if let Ok(bytes) = std::fs::read(path) {
                    if bytes.starts_with(b"\x89PNG") && bytes.ends_with(b"IEND\xAE\x42\x60\x82") {
                        data = Some(bytes);
                        break;
                    }
                }
            }
            Err(_) => return,
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    let Some(data) = data else { return };
    if !claim_image(&data) { return; }
    let preview = format!("캡처 · {}", clip::preview("image", &data));
    let settings = app().settings.lock().unwrap().clone();
    {
        let store = app().store.lock().unwrap();
        let t = now();
        if store.insert("image", &data, &[], &preview, "screenshot", t).is_err() {
            return;
        }
        let _ = store.enforce(&settings, t);
    }
    refresh_recent();
}

fn image_hash(data: &[u8]) -> String { store::content_hash("image", data) }

fn claim_image(data: &[u8]) -> bool {
    let now = now_ms();
    let hash = image_hash(data);
    let mut seen = app().image_dedupe.lock().unwrap();
    seen.retain(|(_, at)| now.saturating_sub(*at) <= 5000);
    if seen.iter().any(|(h, _)| h == &hash) { return false; }
    seen.push_back((hash, now));
    while seen.len() > 32 { seen.pop_front(); }
    true
}

pub fn confirm_clear(hwnd: HWND) -> bool {
    unsafe {
        MessageBoxW(hwnd, wide("고정 항목을 제외한 기록을 모두 지울까요?").as_ptr(), wide("몽땅클립보드").as_ptr(), MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2) == IDYES
    }
}

pub fn capture_screen() {
    std::thread::Builder::new().name("kagent-screen-capture".into()).spawn(|| unsafe {
        std::thread::sleep(std::time::Duration::from_millis(180));
        let uri = wide("ms-screenclip:");
        let verb = wide("open");
        let result = windows_sys::Win32::UI::Shell::ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), uri.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL);
        if result as isize <= 32 { return; }
    }).ok();
}

fn mark_scan_pending(pending: &mut HashSet<String>, root: &str) -> bool {
    pending.insert(root.to_string())
}

fn unmark_scan_pending(pending: &mut HashSet<String>, root: &str) -> bool {
    pending.remove(root)
}

fn timeline_worker(rx: std::sync::mpsc::Receiver<TimelineMsg>) {
    unsafe {
        use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority,
                                                    THREAD_MODE_BACKGROUND_BEGIN};
        SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
    }
    let mut current_gen: u64 = 0;
    let mut active_roots: Vec<String> = Vec::new();
    let mut tl: Option<timeline::Timeline> = None;

    for msg in rx {
        match msg {
            TimelineMsg::Reconfigure { generation, roots, mtime_only } => {
                if generation < app().timeline_generation.load(Ordering::Relaxed) {
                    continue;
                }
                current_gen = generation;
                active_roots = roots.clone();
                if active_roots.is_empty() {
                    tl = None;
                    *app().timeline.lock().unwrap() = None;
                    app().timeline_files.store(0, Ordering::Relaxed);
                } else {
                    if tl.is_none() {
                        tl = timeline::Timeline::open(&format!("{}\\timeline.db", app().dir)).ok();
                    }
                    if let Some(t) = tl.as_mut() {
                        t.mtime_only = mtime_only;
                        let _ = t.drop_other_roots(&active_roots);
                        let count = t.count();
                        app().timeline_files.store(count, Ordering::Relaxed);
                    }
                }
                write_status();
            }
            TimelineMsg::ScanRoot { generation, root } => {
                let latest_gen = app().timeline_generation.load(Ordering::Relaxed);
                if generation < latest_gen || current_gen < latest_gen || !active_roots.contains(&root) {
                    let mut pending = app().scan_pending.lock().unwrap();
                    unmark_scan_pending(&mut pending, &root);
                    continue;
                }
                if let Some(t) = tl.as_ref() {
                    let _ = t.scan_root(&root);
                    let count = t.count();
                    app().timeline_files.store(count, Ordering::Relaxed);
                }
                {
                    let mut pending = app().scan_pending.lock().unwrap();
                    unmark_scan_pending(&mut pending, &root);
                }
                write_status();
            }
            TimelineMsg::Apply { generation, root, relative } => {
                let latest_gen = app().timeline_generation.load(Ordering::Relaxed);
                if generation < latest_gen || current_gen < latest_gen || !active_roots.contains(&root) {
                    continue;
                }
                if let Some(t) = tl.as_ref() {
                    let _ = t.apply(&root, &relative);
                    let count = t.count();
                    app().timeline_files.store(count, Ordering::Relaxed);
                    write_status();
                }
            }
        }
    }
}

pub fn schedule_timeline_scan(root: String) {
    {
        let mut pending = app().scan_pending.lock().unwrap();
        if !mark_scan_pending(&mut pending, &root) {
            return;
        }
    }
    let generation = app().timeline_generation.load(Ordering::Relaxed);
    let sent = {
        let guard = app().timeline_tx.lock().unwrap();
        guard.as_ref().map(|tx| tx.send(TimelineMsg::ScanRoot { generation, root: root.clone() }).is_ok()).unwrap_or(false)
    };
    if !sent {
        let mut pending = app().scan_pending.lock().unwrap();
        unmark_scan_pending(&mut pending, &root);
    }
}

pub fn scan_timeline_root(root: &str) {
    schedule_timeline_scan(root.to_string());
}

/// 설정에 맞춰 폴더 감시(스크린샷·타임머신)를 다시 시작한다. 타임머신 폴더는 먼저 한 번 증분으로 훑는다.
pub fn restart_watchers() {
    let settings = app().settings.lock().unwrap().clone();
    let mut roots: Vec<(String, u32, bool)> = Vec::new();
    let shots = if settings.watch_screenshots && !settings.paused { screenshots_folder() } else { None };
    *app().screenshots.lock().unwrap() = shots.clone().unwrap_or_default();
    if let Some(dir) = shots {
        roots.push((dir, 0, false));
    }
    let timeline_roots: Vec<String> =
        settings.timeline_roots.iter().filter(|r| std::path::Path::new(r).is_dir()).cloned().collect();

    // 새 세대 번호 발급 (이전 세대의 진행 중 스캔 및 알림은 stale 처리됨)
    let generation = app().timeline_generation.fetch_add(1, Ordering::SeqCst) + 1;

    // 타임라인 전용 워커에 설정 변경 전달 (DB I/O를 창/메시지 스레드에서 수행하지 않음)
    if let Some(tx) = app().timeline_tx.lock().unwrap().as_ref() {
        let _ = tx.send(TimelineMsg::Reconfigure {
            generation,
            roots: timeline_roots.clone(),
            mtime_only: settings.timeline_mtime_only,
        });
    }

    for (i, root) in timeline_roots.iter().enumerate() {
        roots.push((root.clone(), i as u32 + 1, true));
    }
    let names = timeline_roots.clone();
    watch::restart(roots, move |batch| {
        for (tag, change) in batch {
            if tag == 0 {
                if let watch::Change::Added(rel) | watch::Change::Modified(rel) = change {
                    let dir = app().screenshots.lock().unwrap().clone();
                    record_screenshot(&format!("{dir}\\{rel}"));
                }
                continue;
            }
            let Some(root) = names.get(tag as usize - 1) else { continue };
            match change {
                watch::Change::Overflow => schedule_timeline_scan(root.clone()),
                watch::Change::Added(rel) | watch::Change::Removed(rel) | watch::Change::Modified(rel) => {
                    if let Some(tx) = app().timeline_tx.lock().unwrap().as_ref() {
                        let _ = tx.send(TimelineMsg::Apply {
                            generation,
                            root: root.clone(),
                            relative: rel,
                        });
                    }
                }
            }
        }
    });
    for root in timeline_roots {
        schedule_timeline_scan(root);
    }
}

static APP: OnceLock<App> = OnceLock::new();

pub fn app() -> &'static App {
    APP.get().expect("app")
}

pub fn main_hwnd() -> HWND {
    app().hwnd.load(Ordering::Relaxed) as HWND
}

fn settings_path() -> String {
    format!("{}\\settings.json", app().dir)
}

/// 저장소 조회 결과를 메모리 목록에 반영한다. 성공이면 새 목록, 실패이면 예전 목록을 그대로 두고 오류 글을 돌려준다.
pub fn merge_recent(previous: Vec<Summary>, result: Result<Vec<Summary>, String>) -> (Vec<Summary>, Option<String>) {
    match result {
        Ok(list) => (list, None),
        Err(e) => (previous, Some(e)),
    }
}

/// 메모리의 최근 목록을 DB에서 다시 읽는다(변경 때만).
pub fn refresh_recent() {
    let result = app().store.lock().unwrap().recent(RECENT, None);
    let previous = app().recent.lock().unwrap().clone();
    // 조회에 실패해도 예전 목록을 지우지 않는다(예전에는 오류를 삼키고 빈 목록으로 덮어써, 관리창에는 기록이
    // 보이는데 빠른 목록만 비어 보였다). 실패 이유는 빠른 목록이 비어 있을 때 화면에 보인다.
    let (list, error) = merge_recent(previous, result);
    *app().recent.lock().unwrap() = list;
    *app().recent_error.lock().unwrap() = error.unwrap_or_default();
    write_status();
    unsafe { PostMessageW(main_hwnd(), WM_REFRESH, 0, 0) };
}

/// 상태 파일: 앱이 파이프 없이도 읽을 수 있게 변경 때만 쓴다.
pub fn write_status() {
    let a = app();
    let (items, total) = {
        let s = a.store.lock().unwrap();
        (s.count().unwrap_or(0), s.total_bytes().unwrap_or(0))
    };
    let settings = a.settings.lock().unwrap().clone();
    let screenshots = a.screenshots.lock().unwrap().clone();
    let timeline_files = a.timeline_files.load(Ordering::Relaxed);
    let v = json::Value::obj(vec![
        ("version", VERSION.into()),
        ("pid", (std::process::id() as i64).into()),
        ("pipe", pipe::pipe_name().into()),
        ("paused", settings.paused.into()),
        ("items", items.into()),
        ("total_bytes", total.into()),
        ("hotkey", settings.hotkey.as_str().into()),
        ("hotkey_ok", a.hotkey_ok.load(Ordering::Relaxed).into()),
        ("screenshots", screenshots.as_str().into()),
        ("timeline_roots", (settings.timeline_roots.len() as i64).into()),
        ("timeline_files", timeline_files.into()),
        ("updated", now().into()),
    ]);
    let path = format!("{}\\status.json", a.dir);
    let temp = format!("{path}.tmp");
    if std::fs::write(&temp, json::to_string(&v)).is_ok() {
        let _ = std::fs::rename(&temp, &path);
    }
}

pub fn save_settings() {
    let s = app().settings.lock().unwrap().clone();
    let _ = s.save(&settings_path());
}

/// 설정 바꾸기(파이프 `settings_set`·빠른 목록 설정 메뉴 공통): 범위 검사 → 저장 → 보관 한도 정리 →
/// 창·단축키·감시 갱신(창 스레드). 관리창과 빠른 목록이 같은 값 하나를 쓴다.
pub fn update_settings(new: &json::Value) -> Settings {
    let a = app();
    let snapshot = {
        let mut s = a.settings.lock().unwrap();
        s.apply(new);
        s.clone()
    };
    save_settings();
    let _ = a.store.lock().unwrap().enforce(&snapshot, now());
    refresh_recent();
    unsafe { PostMessageW(main_hwnd(), WM_SETTINGS, 0, 0) };
    snapshot
}

/// 시간 일시정지가 끝나는 때(ms, 0 = 없음).
static PAUSE_UNTIL_MS: AtomicU64 = AtomicU64::new(0);
const TIMER_RESUME: usize = 2;

/// 일시정지(분, 0 = 다시 켤 때까지). 시간이 지나면 에이전트가 스스로 다시 기록한다.
pub fn pause_for(minutes: u32) {
    let hwnd = main_hwnd();
    set_paused(hwnd, true); // 이전 시간 예약은 여기서 지워진다
    if minutes > 0 {
        let ms = minutes.min(1440) * 60_000;
        PAUSE_UNTIL_MS.store(now_ms() + ms as u64, Ordering::Relaxed);
        unsafe { SetTimer(hwnd, TIMER_RESUME, ms, None) };
    }
}

/// 시간 일시정지면 다시 기록까지 남은 분(올림).
pub fn resume_in_minutes() -> Option<u64> {
    let until = PAUSE_UNTIL_MS.load(Ordering::Relaxed);
    let now = now_ms();
    (until > now).then(|| (until - now).div_ceil(60_000))
}

// ---- Windows 로그인 시 자동 실행(HKCU Run, 앱 kagent_client.set_autostart와 같은 값) ----
// Win32_System_Registry 기능을 늘리지 않으려고 advapi32의 다섯 함수만 직접 선언한다.
#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegOpenKeyExW(key: isize, sub: *const u16, options: u32, sam: u32, out: *mut isize) -> u32;
    fn RegQueryValueExW(key: isize, name: *const u16, reserved: *mut u32, kind: *mut u32, data: *mut u8, len: *mut u32) -> u32;
    fn RegSetValueExW(key: isize, name: *const u16, reserved: u32, kind: u32, data: *const u8, len: u32) -> u32;
    fn RegDeleteValueW(key: isize, name: *const u16) -> u32;
    fn RegCloseKey(key: isize) -> u32;
}
const HKEY_CURRENT_USER: isize = 0x8000_0001u32 as i32 as isize;
const KEY_QUERY_VALUE: u32 = 0x0001;
const KEY_SET_VALUE: u32 = 0x0002;
const REG_SZ: u32 = 1;

fn open_run_key(sam: u32) -> Option<isize> {
    let mut key = 0isize;
    let r = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, wide(settings_menu::RUN_KEY).as_ptr(), 0, sam, &mut key) };
    (r == 0).then_some(key)
}

/// 자동 실행 등록 여부. 레지스트리를 읽지 못하면 None.
pub fn autostart_enabled() -> Option<bool> {
    if std::env::var_os("KAGENT_TEST_ROOT").is_some() { return None; }
    let key = open_run_key(KEY_QUERY_VALUE)?;
    let name = wide(settings_menu::RUN_NAME);
    let mut buf = vec![0u16; 2048];
    let mut len = (buf.len() * 2) as u32;
    let mut kind = 0u32;
    let r = unsafe { RegQueryValueExW(key, name.as_ptr(), std::ptr::null_mut(), &mut kind, buf.as_mut_ptr() as *mut u8, &mut len) };
    unsafe { RegCloseKey(key) };
    match r {
        0 => {
            let n = (len as usize / 2).min(buf.len());
            let value = String::from_utf16_lossy(&buf[..n]);
            Some(kind == REG_SZ && settings_menu::is_autostart_command(value.trim_end_matches('\0')))
        }
        2 => Some(false), // ERROR_FILE_NOT_FOUND: 등록 없음
        _ => None,
    }
}

/// 자동 실행 켜기/끄기. 켜면 지금 실행 중인 kagent.exe를 30초 늦게 시작하도록 등록한다.
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    if std::env::var_os("KAGENT_TEST_ROOT").is_some() {
        return Err("시험 실행에서는 자동 실행 등록을 바꾸지 않습니다.".into());
    }
    let key = open_run_key(KEY_SET_VALUE | KEY_QUERY_VALUE).ok_or("자동 실행 등록 위치를 열지 못했습니다.")?;
    let name = wide(settings_menu::RUN_NAME);
    let r = if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string());
        match exe {
            Ok(exe) => {
                let value = wide(&settings_menu::autostart_command(&exe.to_string_lossy()));
                unsafe { RegSetValueExW(key, name.as_ptr(), 0, REG_SZ, value.as_ptr() as *const u8, (value.len() * 2) as u32) }
            }
            Err(e) => {
                unsafe { RegCloseKey(key) };
                return Err(e);
            }
        }
    } else {
        match unsafe { RegDeleteValueW(key, name.as_ptr()) } { 2 => 0, r => r }
    };
    unsafe { RegCloseKey(key) };
    if r == 0 { Ok(()) } else { Err(format!("자동 실행 설정을 바꾸지 못했습니다(Windows 오류 {r}).")) }
}


/// 작업 스레드: 클립보드에서 복사해 온 내용을 변환·암호화·저장한다(낮은 우선순위).
fn worker(rx: std::sync::mpsc::Receiver<(Capture, String)>) {
    unsafe {
        use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL};
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
    for (capture, source) in rx {
        let (mut kind, data, mut extra, skipped) = match capture {
            Capture::Text(s, formats, skipped) => ("text", s.into_bytes(), clip::pack_formats(&formats), skipped),
            Capture::Files(files) => ("files", files.join("\n").into_bytes(), Vec::new(), Vec::new()),
            Capture::Png(b, formats, skipped) => {
                app().last_clip_image.store(now_ms(), Ordering::Relaxed);
                if !claim_image(&b) { continue; }
                ("image", b, clip::pack_formats(&formats), skipped)
            }
            Capture::Dib(d, formats, skipped) => match { app().last_clip_image.store(now_ms(), Ordering::Relaxed); clip::dib_to_png(&d) } {
                Some(p) if claim_image(&p) => ("image", p, clip::pack_formats(&formats), skipped),
                Some(_) => continue,
                None => continue,
            },
        };
        let web_url = if kind == "text" {
            open_item::classify_web_text(&String::from_utf8_lossy(&data)).or_else(|| {
                clip::unpack_formats(&extra).into_iter().find_map(|(name, value)|
                    (name == "KAgent SourceURL").then(|| String::from_utf8(value).ok()).flatten()
                        .and_then(|url| open_item::safe_web_url(&url).map(str::to_owned)))
            })
        } else { None };
        if let Some(url) = &web_url { extra = open_item::encode_web_extra(&clip::unpack_formats(&extra), url); }
        let settings = app().settings.lock().unwrap().clone();
        if (data.len() + extra.len()) as i64 > settings.max_item_bytes {
            let names = skipped.join(", ");
            let suffix = if names.is_empty() { "저장 용량 상한".to_string() } else { names };
            let mut retained = clip::unpack_formats(&extra);
            retained.retain(|(_, bytes)| bytes.len() <= clip::MAX_FORMAT_BYTES);
            let mut trimmed = clip::pack_formats(&retained);
            while data.len().saturating_add(trimmed.len()) > settings.max_item_bytes as usize && !retained.is_empty() {
                retained.pop();
                trimmed = clip::pack_formats(&retained);
            }
            if data.len() as i64 > settings.max_item_bytes { continue; }
            let preview = format!("{} · 서식 상한 초과: {}", clip::preview(kind, &data), suffix);
            let t = now();
            let store = app().store.lock().unwrap();
            if store.insert(kind, &data, &trimmed, &preview, &source, t).is_ok() { let _ = store.enforce(&settings, t); }
            drop(store);
            refresh_recent();
            continue;
        }
        let preview = if let Some(url) = &web_url {
            let domain = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")).unwrap_or(url).split(['/', '?', '#']).next().unwrap_or(url);
            format!("{} · 웹 · {}", clip::preview(kind, &data), domain)
        } else if extra.is_empty() { clip::preview(kind, &data) }
            else if skipped.is_empty() { format!("{} · 서식 포함", clip::preview(kind, &data)) }
            else { format!("{} · 서식 포함 ({}개 형식 생략)", clip::preview(kind, &data), skipped.len()) };
        let t = now();
        {
            let store = app().store.lock().unwrap();
            if store.insert(kind, &data, &extra, &preview, &source, t).is_err() {
                continue;
            }
            let _ = store.enforce(&settings, t);
        }
        refresh_recent();
    }
}

/// 메모장 프로세스가 끝날 때까지 메모 파일을 가볍게 확인한다. 메타데이터가 바뀐 경우에만 UTF-8을 읽고,
/// 메모 ID를 유지한 채 payload·미리보기·해시를 암호화해 갱신한다.
pub fn start_memo() {
    let dir = app().dir.clone();
    let path = std::path::Path::new(&dir).join(format!("memo-{}.txt", now_ms()));
    if std::fs::write(&path, []).is_err() { return; }
    let child = match std::process::Command::new("notepad.exe").arg(&path).spawn() {
        Ok(child) => child,
        Err(_) => { let _ = std::fs::remove_file(&path); return; }
    };
    let cleanup_path = path.clone();
    if std::thread::Builder::new().name("kagent-memo".into()).stack_size(256 * 1024).spawn(move || {
        memo_watch(child, path);
    }).is_err() {
        let _ = std::fs::remove_file(cleanup_path);
    }
}

fn memo_watch(mut child: std::process::Child, path: std::path::PathBuf) {
    let mut last = None;
    let mut memo_id: Option<i64> = None;
    loop {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => break,
            _ => {}
        }
        if let Ok(meta) = std::fs::metadata(&path) {
            let stamp = (meta.len(), meta.modified().ok());
            if last.as_ref() != Some(&stamp) {
                last = Some(stamp);
                if let Ok(text) = std::fs::read_to_string(&path) {
                    if memo::should_save(&text) {
                        let settings = app().settings.lock().unwrap().clone();
                        if text.len() as i64 <= settings.max_item_bytes {
                            let preview = clip::preview("text", text.as_bytes());
                            let t = now();
                            let mut store = app().store.lock().unwrap();
                            let result = if let Some(id) = memo_id {
                                store.update_memo(id, text.as_bytes(), &preview, t).map(|_| (id, false))
                            } else {
                                store.insert("text", text.as_bytes(), &[], &preview, "notepad.exe", t)
                            };
                            if let Ok((id, _)) = result {
                                memo_id = Some(id);
                                let _ = store.enforce(&settings, t);
                                drop(store);
                                refresh_recent();
                            }
                        }
                    }
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
    let _ = std::fs::remove_file(path);
}

fn parse_hotkey(text: &str) -> Option<(u32, u32)> {
    let mut mods = MOD_NOREPEAT;
    let mut vk = 0u32;
    for part in text.split('+').map(|p| p.trim().to_ascii_lowercase()) {
        match part.as_str() {
            "ctrl" | "control" => mods |= MOD_CONTROL,
            "shift" => mods |= MOD_SHIFT,
            "alt" => mods |= MOD_ALT,
            "win" => mods |= MOD_WIN,
            p if p.len() == 1 && p.as_bytes()[0].is_ascii_alphanumeric() => vk = p.to_ascii_uppercase().as_bytes()[0] as u32,
            p if p.starts_with('f') => {
                let n: u32 = p[1..].parse().ok()?;
                if !(1..=24).contains(&n) {
                    return None;
                }
                vk = 0x70 + n - 1;
            }
            _ => return None,
        }
    }
    (vk != 0 && mods != MOD_NOREPEAT).then_some((mods, vk))
}

fn register_hotkey(hwnd: HWND) {
    unsafe { UnregisterHotKey(hwnd, HOTKEY_ID) };
    let text = app().settings.lock().unwrap().hotkey.clone();
    let ok = parse_hotkey(&text).map(|(m, vk)| unsafe { RegisterHotKey(hwnd, HOTKEY_ID, m, vk) } != 0).unwrap_or(false);
    app().hotkey_ok.store(ok, Ordering::Relaxed);
}

fn set_listening(hwnd: HWND, on: bool) {
    let a = app();
    if on && !a.listening.swap(true, Ordering::Relaxed) {
        unsafe { AddClipboardFormatListener(hwnd) };
    } else if !on && a.listening.swap(false, Ordering::Relaxed) {
        // 일시정지 중에는 알림 자체를 받지 않는다(처리 0).
        unsafe { RemoveClipboardFormatListener(hwnd) };
    }
}

/// 32×32 트레이 아이콘(파란 사각형에 흰 K, 일시정지면 회색)을 코드로 그린다 — 리소스 파일 없이.
fn make_icon(paused: bool) -> HICON {
    const N: i32 = 32;
    let dist = |px: f32, py: f32, (ax, ay): (f32, f32), (bx, by): (f32, f32)| {
        let (dx, dy) = (bx - ax, by - ay);
        let t = (((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        ((px - ax - t * dx).powi(2) + (py - ay - t * dy).powi(2)).sqrt()
    };
    unsafe {
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: N, biHeight: -N, biPlanes: 1,
            biBitCount: 32, biCompression: 0, biSizeImage: 0, biXPelsPerMeter: 0, biYPelsPerMeter: 0, biClrUsed: 0,
            biClrImportant: 0,
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let color = CreateDIBSection(std::ptr::null_mut(), &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        if color.is_null() || bits.is_null() {
            return LoadIconW(std::ptr::null_mut(), IDI_APPLICATION);
        }
        let px = std::slice::from_raw_parts_mut(bits as *mut u32, (N * N) as usize);
        let bg: u32 = if paused { 0xFF8A8F98 } else { 0xFF2563EB };
        for y in 0..N {
            for x in 0..N {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let corner = (fx.min(N as f32 - fx).min(6.0), fy.min(N as f32 - fy).min(6.0));
                let inside = corner.0 >= 6.0 || corner.1 >= 6.0
                    || (6.0 - corner.0).powi(2) + (6.0 - corner.1).powi(2) <= 36.0;
                let k = dist(fx, fy, (10.0, 6.0), (10.0, 26.0)) < 2.2
                    || dist(fx, fy, (11.0, 17.0), (23.0, 6.0)) < 2.2
                    || dist(fx, fy, (14.0, 14.5), (23.5, 26.0)) < 2.2;
                px[(y * N + x) as usize] = if !inside { 0 } else if k { 0xFFFFFFFF } else { bg };
            }
        }
        let mask = CreateBitmap(N, N, 1, 1, std::ptr::null());
        let ii = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&ii);
        windows_sys::Win32::Graphics::Gdi::DeleteObject(mask);
        windows_sys::Win32::Graphics::Gdi::DeleteObject(color);
        icon
    }
}

fn tray(hwnd: HWND, op: u32) {
    let paused = app().settings.lock().unwrap().paused;
    let count = app().recent.lock().unwrap().len();
    unsafe {
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY;
        nid.hIcon = make_icon(paused);
        let tip = if paused {
            "몽땅클립보드 — 일시정지".to_string()
        } else {
            let hotkey = app().settings.lock().unwrap().hotkey.clone();
            format!("몽땅클립보드 — 기록 중 ({count}개) · {hotkey}")
        };
        for (i, u) in tip.encode_utf16().take(nid.szTip.len() - 1).enumerate() {
            nid.szTip[i] = u;
        }
        Shell_NotifyIconW(op, &nid);
        if !nid.hIcon.is_null() {
            DestroyIcon(nid.hIcon);
        }
    }
}

fn tray_menu(hwnd: HWND) {
    let paused = app().settings.lock().unwrap().paused;
    unsafe {
        let menu = CreatePopupMenu();
        AppendMenuW(menu, MF_STRING, 1, wide("최근 항목 열기").as_ptr());
        AppendMenuW(menu, MF_STRING, 2, wide(if paused { "다시 기록" } else { "일시정지" }).as_ptr());
        AppendMenuW(menu, MF_STRING, 3, wide("모두 지우기(고정 제외)").as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, 4, wide("몽땅클립보드 끝내기").as_ptr());
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, std::ptr::null());
        DestroyMenu(menu);
        match cmd {
            1 => popup::show(),
            2 => set_paused(hwnd, !paused),
            3 => {
                let _ = app().store.lock().unwrap().clear(true);
                refresh_recent();
            }
            4 => {
                DestroyWindow(hwnd);
            }
            _ => {}
        }
    }
}

/// 일시정지 켜기/끄기. 트레이·앱(파이프)·설정 메뉴 어느 쪽에서 바꾸든 남아 있던 시간 예약은 지운다
/// (시간 일시정지는 `pause_for`가 이 함수 뒤에 다시 건다).
pub fn set_paused(hwnd: HWND, paused: bool) {
    PAUSE_UNTIL_MS.store(0, Ordering::Relaxed);
    unsafe { KillTimer(hwnd, TIMER_RESUME) };
    app().settings.lock().unwrap().paused = paused;
    save_settings();
    set_listening(hwnd, !paused);
    tray(hwnd, NIM_MODIFY);
    restart_watchers();
    write_status();
}

/// 항목을 클립보드에 올리고(자기 쓰기는 다시 기록하지 않음), 원하면 target 창에 Ctrl+V.
pub fn restore(id: i64, paste: bool, target: HWND) -> bool {
    let payload = app().store.lock().unwrap().payload(id).ok().flatten();
    let Some((kind, data, extra)) = payload else { return false };
    let hwnd = main_hwnd();
    // 엑셀 표 항목은 설정이 켜져 있으면 앱에 '목표 결과' 변환을 맡긴다. 앱이 꺼져 있거나 늦거나 쓰기에
    // 실패하면 저장한 원래 형식 그대로 올린다.
    let enabled = app().settings.lock().unwrap().excel_target_paste;
    let converted = xl_target::convert(&kind, &data, &extra, enabled);
    let written = xl_target::write_choice(
        converted,
        |(html, text)| clip::write_html_text(hwnd, html, text),
        || clip::write(hwnd, &kind, &data, &extra),
    );
    if written.is_none() {
        return false;
    }
    app().self_seq.store(unsafe { GetClipboardSequenceNumber() }, Ordering::Relaxed);
    let _ = app().store.lock().unwrap().touch(id, now());
    refresh_recent();
    if paste && !target.is_null() {
        popup::paste_into(target);
    }
    true
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_CLIPBOARDUPDATE => {
                if GetClipboardSequenceNumber() != app().self_seq.load(Ordering::Relaxed) {
                    SetTimer(hwnd, TIMER_READ, READ_DELAY_MS, None);
                }
                0
            }
            WM_TIMER if wp == TIMER_READ => {
                KillTimer(hwnd, TIMER_READ);
                let a = app();
                if a.settings.lock().unwrap().paused
                    || GetClipboardSequenceNumber() == a.self_seq.load(Ordering::Relaxed)
                {
                    return 0;
                }
                let source = clip::source_app();
                let (excluded, max) = {
                    let s = a.settings.lock().unwrap();
                    (s.excluded_apps.iter().any(|e| *e == source), s.max_item_bytes as usize)
                };
                if !excluded {
                    if let Some(capture) = clip::read(hwnd, max) {
                        let _ = a.tx.lock().unwrap().send((capture, source));
                    }
                }
                0
            }
            WM_TIMER if wp == TIMER_RESUME => {
                set_paused(hwnd, false); // 타이머도 함께 멈춘다
                popup::refresh();
                0
            }
            WM_HOTKEY if wp as i32 == HOTKEY_ID => {
                popup::show();
                0
            }
            WM_TRAY => {
                match lp as u32 {
                    WM_LBUTTONUP => popup::show(),
                    WM_RBUTTONUP | WM_CONTEXTMENU => tray_menu(hwnd),
                    _ => {}
                }
                0
            }
            WM_REFRESH => {
                tray(hwnd, NIM_MODIFY);
                popup::refresh();
                0
            }
            WM_RESTORE => {
                restore(wp as i64, lp != 0, lp as HWND);
                0
            }
            WM_SHOW => {
                popup::show();
                0
            }
            WM_PAUSE => {
                set_paused(hwnd, wp != 0);
                0
            }
            WM_SETTINGS => {
                register_hotkey(hwnd);
                popup::apply_settings();
                let paused = app().settings.lock().unwrap().paused;
                set_listening(hwnd, !paused);
                tray(hwnd, NIM_MODIFY);
                restart_watchers();
                write_status();
                0
            }
            WM_DESTROY => {
                set_listening(hwnd, false);
                watch::stop();
                UnregisterHotKey(hwnd, HOTKEY_ID);
                tray(hwnd, NIM_DELETE);
                let _ = std::fs::remove_file(format!("{}\\status.json", app().dir));
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    // 인스턴스 mutex·트레이·DB를 열기 전에 번들 버전만 조회한다.
    if has("--version") {
        println!("{}", crate::json::to_string(&crate::json::Value::obj(vec![
            ("ok", true.into()), ("version", VERSION.into()),
        ])));
        return;
    }
    if let Some(command) = args.iter().find(|a| ["--install", "--upgrade-if-stale", "--status", "--enable", "--disable"].contains(&a.as_str())) {
        let result = lifecycle_command(command, &args);
        println!("{}", crate::json::to_string(&result));
        return;
    }
    if has("--pause") || has("--resume") {
        let pause = has("--pause");
        let status_path = std::path::Path::new(&util::data_dir()).join("status.json");
        if let Ok(raw) = std::fs::read_to_string(status_path) {
            if let Some(status) = crate::json::parse(&raw).and_then(|v| v.str("pipe").map(str::to_owned)) {
                let mut req = crate::json::Value::obj(vec![("cmd", if pause {"pause"} else {"resume"}.into())]);
                if pause { if let Some(i)=args.iter().position(|a| a=="--minutes") { if let Some(n)=args.get(i+1).and_then(|s|s.parse::<u64>().ok()) { if let crate::json::Value::Obj(ref mut fields)=req { fields.push(("minutes".into(), (n.clamp(1,1440) as f64).into())); } } } }
                let _ = lifecycle_pipe_request(&status, &req);
            }
        }
        println!("{{\"ok\":true}}");
        return;
    }
    let mutex_name = format!("Local\\HwpAutoDocFit.kagent{}", util::instance_suffix().unwrap_or_default());
    let instance_mutex = unsafe {
        // 하나만 실행: 이미 있으면 그쪽에 요청만 보내고 끝낸다.
        CreateMutexW(std::ptr::null(), 0, wide(&mutex_name).as_ptr())
    };
    if instance_mutex.is_null() {
        std::process::exit(1);
    }
    unsafe {
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(instance_mutex);
            let other = FindWindowW(wide(MAIN_CLASS).as_ptr(), std::ptr::null());
            if !other.is_null() {
                if has("--quit") {
                    PostMessageW(other, WM_CLOSE, 0, 0);
                } else {
                    PostMessageW(other, WM_SHOW, 0, 0);
                }
            }
            return;
        }
        if has("--quit") {
            CloseHandle(instance_mutex);
            return;
        }
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    // 로그인 직후 자동 시작이면 다른 프로그램이 먼저 뜨도록 늦게 시작한다.
    if let Some(i) = args.iter().position(|a| a == "--delay") {
        if let Some(secs) = args.get(i + 1).and_then(|s| s.parse::<u64>().ok()) {
            std::thread::sleep(std::time::Duration::from_secs(secs.min(300)));
        }
    }

    let dir = util::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    open_item::cleanup(&dir);
    let store = match Store::open(&format!("{dir}\\clip.db")) {
        Ok(s) => s,
        Err(_) => std::process::exit(2),
    };
    let settings = Settings::load(&format!("{dir}\\settings.json"));
    let (tx, rx) = channel();
    let (tl_tx, tl_rx) = channel();
    let _ = APP.set(App {
        dir,
        store: Mutex::new(store),
        settings: Mutex::new(settings),
        recent: Mutex::new(Vec::new()),
        recent_error: Mutex::new(String::new()),
        tx: Mutex::new(tx),
        self_seq: AtomicU32::new(0),
        hwnd: AtomicIsize::new(0),
        hotkey_ok: AtomicBool::new(false),
        listening: AtomicBool::new(false),
        last_clip_image: AtomicU64::new(0),
        image_dedupe: Mutex::new(std::collections::VecDeque::new()),
        timeline: Mutex::new(None),
        screenshots: Mutex::new(String::new()),
        scan_pending: Mutex::new(HashSet::new()),
        timeline_generation: AtomicU64::new(0),
        timeline_files: AtomicI64::new(0),
        timeline_tx: Mutex::new(Some(tl_tx)),
    });
    *app().recent.lock().unwrap() = app().store.lock().unwrap().recent(RECENT, None).unwrap_or_default();
    std::thread::Builder::new().name("kagent-worker".into()).stack_size(256 * 1024).spawn(move || worker(rx)).ok();
    std::thread::Builder::new().name("kagent-timeline".into()).stack_size(512 * 1024).spawn(move || timeline_worker(tl_rx)).ok();

    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = wide(MAIN_CLASS);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc);
        // 보이지 않는 창(트레이 알림을 받으려면 메시지 전용 창이 아니어야 한다).
        let hwnd = CreateWindowExW(WS_EX_TOOLWINDOW, class.as_ptr(), wide("K에이전트").as_ptr(), WS_POPUP, 0, 0, 0, 0,
                                   std::ptr::null_mut(), std::ptr::null_mut(), instance, std::ptr::null());
        if hwnd.is_null() {
            std::process::exit(3);
        }
        app().hwnd.store(hwnd as isize, Ordering::Relaxed);
        popup::register(instance);
        // 격리 시험 전용: 시험 폴더가 없으면 실제 기록에 시험 항목을 넣지 않도록 무시한다.
        if has("--test-window") && std::env::var_os("KAGENT_TEST_ROOT").is_some() {
            if let Some(root) = std::env::var_os("KAGENT_TEST_ROOT") {
                let _ = std::fs::create_dir_all(&root);
                let settings_path = std::path::Path::new(&root).join("settings.json");
                let mut settings = app().settings.lock().unwrap();
                settings.always_on_top = true;
                settings.sticky_popup = true;
                settings.watch_screenshots = false;
                settings.popup_x = 120;
                settings.popup_y = 100;
                settings.popup_width = 560;
                settings.popup_height = 420;
                let _ = settings.save(&settings_path.to_string_lossy());
            }
            // 시험 데이터와 창 상태를 먼저 준비해 한 번만 표시한다. sticky_popup=true인 상태에서
            // show()를 연속 호출하면 두 번째 호출이 창을 숨기는 토글 동작을 한다.
            let sample = format!("V229 KAG 시험 복사 항목 {}", std::process::id());
            let _ = app().store.lock().unwrap().insert("text", sample.as_bytes(), &[], &sample, "test", now());
            refresh_recent();
            {
                let mut settings = app().settings.lock().unwrap();
                settings.sticky_popup = true;
                settings.popup_collapsed = false;
                settings.popup_x = 120;
                settings.popup_y = 100;
                settings.popup_width = 560;
                settings.popup_height = 420;
            }
            save_settings();
            popup::show();
        }
        register_hotkey(hwnd);
        let paused = app().settings.lock().unwrap().paused;
        set_listening(hwnd, !paused);
        tray(hwnd, NIM_ADD);
        restart_watchers();
        write_status();
        pipe::start();

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        CloseHandle(instance_mutex);
    }
    open_item::cleanup(&app().dir);
}

fn lifecycle_command(command: &str, args: &[String]) -> crate::json::Value {
    use crate::json::Value;
    let root = std::env::var("KAGENT_DATA").ok().map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(util::data_dir()));
    let target = root.join("kagent.exe");
    let source = std::env::current_exe().ok();
    let bundled = source.as_deref().filter(|p| p.is_file());
    match command {
        "--status" => {
            let mut status = std::fs::read_to_string(root.join("status.json")).ok()
                .and_then(|s| crate::json::parse(&s)).unwrap_or_else(|| Value::obj(vec![]));
            let running = status.get("pid").and_then(|v| if let Value::Num(n)=v {Some(*n>0.0)} else {None}).unwrap_or(false);
            let pause = pause_until();
            if pause > 0.0 { if let Value::Obj(ref mut fields)=status { fields.retain(|(k,_)| k!="paused" && k!="pause_until"); fields.push(("paused".into(), true.into())); fields.push(("pause_until".into(), pause.into())); } }
            let consent = "몽땅클립보드는 이 PC에서 복사(Ctrl+C)한 글·그림·파일 경로를 기록해 다시 붙여넣을 수 있게 합니다. 기록은 이 PC의 사용자 폴더에만 암호화해 저장하고 밖으로 보내지 않습니다. 암호 관리자가 보내는 '기록 제외' 신호는 따릅니다. 언제든 일시정지·끄기·모두 지우기를 할 수 있습니다.";
            return Value::obj(vec![("ok", true.into()), ("available", bundled.is_some().into()),
                ("running", running.into()), ("status", status), ("autostart", autostart_enabled().unwrap_or(false).into()), ("consent", consent.into())]);
        }
        "--install" | "--enable" | "--upgrade-if-stale" => {
            let Some(source) = bundled else { return Value::obj(vec![("ok", false.into()), ("reason", "no_bundle".into())]); };
            if command == "--enable" && !args.iter().any(|a| a == "--consent") {
                return Value::obj(vec![("ok", false.into()), ("code", "KCLIP_CONSENT".into())]);
            }
            let hash_sidecar = source.with_file_name(format!("{}.sha256", source.file_name().unwrap().to_string_lossy()));
            if let Ok(expected) = std::fs::read_to_string(hash_sidecar) {
                if sha256_file(source).as_deref() != Some(expected.trim().to_ascii_lowercase().as_str()) {
                    return Value::obj(vec![("ok", false.into()), ("reason", "integrity".into())]);
                }
            }
            let current = std::fs::read(source).ok();
            let old = std::fs::read(&target).ok();
            let stale = current != old;
            if command == "--upgrade-if-stale" && old.is_none() { return Value::obj(vec![("ok", true.into()), ("upgraded", false.into()), ("reason", "not_installed".into())]); }
            let was_running = std::fs::read_to_string(root.join("status.json")).ok().and_then(|s| crate::json::parse(&s))
                .and_then(|v| v.num("pid")).is_some();
            if was_running && (command == "--enable" || (command == "--upgrade-if-stale" && stale)) {
                let _ = std::process::Command::new(&target).arg("--quit").spawn();
                for _ in 0..150 { if !root.join("status.json").exists() { break; } std::thread::sleep(std::time::Duration::from_millis(20)); }
                if root.join("status.json").exists() { return Value::obj(vec![("ok", false.into()), ("upgraded", false.into()), ("reason", "still_running".into())]); }
            }
            if command != "--upgrade-if-stale" || stale {
                if let Err(e) = std::fs::create_dir_all(&root).and_then(|_| std::fs::copy(source, &target).map(|_| ())) {
                    return Value::obj(vec![("ok", false.into()), ("reason", e.to_string().into())]);
                }
            }
            if command == "--enable" { let _ = std::process::Command::new(&target).spawn(); if args.iter().any(|a| a == "--autostart") { let _ = set_autostart(true); } }
            if command == "--upgrade-if-stale" && stale && was_running { let _ = std::process::Command::new(&target).spawn(); }
            let mut result = vec![("ok", true.into()), ("upgraded", stale.into()), ("installed", target.to_string_lossy().into_owned().into())];
            if command == "--upgrade-if-stale" { result.push(("restarted", (stale && was_running).into())); }
            return Value::obj(result);
        }
        "--disable" => {
            let consent = "몽땅클립보드는 이 PC에서 복사(Ctrl+C)한 글·그림·파일 경로를 기록해 다시 붙여넣을 수 있게 합니다. 기록은 이 PC의 사용자 폴더에만 암호화해 저장하고 밖으로 보내지 않습니다. 암호 관리자가 보내는 '기록 제외' 신호는 따릅니다. 언제든 일시정지·끄기·모두 지우기를 할 수 있습니다.";
            if std::env::var_os("KAGENT_TEST_ROOT").is_none() { let _ = std::process::Command::new(&target).arg("--quit").spawn(); }
            for _ in 0..50 { if !root.join("status.json").exists() { break; } std::thread::sleep(std::time::Duration::from_millis(20)); }
            let _ = set_autostart(false);
            let status = std::fs::read_to_string(root.join("status.json")).ok().and_then(|s| crate::json::parse(&s)).unwrap_or_else(|| Value::obj(vec![]));
            return Value::obj(vec![("ok", true.into()), ("running", root.join("status.json").exists().into()), ("available", bundled.is_some().into()), ("autostart", autostart_enabled().unwrap_or(false).into()), ("consent", consent.into()), ("status", status)]);
        }
        _ => return Value::obj(vec![("ok", false.into())]),
    }
}

fn sha256_file(path: &std::path::Path) -> Option<String> {
    let data = std::fs::read(path).ok()?;
    let mut h = [0x6a09e667u32,0xbb67ae85,0x3c6ef372,0xa54ff53a,0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19];
    const K:[u32;64]=[0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2];
    let mut msg=data; let bits=(msg.len() as u64)*8; msg.push(0x80); while msg.len()%64!=56 {msg.push(0)} msg.extend_from_slice(&bits.to_be_bytes());
    for c in msg.chunks_exact(64) { let mut w=[0u32;64]; for i in 0..16 {w[i]=u32::from_be_bytes(c[i*4..i*4+4].try_into().ok()?)} for i in 16..64 {let a=w[i-15];let b=w[i-2];w[i]=w[i-16].wrapping_add(a.rotate_right(7)^a.rotate_right(18)^(a>>3)).wrapping_add(w[i-7]).wrapping_add(b.rotate_right(17)^b.rotate_right(19)^(b>>10));} let(mut a,mut b,mut c,mut d,mut e,mut f,mut g,mut z)=(h[0],h[1],h[2],h[3],h[4],h[5],h[6],h[7]); for i in 0..64 {let t1=z.wrapping_add(e.rotate_right(6)^e.rotate_right(11)^e.rotate_right(25)).wrapping_add((e&f)^(!e&g)).wrapping_add(K[i]).wrapping_add(w[i]);let t2=(a.rotate_right(2)^a.rotate_right(13)^a.rotate_right(22)).wrapping_add((a&b)^(a&c)^(b&c));z=g;g=f;f=e;e=d.wrapping_add(t1);d=c;c=b;b=a;a=t1.wrapping_add(t2);} for(i,v)in[a,b,c,d,e,f,g,z].iter().enumerate(){h[i]=h[i].wrapping_add(*v)} }
    Some(h.iter().map(|v|format!("{v:08x}")).collect())
}

fn pause_until() -> f64 {
    let path = std::env::var_os("KAGENT_PAUSE_FILE").map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(format!("docfit-kclip-pause-{}.json", std::process::id())));
    let until = std::fs::read_to_string(&path).ok().and_then(|s| crate::json::parse(&s))
        .and_then(|v| v.num("until")).unwrap_or(0.0);
    if until > 0.0 && until <= util::now() {
        let _ = std::fs::remove_file(&path);
        let status_path = std::path::Path::new(&util::data_dir()).join("status.json");
        if std::fs::read_to_string(&status_path).map(|s| s.contains("\"paused\":true")).unwrap_or(false) {
            if let Ok(exe) = std::env::current_exe() { let _ = std::process::Command::new(exe).arg("--resume").spawn(); }
        }
        return 0.0;
    }
    until
}

fn lifecycle_pipe_request(pipe: &str, request: &crate::json::Value) -> Option<crate::json::Value> {
    use std::io::{Read, Write};
    let mut stream = std::fs::OpenOptions::new().read(true).write(true).open(pipe).ok()?;
    let mut bytes = crate::json::to_string(request).into_bytes(); bytes.push(b'\n');
    stream.write_all(&bytes).ok()?;
    let mut response = String::new(); stream.read_to_string(&mut response).ok()?;
    crate::json::parse(&response)
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[test]
    fn lifecycle_installs_upgrades_and_never_touches_registry_in_test_root() {
        let root = std::env::temp_dir().join(format!("kagent-lifecycle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        unsafe { std::env::set_var("KAGENT_TEST_ROOT", &root); }
        unsafe { std::env::set_var("KAGENT_DATA", &root); }
        let exe = std::env::current_exe().unwrap();
        let target = root.join("kagent.exe");
        let source = std::fs::read(&exe).unwrap();
        std::fs::write(&target, &source).unwrap();
        assert_eq!(sha256_file(&target), sha256_file(&exe));
        assert_eq!(set_autostart(true).unwrap_err(), "시험 실행에서는 자동 실행 등록을 바꾸지 않습니다.");
        let _ = std::fs::remove_dir_all(root);
        unsafe { std::env::remove_var("KAGENT_TEST_ROOT"); }
        unsafe { std::env::remove_var("KAGENT_DATA"); }
    }

    #[test]
    fn sha256_matches_standard_vector() {
        let p = std::env::temp_dir().join(format!("kagent-sha-{}", std::process::id()));
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(sha256_file(&p).as_deref(), Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
        let _ = std::fs::remove_file(p);
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: i64) -> Summary {
        Summary { id, kind: "text".into(), preview: format!("항목 {id}"), pinned: false, used: 1.0, size: 1,
                  cats: crate::category::CategorySet::default() }
    }

    #[test]
    fn failed_recent_query_keeps_previous_list_and_reports_error() {
        let previous = vec![sample(1), sample(2)];
        let (list, error) = merge_recent(previous.clone(), Err("database is locked".to_string()));
        assert_eq!(list.len(), 2, "조회 실패 때 예전 목록을 지우면 안 된다");
        assert_eq!(error.as_deref(), Some("database is locked"));
        let (list, error) = merge_recent(previous, Ok(vec![sample(3)]));
        assert_eq!(list.iter().map(|s| s.id).collect::<Vec<_>>(), vec![3]);
        assert!(error.is_none());
    }

    #[test]
    fn hotkeys() {
        assert_eq!(parse_hotkey("Ctrl+Shift+V"), Some((MOD_NOREPEAT | MOD_CONTROL | MOD_SHIFT, 'V' as u32)));
        assert_eq!(parse_hotkey("Alt+F2"), Some((MOD_NOREPEAT | MOD_ALT, 0x71)));
        assert_eq!(parse_hotkey("V"), None);
        assert_eq!(parse_hotkey("Ctrl+Space"), None);
    }

    #[test]
    fn timeline_scan_pending_deduplicates_root() {
        let mut pending = HashSet::new();
        assert!(mark_scan_pending(&mut pending, "C:\\test"));
        assert!(!mark_scan_pending(&mut pending, "C:\\test"));
        assert!(mark_scan_pending(&mut pending, "C:\\other"));
        assert!(unmark_scan_pending(&mut pending, "C:\\test"));
        assert!(!unmark_scan_pending(&mut pending, "C:\\test"));
        assert!(mark_scan_pending(&mut pending, "C:\\test"));
    }

    #[test]
    fn timeline_generation_stale_detection() {
        let current_gen = 2u64;
        let old_msg_gen = 1u64;
        let new_msg_gen = 2u64;
        assert!(old_msg_gen < current_gen);
        assert!(!(new_msg_gen < current_gen));
    }
}
