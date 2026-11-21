//! 그리기(GDI). 실제 창과 캡처 모드가 같은 `draw` 를 쓴다. 색은 `ui_theme::Palette` 와 확장자 색표(Flutter 와 같음)만 쓴다.
//! 가상화: 화면에 걸리는 칸(`Metrics::visible_range`)만 그린다 — 항목이 수만이어도 한 번에 수십 칸.
use crate::layout::{BANNER_H, Rect, Toolbar, scale};
use crate::model::{App, ScanState, badge_text, photo_color, format_date, format_elapsed, format_size, group_digits};
use crate::settings::{MAX_THUMB, MIN_THUMB};
use grid_core::gdi::*;
use std::ptr::null_mut;
use ui_theme::Palette;
use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::Graphics::Gdi::*;

pub use grid_core::fmt::blend;
/// 한 칸: 자리표시 썸네일(THUMBRS 전·해와 산 그림) + 확장자 배지 + 파일 이름 + 크기·수정일.
unsafe fn draw_card(hdc: HDC, f: &Fonts, r: &Rect, d: &crate::model::Doc, selected: bool, tz: i64, pal: &Palette, dpi: u32) {
    unsafe {
        let s = |v: i32| scale(v, dpi);
        let col = photo_color(&d.ext);
        let (bg, border) = if selected { (pal.selected, pal.accent) } else { (pal.bg, pal.divider) };
        boxed(hdc, r, bg, border, if selected { s(2) } else { 1 }, s(10));
        let info_h = s(44);
        let thumb = Rect::new(r.left + s(6), r.top + s(6), r.right - s(6), (r.bottom - info_h - s(2)).max(r.top + s(20)));
        boxed(hdc, &thumb, blend(pal.bg, col, 0.10), blend(pal.bg, col, 0.22), 1, s(6));
        // 사진 모양 자리표시(실제 썸네일은 THUMBRS 후속): 해 + 산 두 개. 칸 모양에 맞게 안쪽 영역에 그린다.
        let ink = blend(pal.bg, col, 0.45);
        let inner = Rect::new(thumb.left + thumb.width() / 6, thumb.top + thumb.height() / 5, thumb.right - thumb.width() / 6, thumb.bottom - thumb.height() / 5);
        let (iw, ih) = (inner.width(), inner.height());
        let sun = (ih.min(iw) / 6).max(s(3));
        let (sx, sy) = (inner.left + iw * 72 / 100, inner.top + ih * 24 / 100);
        let (pen, br) = (CreatePen(PS_SOLID, 1, ink), CreateSolidBrush(ink));
        let (op, ob) = (SelectObject(hdc, pen), SelectObject(hdc, br));
        Ellipse(hdc, sx - sun, sy - sun, sx + sun, sy + sun);
        let base = inner.bottom;
        let hill = |x0: i32, x1: i32, peak_x: i32, peak_y: i32| {
            let pts = [POINT { x: x0, y: base }, POINT { x: peak_x, y: peak_y }, POINT { x: x1, y: base }];
            Polygon(hdc, pts.as_ptr(), 3);
        };
        hill(inner.left, inner.left + iw * 62 / 100, inner.left + iw * 30 / 100, inner.top + ih * 42 / 100);
        hill(inner.left + iw * 34 / 100, inner.right, inner.left + iw * 68 / 100, inner.top + ih * 58 / 100);
        SelectObject(hdc, op);
        SelectObject(hdc, ob);
        DeleteObject(pen);
        DeleteObject(br);
        // 확장자 배지(왼쪽 아래).
        let label = badge_text(&d.ext);
        let bw = text_width(hdc, f.badge, &label) + s(12);
        let badge = Rect::new(thumb.left + s(6), thumb.bottom - s(24), thumb.left + s(6) + bw, thumb.bottom - s(6));
        boxed(hdc, &badge, col, col, 1, s(8));
        text(hdc, f.badge, &badge, &label, 0x00FF_FFFF, CENTER);
        // 파일 이름(한 줄)·크기·수정일.
        let name = Rect::new(r.left + s(10), r.bottom - info_h, r.right - s(10), r.bottom - s(24));
        text(hdc, f.bold, &name, &d.name, pal.text, ONE_LINE);
        let mut meta = format!("{} · {}", format_size(d.size), format_date(d.mtime, tz));
        if d.cloud {
            meta.push_str(" · 클라우드");
        }
        let m = Rect::new(r.left + s(10), r.bottom - s(23), r.right - s(10), r.bottom - s(5));
        // 칸이 좁으면(격자 전체가 같게) 연도를 두 자리로 줄인다(`26-06-07`).
        if m.width() < s(135) {
            let short = format_date(d.mtime, tz);
            meta = meta.replacen(&short, &short[2..], 1);
        }
        text(hdc, f.small, &m, &meta, pal.text_sub, ONE_LINE);
    }
}

unsafe fn draw_toolbar(hdc: HDC, f: &Fonts, tb: &Toolbar, app: &App, pal: &Palette, dpi: u32) {
    unsafe {
        let s = |v: i32| scale(v, dpi);
        fill(hdc, &tb.bar, pal.tab_bg);
        line(hdc, 0, tb.bar.bottom - 1, tb.bar.right, tb.bar.bottom - 1, pal.divider, 1);
        // 검색 칸.
        let (border, bw) = if app.search_focus { (pal.accent, 2) } else { (pal.divider, 1) };
        boxed(hdc, &tb.search, pal.bg, border, bw, s(8));
        let inner = Rect::new(tb.search.left + s(10), tb.search.top, tb.search.right - s(8), tb.search.bottom);
        if app.coll.query.is_empty() {
            text(hdc, f.normal, &inner, "파일 이름 검색", pal.text_faint, ONE_LINE);
        } else {
            text(hdc, f.normal, &inner, &app.coll.query, pal.text, ONE_LINE);
        }
        if app.search_focus {
            let x = inner.left + text_width(hdc, f.normal, &app.coll.query).min(inner.width() - s(2));
            line(hdc, x, tb.search.top + s(7), x, tb.search.bottom - s(7), pal.text, 1);
        }
        // 정렬.
        let arrow = if app.coll.descending { "▼" } else { "▲" };
        button(hdc, f, &tb.sort, &format!("정렬: {} {arrow}", app.coll.sort.label()), true, false, pal, dpi);
        // 개수 알약(사진 개수 알약).
        let n = if app.coll.query.trim().is_empty() { group_digits(app.coll.total()) } else { format!("{} / {}", group_digits(app.coll.len()), group_digits(app.coll.total())) };
        let chip = Rect::new(tb.count.left, tb.count.top + s(3), tb.count.right, tb.count.bottom - s(3));
        boxed(hdc, &chip, blend(pal.tab_bg, pal.accent, 0.18), blend(pal.tab_bg, pal.accent, 0.18), 1, s(14));
        text(hdc, f.bold, &chip, &format!("사진 {n}장"), pal.accent, CENTER);
        // 썸네일 크기 슬라이더.
        text(hdc, f.small, &tb.slider_label, "크기", pal.text_sub, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
        let cy = (tb.slider.top + tb.slider.bottom) / 2;
        let (x0, x1) = (tb.slider_x(MIN_THUMB), tb.slider_x(MAX_THUMB));
        let xv = tb.slider_x(app.thumb);
        line(hdc, x0, cy, x1, cy, pal.divider, s(4));
        line(hdc, x0, cy, xv, cy, pal.accent, s(4));
        let rr = s(8);
        boxed(hdc, &Rect::new(xv - rr, cy - rr, xv + rr, cy + rr), pal.accent, pal.bg, s(2), rr * 2);
        // 제외 폴더·새로고침/취소.
        let ex = if app.excluded_count > 0 { format!("제외 폴더 ({})", app.excluded_count) } else { "제외 폴더".to_string() };
        button(hdc, f, &tb.exclude, &ex, true, false, pal, dpi);
        let (label, enabled) = match app.scan.state {
            ScanState::Running => ("스캔 취소", true),
            ScanState::Cancelling => ("취소 중…", false),
            _ => ("새로고침", true),
        };
        button(hdc, f, &tb.action, label, enabled, !app.scan.busy(), pal, dpi);
    }
}

unsafe fn draw_banner(hdc: HDC, f: &Fonts, w: i32, app: &App, pal: &Palette, dpi: u32) {
    unsafe {
        let s = |v: i32| scale(v, dpi);
        let top = s(crate::layout::TOOLBAR_H);
        let r = Rect::new(0, top, w, top + s(BANNER_H));
        fill(hdc, &r, blend(pal.bg, pal.accent, 0.10));
        line(hdc, 0, r.bottom - 1, w, r.bottom - 1, pal.divider, 1);
        let head = if app.scan.state == ScanState::Cancelling { "취소하는 중" } else { "사진 검색 중" };
        let mut msg = format!("{head} · 훑은 사진 {}장 · {}", group_digits(app.scan.visited), format_elapsed(app.scan.elapsed_secs));
        if !app.scan.folder.is_empty() {
            msg.push_str(&format!(" · 현재 폴더: {}", app.scan.folder));
        }
        text(hdc, f.normal, &Rect::new(s(16), r.top, w - s(16), r.bottom), &msg, pal.text, ONE_LINE);
    }
}

unsafe fn draw_empty(hdc: HDC, f: &Fonts, vp: &Rect, app: &App, pal: &Palette, dpi: u32) {
    unsafe {
        let s = |v: i32| scale(v, dpi);
        let (head, sub, color): (String, String, u32) = match &app.scan.state {
            ScanState::Running | ScanState::Cancelling => (
                format!("사진 검색 중 · 훑은 사진 {}장 · {}", group_digits(app.scan.visited), format_elapsed(app.scan.elapsed_secs)),
                if app.scan.folder.is_empty() { String::new() } else { format!("현재 폴더: {}", app.scan.folder) },
                pal.text,
            ),
            ScanState::Failed(e) => ("사진 목록을 불러오지 못했습니다.".into(), e.clone(), ui_theme::rgb(0xDC, 0x26, 0x26)),
            _ if app.coll.total() > 0 => ("검색 결과가 없습니다.".into(), "다른 검색어를 입력해 보세요.".into(), pal.text),
            ScanState::Idle => ("모든 고정 드라이브의 사진을 확인합니다.".into(), "지원 형식: JPG, JPEG, PNG, WEBP, BMP, TIF, TIFF, HEIC".into(), pal.text),
            _ => ("사진이 없습니다.".into(), "폴더에 JPG, PNG, HEIC 등 사진 파일이 없습니다.".into(), pal.text),
        };
        let mid = (vp.top + vp.bottom) / 2;
        text(hdc, f.title, &Rect::new(vp.left + s(24), mid - s(30), vp.right - s(24), mid), &head, color, CENTER);
        text(hdc, f.normal, &Rect::new(vp.left + s(24), mid + s(4), vp.right - s(24), mid + s(34)), &sub, pal.text_muted, CENTER);
    }
}

unsafe fn draw_status(hdc: HDC, f: &Fonts, w: i32, h: i32, app: &App, pal: &Palette, dpi: u32) {
    unsafe {
        let s = |v: i32| scale(v, dpi);
        let r = Rect::new(0, h - s(crate::layout::STATUS_H), w, h);
        fill(hdc, &r, pal.tab_bg);
        line(hdc, 0, r.top, w, r.top, pal.divider, 1);
        let engine = if app.engine.is_empty() { "-" } else { &app.engine };
        let mut left = format!("엔진: {engine} · 사진 {}장", group_digits(app.coll.total()));
        if !app.coll.query.trim().is_empty() {
            left.push_str(&format!(" (검색 {}장)", group_digits(app.coll.len())));
        }
        let st = match &app.scan.state {
            ScanState::Idle => String::new(),
            ScanState::Running => format!(" · 검색 중 {}", format_elapsed(app.scan.elapsed_secs)),
            ScanState::Cancelling => " · 취소하는 중".into(),
            ScanState::Complete => format!(" · 완료 {}", format_elapsed(app.scan.elapsed_secs)),
            ScanState::Cancelled => format!(" · 취소됨(그때까지 {}장)", group_digits(app.scan.visited)),
            ScanState::Failed(_) => " · 실패".into(),
        };
        left.push_str(&st);
        let lw = text_width(hdc, f.small, &left) + s(24);
        text(hdc, f.small, &Rect::new(s(16), r.top, s(16) + lw, r.bottom), &left, pal.text_sub, ONE_LINE);
        let right = match app.selected_doc() {
            Some(d) => d.path.clone(),
            None => app.notice.clone(),
        };
        text(hdc, f.small, &Rect::new(s(16) + lw, r.top, w - s(16), r.bottom), &right, pal.text_muted, DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_PATH_ELLIPSIS);
    }
}

/// 전체 그리기. 반환: 이번에 실제로 그린 칸 수(가상화 확인용).
pub fn draw(hdc: HDC, w: i32, h: i32, app: &App, pal: &Palette, dpi: u32) -> usize {
    unsafe {
        let f = Fonts::new(dpi);
        let banner = app.banner_shown();
        let m = app.metrics(w, h, dpi);
        fill(hdc, &Rect::new(0, 0, w, h), pal.head_bg);
        let mut drawn = 0;
        if app.coll.is_empty() {
            let mut vp = m.viewport;
            vp.top = scale(crate::layout::TOOLBAR_H, dpi);
            draw_empty(hdc, &f, &vp, app, pal, dpi);
        } else {
            let clip = CreateRectRgn(m.viewport.left, m.viewport.top, m.viewport.right, m.viewport.bottom);
            SelectClipRgn(hdc, clip);
            for i in m.visible_range(app.scroll, app.coll.len()) {
                if let Some(d) = app.coll.get(i) {
                    draw_card(hdc, &f, &m.item_rect(i, app.scroll), d, app.selected == Some(i), app.tz_offset, pal, dpi);
                    drawn += 1;
                }
            }
            SelectClipRgn(hdc, null_mut());
            DeleteObject(clip);
            if let Some(t) = m.scrollbar_thumb(app.scroll, app.coll.len()) {
                fill(hdc, &m.scrollbar_track(), blend(pal.head_bg, pal.divider, 0.35));
                boxed(hdc, &t, pal.text_faint, pal.text_faint, 1, t.width());
            }
        }
        if banner {
            draw_banner(hdc, &f, w, app, pal, dpi);
        }
        draw_toolbar(hdc, &f, &Toolbar::new(w, dpi), app, pal, dpi);
        draw_status(hdc, &f, w, h, app, pal, dpi);
        f.free();
        drawn
    }
}
