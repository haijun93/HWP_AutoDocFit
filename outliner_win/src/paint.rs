//! 블록 목록 그리기(GDI). 실제 창과 캡처 모드가 같은 `draw` 를 쓴다.
use crate::view::{View, rows_per_page};
use std::ptr::null_mut;
use ui_theme::Palette;
use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::*;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 96 DPI 기준 치수를 `dpi` 로 키운다.
pub fn scale(v: i32, dpi: u32) -> i32 {
    (v as i64 * dpi as i64 / 96) as i32
}

/// 화면 치수(모두 화소).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metrics {
    pub header_h: i32,
    pub row_h: i32,
    pub indent_w: i32,
    pub pad: i32,
    pub font_h: i32,
}

impl Metrics {
    pub fn new(dpi: u32) -> Metrics {
        Metrics {
            header_h: scale(36, dpi),
            row_h: scale(28, dpi),
            indent_w: scale(24, dpi),
            pad: scale(12, dpi),
            font_h: scale(16, dpi),
        }
    }

    pub fn page_rows(&self, client_h: i32) -> usize {
        rows_per_page(client_h, self.header_h, self.row_h)
    }

    /// 깊이 `depth` 줄의 불릿(접기 삼각형) 중심 x.
    pub fn marker_x(&self, depth: usize) -> i32 {
        self.pad + depth as i32 * self.indent_w
    }

    pub fn bullet_x(&self, depth: usize) -> i32 {
        self.marker_x(depth) + self.indent_w / 2
    }

    /// 깊이 `depth` 줄의 글자 시작 x(TODO 칸이 없을 때).
    pub fn text_x(&self, depth: usize) -> i32 {
        self.pad + self.indent_w + depth as i32 * self.indent_w
    }

    /// TODO·DONE 체크 칸이 있을 때 글자가 밀리는 폭(`draw` 의 `tx += 2*b + 8` 과 같은 값).
    pub fn status_shift(&self, dpi: u32) -> i32 {
        2 * scale(7, dpi) + scale(8, dpi)
    }

    /// 편집 중 블록 위에 겹칠 EDIT 의 위치(클라이언트 좌표). 글자 시작 x 는 목록의 글자와 같고
    /// (EDIT 왼쪽 여백 4px 만큼 앞으로 당긴다), 줄 높이 안에서 위아래 가운데에 놓인다.
    pub fn edit_rect(&self, dpi: u32, depth: usize, has_status: bool, slot: usize, width: i32) -> RECT {
        let mut tx = self.text_x(depth);
        if has_status {
            tx += self.status_shift(dpi);
        }
        let top = self.header_h + slot as i32 * self.row_h + scale(3, dpi);
        RECT { left: tx - scale(4, dpi), top, right: (width - self.pad).max(tx), bottom: top + self.row_h - 2 * scale(3, dpi) }
    }

    /// 줄 `pos` 의 편집 칸(여러 줄 블록은 줄 수만큼 높다).
    pub fn row_edit_rect(&self, dpi: u32, view: &View, pos: usize, width: i32) -> RECT {
        let row = &view.rows[pos];
        let mut r = self.edit_rect(dpi, row.depth, row.status.is_some(), view.slot_of(pos), width);
        r.bottom += (row.lines as i32 - 1) * self.row_h;
        r
    }
}

/// 그리기에 덧붙는 정보(편집 중 줄·안내 문구·변경 표시).
#[derive(Clone, Debug, Default)]
pub struct Extras {
    /// 화면 줄 번호(`rows` 안). 이 줄의 글자는 EDIT 가 덮으므로 그리지 않고 테두리만 그린다.
    pub editing_row: Option<usize>,
    /// 머리글 오른쪽에 블록 수 대신 보일 안내(오류·경고·저장함).
    pub message: Option<String>,
    /// 저장하지 않은 변경 → 머리글 제목 앞에 ●.
    pub dirty: bool,
    pub drop_hint: Option<crate::bullet::DropHint>,
    pub drag_invalid: bool,
}

/// 편집 창(EDIT)과 같은 글꼴을 만든다(호출한 쪽이 `DeleteObject`).
pub unsafe fn create_font(dpi: u32) -> HFONT {
    unsafe { font(&Metrics::new(dpi), false) }
}

pub(crate) unsafe fn font(m: &Metrics, bold: bool) -> HFONT {
    unsafe {
        CreateFontW(
            -m.font_h,
            0,
            0,
            0,
            if bold { 700 } else { 400 },
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            (DEFAULT_PITCH | FF_DONTCARE) as u32,
            wide("Malgun Gothic").as_ptr(),
        )
    }
}

pub(crate) unsafe fn fill(dc: HDC, r: RECT, color: u32) {
    unsafe {
        let brush = CreateSolidBrush(color);
        FillRect(dc, &r, brush);
        DeleteObject(brush);
    }
}

pub(crate) unsafe fn text(dc: HDC, s: &str, mut r: RECT, color: u32, flags: u32) {
    unsafe {
        SetTextColor(dc, color);
        let w = wide(s);
        DrawTextW(dc, w.as_ptr(), -1, &mut r, flags | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS);
    }
}

pub(crate) unsafe fn line(dc: HDC, color: u32, width: i32, pts: &[(i32, i32)]) {
    unsafe {
        let pen = CreatePen(PS_SOLID, width, color);
        let old = SelectObject(dc, pen);
        MoveToEx(dc, pts[0].0, pts[0].1, null_mut());
        for p in &pts[1..] {
            LineTo(dc, p.0, p.1);
        }
        SelectObject(dc, old);
        DeleteObject(pen);
    }
}

unsafe fn polygon(dc: HDC, color: u32, pts: &[POINT]) {
    unsafe {
        let brush = CreateSolidBrush(color);
        let pen = CreatePen(PS_SOLID, 1, color);
        let (ob, op) = (SelectObject(dc, brush), SelectObject(dc, pen));
        Polygon(dc, pts.as_ptr(), pts.len() as i32);
        SelectObject(dc, ob);
        SelectObject(dc, op);
        DeleteObject(brush);
        DeleteObject(pen);
    }
}

/// 현재 글꼴로 `s` 를 그릴 때의 가로 폭.
pub(crate) unsafe fn text_width(dc: HDC, s: &str) -> i32 {
    unsafe {
        let w = wide(s);
        let mut size = std::mem::zeroed();
        GetTextExtentPoint32W(dc, w.as_ptr(), (w.len() - 1) as i32, &mut size);
        size.cx
    }
}

/// 한 줄의 글에서 `[[링크]]` 가 차지하는 가로 구간 `(왼쪽, 오른쪽, 페이지 이름)`(글자 시작 x = `tx`).
pub fn link_spans_x(row_text: &str, tx: i32, dpi: u32) -> Vec<(i32, i32, String)> {
    let segs = crate::links::segments(row_text);
    if segs.iter().all(|s| s.link.is_none()) {
        return Vec::new();
    }
    unsafe {
        let screen = GetDC(null_mut());
        let dc = CreateCompatibleDC(screen);
        let f = font(&Metrics::new(dpi), false);
        let old = SelectObject(dc, f);
        let mut x = tx;
        let mut out = Vec::new();
        for seg in &segs {
            let wd = text_width(dc, &seg.text);
            if let Some(name) = &seg.link {
                out.push((x, x + wd, name.clone()));
            }
            x += wd;
        }
        SelectObject(dc, old);
        DeleteObject(f);
        DeleteDC(dc);
        ReleaseDC(null_mut(), screen);
        out
    }
}

/// `w`×`h` 화소의 `dc` 에 목록을 그린다(읽기 전용 모양).
pub unsafe fn draw(dc: HDC, w: i32, h: i32, view: &View, pal: &Palette, dpi: u32, title: &str) {
    unsafe { draw_ext(dc, w, h, view, pal, dpi, title, &Extras::default()) }
}

/// `draw` + 편집 중 줄·안내 문구·변경 표시.
pub unsafe fn draw_ext(dc: HDC, w: i32, h: i32, view: &View, pal: &Palette, dpi: u32, title: &str, extras: &Extras) {
    unsafe {
        let m = Metrics::new(dpi);
        fill(dc, RECT { left: 0, top: 0, right: w, bottom: h }, pal.bg);
        SetBkMode(dc, TRANSPARENT as i32);
        let normal = font(&m, false);
        let bold = font(&m, true);
        let old_font = SelectObject(dc, normal);

        // 머리글: 문서 이름 + 블록 수/보이는 줄 수.
        fill(dc, RECT { left: 0, top: 0, right: w, bottom: m.header_h }, pal.head_bg);
        SelectObject(dc, bold);
        let head = RECT { left: m.pad, top: 0, right: w - m.pad, bottom: m.header_h };
        let shown_title = if extras.dirty { format!("● {title}") } else { title.to_string() };
        text(dc, &shown_title, head, pal.text, DT_LEFT);
        SelectObject(dc, normal);
        match &extras.message {
            Some(msg) => text(dc, msg, head, pal.accent, DT_RIGHT),
            None => {
                let counts = format!("블록 {}개 · Alt+↑ 선택 · Alt+Shift 이동", view.block_count());
                text(dc, &counts, head, pal.text_sub, DT_RIGHT);
            }
        }
        fill(dc, RECT { left: 0, top: m.header_h - 1, right: w, bottom: m.header_h }, pal.divider);
        if view.zoom.is_some() {
            fill(dc, RECT { left: 0, top: 0, right: w, bottom: m.header_h - 1 }, pal.head_bg);
            for crumb in crate::bullet::crumbs(view, dpi, w) {
                let r = crumb.rect;
                text(dc, &crumb.label, RECT { left: r.l + scale(4, dpi), top: r.t, right: r.r, bottom: r.b }, pal.accent, DT_LEFT);
            }
            let status = format!("{}{}", if extras.dirty { "● " } else { "" }, extras.message.as_deref().unwrap_or("블록 줌 · Ctrl+[ 뒤로 · ↑ 상위 블록"));
            text(dc, &status, RECT { left: m.pad, top: m.header_h / 2, right: w - m.pad, bottom: m.header_h - 1 }, pal.text_sub, DT_LEFT);
        }

        if view.rows.is_empty() {
            let r = RECT { left: m.pad, top: m.header_h, right: w - m.pad, bottom: m.header_h + m.row_h * 2 };
            text(dc, "(블록이 없습니다)", r, pal.text_muted, DT_LEFT);
        }

        let page = m.page_rows(h) + 1; // 아래로 잘려 보이는 한 줄까지
        let end = (view.first + page).min(view.rows.len());
        let mut slot = 0usize;
        for index in view.first..end {
            let row = &view.rows[index];
            let top = m.header_h + slot as i32 * m.row_h;
            slot += row.lines;
            let height = row.lines as i32 * m.row_h;
            let band = RECT { left: 0, top, right: w, bottom: top + height };
            let in_range = view.selection_anchor.zip(view.selection_end)
                .is_some_and(|(a, b)| index >= a.min(b) && index <= a.max(b));
            let selected = index == view.selected || in_range;
            if selected {
                fill(dc, band, pal.selected);
            }
            // 들여쓰기 안내선(조상 단계마다 세로선).
            for d in 0..row.depth {
                let x = m.bullet_x(d);
                line(dc, pal.divider, 1, &[(x, top), (x, top + height)]);
            }
            let mid = top + m.row_h / 2;
            let mx = m.marker_x(row.depth);
            let ink = pal.row_text(selected, pal.text);
            let s = scale(4, dpi);
            if row.has_children {
                // 접힘 ▸ / 펼침 ▾ 삼각형.
                let tri = if row.folded {
                    [POINT { x: mx - s, y: mid - s - 1 }, POINT { x: mx - s, y: mid + s + 1 }, POINT { x: mx + s, y: mid }]
                } else {
                    [POINT { x: mx - s - 1, y: mid - s }, POINT { x: mx + s + 1, y: mid - s }, POINT { x: mx, y: mid + s }]
                };
                polygon(dc, pal.row_text(selected, pal.accent), &tri);
            }
            {
                // 불릿 점.
                let mx = m.bullet_x(row.depth);
                let brush = CreateSolidBrush(pal.row_text(selected, pal.text_faint));
                let pen = CreatePen(PS_SOLID, 1, pal.row_text(selected, pal.text_faint));
                let (ob, op) = (SelectObject(dc, brush), SelectObject(dc, pen));
                let r = (s / 2).max(2);
                Ellipse(dc, mx - r, mid - r, mx + r + 1, mid + r + 1);
                SelectObject(dc, ob);
                SelectObject(dc, op);
                DeleteObject(brush);
                DeleteObject(pen);
                if row.folded && row.has_children {
                    let radius = scale(6, dpi);
                    let brush = GetStockObject(NULL_BRUSH);
                    let pen = CreatePen(PS_SOLID, scale(1, dpi).max(1), pal.row_text(selected, pal.accent));
                    let (ob, op) = (SelectObject(dc, brush), SelectObject(dc, pen));
                    Ellipse(dc, mx - radius, mid - radius, mx + radius + 1, mid + radius + 1);
                    SelectObject(dc, ob); SelectObject(dc, op); DeleteObject(pen);
                }
            }
            let mut tx = m.text_x(row.depth);
            let mut color = ink;
            if let Some(status) = &row.status {
                // TODO·DONE 체크 칸.
                let b = scale(7, dpi);
                let (l, t) = (tx, mid - b);
                let rect = RECT { left: l, top: t, right: l + 2 * b, bottom: t + 2 * b };
                let box_color = pal.row_text(selected, pal.accent);
                let brush = CreateSolidBrush(box_color);
                FrameRect(dc, &rect, brush);
                DeleteObject(brush);
                if status == "DONE" {
                    line(dc, box_color, 2, &[(l + b / 2, mid), (l + b - 1, mid + b / 2 + 1), (l + 2 * b - b / 3, mid - b / 2)]);
                    color = pal.row_text(selected, pal.text_muted);
                }
                tx += 2 * b + scale(8, dpi);
            }
            if extras.editing_row == Some(index) {
                // 글자는 EDIT 가 덮는다. 테두리만(EDIT 바깥 1px).
                let er = m.row_edit_rect(dpi, view, index, w);
                let outer = RECT { left: er.left - 1, top: er.top - 1, right: er.right + 1, bottom: er.bottom + 1 };
                let brush = CreateSolidBrush(pal.accent);
                FrameRect(dc, &outer, brush);
                DeleteObject(brush);
                continue;
            }
            if row.text.is_empty() {
                continue;
            }
            for (n, part) in row.text.split('\n').enumerate() {
                let top = top + n as i32 * m.row_h;
                let segs = crate::links::segments(part);
                if segs.iter().all(|s| s.link.is_none()) {
                    let r = RECT { left: tx, top, right: w - m.pad, bottom: top + m.row_h };
                    text(dc, part, r, color, DT_LEFT);
                } else {
                    // `[[링크]]` 는 강조색으로 이어 그린다(클릭 판정은 `link_spans_x` 가 같은 폭으로 계산).
                    let mut x = tx;
                    for seg in &segs {
                        let r = RECT { left: x, top, right: (w - m.pad).max(x), bottom: top + m.row_h };
                        let c = if seg.link.is_some() { pal.row_text(selected, pal.accent) } else { color };
                        text(dc, &seg.text, r, c, DT_LEFT);
                        x += text_width(dc, &seg.text);
                    }
                }
            }
        }
        if let Some(hint) = extras.drop_hint {
            if let Some(row) = view.rows.get(hint.row) {
                let top = m.header_h + view.slot_of(hint.row) as i32 * m.row_h;
                let last = view.rows.iter().enumerate().skip(hint.row + 1).take_while(|(_, r)| r.depth > row.depth).map(|(i, _)| i).last().unwrap_or(hint.row);
                let subtree_bottom = m.header_h + (view.slot_of(last) + view.rows[last].lines) as i32 * m.row_h;
                let (x, y) = match hint.place {
                    outliner_core::DropPlace::Before => (m.bullet_x(row.depth), top),
                    outliner_core::DropPlace::After => (m.bullet_x(row.depth), subtree_bottom),
                    outliner_core::DropPlace::Child => (m.bullet_x(row.depth + 1), subtree_bottom),
                };
                line(dc, pal.accent, scale(2, dpi).max(1), &[(x, y), (w - m.pad, y)]);
                let label = match hint.place { outliner_core::DropPlace::Before => "앞에 놓기", outliner_core::DropPlace::After => "하위 전체 뒤에 놓기", outliner_core::DropPlace::Child => "마지막 자식으로 놓기" };
                text(dc, label, RECT { left: m.pad, top: h - m.row_h, right: w - m.pad, bottom: h }, pal.accent, DT_RIGHT);
            }
        } else if extras.drag_invalid {
            text(dc, "여기에는 놓을 수 없습니다 · Esc 취소", RECT { left: m.pad, top: h - m.row_h, right: w - m.pad, bottom: h }, pal.accent, DT_RIGHT);
        }
        SelectObject(dc, old_font);
        DeleteObject(normal);
        DeleteObject(bold);
    }
}
