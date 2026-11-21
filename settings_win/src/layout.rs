//! 화면 배치 계산(창과 무관한 순수 함수). 치수는 96 DPI 기준을 `dpi` 로 키운다.

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
    pub fn inside(&self, o: &Rect) -> bool {
        self.left >= o.left && self.right <= o.right && self.top >= o.top && self.bottom <= o.bottom
    }
}

pub fn scale(v: i32, dpi: u32) -> i32 {
    (v as i64 * dpi as i64 / 96) as i32
}

/// 기본 창 크기(96 DPI).
pub const DEFAULT_W: i32 = 1000;
pub const DEFAULT_H: i32 = 680;
pub const MIN_W: i32 = 760;
pub const MIN_H: i32 = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub dpi: u32,
    pub w: i32,
    pub h: i32,
    /// 제목·검색 상자가 있는 위쪽 띠.
    pub header: Rect,
    pub search: Rect,
    pub tree: Rect,
    pub list: Rect,
    pub footer: Rect,
    pub save_btn: Rect,
    pub revert_btn: Rect,
    /// 항목 한 줄 높이(안내문 2줄 + 오류/꼬리표 칸 포함).
    pub row_h: i32,
    pub tree_row_h: i32,
}

pub fn compute(w: i32, h: i32, dpi: u32) -> Layout {
    let s = |v: i32| scale(v, dpi);
    let header = Rect::new(0, 0, w, s(56));
    let footer = Rect::new(0, h - s(56), w, h);
    let tree_w = s(240);
    let tree = Rect::new(0, header.bottom, tree_w, footer.top);
    let list = Rect::new(tree_w, header.bottom, w, footer.top);
    let search = Rect::new(tree_w + s(16), s(11), w - s(16), s(45));
    // 트리 줄 높이: 묶음 수에 맞춰 줄이되 96 DPI 48 화소를 넘지 않는다(작은 창에서도 트리가 잘리지 않게).
    let tree_row_h = ((tree.height() - s(8)) / crate::schema::GROUPS.len() as i32).min(s(48));
    let btn_h = s(36);
    let btn_top = footer.top + (footer.height() - btn_h) / 2;
    let save_btn = Rect::new(w - s(16) - s(104), btn_top, w - s(16), btn_top + btn_h);
    let revert_btn = Rect::new(save_btn.left - s(10) - s(116), btn_top, save_btn.left - s(10), btn_top + btn_h);
    Layout { dpi, w, h, header, search, tree, list, footer, save_btn, revert_btn, row_h: s(98), tree_row_h }
}

impl Layout {
    fn s(&self, v: i32) -> i32 {
        scale(v, self.dpi)
    }

    /// 묶음 `i` 줄(트리).
    pub fn tree_row(&self, i: usize) -> Rect {
        let top = self.tree.top + self.s(8) + i as i32 * self.tree_row_h;
        Rect::new(self.tree.left + self.s(8), top, self.tree.right - self.s(8), top + self.tree_row_h - self.s(4))
    }

    /// 목록의 `n` 번째 줄(스크롤 `scroll` 화소 적용). 화면 밖이어도 값은 돌려준다.
    pub fn row_rect(&self, n: usize, scroll: i32) -> Rect {
        let top = self.list.top + self.s(8) + n as i32 * self.row_h - scroll;
        Rect::new(self.list.left + self.s(12), top, self.list.right - self.s(16), top + self.row_h - self.s(6))
    }

    pub fn content_height(&self, rows: usize) -> i32 {
        self.s(8) + rows as i32 * self.row_h + self.s(8)
    }

    pub fn max_scroll(&self, rows: usize) -> i32 {
        (self.content_height(rows) - self.list.height()).max(0)
    }

    pub fn clamp_scroll(&self, scroll: i32, rows: usize) -> i32 {
        scroll.clamp(0, self.max_scroll(rows))
    }

    /// `n` 번째 줄이 보이도록 스크롤을 조정한다.
    pub fn scroll_to_show(&self, n: usize, scroll: i32, rows: usize) -> i32 {
        let top = self.s(8) + n as i32 * self.row_h;
        let bottom = top + self.row_h;
        let mut s = scroll;
        if top < s {
            s = top - self.s(8);
        }
        if bottom > s + self.list.height() {
            s = bottom - self.list.height() + self.s(8);
        }
        self.clamp_scroll(s, rows)
    }

    /// 줄 안 요소 위치.
    pub fn row_parts(&self, row: Rect, shape: Shape) -> RowParts {
        let s = |v: i32| self.s(v);
        let ctrl_w = match shape {
            Shape::Check => s(120),
            Shape::Number => s(150),
            Shape::Choice => s(210),
            Shape::Text | Shape::ReadOnly => s(250),
        };
        let top = row.top + s(10);
        let right = row.right - s(14);
        let control = Rect::new(right - ctrl_w, top, right, top + s(32));
        let (minus, plus, field) = if shape == Shape::Number {
            let b = s(32);
            let plus = Rect::new(control.right - b, control.top, control.right, control.bottom);
            let minus = Rect::new(plus.left - s(4) - b, control.top, plus.left - s(4), control.bottom);
            (minus, plus, Rect::new(control.left, control.top, minus.left - s(6), control.bottom))
        } else {
            (Rect::default(), Rect::default(), control)
        };
        let chip = Rect::new(control.left, control.bottom + s(8), control.right, control.bottom + s(8) + s(24));
        let dot = Rect::new(row.left + s(8), row.top + s(14), row.left + s(8) + s(10), row.top + s(14) + s(10));
        let text_left = row.left + s(26);
        let text_right = control.left - s(16);
        let label = Rect::new(text_left, row.top + s(6), text_right, row.top + s(6) + s(22));
        let summary = Rect::new(text_left, label.bottom, text_right, label.bottom + s(36));
        let extra = Rect::new(text_left, summary.bottom + s(4), text_right, summary.bottom + s(4) + s(24));
        RowParts { dot, label, summary, extra, control, field, minus, plus, chip }
    }

    pub fn hit(&self, x: i32, y: i32, scroll: i32, rows: usize) -> Hit {
        if self.save_btn.contains(x, y) {
            return Hit::Save;
        }
        if self.revert_btn.contains(x, y) {
            return Hit::Revert;
        }
        if self.search.contains(x, y) {
            return Hit::Search;
        }
        if self.tree.contains(x, y) {
            for i in 0..crate::schema::GROUPS.len() {
                if self.tree_row(i).contains(x, y) {
                    return Hit::Tree(i);
                }
            }
            return Hit::None;
        }
        if self.list.contains(x, y) {
            for n in 0..rows {
                if self.row_rect(n, scroll).contains(x, y) {
                    return Hit::Row(n);
                }
            }
        }
        Hit::None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Check,
    Number,
    Text,
    Choice,
    ReadOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowParts {
    pub dot: Rect,
    pub label: Rect,
    pub summary: Rect,
    pub extra: Rect,
    pub control: Rect,
    pub field: Rect,
    pub minus: Rect,
    pub plus: Rect,
    pub chip: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    None,
    Search,
    Tree(usize),
    Row(usize),
    Save,
    Revert,
}
