//! 블록 목록 창 + 편집 중 블록 위에만 겹치는 Win32 EDIT(한 줄, `ES_AUTOHSCROLL`).
//! 키·IME 처리는 `edit::Editor`(순수 상태 기계)에 맡기고, 여기서는 메시지를 `KeyEvent` 로 바꾸고
//! 결과(EDIT 위치·제목 줄 ●·안내 문구)를 화면에 반영만 한다.
use crate::capture::{Shot, palette_of};
use crate::edit::{Cmd, EditHost, Editor, KeyEvent, KeyResult, VK_OEM_1, VK_Y, VK_Z};
use crate::layout::{self, Btn, Layout, LeftItem, RightItem};
use crate::paint::{Extras, Metrics, create_font, scale, wide};
use crate::paint_shell::{draw_shell, edit_rect_of};
use crate::shell::{PromptKind, Shell};
use crate::view::{Nav, ensure_visible};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use ui_theme::{Kind, Palette, ThemePref};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemInformation::GetTickCount64;
use windows_sys::Win32::UI::Controls::SetScrollInfo;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::Ime::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const WM_APP_RUN_PENDING: u32 = WM_APP + 1;
const WM_APP_FOLLOW: u32 = WM_APP + 2;
const TIMER_ID: usize = 1;
const TIMER_MS: u32 = 100;
const EDIT_ID: isize = 101;
const SEARCH_ID: isize = 102;
const EN_CHANGE_CODE: u32 = 0x0300;
// windows-sys 0.59 에는 EDIT 메시지 상수가 없어 직접 둔다(winuser.h 값).
const EM_GETSEL: u32 = 0x00B0;
const EM_SETSEL: u32 = 0x00B1;
const EM_SETMARGINS: u32 = 0x00D3;
const EM_POSFROMCHAR: u32 = 0x00D6;
const EM_SCROLLCARET: u32 = 0x00B7;
const EM_CHARFROMPOS: u32 = 0x00D7;
const CF_UNICODETEXT: u32 = 13;
const EM_REPLACESEL: u32 = 0x00C2;
const GMEM_MOVEABLE: u32 = 2;

#[link(name = "user32")]
unsafe extern "system" {
    fn OpenClipboard(hwnd: HWND) -> BOOL;
    fn EmptyClipboard() -> BOOL;
    fn SetClipboardData(format: u32, memory: HANDLE) -> HANDLE;
    fn GetClipboardData(format: u32) -> HANDLE;
    fn CloseClipboard() -> BOOL;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalAlloc(flags: u32, bytes: usize) -> HGLOBAL;
    fn GlobalLock(memory: HGLOBAL) -> *mut core::ffi::c_void;
    fn GlobalUnlock(memory: HGLOBAL) -> BOOL;
    fn GlobalFree(memory: HGLOBAL) -> HGLOBAL;
    fn GlobalSize(memory: HGLOBAL) -> usize;
}

/// EDIT 하나뿐이라 IME 상태는 전역(서브클래스 프로시저가 `State` 를 잡지 않고도 갱신해야 해서).
static COMPOSING: AtomicBool = AtomicBool::new(false);
static CANDIDATE: AtomicBool = AtomicBool::new(false);
static ORIG_EDIT_PROC: AtomicIsize = AtomicIsize::new(0);
static ORIG_SEARCH_PROC: AtomicIsize = AtomicIsize::new(0);
/// 방금 KEYDOWN 을 우리가 처리했다 → 뒤따르는 제어 문자(Backspace 8 등)는 EDIT 에 넘기지 않는다.
static EAT_CONTROL_CHAR: AtomicBool = AtomicBool::new(false);

/// 여러 줄 EDIT 의 줄바꿈은 `\r\n`, 규칙 쪽은 `\n` 이다. EDIT 오프셋 → 규칙 오프셋.
fn from_edit_offset(raw: &[u16], offset: usize) -> usize {
    offset - raw[..offset.min(raw.len())].iter().filter(|&&c| c == b'\r' as u16).count()
}

/// 규칙 오프셋(`\n` 한 글자) → EDIT 오프셋(`\r\n` 두 글자).
fn to_edit_offset(text: &str, offset: usize) -> usize {
    offset + text.encode_utf16().take(offset).filter(|&c| c == b'\n' as u16).count()
}

unsafe fn window_units(hwnd: HWND) -> Vec<u16> {
    unsafe {
        let n = GetWindowTextLengthW(hwnd).max(0) as usize;
        let mut buf = vec![0u16; n + 2];
        let got = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        buf.truncate(got.max(0) as usize);
        buf
    }
}

fn now_ms() -> u64 {
    unsafe { GetTickCount64() }
}

/// 실제 EDIT 컨트롤을 `EditHost` 로 만든 것.
pub struct WinHost {
    pub edit: HWND,
}

impl WinHost {
    pub fn new() -> WinHost {
        WinHost { edit: null_mut() }
    }
}

impl Default for WinHost {
    fn default() -> Self {
        Self::new()
    }
}

unsafe fn comp_len(hwnd: HWND) -> i32 {
    unsafe {
        let imc = ImmGetContext(hwnd);
        if imc.is_null() {
            return 0;
        }
        let n = ImmGetCompositionStringW(imc, GCS_COMPSTR, null_mut(), 0);
        ImmReleaseContext(hwnd, imc);
        n.max(0)
    }
}

impl EditHost for WinHost {
    fn read_text(&mut self) -> String {
        unsafe { String::from_utf16_lossy(&window_units(self.edit)).replace("\r\n", "\n") }
    }
    fn selection(&mut self) -> (usize, usize) {
        unsafe {
            let (mut s, mut e) = (0u32, 0u32);
            SendMessageW(self.edit, EM_GETSEL, &mut s as *mut u32 as WPARAM, &mut e as *mut u32 as LPARAM);
            let raw = window_units(self.edit);
            (from_edit_offset(&raw, s as usize), from_edit_offset(&raw, e as usize))
        }
    }
    fn set_text(&mut self, text: &str, caret: usize) {
        unsafe {
            COMPOSING.store(false, Ordering::SeqCst);
            SetWindowTextW(self.edit, wide(&text.replace('\n', "\r\n")).as_ptr());
            let at = to_edit_offset(text, caret);
            SendMessageW(self.edit, EM_SETSEL, at, at as LPARAM);
        }
    }
    fn set_selection(&mut self, start: usize, end: usize) {
        let text = self.read_text();
        unsafe {
            SendMessageW(self.edit, EM_SETSEL, to_edit_offset(&text, start), to_edit_offset(&text, end) as LPARAM);
        }
    }
    fn composing(&mut self) -> bool {
        COMPOSING.load(Ordering::SeqCst) || unsafe { comp_len(self.edit) > 0 }
    }
    fn candidate_open(&mut self) -> bool {
        CANDIDATE.load(Ordering::SeqCst)
    }
    fn request_complete(&mut self) {
        unsafe {
            let imc = ImmGetContext(self.edit);
            if !imc.is_null() {
                ImmNotifyIME(imc, NI_COMPOSITIONSTR, CPS_COMPLETE, 0);
                ImmReleaseContext(self.edit, imc);
            }
        }
    }
}

pub struct State {
    pub shell: Shell<WinHost>,
    /// 위쪽 검색·이름 입력 칸(페이지 모드에서만 만든다).
    search_edit: HWND,
    /// 프로그램이 검색 칸 글을 바꾸는 중(EN_CHANGE 무시).
    quiet: bool,
    title: String,
    dpi: u32,
    pref: ThemePref,
    /// 캡처 시험에서 팔레트를 고정한다(없으면 시스템 설정을 따른다).
    force_kind: Option<Kind>,
    font: HFONT,
    edit_brush: HBRUSH,
    edit_brush_color: u32,
    last_title: String,
    /// Ctrl+클릭으로 따라갈 링크(EDIT 에서 창으로 우편 전달).
    pending_link: Option<(String, bool)>,
}

unsafe fn state<'a>(hwnd: HWND) -> Option<&'a mut State> {
    unsafe {
        let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        p.as_mut()
    }
}

unsafe fn client(hwnd: HWND) -> RECT {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut r);
        r
    }
}

fn palette(s: &State) -> Palette {
    match s.force_kind {
        Some(k) => palette_of(k),
        None => ui_theme::current(s.pref).1,
    }
}

unsafe fn layout_of(hwnd: HWND, s: &State) -> Layout {
    unsafe {
        let r = client(hwnd);
        layout::compute_visible(r.right, r.bottom, s.dpi, s.shell.panels(), s.shell.left_open, s.shell.right_open)
    }
}

unsafe fn sync_scroll(hwnd: HWND, s: &State) {
    unsafe {
        let m = Metrics::new(s.dpi);
        let page = m.page_rows(layout_of(hwnd, s).center.h());
        let info = SCROLLINFO {
            cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
            fMask: SIF_ALL,
            nMin: 0,
            nMax: s.shell.editor.view.rows.len().max(1) as i32 - 1,
            nPage: page as u32,
            nPos: s.shell.editor.view.first as i32,
            nTrackPos: 0,
        };
        SetScrollInfo(hwnd, SB_VERT, &info, 1);
    }
}

unsafe fn page_rows(hwnd: HWND, s: &State) -> usize {
    unsafe { Metrics::new(s.dpi).page_rows(layout_of(hwnd, s).center.h()) }
}

fn extras_of(s: &mut State) -> Extras {
    let editing_row = s.shell.editor.editing_block().and_then(|b| s.shell.editor.view.rows.iter().position(|r| r.block == b));
    let dirty = s.shell.editor.dirty();
    Extras { editing_row, message: s.shell.editor.message.clone(), dirty }
}

/// 검색·이름 입력 칸의 화면 위치(그리기와 같은 값).
fn search_rect(lay: &Layout, prompt: bool) -> RECT {
    let mut e = lay.search_edit;
    if prompt {
        e.l += scale(24, lay.dpi);
    }
    let d = scale(4, lay.dpi);
    RECT { left: e.l + d, top: e.t + d, right: e.r - d, bottom: e.b - d }
}

/// 편집 상태·스크롤·제목을 화면에 맞춘다. `ensure`: 편집 중인 줄이 보이도록 스크롤(명령 직후에만).
unsafe fn ui_sync(hwnd: HWND, ensure: bool) {
    unsafe {
        let Some(s) = state(hwnd) else { return };
        let lay = layout_of(hwnd, s);
        let m = Metrics::new(s.dpi);
        let page = m.page_rows(lay.center.h());
        s.shell.editor.refresh_popup();
        let edit = s.shell.editor.host.edit;
        let editing_pos = if s.shell.search_open { None } else {
            s.shell.editor.editing_block().and_then(|b| s.shell.editor.view.rows.iter().position(|r| r.block == b))
        };
        match editing_pos {
            Some(pos) => {
                if ensure {
                    let total = s.shell.editor.view.rows.len();
                    debug_assert!(pos < total);
                    s.shell.editor.view.first = s.shell.editor.view.visible_first(pos, page);
                }
                let first = s.shell.editor.view.first;
                let in_view = pos >= first && s.shell.editor.view.slot_of(pos) <= page;
                if in_view {
                    let er = m.row_edit_rect(s.dpi, &s.shell.editor.view, pos, lay.center.w());
                    let (ex, ey) = (er.left + lay.center.l, er.top + lay.center.t);
                    let mut old: RECT = std::mem::zeroed();
                    GetWindowRect(edit, &mut old);
                    let was_visible = IsWindowVisible(edit) != 0;
                    let (nw, nh) = (er.right - er.left, er.bottom - er.top);
                    let resized = old.right - old.left != nw || old.bottom - old.top != nh;
                    MoveWindow(edit, ex, ey, nw, nh, 1);
                    if !was_visible {
                        ShowWindow(edit, SW_SHOW);
                    }
                    if resized || !was_visible {
                        // 좁은 상태에서 글을 넣으면 끝쪽으로 가로 스크롤돼 있다 → 맨 앞으로 되돌린 뒤 캐럿이 보이게 한다.
                        let (mut a, mut b) = (0u32, 0u32);
                        SendMessageW(edit, EM_GETSEL, &mut a as *mut u32 as WPARAM, &mut b as *mut u32 as LPARAM);
                        SendMessageW(edit, EM_SETSEL, 0, 0);
                        SendMessageW(edit, EM_SCROLLCARET, 0, 0);
                        SendMessageW(edit, EM_SETSEL, a as usize, b as LPARAM);
                        SendMessageW(edit, EM_SCROLLCARET, 0, 0);
                    }
                    let searching = !s.search_edit.is_null() && GetFocus() == s.search_edit;
                    if !searching && GetFocus() != edit && !GetParent(edit).is_null() && IsWindowVisible(hwnd) != 0 {
                        SetFocus(edit);
                    }
                } else {
                    ShowWindow(edit, SW_HIDE);
                }
            }
            None => {
                if IsWindowVisible(edit) != 0 {
                    ShowWindow(edit, SW_HIDE);
                    if IsWindowVisible(hwnd) != 0 && (s.search_edit.is_null() || GetFocus() != s.search_edit) {
                        SetFocus(hwnd);
                    }
                }
            }
        }
        if !s.search_edit.is_null() {
            if s.shell.prompt.is_none() && read_edit(s.search_edit) != s.shell.query {
                set_search_text(s, &s.shell.query.clone());
            }
            let r = search_rect(&lay, s.shell.prompt.is_some());
            let show = lay.panels && lay.top.h() > 0;
            MoveWindow(s.search_edit, r.left, r.top, r.right - r.left, r.bottom - r.top, 1);
            if show != (IsWindowVisible(s.search_edit) != 0) {
                ShowWindow(s.search_edit, if show { SW_SHOW } else { SW_HIDE });
            }
        }
        sync_scroll(hwnd, s);
        let dirty = s.shell.editor.dirty();
        let name = s.shell.page.clone().unwrap_or_else(|| s.title.clone());
        let title = format!("{}아웃라이너 — {}", if dirty { "● " } else { "" }, name);
        if title != s.last_title {
            SetWindowTextW(hwnd, wide(&title).as_ptr());
            s.last_title = title;
        }
        InvalidateRect(hwnd, null(), 0);
        show_alert(hwnd);
    }
}

unsafe fn show_alert(hwnd: HWND) {
    unsafe {
        let alert = state(hwnd).and_then(|s| s.shell.editor.alert.take());
        if let Some(text) = alert {
            if IsWindowVisible(hwnd) != 0 {
                MessageBoxW(hwnd, wide(&text).as_ptr(), wide("아웃라이너 — 저장 경고").as_ptr(), MB_OK | MB_ICONWARNING);
            }
        }
    }
}

unsafe fn paint(hwnd: HWND) {
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let dc = BeginPaint(hwnd, &mut ps);
        if let Some(s) = state(hwnd) {
            let r = client(hwnd);
            let lay = layout_of(hwnd, s);
            let extras = extras_of(s);
            let mem = CreateCompatibleDC(dc);
            let bm = CreateCompatibleBitmap(dc, r.right.max(1), r.bottom.max(1));
            let old = SelectObject(mem, bm);
            let pal = palette(s);
            draw_shell(mem, r.right, r.bottom, &s.shell, &pal, s.dpi, &extras, &lay);
            BitBlt(dc, 0, 0, r.right, r.bottom, mem, 0, 0, SRCCOPY);
            SelectObject(mem, old);
            DeleteObject(bm);
            DeleteDC(mem);
        }
        EndPaint(hwnd, &ps);
    }
}

fn shift_down() -> bool {
    unsafe { GetKeyState(VK_SHIFT as i32) < 0 }
}
fn ctrl_down() -> bool {
    unsafe { GetKeyState(VK_CONTROL as i32) < 0 }
}
fn alt_down() -> bool {
    unsafe { GetKeyState(VK_MENU as i32) < 0 }
}

// ---------------- 페이지 모드 동작(단추·검색·클릭) ----------------

fn report(s: &mut State, r: Result<(), String>) {
    if let Err(e) = r {
        s.shell.editor.message = Some(e);
    }
}

unsafe fn set_search_text(s: &mut State, text: &str) {
    unsafe {
        if s.search_edit.is_null() {
            return;
        }
        s.quiet = true;
        SetWindowTextW(s.search_edit, wide(text).as_ptr());
        s.quiet = false;
    }
}

unsafe fn focus_search(hwnd: HWND) {
    unsafe {
        let Some(s) = state(hwnd) else { return };
        if s.search_edit.is_null() || IsWindowVisible(s.search_edit) == 0 {
            return;
        }
        SetFocus(s.search_edit);
        SendMessageW(s.search_edit, EM_SETSEL, 0, -1);
    }
}

unsafe fn begin_prompt(hwnd: HWND, kind: PromptKind) {
    unsafe {
        let Some(s) = state(hwnd) else { return };
        if let Some(initial) = s.shell.begin_prompt(kind) {
            set_search_text(s, &initial);
            ui_sync(hwnd, false);
            focus_search(hwnd);
        }
    }
}

unsafe fn finish_prompt(hwnd: HWND) {
    unsafe {
        let Some(s) = state(hwnd) else { return };
        let text = read_edit(s.search_edit);
        match s.shell.finish_prompt(&text) {
            Ok(()) => {
                set_search_text(s, "");
                SetFocus(hwnd);
            }
            Err(e) => s.shell.editor.message = Some(e),
        }
        ui_sync(hwnd, true);
    }
}

unsafe fn cancel_prompt(hwnd: HWND) {
    unsafe {
        let Some(s) = state(hwnd) else { return };
        s.shell.cancel_prompt();
        set_search_text(s, "");
        SetFocus(hwnd);
        ui_sync(hwnd, false);
    }
}

unsafe fn read_edit(edit: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(edit).max(0) as usize;
        let mut buf = vec![0u16; n + 2];
        let got = GetWindowTextW(edit, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..got.max(0) as usize])
    }
}

unsafe fn search_changed(hwnd: HWND) {
    unsafe {
        let Some(s) = state(hwnd) else { return };
        if s.quiet || s.shell.prompt.is_some() {
            return;
        }
        let text = read_edit(s.search_edit);
        s.shell.set_query(&text);
        ui_sync(hwnd, false);
    }
}

unsafe fn open_hit(hwnd: HWND, i: usize) {
    unsafe {
        let Some(s) = state(hwnd) else { return };
        let page = page_rows(hwnd, s);
        let r = s.shell.open_hit(i, page);
        report(s, r);
        SetFocus(hwnd);
        ui_sync(hwnd, true);
    }
}

unsafe fn do_button(hwnd: HWND, b: Btn) {
    unsafe {
        let Some(s) = state(hwnd) else { return };
        match b {
            Btn::ToggleLeft => s.shell.left_open = !s.shell.left_open,
            Btn::ToggleRight => s.shell.right_open = !s.shell.right_open,
            Btn::Back | Btn::Forward => {
                let rows = page_rows(hwnd, s);
                let r = s.shell.navigate(b == Btn::Forward, rows);
                report(s, r);
            }
            Btn::Favorite => s.shell.toggle_favorite(),
            Btn::Today => {
                let r = s.shell.open_today();
                report(s, r);
            }
            Btn::New => return begin_prompt(hwnd, PromptKind::NewPage),
            Btn::Rename => return begin_prompt(hwnd, PromptKind::Rename),
            Btn::NewBlock => {
                let page = page_rows(hwnd, s);
                if s.shell.page.is_none() {
                    return;
                }
                s.shell.editor.add_block_at_end(page);
            }
            Btn::Indent | Btn::Outdent => {
                if s.shell.page.is_none() {
                    return;
                }
                s.shell.editor.nav_command(if b == Btn::Indent { Cmd::Indent } else { Cmd::Outdent });
            }
            Btn::Delete => {
                let Some(name) = s.shell.page.clone() else { return };
                let msg = format!("'{name}' 페이지를 삭제할까요?\n\n삭제하면 데이터 폴더의 .trash 폴더로 옮겨지며, 파일 탐색기에서 되돌릴 수 있습니다.");
                let ans = MessageBoxW(hwnd, wide(&msg).as_ptr(), wide("아웃라이너 — 페이지 삭제").as_ptr(), MB_YESNO | MB_ICONQUESTION);
                if ans == IDYES {
                    let r = s.shell.delete_current();
                    report(s, r);
                }
            }
        }
        ui_sync(hwnd, true);
    }
}

unsafe fn search_key(parent: HWND, vk: u16) -> bool {
    unsafe {
        let Some(s) = state(parent) else { return false };
        match vk {
            VK_RETURN => {
                if s.shell.prompt.is_some() {
                    finish_prompt(parent);
                } else if s.shell.search_open && !s.shell.hits.is_empty() {
                    let i = s.shell.hit_sel;
                    open_hit(parent, i);
                }
                true
            }
            VK_ESCAPE => {
                if s.shell.prompt.is_some() {
                    cancel_prompt(parent);
                } else {
                    s.shell.close_search();
                    set_search_text(s, "");
                    SetFocus(parent);
                    ui_sync(parent, false);
                }
                true
            }
            VK_DOWN | VK_UP if s.shell.search_open => {
                s.shell.move_hit(vk == VK_DOWN);
                let slots = layout_of(parent, s).result_slots().max(1);
                s.shell.hit_first = ensure_visible(s.shell.hit_first, s.shell.hit_sel, s.shell.hits.len(), slots);
                InvalidateRect(parent, null(), 0);
                true
            }
            _ => false,
        }
    }
}

/// 목록(편집 아님) 상태의 키.
unsafe fn global_key(hwnd: HWND, vk: u16, ctrl: bool, alt: bool, shift: bool, input: bool) -> bool {
    unsafe {
        let Some(s) = state(hwnd) else { return false };
        if !s.shell.panels() || s.shell.prompt.is_some() { return false; }
        let action = if ctrl && !alt && !shift && matches!(vk, 0xDB | 0xDD) {
            Some(if vk == 0xDB { Btn::Back } else { Btn::Forward })
        } else if ctrl && shift && !alt && vk == b'F' as u16 {
            Some(Btn::Favorite)
        } else if !input && !ctrl && !alt && !shift {
            let now = now_ms();
            let prefix = s.shell.toggle_prefix.take().is_some_and(|t| now.saturating_sub(t) < 1500);
            if prefix && matches!(vk, 0x4C | 0x52) {
                Some(if vk == 0x4C { Btn::ToggleLeft } else { Btn::ToggleRight })
            } else {
                if vk == b'T' as u16 { s.shell.toggle_prefix = Some(now); return true; }
                None
            }
        } else { s.shell.toggle_prefix = None; None };
        if let Some(button) = action { do_button(hwnd, button); return true; }
        if ctrl && !alt && !shift && vk == b'K' as u16 {
            if s.shell.editor.finish_edit() { focus_search(hwnd); ui_sync(hwnd, false); }
            return true;
        }
        false
    }
}

unsafe fn key_down(hwnd: HWND, vk: u16) -> bool {
    unsafe { key_down_modifiers(hwnd, vk, ctrl_down(), alt_down(), shift_down()) }
}

unsafe fn key_down_modifiers(hwnd: HWND, vk: u16, ctrl: bool, alt: bool, shift: bool) -> bool {
    unsafe {
        if global_key(hwnd, vk, ctrl, alt, shift, false) { return true; }
        let Some(s) = state(hwnd) else { return false };
        if s.shell.editor.editing() {
            return false;
        }
        let page = page_rows(hwnd, s);
        let (ctrl, alt) = (ctrl, alt);
        if s.shell.panels() {
            if ctrl && vk == b'F' as u16 {
                focus_search(hwnd);
                return true;
            }
            if ctrl && vk == b'N' as u16 {
                begin_prompt(hwnd, PromptKind::NewPage);
                return true;
            }
            if s.shell.search_open {
                match vk {
                    VK_ESCAPE => return search_key(hwnd, vk),
                    VK_RETURN if !s.shell.hits.is_empty() => {
                        let i = s.shell.hit_sel;
                        open_hit(hwnd, i);
                        return true;
                    }
                    VK_UP | VK_DOWN => return search_key(hwnd, vk),
                    _ => {}
                }
            }
        }
        let nav = match vk {
            VK_UP => Some(Nav::Up),
            VK_DOWN => Some(Nav::Down),
            VK_PRIOR => Some(Nav::PageUp),
            VK_NEXT => Some(Nav::PageDown),
            VK_HOME => Some(Nav::Home),
            VK_END => Some(Nav::End),
            _ => None,
        };
        let selecting = s.shell.editor.view.selection_anchor.is_some();
        let vertical = vk == VK_UP || vk == VK_DOWN;
        let up = vk == VK_UP;
        let toward = if up { Nav::Up } else { Nav::Down };
        if vk == VK_ESCAPE && selecting {
            s.shell.editor.view.clear_selection();
        } else if alt && shift && vertical {
            s.shell.editor.selection_command(if up { "move_up" } else { "move_down" });
        } else if (alt || shift) && !ctrl && vertical {
            s.shell.editor.view.extend_selection(toward, page);
        } else if ctrl && !alt && (vertical || vk == VK_OEM_1) {
            s.shell.editor.selection_command(if vk == VK_OEM_1 { "toggle_collapse" } else if up { "collapse" } else { "expand" });
        } else if (vk == VK_LEFT || vk == VK_RIGHT) && selecting {
            s.shell.editor.selection_command(if vk == VK_LEFT { "collapse" } else { "expand" });
        } else if ctrl && !alt && shift && vk == b'A' as u16 {
            s.shell.editor.view.select_all();
        } else if ctrl && !alt && vk == b'A' as u16 && selecting {
            s.shell.editor.view.select_parent();
        } else if ctrl && !alt && selecting && matches!(vk, 0x43 | 0x58 | 0x56) {
            if vk == 0x56 {
                if let Some(text) = clipboard_read(hwnd) {
                    s.shell.editor.selection_paste(&text);
                }
            } else if let Some(markdown) = s.shell.editor.selection_command(if vk == 0x58 { "cut" } else { "copy" }) {
                clipboard_write(hwnd, &markdown.replace('\n', "\r\n"));
            }
        } else if (vk == 0x2E || vk == 0x08) && selecting {
            s.shell.editor.selection_command("delete_selection");
        } else if vk == VK_TAB && selecting {
            s.shell.editor.selection_command(if shift { "outdent" } else { "indent" });
        } else if ctrl && vk == VK_RETURN {
            if selecting {
                s.shell.editor.selection_command("cycle_todo");
            } else {
                s.shell.editor.nav_command(Cmd::Toggle);
            }
        } else if let Some(nav) = nav {
            s.shell.editor.view.select(nav, page);
            if selecting && vertical {
                s.shell.editor.view.select_current();
            }
        } else if vk == VK_LEFT || vk == VK_RIGHT {
            s.shell.editor.view.set_fold(vk == VK_LEFT, page);
        } else if vk == VK_RETURN || vk == VK_F2 {
            s.shell.editor.view.clear_selection();
            s.shell.editor.start_edit_selected();
        } else if vk == VK_TAB {
            s.shell.editor.nav_command(if shift { Cmd::Outdent } else { Cmd::Indent });
        } else if vk == b'S' as u16 && ctrl {
            s.shell.editor.save();
        } else if ctrl && !alt && (vk == VK_Z || vk == VK_Y) {
            s.shell.editor.history_step(!(vk == VK_Y || shift));
        } else {
            return false;
        }
        ui_sync(hwnd, true);
        true
    }
}

unsafe fn clipboard_write(hwnd: HWND, text: &str) {
    unsafe {
        if OpenClipboard(hwnd) == 0 { return; }
        EmptyClipboard();
        let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let memory = GlobalAlloc(GMEM_MOVEABLE, utf16.len() * 2);
        if !memory.is_null() {
            let target = GlobalLock(memory) as *mut u16;
            if !target.is_null() {
                std::ptr::copy_nonoverlapping(utf16.as_ptr(), target, utf16.len());
                GlobalUnlock(memory);
                if SetClipboardData(CF_UNICODETEXT, memory).is_null() { GlobalFree(memory); }
            } else { GlobalFree(memory); }
        }
        CloseClipboard();
    }
}

unsafe fn clipboard_read(hwnd: HWND) -> Option<String> {
    unsafe {
        if OpenClipboard(hwnd) == 0 { return None; }
        let handle = GetClipboardData(CF_UNICODETEXT);
        let value = if handle.is_null() { None } else {
            let size = GlobalSize(handle) / 2;
            let ptr = GlobalLock(handle) as *const u16;
            if ptr.is_null() { None } else {
                let slice = std::slice::from_raw_parts(ptr, size);
                let len = slice.iter().position(|c| *c == 0).unwrap_or(size);
                let text = String::from_utf16_lossy(&slice[..len]);
                GlobalUnlock(handle);
                Some(text)
            }
        };
        CloseClipboard();
        value
    }
}

/// 마우스 클릭. 위쪽 단추·왼쪽 페이지 목록·검색 결과·오른쪽 역링크·가운데 블록 목록을 가려 처리한다.
unsafe fn click(hwnd: HWND, x: i32, y: i32, double: bool) {
    unsafe { click_modifiers(hwnd, x, y, double, shift_down()); }
}

unsafe fn click_modifiers(hwnd: HWND, x: i32, y: i32, double: bool, shift: bool) {
    unsafe {
        let Some(s) = state(hwnd) else { return };
        let lay = layout_of(hwnd, s);
        // 열려 있는 `[[` 자동완성 팝업의 후보를 누르면 그 후보로 바꾼다.
        if let Some(p) = s.shell.editor.popup.clone() {
            if let Some(er) = edit_rect_of(&s.shell, &lay) {
                let pr = layout::popup_rect(&lay, er, p.items.len());
                if pr.contains(x, y) {
                    let i = ((y - pr.t - scale(2, s.dpi)) / lay.popup_h).max(0) as usize;
                    if i < p.items.len() {
                        s.shell.editor.accept_popup(Some(i));
                    }
                    ui_sync(hwnd, true);
                    return;
                }
            }
        }
        if lay.panels {
            if lay.top.contains(x, y) {
                if let Some(b) = lay.button_at(x, y).filter(|_| s.shell.prompt.is_none()) {
                    do_button(hwnd, b);
                }
                return;
            }
            if s.shell.search_open && lay.center.contains(x, y) {
                if let Some(slot) = lay.result_slot_at(y) {
                    let i = s.shell.hit_first + slot;
                    if i < s.shell.hits.len() {
                        if shift {
                            let hit = s.shell.hits[i].clone();
                            let res = s.shell.open_card(&hit.page, hit.block);
                            report(s, res);
                            ui_sync(hwnd, true);
                        } else { open_hit(hwnd, i); }
                    }
                }
                return;
            }
            if let Some(l) = lay.left.filter(|l| l.contains(x, y)) {
                if let Some(section) = lay.section_at(x, y) {
                    s.shell.select_section(section);
                    if section == layout::LeftSection::Journals {
                        let res = s.shell.open_today();
                        report(s, res);
                    }
                    ui_sync(hwnd, true);
                    return;
                }
                if let Some(slot) = lay.slot_at(l, y) {
                    let items = s.shell.left_list();
                    if let Some(LeftItem::Page(name)) = items.get(s.shell.left_first + slot) {
                        let r = if shift { s.shell.open_card(&name.clone(), None) } else { s.shell.open_page(&name.clone()) };
                        report(s, r);
                        ui_sync(hwnd, true);
                    }
                }
                return;
            }
            if let Some(rt) = lay.right.filter(|r| r.contains(x, y)) {
                if let Some(slot) = lay.slot_at(rt, y) {
                    let items = s.shell.right_list();
                    if let Some(item) = items.get(s.shell.back_first + slot) {
                        let page = page_rows(hwnd, s);
                        let r = match item {
                            RightItem::Card(i) if x >= rt.r - scale(38, s.dpi) => { s.shell.close_card(*i); Ok(()) }
                            RightItem::Card(i) | RightItem::Preview(i) => s.shell.activate_card(*i, page),
                            RightItem::Block(i) if shift => {
                                let target = s.shell.back[*i].clone();
                                s.shell.open_card(&target.page, Some(target.block))
                            }
                            RightItem::Block(i) => s.shell.goto_backlink(*i, page),
                            RightItem::Group(_) => Ok(()),
                        };
                        report(s, r);
                        ui_sync(hwnd, true);
                    }
                }
                return;
            }
            if !lay.center.contains(x, y) {
                return;
            }
        }
        let (cx, cy) = (x - lay.center.l, y - lay.center.t);
        let m = Metrics::new(s.dpi);
        let page = m.page_rows(lay.center.h());
        let row = s.shell.editor.view.row_at(cy, m.header_h, m.row_h);
        if let Some(i) = row {
            let r = s.shell.editor.view.rows[i].clone();
            let block = r.block;
            if lay.panels && shift {
                let link = crate::paint::link_spans_x(&r.text, m.text_x(r.depth) + if r.status.is_some() { m.status_shift(s.dpi) } else { 0 }, s.dpi)
                    .into_iter().find(|(l, rr, _)| cx >= *l && cx < *rr);
                let target = if let Some((_, _, name)) = link { Some((name, None)) }
                    else if (cx - m.marker_x(r.depth)).abs() <= scale(9, s.dpi) { s.shell.page.clone().map(|p| (p, Some(block))) }
                    else { None };
                if let Some((name, block)) = target {
                    let res = s.shell.open_card(&name, block);
                    report(s, res);
                    ui_sync(hwnd, true);
                    return;
                }
            }
            // 점·접기 삼각형 클릭 = 접기/펼치기.
            if r.has_children && (cx - m.marker_x(r.depth)).abs() <= scale(9, s.dpi) {
                if s.shell.editor.finish_edit() {
                    s.shell.editor.view.set_selected(i, page);
                    s.shell.editor.view.set_fold(!r.folded, page);
                }
                ui_sync(hwnd, true);
                return;
            }
            let mut tx = m.text_x(r.depth);
            if r.status.is_some() {
                // 체크 칸 클릭 = TODO ↔ DONE.
                let b = scale(7, s.dpi);
                if cx >= tx - scale(2, s.dpi) && cx <= tx + 2 * b + scale(2, s.dpi) {
                    s.shell.editor.toggle_block(block);
                    if !s.shell.editor.editing() {
                        s.shell.editor.view.set_selected(i, page);
                    }
                    ui_sync(hwnd, true);
                    return;
                }
                tx += m.status_shift(s.dpi);
            }
            // `[[링크]]` 클릭 = 그 페이지로 이동(없으면 만든다).
            if s.shell.store.is_some() {
                if let Some((_, _, name)) = crate::paint::link_spans_x(&r.text, tx, s.dpi).into_iter().find(|(l, rr, _)| cx >= *l && cx < *rr) {
                    let res = s.shell.follow_link(&name);
                    report(s, res);
                    ui_sync(hwnd, true);
                    return;
                }
            }
            if s.shell.editor.editing() {
                s.shell.editor.submit(Cmd::Goto(block), now_ms(), false, false);
            } else {
                s.shell.editor.view.set_selected(i, page);
                if double {
                    s.shell.editor.start_edit_selected();
                }
            }
        }
        ui_sync(hwnd, true);
    }
}

/// 마우스 휠: 커서 아래 영역(왼쪽 목록·검색 결과·오른쪽 역링크·블록 목록)을 민다.
unsafe fn wheel(hwnd: HWND, delta: i32, x: i32, y: i32) {
    unsafe {
        let Some(s) = state(hwnd) else { return };
        let lay = layout_of(hwnd, s);
        let step = -(delta / 120) as isize * 3;
        let mv = |first: usize, total: usize, slots: usize| -> usize {
            ((first as isize + step).max(0) as usize).min(total.saturating_sub(slots))
        };
        if lay.panels && lay.left.is_some_and(|l| l.contains(x, y)) {
            let total = s.shell.left_list().len();
            s.shell.left_first = mv(s.shell.left_first, total, lay.left_slots());
        } else if lay.panels && lay.right.is_some_and(|r| r.contains(x, y)) {
            let total = s.shell.right_list().len();
            s.shell.back_first = mv(s.shell.back_first, total, lay.right_slots());
        } else if lay.panels && s.shell.search_open && lay.center.contains(x, y) {
            s.shell.hit_first = mv(s.shell.hit_first, s.shell.hits.len(), lay.result_slots());
        } else {
            let page = page_rows(hwnd, s);
            s.shell.editor.view.scroll_by(step, page);
        }
        ui_sync(hwnd, false);
    }
}

unsafe fn close_request(hwnd: HWND) -> bool {
    unsafe {
        let Some(s) = state(hwnd) else { return true };
        if !s.shell.editor.flush_for_close() {
            let msg = s.shell.editor.message.clone().unwrap_or_default();
            let ans = MessageBoxW(hwnd, wide(&format!("{msg}\n\n그래도 닫을까요?")).as_ptr(), wide("아웃라이너").as_ptr(), MB_YESNO | MB_ICONWARNING);
            return ans == IDYES;
        }
        if s.shell.editor.dirty() {
            let ans = MessageBoxW(
                hwnd,
                wide("저장하지 않은 변경이 있습니다. 저장할까요?").as_ptr(),
                wide("아웃라이너").as_ptr(),
                MB_YESNOCANCEL | MB_ICONQUESTION,
            );
            if ans == IDCANCEL {
                return false;
            }
            if ans == IDYES && !s.shell.editor.save() {
                let text = s.shell.editor.alert.take().unwrap_or_else(|| "저장하지 못했습니다".into());
                MessageBoxW(hwnd, wide(&text).as_ptr(), wide("아웃라이너 — 저장 경고").as_ptr(), MB_OK | MB_ICONWARNING);
                return false; // 저장 실패(충돌 등)면 닫지 않는다
            }
        }
        true
    }
}

// ---------------- EDIT 서브클래스 ----------------

unsafe extern "system" fn edit_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        let orig: WNDPROC = std::mem::transmute::<isize, WNDPROC>(ORIG_EDIT_PROC.load(Ordering::SeqCst));
        let parent = GetParent(hwnd);
        match msg {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let processkey = wp as u32 == VK_PROCESSKEY as u32;
                let vk = if processkey { ImmGetVirtualKey(hwnd) as u16 } else { wp as u16 };
                let ev = KeyEvent { vk, processkey, shift: shift_down(), ctrl: ctrl_down(), alt: alt_down() || msg == WM_SYSKEYDOWN, now_ms: now_ms() };
                if !processkey && comp_len(hwnd) == 0 && !COMPOSING.load(Ordering::SeqCst) && !CANDIDATE.load(Ordering::SeqCst)
                    && global_key(parent, vk, ev.ctrl, ev.alt, ev.shift, true) {
                    EAT_CONTROL_CHAR.store(true, Ordering::SeqCst);
                    return 0;
                }
                let result = match state(parent) {
                    Some(s) => s.shell.editor.key(ev),
                    None => KeyResult::Pass,
                };
                ui_sync(parent, true);
                EAT_CONTROL_CHAR.store(result == KeyResult::Swallow, Ordering::SeqCst);
                if result == KeyResult::Swallow {
                    return 0;
                }
                if ev.ctrl && !ev.alt && !processkey && vk == b'A' as u16 && !ev.shift {
                    SendMessageW(hwnd, EM_SETSEL, 0, -1);
                    EAT_CONTROL_CHAR.store(true, Ordering::SeqCst);
                    return 0;
                }
                if ev.ctrl && !ev.alt && ev.shift && !processkey && vk == b'V' as u16 {
                    if let (Some(s), Some(text)) = (state(parent), clipboard_read(parent)) {
                        s.shell.editor.paste_inline(&text);
                        ui_sync(parent, true);
                        EAT_CONTROL_CHAR.store(true, Ordering::SeqCst);
                        return 0;
                    }
                }
            }
            WM_PASTE => {
                if let (Some(s), Some(text)) = (state(parent), clipboard_read(parent)) {
                    if s.shell.editor.paste_text(&text, now_ms()) {
                        ui_sync(parent, true);
                        return 0;
                    }
                    // 한 줄: 줄 끝 문자를 떼고 그대로 넣는다.
                    let one = wide(text.trim_end_matches(['\r', '\n']));
                    SendMessageW(hwnd, EM_REPLACESEL, 1, one.as_ptr() as LPARAM);
                    return 0;
                }
            }
            // Tab·Enter·Esc 글자 메시지는 우리가 명령으로 처리했으므로 EDIT 에 넘기지 않는다(경고음·탭 문자 방지).
            WM_LBUTTONDOWN if ctrl_down() || shift_down() => {
                // Ctrl+클릭 = 클릭한 글자가 `[[링크]]` 안이면 그 페이지로(창에 우편으로 부탁 — EDIT 처리 도중 문서를 바꾸지 않는다).
                let pos = SendMessageW(hwnd, EM_CHARFROMPOS, 0, lp) as u32 & 0xFFFF;
                let mut buf = vec![0u16; GetWindowTextLengthW(hwnd).max(0) as usize + 2];
                let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32).max(0) as usize;
                let text = String::from_utf16_lossy(&buf[..n]);
                if let Some(name) = crate::links::link_at(&text, pos as usize) {
                    if let Some(s) = state(parent) {
                        s.pending_link = Some((name, shift_down()));
                        PostMessageW(parent, WM_APP_FOLLOW, 0, 0);
                        return 0;
                    }
                }
            }
            WM_CHAR if wp == 9 || wp == 13 || wp == 10 || wp == 27 || wp == 1 => return 0,
            WM_CHAR if wp < 32 && EAT_CONTROL_CHAR.load(Ordering::SeqCst) => return 0,
            WM_CHAR if wp < 128 && b"[](){}`".contains(&(wp as u8)) && !ctrl_down() => {
                if let Some(s) = state(parent) {
                    if s.shell.editor.typed_char(wp as u8 as char) {
                        ui_sync(parent, true);
                        return 0;
                    }
                }
            }
            WM_IME_STARTCOMPOSITION => COMPOSING.store(true, Ordering::SeqCst),
            WM_IME_ENDCOMPOSITION => {
                let r = CallWindowProcW(orig, hwnd, msg, wp, lp);
                COMPOSING.store(false, Ordering::SeqCst);
                // 결과 글자 처리가 끝난 뒤에 대기 명령을 실행하도록 자기 창에 우편으로 보낸다(설계 §3.5-3).
                PostMessageW(parent, WM_APP_RUN_PENDING, 0, 0);
                return r;
            }
            WM_IME_NOTIFY => match wp as u32 {
                IMN_OPENCANDIDATE => CANDIDATE.store(true, Ordering::SeqCst),
                IMN_CLOSECANDIDATE => CANDIDATE.store(false, Ordering::SeqCst),
                _ => {}
            },
            WM_KILLFOCUS => {
                CANDIDATE.store(false, Ordering::SeqCst);
            }
            _ => {}
        }
        CallWindowProcW(orig, hwnd, msg, wp, lp)
    }
}

unsafe fn create_edit(parent: HWND, hinst: HINSTANCE, font: HFONT, dpi: u32) -> HWND {
    unsafe {
        let class = wide("EDIT");
        let edit = CreateWindowExW(
            0,
            class.as_ptr(),
            null(),
            WS_CHILD | ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_AUTOHSCROLL as u32 | ES_LEFT as u32,
            0,
            0,
            10,
            10,
            parent,
            EDIT_ID as HMENU,
            hinst,
            null(),
        );
        SendMessageW(edit, WM_SETFONT, font as WPARAM, 0);
        let left = crate::paint::scale(4, dpi) as isize;
        SendMessageW(edit, EM_SETMARGINS, (EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize, (left | (left << 16)) as LPARAM);
        let prev = SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_proc as *const () as isize);
        ORIG_EDIT_PROC.store(prev, Ordering::SeqCst);
        edit
    }
}

unsafe extern "system" fn search_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        let orig: WNDPROC = std::mem::transmute::<isize, WNDPROC>(ORIG_SEARCH_PROC.load(Ordering::SeqCst));
        let parent = GetParent(hwnd);
        match msg {
            WM_KEYDOWN if wp as u32 != VK_PROCESSKEY as u32 && comp_len(hwnd) == 0 => {
                if global_key(parent, wp as u16, ctrl_down(), alt_down(), shift_down(), true) { return 0; }
                if search_key(parent, wp as u16) {
                    return 0;
                }
            }
            WM_CHAR if wp == 13 || wp == 27 || (wp < 32 && ctrl_down()) => return 0,
            _ => {}
        }
        CallWindowProcW(orig, hwnd, msg, wp, lp)
    }
}

unsafe fn create_search_edit(parent: HWND, hinst: HINSTANCE, font: HFONT, dpi: u32) -> HWND {
    unsafe {
        let edit = CreateWindowExW(0, wide("EDIT").as_ptr(), null(), WS_CHILD | ES_AUTOHSCROLL as u32 | ES_LEFT as u32, 0, 0, 10, 10, parent, SEARCH_ID as HMENU, hinst, null());
        SendMessageW(edit, WM_SETFONT, font as WPARAM, 0);
        let m = crate::paint::scale(4, dpi) as isize;
        SendMessageW(edit, EM_SETMARGINS, (EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize, (m | (m << 16)) as LPARAM);
        let prev = SetWindowLongPtrW(edit, GWLP_WNDPROC, search_proc as *const () as isize);
        ORIG_SEARCH_PROC.store(prev, Ordering::SeqCst);
        edit
    }
}

// ---------------- 부모 창 ----------------

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_CREATE => {
                let cs = lp as *const CREATESTRUCTW;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*cs).lpCreateParams as isize);
                0
            }
            WM_PAINT => {
                paint(hwnd);
                0
            }
            WM_ERASEBKGND => 1,
            WM_CTLCOLOREDIT => {
                let Some(s) = state(hwnd) else { return DefWindowProcW(hwnd, msg, wp, lp) };
                let pal = palette(s);
                SetTextColor(wp as HDC, pal.text);
                SetBkColor(wp as HDC, pal.bg);
                if s.edit_brush.is_null() || s.edit_brush_color != pal.bg {
                    if !s.edit_brush.is_null() {
                        DeleteObject(s.edit_brush);
                    }
                    s.edit_brush = CreateSolidBrush(pal.bg);
                    s.edit_brush_color = pal.bg;
                }
                s.edit_brush as LRESULT
            }
            WM_SIZE => {
                if let Some(s) = state(hwnd) {
                    let page = page_rows(hwnd, s);
                    let first = s.shell.editor.view.first;
                    s.shell.editor.view.set_first(first, page);
                }
                ui_sync(hwnd, false);
                0
            }
            WM_DPICHANGED => {
                if let Some(s) = state(hwnd) {
                    s.dpi = (wp & 0xFFFF) as u32;
                    let old = s.font;
                    s.font = create_font(s.dpi);
                    SendMessageW(s.shell.editor.host.edit, WM_SETFONT, s.font as WPARAM, 1);
                    if !s.search_edit.is_null() {
                        SendMessageW(s.search_edit, WM_SETFONT, s.font as WPARAM, 1);
                    }
                    DeleteObject(old);
                }
                let r = lp as *const RECT;
                SetWindowPos(hwnd, null_mut(), (*r).left, (*r).top, (*r).right - (*r).left, (*r).bottom - (*r).top, SWP_NOZORDER | SWP_NOACTIVATE);
                ui_sync(hwnd, false);
                0
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if key_down(hwnd, wp as u16) {
                    0
                } else {
                    DefWindowProcW(hwnd, msg, wp, lp)
                }
            }
            WM_COMMAND => {
                if ((wp >> 16) & 0xFFFF) as u32 == EN_CHANGE_CODE {
                    if (wp & 0xFFFF) as isize == SEARCH_ID {
                        search_changed(hwnd);
                    } else {
                        ui_sync(hwnd, false);
                    }
                }
                0
            }
            WM_APP_FOLLOW => {
                if let Some(s) = state(hwnd) {
                    if let Some((name, sidebar)) = s.pending_link.take() {
                        let r = if sidebar { s.shell.open_card(&name, None) } else { s.shell.follow_link(&name) };
                        report(s, r);
                    }
                }
                ui_sync(hwnd, true);
                0
            }
            WM_SETFOCUS => {
                // Alt+Tab 으로 돌아왔을 때 편집 중이면 EDIT 로 포커스를 돌려 준다.
                if let Some(s) = state(hwnd) {
                    if s.shell.editor.editing() && IsWindowVisible(s.shell.editor.host.edit) != 0 {
                        SetFocus(s.shell.editor.host.edit);
                    }
                }
                0
            }
            WM_APP_RUN_PENDING => {
                if let Some(s) = state(hwnd) {
                    s.shell.editor.run_pending(now_ms());
                }
                ui_sync(hwnd, true);
                0
            }
            WM_TIMER => {
                if wp == TIMER_ID {
                    if let Some(s) = state(hwnd) {
                        if s.shell.editor.waiting() {
                            s.shell.editor.run_pending(now_ms());
                            ui_sync(hwnd, true);
                        } else {
                            ui_sync(hwnd, false); // 제목 줄 ●·안내 갱신(EDIT 에 입력만 한 경우)
                        }
                    }
                }
                0
            }
            WM_MOUSEWHEEL => {
                let delta = ((wp >> 16) as u16 as i16) as i32;
                let mut pt = POINT { x: (lp & 0xFFFF) as u16 as i16 as i32, y: ((lp >> 16) & 0xFFFF) as u16 as i16 as i32 };
                ScreenToClient(hwnd, &mut pt);
                wheel(hwnd, delta, pt.x, pt.y);
                0
            }
            WM_VSCROLL => {
                if let Some(s) = state(hwnd) {
                    let page = page_rows(hwnd, s);
                    let first = s.shell.editor.view.first as isize;
                    let target = match (wp & 0xFFFF) as i32 {
                        x if x == SB_LINEUP => first - 1,
                        x if x == SB_LINEDOWN => first + 1,
                        x if x == SB_PAGEUP => first - page as isize,
                        x if x == SB_PAGEDOWN => first + page as isize,
                        x if x == SB_THUMBTRACK || x == SB_THUMBPOSITION => ((wp >> 16) & 0xFFFF) as isize,
                        x if x == SB_TOP => 0,
                        x if x == SB_BOTTOM => isize::MAX / 2,
                        _ => first,
                    };
                    s.shell.editor.view.set_first(target.max(0) as usize, page);
                }
                ui_sync(hwnd, false);
                0
            }
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
                SetFocus(hwnd);
                let x = (lp & 0xFFFF) as u16 as i16 as i32;
                let y = ((lp >> 16) & 0xFFFF) as u16 as i16 as i32;
                click(hwnd, x, y, msg == WM_LBUTTONDBLCLK);
                0
            }
            WM_GETDLGCODE => (DLGC_WANTARROWS | DLGC_WANTCHARS | DLGC_WANTALLKEYS) as isize,
            WM_CHAR => 0,
            WM_CLOSE => {
                if close_request(hwnd) {
                    DestroyWindow(hwnd);
                }
                0
            }
            WM_DESTROY => {
                KillTimer(hwnd, TIMER_ID);
                PostQuitMessage(0);
                0
            }
            _ => {
                if ui_theme::is_theme_message(msg, wp, lp) {
                    if let Some(s) = state(hwnd) {
                        ui_theme::refresh(s.pref);
                        let (kind, _) = ui_theme::current(s.pref);
                        ui_theme::apply_window(hwnd, kind);
                    }
                    InvalidateRect(hwnd, null(), 0);
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
        }
    }
}

unsafe fn register(hinst: HINSTANCE) {
    unsafe {
        let class = wide("OutlinerWinMain");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
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
        RegisterClassW(&wc); // 두 번째부터는 실패해도 이미 등록된 것
    }
}

unsafe fn make_state(mut shell: Shell<WinHost>, title: &str, dpi: u32, force_kind: Option<Kind>) -> *mut State {
    unsafe {
        shell.editor.host = WinHost::new();
        Box::into_raw(Box::new(State {
            shell,
            search_edit: null_mut(),
            quiet: false,
            pending_link: None,
            title: title.to_string(),
            dpi,
            pref: ThemePref::System,
            force_kind,
            font: create_font(dpi),
            edit_brush: null_mut(),
            edit_brush_color: 0,
            last_title: String::new(),
        }))
    }
}

unsafe fn free_state(p: *mut State) {
    unsafe {
        let s = Box::from_raw(p);
        DeleteObject(s.font);
        if !s.edit_brush.is_null() {
            DeleteObject(s.edit_brush);
        }
    }
}

/// 페이지 모드 단일 인스턴스: 이미 떠 있는 창이 있으면 그 창을 앞으로 가져오고 true(이 프로세스는 곧 끝낸다).
/// 뮤텍스 이름 뒤에 환경변수 `OUTLINER_WIN_INSTANCE` 를 붙일 수 있다(시험이 실제 사용자 창과 섞이지 않게).
pub fn activate_existing_instance() -> bool {
    use windows_sys::Win32::System::Threading::CreateMutexW;
    unsafe {
        let suffix = std::env::var("OUTLINER_WIN_INSTANCE").unwrap_or_default();
        let name = format!(r"Local\HwpAutoDocFit.OutlinerWin.Singleton{suffix}");
        // 핸들은 일부러 닫지 않는다: 프로세스가 끝날 때까지 뮤텍스를 쥐고 있어야 한다.
        let handle = CreateMutexW(null(), 0, wide(&name).as_ptr());
        if handle.is_null() || GetLastError() != ERROR_ALREADY_EXISTS {
            return false;
        }
        let existing = FindWindowW(wide("OutlinerWinMain").as_ptr(), null());
        if !existing.is_null() {
            if IsIconic(existing) != 0 {
                ShowWindow(existing, SW_RESTORE);
            }
            SetForegroundWindow(existing);
        }
        true
    }
}

/// 창을 띄우고 닫힐 때까지 메시지 루프를 돈다.
pub fn run(editor: Editor<WinHost>, title: &str) -> Result<(), String> {
    run_shell(Shell::single(editor), title)
}

/// 페이지 모드(데이터 폴더) 또는 한 파일 모드의 창.
pub fn run_shell(shell: Shell<WinHost>, title: &str) -> Result<(), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let hinst = GetModuleHandleW(null());
        register(hinst);
        let pref = ThemePref::System;
        ui_theme::refresh(pref);
        let boxed = make_state(shell, title, 96, None);
        let hwnd = CreateWindowExW(
            0,
            wide("OutlinerWinMain").as_ptr(),
            wide(&format!("아웃라이너 — {title}")).as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VSCROLL,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            820,
            620,
            null_mut(),
            null_mut(),
            hinst,
            boxed.cast(),
        );
        if hwnd.is_null() {
            free_state(boxed);
            return Err("창을 만들지 못했습니다".into());
        }
        let dpi = GetDpiForWindow(hwnd).max(96);
        (*boxed).dpi = dpi;
        let old = (*boxed).font;
        (*boxed).font = create_font(dpi);
        DeleteObject(old);
        (*boxed).shell.editor.host.edit = create_edit(hwnd, hinst, (*boxed).font, dpi);
        if (*boxed).shell.panels() {
            (*boxed).search_edit = create_search_edit(hwnd, hinst, (*boxed).font, dpi);
        }
        let (kind, _) = ui_theme::current(pref);
        ui_theme::apply_window(hwnd, kind);
        ShowWindow(hwnd, SW_SHOW);
        SetTimer(hwnd, TIMER_ID, TIMER_MS, None);
        ui_sync(hwnd, false);
        SetFocus(hwnd);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        free_state(boxed);
        Ok(())
    }
}

// ---------------- 캡처·시험용 하니스 ----------------

/// 자식 EDIT 를 `WM_PRINT` 로 그려 목록 그림(`mem`) 위 그 자리에 찍는다.
unsafe fn print_child(parent: HWND, edit: HWND, mem: HDC, screen: HDC) {
    unsafe {
        let mut er: RECT = std::mem::zeroed();
        GetWindowRect(edit, &mut er);
        let mut origin = POINT { x: er.left, y: er.top };
        ScreenToClient(parent, &mut origin);
        let (ew, eh) = (er.right - er.left, er.bottom - er.top);
        let ebm = CreateCompatibleBitmap(screen, ew, eh);
        let edc = CreateCompatibleDC(screen);
        let eold = SelectObject(edc, ebm);
        let fb = CreateSolidBrush(0x00FF00FF);
        FillRect(edc, &RECT { left: 0, top: 0, right: ew, bottom: eh }, fb);
        DeleteObject(fb);
        SendMessageW(edit, WM_PRINT, edc as WPARAM, (PRF_CLIENT | PRF_NONCLIENT | PRF_ERASEBKGND) as LPARAM);
        BitBlt(mem, origin.x, origin.y, ew, eh, edc, 0, 0, SRCCOPY);
        SelectObject(edc, eold);
        DeleteObject(ebm);
        DeleteDC(edc);
    }
}

/// 화면에 보이지 않는 실제 창(+실제 EDIT)을 만들어 편집 상태를 그림으로 뽑는다.
/// 편집기에 실제 `WinHost` 가 붙어 있으므로 EDIT 의 글·캐럿·서브클래스가 실제와 같다.
pub struct Harness {
    hwnd: HWND,
    state: *mut State,
    pub width: i32,
    pub height: i32,
}

impl Harness {
    pub fn new(editor: Editor<WinHost>, title: &str, kind: Kind, dpi: u32, width: i32, height: i32) -> Result<Harness, String> {
        Harness::new_shell(Shell::single(editor), title, kind, dpi, width, height)
    }

    pub fn new_shell(shell: Shell<WinHost>, title: &str, kind: Kind, dpi: u32, width: i32, height: i32) -> Result<Harness, String> {
        unsafe {
            let hinst = GetModuleHandleW(null());
            register(hinst);
            let state = make_state(shell, title, dpi, Some(kind));
            // 화면 밖 팝업(보이지 않음)이라 사용자 화면·실행 중 앱에 영향이 없다. 클라이언트 영역 = width×height.
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                wide("OutlinerWinMain").as_ptr(),
                wide(title).as_ptr(),
                WS_POPUP,
                -32000,
                -32000,
                width,
                height,
                null_mut(),
                null_mut(),
                hinst,
                state.cast(),
            );
            if hwnd.is_null() {
                free_state(state);
                return Err("캡처용 창을 만들지 못했습니다".into());
            }
            (*state).shell.editor.host.edit = create_edit(hwnd, hinst, (*state).font, dpi);
            if (*state).shell.panels() {
                (*state).search_edit = create_search_edit(hwnd, hinst, (*state).font, dpi);
            }
            // 보이는 상태여야 EDIT 가 WM_PRINT 로 그려진다. 화면 밖(-32000)·작업 표시줄 단추 없음·활성화 안 함.
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            Ok(Harness { hwnd, state, width, height })
        }
    }

    pub fn editor(&mut self) -> &mut Editor<WinHost> {
        unsafe { &mut (*self.state).shell.editor }
    }

    pub fn shell(&mut self) -> &mut Shell<WinHost> {
        unsafe { &mut (*self.state).shell }
    }

    /// 위쪽 검색 칸에 글을 넣는다(`EN_CHANGE` 와 같은 경로로 검색이 돈다) — 시험·캡처용.
    pub fn type_search(&mut self, text: &str) {
        unsafe {
            let s = &mut *self.state;
            SetWindowTextW(s.search_edit, wide(text).as_ptr());
            search_changed(self.hwnd);
        }
    }

    /// 검색 칸·이름 입력 칸의 현재 글.
    pub fn search_text(&mut self) -> String {
        unsafe { read_edit((*self.state).search_edit) }
    }

    /// 창 안 한 점을 눌렀을 때와 같은 처리(시험용).
    pub fn click_at(&mut self, x: i32, y: i32, double: bool) {
        unsafe { click(self.hwnd, x, y, double) }
    }

    pub fn shift_click_at(&mut self, x: i32, y: i32) {
        unsafe { click_modifiers(self.hwnd, x, y, false, true) }
    }

    pub fn layout_now(&mut self) -> Layout {
        unsafe { layout_of(self.hwnd, &*self.state) }
    }

    /// 위쪽 단추를 누른다(시험용; 삭제는 대화상자를 띄우므로 쓰지 않는다).
    pub fn press(&mut self, b: Btn) {
        unsafe { do_button(self.hwnd, b) }
    }

    /// 입력 줄 확정(Enter 와 같음).
    pub fn confirm_prompt(&mut self) {
        unsafe { finish_prompt(self.hwnd) }
    }

    /// 부모 창(목록)이 받은 키.
    pub fn list_key(&mut self, vk: u16) -> bool {
        unsafe { key_down(self.hwnd, vk) }
    }

    /// 가짜 키보드 수정자를 지정해 목록 단축키를 시험한다.
    pub fn list_key_modifiers(&mut self, vk: u16, ctrl: bool, alt: bool, shift: bool) -> bool {
        unsafe { key_down_modifiers(self.hwnd, vk, ctrl, alt, shift) }
    }

    /// 편집 명령 뒤 화면 배치(EDIT 위치)를 맞춘다.
    pub fn layout(&mut self) {
        unsafe {
            ui_sync(self.hwnd, true);
        }
    }

    /// 현재 상태를 BGRA 화소로. 목록을 그린 뒤 EDIT 를 `WM_PRINT` 로 그 자리에 찍고 캐럿 선을 얹는다.
    pub fn render(&mut self) -> Result<Shot, String> {
        unsafe {
            let (w, h) = (self.width, self.height);
            let s = &mut *self.state;
            let extras = extras_of(s);
            let pal = palette(s);
            let lay = layout::compute_visible(w, h, s.dpi, s.shell.panels(), s.shell.left_open, s.shell.right_open);
            let screen = GetDC(null_mut());
            let mem = CreateCompatibleDC(screen);
            let bm = CreateCompatibleBitmap(screen, w, h);
            let old = SelectObject(mem, bm);
            draw_shell(mem, w, h, &s.shell, &pal, s.dpi, &extras, &lay);
            if !s.search_edit.is_null() && IsWindowVisible(s.search_edit) != 0 {
                print_child(self.hwnd, s.search_edit, mem, screen);
            }
            let edit = s.shell.editor.host.edit;
            if extras.editing_row.is_some() {
                let mut er: RECT = std::mem::zeroed();
                GetWindowRect(edit, &mut er);
                let mut origin = POINT { x: er.left, y: er.top };
                ScreenToClient(self.hwnd, &mut origin);
                let (ew, eh) = (er.right - er.left, er.bottom - er.top);
                // EDIT 자체 그림: 자기 크기의 비트맵에 그려 목록 그림에 복사한다.
                let ebm = CreateCompatibleBitmap(screen, ew, eh);
                let edc = CreateCompatibleDC(screen);
                let eold = SelectObject(edc, ebm);
                let fb = CreateSolidBrush(0x00FF00FF); // 마젠타: EDIT 가 그리지 않으면 눈에 띄게 남는다
                FillRect(edc, &RECT { left: 0, top: 0, right: ew, bottom: eh }, fb);
                DeleteObject(fb);
                SendMessageW(edit, WM_PRINT, edc as WPARAM, (PRF_CLIENT | PRF_NONCLIENT | PRF_ERASEBKGND) as LPARAM);
                BitBlt(mem, origin.x, origin.y, ew, eh, edc, 0, 0, SRCCOPY);

                // 캐럿(시험용 그림에는 실제 캐럿이 안 찍히므로 같은 위치에 선을 긋는다).
                let mut start = 0u32;
                SendMessageW(edit, EM_GETSEL, &mut start as *mut u32 as WPARAM, 0);
                let pos = SendMessageW(edit, EM_POSFROMCHAR, start as WPARAM, 0) as u32;
                let cx = origin.x + (pos & 0xFFFF) as i16 as i32;
                let pen = CreatePen(PS_SOLID, crate::paint::scale(2, s.dpi), pal.text);
                let op = SelectObject(mem, pen);
                MoveToEx(mem, cx, origin.y + crate::paint::scale(3, s.dpi), null_mut());
                LineTo(mem, cx, origin.y + eh - crate::paint::scale(3, s.dpi));
                SelectObject(mem, op);
                DeleteObject(pen);
                SelectObject(edc, eold);
                DeleteObject(ebm);
                DeleteDC(edc);
            }
            SelectObject(mem, old);
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = w;
            info.bmiHeader.biHeight = -h;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            let mut pixels = vec![0u8; (w * h * 4) as usize];
            let got = GetDIBits(screen, bm, 0, h as u32, pixels.as_mut_ptr().cast(), &mut info, DIB_RGB_COLORS);
            DeleteObject(bm);
            DeleteDC(mem);
            ReleaseDC(null_mut(), screen);
            if got == 0 {
                return Err("캡처 실패(GetDIBits)".into());
            }
            Ok(Shot { width: w as usize, height: h as usize, bgra: pixels })
        }
    }

    /// EDIT 에 문자 메시지를 보낸다(시험용).
    pub fn send_char(&mut self, ch: u16) {
        unsafe {
            let edit = self.state.as_ref().map(|s| s.shell.editor.host.edit).unwrap_or(null_mut());
            SendMessageW(edit, WM_CHAR, ch as WPARAM, 0);
        }
    }

    /// EDIT 에 키를 직접 보낸다(실제 서브클래스 프로시저를 거친다) — 시험용.
    pub fn send_key(&mut self, vk: u16) {
        unsafe {
            let edit = self.state.as_ref().map(|s| s.shell.editor.host.edit).unwrap_or(null_mut());
            SendMessageW(edit, WM_KEYDOWN, vk as WPARAM, 0);
        }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        unsafe {
            // WM_DESTROY 가 PostQuitMessage 를 부르지만 이 스레드의 메시지 루프는 없으므로 무해하다.
            DestroyWindow(self.hwnd);
            free_state(self.state);
        }
    }
}
