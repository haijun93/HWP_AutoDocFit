//! ?????? ?? ?. kagent ?? ??? ?? ?? ??.
use crate::{store::Settings, util::wide};
use std::sync::atomic::{AtomicIsize, Ordering};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, RECT},
    Graphics::Gdi::{
        BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, EndPaint, FillRect, PAINTSTRUCT, SetBkColor,
        SetBkMode, SetTextColor, TRANSPARENT,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        HiDpi::{
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem, GetDpiForWindow,
            SetProcessDpiAwarenessContext,
        },
        Input::KeyboardAndMouse::VK_ESCAPE,
    WindowsAndMessaging::*,
    },
};

pub const OPEN_MESSAGE: u32 = WM_APP + 42;
const APPLY: i32 = 1001;
const ENABLE: i32 = 1002;
const CANCEL: i32 = 1003;
const CONSENT: i32 = 1004;
const NUM: i32 = 1010;
const ITEM: i32 = 1011;
const TOTAL: i32 = 1012;
const AGE: i32 = 1013;
const STICKY: i32 = 1020;
const TOP: i32 = 1021;
const PASTE: i32 = 1022;
const EXCEL: i32 = 1023;
const PREVIEW: i32 = 1024;
const SHOTS: i32 = 1025;
const AUTOSTART: i32 = 1026;
const CAP_TEXT: i32 = 1027;
const CAP_IMAGE: i32 = 1028;
const CAP_FILE: i32 = 1029;
const APP_LIST: i32 = 1040;
const FOLDER_LIST: i32 = 1041;
const LIST_EDIT: i32 = 1042;
const HOTKEY_EDIT: i32 = 1045;
const APP_ADD: i32 = 1047;
const APP_DEL: i32 = 1048;
const FOLDER_ADD: i32 = 1049;
const FOLDER_DEL: i32 = 1050;
const STATUS: i32 = 1030;
const DONE: u32 = WM_APP + 41;
static WINDOW: AtomicIsize = AtomicIsize::new(0);
static LIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[link(name = "user32")]
unsafe extern "system" { fn EnableWindow(hwnd: HWND, enabled: i32) -> i32; fn GetKeyState(key: i32) -> i16; }
type SubclassProc = unsafe extern "system" fn(HWND, u32, usize, isize, usize, usize) -> isize;
#[link(name = "comctl32")]
unsafe extern "system" {
    fn SetWindowSubclass(hwnd: HWND, proc: Option<SubclassProc>, id: usize, data: usize) -> i32;
    fn DefSubclassProc(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize;
}
#[repr(C)]
struct BrowseInfoW {
    owner: HWND, root: *mut std::ffi::c_void, display_name: *mut u16, title: *const u16,
    flags: u32, callback: Option<unsafe extern "system" fn(HWND, u32, isize, isize) -> i32>,
    param: isize, image: i32,
}
#[link(name = "shell32")]
unsafe extern "system" {
    fn SHBrowseForFolderW(info: *mut BrowseInfoW) -> *mut std::ffi::c_void;
    fn SHGetPathFromIDListW(pidl: *mut std::ffi::c_void, path: *mut u16) -> i32;
}
#[link(name = "ole32")]
unsafe extern "system" { fn CoTaskMemFree(memory: *mut std::ffi::c_void); }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub width: i32,
    pub height: i32,
    pub row: i32,
    pub margin: i32,
}
pub fn layout(dpi: u32) -> Layout {
    let s = |n: i32| n * dpi.max(96) as i32 / 96;
    Layout {
        width: s(650),
        height: s(1160),
        row: s(34),
        margin: s(28),
    }
}
fn data_dir() -> String {
    std::env::var("KAGENT_TEST_ROOT")
        .or_else(|_| std::env::var("KAGENT_DATA"))
        .unwrap_or_else(|_| {
            std::env::var("LOCALAPPDATA")
                .map(|p| format!("{p}\\HwpAutoDocFit\\kagent"))
                .unwrap_or_else(|_| ".\\HwpAutoDocFit\\kagent".into())
        })
}
fn load() -> Settings {
    Settings::load(&format!("{}\\settings.json", data_dir()))
}
fn save(s: &Settings) -> Result<(), String> {
    let d = data_dir();
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    s.save(&format!("{d}\\settings.json"))
        .map_err(|e| e.to_string())
}
pub fn open(capture: bool, dark: bool) {
    if capture {
        capture_png(dark);
        return;
    }
    run(false);
}
pub fn open_live() {
    run(true)
}
pub fn request_open() {
    unsafe {
        PostMessageW(crate::main_hwnd(), OPEN_MESSAGE, 0, 0);
    }
}
fn run(live: bool) {
    LIVE.store(live, Ordering::Relaxed);
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // `--settings` is started as a new process by the app. The process-local
        // WINDOW handle only protects repeated opens from this process, so also
        // look up the class across processes before creating another window.
        let cls = wide("HwpAutoDocFit.KAgentSettings");
        let existing = FindWindowW(cls.as_ptr(), std::ptr::null());
        if !existing.is_null() {
            ShowWindow(existing, SW_RESTORE);
            SetForegroundWindow(existing);
            return;
        }
        if WINDOW.load(Ordering::Relaxed) != 0 {
            let h = WINDOW.load(Ordering::Relaxed) as HWND;
            ShowWindow(h, SW_RESTORE);
            SetForegroundWindow(h);
            return;
        }
        let inst = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: inst,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: cls.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc);
        let dpi = GetDpiForSystem().max(96) as i32;
        let h = CreateWindowExW(
            WS_EX_APPWINDOW,
            cls.as_ptr(),
            wide("몽땅클립보드 설정").as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            680 * dpi / 96,
            1220 * dpi / 96,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            inst,
            std::ptr::null(),
        );
        if h.is_null() {
            return;
        }
        WINDOW.store(h as isize, Ordering::Relaxed);
        let mut m: MSG = std::mem::zeroed();
        while GetMessageW(&mut m, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&m);
            DispatchMessageW(&m);
        }
    }
}
unsafe fn child(
    h: HWND,
    id: i32,
    class: &str,
    text: &str,
    x: i32,
    y: i32,
    w: i32,
    ht: i32,
    style: u32,
) {
    let dpi = unsafe { GetDpiForWindow(h).max(96) } as i32;
    unsafe {
        CreateWindowExW(
            0,
            wide(class).as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            x * dpi / 96,
            y * dpi / 96,
            w * dpi / 96,
            ht * dpi / 96,
            h,
            id as usize as _,
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
    }
}
unsafe fn checkbox(h: HWND, id: i32, text: &str, on: bool, y: i32) {
    unsafe {
        child(
            h,
            id,
            "BUTTON",
            text,
            28,
            y,
            590,
            32,
            BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
        );
        SendMessageW(GetDlgItem(h, id), BM_SETCHECK, if on { 1 } else { 0 }, 0);
    }
}
unsafe fn listbox(h: HWND, id: i32, rows: &[String], y: i32) {
    unsafe {
        child(h,id,"LISTBOX","",28,y,450,64,LBS_NOTIFY as u32|WS_VSCROLL|WS_BORDER|WS_TABSTOP|LBS_NOINTEGRALHEIGHT as u32);
        let c=GetDlgItem(h,id);
        for row in rows { SendMessageW(c,LB_ADDSTRING,0,wide(row).as_ptr() as LPARAM); }
    }
}
unsafe fn combo(h: HWND, id: i32, label: &str, vals: &[i64], cur: i64, unit: &str, y: i32) {
    unsafe {
        child(h, id, "STATIC", label, 28, y, 245, 28, 0);
        child(
            h,
            id,
            "COMBOBOX",
            "",
            285,
            y - 3,
            260,
            150,
            CBS_DROPDOWNLIST as u32 | WS_TABSTOP,
        );
        let c = GetDlgItem(h, id);
        for n in vals {
            let t = wide(&format!("{n}{unit}"));
            SendMessageW(c, CB_ADDSTRING, 0, t.as_ptr() as LPARAM);
        }
        let ix = vals.iter().position(|n| *n == cur).unwrap_or(0);
        SendMessageW(c, CB_SETCURSEL, ix, 0);
    }
}
unsafe fn build(h: HWND) {
    let s = if LIVE.load(Ordering::Relaxed) {
        crate::app().settings.lock().unwrap().clone()
    } else {
        load()
    };
    unsafe {
        child(
            h,
            1109,
            "STATIC",
            "복사한 내용을 이 PC에 암호화해 보관하고 다시 붙여넣습니다.",
            28,
            52,
            600,
            36,
            0,
        );
        child(h, 1100, "STATIC", "몽땅클립보드 설정", 28, 20, 600, 34, 0);
        child(h, 1101, "STATIC", "보관 설정", 28, 82, 590, 30, 0);
        combo(
            h,
            NUM,
            "보관 개수",
            &[20, 50, 100, 200, 500, 1000],
            s.max_items,
            "개",
            118,
        );
        combo(
            h,
            ITEM,
            "항목 하나 최대 크기",
            &[1, 5, 10, 20, 50],
            s.max_item_bytes / 1048576,
            "MB",
            154,
        );
        combo(
            h,
            TOTAL,
            "전체 최대 크기",
            &[50, 100, 200, 500, 1000],
            s.max_total_bytes / 1048576,
            "MB",
            190,
        );
        combo(
            h,
            AGE,
            "보관 기간",
            &[7, 30, 90, 365],
            s.max_age_days.round() as i64,
            "일",
            226,
        );
        child(
            h,
            1102,
            "STATIC",
            &format!("단축키: {} · 암호화 저장: 항상 켜짐", s.hotkey),
            28,
            270,
            590,
            30,
            0,
        );
        child(h, 1103, "STATIC", "동작 설정", 28, 310, 590, 30, 0);
        checkbox(
            h,
            STICKY,
            "스티커 메모처럼 화면에 유지",
            s.sticky_popup,
            334,
        );
        checkbox(h, TOP, "항상 위", s.always_on_top, 370);
        checkbox(h, PASTE, "고르면 바로 붙여넣기", s.auto_paste, 406);
        checkbox(
            h,
            EXCEL,
            "엑셀 표는 목표 결과로 붙이기",
            s.excel_target_paste,
            442,
        );
        checkbox(h, PREVIEW, "그림 미리보기", s.preview_images, 478);
        checkbox(h, SHOTS, "화면캡처 폴더 감시", s.watch_screenshots, 514);
        checkbox(h,CAP_TEXT,"글 기록 허용",s.capture_text,550);
        checkbox(h,CAP_IMAGE,"그림 기록 허용",s.capture_images,586);
        checkbox(h,CAP_FILE,"파일 기록 허용",s.capture_files,622);
        child(h, 1104, "STATIC", "제외 앱 (실행 파일 이름)", 28, 634, 450, 25, 0);
        listbox(h,APP_LIST,&s.excluded_apps,660);
        child(h, 1105, "STATIC", "폴더별 캡처 끄기", 28, 732, 450, 25, 0);
        listbox(h,FOLDER_LIST,&s.capture_dirs_off,758);
        child(h,1107,"STATIC","추가할 앱 이름(.exe) 또는 폴더 경로",28,850,430,24,0);
        child(h,LIST_EDIT,"EDIT","",28,874,330,28,WS_BORDER as u32|ES_AUTOHSCROLL as u32|WS_TABSTOP);
        child(h,APP_ADD,"BUTTON","앱 추가",486,660,90,30,WS_TABSTOP);
        child(h,APP_DEL,"BUTTON","앱 삭제",486,698,90,30,WS_TABSTOP);
        child(h,FOLDER_ADD,"BUTTON","폴더 추가",486,758,90,30,WS_TABSTOP);
        child(h,FOLDER_DEL,"BUTTON","폴더 삭제",486,796,90,30,WS_TABSTOP);
        child(h,1108,"STATIC","단축키 (입력한 뒤 저장)",28,922,240,25,0);
        child(h,HOTKEY_EDIT,"EDIT",&s.hotkey,260,918,220,30,WS_BORDER as u32|ES_AUTOHSCROLL as u32|WS_TABSTOP);
        SetWindowSubclass(GetDlgItem(h, HOTKEY_EDIT), Some(hotkey_capture_proc), 1, h as usize);
        child(
            h,
            1106,
            "STATIC",
            "기록은 이 PC 사용자 폴더에 암호화되어 저장됩니다. 외부로 전송하지 않습니다.",
            28,
            1002,
            610,
            30,
            0,
        );
        checkbox(
            h,
            AUTOSTART,
            "Windows 로그인 시 자동 실행",
            crate::autostart_enabled().unwrap_or(false),
            964,
        );
        checkbox(
            h,
            CONSENT,
            "안내를 읽었고 몽땅클립보드 사용에 동의합니다",
            false,
            1034,
        );
        child(
            h,
            STATUS,
            "STATIC",
            "실행 중이면 저장 즉시 적용하고, 꺼져 있으면 다음 시작부터 적용합니다.",
            28,
            1066,
            610,
            28,
            0,
        );
        child(
            h,
            APPLY,
            "BUTTON",
            "설정 저장",
            300,
            1104,
            100,
            32,
            WS_TABSTOP,
        );
        child(
            h,
            ENABLE,
            "BUTTON",
            "몽땅클립보드 켜기",
            410,
            1104,
            155,
            32,
            WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
        );
        child(h, CANCEL, "BUTTON", "닫기", 575, 1104, 70, 32, WS_TABSTOP);
        EnableWindow(GetDlgItem(h, ENABLE), 0);
    }
}
fn persist(h: HWND) {
    let mut s = if LIVE.load(Ordering::Relaxed) {
        crate::app().settings.lock().unwrap().clone()
    } else {
        load()
    };
    let get = |id| unsafe { SendMessageW(GetDlgItem(h, id), BM_GETCHECK, 0, 0) == 1 };
    let val = |id| unsafe {
        let c = GetDlgItem(h, id);
        let ix = SendMessageW(c, CB_GETCURSEL, 0, 0).max(0) as usize;
        let n = SendMessageW(c, CB_GETLBTEXTLEN, ix, 0) as usize;
        let mut b = vec![0u16; n + 1];
        SendMessageW(c, CB_GETLBTEXT, ix, b.as_mut_ptr() as LPARAM);
        String::from_utf16_lossy(&b)
            .chars()
            .take_while(|x| x.is_ascii_digit())
            .collect::<String>()
            .parse::<i64>()
            .unwrap_or(0)
    };
    let hotkey=unsafe {read_edit(GetDlgItem(h,HOTKEY_EDIT))};
    if let Some(reason) = crate::hotkey::hotkey_rejection_reason(&hotkey) {
        unsafe { SetWindowTextW(GetDlgItem(h,STATUS),wide(reason).as_ptr()); }
        return;
    }
    if crate::hotkey::parse_hotkey(&hotkey).is_none() {
        unsafe {SetWindowTextW(GetDlgItem(h,STATUS),wide("단축키가 유효하지 않습니다. Ctrl/Alt/Shift/Win 중 하나와 사용 가능한 키를 입력하세요.").as_ptr());}
        return;
    }
    let p = crate::json::Value::obj(vec![
        ("sticky_popup", get(STICKY).into()),
        ("always_on_top", get(TOP).into()),
        ("auto_paste", get(PASTE).into()),
        ("excel_target_paste", get(EXCEL).into()),
        ("preview_images", get(PREVIEW).into()),
        ("watch_screenshots", get(SHOTS).into()),
        ("capture_text", get(CAP_TEXT).into()),
        ("capture_images", get(CAP_IMAGE).into()),
        ("capture_files", get(CAP_FILE).into()),
        ("hotkey", hotkey.into()),
        ("max_items", val(NUM).into()),
        ("max_item_mb", val(ITEM).into()),
        ("max_total_mb", val(TOTAL).into()),
        ("max_age_days", val(AGE).into()),
    ]);
    let (apps,folders) = unsafe {(read_list(GetDlgItem(h,APP_LIST)),read_list(GetDlgItem(h,FOLDER_LIST)))};
    if apps.iter().any(|x| x.is_empty() || x.len()>100 || !x.ends_with(".exe") || x.contains('\\') || x.contains('/')) {
        unsafe {SetWindowTextW(GetDlgItem(h,STATUS),wide("제외 앱은 경로가 아닌 실행 파일 이름(.exe)으로 입력하세요.").as_ptr());}
        return;
    }
    if folders.iter().any(|x| !std::path::Path::new(x).is_absolute() || x.contains("..") || x.starts_with("\\\\")) {
        unsafe {SetWindowTextW(GetDlgItem(h,STATUS),wide("폴더 제외는 UNC·상위 경로 없이 로컬 절대 경로로 입력하세요.").as_ptr());}
        return;
    }
    let mut fields = match p { crate::json::Value::Obj(v) => v, _ => unreachable!() };
    fields.push(("excluded_apps".into(), crate::json::Value::Arr(apps.iter().map(|x|x.as_str().into()).collect())));
    fields.push(("capture_dirs_off".into(), crate::json::Value::Arr(folders.iter().map(|x|x.as_str().into()).collect())));
    let p=crate::json::Value::Obj(fields);
    s.apply(&p);
    let autostart_result = crate::set_autostart(get(AUTOSTART));
    let external_pipe = std::env::args().collect::<Vec<_>>().windows(2)
        .find(|pair| pair[0] == "--pipe").map(|pair| pair[1].clone());
    let mut external_hotkey_conflict = false;
    let mut external_pipe_applied = false;
    let result = if LIVE.load(Ordering::Relaxed) {
        crate::update_settings(&p);
        Ok(())
    } else if let Some(pipe) = external_pipe {
        let request = crate::json::Value::obj(vec![
            ("cmd", "settings_set".into()), ("settings", p.clone()),
        ]);
        match crate::lifecycle_pipe_request(&pipe, &request) {
            Some(reply) if reply.bool("ok") == Some(true) => {
                external_hotkey_conflict = reply.bool("hotkey_ok") == Some(false);
                external_pipe_applied = true;
                Ok(())
            }
            _ => save(&s),
        }
    } else {
        save(&s)
    };
    let hotkey_conflict = external_hotkey_conflict || (LIVE.load(Ordering::Relaxed)
        && !crate::app().hotkey_ok.load(std::sync::atomic::Ordering::Relaxed));
    let msg = result
        .err()
        .or_else(|| autostart_result.err())
        .unwrap_or_else(|| if hotkey_conflict {
            "설정은 저장했지만 다른 프로그램과 단축키가 겹칩니다. 다른 조합을 지정해 주세요.".into()
        } else if LIVE.load(Ordering::Relaxed) || external_pipe_applied {
            "설정을 저장하고 실행 중인 몽땅클립보드에 적용했습니다.".into()
        } else {
            "설정을 저장했습니다. 에이전트가 실행 중이 아니므로 다음 시작부터 적용됩니다.".into()
        });
    unsafe {
        SetWindowTextW(GetDlgItem(h, STATUS), wide(&msg).as_ptr());
    }
}
unsafe fn read_edit(h: HWND) -> String {
    let n=unsafe{GetWindowTextLengthW(h)}.max(0) as usize;
    let mut b=vec![0u16;n+1];
    unsafe{GetWindowTextW(h,b.as_mut_ptr(),b.len() as i32)};
    String::from_utf16_lossy(&b[..n]).trim().to_string()
}
unsafe fn read_list(h: HWND)->Vec<String>{
    let n=unsafe{SendMessageW(h,LB_GETCOUNT,0,0)}.max(0) as usize;
    (0..n).filter_map(|i|{let len=unsafe{SendMessageW(h,LB_GETTEXTLEN,i,0)}.max(0) as usize;let mut b=vec![0u16;len+1];unsafe{SendMessageW(h,LB_GETTEXT,i,b.as_mut_ptr() as LPARAM)};Some(String::from_utf16_lossy(&b[..len]))}).collect()
}
unsafe fn edit_list(h:HWND, list_id:i32, add:bool){
    let edit=GetDlgItem(h,LIST_EDIT);let value=read_edit(edit);
    let list=GetDlgItem(h,list_id);
    if add {
        if value.is_empty() || value.contains('\0') || value.len()>1000 { SetWindowTextW(GetDlgItem(h,STATUS),wide("값을 입력하세요 (최대 1000자). ").as_ptr());return; }
        let v=if list_id==APP_LIST {value.to_lowercase()}else{value.to_lowercase()};
        if SendMessageW(list,LB_FINDSTRINGEXACT,usize::MAX,wide(&v).as_ptr() as LPARAM)>=0 { SetWindowTextW(GetDlgItem(h,STATUS),wide("이미 목록에 있습니다.").as_ptr());return; }
        if SendMessageW(list,LB_GETCOUNT,0,0)>=if list_id==APP_LIST{100}else{50} { SetWindowTextW(GetDlgItem(h,STATUS),wide("목록의 최대 개수에 도달했습니다.").as_ptr());return; }
        SendMessageW(list,LB_ADDSTRING,0,wide(&v).as_ptr() as LPARAM);
        SetWindowTextW(edit,wide("").as_ptr());
    }else{
        let ix=SendMessageW(list,LB_GETCURSEL,0,0);if ix>=0 {SendMessageW(list,LB_DELETESTRING,ix as usize,0);}
    }
}
unsafe fn add_folder_from_dialog(h: HWND) {
    let mut display = vec![0u16; 260];
    let title = wide("캡처를 끌 폴더 선택");
    let mut info = BrowseInfoW {
        owner: h, root: std::ptr::null_mut(), display_name: display.as_mut_ptr(),
        title: title.as_ptr(), flags: 0x0001 | 0x0040,
        callback: None, param: 0, image: 0,
    };
    let pidl = unsafe { SHBrowseForFolderW(&mut info) };
    if pidl.is_null() { return; }
    let mut path = vec![0u16; 32768];
    let ok = unsafe { SHGetPathFromIDListW(pidl, path.as_mut_ptr()) } != 0;
    unsafe { CoTaskMemFree(pidl); }
    if ok {
        let len = path.iter().position(|c| *c == 0).unwrap_or(path.len());
        let value = String::from_utf16_lossy(&path[..len]);
        unsafe { SetWindowTextW(GetDlgItem(h, LIST_EDIT), wide(&value).as_ptr()); edit_list(h, FOLDER_LIST, true); }
    }
}
unsafe extern "system" fn hotkey_capture_proc(edit: HWND, msg: u32, key: usize, lparam: isize, id: usize, data: usize) -> isize {
    if msg == WM_KEYDOWN {
        let vk = key as u32;
        if matches!(vk, 0x10 | 0x11 | 0x12 | 0x5b | 0x5c) {
            return unsafe { DefSubclassProc(edit, msg, key, lparam) };
        }
        let key_name = match vk {
            0x30..=0x39 | 0x41..=0x5a => (vk as u8 as char).to_string(),
            0x70..=0x87 => format!("F{}", vk - 0x70 + 1),
            _ => {
                unsafe { SetWindowTextW(GetDlgItem(data as HWND, STATUS), wide("문자·숫자 또는 F1~F24 키 조합을 입력해 주세요.").as_ptr()); }
                return 0;
            }
        };
        let mut parts: Vec<String> = Vec::new();
        for (vk, name) in [(0x11,"Ctrl"),(0x12,"Alt"),(0x10,"Shift"),(0x5b,"Win")] {
            if unsafe { GetKeyState(vk) } < 0 { parts.push(name.to_string()); }
        }
        if parts.is_empty() {
            unsafe { SetWindowTextW(GetDlgItem(data as HWND, STATUS), wide("단축키는 Ctrl, Alt, Shift 또는 Win 키와 함께 지정해 주세요.").as_ptr()); }
            return 0;
        }
        parts.push(key_name);
        let chord = parts.join("+");
        unsafe { SetWindowTextW(edit, wide(&chord).as_ptr()); }
        return 0;
    }
    unsafe { DefSubclassProc(edit, msg, key, lparam) }
}
unsafe extern "system" fn wndproc(h: HWND, m: u32, w: usize, _l: isize) -> isize {
    unsafe {
        match m {
            WM_CREATE => {
                build(h);
                let title_font = CreateFontW(24,0,0,0,700,0,0,0,1,0,0,5,0,wide("맑은 고딕").as_ptr());
                let heading_font = CreateFontW(21,0,0,0,700,0,0,0,1,0,0,5,0,wide("맑은 고딕").as_ptr());
                SendMessageW(GetDlgItem(h,1100),WM_SETFONT,title_font as usize,1);
                SendMessageW(GetDlgItem(h,1101),WM_SETFONT,heading_font as usize,1);
                SendMessageW(GetDlgItem(h,1103),WM_SETFONT,heading_font as usize,1);
                SendMessageW(GetDlgItem(h,1104),WM_SETFONT,heading_font as usize,1);
                SendMessageW(GetDlgItem(h,1105),WM_SETFONT,heading_font as usize,1);
                0
            }
            WM_COMMAND => {
                match (w & 0xffff) as i32 {
                    CONSENT => { EnableWindow(GetDlgItem(h, ENABLE),
                        (SendMessageW(GetDlgItem(h, CONSENT), BM_GETCHECK, 0, 0) == 1) as i32); },
                    APPLY => persist(h),
                    APP_ADD => edit_list(h,APP_LIST,true),
                    APP_DEL => edit_list(h,APP_LIST,false),
                    FOLDER_ADD => add_folder_from_dialog(h),
                    FOLDER_DEL => edit_list(h,FOLDER_LIST,false),
                    CANCEL => {
                        DestroyWindow(h);
                    }
                    ENABLE => {
                        if SendMessageW(GetDlgItem(h, CONSENT), BM_GETCHECK, 0, 0) != 1 {
                            SetWindowTextW(
                                GetDlgItem(h, STATUS),
                                wide("켜려면 먼저 동의 항목을 체크하세요.").as_ptr(),
                            );
                        } else {
                            enable(h)
                        }
                    }
                    _ => {}
                }
                0
            }
            WM_KEYDOWN if w as u32 == VK_ESCAPE as u32 => {
                DestroyWindow(h);
                0
            }
            DONE => {
                EnableWindow(GetDlgItem(h, ENABLE),
                    (SendMessageW(GetDlgItem(h, CONSENT), BM_GETCHECK, 0, 0) == 1) as i32);
                if w == 1 {
                    DestroyWindow(h);
                } else {
                    SetWindowTextW(
                        GetDlgItem(h, STATUS),
                        wide(if w==2 {"15초 안에 응답하지 않았습니다. 다시 시도해 주세요."} else {"몽땅클립보드를 켜지 못했습니다. 상태와 권한을 확인하고 다시 시도해 주세요."}).as_ptr(),
                    );
                }
                0
            }
            WM_CTLCOLORSTATIC => {
                let p = ui_theme::palette();
                SetBkMode(w as _, TRANSPARENT as i32);
                SetTextColor(w as _, p.text);
                CreateSolidBrush(p.bg) as isize
            }
            WM_CTLCOLORBTN => {
                let p = ui_theme::palette();
                SetBkMode(w as _, TRANSPARENT as i32);
                SetTextColor(w as _, p.text);
                CreateSolidBrush(p.bg) as isize
            }
            WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
                let p = ui_theme::palette();
                SetBkColor(w as _, p.bg);
                SetTextColor(w as _, p.text);
                CreateSolidBrush(p.bg) as isize
            }
            WM_ERASEBKGND => {
                let p = ui_theme::palette();
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(h, &mut r);
                let b = CreateSolidBrush(p.bg);
                FillRect(w as _, &r, b);
                DeleteObject(b);
                1
            }
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let dc = BeginPaint(h, &mut ps);
                let s = if LIVE.load(Ordering::Relaxed) {
                    crate::app().settings.lock().unwrap().clone()
                } else {
                    load()
                };
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(h, &mut r);
                paint_surface(dc, r.right, r.bottom, &s, ui_theme::palette());
                EndPaint(h, &ps);
                0
            }
            WM_DESTROY => {
                WINDOW.store(0, Ordering::Relaxed);
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(h, m, w as _, 0),
        }
    }
}
fn enable(h: HWND) {
    unsafe {
        EnableWindow(GetDlgItem(h, ENABLE), 0);
        SetWindowTextW(GetDlgItem(h, STATUS), wide("몽땅클립보드를 켜는 중입니다. 최대 15초 기다립니다…").as_ptr());
    }
    let hwnd = h as isize;
    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + enable_timeout();
        let result = std::env::current_exe()
            .ok()
            .and_then(|e| {
                std::process::Command::new(e)
                    .args(["--enable", "--consent"])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .ok()
            })
            .map(|mut c| {
                loop {
                    if std::time::Instant::now() >= deadline {
                        let _ = c.kill();
                        let _ = c.wait();
                        break classify_enable(false, true);
                    }
                    match c.try_wait() {
                        Ok(Some(s)) => break classify_enable(s.success(), false),
                        Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
                        Err(_) => {
                            let _ = c.kill();
                            break 0;
                        }
                    }
                }
            })
            .unwrap_or(0);
        unsafe {
            PostMessageW(hwnd as HWND, DONE, result, 0);
        }
    });
}
fn capture_png(dark: bool) {
    use windows_sys::Win32::Graphics::Gdi::*;
    let dir = std::env::var_os("KAGENT_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("kagent-capture"));
    let _ = std::fs::create_dir_all(&dir);
    let dpi=std::env::var("KAGENT_CAPTURE_DPI").ok().and_then(|x|x.parse::<u32>().ok()).unwrap_or(96).clamp(96,192);
    let scale=dpi as i32;
    let (w, h) = ((680*dpi/96) as u32,(1160*dpi/96) as u32);
    let mem = unsafe { CreateCompatibleDC(std::ptr::null_mut()) };
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [RGBQUAD {
            rgbBlue: 0,
            rgbGreen: 0,
            rgbRed: 0,
            rgbReserved: 0,
        }],
    };
    let mut bits = std::ptr::null_mut();
    let mut raw = vec![0u8; (w * h * 4) as usize];
    unsafe {
        let bmp = CreateDIBSection(
            mem,
            &info,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        );
        let old = SelectObject(mem, bmp);
        let palette = if dark {
            ui_theme::Palette::DARK
        } else {
            ui_theme::Palette::LIGHT
        };
        SetMapMode(mem,MM_ANISOTROPIC);
        SetWindowExtEx(mem,680,1160,std::ptr::null_mut());
        SetViewportExtEx(mem,680*scale/96,1160*scale/96,std::ptr::null_mut());
        paint_surface(mem,680,1160,&load(),palette);
        std::ptr::copy_nonoverlapping(bits as *const u8, raw.as_mut_ptr(), raw.len());
        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
    }
    for px in raw.chunks_exact_mut(4) {
        px.swap(0, 2);
        px[3] = 255;
    }
    let suffix=if dpi==96 {""} else if dpi==144 {"-150dpi"} else {"-scaled"};
    let path = dir.join(format!("kclip-settings-{}{}.png",if dark{"dark"}else{"light"},suffix));
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        if let Ok(mut wr) = e.write_header() {
            let _ = wr.write_image_data(&raw);
        }
    }
    let _ = std::fs::write(path, out);
}

/// 실제 창의 WM_PAINT와 캡처 모드가 함께 쓰는 화면 그리기 함수.
unsafe fn paint_surface(
    dc: windows_sys::Win32::Graphics::Gdi::HDC,
    w: i32,
    h: i32,
    s: &Settings,
    p: ui_theme::Palette,
) {
    use windows_sys::Win32::Graphics::Gdi::*;
    unsafe {
        let bg = CreateSolidBrush(p.bg);
        let all = RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        FillRect(dc, &all, bg);
        DeleteObject(bg);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, p.text);
        let font = CreateFontW(
            19,
            0,
            0,
            0,
            700,
            0,
            0,
            0,
            1,
            0,
            0,
            5,
            0,
            wide("맑은 고딕").as_ptr(),
        );
        let old_font = SelectObject(dc, font);
        let text = |x: i32, y: i32, value: &str| {
            let u = wide(value);
            TextOutW(dc, x, y, u.as_ptr(), u.len().saturating_sub(1) as i32);
        };
        let box_control = |r: RECT| {
            let fill = CreateSolidBrush(p.bg);
            FillRect(dc, &r, fill);
            DeleteObject(fill);
            let edge = CreateSolidBrush(p.border);
            FrameRect(dc, &r, edge);
            DeleteObject(edge);
        };
        let line = |y: i32| {
            MoveToEx(dc, 24, y, std::ptr::null_mut());
            LineTo(dc, w - 24, y);
        };
        let title_font = CreateFontW(27,0,0,0,700,0,0,0,1,0,0,5,0,wide("맑은 고딕").as_ptr());
        let heading_font = CreateFontW(23,0,0,0,700,0,0,0,1,0,0,5,0,wide("맑은 고딕").as_ptr());
        SelectObject(dc,title_font);
        text(28, 22, "몽땅클립보드 설정");
        SelectObject(dc,heading_font);
        text(
            28,
            52,
            "복사한 내용을 이 PC에 암호화해 보관하고 다시 붙여넣습니다.",
        );
        text(28, 82, "보관 설정");
        SelectObject(dc,old_font);
        let combos = [
            ("보관 개수", format!("{}개", s.max_items), 118),
            (
                "항목 하나 최대 크기",
                format!("{} MB", s.max_item_bytes / 1048576),
                154,
            ),
            (
                "전체 최대 크기",
                format!("{} MB", s.max_total_bytes / 1048576),
                190,
            ),
            (
                "보관 기간",
                format!("{}일", s.max_age_days.round() as i64),
                226,
            ),
        ];
        for (label, value, y) in combos {
            text(28, y + 3, label);
            let r = RECT {
                left: 285,
                top: y - 4,
                right: 545,
                bottom: y + 28,
            };
            box_control(r);
            text(294, y + 2, &value);
        }
        text(
            28,
            270,
            &format!("단축키: {} · 암호화 저장: 항상 켜짐", s.hotkey),
        );
        SelectObject(dc,heading_font);
        text(28, 310, "동작 설정");
        SelectObject(dc,old_font);
        let flags = [
            ("스티커 메모처럼 화면에 유지", s.sticky_popup, 334),
            ("항상 위", s.always_on_top, 370),
            ("고르면 바로 붙여넣기", s.auto_paste, 406),
            ("엑셀 표는 목표 결과로 붙이기", s.excel_target_paste, 442),
            ("그림 미리보기", s.preview_images, 478),
            ("화면캡처 폴더 감시", s.watch_screenshots, 514),
        ];
        for (label, on, y) in flags {
            box_control(RECT {
                left: 29,
                top: y + 3,
                right: 49,
                bottom: y + 23,
            });
            if on {
                let pen=CreatePen(PS_SOLID,2,p.toggle_on_text);
                let old_pen=SelectObject(dc,pen);
                MoveToEx(dc, 31, y + 13, std::ptr::null_mut());
                LineTo(dc, 36, y + 18);
                LineTo(dc, 46, y + 7);
                SelectObject(dc,old_pen);DeleteObject(pen);
            }
            text(54, y, label);
        }
        text(28,550,"캡처 종류");
        for (x,label,on) in [(28,"글",s.capture_text),(130,"그림",s.capture_images),(250,"파일",s.capture_files)] {
            box_control(RECT{left:x,top:574,right:x+20,bottom:594});
            if on {let pen=CreatePen(PS_SOLID,2,p.toggle_on_text);let old_pen=SelectObject(dc,pen);MoveToEx(dc,x+3,584,std::ptr::null_mut());LineTo(dc,x+8,589);LineTo(dc,x+17,577);SelectObject(dc,old_pen);DeleteObject(pen);}
            text(x+28,574,label);
        }
        SelectObject(dc,heading_font);
        text(28,634,"제외 앱");
        SelectObject(dc,old_font);
        box_control(RECT{left:28,top:660,right:470,bottom:724});
        for (i,row) in s.excluded_apps.iter().take(3).enumerate() { text(34,668+i as i32*18,row); }
        if s.excluded_apps.is_empty() { text(34,668,"(없음)"); }
        for (y,label) in [(660,"앱 추가"),(698,"앱 삭제"),(758,"폴더 추가"),(796,"폴더 삭제")] {
            box_control(RECT{left:486,top:y,right:576,bottom:y+30});
            text(494,y+7,label);
        }
        SelectObject(dc,heading_font);
        text(28,732,"폴더별 캡처 끄기");
        SelectObject(dc,old_font);
        box_control(RECT{left:28,top:758,right:470,bottom:822});
        for (i,row) in s.capture_dirs_off.iter().take(3).enumerate() { text(34,766+i as i32*18,row); }
        if s.capture_dirs_off.is_empty() { text(34,766,"(없음)"); }
        text(28,850,"추가할 앱 이름(.exe) 또는 폴더 경로");
        box_control(RECT{left:28,top:874,right:358,bottom:902});
        text(28,922,"단축키 (입력한 뒤 저장)");
        box_control(RECT{left:260,top:918,right:480,bottom:948});
        text(266,923,&s.hotkey);
        line(956);
        let auto = crate::autostart_enabled().unwrap_or(false);
        for (y,label,on) in [(964,"Windows 로그인 시 자동 실행",auto),(1034,"안내를 읽었고 몽땅클립보드 사용에 동의합니다",false)] {
            box_control(RECT{left:29,top:y+3,right:49,bottom:y+23});
            if on { let pen=CreatePen(PS_SOLID,2,p.toggle_on_text);let old_pen=SelectObject(dc,pen);MoveToEx(dc,31,y+13,std::ptr::null_mut());LineTo(dc,36,y+18);LineTo(dc,46,y+7);SelectObject(dc,old_pen);DeleteObject(pen); }
            text(58,y,label);
        }
        text(
            28,
            1002,
            "기록은 이 PC 사용자 폴더에 암호화되어 저장됩니다. 외부로 전송하지 않습니다.",
        );
        text(
            28,
            1066,
            "실행 중이면 저장 즉시 적용하고, 꺼져 있으면 다음 시작부터 적용합니다.",
        );
        let buttons = [
            (315, 1104, 405, 1136, "설정 저장"),
            (420, 1104, 560, 1136, "몽땅클립보드 켜기"),
            (575, 1104, 645, 1136, "닫기"),
        ];
        for (l, t, r, b, label) in buttons {
            box_control(RECT {
                left: l,
                top: t,
                right: r,
                bottom: b,
            });
            text(l + 8, t + 7, label);
        }
        SelectObject(dc, old_font);
        DeleteObject(title_font);
        DeleteObject(heading_font);
        DeleteObject(font);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout_scales_supported_dpi() {
        for d in [96, 120, 144, 192] {
            let l = layout(d);
            assert_eq!(l.width, 650 * d as i32 / 96);
            assert_eq!(l.height, 1160 * d as i32 / 96);
        }
    }
    #[test]
    fn rejects_invalid_values_through_store_validation() {
        let mut s = Settings::default();
        s.apply(&crate::json::Value::obj(vec![
            ("max_items", 5.into()),
            ("max_total_mb", 1.into()),
            ("max_age_days", 0.into()),
        ]));
        assert_eq!(
            (
                s.max_items,
                s.max_total_bytes / 1048576,
                s.max_age_days as i64
            ),
            (500, 200, 30)
        );
    }
    #[test]
    fn round_trip_isolated_settings_file() {
        let d = std::env::temp_dir().join(format!("kclip-roundtrip-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("settings.json");
        let mut s = Settings::default();
        s.apply(&crate::json::Value::obj(vec![
            ("max_items", 200.into()),
            ("watch_screenshots", false.into()),
        ]));
        s.save(&p.to_string_lossy()).unwrap();
        let t = Settings::load(&p.to_string_lossy());
        assert_eq!(t.max_items, 200);
        assert!(!t.watch_screenshots);
        let _ = std::fs::remove_dir_all(d);
    }
    #[test]
    fn enable_deadline_is_bounded() {
        assert_eq!(enable_timeout(), std::time::Duration::from_secs(15));
    }
    #[test]
    fn fake_enable_lifecycle_success_failure_and_timeout_are_terminal() {
        assert_eq!(classify_enable(true, false), 1);
        assert_eq!(classify_enable(false, false), 0);
        assert_eq!(classify_enable(false, true), 2);
    }
}

fn enable_timeout() -> std::time::Duration {
    std::time::Duration::from_secs(15)
}

fn classify_enable(success: bool, timed_out: bool) -> usize {
    if timed_out { 2 } else if success { 1 } else { 0 }
}
