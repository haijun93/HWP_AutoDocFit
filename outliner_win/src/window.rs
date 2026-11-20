//! 읽기 전용 블록 목록 창(편집 없음). ↑↓·PgUp/PgDn·Home/End 로 블록 선택, ←/→ 접기·펼치기, 휠·막대 스크롤.
use crate::paint::{Metrics, draw, wide};
use crate::view::{Nav, View, row_at};
use std::ptr::{null, null_mut};
use ui_theme::ThemePref;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::SetScrollInfo;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

struct State {
    view: View,
    title: String,
    dpi: u32,
    pref: ThemePref,
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

unsafe fn sync_scroll(hwnd: HWND, s: &State) {
    unsafe {
        let m = Metrics::new(s.dpi);
        let page = m.page_rows(client(hwnd).bottom);
        let info = SCROLLINFO {
            cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
            fMask: SIF_ALL,
            nMin: 0,
            nMax: s.view.rows.len().max(1) as i32 - 1,
            nPage: page as u32,
            nPos: s.view.first as i32,
            nTrackPos: 0,
        };
        SetScrollInfo(hwnd, SB_VERT, &info, 1);
    }
}

unsafe fn refresh_view(hwnd: HWND) {
    unsafe {
        if let Some(s) = state(hwnd) {
            sync_scroll(hwnd, s);
        }
        InvalidateRect(hwnd, null(), 0);
    }
}

unsafe fn page_rows(hwnd: HWND, s: &State) -> usize {
    unsafe { Metrics::new(s.dpi).page_rows(client(hwnd).bottom) }
}

unsafe fn paint(hwnd: HWND) {
    unsafe {
        let Some(s) = state(hwnd) else { return };
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let dc = BeginPaint(hwnd, &mut ps);
        let r = client(hwnd);
        let mem = CreateCompatibleDC(dc);
        let bm = CreateCompatibleBitmap(dc, r.right.max(1), r.bottom.max(1));
        let old = SelectObject(mem, bm);
        let (_, pal) = ui_theme::current(s.pref);
        draw(mem, r.right, r.bottom, &s.view, &pal, s.dpi, &s.title);
        BitBlt(dc, 0, 0, r.right, r.bottom, mem, 0, 0, SRCCOPY);
        SelectObject(mem, old);
        DeleteObject(bm);
        DeleteDC(mem);
        EndPaint(hwnd, &ps);
    }
}

unsafe fn key_down(hwnd: HWND, vk: u16) -> bool {
    unsafe {
        let Some(s) = state(hwnd) else { return false };
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
            s.view.select(nav, page);
        } else if vk == VK_LEFT || vk == VK_RIGHT {
            s.view.set_fold(vk == VK_LEFT, page);
        } else {
            return false;
        }
        refresh_view(hwnd);
        true
    }
}

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
            WM_SIZE => {
                if let Some(s) = state(hwnd) {
                    let page = page_rows(hwnd, s);
                    let first = s.view.first;
                    s.view.set_first(first, page);
                }
                refresh_view(hwnd);
                0
            }
            WM_DPICHANGED => {
                if let Some(s) = state(hwnd) {
                    s.dpi = (wp & 0xFFFF) as u32;
                }
                let r = lp as *const RECT;
                SetWindowPos(hwnd, null_mut(), (*r).left, (*r).top, (*r).right - (*r).left, (*r).bottom - (*r).top, SWP_NOZORDER | SWP_NOACTIVATE);
                refresh_view(hwnd);
                0
            }
            WM_KEYDOWN => {
                if key_down(hwnd, wp as u16) {
                    0
                } else {
                    DefWindowProcW(hwnd, msg, wp, lp)
                }
            }
            WM_MOUSEWHEEL => {
                if let Some(s) = state(hwnd) {
                    let delta = ((wp >> 16) as u16 as i16) as i32;
                    let page = page_rows(hwnd, s);
                    s.view.scroll_by(-(delta / 120) as isize * 3, page);
                }
                refresh_view(hwnd);
                0
            }
            WM_VSCROLL => {
                if let Some(s) = state(hwnd) {
                    let page = page_rows(hwnd, s);
                    let first = s.view.first as isize;
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
                    s.view.set_first(target.max(0) as usize, page);
                }
                refresh_view(hwnd);
                0
            }
            WM_LBUTTONDOWN => {
                SetFocus(hwnd);
                if let Some(s) = state(hwnd) {
                    let y = ((lp >> 16) & 0xFFFF) as u16 as i16 as i32;
                    let m = Metrics::new(s.dpi);
                    if let Some(i) = row_at(y, m.header_h, m.row_h, s.view.first, s.view.rows.len()) {
                        let page = page_rows(hwnd, s);
                        s.view.set_selected(i, page);
                    }
                }
                refresh_view(hwnd);
                0
            }
            WM_GETDLGCODE => (DLGC_WANTARROWS | DLGC_WANTCHARS) as isize,
            WM_DESTROY => {
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

/// 창을 띄우고 닫힐 때까지 메시지 루프를 돈다.
pub fn run(view: View, title: &str) -> Result<(), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let hinst = GetModuleHandleW(null());
        let class = wide("OutlinerWinMain");
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
        let boxed = Box::into_raw(Box::new(State { view, title: title.to_string(), dpi: 96, pref }));
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
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
            drop(Box::from_raw(boxed));
            return Err("창을 만들지 못했습니다".into());
        }
        (*boxed).dpi = GetDpiForWindow(hwnd).max(96);
        let (kind, _) = ui_theme::current(pref);
        ui_theme::apply_window(hwnd, kind);
        ShowWindow(hwnd, SW_SHOW);
        sync_scroll(hwnd, &*boxed);
        SetFocus(hwnd);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        drop(Box::from_raw(boxed));
        Ok(())
    }
}
