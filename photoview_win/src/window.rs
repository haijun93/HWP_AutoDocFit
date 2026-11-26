//! 실제 창: 메시지 처리·마우스·키보드·스캔 스레드 연결. 그림은 `paint::draw`, 값 처리는 `model::App`.
use crate::extras::{Command, EDIT_DEFAULT, EDIT_HELP, Photo, Row2, Viewer, ViewerHit, ViewerLayout, parse_edit};
use crate::layout::{DEFAULT_H, DEFAULT_W, MIN_H, MIN_W, Toolbar, scale};
use crate::library::{self, Library};
use crate::tools::{self, Loader, ToolJob};
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
const TIMER_FILE_OPS: usize = 2;
const TIMER_SLIDE: usize = 3;
const WM_VIEW: u32 = WM_APP + 3;
const WM_TOOL: u32 = WM_APP + 4;
/// 슬라이드쇼 간격(앱 화면과 같음).
const SLIDE_MS: u32 = 3000;
/// 오른쪽 메뉴의 사진 줄.
const MENU_COMMANDS: [(Command, &str); 5] = [
    (Command::View, "크게 보기\tSpace"),
    (Command::Favorite, "즐겨찾기 켜기·끄기\tCtrl+D"),
    (Command::Tags, "태그…\tCtrl+T"),
    (Command::Edit, "사진 편집(사본)…"),
    (Command::Export, "공문 첨부용 사본 저장…"),
];

#[derive(Clone, Default)]
pub struct Options {
    /// 설정 파일(없으면 기본 위치). 읽기만 한다.
    pub settings: Option<PathBuf>,
    pub klib: Option<PathBuf>,
    /// 합성 목록 N 개를 비동기 스캔처럼 흘려 보낸다(klib 없이 창·진행·취소 시험).
    pub synthetic: Option<usize>,
    pub no_scan: bool,
    /// 즐겨찾기·태그 DB(없으면 앱과 같은 기본 위치. 합성 목록에는 쓰지 않는다).
    pub photo_db: Option<PathBuf>,
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
    file_ops: grid_core::native_ops::NativeOps,
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
    photo: Photo,
    library: Option<Library>,
    loader: Loader,
    tool: Option<ToolJob>,
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
    if s.job.as_ref().is_some() || s.file_ops.busy() {
        return;
    }
    if s.tool.is_some() {
        s.app.open_error = "사진 작업이 끝난 뒤 다시 검색해 주세요".into();
        redraw(hwnd);
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
            // 다시 훑으면 중복 묶음은 낡는다. 즐겨찾기·태그는 앱에서 바꾼 것까지 새로 읽는다.
            s.photo.duplicates = None;
            s.photo.status.clear();
            reload_marks(s);
            s.app.coll.replace(Vec::new());
            refilter(s);
            s.app.retired_paths.clear();
            s.app.clear_selection();
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
        // 새로 들어온 사진에도 보기 조건을 적용한다.
        if s.photo.filtering() {
            refilter(s);
        }
        if done {
            KillTimer(hwnd, TIMER_ELAPSED);
            if let Some(mut j) = s.job.take() {
                j.join();
            }
            if s.rescan && !s.file_ops.busy() {
                s.rescan = false;
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
            paint::draw_all(mem, w, h, &s.app, &s.photo, s.panel.as_ref(), &s.pal, s.dpi, &s.thumbs);
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
    if s.job.is_some() || s.file_ops.busy() || s.tool.is_some() {
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
        s.app.clicked = None;
        if s.panel.is_some() {
            panel_click(hwnd, s, x, y);
            return;
        }
        let (w, h) = client_size(hwnd);
        let tb = Toolbar::new(w, s.dpi);
        s.app.search_focus = false;
        let row = Row2::new(w, s.dpi);
        if row.bar.contains(x, y) {
            if let Some(command) = row.hit(x, y).filter(|_| !right) {
                PostMessageW(hwnd, WM_COMMAND, command_id(command), 0);
            }
            redraw(hwnd);
            return;
        }
        if tb.bar.contains(x, y) {
            if tb.search.contains(x, y) {
                s.app.search_focus = true;
            } else if tb.sort.contains(x, y) {
                if right || ctrl_down() || GetKeyState(VK_SHIFT as i32) < 0 {
                    s.app.coll.toggle_direction();
                } else {
                    s.app.coll.cycle_sort();
                }
                s.app.clear_selection();
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
        if right {
            s.app.click_modified(&m, m.hit(x, y, s.app.scroll, n), false, false, true);
            redraw(hwnd);
            return;
        }
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
        s.app.click_modified(&m, m.hit(x, y, s.app.scroll, n), ctrl_down(), GetKeyState(VK_SHIFT as i32) < 0, right);
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
                    s.app.clear_selection();
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
        let command = match vk as u16 {
            VK_SPACE => Some(Command::View),
            0x44 if ctrl_down() => Some(Command::Favorite),
            0x54 if ctrl_down() => Some(Command::Tags),
            _ => None,
        };
        if let Some(command) = command {
            PostMessageW(hwnd, WM_COMMAND, command_id(command), 0);
            return true;
        }
        if vk == 0x41 && ctrl_down() {
            s.app.select_all();
        } else if vk as u16 == VK_ESCAPE && s.photo.duplicates.is_some() {
            s.photo.duplicates = None;
            s.photo.status.clear();
            s.app.clear_selection();
            refilter(s);
        } else if vk as u16 == VK_ESCAPE {
            if s.app.selection.paths.is_empty() && s.job.is_some() { cancel_scan(hwnd, s); }
            s.app.clear_selection();
        } else if let Some(n) = nav {
            s.app.nav(&m, n);
        } else if vk == 0x46 && ctrl_down() {
            s.app.search_focus = true; // Ctrl+F
        } else if vk as u16 == VK_F5 {
            start_scan(hwnd, s);
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
    s.app.clear_selection();
    s.app.scroll = 0;
    redraw(hwnd);
}

/// 연결 앱으로 연다. 셸 호출 중 창 메시지가 다시 들어올 수 있어 상태는 호출이 끝난 뒤에 다시 얻는다.
unsafe fn open_item(hwnd: HWND, path: &str) {
    let result = grid_core::open::open_with(path, &crate::scan::PHOTO_EXTENSIONS, |p| grid_core::open::shell_open(hwnd as isize, p));
    if let Some(s) = unsafe { state(hwnd) } {
        s.app.open_error = result.err().unwrap_or_default();
        redraw(hwnd);
    }
}

/// 공용 메뉴·대화상자를 부르기 전 State 참조를 놓아 모달 메시지 재진입을 보호한다.
unsafe fn file_action(hwnd: HWND, action: grid_core::native_ops::Action) {
    use grid_core::native_ops::{self, Phase, Action};
    let docs = unsafe { state(hwnd) }.and_then(|s| {
        let docs = s.app.selected_docs();
        if s.panel.is_some() || s.app.search_focus || s.photo.viewer.is_some() || !action.enabled(docs.len(), s.file_ops.busy()) { return None; }
        if s.tool.is_some() && action != Action::Open {
            s.app.open_error = "사진 작업이 끝난 뒤 다시 시도해 주세요".into();
            redraw(hwnd);
            return None;
        }
        s.file_ops.phase = Phase::Preparing;
        Some(docs)
    });
    let Some(docs) = docs else { return };
    if action == Action::Open {
        unsafe { open_item(hwnd, &docs[0].path); }
        if let Some(s) = unsafe { state(hwnd) } { s.file_ops.phase = Phase::Complete; }
        return;
    }
    let prepared = native_ops::prepare(hwnd, action, docs, &crate::scan::PHOTO_EXTENSIONS);
    if let Some(s) = unsafe { state(hwnd) } {
        s.file_ops.phase = Phase::Complete;
        if let Some(request) = prepared {
            match s.file_ops.start(hwnd, request) {
                Ok(()) => {
                    s.app.file_status = "파일 작업 중".into();
                    unsafe { SetTimer(hwnd, TIMER_FILE_OPS, 100, None); }
                }
                Err(e) => s.app.open_error = e,
            }
        }
        redraw(hwnd);
    }
}

unsafe fn context_menu(hwnd: HWND, lp: LPARAM) {
    use grid_core::native_ops::{self, Phase};
    let (mut x, mut y) = xy(lp);
    let snapshot = unsafe { state(hwnd) }.and_then(|s| {
        if s.panel.is_some() || s.photo.viewer.is_some() || s.app.search_focus || s.file_ops.busy() { return None; }
        if x == -1 && y == -1 {
            // 메뉴 키·Shift+F10: 초점 카드 옆에 연다.
            let (w, h) = client_size(hwnd);
            let m = s.app.metrics(w, h, s.dpi);
            let r = m.item_rect(s.app.selected.unwrap_or(0), s.app.scroll);
            let mut p = POINT { x: r.left, y: r.top };
            unsafe { ClientToScreen(hwnd, &mut p); }
            x = p.x; y = p.y;
        } else {
            let mut p = POINT { x, y };
            unsafe { ScreenToClient(hwnd, &mut p); }
            let (w, h) = client_size(hwnd);
            let m = s.app.metrics(w, h, s.dpi);
            let hit = m.hit(p.x, p.y, s.app.scroll, s.app.coll.len());
            // 도구줄·빈 곳에서는 항목 메뉴를 열지 않는다.
            if hit.is_none() { return None; }
        }
        let paths: Vec<String> = s.app.selected_docs().into_iter().map(|d| d.path).collect();
        if paths.is_empty() { return None; }
        s.app.clicked = None;
        s.file_ops.phase = Phase::Preparing;
        Some((paths, s.settings_path.clone()))
    });
    let Some((paths, settings)) = snapshot else { return };
    // 파일 작업 아래에 '다른 창으로 보내기'. 선택한 파일 전부가 대상이다.
    let send = grid_core::send_menu::lines(&grid_core::handoff::PHOTOVIEW_MENUS, &paths, &settings);
    let extra: Vec<native_ops::Extra> = MENU_COMMANDS.iter().enumerate()
        .map(|(i, (command, label))| native_ops::Extra { id: i as u8, label: label.to_string(), enabled: command.enabled(paths.len()) })
        .collect();
    let choice = native_ops::menu(hwnd, x, y, paths.len(), false, &send, &extra);
    if let Some(s) = unsafe { state(hwnd) } { s.file_ops.phase = Phase::Complete; }
    match choice {
        Some(native_ops::Choice::File(action)) => unsafe { file_action(hwnd, action) },
        Some(native_ops::Choice::Send(menu)) => {
            let result = grid_core::send_menu::send(menu, &paths, &settings);
            if let (Err(message), Some(s)) = (result, unsafe { state(hwnd) }) {
                s.app.open_error = message;
                redraw(hwnd);
            }
        }
        Some(native_ops::Choice::Extra(n)) => {
            if let Some((command, _)) = MENU_COMMANDS.get(n as usize) {
                unsafe { photo_command(hwnd, *command) };
            }
        }
        None => {}
    }
}

unsafe fn poll_file_ops(hwnd: HWND) {
    let completion = unsafe { state(hwnd) }.and_then(|s| {
        let done = s.file_ops.poll();
        if let Some(progress) = s.file_ops.progress() { s.app.file_status = progress; }
        let (op, results) = done?;
        for result in results.iter().filter(|r| r.error.is_none()) {
            for path in std::iter::once(&result.source.path).chain(result.target.as_ref().map(|d| &d.path)) {
                if let Some(old) = s.thumbs.remove(path) { s.thumb_bytes = s.thumb_bytes.saturating_sub(old.bgra.len()); }
                s.thumb_order.retain(|p| p != path);
                s.thumb_service.forget(std::path::Path::new(path));
            }
        }
        let db_error = sync_library(s, op, &results);
        s.photo.follow_renames(op, &results);
        let excludes = &s.excludes;
        s.app.apply_file_results(op, &results, |d| grid_core::native_ops::listed_destination(&d.path, excludes));
        let failed = results.iter().filter(|r| r.error.is_some()).count();
        s.app.file_status = format!("파일 작업 완료: 성공 {}개 · 실패 {}개", results.len() - failed, failed);
        s.app.open_error = db_error.unwrap_or_default();
        reload_marks(s);
        refilter(s);
        let (w, h) = client_size(hwnd);
        s.app.clamp_scroll(&s.app.metrics(w, h, s.dpi));
        unsafe { KillTimer(hwnd, TIMER_FILE_OPS); }
        s.file_ops.phase = grid_core::native_ops::Phase::Preparing;
        Some(results)
    });
    redraw(hwnd);
    if let Some(results) = completion {
        grid_core::native_ops::show_results(hwnd, &results);
        if let Some(s) = unsafe { state(hwnd) } {
            s.file_ops.phase = grid_core::native_ops::Phase::Complete;
            if s.rescan && s.job.is_none() { s.rescan = false; unsafe { start_scan(hwnd, s); } }
        }
    }
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
            WM_COMMAND if lp == 0 => {
                if let Some(command) = Command::ALL.into_iter().find(|c| command_id(*c) == wp) {
                    photo_command(hwnd, command);
                }
                0
            }
            // 크게 보기 중의 마우스·휠·키.
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK if state(hwnd).is_some_and(|s| s.photo.viewer.is_some()) => {
                let (x, y) = xy(lp);
                SetFocus(hwnd);
                let (w, h) = client_size(hwnd);
                let hit = state(hwnd).and_then(|s| ViewerLayout::new(w, h, s.dpi).hit(x, y));
                if let Some(hit) = hit {
                    viewer_action(hwnd, hit);
                }
                0
            }
            WM_MOUSEWHEEL if state(hwnd).is_some_and(|s| s.photo.viewer.is_some()) => {
                let up = ((wp >> 16) as u16 as i16) > 0;
                viewer_action(hwnd, if up { ViewerHit::Prev } else { ViewerHit::Next });
                0
            }
            WM_RBUTTONDOWN if state(hwnd).is_some_and(|s| s.photo.viewer.is_some()) => 0,
            WM_KEYDOWN if state(hwnd).is_some_and(|s| s.photo.viewer.is_some()) => {
                viewer_key(hwnd, wp as u32, lp & (1 << 30) != 0);
                0
            }
            WM_VIEW => {
                if let Some(s) = state(hwnd) {
                    receive_view(s);
                    redraw(hwnd);
                }
                0
            }
            WM_TOOL => {
                poll_tool(hwnd);
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
            // 항목 위 더블클릭만 연다(도구줄·빈 곳·패널에서는 `clicked` 가 비어 있다).
            WM_CONTEXTMENU => {
                let (x, y) = xy(lp);
                if grid_core::native_ops::client_context(hwnd, x, y) { context_menu(hwnd, lp); 0 }
                else { DefWindowProcW(hwnd, msg, wp, lp) }
            }
            WM_LBUTTONDBLCLK => {
                let target = state(hwnd).and_then(|s| s.app.clicked.take());
                if let Some(path) = target {
                    open_item(hwnd, &path);
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
                    if s.drag == Drag::Slider && s.opts.synthetic.is_none() {
                        if let Err(e) = docfit_config::set_thumb_size(&s.settings_path, "photo_album", s.app.thumb) {
                            s.app.open_error = format!("크기를 저장하지 못했습니다({})", e.message);
                            redraw(hwnd);
                        }
                    }
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
                let action = state(hwnd).filter(|s| s.panel.is_none() && !s.app.search_focus)
                    .and_then(|_| grid_core::native_ops::shortcut(wp as u32, ctrl_down(), GetKeyState(VK_SHIFT as i32) < 0));
                if let Some(action) = action {
                    if lp & (1 << 30) == 0 { file_action(hwnd, action); }
                    return 0;
                }
                // Enter = 선택 항목 열기. 누르고 있어 반복되는 입력(lp 30번 비트)은 무시한다.
                if wp as u16 == VK_RETURN && lp & (1 << 30) == 0 {
                    let target = state(hwnd).filter(|s| s.panel.is_none()).and_then(|s| s.app.enter_target());
                    if let Some(path) = target {
                        open_item(hwnd, &path);
                        return 0;
                    }
                }
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
                if wp == TIMER_FILE_OPS { poll_file_ops(hwnd); return 0; }
                if wp == TIMER_SLIDE { viewer_action(hwnd, ViewerHit::Next); return 0; }
                if let Some(s) = state(hwnd) {
                    pump(hwnd, s);
                }
                0
            }
            WM_CLOSE => {
                if let Some(s) = state(hwnd) {
                    if s.file_ops.busy() {
                        s.app.open_error = "파일 작업을 마친 뒤 창을 닫아 주세요".into();
                        redraw(hwnd);
                        return 0;
                    }
                    if let Some(tool) = &s.tool {
                        // 찾기는 버려도 되지만, 사본을 쓰는 중에는 반쪽 파일이 남지 않게 기다린다.
                        if tool.kind != tools::Kind::Duplicates {
                            s.app.open_error = "사본 저장을 마친 뒤 창을 닫아 주세요".into();
                            redraw(hwnd);
                            return 0;
                        }
                        tool.cancel();
                    }
                }
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

fn command_id(command: Command) -> usize {
    1000 + command as usize
}

/// 배경 스레드가 창을 깨우는 알림.
fn poster(target: Arc<AtomicUsize>, message: u32) -> Arc<dyn Fn() + Send + Sync> {
    Arc::new(move || {
        let hwnd = target.load(Ordering::Relaxed);
        if hwnd != 0 {
            unsafe { PostMessageW(hwnd as HWND, message, 0, 0) };
        }
    })
}

/// 즐겨찾기·태그를 DB 에서 다시 읽는다. 읽지 못하면 이전 표시를 두고 알린다.
fn reload_marks(s: &mut State) {
    match s.library.as_ref().map(Library::marks) {
        Some(Ok(marks)) => s.photo.marks = marks,
        Some(Err(e)) => s.app.open_error = e,
        None => {}
    }
}

/// 보기 조건을 목록에 반영한다. 선택은 같은 경로에 머물고, 가려진 사진은 선택에서 뺀다.
fn refilter(s: &mut State) {
    let focus = s.app.selected_doc().map(|d| d.path.clone());
    let only = s.photo.only(&s.app.coll.docs);
    if let Some(only) = &only {
        s.app.selection.paths.retain(|p| only.rank.contains_key(p));
    }
    s.app.coll.set_only(only);
    s.app.selected = focus.and_then(|p| (0..s.app.coll.len()).find(|i| s.app.coll.get(*i).is_some_and(|d| d.path == p)));
}

/// 파일 작업 결과를 즐겨찾기·태그에도 반영한다(앱의 `_after_file_op`). 실패하면 안내 문장.
fn sync_library(s: &State, op: grid_core::file_ops::Operation, results: &[grid_core::file_ops::FileResult]) -> Option<String> {
    use grid_core::file_ops::Operation;
    let library = s.library.as_ref()?;
    let mut error = None;
    for result in results.iter().filter(|r| r.error.is_none()) {
        let source = result.source.path.as_str();
        let done = match (op, &result.target) {
            (Operation::Delete, _) => library.delete(source),
            (Operation::Move | Operation::Rename, Some(target)) => library.transfer(source, &target.path, false),
            (Operation::Copy, Some(target)) if grid_core::native_ops::listed_destination(&target.path, &s.excludes) => {
                library.transfer(source, &target.path, true)
            }
            _ => Ok(()),
        };
        if let Err(e) = done {
            error = Some(e);
        }
    }
    error
}

/// 크게 보기의 현재 사진을 읽게 한다. 클라우드에만 있는 사진은 읽지 않는다.
fn load_current(s: &mut State) {
    let Some(v) = s.photo.viewer.as_mut() else { return };
    if v.current().cloud {
        v.image = Some(Err("클라우드에만 있는 사진은 열지 않습니다".into()));
    } else {
        let path = PathBuf::from(&v.current().path);
        s.loader.request(path);
    }
}

fn receive_view(s: &mut State) {
    for (path, image) in s.loader.drain() {
        if let Some(v) = s.photo.viewer.as_mut() {
            if std::path::Path::new(&v.current().path) == path {
                v.image = Some(image.map(Arc::new));
            }
        }
    }
}

unsafe fn open_viewer(hwnd: HWND, s: &mut State) {
    let Some(focus) = s.app.selected_doc().map(|d| d.path.clone()) else { return };
    let items: Vec<_> = (0..s.app.coll.len()).filter_map(|i| s.app.coll.get(i).cloned()).collect();
    let Some(index) = items.iter().position(|d| d.path == focus) else { return };
    s.photo.viewer = Viewer::open(items, index);
    s.app.clicked = None;
    load_current(s);
    redraw(hwnd);
}

unsafe fn close_viewer(hwnd: HWND, s: &mut State) {
    let Some(v) = s.photo.viewer.take() else { return };
    unsafe { KillTimer(hwnd, TIMER_SLIDE) };
    // 마지막으로 본 사진을 선택해 둔다.
    let path = v.current().path.clone();
    let (w, h) = client_size(hwnd);
    let m = s.app.metrics(w, h, s.dpi);
    if let Some(i) = (0..s.app.coll.len()).find(|i| s.app.coll.get(*i).is_some_and(|d| d.path == path)) {
        s.app.select(&m, Some(i));
    }
    redraw(hwnd);
}

unsafe fn viewer_action(hwnd: HWND, hit: ViewerHit) {
    let open = unsafe { state(hwnd) }.and_then(|s| {
        let v = s.photo.viewer.as_mut()?;
        match hit {
            ViewerHit::Prev | ViewerHit::Next => {
                if v.step(hit == ViewerHit::Next) {
                    s.app.open_error.clear();
                    load_current(s);
                }
            }
            ViewerHit::Play => {
                v.playing = !v.playing;
                unsafe {
                    if v.playing {
                        SetTimer(hwnd, TIMER_SLIDE, SLIDE_MS, None);
                    } else {
                        KillTimer(hwnd, TIMER_SLIDE);
                    }
                }
            }
            ViewerHit::Close => unsafe { close_viewer(hwnd, s) },
            ViewerHit::Open => return Some(v.current().path.clone()),
            ViewerHit::Favorite | ViewerHit::Tags | ViewerHit::Edit => {}
        }
        None
    });
    redraw(hwnd);
    if let Some(path) = open {
        unsafe { open_item(hwnd, &path) };
    }
    match hit {
        ViewerHit::Favorite => unsafe { photo_command(hwnd, Command::Favorite) },
        ViewerHit::Tags => unsafe { photo_command(hwnd, Command::Tags) },
        ViewerHit::Edit => unsafe { photo_command(hwnd, Command::Edit) },
        _ => {}
    }
}

unsafe fn viewer_key(hwnd: HWND, vk: u32, repeated: bool) {
    let hit = match vk as u16 {
        VK_LEFT | VK_PRIOR => Some(ViewerHit::Prev),
        VK_RIGHT | VK_NEXT => Some(ViewerHit::Next),
        VK_ESCAPE => Some(ViewerHit::Close),
        VK_SPACE if !repeated => Some(ViewerHit::Play),
        VK_RETURN if !repeated => Some(ViewerHit::Open),
        0x44 if ctrl_down() && !repeated => Some(ViewerHit::Favorite),
        0x54 if ctrl_down() && !repeated => Some(ViewerHit::Tags),
        _ => None,
    };
    if let Some(hit) = hit {
        unsafe { viewer_action(hwnd, hit) };
    }
}

/// 마우스 위치에 고르는 메뉴를 띄운다. 고른 줄 번호.
unsafe fn pick(hwnd: HWND, lines: &[(String, bool)]) -> Option<usize> {
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return None;
        }
        for (i, (label, checked)) in lines.iter().enumerate() {
            let flags = MF_STRING | if *checked { MF_CHECKED } else { 0 };
            AppendMenuW(menu, flags, i + 1, wide(&label.replace('&', "&&")).as_ptr());
        }
        let mut at = POINT { x: 0, y: 0 };
        GetCursorPos(&mut at);
        let chosen = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, at.x, at.y, 0, hwnd, null());
        DestroyMenu(menu);
        usize::try_from(chosen).ok()?.checked_sub(1)
    }
}

/// 태그 메뉴·입력창. State 참조 없이 부른다. 바뀌었으면 true.
unsafe fn edit_tags(hwnd: HWND, library: &Library, paths: &[String], current: &[String]) -> Result<bool, String> {
    let mut lines = vec![("태그 추가…".to_string(), false)];
    lines.extend(current.iter().map(|t| (format!("'{t}' 태그 빼기"), false)));
    let Some(choice) = (unsafe { pick(hwnd, &lines) }) else { return Ok(false) };
    if choice == 0 {
        let prompt = format!("선택한 사진 {}장에 붙일 태그", paths.len());
        let Some(tag) = grid_core::native_ops::text_dialog(hwnd, "태그 추가", &prompt, "", true) else { return Ok(false) };
        for path in paths {
            library.add_tag(path, &tag)?;
        }
    } else {
        for path in paths {
            library.remove_tag(path, &current[choice - 1])?;
        }
    }
    Ok(true)
}

/// 모달 메뉴·대화상자 동안 파일 작업·다른 명령이 끼어들지 못하게 한다.
unsafe fn modal(hwnd: HWND, on: bool) {
    use grid_core::native_ops::Phase;
    if let Some(s) = unsafe { state(hwnd) } {
        s.file_ops.phase = if on { Phase::Preparing } else { Phase::Complete };
        if on {
            if let Some(v) = s.photo.viewer.as_mut() {
                v.playing = false;
                unsafe { KillTimer(hwnd, TIMER_SLIDE) };
            }
        }
    }
}

/// 둘째 도구줄·오른쪽 메뉴·단축키·크게 보기의 사진 기능.
unsafe fn photo_command(hwnd: HWND, command: Command) {
    // 필요한 것만 복사해 두고 State 참조를 놓는다(메뉴·대화상자가 메시지 루프를 돈다).
    let prepared = unsafe { state(hwnd) }.and_then(|s| {
        if s.panel.is_some() || s.file_ops.busy() {
            return None;
        }
        let docs = match &s.photo.viewer {
            Some(v) => vec![v.current().clone()],
            None => s.app.selected_docs(),
        };
        if !command.enabled(docs.len()) {
            return None;
        }
        s.app.open_error.clear();
        s.app.search_focus = false;
        Some(docs)
    });
    let Some(docs) = prepared else {
        redraw(hwnd);
        return;
    };
    let paths: Vec<String> = docs.iter().map(|d| d.path.clone()).collect();
    match command {
        Command::View => {
            if let Some(s) = unsafe { state(hwnd) } {
                unsafe { open_viewer(hwnd, s) };
            }
        }
        Command::FavoritesOnly => {
            if let Some(s) = unsafe { state(hwnd) } {
                s.photo.favorites_only = !s.photo.favorites_only;
                s.app.scroll = 0;
                unsafe { apply_view(hwnd, s) };
            }
        }
        Command::TagFilter => {
            let Some((tags, filter)) = (unsafe { state(hwnd) }).map(|s| {
                let listed = s.app.coll.docs.iter().map(|d| d.path.as_str());
                (library::tag_counts(&s.photo.marks, listed), s.photo.tag.clone())
            }) else { return };
            let mut lines = vec![("태그 전체".to_string(), filter.is_none())];
            lines.extend(tags.iter().map(|(t, n)| (format!("{t} ({n})"), filter.as_ref() == Some(t))));
            unsafe { modal(hwnd, true) };
            let choice = unsafe { pick(hwnd, &lines) };
            unsafe { modal(hwnd, false) };
            if let (Some(choice), Some(s)) = (choice, unsafe { state(hwnd) }) {
                s.photo.tag = choice.checked_sub(1).map(|i| tags[i].0.clone());
                s.app.scroll = 0;
                unsafe { apply_view(hwnd, s) };
            }
        }
        Command::Favorite | Command::Tags => {
            let Some((library, flips, current)) = (unsafe { state(hwnd) }).map(|s| {
                (s.library.clone(), s.photo.favorite_flips(&paths), s.photo.tags_of(&paths))
            }) else { return };
            let result = match &library {
                None => Err("즐겨찾기·태그를 저장할 곳이 없습니다".to_string()),
                Some(library) if command == Command::Favorite => {
                    flips.iter().try_for_each(|p| library.toggle_favorite(p).map(|_| ()))
                }
                Some(library) => {
                    unsafe { modal(hwnd, true) };
                    let result = unsafe { edit_tags(hwnd, library, &paths, &current) };
                    unsafe { modal(hwnd, false) };
                    result.map(|_| ())
                }
            };
            if let Some(s) = unsafe { state(hwnd) } {
                reload_marks(s);
                if let Err(e) = result {
                    s.app.open_error = e;
                }
                unsafe { apply_view(hwnd, s) };
            }
        }
        Command::Duplicates => {
            if let Some(s) = unsafe { state(hwnd) } {
                if let Some(tool) = &s.tool {
                    if tool.kind == tools::Kind::Duplicates {
                        tool.cancel();
                    } else {
                        s.app.open_error = "사진 작업이 끝난 뒤 다시 시도해 주세요".into();
                    }
                } else if s.photo.duplicates.take().is_some() {
                    s.photo.status.clear();
                    s.app.clear_selection();
                    unsafe { apply_view(hwnd, s) };
                } else if s.job.is_some() {
                    s.app.open_error = "검색이 끝난 뒤 중복을 찾아 주세요".into();
                } else if !s.app.coll.docs.is_empty() {
                    s.photo.searching = true;
                    s.photo.status = "중복 찾는 중".into();
                    let notify = poster(Arc::clone(&s.notify_hwnd), WM_TOOL);
                    s.tool = Some(ToolJob::duplicates(s.app.coll.docs.clone(), notify));
                }
                redraw(hwnd);
            }
        }
        Command::Export | Command::Edit => unsafe { start_copy(hwnd, command, &docs) },
    }
}

/// 첨부용 사본(폴더를 고른다)·사본 편집(값을 입력한다).
unsafe fn start_copy(hwnd: HWND, command: Command, docs: &[crate::model::Doc]) {
    enum Request {
        Export(PathBuf),
        Edit(crate::extras::EditSpec),
    }
    if let Some(s) = unsafe { state(hwnd) } {
        if s.tool.is_some() {
            s.app.open_error = "사진 작업이 끝난 뒤 다시 시도해 주세요".into();
            redraw(hwnd);
            return;
        }
    }
    unsafe { modal(hwnd, true) };
    let request = if command == Command::Export {
        grid_core::native_ops::pick_folder(hwnd).map(|folder| Ok(Request::Export(folder)))
    } else if docs[0].cloud {
        Some(Err("클라우드에만 있는 사진은 편집할 수 없습니다".to_string()))
    } else {
        grid_core::native_ops::text_dialog(hwnd, "사진 편집(사본)", EDIT_HELP, EDIT_DEFAULT, true)
            .map(|text| parse_edit(&text).map(Request::Edit))
    };
    unsafe { modal(hwnd, false) };
    if let Some(s) = unsafe { state(hwnd) } {
        let notify = poster(Arc::clone(&s.notify_hwnd), WM_TOOL);
        match request {
            Some(Ok(Request::Export(folder))) => {
                let paths = docs.iter().map(|d| PathBuf::from(&d.path)).collect();
                s.tool = Some(ToolJob::export(paths, folder, notify));
            }
            Some(Ok(Request::Edit(spec))) => s.tool = Some(ToolJob::edit(PathBuf::from(&docs[0].path), spec, notify)),
            Some(Err(e)) => s.app.open_error = e,
            None => {}
        }
        redraw(hwnd);
    }
}

/// 보기 조건·표시가 바뀐 뒤 목록과 스크롤을 맞춘다.
unsafe fn apply_view(hwnd: HWND, s: &mut State) {
    refilter(s);
    let (w, h) = client_size(hwnd);
    let m = s.app.metrics(w, h, s.dpi);
    s.app.clamp_scroll(&m);
    redraw(hwnd);
}

/// 방금 만든 사본을 목록에 더한다.
fn add_output(s: &mut State, path: &std::path::Path) {
    let Ok(meta) = std::fs::metadata(path) else { return };
    let text = path.to_string_lossy().into_owned();
    if s.app.coll.docs.iter().any(|d| d.path == text) {
        return;
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0.0, |d| d.as_secs_f64());
    s.app.extend_docs(vec![crate::model::Doc { path: text, name, ext, size: meta.len(), mtime, cloud: false }]);
}

unsafe fn poll_tool(hwnd: HWND) {
    let mut failed = Vec::new();
    if let Some(s) = unsafe { state(hwnd) } {
        let events = s.tool.as_ref().map(ToolJob::drain).unwrap_or_default();
        for event in events {
            match event {
                tools::Event::Progress(text) => s.photo.status = text,
                tools::Event::Duplicates { groups, read, skipped, cancelled } => {
                    s.tool = None;
                    s.photo.searching = false;
                    let skipped = if skipped > 0 { format!(" · 건너뜀 {skipped}장") } else { String::new() };
                    s.photo.status = if cancelled {
                        "중복 찾기를 취소했습니다".into()
                    } else if groups.is_empty() {
                        format!("비슷한 사진이 없습니다({read}장 확인{skipped})")
                    } else {
                        let message = format!("비슷한 사진 묶음 {}개({read}장 확인{skipped}) · Esc 로 닫기", groups.len());
                        s.photo.duplicates = Some(groups);
                        s.app.clear_selection();
                        s.app.scroll = 0;
                        message
                    };
                    refilter(s);
                }
                tools::Event::Saved { outputs, failures, edited } => {
                    s.tool = None;
                    if edited {
                        for path in &outputs {
                            add_output(s, path);
                        }
                        refilter(s);
                    }
                    s.photo.status = match outputs.first() {
                        Some(first) if edited => format!("사본을 저장했습니다: {}", first.display()),
                        _ => format!("사본 저장 {}장 · 실패 {}장", outputs.len(), failures.len()),
                    };
                    failed = failures;
                }
            }
        }
    }
    redraw(hwnd);
    // 작업 중에 바뀐 제외 폴더가 있으면 이제 다시 검색한다.
    if let Some(s) = unsafe { state(hwnd) } {
        if s.rescan && s.tool.is_none() && s.job.is_none() {
            s.rescan = false;
            unsafe { start_scan(hwnd, s) };
        }
    }
    if !failed.is_empty() {
        let text = failed.iter().map(|(p, e)| format!("{p}\r\n{e}")).collect::<Vec<_>>().join("\r\n\r\n");
        unsafe { modal(hwnd, true) };
        grid_core::native_ops::text_dialog(hwnd, "사본 저장 결과", &format!("실패 {}개", failed.len()), &text, false);
        unsafe { modal(hwnd, false) };
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
        let library = opts.photo_db.clone()
            .or_else(|| opts.synthetic.is_none().then(library::default_db_path).flatten())
            .map(Library::new);
        let loader = Loader::new(poster(Arc::clone(&notify_hwnd), WM_VIEW));
        let raw = Box::into_raw(Box::new(State { photo: Photo::default(), library, loader, tool: None, app, file_ops: Default::default(), pal, pref, dpi, job: None, drag: Drag::None, high: None, opts, excludes, settings_path: path.clone(), panel: None, rescan: false, thumb_service, thumbs: HashMap::new(), thumb_order: VecDeque::new(), thumb_bytes: 0, notify_hwnd }));
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
