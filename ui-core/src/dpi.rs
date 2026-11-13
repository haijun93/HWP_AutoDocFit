//! 화면 배율(Per-Monitor v2)과 WM_DPICHANGED 다시 배치.
//!
//! 창이 다른 배율 모니터로 옮겨지면 Windows가 WM_DPICHANGED(WPARAM 하위 = 새 DPI, LPARAM = 제안 사각형)를 보낸다.
//! 제안 사각형을 그대로 적용하고(Windows 권장), 글꼴 캐시는 DPI를 열쇠로 쓰므로 새 DPI 글꼴이 자동으로 만들어진다.

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos};

use crate::geom::Rect;

/// 프로세스를 Per-Monitor v2로(창을 만들기 전 한 번). 이미 정해졌으면 실패해도 그대로 둔다.
pub fn enable_per_monitor_v2() -> bool {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) != 0 }
}

/// 창의 DPI(최소 96).
pub fn window_dpi(hwnd: HWND) -> u32 {
    unsafe { GetDpiForWindow(hwnd) }.max(96)
}

/// WM_DPICHANGED의 새 DPI(WPARAM 하위 16비트 = X DPI).
pub fn dpi_of_wparam(wp: usize) -> u32 {
    ((wp & 0xFFFF) as u32).max(96)
}

/// WM_DPICHANGED의 제안 사각형.
///
/// # Safety
/// `lp`는 WM_DPICHANGED의 LPARAM(유효한 RECT 포인터)이어야 한다.
pub unsafe fn suggested_rect(lp: isize) -> Rect {
    unsafe { Rect::from_win(&*(lp as *const RECT)) }
}

/// 배율이 바뀔 때 논리 크기를 유지한 새 px 크기(제안 사각형이 없을 때 쓰는 순수 계산).
pub fn rescale(px: i32, from_dpi: u32, to_dpi: u32) -> i32 {
    (px as i64 * to_dpi.max(96) as i64 / from_dpi.max(96) as i64) as i32
}

/// WM_DPICHANGED 처리: 제안 사각형으로 옮기고 크기를 바꾼다. 돌려주는 값은 새 DPI.
///
/// # Safety
/// WM_DPICHANGED의 WPARAM·LPARAM을 그대로 넘겨야 한다.
pub unsafe fn on_dpi_changed(hwnd: HWND, wp: usize, lp: isize) -> u32 {
    unsafe {
        let r = suggested_rect(lp);
        SetWindowPos(hwnd, std::ptr::null_mut(), r.left, r.top, r.width(), r.height(), SWP_NOZORDER | SWP_NOACTIVATE);
    }
    dpi_of_wparam(wp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wparam_dpi_and_rescale() {
        assert_eq!(dpi_of_wparam(0x0090_0090), 144);
        assert_eq!(dpi_of_wparam(0), 96);
        // 100 → 125 → 150 → 200% 사이를 오가도 논리 크기는 같다.
        for (from, to) in [(96, 120), (120, 144), (144, 192), (192, 96)] {
            let logical = 440;
            let px = logical * from as i32 / 96;
            assert_eq!(rescale(px, from, to), logical * to as i32 / 96);
        }
    }

    #[test]
    fn suggested_rect_reads_lparam() {
        let r = RECT { left: 10, top: 20, right: 670, bottom: 620 };
        let got = unsafe { suggested_rect(&r as *const RECT as isize) };
        assert_eq!(got, Rect::new(10, 20, 670, 620));
    }
}
