//! 그림 미리보기(맞춤·확대·끌어 이동, 순수 함수) + GDI 축소 그리기.
//! S3 옵션 그림·S12 타임머신 썸네일·S14 표 칸 그림이 같은 부품을 쓴다. 그림 해독(WIC)은 kagent `thumb.rs`에 이미 있어
//! 화면 단계(UI3)에서 공용으로 올린다.

use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, HALFTONE, HBITMAP, HDC, SRCCOPY, SelectObject, SetBrushOrgEx, SetStretchBltMode,
    StretchBlt,
};

use crate::geom::Rect;

/// 그림(sw×sh)을 상자 안에 비율 유지로 맞춘 사각형(가운데). upscale=false면 원래보다 키우지 않는다.
pub fn fit(sw: i32, sh: i32, area: Rect, upscale: bool) -> Rect {
    if sw <= 0 || sh <= 0 || area.is_empty() {
        return Rect::new(area.left, area.top, area.left, area.top);
    }
    let (aw, ah) = (area.width() as i64, area.height() as i64);
    let (mut w, mut h) = if aw * sh as i64 <= ah * sw as i64 {
        (aw, aw * sh as i64 / sw as i64)
    } else {
        (ah * sw as i64 / sh as i64, ah)
    };
    if !upscale && (w > sw as i64 || h > sh as i64) {
        w = sw as i64;
        h = sh as i64;
    }
    let left = area.left + ((aw - w) / 2) as i32;
    let top = area.top + ((ah - h) / 2) as i32;
    Rect::xywh(left, top, w as i32, h as i32)
}

/// 확대·이동 상태(맞춤 기준 배율 1.0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zoom {
    pub scale: f64,
    pub pan_x: f64,
    pub pan_y: f64,
}

impl Default for Zoom {
    fn default() -> Self {
        Self { scale: 1.0, pan_x: 0.0, pan_y: 0.0 }
    }
}

impl Zoom {
    pub const MIN: f64 = 1.0;
    pub const MAX: f64 = 16.0;

    /// (cx, cy)(상자 안 좌표)를 중심으로 factor배. 그 점 아래 그림 위치가 그대로 남는다.
    pub fn zoom_at(&mut self, cx: f64, cy: f64, factor: f64) {
        let next = (self.scale * factor).clamp(Self::MIN, Self::MAX);
        let k = next / self.scale;
        self.pan_x = cx - (cx - self.pan_x) * k;
        self.pan_y = cy - (cy - self.pan_y) * k;
        self.scale = next;
    }
    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.pan_x += dx;
        self.pan_y += dy;
    }
    /// 그림이 상자(w×h, 맞춤 크기 fw×fh 기준)를 벗어나 빈 곳이 생기지 않게 이동을 제한한다.
    pub fn clamp(&mut self, area_w: f64, area_h: f64, fit_w: f64, fit_h: f64) {
        let lim = |pan: f64, area: f64, size: f64| {
            let s = size;
            if s <= area { (area - s) / 2.0 } else { pan.clamp(area - s, 0.0) }
        };
        self.pan_x = lim(self.pan_x, area_w, fit_w * self.scale);
        self.pan_y = lim(self.pan_y, area_h, fit_h * self.scale);
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// HBITMAP(sw×sh)을 dst에 축소·확대해 그린다(HALFTONE).
///
/// # Safety
/// `dc`·`bmp`는 유효해야 한다.
pub unsafe fn draw_bitmap(dc: HDC, bmp: HBITMAP, sw: i32, sh: i32, dst: Rect) {
    unsafe {
        let src = CreateCompatibleDC(dc);
        if src.is_null() {
            return;
        }
        let old = SelectObject(src, bmp);
        SetStretchBltMode(dc, HALFTONE);
        SetBrushOrgEx(dc, 0, 0, std::ptr::null_mut());
        StretchBlt(dc, dst.left, dst.top, dst.width(), dst.height(), src, 0, 0, sw, sh, SRCCOPY);
        SelectObject(src, old);
        DeleteDC(src);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_contains_and_centers() {
        let area = Rect::xywh(0, 0, 200, 100);
        assert_eq!(fit(400, 100, area, true), Rect::xywh(0, 25, 200, 50));
        assert_eq!(fit(100, 400, area, true), Rect::xywh(87, 0, 25, 100));
        assert_eq!(fit(50, 20, area, false), Rect::xywh(75, 40, 50, 20), "작은 그림은 키우지 않음");
        assert_eq!(fit(50, 20, area, true), Rect::xywh(0, 10, 200, 80));
        assert_eq!(fit(0, 20, area, true).width(), 0);
    }

    #[test]
    fn zoom_keeps_point_under_cursor_and_clamps() {
        let mut z = Zoom::default();
        z.zoom_at(50.0, 50.0, 2.0);
        assert_eq!((z.scale, z.pan_x, z.pan_y), (2.0, -50.0, -50.0));
        // 커서 아래 그림 위치 (50 - pan)/scale = 50 그대로.
        assert_eq!((50.0 - z.pan_x) / z.scale, 50.0);
        z.zoom_at(0.0, 0.0, 100.0);
        assert_eq!(z.scale, Zoom::MAX);
        z.zoom_at(0.0, 0.0, 0.0001);
        assert_eq!(z.scale, Zoom::MIN);
        let mut z = Zoom { scale: 2.0, pan_x: 30.0, pan_y: -500.0 };
        z.clamp(100.0, 100.0, 100.0, 40.0);
        assert_eq!(z.pan_x, 0.0, "왼쪽 빈 곳 없음");
        assert_eq!(z.pan_y, 10.0, "작으면 가운데");
        z.pan(-1000.0, 0.0);
        z.clamp(100.0, 100.0, 100.0, 40.0);
        assert_eq!(z.pan_x, -100.0);
        z.reset();
        assert_eq!(z, Zoom::default());
    }

    #[test]
    fn draw_bitmap_scales_pixels() {
        let src = crate::paint::Offscreen::new(4, 4).unwrap();
        src.painter().fill_rect(Rect::xywh(0, 0, 4, 4), 0x000000FF);
        let dst = crate::paint::Offscreen::new(16, 16).unwrap();
        dst.painter().fill_rect(Rect::xywh(0, 0, 16, 16), 0x00FFFFFF);
        unsafe {
            let bmp = windows_sys::Win32::Graphics::Gdi::GetCurrentObject(src.dc, windows_sys::Win32::Graphics::Gdi::OBJ_BITMAP as u32);
            // 선택된 비트맵을 잠시 빼서 그린다(같은 비트맵을 두 DC에 동시에 선택할 수 없음).
            let tmp = windows_sys::Win32::Graphics::Gdi::CreateCompatibleBitmap(src.dc, 1, 1);
            SelectObject(src.dc, tmp);
            draw_bitmap(dst.dc, bmp as HBITMAP, 4, 4, Rect::xywh(4, 4, 8, 8));
            SelectObject(src.dc, bmp);
            windows_sys::Win32::Graphics::Gdi::DeleteObject(tmp);
        }
        assert_eq!(dst.pixel(8, 8), 0x000000FF);
        assert_eq!(dst.pixel(1, 1), 0x00FFFFFF);
    }
}
