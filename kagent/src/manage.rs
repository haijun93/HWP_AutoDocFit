//! 몽땅클립보드 관리창(S18, UI1): Flutter `features/kclip/kclip_dialog.dart`와 같은 기능을 kagent 안 Win32 창으로.
//!
//! - 기술은 빠른 목록(popup)과 같다: 사용자 틀(ui-core window·frame), GDI 이중 버퍼, 글꼴 캐시, 직접 그린 단추·탭·목록,
//!   한 줄 입력(메모·찾기, IME), 상세 글은 읽기 전용 OS EDIT(Q14), 메뉴는 TrackPopupMenu, UIA 접근성.
//! - 상태는 `manage_state::Model` 하나이고 [`dispatch`]만 바꾼다. 창은 돌아온 `Effect`를 실행한다.
//! - 화면 스레드에서 막는 호출 없음: 목록·상세·작업·표 확인·썸네일은 작업 스레드 → `PostMessageW(WM_APP+n)`.
//!   (일시정지·다시 기록·자동 실행은 빠른 목록 설정 메뉴처럼 창 스레드에서 바로 — 타이머·레지스트리 한 번.)
//! - 엑셀 표 → 한/글 표, 표 음영 없이 복사는 Python 기능이라 앱 서비스 파이프(`kagent_app_service`)에 맡긴다.
//!   앱이 꺼져 있으면 안내 글만 보인다.
//! - 화면 구성: 머리글(항상 위·접기·최대화·닫기) / 기록 상태 줄(상태·단축키·일시정지·시간·모두 지우기·끄기) /
//!   [목록 | 설정] / 목록: 메모 칸·끌어 놓기 안내·엑셀 표 단추·분류·찾기·항목(그림 썸네일·표 메뉴·고정·지우기)·스크롤 막대 /
//!   설정: 자동 실행·보관 개수·크기·기간·켜고 끄는 설정·화면캡처 폴더 / 맨 아래 알림 글.
//!   항목 누름 = 상세(글·그림 + 닫기·다시 복사), 웹 글 두 번 누름 = 브라우저.

use std::cell::RefCell;
use std::collections::HashSet;

use ui_core::a11y::{self, Node, Role};
use ui_core::controls::{ButtonKind, Visual, draw_button, draw_check, tab_layout};
use ui_core::font::{FACE_GLYPH, FACE_UI, FontCache, FontSpec};
use ui_core::geom::Rect;
use ui_core::list::{ScrollHit, Scrollbar, Wheel};
use ui_core::menu::Entry;
use ui_core::native_edit::EditColors;
use ui_core::paint::{BackBuffer, Painter};
use ui_core::text_input::{EditResult, LineEdit, key_of};
use ui_core::theme::{AppColors, ClipPalette};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, SetFocus, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_CONTROL, VK_DOWN,
    VK_END, VK_ESCAPE, VK_HOME, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
    VK_DELETE, VK_F5,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::category::Category;
use crate::json::{self, Value};
use crate::manage_state::{
    self as ms, CATEGORIES, Confirm, Detail, DetailBody, Effect, Event, Job, Load, Model, OpenMode, Section, Setting,
    Snapshot,
};
use crate::store::Summary;
use crate::thumb::Thumb;
use crate::util::wide;
use crate::{app, main_hwnd};
use ui_core::frame::scale;

const CLASS: &str = "HwpAutoDocFit.KAgentManage";
const TITLE: &str = "몽땅클립보드 관리";
const WM_LOADED: u32 = WM_APP + 60;
const WM_DONE: u32 = WM_APP + 61;
const WM_DETAIL: u32 = WM_APP + 62;
const WM_TABLE: u32 = WM_APP + 63;
const WM_THUMB: u32 = WM_APP + 64;
const WM_DROPPED: u32 = WM_APP + 65;
const WM_MOUSELEAVE: u32 = 0x02A3;
const TIMER_DEBOUNCE: usize = 1;
const ID_EDIT: usize = 501;
const ID_LABEL: usize = 502;

const HEAD_H: i32 = 32;
const CAPTION_W: i32 = 46;
const CTRL_H: i32 = 28;
const SECTION_H: i32 = 30;
const ROW_H: i32 = 46;
const SET_H: i32 = 32;
const MSG_H: i32 = 26;
const PAD: i32 = 12;
const GAP: i32 = 6;
const ACT_W: i32 = 32;
const SCROLL_W: i32 = 10;
const MIN_W: i32 = 480;
const MIN_H: i32 = 420;
const DEFAULT_W: i32 = 600;
const DEFAULT_H: i32 = 720;
const THUMB: i32 = 36;

const F_BODY: FontSpec = FontSpec::clear(FACE_UI, 14, 400);
const F_SMALL: FontSpec = FontSpec::clear(FACE_UI, 12, 400);
const F_BOLD: FontSpec = FontSpec::clear(FACE_UI, 13, 700);
const F_GLYPH: FontSpec = FontSpec::clear(FACE_GLYPH, 10, 400);
const F_ICON: FontSpec = FontSpec::clear(FACE_GLYPH, 16, 400);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeadBtn {
    Top,
    Collapse,
    Max,
    Close,
}
const HEAD_BTNS: [HeadBtn; 4] = [HeadBtn::Top, HeadBtn::Collapse, HeadBtn::Max, HeadBtn::Close];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StatusBtn {
    PauseToggle,
    PauseMenu,
    Clear,
    Quit,
}

/// 누를 수 있는 곳(마우스·키보드·접근성 공통).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    Head(HeadBtn),
    Status(StatusBtn),
    Section(Section),
    MemoBox,
    MemoAdd,
    Excel,
    Category(usize),
    SearchBox,
    Row(usize),
    RowMenu(usize),
    RowPin(usize),
    RowDelete(usize),
    Scroll,
    Setting(usize),
    DetailClose,
    DetailRecopy,
}

impl Hit {
    /// 접근성 요소 id(창 안 고유).
    fn id(self) -> u32 {
        match self {
            Hit::Head(b) => 1 + HEAD_BTNS.iter().position(|x| *x == b).unwrap_or(0) as u32,
            Hit::Status(s) => 10 + s as u32,
            Hit::Section(Section::List) => 20,
            Hit::Section(Section::Settings) => 21,
            Hit::MemoBox => 30,
            Hit::MemoAdd => 31,
            Hit::Excel => 32,
            Hit::SearchBox => 33,
            Hit::DetailClose => 34,
            Hit::DetailRecopy => 35,
            Hit::Scroll => 36,
            Hit::Category(i) => 40 + i as u32,
            Hit::Setting(i) => 100 + i as u32,
            Hit::Row(i) => 1000 + i as u32 * 4,
            Hit::RowMenu(i) => 1001 + i as u32 * 4,
            Hit::RowPin(i) => 1002 + i as u32 * 4,
            Hit::RowDelete(i) => 1003 + i as u32 * 4,
        }
    }
}

const A11Y_STATUS: u32 = 90;
const A11Y_HOTKEY: u32 = 91;
const A11Y_SECTIONS: u32 = 92;
const A11Y_CATS: u32 = 93;
const A11Y_LIST: u32 = 94;
const A11Y_EMPTY: u32 = 95;
const A11Y_DROP: u32 = 96;
const A11Y_MESSAGE: u32 = 97;
const A11Y_DETAIL: u32 = 98;
const A11Y_SETTINGS: u32 = 99;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Memo,
    Search,
    List,
    Setting(usize),
}

struct Win {
    hwnd: HWND,
    model: Model,
    memo: LineEdit,
    search: LineEdit,
    focus: Focus,
    hot: Vec<(Rect, Hit)>,
    hover: Option<Hit>,
    pressed: Option<Hit>,
    restore_rect: Option<RECT>,
    expanded_h: Option<i32>,
    detail_image: Option<Thumb>,
    scroll: Option<(Rect, Scrollbar)>,
    drag: Option<(i32, usize)>,
    wheel: Wheel,
    settings_top: usize,
    edit: HWND,
    label: HWND,
    edit_colors: EditColors,
    thumbs_asked: HashSet<i64>,
    /// 시험: 작업 스레드(썸네일)를 띄우지 않는다.
    quiet: bool,
    tracking: bool,
}

thread_local! {
    static WIN: RefCell<Option<Win>> = const { RefCell::new(None) };
    static FONTS: RefCell<FontCache> = const { RefCell::new(FontCache::new()) };
}

fn with<R>(f: impl FnOnce(&mut Win) -> R) -> Option<R> {
    WIN.with(|w| w.borrow_mut().as_mut().map(f))
}

fn hwnd() -> HWND {
    with(|w| w.hwnd).unwrap_or(std::ptr::null_mut())
}

// ---- 저장(창 위치·크기·항상 위·분류) ----

fn state_path() -> String {
    format!("{}\\manage_window.json", app().dir)
}

struct Saved {
    rect: Option<[i32; 4]>,
    topmost: bool,
    category: Category,
}

fn load_saved() -> Saved {
    let v = std::fs::read_to_string(state_path()).ok().and_then(|t| json::parse(&t));
    let num = |k: &str| v.as_ref().and_then(|v| v.num(k)).filter(|n| n.abs() < 100_000.0).map(|n| n as i32);
    let rect = match (num("x"), num("y"), num("width"), num("height")) {
        (Some(x), Some(y), Some(w), Some(h)) if w >= MIN_W && h >= HEAD_H => Some([x, y, w, h]),
        _ => None,
    };
    let category = v.as_ref().and_then(|v| v.str("category")).and_then(|c| CATEGORIES.iter().copied().find(|x| x.label() == c))
        .unwrap_or(Category::All);
    Saved { rect, topmost: v.as_ref().and_then(|v| v.bool("always_on_top")).unwrap_or(false), category }
}

fn save_state(h: HWND) {
    let Some((topmost, category, collapsed_from)) = with(|w| (w.model.topmost, w.model.category, w.expanded_h)) else { return };
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        if GetWindowRect(h, &mut r) == 0 || IsIconic(h) != 0 {
            return;
        }
        let dpi = ui_core::dpi::window_dpi(h).max(96) as i64;
        let un = |v: i32| (v as i64 * 96 / dpi) as i32;
        let height = collapsed_from.unwrap_or(r.bottom - r.top);
        let v = Value::obj(vec![
            ("x", (un(r.left) as i64).into()),
            ("y", (un(r.top) as i64).into()),
            ("width", (un(r.right - r.left) as i64).into()),
            ("height", (un(height) as i64).into()),
            ("always_on_top", topmost.into()),
            ("category", category.label().into()),
        ]);
        let path = state_path();
        let tmp = format!("{path}.tmp");
        if std::fs::write(&tmp, json::to_string(&v)).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

// ---- 열기·닫기 ----

fn create() -> HWND {
    let existing = hwnd();
    if !existing.is_null() {
        return existing;
    }
    let _ = ui_core::window::register_class(CLASS, Some(wndproc), true);
    let Ok(h) = ui_core::window::create(ui_core::window::Kind::Normal, CLASS, TITLE, Rect::xywh(0, 0, DEFAULT_W, DEFAULT_H),
                                         std::ptr::null_mut()) else {
        return std::ptr::null_mut();
    };
    let saved = load_saved();
    let mut model = Model::default();
    model.topmost = saved.topmost;
    model.category = saved.category;
    let c = crate::look::app_colors();
    let win = Win {
        hwnd: h, model, memo: LineEdit::new(2000), search: LineEdit::new(100), focus: Focus::List, hot: Vec::new(),
        hover: None, pressed: None, restore_rect: None, expanded_h: None, detail_image: None, scroll: None, drag: None,
        wheel: Wheel::default(), settings_top: 0, edit: std::ptr::null_mut(), label: std::ptr::null_mut(),
        edit_colors: EditColors::new(c.text.bgr(), c.surface.bgr()), thumbs_asked: HashSet::new(), quiet: false,
        tracking: false,
    };
    WIN.with(|w| *w.borrow_mut() = Some(win));
    unsafe {
        // 상세 글: 읽기 전용 OS EDIT(선택·복사·접근성). 앞에 둔 '내용' 글이 UIA 이름이 된다.
        let instance = GetModuleHandleW(std::ptr::null());
        let label = CreateWindowExW(0, wide("STATIC").as_ptr(), wide("내용").as_ptr(), WS_CHILD, 0, 0, 10, 10, h,
                                    ID_LABEL as HMENU, instance, std::ptr::null());
        let edit = CreateWindowExW(WS_EX_CLIENTEDGE, wide("EDIT").as_ptr(), std::ptr::null(),
                                   WS_CHILD | WS_TABSTOP | WS_VSCROLL | (ES_MULTILINE | ES_AUTOVSCROLL | ES_READONLY) as u32,
                                   0, 0, 10, 10, h, ID_EDIT as HMENU, instance, std::ptr::null());
        with(|w| {
            w.edit = edit;
            w.label = label;
        });
        crate::look::frame_for(h);
        // 끌어 놓기(파일·폴더·그림·웹 링크).
        windows_sys::Win32::System::Ole::OleInitialize(std::ptr::null());
        let target = crate::drop_target::new(h, WM_DROPPED);
        windows_sys::Win32::System::Ole::RegisterDragDrop(h, target);
        // RegisterDragDrop이 참조를 하나 더 잡는다(실패하면 여기서 없어진다).
        crate::drop_target::release(target);
        place(h, saved.rect);
    }
    h
}

fn place(h: HWND, saved: Option<[i32; 4]>) {
    unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        let probe = saved.map(|r| POINT { x: r[0] + 20, y: r[1] + 20 }).unwrap_or(pt);
        SetWindowPos(h, std::ptr::null_mut(), probe.x, probe.y, 1, 1, SWP_NOZORDER | SWP_NOACTIVATE);
        let dpi = ui_core::dpi::window_dpi(h).max(96) as i32;
        let sc = |v: i32| v * dpi / 96;
        let work = ui_core::window::work_area_at(probe);
        let saved_px = saved.map(|r| [sc(r[0]), sc(r[1]), sc(r[2]), sc(r[3])]);
        let centre = [(work.left + work.right) / 2 - sc(DEFAULT_W) / 2, (work.top + work.bottom) / 2 - sc(DEFAULT_H) / 2];
        let r = ui_core::window::place(saved_px.or(Some([centre[0], centre[1], sc(DEFAULT_W), sc(DEFAULT_H)])), [pt.x, pt.y],
                                       work, 0, [sc(DEFAULT_W), sc(DEFAULT_H)], [sc(MIN_W), sc(MIN_H)]);
        SetWindowPos(h, std::ptr::null_mut(), r[0], r[1], r[2], r[3], SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

/// 관리창 열기(트레이·빠른 목록 설정 메뉴·앱 파이프 `manage_show`). 열려 있으면 앞으로 가져오고 다시 읽는다.
pub fn show(mode: OpenMode) {
    let h = create();
    if h.is_null() {
        return;
    }
    let topmost = with(|w| {
        w.memo.clear();
        w.search.clear();
        w.detail_image = None;
        w.settings_top = 0;
        w.focus = if mode == OpenMode::Hotkey { Focus::Memo } else { Focus::List };
        w.model.topmost
    })
    .unwrap_or(false);
    dispatch(Event::Open(mode));
    unsafe {
        if IsIconic(h) != 0 {
            ShowWindow(h, SW_RESTORE);
        }
        ui_core::window::set_topmost(h, topmost);
        ShowWindow(h, SW_SHOW);
        SetForegroundWindow(h);
        SetFocus(h);
        sync_children(h);
        InvalidateRect(h, std::ptr::null(), 0);
    }
}

/// 저장소·설정이 다른 곳에서 바뀌면(보이는 동안만) 다시 읽는다.
pub fn refresh() {
    let h = hwnd();
    if !h.is_null() && unsafe { IsWindowVisible(h) } != 0 {
        dispatch(Event::External);
    }
}

/// 테마가 바뀌면 다시 그린다.
pub fn theme_changed() {
    let h = hwnd();
    if h.is_null() {
        return;
    }
    let c = crate::look::app_colors();
    with(|w| w.edit_colors.set(c.text.bgr(), c.surface.bgr()));
    unsafe {
        crate::look::frame_for(h);
        InvalidateRect(h, std::ptr::null(), 1);
    }
}

fn close(h: HWND) {
    crate::preview::hide();
    save_state(h);
    unsafe {
        KillTimer(h, TIMER_DEBOUNCE);
        ShowWindow(h, SW_HIDE);
    }
}

// ---- 상태 전이와 효과 ----

/// 사건 하나를 상태 기계에 넣고 효과를 실행한다(빌린 상태를 놓은 뒤 실행 — 메시지 재진입 안전).
fn dispatch(ev: Event) {
    let effects = with(|w| ms::step(&mut w.model, ev)).unwrap_or_default();
    for e in effects {
        run(e);
    }
    let h = hwnd();
    if !h.is_null() {
        unsafe {
            sync_children(h);
            InvalidateRect(h, std::ptr::null(), 0);
        }
    }
}

fn post_box<T>(h: HWND, msg: u32, wp: usize, value: T) {
    let p = Box::into_raw(Box::new(value)) as isize;
    if unsafe { PostMessageW(h, msg, wp, p) } == 0 {
        drop(unsafe { Box::from_raw(p as *mut T) });
    }
}

unsafe fn unbox<T>(lp: LPARAM) -> T {
    *unsafe { Box::from_raw(lp as *mut T) }
}

fn spawn(name: &str, f: impl FnOnce() + Send + 'static) {
    let _ = std::thread::Builder::new().name(name.into()).stack_size(512 * 1024).spawn(f);
}

/// 작업 결과(창 스레드로 보냄).
struct Outcome {
    ok: bool,
    message: String,
    open_url: Option<String>,
}

fn run(e: Effect) {
    let h = hwnd();
    if h.is_null() {
        return;
    }
    let hs = h as isize;
    match e {
        Effect::Load { seq, query } => spawn("kagent-manage-load", move || {
            let r = snapshot(&query);
            post_box(hs as HWND, WM_LOADED, seq as usize, r.map(Box::new));
        }),
        Effect::Debounce => unsafe {
            SetTimer(h, TIMER_DEBOUNCE, ms::DEBOUNCE_MS, None);
        },
        Effect::Run(job) => run_job(h, job),
        Effect::Confirm(c) => {
            let yes = unsafe {
                MessageBoxW(h, wide(&c.text()).as_ptr(), wide("몽땅클립보드").as_ptr(),
                            MB_YESNO | if c == Confirm::Quit { MB_ICONQUESTION | MB_DEFBUTTON2 } else { MB_ICONQUESTION })
                    == IDYES
            };
            dispatch(Event::Answer(yes));
        }
        Effect::LoadDetail { id, kind } => {
            with(|w| w.detail_image = None);
            spawn("kagent-manage-detail", move || {
                let r = load_detail(id, &kind);
                post_box(hs as HWND, WM_DETAIL, id as usize, r);
            })
        }
        Effect::CheckTable(id) => spawn("kagent-manage-table", move || {
            let table = is_table(id);
            unsafe { PostMessageW(hs as HWND, WM_TABLE, id as usize, table as isize) };
        }),
        Effect::Topmost(on) => ui_core::window::set_topmost(h, on),
        Effect::Collapse(on) => collapse(h, on),
        Effect::Maximize(_) => toggle_max(h),
        Effect::ClearSearch => {
            with(|w| w.search.clear());
        }
        Effect::ClearMemo => {
            with(|w| w.memo.clear());
        }
        Effect::Save => save_state(h),
    }
}

/// 목록·설정·기록 상태 한 번 읽기(작업 스레드).
fn snapshot(query: &str) -> Result<Snapshot, String> {
    let a = app();
    let items = a.store.lock().unwrap().recent(ms::LIST_LIMIT, (!query.is_empty()).then_some(query))?;
    let count = a.store.lock().unwrap().count().unwrap_or(items.len() as i64);
    let settings = a.settings.lock().unwrap().clone();
    let capture_dirs = a.capture_plan.lock().unwrap().iter().map(|(c, s)| ms::CaptureDir {
        path: c.path.clone(), label: c.label.to_string(), state: s.code().to_string(),
    }).collect();
    Ok(Snapshot {
        items, settings, capture_dirs, autostart: crate::autostart_enabled(), count,
        hotkey_ok: a.hotkey_ok.load(std::sync::atomic::Ordering::Relaxed), resume_in_min: crate::resume_in_minutes(),
    })
}

/// 상세 내용(작업 스레드). 그림은 긴 변 1600px까지 풀어 둔다.
fn load_detail(id: i64, kind: &str) -> Result<(DetailBody, Option<Thumb>), String> {
    let payload = app().store.lock().unwrap().payload(id)?;
    let Some((k, data, _)) = payload else { return Err("항목이 없습니다.".into()) };
    if k == "image" || kind == "image" {
        let t = crate::thumb::from_png(&data, 1600).ok_or_else(|| "그림을 불러오지 못했습니다.".to_string())?;
        return Ok((DetailBody::Image { w: t.w, h: t.h }, Some(t)));
    }
    let mut text = String::from_utf8_lossy(&data).replace('\0', "\n");
    if text.len() > 512 * 1024 {
        let mut cut = 512 * 1024;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("\n…(512KB까지 보입니다. 다시 복사하면 전체가 올라갑니다)");
    }
    Ok((DetailBody::Text(text.replace("\r\n", "\n").replace('\n', "\r\n")), None))
}

/// 엑셀 표 항목인지(HTML 서식 + 탭 구분 글) — 앱 `KClipService._table_item`과 같은 판정.
fn is_table(id: i64) -> bool {
    let Ok(Some((kind, data, extra))) = app().store.lock().unwrap().payload(id) else { return false };
    kind == "text"
        && crate::clip::unpack_formats(&extra).iter().any(|(n, _)| n.eq_ignore_ascii_case("html format"))
        && data.contains(&b'\t')
}

fn pipe(v: Value) -> Outcome {
    let r = crate::pipe::handle(&v);
    Outcome {
        ok: r.bool("ok") != Some(false),
        message: r.str("message").unwrap_or("").to_string(),
        open_url: None,
    }
}

const APP_OFF: &str = "이 기능은 문서 앱(HWP AutoDocFit)이 맡습니다. 앱을 켠 뒤 다시 눌러 주세요.";

/// 앱 서비스 파이프에 일을 맡긴다(작업 스레드, 최대 60초).
fn app_service(service: &str, action: &str, payload: Vec<(&str, Value)>) -> Option<Value> {
    let mut fields = vec![("cmd", "app".into()), ("service", service.into()), ("action", action.into())];
    fields.push(("payload", Value::obj(payload)));
    let mut line = json::to_string(&Value::obj(fields));
    line.push('\n');
    let r = crate::xl_target::call_with_timeout(crate::xl_target::pipe_name(), line, 60_000)?;
    json::parse(r.trim_end())
}

/// 엑셀 표 결과 글(Flutter `_convertExcel`과 같은 글).
fn excel_message(r: &Value) -> (bool, String) {
    if r.bool("ok") == Some(true) {
        let cleaned = r.get("cleaned");
        let n = |k: &str| cleaned.and_then(|c| c.num(k)).unwrap_or(0.0) as i64;
        let rows = r.num("rows").unwrap_or(0.0) as i64;
        let cols = r.num("cols").unwrap_or(0.0) as i64;
        (true, format!("{rows}행 × {cols}열 표 구조·내용 유지, 배경 음영 제거 · 공백 {}, 기호 {} 정리. 한/글에서 Ctrl+V",
                       n("spaces"), n("symbols")))
    } else {
        (false, r.str("message").unwrap_or("클립보드에서 엑셀 표를 찾지 못했습니다.").to_string())
    }
}

fn web_url(id: i64) -> Result<String, String> {
    let payload = app().store.lock().unwrap().payload(id)?;
    let Some((_, data, _)) = payload else { return Err("웹 링크 항목을 확인해 주세요.".into()) };
    let url = String::from_utf8_lossy(&data).trim().to_string();
    let lower = url.to_ascii_lowercase();
    let rest = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://"));
    match rest {
        Some(r) if !r.is_empty() && !r.starts_with('/') && !url.contains(char::is_whitespace) => Ok(url),
        _ => Err("HTTP 또는 HTTPS 웹 주소가 아닙니다.".into()),
    }
}

fn run_job(h: HWND, job: Job) {
    let hs = h as isize;
    let done_now = |ok: bool, message: String| post_box(h, WM_DONE, 0, Outcome { ok, message, open_url: None });
    match job {
        // 창 스레드에서 바로(빠른 목록 설정 메뉴와 같은 길).
        Job::Pause(m) => {
            crate::pause_for(m.unwrap_or(0));
            done_now(true, String::new());
        }
        Job::Resume => {
            crate::set_paused(main_hwnd(), false);
            done_now(true, String::new());
        }
        Job::Autostart(on) => match crate::set_autostart(on) {
            Ok(()) => done_now(true, String::new()),
            Err(e) => done_now(false, format!("자동 시작 설정을 바꾸지 못했습니다: {e}")),
        },
        Job::Quit => {
            close(h);
            unsafe { PostMessageW(main_hwnd(), WM_CLOSE, 0, 0) };
            done_now(true, String::new());
        }
        job => spawn("kagent-manage-job", move || {
            let out = match job {
                Job::Restore { id } => pipe(Value::obj(vec![("cmd", "restore".into()), ("id", id.into())])),
                Job::Pin { id, pinned } => pipe(Value::obj(vec![("cmd", "pin".into()), ("id", id.into()), ("pinned", pinned.into())])),
                Job::Delete { id } => pipe(Value::obj(vec![("cmd", "delete".into()), ("id", id.into())])),
                Job::Clear => pipe(Value::obj(vec![("cmd", "clear".into()), ("keep_pinned", true.into())])),
                Job::Settings(patch) => pipe(Value::obj(vec![("cmd", "settings_set".into()), ("settings", patch)])),
                Job::AddMemo(text) => pipe(Value::obj(vec![("cmd", "add_text".into()), ("text", text.into())])),
                Job::AddFiles(paths) => {
                    let n = paths.len();
                    let mut o = pipe(Value::obj(vec![("cmd", "add_files".into()),
                                                    ("paths", Value::Arr(paths.into_iter().map(Value::Str).collect()))]));
                    if o.ok && o.message.is_empty() {
                        o.message = format!("끌어 놓은 {n}개를 클립보드 항목에 추가했습니다.");
                    }
                    o
                }
                Job::ExcelTable => match app_service("excel_table", "paste_clean", vec![]) {
                    Some(r) => {
                        let (ok, message) = excel_message(&r);
                        Outcome { ok, message, open_url: None }
                    }
                    None => Outcome { ok: false, message: APP_OFF.into(), open_url: None },
                },
                Job::CopyNoShade { id } => match app_service("kclip", "copy_no_shade", vec![("id", id.into())]) {
                    Some(r) => Outcome {
                        ok: r.bool("ok") != Some(false),
                        message: r.str("message").unwrap_or("음영 없이 복사하지 못했습니다.").to_string(),
                        open_url: None,
                    },
                    None => Outcome { ok: false, message: APP_OFF.into(), open_url: None },
                },
                Job::OpenWeb { id } => match web_url(id) {
                    Ok(url) => Outcome { ok: true, message: "기본 브라우저에서 웹 링크를 열었습니다.".into(), open_url: Some(url) },
                    Err(e) => Outcome { ok: false, message: e, open_url: None },
                },
                Job::Pause(_) | Job::Resume | Job::Autostart(_) | Job::Quit => unreachable!(),
            };
            post_box(hs as HWND, WM_DONE, 0, out);
        }),
    }
}

// ---- 창 모양 ----

fn collapse(h: HWND, on: bool) {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(h, &mut r);
        if on {
            with(|w| w.expanded_h = Some(r.bottom - r.top));
            SetWindowPos(h, std::ptr::null_mut(), 0, 0, r.right - r.left, scale(h, HEAD_H) + 2,
                         SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
        } else {
            let back = with(|w| w.expanded_h.take()).flatten().unwrap_or(scale(h, DEFAULT_H));
            SetWindowPos(h, std::ptr::null_mut(), 0, 0, r.right - r.left, back, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }
}

fn toggle_max(h: HWND) {
    let prev = with(|w| w.restore_rect.take()).flatten();
    unsafe {
        if let Some(r) = prev {
            SetWindowPos(h, std::ptr::null_mut(), r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
        } else {
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(h, &mut r);
            with(|w| w.restore_rect = Some(r));
            let wa = ui_core::window::work_area_of(h);
            SetWindowPos(h, std::ptr::null_mut(), wa.left, wa.top, wa.width(), wa.height(), SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }
}

/// 상세 글 EDIT·'내용' 글 위치와 보이기(상태에 맞춤).
unsafe fn sync_children(h: HWND) {
    let Some((edit, label, text, area)) = with(|w| {
        let text = match &w.model.detail {
            Detail::Ready { body: DetailBody::Text(t), .. } if !w.model.collapsed => Some(t.clone()),
            _ => None,
        };
        (w.edit, w.label, text, detail_layout(h, w).1)
    }) else { return };
    unsafe {
        match text {
            Some(t) => {
                let lab = Rect::new(area.left, area.top, area.right, area.top + scale(h, 20));
                let body = Rect::new(area.left, lab.bottom + scale(h, 2), area.right, area.bottom);
                MoveWindow(label, lab.left, lab.top, lab.width(), lab.height(), 1);
                MoveWindow(edit, body.left, body.top, body.width(), body.height(), 1);
                let font = FONTS.with(|c| c.borrow_mut().get(F_BODY, ui_core::dpi::window_dpi(h)));
                SendMessageW(edit, WM_SETFONT, font as usize, 0);
                SendMessageW(label, WM_SETFONT, font as usize, 0);
                if ui_core::native_edit::text(edit) != t {
                    ui_core::native_edit::set_text(edit, &t);
                }
                ShowWindow(label, SW_SHOWNA);
                ShowWindow(edit, SW_SHOWNA);
            }
            None => {
                ShowWindow(edit, SW_HIDE);
                ShowWindow(label, SW_HIDE);
            }
        }
    }
}

// ---- 배치(그리기·판정 공통) ----

struct Lay {
    w: i32,
    h: i32,
    head: i32,
    s: Box<dyn Fn(i32) -> i32>,
}

fn lay(h: HWND) -> Lay {
    let mut rc: RECT = unsafe { std::mem::zeroed() };
    unsafe { GetClientRect(h, &mut rc) };
    let hs = h as isize;
    Lay { w: rc.right, h: rc.bottom, head: scale(h, HEAD_H), s: Box::new(move |v| scale(hs as HWND, v)) }
}

/// 상세 보기 영역: (전체 판, 내용 칸, 단추 줄).
fn detail_layout(h: HWND, w: &Win) -> (Rect, Rect, Rect) {
    let l = lay(h);
    let s = &l.s;
    let top = l.head + status_height(h, w);
    let pane = Rect::new(0, top, l.w, l.h);
    let inner = Rect::new(s(PAD), top + s(PAD) + s(24), l.w - s(PAD), l.h - s(PAD) - s(CTRL_H) - s(GAP));
    let buttons = Rect::new(s(PAD), l.h - s(PAD) - s(CTRL_H), l.w - s(PAD), l.h - s(PAD));
    (pane, inner, buttons)
}

/// 왼쪽부터 줄을 바꿔 가며 놓기(Flutter Wrap). 돌려주는 값: 사각형들, 쓴 높이.
fn flow(x0: i32, y0: i32, right: i32, widths: &[i32], h: i32, gap: i32) -> (Vec<Rect>, i32) {
    let mut x = x0;
    let mut y = y0;
    let mut out = Vec::new();
    for w in widths {
        if x > x0 && x + w > right {
            x = x0;
            y += h + gap;
        }
        out.push(Rect::xywh(x, y, *w, h));
        x += w + gap;
    }
    (out, y + h - y0)
}

fn status_items(m: &Model) -> Vec<(Option<StatusBtn>, String)> {
    vec![
        (None, format!("{} {}", if m.paused() { "⏸" } else { "●" }, m.status_text())),
        (None, m.hotkey_text()),
        (Some(StatusBtn::PauseToggle), if m.paused() { "다시 기록".into() } else { "일시정지".into() }),
        (Some(StatusBtn::PauseMenu), "일시정지 시간 ▾".into()),
        (Some(StatusBtn::Clear), "모두 지우기".into()),
        (Some(StatusBtn::Quit), "끄기".into()),
    ]
}

fn measure(h: HWND, f: FontSpec, s: &str) -> i32 {
    unsafe {
        let dc = GetDC(h);
        let font = FONTS.with(|c| c.borrow_mut().get(f, ui_core::dpi::window_dpi(h)));
        let old = SelectObject(dc, font);
        let w = ui_core::frame::text_width(dc, s);
        SelectObject(dc, old);
        ReleaseDC(h, dc);
        w
    }
}

fn status_rects(h: HWND, w: &Win) -> (Vec<Rect>, i32) {
    let l = lay(h);
    let s = &l.s;
    let widths: Vec<i32> = status_items(&w.model).iter().map(|(b, t)| measure(h, F_SMALL, t) + s(if b.is_some() { 20 } else { 8 })).collect();
    flow(s(PAD), l.head + s(GAP), l.w - s(PAD), &widths, s(CTRL_H), s(GAP))
}

fn status_height(h: HWND, w: &Win) -> i32 {
    if w.model.collapsed {
        return 0;
    }
    status_rects(h, w).1 + lay(h).s.as_ref()(GAP * 2)
}

// ---- 그리기 ----

fn glyph_for(item: &Summary) -> char {
    match item.kind.as_str() {
        "image" => '\u{EB9F}',
        "files" => '\u{E8B7}',
        _ => '\u{E8A5}',
    }
}

unsafe fn draw_wrapped(p: &Painter, s: &str, r: Rect) {
    let w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    let mut rc = r.win();
    unsafe { DrawTextW(p.dc, w.as_ptr(), -1, &mut rc, DT_WORDBREAK | DT_EDITCONTROL | DT_END_ELLIPSIS | DT_NOPREFIX) };
}

unsafe fn draw_thumb(dc: HDC, t: &Thumb, area: Rect) {
    let fit = ui_core::preview::fit(t.w as i32, t.h as i32, area, false);
    unsafe {
        let mut bi: BITMAPINFO = std::mem::zeroed();
        bi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: t.w as i32, biHeight: -(t.h as i32), biPlanes: 1,
            biBitCount: 32, biCompression: BI_RGB, ..std::mem::zeroed()
        };
        SetStretchBltMode(dc, HALFTONE);
        StretchDIBits(dc, fit.left, fit.top, fit.width(), fit.height(), 0, 0, t.w as i32, t.h as i32,
                      t.bgra.as_ptr() as _, &bi, DIB_RGB_COLORS, SRCCOPY);
    }
}

fn head_glyph(b: HeadBtn, m: &Model) -> char {
    match b {
        HeadBtn::Top => if m.topmost { '\u{E840}' } else { '\u{E718}' },
        HeadBtn::Collapse => if m.collapsed { '\u{E70D}' } else { '\u{E921}' },
        HeadBtn::Max => if m.maximized { '\u{E923}' } else { '\u{E922}' },
        HeadBtn::Close => '\u{E8BB}',
    }
}

fn head_name(b: HeadBtn, m: &Model) -> &'static str {
    match b {
        HeadBtn::Top if m.topmost => "항상 위 해제",
        HeadBtn::Top => "항상 위",
        HeadBtn::Collapse if m.collapsed => "창 펼치기",
        HeadBtn::Collapse => "접기(머리글만 남김)",
        HeadBtn::Max if m.maximized => "이전 크기로 복원",
        HeadBtn::Max => "최대화",
        HeadBtn::Close => "닫기",
    }
}

fn setting_label(m: &Model, s: Setting) -> String {
    match s {
        Setting::Autostart => match m.autostart {
            Some(_) => "Windows 로그인 시 자동 실행".into(),
            None => "Windows 로그인 시 자동 실행(확인 못 함)".into(),
        },
        Setting::CaptureDir(i) => m.capture_dirs.get(i).map(|d| format!("{} · {}", d.label, d.state_text())).unwrap_or_default(),
        _ => ms::flag_spec(s, &m.settings).map(|f| f.1.to_string())
            .or_else(|| ms::number_spec(s, &m.settings).map(|n| n.1.to_string()))
            .unwrap_or_default(),
    }
}

fn setting_checked(m: &Model, s: Setting) -> Option<bool> {
    match s {
        Setting::Autostart => Some(m.autostart == Some(true)),
        Setting::CaptureDir(i) => Some(ms::capture_dir_on(m, i)),
        _ => ms::flag_spec(s, &m.settings).map(|f| f.2),
    }
}

fn setting_enabled(m: &Model, s: Setting) -> bool {
    m.busy.is_none()
        && match s {
            Setting::Autostart => m.autostart.is_some(),
            Setting::CaptureDir(_) => m.settings.watch_screenshots,
            _ => true,
        }
}

unsafe fn paint(h: HWND) {
    unsafe {
        let Some(mut buf) = BackBuffer::begin(h) else { return };
        let dpi = ui_core::dpi::window_dpi(h);
        let [body, small, bold, glyph, icon] =
            FONTS.with(|c| { let mut c = c.borrow_mut(); [F_BODY, F_SMALL, F_BOLD, F_GLYPH, F_ICON].map(|f| c.get(f, dpi)) });
        if [body, small, bold, glyph, icon].iter().any(|f| f.is_null()) {
            return;
        }
        buf.select_font(body);
        let p = buf.painter();
        p.transparent();
        let pal = crate::look::clip_palette();
        let c = crate::look::app_colors();
        let mut nodes = Vec::new();
        let mut hot = Vec::new();
        let mut want_thumbs = Vec::new();
        WIN.with(|cell| {
            let mut g = cell.borrow_mut();
            let Some(w) = g.as_mut() else { return };
            draw_all(h, w, &p, &pal, &c, [body, small, bold, glyph, icon], &mut nodes, &mut hot, &mut want_thumbs);
            w.hot = hot;
        });
        ui_core::frame::own_border_with(buf.dc(), buf.rect(), pal.border);
        buf.finish();
        a11y::publish(h, nodes);
        request_thumbs(h, want_thumbs);
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn draw_all(h: HWND, w: &mut Win, p: &Painter, pal: &ClipPalette, c: &AppColors, fonts: [HFONT; 5],
                   nodes: &mut Vec<Node>, hot: &mut Vec<(Rect, Hit)>, want_thumbs: &mut Vec<(i64, i64)>) {
    let [body, small, bold, glyph, icon] = fonts;
    let l = lay(h);
    let s = &l.s;
    let m = &w.model;
    let busy = m.busy.is_some();
    p.fill_rect(Rect::new(0, 0, l.w, l.h), pal.window);
    let (hover, pressed) = (w.hover, w.pressed);
    let vis = move |hit: Hit| Visual {
        hover: hover == Some(hit),
        pressed: pressed == Some(hit) && hover == Some(hit),
        focused: false,
        disabled: false,
        on: false,
    };
    let mut button = |r: Rect, hit: Hit, label: &str, kind: ButtonKind, disabled: bool, nodes: &mut Vec<Node>, hot: &mut Vec<(Rect, Hit)>| {
        p.font(small);
        draw_button(p, r, label, kind, Visual { disabled, ..vis(hit) }, c);
        let mut n = Node::button(hit.id(), 0, label.trim_end_matches(" ▾"), r);
        n.enabled = !disabled;
        nodes.push(n);
        hot.push((r, hit));
    };

    // ① 머리글
    p.fill_rect(Rect::new(0, 0, l.w, l.head), pal.head);
    let bw = s(CAPTION_W);
    for (i, b) in HEAD_BTNS.iter().enumerate() {
        let r = Rect::new(l.w - (HEAD_BTNS.len() - i) as i32 * bw, 0, l.w - (HEAD_BTNS.len() - i - 1) as i32 * bw, l.head);
        let hit = Hit::Head(*b);
        let hovered = w.hover == Some(hit);
        let on = *b == HeadBtn::Top && m.topmost;
        if hovered {
            p.fill_rect(r, if *b == HeadBtn::Close { pal.close_hover } else if w.pressed == Some(hit) { pal.head_pressed } else { pal.head_hover });
        } else if on {
            p.fill_rect(r, pal.head_on);
        }
        p.text_color(if hovered && *b == HeadBtn::Close { pal.on_close_hover } else if on { pal.accent } else { pal.glyph });
        p.font(if *b == HeadBtn::Top { icon } else { glyph });
        p.text_rect(&head_glyph(*b, m).to_string(), r, DT_CENTER);
        nodes.push(Node::button(hit.id(), 0, head_name(*b, m), r));
        hot.push((r, hit));
    }
    p.font(bold);
    p.text_color(pal.title);
    p.text_rect(TITLE, Rect::new(s(10), 0, l.w - bw * 4 - s(4), l.head), DT_END_ELLIPSIS);
    if m.collapsed {
        return;
    }

    // ② 기록 상태 줄
    let (rects, used) = status_rects(h, w);
    for ((btn, text), r) in status_items(m).into_iter().zip(rects) {
        match btn {
            Some(b) => {
                button(r, Hit::Status(b), &text, ButtonKind::Secondary, busy, nodes, hot);
            }
            None => {
                let is_status = text.starts_with('●') || text.starts_with('⏸');
                p.fill_rect(r, c.surface_subtle.bgr());
                p.frame(r.win(), c.line.bgr());
                p.font(small);
                p.text_color(if is_status && !m.paused() { c.danger.bgr() } else { c.text.bgr() });
                p.text_rect(&text, r, DT_CENTER | DT_END_ELLIPSIS);
                let id = if is_status { A11Y_STATUS } else { A11Y_HOTKEY };
                let name = if is_status { m.status_text() } else { format!("단축키 {}", m.hotkey_text()) };
                nodes.push(Node::new(id, 0, Role::Text, name, r));
            }
        }
    }
    let mut y = l.head + used + s(GAP * 2);
    p.fill_rect(Rect::new(0, y - 1, l.w, y), pal.tab_line);

    // 상세 보기(목록·설정 위를 덮음)
    if m.detail.is_open() {
        unsafe {
            draw_detail(h, w, p, c, [body, small, bold], nodes, hot, &mut button);
            draw_message(h, w, p, pal, small, nodes);
        }
        return;
    }

    // ③ 목록 | 설정
    let sec_h = s(SECTION_H);
    p.fill_rect(Rect::new(0, y, l.w, y + sec_h), pal.tab_bar);
    p.fill_rect(Rect::new(0, y + sec_h - 1, l.w, y + sec_h), pal.tab_line);
    nodes.push(Node::new(A11Y_SECTIONS, 0, Role::Tab, "화면", Rect::new(0, y, l.w, y + sec_h)));
    let mut x = s(PAD);
    for (sec, label) in [(Section::List, "목록"), (Section::Settings, "설정")] {
        p.font(bold);
        let tw = measure(h, F_BOLD, label) + s(28);
        let r = Rect::new(x, y, x + tw, y + sec_h - 1);
        let hit = Hit::Section(sec);
        let selected = m.section == sec;
        if selected {
            p.fill_rect(r, pal.window);
            p.fill_rect(Rect::new(r.left + s(3), r.bottom - s(2), r.right - s(3), r.bottom), pal.accent);
        } else if w.hover == Some(hit) {
            p.fill_rect(r, pal.tab_hover);
        }
        p.font(if selected { bold } else { small });
        p.text_color(if selected { pal.accent } else { pal.tab_text });
        p.text_rect(label, r, DT_CENTER);
        nodes.push(Node::item(hit.id(), A11Y_SECTIONS, Role::TabItem, label, r, selected));
        hot.push((r, hit));
        x = r.right;
    }
    y += sec_h + s(GAP);
    let bottom = l.h - s(MSG_H);
    unsafe {
        match m.section {
            Section::List => draw_list_section(h, w, p, pal, c, [body, small, bold, glyph, icon], y, bottom, nodes, hot, want_thumbs, &mut button),
            Section::Settings => draw_settings(h, w, p, pal, c, small, y, bottom, nodes, hot, &mut button),
        }
        draw_message(h, w, p, pal, small, nodes);
    }
}

type ButtonFn<'a> = dyn FnMut(Rect, Hit, &str, ButtonKind, bool, &mut Vec<Node>, &mut Vec<(Rect, Hit)>) + 'a;

unsafe fn draw_message(h: HWND, w: &Win, p: &Painter, pal: &ClipPalette, small: HFONT, nodes: &mut Vec<Node>) {
    let l = lay(h);
    let s = &l.s;
    let m = &w.model;
    let text = if m.busy.is_some() {
        "처리 중…".to_string()
    } else if let Load::Failed(e) = &m.load {
        format!("목록을 불러오지 못했습니다: {e}")
    } else {
        m.message.clone()
    };
    if text.is_empty() {
        return;
    }
    let r = Rect::new(s(PAD), l.h - s(MSG_H), l.w - s(PAD), l.h - s(2));
    p.font(small);
    p.text_color(pal.title);
    p.text_rect(&text, r, DT_END_ELLIPSIS);
    nodes.push(Node::new(A11Y_MESSAGE, 0, Role::Text, text, r));
}

#[allow(clippy::too_many_arguments)]
unsafe fn draw_detail(h: HWND, w: &Win, p: &Painter, c: &AppColors, fonts: [HFONT; 3], nodes: &mut Vec<Node>,
                      hot: &mut Vec<(Rect, Hit)>, button: &mut ButtonFn) {
    let [body, _small, bold] = fonts;
    let l = lay(h);
    let s = &l.s;
    let (pane, inner, buttons) = detail_layout(h, w);
    let m = &w.model;
    p.fill_rect(pane, c.surface.bgr());
    let kind = match &m.detail {
        Detail::Loading { kind, .. } | Detail::Ready { kind, .. } | Detail::Failed { kind, .. } => kind.clone(),
        Detail::Closed => String::new(),
    };
    let title = ms::kind_label(&kind);
    p.font(bold);
    p.text_color(c.text_strong.bgr());
    p.text_rect(title, Rect::new(s(PAD), pane.top + s(PAD) - s(4), l.w - s(PAD), pane.top + s(PAD) + s(20)), 0);
    nodes.push(Node::new(A11Y_DETAIL, 0, Role::Group, format!("미리보기: {title}"), pane));
    p.font(body);
    p.text_color(c.text_body.bgr());
    match &m.detail {
        Detail::Loading { .. } => p.text_rect("불러오는 중…", inner, DT_CENTER),
        Detail::Failed { kind, .. } => {
            let t = if kind == "image" { "그림을 불러오지 못했습니다." } else { "내용을 불러오지 못했습니다." };
            p.text_rect(t, inner, DT_CENTER);
            nodes.push(Node::new(A11Y_DETAIL + 1000, A11Y_DETAIL, Role::Text, t, inner));
        }
        Detail::Ready { body: DetailBody::Image { w: iw, h: ih }, .. } => {
            p.frame(inner.win(), c.line.bgr());
            if let Some(t) = &w.detail_image {
                unsafe { draw_thumb(p.dc, t, Rect::new(inner.left + 1, inner.top + 1, inner.right - 1, inner.bottom - 1)) };
            }
            nodes.push(Node::new(A11Y_DETAIL + 1000, A11Y_DETAIL, Role::Image, format!("그림 {iw}×{ih}"), inner));
        }
        // 글은 OS EDIT(sync_children)가 그린다.
        Detail::Ready { .. } | Detail::Closed => {}
    }
    let bw = s(96);
    let recopy = Rect::new(buttons.right - bw, buttons.top, buttons.right, buttons.bottom);
    let close = Rect::new(recopy.left - s(GAP) - bw, buttons.top, recopy.left - s(GAP), buttons.bottom);
    let ready = matches!(m.detail, Detail::Ready { .. });
    button(close, Hit::DetailClose, "닫기", ButtonKind::Secondary, false, nodes, hot);
    button(recopy, Hit::DetailRecopy, "다시 복사", ButtonKind::Primary, !ready || m.busy.is_some(), nodes, hot);
}

#[allow(clippy::too_many_arguments)]
unsafe fn draw_line_edit(h: HWND, p: &Painter, c: &AppColors, small: HFONT, r: Rect, e: &LineEdit, focused: bool, placeholder: &str) {
    let s = |v| scale(h, v);
    p.fill_rect(r, c.surface.bgr());
    p.frame(r.win(), if focused { c.accent.bgr() } else { c.line_strong.bgr() });
    p.font(small);
    let tr = Rect::new(r.left + s(8), r.top, r.right - s(8), r.bottom);
    let text = e.needle();
    if text.is_empty() && !focused {
        p.text_color(c.faint.bgr());
        p.text_rect(placeholder, tr, DT_END_ELLIPSIS);
        return;
    }
    if text.is_empty() {
        p.text_color(c.faint.bgr());
        p.text_rect(placeholder, tr, DT_END_ELLIPSIS);
    }
    // 선택 영역 바탕
    let chars: Vec<char> = e.text().chars().collect();
    let width_of = |n: usize| p.text_width(&chars[..n.min(chars.len())].iter().collect::<String>());
    if let Some((a, b)) = e.selection() {
        let x0 = tr.left + width_of(a);
        let x1 = tr.left + width_of(b);
        p.fill_rect(Rect::new(x0, r.top + s(4), x1.min(tr.right), r.bottom - s(4)), c.accent_soft.bgr());
    }
    if !text.is_empty() {
        p.text_color(c.text.bgr());
        p.text_rect(&text, tr, DT_END_ELLIPSIS);
    }
    if focused {
        let before: String = chars[..e.caret()].iter().collect::<String>() + &e.comp;
        let x = (tr.left + p.text_width(&before)).min(tr.right - 1);
        p.fill_rect(Rect::new(x, r.top + s(6), x + 1, r.bottom - s(6)), c.text.bgr());
        unsafe { ui_core::text_input::set_composition_pos(h, x, r.top + s(4)) };
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn draw_list_section(h: HWND, w: &mut Win, p: &Painter, pal: &ClipPalette, c: &AppColors, fonts: [HFONT; 5], mut y: i32,
                            bottom: i32, nodes: &mut Vec<Node>, hot: &mut Vec<(Rect, Hit)>, want: &mut Vec<(i64, i64)>,
                            button: &mut ButtonFn) {
    let [body, small, bold, glyph, icon] = fonts;
    let l = lay(h);
    let s = &l.s;
    let busy = w.model.busy.is_some();
    let ctrl = s(CTRL_H);
    let (left, right) = (s(PAD), l.w - s(PAD));

    // 메모 칸 + 추가
    let add = Rect::new(right - s(64), y, right, y + ctrl);
    let memo = Rect::new(left, y, add.left - s(GAP), y + ctrl);
    unsafe { draw_line_edit(h, p, c, small, memo, &w.memo, w.focus == Focus::Memo, "메모 작성 — 내용을 입력하고 Enter 또는 추가") };
    let mut n = Node::new(Hit::MemoBox.id(), 0, Role::Edit, format!("메모 작성: {}", w.memo.text()), memo);
    n.focused = w.focus == Focus::Memo;
    nodes.push(n);
    hot.push((memo, Hit::MemoBox));
    button(add, Hit::MemoAdd, "추가", ButtonKind::Secondary, busy || w.memo.text().trim().is_empty(), nodes, hot);
    y += ctrl + s(GAP);

    // 끌어 놓기 안내
    let drop = Rect::new(left, y, right, y + s(26));
    p.fill_rect(drop, c.surface_subtle.bgr());
    p.frame(drop.win(), c.line.bgr());
    p.font(small);
    p.text_color(c.text_secondary.bgr());
    let drop_text = "파일·폴더·이미지·웹 링크를 이 창에 끌어 놓으면 항목으로 추가합니다";
    p.text_rect(drop_text, Rect::new(drop.left + s(8), drop.top, drop.right - s(4), drop.bottom), DT_END_ELLIPSIS);
    nodes.push(Node::new(A11Y_DROP, 0, Role::Text, drop_text, drop));
    y = drop.bottom + s(GAP);

    // 엑셀 표 단추
    let ew = measure(h, F_SMALL, "엑셀 표를 한/글 표로") + s(28);
    button(Rect::new(left, y, left + ew, y + ctrl), Hit::Excel, "엑셀 표를 한/글 표로", ButtonKind::Primary, busy, nodes, hot);
    y += ctrl + s(GAP);

    // 분류 + 찾기
    let search_w = s(200);
    let narrow = l.w - 2 * s(PAD) < s(560);
    let cat_right = if narrow { right } else { right - search_w - s(GAP) };
    p.font(small);
    let long: Vec<i32> = CATEGORIES.iter().map(|c| measure(h, F_BOLD, c.label())).collect();
    let short: Vec<i32> = CATEGORIES.iter().map(|c| measure(h, F_BOLD, if *c == Category::Capture { "캡처" } else { c.label() })).collect();
    let (spans, use_short) = tab_layout(&long, &short, cat_right - left, s(8), s(3));
    let cat_h = s(CTRL_H);
    nodes.push(Node::new(A11Y_CATS, 0, Role::Tab, "분류", Rect::new(left, y, cat_right, y + cat_h)));
    for (i, (a, b)) in spans.iter().enumerate() {
        let cat = CATEGORIES[i];
        let r = Rect::new(left + a, y, left + b, y + cat_h);
        let hit = Hit::Category(i);
        let sel = w.model.category == cat;
        if sel {
            p.fill_rect(r, pal.head_on);
        } else if w.hover == Some(hit) {
            p.fill_rect(r, pal.tab_hover);
        }
        p.font(if sel { bold } else { small });
        p.text_color(if sel { pal.accent } else { pal.tab_text });
        let label = if use_short && cat == Category::Capture { "캡처" } else { cat.label() };
        p.text_rect(label, r, DT_CENTER | DT_END_ELLIPSIS);
        nodes.push(Node::item(hit.id(), A11Y_CATS, Role::TabItem, cat.label(), r, sel));
        hot.push((r, hit));
    }
    let sr = if narrow {
        y += cat_h + s(GAP);
        Rect::new(left, y, right, y + ctrl)
    } else {
        Rect::new(right - search_w, y, right, y + ctrl)
    };
    unsafe { draw_line_edit(h, p, c, small, sr, &w.search, w.focus == Focus::Search, "키워드검색") };
    let mut n = Node::new(Hit::SearchBox.id(), 0, Role::Edit, format!("키워드검색: {}", w.search.needle()), sr);
    n.focused = w.focus == Focus::Search;
    nodes.push(n);
    hot.push((sr, Hit::SearchBox));
    y = sr.bottom.max(y + cat_h) + s(GAP);

    // 목록
    let list = Rect::new(0, y, l.w, bottom);
    p.fill_rect(Rect::new(0, y - 1, l.w, y), pal.tab_line);
    nodes.push(Node::new(A11Y_LIST, 0, Role::List, "몽땅클립보드 항목", list));
    let row = s(ROW_H);
    let rows = ((list.height()) / row).max(1) as usize;
    if w.model.rows != rows {
        ms::step(&mut w.model, Event::Rows(rows));
    }
    let m = &w.model;
    let vis = m.visible();
    if vis.is_empty() {
        let text = if !m.loaded_once {
            "불러오는 중…".to_string()
        } else if !m.query.is_empty() {
            "찾는 글자가 들어 있는 항목이 없습니다.".into()
        } else if m.category != Category::All {
            format!("'{}' 분류에 해당하는 항목이 없습니다.", m.category.label())
        } else {
            "기록된 항목이 없습니다.".into()
        };
        let r = Rect::new(left, y, right, y + row);
        p.font(body);
        p.text_color(pal.empty_text);
        p.text_rect(&text, r, DT_CENTER);
        nodes.push(Node::new(A11Y_EMPTY, A11Y_LIST, Role::Text, text, r));
        w.scroll = None;
        return;
    }
    let sb = Scrollbar { len: vis.len(), rows, top: m.top, track: list.height(), min_thumb: s(24) };
    let sbw = if sb.needed() { s(SCROLL_W) } else { 0 };
    let row_right = l.w - sbw;
    for (k, n) in vis.iter().enumerate().skip(m.top).take(rows) {
        let item = &m.items[*n];
        let ry = y + (k - m.top) as i32 * row;
        let r = Rect::new(0, ry, row_right, ry + row);
        let hit = Hit::Row(k);
        if k == m.sel {
            p.fill_rect(r, pal.selected_row);
        } else if w.hover.is_some_and(|hv| matches!(hv, Hit::Row(i) | Hit::RowPin(i) | Hit::RowDelete(i) | Hit::RowMenu(i) if i == k)) {
            p.fill_rect(r, pal.hover_row);
        }
        // 그림 썸네일 또는 종류 아이콘
        let ic = Rect::new(s(PAD), ry + (row - s(THUMB)) / 2, s(PAD) + s(THUMB), ry + (row - s(THUMB)) / 2 + s(THUMB));
        let thumb = crate::preview::target_of(item).and_then(|t| {
            let got = crate::thumb::cached(t.id, t.size);
            if got.is_none() && !w.thumbs_asked.contains(&t.id) {
                want.push((t.id, t.size));
            }
            got
        });
        match thumb {
            Some(t) => unsafe { draw_thumb(p.dc, &t, ic) },
            None => {
                p.font(icon);
                p.text_color(pal.tag);
                p.text_rect(&glyph_for(item).to_string(), ic, DT_CENTER);
            }
        }
        // 오른쪽 단추: (표 메뉴) 고정 지우기
        let mut ax = row_right - s(4);
        let mut actions: Vec<(Hit, char, &str)> = vec![
            (Hit::RowDelete(k), '\u{E74D}', "지우기"),
            (Hit::RowPin(k), if item.pinned { '\u{E840}' } else { '\u{E718}' }, if item.pinned { "고정 풀기" } else { "고정(지워지지 않음)" }),
        ];
        if m.tables.get(&item.id) == Some(&true) {
            actions.push((Hit::RowMenu(k), '\u{E712}', "표 항목 메뉴"));
        }
        let id_row = hit.id();
        let label = format!("{}{}: {}", if item.pinned { "고정 · " } else { "" }, ms::kind_label(&item.kind), item.preview);
        let mut node = Node::item(id_row, A11Y_LIST, Role::ListItem, label, r, k == m.sel);
        node.invoke = true;
        node.focused = w.focus == Focus::List && k == m.sel;
        nodes.push(node);
        for (ah, g, name) in actions {
            let ar = Rect::new(ax - s(ACT_W), ry + (row - s(ACT_W)) / 2, ax, ry + (row - s(ACT_W)) / 2 + s(ACT_W));
            if w.hover == Some(ah) {
                p.fill_rect(ar, pal.head_hover);
            }
            p.font(glyph);
            p.text_color(if matches!(ah, Hit::RowPin(_)) && item.pinned { pal.accent } else { pal.row_action });
            p.text_rect(&g.to_string(), ar, DT_CENTER);
            nodes.push(Node::button(ah.id(), id_row, name, ar));
            hot.push((ar, ah));
            ax = ar.left;
        }
        p.font(body);
        p.text_color(pal.title);
        let tr = Rect::new(ic.right + s(10), ry + s(4), ax - s(6), ry + row - s(4));
        unsafe { draw_wrapped(p, &item.preview, tr) };
        hot.push((r, hit));
    }
    if sb.needed() {
        let track = Rect::new(l.w - sbw, y, l.w, y + list.height());
        p.fill_rect(track, pal.tab_bar);
        if let Some((t0, tn)) = sb.thumb() {
            let dragging = w.drag.is_some();
            p.fill_rect(Rect::new(track.left + s(2), y + t0, track.right - s(2), y + t0 + tn),
                        if dragging || w.hover == Some(Hit::Scroll) { pal.glyph } else { pal.placeholder });
        }
        nodes.push(Node::new(Hit::Scroll.id(), A11Y_LIST, Role::ScrollBar, "목록 스크롤", track));
        hot.push((track, Hit::Scroll));
        w.scroll = Some((track, sb));
    } else {
        w.scroll = None;
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn draw_settings(h: HWND, w: &mut Win, p: &Painter, pal: &ClipPalette, c: &AppColors, small: HFONT, y: i32, bottom: i32,
                        nodes: &mut Vec<Node>, hot: &mut Vec<(Rect, Hit)>, button: &mut ButtonFn) {
    let l = lay(h);
    let s = &l.s;
    let m = &w.model;
    let rows = ms::setting_rows(m);
    let area = Rect::new(0, y, l.w, bottom);
    nodes.push(Node::new(A11Y_SETTINGS, 0, Role::Group, "몽땅클립보드 설정", area));
    let mut ry = y;
    let first = w.settings_top.min(rows.len().saturating_sub(1));
    for (i, st) in rows.iter().enumerate().skip(first) {
        let row_h = if matches!(st, Setting::CaptureDir(_)) { s(40) } else { s(SET_H) };
        if ry + row_h > bottom {
            break;
        }
        let indent = if matches!(st, Setting::CaptureDir(_)) { s(PAD + 22) } else { s(PAD) };
        let r = Rect::new(indent, ry, l.w - s(PAD), ry + row_h);
        let hit = Hit::Setting(i);
        let enabled = setting_enabled(m, *st);
        let focused = w.focus == Focus::Setting(i);
        let label = setting_label(m, *st);
        if let Some((_, name, choices, cur)) = ms::number_spec(*st, &m.settings) {
            p.font(small);
            p.text_color(c.text.bgr());
            p.text_rect(name, Rect::new(r.left, r.top, r.left + s(160), r.bottom), 0);
            let value = format!("{} ▾", ms::number_text(*st, cur));
            let vr = Rect::new(r.left + s(170), r.top + s(2), r.left + s(290), r.bottom - s(2));
            let mut v = Visual { hover: w.hover == Some(hit), focused, disabled: !enabled, ..Default::default() };
            v.pressed = w.pressed == Some(hit) && v.hover;
            p.font(small);
            draw_button(p, vr, &value, ButtonKind::Secondary, v, c);
            let mut n = Node::new(hit.id(), A11Y_SETTINGS, Role::ComboBox, format!("{name}: {}", ms::number_text(*st, cur)), vr);
            n.invoke = true;
            n.enabled = enabled;
            n.focused = focused;
            nodes.push(n);
            hot.push((vr, hit));
            let _ = choices;
            let _ = &button;
        } else {
            let checked = setting_checked(m, *st).unwrap_or(false);
            let v = Visual { hover: w.hover == Some(hit), focused, disabled: !enabled, ..Default::default() };
            p.font(small);
            let box_r = if let Setting::CaptureDir(d) = st {
                let top = Rect::new(r.left, r.top, r.right, r.top + s(20));
                draw_check(p, top, &label, checked, v, c, s(16));
                p.text_color(c.muted.bgr());
                let path = m.capture_dirs.get(*d).map(|d| d.path.clone()).unwrap_or_default();
                p.text_rect(&path, Rect::new(r.left + s(24), r.top + s(20), r.right, r.bottom), DT_END_ELLIPSIS | DT_PATH_ELLIPSIS);
                r
            } else {
                draw_check(p, r, &label, checked, v, c, s(16));
                r
            };
            let mut n = Node::new(hit.id(), A11Y_SETTINGS, Role::CheckBox, label.clone(), box_r);
            n.invoke = true;
            n.enabled = enabled;
            n.focused = focused;
            n.selected = None;
            nodes.push(n);
            hot.push((box_r, hit));
        }
        ry += row_h;
        if matches!(st, Setting::CaptureAll) && m.capture_dirs.is_empty() {
            let er = Rect::new(s(PAD + 22), ry, l.w - s(PAD), ry + s(24));
            p.font(small);
            p.text_color(c.muted.bgr());
            p.text_rect("찾은 화면캡처 폴더가 없습니다.", er, 0);
            nodes.push(Node::new(A11Y_SETTINGS + 1000, A11Y_SETTINGS, Role::Text, "찾은 화면캡처 폴더가 없습니다.", er));
            ry += s(24);
        }
        if matches!(st, Setting::ExcelTarget) {
            let er = Rect::new(s(PAD + 24), ry - s(6), l.w - s(PAD), ry + s(16));
            p.font(small);
            p.text_color(c.muted.bgr());
            p.text_rect("엑셀 표 항목은 숫자 앞 공백·빈 칸·열 너비·칸 여백을 정리해 붙입니다(앱이 꺼져 있으면 원래대로).", er, DT_END_ELLIPSIS);
            ry += s(14);
        }
    }
    let _ = pal;
}

fn request_thumbs(h: HWND, want: Vec<(i64, i64)>) {
    if want.is_empty() || with(|w| w.quiet).unwrap_or(true) {
        return;
    }
    with(|w| w.thumbs_asked.extend(want.iter().map(|(id, _)| *id)));
    let hs = h as isize;
    spawn("kagent-manage-thumb", move || {
        for (id, size) in want {
            if crate::thumb::get(id, size).is_some() {
                unsafe { PostMessageW(hs as HWND, WM_THUMB, id as usize, 0) };
            }
        }
    });
}

// ---- 입력 ----

fn hit_at(x: i32, y: i32) -> Option<Hit> {
    with(|w| {
        // 행 안 단추가 행보다 먼저(뒤에 넣었어도 먼저 찾는다).
        let mut found = None;
        for (r, hit) in &w.hot {
            if r.contains(x, y) {
                match hit {
                    Hit::Row(_) => {
                        if found.is_none() {
                            found = Some(*hit);
                        }
                    }
                    _ => return Some(*hit),
                }
            }
        }
        found
    })
    .flatten()
}

fn rect_of(hit: Hit) -> Option<Rect> {
    with(|w| w.hot.iter().find(|(_, x)| *x == hit).map(|(r, _)| *r)).flatten()
}

fn item_id(k: usize) -> Option<(i64, bool)> {
    with(|w| w.model.visible_item(k).map(|i| (i.id, i.pinned))).flatten()
}

/// 누르기(마우스·Enter·Space·접근성 호출 공통).
fn activate(h: HWND, hit: Hit) {
    match hit {
        Hit::Head(HeadBtn::Top) => dispatch(Event::ToggleTop),
        Hit::Head(HeadBtn::Collapse) => dispatch(Event::ToggleCollapse),
        Hit::Head(HeadBtn::Max) => dispatch(Event::ToggleMax),
        Hit::Head(HeadBtn::Close) => close(h),
        Hit::Status(StatusBtn::PauseToggle) => {
            let paused = with(|w| w.model.paused()).unwrap_or(false);
            dispatch(Event::Do(if paused { Job::Resume } else { Job::Pause(None) }));
        }
        Hit::Status(StatusBtn::PauseMenu) => pause_menu(h),
        Hit::Status(StatusBtn::Clear) => dispatch(Event::AskClear),
        Hit::Status(StatusBtn::Quit) => dispatch(Event::AskQuit),
        Hit::Section(s) => {
            with(|w| w.focus = if s == Section::Settings { Focus::Setting(0) } else { Focus::List });
            dispatch(Event::Section(s));
        }
        Hit::MemoBox => set_focus(Focus::Memo),
        Hit::SearchBox => set_focus(Focus::Search),
        Hit::MemoAdd => add_memo(),
        Hit::Excel => dispatch(Event::Do(Job::ExcelTable)),
        Hit::Category(i) => dispatch(Event::Category(CATEGORIES[i.min(CATEGORIES.len() - 1)])),
        Hit::Row(k) => {
            set_focus(Focus::List);
            dispatch(Event::Select(k));
            dispatch(Event::OpenDetail(k));
        }
        Hit::RowPin(k) => {
            if let Some((id, pinned)) = item_id(k) {
                dispatch(Event::Do(Job::Pin { id, pinned: !pinned }));
            }
        }
        Hit::RowDelete(k) => {
            if let Some((id, _)) = item_id(k) {
                dispatch(Event::Do(Job::Delete { id }));
            }
        }
        Hit::RowMenu(k) => table_menu(h, k),
        Hit::Scroll => {}
        Hit::Setting(i) => activate_setting(h, i),
        Hit::DetailClose => dispatch(Event::CloseDetail),
        Hit::DetailRecopy => dispatch(Event::Recopy),
    }
}

fn activate_setting(h: HWND, i: usize) {
    let Some((st, job, enabled)) = with(|w| {
        let rows = ms::setting_rows(&w.model);
        let st = *rows.get(i)?;
        Some((st, ms::toggle_job(&w.model, st), setting_enabled(&w.model, st)))
    })
    .flatten() else { return };
    set_focus(Focus::Setting(i));
    if !enabled {
        return;
    }
    if let Some(job) = job {
        dispatch(Event::Do(job));
        return;
    }
    // 숫자 칸: 선택지 메뉴(관리창 드롭다운과 같은 값).
    let Some((choices, cur)) = with(|w| ms::number_spec(st, &w.model.settings).map(|n| (n.2, n.3))).flatten() else { return };
    let mut entries = Vec::new();
    if !choices.contains(&cur) {
        entries.push(Entry::Item { id: 1, label: format!("현재 {}", ms::number_text(st, cur)), checked: true, radio: true, enabled: false });
        entries.push(Entry::Separator);
    }
    for (n, v) in choices.iter().enumerate() {
        entries.push(Entry::Item { id: 10 + n, label: ms::number_text(st, *v), checked: *v == cur, radio: true, enabled: true });
    }
    let Some(r) = rect_of(Hit::Setting(i)) else { return };
    let chosen = unsafe { menu_below(h, &entries, r) };
    if chosen >= 10 {
        if let Some(job) = choices.get(chosen - 10).and_then(|v| ms::number_job(st, *v)) {
            dispatch(Event::Do(job));
        }
    }
}

unsafe fn menu_below(h: HWND, entries: &[Entry], r: Rect) -> usize {
    unsafe {
        let mut pt = POINT { x: r.left, y: r.bottom };
        ClientToScreen(h, &mut pt);
        SetForegroundWindow(h);
        let c = ui_core::menu::track(h, entries, pt, TPM_LEFTALIGN | TPM_TOPALIGN | TPM_LEFTBUTTON);
        with(|w| {
            w.pressed = None;
            w.hover = None;
        });
        InvalidateRect(h, std::ptr::null(), 0);
        c
    }
}

fn pause_menu(h: HWND) {
    if with(|w| w.model.busy.is_some()).unwrap_or(true) {
        return;
    }
    let entries: Vec<Entry> = ms::PAUSE_MINUTES.iter().enumerate()
        .map(|(i, m)| Entry::item(10 + i, if m % 60 == 0 { format!("{}시간", m / 60) } else { format!("{m}분") }))
        .chain([Entry::item(20, "다시 켤 때까지")])
        .collect();
    let Some(r) = rect_of(Hit::Status(StatusBtn::PauseMenu)) else { return };
    let c = unsafe { menu_below(h, &entries, r) };
    match c {
        20 => dispatch(Event::Do(Job::Pause(None))),
        n if (10..10 + ms::PAUSE_MINUTES.len()).contains(&n) => dispatch(Event::Do(Job::Pause(Some(ms::PAUSE_MINUTES[n - 10])))),
        _ => {}
    }
}

fn table_menu(h: HWND, k: usize) {
    let Some((id, _)) = item_id(k) else { return };
    let Some(r) = rect_of(Hit::RowMenu(k)) else { return };
    if unsafe { menu_below(h, &[Entry::item(1, "음영 없이 복사")], r) } == 1 {
        dispatch(Event::Do(Job::CopyNoShade { id }));
    }
}

fn row_menu(h: HWND, k: usize, pt: POINT) {
    let Some(info) = with(|w| w.model.visible_item(k).map(|i| (i.id, i.pinned, ms::is_web_text(i), w.model.tables.get(&i.id) == Some(&true)))).flatten() else { return };
    let (id, pinned, web, table) = info;
    let mut e = vec![Entry::item(1, "미리보기"), Entry::item(2, "다시 복사(클립보드에 올리기)"),
                     Entry::item(3, if pinned { "고정 풀기" } else { "고정(지워지지 않음)" }), Entry::item(4, "지우기")];
    if web {
        e.push(Entry::item(5, "브라우저에서 열기"));
    }
    if table {
        e.push(Entry::item(6, "음영 없이 복사"));
    }
    let c = unsafe {
        SetForegroundWindow(h);
        ui_core::menu::track(h, &e, pt, TPM_RIGHTBUTTON)
    };
    match c {
        1 => dispatch(Event::OpenDetail(k)),
        2 => dispatch(Event::Do(Job::Restore { id })),
        3 => dispatch(Event::Do(Job::Pin { id, pinned: !pinned })),
        4 => dispatch(Event::Do(Job::Delete { id })),
        5 => dispatch(Event::Do(Job::OpenWeb { id })),
        6 => dispatch(Event::Do(Job::CopyNoShade { id })),
        _ => {}
    }
}

fn add_memo() {
    let text = with(|w| w.memo.text().trim().to_string()).unwrap_or_default();
    if !text.is_empty() {
        dispatch(Event::Do(Job::AddMemo(text)));
    }
}

fn set_focus(f: Focus) {
    with(|w| w.focus = f);
    let h = hwnd();
    unsafe {
        SetFocus(h);
        InvalidateRect(h, std::ptr::null(), 0);
    }
}

/// Tab 순서: 목록 화면 = 메모 → 찾기 → 목록, 설정 화면 = 설정 칸 차례.
fn next_focus(back: bool) {
    let Some((section, focus, n)) = with(|w| (w.model.section, w.focus, ms::setting_rows(&w.model).len())) else { return };
    let next = match section {
        Section::List => {
            let ring = [Focus::Memo, Focus::Search, Focus::List];
            let i = ring.iter().position(|f| *f == focus).unwrap_or(2);
            ring[ui_core::controls::wrap_index(i, if back { -1 } else { 1 }, ring.len())]
        }
        Section::Settings => {
            let i = if let Focus::Setting(i) = focus { i } else { 0 };
            Focus::Setting(ui_core::controls::wrap_index(i, if back { -1 } else { 1 }, n.max(1)))
        }
    };
    set_focus(next);
}

fn clipboard_text(h: HWND) -> Option<String> {
    unsafe {
        if OpenClipboard(h) == 0 {
            return None;
        }
        let data = GetClipboardData(13);
        let mut out = None;
        if !data.is_null() {
            let p = GlobalLock(data) as *const u16;
            if !p.is_null() {
                let mut n = 0;
                while *p.add(n) != 0 && n < 1 << 20 {
                    n += 1;
                }
                out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)));
                GlobalUnlock(data);
            }
        }
        CloseClipboard();
        out
    }
}

fn set_clipboard_text(h: HWND, s: &str) {
    let w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        if OpenClipboard(h) == 0 {
            return;
        }
        EmptyClipboard();
        let g = GlobalAlloc(GMEM_MOVEABLE, w.len() * 2);
        if !g.is_null() {
            let p = GlobalLock(g) as *mut u16;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
                GlobalUnlock(g);
                SetClipboardData(13, g);
            }
        }
        CloseClipboard();
    }
}

/// 한 줄 입력 키(메모·찾기). 처리했으면 true.
fn edit_key(h: HWND, vk: u16) -> bool {
    let ctrl = unsafe { GetKeyState(VK_CONTROL as i32) } < 0;
    let shift = unsafe { GetKeyState(VK_SHIFT as i32) } < 0;
    let Some(key) = key_of(vk, ctrl) else { return false };
    let Some(r) = with(|w| match w.focus {
        Focus::Memo => Some((w.memo.key(key, shift), false)),
        Focus::Search => Some((w.search.key(key, shift), true)),
        _ => None,
    })
    .flatten() else { return false };
    let (result, search) = r;
    match result {
        EditResult::Ignored => return false,
        EditResult::Moved => {}
        EditResult::Changed => edit_changed(search),
        EditResult::Copy(s) => set_clipboard_text(h, &s),
        EditResult::Cut(s) => {
            set_clipboard_text(h, &s);
            edit_changed(search);
        }
        EditResult::PasteRequest => {
            if let Some(t) = clipboard_text(h) {
                with(|w| if search { w.search.paste(&t) } else { w.memo.paste(&t) });
                edit_changed(search);
            }
        }
    }
    unsafe { InvalidateRect(h, std::ptr::null(), 0) };
    true
}

fn edit_changed(search: bool) {
    if search {
        let t = with(|w| w.search.needle()).unwrap_or_default();
        dispatch(Event::Typed(t));
    }
}

fn on_key(h: HWND, vk: u16) {
    let ctrl = unsafe { GetKeyState(VK_CONTROL as i32) } < 0;
    let shift = unsafe { GetKeyState(VK_SHIFT as i32) } < 0;
    let (focus, detail, section) = with(|w| (w.focus, w.model.detail.is_open(), w.model.section)).unwrap_or((Focus::List, false, Section::List));
    if vk == VK_ESCAPE {
        let typed = with(|w| !w.model.typed.is_empty()).unwrap_or(false);
        if focus == Focus::Memo && !detail {
            with(|w| w.memo.clear());
        } else if !detail && !typed {
            set_focus(Focus::List);
        }
        dispatch(Event::Escape);
        return;
    }
    if vk == VK_TAB {
        if ctrl {
            let next = if section == Section::List { Section::Settings } else { Section::List };
            activate(h, Hit::Section(next));
        } else {
            next_focus(shift);
        }
        return;
    }
    if ctrl && vk == 0x46 {
        // Ctrl+F
        dispatch(Event::Section(Section::List));
        set_focus(Focus::Search);
        return;
    }
    if vk == VK_F5 {
        dispatch(Event::External);
        return;
    }
    if detail {
        if vk == VK_RETURN {
            dispatch(Event::Recopy);
        }
        return;
    }
    match focus {
        Focus::Memo | Focus::Search => {
            if vk == VK_RETURN {
                if focus == Focus::Memo {
                    add_memo();
                } else {
                    unsafe { KillTimer(h, TIMER_DEBOUNCE) };
                    dispatch(Event::DebounceFired);
                }
                return;
            }
            if vk == VK_DOWN {
                set_focus(Focus::List);
                return;
            }
            edit_key(h, vk);
        }
        Focus::List => {
            let rows = with(|w| w.model.rows).unwrap_or(1) as isize;
            match vk {
                VK_UP => dispatch(Event::Move(-1)),
                VK_DOWN => dispatch(Event::Move(1)),
                VK_PRIOR => dispatch(Event::Move(-rows)),
                VK_NEXT => dispatch(Event::Move(rows)),
                VK_HOME => dispatch(Event::Move(-1_000_000)),
                VK_END => dispatch(Event::Move(1_000_000)),
                VK_LEFT | VK_RIGHT => {
                    let cur = with(|w| CATEGORIES.iter().position(|c| *c == w.model.category).unwrap_or(0)).unwrap_or(0);
                    let next = ui_core::controls::wrap_index(cur, if vk == VK_LEFT { -1 } else { 1 }, CATEGORIES.len());
                    dispatch(Event::Category(CATEGORIES[next]));
                }
                VK_RETURN => {
                    let sel = with(|w| w.model.sel).unwrap_or(0);
                    dispatch(Event::OpenDetail(sel));
                }
                VK_DELETE => {
                    let sel = with(|w| w.model.sel).unwrap_or(0);
                    if let Some((id, _)) = item_id(sel) {
                        dispatch(Event::Do(Job::Delete { id }));
                    }
                }
                0x43 if ctrl => {
                    // Ctrl+C = 다시 복사
                    let sel = with(|w| w.model.sel).unwrap_or(0);
                    if let Some((id, _)) = item_id(sel) {
                        dispatch(Event::Do(Job::Restore { id }));
                    }
                }
                _ => return,
            }
            preview_selected(h);
        }
        Focus::Setting(i) => {
            let n = with(|w| ms::setting_rows(&w.model).len()).unwrap_or(1);
            match vk {
                VK_UP => set_focus(Focus::Setting(i.saturating_sub(1))),
                VK_DOWN => set_focus(Focus::Setting((i + 1).min(n - 1))),
                VK_SPACE | VK_RETURN => activate(h, Hit::Setting(i)),
                _ => {}
            }
            ensure_setting_visible(h);
        }
    }
}

fn ensure_setting_visible(h: HWND) {
    let shown = with(|w| w.hot.iter().filter(|(_, x)| matches!(x, Hit::Setting(_))).count()).unwrap_or(0).max(1);
    with(|w| {
        if let Focus::Setting(i) = w.focus {
            if i < w.settings_top {
                w.settings_top = i;
            } else if i >= w.settings_top + shown {
                w.settings_top = i + 1 - shown;
            }
        }
    });
    unsafe { InvalidateRect(h, std::ptr::null(), 0) };
}

fn preview_selected(h: HWND) {
    let Some((target, offset)) = with(|w| (w.model.selected().and_then(crate::preview::target_of), w.model.sel.saturating_sub(w.model.top))) else { return };
    let Some(r) = rect_of(Hit::Row(offset + with(|w| w.model.top).unwrap_or(0))) else {
        crate::preview::hide();
        return;
    };
    unsafe {
        let mut pt = POINT { x: 0, y: r.top };
        ClientToScreen(h, &mut pt);
        let mut wr: RECT = std::mem::zeroed();
        GetWindowRect(h, &mut wr);
        crate::preview::point(target, crate::preview::Anchor::Row { y: pt.y, left: wr.left, right: wr.right });
    }
}

fn on_char(h: HWND, ch: char) {
    if ch.is_control() {
        return;
    }
    let focus = with(|w| w.focus).unwrap_or(Focus::List);
    let focus = match focus {
        // 목록·설정에서 글자를 치면 찾기 칸으로(빠른 목록과 같음).
        Focus::List | Focus::Setting(_) => {
            dispatch(Event::Section(Section::List));
            set_focus(Focus::Search);
            Focus::Search
        }
        f => f,
    };
    let changed = with(|w| if focus == Focus::Memo { w.memo.insert_char(ch) } else { w.search.insert_char(ch) }).unwrap_or(false);
    if changed {
        edit_changed(focus == Focus::Search);
    }
    unsafe { InvalidateRect(h, std::ptr::null(), 0) };
}

fn xy(lp: LPARAM) -> (i32, i32) {
    ui_core::geom::xy(lp)
}

fn on_mouse_move(h: HWND, lp: LPARAM) {
    let (x, y) = xy(lp);
    let drag = with(|w| w.drag).flatten();
    if let Some((y0, top0)) = drag {
        if let Some(sb) = with(|w| w.scroll.map(|s| s.1)).flatten() {
            dispatch(Event::ScrollTo(sb.drag(top0, y - y0)));
        }
        return;
    }
    unsafe {
        let tracking = with(|w| std::mem::replace(&mut w.tracking, true)).unwrap_or(true);
        if !tracking {
            let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: h, dwHoverTime: 0 };
            TrackMouseEvent(&mut tme);
        }
    }
    let hit = hit_at(x, y);
    let changed = with(|w| std::mem::replace(&mut w.hover, hit) != hit).unwrap_or(false);
    if changed {
        unsafe { InvalidateRect(h, std::ptr::null(), 0) };
    }
    // 그림 미리보기(빠른 목록과 같은 창·규칙).
    let row = match hit {
        Some(Hit::Row(k) | Hit::RowPin(k) | Hit::RowDelete(k) | Hit::RowMenu(k)) => Some(k),
        _ => None,
    };
    let target = row.and_then(|k| with(|w| w.model.visible_item(k).and_then(crate::preview::target_of)).flatten());
    let mut pt = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut pt) };
    crate::preview::point(target, crate::preview::Anchor::Cursor(pt));
}

unsafe extern "system" fn wndproc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                paint(h);
                0
            }
            WM_ERASEBKGND => 1,
            WM_GETMINMAXINFO | WM_NCCALCSIZE | WM_NCHITTEST => {
                let (collapsed, maximized) = with(|w| (w.model.collapsed, w.model.maximized)).unwrap_or((false, false));
                let width = {
                    let mut rc: RECT = std::mem::zeroed();
                    GetClientRect(h, &mut rc);
                    rc.right
                };
                let on_button = |x: i32, y: i32| y < scale(h, HEAD_H) && x >= width - scale(h, CAPTION_W) * HEAD_BTNS.len() as i32;
                let spec = ui_core::window::FrameSpec { head: HEAD_H, min_w: MIN_W, min_h: if collapsed { HEAD_H } else { MIN_H },
                                                        on_button: &on_button, maximized, collapsed };
                match ui_core::window::frame_message(h, msg, wp, lp, &spec) {
                    Some(r) => r,
                    None => DefWindowProcW(h, msg, wp, lp),
                }
            }
            WM_SIZE => {
                sync_children(h);
                InvalidateRect(h, std::ptr::null(), 0);
                0
            }
            WM_DPICHANGED => {
                let dpi = ui_core::dpi::on_dpi_changed(h, wp, lp);
                FONTS.with(|c| c.borrow_mut().retain_dpi(dpi));
                sync_children(h);
                InvalidateRect(h, std::ptr::null(), 0);
                0
            }
            WM_SETTINGCHANGE | WM_SYSCOLORCHANGE | WM_THEMECHANGED => {
                if crate::look::on_message(msg, lp) {
                    theme_changed();
                    crate::popup::theme_changed();
                }
                DefWindowProcW(h, msg, wp, lp)
            }
            WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC => with(|w| w.edit_colors.on_ctlcolor(wp)).unwrap_or(0),
            WM_GETOBJECT => match a11y::on_getobject(h, wp, lp) {
                Some(r) => r,
                None => DefWindowProcW(h, msg, wp, lp),
            },
            m if m == a11y::action_message() => {
                if let Some((action, id)) = a11y::decode(wp, lp) {
                    let hit = with(|w| w.hot.iter().find(|(_, x)| x.id() == id).map(|(_, x)| *x)).flatten();
                    match (hit, action) {
                        (Some(Hit::Row(k)), a11y::Action::Select | a11y::Action::Focus) => {
                            set_focus(Focus::List);
                            dispatch(Event::Select(k));
                        }
                        (Some(Hit::Category(i)), _) => activate(h, Hit::Category(i)),
                        (Some(Hit::Section(s)), _) => activate(h, Hit::Section(s)),
                        (Some(Hit::MemoBox), _) => set_focus(Focus::Memo),
                        (Some(Hit::SearchBox), _) => set_focus(Focus::Search),
                        (Some(hit), a11y::Action::Invoke) => activate(h, hit),
                        _ => {}
                    }
                }
                0
            }
            WM_DESTROY => {
                windows_sys::Win32::System::Ole::RevokeDragDrop(h);
                a11y::on_destroy(h);
                WIN.with(|w| *w.borrow_mut() = None);
                0
            }
            WM_CLOSE => {
                close(h);
                0
            }
            WM_EXITSIZEMOVE => {
                let collapsed = with(|w| {
                    w.restore_rect = None;
                    w.model.maximized = false;
                    w.model.collapsed
                })
                .unwrap_or(false);
                if !collapsed {
                    save_state(h);
                }
                InvalidateRect(h, std::ptr::null(), 0);
                0
            }
            WM_NCLBUTTONDBLCLK if wp == HTCAPTION as usize => {
                dispatch(Event::ToggleMax);
                0
            }
            WM_TIMER if wp == TIMER_DEBOUNCE => {
                KillTimer(h, TIMER_DEBOUNCE);
                dispatch(Event::DebounceFired);
                0
            }
            WM_LOADED => {
                let r: Result<Box<Snapshot>, String> = unbox(lp);
                dispatch(Event::Loaded { seq: wp as u64, result: r });
                0
            }
            WM_DONE => {
                let o: Outcome = unbox(lp);
                if let Some(url) = &o.open_url {
                    windows_sys::Win32::UI::Shell::ShellExecuteW(h, wide("open").as_ptr(), wide(url).as_ptr(), std::ptr::null(),
                                                                 std::ptr::null(), SW_SHOWNORMAL);
                }
                dispatch(Event::Done { ok: o.ok, message: o.message });
                0
            }
            WM_DETAIL => {
                let r: Result<(DetailBody, Option<Thumb>), String> = unbox(lp);
                let result = match r {
                    Ok((body, img)) => {
                        with(|w| w.detail_image = img);
                        Ok(body)
                    }
                    Err(e) => Err(e),
                };
                dispatch(Event::DetailLoaded { id: wp as i64, result });
                0
            }
            WM_TABLE => {
                dispatch(Event::TableChecked { id: wp as i64, table: lp != 0 });
                0
            }
            WM_THUMB => {
                InvalidateRect(h, std::ptr::null(), 0);
                0
            }
            WM_DROPPED => {
                let list = crate::drop_target::take(lp);
                if list.is_empty() {
                    with(|w| w.model.message = "지원하는 파일·폴더·웹 링크가 아닙니다.".into());
                    InvalidateRect(h, std::ptr::null(), 0);
                } else {
                    dispatch(Event::Section(Section::List));
                    dispatch(Event::Do(Job::AddFiles(list)));
                }
                0
            }
            WM_SETCURSOR if (lp & 0xFFFF) as u32 == HTCLIENT => {
                let mut pt = POINT { x: 0, y: 0 };
                GetCursorPos(&mut pt);
                ScreenToClient(h, &mut pt);
                if matches!(hit_at(pt.x, pt.y), Some(Hit::MemoBox | Hit::SearchBox)) {
                    SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_IBEAM));
                    return 1;
                }
                DefWindowProcW(h, msg, wp, lp)
            }
            WM_MOUSEMOVE => {
                on_mouse_move(h, lp);
                0
            }
            WM_MOUSELEAVE => {
                with(|w| {
                    w.tracking = false;
                    w.hover = None;
                });
                crate::preview::hide();
                InvalidateRect(h, std::ptr::null(), 0);
                0
            }
            WM_LBUTTONDOWN => {
                let (x, y) = xy(lp);
                SetFocus(h);
                let hit = hit_at(x, y);
                if hit == Some(Hit::Scroll) {
                    if let Some((track, sb)) = with(|w| w.scroll).flatten() {
                        match sb.hit(y - track.top) {
                            Some(ScrollHit::Thumb) => {
                                with(|w| w.drag = Some((y, w.model.top)));
                                SetCapture(h);
                            }
                            Some(ScrollHit::PageUp) => dispatch(Event::Scroll(-(sb.rows as isize))),
                            Some(ScrollHit::PageDown) => dispatch(Event::Scroll(sb.rows as isize)),
                            None => {}
                        }
                    }
                    return 0;
                }
                with(|w| w.pressed = hit);
                match hit {
                    Some(Hit::MemoBox) => set_focus(Focus::Memo),
                    Some(Hit::SearchBox) => set_focus(Focus::Search),
                    _ => {
                        InvalidateRect(h, std::ptr::null(), 0);
                    }
                }
                0
            }
            WM_LBUTTONUP => {
                if with(|w| w.drag.take()).flatten().is_some() {
                    ReleaseCapture();
                    InvalidateRect(h, std::ptr::null(), 0);
                    return 0;
                }
                let (x, y) = xy(lp);
                let pressed = with(|w| w.pressed.take()).flatten();
                let hit = hit_at(x, y);
                if let Some(hit) = hit.filter(|h| Some(*h) == pressed) {
                    if !matches!(hit, Hit::MemoBox | Hit::SearchBox) {
                        activate(h, hit);
                    }
                }
                InvalidateRect(h, std::ptr::null(), 0);
                0
            }
            WM_LBUTTONDBLCLK => {
                let (x, y) = xy(lp);
                if let Some(Hit::Row(k)) = hit_at(x, y) {
                    // 웹 글은 두 번 누르면 브라우저로(Flutter onDoubleTap), 아니면 상세.
                    let web = with(|w| w.model.visible_item(k).map(|i| (i.id, ms::is_web_text(i)))).flatten();
                    match web {
                        Some((id, true)) => {
                            dispatch(Event::CloseDetail);
                            dispatch(Event::Do(Job::OpenWeb { id }));
                        }
                        Some(_) => dispatch(Event::OpenDetail(k)),
                        None => {}
                    }
                }
                0
            }
            WM_RBUTTONUP => {
                let (x, y) = xy(lp);
                if let Some(Hit::Row(k) | Hit::RowPin(k) | Hit::RowDelete(k) | Hit::RowMenu(k)) = hit_at(x, y) {
                    dispatch(Event::Select(k));
                    let mut pt = POINT { x, y };
                    ClientToScreen(h, &mut pt);
                    row_menu(h, k, pt);
                }
                0
            }
            WM_MOUSEWHEEL => {
                let lines = with(|w| w.wheel.add(ui_core::geom::wheel_delta(wp), 3)).unwrap_or(0);
                if lines != 0 {
                    let section = with(|w| w.model.section).unwrap_or(Section::List);
                    if section == Section::List {
                        dispatch(Event::Scroll(lines));
                    } else {
                        with(|w| w.settings_top = (w.settings_top as isize + lines.signum()).max(0) as usize);
                        InvalidateRect(h, std::ptr::null(), 0);
                    }
                }
                0
            }
            WM_KEYDOWN => {
                on_key(h, wp as u16);
                0
            }
            WM_CHAR => {
                if let Some(ch) = char::from_u32(wp as u32) {
                    on_char(h, ch);
                }
                0
            }
            WM_IME_COMPOSITION => {
                if let Some(comp) = ui_core::text_input::composition_string(h, lp) {
                    let search = with(|w| match w.focus {
                        Focus::Memo => {
                            w.memo.set_composition(comp.clone());
                            Some(false)
                        }
                        Focus::Search => {
                            w.search.set_composition(comp.clone());
                            Some(true)
                        }
                        _ => None,
                    })
                    .flatten();
                    if search == Some(true) {
                        edit_changed(true);
                    }
                    InvalidateRect(h, std::ptr::null(), 0);
                }
                DefWindowProcW(h, msg, wp, lp)
            }
            WM_IME_ENDCOMPOSITION => {
                with(|w| {
                    w.memo.end_composition();
                    w.search.end_composition();
                });
                DefWindowProcW(h, msg, wp, lp)
            }
            _ => DefWindowProcW(h, msg, wp, lp),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::category::CategorySet;
    use ui_core::theme::Mode;

    fn item(id: i64, kind: &str, preview: &str, pinned: bool, cats: &[Category]) -> Summary {
        Summary { id, kind: kind.into(), preview: preview.into(), pinned, used: 0.0, size: 0, cats: CategorySet::of(cats) }
    }

    fn sample_items() -> Vec<Summary> {
        let mut v = vec![
            item(1, "text", "고정한 회의 메모: 다음 주 화요일 10시 대회의실", true, &[Category::All]),
            item(2, "image", "그림 640×480", false, &[Category::All, Category::Image]),
            item(3, "text", "https://example.com/아주/긴/주소/말줄임/확인용/경로/이어서/계속/더/길게/두/줄/넘게/만들기", false,
                 &[Category::All, Category::Web]),
            item(4, "files", "파일 2개: 보고서.hwpx, 표.xlsx", false, &[Category::All, Category::File, Category::Document]),
            item(5, "text", "서식 포함 · 표 머리글 · 엑셀", false, &[Category::All, Category::Document]),
        ];
        for i in 6..30 {
            v.push(item(i, "text", &format!("{i}번째 항목"), false, &[Category::All]));
        }
        v
    }

    fn loaded_model() -> Model {
        let mut m = Model::default();
        ms::step(&mut m, Event::Open(OpenMode::Sidebar));
        let snap = Snapshot {
            items: sample_items(),
            settings: crate::store::Settings::default(),
            capture_dirs: vec![ms::CaptureDir { path: "C:\\Users\\u\\Pictures\\Screenshots".into(), label: "Win+PrtScn".into(),
                                                state: "watching".into() }],
            autostart: Some(false),
            count: 29,
            hotkey_ok: true,
            resume_in_min: None,
        };
        let seq = m.seq;
        ms::step(&mut m, Event::Loaded { seq, result: Ok(Box::new(snap)) });
        ms::step(&mut m, Event::TableChecked { id: 5, table: true });
        m
    }

    /// 창 하나를 만들고(보이지 않음) 상태별로 그려 해시를 모은다.
    pub(crate) fn render_cases(mode: Mode) -> Vec<(&'static str, ui_core::probe::Frame)> {
        crate::test_app();
        crate::look::force(Some(mode));
        let h = create();
        assert!(!h.is_null());
        with(|w| w.quiet = true);
        let mut out = Vec::new();
        let mut case = |name: &'static str, width: i32, height: i32, f: &dyn Fn(&mut Win)| {
            with(|w| {
                w.model = loaded_model();
                w.memo.clear();
                w.search.clear();
                w.focus = Focus::List;
                w.hover = None;
                w.pressed = None;
                w.detail_image = None;
                w.settings_top = 0;
                f(w);
            });
            unsafe {
                SetWindowPos(h, std::ptr::null_mut(), 0, 0, width, height, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
                sync_children(h);
                ui_core::probe::arm();
                SendMessageW(h, WM_PAINT, 0, 0);
            }
            out.push((name, ui_core::probe::take().expect(name)));
        };
        case("list", 600, 720, &|_| {});
        case("hover_scrolled", 600, 720, &|w| {
            w.model.sel = 6;
            w.model.top = 3;
            w.hover = Some(Hit::RowPin(4));
        });
        case("search_memo", 600, 720, &|w| {
            w.focus = Focus::Search;
            w.memo.paste("붙여 넣은 메모");
            w.search.paste("회의");
            w.search.set_composition("글".into());
            w.model.typed = "회의글".into();
        });
        case("narrow_category_empty", 480, 560, &|w| {
            w.model.category = Category::Capture;
        });
        case("settings", 600, 720, &|w| {
            w.model.section = Section::Settings;
            w.focus = Focus::Setting(1);
        });
        case("busy_message_paused", 600, 720, &|w| {
            w.model.busy = Some(Job::Clear);
            w.model.settings.paused = true;
            w.model.resume_in_min = Some(25);
            w.model.message = "고정하지 않은 항목을 지웠습니다.".into();
        });
        case("detail_image_failed", 600, 720, &|w| {
            w.model.detail = Detail::Failed { id: 2, kind: "image".into() };
        });
        case("collapsed", 600, 34, &|w| {
            w.model.collapsed = true;
            w.model.topmost = true;
        });
        crate::look::force(Some(Mode::Light));
        out
    }

    #[test]
    fn renders_are_deterministic_and_three_themes_differ() {
        let _g = crate::look::tests_lock();
        let light = render_cases(Mode::Light);
        let again = render_cases(Mode::Light);
        let dark = render_cases(Mode::Dark);
        let hc = render_cases(Mode::HighContrast);
        for (((a, b), d), c) in light.iter().zip(&again).zip(&dark).zip(&hc) {
            println!("RENDER manage {} {}x{} light={:016x} dark={:016x} hc={:016x}", a.0, a.1.width, a.1.height, a.1.hash, d.1.hash, c.1.hash);
            assert_eq!(a.1.hash, b.1.hash, "{} 같은 상태 → 같은 픽셀", a.0);
            assert_ne!(a.1.hash, d.1.hash, "{} 라이트·다크가 같음", a.0);
        }
        let distinct: HashSet<u64> = light.iter().map(|(_, f)| f.hash).collect();
        assert_eq!(distinct.len(), light.len(), "상태마다 다른 화면");
    }

    #[test]
    fn accessibility_tree_has_no_unnamed_elements_in_any_state() {
        let _g = crate::look::tests_lock();
        crate::test_app();
        crate::look::force(Some(Mode::Light));
        let h = create();
        with(|w| w.quiet = true);
        for section in [Section::List, Section::Settings] {
            for detail in [false, true] {
                with(|w| {
                    w.model = loaded_model();
                    w.model.section = section;
                    if detail {
                        w.model.detail = Detail::Ready { id: 1, kind: "image".into(), body: DetailBody::Image { w: 10, h: 10 } };
                    }
                });
                unsafe {
                    SetWindowPos(h, std::ptr::null_mut(), 0, 0, 600, 720, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
                    SendMessageW(h, WM_PAINT, 0, 0);
                }
                let nodes = a11y::nodes(h);
                assert!(a11y::unnamed(&nodes).is_empty(), "{section:?} {detail}: 빈 이름 {:?}", a11y::unnamed(&nodes));
                let mut ids: Vec<u32> = nodes.iter().map(|n| n.id).collect();
                ids.sort();
                let before = ids.len();
                ids.dedup();
                assert_eq!(before, ids.len(), "id 겹침");
                assert_eq!(nodes.iter().filter(|n| n.role == Role::Button && n.parent == 0 && n.id < 10).count(), 4, "머리글 단추 4");
                if detail {
                    assert!(nodes.iter().any(|n| n.name == "다시 복사"));
                } else if section == Section::List {
                    assert_eq!(nodes.iter().filter(|n| n.role == Role::TabItem && n.parent == A11Y_CATS).count(), 7);
                    let rows: Vec<_> = nodes.iter().filter(|n| n.role == Role::ListItem).collect();
                    assert!(rows.len() >= 5);
                    assert!(rows[0].name.starts_with("고정 · 글: 고정한 회의 메모"));
                    assert!(nodes.iter().any(|n| n.name == "표 항목 메뉴"), "표 항목에는 메뉴 단추");
                    assert!(nodes.iter().any(|n| n.role == Role::ScrollBar), "29개 → 스크롤 막대");
                    assert!(nodes.iter().any(|n| n.role == Role::Edit && n.name.starts_with("메모 작성")));
                } else {
                    let checks = nodes.iter().filter(|n| n.role == Role::CheckBox).count();
                    let combos = nodes.iter().filter(|n| n.role == Role::ComboBox).count();
                    assert_eq!((checks, combos), (7, 4), "자동 실행·켜고 끄는 5·폴더 1 / 숫자 4");
                }
            }
        }
        // 접근성 '선택'·'호출'은 마우스와 같은 길.
        with(|w| {
            w.model = loaded_model();
            w.model.section = Section::List;
        });
        unsafe { SendMessageW(h, WM_PAINT, 0, 0) };
        unsafe { SendMessageW(h, a11y::action_message(), 2, Hit::Row(3).id() as isize) };
        assert_eq!(with(|w| w.model.sel), Some(3));
        unsafe { SendMessageW(h, a11y::action_message(), 1, Hit::Category(2).id() as isize) };
        assert_eq!(with(|w| w.model.category), Some(Category::Image));
        unsafe { SendMessageW(h, a11y::action_message(), 1, Hit::Head(HeadBtn::Top).id() as isize) };
        assert_eq!(with(|w| w.model.topmost), Some(true));
        unsafe { SendMessageW(h, a11y::action_message(), 1, Hit::Head(HeadBtn::Top).id() as isize) };
    }

    #[test]
    fn keyboard_and_ime_drive_the_same_state_machine() {
        let _g = crate::look::tests_lock();
        crate::test_app();
        crate::look::force(Some(Mode::Light));
        let h = create();
        with(|w| {
            w.quiet = true;
            w.model = loaded_model();
            w.focus = Focus::List;
        });
        unsafe {
            SetWindowPos(h, std::ptr::null_mut(), 0, 0, 600, 720, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
            SendMessageW(h, WM_PAINT, 0, 0);
            SendMessageW(h, WM_KEYDOWN, VK_DOWN as usize, 0);
            SendMessageW(h, WM_KEYDOWN, VK_DOWN as usize, 0);
        }
        assert_eq!(with(|w| w.model.sel), Some(2));
        unsafe { SendMessageW(h, WM_KEYDOWN, VK_RIGHT as usize, 0) };
        assert_eq!(with(|w| w.model.category), Some(Category::Document));
        // 목록에서 글자를 치면 찾기 칸으로, 디바운스(타이머) 뒤 검색어.
        unsafe { SendMessageW(h, WM_CHAR, '회' as usize, 0) };
        assert_eq!(with(|w| (w.focus, w.search.text(), w.model.typed.clone())), Some((Focus::Search, "회".into(), "회".into())));
        unsafe { SendMessageW(h, WM_TIMER, TIMER_DEBOUNCE, 0) };
        assert_eq!(with(|w| w.model.query.clone()), Some("회".into()));
        unsafe { SendMessageW(h, WM_KEYDOWN, VK_ESCAPE as usize, 0) };
        assert_eq!(with(|w| (w.search.text(), w.model.query.clone())), Some((String::new(), String::new())));
        // 메모 칸: 글자 → Enter → 작업(AddMemo) 시작.
        assert_eq!(with(|w| w.focus), Some(Focus::Search), "Esc는 찾기만 지우고 칸은 그대로");
        unsafe { SendMessageW(h, WM_KEYDOWN, VK_TAB as usize, 0) };
        assert_eq!(with(|w| w.focus), Some(Focus::List));
        unsafe { SendMessageW(h, WM_KEYDOWN, VK_TAB as usize, 0) };
        assert_eq!(with(|w| w.focus), Some(Focus::Memo), "Tab 순서: 메모 → 찾기 → 목록 → 메모");
        for ch in "새 메모".chars() {
            unsafe { SendMessageW(h, WM_CHAR, ch as usize, 0) };
        }
        with(|w| w.model.load = Load::Idle);
        // 작업은 실제로 돌리지 않으려고 busy를 먼저 채워 둔다(받지 않음) → 상태만 확인.
        with(|w| w.model.busy = Some(Job::Resume));
        unsafe { SendMessageW(h, WM_KEYDOWN, VK_RETURN as usize, 0) };
        assert_eq!(with(|w| w.memo.text()), Some("새 메모".into()), "작업 중이면 메모 그대로");
        with(|w| w.model.busy = None);
        // Ctrl+Tab = 설정 화면.
        with(|w| w.focus = Focus::List);
        let _ = h;
    }

    /// 관리창 표시 시간(1회 측정, `--ignored --release`): show() 호출 → 첫 그리기 끝.
    #[test]
    #[ignore]
    fn measure_show_ms() {
        let _g = crate::look::tests_lock();
        crate::test_app();
        {
            let a = crate::test_app();
            let s = a.store.lock().unwrap();
            for i in 0..200 {
                let t = format!("측정 항목 {i} 회의 메모와 긴 글 예시");
                let _ = s.insert("text", t.as_bytes(), &[], &t, "test", i as f64);
            }
        }
        crate::look::force(Some(Mode::Light));
        let mut first = None;
        let mut ms_list = Vec::new();
        for _ in 0..30 {
            let h = hwnd();
            if !h.is_null() {
                close(h);
            }
            let t = std::time::Instant::now();
            show(OpenMode::Sidebar);
            // 목록 결과(작업 스레드)를 받아 그릴 때까지.
            let h = hwnd();
            let mut msg: MSG = unsafe { std::mem::zeroed() };
            while with(|w| !w.model.loaded_once).unwrap_or(false) {
                unsafe {
                    if GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) <= 0 {
                        break;
                    }
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            unsafe { UpdateWindow(h) };
            let v = t.elapsed().as_secs_f64() * 1000.0;
            first.get_or_insert(v);
            ms_list.push(v);
        }
        close(hwnd());
        ms_list.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!("MEASURE manage show first={:.3}ms median={:.3}ms p90={:.3}ms", first.unwrap(), ms_list[15], ms_list[27]);
    }
}
