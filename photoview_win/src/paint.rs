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
unsafe fn draw_card(hdc: HDC, f: &Fonts, r: &Rect, d: &crate::model::Doc, selected: bool, tz: i64, pal: &Palette, dpi: u32, bitmap: Option<&grid_core::shell_thumb::Bitmap>) {
    unsafe {
        let s = |v: i32| scale(v, dpi);
        let col = photo_color(&d.ext);
        let (bg, border) = if selected { (pal.selected, pal.accent) } else { (pal.bg, pal.divider) };
        boxed(hdc, r, bg, border, if selected { s(2) } else { 1 }, s(10));
        let info_h = s(44);
        let thumb = Rect::new(r.left + s(6), r.top + s(6), r.right - s(6), (r.bottom - info_h - s(2)).max(r.top + s(20)));
        boxed(hdc, &thumb, blend(pal.bg, col, 0.10), blend(pal.bg, col, 0.22), 1, s(6));
        if let Some(bitmap) = bitmap { draw_shell_bitmap(hdc, &thumb, bitmap); }
        // 사진 모양 자리표시(실제 썸네일은 THUMBRS 후속): 해 + 산 두 개. 칸 모양에 맞게 안쪽 영역에 그린다.
        if bitmap.is_none() {
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
        }
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

unsafe fn draw_shell_bitmap(hdc: HDC, r: &Rect, bitmap: &grid_core::shell_thumb::Bitmap) {
    if bitmap.width == 0 || bitmap.height == 0 || bitmap.bgra.len() < bitmap.width as usize * bitmap.height as usize * 4 { return; }
    let mut info: BITMAPINFO = unsafe { std::mem::zeroed() };
    info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = bitmap.width as i32;
    info.bmiHeader.biHeight = -(bitmap.height as i32);
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;
    unsafe { StretchDIBits(hdc, r.left, r.top, r.width(), r.height(), 0, 0, bitmap.width as i32, bitmap.height as i32, bitmap.bgra.as_ptr().cast(), &mut info, DIB_RGB_COLORS, SRCCOPY); }
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
        left.push_str(&format!(" · {}개 선택", app.selection.paths.len()));
        let lw = text_width(hdc, f.small, &left) + s(24);
        text(hdc, f.small, &Rect::new(s(16), r.top, s(16) + lw, r.bottom), &left, pal.text_sub, ONE_LINE);
        let right = match app.selected_doc() {
            _ if !app.open_error.is_empty() => app.open_error.clone(),
            _ if !app.file_status.is_empty() => app.file_status.clone(),
            Some(d) => d.path.clone(),
            None => app.notice.clone(),
        };
        text(hdc, f.small, &Rect::new(s(16) + lw, r.top, w - s(16), r.bottom), &right, pal.text_muted, DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_PATH_ELLIPSIS);
    }
}

/// 제외 폴더 편집 패널(창 위에 떠 있는 모달 상자). 뒤 화면은 그대로 두고 그림자 테두리로만 띄운다.
unsafe fn draw_panel(hdc: HDC, f: &Fonts, w: i32, h: i32, p: &crate::panel::ExcludePanel, pal: &Palette, dpi: u32) {
    use crate::panel::{PanelLayout, Tone};
    unsafe {
        let s = |v: i32| scale(v, dpi);
        let l = PanelLayout::new(w, h, dpi);
        // 그림자 + 상자.
        let d = l.dialog;
        boxed(hdc, &Rect::new(d.left + s(4), d.top + s(6), d.right + s(4), d.bottom + s(6)), blend(pal.head_bg, 0x0000_0000, 0.35), blend(pal.head_bg, 0x0000_0000, 0.35), 1, s(14));
        boxed(hdc, &d, pal.bg, pal.accent, s(2), s(14));
        text(hdc, f.title, &l.title, "제외할 폴더", pal.text, ONE_LINE);
        text(hdc, f.small, &l.hint, "여기에 추가한 폴더(와 그 아래 폴더)는 사진 검색에서 빠집니다.
windows·program files·appdata·node_modules·.git 등은 항상 제외됩니다.", pal.text_sub, DT_LEFT | DT_WORDBREAK);
        // 목록.
        boxed(hdc, &l.list, pal.head_bg, pal.divider, 1, s(8));
        if p.items.is_empty() {
            text(hdc, f.normal, &l.list, "제외한 폴더가 없습니다.", pal.text_muted, CENTER);
        } else {
            let clip = CreateRectRgn(l.list.left + 1, l.list.top + 1, l.list.right - 1, l.list.bottom - 1);
            SelectClipRgn(hdc, clip);
            for row in 0..l.visible_rows() {
                let i = p.scroll + row;
                let Some(item) = p.items.get(i) else { break };
                let r = l.row_rect(row);
                let inner = Rect::new(r.left + s(4), r.top + s(2), r.right - s(4), r.bottom - s(2));
                if p.selected == Some(i) {
                    boxed(hdc, &inner, pal.selected, pal.accent, 1, s(6));
                }
                text(hdc, f.normal, &Rect::new(r.left + s(14), r.top, r.right - s(12), r.bottom), item, pal.text, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_PATH_ELLIPSIS);
            }
            SelectClipRgn(hdc, null_mut());
            DeleteObject(clip);
            // 목록이 더 길면 오른쪽에 가는 스크롤 표시.
            let vis = l.visible_rows();
            if p.items.len() > vis {
                let track = Rect::new(l.list.right - s(8), l.list.top + s(4), l.list.right - s(4), l.list.bottom - s(4));
                let th = (track.height() * vis as i32 / p.items.len() as i32).max(s(16));
                let top = track.top + (track.height() - th) * p.scroll as i32 / (p.items.len() - vis).max(1) as i32;
                boxed(hdc, &Rect::new(track.left, top, track.right, top + th), pal.text_faint, pal.text_faint, 1, track.width());
            }
        }
        // 안내 줄(저장 결과·충돌·오류).
        let tone_color = if p.tone == Tone::Error { ui_theme::rgb(0xDC, 0x26, 0x26) } else { pal.text_sub };
        let note = if p.message.is_empty() && p.changed { "닫으면 바뀐 제외 폴더로 새로 검색합니다." } else { p.message.as_str() };
        text(hdc, f.small, &l.message, note, tone_color, DT_LEFT | DT_VCENTER | DT_WORDBREAK | DT_END_ELLIPSIS);
        button(hdc, f, &l.add, "폴더 추가…", true, true, pal, dpi);
        button(hdc, f, &l.remove, "선택 삭제", p.selected.is_some(), false, pal, dpi);
        button(hdc, f, &l.close, "닫기", true, false, pal, dpi);
    }
}

/// 전체 그리기. 반환: 이번에 실제로 그린 칸 수(가상화 확인용).
pub fn draw(hdc: HDC, w: i32, h: i32, app: &App, pal: &Palette, dpi: u32) -> usize {
    draw_with_panel(hdc, w, h, app, None, pal, dpi)
}

/// `draw` + (있으면) 제외 폴더 편집 패널. 실제 창과 캡처 모드가 같은 함수를 쓴다.
pub fn draw_with_panel(hdc: HDC, w: i32, h: i32, app: &App, panel: Option<&crate::panel::ExcludePanel>, pal: &Palette, dpi: u32) -> usize {
    draw_with_shell_thumbs(hdc, w, h, app, panel, pal, dpi, &std::collections::HashMap::new())
}

pub fn draw_with_shell_thumbs(hdc: HDC, w: i32, h: i32, app: &App, panel: Option<&crate::panel::ExcludePanel>, pal: &Palette, dpi: u32, thumbs: &std::collections::HashMap<String, grid_core::shell_thumb::Bitmap>) -> usize {
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
                    draw_card(hdc, &f, &m.item_rect(i, app.scroll), d, app.is_selected(i), app.tz_offset, pal, dpi, thumbs.get(&d.path));
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
        if let Some(p) = panel {
            draw_panel(hdc, &f, w, h, p, pal, dpi);
        }
        f.free();
        drawn
    }
}
