//! Win32 입력/IME + GDI 이중 버퍼. 검색 스레드는 이벤트로만 깨며 창 상태는 step으로만 바꾼다.
#[path = "window_probe.rs"]
mod probe;
use ktm::{
    backend::{Answer, Backend, Row},
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
        Accessibility::*,
        HiDpi::*,
        Input::{Ime::*, KeyboardAndMouse::*},
        WindowsAndMessaging::*,
    },
};
const READY: u32 = WM_APP + 21;
const WARMED: u32 = WM_APP + 22;
pub(crate) const CLASS: &str = "HwpAutoDocFit.MongttangSearch";
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
/// 일꾼이 검색을 집어 든 시각과 마친 시각(첫 입력 구간 단계별 계측, MG3).
#[derive(Clone, Copy)]
struct WorkTimes {
    picked: Instant,
    done: Instant,
}
enum Reply {
    /// (세대, 답, 입력 시각, 일꾼 단계 시각).
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
    brush: HBRUSH,
    theme: String,
    tx: Arc<(Mutex<Pending>, Condvar)>,
    rx: mpsc::Receiver<Reply>,
    status: mpsc::Sender<String>,
    top: usize,
    timing: Option<Instant>,
    /// 결과를 받은 시각(READY 처리) — 그리기까지 구간 계측.
    ready_at: Option<Instant>,
    hotkey: String,
    hotkey_error: String,
    cancel: Arc<AtomicBool>,
    settings_path: PathBuf,
    /// 시작 데우기가 끝났는지(측정 탐침이 기다림).
    warmed: bool,
    /// 파일명 탭 정렬(`backend::NAME_SORTS` 번호, Ctrl+S로 돌림).
    name_sort: usize,
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
        let changed = s.query != prev_query;
        u.state = s;
        if u.state.result_count == 0 {
            u.rows.clear();
            u.terms.clear();
            u.top = 0;
        }
        (fx, changed)
    });
    if query_changed {
        with(|u| unsafe {
            let q = wide(&u.state.query);
            SetWindowTextW(u.edit, q.as_ptr());
            SendMessageW(
                u.edit,
                0x00b1,
                u.state.query.len() as usize,
                u.state.query.len() as isize,
            );
        });
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
                let tab = with(|u| match (u.state.tab, shift) {
                    (Tab::Body, false) | (Tab::Names, true) => Tab::Hierarchy,
                    (Tab::Hierarchy, false) | (Tab::Body, true) => Tab::Names,
                    _ => Tab::Body,
                });
                event(h, Event::SwitchTab(tab));
            }
            _ if ctrl && key == b'S' as usize => {
                // 파일명 정렬 돌리기(관련도 → 이름 → 크기 → 날짜). 파일명 탭이면 바로 다시 찾는다.
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
            if let Some(ready) = edit(|u| u.ready_at.take()) {
                phase("search_ready_to_paint_ms", ready.elapsed());
            }
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
            let names_label = format!("파일명 · {}", ktm::backend::NAME_SORTS[u.name_sort % 4].1);
            for (i, (label, tab)) in [
                ("본문", Tab::Body),
                ("위계", Tab::Hierarchy),
                (names_label.as_str(), Tab::Names),
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
            // 거르기 줄 (기간 거르기 + 폴더 거르기) — SEMI2 scope API
            let fy = scale(h, 118);
            let fh = scale(h, 24);
            let date_chips = [
                (DateFilter::All, "전체"),
                (DateFilter::Today, "오늘"),
                (DateFilter::ThisWeek, "이번 주"),
                (DateFilter::ThisMonth, "이번 달"),
                (DateFilter::ThisYear, "올해"),
                (DateFilter::Custom, "직접"),
            ];
            let mut fx = p;
            text(
                "기간:",
                RECT {
                    left: fx,
                    top: fy,
                    right: fx + scale(h, 36),
                    bottom: fy + fh,
                },
                fg,
                0,
            );
            fx += scale(h, 38);
            for (df, lbl) in &date_chips {
                let cw = scale(h, 52);
                let rc = RECT {
                    left: fx,
                    top: fy,
                    right: fx + cw,
                    bottom: fy + fh,
                };
                let active = u.state.date_filter == *df;
                if active {
                    fill(rc, sel);
                }
                text(lbl, rc, if active { selfg } else { fg }, DT_CENTER);
                fx += cw + scale(h, 4);
            }
            if !u.state.folder_counts.is_empty() {
                fx += scale(h, 8);
                text(
                    "폴더:",
                    RECT {
                        left: fx,
                        top: fy,
                        right: fx + scale(h, 36),
                        bottom: fy + fh,
                    },
                    fg,
                    0,
                );
                fx += scale(h, 38);
                let all_active = u.state.selected_folder.is_none();
                let all_rc = RECT {
                    left: fx,
                    top: fy,
                    right: fx + scale(h, 44),
                    bottom: fy + fh,
                };
                if all_active {
                    fill(all_rc, sel);
                }
                text("전체", all_rc, if all_active { selfg } else { fg }, DT_CENTER);
                fx += scale(h, 48);
                for (folder, count) in &u.state.folder_counts {
                    let lbl = format!("{folder}({count})");
                    let cw = scale(h, 75).min(r.right - fx - p);
                    if cw < scale(h, 30) {
                        break;
                    }
                    let rc = RECT {
                        left: fx,
                        top: fy,
                        right: fx + cw,
                        bottom: fy + fh,
                    };
                    let active = u.state.selected_folder.as_deref() == Some(folder);
                    if active {
                        fill(rc, sel);
                    }
                    text(
                        &lbl,
                        rc,
                        if active { selfg } else { fg },
                        DT_CENTER | DT_END_ELLIPSIS,
                    );
                    fx += cw + scale(h, 4);
                }
            }

            // ② 빈 검색어 첫 화면 주제 1~10위 단추
            if u.state.query.trim().is_empty() && u.rows.is_empty() {
                let ty = scale(h, 154);
                text(
                    "빈 검색어 첫 화면 · 주요 주제 1~10위",
                    RECT {
                        left: p,
                        top: ty,
                        right: r.right - p,
                        bottom: ty + line,
                    },
                    fg,
                    0,
                );
                let btn_y = ty + scale(h, 28);
                let btn_h = scale(h, 32);
                let col_w = (r.right - 2 * p - scale(h, 16)) / 2;
                for (i, topic) in u.state.top_topics.iter().take(10).enumerate() {
                    let col = (i % 2) as i32;
                    let row = (i / 2) as i32;
                    let left = p + col * (col_w + scale(h, 16));
                    let top = btn_y + row * (btn_h + scale(h, 6));
                    let rc = RECT {
                        left,
                        top,
                        right: left + col_w,
                        bottom: top + btn_h,
                    };
                    let active = u.state.selected_top_topic == Some(i);
                    if active {
                        fill(rc, sel);
                    } else {
                        fill(rc, if u.theme == "light" { 0xf2f2f2 } else { 0x2e2e2e });
                    }
                    let label = format!("{}. {}", i + 1, topic);
                    text(&label, rc, if active { selfg } else { fg }, DT_CENTER);
                }
            }

            // ① 입력 중 자동 완성(SEMI3 빈도순, 2글자부터, 키보드 위·아래 선택)
            if !u.state.autocompletes.is_empty() {
                let ay = scale(h, 146);
                let ah = scale(h, 24);
                let mut ax = p;
                text(
                    "자동완성:",
                    RECT {
                        left: ax,
                        top: ay,
                        right: ax + scale(h, 60),
                        bottom: ay + ah,
                    },
                    fg,
                    0,
                );
                ax += scale(h, 62);
                for (i, term) in u.state.autocompletes.iter().take(5).enumerate() {
                    let cw = scale(h, 85).min(r.right - ax - p);
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
                        fill(rc, sel);
                    } else {
                        fill(rc, if u.theme == "light" { 0xfaebd7 } else { 0x3d3020 });
                    }
                    text(
                        term,
                        rc,
                        if active { selfg } else { fg },
                        DT_CENTER | DT_END_ELLIPSIS,
                    );
                    ax += cw + scale(h, 6);
                }
            }

            let y = if !u.state.autocompletes.is_empty() {
                scale(h, 174)
            } else {
                scale(h, 146)
            };
            let rh = scale(h, 72);
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
thread_local! {static METRICS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };}
/// 측정(KTM_METRICS) 켰을 때만 줄을 메모리에 모은다. 파일 쓰기는 창이 닫힐 때 한 번
/// (`flush_metrics`): 쓰기가 잰 구간·다음 표시·첫 결과에 끼어들지 않게.
fn phase(name: &'static str, d: std::time::Duration) {
    log(name, d.as_secs_f64() * 1000.0);
}
fn log(name: &str, value: f64) {
    log_line(format!("{{\"metric\":\"{name}\",\"ms\":{value:.4}}}"));
}
fn log_line(line: String) {
    if std::env::var_os("KTM_METRICS").is_some() {
        // 오래 켜 둔 수동 측정은 1,000줄마다 한 번 쓴다(메모리 상한).
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
                let p = scale(h, 14);
                if y >= scale(h, 82) && y < scale(h, 116) {
                    let tab = if x < scale(h, 179) {
                        Tab::Body
                    } else if x < scale(h, 344) {
                        Tab::Hierarchy
                    } else {
                        Tab::Names
                    };
                    event(h, Event::SwitchTab(tab));
                } else if y >= scale(h, 118) && y < scale(h, 146) {
                    // 기간 거르기 단추 확인
                    let mut fx = p + scale(h, 38);
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
                    // ② 빈 검색어 첫 화면 주제 1~10위 단추 클릭
                    let ty = scale(h, 154) + scale(h, 28);
                    let btn_h = scale(h, 32);
                    let mut rc_w: RECT = std::mem::zeroed();
                    GetClientRect(h, &mut rc_w);
                    let col_w = (rc_w.right - 2 * p - scale(h, 16)) / 2;
                    let n_topics = with(|u| u.state.top_topics.len().min(10));
                    for i in 0..n_topics {
                        let col = (i % 2) as i32;
                        let row = (i / 2) as i32;
                        let left = p + col * (col_w + scale(h, 16));
                        let top = ty + row * (btn_h + scale(h, 6));
                        if x >= left && x < left + col_w && y >= top && y < top + btn_h {
                            event(h, Event::ApplyTopTopic(i));
                            break;
                        }
                    }
                } else if !with(|u| u.state.autocompletes.is_empty())
                    && y >= scale(h, 146)
                    && y < scale(h, 170)
                {
                    // ① 자동 완성 칩 클릭
                    let mut ax = p + scale(h, 62);
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
                    let y_start = with(|u| {
                        if !u.state.autocompletes.is_empty() {
                            scale(h, 174)
                        } else {
                            scale(h, 146)
                        }
                    });
                    if y >= y_start {
                        let n = with(|u| u.top) + ((y - y_start) / scale(h, 72)) as usize;
                        if n < with(|u| u.rows.len()) && y < client_bottom(h) - scale(h, 24) {
                            event(h, Event::Select(n));
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
            // 이미 떠 있음: 자동 실행(--hidden)이면 그대로 두고, 사람이 다시 실행했으면 창을 보인다.
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
                ready_at: None,
                hotkey: String::new(),
                hotkey_error: String::new(),
                cancel: Arc::new(AtomicBool::new(false)),
                settings_path: data.join("settings.json"),
                warmed: pipe_mode_early,
                name_sort: 0,
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
