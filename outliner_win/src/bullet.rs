//! 불릿 포인터 상태와 상위 경로. Win32 이벤트와 캡처가 같은 좌표를 사용한다.
use crate::{layout::R, paint::{Metrics, scale}, view::View};
use outliner_core::DropPlace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropHint { pub target: u64, pub row: usize, pub place: DropPlace }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pointer {
    #[default]
    Idle,
    Armed { source: u64, x: i32, y: i32 },
    Dragging { source: u64, hint: Option<DropHint> },
}

impl Pointer {
    pub fn hint(&self) -> Option<DropHint> { if let Self::Dragging { hint, .. } = self { *hint } else { None } }
    pub fn dragging(&self) -> bool { matches!(self, Self::Dragging { .. }) }

    pub fn motion(&mut self, view: &View, m: &Metrics, x: i32, y: i32, width: i32, height: i32, threshold: (i32, i32)) {
        if let Self::Armed { source, x: sx, y: sy } = *self {
            if (x - sx).abs() < threshold.0.max(1) && (y - sy).abs() < threshold.1.max(1) { return; }
            *self = Self::Dragging { source, hint: None };
        }
        let Self::Dragging { source, hint } = self else { return; };
        *hint = None;
        if x < 0 || x >= width || y < m.header_h || y >= height { return; }
        let Some(row) = view.row_at(y, m.header_h, m.row_h) else { return; };
        let r = &view.rows[row];
        let Some(from) = view.doc.blocks.iter().position(|b| b.id == *source) else { return; };
        if (from..crate::view::subtree_end(&view.doc, from)).contains(&r.block) { return; }
        let local_y = y - m.header_h - view.slot_of(row) as i32 * m.row_h;
        let height = r.lines as i32 * m.row_h;
        let place = if local_y < height / 4 { DropPlace::Before }
            else if local_y >= height * 3 / 4 { DropPlace::After }
            else if x >= m.text_x(r.depth) { DropPlace::Child }
            else if local_y < height / 2 { DropPlace::Before } else { DropPlace::After };
        *hint = Some(DropHint { target: view.doc.blocks[r.block].id, row, place });
    }
}

#[derive(Clone, Debug)]
pub struct Crumb { pub rect: R, pub root: Option<u64>, pub label: String }

/// 폭이 좁으면 조상 앞부분을 줄인다. ←는 전체 이력, …는 바로 위 조상이다.
pub fn crumbs(view: &View, dpi: u32, width: i32) -> Vec<Crumb> {
    if view.zoom.is_none() { return Vec::new(); }
    let m = Metrics::new(dpi);
    let bottom = m.header_h / 2;
    let unit = scale(112, dpi);
    let mut out = vec![Crumb { rect: R::new(0, 0, scale(38, dpi).min(width), bottom), root: None, label: "←".into() }];
    let path = view.ancestors();
    let mut x = scale(38, dpi);
    out.push(Crumb { rect: R::new(x, 0, (x + scale(60, dpi)).min(width), bottom), root: None, label: "페이지".into() });
    x += scale(60, dpi);
    let parent = path.len().checked_sub(2).map(|i| view.doc.blocks[path[i]].id);
    out.push(Crumb { rect: R::new(x, 0, (x + scale(38, dpi)).min(width), bottom), root: parent, label: "↑".into() });
    x += scale(38, dpi);
    let slots = ((width - x) / unit).max(1) as usize;
    let start = path.len().saturating_sub(slots);
    for (pos, &i) in path.iter().enumerate().skip(start) {
        if x >= width { break; }
        let b = &view.doc.blocks[i];
        let label = if pos == start && start > 0 { format!("… {}", b.text.lines().next().unwrap_or("")) }
            else { format!("› {}", b.text.lines().next().unwrap_or("")) };
        out.push(Crumb { rect: R::new(x, 0, (x + unit).min(width), bottom), root: Some(b.id), label });
        x += unit;
    }
    out
}
