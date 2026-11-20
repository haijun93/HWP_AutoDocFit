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
        self.pad + self.indent_w / 2 + depth as i32 * self.indent_w
    }

    /// 깊이 `depth` 줄의 글자 시작 x(TODO 칸이 없을 때).
    pub fn text_x(&self, depth: usize) -> i32 {
        self.pad + self.indent_w + depth as i32 * self.indent_w
    }
}

unsafe fn font(m: &Metrics, bold: bool) -> HFONT {
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

unsafe fn fill(dc: HDC, r: RECT, color: u32) {
    unsafe {
        let brush = CreateSolidBrush(color);
        FillRect(dc, &r, brush);
        DeleteObject(brush);
    }
}

unsafe fn text(dc: HDC, s: &str, mut r: RECT, color: u32, flags: u32) {
    unsafe {
        SetTextColor(dc, color);
        let w = wide(s);
        DrawTextW(dc, w.as_ptr(), -1, &mut r, flags | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS);
    }
}

unsafe fn line(dc: HDC, color: u32, width: i32, pts: &[(i32, i32)]) {
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

/// `w`×`h` 화소의 `dc` 에 목록을 그린다.
pub unsafe fn draw(dc: HDC, w: i32, h: i32, view: &View, pal: &Palette, dpi: u32, title: &str) {
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
        text(dc, title, head, pal.text, DT_LEFT);
        SelectObject(dc, normal);
        let counts = format!("블록 {}개 · 보이는 줄 {}", view.block_count(), view.rows.len());
        text(dc, &counts, head, pal.text_sub, DT_RIGHT);
        fill(dc, RECT { left: 0, top: m.header_h - 1, right: w, bottom: m.header_h }, pal.divider);

        if view.rows.is_empty() {
            let r = RECT { left: m.pad, top: m.header_h, right: w - m.pad, bottom: m.header_h + m.row_h * 2 };
            text(dc, "(블록이 없습니다)", r, pal.text_muted, DT_LEFT);
        }

        let page = m.page_rows(h) + 1; // 아래로 잘려 보이는 한 줄까지
        let end = (view.first + page).min(view.rows.len());
        for (slot, index) in (view.first..end).enumerate() {
            let row = &view.rows[index];
            let top = m.header_h + slot as i32 * m.row_h;
            let band = RECT { left: 0, top, right: w, bottom: top + m.row_h };
            let selected = index == view.selected;
            if selected {
                fill(dc, band, pal.selected);
            }
            // 들여쓰기 안내선(조상 단계마다 세로선).
            for d in 0..row.depth {
                let x = m.marker_x(d);
                line(dc, pal.divider, 1, &[(x, top), (x, top + m.row_h)]);
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
            } else {
                // 불릿 점.
                let brush = CreateSolidBrush(pal.row_text(selected, pal.text_faint));
                let pen = CreatePen(PS_SOLID, 1, pal.row_text(selected, pal.text_faint));
                let (ob, op) = (SelectObject(dc, brush), SelectObject(dc, pen));
                let r = (s / 2).max(2);
                Ellipse(dc, mx - r, mid - r, mx + r + 1, mid + r + 1);
                SelectObject(dc, ob);
                SelectObject(dc, op);
                DeleteObject(brush);
                DeleteObject(pen);
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
            if row.text.is_empty() {
                continue;
            }
            let r = RECT { left: tx, top, right: w - m.pad, bottom: top + m.row_h };
            text(dc, &row.text, r, color, DT_LEFT);
        }
        SelectObject(dc, old_font);
        DeleteObject(normal);
        DeleteObject(bold);
    }
}
