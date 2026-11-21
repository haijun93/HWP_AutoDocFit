//! 실제 창: 메시지 처리·입력 EDIT 겹침·키보드·마우스·저장. 그림은 `paint::draw`, 값 처리는 `model::Model`.
use crate::layout::{DEFAULT_H, DEFAULT_W, Hit, MIN_H, MIN_W, Rect, Shape, compute, scale};
use crate::model::Model;
use crate::paint::{self, Ui, Zone, shape_of, wide};
use crate::schema::Control;
use std::ptr::{null, null_mut};
use ui_theme::{Palette, ThemePref};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const EM_SETSEL: u32 = 0x00B1;
const EM_SETMARGINS: u32 = 0x00D3;
const WM_MOUSELEAVE: u32 = 0x02A3;
const SEARCH_ID: isize = 101;
const VALUE_ID: isize = 102;

struct State {
    model: Model,
    ui: Ui,
    dpi: u32,
    pref: ThemePref,
    pal: Palette,
    search_edit: HWND,
    value_edit: HWND,
    /// 값 EDIT 로 편집 중인 항목(키, 시작할 때의 글).
    editing: Option<(String, String)>,
    quiet: bool,
    font: HFONT,
    brush: HBRUSH,
    brush_color: u32,
    tracking: bool,
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

unsafe fn edit_text(edit: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(edit).max(0) as usize;
        let mut buf = vec![0u16; n + 1];
        let got = GetWindowTextW(edit, buf.as_mut_ptr(), buf.len() as i32).max(0) as usize;
        String::from_utf16_lossy(&buf[..got])
    }
}

unsafe fn set_edit_text(s: &mut State, edit: HWND, text: &str) {
    unsafe {
        s.quiet = true;
        SetWindowTextW(edit, wide(text).as_ptr());
        s.quiet = false;
    }
}

unsafe fn create_edit(parent: HWND, id: isize, font: HFONT, dpi: u32, visible: bool) -> HWND {
    unsafe {
        let hinst = GetModuleHandleW(null());
        let style = WS_CHILD | ES_AUTOHSCROLL as u32 | ES_LEFT as u32 | if visible { WS_VISIBLE } else { 0 };
        let edit = CreateWindowExW(0, wide("EDIT").as_ptr(), null(), style, 0, 0, 10, 10, parent, id as _, hinst, null());
        SendMessageW(edit, WM_SETFONT, font as WPARAM, 0);
        let m = scale(2, dpi) as isize;
        SendMessageW(edit, EM_SETMARGINS, (EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize, (m | (m << 16)) as LPARAM);
        edit
    }
}

fn is_edit_field(c: &Control) -> bool {
    matches!(c, Control::Number { .. } | Control::Text)
}

/// 검색 EDIT·값 EDIT 의 자리를 현재 배치에 맞춘다.
unsafe fn sync_edits(hwnd: HWND, s: &mut State) {
    unsafe {
        let (w, h) = client_size(hwnd);
        let lay = compute(w, h, s.dpi);
        let eh = scale(22, s.dpi);
        let input = Rect::new(lay.search.left + scale(38, s.dpi), lay.search.top, lay.search.right - scale(10, s.dpi), lay.search.bottom);
        let top = input.top + (input.height() - eh) / 2;
        SetWindowPos(s.search_edit, null_mut(), input.left, top, input.width(), eh, SWP_NOZORDER | SWP_NOACTIVATE);
        if let Some((key, _)) = s.editing.clone() {
            let visible = s.model.visible();
            let pos = visible.iter().position(|i| s.model.items[*i].key == key);
            match pos {
                Some(n) => {
                    let row = lay.row_rect(n, s.ui.scroll);
                    let parts = lay.row_parts(row, Shape::Number);
                    let item = &s.model.items[visible[n]];
                    let field = if matches!(item.control, Control::Number { .. }) { parts.field } else { lay.row_parts(row, Shape::Text).field };
                    let inner = Rect::new(field.left + scale(6, s.dpi), field.top + (field.height() - eh) / 2, field.right - scale(6, s.dpi), field.top + (field.height() - eh) / 2 + eh);
                    let in_list = row.top >= lay.list.top && row.bottom <= lay.list.bottom;
                    if in_list {
                        SetWindowPos(s.value_edit, HWND_TOP, inner.left, inner.top, inner.width(), inner.height(), SWP_NOACTIVATE | SWP_SHOWWINDOW);
                    } else {
                        ShowWindow(s.value_edit, SW_HIDE);
                    }
                }
                None => {
                    ShowWindow(s.value_edit, SW_HIDE);
                }
            }
        } else {
            ShowWindow(s.value_edit, SW_HIDE);
        }
    }
}

/// 검색 EDIT 는 포커스가 있거나 글이 있을 때만 보인다(비어 있으면 아래에 그려 둔 안내 문구가 보이도록).
unsafe fn hide_search_if_empty(s: &State) {
    unsafe {
        if GetWindowTextLengthW(s.search_edit) == 0 && GetFocus() != s.search_edit {
            ShowWindow(s.search_edit, SW_HIDE);
        }
    }
}

unsafe fn begin_edit(hwnd: HWND, s: &mut State, n: usize) {
    unsafe {
        let visible = s.model.visible();
        let Some(&idx) = visible.get(n) else { return };
        let item = s.model.items[idx].clone();
        if !is_edit_field(&item.control) {
            return;
        }
        end_edit(hwnd, s, false);
        let now = s.model.current_text(&item.key);
        s.editing = Some((item.key.clone(), now.clone()));
        s.ui.zone = Zone::List;
        s.ui.focus_row = n;
        set_edit_text(s, s.value_edit, &now);
        sync_edits(hwnd, s);
        SetFocus(s.value_edit);
        SendMessageW(s.value_edit, EM_SETSEL, 0, -1);
        InvalidateRect(hwnd, null(), 0);
    }
}

unsafe fn end_edit(hwnd: HWND, s: &mut State, cancel: bool) {
    unsafe {
        if let Some((key, original)) = s.editing.take() {
            if cancel {
                s.model.set_text(&key, &original);
            }
            ShowWindow(s.value_edit, SW_HIDE);
            if GetFocus() == s.value_edit {
                SetFocus(hwnd);
            }
            InvalidateRect(hwnd, null(), 0);
        }
    }
}

unsafe fn focus_zone(hwnd: HWND, s: &mut State, zone: Zone) {
    unsafe {
        end_edit(hwnd, s, false);
        s.ui.zone = zone;
        if zone == Zone::Search {
            ShowWindow(s.search_edit, SW_SHOW);
            SetFocus(s.search_edit);
        } else {
            SetFocus(hwnd);
            hide_search_if_empty(s);
        }
        InvalidateRect(hwnd, null(), 0);
    }
}

unsafe fn reset_view(hwnd: HWND, s: &mut State) {
    unsafe {
        end_edit(hwnd, s, false);
        s.ui.scroll = 0;
        s.ui.focus_row = 0;
        InvalidateRect(hwnd, null(), 0);
    }
}

unsafe fn do_save(hwnd: HWND, s: &mut State) {
    unsafe {
        end_edit(hwnd, s, false);
        let _ = s.model.save(); // 결과 문구는 model.status 에 들어 있다
        InvalidateRect(hwnd, null(), 0);
    }
}

unsafe fn do_revert(hwnd: HWND, s: &mut State) {
    unsafe {
        end_edit(hwnd, s, false);
        if let Err(e) = s.model.revert() {
            s.model.status = Some(crate::model::Status { text: e, error: true });
        }
        InvalidateRect(hwnd, null(), 0);
    }
}

unsafe fn move_focus(hwnd: HWND, s: &mut State, to: isize) {
    unsafe {
        let rows = s.model.visible().len();
        if rows == 0 {
            return;
        }
        let n = to.clamp(0, rows as isize - 1) as usize;
        s.ui.focus_row = n;
        let (w, h) = client_size(hwnd);
        s.ui.scroll = compute(w, h, s.dpi).scroll_to_show(n, s.ui.scroll, rows);
        InvalidateRect(hwnd, null(), 0);
    }
}

/// 목록에서 포커스 줄의 기본 동작(Enter·Space).
unsafe fn activate_row(hwnd: HWND, s: &mut State, n: usize) {
    unsafe {
        let visible = s.model.visible();
        let Some(&idx) = visible.get(n) else { return };
        let item = s.model.items[idx].clone();
        match item.control {
            Control::Check => s.model.toggle(&item.key),
            Control::Choice(_) => s.model.cycle(&item.key, 1),
            Control::Number { .. } | Control::Text => begin_edit(hwnd, s, n),
            Control::ReadOnly(_) => {}
        }
        InvalidateRect(hwnd, null(), 0);
    }
}

unsafe fn request_close(hwnd: HWND) {
    unsafe {
        PostMessageW(hwnd, WM_CLOSE, 0, 0);
    }
}

fn ctrl_down() -> bool {
    unsafe { GetKeyState(VK_CONTROL as i32) < 0 }
}
fn shift_down() -> bool {
    unsafe { GetKeyState(VK_SHIFT as i32) < 0 }
}

unsafe fn on_key(hwnd: HWND, s: &mut State, vk: u16) -> bool {
    unsafe {
        let rows = s.model.visible().len() as isize;
        if vk == VK_TAB {
            let z = s.ui.zone.next(shift_down());
            focus_zone(hwnd, s, z);
            return true;
        }
        if ctrl_down() && vk == b'F' as u16 {
            focus_zone(hwnd, s, Zone::Search);
            return true;
        }
        if ctrl_down() && vk == b'S' as u16 {
            do_save(hwnd, s);
            return true;
        }
        if vk == VK_ESCAPE {
            if !s.model.search.is_empty() {
                set_edit_text(s, s.search_edit, "");
                s.model.set_search("");
                reset_view(hwnd, s);
            } else {
                request_close(hwnd);
            }
            return true;
        }
        match s.ui.zone {
            Zone::Tree => {
                let g = s.model.group as isize;
                let last = crate::schema::GROUPS.len() as isize - 1;
                let next = match vk {
                    VK_UP => Some(g - 1),
                    VK_DOWN => Some(g + 1),
                    VK_HOME => Some(0),
                    VK_END => Some(last),
                    VK_RIGHT | VK_RETURN => {
                        focus_zone(hwnd, s, Zone::List);
                        return true;
                    }
                    _ => None,
                };
                if let Some(n) = next {
                    s.model.group = n.clamp(0, last) as usize;
                    reset_view(hwnd, s);
                    return true;
                }
            }
            Zone::List => {
                let f = s.ui.focus_row as isize;
                let page = 4;
                let handled = match vk {
                    VK_UP => Some(f - 1),
                    VK_DOWN => Some(f + 1),
                    VK_PRIOR => Some(f - page),
                    VK_NEXT => Some(f + page),
                    VK_HOME => Some(0),
                    VK_END => Some(rows - 1),
                    _ => None,
                };
                if let Some(n) = handled {
                    move_focus(hwnd, s, n);
                    return true;
                }
                if vk == VK_SPACE || vk == VK_RETURN {
                    activate_row(hwnd, s, s.ui.focus_row);
                    return true;
                }
                if vk == VK_LEFT || vk == VK_RIGHT {
                    let visible = s.model.visible();
                    if let Some(&idx) = visible.get(s.ui.focus_row) {
                        let item = s.model.items[idx].clone();
                        let d = if vk == VK_LEFT { -1 } else { 1 };
                        match item.control {
                            Control::Number { .. } => s.model.step(&item.key, d as i64),
                            Control::Choice(_) => s.model.cycle(&item.key, d),
                            Control::Check => s.model.toggle(&item.key),
                            _ => {}
                        }
                        InvalidateRect(hwnd, null(), 0);
                    }
                    return true;
                }
                if vk == VK_DELETE {
                    let visible = s.model.visible();
                    if let Some(&idx) = visible.get(s.ui.focus_row) {
                        let key = s.model.items[idx].key.clone();
                        s.model.reset_to_default(&key);
                        InvalidateRect(hwnd, null(), 0);
                    }
                    return true;
                }
            }
            Zone::Revert if vk == VK_SPACE || vk == VK_RETURN => {
                do_revert(hwnd, s);
                return true;
            }
            Zone::Save if vk == VK_SPACE || vk == VK_RETURN => {
                do_save(hwnd, s);
                return true;
            }
            _ => {}
        }
        false
    }
}

/// EDIT 가 키를 가져가기 전에 Tab·Enter·Esc·↓ 를 처리한다(메시지 루프에서 부른다). 처리했으면 true.
unsafe fn edit_key(msg: &MSG) -> bool {
    unsafe {
        if msg.message != WM_KEYDOWN {
            return false;
        }
        let parent = GetParent(msg.hwnd);
        if parent.is_null() {
            return false;
        }
        let Some(s) = state(parent) else { return false };
        let vk = msg.wParam as u16;
        if msg.hwnd == s.search_edit {
            match vk {
                VK_TAB => {
                    let z = Zone::Search.next(shift_down());
                    focus_zone(parent, s, z);
                    true
                }
                VK_ESCAPE => on_key(parent, s, VK_ESCAPE),
                VK_RETURN => {
                    s.model.search_forced = true;
                    reset_view(parent, s);
                    focus_zone(parent, s, Zone::List);
                    true
                }
                VK_DOWN => {
                    focus_zone(parent, s, Zone::List);
                    true
                }
                _ => false,
            }
        } else if msg.hwnd == s.value_edit {
            match vk {
                VK_RETURN => {
                    end_edit(parent, s, false);
                    SetFocus(parent);
                    true
                }
                VK_ESCAPE => {
                    end_edit(parent, s, true);
                    SetFocus(parent);
                    true
                }
                VK_TAB => {
                    let z = Zone::List.next(shift_down());
                    focus_zone(parent, s, z);
                    true
                }
                _ => false,
            }
        } else {
            false
        }
    }
}

unsafe fn paint_window(hwnd: HWND, s: &State) {
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let hdc = BeginPaint(hwnd, &mut ps);
        let (w, h) = client_size(hwnd);
        if w > 0 && h > 0 {
            let mem = CreateCompatibleDC(hdc);
            let bm = CreateCompatibleBitmap(hdc, w, h);
            let old = SelectObject(mem, bm);
            paint::draw(mem, w, h, &s.model, &s.ui, &s.pal, s.dpi);
            BitBlt(hdc, 0, 0, w, h, mem, 0, 0, SRCCOPY);
            SelectObject(mem, old);
            DeleteObject(bm);
            DeleteDC(mem);
        }
        EndPaint(hwnd, &ps);
    }
}

unsafe fn on_click(hwnd: HWND, s: &mut State, x: i32, y: i32) {
    unsafe {
        end_edit(hwnd, s, false);
        let (w, h) = client_size(hwnd);
        let lay = compute(w, h, s.dpi);
        let visible = s.model.visible();
        match lay.hit(x, y, s.ui.scroll, visible.len()) {
            Hit::Search => focus_zone(hwnd, s, Zone::Search),
            Hit::Tree(i) => {
                if s.model.search_active() || !s.model.search.is_empty() {
                    set_edit_text(s, s.search_edit, "");
                    s.model.set_search("");
                    hide_search_if_empty(s);
                }
                s.model.group = i;
                s.ui.zone = Zone::Tree;
                SetFocus(hwnd);
                reset_view(hwnd, s);
            }
            Hit::Row(n) => {
                s.ui.zone = Zone::List;
                s.ui.focus_row = n;
                SetFocus(hwnd);
                let item = s.model.items[visible[n]].clone();
                let row = lay.row_rect(n, s.ui.scroll);
                let parts = lay.row_parts(row, shape_of(&item.control));
                match &item.control {
                    Control::Check if parts.control.contains(x, y) => s.model.toggle(&item.key),
                    Control::Choice(_) if parts.field.contains(x, y) => s.model.cycle(&item.key, 1),
                    Control::Number { .. } => {
                        if parts.minus.contains(x, y) {
                            s.model.step(&item.key, -1);
                        } else if parts.plus.contains(x, y) {
                            s.model.step(&item.key, 1);
                        } else if parts.field.contains(x, y) {
                            begin_edit(hwnd, s, n);
                        }
                    }
                    Control::Text if parts.field.contains(x, y) => begin_edit(hwnd, s, n),
                    _ => {}
                }
                InvalidateRect(hwnd, null(), 0);
            }
            Hit::Save => {
                s.ui.zone = Zone::Save;
                do_save(hwnd, s);
            }
            Hit::Revert => {
                s.ui.zone = Zone::Revert;
                do_revert(hwnd, s);
            }
            Hit::None => {
                SetFocus(hwnd);
                InvalidateRect(hwnd, null(), 0);
            }
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
            WM_SIZE => {
                if let Some(s) = state(hwnd) {
                    let (w, h) = client_size(hwnd);
                    let rows = s.model.visible().len();
                    s.ui.scroll = compute(w, h, s.dpi).clamp_scroll(s.ui.scroll, rows);
                    sync_edits(hwnd, s);
                    InvalidateRect(hwnd, null(), 0);
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
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                if let Some(s) = state(hwnd) {
                    paint_window(hwnd, s);
                }
                0
            }
            WM_SETFOCUS => 0,
            WM_LBUTTONDOWN => {
                if let Some(s) = state(hwnd) {
                    on_click(hwnd, s, (lp & 0xFFFF) as u16 as i16 as i32, ((lp >> 16) & 0xFFFF) as u16 as i16 as i32);
                }
                0
            }
            WM_MOUSEMOVE => {
                if let Some(s) = state(hwnd) {
                    let (x, y) = ((lp & 0xFFFF) as u16 as i16 as i32, ((lp >> 16) & 0xFFFF) as u16 as i16 as i32);
                    let (w, h) = client_size(hwnd);
                    let hit = compute(w, h, s.dpi).hit(x, y, s.ui.scroll, s.model.visible().len());
                    if hit != s.ui.hover {
                        s.ui.hover = hit;
                        InvalidateRect(hwnd, null(), 0);
                    }
                    if !s.tracking {
                        let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                        TrackMouseEvent(&mut t);
                        s.tracking = true;
                    }
                }
                0
            }
            WM_MOUSELEAVE => {
                if let Some(s) = state(hwnd) {
                    s.tracking = false;
                    s.ui.hover = Hit::None;
                    InvalidateRect(hwnd, null(), 0);
                }
                0
            }
            WM_MOUSEWHEEL => {
                if let Some(s) = state(hwnd) {
                    let delta = ((wp >> 16) as u16 as i16) as i32;
                    let (w, h) = client_size(hwnd);
                    let lay = compute(w, h, s.dpi);
                    let rows = s.model.visible().len();
                    s.ui.scroll = lay.clamp_scroll(s.ui.scroll - delta * lay.row_h / 240, rows);
                    sync_edits(hwnd, s);
                    InvalidateRect(hwnd, null(), 0);
                }
                0
            }
            WM_KEYDOWN => {
                if let Some(s) = state(hwnd) {
                    if on_key(hwnd, s, wp as u16) {
                        return 0;
                    }
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_COMMAND => {
                if let Some(s) = state(hwnd) {
                    let (code, from) = (((wp >> 16) & 0xFFFF) as u32, lp as HWND);
                    if code == EN_SETFOCUS && from == s.search_edit {
                        s.ui.zone = Zone::Search;
                        InvalidateRect(hwnd, null(), 0);
                    }
                    if code == EN_KILLFOCUS && from == s.search_edit && GetWindowTextLengthW(from) == 0 {
                        ShowWindow(from, SW_HIDE);
                    }
                    if code == EN_KILLFOCUS && from == s.value_edit {
                        end_edit(hwnd, s, false);
                    }
                    if code == EN_CHANGE && !s.quiet {
                        if from == s.search_edit {
                            let t = edit_text(from);
                            s.model.set_search(&t);
                            reset_view(hwnd, s);
                        } else if from == s.value_edit {
                            if let Some((key, _)) = s.editing.clone() {
                                let t = edit_text(from);
                                s.model.set_text(&key, &t);
                                InvalidateRect(hwnd, null(), 0);
                            }
                        }
                    }
                }
                0
            }
            WM_CTLCOLOREDIT => {
                if let Some(s) = state(hwnd) {
                    SetTextColor(wp as HDC, s.pal.text);
                    SetBkColor(wp as HDC, s.pal.bg);
                    if s.brush.is_null() || s.brush_color != s.pal.bg {
                        if !s.brush.is_null() {
                            DeleteObject(s.brush);
                        }
                        s.brush = CreateSolidBrush(s.pal.bg);
                        s.brush_color = s.pal.bg;
                    }
                    return s.brush as LRESULT;
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_DPICHANGED => {
                if let Some(s) = state(hwnd) {
                    s.dpi = (wp & 0xFFFF) as u32;
                    let old = s.font;
                    s.font = paint::create_edit_font(s.dpi);
                    SendMessageW(s.search_edit, WM_SETFONT, s.font as WPARAM, 1);
                    SendMessageW(s.value_edit, WM_SETFONT, s.font as WPARAM, 1);
                    DeleteObject(old);
                }
                let r = lp as *const RECT;
                SetWindowPos(hwnd, null_mut(), (*r).left, (*r).top, (*r).right - (*r).left, (*r).bottom - (*r).top, SWP_NOZORDER | SWP_NOACTIVATE);
                if let Some(s) = state(hwnd) {
                    sync_edits(hwnd, s);
                }
                0
            }
            WM_CLOSE => {
                if let Some(s) = state(hwnd) {
                    let n = s.model.changed_count();
                    if n > 0 {
                        let text = format!("저장하지 않은 변경 {n}건이 있습니다. 저장하지 않고 닫을까요?");
                        let r = MessageBoxW(hwnd, wide(&text).as_ptr(), wide("설정").as_ptr(), MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2);
                        if r != IDYES {
                            return 0;
                        }
                    }
                }
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
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
                    InvalidateRect(hwnd, null(), 0);
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
        }
    }
}

/// 설정 창을 열고 닫힐 때까지 돈다.
pub fn run(model: Model) -> Result<(), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let hinst = GetModuleHandleW(null());
        let class = wide("SettingsWinMain");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
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
        let mut r = RECT { left: 0, top: 0, right: scale(DEFAULT_W, dpi), bottom: scale(DEFAULT_H, dpi) };
        AdjustWindowRectExForDpi(&mut r, WS_OVERLAPPEDWINDOW, 0, 0, dpi);
        let state = Box::new(State {
            model,
            ui: Ui::default(),
            dpi,
            pref,
            pal,
            search_edit: null_mut(),
            value_edit: null_mut(),
            editing: None,
            quiet: false,
            font: paint::create_edit_font(dpi),
            brush: null_mut(),
            brush_color: 0,
            tracking: false,
        });
        let raw = Box::into_raw(state);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            wide("앱 설정").as_ptr(),
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
        let dpi = GetDpiForWindow(hwnd).max(96);
        if dpi != GetDpiForSystem().max(96) {
            // 창이 시스템과 다른 DPI 의 모니터에 떴다: 그 DPI 에 맞는 크기로 다시 맞춘다.
            let mut r = RECT { left: 0, top: 0, right: scale(DEFAULT_W, dpi), bottom: scale(DEFAULT_H, dpi) };
            AdjustWindowRectExForDpi(&mut r, WS_OVERLAPPEDWINDOW, 0, 0, dpi);
            SetWindowPos(hwnd, null_mut(), 0, 0, r.right - r.left, r.bottom - r.top, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
        let s = &mut *raw;
        s.dpi = dpi;
        let old = s.font;
        s.font = paint::create_edit_font(dpi);
        DeleteObject(old);
        s.search_edit = create_edit(hwnd, SEARCH_ID, s.font, dpi, false);
        s.value_edit = create_edit(hwnd, VALUE_ID, s.font, dpi, false);
        ui_theme::apply_window(hwnd, kind);
        sync_edits(hwnd, s);
        ShowWindow(hwnd, SW_SHOW);
        SetFocus(hwnd);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if edit_key(&msg) {
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let s = Box::from_raw(raw);
        DeleteObject(s.font);
        if !s.brush.is_null() {
            DeleteObject(s.brush);
        }
        Ok(())
    }
}
