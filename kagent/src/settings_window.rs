//! ?????? ?? ?. kagent ?? ??? ?? ?? ??.
use crate::{store::Settings, util::wide};
use std::sync::atomic::{AtomicIsize, Ordering};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, RECT},
    Graphics::Gdi::{
        BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, PAINTSTRUCT, SetBkColor,
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
const STATUS: i32 = 1030;
const DONE: u32 = WM_APP + 41;
static WINDOW: AtomicIsize = AtomicIsize::new(0);
static LIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
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
        height: s(900),
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
        if WINDOW.load(Ordering::Relaxed) != 0 {
            let h = WINDOW.load(Ordering::Relaxed) as HWND;
            ShowWindow(h, SW_RESTORE);
            SetForegroundWindow(h);
            return;
        }
        let inst = GetModuleHandleW(std::ptr::null());
        let cls = wide("HwpAutoDocFit.KAgentSettings");
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
            940 * dpi / 96,
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
            1100,
            "STATIC",
            "복사한 내용을 이 PC에 암호화해 보관하고 다시 붙여넣습니다.",
            28,
            24,
            600,
            36,
            0,
        );
        child(h, 1101, "STATIC", "보관 설정", 28, 78, 590, 30, 0);
        combo(
            h,
            NUM,
            "보관 개수",
            &[20, 50, 100, 200, 500, 1000],
            s.max_items,
            "개",
            116,
        );
        combo(
            h,
            ITEM,
            "항목 하나 최대 크기",
            &[1, 5, 10, 20, 50],
            s.max_item_bytes / 1048576,
            "MB",
            156,
        );
        combo(
            h,
            TOTAL,
            "전체 최대 크기",
            &[50, 100, 200, 500, 1000],
            s.max_total_bytes / 1048576,
            "MB",
            196,
        );
        combo(
            h,
            AGE,
            "보관 기간",
            &[7, 30, 90, 365],
            s.max_age_days.round() as i64,
            "일",
            236,
        );
        child(
            h,
            1102,
            "STATIC",
            &format!("단축키: {} · 암호화 저장: 항상 켜짐", s.hotkey),
            28,
            278,
            590,
            30,
            0,
        );
        child(h, 1103, "STATIC", "동작 설정", 28, 322, 590, 30, 0);
        checkbox(
            h,
            STICKY,
            "스티커 메모처럼 화면에 유지",
            s.sticky_popup,
            354,
        );
        checkbox(h, TOP, "항상 위", s.always_on_top, 390);
        checkbox(h, PASTE, "고르면 바로 붙여넣기", s.auto_paste, 426);
        checkbox(
            h,
            EXCEL,
            "엑셀 표는 목표 결과로 붙이기",
            s.excel_target_paste,
            462,
        );
        checkbox(h, PREVIEW, "그림 미리보기", s.preview_images, 498);
        checkbox(h, SHOTS, "화면캡처 폴더 감시", s.watch_screenshots, 534);
        child(
            h,
            1107,
            "STATIC",
            "캡처 종류: 글 · 그림 · 파일 (모두 허용)",
            28,
            568,
            610,
            28,
            0,
        );
        let exc = if s.excluded_apps.is_empty() {
            "제외 앱: 없음".to_string()
        } else {
            format!("제외 앱: {}", s.excluded_apps.join(", "))
        };
        child(h, 1104, "STATIC", &exc, 28, 600, 610, 30, 0);
        let off = if s.capture_dirs_off.is_empty() {
            "폴더별 캡처 끄기: 없음".to_string()
        } else {
            format!("폴더별 캡처 끄기: {}", s.capture_dirs_off.join(", "))
        };
        child(h, 1105, "STATIC", &off, 28, 632, 610, 30, 0);
        child(
            h,
            1106,
            "STATIC",
            "기록은 이 PC 사용자 폴더에 암호화되어 저장됩니다. 외부로 전송하지 않습니다.",
            28,
            692,
            610,
            30,
            0,
        );
        checkbox(
            h,
            AUTOSTART,
            "Windows 로그인 시 자동 실행",
            crate::autostart_enabled().unwrap_or(false),
            660,
        );
        checkbox(
            h,
            CONSENT,
            "안내를 읽었고 몽땅클립보드 사용에 동의합니다",
            false,
            728,
        );
        child(
            h,
            STATUS,
            "STATIC",
            "설정은 저장 즉시 적용됩니다. 켜기에는 동의가 필요합니다.",
            28,
            772,
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
            820,
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
            820,
            155,
            32,
            WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
        );
        child(h, CANCEL, "BUTTON", "닫기", 575, 820, 70, 32, WS_TABSTOP);
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
    let p = crate::json::Value::obj(vec![
        ("sticky_popup", get(STICKY).into()),
        ("always_on_top", get(TOP).into()),
        ("auto_paste", get(PASTE).into()),
        ("excel_target_paste", get(EXCEL).into()),
        ("preview_images", get(PREVIEW).into()),
        ("watch_screenshots", get(SHOTS).into()),
        ("max_items", val(NUM).into()),
        ("max_item_mb", val(ITEM).into()),
        ("max_total_mb", val(TOTAL).into()),
        ("max_age_days", val(AGE).into()),
    ]);
    s.apply(&p);
    let autostart_result = crate::set_autostart(get(AUTOSTART));
    let result = if LIVE.load(Ordering::Relaxed) {
        crate::update_settings(&p);
        Ok(())
    } else {
        save(&s)
    };
    let msg = result
        .err()
        .or_else(|| autostart_result.err())
        .unwrap_or_else(|| "설정을 저장했습니다.".into());
    unsafe {
        SetWindowTextW(GetDlgItem(h, STATUS), wide(&msg).as_ptr());
    }
}
unsafe extern "system" fn wndproc(h: HWND, m: u32, w: usize, _l: isize) -> isize {
    unsafe {
        match m {
            WM_CREATE => {
                build(h);
                0
            }
            WM_COMMAND => {
                match (w & 0xffff) as i32 {
                    APPLY => persist(h),
                    CANCEL => {
                        DestroyWindow(h);
                    }
                    ENABLE => {
                        if SendMessageW(GetDlgItem(h, CONSENT), BM_GETCHECK, 0, 0) != 1 {
                            SetWindowTextW(
                                GetDlgItem(h, STATUS),
                                wide("????? ?? ???? ??? ???.").as_ptr(),
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
                if w == 1 {
                    DestroyWindow(h);
                } else {
                    SetWindowTextW(
                        GetDlgItem(h, STATUS),
                        wide("??? ????? 15? ??? ?????. ??? ??? ? ?? ??? ???.").as_ptr(),
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
                        break 2usize;
                    }
                    match c.try_wait() {
                        Ok(Some(s)) => break if s.success() { 1 } else { 0 },
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
    let (w, h) = (680u32, 900u32);
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
        paint_surface(mem, w as i32, h as i32, &load(), palette);
        std::ptr::copy_nonoverlapping(bits as *const u8, raw.as_mut_ptr(), raw.len());
        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
    }
    for px in raw.chunks_exact_mut(4) {
        px.swap(0, 2);
        px[3] = 255;
    }
    let path = dir.join(if dark {
        "kclip-settings-dark.png"
    } else {
        "kclip-settings-light.png"
    });
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
            400,
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
        text(28, 22, "몽땅클립보드 설정");
        text(
            28,
            52,
            "복사한 내용을 이 PC에 암호화해 보관하고 다시 붙여넣습니다.",
        );
        text(28, 83, "보관 설정");
        let combos = [
            ("보관 개수", format!("{}개", s.max_items), 116),
            (
                "항목 하나 최대 크기",
                format!("{} MB", s.max_item_bytes / 1048576),
                156,
            ),
            (
                "전체 최대 크기",
                format!("{} MB", s.max_total_bytes / 1048576),
                196,
            ),
            (
                "보관 기간",
                format!("{}일", s.max_age_days.round() as i64),
                236,
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
            280,
            &format!("단축키: {} · 암호화 저장: 항상 켜짐", s.hotkey),
        );
        text(28, 322, "동작 설정");
        let flags = [
            ("스티커 메모처럼 화면에 유지", s.sticky_popup, 354),
            ("항상 위", s.always_on_top, 390),
            ("고르면 바로 붙여넣기", s.auto_paste, 426),
            ("엑셀 표는 목표 결과로 붙이기", s.excel_target_paste, 462),
            ("그림 미리보기", s.preview_images, 498),
            ("화면캡처 폴더 감시", s.watch_screenshots, 534),
        ];
        for (label, on, y) in flags {
            box_control(RECT {
                left: 29,
                top: y + 3,
                right: 45,
                bottom: y + 19,
            });
            if on {
                MoveToEx(dc, 32, y + 11, std::ptr::null_mut());
                LineTo(dc, 37, y + 16);
                LineTo(dc, 43, y + 6);
            }
            text(54, y, label);
        }
        text(28, 568, "캡처 종류: 글 · 그림 · 파일 (모두 허용)");
        let apps = if s.excluded_apps.is_empty() {
            "없음".to_string()
        } else {
            s.excluded_apps.join(", ")
        };
        let folders = if s.capture_dirs_off.is_empty() {
            "없음".to_string()
        } else {
            s.capture_dirs_off.join(", ")
        };
        text(
            28,
            600,
            &format!(
                "제외 앱 (표시 전용): {}{}",
                apps.chars().take(60).collect::<String>(),
                if apps.chars().count() > 60 { "…" } else { "" }
            ),
        );
        text(
            28,
            632,
            &format!(
                "폴더별 캡처 끄기 (표시 전용): {}{}",
                folders.chars().take(60).collect::<String>(),
                if folders.chars().count() > 60 {
                    "…"
                } else {
                    ""
                }
            ),
        );
        line(650);
        let auto = crate::autostart_enabled().unwrap_or(false);
        text(
            28,
            664,
            if auto {
                "☑ Windows 로그인 시 자동 실행"
            } else {
                "□ Windows 로그인 시 자동 실행"
            },
        );
        text(
            28,
            696,
            "기록은 이 PC 사용자 폴더에 암호화되어 저장됩니다. 외부로 전송하지 않습니다.",
        );
        text(28, 732, "□ 안내를 읽었고 몽땅클립보드 사용에 동의합니다");
        text(
            28,
            776,
            "설정은 저장 즉시 적용됩니다. 켜기에는 동의가 필요합니다.",
        );
        let buttons = [
            (300, 820, 400, 852, "설정 저장"),
            (410, 820, 565, 852, "몽땅클립보드 켜기"),
            (575, 820, 645, 852, "닫기"),
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
            assert_eq!(l.height, 900 * d as i32 / 96);
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
}

fn enable_timeout() -> std::time::Duration {
    std::time::Duration::from_secs(15)
}
