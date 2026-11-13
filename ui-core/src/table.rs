//! 표(순수 상태 기계): 머리줄·열 너비 끌기·정렬·가상 줄(목록은 [`crate::list::ListState`]).
//! 칸 안 편집은 OS EDIT 자식을 그 칸 위에 띄운다(Q14) — [`Table::cell_rect`].
//!
//! | 사건 | 결과 |
//! |---|---|
//! | 머리줄 클릭(정렬 가능 열) | 같은 열이면 오름↔내림, 다른 열이면 그 열 오름차순 |
//! | 경계 누름 → 끌기 → 뗌 | 그 열 너비 = 시작 너비 + 이동량(최소 너비 이상) |

use std::cmp::Ordering;

use crate::geom::Rect;
use crate::list::ListState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    pub title: String,
    pub width: i32,
    pub min: i32,
    pub align: Align,
    pub sortable: bool,
}

impl Column {
    pub fn new(title: impl Into<String>, width: i32) -> Self {
        Self { title: title.into(), width, min: 24, align: Align::Left, sortable: true }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderHit {
    Column(usize),
    /// i번째 열의 오른쪽 경계(너비 끌기).
    Divider(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub columns: Vec<Column>,
    pub sort: Option<(usize, bool)>,
    pub list: ListState,
    /// 끌기 중: (열, 시작 x, 시작 너비).
    resizing: Option<(usize, i32, i32)>,
    /// 가로 스크롤(px).
    pub scroll_x: i32,
}

impl Table {
    pub fn new(columns: Vec<Column>, rows: usize, visible_rows: usize) -> Self {
        Self { columns, sort: None, list: ListState::new(rows, visible_rows), resizing: None, scroll_x: 0 }
    }
    /// 각 열의 (왼쪽, 오른쪽)(창 x, 가로 스크롤 반영).
    pub fn spans(&self, x0: i32) -> Vec<(i32, i32)> {
        let mut x = x0 - self.scroll_x;
        self.columns.iter().map(|c| { let l = x; x += c.width; (l, x) }).collect()
    }
    pub fn total_width(&self) -> i32 {
        self.columns.iter().map(|c| c.width).sum()
    }
    /// 머리줄 x가 가리키는 곳. 경계 ±grip은 끌기.
    pub fn header_hit(&self, x0: i32, x: i32, grip: i32) -> Option<HeaderHit> {
        let spans = self.spans(x0);
        if let Some(i) = spans.iter().position(|(_, r)| (x - r).abs() <= grip) {
            return Some(HeaderHit::Divider(i));
        }
        spans.iter().position(|(l, r)| x >= *l && x < *r).map(HeaderHit::Column)
    }
    /// 머리줄 클릭으로 정렬. 바뀌면 true.
    pub fn click_header(&mut self, col: usize) -> bool {
        if !self.columns.get(col).is_some_and(|c| c.sortable) {
            return false;
        }
        self.sort = Some(match self.sort {
            Some((c, asc)) if c == col => (c, !asc),
            _ => (col, true),
        });
        true
    }
    pub fn begin_resize(&mut self, col: usize, x: i32) {
        if let Some(c) = self.columns.get(col) {
            self.resizing = Some((col, x, c.width));
        }
    }
    /// 끌기 중이면 너비를 바꾸고 true.
    pub fn drag(&mut self, x: i32) -> bool {
        let Some((col, sx, sw)) = self.resizing else { return false };
        let c = &mut self.columns[col];
        let w = (sw + x - sx).max(c.min);
        let changed = w != c.width;
        c.width = w;
        changed
    }
    pub fn end_resize(&mut self) -> bool {
        self.resizing.take().is_some()
    }
    pub fn resizing(&self) -> bool {
        self.resizing.is_some()
    }
    /// 칸 사각형(편집 EDIT 위치). row = 전체 줄 위치, 화면 밖이면 None.
    pub fn cell_rect(&self, x0: i32, y0: i32, row_h: i32, row: usize, col: usize) -> Option<Rect> {
        if !self.list.visible().contains(&row) {
            return None;
        }
        let (l, r) = *self.spans(x0).get(col)?;
        let top = y0 + (row - self.list.top) as i32 * row_h;
        Some(Rect::new(l, top, r, top + row_h))
    }
}

/// 정렬된 줄 순서(안정 정렬). key(a, b, col)로 비교한다.
pub fn sorted_order<F: Fn(usize, usize, usize) -> Ordering>(rows: usize, sort: Option<(usize, bool)>, cmp: F) -> Vec<usize> {
    let mut v: Vec<usize> = (0..rows).collect();
    if let Some((col, asc)) = sort {
        v.sort_by(|a, b| {
            let o = cmp(*a, *b, col);
            if asc { o } else { o.reverse() }
        });
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Table {
        let mut t = Table::new(vec![Column::new("현재 이름", 200), Column::new("새 이름", 200), Column::new("상태", 80)], 50, 10);
        t.columns[2].sortable = false;
        t
    }

    #[test]
    fn header_hits_sort_and_resize() {
        let mut t = table();
        assert_eq!(t.header_hit(0, 100, 3), Some(HeaderHit::Column(0)));
        assert_eq!(t.header_hit(0, 198, 3), Some(HeaderHit::Divider(0)));
        assert_eq!(t.header_hit(0, 300, 3), Some(HeaderHit::Column(1)));
        assert_eq!(t.header_hit(0, 999, 3), None);
        assert!(t.click_header(1));
        assert_eq!(t.sort, Some((1, true)));
        assert!(t.click_header(1));
        assert_eq!(t.sort, Some((1, false)));
        assert!(t.click_header(0));
        assert_eq!(t.sort, Some((0, true)));
        assert!(!t.click_header(2), "정렬 안 되는 열");
        t.begin_resize(0, 200);
        assert!(t.drag(250));
        assert_eq!(t.columns[0].width, 250);
        assert!(t.drag(-500));
        assert_eq!(t.columns[0].width, 24, "최소 너비");
        assert!(t.end_resize());
        assert!(!t.drag(300));
        assert_eq!(t.total_width(), 24 + 200 + 80);
    }

    #[test]
    fn cell_rect_follows_scroll() {
        let mut t = table();
        assert_eq!(t.cell_rect(10, 40, 24, 2, 1), Some(Rect::new(210, 88, 410, 112)));
        t.list.scroll_by(5);
        assert_eq!(t.cell_rect(10, 40, 24, 2, 1), None);
        assert_eq!(t.cell_rect(10, 40, 24, 5, 0), Some(Rect::new(10, 40, 210, 64)));
        t.scroll_x = 50;
        assert_eq!(t.cell_rect(10, 40, 24, 5, 0).unwrap().left, -40);
        assert_eq!(t.cell_rect(10, 40, 24, 5, 9), None);
    }

    #[test]
    fn sorting_is_stable_and_reversible() {
        let names = ["나", "가", "다", "가"];
        let cmp = |a: usize, b: usize, _c: usize| names[a].cmp(names[b]);
        assert_eq!(sorted_order(4, None, cmp), vec![0, 1, 2, 3]);
        assert_eq!(sorted_order(4, Some((0, true)), cmp), vec![1, 3, 0, 2]);
        assert_eq!(sorted_order(4, Some((0, false)), cmp), vec![2, 0, 1, 3]);
    }
}
