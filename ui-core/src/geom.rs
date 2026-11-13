//! 좌표·사각형·배율(순수 함수). Win32 `RECT`와 오갈 수 있다.

use windows_sys::Win32::Foundation::RECT;

/// 창 안 사각형(왼쪽·위 포함, 오른쪽·아래 제외).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self { left, top, right, bottom }
    }
    pub const fn xywh(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { left: x, top: y, right: x + w, bottom: y + h }
    }
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
    pub fn is_empty(&self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
    /// 안쪽으로 줄인 사각형(음수가 되지 않게).
    pub fn inset(&self, i: Insets) -> Self {
        let left = self.left + i.left;
        let top = self.top + i.top;
        Self { left, top, right: (self.right - i.right).max(left), bottom: (self.bottom - i.bottom).max(top) }
    }
    pub fn offset(&self, dx: i32, dy: i32) -> Self {
        Self { left: self.left + dx, top: self.top + dy, right: self.right + dx, bottom: self.bottom + dy }
    }
    pub fn intersect(&self, o: &Rect) -> Rect {
        let r = Rect {
            left: self.left.max(o.left),
            top: self.top.max(o.top),
            right: self.right.min(o.right),
            bottom: self.bottom.min(o.bottom),
        };
        if r.is_empty() { Rect::default() } else { r }
    }
    pub fn win(&self) -> RECT {
        RECT { left: self.left, top: self.top, right: self.right, bottom: self.bottom }
    }
    pub fn from_win(r: &RECT) -> Self {
        Self { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
    }
}

/// 여백(왼·위·오·아래).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Insets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Insets {
    pub const fn all(v: i32) -> Self {
        Self { left: v, top: v, right: v, bottom: v }
    }
    pub const fn hv(h: i32, v: i32) -> Self {
        Self { left: h, top: v, right: h, bottom: v }
    }
    pub fn scaled(&self, dpi: u32) -> Self {
        Self { left: scale(self.left, dpi), top: scale(self.top, dpi), right: scale(self.right, dpi), bottom: scale(self.bottom, dpi) }
    }
}

/// 96 기준 논리 px → 실제 px(정수 나눗셈, popup·kvf의 `v * dpi / 96`과 같음).
pub fn scale(v: i32, dpi: u32) -> i32 {
    v * dpi.max(96) as i32 / 96
}

/// 실제 px → 96 기준 논리 px(설정 저장용, popup의 `px * 96 / dpi`와 같음).
pub fn unscale(px: i64, dpi: u32) -> i64 {
    px * 96 / dpi.max(96) as i64
}

/// LPARAM의 부호 있는 x·y(GET_X_LPARAM·GET_Y_LPARAM).
pub fn xy(lp: isize) -> (i32, i32) {
    ((lp & 0xFFFF) as u16 as i16 as i32, ((lp >> 16) & 0xFFFF) as u16 as i16 as i32)
}

/// 휠 WPARAM의 회전량(HIWORD, 부호 있음).
pub fn wheel_delta(wp: usize) -> i16 {
    ((wp >> 16) & 0xFFFF) as u16 as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xy_handles_negative_multi_monitor_coordinates() {
        assert_eq!(xy(0x0010_0020), (0x20, 0x10));
        let lp = ((-5i16 as u16 as isize) << 16) | (-7i16 as u16 as isize);
        assert_eq!(xy(lp), (-7, -5));
        assert_eq!(wheel_delta(120 << 16), 120);
        assert_eq!(wheel_delta(((-120i16) as u16 as usize) << 16), -120);
    }

    #[test]
    fn scale_matches_popup_integer_math() {
        for dpi in [96u32, 120, 144, 192] {
            for v in [6, 30, 46, 440] {
                assert_eq!(scale(v, dpi), v * dpi as i32 / 96);
            }
        }
        assert_eq!(scale(10, 0), 10, "DPI 0은 96으로");
        assert_eq!(unscale(660, 144), 440);
    }

    #[test]
    fn rect_ops() {
        let r = Rect::xywh(10, 20, 100, 50);
        assert_eq!((r.width(), r.height()), (100, 50));
        assert!(r.contains(10, 20) && !r.contains(110, 20));
        assert_eq!(r.inset(Insets::all(5)), Rect::new(15, 25, 105, 65));
        assert_eq!(r.inset(Insets::all(80)).width(), 0);
        assert_eq!(r.intersect(&Rect::new(0, 0, 20, 30)), Rect::new(10, 20, 20, 30));
        assert!(r.intersect(&Rect::new(0, 0, 5, 5)).is_empty());
        assert_eq!(Rect::from_win(&r.win()), r);
    }
}
