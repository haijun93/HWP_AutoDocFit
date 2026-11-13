//! Win32 입력/IME + GDI 이중 버퍼. 검색 스레드는 이벤트로만 깨며 창 상태는 step으로만 바꾼다.
#[path = "window_probe.rs"]
mod probe;
use ktm::{
    backend::{Backend, Row},
    settings::Settings,
    tm_state::{self, Effect, Event, Phase, State, Tab},
    tray,
};
use std::{
    cell::RefCell,
    path::PathBuf,
    ptr::{null, null_mut},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Accessibility::*,
        HiDpi::*,
        Input::{Ime::*, KeyboardAndMouse::*},
        WindowsAndMessaging::*,
    },
};
const READY: u32 = WM_APP + 21;
const CLASS: &str = "HwpAutoDocFit.MongttangSearch";
type Job = (u64, String, bool, Instant, Arc<AtomicBool>);
type Answer = (u64, Result<Vec<Row>, String>, Instant);
struct Ui {
    state: State,
    rows: Vec<Row>,
    edit: HWND,
    font: HFONT,
    brush: HBRUSH,
    theme: String,
    tx: Arc<(Mutex<Option<Job>>, Condvar)>,
    rx: mpsc::Receiver<Answer>,
    status: mpsc::Sender<String>,
    top: usize,
    timing: Option<Instant>,
    hotkey_error: String,
    cancel: Arc<AtomicBool>,
    settings_path: PathBuf,
}
thread_local! {static UI:RefCell<Option<Ui>>=const{RefCell::new(None)};}
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn with<R>(f: impl FnOnce(&Ui) -> R) -> R {
    UI.with(|u| f(u.borrow().as_ref().unwrap()))
}
fn edit<R>(f: impl FnOnce(&mut Ui) -> R) -> R {
    UI.with(|u| f(u.borrow_mut().as_mut().unwrap()))
}
fn scale(h: HWND, n: i32) -> i32 {
    unsafe { n * GetDpiForWindow(h) as i32 / 96 }
}
fn colors() -> (u32, u32, u32, u32) {
    with(|u| unsafe {
        let mut hc: HIGHCONTRASTW = std::mem::zeroed();
        hc.cbSize = std::mem::size_of_val(&hc) as u32;
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            hc.cbSize,
            (&mut hc as *mut HIGHCONTRASTW).cast(),
            0,
        );
        if hc.dwFlags & HCF_HIGHCONTRASTON != 0
            || u.theme == "contrast"
            || u.theme == "high-contrast"
        {
            (
                GetSysColor(COLOR_WINDOW),
                GetSysColor(COLOR_WINDOWTEXT),
                GetSysColor(COLOR_HIGHLIGHT),
                GetSysColor(COLOR_HIGHLIGHTTEXT),
            )
        } else if u.theme == "light" {
            (0xffffff, 0x222222, 0xf6e5d5, 0x663300)
        } else {
            (0x202020, 0xeeeeee, 0x604020, 0xffffff)
        }
    })
}
fn event(h: HWND, e: Event) {
    let effects = edit(|u| {
        if matches!(
            &e,
            Event::Input(_) | Event::SwitchTab(_) | Event::Hide | Event::Submit
        ) {
            u.cancel.store(true, Ordering::Relaxed);
        }
        let (s, fx) = tm_state::step(&u.state, e);
        u.state = s;
        if u.state.result_count == 0 {
            u.rows.clear();
            u.top = 0;
        }
        fx
    });
    for fx in effects {
        unsafe {
            match fx {
                Effect::Show => {
                    let mut p = POINT { x: 0, y: 0 };
                    GetCursorPos(&mut p);
                    let mon = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
                    let mut mi: MONITORINFO = std::mem::zeroed();
                    mi.cbSize = std::mem::size_of_val(&mi) as u32;
                    GetMonitorInfoW(mon, &mut mi);
                    let w = scale(h, 700).min(mi.rcWork.right - mi.rcWork.left);
                    let hh = scale(h, 490).min(mi.rcWork.bottom - mi.rcWork.top);
                    SetWindowPos(
                        h,
                        HWND_TOPMOST,
                        (p.x + 12).min(mi.rcWork.right - w).max(mi.rcWork.left),
                        (p.y + 12).min(mi.rcWork.bottom - hh).max(mi.rcWork.top),
                        w,
                        hh,
                        SWP_SHOWWINDOW,
                    );
                    SetForegroundWindow(h);
                    SetFocus(with(|u| u.edit));
                }
                Effect::Hide => {
                    ShowWindow(h, SW_HIDE);
                }
                Effect::Search {
                    generation,
                    query,
                    tab,
                } => {
                    let tx = with(|u| u.tx.clone());
                    let cancel = Arc::new(AtomicBool::new(false));
                    edit(|u| u.cancel = cancel.clone());
                    *tx.0.lock().unwrap() = Some((
                        generation,
                        query,
                        tab == Tab::Hierarchy,
                        Instant::now(),
                        cancel,
                    ));
                    tx.1.notify_one();
                }
                Effect::Open { index, folder } => {
                    let path = with(|u| u.rows.get(index).map(|r| r.path.clone()));
                    if let Some(p) = path {
                        match ktm::open::open(&p, folder) {
                            Ok(()) => event(h, Event::Hide),
                            Err(message) => event(h, Event::OpenFailed(message)),
                        }
                    }
                }
            }
        }
    }
    edit(|u| {
        if let Some(index) = u.state.selected {
            let mut rc: RECT = unsafe { std::mem::zeroed() };
            unsafe {
                GetClientRect(h, &mut rc);
            }
            let count = ((rc.bottom - scale(h, 148)) / scale(h, 76)).max(1) as usize;
            if index < u.top {
                u.top = index;
            } else if index >= u.top + count {
                u.top = index + 1 - count;
            }
        }
    });
    let (s, path) = with(|u| {
        (
            serde_json::json!({"pid":std::process::id(),"state":format!("{:?}",u.state.phase),"generation":u.state.generation,"results":u.state.result_count,"error":u.state.error,"hotkey_error":u.hotkey_error}),
            u.status.clone(),
        )
    });
    let _ = path.send(s.to_string());
    unsafe {
        InvalidateRect(h, null(), 0);
    }
}
fn key(h: HWND, key: usize) -> bool {
    unsafe {
        let ed = with(|u| u.edit);
        let imc = ImmGetContext(ed);
        let composing = if !imc.is_null() {
            let n = ImmGetCompositionStringW(imc, GCS_COMPSTR, null_mut(), 0);
            ImmReleaseContext(ed, imc);
            n > 0
        } else {
            false
        };
        if composing {
            return false;
        }
        let ctrl = GetKeyState(VK_CONTROL as i32) < 0;
        let shift = GetKeyState(VK_SHIFT as i32) < 0;
        match key as u16 {
            VK_ESCAPE => event(h, Event::Hide),
            VK_RETURN => {
                if ctrl {
                    event(h, Event::Open { folder: true })
                } else {
                    event(h, Event::Submit)
                }
            }
            VK_DOWN => event(h, Event::MoveSelection(1)),
            VK_UP => event(h, Event::MoveSelection(-1)),
            VK_NEXT => event(h, Event::MoveSelection(5)),
            VK_PRIOR => event(h, Event::MoveSelection(-5)),
            VK_TAB => {
                let tab = with(|u| {
                    if u.state.tab == Tab::Body {
                        Tab::Hierarchy
                    } else {
                        Tab::Body
                    }
                });
                event(h, Event::SwitchTab(tab));
            }
            _ if ctrl && key == b'L' as usize => {
                let t = edit(|u| {
                    u.theme = if u.theme == "dark" { "light" } else { "dark" }.into();
                    u.theme.clone()
                });
                let path = with(|u| u.settings_path.clone());
                let hotkey = Settings::load(&path).unwrap_or_default().hotkey;
                let _ = Settings { theme: t, hotkey }.save(&path);
                InvalidateRect(h, null(), 1);
                InvalidateRect(ed, null(), 1);
            }
            _ if ctrl && key == b'F' as usize => {
                SetFocus(with(|u| u.edit));
                SendMessageW(with(|u| u.edit), 0x00b1, 0, -1);
            }
            _ => {
                let _ = shift;
                return false;
            }
        }
        true
    }
}
unsafe fn font(h: HWND) {
    unsafe {
        let f = CreateFontW(
            -scale(h, 15),
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            0,
            0,
            wide("맑은 고딕").as_ptr(),
        );
        let (old, ed) = edit(|u| {
            let old = u.font;
            u.font = f;
            (old, u.edit)
        });
        SendMessageW(ed, WM_SETFONT, f as usize, 1);
        if !old.is_null() {
            DeleteObject(old);
        }
    }
}
unsafe fn layout(h: HWND) {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(h, &mut r);
        MoveWindow(
            with(|u| u.edit),
            scale(h, 14),
            scale(h, 44),
            r.right - scale(h, 28),
            scale(h, 30),
            1,
        );
    }
}
unsafe fn highlighted(dc:HDC,s:&str,query:&str,rc:RECT,fg:u32,bg:u32) {unsafe{
    let saved=SaveDC(dc);IntersectClipRect(dc,rc.left,rc.top,rc.right,rc.bottom);
    let mut offset=0;let mut x=rc.left;
    for (start,end) in ktm::highlight::ranges(s,query).into_iter().chain(Some((s.len(),s.len()))) {
        for (part,marked) in [(&s[offset..start],false),(&s[start..end],true)] {
            if part.is_empty(){continue;}
            let w=wide(part);let mut size:SIZE=std::mem::zeroed();GetTextExtentPoint32W(dc,w.as_ptr(),(w.len()-1)as i32,&mut size);
            SetTextColor(dc,fg);SetBkColor(dc,bg);SetBkMode(dc,if marked{OPAQUE as i32}else{TRANSPARENT as i32});
            TextOutW(dc,x,rc.top+(rc.bottom-rc.top-size.cy)/2,w.as_ptr(),(w.len()-1)as i32);x+=size.cx;
        }
        offset=end;
    }
    RestoreDC(dc,saved);
}}
unsafe fn paint(h: HWND) {
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let dc = BeginPaint(h, &mut ps);
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(h, &mut r);
        if r.right <= 0 || r.bottom <= 0 {
            EndPaint(h, &ps);
            return;
        }
        let mem = CreateCompatibleDC(dc);
        let bitmap = CreateCompatibleBitmap(dc, r.right, r.bottom);
        let old = SelectObject(mem, bitmap);
        let oldfont = SelectObject(mem, with(|u| u.font));
        SetBkMode(mem, TRANSPARENT as i32);
        let (bg, fg, sel, selfg) = colors();
        let fill = |rc: RECT, c: u32| {
            let b = CreateSolidBrush(c);
            FillRect(mem, &rc, b);
            DeleteObject(b);
        };
        let text = |s: &str, mut rc: RECT, c: u32, flags: u32| {
            SetTextColor(mem, c);
            let ws = wide(s);
            DrawTextW(
                mem,
                ws.as_ptr(),
                (ws.len() - 1) as i32,
                &mut rc,
                DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | flags,
            );
        };
        fill(r, bg);
        let p = scale(h, 14);
        let line = scale(h, 24);
        text(
            "몽땅검색",
            RECT {
                left: p,
                top: 0,
                right: r.right - p,
                bottom: scale(h, 38),
            },
            fg,
            0,
        );
        text(
            "Ctrl+Alt+Space · Esc 숨김 · Ctrl+L 테마",
            RECT {
                left: r.right - scale(h, 345),
                top: 0,
                right: r.right - p,
                bottom: scale(h, 38),
            },
            fg,
            DT_RIGHT,
        );
        with(|u| {
            for (i, (label, tab)) in [
                ("본문", Tab::Body),
                ("위계", Tab::Hierarchy),
                ("파일명 · 준비 중", Tab::Names),
            ]
            .iter()
            .enumerate()
            {
                let rc = RECT {
                    left: p + i as i32 * scale(h, 165),
                    top: scale(h, 82),
                    right: p + (i as i32 + 1) * scale(h, 165),
                    bottom: scale(h, 116),
                };
                if u.state.tab == *tab {
                    fill(rc, sel);
                }
                text(
                    label,
                    rc,
                    if u.state.tab == *tab { selfg } else { fg },
                    DT_CENTER,
                );
            }
            let y = scale(h, 124);
            let rh = scale(h, 76);
            let visible = ((r.bottom - y - line) / rh).max(0) as usize;
            for (n, row) in u.rows.iter().enumerate().skip(u.top).take(visible) {
                let top = y + (n - u.top) as i32 * rh;
                let selected = u.state.selected == Some(n);
                let c = if selected { selfg } else { fg };
                if selected {
                    fill(
                        RECT {
                            left: p / 2,
                            top,
                            right: r.right - p / 2,
                            bottom: top + rh,
                        },
                        sel,
                    );
                }
                text(
                    &row.name,
                    RECT {
                        left: p,
                        top,
                        right: r.right - p,
                        bottom: top + line,
                    },
                    c,
                    DT_END_ELLIPSIS,
                );
                text(
                    &row.parent,
                    RECT {
                        left: p,
                        top: top + line,
                        right: r.right - p,
                        bottom: top + 2 * line,
                    },
                    c,
                    DT_PATH_ELLIPSIS,
                );
                let snippet = if row.snippet.is_empty() {
                    row.location.clone()
                } else {
                    row.snippet.clone()
                };
                highlighted(
                    mem, &snippet, &u.state.query,
                    RECT {
                        left: p,
                        top: top + 2 * line,
                        right: r.right - p,
                        bottom: top + 3 * line,
                    },
                    c, if u.theme=="light" {0x70eaff} else {0x805020},
                );
            }
            let status = if !u.hotkey_error.is_empty() {
                u.hotkey_error.clone()
            } else if let Some(e) = &u.state.error {
                e.clone()
            } else {
                match u.state.phase {
                    Phase::Searching => "검색 중…".into(),
                    Phase::Results => format!(
                        "{}건 · Enter 열기 · Ctrl+Enter 폴더 · ↑↓ 선택",
                        u.rows.len()
                    ),
                    _ => "2글자부터 실시간 검색 · 1글자는 Enter".into(),
                }
            };
            text(
                &status,
                RECT {
                    left: p,
                    top: r.bottom - line,
                    right: r.right - p,
                    bottom: r.bottom,
                },
                fg,
                DT_END_ELLIPSIS,
            );
        });
        BitBlt(dc, 0, 0, r.right, r.bottom, mem, 0, 0, SRCCOPY);
        SelectObject(mem, oldfont);
        SelectObject(mem, old);
        DeleteObject(bitmap);
        DeleteDC(mem);
        EndPaint(h, &ps);
        if let Some(start) = edit(|u| u.timing.take()) {
            log("input_to_paint_ms", start.elapsed().as_secs_f64() * 1000.0);
        }
    }
}
fn client_bottom(h: HWND) -> i32 {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(h, &mut r);
        r.bottom
    }
}
fn log(name: &str, value: f64) {
    if let Some(path) = std::env::var_os("KTM_METRICS") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{{\"metric\":\"{name}\",\"ms\":{value:.4}}}");
        }
    }
}
unsafe extern "system" fn proc(h: HWND, msg: u32, w: usize, l: isize) -> isize {
    unsafe {
        match msg {
            WM_NCHITTEST => {
                let hit = DefWindowProcW(h, msg, w, l);
                if hit == HTCLIENT as isize {
                    let mut p = POINT { x: l as i16 as i32, y: (l >> 16) as i16 as i32 };
                    ScreenToClient(h, &mut p);
                    if p.y < scale(h,38) { return HTCAPTION as isize; }
                }
                hit
            }
            WM_GETMINMAXINFO => {
                let info = &mut *(l as *mut MINMAXINFO);
                info.ptMinTrackSize = POINT { x: scale(h,520), y: scale(h,300) }; 0
            }
            WM_PAINT => {
                paint(h);
                0
            }
            WM_ERASEBKGND => 1,
            WM_HOTKEY => {
                let start = Instant::now();
                event(h, Event::Show);
                UpdateWindow(h);
                log("hotkey_to_paint_ms", start.elapsed().as_secs_f64() * 1000.0);
                0
            }
            WM_SIZE => {
                layout(h);
                0
            }
            WM_DPICHANGED => {
                let r = &*(l as *const RECT);
                SetWindowPos(
                    h,
                    null_mut(),
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                font(h);
                layout(h);
                0
            }
            WM_COMMAND if (w >> 16) as u16 == EN_CHANGE as u16 => {
                let ed = with(|u| u.edit);
                let len = GetWindowTextLengthW(ed).min(4096);
                let mut s = vec![0u16; len as usize + 1];
                GetWindowTextW(ed, s.as_mut_ptr(), s.len() as i32);
                event(
                    h,
                    Event::Input(String::from_utf16_lossy(&s[..len as usize])),
                );
                0
            }
            WM_CTLCOLOREDIT => {
                let (bg, fg, _, _) = colors();
                SetBkColor(w as HDC, bg);
                SetTextColor(w as HDC, fg);
                edit(|u| {
                    if !u.brush.is_null() {
                        DeleteObject(u.brush);
                    }
                    u.brush = CreateSolidBrush(bg);
                    u.brush as isize
                })
            }
            WM_KEYDOWN => {
                if key(h, w) {
                    0
                } else {
                    DefWindowProcW(h, msg, w, l)
                }
            }
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
                let x = l as i16 as i32;
                let y = (l >> 16) as i16 as i32;
                if y >= scale(h, 82) && y < scale(h, 116) {
                    let tab = if x < scale(h, 179) {
                        Tab::Body
                    } else if x < scale(h, 344) {
                        Tab::Hierarchy
                    } else {
                        Tab::Names
                    };
                    event(h, Event::SwitchTab(tab));
                } else if y >= scale(h, 124) {
                    let n = with(|u| u.top) + ((y - scale(h, 124)) / scale(h, 76)) as usize;
                    if n < with(|u| u.rows.len()) && y < client_bottom(h) - scale(h, 24) {
                        event(h, Event::Select(n));
                        if msg == WM_LBUTTONDBLCLK {
                            event(h, Event::Open { folder: false });
                        }
                    }
                }
                SetFocus(with(|u| u.edit));
                0
            }
            WM_MOUSEWHEEL => {
                event(
                    h,
                    Event::MoveSelection(if (w >> 16) as i16 > 0 { -1 } else { 1 }),
                );
                0
            }
            WM_CLOSE => {
                event(h, Event::Hide);
                0
            }
            READY => {
                let answers = with(|u| u.rx.try_iter().collect::<Vec<_>>());
                for (g, result, start) in answers {
                    if g != with(|u| u.state.generation) {
                        continue;
                    }
                    match result {
                        Ok(rows) => {
                            let count = rows.len();
                            event(
                                h,
                                Event::SearchFinished {
                                    generation: g,
                                    count,
                                },
                            );
                            edit(|u| {
                                u.rows = rows;
                                u.timing = Some(start);
                            });
                        }
                        Err(message) => event(
                            h,
                            Event::SearchFailed {
                                generation: g,
                                message,
                            },
                        ),
                    }
                }
                InvalidateRect(h, null(), 0);
                0
            }
            m if m == tray::MESSAGE => {
                if l as u32 == WM_RBUTTONUP {
                    match tray::menu(h) {
                        1 => event(h, Event::Show),
                        2 => {
                            DestroyWindow(h);
                        }
                        _ => {}
                    }
                } else if l as u32 == WM_LBUTTONUP {
                    event(h, Event::Show);
                }
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(h, msg, w, l),
        }
    }
}
pub fn run() -> Result<(), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let args: Vec<String> = std::env::args().collect();
        let arg = |name: &str| args.windows(2).find(|a| a[0] == name).map(|a| a[1].clone());
        let data = std::env::var_os("KTM_TEST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default())
                    .join("HwpAutoDocFit/ktm")
            });
        std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
        let settings = Settings::load(&data.join("settings.json"))?;
        let source = std::env::current_exe().map_err(|e| e.to_string())?;
        let destination = arg("--install-path")
            .map(PathBuf::from)
            .unwrap_or_else(|| data.join("bin/ktm.exe"));
        if let Some(actions) = ktm::install::plan(&args) {
            for action in actions {
                ktm::install::execute(action, &source, &destination)?;
            }
            return Ok(());
        }
        let previous = FindWindowW(wide(CLASS).as_ptr(), null());
        if !previous.is_null() && std::env::var_os("KTM_TEST_ROOT").is_none() {
            PostMessageW(previous, WM_HOTKEY, 0, 0);
            return Ok(());
        }
        let root = match arg("--index-root") {
            Some(p) => PathBuf::from(p),
            None => ktm::paths::default_index()?,
        };
        let pending = Arc::new((Mutex::new(None::<Job>), Condvar::new()));
        let (tx, rx) = mpsc::channel();
        let (status_tx,status_rx) = mpsc::channel::<String>();
        let status_path = data.join("status.json");
        std::thread::spawn(move || {
            let mut previous = String::new();
            while let Ok(mut value) = status_rx.recv() {
                for latest in status_rx.try_iter() { value = latest; }
                if value != previous { let _ = std::fs::write(&status_path,&value); previous = value; }
            }
        });
        let inst = GetModuleHandleW(null());
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.style = CS_DBLCLKS;
        wc.lpfnWndProc = Some(DefWindowProcW);
        wc.hInstance = inst;
        wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        let class = wide(CLASS);
        wc.lpszClassName = class.as_ptr();
        if RegisterClassW(&wc) == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        // CreateWindow sends setup messages before Ui exists; use the default procedure until construction ends.
        let h = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class.as_ptr(),
            wide("몽땅검색").as_ptr(),
            WS_POPUP | WS_THICKFRAME | WS_CLIPCHILDREN,
            0,
            0,
            700,
            490,
            null_mut(),
            null_mut(),
            inst,
            null(),
        );
        if h.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let ed = CreateWindowExW(
            0,
            wide("EDIT").as_ptr(),
            wide("").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32,
            14,
            44,
            670,
            30,
            h,
            1 as _,
            inst,
            null(),
        );
        SendMessageW(ed, 0x00c5, 4096, 0);
        UI.with(|u| {
            *u.borrow_mut() = Some(Ui {
                state: State::default(),
                rows: Vec::new(),
                edit: ed,
                font: null_mut(),
                brush: null_mut(),
                theme: arg("--theme").unwrap_or(settings.theme.clone()),
                tx: pending.clone(),
                rx,
                status: status_tx,
                top: 0,
                timing: None,
                hotkey_error: String::new(),
                cancel: Arc::new(AtomicBool::new(false)),
                settings_path: data.join("settings.json"),
            })
        });
        SetWindowLongPtrW(h, GWLP_WNDPROC, proc as *const () as isize);
        font(h);
        layout(h);
        let worker_pending = pending;
        let handle = h as usize;
        let pipe_name = arg("--pipe-name").unwrap_or(ktm::backend::PIPE_NAME.into());
        let pipe_mode = arg("--backend").as_deref() == Some("pipe");
        std::thread::spawn(move || {
            let mut backend: Option<Backend> = None;
            loop {
                let (g, q, hierarchy, start, cancel) = {
                    let mut slot = worker_pending.0.lock().unwrap();
                    while slot.is_none() {
                        slot = worker_pending.1.wait(slot).unwrap();
                    }
                    slot.take().unwrap()
                };
                let answer = (|| {
                    if pipe_mode {
                        return ktm::backend::pipe_search(&pipe_name, &q, hierarchy);
                    }
                    if backend.is_none() {
                        backend = Some(Backend::open(&root)?);
                    }
                    backend
                        .as_mut()
                        .unwrap()
                        .search_cancel(&q, hierarchy, Some(cancel))
                })();
                if tx.send((g, answer, start)).is_err() {
                    break;
                }
                PostMessageW(handle as HWND, READY, 0, 0);
            }
        });
        let hotkey = arg("--hotkey").unwrap_or(settings.hotkey);
        let _tray = match tray::Tray::new(h, &hotkey) {
            Ok(t) => { if let Some(error)=&t.error {edit(|u|u.hotkey_error=error.clone());} Some(t) },
            Err(e) => {
                edit(|u| u.hotkey_error = e);
                event(h, Event::Show);
                None
            }
        };
        event(
            h,
            if args.iter().any(|a| a == "--show") || with(|u| !u.hotkey_error.is_empty()) {
                Event::Show
            } else {
                Event::Hide
            },
        );
        if let Some(query) = arg("--show-query") {
            event(h, Event::Show);
            SetWindowTextW(ed, wide(&query).as_ptr());
        }
        if let Some(output) = arg("--self-test").or_else(|| arg("--measure")) {
            probe::smoke(h, std::path::Path::new(&output), arg("--measure").is_some())?;
        }
        let mut msg: MSG = std::mem::zeroed();
        loop {
            let got = GetMessageW(&mut msg, null_mut(), 0, 0);
            if got <= 0 {
                break;
            }
            if msg.message == WM_KEYDOWN && key(h, msg.wParam) {
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        edit(|u| {
            DeleteObject(u.font);
            if !u.brush.is_null() {
                DeleteObject(u.brush);
            }
        });
        Ok(())
    }
}
