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

/// (세대, 검색어, 칸, 시작 시각, 취소, 파일명 정렬, 기간 필터, 커스텀 기간, 폴더 필터).
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
);
/// 스크롤로 새로 보이게 된 줄의 원문 찾기: (세대, [(줄 번호, 줄)], 강조 낱말).
type SnipJob = (u64, Vec<(usize, Row)>, Vec<String>);

/// 일꾼 대기열: 검색은 늘 최신 하나, 원문 찾기도 최신 하나(검색 먼저).
#[derive(Default)]
struct Pending {
    search: Option<Job>,
    snippets: Option<SnipJob>,
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
    // 호버 상태
    hover_head_btn: Option<HeadButton>,
    hover_row: Option<usize>,
    hover_folder_btn: Option<usize>,
    hover_tab: Option<usize>,
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
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

fn event(h: HWND, e: Event) {
    let user_input = matches!(e, Event::Input(_));
    let (effects, query_changed) = edit(|u| {
        let prev_query = u.state.query.clone();
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
        ) {
            u.cancel.store(true, Ordering::Relaxed);
        }
        let (s, fx) = tm_state::step(&u.state, e);
        let changed = !user_input && s.query != prev_query;
        u.state = s;
        if u.state.result_count == 0 {
            u.rows.clear();
            u.terms.clear();
            u.top = 0;
        }
        (fx, changed)
    });
    if query_changed {
        let (ed, q) = with(|u| (u.edit, wide(&u.state.query)));
        let caret = q.len().saturating_sub(1);
        unsafe {
            let mut cur = vec![0u16; caret + 2];
            let n = GetWindowTextW(ed, cur.as_mut_ptr(), cur.len() as i32) as usize;
            if !(n == caret && cur[..n] == q[..caret]) {
                SetWindowTextW(ed, q.as_ptr());
                SendMessageW(ed, 0x00b1, caret, caret as isize);
            }
        }
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
            let count = ((rc.bottom - top_y - scale(h, STATUS_H)).max(0) / rh).max(1) as usize;
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
    let tab = scale(h, TAB_H);
    let filter = scale(h, FILTER_H + 8);
    let ac_h = scale(h, 28);
    calc_list_top(head, search, tab, filter, !u.state.autocompletes.is_empty(), ac_h)
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
                let names = edit(|u| {
                    u.name_sort = (u.name_sort + 1) % 4;
                    u.state.tab == Tab::Names
                });
                if names {
                    event(h, Event::Refresh);
                } else {
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
                let hotkey = Settings::load(&path).unwrap_or_default().hotkey;
                let _ = Settings { theme: t, hotkey }.save(&path);
                InvalidateRect(h, null(), 1);
                InvalidateRect(ed, null(), 1);
            }
            _ if ctrl && key == b'F' as usize => {
                SetFocus(with(|u| u.edit));
                SendMessageW(with(|u| u.edit), 0x00b1, 0, -1);
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

        MoveWindow(
            with(|u| u.edit),
            pad + ed_margin_x,
            box_y + ed_margin_y,
            r.right - 2 * pad - 2 * ed_margin_x,
            box_h - 2 * ed_margin_y,
            1,
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
                SetBkMode(dc, if marked { OPAQUE as i32 } else { TRANSPARENT as i32 });
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

        let head = scale(h, HEAD_H);
        let pad = scale(h, 12);
        let bw = scale(h, CAPTION_W);

        // ① 상단 메뉴바
        fill(RECT { left: 0, top: 0, right: r.right, bottom: head }, p.head_bg);
        let (hover_btn, theme_str) = with(|u| (u.hover_head_btn, u.theme.clone()));
        for b in HEAD_BUTTONS {
            let (l, right) = head_button_span(b, r.right, bw);
            let br = RECT { left: l, top: 0, right, bottom: head };
            let hovered = hover_btn == Some(b);
            if hovered {
                fill(br, if b == HeadButton::Close { p.close_hover } else { p.head_button_hover });
            }
            SetTextColor(mem, if hovered && b == HeadButton::Close { p.on_close_hover } else { p.head_glyph });
            SelectObject(mem, with(|u| u.glyph));
            text(&head_button_glyph(b).to_string(), br, if hovered && b == HeadButton::Close { p.on_close_hover } else { p.head_glyph }, DT_CENTER);
        }

        let buttons_left = head_button_span(HEAD_BUTTONS[0], r.right, bw).0;
        SelectObject(mem, with(|u| u.title_font));
        let title_left = scale(h, 12);
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
            left: pad,
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
        fill(RECT { left: 0, top: tab_y, right: r.right, bottom: tab_y + tab_h }, p.tab_bg);
        fill(RECT { left: 0, top: tab_y + tab_h - 1, right: r.right, bottom: tab_y + tab_h }, p.divider);

        with(|u| {
            let names_label = format!("파일명 · {}", ktm::backend::NAME_SORTS[u.name_sort % 4].1);
            let tabs = [
                ("본문", Tab::Body),
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

            // ④ 기간 / 폴더 거르기 줄
            let fy = tab_y + tab_h + scale(h, 6);
            let fh = scale(h, FILTER_H);
            SelectObject(mem, u.small);
            text(
                "기간:",
                RECT { left: pad, top: fy, right: pad + scale(h, 36), bottom: fy + fh },
                p.text_sub,
                0,
            );
            let mut fx = pad + scale(h, 38);
            let date_chips = [
                (DateFilter::All, "전체"),
                (DateFilter::Today, "오늘"),
                (DateFilter::ThisWeek, "이번 주"),
                (DateFilter::ThisMonth, "이번 달"),
                (DateFilter::ThisYear, "올해"),
                (DateFilter::Custom, "직접"),
            ];
            for (df, lbl) in &date_chips {
                let cw = scale(h, 52);
                let rc = RECT { left: fx, top: fy, right: fx + cw, bottom: fy + fh };
                let active = u.state.date_filter == *df;
                if active {
                    fill(rc, p.accent);
                    text(lbl, rc, p.bg, DT_CENTER);
                } else {
                    fill(rc, p.tab_hover);
                    text(lbl, rc, p.tab_text, DT_CENTER);
                }
                fx += cw + scale(h, 4);
            }

            if !u.state.folder_counts.is_empty() {
                fx += scale(h, 8);
                text(
                    "폴더:",
                    RECT { left: fx, top: fy, right: fx + scale(h, 36), bottom: fy + fh },
                    p.text_sub,
                    0,
                );
                fx += scale(h, 38);
                let all_active = u.state.selected_folder.is_none();
                let all_rc = RECT { left: fx, top: fy, right: fx + scale(h, 44), bottom: fy + fh };
                if all_active {
                    fill(all_rc, p.accent);
                    text("전체", all_rc, p.bg, DT_CENTER);
                } else {
                    fill(all_rc, p.tab_hover);
                    text("전체", all_rc, p.tab_text, DT_CENTER);
                }
                fx += scale(h, 48);

                for (folder, count) in &u.state.folder_counts {
                    let lbl = format!("{folder}({count})");
                    let cw = scale(h, 75).min(r.right - fx - pad);
                    if cw < scale(h, 30) {
                        break;
                    }
                    let rc = RECT { left: fx, top: fy, right: fx + cw, bottom: fy + fh };
                    let active = u.state.selected_folder.as_deref() == Some(folder);
                    if active {
                        fill(rc, p.accent);
                        text(&lbl, rc, p.bg, DT_CENTER | DT_END_ELLIPSIS);
                    } else {
                        fill(rc, p.tab_hover);
                        text(&lbl, rc, p.tab_text, DT_CENTER | DT_END_ELLIPSIS);
                    }
                    fx += cw + scale(h, 4);
                }
            }

            // ⑤ 자동완성 칩 (입력 중일 때)
            if !u.state.autocompletes.is_empty() {
                let ay = fy + fh + scale(h, 4);
                let ah = scale(h, 22);
                let mut ax = pad;
                text(
                    "자동완성:",
                    RECT { left: ax, top: ay, right: ax + scale(h, 60), bottom: ay + ah },
                    p.text_sub,
                    0,
                );
                ax += scale(h, 62);
                for (i, term) in u.state.autocompletes.iter().take(5).enumerate() {
                    let cw = scale(h, 85).min(r.right - ax - pad);
                    if cw < scale(h, 35) {
                        break;
                    }
                    let rc = RECT { left: ax, top: ay, right: ax + cw, bottom: ay + ah };
                    let active = u.state.selected_autocomplete == Some(i);
                    if active {
                        fill(rc, p.selected);
                        text(term, rc, p.row_text(true, p.accent), DT_CENTER | DT_END_ELLIPSIS);
                    } else {
                        fill(rc, p.tab_hover);
                        text(term, rc, p.text, DT_CENTER | DT_END_ELLIPSIS);
                    }
                    ax += cw + scale(h, 6);
                }
            }

            // ⑥ 빈 검색어 첫 화면 주요 주제 1~10위 그리드
            if u.state.query.trim().is_empty() && u.rows.is_empty() {
                let ty = fy + fh + scale(h, 14);
                SelectObject(mem, u.small_bold);
                text(
                    "주요 주제 1~10위",
                    RECT { left: pad, top: ty, right: r.right - pad, bottom: ty + scale(h, 20) },
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
                    let rc = RECT { left, top, right: left + col_w, bottom: top + btn_h };
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
                    text(&label, rc, if active { p.accent } else { p.text }, DT_CENTER | DT_END_ELLIPSIS);
                }
            }

            // ⑦ 결과 목록 (클립보드 popup.rs 행 스타일)
            let top_y = list_top_coord(h, u);
            let rh = scale(h, ROW_H);
            let status_h = scale(h, STATUS_H);
            let visible = ((r.bottom - top_y - status_h).max(0) / rh).max(0) as usize;

            let line_h1 = scale(h, 22);
            let line_h2 = scale(h, 20);
            let line_h3 = scale(h, 24);
            let folder_w = scale(h, ROW_FOLDER_W);

            for (n, row) in u.rows.iter().enumerate().skip(u.top).take(visible) {
                let y = top_y + (n - u.top) as i32 * rh;
                let row_rc = RECT { left: 0, top: y, right: r.right, bottom: y + rh };
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
                    RECT { left: scale(h, 8), top: y + scale(h, 2), right: scale(h, 24), bottom: y + line_h1 },
                    p.row_text(sel, p.number),
                    DT_CENTER,
                );

                // 종류 태그/아이콘
                let tag = extension_tag(&row.name);
                SelectObject(mem, u.small_bold);
                text(
                    tag,
                    RECT { left: scale(h, 28), top: y + scale(h, 2), right: scale(h, 66), bottom: y + line_h1 },
                    p.row_text(sel, p.text_faint),
                    0,
                );

                // 첫 줄: 파일명 (굵게)
                SelectObject(mem, u.font_bold);
                let text_right = r.right - folder_w - scale(h, 8);
                text(
                    &row.name,
                    RECT { left: scale(h, 68), top: y + scale(h, 2), right: text_right, bottom: y + line_h1 },
                    p.row_text(sel, p.text),
                    DT_END_ELLIPSIS,
                );

                // 오른쪽: 작은 단추 [폴더 열기]
                let folder_rc = calc_folder_button_rect(y, r.right, scale(h, 6), scale(h, 4), folder_w, scale(h, 22));
                let folder_hovered = u.hover_folder_btn == Some(n);
                if folder_hovered {
                    fill(folder_rc, p.head_button_hover);
                }
                draw_open_icon(mem, folder_rc, p.row_text(sel, p.icon));

                // 둘째 줄: 경로 (흐리게)
                SelectObject(mem, u.small);
                text(
                    &row.parent,
                    RECT {
                        left: scale(h, 68),
                        top: y + line_h1,
                        right: text_right,
                        bottom: y + line_h1 + line_h2,
                    },
                    p.row_text(sel, p.text_faint),
                    DT_PATH_ELLIPSIS,
                );

                // 셋째 줄: 발췌 문장 (강조 하이라이트)
                let (snippet, terms): (&str, &[String]) = if row.snippet.is_empty() {
                    (&row.location, &[])
                } else {
                    (&row.snippet, &u.terms)
                };
                let highlight_bg = if theme_str == "light" { 0x70eaff } else { 0x805020 };
                SelectObject(mem, u.small);
                highlighted(
                    mem,
                    snippet,
                    terms,
                    RECT {
                        left: scale(h, 68),
                        top: y + line_h1 + line_h2,
                        right: r.right - pad,
                        bottom: y + line_h1 + line_h2 + line_h3,
                    },
                    p.row_text(sel, p.text),
                    highlight_bg,
                );
            }

            // ⑧ 하단 상태 표시줄
            let status_y = r.bottom - status_h;
            fill(RECT { left: 0, top: status_y, right: r.right, bottom: r.bottom }, p.tab_bg);
            fill(RECT { left: 0, top: status_y, right: r.right, bottom: status_y + 1 }, p.divider);

            let status = if !u.hotkey_error.is_empty() {
                u.hotkey_error.clone()
            } else if let Some(e) = &u.state.error {
                e.clone()
            } else {
                match u.state.phase {
                    Phase::Searching => "검색 중…".into(),
                    Phase::Results if u.state.tab == Tab::Names => format!(
                        "{}건 · Enter 열기 · Ctrl+Enter 폴더 · Ctrl+S 정렬",
                        u.rows.len()
                    ),
                    Phase::Results => format!(
                        "{}건 · Enter 열기 · Ctrl+Enter 폴더 · ↑↓ 선택",
                        u.rows.len()
                    ),
                    _ if u.state.query.trim().is_empty() => {
                        "주제 단추를 누르거나 검색어를 입력하세요".into()
                    }
                    _ => "2글자부터 실시간 검색 · 1글자는 Enter".into(),
                }
            };
            SelectObject(mem, u.small);
            text(
                &status,
                RECT { left: pad, top: status_y, right: r.right - pad, bottom: r.bottom },
                p.text_sub,
                DT_END_ELLIPSIS,
            );
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
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
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
                frame::frame_hit(p.x, p.y, r.right, r.bottom, grip, head, on_btn, false, false).code() as isize
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
                let folder_w = scale(h, ROW_FOLDER_W);

                let new_head_btn = head_button_hit(x, y, r.right, bw, head);
                let tab_y = head + scale(h, SEARCH_BOX_H + 12);
                let tab_h = scale(h, TAB_H);
                let new_tab = hit_tab(x, y, pad, tab_y, tab_h, r.right);

                let top_y = with(|u| list_top_coord(h, u));
                let rh = scale(h, ROW_H);
                let (new_row, new_folder_btn) = if y >= top_y && y < client_bottom(h) - scale(h, STATUS_H) {
                    let row_idx = frame::row_index(y, top_y, rh, with(|u| u.top), with(|u| u.rows.len()));
                    let is_folder = if let Some(idx) = row_idx {
                        let row_top = top_y + (idx.saturating_sub(with(|u| u.top))) as i32 * rh;
                        let btn_rc = calc_folder_button_rect(row_top, r.right, scale(h, 6), scale(h, 4), folder_w, scale(h, 22));
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

                // ① 상단 캡션 단추 클릭
                if let Some(btn) = head_button_hit(x, y, rc_w.right, bw, head) {
                    match btn {
                        HeadButton::Minimize | HeadButton::Close => {
                            event(h, Event::Hide);
                            return 0;
                        }
                    }
                }

                let tab_y = head + scale(h, SEARCH_BOX_H + 12);
                let tab_h = scale(h, TAB_H);
                let fy = tab_y + tab_h + scale(h, 6);
                let fh = scale(h, FILTER_H);

                if y >= tab_y && y < tab_y + tab_h {
                    // ② 탭 클릭
                    if let Some(tab_idx) = hit_tab(x, y, pad, tab_y, tab_h, rc_w.right) {
                        let tab = match tab_idx {
                            0 => Tab::Body,
                            1 => Tab::Hierarchy,
                            _ => Tab::Names,
                        };
                        event(h, Event::SwitchTab(tab));
                    }
                } else if y >= fy && y < fy + fh {
                    // ③ 기간 / 폴더 거르기 단추
                    let mut fx = pad + scale(h, 38);
                    let date_chips = [
                        DateFilter::All,
                        DateFilter::Today,
                        DateFilter::ThisWeek,
                        DateFilter::ThisMonth,
                        DateFilter::ThisYear,
                        DateFilter::Custom,
                    ];
                    let mut handled = false;
                    for df in &date_chips {
                        let cw = scale(h, 52);
                        if x >= fx && x < fx + cw {
                            event(h, Event::SetDateFilter(*df));
                            handled = true;
                            break;
                        }
                        fx += cw + scale(h, 4);
                    }
                    if !handled && !with(|u| u.state.folder_counts.is_empty()) {
                        fx += scale(h, 8) + scale(h, 38);
                        if x >= fx && x < fx + scale(h, 44) {
                            event(h, Event::SetFolderFilter(None));
                        } else {
                            fx += scale(h, 48);
                            let fcs = with(|u| u.state.folder_counts.clone());
                            for (folder, _) in fcs {
                                let cw = scale(h, 75);
                                if x >= fx && x < fx + cw {
                                    let cur = with(|u| u.state.selected_folder.clone());
                                    if cur.as_deref() == Some(&folder) {
                                        event(h, Event::SetFolderFilter(None));
                                    } else {
                                        event(h, Event::SetFolderFilter(Some(folder)));
                                    }
                                    break;
                                }
                                fx += cw + scale(h, 4);
                            }
                        }
                    }
                } else if with(|u| u.state.query.trim().is_empty() && u.rows.is_empty()) {
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
                    if y >= top_y && y < client_bottom(h) - scale(h, STATUS_H) {
                        let n = with(|u| u.top) + ((y - top_y) / rh) as usize;
                        if n < with(|u| u.rows.len()) {
                            let row_top = top_y + ((y - top_y) / rh) * rh;
                            let btn_rc = calc_folder_button_rect(row_top, rc_w.right, scale(h, 6), scale(h, 4), folder_w, scale(h, 22));
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

        let initial_theme = arg("--theme").unwrap_or(settings.theme.clone());
        sync_theme(&initial_theme);

        UI.with(|u| {
            *u.borrow_mut() = Some(Ui {
                state: State::default(),
                rows: Vec::new(),
                terms: Vec::new(),
                edit: ed,
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
                hover_head_btn: None,
                hover_row: None,
                hover_folder_btn: None,
                hover_tab: None,
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
            let open = |root: &PathBuf| Backend::open(root).map(|b| b.with_paragraphs(para_path.clone()));
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
                    Work::Search((g, q, tab, start, cancel, sort, date_filter, custom_date_range, folder)) => {
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
                            let date_range = ktm::backend::date_range_for_filter(date_filter, custom_date_range);
                            b.search_with_scope(&q, hierarchy, date_range, folder.as_deref(), Some(cancel))
                        })();
                        Reply::Search(g, answer, start, WorkTimes { picked, done: Instant::now() })
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
            for f in [u.font, u.font_bold, u.small, u.small_bold, u.title_font, u.glyph, u.glyph_big] {
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
