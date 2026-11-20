#![windows_subsystem = "windows"]
//! 한글 IME 키 순서 탐침. 입력칸 2개(A·B)와 로그 창을 가진 작은 창을 띄우고, 입력칸이 받는
//! WM_KEYDOWN(VK_PROCESSKEY 포함)·WM_IME_*·WM_CHAR·포커스 메시지를 타임스탬프와 함께
//! `%TEMP%\outliner-ime-probe\ime-probe-<날짜>-<시각>.log` 에 기록한다(UTF-8, BOM 없음).
//! 키 입력은 모두 원래 처리(EDIT 기본 동작)로 넘기므로 실제 편집 때와 같은 순서가 기록된다.
//! F5 = 시나리오 번호 표시줄 기록, F6 = ImmNotifyIME(CPS_COMPLETE) 시험(U2).
use outliner_win::paint::wide;
use outliner_win::probe_fmt::*;
use std::io::Write;
use std::ptr::{null, null_mut};
use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::Ime::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const EM_LINESCROLL_MSG: u32 = 0x00B6;
const MAX_TAIL: usize = 400;
const WM_PROBE_AFTER_PUMP: u32 = WM_APP + 1;

struct Logger {
    file: Option<std::fs::File>,
    path: String,
    start: i64,
    freq: i64,
    tail: Vec<String>,
    scenario: u32,
}

static LOGGER: Mutex<Option<Logger>> = Mutex::new(None);
static LOG_HWND: AtomicUsize = AtomicUsize::new(0);
static MAIN_HWND: AtomicUsize = AtomicUsize::new(0);
// 입력칸 A, B, 로그 창의 원래 창 프로시저와 핸들.
static ORIG: [AtomicIsize; 3] = [AtomicIsize::new(0), AtomicIsize::new(0), AtomicIsize::new(0)];
static HANDLES: [AtomicUsize; 3] = [AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0)];
const SRC: [&str; 3] = ["EDIT_A", "EDIT_B", "LOG"];

unsafe fn now_ms(l: &Logger) -> f64 {
    unsafe {
        let mut t = 0i64;
        QueryPerformanceCounter(&mut t);
        (t - l.start) as f64 * 1000.0 / l.freq as f64
    }
}

fn log(src: &str, body: &str) {
    unsafe {
        let mut guard = LOGGER.lock().unwrap_or_else(|e| e.into_inner());
        let Some(l) = guard.as_mut() else { return };
        let mut st: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut st);
        let line = log_line((st.wHour, st.wMinute, st.wSecond, st.wMilliseconds), now_ms(l), src, body);
        if let Some(f) = l.file.as_mut() {
            let _ = writeln!(f, "{line}");
            let _ = f.flush();
        }
        l.tail.push(line);
        if l.tail.len() > MAX_TAIL {
            l.tail.remove(0);
        }
        let log_hwnd = LOG_HWND.load(Ordering::Relaxed) as HWND;
        if !log_hwnd.is_null() {
            let text = l.tail.join("\r\n");
            SetWindowTextW(log_hwnd, wide(&text).as_ptr());
            SendMessageW(log_hwnd, EM_LINESCROLL_MSG, 0, 100_000);
        }
    }
}

unsafe fn edit_text(hwnd: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(hwnd);
        let mut buf = vec![0u16; n as usize + 1];
        let got = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..got.max(0) as usize])
    }
}

/// 조합 중 글자(GCS_COMPSTR 또는 GCS_RESULTSTR)를 읽는다. 컨텍스트가 없으면 None.
unsafe fn ime_string(hwnd: HWND, index: u32) -> Option<String> {
    unsafe {
        let imc = ImmGetContext(hwnd);
        if imc.is_null() {
            return None;
        }
        let bytes = ImmGetCompositionStringW(imc, index, null_mut(), 0);
        let mut out = String::new();
        if bytes > 0 {
            let mut buf = vec![0u16; bytes as usize / 2 + 1];
            let n = ImmGetCompositionStringW(imc, index, buf.as_mut_ptr().cast(), bytes as u32);
            out = String::from_utf16_lossy(&buf[..(n.max(0) as usize / 2)]);
        }
        ImmReleaseContext(hwnd, imc);
        Some(out)
    }
}

unsafe fn comp_len(hwnd: HWND) -> String {
    unsafe {
        match ime_string(hwnd, GCS_COMPSTR) {
            Some(s) => format!("compLen={}", s.chars().count()),
            None => "compLen=?(IME 컨텍스트 없음)".into(),
        }
    }
}

unsafe fn mods() -> String {
    unsafe {
        let down = |vk: u16| GetKeyState(vk as i32) < 0;
        let mut s = String::new();
        if down(VK_SHIFT) {
            s.push('S');
        }
        if down(VK_CONTROL) {
            s.push('C');
        }
        if down(VK_MENU) {
            s.push('A');
        }
        if s.is_empty() { "-".into() } else { s }
    }
}

/// 핵심 지점에서 EDIT 가 가진 글을 함께 남긴다(조합 확정이 글에 언제 반영되는지 보려는 것: U2).
fn quote(s: &str) -> String {
    format!("{s:?}")
}

unsafe fn describe(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<String> {
    unsafe {
        let name = msg_name(msg)?;
        let body = match msg {
            WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP => {
                let real = if wp as u32 == 0xE5 { format!(" real={}", vk_name(ImmGetVirtualKey(hwnd))) } else { String::new() };
                let nav = matches!(wp as u16, VK_RETURN | VK_TAB | VK_UP | VK_DOWN | VK_LEFT | VK_RIGHT | VK_ESCAPE | VK_BACK)
                    || (wp as u32 == 0xE5);
                let text = if nav && (msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN) { format!(" text={}", quote(&edit_text(hwnd))) } else { String::new() };
                format!("{name} vk={}{real} mods={} {} {}{text}", vk_name(wp as u32), mods(), key_lparam(lp), comp_len(hwnd))
            }
            WM_CHAR | WM_SYSCHAR | WM_IME_CHAR => {
                format!("{name} {} {} text_now={}", char_text(wp as u32), key_lparam(lp), quote(&edit_text(hwnd)))
            }
            WM_IME_STARTCOMPOSITION => format!("{name} text_now={}", quote(&edit_text(hwnd))),
            WM_IME_ENDCOMPOSITION => format!("{name} text_now={} {}", quote(&edit_text(hwnd)), comp_len(hwnd)),
            WM_IME_COMPOSITION => {
                let flags = lp as u32;
                let comp = if flags & GCS_COMPSTR != 0 { format!(" comp={}", quote(&ime_string(hwnd, GCS_COMPSTR).unwrap_or_default())) } else { String::new() };
                let res = if flags & GCS_RESULTSTR != 0 { format!(" result={}", quote(&ime_string(hwnd, GCS_RESULTSTR).unwrap_or_default())) } else { String::new() };
                format!("{name} flags={}{comp}{res} wParam={} text_now={}", gcs_flags(flags), char_text(wp as u32), quote(&edit_text(hwnd)))
            }
            WM_IME_NOTIFY => format!("{name} {} lParam=0x{:X}", imn_name(wp), lp),
            WM_IME_SETCONTEXT => format!("{name} active={} lParam=0x{:X}", wp, lp),
            WM_SETFOCUS | WM_KILLFOCUS => format!("{name} {} text={}", comp_len(hwnd), quote(&edit_text(hwnd))),
            WM_LBUTTONDOWN | WM_LBUTTONUP => format!("{name} x={} y={} {}", (lp & 0xFFFF) as i16, ((lp >> 16) & 0xFFFF) as i16, comp_len(hwnd)),
            _ => format!("{name} wParam=0x{wp:X} lParam=0x{lp:X}"),
        };
        Some(body)
    }
}

unsafe fn scenario_marker() {
    let mut guard = LOGGER.lock().unwrap_or_else(|e| e.into_inner());
    let n = guard.as_mut().map(|l| {
        l.scenario += 1;
        l.scenario
    });
    drop(guard);
    if let Some(n) = n {
        log("MARK", &format!("=============== 시나리오 {n} 시작 (F5) ==============="));
    }
}

/// F6: 조합 강제 확정(ImmNotifyIME CPS_COMPLETE) 직후 글이 바로 바뀌는지(U2).
unsafe fn complete_test(hwnd: HWND, src: &str) {
    unsafe {
        let before = edit_text(hwnd);
        let comp_before = ime_string(hwnd, GCS_COMPSTR).unwrap_or_default();
        let imc = ImmGetContext(hwnd);
        if imc.is_null() {
            log(src, "ImmNotifyIME 시험: IME 컨텍스트 없음");
            return;
        }
        let ret = ImmNotifyIME(imc, NI_COMPOSITIONSTR, CPS_COMPLETE, 0);
        ImmReleaseContext(hwnd, imc);
        log(
            src,
            &format!(
                "ImmNotifyIME(CPS_COMPLETE) ret={ret} before_text={} before_comp={} right_after_text={} right_after_{}",
                quote(&before),
                quote(&comp_before),
                quote(&edit_text(hwnd)),
                comp_len(hwnd)
            ),
        );
        PostMessageW(MAIN_HWND.load(Ordering::Relaxed) as HWND, WM_PROBE_AFTER_PUMP, hwnd as usize, 0);
    }
}

unsafe extern "system" fn sub_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        let idx = (0..3).find(|&i| HANDLES[i].load(Ordering::Relaxed) as HWND == hwnd).unwrap_or(2);
        let src = SRC[idx];
        if let Some(body) = describe(hwnd, msg, wp, lp) {
            log(src, &body);
        }
        if msg == WM_KEYDOWN {
            let eff = if wp as u32 == 0xE5 { ImmGetVirtualKey(hwnd) as u16 } else { wp as u16 };
            if eff == VK_F5 {
                scenario_marker();
                return 0;
            }
            if eff == VK_F6 {
                complete_test(hwnd, src);
                return 0;
            }
        }
        let prev: WNDPROC = std::mem::transmute::<isize, WNDPROC>(ORIG[idx].load(Ordering::Relaxed));
        let result = CallWindowProcW(prev, hwnd, msg, wp, lp);
        if msg == WM_IME_ENDCOMPOSITION || msg == WM_IME_COMPOSITION {
            // 기본 처리까지 마친 직후의 글(결과 글자가 EDIT 에 들어갔는지).
            log(src, &format!("  └ {} 처리 후 text={} {}", msg_name(msg).unwrap_or("?"), quote(&edit_text(hwnd)), comp_len(hwnd)));
        }
        result
    }
}

unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_SIZE => {
                let (w, h) = ((lp & 0xFFFF) as i32, ((lp >> 16) & 0xFFFF) as i32);
                let dpi = GetDpiForWindow(hwnd).max(96) as i32;
                let s = |v: i32| v * dpi / 96;
                let top = s(96);
                let row = s(34);
                let (a, b, l) = (HANDLES[0].load(Ordering::Relaxed) as HWND, HANDLES[1].load(Ordering::Relaxed) as HWND, HANDLES[2].load(Ordering::Relaxed) as HWND);
                let label = MAIN_LABEL.load(Ordering::Relaxed) as HWND;
                MoveWindow(label, s(8), s(6), w - s(16), top - s(10), 1);
                MoveWindow(a, s(8), top, w - s(16), row, 1);
                MoveWindow(b, s(8), top + row + s(6), w - s(16), row, 1);
                MoveWindow(l, s(8), top + 2 * row + s(16), w - s(16), (h - (top + 2 * row + s(24))).max(s(40)), 1);
                0
            }
            WM_PROBE_AFTER_PUMP => {
                let target = wp as HWND;
                log("MAIN", &format!("(메시지 펌프 한 바퀴 뒤) text={} {}", quote(&edit_text(target)), comp_len(target)));
                0
            }
            WM_SETFOCUS => {
                SetFocus(HANDLES[0].load(Ordering::Relaxed) as HWND);
                0
            }
            WM_CLOSE => {
                let focus = GetFocus();
                log("MAIN", &format!("WM_CLOSE (포커스 창 {})", if focus.is_null() { "없음".into() } else { comp_len(focus) }));
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => {
                if let Some(body) = describe(hwnd, msg, wp, lp) {
                    if matches!(msg, WM_ACTIVATE | WM_LBUTTONDOWN | WM_LBUTTONUP) {
                        log("MAIN", &body);
                    }
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
        }
    }
}

static MAIN_LABEL: AtomicUsize = AtomicUsize::new(0);

fn open_logger() -> Logger {
    unsafe {
        let mut st: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut st);
        let dir = std::env::temp_dir().join("outliner-ime-probe");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!(
            "ime-probe-{:04}{:02}{:02}-{:02}{:02}{:02}.log",
            st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
        ));
        let (mut start, mut freq) = (0i64, 0i64);
        QueryPerformanceCounter(&mut start);
        QueryPerformanceFrequency(&mut freq);
        Logger { file: std::fs::File::create(&path).ok(), path: path.display().to_string(), start, freq: freq.max(1), tail: Vec::new(), scenario: 0 }
    }
}

fn main() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let logger = open_logger();
        let path = logger.path.clone();
        *LOGGER.lock().unwrap_or_else(|e| e.into_inner()) = Some(logger);

        let hinst = GetModuleHandleW(null());
        let class = wide("OutlinerImeProbe");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(main_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: null_mut(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_BTNFACE + 1) as HBRUSH,
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(0, class.as_ptr(), wide("아웃라이너 한글 입력(IME) 순서 탐침").as_ptr(), WS_OVERLAPPEDWINDOW, CW_USEDEFAULT, CW_USEDEFAULT, 980, 720, null_mut(), null_mut(), hinst, null());
        if hwnd.is_null() {
            return;
        }
        MAIN_HWND.store(hwnd as usize, Ordering::Relaxed);
        let dpi = GetDpiForWindow(hwnd).max(96) as i32;
        let font = CreateFontW(-(16 * dpi / 96), 0, 0, 0, 400, 0, 0, 0, DEFAULT_CHARSET as u32, OUT_DEFAULT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32, CLEARTYPE_QUALITY as u32, (DEFAULT_PITCH | FF_DONTCARE) as u32, wide("Malgun Gothic").as_ptr());
        let child = |class: &str, text: &str, style: u32| {
            let h = CreateWindowExW(0, wide(class).as_ptr(), wide(text).as_ptr(), WS_CHILD | WS_VISIBLE | style, 0, 0, 10, 10, hwnd, null_mut(), hinst, null());
            SendMessageW(h, WM_SETFONT, font as usize, 1);
            h
        };
        let help = format!(
            "한글 입력 후 Enter·Tab·방향키·Alt+↑↓·Backspace 를 눌러 보세요. 입력칸 A↔B 클릭, 로그 창 클릭, Alt+Tab, 창 닫기도 시험합니다.\r\n\
             F5 = 시나리오 시작 표시(조합이 끝난 상태에서 누른 뒤 시나리오 시작) · F6 = 조합 강제 확정 시험.\r\n\
             로그 파일: {path}"
        );
        let label = child("STATIC", &help, 0);
        let a = child("EDIT", "", (WS_BORDER | ES_AUTOHSCROLL as u32) as u32);
        let b = child("EDIT", "", (WS_BORDER | ES_AUTOHSCROLL as u32) as u32);
        let l = child("EDIT", "", (WS_BORDER | WS_VSCROLL | ES_MULTILINE as u32 | ES_READONLY as u32 | ES_AUTOVSCROLL as u32) as u32);
        MAIN_LABEL.store(label as usize, Ordering::Relaxed);
        LOG_HWND.store(l as usize, Ordering::Relaxed);
        for (i, h) in [a, b, l].into_iter().enumerate() {
            HANDLES[i].store(h as usize, Ordering::Relaxed);
            let prev = SetWindowLongPtrW(h, GWLP_WNDPROC, sub_proc as *const () as isize);
            ORIG[i].store(prev, Ordering::Relaxed);
        }
        log("MAIN", &format!("탐침 시작. 로그 파일: {path}"));
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut r);
        SendMessageW(hwnd, WM_SIZE, 0, ((r.bottom as isize) << 16) | r.right as isize);
        ShowWindow(hwnd, SW_SHOW);
        SetFocus(a);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        log("MAIN", "탐침 종료");
    }
}
