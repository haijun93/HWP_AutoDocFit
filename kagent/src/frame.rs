//! 틀 없는 창의 공용 틀 함수: 크기 조절 판정·테두리 한 겹·DWM 꾸밈·화면 배율·줄 위치·글자 폭.
//! `popup.rs`(몽땅클립보드 창)에서 내용 변경 없이 옮겼다(2026-11-12, VF V1).
//! kvf(가상 폴더 창)도 이 파일을 컴파일한다. `std`·`windows-sys`·`crate::util`·`ui_theme` 말고는 쓰지 말 것.
//! 테두리 색은 현재 테마(`ui_theme::palette().border`)를 따른다(RUSTALL1 DARK1).

use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::{HWND, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject, FrameRect, GetTextExtentPoint32W, HDC};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT, HTRIGHT,
};

/// 크기 조절로 쓰는 가장자리 폭(창 안쪽).
pub(crate) const GRIP: i32 = 6;

// ---- 틀(테두리·크기 조절) ----

/// WM_NCHITTEST 결과.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Hit { Client, Caption, Left, Right, Bottom, BottomLeft, BottomRight }

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
/// - 좌·우·아래·아래 모서리만 크기 조절(위쪽은 머리글 = 끌어 옮기기, 요구대로).
/// - 머리글의 단추 위는 오른쪽 가장자리여도 단추(닫기 단추 끝까지 누를 수 있게).
/// - 최대화 중이면 크기 조절 없음, 접힌 창이면 아래로 늘이지 않는다(펼치기는 메뉴로).
pub(crate) fn frame_hit(x: i32, y: i32, w: i32, h: i32, grip: i32, head: i32, on_button: bool, maximized: bool,
             collapsed: bool) -> Hit {
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
    if y >= 0 && y < head { Hit::Caption } else { Hit::Client }
}

/// 테두리 선을 직접 그릴지: DWM이 테두리 색을 받아 그려 주면(Windows 11) 그리지 않는다 — 한 겹만.
pub(crate) fn draws_own_border(dwm_border: bool) -> bool {
    !dwm_border
}

/// Windows 11에서 DWM 테두리를 쓰는지(창을 만들 때 정한다).
static DWM_BORDER: AtomicBool = AtomicBool::new(false);

#[repr(C)]
struct Margins { left: i32, right: i32, top: i32, bottom: i32 }

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmSetWindowAttribute(hwnd: HWND, attr: u32, value: *const core::ffi::c_void, size: u32) -> i32;
    fn DwmExtendFrameIntoClientArea(hwnd: HWND, margins: *const Margins) -> i32;
}
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWA_BORDER_COLOR: u32 = 34;
const DWMWCP_ROUND: u32 = 2;

/// 틀 꾸밈: DWM 그림자(틀을 1px 안으로 늘려야 틀 없는 창에도 4면 그림자가 생김), Windows 11 둥근 모서리·테두리 색.
/// Windows 10에서는 모서리·테두리 색 속성이 실패하고 직접 그린 1px 선을 쓴다.
pub(crate) unsafe fn style_frame(hwnd: HWND) {
    unsafe {
        let m = Margins { left: 0, right: 0, top: 1, bottom: 0 };
        DwmExtendFrameIntoClientArea(hwnd, &m);
        let corner = DWMWCP_ROUND;
        DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const u32 as _, 4);
        apply_theme(hwnd);
    }
}

/// 현재 테마를 창 틀에 입힌다(어두운 DWM 모드·테두리 색). 창을 만들 때와 테마가 바뀔 때 부른다.
/// 그리기 영역은 부르는 쪽이 InvalidateRect로 다시 그린다.
pub(crate) unsafe fn apply_theme(hwnd: HWND) {
    let theme = ui_theme::theme();
    ui_theme::apply_window(hwnd, theme.kind);
    let color = theme.palette.border;
    let ok = unsafe { DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &color as *const u32 as _, 4) } >= 0;
    DWM_BORDER.store(ok, Ordering::Relaxed);
}

/// Windows 11이 아니면 창 둘레에 1px 선을 한 번만 그린다(틀·WS_BORDER가 없으므로 이것이 유일한 테두리).
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

pub(crate) fn scale(hwnd: HWND, v: i32) -> i32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96) as i32;
    v * dpi / 96
}

/// y가 가리키는 항목 위치(스크롤 top 반영). 목록 밖이면 None.
pub(crate) fn row_index(y: i32, top_y: i32, row_h: i32, scroll_top: usize, len: usize) -> Option<usize> {
    if y < top_y || row_h <= 0 {
        return None;
    }
    let i = scroll_top + ((y - top_y) / row_h) as usize;
    (i < len).then_some(i)
}

pub(crate) unsafe fn text_width(dc: HDC, s: &str) -> i32 {
    let w: Vec<u16> = s.encode_utf16().collect();
    let mut size = SIZE { cx: 0, cy: 0 };
    unsafe { GetTextExtentPoint32W(dc, w.as_ptr(), w.len() as i32, &mut size) };
    size.cx
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 시험에 쓰는 창 치수(몽땅클립보드 창의 머리글 높이·접힌 창 높이와 같은 값).
    const HEAD_H: i32 = 32;
    const COLLAPSED_H: i32 = HEAD_H + 2;

    /// 크기 조절 판정을 화면 배율별로(100·150·200%) — 같은 논리 좌표가 같은 결과.
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
            // 위쪽은 크기 조절이 아니라 머리글(끌어 옮기기). 왼쪽 가장자리는 머리글 높이에서도 크기 조절.
            assert_eq!(hit(s(200), 0), Hit::Caption);
            assert_eq!(hit(s(200), head - 1), Hit::Caption);
            assert_eq!(hit(s(200), head), Hit::Client);
            assert_eq!(hit(0, 0), Hit::Left);
        }
    }

    #[test]
    fn frame_hit_buttons_win_and_maximized_or_collapsed_limits_resizing() {
        let (w, h, grip, head) = (440, 400, 6, 32);
        // 머리글 오른쪽 끝(닫기 단추)은 단추가 이긴다.
        assert_eq!(frame_hit(w - 1, 5, w, h, grip, head, true, false, false), Hit::Client);
        assert_eq!(frame_hit(w - 1, head, w, h, grip, head, false, false, false), Hit::Right);
        // 최대화: 크기 조절 없음, 머리글은 그대로 이동 영역(두 번 클릭으로 복원).
        assert_eq!(frame_hit(0, 200, w, h, grip, head, false, true, false), Hit::Client);
        assert_eq!(frame_hit(200, h - 1, w, h, grip, head, false, true, false), Hit::Client);
        assert_eq!(frame_hit(200, 5, w, h, grip, head, false, true, false), Hit::Caption);
        // 접힌 창(머리글만): 아래로 늘이지 않고 좌우 폭만 조절.
        let ch = COLLAPSED_H;
        assert_eq!(frame_hit(200, ch - 1, w, ch, grip, head, false, false, true), Hit::Client);
        assert_eq!(frame_hit(200, 5, w, ch, grip, head, false, false, true), Hit::Caption);
        assert_eq!(frame_hit(0, 5, w, ch, grip, head, false, false, true), Hit::Left);
        assert_eq!(frame_hit(w - 1, ch - 1, w, ch, grip, head, false, false, true), Hit::Right);
        // 창 밖 좌표는 크기 조절로 보지 않는다.
        assert_eq!(frame_hit(-1, 200, w, h, grip, head, false, false, false), Hit::Client);
        assert_eq!(Hit::Caption.code(), HTCAPTION);
        assert_eq!(Hit::BottomRight.code(), HTBOTTOMRIGHT);
    }
}
