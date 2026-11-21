//! 그리기(GDI). 실제 창과 캡처 모드가 같은 `draw` 를 쓴다. 색은 모두 `ui_theme::Palette` 에서만 가져온다.
use crate::layout::{Hit, Layout, Rect, Shape, compute, scale};
use crate::model::Model;
use crate::schema::{Control, GROUPS, value_text};
use std::ptr::null_mut;
use ui_theme::Palette;
use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::*;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 키보드 포커스가 있는 영역(Tab 순서).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Search,
    Tree,
    List,
    Revert,
    Save,
}

impl Zone {
    pub fn next(self, back: bool) -> Zone {
        const ORDER: [Zone; 5] = [Zone::Search, Zone::Tree, Zone::List, Zone::Revert, Zone::Save];
        let i = ORDER.iter().position(|z| *z == self).unwrap_or(0);
        ORDER[(i as i32 + if back { -1 } else { 1 }).rem_euclid(5) as usize]
    }
}

/// 모델 밖의 화면 상태.
#[derive(Clone, Debug)]
pub struct Ui {
    pub zone: Zone,
    /// 목록(보이는 항목 순서) 안의 포커스 줄.
    pub focus_row: usize,
    pub hover: Hit,
    pub scroll: i32,
}

impl Default for Ui {
    fn default() -> Self {
        Ui { zone: Zone::List, focus_row: 0, hover: Hit::None, scroll: 0 }
    }
}

const FACE: &str = "Malgun Gothic";

struct Fonts {
    normal: HFONT,
    bold: HFONT,
    small: HFONT,
    title: HFONT,
}

unsafe fn make_font(px: i32, weight: i32) -> HFONT {
    unsafe {
        CreateFontW(
            -px,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            (DEFAULT_PITCH | FF_DONTCARE) as u32,
            wide(FACE).as_ptr(),
        )
    }
}

impl Fonts {
    unsafe fn new(dpi: u32) -> Fonts {
        unsafe {
            Fonts {
                normal: make_font(scale(15, dpi), 400),
                bold: make_font(scale(15, dpi), 700),
                small: make_font(scale(13, dpi), 400),
                title: make_font(scale(20, dpi), 700),
            }
        }
    }
    unsafe fn free(&self) {
        unsafe {
            for f in [self.normal, self.bold, self.small, self.title] {
                DeleteObject(f);
            }
        }
    }
}

/// 입력 EDIT 의 글꼴(호출한 쪽이 `DeleteObject`).
pub unsafe fn create_edit_font(dpi: u32) -> HFONT {
    unsafe { make_font(scale(15, dpi), 400) }
}

fn rc(r: &Rect) -> RECT {
    RECT { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

unsafe fn fill(hdc: HDC, r: &Rect, color: u32) {
    unsafe {
        let b = CreateSolidBrush(color);
        FillRect(hdc, &rc(r), b);
        DeleteObject(b);
    }
}

/// 둥근 상자(채움 + 테두리). `border_w` 화소 테두리.
unsafe fn boxed(hdc: HDC, r: &Rect, fill_c: u32, border: u32, border_w: i32, radius: i32) {
    unsafe {
        let pen = CreatePen(PS_SOLID, border_w.max(1), border);
        let brush = CreateSolidBrush(fill_c);
        let (op, ob) = (SelectObject(hdc, pen), SelectObject(hdc, brush));
        RoundRect(hdc, r.left, r.top, r.right, r.bottom, radius, radius);
        SelectObject(hdc, op);
        SelectObject(hdc, ob);
        DeleteObject(pen);
        DeleteObject(brush);
    }
}

unsafe fn hline(hdc: HDC, x1: i32, x2: i32, y: i32, color: u32) {
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, color);
        let old = SelectObject(hdc, pen);
        MoveToEx(hdc, x1, y, null_mut());
        LineTo(hdc, x2, y);
        SelectObject(hdc, old);
        DeleteObject(pen);
    }
}

unsafe fn vline(hdc: HDC, x: i32, y1: i32, y2: i32, color: u32) {
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, color);
        let old = SelectObject(hdc, pen);
        MoveToEx(hdc, x, y1, null_mut());
        LineTo(hdc, x, y2);
        SelectObject(hdc, old);
        DeleteObject(pen);
    }
}

unsafe fn text(hdc: HDC, font: HFONT, r: &Rect, s: &str, color: u32, flags: u32) {
    unsafe {
        let old = SelectObject(hdc, font);
        SetBkMode(hdc, TRANSPARENT as i32);
        SetTextColor(hdc, color);
        let w = wide(s);
        let mut r = rc(r);
        DrawTextW(hdc, w.as_ptr(), -1, &mut r, flags | DT_NOPREFIX);
        SelectObject(hdc, old);
    }
}

unsafe fn text_width(hdc: HDC, font: HFONT, s: &str) -> i32 {
    unsafe {
        let old = SelectObject(hdc, font);
        let w = wide(s);
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        DrawTextW(hdc, w.as_ptr(), -1, &mut r, DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX);
        SelectObject(hdc, old);
        r.right - r.left
    }
}

const ONE_LINE: u32 = DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS;

/// 알약 꼬리표(오른쪽 맞춤이면 `right_align`). 그린 상자를 돌려준다.
unsafe fn chip(hdc: HDC, font: HFONT, area: &Rect, s: &str, bg: u32, fg: u32, right_align: bool, dpi: u32) -> Rect {
    unsafe {
        let pad = scale(10, dpi);
        let w = (text_width(hdc, font, s) + 2 * pad).min(area.width());
        let r = if right_align {
            Rect::new(area.right - w, area.top, area.right, area.bottom)
        } else {
            Rect::new(area.left, area.top, area.left + w, area.bottom)
        };
        boxed(hdc, &r, bg, bg, 1, scale(10, dpi));
        text(hdc, font, &r, s, fg, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
        r
    }
}

pub fn shape_of(c: &Control) -> Shape {
    match c {
        Control::Check => Shape::Check,
        Control::Number { .. } => Shape::Number,
        Control::Text => Shape::Text,
        Control::Choice(_) => Shape::Choice,
        Control::ReadOnly(_) => Shape::ReadOnly,
    }
}

unsafe fn draw_focus(hdc: HDC, r: &Rect, pal: &Palette, dpi: u32, radius: i32) {
    unsafe {
        let pen = CreatePen(PS_SOLID, scale(2, dpi).max(2), pal.accent);
        let old = SelectObject(hdc, pen);
        let ob = SelectObject(hdc, GetStockObject(NULL_BRUSH));
        RoundRect(hdc, r.left, r.top, r.right, r.bottom, radius, radius);
        SelectObject(hdc, old);
        SelectObject(hdc, ob);
        DeleteObject(pen);
    }
}

unsafe fn draw_checkbox(hdc: HDC, r: &Rect, on: bool, pal: &Palette, dpi: u32) {
    unsafe {
        let line = scale(2, dpi).max(1);
        if on {
            boxed(hdc, r, pal.accent, pal.accent, 1, scale(5, dpi));
            let pen = CreatePen(PS_SOLID, line, pal.bg);
            let old = SelectObject(hdc, pen);
            let (w, h) = (r.width(), r.height());
            MoveToEx(hdc, r.left + w * 22 / 100, r.top + h * 52 / 100, null_mut());
            LineTo(hdc, r.left + w * 42 / 100, r.top + h * 72 / 100);
            LineTo(hdc, r.left + w * 78 / 100, r.top + h * 30 / 100);
            SelectObject(hdc, old);
            DeleteObject(pen);
        } else {
            boxed(hdc, r, pal.bg, pal.border, line, scale(5, dpi));
        }
    }
}

unsafe fn draw_triangle(hdc: HDC, cx: i32, cy: i32, half: i32, color: u32) {
    unsafe {
        let pts = [POINT { x: cx - half, y: cy - half / 2 }, POINT { x: cx + half, y: cy - half / 2 }, POINT { x: cx, y: cy + half / 2 + 1 }];
        let (pen, brush) = (CreatePen(PS_SOLID, 1, color), CreateSolidBrush(color));
        let (op, ob) = (SelectObject(hdc, pen), SelectObject(hdc, brush));
        Polygon(hdc, pts.as_ptr(), 3);
        SelectObject(hdc, op);
        SelectObject(hdc, ob);
        DeleteObject(pen);
        DeleteObject(brush);
    }
}

unsafe fn draw_search_icon(hdc: HDC, r: &Rect, color: u32, dpi: u32) {
    unsafe {
        let d = scale(11, dpi);
        let (x, y) = (r.left, r.top + (r.height() - d) / 2 - scale(1, dpi));
        let pen = CreatePen(PS_SOLID, scale(2, dpi).max(1), color);
        let old = SelectObject(hdc, pen);
        let ob = SelectObject(hdc, GetStockObject(NULL_BRUSH));
        Ellipse(hdc, x, y, x + d, y + d);
        MoveToEx(hdc, x + d - scale(2, dpi), y + d - scale(2, dpi), null_mut());
        LineTo(hdc, x + d + scale(5, dpi), y + d + scale(5, dpi));
        SelectObject(hdc, old);
        SelectObject(hdc, ob);
        DeleteObject(pen);
    }
}

fn read_only_value(model: &Model, key: &str) -> String {
    let v = model.saved_value(key);
    let t = value_text(&v);
    if t.is_empty() { "(비어 있음)".to_string() } else { t }
}

/// 한 항목 줄.
#[allow(clippy::too_many_arguments)]
unsafe fn draw_row(hdc: HDC, f: &Fonts, lay: &Layout, model: &Model, ui: &Ui, pal: &Palette, n: usize, item_idx: usize, searching: bool) {
    unsafe {
        let dpi = lay.dpi;
        let item = &model.items[item_idx];
        let row = lay.row_rect(n, ui.scroll);
        let focused = ui.zone == Zone::List && ui.focus_row == n;
        let hovered = ui.hover == Hit::Row(n);
        boxed(hdc, &row, if hovered { pal.hover } else { pal.tab_bg }, pal.divider, 1, scale(10, dpi));
        let parts = lay.row_parts(row, shape_of(&item.control));
        let changed = model.is_changed(&item.key);
        let error = model.error_of(&item.key);
        // 변경 점 ●
        if changed {
            let pen = CreatePen(PS_SOLID, 1, pal.accent);
            let brush = CreateSolidBrush(pal.accent);
            let (op, ob) = (SelectObject(hdc, pen), SelectObject(hdc, brush));
            Ellipse(hdc, parts.dot.left, parts.dot.top, parts.dot.right, parts.dot.bottom);
            SelectObject(hdc, op);
            SelectObject(hdc, ob);
            DeleteObject(pen);
            DeleteObject(brush);
        }
        let label_color = if matches!(item.control, Control::ReadOnly(_)) { pal.text_muted } else { pal.text };
        text(hdc, f.bold, &parts.label, &item.label, label_color, ONE_LINE);
        if searching {
            // 검색 결과 줄에는 묶음 이름을 붙인다.
            let w = text_width(hdc, f.bold, &item.label) + scale(10, dpi);
            let tag = Rect::new(parts.label.left + w, parts.label.top, parts.label.right, parts.label.bottom);
            if tag.width() > scale(40, dpi) {
                text(hdc, f.small, &tag, &format!("· {}", GROUPS[item.group].title), pal.text_faint, ONE_LINE);
            }
        }
        if !item.summary.is_empty() {
            text(hdc, f.small, &parts.summary, &item.summary, pal.text_sub, DT_LEFT | DT_WORDBREAK | DT_END_ELLIPSIS | DT_EDITCONTROL);
        } else {
            text(hdc, f.small, &parts.summary, &format!("키: {}", item.key), pal.text_faint, ONE_LINE);
        }
        // 컨트롤
        let err_border = if error.is_some() { pal.close_hover } else { pal.border };
        match &item.control {
            Control::Check => {
                let on = model.current_bool(&item.key);
                let b = scale(24, dpi);
                let box_r = Rect::new(parts.control.left, parts.control.top + (parts.control.height() - b) / 2, parts.control.left + b, parts.control.top + (parts.control.height() - b) / 2 + b);
                draw_checkbox(hdc, &box_r, on, pal, dpi);
                let lab = Rect::new(box_r.right + scale(10, dpi), parts.control.top, parts.control.right, parts.control.bottom);
                text(hdc, f.normal, &lab, if on { "켬" } else { "끔" }, pal.text, ONE_LINE);
                if focused {
                    let fr = Rect::new(parts.control.left - scale(4, dpi), parts.control.top, parts.control.right, parts.control.bottom);
                    draw_focus(hdc, &fr, pal, dpi, scale(8, dpi));
                }
            }
            Control::Number { .. } | Control::Text => {
                boxed(hdc, &parts.field, pal.bg, err_border, scale(if error.is_some() { 2 } else { 1 }, dpi), scale(8, dpi));
                let inner = Rect::new(parts.field.left + scale(10, dpi), parts.field.top, parts.field.right - scale(8, dpi), parts.field.bottom);
                text(hdc, f.normal, &inner, &model.current_text(&item.key), pal.text, ONE_LINE);
                if matches!(item.control, Control::Number { .. }) {
                    for (r, glyph) in [(&parts.minus, "−"), (&parts.plus, "+")] {
                        boxed(hdc, r, pal.bg, pal.border, 1, scale(8, dpi));
                        text(hdc, f.bold, r, glyph, pal.text, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                    }
                }
                if focused {
                    let fr = Rect::new(parts.control.left - 3, parts.control.top - 3, parts.control.right + 3, parts.control.bottom + 3);
                    draw_focus(hdc, &fr, pal, dpi, scale(10, dpi));
                }
            }
            Control::Choice(options) => {
                boxed(hdc, &parts.field, pal.bg, pal.border, 1, scale(8, dpi));
                let now = model.current_text(&item.key);
                let label = options.iter().find(|o| o.0 == now).map(|o| o.1.clone()).unwrap_or(now);
                let inner = Rect::new(parts.field.left + scale(10, dpi), parts.field.top, parts.field.right - scale(30, dpi), parts.field.bottom);
                text(hdc, f.normal, &inner, &label, pal.text, ONE_LINE);
                draw_triangle(hdc, parts.field.right - scale(16, dpi), parts.field.top + parts.field.height() / 2, scale(5, dpi), pal.icon);
                if focused {
                    let fr = Rect::new(parts.control.left - 3, parts.control.top - 3, parts.control.right + 3, parts.control.bottom + 3);
                    draw_focus(hdc, &fr, pal, dpi, scale(10, dpi));
                }
            }
            Control::ReadOnly(why) => {
                boxed(hdc, &parts.field, pal.bg, pal.divider, 1, scale(8, dpi));
                let inner = Rect::new(parts.field.left + scale(10, dpi), parts.field.top, parts.field.right - scale(8, dpi), parts.field.bottom);
                text(hdc, f.normal, &inner, &read_only_value(model, &item.key), pal.text_faint, ONE_LINE);
                chip(hdc, f.small, &parts.chip, why.chip(), pal.toggle_on_bg, pal.toggle_on_text, true, dpi);
                text(hdc, f.small, &parts.extra, why.note(), pal.text_faint, ONE_LINE);
                if focused {
                    draw_focus(hdc, &row, pal, dpi, scale(10, dpi));
                }
            }
        }
        if let Some(why) = error {
            chip(hdc, f.small, &parts.extra, &format!("오류: {why}"), pal.close_hover, pal.on_close_hover, false, dpi);
        } else if matches!(item.control, Control::Check | Control::Number { .. } | Control::Text | Control::Choice(_)) && !item.summary.is_empty() {
            // 기본값 안내(편집 가능한 항목).
            let d = match &item.control {
                Control::Check => if item.default.as_bool().unwrap_or(false) { "켬".to_string() } else { "끔".to_string() },
                _ => value_text(&item.default),
            };
            let d = if d.is_empty() { "(비어 있음)".to_string() } else { d };
            text(hdc, f.small, &parts.extra, &format!("기본값: {d}"), pal.text_faint, ONE_LINE);
        }
    }
}

unsafe fn draw_button(hdc: HDC, f: &Fonts, r: &Rect, label: &str, primary: bool, enabled: bool, focused: bool, pal: &Palette, dpi: u32) {
    unsafe {
        let (bg, fg, border) = if !enabled {
            (pal.bg, pal.text_faint, pal.divider)
        } else if primary {
            (pal.accent, pal.bg, pal.accent)
        } else {
            (pal.bg, pal.text, pal.border)
        };
        boxed(hdc, r, bg, border, 1, scale(10, dpi));
        text(hdc, f.bold, r, label, fg, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        if focused {
            let g = Rect::new(r.left - 3, r.top - 3, r.right + 3, r.bottom + 3);
            draw_focus(hdc, &g, pal, dpi, scale(12, dpi));
        }
    }
}

/// 전체 화면을 `hdc` 에 그린다(0,0 기준, 크기 `w`×`h`).
pub fn draw(hdc: HDC, w: i32, h: i32, model: &Model, ui: &Ui, pal: &Palette, dpi: u32) {
    unsafe {
        let lay = compute(w, h, dpi);
        let f = Fonts::new(dpi);
        fill(hdc, &Rect::new(0, 0, w, h), pal.bg);

        // 머리 띠: 제목 + 검색 상자
        fill(hdc, &lay.header, pal.head_bg);
        hline(hdc, 0, w, lay.header.bottom - 1, pal.divider);
        let title = Rect::new(scale(20, dpi), 0, lay.tree.right - scale(8, dpi), lay.header.bottom);
        text(hdc, f.title, &title, "설정", pal.text, ONE_LINE);
        let focus_search = ui.zone == Zone::Search;
        boxed(hdc, &lay.search, pal.bg, if focus_search { pal.accent } else { pal.border }, scale(if focus_search { 2 } else { 1 }, dpi), scale(10, dpi));
        let icon = Rect::new(lay.search.left + scale(12, dpi), lay.search.top, lay.search.left + scale(30, dpi), lay.search.bottom);
        draw_search_icon(hdc, &icon, pal.icon, dpi);
        let input = Rect::new(lay.search.left + scale(38, dpi), lay.search.top, lay.search.right - scale(10, dpi), lay.search.bottom);
        if model.search.is_empty() {
            text(hdc, f.normal, &input, "항목 이름·설명 검색 (2글자부터 바로 찾아요)", pal.text_faint, ONE_LINE);
        } else {
            text(hdc, f.normal, &input, &model.search, pal.text, ONE_LINE);
        }

        let searching = model.search_active();
        // 트리
        fill(hdc, &lay.tree, pal.tab_bg);
        vline(hdc, lay.tree.right - 1, lay.tree.top, lay.tree.bottom, pal.divider);
        for (i, g) in GROUPS.iter().enumerate() {
            let r = lay.tree_row(i);
            let selected = !searching && i == model.group;
            let hits = model.group_count(i);
            let dim = searching && hits == 0;
            if selected {
                boxed(hdc, &r, pal.selected, pal.selected, 1, scale(10, dpi));
                fill(hdc, &Rect::new(r.left, r.top + scale(8, dpi), r.left + scale(4, dpi), r.bottom - scale(8, dpi)), pal.accent_line);
            } else if ui.hover == Hit::Tree(i) {
                boxed(hdc, &r, pal.hover, pal.hover, 1, scale(10, dpi));
            }
            let tc = pal.row_text(selected, if dim { pal.text_faint } else { pal.tab_text });
            let sc = pal.row_text(selected, pal.text_faint);
            let t = Rect::new(r.left + scale(16, dpi), r.top + scale(3, dpi), r.right - scale(52, dpi), r.top + scale(24, dpi));
            text(hdc, if selected { f.bold } else { f.normal }, &t, g.title, tc, ONE_LINE);
            let sub = Rect::new(t.left, t.bottom - scale(2, dpi), t.right, r.bottom - scale(2, dpi));
            text(hdc, f.small, &sub, g.subtitle, sc, ONE_LINE);
            let changed = model.group_changed(i);
            let cnt = Rect::new(r.right - scale(48, dpi), r.top, r.right - scale(10, dpi), r.bottom);
            let label = if changed > 0 && !searching { format!("● {hits}") } else { hits.to_string() };
            text(hdc, f.small, &cnt, &label, pal.row_text(selected, if changed > 0 && !searching { pal.accent } else { pal.text_faint }), DT_RIGHT | DT_VCENTER | DT_SINGLELINE);
            if ui.zone == Zone::Tree && i == model.group && !searching {
                draw_focus(hdc, &r, pal, dpi, scale(10, dpi));
            }
        }

        // 목록
        let visible = model.visible();
        let saved = SaveDC(hdc);
        IntersectClipRect(hdc, lay.list.left, lay.list.top, lay.list.right, lay.list.bottom);
        if visible.is_empty() {
            let r = Rect::new(lay.list.left, lay.list.top + scale(40, dpi), lay.list.right, lay.list.top + scale(80, dpi));
            text(hdc, f.normal, &r, "검색 결과가 없습니다.", pal.text_muted, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        }
        for (n, idx) in visible.iter().enumerate() {
            let row = lay.row_rect(n, ui.scroll);
            if row.bottom < lay.list.top || row.top > lay.list.bottom {
                continue;
            }
            draw_row(hdc, &f, &lay, model, ui, pal, n, *idx, searching);
        }
        // 스크롤 막대
        let max = lay.max_scroll(visible.len());
        if max > 0 {
            let track_h = lay.list.height();
            let thumb_h = (track_h * track_h / lay.content_height(visible.len())).max(scale(24, dpi));
            let top = lay.list.top + (track_h - thumb_h) * ui.scroll / max;
            let r = Rect::new(lay.list.right - scale(8, dpi), top, lay.list.right - scale(3, dpi), top + thumb_h);
            boxed(hdc, &r, pal.border, pal.border, 1, scale(5, dpi));
        }
        RestoreDC(hdc, saved);

        // 바닥 띠: 상태 + [되돌리기][저장]
        fill(hdc, &lay.footer, pal.head_bg);
        hline(hdc, 0, w, lay.footer.top, pal.divider);
        let msg = Rect::new(scale(20, dpi), lay.footer.top, lay.revert_btn.left - scale(16, dpi), lay.footer.bottom);
        let changed = model.changed_count();
        match &model.status {
            Some(s) if s.error => {
                let area = Rect::new(msg.left, msg.top + scale(14, dpi), msg.right, msg.bottom - scale(14, dpi));
                chip(hdc, f.small, &area, &s.text, pal.close_hover, pal.on_close_hover, false, dpi);
            }
            Some(s) => text(hdc, f.normal, &msg, &s.text, pal.text, ONE_LINE),
            None if changed > 0 => {
                let errs = model.error_count();
                let t = if errs > 0 { format!("● 변경 {changed}건 · 입력 오류 {errs}건") } else { format!("● 변경 {changed}건 (저장 전)") };
                text(hdc, f.normal, &msg, &t, pal.accent, ONE_LINE);
            }
            None => text(hdc, f.normal, &msg, "변경한 항목이 없습니다.", pal.text_muted, ONE_LINE),
        }
        let can_save = changed > 0 && model.error_count() == 0;
        draw_button(hdc, &f, &lay.revert_btn, "되돌리기", false, changed > 0, ui.zone == Zone::Revert, pal, dpi);
        draw_button(hdc, &f, &lay.save_btn, "저장", true, can_save, ui.zone == Zone::Save, pal, dpi);
        f.free();
    }
}
