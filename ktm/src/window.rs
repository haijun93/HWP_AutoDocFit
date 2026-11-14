//! Win32 입력/IME + GDI 이중 버퍼. 키워드 검색 최적화 네이티브 단축키 창 UI.
//! PolyForm Strict License 1.0.0
#[path = "window_probe.rs"]
mod probe;
use ktm::{
    backend::{Answer, Backend, Row},
    highlight,
    settings::{RecentDoc, Settings},
    tm_state::{
        self, DisplayItem, Effect, Event, KeyAction, Phase, State, Tab,
        build_display_list, display_to_selectable_index, handle_key, selectable_count,
        selectable_to_display_index,
    },
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
pub(crate) const CLASS: &str = "HwpAutoDocFit.MongttangSearch";

/// (세대, 검색어, 칸, 시작 시각, 취소, 파일명 정렬).
type Job = (u64, String, Tab, Instant, Arc<AtomicBool>, &'static str);
/// 스크롤로 새로 보이게 된 줄의 원문 찾기: (세대, [(줄 번호, 줄)], 강조 낱말).
type SnipJob = (u64, Vec<(usize, Row)>, Vec<String>);

/// 일꾼 대기열: 검색은 늘 최신 하나, 원문 찾기도 최신 하나(검색 먼저).
#[derive(Default)]
struct Pending {
    search: Option<Job>,
    snippets: Option<SnipJob>,
}

/// 일꾼이 검색을 집어 든 시각과 마친 시각.
#[derive(Clone, Copy)]
struct WorkTimes {
    picked: Instant,
    done: Instant,
}

#[derive(Default)]
struct SearchBuffer {
    generation: u64,
    names: Option<Vec<Row>>,
    hier: Option<Vec<Row>>,
    body: Option<Vec<Row>>,
    names_count: usize,
    hier_count: usize,
    body_count: usize,
}

enum Reply {
    Names {
        generation: u64,
        result: Result<Vec<Row>, String>,
        total: usize,
        start: Instant,
        times: WorkTimes,
    },
    Content {
        generation: u64,
        result: Result<ktm::backend::UnifiedContentAnswer, String>,
        start: Instant,
        times: WorkTimes,
    },
    Snippets(u64, Vec<(usize, Row)>),
}

/// 테마별 색상 모음
#[derive(Clone, Copy)]
struct ThemeColors {
    bg: u32,
    fg: u32,
    fg_sub: u32,
    fg_faint: u32,
    sel_bg: u32,
    sel_fg: u32,
    border: u32,
    tab_bg: u32,
    tab_sel_line: u32,
    header_bg: u32,
    group_accents: [u32; 4],
}

struct Ui {
    state: State,
    rows: Vec<Row>,
    terms: Vec<String>,
    groups: Vec<Vec<String>>,
    buffer: SearchBuffer,
    display_items: Vec<DisplayItem>,
    edit: HWND,
    font_normal: HFONT,
    font_bold: HFONT,
    font_large: HFONT,
    font_small: HFONT,
    brush: HBRUSH,
    theme: String,
    tx: Arc<(Mutex<Pending>, Condvar)>,
    rx: mpsc::Receiver<Reply>,
    status: mpsc::Sender<String>,
    top: usize,
    timing: Option<Instant>,
    ready_at: Option<Instant>,
    search_duration_ms: Option<f64>,
    hotkey: String,
    hotkey_error: String,
    cancel: Arc<AtomicBool>,
    settings_path: PathBuf,
    settings: Settings,
    warmed: bool,
    name_sort: usize,
    is_indexing: bool,
    fallback_engine: bool,
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
    static METRICS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

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

fn get_theme_colors() -> ThemeColors {
    with(|u| unsafe {
        let mut hc: HIGHCONTRASTW = std::mem::zeroed();
        hc.cbSize = std::mem::size_of_val(&hc) as u32;
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            hc.cbSize,
            (&mut hc as *mut HIGHCONTRASTW).cast(),
            0,
        );
        let is_hc = hc.dwFlags & HCF_HIGHCONTRASTON != 0
            || u.theme == "contrast"
            || u.theme == "high-contrast";

        if is_hc {
            let win = GetSysColor(COLOR_WINDOW);
            let text = GetSysColor(COLOR_WINDOWTEXT);
            let hl = GetSysColor(COLOR_HIGHLIGHT);
            let hlt = GetSysColor(COLOR_HIGHLIGHTTEXT);
            let hot = GetSysColor(COLOR_HOTLIGHT);
            ThemeColors {
                bg: win,
                fg: text,
                fg_sub: GetSysColor(COLOR_GRAYTEXT),
                fg_faint: text,
                sel_bg: hl,
                sel_fg: hlt,
                border: GetSysColor(COLOR_ACTIVEBORDER),
                tab_bg: win,
                tab_sel_line: hot,
                header_bg: win,
                group_accents: [hot, hot, hot, hot],
            }
        } else if u.theme == "light" {
            ThemeColors {
                bg: 0xffffff,
                fg: 0x222222,
                fg_sub: 0x666666,
                fg_faint: 0x888888,
                sel_bg: 0xf5ebd8,
                sel_fg: 0x111111,
                border: 0xd0d0d0,
                tab_bg: 0xf6f6f6,
                tab_sel_line: 0xd06600,
                header_bg: 0xf0f0f0,
                // BGR: 블루, 틸, 그린, 퍼플
                group_accents: [0xcc6600, 0x999900, 0x327d2e, 0xaa248e],
            }
        } else {
            // Dark (기본)
            ThemeColors {
                bg: 0x1e1e1e,
                fg: 0xeeeeee,
                fg_sub: 0xaaaaaa,
                fg_faint: 0x777777,
                sel_bg: 0x3a3a3a,
                sel_fg: 0xffffff,
                border: 0x3f3f3f,
                tab_bg: 0x252526,
                tab_sel_line: 0xf09030,
                header_bg: 0x282828,
                // BGR: 오렌지/골드, 시안/블루, 그린, 퍼플
                group_accents: [0x3090f0, 0xd0a000, 0x40c040, 0xc050c0],
            }
        }
    })
}

/// Windows 클립보드에 문자열 복사
fn copy_to_clipboard(hwnd: HWND, text: &str) -> bool {
    unsafe {
        use windows_sys::Win32::System::DataExchange::{
            CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
        };
        use windows_sys::Win32::System::Memory::{
            GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock,
        };
        if OpenClipboard(hwnd) == 0 {
            return false;
        }
        EmptyClipboard();
        let w = wide(text);
        let bytes = (w.len() * 2) as usize;
        let hmem = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if !hmem.is_null() {
            let ptr = GlobalLock(hmem);
            if !ptr.is_null() {
                std::ptr::copy_nonoverlapping(w.as_ptr() as *const u8, ptr as *mut u8, bytes);
                GlobalUnlock(hmem);
                SetClipboardData(13 /* CF_UNICODETEXT */, hmem);
            }
        }
        CloseClipboard();
        true
    }
}

/// Alt+→ 미리보기 창 트리거 자리 (T9 별도 프로세스 연동용 stub)
fn preview_item(path: &str, row: Option<&Row>) {
    let _ = (path, row);
}

/// 창 크기를 결과 내용물에 맞추어 조절 (최대 모니터 60%)
fn resize_to_fit(h: HWND) {
    unsafe {
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of_val(&mi) as u32;
        let mon = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
        GetMonitorInfoW(mon, &mut mi);

        let work_w = mi.rcWork.right - mi.rcWork.left;
        let work_h = mi.rcWork.bottom - mi.rcWork.top;
        let w = scale(h, 720).min(work_w);
        let max_h = (work_h * 60 / 100).max(scale(h, 160));

        let top_area = scale(h, 92); // 검색칸 + 밑줄 + 거르기 줄 + 여백
        let header_h = scale(h, 24);
        let row_h = scale(h, 44);

        let (items_count, headers_count, has_status) = with(|u| {
            let mut items = 0;
            let mut headers = 0;
            for it in &u.display_items {
                match it {
                    DisplayItem::Header { .. }
                    | DisplayItem::RecentQueryHeader
                    | DisplayItem::RecentDocHeader => headers += 1,
                    DisplayItem::ResultRow { .. }
                    | DisplayItem::RecentQuery { .. }
                    | DisplayItem::RecentDoc { .. } => items += 1,
                }
            }
            let status = u.is_indexing || u.fallback_engine || u.state.error.is_some() || !u.hotkey_error.is_empty();
            (items, headers, status)
        });

        let status_h = if has_status { scale(h, 24) } else { 0 };
        let content_h = top_area + headers_count * header_h + items_count * row_h + status_h + scale(h, 12);
        let desired_h = content_h.max(scale(h, 140)).min(max_h);

        let mut rc: RECT = std::mem::zeroed();
        GetWindowRect(h, &mut rc);
        let cur_x = rc.left;
        let cur_y = rc.top;

        SetWindowPos(
            h,
            HWND_TOPMOST,
            cur_x,
            cur_y,
            w,
            desired_h,
            SWP_NOACTIVATE | SWP_NOZORDER,
        );
    }
}

fn rebuild_view(u: &mut Ui) {
    u.groups = highlight::parse_groups(&u.state.query);
    u.display_items = build_display_list(
        &u.state.query,
        u.state.tab,
        &u.rows,
        u.buffer.names_count,
        u.buffer.hier_count,
        u.buffer.body_count,
        u.state.collapsed_names,
        u.state.collapsed_hier,
        u.state.collapsed_body,
        &u.settings.recent_docs,
        &u.settings.recent_queries,
    );
    u.state.result_count = selectable_count(&u.display_items);
    if u.state.result_count > 0 {
        if u.state.selected.map_or(true, |s| s >= u.state.result_count) {
            u.state.selected = Some(0);
        }
    } else {
        u.state.selected = None;
    }
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
        if u.state.result_count == 0 && u.state.query.trim().is_empty() {
            u.rows.clear();
            u.terms.clear();
            u.top = 0;
        }
        rebuild_view(u);
        fx
    });

    for fx in effects {
        unsafe {
            match fx {
                Effect::Show => {
                    let t = Instant::now();
                    let mut mi: MONITORINFO = std::mem::zeroed();
                    mi.cbSize = std::mem::size_of_val(&mi) as u32;

                    let (saved_pos, recent_docs_empty) = with(|u| (u.settings.window_pos, u.settings.recent_docs.is_empty()));
                    let mon = if let Some((sx, sy)) = saved_pos {
                        MonitorFromPoint(POINT { x: sx, y: sy }, MONITOR_DEFAULTTONEAREST)
                    } else {
                        let mut p = POINT { x: 0, y: 0 };
                        GetCursorPos(&mut p);
                        MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST)
                    };
                    GetMonitorInfoW(mon, &mut mi);

                    let work_w = mi.rcWork.right - mi.rcWork.left;
                    let work_h = mi.rcWork.bottom - mi.rcWork.top;
                    let w = scale(h, 720).min(work_w);
                    let max_h = (work_h * 60 / 100).max(scale(h, 160));
                    let hh = if recent_docs_empty { scale(h, 140) } else { scale(h, 420) }.min(max_h);

                    let (x, y) = if let Some((sx, sy)) = saved_pos {
                        (
                            sx.min(mi.rcWork.right - w).max(mi.rcWork.left),
                            sy.min(mi.rcWork.bottom - hh).max(mi.rcWork.top),
                        )
                    } else {
                        (
                            mi.rcWork.left + (work_w - w) / 2,
                            mi.rcWork.top + work_h / 4,
                        )
                    };

                    SetWindowPos(
                        h,
                        HWND_TOPMOST,
                        x,
                        y,
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
                Effect::ClearInput => {
                    let ed = with(|u| u.edit);
                    SetWindowTextW(ed, wide("").as_ptr());
                }
                Effect::Search {
                    generation,
                    query,
                    tab,
                } => {
                    let tx = with(|u| u.tx.clone());
                    let cancel = Arc::new(AtomicBool::new(false));
                    edit(|u| {
                        u.cancel = cancel.clone();
                        u.settings.add_recent_query(&query);
                        let path = u.settings_path.clone();
                        let _ = u.settings.save(&path);
                    });
                    let sort = with(|u| ktm::backend::NAME_SORTS[u.name_sort % 4].0);
                    {
                        let mut p = tx.0.lock().unwrap();
                        p.search = Some((generation, query, tab, Instant::now(), cancel, sort));
                        p.snippets = None;
                    }
                    tx.1.notify_one();
                }
                Effect::Open { index, folder } => {
                    let (path, name, row_opt) = with(|u| {
                        if let Some(disp_idx) = selectable_to_display_index(&u.display_items, index) {
                            match &u.display_items[disp_idx] {
                                DisplayItem::ResultRow { row, .. } => (Some(row.path.clone()), Some(row.name.clone()), Some(row.clone())),
                                DisplayItem::RecentDoc { doc, .. } => (Some(doc.path.clone()), Some(doc.name.clone()), None),
                                DisplayItem::RecentQuery { query, .. } => {
                                    // 최근 검색어 엔터 -> 해당 검색어로 검색
                                    let ed = u.edit;
                                    let q = query.clone();
                                    SetWindowTextW(ed, wide(&q).as_ptr());
                                    SendMessageW(ed, 0x00b1, q.encode_utf16().count() as usize, -1);
                                    (None, None, None)
                                }
                                _ => (None, None, None),
                            }
                        } else {
                            (None, None, None)
                        }
                    });

                    if let Some(p) = path {
                        match ktm::open::open(&p, folder) {
                            Ok(()) => {
                                edit(|u| {
                                    if let Some(n) = name {
                                        u.settings.add_recent_doc(RecentDoc {
                                            path: p.clone(),
                                            name: n,
                                            location: row_opt.as_ref().map(|r| r.location.clone()).unwrap_or_default(),
                                            modified: String::new(),
                                        });
                                        let sp = u.settings_path.clone();
                                        let _ = u.settings.save(&sp);
                                    }
                                });
                                event(h, Event::Hide);
                            }
                            Err(message) => event(h, Event::OpenFailed(message)),
                        }
                    }
                }
                Effect::CopyClipboard(text) => {
                    copy_to_clipboard(h, &text);
                }
                Effect::TriggerPreview { index } => {
                    let item = with(|u| {
                        selectable_to_display_index(&u.display_items, index)
                            .and_then(|disp| match &u.display_items[disp] {
                                DisplayItem::ResultRow { row, .. } => Some((row.path.clone(), Some(row.clone()))),
                                DisplayItem::RecentDoc { doc, .. } => Some((doc.path.clone(), None)),
                                _ => None,
                            })
                    });
                    if let Some((p, r)) = item {
                        preview_item(&p, r.as_ref());
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
            let top_area = scale(h, 92);
            let row_h = scale(h, 44);
            let visible_count = ((rc.bottom - top_area) / row_h).max(1) as usize;
            if index < u.top {
                u.top = index;
            } else if index >= u.top + visible_count {
                u.top = index + 1 - visible_count;
            }
            request_snippets(u, visible_count);
        }
    });

    let (s, path) = with(|u| {
        (
            serde_json::json!({
                "pid": std::process::id(),
                "state": format!("{:?}", u.state.phase),
                "generation": u.state.generation,
                "results": u.state.result_count,
                "error": u.state.error,
                "hotkey_error": u.hotkey_error
            }),
            u.status.clone(),
        )
    });
    let _ = path.send(s.to_string());

    resize_to_fit(h);
    unsafe {
        InvalidateRect(h, null(), 0);
    }
}

/// 보이는 줄 중 원문을 아직 찾지 않은 줄만 일꾼에게 보낸다.
fn request_snippets(u: &mut Ui, count: usize) {
    let mut want = Vec::new();
    for sel_idx in u.top..(u.top + count + 1) {
        if let Some(disp_idx) = selectable_to_display_index(&u.display_items, sel_idx) {
            if let DisplayItem::ResultRow { row_index, .. } = &u.display_items[disp_idx] {
                let i = *row_index;
                if i < u.rows.len() && !u.rows[i].snippet_tried {
                    u.rows[i].snippet_tried = true;
                    let mut r = u.rows[i].clone();
                    r.snippet_tried = false;
                    want.push((i, r));
                }
            }
        }
    }
    if !want.is_empty() {
        u.tx.0.lock().unwrap().snippets = Some((u.state.generation, want, u.terms.clone()));
        u.tx.1.notify_one();
    }
}

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
        let alt = GetKeyState(VK_MENU as i32) < 0;
        let q_empty = with(|u| u.state.query.is_empty());

        let action = handle_key(key as u16, ctrl, shift, alt, q_empty);

        match action {
            KeyAction::None => false,
            KeyAction::ClearQuery => {
                event(h, Event::Escape);
                true
            }
            KeyAction::Hide => {
                event(h, Event::Hide);
                true
            }
            KeyAction::Submit => {
                event(h, Event::Submit);
                true
            }
            KeyAction::OpenFolder => {
                event(h, Event::Open { folder: true });
                true
            }
            KeyAction::MoveSelection(delta) => {
                event(h, Event::MoveSelection(delta));
                true
            }
            KeyAction::PageMove(delta) => {
                event(h, Event::PageMove(delta));
                true
            }
            KeyAction::MoveFirst => {
                event(h, Event::MoveFirst);
                true
            }
            KeyAction::MoveLast => {
                event(h, Event::MoveLast);
                true
            }
            KeyAction::SwitchTab(tab) => {
                event(h, Event::SwitchTab(tab));
                true
            }
            KeyAction::NextTab(reverse) => {
                let tab = with(|u| match (u.state.tab, reverse) {
                    (Tab::All, false) => Tab::Names,
                    (Tab::Names, false) => Tab::Hierarchy,
                    (Tab::Hierarchy, false) => Tab::Body,
                    (Tab::Body, false) => Tab::All,

                    (Tab::All, true) => Tab::Body,
                    (Tab::Names, true) => Tab::All,
                    (Tab::Hierarchy, true) => Tab::Names,
                    (Tab::Body, true) => Tab::Hierarchy,
                });
                event(h, Event::SwitchTab(tab));
                true
            }
            KeyAction::CopyPath => {
                let path = with(|u| {
                    u.state.selected
                        .and_then(|sel| selectable_to_display_index(&u.display_items, sel))
                        .and_then(|disp| match &u.display_items[disp] {
                            DisplayItem::ResultRow { row, .. } => Some(row.path.clone()),
                            DisplayItem::RecentDoc { doc, .. } => Some(doc.path.clone()),
                            _ => None,
                        })
                });
                if let Some(p) = path {
                    copy_to_clipboard(h, &p);
                }
                true
            }
            KeyAction::CopySnippet => {
                let text = with(|u| {
                    u.state.selected
                        .and_then(|sel| selectable_to_display_index(&u.display_items, sel))
                        .and_then(|disp| match &u.display_items[disp] {
                            DisplayItem::ResultRow { row, .. } => {
                                if !row.snippet.is_empty() {
                                    Some(row.snippet.clone())
                                } else if !row.location.is_empty() {
                                    Some(row.location.clone())
                                } else {
                                    Some(row.name.clone())
                                }
                            }
                            DisplayItem::RecentDoc { doc, .. } => Some(doc.location.clone()),
                            _ => None,
                        })
                });
                if let Some(t) = text {
                    copy_to_clipboard(h, &t);
                }
                true
            }
            KeyAction::Preview => {
                if let Some(sel) = with(|u| u.state.selected) {
                    let item = with(|u| {
                        selectable_to_display_index(&u.display_items, sel)
                            .and_then(|disp| match &u.display_items[disp] {
                                DisplayItem::ResultRow { row, .. } => Some((row.path.clone(), Some(row.clone()))),
                                DisplayItem::RecentDoc { doc, .. } => Some((doc.path.clone(), None)),
                                _ => None,
                            })
                    });
                    if let Some((p, r)) = item {
                        preview_item(&p, r.as_ref());
                    }
                }
                true
            }
            KeyAction::CycleSort => {
                let names = edit(|u| {
                    u.name_sort = (u.name_sort + 1) % 4;
                    u.state.tab == Tab::Names
                });
                if names {
                    event(h, Event::Refresh);
                } else {
                    InvalidateRect(h, null(), 0);
                }
                true
            }
            KeyAction::ToggleTheme => {
                let t = edit(|u| {
                    u.theme = if u.theme == "dark" { "light" } else { "dark" }.into();
                    u.theme.clone()
                });
                let path = with(|u| u.settings_path.clone());
                edit(|u| {
                    u.settings.theme = t.clone();
                    let _ = u.settings.save(&path);
                });
                InvalidateRect(h, null(), 1);
                InvalidateRect(ed, null(), 1);
                true
            }
            KeyAction::FocusEdit => {
                SetFocus(with(|u| u.edit));
                SendMessageW(with(|u| u.edit), 0x00b1, 0, -1);
                true
            }
        }
    }
}

unsafe fn create_cached_fonts(h: HWND) {
    unsafe {
        let f_norm = CreateFontW(
            -scale(h, 14), 0, 0, 0, 400, 0, 0, 0,
            DEFAULT_CHARSET as u32, 0, 0, 0, 0,
            wide("맑은 고딕").as_ptr(),
        );
        let f_bold = CreateFontW(
            -scale(h, 14), 0, 0, 0, 700, 0, 0, 0,
            DEFAULT_CHARSET as u32, 0, 0, 0, 0,
            wide("맑은 고딕").as_ptr(),
        );
        let f_large = CreateFontW(
            -scale(h, 18), 0, 0, 0, 400, 0, 0, 0,
            DEFAULT_CHARSET as u32, 0, 0, 0, 0,
            wide("맑은 고딕").as_ptr(),
        );
        let f_small = CreateFontW(
            -scale(h, 12), 0, 0, 0, 400, 0, 0, 0,
            DEFAULT_CHARSET as u32, 0, 0, 0, 0,
            wide("맑은 고딕").as_ptr(),
        );

        let (old_norm, old_bold, old_large, old_small, ed) = edit(|u| {
            let on = u.font_normal;
            let ob = u.font_bold;
            let ol = u.font_large;
            let os = u.font_small;
            u.font_normal = f_norm;
            u.font_bold = f_bold;
            u.font_large = f_large;
            u.font_small = f_small;
            (on, ob, ol, os, u.edit)
        });

        SendMessageW(ed, WM_SETFONT, f_large as usize, 1);

        for f in [old_norm, old_bold, old_large, old_small] {
            if !f.is_null() {
                DeleteObject(f);
            }
        }
    }
}

unsafe fn layout(h: HWND) {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(h, &mut r);
        let pad = scale(h, 14);
        let edit_h = scale(h, 34);
        let right_info_w = scale(h, 120);

        MoveWindow(
            with(|u| u.edit),
            pad,
            scale(h, 10),
            r.right - 2 * pad - right_info_w,
            edit_h,
            1,
        );
    }
}

/// 텍스트 내에서 묶음별 강조 구간을 서로 다른 색상 및 굵은 글꼴로 그리기
unsafe fn draw_highlighted_text(
    dc: HDC,
    s: &str,
    groups: &[Vec<String>],
    rc: RECT,
    c_normal: u32,
    colors: &ThemeColors,
    font_normal: HFONT,
    font_bold: HFONT,
) {
    unsafe {
        let saved = SaveDC(dc);
        IntersectClipRect(dc, rc.left, rc.top, rc.right, rc.bottom);

        let ranges = highlight::ranges_by_groups(s, groups);
        let mut offset = 0;
        let mut x = rc.left;

        for (start, end, grp_idx) in ranges.into_iter().chain(Some((s.len(), s.len(), 0))) {
            for (part, marked, grp) in [
                (&s[offset..start], false, 0),
                (&s[start..end], true, grp_idx),
            ] {
                if part.is_empty() {
                    continue;
                }
                let w = wide(part);
                let mut size: SIZE = std::mem::zeroed();

                let font = if marked { font_bold } else { font_normal };
                SelectObject(dc, font);

                GetTextExtentPoint32W(dc, w.as_ptr(), (w.len() - 1) as i32, &mut size);

                let fg = if marked {
                    colors.group_accents[grp % colors.group_accents.len()]
                } else {
                    c_normal
                };

                SetTextColor(dc, fg);
                SetBkMode(dc, TRANSPARENT as i32);

                let y = rc.top + (rc.bottom - rc.top - size.cy) / 2;
                TextOutW(dc, x, y, w.as_ptr(), (w.len() - 1) as i32);
                x += size.cx;
            }
            offset = end;
        }
        RestoreDC(dc, saved);
    }
}

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
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            edit(|u| u.search_duration_ms = Some(ms));
            log("input_to_paint_ms", ms);
            if let Some(ready) = edit(|u| u.ready_at.take()) {
                phase("search_ready_to_paint_ms", ready.elapsed());
            }
        }
    }
}

unsafe fn draw(h: HWND, dc: HDC, r: RECT) {
    unsafe {
        let mem = CreateCompatibleDC(dc);
        let bitmap = CreateCompatibleBitmap(dc, r.right, r.bottom);
        let old = SelectObject(mem, bitmap);

        let colors = get_theme_colors();
        let (f_norm, f_bold, f_large, f_small) = with(|u| {
            (u.font_normal, u.font_bold, u.font_large, u.font_small)
        });

        SetBkMode(mem, TRANSPARENT as i32);

        let fill = |rc: RECT, c: u32| {
            let b = CreateSolidBrush(c);
            FillRect(mem, &rc, b);
            DeleteObject(b);
        };

        let draw_text = |s: &str, mut rc: RECT, c: u32, font: HFONT, flags: u32| {
            SelectObject(mem, font);
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

        // 1. 전체 배경 채우기
        fill(r, colors.bg);

        // 창 테두리 (1px 얇은 선)
        let border_brush = CreateSolidBrush(colors.border);
        FrameRect(mem, &r, border_brush);
        DeleteObject(border_brush);

        let p = scale(h, 14);

        // 2. 검색칸 오른쪽 정보: "결과 수 · 소요 ms" (작게 12px)
        with(|u| {
            let info_str = if u.state.phase == Phase::Searching {
                "검색 중…".to_string()
            } else if !u.state.query.trim().is_empty() {
                let ms_str = u.search_duration_ms.map_or(String::new(), |m| format!(" · {:.1}ms", m));
                format!("{}건{}", u.state.result_count, ms_str)
            } else {
                String::new()
            };

            let info_rc = RECT {
                left: r.right - p - scale(h, 120),
                top: scale(h, 10),
                right: r.right - p,
                bottom: scale(h, 44),
            };
            draw_text(&info_str, info_rc, colors.fg_sub, f_small, DT_RIGHT);
        });

        // 3. 검색칸 밑줄: ';'로 나눈 묶음별 밑줄 색으로 표시
        with(|u| {
            let line_y = scale(h, 48);
            let line_h = scale(h, 3);
            let edit_w = r.right - 2 * p;

            if u.groups.is_empty() {
                fill(
                    RECT {
                        left: p,
                        top: line_y,
                        right: p + edit_w,
                        bottom: line_y + line_h,
                    },
                    colors.border,
                );
            } else {
                let n_grp = u.groups.len();
                let grp_w = edit_w / n_grp as i32;
                for i in 0..n_grp {
                    let col = colors.group_accents[i % colors.group_accents.len()];
                    let start_x = p + i as i32 * grp_w;
                    let end_x = if i == n_grp - 1 { p + edit_w } else { start_x + grp_w };
                    fill(
                        RECT {
                            left: start_x,
                            top: line_y,
                            right: end_x,
                            bottom: line_y + line_h,
                        },
                        col,
                    );
                }
            }
        });

        // 4. 거르기 줄: 모두 · 파일명 · 위계 · 본문 (각 개수)
        let tab_y = scale(h, 56);
        let tab_h = scale(h, 28);
        with(|u| {
            let tabs = [
                (format!("모두 {}", u.buffer.names_count + u.buffer.hier_count + u.buffer.body_count), Tab::All),
                (format!("파일명 {}", u.buffer.names_count), Tab::Names),
                (format!("위계 {}", u.buffer.hier_count), Tab::Hierarchy),
                (format!("본문 {}", u.buffer.body_count), Tab::Body),
            ];
            let tab_w = (r.right - 2 * p) / 4;
            for (i, (label, tab)) in tabs.iter().enumerate() {
                let rc = RECT {
                    left: p + i as i32 * tab_w,
                    top: tab_y,
                    right: p + (i as i32 + 1) * tab_w,
                    bottom: tab_y + tab_h,
                };
                let is_sel = u.state.tab == *tab;
                if is_sel {
                    fill(rc, colors.tab_bg);
                    fill(
                        RECT {
                            left: rc.left,
                            top: rc.bottom - scale(h, 2),
                            right: rc.right,
                            bottom: rc.bottom,
                        },
                        colors.tab_sel_line,
                    );
                }
                draw_text(
                    label,
                    rc,
                    if is_sel { colors.fg } else { colors.fg_sub },
                    f_norm,
                    DT_CENTER,
                );
            }
        });

        // 5. 결과 목록 (가상 스크롤, 한 줄 2단 높이 44px, 단 머리 24px)
        let list_y = scale(h, 88);
        let header_h = scale(h, 24);
        let row_h = scale(h, 44);

        with(|u| {
            let mut cur_y = list_y;
            let mut sel_counter = 0;

            for (disp_idx, item) in u.display_items.iter().enumerate() {
                if cur_y >= r.bottom - scale(h, 24) {
                    break;
                }

                match item {
                    DisplayItem::Header { tier, label, count, collapsed } => {
                        let h_rc = RECT {
                            left: p,
                            top: cur_y,
                            right: r.right - p,
                            bottom: cur_y + header_h,
                        };
                        fill(h_rc, colors.header_bg);
                        let arrow = if *collapsed { "▶" } else { "▼" };
                        let header_text = format!("{} {} ({}건)", arrow, label, count);
                        draw_text(&header_text, RECT { left: p + scale(h, 8), ..h_rc }, colors.fg_sub, f_small, 0);
                        cur_y += header_h;
                    }
                    DisplayItem::RecentQueryHeader => {
                        let h_rc = RECT {
                            left: p,
                            top: cur_y,
                            right: r.right - p,
                            bottom: cur_y + header_h,
                        };
                        fill(h_rc, colors.header_bg);
                        draw_text("최근 검색어", RECT { left: p + scale(h, 8), ..h_rc }, colors.fg_sub, f_small, 0);
                        cur_y += header_h;
                    }
                    DisplayItem::RecentDocHeader => {
                        let h_rc = RECT {
                            left: p,
                            top: cur_y,
                            right: r.right - p,
                            bottom: cur_y + header_h,
                        };
                        fill(h_rc, colors.header_bg);
                        draw_text("최근 연 문서", RECT { left: p + scale(h, 8), ..h_rc }, colors.fg_sub, f_small, 0);
                        cur_y += header_h;
                    }
                    DisplayItem::RecentQuery { query, .. } => {
                        let is_sel = u.state.selected == Some(sel_counter);
                        let item_rc = RECT {
                            left: p,
                            top: cur_y,
                            right: r.right - p,
                            bottom: cur_y + scale(h, 30),
                        };
                        if is_sel {
                            fill(item_rc, colors.sel_bg);
                        }
                        draw_text(
                            &format!("🔍 {}", query),
                            RECT { left: p + scale(h, 12), ..item_rc },
                            if is_sel { colors.sel_fg } else { colors.fg },
                            f_norm,
                            0,
                        );
                        cur_y += scale(h, 30);
                        sel_counter += 1;
                    }
                    DisplayItem::RecentDoc { doc, .. } => {
                        let is_sel = u.state.selected == Some(sel_counter);
                        let item_rc = RECT {
                            left: p,
                            top: cur_y,
                            right: r.right - p,
                            bottom: cur_y + row_h,
                        };
                        if is_sel {
                            fill(item_rc, colors.sel_bg);
                        }
                        let fg = if is_sel { colors.sel_fg } else { colors.fg };
                        // 1단: [HWP] 문서명
                        draw_text(
                            &format!("📄 {}", doc.name),
                            RECT {
                                left: p + scale(h, 8),
                                top: cur_y,
                                right: r.right - p - scale(h, 80),
                                bottom: cur_y + scale(h, 22),
                            },
                            fg,
                            f_norm,
                            DT_END_ELLIPSIS,
                        );
                        // 2단: 경로
                        draw_text(
                            &doc.path,
                            RECT {
                                left: p + scale(h, 8),
                                top: cur_y + scale(h, 22),
                                right: r.right - p,
                                bottom: cur_y + row_h,
                            },
                            colors.fg_faint,
                            f_small,
                            DT_PATH_ELLIPSIS,
                        );
                        cur_y += row_h;
                        sel_counter += 1;
                    }
                    DisplayItem::ResultRow { row, tier, .. } => {
                        let is_sel = u.state.selected == Some(sel_counter);
                        let item_rc = RECT {
                            left: p,
                            top: cur_y,
                            right: r.right - p,
                            bottom: cur_y + row_h,
                        };
                        if is_sel {
                            fill(item_rc, colors.sel_bg);
                        }
                        let fg = if is_sel { colors.sel_fg } else { colors.fg };

                        // 1단 (첫째 줄 22px): [종류 아이콘 16px] 파일명 · 오른쪽 날짜/크기
                        let icon_str = if row.name.ends_with(".hwp") || row.name.ends_with(".hwpx") {
                            "📄"
                        } else if row.name.contains('.') {
                            "📝"
                        } else {
                            "📁"
                        };
                        draw_text(
                            icon_str,
                            RECT {
                                left: p + scale(h, 6),
                                top: cur_y,
                                right: p + scale(h, 24),
                                bottom: cur_y + scale(h, 22),
                            },
                            fg,
                            f_norm,
                            0,
                        );

                        // 파일명: 일치 글자 굵게+강조색
                        let name_rc = RECT {
                            left: p + scale(h, 26),
                            top: cur_y,
                            right: r.right - p - scale(h, 110),
                            bottom: cur_y + scale(h, 22),
                        };
                        draw_highlighted_text(
                            mem,
                            &row.name,
                            &u.groups,
                            name_rc,
                            fg,
                            &colors,
                            f_norm,
                            f_bold,
                        );

                        // 오른쪽 날짜/크기
                        let date_size = if !row.location.is_empty() && (row.location.contains("KB") || row.location.contains("MB") || row.location.contains('-')) {
                            row.location.split('·').next().unwrap_or(&row.location).trim()
                        } else {
                            ""
                        };
                        draw_text(
                            date_size,
                            RECT {
                                left: r.right - p - scale(h, 110),
                                top: cur_y,
                                right: r.right - p - scale(h, 6),
                                bottom: cur_y + scale(h, 22),
                            },
                            colors.fg_faint,
                            f_small,
                            DT_RIGHT,
                        );

                        // 2단 (둘째 줄 22px):
                        // 위계 단은 "제목 › 장 › 절" 경로, 본문 단은 일치 문장 발췌 앞뒤 30자
                        let sub_rc = RECT {
                            left: p + scale(h, 26),
                            top: cur_y + scale(h, 22),
                            right: r.right - p - scale(h, 6),
                            bottom: cur_y + row_h,
                        };

                        if *tier == 2 {
                            // 위계: location/parent를 "제목 › 장 › 절"로 서식화
                            let hier_path = if !row.snippet.is_empty() {
                                &row.snippet
                            } else if !row.location.is_empty() {
                                &row.location
                            } else {
                                &row.parent
                            };
                            let hier_formatted = hier_path.replace(" / ", " › ").replace('/', " › ");
                            draw_highlighted_text(
                                mem,
                                &hier_formatted,
                                &u.groups,
                                sub_rc,
                                colors.fg_sub,
                                &colors,
                                f_small,
                                f_bold,
                            );
                        } else if *tier == 3 {
                            // 본문 단: 일치 문장 발췌 앞뒤 30자
                            let excerpt_text = if !row.snippet.is_empty() {
                                &row.snippet
                            } else {
                                &row.location
                            };
                            draw_highlighted_text(
                                mem,
                                excerpt_text,
                                &u.groups,
                                sub_rc,
                                colors.fg_sub,
                                &colors,
                                f_small,
                                f_bold,
                            );
                        } else {
                            // 파일명 단: 상위 경로
                            draw_text(&row.parent, sub_rc, colors.fg_faint, f_small, DT_PATH_ELLIPSIS);
                        }

                        cur_y += row_h;
                        sel_counter += 1;
                    }
                }
            }

            // 6. 상태 줄: 필요할 때만 1줄 표시
            let need_status = u.is_indexing || u.fallback_engine || u.state.error.is_some() || !u.hotkey_error.is_empty();
            if need_status {
                let status_msg = if !u.hotkey_error.is_empty() {
                    u.hotkey_error.clone()
                } else if let Some(e) = &u.state.error {
                    e.clone()
                } else if u.is_indexing {
                    "색인 작업 진행 중… (검색 결과가 갱신됩니다)".to_string()
                } else if u.fallback_engine {
                    "새 검색 엔진 사용 불가: 기존 검색 모드로 대체 작동 중".to_string()
                } else {
                    String::new()
                };

                let st_rc = RECT {
                    left: p,
                    top: r.bottom - scale(h, 22),
                    right: r.right - p,
                    bottom: r.bottom - scale(h, 2),
                };
                draw_text(&status_msg, st_rc, colors.group_accents[0], f_small, DT_END_ELLIPSIS);
            }
        });

        BitBlt(dc, 0, 0, r.right, r.bottom, mem, 0, 0, SRCCOPY);
        SelectObject(mem, old);
        DeleteObject(bitmap);
        DeleteDC(mem);
    }
}

fn phase(name: &'static str, d: std::time::Duration) {
    log(name, d.as_secs_f64() * 1000.0);
}

fn log(name: &str, value: f64) {
    log_line(format!("{{\"metric\":\"{name}\",\"ms\":{value:.4}}}"));
}

fn log_line(line: String) {
    if std::env::var_os("KTM_METRICS").is_some() {
        if METRICS.with(|m| {
            let mut m = m.borrow_mut();
            m.push(line);
            m.len() >= 1000
        }) {
            flush_metrics();
        }
    }
}

fn flush_metrics() {
    let lines = METRICS.with(|m| std::mem::take(&mut *m.borrow_mut()));
    if let (Some(path), false) = (std::env::var_os("KTM_METRICS"), lines.is_empty()) {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all((lines.join("\n") + "\n").as_bytes());
        }
    }
}

fn apply_buffer(u: &mut Ui) {
    let names = u.buffer.names.clone().unwrap_or_default();
    let hier = u.buffer.hier.clone().unwrap_or_default();
    let body = u.buffer.body.clone().unwrap_or_default();
    let merged = tm_state::merge_tiers(names, hier, body);
    let filtered: Vec<Row> = match u.state.tab {
        Tab::All => merged,
        Tab::Names => merged.into_iter().filter(|r| r.tier == 1).collect(),
        Tab::Hierarchy => merged.into_iter().filter(|r| r.tier == 2).collect(),
        Tab::Body => merged.into_iter().filter(|r| r.tier == 3).collect(),
    };
    u.rows = filtered;
    rebuild_view(u);
}

unsafe extern "system" fn proc(h: HWND, msg: u32, w: usize, l: isize) -> isize {
    unsafe {
        match msg {
            WM_NCHITTEST => {
                let hit = DefWindowProcW(h, msg, w, l);
                if hit == HTCLIENT as isize {
                    let mut p = POINT {
                        x: l as i16 as i32,
                        y: (l >> 16) as i16 as i32,
                    };
                    ScreenToClient(h, &mut p);
                    if p.y < scale(h, 48) {
                        return HTCAPTION as isize;
                    }
                }
                hit
            }
            WM_GETMINMAXINFO => {
                let info = &mut *(l as *mut MINMAXINFO);
                info.ptMinTrackSize = POINT {
                    x: scale(h, 520),
                    y: scale(h, 140),
                };
                0
            }
            WM_WINDOWPOSCHANGED => {
                let pos = &*(l as *const WINDOWPOS);
                if (pos.flags & SWP_NOMOVE) == 0 && pos.x > -30000 && pos.y > -30000 {
                    edit(|u| {
                        u.settings.window_pos = Some((pos.x, pos.y));
                        let sp = u.settings_path.clone();
                        let _ = u.settings.save(&sp);
                    });
                }
                DefWindowProcW(h, msg, w, l)
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
                create_cached_fonts(h);
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
                let colors = get_theme_colors();
                SetBkColor(w as HDC, colors.bg);
                SetTextColor(w as HDC, colors.fg);
                edit(|u| {
                    if !u.brush.is_null() {
                        DeleteObject(u.brush);
                    }
                    u.brush = CreateSolidBrush(colors.bg);
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
                let p = scale(h, 14);
                let tab_y = scale(h, 56);
                let tab_h = scale(h, 28);
                let list_y = scale(h, 88);
                let header_h = scale(h, 24);
                let row_h = scale(h, 44);

                if y >= tab_y && y < tab_y + tab_h {
                    // 거르기 줄 탭 클릭
                    let mut r: RECT = std::mem::zeroed();
                    GetClientRect(h, &mut r);
                    let tab_w = (r.right - 2 * p).max(1) / 4;
                    let tab = if x < p + tab_w {
                        Tab::All
                    } else if x < p + 2 * tab_w {
                        Tab::Names
                    } else if x < p + 3 * tab_w {
                        Tab::Hierarchy
                    } else {
                        Tab::Body
                    };
                    event(h, Event::SwitchTab(tab));
                } else if y >= list_y {
                    // 목록 항목 또는 단 머리 클릭
                    let clicked_info = with(|u| {
                        let mut cur = list_y;
                        let mut sel_idx = 0;
                        for item in &u.display_items {
                            let item_h = match item {
                                DisplayItem::Header { .. }
                                | DisplayItem::RecentQueryHeader
                                | DisplayItem::RecentDocHeader => header_h,
                                DisplayItem::RecentQuery { .. } => scale(h, 30),
                                DisplayItem::RecentDoc { .. }
                                | DisplayItem::ResultRow { .. } => row_h,
                            };
                            if y >= cur && y < cur + item_h {
                                match item {
                                    DisplayItem::Header { tier, .. } => return Some((true, *tier as usize, 0)),
                                    DisplayItem::ResultRow { .. }
                                    | DisplayItem::RecentDoc { .. }
                                    | DisplayItem::RecentQuery { .. } => return Some((false, 0, sel_idx)),
                                    _ => return None,
                                }
                            }
                            if matches!(item, DisplayItem::ResultRow { .. } | DisplayItem::RecentDoc { .. } | DisplayItem::RecentQuery { .. }) {
                                sel_idx += 1;
                            }
                            cur += item_h;
                        }
                        None
                    });

                    if let Some((is_header, tier, sel_idx)) = clicked_info {
                        if is_header {
                            event(h, Event::ToggleCollapse(tier as u8));
                        } else {
                            event(h, Event::Select(sel_idx));
                            if msg == WM_LBUTTONDBLCLK {
                                event(h, Event::Open { folder: false });
                            }
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
            WM_GETOBJECT => {
                // UI Automation / MSAA Accessible 지원 (표준 컨트롤 처리 위임)
                DefWindowProcW(h, msg, w, l)
            }
            WM_CLOSE => {
                event(h, Event::Hide);
                0
            }
            READY => {
                let answers = with(|u| u.rx.try_iter().collect::<Vec<_>>());
                for reply in answers {
                    match reply {
                        Reply::Names {
                            generation: g,
                            result,
                            total,
                            start,
                            times,
                        } => {
                            phase("names_queue_ms", times.picked - start);
                            phase("names_work_ms", times.done - times.picked);
                            phase("names_deliver_ms", times.done.elapsed());
                            if g != with(|u| u.state.generation) {
                                continue;
                            }
                            match result {
                                Ok(rows) => {
                                    edit(|u| {
                                        if u.buffer.generation != g {
                                            u.buffer = SearchBuffer {
                                                generation: g,
                                                ..Default::default()
                                            };
                                        }
                                        u.buffer.names = Some(rows);
                                        u.buffer.names_count = total;
                                        apply_buffer(u);
                                        u.timing = Some(start);
                                        u.ready_at = Some(Instant::now());
                                    });
                                    let count = with(|u| u.state.result_count);
                                    event(h, Event::SearchFinished { generation: g, count });
                                }
                                Err(_) => {
                                    edit(|u| {
                                        if u.buffer.generation != g {
                                            u.buffer = SearchBuffer {
                                                generation: g,
                                                ..Default::default()
                                            };
                                        }
                                        u.buffer.names = Some(Vec::new());
                                        u.buffer.names_count = 0;
                                        apply_buffer(u);
                                    });
                                }
                            }
                        }
                        Reply::Content {
                            generation: g,
                            result,
                            start,
                            times,
                        } => {
                            phase("content_queue_ms", times.picked - start);
                            phase("content_work_ms", times.done - times.picked);
                            phase("content_deliver_ms", times.done.elapsed());
                            if g != with(|u| u.state.generation) {
                                continue;
                            }
                            match result {
                                Ok(ans) => {
                                    edit(|u| {
                                        if u.buffer.generation != g {
                                            u.buffer = SearchBuffer {
                                                generation: g,
                                                ..Default::default()
                                            };
                                        }
                                        u.buffer.hier = Some(ans.hier_rows);
                                        u.buffer.body = Some(ans.body_rows);
                                        u.buffer.hier_count = ans.hier_count;
                                        u.buffer.body_count = ans.body_count;
                                        u.terms = ans.terms;
                                        apply_buffer(u);
                                        u.timing = Some(start);
                                        u.ready_at = Some(Instant::now());
                                    });
                                    let count = with(|u| u.state.result_count);
                                    event(h, Event::SearchFinished { generation: g, count });
                                }
                                Err(message) => {
                                    event(h, Event::SearchFailed { generation: g, message });
                                }
                            }
                        }
                        Reply::Snippets(g, rows) => {
                            edit(|u| {
                                if g == u.state.generation {
                                    for (i, r) in rows {
                                        if let Some(slot) = u.rows.get_mut(i) {
                                            slot.snippet = r.snippet;
                                        }
                                    }
                                    rebuild_view(u);
                                }
                            });
                            continue;
                        }
                    }
                }
                resize_to_fit(h);
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
                    let exe = std::env::current_exe().unwrap_or_default();
                    let autostart = ktm::install::autostart_points_to(
                        ktm::install::autostart_command().as_deref(),
                        &exe,
                    );
                    let line = with(|u| tray::status_line(&u.hotkey, &u.hotkey_error));
                    match tray::menu(h, &line, autostart) {
                        tray::SHOW => event(h, Event::Show),
                        tray::QUIT => {
                            DestroyWindow(h);
                        }
                        tray::AUTOSTART => {
                            let action = if autostart { "autostart-off" } else { "autostart-on" };
                            if let Err(e) = ktm::install::execute(action, &exe, &exe) {
                                event(h, Event::OpenFailed(e));
                            }
                        }
                        _ => {}
                    }
                } else if l as u32 == WM_LBUTTONUP {
                    event(h, Event::Show);
                }
                0
            }
            m if m == crate::lifecycle::QUIT => {
                DestroyWindow(h);
                0
            }
            WM_DESTROY => {
                flush_metrics();
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
        if crate::lifecycle::run(&args, &data, &source, &destination) {
            return Ok(());
        }
        let pipe_mode_early = arg("--backend").as_deref() == Some("pipe");
        let previous = FindWindowW(wide(CLASS).as_ptr(), null());
        if !previous.is_null() && std::env::var_os("KTM_TEST_ROOT").is_none() {
            if !args.iter().any(|a| a == "--hidden") {
                PostMessageW(previous, WM_HOTKEY, 0, 0);
            }
            return Ok(());
        }
        let root = match arg("--index-root") {
            Some(p) => PathBuf::from(p),
            None => ktm::paths::default_index()?,
        };
        let started = Instant::now();
        let pending = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let (tx, rx) = mpsc::channel();
        let (status_tx, status_rx) = mpsc::channel::<String>();
        let status_path = data.join("status.json");
        std::thread::spawn(move || {
            let mut previous = String::new();
            while let Ok(mut value) = status_rx.recv() {
                for latest in status_rx.try_iter() {
                    value = latest;
                }
                if value != previous {
                    let _ = std::fs::write(&status_path, &value);
                    previous = value;
                }
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

        // 창: 폭 720px, 얇은 테두리(WS_BORDER), DWM 그림자 없음(CS_DROPSHADOW 없음)
        let h = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class.as_ptr(),
            wide("몽땅검색").as_ptr(),
            WS_POPUP | WS_BORDER | WS_CLIPCHILDREN,
            0,
            0,
            720,
            420,
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
            10,
            560,
            34,
            h,
            1 as _,
            inst,
            null(),
        );
        SendMessageW(ed, 0x00c5, 4096, 0);

        UI.with(|u| {
            let mut ui = Ui {
                state: State::default(),
                rows: Vec::new(),
                terms: Vec::new(),
                groups: Vec::new(),
                buffer: SearchBuffer::default(),
                display_items: Vec::new(),
                edit: ed,
                font_normal: null_mut(),
                font_bold: null_mut(),
                font_large: null_mut(),
                font_small: null_mut(),
                brush: null_mut(),
                theme: arg("--theme").unwrap_or(settings.theme.clone()),
                tx: pending.clone(),
                rx,
                status: status_tx,
                top: 0,
                timing: None,
                ready_at: None,
                search_duration_ms: None,
                hotkey: String::new(),
                hotkey_error: String::new(),
                cancel: Arc::new(AtomicBool::new(false)),
                settings_path: data.join("settings.json"),
                settings,
                warmed: pipe_mode_early,
                name_sort: 0,
                is_indexing: false,
                fallback_engine: false,
            };
            rebuild_view(&mut ui);
            *u.borrow_mut() = Some(ui);
        });

        SetWindowLongPtrW(h, GWLP_WNDPROC, proc as *const () as isize);
        create_cached_fonts(h);
        layout(h);

        let worker_pending = pending;
        let handle = h as usize;
        let pipe_name = arg("--pipe-name").unwrap_or(ktm::backend::PIPE_NAME.into());
        let pipe_mode = arg("--backend").as_deref() == Some("pipe");
        let para_path = arg("--content-db")
            .map(PathBuf::from)
            .or_else(ktm::paragraphs::default_path);

        std::thread::spawn(move || {
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
                match work {
                    Work::Search((g, q, _tab, start, cancel, sort)) => {
                        let tx_names = tx.clone();
                        let pipe_name_clone = pipe_name.clone();
                        let q_names = q.clone();
                        let sort_names = sort;
                        let h_names = handle as HWND;
                        let cancel_names = cancel.clone();
                        std::thread::spawn(move || {
                            let picked = Instant::now();
                            let res = ktm::backend::pipe_names_total(&pipe_name_clone, &q_names, sort_names);
                            let done = Instant::now();
                            if cancel_names.load(Ordering::Relaxed) {
                                return;
                            }
                            let (rows_res, total) = match res {
                                Ok((rows, tot)) => (Ok(rows), tot),
                                Err(e) => (Err(e), 0),
                            };
                            let _ = tx_names.send(Reply::Names {
                                generation: g,
                                result: rows_res,
                                total,
                                start,
                                times: WorkTimes { picked, done },
                            });
                            unsafe { PostMessageW(h_names, READY, 0, 0) };
                        });

                        let picked = Instant::now();
                        let content_res = (|| {
                            if backend.is_none() {
                                backend = Some(open(&root)?);
                            }
                            backend.as_mut().unwrap().search_unified(&q, Some(cancel.clone()))
                        })();
                        let done = Instant::now();
                        if tx
                            .send(Reply::Content {
                                generation: g,
                                result: content_res,
                                start,
                                times: WorkTimes { picked, done },
                            })
                            .is_err()
                        {
                            break;
                        }
                        PostMessageW(handle as HWND, READY, 0, 0);
                    }
                    Work::Snippets((g, mut rows, terms)) => {
                        if let Some(b) = backend.as_mut() {
                            let mut only: Vec<Row> = rows.iter().map(|r| r.1.clone()).collect();
                            b.fill_snippets(&mut only, &terms);
                            for (slot, r) in rows.iter_mut().zip(only) {
                                slot.1 = r;
                            }
                        }
                        if tx.send(Reply::Snippets(g, rows)).is_err() {
                            break;
                        }
                        PostMessageW(handle as HWND, READY, 0, 0);
                    }
                }
            }
        });

        let hotkey = arg("--hotkey").unwrap_or(with(|u| u.settings.hotkey.clone()));
        edit(|u| u.hotkey = hotkey.clone());
        let _tray = match tray::Tray::new(h, &hotkey) {
            Ok(t) => {
                if let Some(error) = &t.error {
                    edit(|u| u.hotkey_error = error.clone());
                }
                Some(t)
            }
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
            for f in [u.font_normal, u.font_bold, u.font_large, u.font_small] {
                if !f.is_null() {
                    DeleteObject(f);
                }
            }
            if !u.brush.is_null() {
                DeleteObject(u.brush);
            }
        });
        Ok(())
    }
}
