//! 배치·가상화 계산(창과 무관한 순수 함수). 치수는 96 DPI 기준을 `dpi` 로 키운다.
//! 격자 규칙은 Flutter `SliverGridDelegateWithMaxCrossAxisExtent`(maxExtent=썸네일 크기, 간격 14, 가로세로비 0.74, 바깥 여백 16)와 같다.
use crate::state::Nav;
use crate::thumb::{MAX_THUMB, MIN_THUMB};
use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
        Rect { left, top, right, bottom }
    }
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
    pub fn overlaps(&self, o: &Rect) -> bool {
        self.left < o.right && o.left < self.right && self.top < o.bottom && o.top < self.bottom
    }
}

pub fn scale(v: i32, dpi: u32) -> i32 {
    (v as i64 * dpi as i64 / 96) as i32
}

pub const DEFAULT_W: i32 = 1100;
pub const DEFAULT_H: i32 = 720;
pub const MIN_W: i32 = 820;
pub const MIN_H: i32 = 480;

pub const TOOLBAR_H: i32 = 52;
pub const BANNER_H: i32 = 30;
pub const STATUS_H: i32 = 28;
pub const PAD: i32 = 16;
pub const GAP: i32 = 14;
pub const ASPECT: f64 = 0.74;

/// 격자 규칙(96 DPI 기준 간격·바깥 여백, 칸 가로세로비). 화면마다 Flutter 값과 같게 둔다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridSpec {
    pub gap: i32,
    pub pad: i32,
    pub aspect: f64,
}

impl GridSpec {
    /// 문서뷰: 간격 14·여백 16·비 0.74.
    pub const DOC: GridSpec = GridSpec { gap: GAP, pad: PAD, aspect: ASPECT };
    /// 사진뷰(`photo_album_screen.dart`): 간격 10·여백 16·비 0.8.
    pub const PHOTO: GridSpec = GridSpec { gap: 10, pad: PAD, aspect: 0.8 };
}
pub const SCROLLBAR_W: i32 = 12;

fn ceil_div(a: i32, b: i32) -> i32 {
    if a <= 0 { 0 } else { (a + b - 1) / b }
}

/// 격자 한 장면의 치수(창 크기·dpi·썸네일 크기·스캔 띠 유무로 정해진다).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metrics {
    pub dpi: u32,
    /// 격자가 그려지는 창 좌표 영역(스크롤바 포함).
    pub viewport: Rect,
    pub cols: usize,
    pub card_w: i32,
    pub card_h: i32,
    pub gap: i32,
    pub pad: i32,
}

impl Metrics {
    /// 문서뷰 규칙(`GridSpec::DOC`).
    pub fn new(w: i32, h: i32, dpi: u32, thumb: f64, banner: bool) -> Metrics {
        Metrics::with_spec(GridSpec::DOC, w, h, dpi, thumb, banner)
    }

    pub fn with_spec(spec: GridSpec, w: i32, h: i32, dpi: u32, thumb: f64, banner: bool) -> Metrics {
        let top = scale(TOOLBAR_H, dpi) + if banner { scale(BANNER_H, dpi) } else { 0 };
        let bottom = (h - scale(STATUS_H, dpi)).max(top + 1);
        let viewport = Rect::new(0, top, w.max(1), bottom);
        let gap = scale(spec.gap, dpi);
        let pad = scale(spec.pad, dpi);
        let avail = (viewport.width() - scale(SCROLLBAR_W, dpi) - 2 * pad).max(1);
        let max_extent = (thumb.clamp(MIN_THUMB, MAX_THUMB) * dpi as f64 / 96.0).round() as i32;
        let cols = ceil_div(avail, (max_extent + gap).max(1)).max(1) as usize;
        let card_w = ((avail - gap * (cols as i32 - 1)) / cols as i32).max(8);
        let card_h = (card_w as f64 / spec.aspect).round() as i32;
        Metrics { dpi, viewport, cols, card_w, card_h, gap, pad }
    }

    pub fn rows(&self, n: usize) -> usize {
        n.div_ceil(self.cols)
    }
    pub fn stride(&self) -> i32 {
        self.card_h + self.gap
    }
    /// 내용 좌표(스크롤 0 기준) 행 위 끝.
    pub fn row_top(&self, row: usize) -> i32 {
        self.pad + row as i32 * self.stride()
    }
    pub fn content_height(&self, n: usize) -> i32 {
        let rows = self.rows(n) as i32;
        if rows == 0 { 0 } else { 2 * self.pad + rows * self.card_h + (rows - 1) * self.gap }
    }
    pub fn max_scroll(&self, n: usize) -> i32 {
        (self.content_height(n) - self.viewport.height()).max(0)
    }
    /// 화면에 조금이라도 걸리는 항목 번호 범위(그릴 칸만).
    pub fn visible_range(&self, scroll: i32, n: usize) -> Range<usize> {
        if n == 0 {
            return 0..0;
        }
        let first_row = if scroll <= self.pad + self.card_h { 0 } else { ((scroll - self.pad - self.card_h) / self.stride() + 1) as usize };
        let end_row = ceil_div(scroll + self.viewport.height() - self.pad, self.stride()) as usize;
        let start = (first_row * self.cols).min(n);
        let end = (end_row * self.cols).min(n).max(start);
        start..end
    }
    pub fn first_visible(&self, scroll: i32, n: usize) -> usize {
        self.visible_range(scroll, n).start
    }
    /// 창 좌표 항목 칸.
    pub fn item_rect(&self, i: usize, scroll: i32) -> Rect {
        let (row, col) = (i / self.cols, i % self.cols);
        let x = self.viewport.left + self.pad + col as i32 * (self.card_w + self.gap);
        let y = self.viewport.top + self.row_top(row) - scroll;
        Rect::new(x, y, x + self.card_w, y + self.card_h)
    }
    pub fn hit(&self, x: i32, y: i32, scroll: i32, n: usize) -> Option<usize> {
        if !self.viewport.contains(x, y) || x >= self.viewport.right - scale(SCROLLBAR_W, self.dpi) {
            return None;
        }
        let (cx, cy) = (x - self.viewport.left - self.pad, y - self.viewport.top + scroll - self.pad);
        if cx < 0 || cy < 0 {
            return None;
        }
        let (col, row) = ((cx / (self.card_w + self.gap)) as usize, (cy / self.stride()) as usize);
        if cx % (self.card_w + self.gap) >= self.card_w || cy % self.stride() >= self.card_h || col >= self.cols {
            return None;
        }
        let i = row * self.cols + col;
        (i < n).then_some(i)
    }
    /// 항목이 완전히 보이도록 한 최소 스크롤.
    pub fn scroll_to_show(&self, i: usize, scroll: i32, n: usize) -> i32 {
        let row = i / self.cols;
        let (top, bottom) = (self.row_top(row), self.row_top(row) + self.card_h);
        let vh = self.viewport.height();
        let s = if top - self.pad < scroll {
            top - if row == 0 { self.pad } else { self.gap }
        } else if bottom + self.pad > scroll + vh {
            bottom + self.pad - vh
        } else {
            scroll
        };
        s.clamp(0, self.max_scroll(n))
    }
    pub fn rows_per_page(&self) -> usize {
        ((self.viewport.height() / self.stride()).max(1)) as usize
    }
    /// 방향키·Home/End·PageUp/Down 이동 결과(선택이 없으면 첫 칸, End 는 마지막 칸).
    pub fn navigate(&self, sel: Option<usize>, key: Nav, n: usize) -> Option<usize> {
        if n == 0 {
            return None;
        }
        let Some(s) = sel else {
            return Some(if key == Nav::End { n - 1 } else { 0 });
        };
        let c = self.cols;
        let page = self.rows_per_page() * c;
        Some(match key {
            Nav::Left => s.saturating_sub(1),
            Nav::Right => (s + 1).min(n - 1),
            Nav::Up => if s >= c { s - c } else { s },
            Nav::Down => {
                if s + c < n {
                    s + c
                } else if s / c < (n - 1) / c {
                    n - 1
                } else {
                    s
                }
            }
            Nav::Home => 0,
            Nav::End => n - 1,
            Nav::PageUp => s.saturating_sub(page),
            Nav::PageDown => (s + page).min(n - 1),
        })
    }

    pub fn scrollbar_track(&self) -> Rect {
        let v = self.viewport;
        Rect::new(v.right - scale(SCROLLBAR_W, self.dpi), v.top, v.right, v.bottom)
    }
    /// 스크롤할 내용이 있을 때만 막대 손잡이 칸.
    pub fn scrollbar_thumb(&self, scroll: i32, n: usize) -> Option<Rect> {
        let max = self.max_scroll(n);
        if max <= 0 {
            return None;
        }
        let t = self.scrollbar_track();
        let (vh, ch) = (t.height(), self.content_height(n));
        let th = ((vh as i64 * vh as i64 / ch as i64) as i32).max(scale(28, self.dpi)).min(vh);
        let y = t.top + ((vh - th) as i64 * scroll.clamp(0, max) as i64 / max as i64) as i32;
        Some(Rect::new(t.left + 2, y, t.right - 2, y + th))
    }
    /// 손잡이 윗변이 `thumb_top`(창 좌표)일 때의 스크롤.
    pub fn scroll_from_thumb_top(&self, thumb_top: i32, n: usize) -> i32 {
        let max = self.max_scroll(n);
        let t = self.scrollbar_track();
        let vh = t.height();
        let ch = self.content_height(n).max(1);
        let th = ((vh as i64 * vh as i64 / ch as i64) as i32).max(scale(28, self.dpi)).min(vh);
        let room = (vh - th).max(1);
        ((thumb_top - t.top).clamp(0, room) as i64 * max as i64 / room as i64) as i32
    }
}

/// 상단 도구줄 칸 배치.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Toolbar {
    pub bar: Rect,
    pub search: Rect,
    pub sort: Rect,
    pub count: Rect,
    pub slider_label: Rect,
    pub slider: Rect,
    pub exclude: Rect,
    pub action: Rect,
}

impl Toolbar {
    pub fn new(w: i32, dpi: u32) -> Toolbar {
        let s = |v: i32| scale(v, dpi);
        let (top, bot) = (s(10), s(TOOLBAR_H - 10));
        let bar = Rect::new(0, 0, w, s(TOOLBAR_H));
        let action = Rect::new(w - s(16 + 92), top, w - s(16), bot);
        let exclude = Rect::new(action.left - s(8 + 120), top, action.left - s(8), bot);
        let slider = Rect::new(exclude.left - s(12 + 120), top, exclude.left - s(12), bot);
        let slider_label = Rect::new(slider.left - s(6 + 32), top, slider.left - s(6), bot);
        let left_avail = slider_label.left - s(12) - s(16);
        let search_w = (left_avail - s(120 + 8 + 124 + 16)).clamp(s(100), s(280));
        let search = Rect::new(s(16), top, s(16) + search_w, bot);
        let sort = Rect::new(search.right + s(8), top, search.right + s(8 + 120), bot);
        let count = Rect::new(sort.right + s(8), top, sort.right + s(8 + 124), bot);
        Toolbar { bar, search, sort, count, slider_label, slider, exclude, action }
    }
    fn track(&self) -> (i32, i32) {
        let inset = self.slider.height() / 2;
        (self.slider.left + inset / 2, self.slider.right - inset / 2)
    }
    /// 슬라이더 값 → 손잡이 중심 x.
    pub fn slider_x(&self, value: f64) -> i32 {
        let (l, r) = self.track();
        let t = ((value.clamp(MIN_THUMB, MAX_THUMB) - MIN_THUMB) / (MAX_THUMB - MIN_THUMB)).clamp(0.0, 1.0);
        l + ((r - l) as f64 * t).round() as i32
    }
    /// 누른 x → 슬라이더 값(정수로 맞춤).
    pub fn slider_value(&self, x: i32) -> f64 {
        let (l, r) = self.track();
        let t = ((x - l) as f64 / (r - l).max(1) as f64).clamp(0.0, 1.0);
        (MIN_THUMB + (MAX_THUMB - MIN_THUMB) * t).round()
    }
}
