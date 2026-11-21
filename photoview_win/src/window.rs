//! 실제 창: 메시지 처리·마우스·키보드·스캔 스레드 연결. 그림은 `paint::draw`, 값 처리는 `model::App`.
use crate::layout::{DEFAULT_H, DEFAULT_W, MIN_H, MIN_W, Toolbar, scale};
use crate::model::{App, Nav, ScanState};
use crate::panel::{ExcludePanel, Hit, PanelLayout};
use crate::paint;
use crate::scan::{Event, ScanJob, find_klib};
use crate::sys::{fixed_drive_roots, tz_offset_secs, wide};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
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
const WM_SHELL_THUMB: u32 = WM_APP + 2;
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
    /// 설정 파일(앱과 같은 파일). 제외 폴더 저장과 revision 검사에 쓴다.
    settings_path: PathBuf,
    /// 열려 있는 제외 폴더 편집 패널(모달).
    panel: Option<ExcludePanel>,
    /// 제외 폴더가 바뀌어 지금 도는 스캔을 취소한 뒤 다시 시작해야 한다.
    rescan: bool,
    thumb_service: grid_core::shell_thumb::ThumbnailService,
    thumbs: HashMap<String, grid_core::shell_thumb::Bitmap>,
    thumb_order: VecDeque<String>,
    thumb_bytes: usize,
    notify_hwnd: Arc<AtomicUsize>,
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
        let docs = crate::synth::synthetic_photos(n, 7);
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
            if std::mem::take(&mut s.rescan) {
                start_scan(hwnd, s);
            }
        }
        let (w, h) = client_size(hwnd);
        let m = s.app.metrics(w, h, s.dpi);
        s.app.clamp_scroll(&m);
        redraw(hwnd);
    }
}

fn receive_shell_thumbs(s: &mut State) {
    for completed in s.thumb_service.drain() {
        if let Ok(bitmap) = completed.result {
            let key = completed.path.to_string_lossy().into_owned();
            let bytes = bitmap.bgra.len();
            if bytes <= 64 * 1024 * 1024 {
                if let Some(old) = s.thumbs.remove(&key) { s.thumb_bytes = s.thumb_bytes.saturating_sub(old.bgra.len()); s.thumb_order.retain(|p| p != &key); }
                while s.thumb_bytes + bytes > 64 * 1024 * 1024 {
                    let Some(oldest) = s.thumb_order.pop_front() else { break };
                    if let Some(old) = s.thumbs.remove(&oldest) { s.thumb_bytes = s.thumb_bytes.saturating_sub(old.bgra.len()); }
                    s.thumb_service.forget(std::path::Path::new(&oldest));
                }
                s.thumb_bytes += bytes;
                s.thumb_order.push_back(key.clone());
                s.thumbs.insert(key, bitmap);
            }
        }
    }
}

unsafe fn paint_window(hwnd: HWND, s: &mut State) {
    receive_shell_thumbs(s);
    let (client_w, client_h) = client_size(hwnd);
    let metrics = s.app.metrics(client_w, client_h, s.dpi);
    let visible_paths: Vec<_> = metrics.visible_range(s.app.scroll, s.app.coll.len())
        .filter_map(|i| s.app.coll.get(i).map(|d| PathBuf::from(&d.path))).collect();
    s.thumb_service.set_visible(visible_paths.clone());
    for p in visible_paths { let key = p.to_string_lossy().into_owned(); if s.thumbs.contains_key(&key) { s.thumb_order.retain(|v| v != &key); s.thumb_order.push_back(key); } }
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let hdc = BeginPaint(hwnd, &mut ps);
        let (w, h) = client_size(hwnd);
        if w > 0 && h > 0 {
            let mem = CreateCompatibleDC(hdc);
            let bm = CreateCompatibleBitmap(hdc, w, h);
            let old = SelectObject(mem, bm);
            paint::draw_with_shell_thumbs(mem, w, h, &s.app, s.panel.as_ref(), &s.pal, s.dpi, &s.thumbs);
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
    s.app.set_thumb(tb.slider_value(x), |t| crate::layout::Metrics::with_spec(crate::layout::GridSpec::PHOTO, w, h, dpi, t, banner));
}

/// 패널을 닫는다. 이 패널에서 저장한 변경이 있으면 새 제외 폴더로 다시 검색한다(도는 중이면 취소하고 끝나면 다시).
unsafe fn close_panel(hwnd: HWND, s: &mut State) {
    let Some(p) = s.panel.take() else { return };
    if !p.changed {
        return;
    }
    s.excludes = p.items.clone();
    s.app.excluded_count = s.excludes.len();
    if s.job.is_some() {
        s.rescan = true;
        unsafe { cancel_scan(hwnd, s) };
    } else {
        unsafe { start_scan(hwnd, s) };
    }
}

unsafe fn panel_add(hwnd: HWND, s: &mut State) {
    let initial = s.panel.as_ref().and_then(|p| p.selected.and_then(|i| p.items.get(i).cloned())).unwrap_or_default();
    let picked = crate::folder::pick_folder(hwnd, "사진 검색에서 뺄 폴더를 고르세요", &initial);
    if let (Some(folder), Some(p)) = (picked, s.panel.as_mut()) {
        p.add(&folder);
    }
}

unsafe fn panel_click(hwnd: HWND, s: &mut State, x: i32, y: i32) {
    let (w, h) = client_size(hwnd);
    let l = PanelLayout::new(w, h, s.dpi);
    let Some(p) = s.panel.as_mut() else { return };
    match l.hit(x, y, p) {
        Hit::Add => unsafe { panel_add(hwnd, s) },
        Hit::Remove => {
            p.remove_selected();
        }
        Hit::Close => unsafe { close_panel(hwnd, s) },
        Hit::Row(i) => p.select(Some(i)),
        Hit::Inside | Hit::Outside => {}
    }
    redraw(hwnd);
}

unsafe fn panel_key(hwnd: HWND, s: &mut State, vk: u32) -> bool {
    let (w, h) = client_size(hwnd);
    let rows = PanelLayout::new(w, h, s.dpi).visible_rows();
    let Some(p) = s.panel.as_mut() else { return false };
    match vk as u16 {
        VK_ESCAPE => unsafe { close_panel(hwnd, s) },
        VK_UP => p.step(-1, rows),
        VK_DOWN => p.step(1, rows),
        VK_DELETE => {
            p.remove_selected();
        }
        VK_INSERT => unsafe { panel_add(hwnd, s) },
        _ => return true, // 모달: 다른 키는 삼킨다.
    }
    redraw(hwnd);
    true
}

unsafe fn on_click(hwnd: HWND, s: &mut State, x: i32, y: i32, right: bool) {
    unsafe {
        if s.panel.is_some() {
            panel_click(hwnd, s, x, y);
            return;
        }
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
                s.panel = Some(ExcludePanel::open(&s.settings_path));
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
        if s.panel.is_some() {
            return panel_key(hwnd, s, vk);
        }
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
    if s.panel.is_some() || !s.app.search_focus {
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
                    if let Some(p) = s.panel.as_mut() {
                        p.wheel(-notches / 120 * 3, PanelLayout::new(w, h, s.dpi).visible_rows());
                        redraw(hwnd);
                        return 0;
                    }
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
            WM_SHELL_THUMB => {
                if let Some(s) = state(hwnd) { receive_shell_thumbs(s); redraw(hwnd); }
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
                    s.notify_hwnd.store(0, Ordering::Relaxed);
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

/// 단일 인스턴스: 같은 설정 파일을 여는 사진뷰가 이미 떠 있으면 그 창을 앞으로 가져오고 true(이 프로세스는 곧 끝낸다).
/// 뮤텍스 이름에 설정 파일 경로의 해시를 붙여, 다른 설정 파일(시험·캡처)을 여는 창과는 섞이지 않는다.
pub fn activate_existing_instance(settings_path: &str) -> bool {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    unsafe {
        let mut hash: u64 = 0xcbf29ce484222325;
        for b in settings_path.to_lowercase().bytes() {
            hash = (hash ^ b as u64).wrapping_mul(0x100000001b3);
        }
        let name = format!(r"Local\HwpAutoDocFit.PhotoViewWin.Singleton.{hash:016x}");
        // 핸들은 일부러 닫지 않는다: 프로세스가 끝날 때까지 뮤텍스를 쥐고 있어야 한다.
        let handle = CreateMutexW(null(), 0, wide(&name).as_ptr());
        if handle.is_null() || GetLastError() != ERROR_ALREADY_EXISTS {
            return false;
        }
        let existing = FindWindowW(wide("PhotoViewWinMain").as_ptr(), null());
        if !existing.is_null() {
            if IsIconic(existing) != 0 {
                ShowWindow(existing, SW_RESTORE);
            }
            SetForegroundWindow(existing);
        }
        true
    }
}

pub fn run(opts: Options) -> Result<(), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let hinst = GetModuleHandleW(null());
        let class = wide("PhotoViewWinMain");
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
        // 이미 같은 설정 파일로 열린 사진뷰가 있으면 그 창을 앞으로 가져오고 이 프로세스는 끝낸다.
        if activate_existing_instance(&path.to_string_lossy()) {
            return Ok(());
        }
        // 앱 설정은 설정 소유자 docfit_config 로 읽는다(썸네일 크기·제외 폴더). 읽지 못하면 기본값 + 상태줄 안내.
        let loaded = crate::settings::load(&path);
        let mut app = crate::synth::new_app(loaded.thumb);
        let excludes = loaded.excluded;
        app.excluded_count = excludes.len();
        if let Some(e) = loaded.error {
            app.notice = e;
        }
        app.tz_offset = tz_offset_secs();
        let mut r = RECT { left: 0, top: 0, right: scale(DEFAULT_W, dpi), bottom: scale(DEFAULT_H, dpi) };
        AdjustWindowRectExForDpi(&mut r, WS_OVERLAPPEDWINDOW, 0, 0, dpi);
        let no_scan = opts.no_scan;
        let notify_hwnd = Arc::new(AtomicUsize::new(0));
        let worker_hwnd = Arc::clone(&notify_hwnd);
        let thumb_service = grid_core::shell_thumb::ThumbnailService::new(4, Arc::new(move || {
            let target = worker_hwnd.load(Ordering::Relaxed);
            if target != 0 { PostMessageW(target as HWND, WM_SHELL_THUMB, 0, 0); }
        }));
        let raw = Box::into_raw(Box::new(State { app, pal, pref, dpi, job: None, drag: Drag::None, high: None, opts, excludes, settings_path: path.clone(), panel: None, rescan: false, thumb_service, thumbs: HashMap::new(), thumb_order: VecDeque::new(), thumb_bytes: 0, notify_hwnd }));
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            wide("사진뷰").as_ptr(),
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
        (*raw).notify_hwnd.store(hwnd as usize, Ordering::Relaxed);
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
