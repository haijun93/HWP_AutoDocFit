//! 블록 목록 창 + 편집 중 블록 위에만 겹치는 Win32 EDIT(한 줄, `ES_AUTOHSCROLL`).
//! 키·IME 처리는 `edit::Editor`(순수 상태 기계)에 맡기고, 여기서는 메시지를 `KeyEvent` 로 바꾸고
//! 결과(EDIT 위치·제목 줄 ●·안내 문구)를 화면에 반영만 한다.
use crate::capture::{Shot, palette_of};
use crate::edit::{Cmd, EditHost, Editor, KeyEvent, KeyResult};
use crate::paint::{Extras, Metrics, create_font, draw_ext, wide};
use crate::view::{Nav, ensure_visible, row_at};
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
const TIMER_ID: usize = 1;
const TIMER_MS: u32 = 100;
const EDIT_ID: isize = 101;
// windows-sys 0.59 에는 EDIT 메시지 상수가 없어 직접 둔다(winuser.h 값).
const EM_GETSEL: u32 = 0x00B0;
const EM_SETSEL: u32 = 0x00B1;
const EM_SETMARGINS: u32 = 0x00D3;
const EM_POSFROMCHAR: u32 = 0x00D6;
const EM_SCROLLCARET: u32 = 0x00B7;

/// EDIT 하나뿐이라 IME 상태는 전역(서브클래스 프로시저가 `State` 를 잡지 않고도 갱신해야 해서).
static COMPOSING: AtomicBool = AtomicBool::new(false);
static CANDIDATE: AtomicBool = AtomicBool::new(false);
static ORIG_EDIT_PROC: AtomicIsize = AtomicIsize::new(0);

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
        unsafe {
            let n = GetWindowTextLengthW(self.edit).max(0) as usize;
            let mut buf = vec![0u16; n + 2];
            let got = GetWindowTextW(self.edit, buf.as_mut_ptr(), buf.len() as i32);
            String::from_utf16_lossy(&buf[..got.max(0) as usize])
        }
    }
    fn selection(&mut self) -> (usize, usize) {
        unsafe {
            let (mut s, mut e) = (0u32, 0u32);
            SendMessageW(self.edit, EM_GETSEL, &mut s as *mut u32 as WPARAM, &mut e as *mut u32 as LPARAM);
            (s as usize, e as usize)
        }
    }
    fn set_text(&mut self, text: &str, caret: usize) {
        unsafe {
            COMPOSING.store(false, Ordering::SeqCst);
            SetWindowTextW(self.edit, wide(text).as_ptr());
            SendMessageW(self.edit, EM_SETSEL, caret, caret as LPARAM);
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
    pub editor: Editor<WinHost>,
    title: String,
    dpi: u32,
    pref: ThemePref,
    /// 캡처 시험에서 팔레트를 고정한다(없으면 시스템 설정을 따른다).
    force_kind: Option<Kind>,
    font: HFONT,
    edit_brush: HBRUSH,
    edit_brush_color: u32,
    last_title: String,
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

unsafe fn sync_scroll(hwnd: HWND, s: &State) {
    unsafe {
        let m = Metrics::new(s.dpi);
        let page = m.page_rows(client(hwnd).bottom);
        let info = SCROLLINFO {
            cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
            fMask: SIF_ALL,
            nMin: 0,
            nMax: s.editor.view.rows.len().max(1) as i32 - 1,
            nPage: page as u32,
            nPos: s.editor.view.first as i32,
            nTrackPos: 0,
        };
        SetScrollInfo(hwnd, SB_VERT, &info, 1);
    }
}

unsafe fn page_rows(hwnd: HWND, s: &State) -> usize {
    unsafe { Metrics::new(s.dpi).page_rows(client(hwnd).bottom) }
}

fn extras_of(s: &mut State) -> Extras {
    let editing_row = s.editor.editing_block().and_then(|b| s.editor.view.rows.iter().position(|r| r.block == b));
    let dirty = s.editor.dirty();
    Extras { editing_row, message: s.editor.message.clone(), dirty }
}

/// 편집 상태·스크롤·제목을 화면에 맞춘다. `ensure`: 편집 중인 줄이 보이도록 스크롤(명령 직후에만).
unsafe fn ui_sync(hwnd: HWND, ensure: bool) {
    unsafe {
        let Some(s) = state(hwnd) else { return };
        let r = client(hwnd);
        let m = Metrics::new(s.dpi);
        let page = m.page_rows(r.bottom);
        let edit = s.editor.host.edit;
        let editing_pos = s.editor.editing_block().and_then(|b| s.editor.view.rows.iter().position(|r| r.block == b));
        match editing_pos {
            Some(pos) => {
                if ensure {
                    let total = s.editor.view.rows.len();
                    s.editor.view.first = ensure_visible(s.editor.view.first, pos, total, page);
                }
                let first = s.editor.view.first;
                let row = &s.editor.view.rows[pos];
                let in_view = pos >= first && pos <= first + page;
                if in_view {
                    let slot = pos - first;
                    let er = m.edit_rect(s.dpi, row.depth, row.status.is_some(), slot, r.right);
                    let mut old: RECT = std::mem::zeroed();
                    GetWindowRect(edit, &mut old);
                    let was_visible = IsWindowVisible(edit) != 0;
                    let (nw, nh) = (er.right - er.left, er.bottom - er.top);
                    let resized = old.right - old.left != nw || old.bottom - old.top != nh;
                    MoveWindow(edit, er.left, er.top, nw, nh, 1);
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
                    if GetFocus() != edit && !GetParent(edit).is_null() && IsWindowVisible(hwnd) != 0 {
                        SetFocus(edit);
                    }
                } else {
                    ShowWindow(edit, SW_HIDE);
                }
            }
            None => {
                if IsWindowVisible(edit) != 0 {
                    ShowWindow(edit, SW_HIDE);
                    if IsWindowVisible(hwnd) != 0 {
                        SetFocus(hwnd);
                    }
                }
            }
        }
        sync_scroll(hwnd, s);
        let dirty = s.editor.dirty();
        let title = format!("{}아웃라이너 — {}", if dirty { "● " } else { "" }, s.title);
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
        let alert = state(hwnd).and_then(|s| s.editor.alert.take());
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
            let extras = extras_of(s);
            let mem = CreateCompatibleDC(dc);
            let bm = CreateCompatibleBitmap(dc, r.right.max(1), r.bottom.max(1));
            let old = SelectObject(mem, bm);
            let pal = palette(s);
            draw_ext(mem, r.right, r.bottom, &s.editor.view, &pal, s.dpi, &s.title, &extras);
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

/// 목록(편집 아님) 상태의 키.
unsafe fn key_down(hwnd: HWND, vk: u16) -> bool {
    unsafe {
        let Some(s) = state(hwnd) else { return false };
        if s.editor.editing() {
            return false;
        }
        let page = page_rows(hwnd, s);
        let nav = match vk {
            VK_UP => Some(Nav::Up),
            VK_DOWN => Some(Nav::Down),
            VK_PRIOR => Some(Nav::PageUp),
            VK_NEXT => Some(Nav::PageDown),
            VK_HOME => Some(Nav::Home),
            VK_END => Some(Nav::End),
            _ => None,
        };
        if let Some(nav) = nav {
            s.editor.view.select(nav, page);
        } else if vk == VK_LEFT || vk == VK_RIGHT {
            s.editor.view.set_fold(vk == VK_LEFT, page);
        } else if vk == VK_RETURN || vk == VK_F2 {
            s.editor.start_edit_selected();
        } else if vk == VK_TAB {
            s.editor.nav_command(if shift_down() { Cmd::Outdent } else { Cmd::Indent });
        } else if vk == b'S' as u16 && ctrl_down() {
            s.editor.save();
        } else {
            return false;
        }
        ui_sync(hwnd, true);
        true
    }
}

/// 마우스 클릭(목록 영역). 편집 중이면 확정 후 그 블록 편집으로 옮긴다.
unsafe fn click(hwnd: HWND, y: i32, double: bool) {
    unsafe {
        let Some(s) = state(hwnd) else { return };
        let m = Metrics::new(s.dpi);
        let row = row_at(y, m.header_h, m.row_h, s.editor.view.first, s.editor.view.rows.len());
        let page = page_rows(hwnd, s);
        if let Some(i) = row {
            let block = s.editor.view.rows[i].block;
            if s.editor.editing() {
                s.editor.submit(Cmd::Goto(block), now_ms(), false, false);
            } else {
                s.editor.view.set_selected(i, page);
                if double {
                    s.editor.start_edit_selected();
                }
            }
        }
        ui_sync(hwnd, true);
    }
}

unsafe fn close_request(hwnd: HWND) -> bool {
    unsafe {
        let Some(s) = state(hwnd) else { return true };
        if !s.editor.flush_for_close() {
            let msg = s.editor.message.clone().unwrap_or_default();
            let ans = MessageBoxW(hwnd, wide(&format!("{msg}\n\n그래도 닫을까요?")).as_ptr(), wide("아웃라이너").as_ptr(), MB_YESNO | MB_ICONWARNING);
            return ans == IDYES;
        }
        if s.editor.dirty() {
            let ans = MessageBoxW(
                hwnd,
                wide("저장하지 않은 변경이 있습니다. 저장할까요?").as_ptr(),
                wide("아웃라이너").as_ptr(),
                MB_YESNOCANCEL | MB_ICONQUESTION,
            );
            if ans == IDCANCEL {
                return false;
            }
            if ans == IDYES && !s.editor.save() {
                let text = s.editor.alert.take().unwrap_or_else(|| "저장하지 못했습니다".into());
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
                let result = match state(parent) {
                    Some(s) => s.editor.key(ev),
                    None => KeyResult::Pass,
                };
                ui_sync(parent, true);
                if result == KeyResult::Swallow {
                    return 0;
                }
            }
            // Tab·Enter·Esc 글자 메시지는 우리가 명령으로 처리했으므로 EDIT 에 넘기지 않는다(경고음·탭 문자 방지).
            WM_CHAR if wp == 9 || wp == 13 || wp == 10 || wp == 27 => return 0,
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
            WS_CHILD | ES_AUTOHSCROLL as u32 | ES_LEFT as u32,
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
                    let first = s.editor.view.first;
                    s.editor.view.set_first(first, page);
                }
                ui_sync(hwnd, false);
                0
            }
            WM_DPICHANGED => {
                if let Some(s) = state(hwnd) {
                    s.dpi = (wp & 0xFFFF) as u32;
                    let old = s.font;
                    s.font = create_font(s.dpi);
                    SendMessageW(s.editor.host.edit, WM_SETFONT, s.font as WPARAM, 1);
                    DeleteObject(old);
                }
                let r = lp as *const RECT;
                SetWindowPos(hwnd, null_mut(), (*r).left, (*r).top, (*r).right - (*r).left, (*r).bottom - (*r).top, SWP_NOZORDER | SWP_NOACTIVATE);
                ui_sync(hwnd, false);
                0
            }
            WM_KEYDOWN => {
                if key_down(hwnd, wp as u16) {
                    0
                } else {
                    DefWindowProcW(hwnd, msg, wp, lp)
                }
            }
            WM_SETFOCUS => {
                // Alt+Tab 으로 돌아왔을 때 편집 중이면 EDIT 로 포커스를 돌려 준다.
                if let Some(s) = state(hwnd) {
                    if s.editor.editing() && IsWindowVisible(s.editor.host.edit) != 0 {
                        SetFocus(s.editor.host.edit);
                    }
                }
                0
            }
            WM_APP_RUN_PENDING => {
                if let Some(s) = state(hwnd) {
                    s.editor.run_pending(now_ms());
                }
                ui_sync(hwnd, true);
                0
            }
            WM_TIMER => {
                if wp == TIMER_ID {
                    if let Some(s) = state(hwnd) {
                        if s.editor.waiting() {
                            s.editor.run_pending(now_ms());
                            ui_sync(hwnd, true);
                        } else {
                            ui_sync(hwnd, false); // 제목 줄 ●·안내 갱신(EDIT 에 입력만 한 경우)
                        }
                    }
                }
                0
            }
            WM_MOUSEWHEEL => {
                if let Some(s) = state(hwnd) {
                    let delta = ((wp >> 16) as u16 as i16) as i32;
                    let page = page_rows(hwnd, s);
                    s.editor.view.scroll_by(-(delta / 120) as isize * 3, page);
                }
                ui_sync(hwnd, false);
                0
            }
            WM_VSCROLL => {
                if let Some(s) = state(hwnd) {
                    let page = page_rows(hwnd, s);
                    let first = s.editor.view.first as isize;
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
                    s.editor.view.set_first(target.max(0) as usize, page);
                }
                ui_sync(hwnd, false);
                0
            }
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
                SetFocus(hwnd);
                let y = ((lp >> 16) & 0xFFFF) as u16 as i16 as i32;
                click(hwnd, y, msg == WM_LBUTTONDBLCLK);
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

unsafe fn make_state(mut editor: Editor<WinHost>, title: &str, dpi: u32, force_kind: Option<Kind>) -> *mut State {
    unsafe {
        editor.host = WinHost::new();
        Box::into_raw(Box::new(State {
            editor,
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

/// 창을 띄우고 닫힐 때까지 메시지 루프를 돈다.
pub fn run(editor: Editor<WinHost>, title: &str) -> Result<(), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let hinst = GetModuleHandleW(null());
        register(hinst);
        let pref = ThemePref::System;
        ui_theme::refresh(pref);
        let boxed = make_state(editor, title, 96, None);
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
        (*boxed).editor.host.edit = create_edit(hwnd, hinst, (*boxed).font, dpi);
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
        unsafe {
            let hinst = GetModuleHandleW(null());
            register(hinst);
            let state = make_state(editor, title, dpi, Some(kind));
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
            (*state).editor.host.edit = create_edit(hwnd, hinst, (*state).font, dpi);
            // 보이는 상태여야 EDIT 가 WM_PRINT 로 그려진다. 화면 밖(-32000)·작업 표시줄 단추 없음·활성화 안 함.
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            Ok(Harness { hwnd, state, width, height })
        }
    }

    pub fn editor(&mut self) -> &mut Editor<WinHost> {
        unsafe { &mut (*self.state).editor }
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
            let screen = GetDC(null_mut());
            let mem = CreateCompatibleDC(screen);
            let bm = CreateCompatibleBitmap(screen, w, h);
            let old = SelectObject(mem, bm);
            draw_ext(mem, w, h, &s.editor.view, &pal, s.dpi, &s.title, &extras);
            let edit = s.editor.host.edit;
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
                let (start, _) = s.editor.host.selection();
                let pos = SendMessageW(edit, EM_POSFROMCHAR, start, 0) as u32;
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

    /// EDIT 에 키를 직접 보낸다(실제 서브클래스 프로시저를 거친다) — 시험용.
    pub fn send_key(&mut self, vk: u16) {
        unsafe {
            let edit = self.state.as_ref().map(|s| s.editor.host.edit).unwrap_or(null_mut());
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
