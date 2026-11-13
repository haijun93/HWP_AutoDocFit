//! 가상 목록(보이는 줄만 그림)·스크롤 막대·휠 누적(순수 함수 상태 기계).
//!
//! [`ListState`] 전이표(len = 항목 수, rows = 한 화면 줄 수):
//! | 사건 | 결과 |
//! |---|---|
//! | `move_by(d)` | sel = clamp(sel+d, 0, len-1), sel이 화면 밖이면 top을 맞춤(popup·kvf `move_sel`과 같음) |
//! | `click(i, 수정키)` | 없음: sel=i·선택 {i}·기준점 i / Ctrl: i 넣고 빼기 / Shift: 기준점~i 범위 |
//! | `scroll_by(n)` | top만 옮김(선택 유지), 0..=len-rows |
//! | `set_len(n)` | sel·top을 새 길이 안으로 |
//! len = 0이면 이동·클릭은 아무것도 하지 않는다.

use std::collections::BTreeSet;

/// popup·kvf가 같이 쓰던 선택 이동: 새 (sel, top). len이 0이면 그대로.
pub fn move_selection(sel: usize, top: usize, len: usize, delta: isize, rows: usize) -> (usize, usize) {
    if len == 0 {
        return (sel, top);
    }
    let rows = rows.max(1);
    let sel = (sel as isize + delta).clamp(0, len as isize - 1) as usize;
    let top = if sel < top {
        sel
    } else if sel >= top + rows {
        sel + 1 - rows
    } else {
        top
    };
    (sel, top)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
}

/// 목록 상태(다중 선택 포함).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListState {
    pub len: usize,
    pub sel: usize,
    pub top: usize,
    pub rows: usize,
    /// 다중 선택(비어 있으면 sel 하나만 선택된 것으로 본다).
    pub selected: BTreeSet<usize>,
    anchor: usize,
}

impl ListState {
    pub fn new(len: usize, rows: usize) -> Self {
        Self { len, rows: rows.max(1), ..Default::default() }
    }
    pub fn set_rows(&mut self, rows: usize) {
        self.rows = rows.max(1);
        self.clamp_top();
    }
    pub fn set_len(&mut self, len: usize) {
        self.len = len;
        self.sel = self.sel.min(len.saturating_sub(1));
        self.selected.retain(|i| *i < len);
        self.anchor = self.anchor.min(len.saturating_sub(1));
        self.clamp_top();
        if self.sel < self.top {
            self.top = self.sel;
        }
    }
    fn max_top(&self) -> usize {
        self.len.saturating_sub(self.rows)
    }
    fn clamp_top(&mut self) {
        self.top = self.top.min(self.max_top());
    }
    pub fn move_by(&mut self, delta: isize, extend: bool) {
        if self.len == 0 {
            return;
        }
        let (sel, top) = move_selection(self.sel, self.top, self.len, delta, self.rows);
        self.sel = sel;
        self.top = top;
        if extend {
            self.selected = range(self.anchor, sel);
        } else {
            self.anchor = sel;
            self.selected.clear();
        }
    }
    pub fn page(&mut self, down: bool, extend: bool) {
        let d = self.rows as isize;
        self.move_by(if down { d } else { -d }, extend);
    }
    pub fn home_end(&mut self, end: bool, extend: bool) {
        self.move_by(if end { isize::MAX / 2 } else { isize::MIN / 2 }, extend);
    }
    pub fn click(&mut self, i: usize, m: Mods) {
        if i >= self.len {
            return;
        }
        if m.shift {
            self.selected = range(self.anchor, i);
        } else if m.ctrl {
            if self.selected.is_empty() {
                self.selected.insert(self.sel);
            }
            if !self.selected.remove(&i) {
                self.selected.insert(i);
            }
            self.anchor = i;
        } else {
            self.selected.clear();
            self.anchor = i;
        }
        self.sel = i;
        let (_, top) = move_selection(i, self.top, self.len, 0, self.rows);
        self.top = top;
    }
    pub fn scroll_by(&mut self, rows: isize) {
        self.top = (self.top as isize + rows).clamp(0, self.max_top() as isize) as usize;
    }
    pub fn scroll_to(&mut self, top: usize) {
        self.top = top.min(self.max_top());
    }
    pub fn is_selected(&self, i: usize) -> bool {
        if self.selected.is_empty() { i == self.sel && i < self.len } else { self.selected.contains(&i) }
    }
    /// 선택된 위치 목록(오름차순).
    pub fn selection(&self) -> Vec<usize> {
        if self.selected.is_empty() {
            if self.len > 0 { vec![self.sel] } else { vec![] }
        } else {
            self.selected.iter().copied().collect()
        }
    }
    /// 그릴 범위(top..끝).
    pub fn visible(&self) -> std::ops::Range<usize> {
        self.top..(self.top + self.rows).min(self.len)
    }
}

fn range(a: usize, b: usize) -> BTreeSet<usize> {
    (a.min(b)..=a.max(b)).collect()
}

/// 휠 누적: WHEEL_DELTA(120) 단위가 아닌 정밀 터치패드 값도 모아서 줄 수로 바꾼다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Wheel {
    rest: i32,
}

impl Wheel {
    /// delta(위로 +)를 더하고, 움직일 줄 수(아래로 +)를 돌려준다. lines = 한 칸당 줄 수(보통 3).
    pub fn add(&mut self, delta: i16, lines: i32) -> isize {
        self.rest += delta as i32 * lines;
        let n = self.rest / 120;
        self.rest -= n * 120;
        -(n as isize)
    }
}

/// 스크롤 막대(세로). 막대 길이 track, 손잡이 최소 길이 min_thumb.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scrollbar {
    pub len: usize,
    pub rows: usize,
    pub top: usize,
    pub track: i32,
    pub min_thumb: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollHit {
    PageUp,
    Thumb,
    PageDown,
}

impl Scrollbar {
    /// 스크롤이 필요한지.
    pub fn needed(&self) -> bool {
        self.len > self.rows && self.track > 0
    }
    /// 손잡이 (시작, 길이). 필요 없으면 None.
    pub fn thumb(&self) -> Option<(i32, i32)> {
        if !self.needed() {
            return None;
        }
        let size = ((self.track as i64 * self.rows as i64 / self.len as i64) as i32).max(self.min_thumb).min(self.track);
        let max_top = (self.len - self.rows) as i64;
        let room = (self.track - size) as i64;
        let start = (room * self.top.min(self.len - self.rows) as i64 / max_top.max(1)) as i32;
        Some((start, size))
    }
    pub fn hit(&self, y: i32) -> Option<ScrollHit> {
        let (s, n) = self.thumb()?;
        if y < 0 || y >= self.track {
            return None;
        }
        Some(if y < s { ScrollHit::PageUp } else if y < s + n { ScrollHit::Thumb } else { ScrollHit::PageDown })
    }
    /// 손잡이를 끌기 시작한 때의 top과 y 이동량 → 새 top.
    pub fn drag(&self, start_top: usize, dy: i32) -> usize {
        let Some((_, size)) = self.thumb() else { return 0 };
        let room = (self.track - size).max(1) as i64;
        let max_top = (self.len - self.rows) as i64;
        // 반올림(0에서 먼 쪽): dy·max_top/room.
        let t = start_top as i64 + (dy as i64 * max_top * 2 + room * dy.signum() as i64) / (2 * room);
        t.clamp(0, max_top) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_selection_matches_popup_and_kvf() {
        // popup: VISIBLE=12 고정.
        assert_eq!(move_selection(0, 0, 30, 1, 12), (1, 0));
        assert_eq!(move_selection(11, 0, 30, 1, 12), (12, 1));
        assert_eq!(move_selection(12, 1, 30, -12, 12), (0, 0));
        assert_eq!(move_selection(5, 0, 30, 100000, 12), (29, 18));
        assert_eq!(move_selection(5, 3, 30, -100000, 12), (0, 0));
        assert_eq!(move_selection(3, 2, 0, 1, 12), (3, 2), "빈 목록은 그대로");
        assert_eq!(move_selection(0, 0, 5, 1, 0), (1, 1), "줄 수 0은 1로");
    }

    #[test]
    fn list_keyboard_and_multi_select() {
        let mut l = ListState::new(100, 10);
        l.move_by(1, false);
        assert_eq!((l.sel, l.top), (1, 0));
        l.page(true, false);
        assert_eq!((l.sel, l.top), (11, 2));
        l.move_by(2, true);
        assert_eq!(l.selection(), vec![11, 12, 13]);
        l.click(20, Mods { ctrl: true, shift: false });
        assert_eq!(l.selection(), vec![11, 12, 13, 20]);
        l.click(12, Mods { ctrl: true, shift: false });
        assert_eq!(l.selection(), vec![11, 13, 20]);
        l.click(15, Mods { ctrl: false, shift: true });
        assert_eq!(l.selection(), vec![12, 13, 14, 15]);
        l.click(3, Mods::default());
        assert_eq!(l.selection(), vec![3]);
        assert!(l.is_selected(3) && !l.is_selected(4));
        l.home_end(true, false);
        assert_eq!((l.sel, l.top), (99, 90));
        assert_eq!(l.visible(), 90..100);
        l.scroll_by(-1000);
        assert_eq!((l.sel, l.top), (99, 0), "스크롤은 선택을 바꾸지 않음");
        l.set_len(5);
        assert_eq!((l.sel, l.top), (4, 0));
        l.set_len(0);
        assert!(l.selection().is_empty());
        l.move_by(1, false);
        l.click(0, Mods::default());
        assert_eq!(l.sel, 0);
    }

    #[test]
    fn wheel_accumulates_precision_deltas() {
        let mut w = Wheel::default();
        assert_eq!(w.add(120, 3), -3);
        assert_eq!(w.add(-120, 3), 3);
        assert_eq!(w.add(30, 1), 0);
        assert_eq!(w.add(30, 1), 0);
        assert_eq!(w.add(60, 1), -1);
        assert_eq!(w.add(-40, 3), 1);
    }

    #[test]
    fn scrollbar_geometry_hit_and_drag() {
        let sb = Scrollbar { len: 100, rows: 10, top: 0, track: 200, min_thumb: 16 };
        assert_eq!(sb.thumb(), Some((0, 20)));
        assert_eq!(Scrollbar { top: 90, ..sb }.thumb(), Some((180, 20)));
        assert_eq!(Scrollbar { top: 45, ..sb }.thumb(), Some((90, 20)));
        assert_eq!(Scrollbar { len: 10000, ..sb }.thumb().unwrap().1, 16, "최소 손잡이");
        assert_eq!(Scrollbar { len: 5, ..sb }.thumb(), None);
        let mid = Scrollbar { top: 45, ..sb };
        assert_eq!(mid.hit(10), Some(ScrollHit::PageUp));
        assert_eq!(mid.hit(95), Some(ScrollHit::Thumb));
        assert_eq!(mid.hit(150), Some(ScrollHit::PageDown));
        assert_eq!(mid.hit(200), None);
        assert_eq!(sb.drag(0, 90), 45);
        assert_eq!(sb.drag(0, 1000), 90);
        assert_eq!(sb.drag(45, -1000), 0);
    }
}
