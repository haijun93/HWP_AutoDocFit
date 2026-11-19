//! GDI 이중 버퍼·공용 frame·직접 키/IME 입력. Model은 event의 step 한 곳에서만 갱신한다.
use crate::{
    frame,
    settings::Settings,
    util::wide,
    vf_data::{self, BoxRow, Item, Snapshot},
    vf_state::{self, Caller, Context, Effect, Event, Model, ReadResult, View},
    vf_store,
};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    cell::RefCell,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::{Ime::*, KeyboardAndMouse::*},
        Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, DragQueryPoint, HDROP},
        WindowsAndMessaging::*,
    },
};
use crate::vf_paint::{Scene, RowRef, SPACES};
pub use crate::vf_paint::window_title;
use crate::vf_ui::{self, Chrome};
#[derive(Clone, Debug, PartialEq, Eq)]
struct ItemDrag {
    source_panel: String,
    item_ids: Vec<String>,
    start: (i32, i32),
    target_panel: Option<String>,
    copy: bool,
}
#[derive(Default)]
struct Ui {
    model: Model,
    hwnd: HWND,
    owner: HWND,
    boxes: Vec<BoxRow>,
    items: Vec<Item>,
    filtered: Vec<i64>,
    top: usize,
    committed: String,
    comp: String,
    pressed: Option<usize>,
    truncated: bool,
    drag_split: Option<crate::vf_layout::SplitHit>,
    item_drag: Option<ItemDrag>,
    last_operation: String,
    active_panel: usize,
    address_edit: bool,
    /// 영역 번호별 가상 항목(virtual.db). 그릴 때마다 읽지 않도록 바뀔 때만 다시 읽는다.
    virt: Vec<Vec<vf_store::VirtualItem>>,
}
thread_local! {static UI:RefCell<Ui>=RefCell::new(Ui::default());}
fn with<R>(f: impl FnOnce(&Ui) -> R) -> R {
    UI.with(|s| f(&s.borrow()))
}
fn edit<R>(f: impl FnOnce(&mut Ui) -> R) -> R {
    UI.with(|s| f(&mut s.borrow_mut()))
}
pub fn snapshot() -> (String, Option<i64>, usize, String) {
    with(|s| {
        (
            s.model.view.name().into(),
            s.model.view.context().and_then(|c| c.box_id),
            s.filtered.len(),
            match &s.model.view {
                View::Failed { error, .. } => error.clone(),
                _ => String::new(),
            },
        )
    })
}
pub fn configure(owner: HWND, settings: &Settings) {
    edit(|s| s.owner = owner);
    event(Event::SetSticky(settings.sticky));
    let h = with(|s| s.hwnd);
    if !h.is_null() {
        topmost(h);
        draw_later();
    }
}
pub fn call(caller: Caller) {
    event(Event::Call(caller));
}
pub fn close() {
    event(Event::Close);
}
pub fn destroy() {
    let h = with(|s| s.hwnd);
    if !h.is_null() {
        unsafe {
            DestroyWindow(h);
        }
    }
}
fn notify(body: &str) {
    let owner = with(|s| s.owner);
    crate::notice(owner, body);
}
/// Model 대입은 이곳 한 군데뿐이다. Win32의 재진입 전에 RefCell 빌림을 끝낸다.
pub fn event(e: Event) {
    if matches!(
        &e,
        Event::Close
            | Event::Escape
            | Event::Outside
            | Event::Minimize
            | Event::Call(_)
            | Event::Opened { .. }
    ) {
        save();
    }
    let effects = edit(|s| {
        let (next, effects) = vf_state::step(&s.model, e);
        s.model = next;
        effects
    });
    for effect in effects {
        match effect {
            Effect::Show => {
                if let Err(error) = ensure() {
                    notify(&error);
                    event(Event::Close);
                    return;
                }
                let h = with(|s| s.hwnd);
                unsafe {
                    ShowWindow(h, SW_SHOW);
                    SetForegroundWindow(h);
                    SetFocus(h);
                }
            }
            Effect::Foreground => {
                let h = with(|s| s.hwnd);
                unsafe {
                    SetForegroundWindow(h);
                    SetFocus(h);
                }
            }
            Effect::Hide => {
                save();
                let h = with(|s| s.hwnd);
                unsafe {
                    ShowWindow(h, SW_HIDE);
                }
            }
            Effect::ClearItems => edit(|s| {
                s.boxes = Vec::new();
                s.items = Vec::new();
                s.filtered = Vec::new();
                s.top = 0;
                s.committed.clear();
                s.comp.clear();
            }),
            Effect::Read { seq, context } => read(seq, context),
            Effect::Filter => refilter(),
            Effect::Draw => draw_later(),
            Effect::Notice(message) => notify(&message),
            Effect::Open { seq, item_id } => open_item(seq, item_id, crate::open::Action::Open),
            Effect::Locate {
                seq,
                item_id,
                origin,
            } => open_item(
                seq,
                item_id,
                if origin {
                    crate::open::Action::LocateOrigin
                } else {
                    crate::open::Action::LocateCopy
                },
            ),
            Effect::PersistLayout(layout) => {
                reload_virtual();
                if let Err(error) = persist_layout(&layout) {
                    notify(&format!("패널 설정 저장 실패: {error}"));
                }
            }
            Effect::PersistVirtualOperation {
                seq,
                operation_id,
                kind,
            } => {
                let result = persist_virtual(&operation_id, &kind);
                if result.is_ok() {
                    let label = match &kind {
                        vf_state::VirtualOperationKind::Move { .. } => "이동됨".to_string(),
                        vf_state::VirtualOperationKind::Copy { .. } => {
                            vf_store::Store::open(&store_path())
                                .ok()
                                .and_then(|s| s.journal().ok())
                                .and_then(|j| {
                                    j.into_iter()
                                        .find(|e| e.operation_id.starts_with(&operation_id))
                                        .map(|e| format!("복제됨({})", e.display_name))
                                })
                                .unwrap_or_else(|| "복제됨".into())
                        }
                    };
                    edit(|s| s.last_operation = label);
                }
                reload_virtual();
                event(Event::VirtualOperationCompleted { seq, result });
            }
            Effect::PersistVirtualHistory {
                seq,
                operation_id,
                redo,
            } => {
                let result = (|| {
                    let store = vf_store::Store::open(&store_path())?;
                    if redo {
                        store.redo(&operation_id)
                    } else {
                        let entries = store.journal()?;
                        if entries.iter().any(|entry| {
                            entry.real_copy
                                && (entry.operation_id == operation_id
                                    || entry.operation_id.starts_with(&format!("{operation_id}:")))
                        }) {
                            match store.undo_real_copy(&operation_id, false) {
                                Ok(()) => Ok(()),
                                Err(error) if error.contains("변경된 파일") => {
                                    let title = wide("실제 복사 되돌리기 확인");
                                    let body =
                                        wide(&format!("{error}\n\n이 파일을 삭제하시겠습니까?"));
                                    if unsafe {
                                        MessageBoxW(
                                            with(|s| s.hwnd),
                                            body.as_ptr(),
                                            title.as_ptr(),
                                            MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
                                        )
                                    } == IDYES
                                    {
                                        store.undo_real_copy(&operation_id, true)
                                    } else {
                                        Err("되돌리기를 취소했습니다".into())
                                    }
                                }
                                Err(error) => Err(error),
                            }
                        } else {
                            store.undo(&operation_id)
                        }
                    }
                })();
                if result.is_ok() {
                    edit(|s| {
                        s.last_operation = if redo {
                            "다시 실행됨".into()
                        } else {
                            "되돌림".into()
                        }
                    });
                }
                reload_virtual();
                event(Event::VirtualOperationCompleted { seq, result });
            }
            Effect::ImportExternal {
                target_panel,
                paths,
            } => {
                let result = (|| {
                    let store = vf_store::Store::open(&store_path())?;
                    store.import_paths(
                        &target_panel,
                        &paths,
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64,
                    )
                })();
                match result {
                    Ok(report) => {
                        if !report.added.is_empty() {
                            edit(|s| {
                                s.last_operation =
                                    format!("{}개 외부 항목 추가", report.added.len())
                            });
                        }
                        let mut messages = vec![];
                        if !report.duplicates.is_empty() {
                            messages.push(format!("중복 경로 {}개", report.duplicates.len()))
                        }
                        messages.extend(report.errors);
                        if !messages.is_empty() {
                            notify(&messages.join("\n"));
                        }
                        event(Event::Refresh);
                    }
                    Err(e) => notify(&format!("외부 항목 등록 실패: {e}")),
                }
            }
            Effect::CopyRealFiles {
                seq,
                operation_id,
                location,
                sources,
                target_folder,
            } => {
                let result = vf_store::Store::open(&store_path()).and_then(|store| {
                    store
                        .real_copy(
                            &operation_id,
                            &sources,
                            std::path::Path::new(&target_folder),
                            &location,
                            SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs() as i64,
                        )
                        .map(|_| ())
                });
                if let Err(e) = &result {
                    notify(&format!("실제 파일 복사 실패: {e}"))
                } else {
                    edit(|s| s.last_operation = "실제 파일 복사 완료".into())
                }
                reload_virtual();
                event(Event::VirtualOperationCompleted { seq, result });
                event(Event::Refresh);
            }
        }
    }
    let (v, _, _, _) = snapshot();
    let _ = crate::status(&v);
}
fn layout_path() -> std::path::PathBuf {
    crate::paths::data_dir().join("settings.json")
}
fn store_path() -> std::path::PathBuf {
    crate::paths::data_dir().join("virtual.db")
}
fn persist_layout(layout: &crate::vf_layout::Layout) -> Result<(), String> {
    let p = layout_path();
    let mut value = Settings::load(&p).value();
    if let crate::json::Value::Obj(ref mut entries) = value {
        entries.retain(|(k, _)| {
            !matches!(
                k.as_str(),
                "layout_version" | "panel_count" | "layout" | "active_panel" | "split" | "panels"
            )
        });
        if let crate::json::Value::Obj(layout) = layout.value() {
            entries.extend(layout);
        }
    }
    crate::settings::write_changed(&p, &crate::json::to_string(&value))
        .map(|_| ())
        .map_err(|e| e.to_string())
}
fn load_layout() -> crate::vf_layout::Layout {
    let p = layout_path();
    match std::fs::read_to_string(&p) {
        Ok(text)
            if crate::json::parse(&text)
                .and_then(|v| v.num("panel_count"))
                .is_some() =>
        {
            crate::vf_layout::Layout::parse(&text)
        }
        Ok(text) => crate::vf_layout::Layout::migrate_legacy(&text),
        Err(_) => crate::vf_layout::Layout::default(),
    }
}
fn latest_operation_id() -> String {
    vf_store::Store::open(&store_path())
        .ok()
        .and_then(|s| s.journal().ok())
        .and_then(|v| {
            v.first().map(|e| {
                e.operation_id
                    .split(":")
                    .next()
                    .unwrap_or(&e.operation_id)
                    .to_string()
            })
        })
        .unwrap_or_default()
}
fn persist_virtual(
    operation_id: &str,
    kind: &vf_state::VirtualOperationKind,
) -> Result<(), String> {
    let store = vf_store::Store::open(&store_path())?;
    match kind {
        vf_state::VirtualOperationKind::Move {
            source_panel: _,
            target_panel,
            item_ids,
        } => store.move_many(operation_id, item_ids, target_panel),
        vf_state::VirtualOperationKind::Copy {
            source_panel: _,
            target_panel,
            item_ids,
        } => store.copy_many(
            operation_id,
            item_ids,
            target_panel,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
        ),
    }
}
fn read(seq: u64, mut c: Context) {
    edit(|s| s.truncated = false);
    draw_later();
    let h = with(|s| s.hwnd);
    unsafe {
        UpdateWindow(h);
    }
    if c.box_id.is_none() {
        c.box_id = Settings::load(&crate::paths::data_dir().join("settings.json")).last_box;
    }
    let result = match vf_data::read(&crate::paths::library_dir(), c.box_id) {
        Ok(Snapshot::Loaded {
            boxes,
            box_id,
            items,
            truncated,
        }) => {
            let empty = items.is_empty();
            edit(|s| {
                s.boxes = boxes;
                s.items = items;
                s.truncated = truncated;
                s.committed = c.query.clone();
                s.comp.clear();
                s.top = 0;
            });
            // 빈 상자의 ID를 유지해야 상자 메뉴에서 다른 상자로 바꿀 수 있다.
            if empty {
                ReadResult::EmptyBox{box_id,reason:"항목이 없습니다. 다른 상자를 고르거나 앱에서 추가한 뒤 F5로 다시 불러오세요".into()}
            } else {
                ReadResult::Items { box_id }
            }
        }
        Ok(Snapshot::Empty(reason)) => {
            edit(|s| {
                s.boxes.clear();
                s.items.clear();
                s.filtered.clear();
            });
            ReadResult::Empty(
                match reason {
                    vf_data::EmptyReason::NoDatabase => {
                        "자료가 없습니다. 앱에서 자료를 추가한 뒤 F5로 다시 불러오세요"
                    }
                    _ => "상자가 없습니다. 앱에서 상자를 만든 뒤 F5로 다시 불러오세요",
                }
                .into(),
            )
        }
        Err(e) => {
            edit(|s| {
                s.items.clear();
                s.filtered.clear();
            });
            ReadResult::Failed(format!("{e} · F5로 다시"))
        }
    };
    event(Event::Loaded { seq, result });
}
fn refilter() {
    reload_virtual();
    edit(|s| {
        let c = s.model.view.context().cloned().unwrap_or_default();
        s.filtered = vf_data::filter(&s.items, c.space.as_deref(), &c.query)
            .iter()
            .map(|i| i.id)
            .collect();
        s.top = s.top.min(s.filtered.len().saturating_sub(1));
    });
    let (sel, first) = with(|s| {
        (
            s.model.view.context().and_then(|c| c.selection),
            s.filtered.first().copied(),
        )
    });
    if !with(|s| sel.is_some_and(|id| s.filtered.contains(&id))) {
        event(Event::Select(first));
    }
}
fn draw_later() {
    let h = with(|s| s.hwnd);
    if !h.is_null() {
        unsafe {
            InvalidateRect(h, null(), 0);
        }
    }
}
fn query() {
    let q = with(|s| format!("{}{}", s.committed, s.comp));
    event(Event::Query(q));
}
fn save() {
    let h = with(|s| s.hwnd);
    if h.is_null() {
        return;
    }
    let p = crate::paths::data_dir().join("settings.json");
    let mut settings = Settings::load(&p);
    settings.sticky = with(|s| s.model.sticky);
    settings.last_box =
        with(|s| s.model.view.context().and_then(|c| c.box_id)).or(settings.last_box);
    unsafe {
        if IsZoomed(h) == 0 {
            let mut r: RECT = std::mem::zeroed();
            if GetWindowRect(h, &mut r) != 0 {
                settings.rect = Some([r.left, r.top, r.right - r.left, r.bottom - r.top]);
            }
        }
    }
    if let Err(e) = settings.save(&p) {
        notify(&format!("창 설정 저장 실패: {e}"));
    }
}
fn ensure() -> Result<(), String> {
    if !with(|s| s.hwnd).is_null() {
        return Ok(());
    }
    unsafe {
        let root = std::env::var("KVF_TEST_ROOT").ok();
        let class = wide(&crate::paths::object_name(
            "HwpAutoDocFit.KvfWindow",
            root.as_deref(),
        ));
        let instance = GetModuleHandleW(null());
        let wc = WNDCLASSW {
            style: CS_DBLCLKS,
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return Err("목록 창 등록 실패".into());
        }
        let settings = Settings::load(&crate::paths::data_dir().join("settings.json"));
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        let mut monitor: MONITORINFO = std::mem::zeroed();
        monitor.cbSize = std::mem::size_of_val(&monitor) as u32;
        GetMonitorInfoW(MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST), &mut monitor);
        let r = place(settings.rect, [pt.x, pt.y], monitor.rcWork);
        let h = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            wide("퀵가상폴더").as_ptr(),
            WS_POPUP | WS_THICKFRAME | WS_MAXIMIZEBOX,
            r[0],
            r[1],
            r[2],
            r[3],
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if h.is_null() {
            return Err("목록 창 생성 실패".into());
        }
        edit(|s| {
            s.hwnd = h;
            s.model.explorer = load_layout();
        });
        frame::style_frame(h);
        topmost(h);
        Ok(())
    }
}
/// 테마 상태 전이(창 스레드): kagent 설정의 선호 + 시스템 → `ui_theme` 상태 하나. 색이 바뀌면 틀을 맞추고 다시 그린다.
/// 시작할 때·테마 메시지(시스템 변경, kagent의 DocFitThemeChanged 방송)·reload에서 부른다.
pub fn refresh_theme() {
    let change = ui_theme::refresh(crate::settings::agent_theme(&crate::paths::agent_settings()));
    let h = with(|s| s.hwnd);
    if change.look && !h.is_null() {
        unsafe {
            frame::apply_theme(h);
        }
        draw_later();
    }
}
fn place(saved: Option<[i32; 4]>, mouse: [i32; 2], work: RECT) -> [i32; 4] {
    let mut r = saved.unwrap_or([mouse[0] + 12, mouse[1] + 12, 600, 420]);
    r[2] = r[2].max(vf_ui::MIN_W).min((work.right - work.left).max(1));
    r[3] = r[3].max(vf_ui::MIN_H).min((work.bottom - work.top).max(1));
    r[0] = r[0].clamp(work.left, (work.right - r[2]).max(work.left));
    r[1] = r[1].clamp(work.top, (work.bottom - r[3]).max(work.top));
    r
}
fn topmost(h: HWND) {
    unsafe {
        SetWindowPos(
            h,
            if with(|s| s.model.sticky) {
                HWND_TOPMOST
            } else {
                HWND_NOTOPMOST
            },
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}
fn xy(l: isize) -> (i32, i32) {
    ((l as u16) as i16 as i32, ((l >> 16) as u16) as i16 as i32)
}
fn size(h: HWND) -> RECT {
    unsafe {
        let mut r = std::mem::zeroed();
        GetClientRect(h, &mut r);
        r
    }
}
fn dpi_of(h: HWND) -> i32 {
    (unsafe { windows_sys::Win32::UI::HiDpi::GetDpiForWindow(h) }).max(96) as i32
}
/// 목록 알림 문구(불러오는 중·오류·빈 상자). 비어 있으면 알림 줄을 두지 않는다.
fn message_of(s: &Ui) -> String {
    match &s.model.view {
        View::Loading(_) => "불러오는 중…".to_string(),
        View::Failed { error, .. } => error.clone(),
        View::Empty { reason, .. } => reason.clone(),
        _ if s.filtered.is_empty() && s.virt.iter().all(|v| v.is_empty()) => {
            "표시할 항목이 없습니다. 검색어를 지우거나 F5로 새로고침하세요".into()
        }
        _ => String::new(),
    }
}
/// 그리기와 클릭 판정이 함께 쓰는 사각형 묶음.
fn chrome(s: &Ui, h: HWND) -> Chrome {
    let rc = size(h);
    Chrome::compute(
        dpi_of(h),
        rc.right,
        rc.bottom,
        s.model.view.context().is_some_and(|c| !c.query.is_empty()),
        s.truncated,
        !message_of(s).is_empty(),
        &s.model.explorer,
    )
}
fn scene(s: &Ui, dpi: i32) -> Scene<'_> {
    let context = s.model.view.context();
    let name = context
        .and_then(|c| c.box_id)
        .and_then(|id| s.boxes.iter().find(|b| b.id == id))
        .map(|b| b.name.clone())
        .unwrap_or_else(|| "오늘의 업무".into());
    Scene {
        dpi,
        title: window_title(&name),
        sticky: s.model.sticky,
        layout: &s.model.explorer,
        boxes: &s.boxes,
        items: &s.items,
        filtered: &s.filtered,
        virt: &s.virt,
        top: s.top,
        query: context.map(|c| c.query.clone()).unwrap_or_default(),
        message: message_of(s),
        selected_id: context.and_then(|c| c.selection),
        truncated: s.truncated,
        last_operation: &s.last_operation,
        drag_target: s.item_drag.as_ref().and_then(|d| d.target_panel.as_deref()),
        context_box: context.and_then(|c| c.box_id),
        context_space: context.and_then(|c| c.space.as_deref()),
    }
}
fn chrome_now(h: HWND) -> Chrome {
    with(|s| chrome(s, h))
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum RowId {
    Library(i64),
    Virtual(String),
}
fn row_id(s: &Ui, h: HWND, panel: usize, index: usize) -> Option<RowId> {
    match scene(s, dpi_of(h)).row(panel, index)? {
        RowRef::Library(i) => Some(RowId::Library(i.id)),
        RowRef::Virtual(v) => Some(RowId::Virtual(v.id.clone())),
    }
}
/// 점이 가리키는 (영역 번호, 그 영역 안의 항목 번호). 영역 밖이면 None, 빈 곳이면 항목 번호가 None.
fn locate(h: HWND, x: i32, y: i32) -> Option<(usize, Option<usize>)> {
    with(|s| {
        let ch = chrome(s, h);
        let panel = ch.panel_at(x, y)?;
        let len = scene(s, ch.dpi).rows_len(panel);
        Some((panel, ch.panels[panel].row_hit(x, y, s.top, len)))
    })
}
fn activate(panel: usize) {
    let id = format!("p{}", panel + 1);
    if with(|s| s.model.explorer.active_panel != id) {
        event(Event::ActivatePanel(id));
    }
    edit(|s| s.active_panel = panel);
}
/// 영역의 항목을 눌렀을 때: 자료함 항목은 기존 선택, 가상 항목은 영역 선택(Ctrl이면 추가·해제).
fn choose_row(h: HWND, panel: usize, index: usize, ctrl: bool) {
    match with(|s| row_id(s, h, panel, index)) {
        Some(RowId::Library(_)) => choose(index - with(|s| scene(s, dpi_of(h)).virtual_len(panel))),
        Some(RowId::Virtual(id)) => {
            event(Event::Select(None));
            let panel_id = format!("p{}", panel + 1);
            let mut selection = with(|s| {
                s.model.explorer.panels.iter().find(|p| p.id == panel_id).map(|p| p.selection.clone()).unwrap_or_default()
            });
            if ctrl {
                if let Some(at) = selection.iter().position(|v| *v == id) {
                    selection.remove(at);
                } else {
                    selection.push(id);
                }
            } else if !selection.contains(&id) {
                selection = vec![id];
            }
            event(Event::SelectPanel { panel_id, selection });
        }
        None => {}
    }
}
/// 가상 항목이 가리키는 파일·폴더를 연결 프로그램으로 연다(탐색기와 같은 동작).
fn open_virtual(h: HWND, panel: usize, index: usize) {
    let Some(RowId::Virtual(id)) = with(|s| row_id(s, h, panel, index)) else { return };
    let source = with(|s| s.virt.get(panel).and_then(|v| v.iter().find(|i| i.id == id).map(|i| i.source_ref.clone())));
    let Some(source) = source else { return };
    if !std::path::Path::new(&source).exists() {
        notify("원본 파일이 옮겨졌거나 지워졌습니다");
        return;
    }
    unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(h, wide("open").as_ptr(), wide(&source).as_ptr(), null(), null(), SW_SHOWNORMAL);
    }
}
/// 영역별 가상 항목을 저장소에서 다시 읽는다. 저장소 열기 실패는 빈 목록으로 본다(그리기는 이 값만 쓴다).
fn reload_virtual() {
    let count = with(|s| s.model.explorer.panels.len());
    let loaded = Some(store_path())
        .filter(|p| p.exists())
        .ok_or(String::new())
        .and_then(|p| vf_store::Store::open(&p))
        .map(|store| (0..count).map(|i| store.list(&format!("p{}", i + 1)).unwrap_or_default()).collect::<Vec<_>>())
        .unwrap_or_default();
    edit(|s| s.virt = loaded);
    draw_later();
}
fn choose(index: usize) {
    let id = with(|s| s.filtered.get(index).copied());
    event(Event::Select(id));
    if let Some(id) = id {
        let panel = with(|s| s.model.explorer.active_panel.clone());
        let mut selection = with(|s| {
            s.model
                .explorer
                .panels
                .iter()
                .find(|p| p.id == panel)
                .map(|p| p.selection.clone())
                .unwrap_or_default()
        });
        let value = id.to_string();
        if !selection.contains(&value) {
            selection.clear();
            selection.push(value);
        }
        event(Event::SelectPanel {
            panel_id: panel,
            selection,
        });
    }
}
fn panel_at(
    layout: &crate::vf_layout::Layout,
    width: i32,
    height: i32,
    gutter: i32,
    x: i32,
    y: i32,
) -> Option<String> {
    let rects = layout.panel_rects(width, height, gutter);
    rects
        .iter()
        .position(|r| x >= r.left && x < r.right && y >= r.top && y < r.bottom)
        .map(|i| format!("p{}", i + 1))
}
fn drag_decision(target: Option<String>, ctrl: bool) -> Option<(String, bool)> {
    target.map(|panel| (panel, ctrl))
}
fn operation_menu_labels() -> [&'static str; 3] {
    [
        "다른 영역으로 이동 ▸",
        "다른 영역으로 복제 ▸",
        "복사본 만들기",
    ]
}
fn request_selected_real_copy(h: HWND) {
    let (location, ids) = selected_virtual_items();
    let store = match vf_store::Store::open(&store_path()) {
        Ok(s) => s,
        Err(e) => {
            notify(&e);
            return;
        }
    };
    let selected = match store.list(&location) {
        Ok(v) => v
            .into_iter()
            .filter(|i| ids.contains(&i.id))
            .collect::<Vec<_>>(),
        Err(e) => {
            notify(&e);
            return;
        }
    };
    if selected.is_empty() {
        notify("실제 복사할 항목을 선택하세요.");
        return;
    }
    let target = std::path::PathBuf::from(selected[0].source_ref.as_str())
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let title = wide("실제 파일 복사 확인");
    let body = wide(&format!(
        "대상 폴더: {}\n항목 수: {}\n\n실제 파일을 복사하시겠습니까?",
        target.display(),
        selected.len()
    ));
    unsafe {
        if MessageBoxW(
            h,
            body.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
        ) == IDYES
        {
            let sources = selected.iter().map(|i| i.source_ref.clone()).collect();
            let operation_id = format!(
                "real-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            );
            event(Event::RequestRealCopy {
                operation_id,
                location,
                sources,
                target_folder: target.to_string_lossy().into_owned(),
                confirmed: true,
            });
        }
    }
}
fn selected_virtual_items() -> (String, Vec<String>) {
    let (source, selection) = with(|s| {
        let source = s.model.explorer.active_panel.clone();
        let ids = s
            .model
            .explorer
            .panels
            .iter()
            .find(|p| p.id == source)
            .map(|p| p.selection.clone())
            .unwrap_or_default();
        (source, ids)
    });
    let stored = vf_store::Store::open(&store_path())
        .and_then(|store| store.list(&source))
        .unwrap_or_default();
    let ids = selection
        .into_iter()
        .filter(|id| stored.iter().any(|item| item.id == *id))
        .collect();
    (source, ids)
}
fn request_virtual(target: String, copy: bool) {
    let (source, item_ids) = selected_virtual_items();
    request_virtual_items(source, target, item_ids, copy);
}
fn request_virtual_items(source: String, target: String, item_ids: Vec<String>, copy: bool) {
    if item_ids.is_empty() {
        notify("선택한 가상 항목이 없습니다.");
        return;
    }
    let operation_id = format!(
        "ui-{}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        item_ids.len()
    );
    if copy && source == target {
        let result = (|| {
            let store = vf_store::Store::open(&store_path())?;
            store.copy_many(
                &operation_id,
                &item_ids,
                &target,
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
            )
        })();
        match result {
            Ok(()) => {
                let name = vf_store::Store::open(&store_path())
                    .ok()
                    .and_then(|s| s.journal().ok())
                    .and_then(|j| {
                        j.into_iter()
                            .find(|e| e.operation_id.starts_with(&operation_id))
                            .map(|e| e.display_name)
                    })
                    .unwrap_or_default();
                edit(|s| s.last_operation = format!("복제됨({name})"));
                reload_virtual();
            }
            Err(e) => notify(&e),
        }
        return;
    }
    event(if copy {
        Event::RequestVirtualCopy {
            operation_id,
            source_panel: source,
            target_panel: target,
            item_ids,
        }
    } else {
        Event::RequestVirtualMove {
            operation_id,
            source_panel: source,
            target_panel: target,
            item_ids,
        }
    });
}
fn set_view_mode(icons: bool) {
    let layout = edit(|s| {
        let id = s.model.explorer.active_panel.clone();
        if let Some(p) = s.model.explorer.panels.iter_mut().find(|p| p.id == id) {
            p.view = if icons { "icons" } else { "details" }.into();
        }
        s.model.explorer.clone()
    });
    let _ = persist_layout(&layout);
    draw_later();
}
fn configure_panels(h: HWND, menu: bool) {
    if !menu {
        return;
    }
    unsafe {
        let m = CreatePopupMenu();
        for (n, label) in [
            (1u8, "영역 1개"),
            (2, "좌우 2개"),
            (3, "좌1 + 우2"),
            (4, "2 × 2"),
        ] {
            AppendMenuW(
                m,
                MF_STRING
                    | if with(|s| s.model.explorer.panel_count == n) {
                        MF_CHECKED
                    } else {
                        0
                    },
                n as usize,
                wide(label).as_ptr(),
            );
        }
        let button = chrome_now(h).toolbar_buttons[0];
        let mut p = POINT {
            x: button.left,
            y: button.bottom,
        };
        ClientToScreen(h, &mut p);
        let cmd = TrackPopupMenu(m, TPM_RETURNCMD, p.x, p.y, 0, h, null());
        DestroyMenu(m);
        if (1..=4).contains(&cmd) {
            event(Event::SetPanelCount(cmd as u8));
        }
    }
}
fn drag_split(h: HWND, x: i32, y: i32) {
    let hit = with(|s| chrome(s, h).split_hit(&s.model.explorer, x, y));
    let Some(hit) = hit else { return };
    edit(|s| s.drag_split = Some(hit));
    unsafe {
        SetCapture(h);
    }
}
fn update_split(h: HWND, x: i32, y: i32) {
    let ch = chrome_now(h);
    let gutter = ch.gutter;
    let changed = edit(|s| {
        let Some(hit) = s.drag_split else {
            return false;
        };
        let l = &mut s.model.explorer;
        match hit {
            crate::vf_layout::SplitHit::Column(_) => {
                let ratio = vf_ui::split_ratio(x - ch.body.left - gutter / 2, ch.body.width() - gutter, vf_ui::sc(ch.dpi, vf_ui::PANEL_MIN_W));
                l.column_splits = vec![ratio, 10000 - ratio];
            }
            crate::vf_layout::SplitHit::Row(_) => {
                let ratio = vf_ui::split_ratio(y - ch.body.top - gutter / 2, ch.body.height() - gutter, vf_ui::sc(ch.dpi, vf_ui::ROW * 3));
                l.row_splits = vec![ratio, 10000 - ratio];
            }
        }
        true
    });
    if changed {
        draw_later();
    }
}
/// 활성 영역에서 한 쪽에 보이는 줄 수.
fn page_rows(h: HWND) -> usize {
    with(|s| {
        let panel = s.model.explorer.panels.iter().position(|p| p.id == s.model.explorer.active_panel).unwrap_or(0);
        chrome(s, h).capacity_of(panel)
    })
}
fn move_selection(h: HWND, delta: isize) {
    // 활성 영역의 줄 전체(자료함 항목 뒤에 가상 항목)에서 선택을 옮긴다.
    let (panel, rows, current, len) = with(|s| {
        let ch = chrome(s, h);
        let panel = s.model.explorer.panels.iter().position(|p| p.id == s.model.explorer.active_panel).unwrap_or(0);
        let sc = scene(s, ch.dpi);
        let len = sc.rows_len(panel);
        let library = s.model.view.context().and_then(|c| c.selection).and_then(|id| sc.filtered.iter().position(|v| *v == id)).filter(|_| sc.message.is_empty());
        let virtual_at = s.model.explorer.panels.get(panel).and_then(|p| {
            p.selection.first().and_then(|id| s.virt.get(panel).and_then(|v| v.iter().position(|i| i.id == *id)))
        });
        let current = virtual_at.or(library.map(|i| sc.virtual_len(panel) + i)).unwrap_or(0);
        (panel, ch.capacity_of(panel), current, len)
    });
    if len == 0 {
        return;
    }
    let i = (current as isize + delta).clamp(0, len as isize - 1) as usize;
    edit(|s| {
        if i < s.top {
            s.top = i;
        }
        if i >= s.top + rows {
            s.top = i + 1 - rows;
        }
    });
    choose_row(h, panel, i, false);
}
fn change_tab(delta: isize) {
    let i = with(|s| {
        SPACES
            .iter()
            .position(|v| *v == s.model.view.context().and_then(|c| c.space.as_deref()))
            .unwrap_or(0)
    });
    edit(|s| {
        s.committed.clear();
        s.comp.clear();
        s.top = 0;
    });
    event(Event::ChangeSpace(
        SPACES[(i as isize + delta).rem_euclid(5) as usize].map(String::from),
    ));
}
fn boxes_menu(h: HWND) {
    unsafe {
        let m = CreatePopupMenu();
        with(|s| {
            for (i, b) in s.boxes.iter().enumerate() {
                AppendMenuW(m, MF_STRING, 100 + i, wide(&b.name).as_ptr());
            }
        });
        let mut pt = POINT {
            x: 0,
            y: chrome_now(h).head.bottom,
        };
        ClientToScreen(h, &mut pt);
        let n = TrackPopupMenu(m, TPM_RETURNCMD, pt.x, pt.y, 0, h, null());
        DestroyMenu(m);
        if n >= 100 {
            let id = with(|s| s.boxes.get(n as usize - 100).map(|b| b.id));
            if let Some(id) = id {
                event(Event::ChangeBox(id));
            }
        }
    }
}
fn virtual_item_menu(h: HWND, l: isize) {
    unsafe {
        let source = with(|s| s.model.explorer.active_panel.clone());
        let ids = selected_virtual_items().1;
        if ids.is_empty() {
            return;
        }
        let menu = CreatePopupMenu();
        let move_menu = CreatePopupMenu();
        let copy_menu = CreatePopupMenu();
        let panels = with(|s| s.model.explorer.panels.clone());
        for (i, _) in panels.iter().enumerate() {
            let label = format!("영역 {}", i + 1);
            AppendMenuW(move_menu, MF_STRING, 200 + i, wide(&label).as_ptr());
            AppendMenuW(copy_menu, MF_STRING, 300 + i, wide(&label).as_ptr());
        }
        AppendMenuW(
            menu,
            MF_POPUP,
            move_menu as usize,
            wide(operation_menu_labels()[0]).as_ptr(),
        );
        AppendMenuW(
            menu,
            MF_POPUP,
            copy_menu as usize,
            wide(operation_menu_labels()[1]).as_ptr(),
        );
        AppendMenuW(
            menu,
            MF_STRING,
            4,
            wide(operation_menu_labels()[2]).as_ptr(),
        );
        AppendMenuW(menu, MF_STRING, 5, wide("실제 파일 복사…").as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, 1, wide("열기").as_ptr());
        AppendMenuW(
            menu,
            MF_STRING,
            2,
            wide("탐색기에서 위치 열기\tCtrl+Enter").as_ptr(),
        );
        let (x, y) = xy(l);
        let mut p = POINT { x, y };
        ClientToScreen(h, &mut p);
        let command = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            p.x,
            p.y,
            0,
            h,
            null(),
        );
        DestroyMenu(menu);
        if (200..204).contains(&command) {
            request_virtual(format!("p{}", command - 199), false);
        } else if (300..304).contains(&command) {
            request_virtual(format!("p{}", command - 299), true);
        } else if command == 4 {
            request_virtual(source, true);
        } else if command == 5 {
            request_selected_real_copy(h);
        } else if command == 1 {
            event(Event::Open);
        } else if command == 2 {
            event(Event::LocateCopy);
        }
    }
}
unsafe fn paint(h: HWND) {
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let dc = BeginPaint(h, &mut ps);
        let rc = size(h);
        if rc.right <= 0 || rc.bottom <= 0 {
            EndPaint(h, &ps);
            return;
        }
        let mem = CreateCompatibleDC(dc);
        let bmp = CreateCompatibleBitmap(dc, rc.right, rc.bottom);
        let old = SelectObject(mem, bmp);
        with(|s| {
            let sc = scene(s, dpi_of(h));
            let ch = chrome(s, h);
            crate::vf_paint::draw(mem, &sc, &ch);
        });
        BitBlt(dc, 0, 0, rc.right, rc.bottom, mem, 0, 0, SRCCOPY);
        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
        EndPaint(h, &ps);
    }
}
unsafe extern "system" fn wndproc(h: HWND, msg: u32, w: usize, l: isize) -> isize {
    unsafe {
        match msg {
            WM_CREATE => {
                DragAcceptFiles(h, 1);
                0
            }
            WM_DROPFILES => {
                let drop = w as HDROP;
                let count = DragQueryFileW(drop, u32::MAX, null_mut(), 0);
                let mut paths = Vec::new();
                for i in 0..count {
                    let n = DragQueryFileW(drop, i, null_mut(), 0) as usize;
                    let mut buf = vec![0u16; n + 1];
                    DragQueryFileW(drop, i, buf.as_mut_ptr(), buf.len() as u32);
                    paths.push(String::from_utf16_lossy(&buf[..n]));
                }
                let pt = POINT { x: 0, y: 0 };
                let mut pt = pt;
                DragQueryPoint(drop, &mut pt);
                DragFinish(drop);
                if let Some(panel) = chrome_now(h).panel_at(pt.x, pt.y).map(|i| format!("p{}", i + 1)) {
                    event(Event::ActivatePanel(panel.clone()));
                    event(Event::DropExternal {
                        target_panel: panel,
                        paths,
                    });
                }
                0
            }
            WM_PAINT => {
                paint(h);
                0
            }
            WM_ERASEBKGND => 1,
            WM_NCCALCSIZE if w != 0 => 0,
            WM_NCHITTEST => {
                let (x, y) = xy(l);
                let mut p = POINT { x, y };
                ScreenToClient(h, &mut p);
                let r = size(h);
                let ch = chrome_now(h);
                frame::frame_hit(
                    p.x,
                    p.y,
                    r.right,
                    r.bottom,
                    frame::scale(h, 6),
                    ch.head.bottom,
                    ch.head_interactive(p.x, p.y),
                    IsZoomed(h) != 0,
                    false,
                )
                .code() as isize
            }
            WM_NCLBUTTONDBLCLK if w == HTCAPTION as usize => {
                ShowWindow(
                    h,
                    if IsZoomed(h) != 0 {
                        SW_RESTORE
                    } else {
                        SW_MAXIMIZE
                    },
                );
                0
            }
            WM_CLOSE => {
                event(Event::Close);
                0
            }
            WM_ACTIVATE if (w & 0xffff) as u32 == WA_INACTIVE => {
                let mut pid = 0;
                GetWindowThreadProcessId(l as HWND, &mut pid);
                if pid != std::process::id() {
                    event(Event::Outside);
                }
                0
            }
            WM_GETMINMAXINFO => {
                let p = &mut *(l as *mut MINMAXINFO);
                p.ptMinTrackSize = POINT {
                    x: frame::scale(h, vf_ui::MIN_W),
                    y: frame::scale(h, vf_ui::MIN_H),
                };
                0
            }
            WM_SIZE => {
                draw_later();
                0
            }
            WM_EXITSIZEMOVE => {
                save();
                0
            }
            WM_DESTROY => {
                DragAcceptFiles(h, 0);
                edit(|s| s.hwnd = null_mut());
                0
            }
            WM_LBUTTONDOWN => {
                let (x, y) = xy(l);
                let ch = chrome_now(h);
                if ch.body.contains(x, y) {
                    // 분할선(마우스 모양이 바뀌는 곳)이 먼저다. 그 밖의 영역 위는 줄 선택·끌어놓기 시작.
                    if with(|s| ch.split_hit(&s.model.explorer, x, y)).is_some() {
                        drag_split(h, x, y);
                        return 0;
                    }
                    if let Some((panel, row)) = locate(h, x, y) {
                        activate(panel);
                        if let Some(row) = row {
                            choose_row(h, panel, row, GetKeyState(VK_CONTROL as i32) < 0);
                            let (source, item_ids) = selected_virtual_items();
                            if !item_ids.is_empty() {
                                edit(|s| {
                                    s.item_drag = Some(ItemDrag {
                                        source_panel: source,
                                        item_ids,
                                        start: (x, y),
                                        target_panel: None,
                                        copy: false,
                                    })
                                });
                                SetCapture(h);
                            }
                        }
                    }
                } else {
                    edit(|s| s.pressed = ch.window_button(x, y));
                }
                0
            }
            WM_MOUSEMOVE => {
                let (x, y) = xy(l);
                if with(|s| s.drag_split.is_some()) {
                    update_split(h, x, y);
                } else if with(|s| s.item_drag.is_some()) {
                    let target = chrome_now(h).panel_at(x, y).map(|i| format!("p{}", i + 1));
                    edit(|s| {
                        if let Some(d) = s.item_drag.as_mut() {
                            d.target_panel = target;
                            d.copy = GetKeyState(VK_CONTROL as i32) < 0;
                        }
                    });
                    SetCursor(LoadCursorW(
                        null_mut(),
                        if GetKeyState(VK_CONTROL as i32) < 0 {
                            IDC_CROSS
                        } else {
                            IDC_HAND
                        },
                    ));
                    draw_later();
                } else {
                    let hit = with(|s| chrome(s, h).split_hit(&s.model.explorer, x, y));
                    if let Some(hit) = hit {
                        let cursor = if matches!(hit, crate::vf_layout::SplitHit::Column(_)) {
                            32644usize
                        } else {
                            32645usize
                        };
                        SetCursor(LoadCursorW(null_mut(), cursor as *const u16));
                    } else {
                        SetCursor(LoadCursorW(null_mut(), IDC_ARROW));
                    }
                }
                0
            }
            WM_LBUTTONUP => {
                let (x, y) = xy(l);
                if let Some(drag) = edit(|s| s.item_drag.take()) {
                    ReleaseCapture();
                    let target = chrome_now(h)
                        .panel_at(x, y)
                        .map(|i| format!("p{}", i + 1))
                        .or(drag.target_panel.clone());
                    if let Some((panel, copy)) =
                        drag_decision(target, GetKeyState(VK_CONTROL as i32) < 0)
                    {
                        if panel != drag.source_panel {
                            request_virtual_items(drag.source_panel, panel, drag.item_ids, copy);
                        }
                    }
                    draw_later();
                    return 0;
                }
                if with(|s| s.drag_split.is_some()) {
                    let layout = edit(|s| s.drag_split.take().map(|_| s.model.explorer.clone()));
                    ReleaseCapture();
                    if let Some(layout) = layout {
                        let _ = persist_layout(&layout);
                    }
                    save();
                    draw_later();
                    return 0;
                }
                let ch = chrome_now(h);
                let pressed = edit(|s| s.pressed.take());
                let b = ch.window_button(x, y);
                if b.is_some() || pressed.is_some() {
                    if b == pressed {
                        match b {
                            Some(0) => event(Event::Close),
                            Some(1) => {
                                ShowWindow(
                                    h,
                                    if IsZoomed(h) != 0 {
                                        SW_RESTORE
                                    } else {
                                        SW_MAXIMIZE
                                    },
                                );
                            }
                            Some(2) => event(Event::Minimize),
                            Some(3) => {
                                let on = with(|s| !s.model.sticky);
                                event(Event::SetSticky(on));
                                topmost(h);
                                save();
                                draw_later();
                            }
                            _ => {}
                        }
                    }
                } else if ch.title_menu(x, y) {
                    boxes_menu(h);
                } else if let Some(index) = ch.toolbar_button(x, y) {
                    match index {
                        0 => configure_panels(h, true),
                        1 => set_view_mode(false),
                        2 => set_view_mode(true),
                        _ => event(Event::Refresh),
                    }
                    draw_later();
                } else if let Some(i) = ch.tab(x, y) {
                    edit(|s| {
                        s.committed.clear();
                        s.comp.clear();
                        s.top = 0;
                    });
                    event(Event::ChangeSpace(SPACES[i].map(String::from)));
                }
                0
            }
            WM_LBUTTONDBLCLK => {
                let (x, y) = xy(l);
                if let Some((panel, Some(row))) = locate(h, x, y) {
                    activate(panel);
                    choose_row(h, panel, row, false);
                    match with(|s| row_id(s, h, panel, row)) {
                        Some(RowId::Library(_)) => event(Event::Open),
                        Some(RowId::Virtual(_)) => open_virtual(h, panel, row),
                        None => {}
                    }
                }
                0
            }
            WM_RBUTTONUP => {
                let (x, y) = xy(l);
                if let Some((panel, Some(row))) = locate(h, x, y) {
                    activate(panel);
                    choose_row(h, panel, row, false);
                    if selected_virtual_items().1.is_empty() {
                        item_menu(h, l);
                    } else {
                        virtual_item_menu(h, l);
                    }
                }
                0
            }
            WM_MOUSEWHEEL => {
                move_selection(h, if ((w >> 16) as u16) as i16 > 0 { -3 } else { 3 });
                0
            }
            WM_KEYDOWN => {
                match w as u16 {
                    VK_ESCAPE => {
                        edit(|s| {
                            s.committed.clear();
                            s.comp.clear();
                        });
                        event(Event::Escape);
                    }
                    VK_F5 => event(Event::Refresh),
                    VK_Z if GetKeyState(VK_CONTROL as i32) < 0 => event(Event::RequestUndo {
                        operation_id: latest_operation_id(),
                    }),
                    VK_Y if GetKeyState(VK_CONTROL as i32) < 0 => event(Event::RequestRedo {
                        operation_id: latest_operation_id(),
                    }),
                    VK_BACK => {
                        edit(|s| {
                            s.committed.pop();
                            s.comp.clear();
                        });
                        query();
                    }
                    VK_RETURN => event(if GetKeyState(VK_CONTROL as i32) < 0 {
                        Event::LocateCopy
                    } else {
                        Event::Open
                    }),
                    VK_LEFT => change_tab(-1),
                    VK_RIGHT => change_tab(1),
                    VK_TAB => change_tab(if GetKeyState(VK_SHIFT as i32) < 0 {
                        -1
                    } else {
                        1
                    }),
                    VK_UP => move_selection(h, -1),
                    VK_DOWN => move_selection(h, 1),
                    VK_PRIOR => move_selection(h, -(page_rows(h) as isize)),
                    VK_NEXT => move_selection(h, page_rows(h) as isize),
                    VK_HOME => move_selection(h, -100000),
                    VK_END => move_selection(h, 100000),
                    _ => {}
                }
                0
            }
            WM_CHAR => {
                if let Some(ch) = char::from_u32(w as u32).filter(|c| !c.is_control()) {
                    edit(|s| {
                        if s.committed.chars().count() < 256 {
                            s.committed.push(ch);
                        }
                        s.comp.clear();
                    });
                    query();
                }
                0
            }
            WM_IME_COMPOSITION => {
                if l as u32 & GCS_COMPSTR != 0 {
                    let imc = ImmGetContext(h);
                    if !imc.is_null() {
                        let n = ImmGetCompositionStringW(imc, GCS_COMPSTR, null_mut(), 0).max(0)
                            as usize;
                        let mut buf = vec![0u16; n / 2];
                        if n > 0 {
                            ImmGetCompositionStringW(
                                imc,
                                GCS_COMPSTR,
                                buf.as_mut_ptr() as _,
                                n as u32,
                            );
                        }
                        ImmReleaseContext(h, imc);
                        edit(|s| s.comp = String::from_utf16_lossy(&buf));
                        query();
                    }
                }
                DefWindowProcW(h, msg, w, l)
            }
            WM_IME_ENDCOMPOSITION => {
                edit(|s| s.comp.clear());
                query();
                DefWindowProcW(h, msg, w, l)
            }
            _ => DefWindowProcW(h, msg, w, l),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_offscreen_clamps() {
        assert_eq!(
            place(
                Some([9999, -9999, 600, 420]),
                [0, 0],
                RECT {
                    left: 0,
                    top: 0,
                    right: 1920,
                    bottom: 1080
                }
            ),
            [1320, 0, 600, 420]
        );
    }
    #[test]
    fn first_position_near_mouse() {
        assert_eq!(
            place(
                None,
                [100, 100],
                RECT {
                    left: 0,
                    top: 0,
                    right: 1920,
                    bottom: 1080
                }
            ),
            [112, 112, 600, 420]
        );
    }
    #[test]
    fn tabs_match_data_spaces() {
        assert_eq!(
            SPACES,
            [
                None,
                Some("core"),
                Some("basis"),
                Some("reference"),
                Some("temp")
            ]
        );
    }
    #[test]
    fn window_title_formats_correctly() {
        assert_eq!(window_title("상자 선택"), "퀵가상폴더 · 상자 선택 ▾");
        assert_eq!(window_title("업무"), "퀵가상폴더 · 업무 ▾");
    }
}

#[cfg(test)]
mod virtual_ui_tests {
    use super::*;
    #[test]
    fn drop_hit_first_area() {
        assert_eq!(
            panel_at(
                &crate::vf_layout::Layout::for_count(2),
                1000,
                600,
                6,
                20,
                20
            ),
            Some("p1".into())
        );
    }
    #[test]
    fn drop_hit_second_area() {
        assert_eq!(
            panel_at(
                &crate::vf_layout::Layout::for_count(2),
                1000,
                600,
                6,
                700,
                20
            ),
            Some("p2".into())
        );
    }
    #[test]
    fn drop_hit_gutter_is_none() {
        assert_eq!(
            panel_at(
                &crate::vf_layout::Layout::for_count(2),
                1000,
                600,
                6,
                500,
                20
            ),
            None
        );
    }
    #[test]
    fn drop_hit_three_panel_lower_area() {
        assert_eq!(
            panel_at(
                &crate::vf_layout::Layout::for_count(3),
                1000,
                600,
                6,
                700,
                500
            ),
            Some("p3".into())
        );
    }
    #[test]
    fn drag_without_ctrl_moves() {
        assert_eq!(
            drag_decision(Some("p2".into()), false),
            Some(("p2".into(), false))
        );
    }
    #[test]
    fn drag_with_ctrl_copies() {
        assert_eq!(
            drag_decision(Some("p2".into()), true),
            Some(("p2".into(), true))
        );
    }
    #[test]
    fn dropping_outside_panels_is_ignored() {
        assert_eq!(drag_decision(None, true), None);
    }
    #[test]
    fn context_menu_has_three_requested_actions() {
        assert_eq!(
            operation_menu_labels(),
            [
                "다른 영역으로 이동 ▸",
                "다른 영역으로 복제 ▸",
                "복사본 만들기"
            ]
        );
    }
    #[test]
    fn ctrl_z_starts_undo_transition() {
        let (m, e) = vf_state::step(
            &Model::default(),
            Event::RequestUndo {
                operation_id: "op".into(),
            },
        );
        assert!(m.pending_operation.is_some());
        assert!(matches!(
            e.as_slice(),
            [Effect::PersistVirtualHistory { redo: false, .. }]
        ));
    }
    #[test]
    fn ctrl_y_starts_redo_transition() {
        let (m, e) = vf_state::step(
            &Model::default(),
            Event::RequestRedo {
                operation_id: "op".into(),
            },
        );
        assert!(m.pending_operation.is_some());
        assert!(matches!(
            e.as_slice(),
            [Effect::PersistVirtualHistory { redo: true, .. }]
        ));
    }
    #[test]
    fn history_completion_clears_pending_operation() {
        let (m, _) = vf_state::step(
            &Model::default(),
            Event::RequestUndo {
                operation_id: "op".into(),
            },
        );
        let seq = m.pending_operation.as_ref().unwrap().seq;
        let (n, e) = vf_state::step(
            &m,
            Event::VirtualOperationCompleted {
                seq,
                result: Ok(()),
            },
        );
        assert!(n.pending_operation.is_none());
        assert_eq!(e, vec![Effect::Draw]);
    }
    #[test]
    fn history_completion_error_clears_pending_and_notifies() {
        let (m, _) = vf_state::step(
            &Model::default(),
            Event::RequestRedo {
                operation_id: "op".into(),
            },
        );
        let seq = m.pending_operation.as_ref().unwrap().seq;
        let (n, e) = vf_state::step(
            &m,
            Event::VirtualOperationCompleted {
                seq,
                result: Err("기록 없음".into()),
            },
        );
        assert!(n.pending_operation.is_none());
        assert_eq!(e, vec![Effect::Notice("기록 없음".into()), Effect::Draw]);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    struct Isolated {
        root: std::path::PathBuf,
        old_root: Option<std::ffi::OsString>,
        old_library: Option<std::ffi::OsString>,
        mutex: HANDLE,
    }
    impl Isolated {
        fn new() -> Self {
            unsafe {
                let root =
                    std::env::temp_dir().join(format!("kvf-window-test-{}", std::process::id()));
                std::fs::create_dir_all(&root).unwrap();
                let old_root = std::env::var_os("KVF_TEST_ROOT");
                let old_library = std::env::var_os("KLIB_TEST_ROOT");
                std::env::set_var("KVF_TEST_ROOT", root.join("kvf"));
                std::env::set_var("KLIB_TEST_ROOT", root.join("library"));
                std::fs::create_dir_all(root.join("library")).unwrap();
                let name = wide(&crate::paths::object_name(
                    "Local\\HwpAutoDocFit.kvf",
                    Some(root.to_str().unwrap()),
                ));
                let mutex =
                    windows_sys::Win32::System::Threading::CreateMutexW(null(), 0, name.as_ptr());
                assert!(!mutex.is_null());
                Self {
                    root,
                    old_root,
                    old_library,
                    mutex,
                }
            }
        }
    }
    impl Drop for Isolated {
        fn drop(&mut self) {
            unsafe {
                close();
                destroy();
                CloseHandle(self.mutex);
                match &self.old_root {
                    Some(v) => std::env::set_var("KVF_TEST_ROOT", v),
                    None => std::env::remove_var("KVF_TEST_ROOT"),
                };
                match &self.old_library {
                    Some(v) => std::env::set_var("KLIB_TEST_ROOT", v),
                    None => std::env::remove_var("KLIB_TEST_ROOT"),
                };
                // 고정된 시험 디렉터리만 정리한다.
                assert!(self.root.starts_with(std::env::temp_dir()));
                let _ = std::fs::remove_dir_all(&self.root);
            }
        }
    }
    #[test]
    fn isolated_window_events_and_rendering() {
        let fixture = Isolated::new();
        let db = crate::sqlite::Connection::open(
            fixture.root.join("library/library.db").to_str().unwrap(),
        )
        .unwrap();
        db.exec("CREATE TABLE box(id INTEGER PRIMARY KEY,name TEXT,description TEXT,created_at REAL,updated_at REAL); CREATE TABLE item(id INTEGER PRIMARY KEY,box_id INTEGER,kind TEXT,source_path TEXT,origin_path TEXT,managed_path TEXT,display_name TEXT,space TEXT,created_at REAL); INSERT INTO box VALUES(1,'시험 상자','',1,2),(2,'빈 상자','',1,1); INSERT INTO item VALUES(1,1,'file','','','','한글 보고','core',3),(2,1,'file','','','','한 자료','core',2),(3,1,'file','','','','영역 자료','basis',1)").unwrap();
        drop(db);
        let before_db = std::fs::read(fixture.root.join("library/library.db")).unwrap();
        configure(null_mut(), &Settings::default());
        assert!(with(|s| s.hwnd.is_null()));
        call(Caller::SecondRun);
        let h = with(|s| s.hwnd);
        assert!(!h.is_null());
        assert_eq!(snapshot().0, "list");
        assert_eq!(snapshot().2, 3);
        let pinned = Settings {
            sticky: true,
            ..Settings::default()
        };
        configure(null_mut(), &pinned);
        unsafe {
            assert_ne!(
                GetWindowLongPtrW(h, GWL_EXSTYLE) & WS_EX_TOPMOST as isize,
                0
            );
        }
        configure(null_mut(), &Settings::default());
        unsafe {
            assert_eq!(
                GetWindowLongPtrW(h, GWL_EXSTYLE) & WS_EX_TOPMOST as isize,
                0
            );
        }
        unsafe {
            SendMessageW(h, WM_ACTIVATE, WA_INACTIVE as usize, h as isize);
        }
        assert_eq!(snapshot().0, "list");
        unsafe {
            assert_ne!(IsWindowVisible(h), 0);
            UpdateWindow(h);
            SendMessageW(h, WM_KEYDOWN, VK_RIGHT as usize, 0);
        }
        assert_eq!(snapshot().2, 2);
        unsafe {
            SendMessageW(h, WM_KEYDOWN, VK_RIGHT as usize, 0);
        }
        assert_eq!(snapshot().2, 1);
        unsafe {
            SendMessageW(h, WM_KEYDOWN, VK_LEFT as usize, 0);
            SendMessageW(h, WM_CHAR, '한' as usize, 0);
        }
        assert_eq!(snapshot().2, 2);
        // 실제 IME 관측 결과와 같은 입력 경로. IME 엔진의 키 합성 자체는 실기 확인 대상.
        edit(|s| s.comp = "글".into());
        query();
        assert_eq!(snapshot().2, 1);
        assert_eq!(
            with(|s| s.model.view.context().unwrap().query.clone()),
            "한글"
        );
        unsafe {
            SendMessageW(h, WM_IME_ENDCOMPOSITION, 0, 0);
            SendMessageW(h, WM_CHAR, '글' as usize, 0);
            UpdateWindow(h);
        }
        assert_eq!(snapshot().2, 1);
        let context = with(|s| s.model.view.context().cloned().unwrap());
        event(Event::Minimize);
        assert_eq!(snapshot().0, "minimized");
        assert_eq!(snapshot().2, 1);
        call(Caller::Tray);
        assert_eq!(snapshot().0, "search");
        assert_eq!(with(|s| s.model.view.context().cloned().unwrap()), context);
        event(Event::Escape);
        assert_eq!(snapshot().0, "list");
        event(Event::ChangeBox(2));
        assert_eq!(snapshot().0, "empty");
        assert_eq!(snapshot().1, Some(2));
        event(Event::ChangeBox(1));
        assert_eq!(snapshot().0, "list");
        event(Event::SetSticky(true));
        event(Event::Outside);
        assert_eq!(snapshot().0, "list");
        call(Caller::Hotkey);
        assert_eq!(snapshot().0, "hidden");
        assert_eq!(with(|s| s.items.len()), 0);
        assert_eq!(snapshot().2, 0);
        let status = std::fs::read_to_string(fixture.root.join("kvf/status.json")).unwrap();
        assert_eq!(
            crate::json::parse(&status).unwrap().str("view"),
            Some("hidden")
        );
        assert_eq!(
            std::fs::read(fixture.root.join("library/library.db")).unwrap(),
            before_db
        );
        unsafe {
            crate::wndproc(null_mut(), WM_DESTROY, 0, 0);
        }
        assert!(!fixture.root.join("kvf/status.json").exists());
    }
}

fn open_item(seq: u64, id: i64, action: crate::open::Action) {
    let item = with(|s| s.items.iter().find(|i| i.id == id).cloned());
    let h = with(|s| s.hwnd);
    let result = item
        .ok_or_else(|| "자료가 옮겨졌거나 지워졌습니다".to_string())
        .and_then(|item| crate::open::open(h, &crate::paths::library_dir(), &item, action));
    event(Event::Opened { seq, result });
}
fn item_menu(h: HWND, l: isize) {
    unsafe {
        let origin = with(|s| {
            s.model
                .view
                .context()
                .and_then(|c| c.selection)
                .and_then(|id| s.items.iter().find(|i| i.id == id))
                .is_some_and(|i| {
                    !i.origin_path.is_empty() && std::path::Path::new(&i.origin_path).exists()
                })
        });
        let menu = CreatePopupMenu();
        AppendMenuW(menu, MF_STRING, 1, wide("열기").as_ptr());
        AppendMenuW(
            menu,
            MF_STRING,
            2,
            wide("탐색기에서 위치 열기\tCtrl+Enter").as_ptr(),
        );
        if origin {
            AppendMenuW(menu, MF_STRING, 3, wide("원본 위치 열기").as_ptr());
        }
        let (x, y) = xy(l);
        let mut p = POINT { x, y };
        ClientToScreen(h, &mut p);
        let command = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            p.x,
            p.y,
            0,
            h,
            null(),
        );
        DestroyMenu(menu);
        match command {
            1 => event(Event::Open),
            2 => event(Event::LocateCopy),
            3 => event(Event::LocateOrigin),
            _ => {}
        }
    }
}
