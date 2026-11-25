//! Win32 입력/IME + GDI 이중 버퍼. 검색 스레드는 이벤트로만 깨며 창 상태는 step으로만 바꾼다.
//! 퀵몽땅클립보드(kagent/src/popup.rs)와 ui-core/ui_theme 스타일을 적용한 재디자인.
#[path = "frame.rs"]
pub(crate) mod frame;
#[path = "window_probe.rs"]
mod probe;
#[path = "window_capture.rs"]
mod capture;

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
const QUICK_SEARCH_EMPTY_HINT: &str = "↑↓ 선택 · Enter 열기 · Ctrl+D 즐겨찾기";
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
    usize,
    bool,
);
/// 스크롤로 새로 보이게 된 줄의 원문 찾기: (세대, [(줄 번호, 줄)], 강조 낱말).
type SnipJob = (u64, Vec<(usize, Row)>, Vec<String>, bool);

fn should_hide_on_close(minimize_to_tray: bool) -> bool {
    minimize_to_tray
}

#[cfg(test)]
mod close_policy_tests {
    use super::should_hide_on_close;

    #[test]
    fn close_policy_follows_tray_setting() {
        assert!(should_hide_on_close(true));
        assert!(!should_hide_on_close(false));
    }
}

/// 일꾼 대기열: 검색은 늘 최신 하나, 원문 찾기도 최신 하나(검색 먼저).
#[derive(Default)]
struct Pending {
    search: Vec<Job>,
    snippets: Option<SnipJob>,
    export: Option<(Job, ktm::export::ExportFormat)>,
}

#[derive(Clone, Copy)]
struct WorkTimes {
    picked: Instant,
    done: Instant,
}

enum Reply {
    Unified(u64, ktm::backend::unified::Snapshot, Instant, WorkTimes),
    Search(u64, Tab, Result<Answer, String>, Instant, WorkTimes),
    Snippets(u64, Vec<(usize, Row)>),
    TopTopics(Vec<String>),
}

struct Ui {
    state: State,
    rows: Vec<Row>,
    combined_batches: Vec<(tm_state::ResultKind, Vec<Row>)>,
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
    unified_paint: Option<(Instant, bool)>,
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
    /// 통합 검색 갈래(파일명·위계·본문) 중 실패한 것의 오류 문구(한 검색 동안).
    leg_errors: Vec<String>,
    smart_presets: Vec<ktm::settings::SmartPreset>,
    recent_query_times: Vec<u64>,
    modal: Option<ktm::layout::ModalKind>,
    modal_tab: usize,
    pending_ui_scale: u32,
    settings: Settings,
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn local_drives() -> Vec<PathBuf> {
    ('A'..='G')
        .filter_map(|c| {
            let path = PathBuf::from(format!("{c}:\\"));
            path.is_dir().then_some(path)
        })
        .collect()
}

fn pick_folder(owner: HWND) -> Option<String> {
    unsafe {
        use windows_sys::Win32::UI::Shell::{
            BIF_RETURNONLYFSDIRS, BROWSEINFOW, ILFree, SHBrowseForFolderW, SHGetPathFromIDListW,
        };
        let mut display = [0u16; 260];
        let title = wide("검색 범위에 추가할 폴더를 선택하세요");
        let info = BROWSEINFOW {
            hwndOwner: owner,
            pidlRoot: null_mut(),
            pszDisplayName: display.as_mut_ptr(),
            lpszTitle: title.as_ptr(),
            ulFlags: BIF_RETURNONLYFSDIRS,
            lpfn: None,
            lParam: 0,
            iImage: 0,
        };
        let pidl = SHBrowseForFolderW(&info);
        if pidl.is_null() {
            return None;
        }
        let mut path = [0u16; 32768];
        let selected = SHGetPathFromIDListW(pidl, path.as_mut_ptr()) != 0;
        ILFree(pidl);
        if selected {
            Some(String::from_utf16_lossy(
                &path[..path.iter().position(|c| *c == 0).unwrap_or(path.len())],
            ))
        } else {
            None
        }
    }
}

fn with<R>(f: impl FnOnce(&Ui) -> R) -> R {
    UI.with(|u| f(u.borrow().as_ref().unwrap()))
}

fn edit<R>(f: impl FnOnce(&mut Ui) -> R) -> R {
    UI.with(|u| f(u.borrow_mut().as_mut().unwrap()))
}

fn scale(h: HWND, n: i32) -> i32 {
    frame::scale(h, n) * with(|u| u.settings.bounded_ui_scale() as i32) / 100
}

/// `edit(|u| ...)` 안에서는 `scale()`이 쓰는 `with()`가 이미 빌려진 RefCell을 다시 빌려 죽으므로
/// (RefCell already mutably borrowed), 같은 계산을 Ui를 직접 받아 한다.
fn scale_u(h: HWND, n: i32, u: &Ui) -> i32 {
    frame::scale(h, n) * u.settings.bounded_ui_scale() as i32 / 100
}

fn quick_layout(h: HWND, u: &Ui) -> QuickSearchLayout {
    let mut r: RECT = unsafe { std::mem::zeroed() };
    unsafe { GetClientRect(h, &mut r); }
    let groups = kfind_core::split_semicolon_groups(&u.state.query);
    ktm::layout::quick_search_layout(
        scale_u(h, 100, u), r.right, r.bottom, groups.len() > 1, u.state.options.options_open,
        !u.state.autocompletes.is_empty(),
    )
}

fn row_height(h: HWND) -> i32 {
    let compact = with(|u| u.settings.result_density == "compact");
    scale(h, ktm::layout::result_card_height(100, compact)).max(1)
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
            SendMessageW(ed, 0x00b1, 0, -1);
            SendMessageW(ed, 0x00c2, 1, q.as_ptr() as isize);
            SendMessageW(ed, 0x00b1, caret, caret as isize);
        }
    }
}

fn default_mode(mode: &str) -> tm_state::SearchMode {
    if mode == "filename" {
        tm_state::SearchMode::Filename
    } else {
        tm_state::SearchMode::Keyword
    }
}

#[allow(dead_code)]
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
        let mut s = with(|u| u.settings.clone());
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
    let mut search_cancel: Option<Arc<AtomicBool>> = None;
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
                    // 사이드바가 열려 있으면 그 폭만큼 넓혀 내용 영역(칩·단추 줄)이 좁아지지 않게 한다.
                    let side_extra = if with(|u| u.state.options.sidebar_open) { scale(h, ktm::layout::SIDEBAR_W) } else { 0 };
                    let w = (scale(h, 700) + side_extra).min(mi.rcWork.right - mi.rcWork.left);
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
                    let cancel = search_cancel.get_or_insert_with(|| Arc::new(AtomicBool::new(false))).clone();
                    edit(|u| u.cancel = cancel.clone());
                    let sort = with(|u| ktm::backend::NAME_SORTS[u.name_sort % 4].0);
                    let max_results = with(|u| u.settings.bounded_max_results());
                    let skip_cloud_network = with(|u| u.settings.skip_cloud_network);
                    {
                        let mut p = tx.0.lock().unwrap();
                        if tab == Tab::All {
                            edit(|u| { u.combined_batches.clear(); u.leg_errors.clear(); });
                            p.search.clear();
                            let parts = if with(|u| u.settings.unified_search) {
                                vec![(Tab::All, max_results)]
                            } else {
                                vec![(Tab::Names, 20), (Tab::Hierarchy, 20), (Tab::Body, 40)]
                            };
                            for (part, limit) in parts {
                                p.search.push((generation, query.clone(), part, Instant::now(), cancel.clone(), sort,
                                    date_filter, custom_date_range, folder.clone(), ext.clone(), match_mode,
                                    max_results.min(limit), skip_cloud_network));
                            }
                        } else {
                            p.search.clear();
                            p.search.push((generation, query, tab, Instant::now(), cancel, sort,
                                date_filter, custom_date_range, folder, ext, match_mode, max_results, skip_cloud_network));
                        }
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
            let compact = u.settings.result_density == "compact";
            let rh = scale_u(h, ktm::layout::result_card_height(100, compact), u).max(1);
            let preview_h = if u.state.selected.is_some() && !u.rows.is_empty() {
                scale_u(h, PREVIEW_H, u)
            } else {
                0
            };
            let count = ((rc.bottom - top_y - scale_u(h, STATUS_H, u) - preview_h).max(0) / rh)
                .max(1) as usize;
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
    quick_layout(h, u).list_top
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
        u.tx.0.lock().unwrap().snippets = Some((
            u.state.generation,
            want,
            u.terms.clone(),
            u.settings.skip_cloud_network,
        ));
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

/// 정렬을 다음 방식으로 돌린다(옵션 줄의 '정렬' 단추와 Ctrl+S가 같은 동작).
fn cycle_sort(h: HWND) {
    let next = with(|u| u.state.options.sort.next());
    event(h, Event::SetSort(next));
    edit(|u| {
        u.body_sort = match next {
            tm_state::SortMode::Relevance => ktm::backend::BodySort::Relevance,
            tm_state::SortMode::Confidence => ktm::backend::BodySort::Confidence,
            tm_state::SortMode::Newest => ktm::backend::BodySort::Date,
            tm_state::SortMode::Oldest => ktm::backend::BodySort::Oldest,
            tm_state::SortMode::Name => ktm::backend::BodySort::Name,
        };
        u.name_sort = match next {
            tm_state::SortMode::Name => 1,
            tm_state::SortMode::Newest | tm_state::SortMode::Oldest => 3,
            _ => 0,
        };
        ktm::backend::sort_rows(&mut u.rows, u.body_sort);
    });
    unsafe { InvalidateRect(h, null(), 0) };
}

fn key(h: HWND, key: usize) -> bool {
    unsafe {
        if with(|u| u.modal.is_some()) {
            match key as u16 {
                VK_ESCAPE => {
                    edit(|u| u.modal = None);
                    InvalidateRect(h, null(), 0);
                    return true;
                }
                VK_TAB | VK_LEFT | VK_RIGHT => {
                    edit(|u| {
                        let n = if u.modal == Some(ktm::layout::ModalKind::Statistics) {
                            2
                        } else {
                            4
                        };
                        u.modal_tab = (u.modal_tab + 1) % n;
                    });
                    InvalidateRect(h, null(), 0);
                    return true;
                }
                _ => return true,
            }
        }
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
            // 탭 줄이 없어졌으므로 Tab 키는 검색창 포커스를 그대로 둔다.
            VK_TAB => {}
            _ if ctrl && key == b'S' as usize => cycle_sort(h),
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
                            u.settings.bounded_max_results(),
                            u.settings.skip_cloud_network,
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

/// 사이드바를 열고 닫을 때 창 폭을 사이드바 폭만큼 늘리고 줄인다(작업 영역을 넘지 않는 범위에서).
unsafe fn resize_for_sidebar(h: HWND, opened: bool) {
    unsafe {
        let mut rc: RECT = std::mem::zeroed();
        GetWindowRect(h, &mut rc);
        let mon = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of_val(&mi) as u32;
        GetMonitorInfoW(mon, &mut mi);
        let delta = scale(h, ktm::layout::SIDEBAR_W);
        let min_w = scale(h, 520);
        let work_w = mi.rcWork.right - mi.rcWork.left;
        let width = rc.right - rc.left;
        let new_w = if opened { (width + delta).min(work_w.max(width)) } else { (width - delta).max(min_w.min(width)) };
        let left = if opened { rc.left.min(mi.rcWork.right - new_w).max(mi.rcWork.left) } else { rc.left };
        SetWindowPos(h, null_mut(), left, rc.top, new_w, rc.bottom - rc.top, SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

unsafe fn layout(h: HWND) {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(h, &mut r);
        let modal_open = with(|u| u.modal.is_some());
        let head = scale(h, HEAD_H);
        let pad = scale(h, 12);
        let box_y = head + scale(h, 6);
        let box_h = scale(h, SEARCH_BOX_H);
        let ed_margin_x = scale(h, 8);
        let ed_margin_y = scale(h, 4);
        let side_w = { let full = scale(h, SIDEBAR_W); with(|u| ktm::layout::sidebar_width_px(u.state.options.sidebar_open, r.right, full)) };

        MoveWindow(
            with(|u| u.edit),
            side_w + pad + ed_margin_x,
            box_y + ed_margin_y,
            r.right - side_w - 2 * pad - 2 * ed_margin_x,
            box_h - 2 * ed_margin_y,
            1,
        );
        ShowWindow(with(|u| u.edit), if modal_open { SW_HIDE } else { SW_SHOW });
        let spans = option_spans(pad + side_w, r.right - pad);
        let option_top = with(|u| quick_layout(h, u).options.top);
        let within = spans[5];
        let within_hwnd = with(|u| u.within_edit);
        MoveWindow(
            within_hwnd,
            within.0 + scale(h, 2),
            option_top,
            (within.1 - within.0 - scale(h, 4)).max(1),
            scale(h, OPTION_H / 2),
            1,
        );
        ShowWindow(
            within_hwnd,
            if !modal_open && with(|u| u.within_open) {
                SW_SHOW
            } else {
                SW_HIDE
            },
        );
        let days = with(|u| u.days_edit);
        MoveWindow(
            days,
            pad + side_w + scale(h, 258),
            option_top + scale(h, OPTION_H / 2),
            scale(h, 48),
            scale(h, 22),
            1,
        );
        ShowWindow(
            days,
            // 직접 일수 입력칸은 없앴다(기간은 전체·오늘·7일간·30일간·올해 칩과 기간 메뉴로 고른다).
            SW_HIDE,
        );
    }
}

unsafe fn highlighted(dc: HDC, s: &str, terms: &[String], rc: RECT, fg: u32, bg: u32) {
    unsafe {
        let saved = SaveDC(dc);
        // 한 줄에 다 들어가지 않는 이름은 칸 끝을 '…'로 표시한다(예전에는 글자가 중간에서 뚝 잘렸다).
        let full = wide(s);
        let mut full_size: SIZE = std::mem::zeroed();
        GetTextExtentPoint32W(dc, full.as_ptr(), (full.len() - 1) as i32, &mut full_size);
        let ellipsis = wide("…");
        let mut ell_size: SIZE = std::mem::zeroed();
        GetTextExtentPoint32W(dc, ellipsis.as_ptr(), 1, &mut ell_size);
        let truncated = full_size.cx > rc.right - rc.left;
        let text_right = if truncated { (rc.right - ell_size.cx).max(rc.left) } else { rc.right };
        IntersectClipRect(dc, rc.left, rc.top, text_right, rc.bottom);
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
                // 강조 칸(노란 바탕)에서는 어두운 글자색을 써야 읽힌다(어두운 테마의 흰 글자는 안 보였다).
                SetTextColor(dc, if marked { 0x202020 } else { fg });
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
        if truncated {
            SelectClipRgn(dc, null_mut());
            IntersectClipRect(dc, rc.left, rc.top, rc.right, rc.bottom);
            SetTextColor(dc, fg);
            SetBkMode(dc, TRANSPARENT as i32);
            TextOutW(dc, text_right, rc.top + (rc.bottom - rc.top - ell_size.cy) / 2, ellipsis.as_ptr(), 1);
        }
        RestoreDC(dc, saved);
    }
}

unsafe fn highlighted_groups(dc: HDC, s: &str, groups: &[Vec<String>], rc: RECT, fg: u32, palette: ui_theme::Palette) {
    unsafe {
        let mut spans: Vec<(usize, usize, usize)> = groups.iter().enumerate().flat_map(|(g, terms)|
            ktm::highlight::ranges_terms(s, terms).into_iter().map(move |(a,b)| (a,b,g))
        ).collect();
        spans.sort_by_key(|x| (x.0, x.1));
        let mut merged: Vec<(usize, usize, usize)> = Vec::new();
        for (a,b,g) in spans {
            if let Some(last) = merged.last_mut().filter(|x| a < x.1) { last.1 = last.1.max(b); }
            else { merged.push((a,b,g)); }
        }
        let saved = SaveDC(dc);
        let mut offset = 0usize;
        let mut x = rc.left;
        for (start,end,group) in merged.into_iter().chain(std::iter::once((s.len(),s.len(),0))) {
            let pieces = if start == end { vec![(start,end,None)] } else { vec![(offset,start,None),(start,end,Some(group))] };
            for (a,b,g) in pieces {
                if a >= b { continue; }
                let w = wide(&s[a..b]); let mut size: SIZE = std::mem::zeroed();
                GetTextExtentPoint32W(dc,w.as_ptr(),(w.len()-1) as i32,&mut size);
                if let Some(i) = g { let (bg,fg2)=palette.group_highlight(i, ui_theme::theme().kind == ui_theme::Kind::Dark); SetTextColor(dc,fg2); SetBkColor(dc,bg); SetBkMode(dc,OPAQUE as i32); }
                else { SetTextColor(dc,fg); SetBkMode(dc,TRANSPARENT as i32); }
                TextOutW(dc,x,rc.top+(rc.bottom-rc.top-size.cy)/2,w.as_ptr(),(w.len()-1) as i32); x+=size.cx;
            }
            offset=end;
        }
        RestoreDC(dc,saved);
    }
}

fn query_group_terms(query: &str) -> Vec<Vec<String>> {
    kfind_core::split_semicolon_groups(query).into_iter().take(8).map(|g| {
        g.split_whitespace().filter(|w| !w.starts_with("ext:") && !w.starts_with("path:") && !w.starts_with("file:") && !w.starts_with('-'))
            .map(|w| w.trim_matches(['"','“','”']).to_owned()).filter(|w| !w.is_empty()).collect()
    }).collect()
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
        if let Some((start, complete)) = edit(|u| u.unified_paint.take()) {
            phase(if complete { "unified_complete_to_paint_ms" } else { "unified_names_to_paint_ms" }, start.elapsed());
        }
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
        if mem.is_null() {
            return;
        }
        let bitmap = CreateCompatibleBitmap(dc, r.right, r.bottom);
        if bitmap.is_null() {
            DeleteDC(mem);
            return;
        }
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

        let side_w = { let full = scale(h, SIDEBAR_W); with(|u| ktm::layout::sidebar_width_px(u.state.options.sidebar_open, r.right, full)) };
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
                    text(
                        "+",
                        RECT {
                            left: side_w - scale(h, 38),
                            top: scale(h, 40 + SIDEBAR_ROW_H),
                            right: side_w - scale(h, 8),
                            bottom: scale(h, 40 + 2 * SIDEBAR_ROW_H),
                        },
                        p.accent,
                        DT_CENTER,
                    );
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

        let chip_y = scale(h, 3);
        let chip_h = (head - scale(h, 6)).max(1);
        let chips = header_chip_rects(r.right, bw);
        let stats_rc = RECT {
            left: chips[0].0,
            top: chip_y,
            right: chips[0].1,
            bottom: chip_y + chip_h,
        };
        let settings_rc = RECT {
            left: chips[1].0,
            top: chip_y,
            right: chips[1].1,
            bottom: chip_y + chip_h,
        };
        fill(stats_rc, p.head_button_hover);
        fill(settings_rc, p.head_button_hover);
        SelectObject(mem, with(|u| u.small_bold));
        text("◷ 통계", stats_rc, p.head_glyph, DT_CENTER);
        text("⚙ 설정", settings_rc, p.head_glyph, DT_CENTER);

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
        // 힌트는 [통계]·[설정] 칩 왼쪽까지만(사이드바가 열려 제목이 밀리면 칩 위로 번졌다).
        let hint_right = (chips[0].0 - scale(h, 8)).min(buttons_left - scale(h, 8));
        if hint_left + scale(h, 60) < hint_right {
            text(
                "Ctrl+Alt+Space · Esc 숨김 · Ctrl+L 테마",
                RECT {
                    left: hint_left,
                    top: 0,
                    right: hint_right,
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

        // ③ 통합 검색: 탭 줄은 없다(본문·위계·파일명을 한 번에 찾는다).
        with(|u| {
            let bands = quick_layout(h, u);
            let lpad = pad + side_w;

            // ③a 옵션 바: 모드, 필터, 제외, 저장 프리셋
            let oy = bands.options.top;
            let oh = bands.options.bottom - bands.options.top;
            fill(
                RECT {
                    left: side_w,
                    top: oy,
                    right: r.right,
                    bottom: oy + oh,
                },
                p.bg,
            );
            SelectObject(mem, u.small);
            // 본문·위계·파일명을 한 번에 찾으므로 첫 칸은 모드 전환이 아닌 안내 글자다.
            let option_labels = [
                "통합 검색".to_string(),
                format!(
                    "정렬:{}",
                    u.state.options.sort.label()
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
                    match u.state.date_filter {
                        DateFilter::All => "전체".to_string(),
                        DateFilter::Today => "오늘".to_string(),
                        DateFilter::Last7Days => "7일간".to_string(),
                        DateFilter::Last30Days => "30일간".to_string(),
                        DateFilter::ThisYear => "올해".to_string(),
                        DateFilter::Custom => u
                            .state
                            .options
                            .days
                            .map(|d| format!("{d}일"))
                            .unwrap_or_else(|| "지정".into()),
                    }
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
                if u.state.options.options_open {
                    "[옵션 ▾]".into()
                } else {
                    "[옵션]".into()
                },
            ];
            let spans = option_spans(pad + side_w, r.right - pad);
            for (label, (left, right)) in option_labels.iter().zip(spans) {
                text(
                    label,
                    // 첫 줄은 옵션 띠의 위쪽 절반에만 그린다. 아래 절반은 옵션 팝업 줄이라
                    // 세로 가운데 맞춤으로 전체 높이에 그리면 팝업 글자와 겹쳤다.
                    RECT {
                        left: left + scale(h, 2),
                        top: oy,
                        right: (right - scale(h, 4)).max(left + scale(h, 2)),
                        bottom: oy + scale(h, OPTION_H / 2),
                    },
                    p.text_sub,
                    DT_END_ELLIPSIS,
                );
            }

            let second_y = oy + scale(h, OPTION_H / 2);
            if u.state.options.options_open {
                let modes = [
                    (tm_state::MatchMode::AllTerms, "모두 포함"),
                    (tm_state::MatchMode::AnyTerm, "하나 이상"),
                    (tm_state::MatchMode::Exact, "정확히 일치"),
                ];
                let mut mx = pad + side_w + scale(h, 2);
                for (mode, label) in modes {
                    let rc = RECT {
                        left: mx,
                        top: second_y,
                        right: mx + scale(h, 82),
                        bottom: second_y + scale(h, 22),
                    };
                    fill(
                        rc,
                        if u.state.options.match_mode == mode {
                            p.accent
                        } else {
                            p.tab_hover
                        },
                    );
                    text(
                        label,
                        rc,
                        if u.state.options.match_mode == mode {
                            p.bg
                        } else {
                            p.tab_text
                        },
                        DT_CENTER,
                    );
                    mx += scale(h, 86);
                }
            }

            let sy = bands.summary.top;
            let sh = bands.summary.bottom - bands.summary.top;
            fill(
                RECT {
                    left: side_w,
                    top: sy,
                    right: r.right,
                    bottom: sy + sh,
                },
                p.tab_bg,
            );
            let elapsed = u.timing.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0);
            let display_ids = ktm::layout::settings_visible_indices(
                &u.rows,
                &u.settings,
                u.state.selected_folder.as_deref(),
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
            // 파일명·본문 매치 수는 오른쪽 접기 단추(group_left~)에 따로 그리므로 여기에 또 쓰면 겹친다.
            let summary = format!("{view}  {}개 · {:.2}초", display_ids.len(), elapsed);
            text(
                &summary,
                RECT {
                    left: lpad,
                    top: sy,
                    right: lpad + scale(h, 230),
                    bottom: sy + sh,
                },
                p.text_sub,
                DT_END_ELLIPSIS,
            );
            let group_left = lpad + scale(h, 235);
            let group_right = r.right - scale(h, 132);
            let group_mid = group_left + (group_right - group_left) / 2;
            let fit_label = |full: String, short: String, avail: i32| -> String {
                ktm::layout::pick_fitting_label(&[full, short], avail, |t| {
                    let w = wide(t);
                    let mut sz: SIZE = std::mem::zeroed();
                    GetTextExtentPoint32W(mem, w.as_ptr(), (w.len() - 1) as i32, &mut sz);
                    sz.cx
                })
            };
            let arrow_f = if u.state.options.collapsed_filename { " ▸" } else { " ▾" };
            let arrow_b = if u.state.options.collapsed_body { " ▸" } else { " ▾" };
            text(
                &fit_label(
                    format!("파일명 매치 {}{}", groups.filename, arrow_f),
                    format!("이름 {}{}", groups.filename, arrow_f),
                    group_mid - group_left,
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
                &fit_label(
                    format!("본문 매치 {}{}", groups.body, arrow_b),
                    format!("본문 {}{}", groups.body, arrow_b),
                    group_right - group_mid,
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
            let fy = bands.filters.top;
            let fh = bands.filters.bottom - bands.filters.top;
            SelectObject(mem, u.small);
            text(
                "기간:",
                RECT {
                    left: lpad,
                    top: fy,
                    right: lpad + scale(h, 32),
                    bottom: fy + fh,
                },
                p.text_sub,
                0,
            );
            let mut fx = lpad + scale(h, 34);
            let date_chips = [
                (DateFilter::All, "전체", 36),
                (DateFilter::Today, "오늘", 36),
                (DateFilter::Last7Days, "7일간", 46),
                (DateFilter::Last30Days, "30일간", 52),
                (DateFilter::ThisYear, "올해", 36),
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
                let ay = bands.autocomplete.top;
                let ah = bands.autocomplete.bottom - bands.autocomplete.top;
                let mut ax = lpad;
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
                // 요약 줄(filters 아래)과 겹치지 않도록 결과 목록 시작선 아래에서 그린다.
                let ty = bands.list_top + scale(h, 8);
                let entries = u.state.history_entries();
                if !entries.is_empty() {
                    SelectObject(mem, u.small_bold);
                    text(
                        "최근 및 즐겨찾기 검색  (↑↓ 선택 · Enter 불러오기 · Ctrl+D 즐겨찾기 토글)",
                        RECT {
                            left: lpad,
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
                            left: lpad,
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
                            left: lpad + pad_x,
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
                            left: lpad,
                            top: ty,
                            right: r.right - pad,
                            bottom: ty + scale(h, 20),
                        },
                        p.text_sub,
                        0,
                    );
                    let btn_y = ty + scale(h, 24);
                    let btn_h = scale(h, 32);
                    let col_w = (r.right - side_w - 2 * pad - scale(h, 16)) / 2;
                    SelectObject(mem, u.font);
                    for (i, topic) in u.state.top_topics.iter().take(10).enumerate() {
                        let col = (i % 2) as i32;
                        let row = (i / 2) as i32;
                        let left = lpad + col * (col_w + scale(h, 16));
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

            // ⑥a 오류: 상태줄 한 줄로만 보이면 놓치므로 목록 자리에 안내를 크게 보인다.
            if u.state.phase == Phase::Error {
                if let Some(message) = &u.state.error {
                    let ey = bands.list_top + scale(h, 12);
                    SelectObject(mem, u.title_font);
                    text(
                        "검색을 완료하지 못했습니다",
                        RECT { left: lpad, top: ey, right: r.right - pad, bottom: ey + scale(h, 26) },
                        p.text,
                        0,
                    );
                    SelectObject(mem, u.font);
                    SetTextColor(mem, p.text_sub);
                    let ws = wide(message);
                    let mut rc = RECT { left: lpad, top: ey + scale(h, 30), right: r.right - pad, bottom: ey + scale(h, 110) };
                    DrawTextW(mem, ws.as_ptr(), (ws.len() - 1) as i32, &mut rc, DT_WORDBREAK | DT_NOPREFIX);
                }
            }

            // ⑥b 결과가 없거나 검색 중일 때 빈 화면 대신 안내를 보인다.
            if u.rows.is_empty() && !u.state.query.trim().is_empty() {
                let note = match u.state.phase {
                    // 오류는 위 ⑥a가 이미 제목·사유를 그린다(겹쳐 그리면 글자가 뭉개진다).
                    Phase::Results => Some((
                        "검색 결과가 없습니다",
                        "철자·띄어쓰기를 바꾸거나, 기간·형식 조건을 '전체'로 바꿔 보세요.",
                    )),
                    Phase::Searching => Some(("검색 중…", "")),
                    _ => None,
                };
                if let Some((title, detail)) = note {
                    let ey = bands.list_top + scale(h, 12);
                    SelectObject(mem, u.title_font);
                    text(title, RECT { left: lpad, top: ey, right: r.right - pad, bottom: ey + scale(h, 26) }, p.text, 0);
                    SelectObject(mem, u.font);
                    text(detail, RECT { left: lpad, top: ey + scale(h, 30), right: r.right - pad, bottom: ey + scale(h, 54) }, p.text_sub, DT_END_ELLIPSIS);
                }
            }

            // ⑦ 결과 목록 (클립보드 popup.rs 행 스타일)
            let top_y = list_top_coord(h, u);
            let rh = row_height(h);
            let status_h = scale(h, STATUS_H);
            let preview_h = if u.state.selected.is_some() && !u.rows.is_empty() {
                scale(h, PREVIEW_H)
            } else {
                0
            };
            let visible = ((r.bottom - top_y - status_h - preview_h).max(0) / rh).max(0) as usize;
            let visible_indices = display_ids;

            let line_h1 = scale(h, 20);
            let line_h2 = scale(h, 24);
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
                // 바둑판(grid) 카드에는 왼쪽 번호·종류 칸이 없다(예전엔 카드 위에 겹쳐 엉뚱한 높이에 그려졌다).
                let num_str = if grid {
                    String::new()
                } else if n - u.top < 9 {
                    format!("{}", n - u.top + 1)
                } else {
                    String::new()
                };
                SelectObject(mem, u.small);
                text(
                    &num_str,
                    RECT {
                        left: side_w + scale(h, 8),
                        top: y + scale(h, 2),
                        right: side_w + scale(h, 24),
                        bottom: y + line_h1,
                    },
                    p.row_text(sel, p.number),
                    DT_CENTER,
                );

                // 종류 태그/아이콘
                let tag = if grid { "" } else { extension_tag(&row.name) };
                SelectObject(mem, u.small_bold);
                text(
                    tag,
                    RECT {
                        left: side_w + scale(h, 28),
                        top: y + scale(h, 2),
                        right: side_w + scale(h, 66),
                        bottom: y + line_h1,
                    },
                    p.row_text(sel, p.text_faint),
                    0,
                );

                // 첫 줄: 파일명 (굵게)
                SelectObject(mem, u.font_bold);
                let text_right =
                    (if grid { row_rc.right } else { r.right }) - folder_w - scale(h, 8);
                let name_left = if grid {
                    row_rc.left + scale(h, 8)
                } else {
                    side_w + scale(h, 68)
                };
                let badge_w = scale(h, 48);
                let reason_label = if u.state.tab == Tab::All {
                    let labels = row.match_kinds.iter().map(|kind| match kind {
                        tm_state::ResultKind::Names => "이름",
                        tm_state::ResultKind::Hierarchy => "위계",
                        tm_state::ResultKind::Body => "본문",
                    }).collect::<Vec<_>>().join("·");
                    labels
                } else { String::new() };
                let reason_w = if reason_label.is_empty() { 0 } else { scale(h, 66) };
                // 바둑판 카드에서도 [폴더 열기] 단추 왼쪽에서 끝내 확장자 꼬리표와 겹치지 않게 한다.
                let badge_right = text_right;
                let ext = ktm::backend::extension_label(&row.name);
                // 배지 사각형은 글자 높이 기준으로 첫 줄 한가운데에 맞춘다(고배율에서 글자가 아래로 치우치지 않게).
                let mut badge_ts: SIZE = std::mem::zeroed();
                let probe = wide("가");
                GetTextExtentPoint32W(mem, probe.as_ptr(), 1, &mut badge_ts);
                let badge_h = (badge_ts.cy + scale(h, 4)).min(line_h1);
                let badge_top = gy + scale(h, 1) + (line_h1 - scale(h, 1) - badge_h) / 2;
                let ext_rc = RECT {
                    left: badge_right - badge_w,
                    top: badge_top,
                    right: badge_right,
                    bottom: badge_top + badge_h,
                };
                fill(ext_rc, p.tab_hover);
                text(ext, ext_rc, p.text_sub, DT_CENTER);
                if !reason_label.is_empty() {
                    let reason_rc = RECT { left: badge_right - badge_w - reason_w - scale(h, 4), top: badge_top, right: badge_right - badge_w - scale(h, 4), bottom: badge_top + badge_h };
                    fill(reason_rc, p.selected);
                    text(&reason_label, reason_rc, p.accent, DT_CENTER);
                }
                // 첫 줄: 파일명 오른쪽에 흐린 폴더 경로를 이어 붙인다(둘째 줄은 발췌 문장 전용이라
                // 예전처럼 경로와 발췌가 같은 줄에 겹쳐 그려지지 않는다).
                let name_limit = badge_right - badge_w - reason_w - scale(h, 4);
                let name_wide = wide(&row.name);
                let mut name_size: SIZE = std::mem::zeroed();
                GetTextExtentPoint32W(mem, name_wide.as_ptr(), (name_wide.len() - 1) as i32, &mut name_size);
                let has_second_line_text = !row.snippet.is_empty() || !row.location.is_empty();
                let path_text = if u.settings.show_location { row.parent.as_str() } else { "" };
                let (name_right, path_left) = result_first_line_split(
                    name_left,
                    name_limit,
                    name_size.cx + scale(h, 4),
                    scale(h, 70),
                    has_second_line_text && !path_text.is_empty(),
                );
                highlighted(
                    mem,
                    &row.name,
                    &u.terms,
                    RECT {
                        left: name_left,
                        top: gy + scale(h, 2),
                        right: name_right,
                        bottom: gy + line_h1,
                    },
                    p.row_text(sel, p.text),
                    0x8AF0FE,
                );
                if let Some(path_left) = path_left {
                    SelectObject(mem, u.small);
                    text(
                        path_text,
                        RECT {
                            left: path_left,
                            top: gy + scale(h, 2),
                            right: name_limit - scale(h, 8),
                            bottom: gy + line_h1,
                        },
                        p.row_text(sel, p.text_faint),
                        DT_PATH_ELLIPSIS,
                    );
                }

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
                let (show_location, show_date) =
                    (u.settings.show_location, u.settings.show_modified);
                let location = if show_location {
                    row.parent.as_str()
                } else {
                    ""
                };
                let meta = match (location.is_empty(), date.is_empty() || !show_date) {
                    (true, true) => String::new(),
                    (false, true) => location.to_string(),
                    (true, false) => date,
                    (false, false) => format!("{location}  |  {date}"),
                };
                // 발췌·위치 문장이 있으면 둘째 줄은 그쪽이 쓴다(경로는 첫 줄 오른쪽에 있음).
                let second_line_taken = !row.snippet.is_empty() || !row.location.is_empty();
                if !second_line_taken {
                text(
                    &meta,
                    RECT {
                        left: if grid {
                            row_rc.left + scale(h, 8)
                        } else {
                            side_w + scale(h, 68)
                        },
                        top: gy + line_h1,
                        right: if grid {
                            row_rc.right - scale(h, 8)
                        } else {
                            text_right
                        },
                        bottom: gy + rh,
                    },
                    p.row_text(sel, p.text_faint),
                    DT_PATH_ELLIPSIS,
                );
                }

                // 둘째 줄: 발췌 문장 (강조 하이라이트) 및 일치 문장 옆 위치 배지
                let snippet: &str = if row.snippet.is_empty() {
                    &row.location
                } else {
                    &row.snippet
                };
                SelectObject(mem, u.small);

                let mut snip_left = side_w + scale(h, 68);
                if grid {
                    snip_left = row_rc.left + scale(h, 8);
                }
                let line_top = gy + line_h1;
                let line_bottom = line_top + line_h2;

                let group_terms = query_group_terms(&u.state.query);
                if !row.snippet.is_empty() {
                    for (gi, terms) in group_terms.iter().enumerate() {
                        if ktm::highlight::ranges_terms(snippet, terms).is_empty() { continue; }
                        let label = format!("{}", gi + 1);
                        let (bg, fg) = p.group_highlight(gi, ui_theme::theme().kind == ui_theme::Kind::Dark);
                        let bw = scale(h, 18);
                        let badge = RECT { left: snip_left, top: line_top + scale(h, 1), right: snip_left + bw, bottom: line_bottom - scale(h, 1) };
                        fill(badge, bg);
                        text(&label, badge, fg, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                        snip_left += bw + scale(h, 4);
                    }
                }

                // ① 일치 문장 옆 위치 배지 (쪽·시트·행 등 kfind 위치 메타가 있는 것만, 없으면 생략)
                if u.settings.show_location {
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
                        let badge_top = line_top + (line_h2 - badge_h) / 2;
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
                }

                highlighted_groups(
                    mem,
                    snippet,
                    &group_terms,
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
                    p,
                );
            }

            // ② 검색창 아래 연관 검색어 묶음 칩
            let related_groups = kfind_core::split_semicolon_groups(&u.state.query);
            let chip_top = quick_layout(h, u).related.top;
            let chip_labels: Vec<String> = related_groups.iter().enumerate().take(8).map(|(i,g)| format!("{}. {}", i+1, g.trim())).collect();
            let chip_rects = related_chip_rects(&chip_labels, pad + side_w, chip_top, r.right - pad, scale(h, 100));
            SelectObject(mem, u.small);
            for (i, (group, chip)) in chip_labels.iter().zip(chip_rects.iter()).enumerate() {
                let (bg, fg) = p.group_highlight(i, ui_theme::theme().kind == ui_theme::Kind::Dark);
                fill(*chip, bg);
                text(group, *chip, fg, DT_CENTER | DT_END_ELLIPSIS | DT_SINGLELINE);
            }

            // ③ 결과 선택 시 미리보기 줄 (선택 행의 일치 문단 전체 3줄)
            if let Some(sel_idx) = u.state.selected {
                if let Some(sel_r) = u.rows.get(sel_idx) {
                    let prev_y = r.bottom - status_h - preview_h;
                    let prev_rc = RECT {
                        left: side_w,
                        top: prev_y,
                        right: r.right,
                        bottom: r.bottom - status_h,
                    };
                    fill(prev_rc, p.tab_bg);
                    fill(
                        RECT {
                            left: side_w,
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

                    let no_preview = preview_text.is_empty();
                    let preview_text: &str = if no_preview {
                        "본문 발췌가 없습니다"
                    } else {
                        preview_text
                    };
                    let pad_l = side_w + scale(h, 12);
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
                        if no_preview { p.text_faint } else { p.text },
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
                    Phase::Searching => ("검색 중…".into(), String::new()),
                    Phase::Results => (
                        format!("문서 {}건", u.rows.len()),
                        format!("폴더 {}개", u.state.folder_counts.len()),
                    ),
                    _ if u.state.query.trim().is_empty()
                        && !u.state.history_entries().is_empty() =>
                    {
                        (
                            QUICK_SEARCH_EMPTY_HINT.into(),
                            String::new(),
                        )
                    }
                    _ if u.state.query.trim().is_empty() => {
                        ("주제나 검색어를 입력하세요".into(), String::new())
                    }
                    _ => (
                        "2글자부터 바로 검색 · 1글자는 Enter".into(),
                        String::new(),
                    ),
                }
            };
            SelectObject(mem, u.small);
            text(
                &status_left,
                RECT {
                    left: pad,
                    top: status_y,
                    // 오른쪽 글자가 없으면 전체 폭을 쓴다(오류 문구가 반쯤에서 잘렸다).
                    right: if status_right.is_empty() { r.right - pad } else { r.right / 2 },
                    bottom: r.bottom,
                },
                p.text_sub,
                DT_END_ELLIPSIS,
            );
            text(
                &status_right,
                RECT {
                    left: r.right / 2,
                    top: status_y,
                    right: r.right - pad,
                    bottom: r.bottom,
                },
                p.text_sub,
                DT_RIGHT | DT_END_ELLIPSIS,
            );
        });

        // 틀 없는 창 테두리 1px
        frame::own_border(mem, r);

        if let Some(kind) = with(|u| u.modal) {
            // 모달 뒤 화면을 어둡게 한다. `fill`은 알파를 무시해 검은 면이 되므로 반투명 합성을 쓴다.
            {
                let dim_dc = CreateCompatibleDC(mem);
                let dim_bmp = CreateCompatibleBitmap(mem, 1, 1);
                let old_bmp = SelectObject(dim_dc, dim_bmp);
                let black = CreateSolidBrush(0);
                FillRect(dim_dc, &RECT { left: 0, top: 0, right: 1, bottom: 1 }, black);
                DeleteObject(black);
                let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 112, AlphaFormat: 0 };
                AlphaBlend(mem, r.left, r.top, r.right - r.left, r.bottom - r.top, dim_dc, 0, 0, 1, 1, blend);
                SelectObject(dim_dc, old_bmp);
                DeleteObject(dim_bmp);
                DeleteDC(dim_dc);
            }
            let count = if kind == ktm::layout::ModalKind::Statistics {
                2
            } else {
                4
            };
            let mr = modal_rects(r.right, r.bottom, count);
            fill(mr.panel, p.bg);
            let frame_brush = CreateSolidBrush(p.divider);
            FrameRect(mem, &mr.panel, frame_brush);
            DeleteObject(frame_brush);
            SelectObject(mem, with(|u| u.title_font));
            let title = if kind == ktm::layout::ModalKind::Statistics {
                "문서 통계"
            } else {
                "설정"
            };
            text(
                title,
                RECT {
                    left: mr.panel.left + 16,
                    top: mr.panel.top,
                    right: mr.close.left,
                    bottom: mr.panel.top + 48,
                },
                p.text,
                0,
            );
            let close_rc = mr.close;
            fill(close_rc, p.tab_hover);
            SelectObject(mem, with(|u| u.glyph));
            text("✕", close_rc, p.text, DT_CENTER);
            let labels: [&str; 4] = if kind == ktm::layout::ModalKind::Statistics {
                ["문서 통계", "검색 히스토리", "", ""]
            } else {
                ["일반", "검색", "시스템", "진단"]
            };
            let active = with(|u| u.modal_tab);
            for i in 0..count {
                let rc = mr.tabs[i];
                fill(rc, if i == active { p.bg } else { p.tab_bg });
                SelectObject(
                    mem,
                    with(|u| if i == active { u.small_bold } else { u.small }),
                );
                text(
                    labels[i],
                    rc,
                    if i == active { p.accent } else { p.text_sub },
                    DT_CENTER,
                );
            }
            SelectObject(mem, with(|u| u.small));
            if kind == ktm::layout::ModalKind::Statistics {
                if active == 0 {
                    let rows = with(|u| u.rows.clone());
                    let s = ktm::backend::document_stats(&rows);
                    let line = scale(h, 26);
                    let left = mr.content.left;
                    let right = mr.content.right;
                    let summary = format!(
                        "현재 결과 {}건    색인된 결과 {}건    전체 인덱스 요약은 미제공",
                        s.total, s.indexed
                    );
                    text(
                        &summary,
                        RECT {
                            left,
                            top: mr.content.top,
                            right,
                            bottom: mr.content.top + line,
                        },
                        p.text,
                        DT_END_ELLIPSIS,
                    );
                    let mut y = mr.content.top + line + scale(h, 8);
                    // 아래 안내 문구 줄(content.bottom 쪽 28px)과 패널 밖으로 넘치지 않도록 그릴 한계선을 둔다.
                    let limit = mr.content.bottom - scale(h, 30);
                    text(
                        "파일 유형",
                        RECT {
                            left,
                            top: y,
                            right,
                            bottom: y + line,
                        },
                        p.text,
                        0,
                    );
                    y += line;
                    for (label, count) in s.by_type.iter().take(6) {
                        if y + line > limit {
                            break;
                        }
                        let ratio = if s.total == 0 {
                            0.0
                        } else {
                            *count as f64 * 100.0 / s.total as f64
                        };
                        let bar_w = ((right - left - scale(h, 150)) as f64 * ratio / 100.0) as i32;
                        fill(
                            RECT {
                                left: left + scale(h, 142),
                                top: y + scale(h, 8),
                                right: left + scale(h, 142) + bar_w,
                                bottom: y + scale(h, 18),
                            },
                            p.accent,
                        );
                        text(
                            &format!("{label}  {count}  ({ratio:.0}%)"),
                            RECT {
                                left,
                                top: y,
                                right: left + scale(h, 138),
                                bottom: y + line,
                            },
                            p.text_sub,
                            DT_END_ELLIPSIS,
                        );
                        y += line;
                    }
                    y += scale(h, 4);
                    if y + 2 * line <= limit {
                    text(
                        "연도별 문서",
                        RECT {
                            left,
                            top: y,
                            right,
                            bottom: y + line,
                        },
                        p.text,
                        0,
                    );
                    }
                    y += line;
                    for (year, count) in s.by_year.iter().take(5) {
                        if y + line > limit {
                            break;
                        }
                        text(
                            &format!("{year}  {count}건"),
                            RECT {
                                left,
                                top: y,
                                right,
                                bottom: y + line,
                            },
                            p.text_sub,
                            0,
                        );
                        y += line;
                    }
                    y += scale(h, 4);
                    if y + 2 * line <= limit {
                    text(
                        "최근 수정 상위 문서",
                        RECT {
                            left,
                            top: y,
                            right,
                            bottom: y + line,
                        },
                        p.text,
                        0,
                    );
                    }
                    y += line;
                    for row in s.recent.iter().take(5) {
                        if y + line > limit {
                            break;
                        }
                        text(
                            &format!("{}  {}", row.name, row.date),
                            RECT {
                                left,
                                top: y,
                                right,
                                bottom: y + line,
                            },
                            p.text_sub,
                            DT_END_ELLIPSIS,
                        );
                        y += line;
                    }
                    text(
                        "전체 폴더별 수·크기 및 최근/최대 문서는 인덱스 메타데이터에 없습니다.",
                        RECT {
                            left,
                            top: mr.content.bottom - scale(h, 28),
                            right,
                            bottom: mr.content.bottom,
                        },
                        p.text_faint,
                        DT_END_ELLIPSIS,
                    );
                } else {
                    let (queries, times) =
                        with(|u| (u.state.recent_queries.clone(), u.recent_query_times.clone()));
                    let left = mr.content.left;
                    let right = mr.content.right;
                    let line = scale(h, 28);
                    text(
                        &format!(
                            "총 검색 {}회    고유 검색어 {}개",
                            queries.len(),
                            queries
                                .iter()
                                .collect::<std::collections::HashSet<_>>()
                                .len()
                        ),
                        RECT {
                            left,
                            top: mr.content.top,
                            right,
                            bottom: mr.content.top + line,
                        },
                        p.text,
                        0,
                    );
                    for (i, q) in queries.iter().enumerate().take(20) {
                        let ts = times.get(i).copied().unwrap_or(0);
                        let when = if ts == 0 {
                            String::new()
                        } else {
                            ktm::settings::relative_time_label(
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                                ts,
                            )
                        };
                        text(
                            &format!("{q}    {when}"),
                            RECT {
                                left,
                                top: mr.content.top + line * (i as i32 + 1),
                                right,
                                bottom: mr.content.top + line * (i as i32 + 2),
                            },
                            p.text_sub,
                            DT_END_ELLIPSIS,
                        );
                    }
                }
            } else {
                let (s, excluded_directories) =
                    with(|u| (u.settings.clone(), u.settings.excluded_directories.clone()));
                let labels: &[&str] = match active {
                    0 => &[
                        "테마",
                        "기본 검색 모드",
                        "최대 검색 결과",
                        "UI 크기",
                        "결과 밀도",
                        "파일 열기 방식",
                    ],
                    1 => &[
                        "저장 위치 표시",
                        "날짜 표시",
                        "하위 폴더 포함",
                        "제외 디렉터리 추가",
                        "문서 버전 그룹핑",
                    ],
                    2 => &[
                        "Windows 로그인 자동 실행",
                        "트레이 최소화",
                        "시작 시 숨김",
                        "최대 파일 크기 MB",
                        "클라우드/네트워크 본문 스킵",
                        "시스템 폴더 허용",
                    ],
                    _ => &["설정과 인덱스 상태", "오류 로그 폴더 열기"],
                };
                let vals: Vec<String> = match active {
                    0 => vec![
                        match s.ui_theme.as_str() {
                            "light" => "밝게".to_string(),
                            "dark" => "어둡게".to_string(),
                            _ => "시스템".to_string(),
                        },
                        if s.default_search_mode == "filename" {
                            "파일명".to_string()
                        } else {
                            "키워드".to_string()
                        },
                        format!("{}건", s.bounded_max_results()),
                        format!("{}%", s.bounded_ui_scale()),
                        if s.result_density == "compact" {
                            "컴팩트".to_string()
                        } else {
                            "넓게".to_string()
                        },
                        if s.open_clicks == 1 {
                            "한 번 클릭".to_string()
                        } else {
                            "두 번 클릭".to_string()
                        },
                    ],
                    1 => vec![
                        if s.show_location {
                            "켬".to_string()
                        } else {
                            "끔".to_string()
                        },
                        if s.show_modified {
                            "켬".to_string()
                        } else {
                            "끔".to_string()
                        },
                        if s.include_subfolders {
                            "켬".to_string()
                        } else {
                            "끔".to_string()
                        },
                        if s.excluded_directories.is_empty() {
                            "클릭하여 폴더 추가".into()
                        } else {
                            format!("{}개 폴더 · 클릭하여 추가", s.excluded_directories.len())
                        },
                        if s.version_grouping {
                            "켬".to_string()
                        } else {
                            "끔".to_string()
                        },
                    ],
                    2 => vec![
                        if s.autostart {
                            "켬".to_string()
                        } else {
                            "끔".to_string()
                        },
                        if s.minimize_to_tray {
                            "켬".to_string()
                        } else {
                            "끔".to_string()
                        },
                        if s.start_hidden {
                            "켬".to_string()
                        } else {
                            "끔".to_string()
                        },
                        s.max_file_size_mb.to_string(),
                        if s.skip_cloud_network {
                            "켬".to_string()
                        } else {
                            "끔".to_string()
                        },
                        if s.allow_system_folders {
                            "켬".to_string()
                        } else {
                            "끔".to_string()
                        },
                    ],
                    _ => vec![
                        format!(
                            "인덱스 준비: {} · 최근 검색 {}",
                            with(|u| u.warmed),
                            s.recent_queries.len()
                        ),
                        "클릭하여 로그 폴더 열기".into(),
                    ],
                };
                let line = scale(h, 38);
                for (i, label) in labels.iter().enumerate() {
                    let y = mr.content.top + line * i as i32;
                    text(
                        label,
                        RECT {
                            left: mr.content.left,
                            top: y,
                            right: mr.content.left + (mr.content.right - mr.content.left) / 2,
                            bottom: y + line,
                        },
                        p.text,
                        0,
                    );
                    text(
                        vals.get(i).map(String::as_str).unwrap_or(""),
                        RECT {
                            left: mr.content.left + (mr.content.right - mr.content.left) / 2,
                            top: y,
                            right: mr.content.right,
                            bottom: y + line,
                        },
                        p.accent,
                        DT_END_ELLIPSIS,
                    );
                }
                if active == 1 {
                    let list_top = mr.content.top + line * 4;
                    let list_line = scale(h, 26).max(1);
                    for (i, path) in excluded_directories.iter().enumerate() {
                        let y = list_top + i as i32 * list_line;
                        if y + list_line > mr.content.bottom {
                            break;
                        }
                        text(
                            path,
                            RECT {
                                left: mr.content.left,
                                top: y,
                                right: mr.content.right * 2 / 3,
                                bottom: y + list_line,
                            },
                            p.text_sub,
                            DT_END_ELLIPSIS,
                        );
                        text(
                            "[Delete]",
                            RECT {
                                left: mr.content.right * 2 / 3,
                                top: y,
                                right: mr.content.right,
                                bottom: y + list_line,
                            },
                            p.accent,
                            DT_RIGHT,
                        );
                    }
                }
            }
        }

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
                // WS_THICKFRAME의 기본 계산이 남긴 우측 비클라이언트 띠가 자체 그리기와 겹쳤다.
                0
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
                // 캡션 단추뿐 아니라 [통계]·[설정] 칩도 클릭이 클라이언트로 와야 한다
                // (예전엔 칩이 HTCAPTION이라 눌러도 창 끌기로만 처리되어 작동하지 않았다).
                let on_btn = ktm::layout::header_is_client_button(p.x, p.y, r.right, bw, head);
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
                if with(|u| u.modal.is_some()) {
                    return 0;
                }
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(h, &mut r);
                let head = scale(h, HEAD_H);
                let bw = scale(h, CAPTION_W);
                let folder_w = scale(h, ROW_FOLDER_W);
                let new_head_btn = head_button_hit(x, y, r.right, bw, head);

                let top_y = with(|u| list_top_coord(h, u));
                let rh = row_height(h);
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
                        || u.hover_row != new_row
                        || u.hover_folder_btn != new_folder_btn;
                    u.hover_head_btn = new_head_btn;
                    u.hover_tab = None;
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
                if let Some(kind) = with(|u| u.modal) {
                    let count = if kind == ktm::layout::ModalKind::Statistics {
                        2
                    } else {
                        4
                    };
                    let hit = hit_modal(x, y, rc_w.right, rc_w.bottom, count);
                    match hit {
                        ModalHit::Outside | ModalHit::Close => {
                            edit(|u| u.modal = None);
                            layout(h);
                            SetFocus(with(|u| u.edit));
                        }
                        ModalHit::Tab(i) => edit(|u| u.modal_tab = i),
                        ModalHit::Content
                            if kind == ktm::layout::ModalKind::Statistics
                                && with(|u| u.modal_tab == 1) =>
                        {
                            let top = modal_rects(rc_w.right, rc_w.bottom, count).content.top;
                            let line = scale(h, 28);
                            let idx = ((y - top) / line - 1).max(0) as usize;
                            if let Some(q) = with(|u| u.state.recent_queries.get(idx).cloned()) {
                                edit(|u| u.modal = None);
                                layout(h);
                                event(h, Event::Input(q));
                                event(h, Event::Submit);
                            }
                        }
                        ModalHit::Content if kind == ktm::layout::ModalKind::Settings => {
                            let top = modal_rects(rc_w.right, rc_w.bottom, count).content.top;
                            let idx = ((y - top) / scale(h, 38)).max(0) as usize;
                            let current_tab = with(|u| u.modal_tab);
                            let previous_ui_scale = with(|u| u.pending_ui_scale);
                            if current_tab == 1 {
                                let excluded = with(|u| u.settings.excluded_directories.clone());
                                let list_top = top + scale(h, 38) * 4;
                                let list_line = scale(h, 26).max(1);
                                let row = (y - list_top) / list_line;
                                if row >= 0
                                    && (row as usize) < excluded.len()
                                    && x >= rc_w.right * 2 / 3
                                {
                                    let removed = excluded[row as usize].clone();
                                    edit(|u| {
                                        ktm::layout::remove_excluded_directory(
                                            &mut u.settings.excluded_directories,
                                            &removed,
                                        );
                                    });
                                    let (settings, path) =
                                        with(|u| (u.settings.clone(), u.settings_path.clone()));
                                    let _ = settings.save(&path);
                                    let roots = with(|u| {
                                        u.rows
                                            .iter()
                                            .map(|r| r.parent.clone())
                                            .filter(|p| !p.is_empty())
                                            .collect::<Vec<_>>()
                                    });
                                    let selected = with(|u| u.state.selected_folder.clone());
                                    let settings = with(|u| u.settings.clone());
                                    edit(|u| {
                                        let keep = ktm::layout::filter_row_indices_with_roots(
                                            &u.rows,
                                            &settings,
                                            selected.as_deref(),
                                            &roots,
                                        );
                                        let allowed: std::collections::HashSet<usize> =
                                            keep.into_iter().collect();
                                        u.rows = std::mem::take(&mut u.rows)
                                            .into_iter()
                                            .enumerate()
                                            .filter_map(|(i, row)| {
                                                allowed.contains(&i).then_some(row)
                                            })
                                            .collect();
                                        u.state.result_count = u.rows.len();
                                        u.state.selected = None;
                                    });
                                    InvalidateRect(h, null(), 0);
                                    return 0;
                                }
                            }
                            if current_tab == 3 && idx == 1 {
                                let log_dir = std::env::var_os("LOCALAPPDATA")
                                    .map(PathBuf::from)
                                    .unwrap_or_else(std::env::temp_dir)
                                    .join("HwpAutoDocFit")
                                    .join("logs");
                                let _ = std::fs::create_dir_all(&log_dir);
                                let _ = ktm::open::open(log_dir.to_string_lossy().as_ref(), false);
                            }
                            if current_tab == 1 && idx == 3 {
                                let excluded = with(|u| u.settings.excluded_directories.clone());
                                let list_top = top + scale(h, 38) * 4;
                                let list_line = scale(h, 26).max(1);
                                let remove_index = if excluded.is_empty() {
                                    None
                                } else {
                                    let n = ((y - list_top) / list_line) as usize;
                                    let left = rc_w.right * 2 / 3;
                                    (n < excluded.len() && x >= left).then_some(n)
                                };
                                if let Some(index) = remove_index {
                                    let removed = excluded[index].clone();
                                    edit(|u| {
                                        ktm::layout::remove_excluded_directory(
                                            &mut u.settings.excluded_directories,
                                            &removed,
                                        );
                                    });
                                    let (s, path) =
                                        with(|u| (u.settings.clone(), u.settings_path.clone()));
                                    let _ = s.save(&path);
                                    let roots = with(|u| {
                                        u.rows
                                            .iter()
                                            .map(|r| r.parent.clone())
                                            .filter(|p| !p.is_empty())
                                            .collect::<Vec<_>>()
                                    });
                                    let (rows, selected) =
                                        with(|u| (u.rows.clone(), u.state.selected_folder.clone()));
                                    let settings = with(|u| u.settings.clone());
                                    let visible = ktm::layout::filter_row_indices_with_roots(
                                        &rows,
                                        &settings,
                                        selected.as_deref(),
                                        &roots,
                                    );
                                    edit(|u| {
                                        let visible_set: std::collections::HashSet<usize> =
                                            visible.iter().copied().collect();
                                        u.rows = rows
                                            .into_iter()
                                            .enumerate()
                                            .filter_map(|(i, row)| {
                                                visible_set.contains(&i).then_some(row)
                                            })
                                            .collect();
                                        u.state.result_count = u.rows.len();
                                        u.state.selected = None;
                                    });
                                    InvalidateRect(h, null(), 0);
                                    return 0;
                                }
                                if let Some(path) = pick_folder(h) {
                                    edit(|u| {
                                        if !u
                                            .settings
                                            .excluded_directories
                                            .iter()
                                            .any(|p| p.eq_ignore_ascii_case(&path))
                                        {
                                            u.settings.excluded_directories.push(path);
                                        }
                                    });
                                    let (s, path) =
                                        with(|u| (u.settings.clone(), u.settings_path.clone()));
                                    let _ = s.save(&path);
                                }
                                InvalidateRect(h, null(), 0);
                                return 0;
                            }
                            let settings_tab = with(|u| u.modal_tab);
                            edit(|u| {
                                let s = &mut u.settings;
                                match (u.modal_tab, idx) {
                                    (0, 0) => {
                                        s.ui_theme = match s.ui_theme.as_str() {
                                            "system" => "light",
                                            "light" => "dark",
                                            _ => "system",
                                        }
                                        .into()
                                    }
                                    (0, 1) => {
                                        s.default_search_mode =
                                            if s.default_search_mode == "keyword" {
                                                "filename"
                                            } else {
                                                "keyword"
                                            }
                                            .into()
                                    }
                                    (0, 2) => {
                                        s.max_results = match s.bounded_max_results() {
                                            50 => 100,
                                            100 => 200,
                                            _ => 50,
                                        }
                                    }
                                    (0, 3) => {
                                        s.ui_scale = match s.bounded_ui_scale() {
                                            90 => 100,
                                            100 => 110,
                                            110 => 125,
                                            _ => 90,
                                        }
                                    }
                                    (0, 4) => {
                                        s.result_density = if s.result_density == "compact" {
                                            "comfortable"
                                        } else {
                                            "compact"
                                        }
                                        .into()
                                    }
                                    (0, 5) => {
                                        s.open_clicks = if s.open_clicks == 1 { 2 } else { 1 }
                                    }
                                    (1, 0) => s.show_location = !s.show_location,
                                    (1, 1) => s.show_modified = !s.show_modified,
                                    (1, 2) => s.include_subfolders = !s.include_subfolders,
                                    (1, 4) => s.version_grouping = !s.version_grouping,
                                    (2, 0) => s.autostart = !s.autostart,
                                    (2, 1) => s.minimize_to_tray = !s.minimize_to_tray,
                                    (2, 2) => s.start_hidden = !s.start_hidden,
                                    (2, 3) => {
                                        s.max_file_size_mb =
                                            if s.max_file_size_mb == 0 { 100 } else { 0 }
                                    }
                                    (2, 4) => s.skip_cloud_network = !s.skip_cloud_network,
                                    (2, 5) => s.allow_system_folders = !s.allow_system_folders,
                                    _ => {}
                                }
                            });
                            let (s, path) = with(|u| (u.settings.clone(), u.settings_path.clone()));
                            let _ = s.save(&path);
                            if settings_tab == 2 && idx == 0 {
                                let exe = std::env::current_exe().unwrap_or_default();
                                let enabled = with(|u| u.settings.autostart);
                                if let Err(error) = ktm::install::set_autostart(&exe, enabled) {
                                    edit(|u| u.status_text = error);
                                }
                            }
                            if settings_tab == 0 && matches!(idx, 3 | 4) {
                                create_fonts(h);
                                if idx == 3 {
                                    let mut client: RECT = std::mem::zeroed();
                                    GetClientRect(h, &mut client);
                                    let new_scale = with(|u| u.settings.bounded_ui_scale());
                                    let (new_width, new_height) = ktm::layout::scaled_window_size(
                                        client.right,
                                        client.bottom,
                                        previous_ui_scale,
                                        new_scale,
                                    );
                                    SetWindowPos(
                                        h,
                                        null_mut(),
                                        0,
                                        0,
                                        new_width,
                                        new_height,
                                        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                                    );
                                    edit(|u| u.pending_ui_scale = new_scale);
                                    layout(h);
                                    InvalidateRect(h, null(), 0);
                                }
                            }
                        }
                        _ => {}
                    }
                    layout(h);
                    InvalidateRect(h, null(), 0);
                    return 0;
                }
                let head = scale(h, HEAD_H);
                let bw = scale(h, CAPTION_W);
                let pad = scale(h, 12);
                if y < head {
                    if let Some(chip) = hit_header_chip(x, y, rc_w.right, bw, head) {
                        edit(|u| {
                            u.modal = Some(if chip == 0 {
                                ktm::layout::ModalKind::Statistics
                            } else {
                                ktm::layout::ModalKind::Settings
                            });
                            u.modal_tab = 0;
                        });
                        layout(h);
                        InvalidateRect(h, null(), 0);
                        return 0;
                    }
                }
                let side_w = { let full = scale(h, SIDEBAR_W); with(|u| ktm::layout::sidebar_width_px(u.state.options.sidebar_open, rc_w.right, full)) };
                let logical_sidebar_x = x * SIDEBAR_W / side_w.max(1);
                let sidebar_top = scale(h, 40);
                let logical_sidebar_y =
                    40 + (y - sidebar_top) * SIDEBAR_ROW_H / scale(h, SIDEBAR_ROW_H).max(1);
                if hit_sidebar_add(logical_sidebar_x, logical_sidebar_y, SIDEBAR_W, side_w > 0) {
                    if let Some(path) = pick_folder(h) {
                        event(h, Event::SetFolderFilter(Some(path)));
                    }
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
                    resize_for_sidebar(h, with(|u| u.state.options.sidebar_open));
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

                let bands = with(|u| quick_layout(h, u));

                if hit_quick_band(x, y, bands.options) {
                    match hit_option(x, y, bands.options.top, scale(h, OPTION_H), pad + side_w, rc_w.right - pad) {
                        Some(OptionControl::MatchMode) => {
                            let modes = [
                                (tm_state::MatchMode::AllTerms, "모두 포함"),
                                (tm_state::MatchMode::AnyTerm, "하나 이상"),
                                (tm_state::MatchMode::Exact, "정확히 일치"),
                            ];
                            let start = pad + side_w + scale(h, 2);
                            let idx = ktm::layout::hit_match_chip(x, start, scale(h, 86), 3)
                                .unwrap_or(0)
                                .min(2);
                            event(h, Event::SetMatchMode(modes[idx].0));
                        }
                        // 직접 일수 입력은 없앴다. 옵션 팝업에는 검색어 결합 방식만 남는다.
                        Some(OptionControl::CustomDays) => {}
                        // 첫 칸('통합 검색')은 안내 글자라 누를 일이 없다.
                        Some(OptionControl::SearchMode) => {}
                        Some(OptionControl::Sort) => cycle_sort(h),
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
                            let labels = [
                                "전체", "오늘", "7일간", "30일간", "올해", "90일", "6개월", "1년",
                            ];
                            for (i, label) in labels.iter().enumerate() {
                                AppendMenuW(menu, MF_STRING, 300 + i, wide(label).as_ptr());
                            }
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
                            // 기간은 검색창 글자가 아닌 내부 상태로만 고른다(칩과 같은 상태).
                            let today = ktm::backend::current_kst_ymd();
                            let end = (today.0 * 10000 + today.1 * 100 + today.2) as i64;
                            match cmd {
                                300 => {
                                    event(h, Event::SetDays(None));
                                    event(h, Event::SetDateFilter(DateFilter::All));
                                }
                                301 => {
                                    event(h, Event::SetDays(Some(0)));
                                    event(h, Event::SetDateFilter(DateFilter::Today));
                                }
                                302 => {
                                    event(h, Event::SetDays(Some(7)));
                                    event(h, Event::SetDateFilter(DateFilter::Last7Days));
                                }
                                303 => {
                                    event(h, Event::SetDays(Some(30)));
                                    event(h, Event::SetDateFilter(DateFilter::Last30Days));
                                }
                                304 => {
                                    event(h, Event::SetDays(None));
                                    event(h, Event::SetDateFilter(DateFilter::ThisYear));
                                }
                                305 => {
                                    event(h, Event::SetDays(Some(90)));
                                    event(h, Event::SetCustomDate(ktm::backend::shift_ymd(end, -90), end));
                                }
                                306 => {
                                    event(h, Event::SetDays(Some(180)));
                                    event(h, Event::SetCustomDate(ktm::backend::shift_ymd_months(end, -6), end));
                                }
                                307 => {
                                    event(h, Event::SetDays(Some(365)));
                                    event(h, Event::SetCustomDate(ktm::backend::shift_ymd_months(end, -12), end));
                                }
                                _ => {}
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
                            if with(|u| u.state.options.options_open) {
                                SetFocus(with(|u| u.edit));
                            } else {
                                SetFocus(with(|u| u.edit));
                            }
                        }
                        None => {}
                    }
                } else if hit_quick_band(x, y, bands.summary) {
                    let sy = bands.summary.top;
                    let group_left = pad + side_w + scale(h, 235);
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
                    } else if x >= side_w && x < pad + side_w + scale(h, 34) {
                        event(h, Event::ToggleView);
                    }
                } else if hit_quick_band(x, y, bands.filters) {
                    // ③ 통합 검색 범위 단추 (기간 / 형식 / 폴더)
                    let mut fx = pad + side_w + scale(h, 34);
                    let date_chips = [
                        (DateFilter::All, 36),
                        (DateFilter::Today, 36),
                        (DateFilter::Last7Days, 46),
                        (DateFilter::Last30Days, 52),
                        (DateFilter::ThisYear, 36),
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
                        let btn_y = bands.list_top + scale(h, 8) + scale(h, 24);
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
                        let ty = bands.list_top + scale(h, 8) + scale(h, 24);
                        let btn_h = scale(h, 32);
                        let col_w = (rc_w.right - side_w - 2 * pad - scale(h, 16)) / 2;
                        let n_topics = with(|u| u.state.top_topics.len().min(10));
                        for i in 0..n_topics {
                            let col = (i % 2) as i32;
                            let row = (i / 2) as i32;
                            let left = pad + side_w + col * (col_w + scale(h, 16));
                            let top = ty + row * (btn_h + scale(h, 6));
                            if x >= left && x < left + col_w && y >= top && y < top + btn_h {
                                event(h, Event::ApplyTopTopic(i));
                                break;
                            }
                        }
                    }
                } else if !with(|u| u.state.autocompletes.is_empty())
                    && hit_quick_band(x, y, bands.autocomplete)
                {
                    // ⑤ 자동 완성 칩 클릭
                    let mut ax = pad + side_w + scale(h, 62);
                    let n_ac = with(|u| u.state.autocompletes.len().min(5));
                    for i in 0..n_ac {
                        let cw = scale(h, 85);
                        if x >= ax && x < ax + cw {
                            event(h, Event::ApplyAutocomplete(i));
                            break;
                        }
                        ax += cw + scale(h, 6);
                    }
                } else if hit_quick_band(x, y, bands.related)
                {
                    let groups = with(|u| kfind_core::split_semicolon_groups(&u.state.query));
                    let labels: Vec<String> = groups.iter().enumerate().take(8).map(|(i,g)| format!("{}. {}", i+1, g.trim())).collect();
                    let rects = related_chip_rects(&labels, pad + side_w, bands.related.top, rc_w.right - pad, scale(h, 100));
                    if let Some(index) = rects.iter().position(|rc| x >= rc.left && x < rc.right && y >= rc.top && y < rc.bottom) {
                        if let Some(group) = groups.get(index) { event(h, Event::Input(group.clone())); }
                    }
                } else {
                    // ⑥ 결과 행 클릭 또는 [폴더 열기] 단추 클릭
                    let top_y = with(|u| list_top_coord(h, u));
                    let rh = row_height(h);
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
                            let ids = ktm::layout::settings_visible_indices(
                                &u.rows,
                                &u.settings,
                                u.state.selected_folder.as_deref(),
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
                                let (open_clicks, double_click) =
                                    with(|u| (u.settings.open_clicks, msg == WM_LBUTTONDBLCLK));
                                if ktm::layout::click_opens(open_clicks, double_click, false) {
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
                if should_hide_on_close(with(|u| u.settings.minimize_to_tray)) {
                    event(h, Event::Hide);
                } else {
                    DestroyWindow(h);
                }
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
                        Reply::Unified(g, snapshot, start, times) => {
                            if g != with(|u| u.state.generation) || with(|u| u.state.phase) == Phase::Hidden {
                                continue;
                            }
                            phase("search_queue_ms", times.picked - start);
                            phase("search_work_ms", times.done - times.picked);
                            phase("search_deliver_ms", times.done.elapsed());
                            let mut answer = snapshot.answer;
                            let (settings, folder, selected_path) = with(|u| (
                                u.settings.clone(), u.state.selected_folder.clone(),
                                u.state.selected.and_then(|i| u.rows.get(i)).map(|r| ktm::backend::unified::path_key(&r.path)),
                            ));
                            let allowed = ktm::layout::filter_row_indices(&answer.rows, &settings, folder.as_deref());
                            answer.rows = allowed.into_iter().map(|i| answer.rows[i].clone()).collect();
                            event(h, Event::SearchPartFinished {
                                generation: g, kind: snapshot.completed,
                                count: answer.rows.len(), final_part: snapshot.final_part,
                            });
                            edit(|u| {
                                u.rows = answer.rows;
                                u.terms = answer.terms;
                                u.state.folder_counts = answer.folder_counts;
                                u.state.autocompletes = answer.autocompletes;
                                u.leg_errors = snapshot.errors;
                                u.state.error = (!u.leg_errors.is_empty()).then(|| u.leg_errors.join(" · "));
                                if let Some(path) = selected_path {
                                    if let Some(i) = u.rows.iter().position(|r| ktm::backend::unified::path_key(&r.path) == path) {
                                        u.state.selected = Some(i);
                                    }
                                }
                                u.timing.get_or_insert(start);
                                u.ready_at.get_or_insert_with(Instant::now);
                                if snapshot.completed == tm_state::ResultKind::Names || snapshot.final_part {
                                    u.unified_paint = Some((start, snapshot.final_part));
                                }
                            });
                            // Paint each published state even if faster later sources
                            // are already queued in rx; the first filenames stay observable.
                            InvalidateRect(h, null(), 0);
                            UpdateWindow(h);
                        }
                        Reply::Search(g, tab, result, start, times) => {
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
                                    groups: _,
                                    folder_counts,
                                    autocompletes,
                                }) => {
                                    let (settings, folder) = with(|u| {
                                        (u.settings.clone(), u.state.selected_folder.clone())
                                    });
                                    let allowed = ktm::layout::filter_row_indices(
                                        &rows,
                                        &settings,
                                        folder.as_deref(),
                                    );
                                    let rows = allowed
                                        .into_iter()
                                        .filter_map(|i| rows.get(i).cloned())
                                        .collect::<Vec<_>>();
                                    let count = rows.len();
                                    let current_tab = with(|u| u.state.tab);
                                    if current_tab == Tab::All {
                                        let kind = match tab { Tab::Names => tm_state::ResultKind::Names, Tab::Hierarchy => tm_state::ResultKind::Hierarchy, _ => tm_state::ResultKind::Body };
                                        let batches = edit(|u| { u.combined_batches.retain(|(k, _)| *k != kind); u.combined_batches.push((kind, rows.clone())); u.combined_batches.clone() });
                                        let merged = tm_state::combine_results(&batches, |r| if r.path.is_empty() { format!("id:{}", r.doc_id) } else { r.path.replace('/', "\\").to_lowercase() });
                                        let mut combined_rows = merged.into_iter().map(|r| { let mut row = r.row; row.match_kinds = r.kinds; row }).collect::<Vec<_>>();
                                        combined_rows.truncate(80);
                                        event(h, Event::SearchPartFinished { generation: g, kind, count: combined_rows.len(), final_part: false });
                                        edit(|u| { u.rows = combined_rows; u.terms = terms.clone(); u.state.folder_counts = folder_counts; u.state.autocompletes = autocompletes; u.timing.get_or_insert(start); u.ready_at.get_or_insert_with(Instant::now); });
                                    } else {
                                        event(h, Event::SearchFinished { generation: g, count });
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
                                }
                                Err(message) => {
                                    if with(|u| u.state.tab) == Tab::All {
                                        eprintln!("combined search leg failed: {message}");
                                        edit(|u| { u.leg_errors.push(message.clone()); });
                                        event(h, Event::SearchFailed { generation: g, message });
                                    } else {
                                        event(h, Event::SearchFailed { generation: g, message });
                                    }
                                }
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
                            let enabled = !autostart;
                            if let Err(e) = ktm::install::set_autostart(&exe, enabled) {
                                event(h, Event::OpenFailed(e));
                            } else {
                                edit(|u| u.settings.autostart = enabled);
                                let (s, path) =
                                    with(|u| (u.settings.clone(), u.settings_path.clone()));
                                let _ = s.save(&path);
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
        // 화면 캡처 모드: 단축키·트레이·단일 인스턴스 뮤텍스·검색 스레드를 만들지 않고, 설정도
        // 캡처 폴더 안의 임시 자료 폴더를 써서 설치본(사용자 PC)에 영향을 주지 않는다.
        let capture_dir = arg("--capture-dir").map(PathBuf::from);
        let data = std::env::var_os("KTM_TEST_ROOT")
            .map(PathBuf::from)
            .or_else(|| capture_dir.as_ref().map(|d| d.join(".ktm-capture-data")))
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
        if capture_dir.is_none() && crate::lifecycle::run(&args, &data, &source, &destination) {
            return Ok(());
        }
        let pipe_mode_early = arg("--backend").as_deref() == Some("pipe");
        let previous = FindWindowW(wide(CLASS).as_ptr(), null());
        if capture_dir.is_none() && !previous.is_null() && std::env::var_os("KTM_TEST_ROOT").is_none() {
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
        let days_ed = CreateWindowExW(
            0,
            wide("EDIT").as_ptr(),
            wide("").as_ptr(),
            WS_CHILD | WS_TABSTOP | ES_AUTOHSCROLL as u32,
            0,
            0,
            1,
            1,
            h,
            3 as _,
            inst,
            null(),
        );
        SendMessageW(days_ed, 0x00c5, 5, 0);

        let initial_theme = arg("--theme").unwrap_or(settings.theme.clone());
        sync_theme(&initial_theme);

        UI.with(|u| {
            *u.borrow_mut() = Some(Ui {
                state: State {
                    recent_queries: settings.recent_queries.clone(),
                    favorite_queries: settings.favorite_queries.clone(),
                    options: tm_state::UiOptions {
                        search_mode: default_mode(&settings.default_search_mode),
                        ..Default::default()
                    },
                    ..State::default()
                },
                rows: Vec::new(),
                combined_batches: Vec::new(),
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
                unified_paint: None,
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
                leg_errors: Vec::new(),
                smart_presets: settings.smart_presets.clone(),
                recent_query_times: settings.recent_query_times.clone(),
                modal: None,
                modal_tab: 0,
                pending_ui_scale: settings.bounded_ui_scale(),
                settings: settings.clone(),
            })
        });

        SetWindowLongPtrW(h, GWLP_WNDPROC, proc as *const () as isize);
        frame::style_frame(h);
        create_fonts(h);
        layout(h);

        if let Some(dir) = capture_dir {
            let dpi = arg("--dpi").and_then(|v| v.parse::<i32>().ok()).unwrap_or(96);
            let scenario = arg("--scenario").unwrap_or_else(|| "all".into());
            let result = capture::run(h, &dir, dpi, &scenario);
            DestroyWindow(h);
            return result;
        }

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
                        if !slot.search.is_empty() {
                            let j = slot.search.remove(0);
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
                        max_results,
                        skip_cloud_network,
                    )) => {
                        let picked = Instant::now();
                        if tab == Tab::All {
                            // Only the opt-in adapter queues All as one job. The legacy
                            // path continues to queue the original three independent jobs.
                            let date_range = ktm::backend::date_range_for_filter(date_filter, custom_date_range);
                            let _ = ktm::backend::unified::stream(max_results, &cancel, |kind, top| {
                                if kind == tm_state::ResultKind::Names {
                                    let (text, dates) = ktm::backend::unified::names_query(&q, date_range, folder.as_deref(), ext.as_deref());
                                    let mut rows = ktm::backend::pipe_names_limit(&pipe_name, &text, sort, top)?;
                                    ktm::backend::unified::filter_name_dates(&mut rows, dates);
                                    return Ok(Answer { rows,
                                        terms: ktm::backend::unified::query_terms(&q),
                                        groups: kfind_core::split_semicolon_groups(&q),
                                        ..Answer::default() });
                                }
                                let hierarchy = kind == tm_state::ResultKind::Hierarchy;
                                if pipe_mode {
                                    let text = ktm::backend::unified::scoped_query(&q, date_range, folder.as_deref(), ext.as_deref(), match_mode);
                                    let rows = ktm::backend::pipe_search_limit(&pipe_name, &text, hierarchy, top)?;
                                    return Ok(Answer { rows,
                                        terms: ktm::backend::unified::query_terms(&q),
                                        groups: kfind_core::split_semicolon_groups(&q),
                                        ..Answer::default() });
                                }
                                if backend.is_none() { backend = Some(open(&root)?); }
                                backend.as_mut().unwrap().search_with_scope_ext_limit_cloud(
                                    &q, hierarchy, date_range, folder.as_deref(), ext.as_deref(), match_mode,
                                    top, Some(cancel.clone()), skip_cloud_network,
                                )
                            }, |snapshot| {
                                let _ = tx.send(Reply::Unified(g, snapshot, start, WorkTimes { picked, done: Instant::now() }));
                                PostMessageW(handle as HWND, READY, 0, 0);
                            });
                            continue;
                        }
                        let hierarchy = tab == Tab::Hierarchy;
                        let answer = (|| {
                            if tab == Tab::Names {
                                let rows = ktm::backend::pipe_names_limit(
                                    &pipe_name,
                                    &q,
                                    sort,
                                    max_results,
                                )?;
                                let terms = q.split_whitespace().map(str::to_owned).collect();
                                return Ok(Answer {
                                    rows,
                                    terms,
                                    groups: kfind_core::split_semicolon_groups(&q),
                                    folder_counts: Vec::new(),
                                    autocompletes: Vec::new(),
                                });
                            }
                            if pipe_mode {
                                let rows = ktm::backend::pipe_search_limit(
                                    &pipe_name,
                                    &q,
                                    hierarchy,
                                    max_results,
                                )?;
                                let terms = q.split_whitespace().map(str::to_owned).collect();
                                return Ok(Answer {
                                    rows,
                                    terms,
                                    groups: kfind_core::split_semicolon_groups(&q),
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
                            b.search_with_scope_ext_limit_cloud(
                                &q,
                                hierarchy,
                                date_range,
                                folder.as_deref(),
                                ext.as_deref(),
                                match_mode,
                                max_results,
                                Some(cancel),
                                skip_cloud_network,
                            )
                        })();
                        Reply::Search(
                            g,
                            tab,
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
                            max_results,
                            skip_cloud_network,
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
                                    max_results.min(ktm::export::EXPORT_LIMIT),
                                );
                            }
                            if pipe_mode {
                                return ktm::backend::pipe_search_limit(
                                    &pipe_name,
                                    &q,
                                    hierarchy,
                                    max_results.min(ktm::export::EXPORT_LIMIT),
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
                                max_results.min(ktm::export::EXPORT_LIMIT),
                                Some(cancel),
                            )?;
                            let mut rows = ans.rows;
                            b.fill_snippets_with_cloud_policy(
                                &mut rows,
                                &ans.terms,
                                skip_cloud_network,
                            );
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
                    Work::Snippets((g, mut rows, terms, skip_cloud_network)) => {
                        if let Some(b) = backend.as_mut() {
                            let mut only: Vec<Row> = rows.iter().map(|r| r.1.clone()).collect();
                            b.fill_snippets_with_cloud_policy(
                                &mut only,
                                &terms,
                                skip_cloud_network,
                            );
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
            if args.iter().any(|a| a == "--show")
                || with(|u| !u.settings.start_hidden && u.hotkey_error.is_empty())
            {
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
            sync_edit_text(ed, &query);
        }

        if let Some(output) = arg("--self-test").or_else(|| arg("--measure")) {
            probe::smoke(h, std::path::Path::new(&output), arg("--measure").is_some())?;
        }

        if let Some(output) = arg("--unified-measure") {
            probe::unified_measure(h, std::path::Path::new(&output))?;
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
