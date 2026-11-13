//! 창: 떠 있는 창(도구 창, 작업 표시줄 없음)·일반 창(작업 표시줄 있음)·모달 대화상자. 모두 사용자 틀
//! (WM_NCCALCSIZE로 비클라이언트 없앰 + WM_NCHITTEST로 크기 조절·끌어 옮기기, [`crate::frame`]).
//!
//! 창 프로시저에서 틀 관련 메시지는 [`frame_message`] 한 곳에 맡긴다:
//! `if let Some(r) = ui_core::window::frame_message(hwnd, msg, wp, lp, &spec) { return r; }`

use windows_sys::Win32::Foundation::{ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, MonitorFromWindow, ScreenToClient};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::frame::{Hit, frame_hit, scale};
use crate::geom::{Rect, xy};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 창 모양.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// 단축키 창(popup·kvf): 도구 창, 작업 표시줄 없음. noactivate면 눌러도 활성화하지 않음.
    Floating { topmost: bool, noactivate: bool },
    /// 일반 창(주 작업 화면 등): 작업 표시줄 단추, 최소화·최대화.
    Normal,
    /// 모달 대화상자(소유 창 위, 작업 표시줄 없음).
    Dialog,
}

/// (확장 스타일, 스타일). WS_THICKFRAME은 크기 조절·DWM 그림자용으로만 두고 틀은 WM_NCCALCSIZE로 없앤다.
pub fn styles(kind: Kind) -> (u32, u32) {
    match kind {
        Kind::Floating { topmost, noactivate } => (
            WS_EX_TOOLWINDOW | if topmost { WS_EX_TOPMOST } else { 0 } | if noactivate { WS_EX_NOACTIVATE } else { 0 },
            WS_POPUP | WS_THICKFRAME,
        ),
        Kind::Normal => (WS_EX_APPWINDOW, WS_POPUP | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_SYSMENU),
        Kind::Dialog => (WS_EX_DLGMODALFRAME, WS_POPUP | WS_THICKFRAME | WS_SYSMENU),
    }
}

/// 창 종류 등록(이미 있으면 그대로 성공).
pub fn register_class(name: &str, proc_: WNDPROC, dblclks: bool) -> Result<(), String> {
    unsafe {
        let class = wide(name);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: if dblclks { CS_DBLCLKS } else { 0 },
            lpfnWndProc: proc_,
            hInstance: GetModuleHandleW(std::ptr::null()),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        if RegisterClassExW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return Err(format!("창 종류 등록 실패: {name}"));
        }
        Ok(())
    }
}

/// 창 만들기 + 틀 꾸밈(DWM 그림자·둥근 모서리).
pub fn create(kind: Kind, class: &str, title: &str, r: Rect, owner: HWND) -> Result<HWND, String> {
    unsafe {
        let (ex, style) = styles(kind);
        let h = CreateWindowExW(ex, wide(class).as_ptr(), wide(title).as_ptr(), style, r.left, r.top, r.width(), r.height(),
                                owner, std::ptr::null_mut(), GetModuleHandleW(std::ptr::null()), std::ptr::null());
        if h.is_null() {
            return Err(format!("창 만들기 실패: {class}"));
        }
        crate::frame::style_frame(h);
        SetWindowPos(h, std::ptr::null_mut(), 0, 0, 0, 0,
                     SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
        Ok(h)
    }
}

/// 틀 판정에 쓰는 창 치수(논리 px)와 판정 함수.
pub struct FrameSpec<'a> {
    pub head: i32,
    pub min_w: i32,
    pub min_h: i32,
    /// 창 안 좌표가 머리글 단추(끌어 옮기기 아님) 위인지.
    pub on_button: &'a dyn Fn(i32, i32) -> bool,
    pub maximized: bool,
    pub collapsed: bool,
}

/// 사용자 틀 메시지(WM_NCCALCSIZE·WM_NCHITTEST·WM_GETMINMAXINFO) 처리. 처리했으면 Some(결과).
///
/// # Safety
/// 창 프로시저 인자를 그대로 넘겨야 한다.
pub unsafe fn frame_message(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, spec: &FrameSpec) -> Option<LRESULT> {
    unsafe {
        match msg {
            WM_NCCALCSIZE if wp != 0 => Some(0),
            WM_NCHITTEST => {
                let (sx, sy) = xy(lp);
                let mut pt = POINT { x: sx, y: sy };
                ScreenToClient(hwnd, &mut pt);
                let mut rc: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut rc);
                let hit = frame_hit(pt.x, pt.y, rc.right, rc.bottom, scale(hwnd, crate::frame::GRIP), scale(hwnd, spec.head),
                                    (spec.on_button)(pt.x, pt.y), spec.maximized, spec.collapsed);
                Some(hit.code() as LRESULT)
            }
            WM_GETMINMAXINFO => {
                let info = &mut *(lp as *mut MINMAXINFO);
                info.ptMinTrackSize = POINT { x: scale(hwnd, spec.min_w), y: scale(hwnd, spec.min_h) };
                Some(0)
            }
            _ => None,
        }
    }
}

/// 판정 결과만(시험용 순수 함수 재노출).
pub fn hit(x: i32, y: i32, w: i32, h: i32, grip: i32, head: i32, on_button: bool, maximized: bool, collapsed: bool) -> Hit {
    frame_hit(x, y, w, h, grip, head, on_button, maximized, collapsed)
}

/// 저장된 위치(없으면 마우스 옆)를 작업 영역 안으로(kvf `place`에서 옮김, 순수 함수).
/// 돌려주는 값 [x, y, w, h]. default = 처음 크기, min = 최소 크기.
pub fn place(saved: Option<[i32; 4]>, mouse: [i32; 2], work: Rect, offset: i32, default: [i32; 2], min: [i32; 2]) -> [i32; 4] {
    let mut r = saved.unwrap_or([mouse[0] + offset, mouse[1] + offset, default[0], default[1]]);
    r[2] = r[2].max(min[0]).min(work.width().max(1));
    r[3] = r[3].max(min[1]).min(work.height().max(1));
    r[0] = r[0].clamp(work.left, (work.right - r[2]).max(work.left));
    r[1] = r[1].clamp(work.top, (work.bottom - r[3]).max(work.top));
    r
}

/// 점이 있는 모니터의 작업 영역.
pub fn work_area_at(pt: POINT) -> Rect {
    unsafe {
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST), &mut mi);
        Rect::from_win(&mi.rcWork)
    }
}

/// 창이 있는 모니터의 작업 영역.
pub fn work_area_of(hwnd: HWND) -> Rect {
    unsafe {
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut mi);
        Rect::from_win(&mi.rcWork)
    }
}

/// 항상 위 켜고 끄기(활성화하지 않음).
pub fn set_topmost(hwnd: HWND, on: bool) {
    unsafe {
        SetWindowPos(hwnd, if on { HWND_TOPMOST } else { HWND_NOTOPMOST }, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

/// 모달 대화상자 실행: 소유 창을 막고, `done()`이 true가 될 때까지 메시지를 돌린다. 끝나면 소유 창을 되살린다.
/// WM_QUIT을 받으면 다시 넣고 끝낸다(앱 종료가 막히지 않게).
pub fn run_modal(owner: HWND, dialog: HWND, done: &dyn Fn() -> bool) {
    unsafe {
        if !owner.is_null() {
            EnableWindow(owner, 0);
        }
        ShowWindow(dialog, SW_SHOW);
        let mut msg: MSG = std::mem::zeroed();
        while !done() && IsWindow(dialog) != 0 {
            let r = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
            if r == 0 {
                PostQuitMessage(msg.wParam as i32);
                break;
            }
            if r < 0 {
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if !owner.is_null() {
            EnableWindow(owner, 1);
            SetForegroundWindow(owner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_matches_kvf() {
        let work = Rect::new(0, 0, 1920, 1080);
        assert_eq!(place(Some([9999, -9999, 600, 420]), [0, 0], work, 12, [600, 420], [360, 160]), [1320, 0, 600, 420]);
        assert_eq!(place(None, [100, 100], work, 12, [600, 420], [360, 160]), [112, 112, 600, 420]);
        assert_eq!(place(Some([0, 0, 10, 10]), [0, 0], work, 12, [600, 420], [360, 160]), [0, 0, 360, 160]);
        assert_eq!(place(Some([0, 0, 5000, 5000]), [0, 0], work, 12, [600, 420], [360, 160]), [0, 0, 1920, 1080]);
    }

    #[test]
    fn styles_per_kind() {
        let (ex, st) = styles(Kind::Floating { topmost: true, noactivate: true });
        assert_ne!(ex & WS_EX_TOOLWINDOW, 0);
        assert_ne!(ex & WS_EX_TOPMOST, 0);
        assert_ne!(ex & WS_EX_NOACTIVATE, 0);
        assert_eq!(st, WS_POPUP | WS_THICKFRAME, "popup과 같은 스타일");
        let (ex, st) = styles(Kind::Normal);
        assert_ne!(ex & WS_EX_APPWINDOW, 0);
        assert_ne!(st & WS_MAXIMIZEBOX, 0);
    }

    unsafe extern "system" fn proc_(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        let spec = FrameSpec { head: 32, min_w: 200, min_h: 100, on_button: &|x, _| x > 150, maximized: false, collapsed: false };
        unsafe {
            if let Some(r) = frame_message(h, m, w, l, &spec) {
                return r;
            }
            DefWindowProcW(h, m, w, l)
        }
    }

    #[test]
    fn real_window_frame_messages() {
        register_class("UiCore.Test.Window", Some(proc_), false).unwrap();
        register_class("UiCore.Test.Window", Some(proc_), false).unwrap();
        let h = create(Kind::Floating { topmost: false, noactivate: true }, "UiCore.Test.Window", "시험", Rect::xywh(0, 0, 300, 200),
                       std::ptr::null_mut()).unwrap();
        unsafe {
            let mut rc: RECT = std::mem::zeroed();
            GetClientRect(h, &mut rc);
            assert_eq!((rc.right, rc.bottom), (300, 200), "틀이 없어 창 = 클라이언트");
            let mut wr: RECT = std::mem::zeroed();
            GetWindowRect(h, &mut wr);
            let at = |x: i32, y: i32| SendMessageW(h, WM_NCHITTEST, 0, (((wr.top + y) as isize & 0xFFFF) << 16) | ((wr.left + x) as isize & 0xFFFF)) as u32;
            assert_eq!(at(100, 10), HTCAPTION);
            assert_eq!(at(200, 10), HTCLIENT, "머리글 단추");
            assert_eq!(at(1, 100), HTLEFT);
            assert_eq!(at(100, 199), HTBOTTOM);
            DestroyWindow(h);
        }
    }
}
