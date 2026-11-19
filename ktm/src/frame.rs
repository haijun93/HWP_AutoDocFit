//! 틀 없는 창의 공용 틀 함수: 크기 조절 판정·테두리 한 겹·DWM 꾸밈·화면 배율·줄 위치·글자 폭.
//! `kagent/src/frame.rs`와 동일한 동작 및 규격을 따르며, Windows 외 환경에서도 순수 레이아웃 시험이 돌아가도록 한다.
#![allow(dead_code)]

#[cfg(windows)]
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use windows_sys::Win32::Foundation::{HWND, RECT};
#[cfg(windows)]
use windows_sys::Win32::Foundation::SIZE;
#[cfg(windows)]
use windows_sys::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject, FrameRect, GetTextExtentPoint32W, HDC};
#[cfg(windows)]
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT, HTRIGHT,
};

/// 크기 조절로 쓰는 가장자리 폭(창 안쪽).
#[allow(dead_code)]
pub(crate) const GRIP: i32 = 6;

// ---- 틀(테두리·크기 조절) ----

/// WM_NCHITTEST 결과.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Hit {
    Client,
    Caption,
    Left,
    Right,
    Bottom,
    BottomLeft,
    BottomRight,
}

impl Hit {
    pub(crate) fn code(self) -> u32 {
        match self {
            Hit::Client => HTCLIENT,
            Hit::Caption => HTCAPTION,
            Hit::Left => HTLEFT,
            Hit::Right => HTRIGHT,
            Hit::Bottom => HTBOTTOM,
            Hit::BottomLeft => HTBOTTOMLEFT,
            Hit::BottomRight => HTBOTTOMRIGHT,
        }
    }
}

/// 창 안 좌표(x, y)가 무엇을 가리키는지(순수 함수). 틀이 없으므로 가장자리 grip 폭을 크기 조절에 쓴다.
pub(crate) fn frame_hit(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    grip: i32,
    head: i32,
    on_button: bool,
    maximized: bool,
    collapsed: bool,
) -> Hit {
    if y < head && on_button {
        return Hit::Client;
    }
    if !maximized && x >= 0 && y >= 0 && x < w && y < h {
        let left = x < grip;
        let right = x >= w - grip;
        let bottom = !collapsed && y >= h - grip;
        match (left, right, bottom) {
            (true, _, true) => return Hit::BottomLeft,
            (_, true, true) => return Hit::BottomRight,
            (_, _, true) => return Hit::Bottom,
            (true, _, _) => return Hit::Left,
            (_, true, _) => return Hit::Right,
            _ => {}
        }
    }
    if y >= 0 && y < head {
        Hit::Caption
    } else {
        Hit::Client
    }
}

/// 테두리 선을 직접 그릴지: DWM이 테두리 색을 받아 그려 주면(Windows 11) 그리지 않는다 — 한 겹만.
#[allow(dead_code)]
pub(crate) fn draws_own_border(dwm_border: bool) -> bool {
    !dwm_border
}

/// Windows 11에서 DWM 테두리를 쓰는지(창을 만들 때 정한다).
#[cfg(windows)]
static DWM_BORDER: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
#[repr(C)]
struct Margins {
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
}

#[cfg(windows)]
#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmSetWindowAttribute(hwnd: HWND, attr: u32, value: *const core::ffi::c_void, size: u32) -> i32;
    fn DwmExtendFrameIntoClientArea(hwnd: HWND, margins: *const Margins) -> i32;
}
#[cfg(windows)]
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
#[cfg(windows)]
const DWMWA_BORDER_COLOR: u32 = 34;
#[cfg(windows)]
const DWMWCP_ROUND: u32 = 2;

/// 틀 꾸밈: DWM 그림자, Windows 11 둥근 모서리·테두리 색.
#[cfg(windows)]
pub(crate) unsafe fn style_frame(hwnd: HWND) {
    unsafe {
        let m = Margins {
            left: 0,
            right: 0,
            top: 1,
            bottom: 0,
        };
        DwmExtendFrameIntoClientArea(hwnd, &m);
        let corner = DWMWCP_ROUND;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const u32 as _,
            4,
        );
        apply_theme(hwnd);
    }
}

#[cfg(not(windows))]
pub(crate) unsafe fn style_frame(_hwnd: HWND) {}

/// 현재 테마를 창 틀에 입힌다(어두운 DWM 모드·테두리 색).
#[cfg(windows)]
pub(crate) unsafe fn apply_theme(hwnd: HWND) {
    let theme = ui_theme::theme();
    ui_theme::apply_window(hwnd, theme.kind);
    let color = theme.palette.border;
    let ok = unsafe { DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &color as *const u32 as _, 4) } >= 0;
    DWM_BORDER.store(ok, Ordering::Relaxed);
}

#[cfg(not(windows))]
pub(crate) unsafe fn apply_theme(_hwnd: HWND) {}

/// Windows 11이 아니면 창 둘레에 1px 선을 한 번만 그린다.
#[cfg(windows)]
pub(crate) unsafe fn own_border(mem: HDC, rc: RECT) {
    if !draws_own_border(DWM_BORDER.load(Ordering::Relaxed)) {
        return;
    }
    unsafe {
        let brush = CreateSolidBrush(ui_theme::palette().border);
        if !brush.is_null() {
            FrameRect(mem, &rc, brush);
            DeleteObject(brush);
        }
    }
}

#[cfg(not(windows))]
pub(crate) unsafe fn own_border(_mem: *mut core::ffi::c_void, _rc: RECT) {}

/// 화면 캡처 모드에서만 쓰는 DPI 강제값(0이면 창의 실제 DPI).
#[cfg(windows)]
static DPI_OVERRIDE: AtomicI32 = AtomicI32::new(0);

#[cfg(windows)]
pub(crate) fn set_dpi_override(dpi: i32) {
    DPI_OVERRIDE.store(dpi, Ordering::Relaxed);
}

pub(crate) fn scale(hwnd: HWND, v: i32) -> i32 {
    #[cfg(windows)]
    {
        let forced = DPI_OVERRIDE.load(Ordering::Relaxed);
        let dpi = if forced > 0 { forced } else { unsafe { GetDpiForWindow(hwnd) }.max(96) as i32 };
        v * dpi / 96
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        v
    }
}

/// y가 가리키는 항목 위치(스크롤 top 반영). 목록 밖이면 None.
pub(crate) fn row_index(y: i32, top_y: i32, row_h: i32, scroll_top: usize, len: usize) -> Option<usize> {
    if y < top_y || row_h <= 0 {
        return None;
    }
    let i = scroll_top + ((y - top_y) / row_h) as usize;
    (i < len).then_some(i)
}

#[cfg(windows)]
pub(crate) unsafe fn text_width(dc: HDC, s: &str) -> i32 {
    let w: Vec<u16> = s.encode_utf16().collect();
    let mut size = SIZE { cx: 0, cy: 0 };
    unsafe { GetTextExtentPoint32W(dc, w.as_ptr(), w.len() as i32, &mut size) };
    size.cx
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD_H: i32 = 32;
    const COLLAPSED_H: i32 = HEAD_H + 2;

    #[test]
    fn frame_hit_resizes_left_right_bottom_only_at_every_scale() {
        for pct in [100, 150, 200] {
            let s = |v: i32| v * pct / 100;
            let (w, h, grip, head) = (s(440), s(400), s(GRIP), s(HEAD_H));
            let hit = |x: i32, y: i32| frame_hit(x, y, w, h, grip, head, false, false, false);
            assert_eq!(hit(0, s(200)), Hit::Left, "{pct}%");
            assert_eq!(hit(grip - 1, s(200)), Hit::Left);
            assert_eq!(hit(grip, s(200)), Hit::Client);
            assert_eq!(hit(w - 1, s(200)), Hit::Right);
            assert_eq!(hit(w - grip, s(200)), Hit::Right);
            assert_eq!(hit(w - grip - 1, s(200)), Hit::Client);
            assert_eq!(hit(s(200), h - 1), Hit::Bottom);
            assert_eq!(hit(s(200), h - grip - 1), Hit::Client);
            assert_eq!(hit(0, h - 1), Hit::BottomLeft);
            assert_eq!(hit(w - 1, h - 1), Hit::BottomRight);
            assert_eq!(hit(s(200), 0), Hit::Caption);
            assert_eq!(hit(s(200), head - 1), Hit::Caption);
            assert_eq!(hit(s(200), head), Hit::Client);
            assert_eq!(hit(0, 0), Hit::Left);
        }
    }

    #[test]
    fn frame_hit_buttons_win_and_maximized_or_collapsed_limits_resizing() {
        let (w, h, grip, head) = (440, 400, 6, 32);
        assert_eq!(frame_hit(w - 1, 5, w, h, grip, head, true, false, false), Hit::Client);
        assert_eq!(frame_hit(w - 1, head, w, h, grip, head, false, false, false), Hit::Right);
        assert_eq!(frame_hit(0, 200, w, h, grip, head, false, true, false), Hit::Client);
        assert_eq!(frame_hit(200, h - 1, w, h, grip, head, false, true, false), Hit::Client);
        assert_eq!(frame_hit(200, 5, w, h, grip, head, false, true, false), Hit::Caption);
        let ch = COLLAPSED_H;
        assert_eq!(frame_hit(200, ch - 1, w, ch, grip, head, false, false, true), Hit::Client);
        assert_eq!(frame_hit(200, 5, w, ch, grip, head, false, false, true), Hit::Caption);
        assert_eq!(frame_hit(0, 5, w, ch, grip, head, false, false, true), Hit::Left);
        assert_eq!(frame_hit(w - 1, ch - 1, w, ch, grip, head, false, false, true), Hit::Right);
        assert_eq!(frame_hit(-1, 200, w, h, grip, head, false, false, false), Hit::Client);
        assert_eq!(Hit::Caption.code(), HTCAPTION);
        assert_eq!(Hit::BottomRight.code(), HTBOTTOMRIGHT);
    }
}
