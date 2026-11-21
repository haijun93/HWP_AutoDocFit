//! 실제 창: 메시지 처리·마우스·키보드·스캔 스레드 연결. 그림은 `paint::draw`, 값 처리는 `model::App`.
use crate::layout::{DEFAULT_H, DEFAULT_W, MIN_H, MIN_W, Toolbar, scale};
use crate::model::{App, Nav, ScanState};
use crate::paint;
use crate::scan::{Event, ScanJob, find_klib};
use crate::sys::{fixed_drive_roots, tz_offset_secs, wide};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::Arc;
use ui_theme::{Palette, ThemePref};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const WM_SCAN: u32 = WM_APP + 1;
const TIMER_ELAPSED: usize = 1;

#[derive(Clone, Default)]
pub struct Options {
    /// 설정 파일(없으면 기본 위치). 읽기만 한다.
    pub settings: Option<PathBuf>,
    pub klib: Option<PathBuf>,
    /// 합성 목록 N 개를 비동기 스캔처럼 흘려 보낸다(klib 없이 창·진행·취소 시험).
    pub synthetic: Option<usize>,
    pub no_scan: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Drag {
    None,
    Slider,
    /// 스크롤 손잡이를 누른 지점(손잡이 윗변에서의 거리).
    Thumb(i32),
}

struct State {
    app: App,
    pal: Palette,
    pref: ThemePref,
    dpi: u32,
    job: Option<ScanJob>,
    drag: Drag,
    high: Option<u16>,
    opts: Options,
    excludes: Vec<String>,
}

unsafe fn state<'a>(hwnd: HWND) -> Option<&'a mut State> {
    unsafe {
        let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        if p.is_null() { None } else { Some(&mut *p) }
    }
}

fn client_size(hwnd: HWND) -> (i32, i32) {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut r);
        (r.right - r.left, r.bottom - r.top)
    }
}

fn xy(lp: LPARAM) -> (i32, i32) {
    ((lp & 0xFFFF) as u16 as i16 as i32, ((lp >> 16) & 0xFFFF) as u16 as i16 as i32)
}

fn redraw(hwnd: HWND) {
    unsafe {
        InvalidateRect(hwnd, null(), 0);
    }
}

fn ctrl_down() -> bool {
    unsafe { GetKeyState(VK_CONTROL as i32) < 0 }
}

/// 스캔을 시작한다(이미 돌고 있으면 무시). 목록은 비우고 들어오는 대로 채운다.
unsafe fn start_scan(hwnd: HWND, s: &mut State) {
    if s.job.as_ref().is_some() {
        return;
    }
    let h = hwnd as usize;
    let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || unsafe {
        PostMessageW(h as HWND, WM_SCAN, 0, 0);
    });
    let job = if let Some(n) = s.opts.synthetic {
        s.app.engine = "합성".into();
        let docs = crate::synth::synthetic_docs(n, 7);
        let lines = docs.into_iter().enumerate().map(|(i, d)| {
            if i % 300 == 299 {
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
            Ok(crate::synth::to_json_line(&d))
        });
        Ok(ScanJob::from_lines(lines, None, notify))
    } else {
        s.app.engine = "klib scan".into();
        match s.opts.klib.clone().or_else(find_klib) {
            None => Err("klib 실행 파일을 찾지 못했습니다(DOCFIT_KLIB 또는 --klib 로 지정).".to_string()),
            Some(exe) => {
                let roots = fixed_drive_roots();
                if roots.is_empty() {
                    Err("고정 드라이브를 찾지 못했습니다.".to_string())
                } else {
                    ScanJob::start_klib(&exe, &roots, &s.excludes, notify)
                }
            }
        }
    };
    match job {
        Ok(j) => {
            s.app.coll.replace(Vec::new());
            s.app.selected = None;
            s.app.scroll = 0;
            s.app.scan = crate::model::ScanView { state: ScanState::Running, ..Default::default() };
            s.job = Some(j);
            unsafe {
                SetTimer(hwnd, TIMER_ELAPSED, 500, None);
            }
        }
        Err(e) => s.app.scan.state = ScanState::Failed(e),
    }
    redraw(hwnd);
}

unsafe fn cancel_scan(hwnd: HWND, s: &mut State) {
    if let Some(j) = &s.job {
        j.cancel();
        s.app.scan.state = ScanState::Cancelling;
        redraw(hwnd);
    }
}

/// 스캔 스레드가 보낸 이벤트를 화면 상태에 반영한다.
unsafe fn pump(hwnd: HWND, s: &mut State) {
    unsafe {
        let Some(job) = &s.job else { return };
        let mut done = false;
        for e in job.drain() {
            match e {
                Event::Batch { docs, visited, folder } => {
                    s.app.extend_docs(docs);
                    s.app.scan.visited = visited;
                    s.app.scan.folder = folder;
                }
                Event::Finished { cancelled } => {
                    s.app.scan.state = if cancelled { ScanState::Cancelled } else { ScanState::Complete };
                    done = true;
                }
                Event::Failed(msg) => {
                    s.app.scan.state = ScanState::Failed(msg);
                    done = true;
                }
            }
        }
        s.app.scan.elapsed_secs = job.started.elapsed().as_secs();
        if done {
            KillTimer(hwnd, TIMER_ELAPSED);
            if let Some(mut j) = s.job.take() {
                j.join();
            }
        }
        let (w, h) = client_size(hwnd);
        let m = s.app.metrics(w, h, s.dpi);
        s.app.clamp_scroll(&m);
        redraw(hwnd);
    }
}

unsafe fn paint_window(hwnd: HWND, s: &State) {
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let hdc = BeginPaint(hwnd, &mut ps);
        let (w, h) = client_size(hwnd);
        if w > 0 && h > 0 {
            let mem = CreateCompatibleDC(hdc);
            let bm = CreateCompatibleBitmap(hdc, w, h);
            let old = SelectObject(mem, bm);
            paint::draw(mem, w, h, &s.app, &s.pal, s.dpi);
            BitBlt(hdc, 0, 0, w, h, mem, 0, 0, SRCCOPY);
            SelectObject(mem, old);
            DeleteObject(bm);
            DeleteDC(mem);
        }
        EndPaint(hwnd, &ps);
    }
}

unsafe fn set_thumb_from_x(hwnd: HWND, s: &mut State, x: i32) {
    let (w, h) = client_size(hwnd);
    let tb = Toolbar::new(w, s.dpi);
    let (dpi, banner) = (s.dpi, s.app.banner_shown());
    s.app.set_thumb(tb.slider_value(x), |t| crate::layout::Metrics::new(w, h, dpi, t, banner));
}

unsafe fn on_click(hwnd: HWND, s: &mut State, x: i32, y: i32, right: bool) {
    unsafe {
        let (w, h) = client_size(hwnd);
        let tb = Toolbar::new(w, s.dpi);
        s.app.search_focus = false;
        if tb.bar.contains(x, y) {
            if tb.search.contains(x, y) {
                s.app.search_focus = true;
            } else if tb.sort.contains(x, y) {
                if right || ctrl_down() || GetKeyState(VK_SHIFT as i32) < 0 {
                    s.app.coll.toggle_direction();
                } else {
                    s.app.coll.cycle_sort();
                }
                s.app.selected = None;
                s.app.scroll = 0;
            } else if tb.slider.contains(x, y) || tb.slider_label.contains(x, y) {
                s.drag = Drag::Slider;
                SetCapture(hwnd);
                set_thumb_from_x(hwnd, s, x);
            } else if tb.exclude.contains(x, y) {
                let list = if s.excludes.is_empty() { "제외한 폴더가 없습니다.".to_string() } else { s.excludes.join("\n") };
                MessageBoxW(hwnd, wide(&format!("{list}\n\n(1단계는 읽기 전용입니다. 추가·삭제는 다음 단계에서 지원합니다.)")).as_ptr(), wide("제외할 폴더").as_ptr(), MB_OK);
            } else if tb.action.contains(x, y) {
                if s.job.is_some() {
                    cancel_scan(hwnd, s);
                } else {
                    start_scan(hwnd, s);
                }
            }
            redraw(hwnd);
            return;
        }
        let n = s.app.coll.len();
        let m = s.app.metrics(w, h, s.dpi);
        if let Some(t) = m.scrollbar_thumb(s.app.scroll, n) {
            if t.contains(x, y) {
                s.drag = Drag::Thumb(y - t.top);
                SetCapture(hwnd);
                return;
            }
            if m.scrollbar_track().contains(x, y) {
                let page = m.viewport.height() - m.stride() / 2;
                s.app.scroll_by(&m, if y < t.top { -page } else { page });
                redraw(hwnd);
                return;
            }
        }
        s.app.select(&m, m.hit(x, y, s.app.scroll, n));
        redraw(hwnd);
    }
}

unsafe fn on_key(hwnd: HWND, s: &mut State, vk: u32) -> bool {
    unsafe {
        let (w, h) = client_size(hwnd);
        let m = s.app.metrics(w, h, s.dpi);
        let nav = match vk as u16 {
            VK_LEFT => Some(Nav::Left),
            VK_RIGHT => Some(Nav::Right),
            VK_UP => Some(Nav::Up),
            VK_DOWN => Some(Nav::Down),
            VK_HOME => Some(Nav::Home),
            VK_END => Some(Nav::End),
            VK_PRIOR => Some(Nav::PageUp),
            VK_NEXT => Some(Nav::PageDown),
            _ => None,
        };
        if s.app.search_focus {
            match vk as u16 {
                VK_ESCAPE if !s.app.coll.query.is_empty() => {
                    s.app.coll.set_query("");
                    s.app.scroll = 0;
                    s.app.selected = None;
                }
                VK_ESCAPE | VK_RETURN => s.app.search_focus = false,
                VK_DOWN => {
                    s.app.search_focus = false;
                    s.app.nav(&m, Nav::Down);
                }
                _ => return false,
            }
            redraw(hwnd);
            return true;
        }
        if let Some(n) = nav {
            s.app.nav(&m, n);
        } else if vk == 0x46 && ctrl_down() {
            s.app.search_focus = true; // Ctrl+F
        } else if vk as u16 == VK_F5 {
            start_scan(hwnd, s);
        } else if vk as u16 == VK_ESCAPE && s.job.is_some() {
            cancel_scan(hwnd, s);
        } else {
            return false;
        }
        redraw(hwnd);
        true
    }
}

unsafe fn on_char(hwnd: HWND, s: &mut State, ch: u16) {
    if !s.app.search_focus {
        return;
    }
    let mut q = s.app.coll.query.clone();
    match ch {
        8 => {
            q.pop();
        }
        c if c < 0x20 || c == 0x7F => return,
        c if (0xD800..0xDC00).contains(&c) => {
            s.high = Some(c);
            return;
        }
        c => {
            let units: Vec<u16> = match s.high.take() {
                Some(hi) => vec![hi, c],
                None => vec![c],
            };
            q.push_str(&String::from_utf16_lossy(&units));
        }
    }
    s.app.coll.set_query(&q);
    s.app.selected = None;
    s.app.scroll = 0;
    redraw(hwnd);
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_NCCREATE => {
                let cs = lp as *const CREATESTRUCTW;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*cs).lpCreateParams as isize);
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                if let Some(s) = state(hwnd) {
                    paint_window(hwnd, s);
                }
                0
            }
            WM_SIZE => {
                if let Some(s) = state(hwnd) {
                    let (w, h) = client_size(hwnd);
                    let m = s.app.metrics(w, h, s.dpi);
                    s.app.clamp_scroll(&m);
                    redraw(hwnd);
                }
                0
            }
            WM_GETMINMAXINFO => {
                if let Some(s) = state(hwnd) {
                    let mut r = RECT { left: 0, top: 0, right: scale(MIN_W, s.dpi), bottom: scale(MIN_H, s.dpi) };
                    AdjustWindowRectExForDpi(&mut r, WS_OVERLAPPEDWINDOW, 0, 0, s.dpi);
                    let mm = lp as *mut MINMAXINFO;
                    (*mm).ptMinTrackSize = POINT { x: r.right - r.left, y: r.bottom - r.top };
                }
                0
            }
            WM_DPICHANGED => {
                if let Some(s) = state(hwnd) {
                    s.dpi = (wp & 0xFFFF) as u32;
                }
                let r = lp as *const RECT;
                SetWindowPos(hwnd, null_mut(), (*r).left, (*r).top, (*r).right - (*r).left, (*r).bottom - (*r).top, SWP_NOZORDER | SWP_NOACTIVATE);
                0
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
                if let Some(s) = state(hwnd) {
                    let (x, y) = xy(lp);
                    SetFocus(hwnd);
                    on_click(hwnd, s, x, y, msg == WM_RBUTTONDOWN);
                }
                0
            }
            WM_MOUSEMOVE => {
                if let Some(s) = state(hwnd) {
                    let (x, y) = xy(lp);
                    match s.drag {
                        Drag::Slider => {
                            set_thumb_from_x(hwnd, s, x);
                            redraw(hwnd);
                        }
                        Drag::Thumb(grab) => {
                            let (w, h) = client_size(hwnd);
                            let m = s.app.metrics(w, h, s.dpi);
                            s.app.scroll = m.scroll_from_thumb_top(y - grab, s.app.coll.len());
                            redraw(hwnd);
                        }
                        Drag::None => {}
                    }
                }
                0
            }
            WM_LBUTTONUP => {
                if let Some(s) = state(hwnd) {
                    if s.drag != Drag::None {
                        s.drag = Drag::None;
                        ReleaseCapture();
                    }
                }
                0
            }
            WM_MOUSEWHEEL => {
                if let Some(s) = state(hwnd) {
                    let notches = ((wp >> 16) as u16 as i16) as i32;
                    let (w, h) = client_size(hwnd);
                    let m = s.app.metrics(w, h, s.dpi);
                    s.app.scroll_by(&m, -notches * m.stride() / 360);
                    redraw(hwnd);
                }
                0
            }
            WM_KEYDOWN => {
                if let Some(s) = state(hwnd) {
                    if on_key(hwnd, s, wp as u32) {
                        return 0;
                    }
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_GETDLGCODE => 4 /* DLGC_WANTARROWS */ | 2 /* DLGC_WANTCHARS */,
            WM_CHAR => {
                if let Some(s) = state(hwnd) {
                    on_char(hwnd, s, wp as u16);
                }
                0
            }
            WM_SCAN => {
                if let Some(s) = state(hwnd) {
                    pump(hwnd, s);
                }
                0
            }
            WM_TIMER => {
                if let Some(s) = state(hwnd) {
                    pump(hwnd, s);
                }
                0
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                if let Some(s) = state(hwnd) {
                    if let Some(j) = &s.job {
                        j.cancel();
                    }
                }
                PostQuitMessage(0);
                0
            }
            _ => {
                if ui_theme::is_theme_message(msg, wp, lp) {
                    if let Some(s) = state(hwnd) {
                        ui_theme::refresh(s.pref);
                        let (kind, pal) = ui_theme::current(s.pref);
                        s.pal = pal;
                        ui_theme::apply_window(hwnd, kind);
                    }
                    redraw(hwnd);
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
        }
    }
}

pub fn run(opts: Options) -> Result<(), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let hinst = GetModuleHandleW(null());
        let class = wide("DocViewWinMain");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
            lpfnWndProc: Some(proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: null_mut(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
        let pref = ThemePref::System;
        ui_theme::refresh(pref);
        let (kind, pal) = ui_theme::current(pref);
        let dpi = GetDpiForSystem().max(96);
        let path = opts.settings.clone().unwrap_or_else(crate::settings::settings_path);
        let text = crate::settings::read_text(&path);
        let mut app = App::new(crate::settings::thumb_size(&text));
        let excludes = crate::settings::excluded_folders(&text);
        app.excluded_count = excludes.len();
        app.tz_offset = tz_offset_secs();
        let mut r = RECT { left: 0, top: 0, right: scale(DEFAULT_W, dpi), bottom: scale(DEFAULT_H, dpi) };
        AdjustWindowRectExForDpi(&mut r, WS_OVERLAPPEDWINDOW, 0, 0, dpi);
        let no_scan = opts.no_scan;
        let raw = Box::into_raw(Box::new(State { app, pal, pref, dpi, job: None, drag: Drag::None, high: None, opts, excludes }));
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            wide("문서뷰").as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            r.right - r.left,
            r.bottom - r.top,
            null_mut(),
            null_mut(),
            hinst,
            raw.cast(),
        );
        if hwnd.is_null() {
            drop(Box::from_raw(raw));
            return Err("창을 만들지 못했습니다".into());
        }
        let real = GetDpiForWindow(hwnd).max(96);
        (*raw).dpi = real;
        ui_theme::apply_window(hwnd, kind);
        ShowWindow(hwnd, SW_SHOW);
        SetFocus(hwnd);
        if !no_scan {
            start_scan(hwnd, &mut *raw);
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        drop(Box::from_raw(raw));
        Ok(())
    }
}
