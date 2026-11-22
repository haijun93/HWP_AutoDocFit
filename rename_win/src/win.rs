//! Win32 창: 파일 드롭·열기 대화상자·규칙 입력(편집 컨트롤)·미리보기. 디스크의 파일 이름은 바꾸지 않는다.
use rename_win::apply::{self, RealFs};
use rename_win::colors::Colors;
use rename_win::{Model, RowStatus, invalid_reason, parse_digits, parse_open_buffer};
use std::{
    ffi::OsString,
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
    ptr::null_mut,
    sync::{
        OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{
        LibraryLoader::GetModuleHandleW,
        Memory::{GMEM_MOVEABLE, GMEM_ZEROINIT, GlobalAlloc, GlobalLock, GlobalUnlock},
    },
    UI::{
        Controls::{Dialogs::*, DRAWITEMSTRUCT},
        HiDpi::*,
        Input::KeyboardAndMouse::{EnableWindow, IsWindowEnabled},
        Shell::{DROPFILES, DragAcceptFiles, DragFinish, DragQueryFileW, HDROP},
        WindowsAndMessaging::*,
    },
};

const TITLE: &str = "파일명 일괄변경";
const ID_FIELD0: i32 = 101; // 접두사, 접미사, 자릿수, 찾을 말, 바꿀 말
const ID_OPEN: i32 = 201;
const ID_CLEAR: i32 = 202;
const ID_APPLY: i32 = 203;
const ID_UNDO: i32 = 204;
const ID_CANCEL: i32 = 205;
const ID_REGEX: i32 = 206;
/// (x, y, w) 96dpi — 적용·되돌리기·취소·정규식 단추.
const BTN_APPLY: (i32, i32, i32) = (700, 152, 170);
const BTN_UNDO: (i32, i32, i32) = (884, 152, 206);
const BTN_CANCEL: (i32, i32, i32) = (960, 80, 120);
const BTN_REGEX: (i32, i32, i32) = (520, 118, 90);
const ODS_SELECTED: u32 = 0x0001;
const ODS_DISABLED: u32 = 0x0004;
const ODS_FOCUS: u32 = 0x0010;
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
static THEME_PREF: OnceLock<ui_theme::ThemePref> = OnceLock::new();
/// 시험용: 확인 대화상자를 건너뛴다.
static NO_CONFIRM: AtomicBool = AtomicBool::new(false);
/// 적용 중(이 동안 다른 입력은 막고 취소만 받는다).
static APPLYING: AtomicBool = AtomicBool::new(false);
static CANCEL_REQ: AtomicBool = AtomicBool::new(false);
/// 시험용: 0이 아니면 취소 확인이 이 횟수를 넘은 뒤 취소 단추를 누른 것으로 친다.
static CANCEL_AFTER: AtomicUsize = AtomicUsize::new(0);
static CANCEL_CHECKS: AtomicUsize = AtomicUsize::new(0);

/// `--theme system|light|dark`.
pub fn set_theme_pref(v: &str) {
    let _ = THEME_PREF.set(ui_theme::ThemePref::parse(v));
}
fn theme_pref() -> ui_theme::ThemePref {
    THEME_PREF.get().copied().unwrap_or(ui_theme::ThemePref::System)
}
fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}
/// 오래된(90일 초과) 작업과 이미 되돌린 작업을 저널에서 지운다. 실패해도 창은 계속 연다.
fn prune_journal_quietly() -> Option<apply::PruneReport> {
    apply::prune_journal(&journal_path(), now_unix(), apply::JOURNAL_KEEP_DAYS).ok()
}

/// 데이터 폴더: `--data-dir` > 환경변수 `RENAME_WIN_DATA` > `%APPDATA%\HwpAutoDocFit\Rename`.
pub fn set_data_dir(dir: Option<String>) -> Result<(), String> {
    let d = match dir.or_else(|| std::env::var("RENAME_WIN_DATA").ok()).filter(|v| !v.is_empty()) {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(std::env::var_os("APPDATA").ok_or("APPDATA 를 찾을 수 없습니다")?)
            .join("HwpAutoDocFit")
            .join("Rename"),
    };
    let _ = DATA_DIR.set(d);
    Ok(())
}
fn journal_path() -> PathBuf {
    DATA_DIR
        .get()
        .cloned()
        .unwrap_or_else(|| std::env::temp_dir().join("HwpAutoDocFitRename"))
        .join(apply::JOURNAL_FILE)
}
const EN_CHANGE: u32 = 0x0300;
const FIELD_LABELS: [&str; 5] = ["접두사", "접미사", "순번 자릿수", "찾을 말", "바꿀 말"];
/// 96dpi 기준 (x, y, w) — 라벨은 x-라벨폭 위치에 그린다.
const FIELD_POS: [(i32, i32, i32); 5] = [
    (100, 80, 150),
    (350, 80, 150),
    (610, 80, 48),
    (100, 118, 150),
    (350, 118, 150),
];
const LABEL_X: [i32; 5] = [24, 280, 500, 24, 280];
const FIELD_H: i32 = 26;
const LIST_TOP: i32 = 208;
const ROW_H: i32 = 26;
const BASE_W: i32 = 1120;
const BASE_H: i32 = 600;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn sc(v: i32, dpi: u32) -> i32 {
    v * dpi as i32 / 96
}

struct Ui {
    model: Model,
    dpi: u32,
    font: HFONT,
    edits: [HWND; 5],
    note: String,
    ready: bool,
    /// 되돌릴 수 있는 직전 작업의 이동 건수.
    undo_n: Option<usize>,
    /// 지금 테마의 색(ui_theme 팔레트에서 만든다).
    colors: Colors,
    kind: ui_theme::Kind,
    /// 입력 칸 바탕 붓(테마가 바뀌면 다시 만든다).
    edit_brush: HBRUSH,
    regex_on: bool,
}

fn new_ui(dpi: u32, font: HFONT) -> Ui {
    let t = ui_theme::theme();
    Ui {
        model: Model::default(),
        dpi,
        font,
        edits: [null_mut(); 5],
        note: String::new(),
        ready: false,
        undo_n: None,
        colors: Colors::of(&t.palette, t.kind),
        kind: t.kind,
        edit_brush: unsafe { CreateSolidBrush(Colors::of(&t.palette, t.kind).list) },
        regex_on: false,
    }
}

unsafe fn ui(hwnd: HWND) -> &'static mut Ui {
    unsafe { &mut *(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ui) }
}

fn make_font(dpi: u32) -> HFONT {
    let face = wide(grid_core::gdi::FACE);
    unsafe {
        CreateFontW(
            -sc(15, dpi),
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
            CLEARTYPE_QUALITY as u32,
            0,
            face.as_ptr(),
        )
    }
}

fn fill(hdc: HDC, r: RECT, c: COLORREF) {
    unsafe {
        let b = CreateSolidBrush(c);
        FillRect(hdc, &r, b);
        DeleteObject(b);
    }
}
fn put(hdc: HDC, dpi: u32, x: i32, y: i32, w: i32, s: &str, c: COLORREF) {
    let v: Vec<u16> = s.encode_utf16().collect();
    if v.is_empty() {
        return;
    }
    let mut r = RECT {
        left: sc(x, dpi),
        top: sc(y, dpi),
        right: sc(x + w, dpi),
        bottom: sc(y + 24, dpi),
    };
    unsafe {
        SetTextColor(hdc, c);
        SetBkMode(hdc, TRANSPARENT as i32);
        DrawTextW(
            hdc,
            v.as_ptr(),
            v.len() as i32,
            &mut r,
            DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS | DT_VCENTER,
        );
    }
}
fn status_label(m: &Model, i: usize, c: &Colors) -> (String, COLORREF) {
    let r = &m.rows[i];
    match r.status {
        RowStatus::Changed => ("변경 예정".into(), c.ok),
        RowStatus::Unchanged => ("변화 없음".into(), c.muted),
        RowStatus::Conflict => ("충돌: 같은 이름이 생김".into(), c.bad),
        RowStatus::Invalid => (
            format!("이름 오류: {}", invalid_reason(&r.proposed).unwrap_or("")),
            c.warn,
        ),
    }
}

fn can_apply(m: &Model) -> bool {
    m.input_error.is_none()
        && !m.files.is_empty()
        && m.count(RowStatus::Changed) > 0
        && m.count(RowStatus::Conflict) == 0
        && m.count(RowStatus::Invalid) == 0
}
fn undo_label(n: Option<usize>) -> String {
    match n {
        Some(n) => format!("직전 작업 되돌리기 ({n})"),
        None => "직전 작업 되돌리기".into(),
    }
}
fn frame(hdc: HDC, r: RECT, c: COLORREF) {
    unsafe {
        let b = CreateSolidBrush(c);
        FrameRect(hdc, &r, b);
        DeleteObject(b);
    }
}
/// 단추 하나를 그린다(실제 owner-draw 단추와 캡처가 같은 함수를 쓴다). `check`가 Some이면 체크 상자.
fn draw_button(hdc: HDC, r: RECT, dpi: u32, label: &str, state: u32, check: Option<bool>, c: &Colors) {
    let enabled = state & ODS_DISABLED == 0;
    let fg = if enabled { c.button_text } else { c.button_off_text };
    let text_rect = |left: i32| -> RECT { RECT { left, top: r.top, right: r.right - sc(6, dpi), bottom: r.bottom } };
    unsafe {
        match check {
            None => {
                let bg = if state & ODS_SELECTED != 0 { c.button_down } else if state & ODS_FOCUS != 0 { c.button_hover } else { c.button };
                fill(hdc, r, bg);
                frame(hdc, r, if state & ODS_FOCUS != 0 { c.accent } else { c.border });
                let mut tr = text_rect(r.left + sc(6, dpi));
                tr.left = r.left + sc(6, dpi);
                let v: Vec<u16> = label.encode_utf16().collect();
                SetTextColor(hdc, fg);
                SetBkMode(hdc, TRANSPARENT as i32);
                DrawTextW(hdc, v.as_ptr(), v.len() as i32, &mut tr, DT_SINGLELINE | DT_NOPREFIX | DT_VCENTER | DT_CENTER);
            }
            Some(on) => {
                fill(hdc, r, c.window);
                let side = sc(16, dpi);
                let top = r.top + (r.bottom - r.top - side) / 2;
                let bx = RECT { left: r.left + sc(2, dpi), top, right: r.left + sc(2, dpi) + side, bottom: top + side };
                fill(hdc, bx, if on { c.accent } else { c.list });
                frame(hdc, bx, if state & ODS_FOCUS != 0 || on { c.accent } else { c.border });
                if on {
                    // 흰 체크 표시(고대비에서도 상자 색과 구분되도록 바탕 창 색을 쓴다)
                    let pen = CreatePen(PS_SOLID, sc(2, dpi).max(2), c.window);
                    let old = SelectObject(hdc, pen);
                    let (x0, y0) = (bx.left, bx.top);
                    MoveToEx(hdc, x0 + side * 3 / 16, y0 + side / 2, null_mut());
                    LineTo(hdc, x0 + side * 7 / 16, y0 + side * 11 / 16);
                    LineTo(hdc, x0 + side * 13 / 16, y0 + side * 4 / 16);
                    SelectObject(hdc, old);
                    DeleteObject(pen);
                }
                let mut tr = text_rect(bx.right + sc(8, dpi));
                let v: Vec<u16> = label.encode_utf16().collect();
                SetTextColor(hdc, fg);
                SetBkMode(hdc, TRANSPARENT as i32);
                DrawTextW(hdc, v.as_ptr(), v.len() as i32, &mut tr, DT_SINGLELINE | DT_NOPREFIX | DT_VCENTER | DT_LEFT);
            }
        }
    }
}
fn pos_rect(pos: (i32, i32, i32), dpi: u32) -> RECT {
    RECT { left: sc(pos.0, dpi), top: sc(pos.1, dpi), right: sc(pos.0 + pos.2, dpi), bottom: sc(pos.1 + FIELD_H, dpi) }
}
fn fake_button(hdc: HDC, dpi: u32, pos: (i32, i32, i32), label: &str, enabled: bool, c: &Colors) {
    draw_button(hdc, pos_rect(pos, dpi), dpi, label, if enabled { 0 } else { ODS_DISABLED }, None, c);
}
fn refresh_buttons(hwnd: HWND) {
    let u = unsafe { ui(hwnd) };
    u.undo_n = apply::undoable_count(&journal_path());
    let busy = APPLYING.load(Ordering::Relaxed);
    unsafe {
        EnableWindow(GetDlgItem(hwnd, ID_APPLY), (!busy && can_apply(&u.model)) as i32);
        EnableWindow(GetDlgItem(hwnd, ID_UNDO), (!busy && u.undo_n.is_some()) as i32);
        EnableWindow(GetDlgItem(hwnd, ID_CANCEL), busy as i32);
        for id in [ID_OPEN, ID_CLEAR, ID_REGEX] {
            EnableWindow(GetDlgItem(hwnd, id), !busy as i32);
        }
        for &e in &u.edits {
            EnableWindow(e, !busy as i32);
        }
        SetWindowTextW(GetDlgItem(hwnd, ID_UNDO), wide(&undo_label(u.undo_n)).as_ptr());
        for id in [ID_APPLY, ID_UNDO, ID_CANCEL, ID_REGEX, ID_OPEN, ID_CLEAR] {
            InvalidateRect(GetDlgItem(hwnd, id), null_mut(), 0);
        }
    }
}

/// 지금 `ui_theme` 상태를 창에 반영한다: 색 표·입력 칸 붓·제목줄 어둡게·다시 그리기.
fn apply_theme(hwnd: HWND) {
    let u = unsafe { ui(hwnd) };
    let t = ui_theme::theme();
    u.kind = t.kind;
    u.colors = Colors::of(&t.palette, t.kind);
    unsafe {
        DeleteObject(u.edit_brush);
        u.edit_brush = CreateSolidBrush(u.colors.list);
    }
    ui_theme::apply_window(hwnd, t.kind);
    unsafe { InvalidateRect(hwnd, null_mut(), 1) };
    for &e in &u.edits {
        unsafe { InvalidateRect(e, null_mut(), 1) };
    }
}

/// 취소 확인용: 대기 중인 창 메시지(취소 단추 클릭·다시 그리기)를 처리한 뒤 취소 요청 여부를 돌려준다.
fn pump_and_check_cancel() -> bool {
    unsafe {
        let mut m: MSG = std::mem::zeroed();
        while PeekMessageW(&mut m, null_mut(), 0, 0, PM_REMOVE) != 0 {
            if m.message == WM_QUIT {
                PostQuitMessage(m.wParam as i32);
                break;
            }
            TranslateMessage(&m);
            DispatchMessageW(&m);
        }
    }
    let n = CANCEL_CHECKS.fetch_add(1, Ordering::Relaxed) + 1;
    let after = CANCEL_AFTER.load(Ordering::Relaxed);
    if after > 0 && n > after {
        CANCEL_REQ.store(true, Ordering::Relaxed);
    }
    CANCEL_REQ.load(Ordering::Relaxed)
}

fn confirm(hwnd: HWND, text: &str) -> bool {
    if NO_CONFIRM.load(Ordering::Relaxed) {
        return true;
    }
    unsafe { MessageBoxW(hwnd, wide(text).as_ptr(), wide(TITLE).as_ptr(), MB_YESNO | MB_ICONQUESTION) == IDYES }
}
fn alert(hwnd: HWND, text: &str) {
    if !NO_CONFIRM.load(Ordering::Relaxed) {
        unsafe { MessageBoxW(hwnd, wide(text).as_ptr(), wide(TITLE).as_ptr(), MB_OK | MB_ICONWARNING) };
    }
}

fn do_apply(hwnd: HWND) {
    let plan = match apply::build_plan(&unsafe { ui(hwnd) }.model) {
        Ok(p) => p,
        Err(e) => {
            unsafe { ui(hwnd) }.note = e;
            unsafe { InvalidateRect(hwnd, null_mut(), 0) };
            return;
        }
    };
    if !confirm(hwnd, &format!("{}개 파일의 이름을 바꿉니다. 적용 중에는 ‘적용 취소’로 멈출 수 있고, 적용 후에는 ‘직전 작업 되돌리기’로 되돌릴 수 있습니다. 계속할까요?", plan.len())) {
        return;
    }
    CANCEL_REQ.store(false, Ordering::Relaxed);
    CANCEL_CHECKS.store(0, Ordering::Relaxed);
    APPLYING.store(true, Ordering::Relaxed);
    unsafe { ui(hwnd) }.note = format!("{}개 적용 중… 멈추려면 ‘적용 취소’", plan.len());
    refresh_buttons(hwnd);
    unsafe { InvalidateRect(hwnd, null_mut(), 0) };
    // 적용 중에는 Ui 를 붙잡고 있지 않는다(취소 확인에서 창 메시지를 처리하므로).
    let result = apply::apply_cancellable(&plan, &journal_path(), &RealFs, &pump_and_check_cancel);
    APPLYING.store(false, Ordering::Relaxed);
    let u = unsafe { ui(hwnd) };
    match result {
        Ok(r) if r.cancelled => {
            let mut t = format!("적용 취소: {}개 변경됨 · {}개는 그대로 둠", r.renamed, r.remaining);
            if r.renamed > 0 {
                t += " · ‘직전 작업 되돌리기’로 이미 바뀐 것을 복구할 수 있습니다";
            }
            if !r.stranded.is_empty() {
                t += &format!(" · 임시 이름으로 남은 파일 {}개: {}", r.stranded.len(), r.stranded.join(", "));
                alert(hwnd, &t);
            }
            u.note = t;
            u.model.clear();
        }
        Ok(r) => {
            u.model.clear();
            u.note = format!("{}개 이름 변경 완료 · ‘직전 작업 되돌리기’로 되돌릴 수 있습니다", r.renamed);
        }
        Err((msg, r)) => {
            let mut t = format!("중단: {msg}");
            if r.renamed > 0 {
                t += &format!(" · 이미 바뀐 {}개는 되돌리기로 복구할 수 있습니다", r.renamed);
            }
            if !r.stranded.is_empty() {
                t += &format!(" · 임시 이름으로 남은 파일 {}개: {}", r.stranded.len(), r.stranded.join(", "));
            }
            alert(hwnd, &t);
            u.note = t;
            u.model.clear();
        }
    }
    refresh_buttons(hwnd);
    unsafe { InvalidateRect(hwnd, null_mut(), 0) };
}

fn do_undo(hwnd: HWND) {
    let u = unsafe { ui(hwnd) };
    let n = u.undo_n.unwrap_or(0);
    if !confirm(hwnd, &format!("직전 작업({n}건)을 되돌립니다. 계속할까요?")) {
        return;
    }
    u.note = match apply::undo_last(&journal_path(), &RealFs) {
        Ok(r) if r.skipped.is_empty() => format!("{}개 되돌림 완료", r.restored),
        Ok(r) => {
            let t = format!("{}개 되돌림 · {}개 건너뜀: {}", r.restored, r.skipped.len(), r.skipped.join(" / "));
            alert(hwnd, &t);
            t
        }
        Err(e) => e,
    };
    u.model.clear();
    refresh_buttons(hwnd);
    unsafe { InvalidateRect(hwnd, null_mut(), 0) };
}

/// 화면 전체를 그린다. `fake_fields`가 있으면(캡처) 편집 칸·단추를 직접 그린다.
fn draw(hdc: HDC, w: i32, h: i32, u: &Ui, fake_fields: Option<[String; 5]>) {
    let dpi = u.dpi;
    let c = &u.colors;
    let (bw, bh) = (w * 96 / dpi as i32, h * 96 / dpi as i32);
    fill(hdc, RECT { left: 0, top: 0, right: w, bottom: h }, c.window);
    unsafe { SelectObject(hdc, u.font) };
    let (ink, muted) = (c.ink, c.muted);
    put(hdc, dpi, 24, 14, 700, TITLE, c.title);
    put(
        hdc,
        dpi,
        24,
        42,
        bw - 48,
        "규칙을 입력하고 미리보기를 확인한 뒤 ‘이름 변경 적용’을 누르세요. 적용한 직전 작업은 되돌릴 수 있습니다.",
        muted,
    );
    for (i, label) in FIELD_LABELS.iter().enumerate() {
        put(hdc, dpi, LABEL_X[i], FIELD_POS[i].1 + 2, 90, label, ink);
        let (x, y, fw) = FIELD_POS[i];
        let r = RECT { left: sc(x, dpi), top: sc(y, dpi), right: sc(x + fw, dpi), bottom: sc(y + FIELD_H, dpi) };
        // 입력 칸 테두리: 정규식이 잘못되면 찾을 말 칸을 빨갛게
        let bad = i == 3 && u.model.input_error.is_some();
        frame(hdc, r, if bad { c.bad } else { c.border });
        if let Some(f) = &fake_fields {
            fill(hdc, RECT { left: r.left + 1, top: r.top + 1, right: r.right - 1, bottom: r.bottom - 1 }, c.list);
            put(hdc, dpi, x + 5, y + 1, fw - 10, &f[i], ink);
        }
    }
    if fake_fields.is_some() {
        fake_button(hdc, dpi, (700, 80, 120), "파일 열기…", true, c);
        fake_button(hdc, dpi, (830, 80, 120), "목록 비우기", true, c);
        fake_button(hdc, dpi, BTN_CANCEL, "적용 취소", false, c);
        draw_button(hdc, pos_rect(BTN_REGEX, dpi), dpi, "정규식", 0, Some(u.regex_on), c);
    }
    put(
        hdc,
        dpi,
        620,
        120,
        bw - 640,
        if u.regex_on {
            "정규식(Rust regex) · 바꿀 말에서 $1 로 묶음 참조"
        } else {
            "자릿수 0 = 순번 없음 · 확장자는 바꾸지 않음"
        },
        muted,
    );
    let summary_color = if u.model.input_error.is_some() { c.bad } else { ink };
    put(hdc, dpi, 24, 156, BTN_APPLY.0 - 40, &u.model.summary(), summary_color);
    if fake_fields.is_some() {
        fake_button(hdc, dpi, BTN_APPLY, "이름 변경 적용", can_apply(&u.model), c);
        fake_button(hdc, dpi, BTN_UNDO, &undo_label(u.undo_n), u.undo_n.is_some(), c);
    }
    // 목록
    let col2 = 24 + (bw - 48 - 220) / 2;
    let stat = bw - 24 - 210;
    let hr = RECT { left: sc(16, dpi), top: sc(LIST_TOP - 24, dpi), right: sc(bw - 16, dpi), bottom: sc(LIST_TOP, dpi) };
    fill(hdc, hr, c.header);
    put(hdc, dpi, 24, LIST_TOP - 24, 300, "원래 이름", ink);
    put(hdc, dpi, col2, LIST_TOP - 24, 300, "미리보기 이름", ink);
    put(hdc, dpi, stat, LIST_TOP - 24, 200, "상태", ink);
    let list_bottom = bh - 44;
    let body = RECT { left: sc(16, dpi), top: sc(LIST_TOP, dpi), right: sc(bw - 16, dpi), bottom: sc(list_bottom, dpi) };
    fill(hdc, body, c.list);
    frame(hdc, RECT { left: body.left, top: hr.top, right: body.right, bottom: body.bottom }, c.border);
    let m = &u.model;
    if m.files.is_empty() {
        put(hdc, dpi, 40, LIST_TOP + 40, 600, "여기에 파일을 놓으세요", muted);
    }
    let visible = ((list_bottom - LIST_TOP) / ROW_H).max(0) as usize;
    for (k, i) in (m.scroll..m.rows.len()).take(visible).enumerate() {
        let y = LIST_TOP + k as i32 * ROW_H;
        let (label, lc) = status_label(m, i, c);
        let row_ink = if m.rows[i].status == RowStatus::Unchanged { muted } else { ink };
        put(hdc, dpi, 24, y + 1, col2 - 36, &m.rows[i].original, row_ink);
        put(hdc, dpi, col2, y + 1, stat - col2 - 12, &m.rows[i].proposed, row_ink);
        put(hdc, dpi, stat, y + 1, 200, &label, lc);
    }
    if !u.note.is_empty() {
        put(hdc, dpi, 24, bh - 34, bw - 48, &u.note, muted);
    }
}

fn read_text(h: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(h);
        let mut buf = vec![0u16; n as usize + 1];
        let got = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..got.max(0) as usize])
    }
}

fn read_rules(u: &mut Ui) {
    let t: Vec<String> = u.edits.iter().map(|&h| read_text(h)).collect();
    u.model.rules.prefix = t[0].clone();
    u.model.rules.suffix = t[1].clone();
    u.model.rules.digits = parse_digits(&t[2]);
    u.model.rules.find = t[3].clone();
    u.model.rules.replace = t[4].clone();
    u.model.rules.use_regex = u.regex_on;
    u.model.recompute();
}

fn fs_is_file(p: &str) -> bool {
    Path::new(p).is_file()
}
fn fs_list_dir(dir: &str) -> Vec<String> {
    let d = if dir.is_empty() { "." } else { dir };
    std::fs::read_dir(d)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn add_paths(hwnd: HWND, paths: Vec<String>) {
    let u = unsafe { ui(hwnd) };
    let total = paths.len();
    let n = u.model.add_paths_with(&paths, fs_is_file, fs_list_dir, |p| apply::stat_file(Path::new(p)));
    u.note = if total > n {
        format!("{n}개 추가 · {}개 건너뜀(폴더이거나 이미 목록에 있음)", total - n)
    } else {
        format!("{n}개 추가")
    };
    refresh_buttons(hwnd);
    unsafe { InvalidateRect(hwnd, null_mut(), 0) };
}

fn open_dialog(hwnd: HWND) {
    let mut buf = vec![0u16; 65536];
    let filter: Vec<u16> = "모든 파일\0*.*\0\0".encode_utf16().collect();
    let title = wide("이름을 바꿀 파일 고르기 (이름은 바뀌지 않습니다)");
    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = hwnd;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = buf.as_mut_ptr();
    ofn.nMaxFile = buf.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.Flags = OFN_ALLOWMULTISELECT
        | OFN_EXPLORER
        | OFN_FILEMUSTEXIST
        | OFN_PATHMUSTEXIST
        | OFN_HIDEREADONLY;
    if unsafe { GetOpenFileNameW(&mut ofn) } != 0 {
        add_paths(hwnd, parse_open_buffer(&buf));
    }
}

fn layout_children(hwnd: HWND) {
    let u = unsafe { ui(hwnd) };
    unsafe {
        for (i, &h) in u.edits.iter().enumerate() {
            let (x, y, w) = FIELD_POS[i];
            // 1px 테두리는 draw()가 그리므로 편집 칸은 그 안쪽에 둔다
            MoveWindow(h, sc(x, u.dpi) + 2, sc(y, u.dpi) + 3, sc(w, u.dpi) - 4, sc(FIELD_H, u.dpi) - 5, 1);
            SendMessageW(h, WM_SETFONT, u.font as usize, 1);
        }
        for (id, (x, y, w)) in [(ID_APPLY, BTN_APPLY), (ID_UNDO, BTN_UNDO), (ID_CANCEL, BTN_CANCEL), (ID_REGEX, BTN_REGEX)] {
            let b = GetDlgItem(hwnd, id);
            MoveWindow(b, sc(x, u.dpi), sc(y, u.dpi), sc(w, u.dpi), sc(FIELD_H, u.dpi), 1);
            SendMessageW(b, WM_SETFONT, u.font as usize, 1);
        }
        for (id, x) in [(ID_OPEN, 700), (ID_CLEAR, 830)] {
            let b = GetDlgItem(hwnd, id);
            MoveWindow(b, sc(x, u.dpi), sc(80, u.dpi), sc(120, u.dpi), sc(FIELD_H, u.dpi), 1);
            SendMessageW(b, WM_SETFONT, u.font as usize, 1);
        }
    }
}

fn drop_paths(drop: HDROP) -> Vec<String> {
    unsafe {
        let n = DragQueryFileW(drop, u32::MAX, null_mut(), 0);
        let mut v = Vec::new();
        for i in 0..n {
            let len = DragQueryFileW(drop, i, null_mut(), 0);
            let mut buf = vec![0u16; len as usize + 1];
            DragQueryFileW(drop, i, buf.as_mut_ptr(), buf.len() as u32);
            v.push(OsString::from_wide(&buf[..len as usize]).to_string_lossy().into_owned());
        }
        DragFinish(drop);
        v
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_NCCREATE => {
                let cs = &*(lp as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_CREATE => {
                let u = ui(hwnd);
                u.dpi = GetDpiForWindow(hwnd).max(96);
                u.font = make_font(u.dpi);
                ui_theme::refresh(theme_pref());
                apply_theme(hwnd);
                let inst = GetModuleHandleW(null_mut());
                let edit = wide("EDIT");
                for i in 0..5 {
                    let init = if i == 2 { wide("3") } else { wide("") };
                    let mut style = WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32;
                    if i == 2 {
                        style |= ES_NUMBER as u32;
                    }
                    u.edits[i] = CreateWindowExW(
                        0,
                        edit.as_ptr(),
                        init.as_ptr(),
                        style,
                        0,
                        0,
                        10,
                        10,
                        hwnd,
                        (ID_FIELD0 + i as i32) as usize as HMENU,
                        inst,
                        null_mut(),
                    );
                }
                let button = wide("BUTTON");
                for (id, label) in [
                    (ID_OPEN, "파일 열기…"),
                    (ID_CLEAR, "목록 비우기"),
                    (ID_APPLY, "이름 변경 적용"),
                    (ID_UNDO, "직전 작업 되돌리기"),
                    (ID_CANCEL, "적용 취소"),
                    (ID_REGEX, "정규식"),
                ] {
                    let t = wide(label);
                    CreateWindowExW(
                        0,
                        button.as_ptr(),
                        t.as_ptr(),
                        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
                        0,
                        0,
                        10,
                        10,
                        hwnd,
                        id as usize as HMENU,
                        inst,
                        null_mut(),
                    );
                }
                layout_children(hwnd);
                DragAcceptFiles(hwnd, 1);
                u.ready = true;
                u.model.recompute();
                refresh_buttons(hwnd);
                0
            }
            WM_DROPFILES => {
                add_paths(hwnd, drop_paths(wp as HDROP));
                0
            }
            WM_COMMAND => {
                let id = (wp & 0xffff) as i32;
                let code = ((wp >> 16) & 0xffff) as u32;
                if id == ID_CANCEL && code == BN_CLICKED {
                    // 적용 중에는 Ui 를 건드리지 않고 깃발만 세운다
                    if APPLYING.load(Ordering::Relaxed) {
                        CANCEL_REQ.store(true, Ordering::Relaxed);
                    }
                    return 0;
                }
                if APPLYING.load(Ordering::Relaxed) {
                    return 0;
                }
                let u = ui(hwnd);
                if u.ready && code == EN_CHANGE && (ID_FIELD0..ID_FIELD0 + 5).contains(&id) {
                    read_rules(u);
                    refresh_buttons(hwnd);
                    InvalidateRect(hwnd, null_mut(), 0);
                } else if id == ID_REGEX && code == BN_CLICKED {
                    u.regex_on = !u.regex_on;
                    read_rules(u);
                    refresh_buttons(hwnd);
                    InvalidateRect(hwnd, null_mut(), 0);
                } else if id == ID_OPEN && code == BN_CLICKED {
                    open_dialog(hwnd);
                } else if id == ID_CLEAR && code == BN_CLICKED {
                    u.model.clear();
                    u.note = "목록을 비웠습니다".into();
                    refresh_buttons(hwnd);
                    InvalidateRect(hwnd, null_mut(), 0);
                } else if id == ID_APPLY && code == BN_CLICKED && can_apply(&u.model) {
                    do_apply(hwnd);
                } else if id == ID_UNDO && code == BN_CLICKED && u.undo_n.is_some() {
                    do_undo(hwnd);
                }
                0
            }
            WM_MOUSEWHEEL => {
                let u = ui(hwnd);
                let delta = ((wp >> 16) & 0xffff) as i16 as i32;
                let step = (delta / 120 * 3) as isize;
                let max = u.model.rows.len().saturating_sub(1) as isize;
                u.model.scroll = (u.model.scroll as isize - step).clamp(0, max) as usize;
                InvalidateRect(hwnd, null_mut(), 0);
                0
            }
            WM_DPICHANGED => {
                let u = ui(hwnd);
                u.dpi = (wp & 0xffff) as u32;
                DeleteObject(u.font);
                u.font = make_font(u.dpi);
                let r = &*(lp as *const RECT);
                SetWindowPos(hwnd, null_mut(), r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
                layout_children(hwnd);
                InvalidateRect(hwnd, null_mut(), 1);
                0
            }
            WM_DRAWITEM => {
                let d = &*(lp as *const DRAWITEMSTRUCT);
                let u = ui(hwnd);
                SelectObject(d.hDC, u.font);
                let check = (d.CtlID as i32 == ID_REGEX).then_some(u.regex_on);
                draw_button(d.hDC, d.rcItem, u.dpi, &read_text(d.hwndItem), d.itemState, check, &u.colors);
                1
            }
            WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC => {
                let u = ui(hwnd);
                let hdc = wp as HDC;
                let off = msg == WM_CTLCOLORSTATIC;
                SetTextColor(hdc, if off { u.colors.muted } else { u.colors.ink });
                SetBkColor(hdc, u.colors.list);
                u.edit_brush as LRESULT
            }
            WM_CLOSE if APPLYING.load(Ordering::Relaxed) => 0,
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let hdc = BeginPaint(hwnd, &mut ps);
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut r);
                let mem = CreateCompatibleDC(hdc);
                let bm = CreateCompatibleBitmap(hdc, r.right.max(1), r.bottom.max(1));
                let old = SelectObject(mem, bm);
                draw(mem, r.right, r.bottom, ui(hwnd), None);
                BitBlt(hdc, 0, 0, r.right, r.bottom, mem, 0, 0, SRCCOPY);
                SelectObject(mem, old);
                DeleteObject(bm);
                DeleteDC(mem);
                EndPaint(hwnd, &ps);
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ if ui_theme::is_theme_message(msg, wp, lp) => {
                ui_theme::refresh_system();
                apply_theme(hwnd);
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

fn create_window(visible: bool) -> Result<(HWND, *mut Ui), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let inst = GetModuleHandleW(null_mut());
        let cls = wide("DocFitRenameWin2");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(proc),
            hInstance: inst,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: null_mut(),
            lpszClassName: cls.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let st = Box::into_raw(Box::new(new_ui(96, null_mut())));
        let title = wide(TITLE);
        let mut style = WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN;
        if visible {
            style |= WS_VISIBLE;
        }
        let hwnd = CreateWindowExW(
            0,
            cls.as_ptr(),
            title.as_ptr(),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            BASE_W,
            BASE_H,
            null_mut(),
            null_mut(),
            inst,
            st.cast(),
        );
        if hwnd.is_null() {
            drop(Box::from_raw(st));
            return Err("창을 만들지 못했습니다".into());
        }
        Ok((hwnd, st))
    }
}

pub fn run() -> Result<(), String> {
    prune_journal_quietly();
    let (hwnd, st) = create_window(true)?;
    unsafe {
        let mut m: MSG = std::mem::zeroed();
        while GetMessageW(&mut m, null_mut(), 0, 0) > 0 {
            if IsDialogMessageW(hwnd, &mut m) == 0 {
                TranslateMessage(&m);
                DispatchMessageW(&m);
            }
        }
        drop(Box::from_raw(st));
    }
    Ok(())
}

/// 예시 목록과 규칙으로 화면을 PNG로 저장한다(헤드리스). 편집 칸·단추는 직접 그린 모양이다.
pub fn capture(path: &Path, dpi: u32, bad_regex: bool) -> Result<std::path::PathBuf, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    ui_theme::refresh(theme_pref());
    let mut u = new_ui(dpi, make_font(dpi));
    u.note = "6개 추가".into();
    u.ready = true;
    u.undo_n = Some(3);
    u.regex_on = true;
    u.model.rules.prefix = "정리_".into();
    u.model.rules.digits = 3;
    u.model.rules.use_regex = true;
    // 정상: 정규식 한 줄로 두 낱말을 바꾼다 / --bad-regex: 닫히지 않은 괄호(입력 오류 표시 확인)
    let (find, repl) = if bad_regex { ("(압축", "압:축") } else { ("(압축|사진)", "[$1]") };
    u.model.rules.find = find.into();
    u.model.rules.replace = repl.into();
    let names = [
        "6C1D56A5-12B9-4F7D-9846-BA1255C43404.JPG",
        "현장 사진 2.jpg",
        "보고서(자간조정).hwpx",
        "압축 자료.zip",
        "이름 중복 확인.txt",
        "정리_이름 중복 확인_005.txt.bak",
    ];
    let paths: Vec<String> = names.iter().map(|n| format!("D:\\샘플\\{n}")).collect();
    let existing = |_: &str| {
        vec!["정리_현장 사진 2_002.jpg".to_string(), "정리_보고서(자간조정)_003.hwpx".to_string()]
    };
    u.model.add_paths(&paths, |_| true, existing);
    u.model.recompute();
    let (w, h) = (sc(BASE_W, dpi), sc(BASE_H, dpi));
    let fields = ["정리_".into(), String::new(), "3".into(), find.into(), repl.into()];
    let shot = grid_core::gdi::render(w, h, |hdc| {
        draw(hdc, w, h, &u, Some(fields));
        u.model.files.len()
    })?;
    grid_core::png::write(path, shot.width, shot.height, &shot.bgra)?;
    Ok(path.to_path_buf())
}

fn dir_snapshot(dir: &Path) -> Vec<String> {
    let mut v = fs_list_dir(&dir.to_string_lossy());
    v.sort();
    v
}
fn send_drop(hwnd: HWND, paths: &[String]) {
    let mut list: Vec<u16> = Vec::new();
    for p in paths {
        list.extend(p.encode_utf16());
        list.push(0);
    }
    list.push(0);
    let head = std::mem::size_of::<DROPFILES>();
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, head + list.len() * 2);
        let p = GlobalLock(h) as *mut u8;
        let df = p as *mut DROPFILES;
        (*df).pFiles = head as u32;
        (*df).fWide = 1;
        std::ptr::copy_nonoverlapping(list.as_ptr() as *const u8, p.add(head), list.len() * 2);
        GlobalUnlock(h);
        SendMessageW(hwnd, WM_DROPFILES, h as usize, 0);
    }
}
fn set_field(hwnd: HWND, i: usize, text: &str) {
    let t = wide(text);
    unsafe { SetWindowTextW(ui(hwnd).edits[i], t.as_ptr()) };
}

/// 실제 창을 숨김 상태로 만들어 드롭·편집 컨트롤 입력이 미리보기에 반영되는지 확인한다.
/// 임시 폴더의 파일을 쓰며 이름 변경은 하지 않는다. 결과는 한 줄씩 `out`에 적는다.
pub fn selftest(out: &Path) -> Result<bool, String> {
    let dir = std::env::temp_dir().join(format!("renamewin4-selftest-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("하위")).map_err(|e| e.to_string())?;
    for n in ["a.txt", "b.txt", "정리_a_001.txt", "보고서.hwpx"] {
        std::fs::write(dir.join(n), b"x").map_err(|e| e.to_string())?;
    }
    let before = dir_snapshot(&dir);
    NO_CONFIRM.store(true, Ordering::Relaxed);
    let (hwnd, st) = create_window(false)?;
    let mut log = Vec::new();
    let mut ok = true;
    let mut check = |name: &str, cond: bool| {
        ok &= cond;
        log.push(format!("{} {name}", if cond { "PASS" } else { "FAIL" }));
    };
    let p = |n: &str| dir.join(n).to_string_lossy().into_owned();
    send_drop(hwnd, &[p("a.txt"), p("b.txt"), p("하위"), p("a.txt"), p("보고서.hwpx")]);
    let u = unsafe { ui(hwnd) };
    check("드롭: 파일 3개만 추가(폴더·중복 제외)", u.model.files.len() == 3);
    check("드롭 직후 기본 규칙(순번 3자리)", u.model.rows[0].proposed == "a_001.txt");
    set_field(hwnd, 0, "정리_");
    let u = unsafe { ui(hwnd) };
    check("접두사 편집 → 규칙 반영", u.model.rules.prefix == "정리_");
    check("접두사 편집 → 미리보기 이름 갱신", u.model.rows[1].proposed == "정리_b_002.txt");
    check("기존 파일과 겹치면 충돌", u.model.rows[0].proposed == "정리_a_001.txt" && u.model.rows[0].status == RowStatus::Conflict);
    check("겹치지 않으면 변경 예정", u.model.rows[1].status == RowStatus::Changed);
    set_field(hwnd, 2, "0");
    set_field(hwnd, 3, "보고");
    set_field(hwnd, 4, "결과");
    let u = unsafe { ui(hwnd) };
    check("자릿수 0 → 순번 없음", u.model.rows[0].proposed == "정리_a.txt");
    check("찾아 바꾸기(한글) 적용", u.model.rows[2].proposed == "정리_결과서.hwpx");
    check("충돌 해소 후 변경 예정", u.model.rows[0].status == RowStatus::Changed);
    set_field(hwnd, 0, "a?");
    let u = unsafe { ui(hwnd) };
    check("금지 문자 → 이름 오류", u.model.rows.iter().all(|r| r.status == RowStatus::Invalid));
    unsafe { SendMessageW(hwnd, WM_COMMAND, ID_CLEAR as usize, 0) };
    let u = unsafe { ui(hwnd) };
    check("비우기 단추 → 목록 비움", u.model.files.is_empty());
    check("디스크의 파일 이름은 그대로", dir_snapshot(&dir) == before);
    // ---- 3단계: 적용·되돌리기(임시 폴더, 확인 대화상자 생략) ----
    let btn = |id: i32| unsafe { IsWindowEnabled(GetDlgItem(hwnd, id)) != 0 };
    check("빈 목록: 적용·되돌리기 단추 비활성", !btn(ID_APPLY) && !btn(ID_UNDO));
    send_drop(hwnd, &[p("a.txt"), p("b.txt")]);
    set_field(hwnd, 0, "");
    set_field(hwnd, 2, "0");
    set_field(hwnd, 3, "");
    set_field(hwnd, 4, "");
    set_field(hwnd, 0, "정리_");
    check("정리_a_001.txt 와 충돌 없음(번호 없음) → 적용 단추 활성", btn(ID_APPLY));
    set_field(hwnd, 2, "3");
    check("충돌 있으면 적용 단추 비활성", !btn(ID_APPLY));
    set_field(hwnd, 2, "0");
    set_field(hwnd, 0, "a?");
    check("이름 오류 있으면 적용 단추 비활성", !btn(ID_APPLY));
    set_field(hwnd, 0, "정리_");
    unsafe { SendMessageW(hwnd, WM_COMMAND, ID_APPLY as usize, 0) };
    let after: Vec<String> = dir_snapshot(&dir);
    check("적용: 이름이 실제로 바뀜", after.contains(&"정리_a.txt".to_string()) && after.contains(&"정리_b.txt".to_string()) && !after.contains(&"b.txt".to_string()));
    check("적용 후 되돌리기 단추 활성", btn(ID_UNDO));
    check("저널 파일이 데이터 폴더에 생김", journal_path().is_file());
    unsafe { SendMessageW(hwnd, WM_COMMAND, ID_UNDO as usize, 0) };
    check("되돌리기: 원래 이름으로 복원", dir_snapshot(&dir) == before);
    check("되돌린 뒤 되돌리기 단추 비활성", !btn(ID_UNDO));
    // ---- 4단계(RENAMEWIN6): 정규식·적용 취소·저널 정리·테마 ----
    let click = |id: i32| unsafe { SendMessageW(hwnd, WM_COMMAND, id as usize, 0) };
    send_drop(hwnd, &[p("a.txt"), p("b.txt")]);
    set_field(hwnd, 0, "");
    set_field(hwnd, 2, "0");
    click(ID_REGEX);
    set_field(hwnd, 3, "(a|b)");
    set_field(hwnd, 4, "<$1>");
    let u = unsafe { ui(hwnd) };
    check("정규식 켬: 묶음 참조로 바꿈", u.regex_on && u.model.rows[0].proposed == "<a>.txt" && u.model.rows[1].proposed == "<b>.txt");
    check("정규식 켬: '<'는 이름 규칙 위반 → 이름 오류(입력 오류 아님)", u.model.input_error.is_none() && u.model.rows.iter().all(|r| r.status == RowStatus::Invalid));
    set_field(hwnd, 4, "[$1]");
    check("정규식 정상 → 변경 예정·적용 단추 활성", unsafe { ui(hwnd) }.model.rows[0].proposed == "[a].txt" && btn(ID_APPLY));
    set_field(hwnd, 3, "(a");
    let u = unsafe { ui(hwnd) };
    check("잘못된 정규식 → 입력 오류로 표시(이름 오류 0건)", u.model.input_error.is_some() && u.model.count(RowStatus::Invalid) == 0);
    check("잘못된 정규식 → 적용 단추 비활성", !btn(ID_APPLY));
    check("요약 줄에 정규식 오류 문구", u.model.summary().starts_with("정규식 오류"));
    click(ID_REGEX);
    check("정규식 끔: 같은 칸이 글자 그대로 → 오류 사라짐", unsafe { ui(hwnd) }.model.input_error.is_none());
    click(ID_CLEAR);
    set_field(hwnd, 3, "");
    set_field(hwnd, 4, "");
    // 적용 중 취소: 세 파일 중 한 건만 끝내고 취소
    for n in ["c1.txt", "c2.txt", "c3.txt"] {
        std::fs::write(dir.join(n), b"x").map_err(|e| e.to_string())?;
    }
    let before_cancel = dir_snapshot(&dir);
    send_drop(hwnd, &[p("c1.txt"), p("c2.txt"), p("c3.txt")]);
    set_field(hwnd, 0, "z_");
    check("취소 시험 준비: 적용 단추 활성·취소 단추 비활성", btn(ID_APPLY) && !btn(ID_CANCEL));
    CANCEL_AFTER.store(1, Ordering::Relaxed);
    click(ID_APPLY);
    CANCEL_AFTER.store(0, Ordering::Relaxed);
    let after = dir_snapshot(&dir);
    let done = ["z_c1.txt", "z_c2.txt", "z_c3.txt"].iter().filter(|n| after.contains(&n.to_string())).count();
    check("적용 중 취소: 한 건만 바뀌고 멈춤", done == 1 && after.contains(&"z_c1.txt".to_string()) && after.contains(&"c2.txt".to_string()));
    check("취소 뒤 안내 문구", unsafe { ui(hwnd) }.note.starts_with("적용 취소: 1개 변경됨 · 2개는 그대로 둠"));
    check("취소 뒤 단추 상태(적용 끝·취소 비활성·되돌리기 활성)", !APPLYING.load(Ordering::Relaxed) && !btn(ID_CANCEL) && btn(ID_UNDO));
    check("저널에 end(cancelled) 기록", std::fs::read_to_string(journal_path()).map(|t| t.contains("\"status\":\"cancelled\"")).unwrap_or(false));
    click(ID_UNDO);
    check("취소 뒤 되돌리기 → 원래 이름", dir_snapshot(&dir) == before_cancel);
    // 저널 정리: 되돌린 작업은 지워져 저널에 작업이 남지 않는다
    let pr = apply::prune_journal(&journal_path(), now_unix(), apply::JOURNAL_KEEP_DAYS);
    check("저널 정리: 되돌린 작업 제거", pr.as_ref().map(|r| r.kept_runs == 0 && r.removed_runs >= 2).unwrap_or(false));
    check("저널 정리 뒤 되돌릴 작업 없음", apply::undoable_count(&journal_path()).is_none());
    // 테마: 어둡게 → 색 표가 ui_theme 어두운 팔레트를 따른다
    ui_theme::refresh(ui_theme::ThemePref::Dark);
    apply_theme(hwnd);
    let u = unsafe { ui(hwnd) };
    check("어둡게 적용: 창·목록 색이 어두운 팔레트", u.kind == ui_theme::Kind::Dark && u.colors.window == ui_theme::Palette::DARK.tab_bg && u.colors.list == ui_theme::Palette::DARK.bg);
    ui_theme::refresh(ui_theme::ThemePref::Light);
    apply_theme(hwnd);
    check("밝게 적용: 밝은 팔레트", unsafe { ui(hwnd) }.colors.list == ui_theme::Palette::LIGHT.bg);
    for n in ["c1.txt", "c2.txt", "c3.txt"] {
        let _ = std::fs::remove_file(dir.join(n));
    }
    unsafe {
        DestroyWindow(hwnd);
        drop(Box::from_raw(st));
    }
    let _ = std::fs::remove_dir_all(&dir);
    if let Some(d) = DATA_DIR.get() {
        if d.starts_with(std::env::temp_dir()) {
            let _ = std::fs::remove_dir_all(d);
        }
    }
    std::fs::write(out, log.join("\n") + "\n").map_err(|e| e.to_string())?;
    Ok(ok)
}
