//! GDI 이중 버퍼 그리기. `WM_PAINT`에서 [`BackBuffer::begin`] → 메모리 DC에 그리기 → [`BackBuffer::finish`](BitBlt).
//! 중간에 실패하거나 일찍 돌아가도 Drop이 DC·비트맵을 정리하고 `EndPaint`를 부른다(popup `PaintGuard`와 같음).
//! `WM_ERASEBKGND`는 1을 돌려 깜박임을 없앤다.
//!
//! [`Painter`]는 임의 DC 위의 그리기 도움 함수 묶음이다(채우기·테두리·한 줄 글자·글자 폭). 창 없이도 메모리 DC로 시험한다.

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DT_NOPREFIX, DT_SINGLELINE,
    DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, FrameRect, HBITMAP, HDC, HFONT,
    HGDIOBJ, PAINTSTRUCT, SRCCOPY, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};

use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

use crate::geom::Rect;

/// 임의 DC에 그리는 도움 함수(상태 없음, 복사 가능).
#[derive(Clone, Copy)]
pub struct Painter {
    pub dc: HDC,
}

impl Painter {
    pub fn new(dc: HDC) -> Self {
        Self { dc }
    }
    /// 단색 채우기.
    pub fn fill(&self, r: RECT, color: u32) {
        unsafe {
            let brush = CreateSolidBrush(color);
            if !brush.is_null() {
                FillRect(self.dc, &r, brush);
                DeleteObject(brush);
            }
        }
    }
    /// 1px 테두리.
    pub fn frame(&self, r: RECT, color: u32) {
        unsafe {
            let brush = CreateSolidBrush(color);
            if !brush.is_null() {
                FrameRect(self.dc, &r, brush);
                DeleteObject(brush);
            }
        }
    }
    /// 한 줄 글자(세로 가운데, & 접두 해석 안 함). flags에 DT_CENTER·DT_END_ELLIPSIS 등을 더한다.
    /// 글자는 첫 NUL까지 그린다(popup의 `DrawTextW(.., -1, ..)`와 같음).
    pub fn text(&self, s: &str, r: RECT, flags: u32) {
        let w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
        let mut r = r;
        unsafe { DrawTextW(self.dc, w.as_ptr(), -1, &mut r, flags | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX) };
    }
    pub fn text_color(&self, color: u32) {
        unsafe { SetTextColor(self.dc, color) };
    }
    /// 글꼴 선택(이전 글꼴을 돌려준다).
    pub fn font(&self, f: HFONT) -> HGDIOBJ {
        unsafe { SelectObject(self.dc, f) }
    }
    pub fn transparent(&self) {
        unsafe { SetBkMode(self.dc, TRANSPARENT as i32) };
    }
    pub fn text_width(&self, s: &str) -> i32 {
        unsafe { crate::frame::text_width(self.dc, s) }
    }
    pub fn fill_rect(&self, r: Rect, color: u32) {
        self.fill(r.win(), color)
    }
    pub fn text_rect(&self, s: &str, r: Rect, flags: u32) {
        self.text(s, r.win(), flags)
    }
}

/// WM_PAINT 한 번의 이중 버퍼.
pub struct BackBuffer {
    hwnd: HWND,
    ps: PAINTSTRUCT,
    hdc: HDC,
    mem: HDC,
    bmp: HBITMAP,
    old_bmp: HGDIOBJ,
    old_font: HGDIOBJ,
    rc: RECT,
}

impl BackBuffer {
    /// BeginPaint와 클라이언트 크기의 메모리 DC·비트맵을 만든다. 실패하면 None(EndPaint는 이미 처리).
    ///
    /// # Safety
    /// `WM_PAINT` 처리 중 이 창의 스레드에서 불러야 한다.
    pub unsafe fn begin(hwnd: HWND) -> Option<BackBuffer> {
        unsafe {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            if hdc.is_null() {
                return None;
            }
            let mut b = BackBuffer {
                hwnd, ps, hdc, mem: std::ptr::null_mut(), bmp: std::ptr::null_mut(), old_bmp: std::ptr::null_mut(),
                old_font: std::ptr::null_mut(), rc: std::mem::zeroed(),
            };
            GetClientRect(hwnd, &mut b.rc);
            if b.rc.right <= 0 || b.rc.bottom <= 0 {
                return None;
            }
            b.mem = CreateCompatibleDC(hdc);
            if b.mem.is_null() {
                return None;
            }
            b.bmp = CreateCompatibleBitmap(hdc, b.rc.right, b.rc.bottom);
            if b.bmp.is_null() {
                return None;
            }
            b.old_bmp = SelectObject(b.mem, b.bmp);
            Some(b)
        }
    }

    /// 메모리 DC.
    pub fn dc(&self) -> HDC {
        self.mem
    }
    pub fn painter(&self) -> Painter {
        Painter::new(self.mem)
    }
    /// 클라이언트 사각형(0, 0, 폭, 높이).
    pub fn rect(&self) -> RECT {
        self.rc
    }
    /// 처음 글꼴을 고른다(정리 때 원래 글꼴로 되돌린다).
    pub fn select_font(&mut self, f: HFONT) {
        let old = unsafe { SelectObject(self.mem, f) };
        if self.old_font.is_null() {
            self.old_font = old;
        }
    }

    /// 화면으로 옮긴다(시험 기록 [`crate::probe`] 포함).
    pub fn finish(self) {
        unsafe {
            crate::probe::record(self.mem, self.rc.right, self.rc.bottom);
            BitBlt(self.hdc, 0, 0, self.rc.right, self.rc.bottom, self.mem, 0, 0, SRCCOPY);
        }
    }
}

impl Drop for BackBuffer {
    fn drop(&mut self) {
        unsafe {
            if !self.mem.is_null() {
                if !self.old_font.is_null() {
                    SelectObject(self.mem, self.old_font);
                }
                if !self.old_bmp.is_null() {
                    SelectObject(self.mem, self.old_bmp);
                }
                DeleteDC(self.mem);
            }
            if !self.bmp.is_null() {
                DeleteObject(self.bmp);
            }
            EndPaint(self.hwnd, &self.ps);
        }
    }
}

/// 창 없이 그려 보는 메모리 화면(시험·미리 그리기용, 32비트 DIB).
pub struct Offscreen {
    pub dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    pub width: i32,
    pub height: i32,
}

impl Offscreen {
    pub fn new(width: i32, height: i32) -> Option<Offscreen> {
        unsafe {
            let screen = windows_sys::Win32::Graphics::Gdi::GetDC(std::ptr::null_mut());
            let dc = CreateCompatibleDC(screen);
            let bmp = CreateCompatibleBitmap(screen, width, height);
            windows_sys::Win32::Graphics::Gdi::ReleaseDC(std::ptr::null_mut(), screen);
            if dc.is_null() || bmp.is_null() {
                if !dc.is_null() { DeleteDC(dc); }
                if !bmp.is_null() { DeleteObject(bmp); }
                return None;
            }
            let old = SelectObject(dc, bmp);
            Some(Offscreen { dc, bmp, old, width, height })
        }
    }
    pub fn painter(&self) -> Painter {
        Painter::new(self.dc)
    }
    /// 지금 픽셀의 해시.
    pub fn hash(&self) -> u64 {
        crate::probe::arm();
        unsafe { crate::probe::record(self.dc, self.width, self.height) };
        crate::probe::take().map(|f| f.hash).unwrap_or(0)
    }
    pub fn pixel(&self, x: i32, y: i32) -> u32 {
        unsafe { windows_sys::Win32::Graphics::Gdi::GetPixel(self.dc, x, y) }
    }
}

impl Drop for Offscreen {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            DeleteObject(self.bmp);
            DeleteDC(self.dc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn painter_fills_frames_and_measures_text() {
        let s = Offscreen::new(40, 20).unwrap();
        let p = s.painter();
        p.fill(Rect::xywh(0, 0, 40, 20).win(), 0x00FFFFFF);
        p.fill(Rect::xywh(0, 0, 10, 10).win(), 0x000000FF);
        assert_eq!(s.pixel(5, 5), 0x000000FF);
        assert_eq!(s.pixel(15, 5), 0x00FFFFFF);
        p.frame(Rect::xywh(20, 0, 10, 10).win(), 0x0000FF00);
        assert_eq!(s.pixel(20, 0), 0x0000FF00);
        assert_eq!(s.pixel(25, 5), 0x00FFFFFF);
        let h1 = s.hash();
        p.text_color(0);
        p.transparent();
        p.text("가", Rect::xywh(0, 10, 40, 10).win(), 0);
        assert_ne!(s.hash(), h1, "글자를 그리면 픽셀이 바뀐다");
        assert!(p.text_width("가나다") > p.text_width("가"));
    }
}
