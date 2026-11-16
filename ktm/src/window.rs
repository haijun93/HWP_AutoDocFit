//! Win32 입력/IME + GDI 이중 버퍼. 검색 스레드는 이벤트로만 깨며 창 상태는 step으로만 바꾼다.
//! 퀵몽땅클립보드(kagent/src/popup.rs)와 ui-core/ui_theme 스타일을 적용한 재디자인.
#[path = "frame.rs"]
pub(crate) mod frame;
#[path = "window_probe.rs"]
mod probe;

use ktm::{
    backend::{Answer, Backend, Row},
    layout::*,
    settings::Settings,
    tm_state::{self, DateFilter, Effect, Event, Phase, State, Tab},
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
        HiDpi::*,
        Input::{Ime::*, KeyboardAndMouse::*},
        WindowsAndMessaging::*,
    },
};

const READY: u32 = WM_APP + 21;
const WARMED: u32 = WM_APP + 22;
pub(crate) const CLASS: &str = "HwpAutoDocFit.MongttangSearch";
const WM_MOUSELEAVE: u32 = 0x02A3;

/// (세대, 검색어, 칸, 시작 시각, 취소, 파일명 정렬, 기간 필터, 커스텀 기간, 폴더 필터, 확장자 필터).
type Job = (
    u64,
    String,
    Tab,
    Instant,
    Arc<AtomicBool>,
    &'static str,
    DateFilter,
    Option<(i64, i64)>,
    Option<String>,
    Option<String>,
    tm_state::MatchMode,
);
/// 스크롤로 새로 보이게 된 줄의 원문 찾기: (세대, [(줄 번호, 줄)], 강조 낱말).
type SnipJob = (u64, Vec<(usize, Row)>, Vec<String>);

/// 일꾼 대기열: 검색은 늘 최신 하나, 원문 찾기도 최신 하나(검색 먼저).
#[derive(Default)]
struct Pending {
    search: Option<Job>,
    snippets: Option<SnipJob>,
    export: Option<(Job, ktm::export::ExportFormat)>,
}

#[derive(Clone, Copy)]
struct WorkTimes {
    picked: Instant,
    done: Instant,
}

enum Reply {
    Search(u64, Result<Answer, String>, Instant, WorkTimes),
    Snippets(u64, Vec<(usize, Row)>),
    TopTopics(Vec<String>),
}

struct Ui {
    state: State,
    rows: Vec<Row>,
    terms: Vec<String>,
    edit: HWND,
    within_edit: HWND,
    days_edit: HWND,
    within_open: bool,
    font: HFONT,
    font_bold: HFONT,
    small: HFONT,
    small_bold: HFONT,
    title_font: HFONT,
    glyph: HFONT,
    glyph_big: HFONT,
    brush: HBRUSH,
    theme: String,
    tx: Arc<(Mutex<Pending>, Condvar)>,
    rx: mpsc::Receiver<Reply>,
    status: mpsc::Sender<String>,
    top: usize,
    timing: Option<Instant>,
    ready_at: Option<Instant>,
    hotkey: String,
    hotkey_error: String,
    cancel: Arc<AtomicBool>,
    settings_path: PathBuf,
    warmed: bool,
    name_sort: usize,
    body_sort: ktm::backend::BodySort,
    // 호버 상태
    hover_head_btn: Option<HeadButton>,
    hover_row: Option<usize>,
    hover_folder_btn: Option<usize>,
    hover_tab: Option<usize>,
    status_text: String,
    smart_presets: Vec<ktm::settings::SmartPreset>,
    recent_query_times: Vec<u64>,
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn local_drives() -> Vec<PathBuf> {
    ('A'..='G').filter_map(|c| {
        let path = PathBuf::from(format!("{c}:\\"));
        path.is_dir().then_some(path)
    }).collect()
}

fn pick_folder(owner: HWND) -> Option<String> {
    unsafe {
        use windows_sys::Win32::UI::Shell::{BIF_RETURNONLYFSDIRS, BROWSEINFOW, ILFree, SHBrowseForFolderW, SHGetPathFromIDListW};
        let mut display = [0u16; 260];
        let title = wide("검색 범위에 추가할 폴더를 선택하세요");
        let info = BROWSEINFOW { hwndOwner: owner, pidlRoot: null_mut(), pszDisplayName: display.as_mut_ptr(), lpszTitle: title.as_ptr(), ulFlags: BIF_RETURNONLYFSDIRS, lpfn: None, lParam: 0, iImage: 0 };
        let pidl = SHBrowseForFolderW(&info);
        if pidl.is_null() { return None; }
        let mut path = [0u16; 32768];
        let selected = SHGetPathFromIDListW(pidl, path.as_mut_ptr()) != 0;
        ILFree(pidl);
        if selected { Some(String::from_utf16_lossy(&path[..path.iter().position(|c| *c == 0).unwrap_or(path.len())])) } else { None }
    }
}

fn with<R>(f: impl FnOnce(&Ui) -> R) -> R {
    UI.with(|u| f(u.borrow().as_ref().unwrap()))
}

fn edit<R>(f: impl FnOnce(&mut Ui) -> R) -> R {
    UI.with(|u| f(u.borrow_mut().as_mut().unwrap()))
}

fn scale(h: HWND, n: i32) -> i32 {
    frame::scale(h, n)
}

fn palette() -> ui_theme::Palette {
    ui_theme::palette()
}

fn sync_theme(theme_str: &str) {
    let pref = match theme_str {
        "light" => ui_theme::ThemePref::Light,
        "dark" => ui_theme::ThemePref::Dark,
        _ => ui_theme::ThemePref::System,
    };
    #[cfg(windows)]
    {
        let _ = ui_theme::refresh(pref);
    }
    #[cfg(not(windows))]
    {
        let is_dark = pref == ui_theme::ThemePref::Dark;
        let theme = ui_theme::Theme::derive(pref, is_dark, false, &ui_theme::SysColors::default());
        ui_theme::store(theme);
    }
}

/// 프로그램이 검색/결과-내-검색 입력칸 텍스트를 반영할 때 한글 IME 조합을 끊지 않고 내용이 다를 때만 갱신한다.
fn sync_edit_text(ed: HWND, text: &str) {
    if ed.is_null() {
        return;
    }
    let q = wide(text);
    let caret = q.len().saturating_sub(1);
    unsafe {
        let imc = ImmGetContext(ed);
        let composing =
            !imc.is_null() && ImmGetCompositionStringW(imc, GCS_COMPSTR, null_mut(), 0) > 0;
        if !imc.is_null() {
            ImmReleaseContext(ed, imc);
        }
        if composing {
            return;
        }
        let mut cur = vec![0u16; caret + 2];
        let n = GetWindowTextW(ed, cur.as_mut_ptr(), cur.len() as i32) as usize;
        if !(n == caret && cur[..n] == q[..caret]) {
            SetWindowTextW(ed, q.as_ptr());
            SendMessageW(ed, 0x00b1, caret, caret as isize);
        }
    }
}

fn edit_text(ed: HWND, capacity: usize) -> String {
    unsafe {
        let mut buf = vec![0u16; capacity.max(2)];
        let n = GetWindowTextW(ed, buf.as_mut_ptr(), buf.len() as i32).max(0) as usize;
        String::from_utf16_lossy(&buf[..n])
    }
}

fn event(h: HWND, e: Event) {
    let user_input = matches!(e, Event::Input(_));
    let need_body_snippets = matches!(&e, Event::SetExcludeFilename(true));
    let record_recent_search = matches!(&e, Event::Submit | Event::ApplyHistory(_));
    let (effects, query_changed, within_changed, history_changed) = edit(|u| {
        let prev_query = u.state.query.clone();
        let prev_within = u.state.options.within_query.clone();
        let prev_recent = u.state.recent_queries.clone();
        let prev_fav = u.state.favorite_queries.clone();
        if matches!(
            &e,
            Event::Input(_)
                | Event::SwitchTab(_)
                | Event::Hide
                | Event::Submit
                | Event::SetDateFilter(_)
                | Event::SetFolderFilter(_)
                | Event::ApplyAutocomplete(_)
                | Event::ApplyTopTopic(_)
                | Event::ApplyHistory(_)
        ) {
            u.cancel.store(true, Ordering::Relaxed);
        }
        let (s, fx) = tm_state::step(&u.state, e);
        let changed = !user_input && s.query != prev_query;
        let within_changed = s.options.within_query != prev_within;
        let hist_changed = s.recent_queries != prev_recent
            || s.favorite_queries != prev_fav
            || (record_recent_search && !s.query.trim().is_empty());
        u.state = s;
        if need_body_snippets {
            let count = u.rows.len();
            request_snippets(u, count);
        }
        if u.state.result_count == 0 {
            u.rows.clear();
            u.terms.clear();
            u.top = 0;
        }
        (fx, changed, within_changed, hist_changed)
    });
    if history_changed {
        let path = with(|u| u.settings_path.clone());
        let (recent, favorites) = with(|u| {
            (
                u.state.recent_queries.clone(),
                u.state.favorite_queries.clone(),
            )
        });
        let mut s = Settings::load(&path).unwrap_or_default();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let old_recent = s.recent_queries.clone();
        let old_times = s.recent_query_times.clone();
        s.recent_queries = recent;
        s.recent_query_times = s
            .recent_queries
            .iter()
            .enumerate()
            .map(|(index, q)| {
                if record_recent_search && index == 0 {
                    return now;
                }
                old_recent
                    .iter()
                    .position(|old| old == q)
                    .and_then(|i| old_times.get(i).copied())
                    .unwrap_or(now)
            })
            .collect();
        s.favorite_queries = favorites;
        edit(|u| u.recent_query_times = s.recent_query_times.clone());
        let _ = s.save(&path);
    }
    if query_changed {
        let (ed, text) = with(|u| (u.edit, u.state.query.clone()));
        sync_edit_text(ed, &text);
    }
    if within_changed {
        let (ed, text) = with(|u| (u.within_edit, u.state.options.within_query.clone()));
        sync_edit_text(ed, &text);
    }
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
                    let hh = scale(h, 510).min(mi.rcWork.bottom - mi.rcWork.top);
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
                    date_filter,
                    custom_date_range,
                    folder,
                    ext,
                    match_mode,
                } => {
                    let tx = with(|u| u.tx.clone());
                    let cancel = Arc::new(AtomicBool::new(false));
                    edit(|u| u.cancel = cancel.clone());
                    let sort = with(|u| ktm::backend::NAME_SORTS[u.name_sort % 4].0);
                    {
                        let mut p = tx.0.lock().unwrap();
                        p.search = Some((
                            generation,
                            query,
                            tab,
                            Instant::now(),
                            cancel,
                            sort,
                            date_filter,
                            custom_date_range,
                            folder,
                            ext,
                            match_mode,
                        ));
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
            let top_y = list_top_coord(h, u);
            let rh = scale(h, ROW_H);
            let preview_h = if u.state.selected.is_some() && !u.rows.is_empty() {
                scale(h, PREVIEW_H)
            } else {
                0
            };
            let count =
                ((rc.bottom - top_y - scale(h, STATUS_H) - preview_h).max(0) / rh).max(1) as usize;
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
    unsafe {
        InvalidateRect(h, null(), 0);
    }
}

fn list_top_coord(h: HWND, u: &Ui) -> i32 {
    let head = scale(h, HEAD_H);
    let search = scale(h, SEARCH_BOX_H + 12);
    let tabs = scale(h, TAB_H);
    let options = scale(h, OPTION_H);
    let filters = scale(h, FILTER_H);
    let summary = scale(h, SUMMARY_H);
    let has_ac = !u.state.autocompletes.is_empty();
    calc_content_top(
        head,
        search,
        tabs,
        options,
        filters,
        summary,
        has_ac,
        scale(h, 28),
    )
}

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

fn trim_memory() {
    if !std::env::args().any(|a| a == "--trim-on-hide") {
        return;
    }
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::System::ProcessStatus::K32EmptyWorkingSet(
            windows_sys::Win32::System::Threading::GetCurrentProcess(),
        );
    }
}

fn copy_text_to_clipboard(value: &str) -> Result<(), String> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::{
            DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
        };
        let mut text: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        if OpenClipboard(null_mut()) == 0 {
            return Err("클립보드를 열지 못했습니다".into());
        }
        let handle = GlobalAlloc(GMEM_MOVEABLE, text.len() * 2);
        if handle.is_null() {
            CloseClipboard();
            return Err("클립보드 메모리를 만들지 못했습니다".into());
        }
        let ptr = GlobalLock(handle) as *mut u16;
        if ptr.is_null() {
            CloseClipboard();
            return Err("클립보드 메모리를 잠그지 못했습니다".into());
        }
        std::ptr::copy_nonoverlapping(text.as_mut_ptr(), ptr, text.len());
        GlobalUnlock(handle);
        EmptyClipboard();
        if SetClipboardData(13, handle as _).is_null() {
            CloseClipboard();
            return Err("클립보드에 복사하지 못했습니다".into());
        }
        CloseClipboard();
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = value;
        Err("클립보드 복사는 Windows에서만 지원합니다".into())
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
        let alt = GetKeyState(VK_MENU as i32) < 0;
        let shift = GetKeyState(VK_SHIFT as i32) < 0;
        match key as u16 {
            VK_ESCAPE => event(h, Event::Hide),
            VK_RETURN => {
                if ctrl {
                    event(h, Event::Open { folder: true });
                } else {
                    event(h, Event::Submit);
                }
            }
            VK_DOWN => event(h, Event::MoveSelection(1)),
            VK_UP => event(h, Event::MoveSelection(-1)),
            VK_NEXT => event(h, Event::MoveSelection(5)),
            VK_PRIOR => event(h, Event::MoveSelection(-5)),
            VK_TAB => {
                let tab = with(|u| match (u.state.tab, shift) {
                    (Tab::Body, false) | (Tab::Names, true) => Tab::Hierarchy,
                    (Tab::Hierarchy, false) | (Tab::Body, true) => Tab::Names,
                    _ => Tab::Body,
                });
                event(h, Event::SwitchTab(tab));
            }
            _ if ctrl && key == b'S' as usize => {
                let tab = with(|u| u.state.tab);
                if tab == Tab::Names {
                    edit(|u| {
                        u.name_sort = (u.name_sort + 1) % 4;
                    });
                    event(h, Event::Refresh);
                } else if tab == Tab::Body || tab == Tab::Hierarchy {
                    edit(|u| {
                        u.body_sort = u.body_sort.next();
                        ktm::backend::sort_rows(&mut u.rows, u.body_sort);
                    });
                    InvalidateRect(h, null(), 0);
                }
            }
            _ if ctrl && key == b'L' as usize => {
                let t = edit(|u| {
                    u.theme = if u.theme == "dark" { "light" } else { "dark" }.into();
                    sync_theme(&u.theme);
                    u.theme.clone()
                });
                frame::apply_theme(h);
                let path = with(|u| u.settings_path.clone());
                let mut loaded = Settings::load(&path).unwrap_or_default();
                loaded.theme = t;
                let _ = loaded.save(&path);
                InvalidateRect(h, null(), 1);
                InvalidateRect(ed, null(), 1);
            }
            _ if ctrl && (key == b'D' as usize || key == b'd' as usize) => {
                event(h, Event::ToggleFavorite);
                InvalidateRect(h, null(), 0);
            }
            _ if ctrl && key == b'F' as usize => {
                edit(|u| u.within_open = true);
                layout(h);
                let within = with(|u| u.within_edit);
                SetFocus(within);
                SendMessageW(within, 0x00b1, 0, -1);
            }
            _ if ctrl && key == b'E' as usize => {
                let chosen = {
                    #[cfg(windows)]
                    {
                        use windows_sys::Win32::UI::WindowsAndMessaging::*;
                        let menu = CreatePopupMenu();
                        let csv_label = wide("CSV로 내보내기 (Excel 호환, UTF-8 BOM)");
                        let tsv_label = wide("TSV로 내보내기 (탭 구분 텍스트, UTF-8 BOM)");
                        AppendMenuW(menu, MF_STRING, 1, csv_label.as_ptr());
                        AppendMenuW(menu, MF_STRING, 2, tsv_label.as_ptr());
                        let mut pt = POINT { x: 0, y: 0 };
                        GetCursorPos(&mut pt);
                        let cmd = TrackPopupMenu(
                            menu,
                            TPM_RETURNCMD | TPM_NONOTIFY,
                            pt.x,
                            pt.y,
                            0,
                            h,
                            null(),
                        );
                        DestroyMenu(menu);
                        SetFocus(with(|u| u.edit));
                        match cmd {
                            1 => Some(ktm::export::ExportFormat::Csv),
                            2 => Some(ktm::export::ExportFormat::Tsv),
                            _ => None,
                        }
                    }
                    #[cfg(not(windows))]
                    {
                        Some(ktm::export::ExportFormat::Csv)
                    }
                };

                if let Some(format) = chosen {
                    let (tx, job) = with(|u| {
                        let cancel = Arc::new(AtomicBool::new(false));
                        let sort = ktm::backend::NAME_SORTS[u.name_sort % 4].0;
                        let job = (
                            u.state.generation,
                            u.state.query.clone(),
                            u.state.tab,
                            Instant::now(),
                            cancel,
                            sort,
                            u.state.date_filter,
                            u.state.custom_date_range,
                            u.state.selected_folder.clone(),
                            u.state.selected_ext.clone(),
                            u.state.options.match_mode,
                        );
                        (u.tx.clone(), job)
                    });
                    {
                        let mut p = tx.0.lock().unwrap();
                        p.export = Some((job, format));
                        tx.1.notify_one();
                    }
                }
            }
            0x31..=0x39 if alt || with(|u| u.state.query.is_empty()) => {
                let n = (key as u16 - 0x31) as usize;
                let top = with(|u| u.top);
                let total = with(|u| u.rows.len());
                if top + n < total {
                    event(h, Event::Select(top + n));
                }
            }
            _ => {
                return false;
            }
        }
        true
    }
}

unsafe fn create_fonts(h: HWND) {
    unsafe {
        let mk = |px: i32, weight: i32, face: &str| {
            CreateFontW(
                -scale(h, px),
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                0,
                0,
                5,
                0,
                wide(face).as_ptr(),
            )
        };
        let font = mk(14, 400, "맑은 고딕");
        let font_bold = mk(14, 700, "맑은 고딕");
        let small = mk(12, 400, "맑은 고딕");
        let small_bold = mk(12, 700, "맑은 고딕");
        let title_font = mk(13, 700, "맑은 고딕");
        let glyph = mk(10, 400, "Segoe MDL2 Assets");
        let glyph_big = mk(14, 400, "Segoe MDL2 Assets");

        let (old_f, old_fb, old_s, old_sb, old_t, old_g, old_gb, ed) = edit(|u| {
            let olds = (
                u.font,
                u.font_bold,
                u.small,
                u.small_bold,
                u.title_font,
                u.glyph,
                u.glyph_big,
                u.edit,
            );
            u.font = font;
            u.font_bold = font_bold;
            u.small = small;
            u.small_bold = small_bold;
            u.title_font = title_font;
            u.glyph = glyph;
            u.glyph_big = glyph_big;
            olds
        });
        SendMessageW(ed, WM_SETFONT, font as usize, 1);
        for f in [old_f, old_fb, old_s, old_sb, old_t, old_g, old_gb] {
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
        let head = scale(h, HEAD_H);
        let pad = scale(h, 12);
        let box_y = head + scale(h, 6);
        let box_h = scale(h, SEARCH_BOX_H);
        let ed_margin_x = scale(h, 8);
        let ed_margin_y = scale(h, 4);
        let side_w = with(|u| sidebar_width(u.state.options.sidebar_open, r.right));

        MoveWindow(
            with(|u| u.edit),
            side_w + pad + ed_margin_x,
            box_y + ed_margin_y,
            r.right - side_w - 2 * pad - 2 * ed_margin_x,
            box_h - 2 * ed_margin_y,
            1,
        );
        let spans = option_spans(pad + side_w, r.right - pad);
        let within = spans[5];
        let within_hwnd = with(|u| u.within_edit);
        MoveWindow(
            within_hwnd,
            within.0 + scale(h, 2),
            head + scale(h, SEARCH_BOX_H + 12 + TAB_H),
            (within.1 - within.0 - scale(h, 4)).max(1),
            scale(h, OPTION_H),
            1,
        );
        ShowWindow(
            within_hwnd,
            if with(|u| u.within_open) {
                SW_SHOW
            } else {
                SW_HIDE
            },
        );
        let days = with(|u| u.days_edit);
        MoveWindow(
            days,
            pad + side_w + scale(h, 258),
            head + scale(h, SEARCH_BOX_H + 12 + TAB_H + OPTION_H / 2),
            scale(h, 48),
            scale(h, 22),
            1,
        );
        ShowWindow(
            days,
            if with(|u| u.state.options.options_open) { SW_SHOW } else { SW_HIDE },
        );
    }
}

unsafe fn highlighted(dc: HDC, s: &str, terms: &[String], rc: RECT, fg: u32, bg: u32) {
    unsafe {
        let saved = SaveDC(dc);
        IntersectClipRect(dc, rc.left, rc.top, rc.right, rc.bottom);
        let mut offset = 0;
        let mut x = rc.left;
        for (start, end) in ktm::highlight::ranges_terms(s, terms)
            .into_iter()
            .chain(Some((s.len(), s.len())))
        {
            for (part, marked) in [(&s[offset..start], false), (&s[start..end], true)] {
                if part.is_empty() {
                    continue;
                }
                let w = wide(part);
                let mut size: SIZE = std::mem::zeroed();
                GetTextExtentPoint32W(dc, w.as_ptr(), (w.len() - 1) as i32, &mut size);
                SetTextColor(dc, fg);
                SetBkColor(dc, bg);
                SetBkMode(
                    dc,
                    if marked {
                        OPAQUE as i32
                    } else {
                        TRANSPARENT as i32
                    },
                );
                TextOutW(
                    dc,
                    x,
                    rc.top + (rc.bottom - rc.top - size.cy) / 2,
                    w.as_ptr(),
                    (w.len() - 1) as i32,
                );
                x += size.cx;
            }
            offset = end;
        }
        RestoreDC(dc, saved);
    }
}

unsafe fn draw_preview_lines(
    dc: HDC,
    text_content: &str,
    terms: &[String],
    rc: RECT,
    line_step: i32,
    fg: u32,
    bg: u32,
) {
    unsafe {
        let max_w = (rc.right - rc.left).max(10);
        let lines = split_into_lines(dc, text_content, max_w, 3);
        for (i, line) in lines.iter().enumerate().take(3) {
            let line_top = rc.top + i as i32 * line_step;
            let line_bottom = line_top + line_step;
            let line_rc = RECT {
                left: rc.left,
                top: line_top,
                right: rc.right,
                bottom: line_bottom,
            };
            highlighted(dc, line, terms, line_rc, fg, bg);
        }
    }
}

unsafe fn split_into_lines(dc: HDC, text: &str, max_w: i32, max_lines: usize) -> Vec<String> {
    unsafe {
        let mut lines = Vec::new();
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            return lines;
        }
        let mut current = String::new();
        for (i, word) in words.iter().enumerate() {
            let test = if current.is_empty() {
                (*word).to_string()
            } else {
                format!("{current} {word}")
            };
            let w_wide = wide(&test);
            let mut size: SIZE = std::mem::zeroed();
            GetTextExtentPoint32W(dc, w_wide.as_ptr(), (w_wide.len() - 1) as i32, &mut size);
            if size.cx > max_w && !current.is_empty() {
                lines.push(current);
                if lines.len() == max_lines - 1 {
                    let remainder = words[i..].join(" ");
                    lines.push(remainder);
                    return lines;
                }
                current = (*word).to_string();
            } else {
                current = test;
            }
        }
        if !current.is_empty() && lines.len() < max_lines {
            lines.push(current);
        }
        lines
    }
}

unsafe fn draw_open_icon(hdc: HDC, r: RECT, color: u32) {
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, color);
        if pen.is_null() {
            return;
        }
        let old_pen = SelectObject(hdc, pen);
        let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH as i32));

        let cx = (r.left + r.right) / 2;
        let cy = (r.top + r.bottom) / 2;

        let left = cx - 5;
        let top = cy - 2;
        let right = cx + 2;
        let bottom = cy + 5;

        MoveToEx(hdc, left, top, null_mut());
        LineTo(hdc, cx - 1, top);

        MoveToEx(hdc, left, top, null_mut());
        LineTo(hdc, left, bottom);

        LineTo(hdc, right, bottom);
        LineTo(hdc, right, cy + 1);

        MoveToEx(hdc, cx - 1, cy + 1, null_mut());
        LineTo(hdc, cx + 5, cy - 5);

        MoveToEx(hdc, cx + 2, cy - 5, null_mut());
        LineTo(hdc, cx + 5, cy - 5);
        LineTo(hdc, cx + 5, cy - 2);

        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(pen);
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
            log("input_to_paint_ms", start.elapsed().as_secs_f64() * 1000.0);
            if let Some(ready) = edit(|u| u.ready_at.take()) {
                phase("search_ready_to_paint_ms", ready.elapsed());
            }
        }
    }
}

unsafe fn prerender(h: HWND) {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(h, &mut r);
        let dc = GetDC(h);
        if r.right > 0 && r.bottom > 0 {
            edit(|u| {
                u.rows = vec![Row {
                    name: "퀵몽땅검색 미리 그리기.hwp".into(),
                    parent: "C:\\문서".into(),
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
            SendMessageW(
                ed,
                WM_PRINT,
                mem as usize,
                (PRF_CLIENT | PRF_NONCLIENT | PRF_ERASEBKGND) as isize,
            );
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

        let p = palette();
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

        fill(r, p.bg);

        let side_w = with(|u| sidebar_width(u.state.options.sidebar_open, r.right));
        if side_w > 0 {
            fill(
                RECT {
                    left: 0,
                    top: 0,
                    right: side_w,
                    bottom: r.bottom,
                },
                p.tab_bg,
            );
            let side = with(|u| {
                let mut lines = vec!["〈  검색 범위".to_string(), "인덱싱된 폴더".to_string()];
                lines.extend(
                    u.state
                        .folder_counts
                        .iter()
                        .map(|(n, c)| format!("{n}  {c} 문서")),
                );
                lines.push("추천 경로".into());
                lines.extend(local_drives().iter().map(|d| d.display().to_string()));
                lines.extend(u.smart_presets.iter().map(|p| format!("◆ {}", p.name)));
                lines.push("최근 검색".into());
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let times = u.recent_query_times.clone();
                lines.extend(u.state.history_entries().iter().map(|e| {
                    let relative = u
                        .state
                        .recent_queries
                        .iter()
                        .position(|q| q == &e.query)
                        .map(|i| {
                            times
                                .get(i)
                                .map(|t| ktm::settings::relative_time_label(now, *t))
                                .unwrap_or_else(|| "이전".into())
                        })
                        .unwrap_or_else(|| "즐겨찾기".into());
                    format!(
                        "{} {}  ·  {relative}",
                        if e.is_favorite { "★" } else { "🕒" },
                        if e.name.is_empty() { &e.query } else { &e.name }
                    )
                }));
                lines.push("전체 지우기".into());
                lines
            });
            for (i, line) in side.iter().enumerate() {
                SelectObject(
                    mem,
                    if i == 0 {
                        with(|u| u.small_bold)
                    } else {
                        with(|u| u.small)
                    },
                );
                text(
                    line,
                    RECT {
                        left: scale(h, 12),
                        top: scale(h, 40 + i as i32 * SIDEBAR_ROW_H),
                        right: side_w - scale(h, 8),
                        bottom: scale(h, 40 + (i as i32 + 1) * SIDEBAR_ROW_H),
                    },
                    p.text,
                    DT_END_ELLIPSIS,
                );
                if i == 1 {
                    text("+", RECT { left: side_w - scale(h, 38), top: scale(h, 40 + SIDEBAR_ROW_H), right: side_w - scale(h, 8), bottom: scale(h, 40 + 2 * SIDEBAR_ROW_H) }, p.accent, DT_CENTER);
                }
            }
        }

        let head = scale(h, HEAD_H);
        let pad = scale(h, 12);
        let bw = scale(h, CAPTION_W);

        // ① 상단 메뉴바
        fill(
            RECT {
                left: 0,
                top: 0,
                right: r.right,
                bottom: head,
            },
            p.head_bg,
        );
        let hover_btn = with(|u| u.hover_head_btn);
        for b in HEAD_BUTTONS {
            let (l, right) = head_button_span(b, r.right, bw);
            let br = RECT {
                left: l,
                top: 0,
                right,
                bottom: head,
            };
            let hovered = hover_btn == Some(b);
            if hovered {
                fill(
                    br,
                    if b == HeadButton::Close {
                        p.close_hover
                    } else {
                        p.head_button_hover
                    },
                );
            }
            SetTextColor(
                mem,
                if hovered && b == HeadButton::Close {
                    p.on_close_hover
                } else {
                    p.head_glyph
                },
            );
            SelectObject(mem, with(|u| u.glyph));
            text(
                &head_button_glyph(b).to_string(),
                br,
                if hovered && b == HeadButton::Close {
                    p.on_close_hover
                } else {
                    p.head_glyph
                },
                DT_CENTER,
            );
        }

        let buttons_left = head_button_span(HEAD_BUTTONS[0], r.right, bw).0;
        SelectObject(mem, with(|u| u.title_font));
        let title_left = side_w + scale(h, 12);
        let title_text = "퀵몽땅검색";
        text(
            title_text,
            RECT {
                left: title_left,
                top: 0,
                right: buttons_left - scale(h, 4),
                bottom: head,
            },
            p.text,
            DT_END_ELLIPSIS,
        );

        SelectObject(mem, with(|u| u.small));
        let hint_left = title_left + scale(h, 85);
        if hint_left < buttons_left - scale(h, 10) {
            text(
                "Ctrl+Alt+Space · Esc 숨김 · Ctrl+L 테마",
                RECT {
                    left: hint_left,
                    top: 0,
                    right: buttons_left - scale(h, 8),
                    bottom: head,
                },
                p.text_sub,
                DT_END_ELLIPSIS,
            );
        }

        // ② 검색칸 외곽 박스
        let box_y = head + scale(h, 6);
        let box_h = scale(h, SEARCH_BOX_H);
        let box_r = RECT {
            left: side_w + pad,
            top: box_y,
            right: r.right - pad,
            bottom: box_y + box_h,
        };
        let border_brush = CreateSolidBrush(p.accent);
        if !border_brush.is_null() {
            FrameRect(mem, &box_r, border_brush);
            DeleteObject(border_brush);
        }

        // ③ 탭 메뉴바
        let tab_y = box_y + box_h + scale(h, 6);
        let tab_h = scale(h, TAB_H);
        fill(
            RECT {
                left: 0,
                top: tab_y,
                right: r.right,
                bottom: tab_y + tab_h,
            },
            p.tab_bg,
        );
        fill(
            RECT {
                left: 0,
                top: tab_y + tab_h - 1,
                right: r.right,
                bottom: tab_y + tab_h,
            },
            p.divider,
        );

        with(|u| {
            let body_label = format!("본문 · {}", u.body_sort.label());
            let names_label = format!("파일명 · {}", ktm::backend::NAME_SORTS[u.name_sort % 4].1);
            let tabs = [
                (body_label.as_str(), Tab::Body),
                ("위계", Tab::Hierarchy),
                (names_label.as_str(), Tab::Names),
            ];
            let tab_rects = calc_tab_rects(pad, tab_y, tab_h, r.right);
            for (i, (label, tab)) in tabs.iter().enumerate() {
                let tr = tab_rects[i];
                let selected = u.state.tab == *tab;
                let hovered = u.hover_tab == Some(i);

                if selected {
                    fill(tr, p.bg);
                    fill(
                        RECT {
                            left: tr.left + scale(h, 4),
                            top: tab_y + tab_h - scale(h, 3),
                            right: tr.right - scale(h, 4),
                            bottom: tab_y + tab_h - 1,
                        },
                        p.accent,
                    );
                    SelectObject(mem, u.small_bold);
                    text(label, tr, p.accent, DT_CENTER);
                } else {
                    if hovered {
                        fill(tr, p.tab_hover);
                    }
                    SelectObject(mem, u.small);
                    text(label, tr, p.tab_text, DT_CENTER);
                }
            }

            // ③a 옵션 바: 모드, 필터, 제외, 저장 프리셋
            let oy = tab_y + tab_h;
            let oh = scale(h, OPTION_H);
            fill(
                RECT {
                    left: 0,
                    top: oy,
                    right: r.right,
                    bottom: oy + oh,
                },
                p.bg,
            );
            SelectObject(mem, u.small);
            let search_mode = if u.state.options.search_mode == tm_state::SearchMode::Keyword {
                "키워드"
            } else {
                "파일명"
            };
            let option_labels = [
                search_mode.to_string(),
                format!(
                    "정렬:{}",
                    match u.state.options.sort {
                        tm_state::SortMode::Relevance => "관련",
                        tm_state::SortMode::Confidence => "신뢰도",
                        tm_state::SortMode::Newest => "최신",
                        tm_state::SortMode::Oldest => "오래된",
                        tm_state::SortMode::Name => "이름",
                    }
                ),
                format!(
                    "형식:{}",
                    if u.state.options.file_types.is_empty() {
                        "전체".into()
                    } else {
                        u.state.options.file_types.join(",")
                    }
                ),
                format!(
                    "기간:{}",
                    u.state
                        .options
                        .days
                        .map(|d| format!("{d}일"))
                        .unwrap_or_else(|| "전체".into())
                ),
                format!(
                    "제외:{}",
                    if u.state.options.exclude_filename {
                        "✓"
                    } else {
                        "□"
                    }
                ),
                format!(
                    "결과 내:{}",
                    if u.state.options.within_query.is_empty() {
                        "Ctrl+F"
                    } else {
                        &u.state.options.within_query
                    }
                ),
                if u.state.options.options_open { "[옵션 ▾]".into() } else { "[옵션]".into() },
            ];
            let spans = option_spans(pad + side_w, r.right - pad);
            for (label, (left, right)) in option_labels.iter().zip(spans) {
                text(
                    label,
                    RECT {
                        left: left + scale(h, 2),
                        top: oy,
                        right,
                        bottom: oy + oh,
                    },
                    p.text_sub,
                    DT_END_ELLIPSIS,
                );
            }

            let second_y = oy + scale(h, OPTION_H / 2);
            if u.state.options.options_open {
                let modes = [(tm_state::MatchMode::AllTerms, "모두 포함"), (tm_state::MatchMode::AnyTerm, "하나 이상"), (tm_state::MatchMode::Exact, "정확히 일치")];
                let mut mx = pad + side_w + scale(h, 2);
                for (mode, label) in modes {
                    let rc = RECT { left: mx, top: second_y, right: mx + scale(h, 82), bottom: second_y + scale(h, 22) };
                    fill(rc, if u.state.options.match_mode == mode { p.accent } else { p.tab_hover });
                    text(label, rc, if u.state.options.match_mode == mode { p.bg } else { p.tab_text }, DT_CENTER);
                    mx += scale(h, 86);
                }
                SelectObject(mem, u.small);
                text("직접 일수", RECT { left: mx, top: second_y, right: mx + scale(h, 326), bottom: second_y + scale(h, 22) }, p.text_sub, DT_CENTER);
                text("일", RECT { left: mx + scale(h, 326), top: second_y, right: mx + scale(h, 340), bottom: second_y + scale(h, 22) }, p.text_sub, DT_CENTER);
                let apply_rc = RECT { left: mx + scale(h, 100), top: second_y, right: mx + scale(h, 140), bottom: second_y + scale(h, 22) };
                fill(apply_rc, p.tab_hover);
                text("적용", apply_rc, p.text, DT_CENTER);
            }

            let sy = oy + oh;
            let sh = scale(h, SUMMARY_H);
            fill(
                RECT {
                    left: 0,
                    top: sy,
                    right: r.right,
                    bottom: sy + sh,
                },
                p.tab_bg,
            );
            let elapsed = u.timing.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0);
            let display_ids = ktm::layout::display_result_indices(
                &u.rows,
                &u.state.options.within_query,
                u.state.options.exclude_filename,
                &u.state.options.file_types,
                u.state.options.collapsed_filename,
                u.state.options.collapsed_body,
            );
            let group_ids = ktm::layout::filter_rows_within_query(
                &u.rows,
                &u.state.options.within_query,
                u.state.options.exclude_filename,
                &u.state.options.file_types,
            );
            let displayed: Vec<&Row> = group_ids.iter().filter_map(|i| u.rows.get(*i)).collect();
            let groups = ktm::layout::match_groups(
                &displayed.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
                &displayed
                    .iter()
                    .map(|r| r.snippet.clone())
                    .collect::<Vec<_>>(),
            );
            let view = if u.state.options.grid_view {
                "▦"
            } else {
                "☷"
            };
            let summary = format!(
                "{view}  {}개 · {:.2}초     파일명 매치 {}  ·  본문 매치 {}",
                display_ids.len(),
                elapsed,
                groups.filename,
                groups.body
            );
            text(
                &summary,
                RECT {
                    left: pad,
                    top: sy,
                    right: pad + scale(h, 230),
                    bottom: sy + sh,
                },
                p.text_sub,
                DT_END_ELLIPSIS,
            );
            let group_left = pad + scale(h, 235);
            let group_right = r.right - scale(h, 132);
            let group_mid = group_left + (group_right - group_left) / 2;
            text(
                &format!(
                    "파일명 매치 {}{}",
                    groups.filename,
                    if u.state.options.collapsed_filename {
                        " ▸"
                    } else {
                        " ▾"
                    }
                ),
                RECT {
                    left: group_left,
                    top: sy,
                    right: group_mid,
                    bottom: sy + sh,
                },
                p.text_sub,
                DT_END_ELLIPSIS,
            );
            text(
                &format!(
                    "본문 매치 {}{}",
                    groups.body,
                    if u.state.options.collapsed_body {
                        " ▸"
                    } else {
                        " ▾"
                    }
                ),
                RECT {
                    left: group_mid,
                    top: sy,
                    right: group_right,
                    bottom: sy + sh,
                },
                p.text_sub,
                DT_END_ELLIPSIS,
            );
            let copy_rc = RECT {
                left: r.right - scale(h, 124),
                top: sy,
                right: r.right - scale(h, 66),
                bottom: sy + sh,
            };
            let csv_rc = RECT {
                left: r.right - scale(h, 62),
                top: sy,
                right: r.right - scale(h, 4),
                bottom: sy + sh,
            };
            fill(copy_rc, p.tab_hover);
            fill(csv_rc, p.tab_hover);
            text("[복사]", copy_rc, p.text, DT_CENTER);
            text("[CSV]", csv_rc, p.text, DT_CENTER);

            // ④ 통합 검색 범위 줄 (기간 / 형식 / 폴더)
            let fy = oy + oh;
            let fh = scale(h, FILTER_H);
            SelectObject(mem, u.small);
            text(
                "기간:",
                RECT {
                    left: pad,
                    top: fy,
                    right: pad + scale(h, 32),
                    bottom: fy + fh,
                },
                p.text_sub,
                0,
            );
            let mut fx = pad + scale(h, 34);
            let date_chips = [
                (DateFilter::All, "전체", 36),
                (DateFilter::Today, "오늘", 36),
                (DateFilter::ThisWeek, "이번 주", 46),
                (DateFilter::ThisMonth, "이번 달", 46),
                (DateFilter::ThisYear, "올해", 36),
                (DateFilter::Custom, "직접", 36),
            ];
            for (df, lbl, w) in &date_chips {
                let cw = scale(h, *w);
                let rc = RECT {
                    left: fx,
                    top: fy,
                    right: fx + cw,
                    bottom: fy + fh,
                };
                let active = u.state.date_filter == *df;
                if active {
                    fill(rc, p.accent);
                    text(lbl, rc, p.bg, DT_CENTER);
                } else {
                    fill(rc, p.tab_hover);
                    text(lbl, rc, p.tab_text, DT_CENTER);
                }
                fx += cw + scale(h, 3);
            }

            // 확장자(형식) 칩
            fx += scale(h, 6);
            text(
                "형식:",
                RECT {
                    left: fx,
                    top: fy,
                    right: fx + scale(h, 32),
                    bottom: fy + fh,
                },
                p.text_sub,
                0,
            );
            fx += scale(h, 34);
            let ext_chips: [(Option<&'static str>, &'static str, i32); 5] = [
                (None, "전체", 36),
                (Some("hwp"), "HWP", 40),
                (Some("pdf"), "PDF", 38),
                (Some("docx"), "DOC", 40),
                (Some("xlsx"), "XLS", 38),
            ];
            for (opt, lbl, w) in &ext_chips {
                let cw = scale(h, *w);
                let rc = RECT {
                    left: fx,
                    top: fy,
                    right: fx + cw,
                    bottom: fy + fh,
                };
                let active = match opt {
                    None => u.state.selected_ext.is_none(),
                    Some(e) => u
                        .state
                        .selected_ext
                        .as_deref()
                        .map(|s| s.contains(e))
                        .unwrap_or(false),
                };
                if active {
                    fill(rc, p.accent);
                    text(lbl, rc, p.bg, DT_CENTER);
                } else {
                    fill(rc, p.tab_hover);
                    text(lbl, rc, p.tab_text, DT_CENTER);
                }
                fx += cw + scale(h, 3);
            }

            // 폴더 칩
            let has_folder_info =
                !u.state.folder_counts.is_empty() || u.state.selected_folder.is_some();
            if has_folder_info && fx + scale(h, 80) < r.right - pad {
                fx += scale(h, 6);
                text(
                    "폴더:",
                    RECT {
                        left: fx,
                        top: fy,
                        right: fx + scale(h, 32),
                        bottom: fy + fh,
                    },
                    p.text_sub,
                    0,
                );
                fx += scale(h, 34);
                let all_active = u.state.selected_folder.is_none();
                let all_rc = RECT {
                    left: fx,
                    top: fy,
                    right: fx + scale(h, 38),
                    bottom: fy + fh,
                };
                if all_active {
                    fill(all_rc, p.accent);
                    text("전체", all_rc, p.bg, DT_CENTER);
                } else {
                    fill(all_rc, p.tab_hover);
                    text("전체", all_rc, p.tab_text, DT_CENTER);
                }
                fx += scale(h, 41);

                let mut folders_to_show: Vec<(String, Option<u32>)> = Vec::new();
                if let Some(ref sel) = u.state.selected_folder {
                    if !u.state.folder_counts.iter().any(|(f, _)| f == sel) {
                        folders_to_show.push((sel.clone(), None));
                    }
                }
                for (f, c) in &u.state.folder_counts {
                    folders_to_show.push((f.clone(), Some(*c)));
                }

                for (folder, count) in folders_to_show {
                    let lbl = if let Some(c) = count {
                        format!("{folder}({c})")
                    } else {
                        folder.clone()
                    };
                    let cw = scale(h, 68).min(r.right - fx - pad);
                    if cw < scale(h, 28) {
                        break;
                    }
                    let rc = RECT {
                        left: fx,
                        top: fy,
                        right: fx + cw,
                        bottom: fy + fh,
                    };
                    let active = u.state.selected_folder.as_deref() == Some(&folder);
                    if active {
                        fill(rc, p.accent);
                        text(&lbl, rc, p.bg, DT_CENTER | DT_END_ELLIPSIS);
                    } else {
                        fill(rc, p.tab_hover);
                        text(&lbl, rc, p.tab_text, DT_CENTER | DT_END_ELLIPSIS);
                    }
                    fx += cw + scale(h, 3);
                }
            }

            // ⑤ 자동완성 칩 (입력 중일 때)
            if !u.state.autocompletes.is_empty() {
                let ay = fy + fh + scale(h, 4);
                let ah = scale(h, 22);
                let mut ax = pad;
                text(
                    "자동완성:",
                    RECT {
                        left: ax,
                        top: ay,
                        right: ax + scale(h, 60),
                        bottom: ay + ah,
                    },
                    p.text_sub,
                    0,
                );
                ax += scale(h, 62);
                for (i, term) in u.state.autocompletes.iter().take(5).enumerate() {
                    let cw = scale(h, 85).min(r.right - ax - pad);
                    if cw < scale(h, 35) {
                        break;
                    }
                    let rc = RECT {
                        left: ax,
                        top: ay,
                        right: ax + cw,
                        bottom: ay + ah,
                    };
                    let active = u.state.selected_autocomplete == Some(i);
                    if active {
                        fill(rc, p.selected);
                        text(
                            term,
                            rc,
                            p.row_text(true, p.accent),
                            DT_CENTER | DT_END_ELLIPSIS,
                        );
                    } else {
                        fill(rc, p.tab_hover);
                        text(term, rc, p.text, DT_CENTER | DT_END_ELLIPSIS);
                    }
                    ax += cw + scale(h, 6);
                }
            }

            // ⑥ 빈 검색어 첫 화면 최근/즐겨찾기 목록 또는 주요 주제 1~10위
            if u.state.query.trim().is_empty() && u.rows.is_empty() {
                let ty = fy + fh + scale(h, 14);
                let entries = u.state.history_entries();
                if !entries.is_empty() {
                    SelectObject(mem, u.small_bold);
                    text(
                        "최근 및 즐겨찾기 검색  (↑↓ 선택 · Enter 불러오기 · Ctrl+D 즐겨찾기 토글)",
                        RECT {
                            left: pad,
                            top: ty,
                            right: r.right - pad,
                            bottom: ty + scale(h, 20),
                        },
                        p.text_sub,
                        0,
                    );
                    let btn_y = ty + scale(h, 24);
                    let rh = scale(h, 28);
                    let status_h = scale(h, STATUS_H);
                    let max_visible = ((r.bottom - status_h - btn_y).max(0) / rh).max(0) as usize;
                    SelectObject(mem, u.font);
                    for (i, entry) in entries.iter().take(max_visible).enumerate() {
                        let top = btn_y + i as i32 * rh;
                        let rc = RECT {
                            left: pad,
                            top,
                            right: r.right - pad,
                            bottom: top + rh,
                        };
                        let active = u.state.selected_history == Some(i);
                        if active {
                            fill(rc, p.selected);
                            let b_brush = CreateSolidBrush(p.accent);
                            FrameRect(mem, &rc, b_brush);
                            DeleteObject(b_brush);
                        } else {
                            fill(rc, p.tab_hover);
                        }
                        let prefix = if entry.is_favorite { "★ " } else { "🕒 " };
                        let label = if entry.is_favorite
                            && !entry.name.is_empty()
                            && entry.name != entry.query
                        {
                            format!("{prefix}{}: {}", entry.name, entry.query)
                        } else {
                            format!("{prefix}{}", entry.query)
                        };
                        let pad_x = scale(h, 8);
                        let text_rc = RECT {
                            left: pad + pad_x,
                            top,
                            right: r.right - pad - pad_x,
                            bottom: top + rh,
                        };
                        text(
                            &label,
                            text_rc,
                            if active { p.accent } else { p.text },
                            DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
                        );
                    }
                } else {
                    SelectObject(mem, u.small_bold);
                    text(
                        "주요 주제 1~10위",
                        RECT {
                            left: pad,
                            top: ty,
                            right: r.right - pad,
                            bottom: ty + scale(h, 20),
                        },
                        p.text_sub,
                        0,
                    );
                    let btn_y = ty + scale(h, 24);
                    let btn_h = scale(h, 32);
                    let col_w = (r.right - 2 * pad - scale(h, 16)) / 2;
                    SelectObject(mem, u.font);
                    for (i, topic) in u.state.top_topics.iter().take(10).enumerate() {
                        let col = (i % 2) as i32;
                        let row = (i / 2) as i32;
                        let left = pad + col * (col_w + scale(h, 16));
                        let top = btn_y + row * (btn_h + scale(h, 6));
                        let rc = RECT {
                            left,
                            top,
                            right: left + col_w,
                            bottom: top + btn_h,
                        };
                        let active = u.state.selected_top_topic == Some(i);
                        if active {
                            fill(rc, p.selected);
                            let b_brush = CreateSolidBrush(p.accent);
                            FrameRect(mem, &rc, b_brush);
                            DeleteObject(b_brush);
                        } else {
                            fill(rc, p.tab_hover);
                        }
                        let label = format!("{}. {}", i + 1, topic);
                        text(
                            &label,
                            rc,
                            if active { p.accent } else { p.text },
                            DT_CENTER | DT_END_ELLIPSIS,
                        );
                    }
                }
            }

            // ⑦ 결과 목록 (클립보드 popup.rs 행 스타일)
            let top_y = list_top_coord(h, u);
            let rh = scale(h, ROW_H);
            let status_h = scale(h, STATUS_H);
            let preview_h = if u.state.selected.is_some() && !u.rows.is_empty() {
                scale(h, PREVIEW_H)
            } else {
                0
            };
            let visible = ((r.bottom - top_y - status_h - preview_h).max(0) / rh).max(0) as usize;
            let visible_indices = display_ids;

            let line_h1 = scale(h, 22);
            let line_h2 = scale(h, 20);
            let line_h3 = scale(h, 24);
            let folder_w = scale(h, ROW_FOLDER_W);

            for (slot, &n) in visible_indices
                .iter()
                .enumerate()
                .skip(u.top)
                .take(visible * if u.state.options.grid_view { 2 } else { 1 })
            {
                let row = &u.rows[n];
                if (row.snippet.is_empty() && u.state.options.collapsed_filename)
                    || (!row.snippet.is_empty() && u.state.options.collapsed_body)
                {
                    continue;
                }
                let y = top_y + (slot - u.top) as i32 * rh;
                let grid = u.state.options.grid_view;
                let card_w = (r.right - side_w - scale(h, 24)) / 2;
                let gx =
                    side_w + scale(h, 8) + ((slot - u.top) % 2) as i32 * (card_w + scale(h, 8));
                let gy = if grid {
                    top_y + ((slot - u.top) / 2) as i32 * rh
                } else {
                    y
                };
                let row_rc = if grid {
                    RECT {
                        left: gx,
                        top: gy,
                        right: gx + card_w,
                        bottom: gy + rh - scale(h, 6),
                    }
                } else {
                    RECT {
                        left: side_w,
                        top: y,
                        right: r.right,
                        bottom: y + rh,
                    }
                };
                let sel = u.state.selected == Some(n);
                let hovered = u.hover_row == Some(n);

                if sel {
                    fill(row_rc, p.selected);
                } else if hovered {
                    fill(row_rc, p.hover);
                }

                // 왼쪽 번호 (1~9)
                let num_str = if n - u.top < 9 {
                    format!("{}", n - u.top + 1)
                } else {
                    String::new()
                };
                SelectObject(mem, u.small);
                text(
                    &num_str,
                    RECT {
                        left: scale(h, 8),
                        top: y + scale(h, 2),
                        right: scale(h, 24),
                        bottom: y + line_h1,
                    },
                    p.row_text(sel, p.number),
                    DT_CENTER,
                );

                // 종류 태그/아이콘
                let tag = extension_tag(&row.name);
                SelectObject(mem, u.small_bold);
                text(
                    tag,
                    RECT {
                        left: scale(h, 28),
                        top: y + scale(h, 2),
                        right: scale(h, 66),
                        bottom: y + line_h1,
                    },
                    p.row_text(sel, p.text_faint),
                    0,
                );

                // 첫 줄: 파일명 (굵게)
                SelectObject(mem, u.font_bold);
                let text_right =
                    (if grid { row_rc.right } else { r.right }) - folder_w - scale(h, 8);
                let name_left = if grid { row_rc.left + scale(h, 8) } else { scale(h, 68) };
                let badge_w = scale(h, 48);
                let badge_right = if grid { row_rc.right - scale(h, 8) } else { text_right };
                let ext = ktm::backend::extension_label(&row.name);
                let ext_rc = RECT { left: badge_right - badge_w, top: gy + scale(h, 3), right: badge_right, bottom: gy + line_h1 - scale(h, 2) };
                fill(ext_rc, p.tab_hover);
                text(ext, ext_rc, p.text_sub, DT_CENTER);
                highlighted(mem, &row.name, &u.terms, RECT { left: name_left, top: gy + scale(h, 2), right: badge_right - badge_w - scale(h, 4), bottom: gy + line_h1 }, p.row_text(sel, p.text), 0x8AF0FE);


                // 오른쪽: 작은 단추 [폴더 열기]
                let folder_rc = calc_folder_button_rect(
                    gy,
                    if grid { row_rc.right } else { r.right },
                    scale(h, 6),
                    scale(h, 4),
                    folder_w,
                    scale(h, 22),
                );
                let folder_hovered = u.hover_folder_btn == Some(n);
                if folder_hovered {
                    fill(folder_rc, p.head_button_hover);
                }
                draw_open_icon(mem, folder_rc, p.row_text(sel, p.icon));

                // 둘째 줄: 경로 (흐리게)
                SelectObject(mem, u.small);
                let date = ktm::backend::format_korean_datetime(row.date).unwrap_or_default();
                let meta = if date.is_empty() { row.parent.clone() } else { format!("{}  |  {date}", row.parent) };
                text(&meta, RECT { left: if grid { row_rc.left + scale(h, 8) } else { scale(h, 68) }, top: gy + line_h1, right: if grid { row_rc.right - scale(h, 8) } else { text_right }, bottom: gy + line_h1 + line_h2 }, p.row_text(sel, p.text_faint), DT_PATH_ELLIPSIS);


                // 셋째 줄: 발췌 문장 (강조 하이라이트) 및 일치 문장 옆 위치 배지
                let (snippet, terms): (&str, &[String]) = if row.snippet.is_empty() {
                    (&row.location, &[])
                } else {
                    (&row.snippet, &u.terms)
                };
                let highlight_bg = 0x8AF0FE;
                SelectObject(mem, u.small);

                let mut snip_left = scale(h, 68);
                if grid {
                    snip_left = row_rc.left + scale(h, 8);
                }
                let line_top = gy + line_h1 + line_h2;
                let line_bottom = line_top + line_h3;

                // ① 일치 문장 옆 위치 배지 (쪽·시트·행 등 kfind 위치 메타가 있는 것만, 없으면 생략)
                if let Some(badge) = ktm::backend::compute_location_badge(&row.location) {
                    let bw_wide = wide(&badge);
                    let mut bsize: SIZE = std::mem::zeroed();
                    GetTextExtentPoint32W(
                        mem,
                        bw_wide.as_ptr(),
                        (bw_wide.len() - 1) as i32,
                        &mut bsize,
                    );
                    let badge_pad_x = scale(h, 4);
                    let badge_w = bsize.cx + badge_pad_x * 2;
                    let badge_h = bsize.cy + scale(h, 2);
                    let badge_top = line_top + (line_h3 - badge_h) / 2;
                    let badge_rc = RECT {
                        left: snip_left,
                        top: badge_top,
                        right: snip_left + badge_w,
                        bottom: badge_top + badge_h,
                    };
                    fill(badge_rc, p.tab_hover);
                    let b_brush = CreateSolidBrush(p.divider);
                    FrameRect(mem, &badge_rc, b_brush);
                    DeleteObject(b_brush);
                    text(
                        &badge,
                        badge_rc,
                        p.text_sub,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    );
                    snip_left += badge_w + scale(h, 6);
                }

                highlighted(
                    mem,
                    snippet,
                    terms,
                    RECT {
                        left: snip_left,
                        top: line_top,
                        right: if grid {
                            row_rc.right - scale(h, 8)
                        } else {
                            r.right - pad
                        },
                        bottom: line_bottom,
                    },
                    p.row_text(sel, p.text),
                    highlight_bg,
                );
            }

            // ② 결과 선택 시 미리보기 줄 (선택 행의 일치 문단 전체 3줄)
            if let Some(sel_idx) = u.state.selected {
                if let Some(sel_r) = u.rows.get(sel_idx) {
                    let prev_y = r.bottom - status_h - preview_h;
                    let prev_rc = RECT {
                        left: 0,
                        top: prev_y,
                        right: r.right,
                        bottom: r.bottom - status_h,
                    };
                    fill(prev_rc, p.tab_bg);
                    fill(
                        RECT {
                            left: 0,
                            top: prev_y,
                            right: r.right,
                            bottom: prev_y + 1,
                        },
                        p.divider,
                    );

                    // 문단 전체 본문
                    let preview_text = if !sel_r.preview.is_empty() {
                        &sel_r.preview
                    } else if !sel_r.snippet.is_empty() {
                        &sel_r.snippet
                    } else {
                        &sel_r.location
                    };

                    let pad_l = scale(h, 12);
                    let pad_r = r.right - scale(h, 12);
                    let line_step = scale(h, 18);
                    let content_top = prev_y + scale(h, 6);
                    let highlight_bg = 0x8AF0FE;

                    SelectObject(mem, u.small);
                    draw_preview_lines(
                        mem,
                        preview_text,
                        &u.terms,
                        RECT {
                            left: pad_l,
                            top: content_top,
                            right: pad_r,
                            bottom: r.bottom - status_h - scale(h, 4),
                        },
                        line_step,
                        p.text,
                        highlight_bg,
                    );
                }
            }

            // ⑧ 하단 상태 표시줄
            let status_y = r.bottom - status_h;
            fill(
                RECT {
                    left: 0,
                    top: status_y,
                    right: r.right,
                    bottom: r.bottom,
                },
                p.tab_bg,
            );
            fill(
                RECT {
                    left: 0,
                    top: status_y,
                    right: r.right,
                    bottom: status_y + 1,
                },
                p.divider,
            );

            let (status_left, status_right) = if !u.status_text.is_empty() {
                (u.status_text.clone(), String::new())
            } else if !u.hotkey_error.is_empty() {
                (u.hotkey_error.clone(), String::new())
            } else if let Some(e) = &u.state.error {
                (e.clone(), String::new())
            } else {
                match u.state.phase {
                    Phase::Searching => ("Searching".into(), String::new()),
                    Phase::Results => (format!("{} documents", u.rows.len()), format!("{} folders | v2.2.71", u.state.folder_counts.len())),
                    _ if u.state.query.trim().is_empty() && !u.state.history_entries().is_empty() => ("Up/down select | Enter load | Ctrl+D favorite".into(), String::new()),
                    _ if u.state.query.trim().is_empty() => ("Type a topic or search term".into(), String::new()),
                    _ => ("Live search from 2 chars | Enter for 1".into(), String::new()),
                }
            };
            SelectObject(mem, u.small);
            text(&status_left, RECT { left: pad, top: status_y, right: r.right / 2, bottom: r.bottom }, p.text_sub, DT_END_ELLIPSIS);
            text(&status_right, RECT { left: r.right / 2, top: status_y, right: r.right - pad, bottom: r.bottom }, p.text_sub, DT_RIGHT | DT_END_ELLIPSIS);
        });

        // 틀 없는 창 테두리 1px
        frame::own_border(mem, r);

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

thread_local! {
    static METRICS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
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
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = f.write_all((lines.join("\n") + "\n").as_bytes());
        }
    }
}

unsafe extern "system" fn proc(h: HWND, msg: u32, w: usize, l: isize) -> isize {
    unsafe {
        match msg {
            WM_NCCALCSIZE => {
                if w == 1 {
                    0
                } else {
                    DefWindowProcW(h, msg, w, l)
                }
            }
            WM_NCHITTEST => {
                let mut p = POINT {
                    x: l as i16 as i32,
                    y: (l >> 16) as i16 as i32,
                };
                ScreenToClient(h, &mut p);
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(h, &mut r);
                let head = scale(h, HEAD_H);
                let bw = scale(h, CAPTION_W);
                let on_btn = head_button_hit(p.x, p.y, r.right, bw, head).is_some();
                let grip = scale(h, frame::GRIP);
                frame::frame_hit(
                    p.x, p.y, r.right, r.bottom, grip, head, on_btn, false, false,
                )
                .code() as isize
            }
            WM_GETMINMAXINFO => {
                let info = &mut *(l as *mut MINMAXINFO);
                info.ptMinTrackSize = POINT {
                    x: scale(h, 520),
                    y: scale(h, 300),
                };
                0
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
                create_fonts(h);
                layout(h);
                0
            }
            WM_COMMAND if (w >> 16) as u16 == EN_CHANGE as u16 => {
                let ed = with(|u| u.edit);
                let within_ed = with(|u| u.within_edit);
                let source = l as HWND;
                if source == within_ed {
                    let len = GetWindowTextLengthW(within_ed).min(1024);
                    let mut s = vec![0u16; len as usize + 1];
                    GetWindowTextW(within_ed, s.as_mut_ptr(), s.len() as i32);
                    event(
                        h,
                        Event::SetWithinQuery(String::from_utf16_lossy(&s[..len as usize])),
                    );
                    InvalidateRect(h, null(), 0);
                    return 0;
                }
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
                let p = palette();
                SetBkColor(w as HDC, p.bg);
                SetTextColor(w as HDC, p.text);
                edit(|u| {
                    if !u.brush.is_null() {
                        DeleteObject(u.brush);
                    }
                    u.brush = CreateSolidBrush(p.bg);
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
            WM_MOUSEMOVE => {
                let x = l as i16 as i32;
                let y = (l >> 16) as i16 as i32;
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(h, &mut r);
                let head = scale(h, HEAD_H);
                let bw = scale(h, CAPTION_W);
                let pad = scale(h, 12);
                let side_w = with(|u| sidebar_width(u.state.options.sidebar_open, r.right));
                let folder_w = scale(h, ROW_FOLDER_W);

                let new_head_btn = head_button_hit(x, y, r.right, bw, head);
                let tab_y = head + scale(h, SEARCH_BOX_H + 12);
                let tab_h = scale(h, TAB_H);
                let new_tab = hit_tab(x - side_w, y, pad, tab_y, tab_h, r.right - side_w);

                let top_y = with(|u| list_top_coord(h, u));
                let rh = scale(h, ROW_H);
                let preview_h = with(|u| {
                    if u.state.selected.is_some() && !u.rows.is_empty() {
                        scale(h, PREVIEW_H)
                    } else {
                        0
                    }
                });
                let (new_row, new_folder_btn) =
                    if y >= top_y && y < client_bottom(h) - scale(h, STATUS_H) - preview_h {
                        let row_idx =
                            frame::row_index(y, top_y, rh, with(|u| u.top), with(|u| u.rows.len()));
                        let is_folder = if let Some(idx) = row_idx {
                            let row_top = top_y + (idx.saturating_sub(with(|u| u.top))) as i32 * rh;
                            let btn_rc = calc_folder_button_rect(
                                row_top,
                                r.right,
                                scale(h, 6),
                                scale(h, 4),
                                folder_w,
                                scale(h, 22),
                            );
                            hit_folder_button(x, y, btn_rc)
                        } else {
                            false
                        };
                        (row_idx, if is_folder { row_idx } else { None })
                    } else {
                        (None, None)
                    };

                let changed = edit(|u| {
                    let c = u.hover_head_btn != new_head_btn
                        || u.hover_tab != new_tab
                        || u.hover_row != new_row
                        || u.hover_folder_btn != new_folder_btn;
                    u.hover_head_btn = new_head_btn;
                    u.hover_tab = new_tab;
                    u.hover_row = new_row;
                    u.hover_folder_btn = new_folder_btn;
                    c
                });

                if changed {
                    InvalidateRect(h, null(), 0);
                }

                let mut tme: TRACKMOUSEEVENT = std::mem::zeroed();
                tme.cbSize = std::mem::size_of_val(&tme) as u32;
                tme.dwFlags = TME_LEAVE;
                tme.hwndTrack = h;
                TrackMouseEvent(&mut tme);
                0
            }
            WM_MOUSELEAVE => {
                edit(|u| {
                    u.hover_head_btn = None;
                    u.hover_tab = None;
                    u.hover_row = None;
                    u.hover_folder_btn = None;
                });
                InvalidateRect(h, null(), 0);
                0
            }
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
                let x = l as i16 as i32;
                let y = (l >> 16) as i16 as i32;
                let mut rc_w: RECT = std::mem::zeroed();
                GetClientRect(h, &mut rc_w);
                let head = scale(h, HEAD_H);
                let bw = scale(h, CAPTION_W);
                let pad = scale(h, 12);
                let side_w = with(|u| sidebar_width(u.state.options.sidebar_open, rc_w.right));
                let logical_sidebar_x = x * SIDEBAR_W / side_w.max(1);
                let sidebar_top = scale(h, 40);
                let logical_sidebar_y = 40 + (y - sidebar_top) * SIDEBAR_ROW_H / scale(h, SIDEBAR_ROW_H).max(1);
                if hit_sidebar_add(logical_sidebar_x, logical_sidebar_y, SIDEBAR_W, side_w > 0) {
                    if let Some(path) = pick_folder(h) { event(h, Event::SetFolderFilter(Some(path))); }
                    return 0;
                }

                // ① 상단 캡션 단추 클릭
                if let Some(btn) = head_button_hit(x, y, rc_w.right, bw, head) {
                    match btn {
                        HeadButton::Minimize | HeadButton::Close => {
                            event(h, Event::Hide);
                            return 0;
                        }
                    }
                }

                if y < head && x >= side_w && x < side_w + scale(h, 30) {
                    event(h, Event::ToggleSidebar);
                    layout(h);
                    InvalidateRect(h, null(), 0);
                    return 0;
                }

                if side_w > 0 && x < side_w {
                    let folders = with(|u| u.state.folder_counts.clone());
                    let preset_count = with(|u| u.smart_presets.len());
                    let history_count = with(|u| u.state.history_entries().len());
                    let sidebar_top = scale(h, 40);
                    let logical_y =
                        40 + (y - sidebar_top) * SIDEBAR_ROW_H / scale(h, SIDEBAR_ROW_H).max(1);
                    let logical_x = x * SIDEBAR_W / side_w.max(1);
                    match hit_sidebar(
                        logical_x,
                        logical_y,
                        SIDEBAR_W,
                        true,
                        folders.len(),
                        preset_count,
                        history_count,
                        local_drives().len(),
                    ) {
                        Some(SidebarHit::Toggle) => event(h, Event::ToggleSidebar),
                        Some(SidebarHit::IndexedFolder(i)) => {
                            if let Some((name, _)) = folders.get(i) {
                                event(h, Event::SetFolderFilter(Some(name.clone())));
                            }
                        }
                        Some(SidebarHit::Recommended(i)) => {
                            let part = ["Desktop", "Documents", "Downloads"][i.min(2)];
                            if let Some(profile) = std::env::var_os("USERPROFILE") {
                                let mut path = PathBuf::from(profile).join(part);
                                if !path.is_dir() {
                                    let korean = ["바탕 화면", "문서", "다운로드"][i.min(2)];
                                    path = PathBuf::from(
                                        std::env::var_os("USERPROFILE").unwrap_or_default(),
                                    )
                                    .join(korean);
                                }
                                event(
                                    h,
                                    Event::SetFolderFilter(Some(
                                        path.to_string_lossy().into_owned(),
                                    )),
                                );
                            }
                        }
                        Some(SidebarHit::Preset(i)) => {
                            if let Some(preset) = with(|u| u.smart_presets.get(i).cloned()) {
                                let old = with(|u| u.state.options.file_types.clone());
                                for t in old {
                                    event(h, Event::ToggleFileType(t));
                                }
                                for t in preset.file_types.clone() {
                                    event(h, Event::ToggleFileType(t));
                                }
                                event(h, Event::SetDays(preset.days));
                                event(h, Event::SetExcludeFilename(preset.exclude_filename));
                                event(h, Event::SetWithinQuery(preset.within_query));
                                if let Some(days) = preset.days {
                                    let today = ktm::backend::current_kst_ymd();
                                    let end = (today.0 * 10000 + today.1 * 100 + today.2) as i64;
                                    if days == 0 {
                                        event(h, Event::SetDateFilter(DateFilter::Today));
                                    } else {
                                        event(
                                            h,
                                            Event::SetCustomDate(
                                                ktm::backend::shift_ymd(end, -(days as i64)),
                                                end,
                                            ),
                                        );
                                    }
                                } else {
                                    event(h, Event::SetDateFilter(DateFilter::All));
                                }
                                event(h, Event::Refresh);
                            }
                        }
                        Some(SidebarHit::History(i)) => event(h, Event::ApplyHistory(i)),
                        Some(SidebarHit::ClearHistory) => event(h, Event::ClearHistory),
                        None => {}
                    }
                    InvalidateRect(h, null(), 0);
                    return 0;
                }

                let tab_y = head + scale(h, SEARCH_BOX_H + 12);
                let tab_h = scale(h, TAB_H);
                let oy = tab_y + tab_h;
                let oh = scale(h, OPTION_H);
                let fy = oy + oh;
                let fh = scale(h, FILTER_H);

                let oy = tab_y + tab_h;
                if y >= oy && y < oy + scale(h, OPTION_H) {
                    match hit_option(x, y, oy, scale(h, OPTION_H), pad + side_w, rc_w.right - pad) {
                        Some(OptionControl::MatchMode) => {
                let modes = [(tm_state::MatchMode::AllTerms, "모두 포함"), (tm_state::MatchMode::AnyTerm, "하나 이상"), (tm_state::MatchMode::Exact, "정확히 일치")];
                            let start = pad + side_w + scale(h, 2);
                            let idx = ktm::layout::hit_match_chip(x, start, scale(h, 86), 3).unwrap_or(0).min(2);
                            event(h, Event::SetMatchMode(modes[idx].0));
                        }
                        Some(OptionControl::CustomDays) => {
                            let left = pad + side_w + scale(h, 2);
                            let edit_left = left + scale(h, 3 * 86);
                            let edit_right = edit_left + scale(h, 48);
                            let apply_left = edit_right + scale(h, 52);
                            if x >= apply_left && x < apply_left + scale(h, 40) {
                                let input = edit_text(with(|u| u.days_edit), 16);
                                if let Ok(days) = input.trim().parse::<u32>() {
                                    if (1..=36500).contains(&days) {
                                        event(h, Event::SetDays(Some(days)));
                                        let today = ktm::backend::current_kst_ymd();
                                        let end = (today.0 * 10000 + today.1 * 100 + today.2) as i64;
                                        event(h, Event::SetCustomDate(ktm::backend::shift_ymd(end, -(days as i64)), end));
                                    }
                                }
                                SetFocus(with(|u| u.edit));
                            } else {
                                event(h, Event::ToggleOptions);
                                if with(|u| u.state.options.options_open) {
                                    SetFocus(with(|u| u.days_edit));
                                } else {
                                    SetFocus(with(|u| u.edit));
                                }
                            }
                        }
                        Some(OptionControl::SearchMode) if y >= oy + scale(h, OPTION_H / 2) => event(h, Event::ToggleOptions),
                        Some(OptionControl::SearchMode) => {
                            let mode = with(|u| u.state.options.search_mode);
                            let next = if mode == tm_state::SearchMode::Keyword {
                                tm_state::SearchMode::Filename
                            } else {
                                tm_state::SearchMode::Keyword
                            };
                            event(h, Event::SetSearchMode(next));
                            event(
                                h,
                                Event::SwitchTab(if next == tm_state::SearchMode::Filename {
                                    Tab::Names
                                } else {
                                    Tab::Body
                                }),
                            );
                        }
                        Some(OptionControl::Sort) => {
                            let order = [
                                tm_state::SortMode::Relevance,
                                tm_state::SortMode::Confidence,
                                tm_state::SortMode::Newest,
                                tm_state::SortMode::Oldest,
                                tm_state::SortMode::Name,
                            ];
                            let cur = with(|u| u.state.options.sort);
                            let next = order[(order.iter().position(|x| *x == cur).unwrap_or(0)
                                + 1)
                                % order.len()];
                            event(h, Event::SetSort(next));
                            edit(|u| {
                                u.body_sort = match next {
                                    tm_state::SortMode::Relevance => {
                                        ktm::backend::BodySort::Relevance
                                    }
                                    tm_state::SortMode::Confidence => ktm::backend::BodySort::Confidence,
                                    tm_state::SortMode::Newest => ktm::backend::BodySort::Date,
                                    tm_state::SortMode::Oldest => ktm::backend::BodySort::Oldest,
                                    tm_state::SortMode::Name => ktm::backend::BodySort::Name,
                                };
                                ktm::backend::sort_rows(&mut u.rows, u.body_sort);
                            });
                        }
                        Some(OptionControl::FileTypes) => {
                            let labels = ["hwp", "hwpx", "docx", "pptx", "xlsx", "pdf", "txt"];
                            let menu = CreatePopupMenu();
                            let selected = with(|u| u.state.options.file_types.clone());
                            for (i, t) in labels.iter().enumerate() {
                                AppendMenuW(
                                    menu,
                                    MF_STRING
                                        | if selected.iter().any(|s| s == t) {
                                            MF_CHECKED
                                        } else {
                                            0
                                        },
                                    100 + i,
                                    wide(t).as_ptr(),
                                );
                            }
                            AppendMenuW(menu, MF_SEPARATOR, 0, null());
                            AppendMenuW(menu, MF_STRING, 199, wide("전체 해제").as_ptr());
                            let mut pt = POINT { x: 0, y: 0 };
                            GetCursorPos(&mut pt);
                            let cmd = TrackPopupMenu(
                                menu,
                                TPM_RETURNCMD | TPM_NONOTIFY,
                                pt.x,
                                pt.y,
                                0,
                                h,
                                null(),
                            );
                            DestroyMenu(menu);
                            if (100..107).contains(&cmd) {
                                event(
                                    h,
                                    Event::ToggleFileType(labels[(cmd - 100) as usize].into()),
                                );
                            } else if cmd == 199 {
                                for t in selected {
                                    event(h, Event::ToggleFileType(t));
                                }
                            }
                        }
                        Some(OptionControl::Period) => {
                            let menu = CreatePopupMenu();
                            let labels = ["기간 없음", "오늘", "7일", "30일", "90일", "6개월", "1년", "직접 일수 입력…"];
                            for (i, label) in labels.iter().enumerate() { AppendMenuW(menu, MF_STRING, 300 + i, wide(label).as_ptr()); }
                            let mut pt = POINT { x: 0, y: 0 };
                            GetCursorPos(&mut pt);
                            let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY, pt.x, pt.y, 0, h, null());
                            DestroyMenu(menu);
                            let choices = [None, Some(0), Some(7), Some(30), Some(90), Some(180), Some(365)];
                            if (300..307).contains(&cmd) {
                                let days = choices[(cmd - 300) as usize];
                                event(h, Event::SetDays(days));
                                if let Some(n) = days {
                                    let today = ktm::backend::current_kst_ymd();
                                    let end = (today.0 * 10000 + today.1 * 100 + today.2) as i64;
                                    if n == 0 { event(h, Event::SetDateFilter(DateFilter::Today)); }
                                    else if cmd == 305 { event(h, Event::SetCustomDate(ktm::backend::shift_ymd_months(end, -6), end)); }
                                    else if cmd == 306 { event(h, Event::SetCustomDate(ktm::backend::shift_ymd_months(end, -12), end)); }
                                    else { event(h, Event::SetCustomDate(ktm::backend::shift_ymd(end, -(n as i64)), end)); }
                                } else { event(h, Event::SetDateFilter(DateFilter::All)); }
                            } else if cmd == 307 {
                                event(h, Event::ToggleOptions);
                                layout(h);
                                SetFocus(with(|u| u.days_edit));
                            }
                        }
                        Some(OptionControl::ExcludeFilename) => {
                            let v = with(|u| !u.state.options.exclude_filename);
                            event(h, Event::SetExcludeFilename(v));
                        }
                        Some(OptionControl::WithinQuery) => {
                            edit(|u| u.within_open = true);
                            layout(h);
                            SetFocus(with(|u| u.within_edit));
                        }
                        Some(OptionControl::Preset) => {
                            event(h, Event::ToggleOptions);
                            layout(h);
                            if with(|u| u.state.options.options_open) { SetFocus(with(|u| u.days_edit)); }
                            else { SetFocus(with(|u| u.edit)); }
                        }
                        None => {}
                    }
                } else if y >= tab_y && y < tab_y + tab_h {
                    // ② 탭 클릭
                    if let Some(tab_idx) = hit_tab(x, y, pad, tab_y, tab_h, rc_w.right) {
                        let tab = match tab_idx {
                            0 => Tab::Body,
                            1 => Tab::Hierarchy,
                            _ => Tab::Names,
                        };
                        event(h, Event::SwitchTab(tab));
                    }
                } else if y >= oy + oh && y < oy + oh + scale(h, SUMMARY_H) {
                    let sy = oy + oh;
                    let group_left = pad + scale(h, 235);
                    let group_right = rc_w.right - scale(h, 132);
                    if x >= rc_w.right - scale(h, 124) && x < rc_w.right - scale(h, 66) {
                        let rows = with(|u| {
                            let idx = ktm::layout::display_result_indices(
                                &u.rows,
                                &u.state.options.within_query,
                                u.state.options.exclude_filename,
                                &u.state.options.file_types,
                                u.state.options.collapsed_filename,
                                u.state.options.collapsed_body,
                            );
                            let shown = idx
                                .into_iter()
                                .filter_map(|i| u.rows.get(i))
                                .collect::<Vec<_>>();
                            ktm::layout::copy_result_text(
                                &shown.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
                                &shown.iter().map(|r| r.path.clone()).collect::<Vec<_>>(),
                            )
                        });
                        let message = if rows.is_empty() {
                            "복사할 결과가 없습니다".to_string()
                        } else {
                            copy_text_to_clipboard(&rows)
                                .map(|_| "표시 결과를 클립보드에 복사했습니다".to_string())
                                .unwrap_or_else(|e| e)
                        };
                        edit(|u| u.status_text = message);
                    } else if x >= rc_w.right - scale(h, 62) && x < rc_w.right - scale(h, 4) {
                        let rows = with(|u| {
                            let idx = ktm::layout::display_result_indices(
                                &u.rows,
                                &u.state.options.within_query,
                                u.state.options.exclude_filename,
                                &u.state.options.file_types,
                                u.state.options.collapsed_filename,
                                u.state.options.collapsed_body,
                            );
                            idx.into_iter()
                                .filter_map(|i| u.rows.get(i))
                                .cloned()
                                .collect::<Vec<_>>()
                        });
                        let tab = with(|u| u.state.tab);
                        match ktm::export::export_results(
                            ktm::export::ExportFormat::Csv,
                            ktm::export::ExportKind::from(tab),
                            &rows,
                        ) {
                            Ok(path) => {
                                edit(|u| u.status_text = format!("CSV 저장: {}", path.display()))
                            }
                            Err(e) => edit(|u| u.status_text = format!("CSV 저장 실패: {e}")),
                        }
                    } else if let Some(filename) =
                        hit_summary_group(x, y, sy, scale(h, SUMMARY_H), group_left, group_right)
                    {
                        event(h, Event::ToggleGroup(filename));
                    } else if x < pad + scale(h, 34) {
                        event(h, Event::ToggleView);
                    }
                } else if y >= fy && y < fy + fh {
                    // ③ 통합 검색 범위 단추 (기간 / 형식 / 폴더)
                    let mut fx = pad + scale(h, 34);
                    let date_chips = [
                        (DateFilter::All, 36),
                        (DateFilter::Today, 36),
                        (DateFilter::ThisWeek, 46),
                        (DateFilter::ThisMonth, 46),
                        (DateFilter::ThisYear, 36),
                        (DateFilter::Custom, 36),
                    ];
                    let mut handled = false;
                    for (df, w) in &date_chips {
                        let cw = scale(h, *w);
                        if x >= fx && x < fx + cw {
                            event(h, Event::SetDateFilter(*df));
                            handled = true;
                            break;
                        }
                        fx += cw + scale(h, 3);
                    }

                    // 형식(확장자) 칩 클릭
                    if !handled {
                        fx += scale(h, 6) + scale(h, 34);
                        let ext_chips: [(Option<&'static str>, i32); 5] = [
                            (None, 36),
                            (Some("hwp"), 40),
                            (Some("pdf"), 38),
                            (Some("docx"), 40),
                            (Some("xlsx"), 38),
                        ];
                        for (opt, w) in &ext_chips {
                            let cw = scale(h, *w);
                            if x >= fx && x < fx + cw {
                                let cur = with(|u| u.state.selected_ext.clone());
                                match opt {
                                    None => event(h, Event::SetExtFilter(None)),
                                    Some(ext) => {
                                        if cur.as_deref() == Some(*ext) {
                                            event(h, Event::SetExtFilter(None));
                                        } else {
                                            event(h, Event::SetExtFilter(Some(ext.to_string())));
                                        }
                                    }
                                }
                                handled = true;
                                break;
                            }
                            fx += cw + scale(h, 3);
                        }
                    }

                    // 폴더 칩 클릭
                    if !handled {
                        let has_folder_info = with(|u| {
                            !u.state.folder_counts.is_empty() || u.state.selected_folder.is_some()
                        });
                        if has_folder_info && fx + scale(h, 80) < rc_w.right - pad {
                            fx += scale(h, 6) + scale(h, 34);
                            let all_w = scale(h, 38);
                            if x >= fx && x < fx + all_w {
                                event(h, Event::SetFolderFilter(None));
                            } else {
                                fx += scale(h, 41);
                                let mut folders_to_check: Vec<String> = Vec::new();
                                let sel_opt = with(|u| u.state.selected_folder.clone());
                                let fcs = with(|u| u.state.folder_counts.clone());
                                if let Some(ref sel) = sel_opt {
                                    if !fcs.iter().any(|(f, _)| f == sel) {
                                        folders_to_check.push(sel.clone());
                                    }
                                }
                                for (f, _) in fcs {
                                    folders_to_check.push(f);
                                }

                                for folder in folders_to_check {
                                    let cw = scale(h, 68).min(rc_w.right - fx - pad);
                                    if cw < scale(h, 28) {
                                        break;
                                    }
                                    if x >= fx && x < fx + cw {
                                        let cur = with(|u| u.state.selected_folder.clone());
                                        if cur.as_deref() == Some(&folder) {
                                            event(h, Event::SetFolderFilter(None));
                                        } else {
                                            event(h, Event::SetFolderFilter(Some(folder)));
                                        }
                                        break;
                                    }
                                    fx += cw + scale(h, 3);
                                }
                            }
                        }
                    }
                } else if with(|u| u.state.query.trim().is_empty() && u.rows.is_empty()) {
                    let entries = with(|u| u.state.history_entries());
                    if !entries.is_empty() {
                        let btn_y = fy + fh + scale(h, 14) + scale(h, 24);
                        let rh = scale(h, 28);
                        let status_h = scale(h, STATUS_H);
                        let max_visible =
                            ((rc_w.bottom - status_h - btn_y).max(0) / rh).max(0) as usize;
                        let n_hist = entries.len().min(max_visible);
                        for i in 0..n_hist {
                            let top = btn_y + i as i32 * rh;
                            let left = pad;
                            let right = rc_w.right - pad;
                            let bottom = top + rh;
                            if x >= left && x < right && y >= top && y < bottom {
                                event(h, Event::ApplyHistory(i));
                                break;
                            }
                        }
                    } else {
                        // ④ 빈 검색어 첫 화면 주제 1~10위 단추
                        let ty = fy + fh + scale(h, 14) + scale(h, 24);
                        let btn_h = scale(h, 32);
                        let col_w = (rc_w.right - 2 * pad - scale(h, 16)) / 2;
                        let n_topics = with(|u| u.state.top_topics.len().min(10));
                        for i in 0..n_topics {
                            let col = (i % 2) as i32;
                            let row = (i / 2) as i32;
                            let left = pad + col * (col_w + scale(h, 16));
                            let top = ty + row * (btn_h + scale(h, 6));
                            if x >= left && x < left + col_w && y >= top && y < top + btn_h {
                                event(h, Event::ApplyTopTopic(i));
                                break;
                            }
                        }
                    }
                } else if !with(|u| u.state.autocompletes.is_empty())
                    && y >= fy + fh + scale(h, 4)
                    && y < fy + fh + scale(h, 26)
                {
                    // ⑤ 자동 완성 칩 클릭
                    let mut ax = pad + scale(h, 62);
                    let n_ac = with(|u| u.state.autocompletes.len().min(5));
                    for i in 0..n_ac {
                        let cw = scale(h, 85);
                        if x >= ax && x < ax + cw {
                            event(h, Event::ApplyAutocomplete(i));
                            break;
                        }
                        ax += cw + scale(h, 6);
                    }
                } else {
                    // ⑥ 결과 행 클릭 또는 [폴더 열기] 단추 클릭
                    let top_y = with(|u| list_top_coord(h, u));
                    let rh = scale(h, ROW_H);
                    let folder_w = scale(h, ROW_FOLDER_W);
                    let preview_h = with(|u| {
                        if u.state.selected.is_some() && !u.rows.is_empty() {
                            scale(h, PREVIEW_H)
                        } else {
                            0
                        }
                    });
                    if y >= top_y && y < client_bottom(h) - scale(h, STATUS_H) - preview_h {
                        let slot = with(|u| u.top) + ((y - top_y) / rh) as usize;
                        let grid = with(|u| u.state.options.grid_view);
                        let col = if x >= side_w + (rc_w.right - side_w) / 2 {
                            1
                        } else {
                            0
                        };
                        let n = with(|u| {
                            let ids = ktm::layout::display_result_indices(
                                &u.rows,
                                &u.state.options.within_query,
                                u.state.options.exclude_filename,
                                &u.state.options.file_types,
                                u.state.options.collapsed_filename,
                                u.state.options.collapsed_body,
                            );
                            if grid {
                                ids.get((slot / 2) * 2 + col).copied()
                            } else {
                                ids.get(slot).copied()
                            }
                        });
                        if let Some(n) = n.filter(|i| *i < with(|u| u.rows.len())) {
                            let screen_slot = slot.saturating_sub(with(|u| u.top));
                            let row_top = if grid {
                                top_y + (screen_slot / 2) as i32 * rh
                            } else {
                                top_y + ((y - top_y) / rh) * rh
                            };
                            let row_right = if grid {
                                let card_w = (rc_w.right - side_w - scale(h, 24)) / 2;
                                side_w + scale(h, 8) + col as i32 * (card_w + scale(h, 8)) + card_w
                            } else {
                                rc_w.right
                            };
                            let btn_rc = calc_folder_button_rect(
                                row_top,
                                row_right,
                                scale(h, 6),
                                scale(h, 4),
                                folder_w,
                                scale(h, 22),
                            );
                            let is_folder_click = hit_folder_button(x, y, btn_rc);
                            if is_folder_click {
                                event(h, Event::Select(n));
                                event(h, Event::Open { folder: true });
                            } else {
                                event(h, Event::Select(n));
                                if msg == WM_LBUTTONDBLCLK {
                                    event(h, Event::Open { folder: false });
                                }
                            }
                        }
                    }
                }
                SetFocus(with(|u| if u.within_open { u.within_edit } else { u.edit }));
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
                    match reply {
                        Reply::TopTopics(topics) => {
                            edit(|u| {
                                u.state.top_topics = topics;
                            });
                            continue;
                        }
                        Reply::Snippets(g, rows) => {
                            edit(|u| {
                                if g == u.state.generation {
                                    for (i, r) in rows {
                                        if let Some(slot) = u.rows.get_mut(i) {
                                            slot.snippet = r.snippet;
                                            slot.preview = r.preview;
                                            if !r.location.is_empty() {
                                                slot.location = r.location;
                                            }
                                        }
                                    }
                                }
                            });
                            continue;
                        }
                        Reply::Search(g, result, start, times) => {
                            phase("search_queue_ms", times.picked - start);
                            phase("search_work_ms", times.done - times.picked);
                            phase("search_deliver_ms", times.done.elapsed());
                            if g != with(|u| u.state.generation) {
                                continue;
                            }
                            match result {
                                Ok(Answer {
                                    rows,
                                    terms,
                                    folder_counts,
                                    autocompletes,
                                }) => {
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
                                        if u.state.tab == Tab::Body || u.state.tab == Tab::Hierarchy
                                        {
                                            ktm::backend::sort_rows(&mut u.rows, u.body_sort);
                                        }
                                        u.terms = terms;
                                        u.state.folder_counts = folder_counts;
                                        u.state.autocompletes = autocompletes;
                                        u.timing = Some(start);
                                        u.ready_at = Some(Instant::now());
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
                            let action = if autostart {
                                "autostart-off"
                            } else {
                                "autostart-on"
                            };
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

        // 창 생성: WS_POPUP | WS_THICKFRAME | WS_CLIPCHILDREN
        let h = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class.as_ptr(),
            wide("퀵몽땅검색").as_ptr(),
            WS_POPUP | WS_THICKFRAME | WS_CLIPCHILDREN,
            0,
            0,
            700,
            510,
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
            20,
            44,
            660,
            24,
            h,
            1 as _,
            inst,
            null(),
        );
        SendMessageW(ed, 0x00c5, 4096, 0);
        let within_ed = CreateWindowExW(
            0,
            wide("EDIT").as_ptr(),
            wide("").as_ptr(),
            WS_CHILD | ES_AUTOHSCROLL as u32,
            0,
            0,
            1,
            1,
            h,
            2 as _,
            inst,
            null(),
        );
        SendMessageW(within_ed, 0x00c5, 1024, 0);
        let days_ed = CreateWindowExW(0, wide("EDIT").as_ptr(), wide("").as_ptr(), WS_CHILD | WS_TABSTOP | ES_AUTOHSCROLL as u32, 0, 0, 1, 1, h, 3 as _, inst, null());
        SendMessageW(days_ed, 0x00c5, 5, 0);

        let initial_theme = arg("--theme").unwrap_or(settings.theme.clone());
        sync_theme(&initial_theme);

        UI.with(|u| {
            *u.borrow_mut() = Some(Ui {
                state: State {
                    recent_queries: settings.recent_queries.clone(),
                    favorite_queries: settings.favorite_queries.clone(),
                    ..State::default()
                },
                rows: Vec::new(),
                terms: Vec::new(),
                edit: ed,
                within_edit: within_ed,
                days_edit: days_ed,
                within_open: false,
                font: null_mut(),
                font_bold: null_mut(),
                small: null_mut(),
                small_bold: null_mut(),
                title_font: null_mut(),
                glyph: null_mut(),
                glyph_big: null_mut(),
                brush: null_mut(),
                theme: initial_theme,
                tx: pending.clone(),
                rx,
                status: status_tx,
                top: 0,
                timing: None,
                ready_at: None,
                hotkey: String::new(),
                hotkey_error: String::new(),
                cancel: Arc::new(AtomicBool::new(false)),
                settings_path: data.join("settings.json"),
                warmed: pipe_mode_early,
                name_sort: 0,
                body_sort: ktm::backend::BodySort::default(),
                hover_head_btn: None,
                hover_row: None,
                hover_folder_btn: None,
                hover_tab: None,
                status_text: String::new(),
                smart_presets: settings.smart_presets.clone(),
                recent_query_times: settings.recent_query_times.clone(),
            })
        });

        SetWindowLongPtrW(h, GWLP_WNDPROC, proc as *const () as isize);
        frame::style_frame(h);
        create_fonts(h);
        layout(h);

        let worker_pending = pending;
        let handle = h as usize;
        let pipe_name = arg("--pipe-name").unwrap_or(ktm::backend::PIPE_NAME.into());
        let pipe_mode = arg("--backend").as_deref() == Some("pipe");
        let para_path = arg("--content-db")
            .map(PathBuf::from)
            .or_else(ktm::paragraphs::default_path);

        std::thread::spawn(move || {
            let open =
                |root: &PathBuf| Backend::open(root).map(|b| b.with_paragraphs(para_path.clone()));
            let mut backend: Option<Backend> = None;
            if !pipe_mode {
                let t = Instant::now();
                backend = open(&root).ok();
                if let Some(b) = backend.as_mut() {
                    let _ = b.warm();
                    let top_topics = b.top_topics(10).into_iter().map(|item| item.term).collect();
                    let _ = tx.send(Reply::TopTopics(top_topics));
                    PostMessageW(handle as HWND, READY, 0, 0);
                }
                PostMessageW(handle as HWND, WARMED, t.elapsed().as_micros() as usize, 0);
            }
            loop {
                enum Work {
                    Search(Job),
                    Snippets(SnipJob),
                    Export(Job, ktm::export::ExportFormat),
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
                        if let Some(j) = slot.export.take() {
                            break Work::Export(j.0, j.1);
                        }
                        slot = worker_pending.1.wait(slot).unwrap();
                    }
                };
                let reply = match work {
                    Work::Search((
                        g,
                        q,
                        tab,
                        start,
                        cancel,
                        sort,
                        date_filter,
                        custom_date_range,
                        folder,
                        ext,
                        match_mode,
                    )) => {
                        let picked = Instant::now();
                        let hierarchy = tab == Tab::Hierarchy;
                        let answer = (|| {
                            if tab == Tab::Names {
                                let rows = ktm::backend::pipe_names(&pipe_name, &q, sort)?;
                                let terms = q.split_whitespace().map(str::to_owned).collect();
                                return Ok(Answer {
                                    rows,
                                    terms,
                                    folder_counts: Vec::new(),
                                    autocompletes: Vec::new(),
                                });
                            }
                            if pipe_mode {
                                let rows = ktm::backend::pipe_search(&pipe_name, &q, hierarchy)?;
                                let terms = q.split_whitespace().map(str::to_owned).collect();
                                return Ok(Answer {
                                    rows,
                                    terms,
                                    folder_counts: Vec::new(),
                                    autocompletes: Vec::new(),
                                });
                            }
                            if backend.is_none() {
                                backend = Some(open(&root)?);
                            }
                            let b = backend.as_mut().unwrap();
                            let date_range =
                                ktm::backend::date_range_for_filter(date_filter, custom_date_range);
                            b.search_with_scope_ext(
                                &q,
                                hierarchy,
                                date_range,
                                folder.as_deref(),
                                ext.as_deref(),
                                match_mode,
                                Some(cancel),
                            )
                        })();
                        Reply::Search(
                            g,
                            answer,
                            start,
                            WorkTimes {
                                picked,
                                done: Instant::now(),
                            },
                        )
                    }
                    Work::Export(
                        (
                            _g,
                            q,
                            tab,
                            _start,
                            cancel,
                            sort,
                            date_filter,
                            custom_date_range,
                            folder,
                            ext,
                            match_mode,
                        ),
                        format,
                    ) => {
                        let hierarchy = tab == Tab::Hierarchy;
                        let kind = ktm::export::ExportKind::from(tab);
                        let rows_result: Result<Vec<Row>, String> = (|| {
                            if q.trim().is_empty() {
                                return Ok(Vec::new());
                            }
                            if tab == Tab::Names {
                                return ktm::backend::pipe_names_limit(
                                    &pipe_name,
                                    &q,
                                    sort,
                                    ktm::export::EXPORT_LIMIT,
                                );
                            }
                            if pipe_mode {
                                return ktm::backend::pipe_search_limit(
                                    &pipe_name,
                                    &q,
                                    hierarchy,
                                    ktm::export::EXPORT_LIMIT,
                                );
                            }
                            if backend.is_none() {
                                backend = Some(open(&root)?);
                            }
                            let b = backend.as_mut().unwrap();
                            let date_range =
                                ktm::backend::date_range_for_filter(date_filter, custom_date_range);
                            let ans = b.search_with_scope_ext_limit(
                                &q,
                                hierarchy,
                                date_range,
                                folder.as_deref(),
                                ext.as_deref(),
                                match_mode,
                                ktm::export::EXPORT_LIMIT,
                                Some(cancel),
                            )?;
                            let mut rows = ans.rows;
                            b.fill_snippets(&mut rows, &ans.terms);
                            Ok(rows)
                        })();

                        if let Ok(rows) = rows_result {
                            if let Ok(saved_path) =
                                ktm::export::export_to_file(format, kind, &rows, None, None)
                            {
                                let _ = ktm::export::open_export_folder(&saved_path);
                            }
                        }
                        continue;
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

        if with(|u| u.state.phase) == Phase::Hidden {
            prerender(h);
            let fg = GetForegroundWindow();
            SetWindowPos(
                h,
                null_mut(),
                -32000,
                -32000,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            UpdateWindow(h);
            if std::env::var_os("KTM_EXP_NO_FOCUS").is_none() {
                SetFocus(ed);
            }
            ShowWindow(h, SW_HIDE);
            if GetForegroundWindow() != fg {
                log("startup_foreground_changed", 1.0);
            }
            log(
                "startup_prerender_ms",
                started.elapsed().as_secs_f64() * 1000.0,
            );
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
            for f in [
                u.font,
                u.font_bold,
                u.small,
                u.small_bold,
                u.title_font,
                u.glyph,
                u.glyph_big,
            ] {
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
