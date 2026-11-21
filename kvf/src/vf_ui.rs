//! 퀵가상폴더 화면 배치 순수 계산. 제목줄·주소줄·도구 막대·탭·영역·목록 줄·상태줄의 사각형을 한 곳에서 정하고,
//! 그리기(vf_paint)와 클릭 판정(window)은 모두 이 결과만 쓴다. Win32 타입을 쓰지 않는다.
use crate::vf_layout::{Layout, Rect};

pub const HEAD: i32 = 32;
pub const TAB: i32 = 30;
pub const ROW: i32 = 30;
pub const ADDRESS: i32 = 36;
pub const TOOLBAR: i32 = 34;
pub const STATUS: i32 = 26;
pub const GUTTER: i32 = 6;
pub const PAD: i32 = 8;
pub const WIN_BTN: i32 = 40;
pub const CELL_W: i32 = 120;
pub const CELL_H: i32 = 100;
pub const ICON_H: i32 = 48;
/// 창 최소 크기(96 DPI 기준). 도구 막대 4개·탭 5개·4영역이 겹치지 않는 하한이다.
pub const MIN_W: i32 = 420;
pub const MIN_H: i32 = 400;
/// 마우스로 분할선을 잡을 수 있는 여유(96 DPI 기준, 분할선 양옆).
pub const SPLIT_GRAB: i32 = 4;
/// 영역 하나의 최소 폭·높이(96 DPI 기준). 분할선을 끌어도 이보다 좁아지지 않는다.
pub const PANEL_MIN_W: i32 = 120;
pub const PANEL_MIN_H: i32 = 2 * ROW;

pub fn sc(dpi: i32, v: i32) -> i32 {
    v * dpi.max(96) / 96
}

impl Rect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
    pub fn is_empty(&self) -> bool {
        self.width() <= 0 || self.height() <= 0
    }
    pub fn overlaps(&self, o: &Rect) -> bool {
        !self.is_empty() && !o.is_empty() && self.left < o.right && o.left < self.right && self.top < o.bottom && o.top < self.bottom
    }
    pub fn inside(&self, o: &Rect) -> bool {
        self.left >= o.left && self.right <= o.right && self.top >= o.top && self.bottom <= o.bottom
    }
}

/// 영역(패널) 하나의 배치.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelGeom {
    pub rect: Rect,
    pub header: Rect,
    /// 목록 알림 줄(불러오는 중 등). 좁은 영역에서 긴 안내가 두 줄로 접히도록 한 줄 반 높이. 알림이 없으면 빈 사각형.
    pub message: Rect,
    pub rows: Rect,
    pub icons: bool,
    pub cell_w: i32,
    pub cell_h: i32,
    pub cols: i32,
    pub row_h: i32,
    /// 자세히 보기에서 한 화면에 들어가는 줄 수.
    pub row_cap: usize,
    /// 큰 아이콘 보기에서 한 화면에 들어가는 칸 수.
    pub icon_cap: usize,
}
impl PanelGeom {
    pub fn capacity(&self) -> usize {
        if self.icons { self.icon_cap } else { self.row_cap }
    }
    /// 보이는 위치(0..capacity)의 사각형.
    pub fn slot(&self, offset: usize) -> Rect {
        if self.icons {
            let cols = self.cols.max(1) as usize;
            let left = self.rows.left + (offset % cols) as i32 * self.cell_w;
            let top = self.rows.top + (offset / cols) as i32 * self.cell_h;
            Rect { left, top, right: (left + self.cell_w).min(self.rows.right), bottom: (top + self.cell_h).min(self.rows.bottom) }
        } else {
            let top = self.rows.top + offset as i32 * self.row_h;
            Rect { left: self.rows.left, top, right: self.rows.right, bottom: (top + self.row_h).min(self.rows.bottom) }
        }
    }
    /// 점이 가리키는 항목 번호(맨 위 항목 번호 `top` 반영). 줄 밖·빈 칸이면 None.
    pub fn row_hit(&self, x: i32, y: i32, top: usize, len: usize) -> Option<usize> {
        if !self.rows.contains(x, y) {
            return None;
        }
        let offset = if self.icons {
            let col = (x - self.rows.left) / self.cell_w.max(1);
            let row = (y - self.rows.top) / self.cell_h.max(1);
            if col >= self.cols.max(1) {
                return None;
            }
            row as usize * self.cols.max(1) as usize + col as usize
        } else {
            ((y - self.rows.top) / self.row_h.max(1)) as usize
        };
        if offset >= self.capacity() {
            return None;
        }
        let index = top + offset;
        (index < len).then_some(index)
    }
}

/// 창 한 장의 모든 사각형.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chrome {
    pub dpi: i32,
    pub client: Rect,
    pub head: Rect,
    /// 0=닫기(맨 오른쪽), 1=최대화, 2=최소화, 3=고정.
    pub window_buttons: [Rect; 4],
    pub title: Rect,
    /// 누르면 상자 메뉴가 열리는 제목 부분.
    pub title_click: Rect,
    pub address: Rect,
    pub address_label: Rect,
    pub address_text: Rect,
    pub toolbar: Rect,
    pub toolbar_buttons: [Rect; 4],
    pub tab_row: Rect,
    pub tabs: [Rect; 5],
    pub query_row: Option<Rect>,
    pub body: Rect,
    pub status: Rect,
    pub status_text: Rect,
    pub status_note: Option<Rect>,
    pub gutter: i32,
    pub panels: Vec<PanelGeom>,
    pub splits: Vec<Rect>,
}

impl Chrome {
    /// `message_row`: 목록 알림 줄을 영역 맨 위에 둘지. `truncated`: 5,000개 초과 안내를 상태줄에 둘지.
    pub fn compute(dpi: i32, width: i32, height: i32, query_row: bool, truncated: bool, message_row: bool, layout: &Layout) -> Chrome {
        let w = width.max(0);
        let h = height.max(0);
        let s = |v: i32| sc(dpi, v);
        let client = Rect { left: 0, top: 0, right: w, bottom: h };
        let head_h = s(HEAD);
        let head = Rect { left: 0, top: 0, right: w, bottom: head_h.min(h) };
        let bw = s(WIN_BTN);
        let mut window_buttons = [Rect { left: 0, top: 0, right: 0, bottom: 0 }; 4];
        for (i, r) in window_buttons.iter_mut().enumerate() {
            let right = w - i as i32 * bw;
            *r = Rect { left: (right - bw).max(0), top: 0, right: right.max(0), bottom: head.bottom };
        }
        let buttons_left = window_buttons[3].left;
        let pad = s(PAD);
        let title = Rect { left: pad.min(buttons_left), top: 0, right: buttons_left, bottom: head.bottom };
        let title_click = Rect { left: 0, top: 0, right: s(260).min(buttons_left), bottom: head.bottom };

        let address = Rect { left: 0, top: head.bottom, right: w, bottom: head.bottom + s(ADDRESS) };
        let label_right = s(64);
        let address_label = Rect { left: pad, top: address.top, right: label_right, bottom: address.bottom };
        let address_text = Rect { left: label_right, top: address.top + s(4), right: (w - s(16)).max(label_right), bottom: address.bottom - s(4) };

        let toolbar = Rect { left: 0, top: address.bottom, right: w, bottom: address.bottom + s(TOOLBAR) };
        let gap = s(2);
        let avail = (w - 2 * pad - 3 * gap).max(0);
        let tool_w = (avail / 4).min(s(104));
        let mut toolbar_buttons = [Rect { left: 0, top: 0, right: 0, bottom: 0 }; 4];
        for (i, r) in toolbar_buttons.iter_mut().enumerate() {
            let left = pad + i as i32 * (tool_w + gap);
            *r = Rect { left, top: toolbar.top + s(3), right: left + tool_w, bottom: toolbar.bottom - s(3) };
        }

        let tab_row = Rect { left: 0, top: toolbar.bottom, right: w, bottom: toolbar.bottom + s(TAB) };
        let mut tabs = [Rect { left: 0, top: 0, right: 0, bottom: 0 }; 5];
        for (i, r) in tabs.iter_mut().enumerate() {
            *r = Rect { left: i as i32 * w / 5, top: tab_row.top, right: (i as i32 + 1) * w / 5, bottom: tab_row.bottom };
        }
        let query_rect = query_row.then(|| Rect { left: 0, top: tab_row.bottom, right: w, bottom: tab_row.bottom + s(ROW) });
        let list_top = query_rect.map(|r| r.bottom).unwrap_or(tab_row.bottom);

        let status_top = (h - s(STATUS)).max(list_top);
        let body = Rect { left: 0, top: list_top.min(h), right: w, bottom: status_top.min(h).max(list_top.min(h)) };
        let status = Rect { left: 0, top: status_top.min(h), right: w, bottom: h };
        let status_note = truncated.then(|| Rect { left: (w - pad - s(170)).max(pad), top: status.top, right: (w - pad).max(pad), bottom: status.bottom });
        let status_text = Rect {
            left: pad.min(w),
            top: status.top,
            right: status_note.map(|n| n.left - pad).unwrap_or(w - pad).max(pad.min(w)),
            bottom: status.bottom,
        };

        let gutter = s(GUTTER);
        let rects = layout.panel_rects(body.width(), body.height(), gutter);
        let row_h = s(ROW);
        let cell_w = s(CELL_W);
        let cell_h = s(CELL_H);
        let panels = rects
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let rect = Rect { left: r.left, top: body.top + r.top, right: r.right, bottom: body.top + r.bottom };
                let header = Rect { left: rect.left, top: rect.top, right: rect.right, bottom: (rect.top + row_h).min(rect.bottom) };
                let message = if message_row {
                    Rect { left: rect.left, top: header.bottom, right: rect.right, bottom: (header.bottom + row_h * 3 / 2).min(rect.bottom) }
                } else {
                    Rect { left: rect.left, top: header.bottom, right: rect.right, bottom: header.bottom }
                };
                let rows = Rect { left: rect.left, top: message.bottom, right: rect.right, bottom: rect.bottom.max(message.bottom) };
                let icons = layout.panels.get(i).is_some_and(|p| p.view == "icons");
                let cols = (rows.width() / cell_w.max(1)).max(1);
                // 작은 영역에서는 칸 높이를 줄여 적어도 한 칸은 보이게 한다.
                let cell_h = cell_h.min(rows.height().max(1));
                PanelGeom {
                    rect,
                    header,
                    message,
                    rows,
                    icons,
                    cell_w,
                    cell_h,
                    cols,
                    row_h,
                    row_cap: (rows.height().max(0) / row_h.max(1)) as usize,
                    icon_cap: (rows.height().max(0) / cell_h.max(1)) as usize * cols as usize,
                }
            })
            .collect();
        let splits = layout
            .split_rects(body.width(), body.height(), gutter)
            .into_iter()
            .map(|r| Rect { left: r.left, top: body.top + r.top, right: r.right, bottom: body.top + r.bottom })
            .collect();
        Chrome {
            dpi,
            client,
            head,
            window_buttons,
            title,
            title_click,
            address,
            address_label,
            address_text,
            toolbar,
            toolbar_buttons,
            tab_row,
            tabs,
            query_row: query_rect,
            body,
            status,
            status_text,
            status_note,
            gutter,
            panels,
            splits,
        }
    }
    pub fn window_button(&self, x: i32, y: i32) -> Option<usize> {
        self.window_buttons.iter().position(|r| r.contains(x, y))
    }
    /// 제목 줄에서 상자 메뉴를 여는 부분. 창 단추가 우선한다.
    pub fn title_menu(&self, x: i32, y: i32) -> bool {
        self.window_button(x, y).is_none() && self.title_click.contains(x, y)
    }
    /// 창 틀 판정에서 "클라이언트(눌림)"로 취급할 머리글 위치.
    pub fn head_interactive(&self, x: i32, y: i32) -> bool {
        self.window_button(x, y).is_some() || self.title_click.contains(x, y)
    }
    pub fn toolbar_button(&self, x: i32, y: i32) -> Option<usize> {
        self.toolbar_buttons.iter().position(|r| r.contains(x, y))
    }
    pub fn tab(&self, x: i32, y: i32) -> Option<usize> {
        self.tabs.iter().position(|r| r.contains(x, y))
    }
    pub fn panel_at(&self, x: i32, y: i32) -> Option<usize> {
        self.panels.iter().position(|p| p.rect.contains(x, y))
    }
    /// 분할선 판정(마우스 모양과 끌기 시작이 같은 함수를 쓴다).
    pub fn split_hit(&self, layout: &Layout, x: i32, y: i32) -> Option<crate::vf_layout::SplitHit> {
        layout.split_hit(self.body.width(), self.body.height(), self.gutter, x - self.body.left, y - self.body.top, sc(self.dpi, SPLIT_GRAB))
    }
    /// 활성 영역에서 한 번에 보이는 항목 수(키보드 스크롤용). 영역이 없으면 1.
    pub fn capacity_of(&self, panel: usize) -> usize {
        self.panels.get(panel).map(|p| p.capacity()).unwrap_or(1).max(1)
    }
}

/// 분할선을 끌 때 비율(1..9999). `pos`는 첫 영역이 차지할 길이, `avail`은 분할선 두께를 뺀 전체 길이.
/// 어느 쪽 영역도 `min_px`보다 좁아지지 않게 한다(전체가 너무 작으면 반반).
pub fn split_ratio(pos: i32, avail: i32, min_px: i32) -> u16 {
    if avail <= 0 {
        return 5000;
    }
    let lo = min_px.min(avail / 2);
    let clamped = pos.clamp(lo, (avail - lo).max(lo));
    ((clamped as i64 * 10000 / avail as i64) as i32).clamp(1, 9999) as u16
}

/// 상태줄 문구 만들기(그리기와 시험이 같은 문구를 쓴다).
pub fn status_text(item_count: usize, selected: usize, panel_count: u8, last_operation: &str) -> String {
    let mut s = format!("{item_count}개 항목 · {selected}개 선택 · {panel_count}개 영역");
    if !last_operation.is_empty() {
        s.push_str(" · ");
        s.push_str(last_operation);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    const DPIS: [i32; 5] = [96, 120, 144, 168, 192];
    fn layouts() -> Vec<Layout> {
        (1..=4)
            .flat_map(|n| {
                let a = Layout::for_count(n);
                let mut b = Layout::for_count(n);
                for p in &mut b.panels {
                    p.view = "icons".into();
                }
                [a, b]
            })
            .collect()
    }
    fn sizes(dpi: i32) -> Vec<(i32, i32)> {
        let mw = sc(dpi, MIN_W);
        let mh = sc(dpi, MIN_H);
        vec![(mw, mh), (mw + 1, mh + 1), (sc(dpi, 600), sc(dpi, 420)), (sc(dpi, 900), sc(dpi, 700)), (1920, 1080)]
    }
    fn fixed_rects(c: &Chrome) -> Vec<(&'static str, Rect)> {
        let mut v = vec![("head", c.head), ("address", c.address), ("toolbar", c.toolbar), ("tab_row", c.tab_row), ("body", c.body), ("status", c.status)];
        if let Some(q) = c.query_row {
            v.push(("query", q));
        }
        v
    }
    #[test]
    fn vertical_bands_do_not_overlap_at_every_scale() {
        for dpi in DPIS {
            for (w, h) in sizes(dpi) {
                for l in layouts() {
                    for q in [false, true] {
                        let c = Chrome::compute(dpi, w, h, q, true, true, &l);
                        let v = fixed_rects(&c);
                        for (i, (an, a)) in v.iter().enumerate() {
                            assert!(a.inside(&c.client), "{an} 밖 dpi={dpi} {w}x{h}");
                            for (bn, b) in v.iter().skip(i + 1) {
                                assert!(!a.overlaps(b), "{an}/{bn} 겹침 dpi={dpi} {w}x{h} q={q}");
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn head_buttons_title_and_toolbar_do_not_overlap() {
        for dpi in DPIS {
            for (w, h) in sizes(dpi) {
                let c = Chrome::compute(dpi, w, h, false, false, false, &Layout::for_count(1));
                for (i, a) in c.window_buttons.iter().enumerate() {
                    assert!(a.inside(&c.head));
                    assert!(!a.overlaps(&c.title), "제목/단추 겹침");
                    for b in c.window_buttons.iter().skip(i + 1) {
                        assert!(!a.overlaps(b));
                    }
                }
                assert!(c.title_click.right <= c.window_buttons[3].left);
                for (i, a) in c.toolbar_buttons.iter().enumerate() {
                    assert!(a.inside(&c.toolbar), "도구 단추가 막대 밖 dpi={dpi} w={w}");
                    assert!(a.right <= w, "도구 단추가 창 밖 dpi={dpi} w={w}");
                    for b in c.toolbar_buttons.iter().skip(i + 1) {
                        assert!(!a.overlaps(b));
                    }
                }
                for (i, a) in c.tabs.iter().enumerate() {
                    for b in c.tabs.iter().skip(i + 1) {
                        assert!(!a.overlaps(b));
                    }
                }
                assert_eq!(c.tabs[0].left, 0);
                assert_eq!(c.tabs[4].right, w);
                let t = Chrome::compute(dpi, w, h, false, true, false, &Layout::for_count(1));
                let note = t.status_note.unwrap();
                assert!(t.status_text.right <= note.left && note.inside(&t.status));
            }
        }
    }
    #[test]
    fn panels_are_inside_body_and_never_overlap() {
        for dpi in DPIS {
            for (w, h) in sizes(dpi) {
                for l in layouts() {
                    for msg in [false, true] {
                        let c = Chrome::compute(dpi, w, h, false, false, msg, &l);
                        assert_eq!(c.panels.len(), l.panel_count as usize);
                        for (i, p) in c.panels.iter().enumerate() {
                            assert!(p.rect.inside(&c.body), "영역 밖 dpi={dpi} {w}x{h} n={}", l.panel_count);
                            assert!(p.header.inside(&p.rect) && p.rows.inside(&p.rect) && p.message.inside(&p.rect));
                            assert!(!p.header.overlaps(&p.rows) && !p.header.overlaps(&p.message) && !p.message.overlaps(&p.rows));
                            assert!(p.capacity() >= 1, "영역에 줄이 하나도 안 들어감 dpi={dpi} {w}x{h} n={}", l.panel_count);
                            for q in c.panels.iter().skip(i + 1) {
                                assert!(!p.rect.overlaps(&q.rect));
                            }
                            for s in &c.splits {
                                assert!(!p.rect.overlaps(s), "영역과 분할선 겹침");
                            }
                            // 보이는 모든 칸이 영역 안에 있다.
                            for k in 0..p.capacity() {
                                assert!(p.slot(k).inside(&p.rows), "칸이 영역 밖 k={k}");
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn row_hits_match_drawn_slots() {
        for dpi in DPIS {
            for (w, h) in sizes(dpi) {
                for l in layouts() {
                    let c = Chrome::compute(dpi, w, h, false, false, false, &l);
                    for (pi, p) in c.panels.iter().enumerate() {
                        for top in [0usize, 3] {
                            let len = 1000;
                            for k in 0..p.capacity() {
                                let r = p.slot(k);
                                let (cx, cy) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
                                assert_eq!(p.row_hit(cx, cy, top, len), Some(top + k), "p{pi} k={k} dpi={dpi}");
                                assert_eq!(c.panel_at(cx, cy), Some(pi));
                            }
                            // 머리글·분할선·창 밖은 줄이 아니다.
                            assert_eq!(p.row_hit(p.header.left + 1, p.header.top + 1, top, len), None);
                            assert_eq!(p.row_hit(p.rows.left - 1, p.rows.top + 1, top, len), None);
                            // 항목 수보다 뒤는 빈 칸이다.
                            assert_eq!(p.row_hit(p.slot(0).left + 1, p.slot(0).top + 1, 0, 0), None);
                        }
                    }
                    assert_eq!(c.panel_at(-1, -1), None);
                }
            }
        }
    }
    #[test]
    fn icon_grid_right_remainder_is_not_a_cell() {
        // 폭이 칸 폭의 정수배가 아니면 오른쪽 남는 띠를 눌러도 다음 줄 첫 칸이 선택되지 않는다.
        let l = {
            let mut l = Layout::for_count(1);
            l.panels[0].view = "icons".into();
            l
        };
        let c = Chrome::compute(96, 500, 500, false, false, false, &l);
        let p = &c.panels[0];
        assert_eq!(p.cols, 4);
        assert_eq!(p.row_hit(p.rows.left + 4 * CELL_W + 5, p.rows.top + 5, 0, 100), None);
        assert_eq!(p.row_hit(p.rows.left + 3 * CELL_W + 5, p.rows.top + 5, 0, 100), Some(3));
        assert_eq!(p.row_hit(p.rows.left + 5, p.rows.top + CELL_H + 5, 0, 100), Some(4));
    }
    #[test]
    fn list_rows_never_leave_lower_panels() {
        // 예전에는 아래쪽 영역의 줄 수를 영역 높이가 아니라 아래 끝 좌표로 계산해 상태줄을 덮었다.
        for dpi in DPIS {
            let c = Chrome::compute(dpi, sc(dpi, 700), sc(dpi, 500), false, false, false, &Layout::for_count(4));
            for p in &c.panels {
                let last = p.slot(p.row_cap - 1);
                assert!(last.bottom <= p.rect.bottom && last.bottom <= c.status.top);
            }
        }
    }
    #[test]
    fn hit_tests_follow_drawn_buttons() {
        for dpi in DPIS {
            let c = Chrome::compute(dpi, sc(dpi, 600), sc(dpi, 420), false, false, false, &Layout::for_count(2));
            for (i, r) in c.window_buttons.iter().enumerate() {
                assert_eq!(c.window_button((r.left + r.right) / 2, 3), Some(i));
                assert!(!c.title_menu((r.left + r.right) / 2, 3));
            }
            for (i, r) in c.toolbar_buttons.iter().enumerate() {
                assert_eq!(c.toolbar_button((r.left + r.right) / 2, (r.top + r.bottom) / 2), Some(i));
            }
            // 단추 밖 빈 막대와 단추 사이 틈은 단추가 아니다(예전에는 마지막 단추로 몰렸다).
            let last = c.toolbar_buttons[3];
            assert_eq!(c.toolbar_button(last.right + 5, (last.top + last.bottom) / 2), None);
            assert_eq!(c.toolbar_button(c.toolbar_buttons[0].left - 1, (last.top + last.bottom) / 2), None);
            assert_eq!(c.toolbar_button(c.toolbar_buttons[0].right, (last.top + last.bottom) / 2), None);
            for (i, r) in c.tabs.iter().enumerate() {
                assert_eq!(c.tab(r.left, r.top), Some(i));
                assert_eq!(c.tab(r.right - 1, r.bottom - 1), Some(i));
            }
            assert_eq!(c.tab(5, c.toolbar.top), None);
            assert!(c.title_menu(10, 3));
        }
    }
    #[test]
    fn split_hit_uses_gutter_with_same_tolerance_for_cursor_and_drag() {
        for dpi in DPIS {
            let l = Layout::for_count(2);
            let c = Chrome::compute(dpi, sc(dpi, 800), sc(dpi, 500), false, false, false, &l);
            let g = c.splits[0];
            let y = (g.top + g.bottom) / 2;
            assert_eq!(c.split_hit(&l, (g.left + g.right) / 2, y), Some(crate::vf_layout::SplitHit::Column(0)));
            assert_eq!(c.split_hit(&l, g.left - sc(dpi, SPLIT_GRAB) - 1, y), None);
            assert_eq!(c.split_hit(&l, 5, y), None);
            // 머리글 위(목록 밖)에서는 분할선이 잡히지 않는다.
            assert_eq!(c.split_hit(&l, (g.left + g.right) / 2, c.tab_row.top), None);
        }
    }
    #[test]
    fn split_ratio_keeps_minimum_panel_size() {
        assert_eq!(split_ratio(500, 1000, 100), 5000);
        assert_eq!(split_ratio(0, 1000, 100), 1000);
        assert_eq!(split_ratio(999, 1000, 100), 9000);
        assert_eq!(split_ratio(10, 100, 80), 5000);
        assert_eq!(split_ratio(5, 0, 10), 5000);
    }
    #[test]
    fn message_row_pushes_rows_down_consistently() {
        let l = Layout::for_count(1);
        let a = Chrome::compute(96, 600, 420, false, false, false, &l);
        let b = Chrome::compute(96, 600, 420, false, false, true, &l);
        assert_eq!(b.panels[0].rows.top, a.panels[0].rows.top + ROW * 3 / 2);
        assert!(b.panels[0].message.height() == ROW * 3 / 2 && a.panels[0].message.is_empty());
        assert!(!b.panels[0].message.overlaps(&b.panels[0].header));
    }
    #[test]
    fn status_text_format() {
        assert_eq!(status_text(3, 1, 2, ""), "3개 항목 · 1개 선택 · 2개 영역");
        assert_eq!(status_text(0, 0, 1, "되돌림"), "0개 항목 · 0개 선택 · 1개 영역 · 되돌림");
    }
}
