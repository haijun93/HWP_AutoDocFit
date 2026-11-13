//! Win32 입력/IME + GDI 이중 버퍼. 검색 스레드는 이벤트로만 깨며 창 상태는 step으로만 바꾼다.
#[path = "window_probe.rs"]
mod probe;
use ktm::{
    backend::{Answer, Backend, Row},
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
const WARMED: u32 = WM_APP + 22;
const CLASS: &str = "HwpAutoDocFit.MongttangSearch";
type Job = (u64, String, Tab, Instant, Arc<AtomicBool>);
/// 스크롤로 새로 보이게 된 줄의 원문 찾기: (세대, [(줄 번호, 줄)], 강조 낱말).
type SnipJob = (u64, Vec<(usize, Row)>, Vec<String>);
/// 일꾼 대기열: 검색은 늘 최신 하나, 원문 찾기도 최신 하나(검색 먼저).
#[derive(Default)]
struct Pending {
    search: Option<Job>,
    snippets: Option<SnipJob>,
}
enum Reply {
    Search(u64, Result<Answer, String>, Instant),
    Snippets(u64, Vec<(usize, Row)>),
}
struct Ui {
    state: State,
    rows: Vec<Row>,
    terms: Vec<String>,
    edit: HWND,
    font: HFONT,
    brush: HBRUSH,
    theme: String,
    tx: Arc<(Mutex<Pending>, Condvar)>,
    rx: mpsc::Receiver<Reply>,
    status: mpsc::Sender<String>,
    top: usize,
    timing: Option<Instant>,
    hotkey_error: String,
    cancel: Arc<AtomicBool>,
    settings_path: PathBuf,
    /// 시작 데우기가 끝났는지(측정 탐침이 기다림).
    warmed: bool,
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
            u.terms.clear();
            u.top = 0;
        }
        fx
    });
    for fx in effects {
        unsafe {
            match fx {
                Effect::Show => {
                    let t = Instant::now();
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
                    let t1 = Instant::now();
                    SetForegroundWindow(h);
                    let t2 = Instant::now();
                    SetFocus(with(|u| u.edit));
                    phase("show_setwindowpos_ms", t1 - t);
                    phase("show_foreground_ms", t2 - t1);
                    phase("show_focus_ms", t2.elapsed());
                }
                Effect::Hide => {
                    ShowWindow(h, SW_HIDE);
                    trim_memory();
                }
                Effect::Search {
                    generation,
                    query,
                    tab,
                } => {
                    let tx = with(|u| u.tx.clone());
                    let cancel = Arc::new(AtomicBool::new(false));
                    edit(|u| u.cancel = cancel.clone());
                    {
                        let mut p = tx.0.lock().unwrap();
                        p.search = Some((generation, query, tab, Instant::now(), cancel));
                        p.snippets = None;
                    }
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
            request_snippets(u, count);
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
/// 보이는 줄 중 원문을 아직 찾지 않은 줄만 일꾼에게 보낸다(한 번만).
fn request_snippets(u: &mut Ui, count: usize) {
    let end = (u.top + count + 1).min(u.rows.len());
    let mut want = Vec::new();
    for i in u.top..end {
        if !u.rows[i].snippet_tried {
            u.rows[i].snippet_tried = true;
            let mut r = u.rows[i].clone();
            r.snippet_tried = false;
            want.push((i, r));
        }
    }
    if !want.is_empty() {
        u.tx.0.lock().unwrap().snippets = Some((u.state.generation, want, u.terms.clone()));
        u.tx.1.notify_one();
    }
}
/// 작업 집합 비우기는 쓰지 않는다(MG2 측정: 다음 표시·첫 질의가 쪽 다시 읽기로 25~90ms 늦어짐).
/// 쉴 때 실사용 메모리(개인 작업 집합)는 비우지 않아도 개인 커밋(약 3.6MB) 안이다. `--trim-on-hide`로만 켠다.
fn trim_memory() {
    if !std::env::args().any(|a| a == "--trim-on-hide") {
        return;
    }
    unsafe {
        windows_sys::Win32::System::ProcessStatus::K32EmptyWorkingSet(
            windows_sys::Win32::System::Threading::GetCurrentProcess(),
        );
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
unsafe fn highlighted(dc:HDC,s:&str,terms:&[String],rc:RECT,fg:u32,bg:u32) {unsafe{
    let saved=SaveDC(dc);IntersectClipRect(dc,rc.left,rc.top,rc.right,rc.bottom);
    let mut offset=0;let mut x=rc.left;
    for (start,end) in ktm::highlight::ranges_terms(s,terms).into_iter().chain(Some((s.len(),s.len()))) {
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
        if r.right > 0 && r.bottom > 0 {
            draw(h, dc, r);
        }
        EndPaint(h, &ps);
        if let Some(start) = edit(|u| u.timing.take()) {
            log("input_to_paint_ms", start.elapsed().as_secs_f64() * 1000.0);
        }
    }
}
/// 시작할 때 한 번: 화면 밖 메모리 DC에 창 전체(글꼴·한글 글꼴 연결·강조 경로 포함)와 검색 칸을 그려 둔다.
/// 첫 부름에서 GDI·Uniscribe·글꼴을 처음 불러오는 비용을 없앤다.
unsafe fn prerender(h: HWND) {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(h, &mut r);
        let dc = GetDC(h);
        if r.right > 0 && r.bottom > 0 {
            edit(|u| {
                u.rows = vec![Row {
                    name: "몽땅검색 미리 그리기.hwp".into(),
                    parent: "C:\\".into(),
                    snippet: "…가나다 예산 ABC 123 문서".into(),
                    ..Row::default()
                }];
                u.terms = vec!["예산".into()];
            });
            draw(h, dc, r);
            let ed = with(|u| u.edit);
            let mem = CreateCompatibleDC(dc);
            let bm = CreateCompatibleBitmap(dc, r.right, scale(h, 40));
            let old = SelectObject(mem, bm);
            SendMessageW(ed, WM_PRINT, mem as usize, (PRF_CLIENT | PRF_NONCLIENT | PRF_ERASEBKGND) as isize);
            SelectObject(mem, old);
            DeleteObject(bm);
            DeleteDC(mem);
            edit(|u| {
                u.rows.clear();
                u.terms.clear();
            });
        }
        ReleaseDC(h, dc);
    }
}
unsafe fn draw(h: HWND, dc: HDC, r: RECT) {
    unsafe {
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
                let (snippet, terms): (&str, &[String]) = if row.snippet.is_empty() {
                    (&row.location, &[])
                } else {
                    (&row.snippet, &u.terms)
                };
                highlighted(
                    mem, snippet, terms,
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
    }
}
fn client_bottom(h: HWND) -> i32 {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(h, &mut r);
        r.bottom
    }
}
thread_local! {static PHASES: RefCell<Vec<(&'static str, f64)>> = const { RefCell::new(Vec::new()) };}
/// 측정 켰을 때만 모았다가 `flush_phases`에서 한 번에 쓴다(쓰기가 잰 구간에 들지 않게).
fn phase(name: &'static str, d: std::time::Duration) {
    if std::env::var_os("KTM_METRICS").is_some() {
        PHASES.with(|p| p.borrow_mut().push((name, d.as_secs_f64() * 1000.0)));
    }
}
fn flush_phases() {
    for (n, v) in PHASES.with(|p| std::mem::take(&mut *p.borrow_mut())) {
        log(n, v);
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
                let t = Instant::now();
                UpdateWindow(h);
                phase("show_update_ms", t.elapsed());
                phase("hotkey_to_paint_ms", start.elapsed());
                flush_phases();
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
                for reply in answers {
                    let (g, result, start) = match reply {
                        Reply::Search(g, result, start) => (g, result, start),
                        Reply::Snippets(g, rows) => {
                            edit(|u| {
                                if g == u.state.generation {
                                    for (i, r) in rows {
                                        if let Some(slot) = u.rows.get_mut(i) {
                                            slot.snippet = r.snippet;
                                        }
                                    }
                                }
                            });
                            continue;
                        }
                    };
                    if g != with(|u| u.state.generation) {
                        continue;
                    }
                    match result {
                        Ok(Answer { rows, terms }) => {
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
                                u.terms = terms;
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
            WARMED => {
                log("startup_warm_ms", w as f64 / 1000.0);
                edit(|u| u.warmed = true);
                if with(|u| u.state.phase) == Phase::Hidden {
                    trim_memory();
                }
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
        let pipe_mode_early = arg("--backend").as_deref() == Some("pipe");
        let previous = FindWindowW(wide(CLASS).as_ptr(), null());
        if !previous.is_null() && std::env::var_os("KTM_TEST_ROOT").is_none() {
            PostMessageW(previous, WM_HOTKEY, 0, 0);
            return Ok(());
        }
        let root = match arg("--index-root") {
            Some(p) => PathBuf::from(p),
            None => ktm::paths::default_index()?,
        };
        let started = Instant::now();
        let pending = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
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
                terms: Vec::new(),
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
                warmed: pipe_mode_early,
            })
        });
        SetWindowLongPtrW(h, GWLP_WNDPROC, proc as *const () as isize);
        font(h);
        layout(h);
        let worker_pending = pending;
        let handle = h as usize;
        let pipe_name = arg("--pipe-name").unwrap_or(ktm::backend::PIPE_NAME.into());
        let pipe_mode = arg("--backend").as_deref() == Some("pipe");
        let para_path = arg("--content-db")
            .map(PathBuf::from)
            .or_else(ktm::paragraphs::default_path);
        std::thread::spawn(move || {
            // 시작 데우기: 색인 매핑·문서 표·문단 표를 미리 열고 질의 한 번(창 표시 경로에서 무거운 일 0).
            let open = |root: &PathBuf| Backend::open(root).map(|b| b.with_paragraphs(para_path.clone()));
            let mut backend: Option<Backend> = None;
            if !pipe_mode {
                let t = Instant::now();
                backend = open(&root).ok();
                if let Some(b) = backend.as_mut() {
                    let _ = b.warm();
                }
                PostMessageW(handle as HWND, WARMED, t.elapsed().as_micros() as usize, 0);
            }
            loop {
                enum Work {
                    Search(Job),
                    Snippets(SnipJob),
                }
                let work = {
                    let mut slot = worker_pending.0.lock().unwrap();
                    loop {
                        if let Some(j) = slot.search.take() {
                            break Work::Search(j);
                        }
                        if let Some(j) = slot.snippets.take() {
                            break Work::Snippets(j);
                        }
                        slot = worker_pending.1.wait(slot).unwrap();
                    }
                };
                let reply = match work {
                    Work::Search((g, q, tab, start, cancel)) => {
                        let hierarchy = tab == Tab::Hierarchy;
                        let answer = (|| {
                            if pipe_mode {
                                let rows = ktm::backend::pipe_search(&pipe_name, &q, hierarchy)?;
                                let terms = q.split_whitespace().map(str::to_owned).collect();
                                return Ok(Answer { rows, terms });
                            }
                            if backend.is_none() {
                                backend = Some(open(&root)?);
                            }
                            backend.as_mut().unwrap().search_cancel(&q, hierarchy, Some(cancel))
                        })();
                        Reply::Search(g, answer, start)
                    }
                    Work::Snippets((g, mut rows, terms)) => {
                        if let Some(b) = backend.as_mut() {
                            let mut only: Vec<Row> = rows.iter().map(|r| r.1.clone()).collect();
                            b.fill_snippets(&mut only, &terms);
                            for (slot, r) in rows.iter_mut().zip(only) {
                                slot.1 = r;
                            }
                        }
                        Reply::Snippets(g, rows)
                    }
                };
                if tx.send(reply).is_err() {
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
        // 숨긴 창을 미리 그려 둔다: 메모리 DC에 한 번, 그리고 화면 밖 자리에서 한 번 보였다 숨겨 DWM 표면까지 만든다.
        if with(|u| u.state.phase) == Phase::Hidden {
            prerender(h);
            let fg = GetForegroundWindow();
            SetWindowPos(h, null_mut(), -32000, -32000, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW);
            UpdateWindow(h);
            if std::env::var_os("KTM_EXP_NO_FOCUS").is_none() {
                // 검색 칸에 한 번 초점을 주어 이 스레드의 IME·입력기(TSF)를 미리 붙인다. 백그라운드라 앞 창은 그대로.
                SetFocus(ed);
            }
            ShowWindow(h, SW_HIDE);
            if GetForegroundWindow() != fg {
                log("startup_foreground_changed", 1.0);
            }
            log("startup_prerender_ms", started.elapsed().as_secs_f64() * 1000.0);
        }
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
