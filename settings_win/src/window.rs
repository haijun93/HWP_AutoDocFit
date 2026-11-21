//! 실제 창: 메시지 처리·입력 EDIT 겹침·키보드·마우스·저장. 그림은 `paint::draw`, 값 처리는 `model::Model`.
use crate::layout::{DEFAULT_H, DEFAULT_W, Hit, MIN_H, MIN_W, Rect, compute, scale};
use crate::model::Model;
use crate::controls::{self, COLORS};
use crate::paint::{self, Overlay, Ui, Zone, shape_of, wide};
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
    matches!(c, Control::Number { .. } | Control::Text | Control::Font | Control::Folder)
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
        if s.ui.overlay.as_ref().is_some_and(|o| o.is_panel()) {
            let p = lay.overlay();
            let y = p.input.top + (p.input.height() - eh) / 2;
            SetWindowPos(s.value_edit, HWND_TOP, p.input.left + scale(6, s.dpi), y, p.input.width() - scale(12, s.dpi), eh, SWP_NOACTIVATE | SWP_SHOWWINDOW);
            return;
        }
        if let Some((key, _)) = s.editing.clone() {
            let visible = s.model.visible();
            let pos = visible.iter().position(|i| s.model.items[*i].key == key);
            match pos {
                Some(n) => {
                    let row = lay.row_rect(n, s.ui.scroll);
                    let item = &s.model.items[visible[n]];
                    let field = lay.row_parts(row, shape_of(&item.control)).field;
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

unsafe fn open_fonts(hwnd: HWND, s: &mut State, key: &str) {
    unsafe {
        end_edit(hwnd, s, false);
        let cur = s.model.current_text(key).trim().to_lowercase();
        let names = s.model.font_choices("");
        let sel = names.iter().position(|n| n.to_lowercase() == cur).unwrap_or(0);
        let (w, h) = client_size(hwnd);
        let scroll = compute(w, h, s.dpi).overlay().scroll_to_show(sel, 0, names.len());
        s.ui.overlay = Some(Overlay::Fonts { key: key.to_string(), filter: String::new(), sel, scroll });
        set_edit_text(s, s.value_edit, "");
        sync_edits(hwnd, s);
        SetFocus(s.value_edit);
        InvalidateRect(hwnd, null(), 0);
    }
}

unsafe fn open_list_edit(hwnd: HWND, s: &mut State, key: &str) {
    unsafe {
        end_edit(hwnd, s, false);
        s.ui.overlay = Some(Overlay::ListEdit { key: key.to_string(), sel: None, input: String::new(), scroll: 0, note: None });
        set_edit_text(s, s.value_edit, "");
        sync_edits(hwnd, s);
        SetFocus(s.value_edit);
        InvalidateRect(hwnd, null(), 0);
    }
}

unsafe fn start_hotkey_wait(hwnd: HWND, s: &mut State, key: &str) {
    unsafe {
        end_edit(hwnd, s, false);
        s.ui.overlay = Some(Overlay::HotkeyWait { key: key.to_string(), note: None });
        SetFocus(hwnd);
        InvalidateRect(hwnd, null(), 0);
    }
}

unsafe fn close_overlay(hwnd: HWND, s: &mut State) {
    unsafe {
        s.ui.overlay = None;
        ShowWindow(s.value_edit, SW_HIDE);
        SetFocus(hwnd);
        InvalidateRect(hwnd, null(), 0);
    }
}

unsafe fn browse_folder(hwnd: HWND, s: &mut State, key: &str) {
    unsafe {
        end_edit(hwnd, s, false);
        let now = s.model.current_text(key);
        if let Some(path) = controls::pick_folder(hwnd, "폴더 선택", &now) {
            s.model.set_text(key, &path);
        }
        InvalidateRect(hwnd, null(), 0);
    }
}

fn key_down(vk: u16) -> bool {
    unsafe { GetKeyState(vk as i32) < 0 }
}

/// 단축키 칸이 기다리는 중이면 눌린 키를 받는다. 처리했으면 true.
unsafe fn hotkey_key(hwnd: HWND, s: &mut State, vk: u16) -> bool {
    unsafe {
        let Some(Overlay::HotkeyWait { key, .. }) = s.ui.overlay.clone() else { return false };
        if vk == VK_ESCAPE {
            s.ui.overlay = None;
        } else if vk == VK_TAB {
            s.ui.overlay = None;
            InvalidateRect(hwnd, null(), 0);
            return false;
        } else {
            match controls::hotkey_from_press(key_down(VK_CONTROL), key_down(VK_MENU), key_down(VK_SHIFT), key_down(VK_LWIN) || key_down(VK_RWIN), vk as u32) {
                None => {}
                Some(Ok(chord)) => {
                    s.model.set_text(&key, &chord);
                    s.ui.overlay = None;
                }
                Some(Err(why)) => s.ui.overlay = Some(Overlay::HotkeyWait { key, note: Some(why) }),
            }
        }
        InvalidateRect(hwnd, null(), 0);
        true
    }
}

unsafe fn overlay_text_changed(hwnd: HWND, s: &mut State, text: &str) {
    unsafe {
        match s.ui.overlay.as_mut() {
            Some(Overlay::Fonts { filter, sel, scroll, .. }) => {
                *filter = text.to_string();
                *sel = 0;
                *scroll = 0;
            }
            Some(Overlay::ListEdit { input, note, .. }) => {
                *input = text.to_string();
                *note = None;
            }
            _ => {}
        }
        InvalidateRect(hwnd, null(), 0);
    }
}

/// 글꼴 목록 줄을 고른다(값 반영 + 패널 닫기).
unsafe fn choose_font(hwnd: HWND, s: &mut State, key: &str, name: &str) {
    unsafe {
        s.model.set_text(key, name);
        close_overlay(hwnd, s);
    }
}

/// 목록 편집 패널의 동작. `what`: "add" | "remove" | "up" | "down".
unsafe fn list_action(hwnd: HWND, s: &mut State, what: &str) {
    unsafe {
        let Some(Overlay::ListEdit { key, sel, input, scroll, .. }) = s.ui.overlay.clone() else { return };
        let (w, h) = client_size(hwnd);
        let p = compute(w, h, s.dpi).overlay();
        let mut new_sel = sel;
        let mut new_input = input.clone();
        let mut note = None;
        match what {
            "add" => match s.model.list_add(&key, &input) {
                Ok(at) => {
                    new_sel = Some(at);
                    new_input.clear();
                    set_edit_text(s, s.value_edit, "");
                }
                Err(e) => note = Some(e),
            },
            "remove" => {
                if let Some(i) = sel {
                    s.model.list_remove(&key, i);
                    let n = s.model.current_list(&key).len();
                    new_sel = if n == 0 { None } else { Some(i.min(n - 1)) };
                }
            }
            "up" | "down" => {
                if let Some(i) = sel {
                    if let Some(to) = s.model.list_move(&key, i, if what == "up" { -1 } else { 1 }) {
                        new_sel = Some(to);
                    }
                }
            }
            _ => {}
        }
        let count = s.model.current_list(&key).len();
        let new_scroll = match new_sel {
            Some(i) => p.scroll_to_show(i, scroll, count),
            None => p.scroll_to_show(0, scroll.min(p.max_scroll(count)), count).min(p.max_scroll(count)),
        };
        s.ui.overlay = Some(Overlay::ListEdit { key, sel: new_sel, input: new_input, scroll: new_scroll, note });
        InvalidateRect(hwnd, null(), 0);
    }
}

/// 패널이 떠 있을 때의 키. `from_edit` = 입력 EDIT 가 키를 받은 경우(문자 편집 키는 EDIT 에 남긴다).
unsafe fn overlay_key(hwnd: HWND, s: &mut State, vk: u16, from_edit: bool) -> bool {
    unsafe {
        let Some(o) = s.ui.overlay.clone() else { return false };
        if !o.is_panel() {
            return false;
        }
        let (w, h) = client_size(hwnd);
        let p = compute(w, h, s.dpi).overlay();
        if vk == VK_ESCAPE {
            close_overlay(hwnd, s);
            return true;
        }
        if vk == VK_TAB {
            return true;
        }
        let page = (p.list.height() / p.item_h.max(1)).max(1) as isize;
        match o {
            Overlay::Fonts { key, filter, sel, scroll } => {
                let names = s.model.font_choices(&filter);
                let last = names.len() as isize - 1;
                let to = match vk {
                    VK_UP => sel as isize - 1,
                    VK_DOWN => sel as isize + 1,
                    VK_PRIOR => sel as isize - page,
                    VK_NEXT => sel as isize + page,
                    VK_HOME if !from_edit => 0,
                    VK_END if !from_edit => last,
                    VK_RETURN => {
                        if let Some(name) = names.get(sel) {
                            choose_font(hwnd, s, &key, name);
                        }
                        return true;
                    }
                    _ => return false,
                };
                if last >= 0 {
                    let n = to.clamp(0, last) as usize;
                    let sc = p.scroll_to_show(n, scroll, names.len());
                    s.ui.overlay = Some(Overlay::Fonts { key, filter, sel: n, scroll: sc });
                }
                InvalidateRect(hwnd, null(), 0);
                true
            }
            Overlay::ListEdit { key, sel, input, .. } => {
                let count = s.model.current_list(&key).len();
                match vk {
                    VK_RETURN => list_action(hwnd, s, "add"),
                    VK_DELETE if !from_edit || input.is_empty() => list_action(hwnd, s, "remove"),
                    VK_UP | VK_DOWN if ctrl_down() => list_action(hwnd, s, if vk == VK_UP { "up" } else { "down" }),
                    VK_UP | VK_DOWN | VK_PRIOR | VK_NEXT => {
                        if count > 0 {
                            let cur = sel.map(|i| i as isize).unwrap_or(-1);
                            let to = match vk {
                                VK_UP => cur - 1,
                                VK_DOWN => cur + 1,
                                VK_PRIOR => cur - page,
                                _ => cur + page,
                            };
                            let n = to.clamp(0, count as isize - 1) as usize;
                            if let Some(Overlay::ListEdit { sel: s2, scroll, .. }) = s.ui.overlay.as_mut() {
                                *s2 = Some(n);
                                *scroll = p.scroll_to_show(n, *scroll, count);
                            }
                        }
                        InvalidateRect(hwnd, null(), 0);
                    }
                    _ => return false,
                }
                true
            }
            Overlay::HotkeyWait { .. } => false,
        }
    }
}

/// 패널 안·밖 클릭. 패널 밖이면 닫는다.
unsafe fn overlay_click(hwnd: HWND, s: &mut State, x: i32, y: i32) {
    unsafe {
        let Some(o) = s.ui.overlay.clone() else { return };
        let (w, h) = client_size(hwnd);
        let p = compute(w, h, s.dpi).overlay();
        if !p.panel.contains(x, y) || p.close_btn.contains(x, y) {
            close_overlay(hwnd, s);
            return;
        }
        match o {
            Overlay::Fonts { key, filter, scroll, .. } => {
                if p.list.contains(x, y) {
                    let at = ((y - p.list.top + scroll) / p.item_h.max(1)) as usize;
                    if let Some(name) = s.model.font_choices(&filter).get(at).cloned() {
                        choose_font(hwnd, s, &key, &name);
                    }
                }
            }
            Overlay::ListEdit { key, scroll, .. } => {
                if p.add_btn.contains(x, y) {
                    list_action(hwnd, s, "add");
                } else if p.remove_btn.contains(x, y) {
                    list_action(hwnd, s, "remove");
                } else if p.up_btn.contains(x, y) {
                    list_action(hwnd, s, "up");
                } else if p.down_btn.contains(x, y) {
                    list_action(hwnd, s, "down");
                } else if p.list.contains(x, y) {
                    let at = ((y - p.list.top + scroll) / p.item_h.max(1)) as usize;
                    let count = s.model.current_list(&key).len();
                    if let Some(Overlay::ListEdit { sel, .. }) = s.ui.overlay.as_mut() {
                        *sel = if at < count { Some(at) } else { None };
                    }
                }
                InvalidateRect(hwnd, null(), 0);
            }
            Overlay::HotkeyWait { .. } => {}
        }
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
            Control::Color => s.model.cycle_color(&item.key, 1),
            Control::Number { .. } | Control::Text | Control::Font | Control::Folder => begin_edit(hwnd, s, n),
            Control::List { .. } => open_list_edit(hwnd, s, &item.key),
            Control::Hotkey => start_hotkey_wait(hwnd, s, &item.key),
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
        if s.ui.overlay.as_ref().is_some_and(|o| o.is_panel()) {
            overlay_key(hwnd, s, vk, false);
            return true;
        }
        if vk == VK_F4 && s.ui.zone == Zone::List {
            // 글꼴·폴더 항목의 고르기/찾아보기(▾·[찾아보기…] 단추의 키보드 대응)
            let visible = s.model.visible();
            if let Some(&idx) = visible.get(s.ui.focus_row) {
                let item = s.model.items[idx].clone();
                match item.control {
                    Control::Font => open_fonts(hwnd, s, &item.key),
                    Control::Folder => browse_folder(hwnd, s, &item.key),
                    _ => {}
                }
            }
            return true;
        }
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
                            Control::Color => s.model.cycle_color(&item.key, d),
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
            if s.ui.overlay.as_ref().is_some_and(|o| o.is_panel()) {
                let empty = GetWindowTextLengthW(msg.hwnd) == 0;
                return match vk {
                    VK_ESCAPE | VK_UP | VK_DOWN | VK_PRIOR | VK_NEXT | VK_RETURN | VK_TAB => overlay_key(parent, s, vk, true),
                    VK_DELETE if empty => overlay_key(parent, s, vk, true),
                    _ => false,
                };
            }
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
        if s.ui.overlay.as_ref().is_some_and(|o| o.is_panel()) {
            overlay_click(hwnd, s, x, y);
            return;
        }
        if s.ui.overlay.is_some() {
            s.ui.overlay = None; // 단축키 입력 대기는 다른 곳을 누르면 끝난다
        }
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
                    Control::Color => {
                        for (i, c) in COLORS.iter().enumerate() {
                            if lay.swatch(parts.control, i).contains(x, y) {
                                s.model.set_text(&item.key, c.0);
                            }
                        }
                    }
                    Control::Font => {
                        if parts.plus.contains(x, y) {
                            open_fonts(hwnd, s, &item.key);
                        } else if parts.field.contains(x, y) {
                            begin_edit(hwnd, s, n);
                        }
                    }
                    Control::Folder => {
                        if parts.plus.contains(x, y) {
                            browse_folder(hwnd, s, &item.key);
                        } else if parts.field.contains(x, y) {
                            begin_edit(hwnd, s, n);
                        }
                    }
                    Control::List { .. } if parts.field.contains(x, y) || parts.plus.contains(x, y) => open_list_edit(hwnd, s, &item.key),
                    Control::Hotkey => {
                        if parts.plus.contains(x, y) {
                            s.model.set_text(&item.key, "");
                        } else if parts.field.contains(x, y) {
                            start_hotkey_wait(hwnd, s, &item.key);
                        }
                    }
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
                    let p = lay.overlay();
                    let count = match &s.ui.overlay {
                        Some(Overlay::Fonts { filter, .. }) => Some(s.model.font_choices(filter).len()),
                        Some(Overlay::ListEdit { key, .. }) => Some(s.model.current_list(key).len()),
                        _ => None,
                    };
                    if let Some(count) = count {
                        if let Some(Overlay::Fonts { scroll, .. } | Overlay::ListEdit { scroll, .. }) = s.ui.overlay.as_mut() {
                            *scroll = (*scroll - delta * p.item_h / 120).clamp(0, p.max_scroll(count));
                        }
                        InvalidateRect(hwnd, null(), 0);
                        return 0;
                    }
                    let rows = s.model.visible().len();
                    s.ui.scroll = lay.clamp_scroll(s.ui.scroll - delta * lay.row_h / 240, rows);
                    sync_edits(hwnd, s);
                    InvalidateRect(hwnd, null(), 0);
                }
                0
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if let Some(s) = state(hwnd) {
                    if hotkey_key(hwnd, s, wp as u16) {
                        return 0;
                    }
                    if msg == WM_KEYDOWN && on_key(hwnd, s, wp as u16) {
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
                        } else if from == s.value_edit && s.ui.overlay.as_ref().is_some_and(|o| o.is_panel()) {
                            let t = edit_text(from);
                            overlay_text_changed(hwnd, s, &t);
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

/// 단일 인스턴스: 같은 설정 파일을 여는 창이 이미 떠 있으면 그 창을 앞으로 가져오고 true(이 프로세스는 곧 끝낸다).
/// 뮤텍스 이름에 설정 파일 경로의 해시를 붙여, 다른 설정 파일(시험·캡처)을 여는 창과는 섞이지 않는다.
pub fn activate_existing_instance(settings_path: &str) -> bool {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    unsafe {
        let mut hash: u64 = 0xcbf29ce484222325;
        for b in settings_path.to_lowercase().bytes() {
            hash = (hash ^ b as u64).wrapping_mul(0x100000001b3);
        }
        let name = format!(r"Local\HwpAutoDocFit.SettingsWin.Singleton.{hash:016x}");
        // 핸들은 일부러 닫지 않는다: 프로세스가 끝날 때까지 뮤텍스를 쥐고 있어야 한다.
        let handle = CreateMutexW(null(), 0, wide(&name).as_ptr());
        if handle.is_null() || GetLastError() != ERROR_ALREADY_EXISTS {
            return false;
        }
        let existing = FindWindowW(wide("SettingsWinMain").as_ptr(), null());
        if !existing.is_null() {
            if IsIconic(existing) != 0 {
                ShowWindow(existing, SW_RESTORE);
            }
            SetForegroundWindow(existing);
        }
        true
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
