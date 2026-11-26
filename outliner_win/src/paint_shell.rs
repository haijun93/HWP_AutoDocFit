//! 페이지 모드 그리기(GDI): 위쪽 줄·왼쪽 페이지 목록·검색 결과·오른쪽 역링크 패널·`[[` 자동완성 팝업.
//! 가운데 블록 목록은 기존 `paint::draw_ext` 를 뷰포트로 옮겨 그대로 쓴다(그래서 한 파일 방식과 같은 그림).
use crate::edit::EditHost;
use crate::layout::{self, Layout, LeftItem, R, RightItem};
use crate::paint::{Extras, Metrics, draw_ext, fill, scale, text, wide};
use crate::shell::{PromptKind, Shell};
use std::ptr::null_mut;
use ui_theme::Palette;
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::Graphics::Gdi::*;

fn rc(r: R) -> RECT {
    RECT { left: r.l, top: r.t, right: r.r, bottom: r.b }
}

unsafe fn mkfont(px: i32, bold: bool) -> HFONT {
    unsafe {
        CreateFontW(
            -px,
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

unsafe fn frame(dc: HDC, r: R, color: u32) {
    unsafe {
        let brush = CreateSolidBrush(color);
        FrameRect(dc, &rc(r), brush);
        DeleteObject(brush);
    }
}

fn inset(r: R, d: i32) -> R {
    R::new(r.l + d, r.t, r.r - d, r.b)
}

/// 창 전체를 그린다. `extras` 는 가운데 목록(편집 줄·안내·●)용.
pub unsafe fn draw_shell<H: EditHost>(dc: HDC, w: i32, h: i32, shell: &Shell<H>, pal: &Palette, dpi: u32, extras: &Extras, lay: &Layout) {
    unsafe {
        let view = &shell.editor.view;
        let title = shell.page.clone().unwrap_or_default();
        if !lay.panels {
            draw_ext(dc, w, h, view, pal, dpi, &title, extras);
            return;
        }
        fill(dc, RECT { left: 0, top: 0, right: w, bottom: h }, pal.bg);
        SetBkMode(dc, TRANSPARENT as i32);
        // 가운데: 기존 그리기를 뷰포트로 옮겨 그린다.
        let c = lay.center;
        let saved = SaveDC(dc);
        SetViewportOrgEx(dc, c.l, c.t, null_mut());
        IntersectClipRect(dc, 0, 0, c.w(), c.h());
        draw_ext(dc, c.w(), c.h(), view, pal, dpi, &title, extras);
        RestoreDC(dc, saved);
        let pad = scale(10, dpi);
        let f = mkfont(scale(14, dpi), false);
        let fb = mkfont(scale(14, dpi), true);
        let old = SelectObject(dc, f);
        SetBkMode(dc, TRANSPARENT as i32);

        draw_top(dc, shell, pal, lay, f, fb);
        if let Some(l) = lay.left {
            draw_left(dc, shell, pal, lay, l, pad, f, fb);
        }
        if let Some(r) = lay.right {
            draw_right(dc, shell, pal, lay, r, pad, f, fb);
        }
        if shell.search_open {
            draw_results(dc, shell, pal, lay, pad, f, fb);
        }
        draw_popup(dc, shell, pal, lay, dpi, f);
        SelectObject(dc, old);
        DeleteObject(f);
        DeleteObject(fb);
    }
}

unsafe fn draw_top<H: EditHost>(dc: HDC, shell: &Shell<H>, pal: &Palette, lay: &Layout, f: HFONT, fb: HFONT) {
    unsafe {
        fill(dc, rc(lay.top), pal.head_bg);
        fill(dc, RECT { left: 0, top: lay.top.b - 1, right: lay.w, bottom: lay.top.b }, pal.divider);
        SelectObject(dc, fb);
        let label = match shell.prompt {
            Some(PromptKind::NewPage) => "새 이름",
            Some(PromptKind::Rename) => "바꿀 이름",
            None => match shell.search_mode {
                crate::shell::SearchMode::Quick => "찾기",
                crate::shell::SearchMode::Commands => "명령",
                _ => "검색",
            },
        };
        let mut lr = lay.search_label;
        lr.r += scale(24, lay.dpi) * shell.prompt.is_some() as i32;
        text(dc, label, rc(lr), pal.text_sub, 0);
        SelectObject(dc, f);
        // 입력 칸 바탕·테두리(실제 EDIT 가 그 위를 덮는다).
        let mut e = lay.search_edit;
        if shell.prompt.is_some() {
            e.l += scale(24, lay.dpi);
        }
        fill(dc, rc(e), pal.bg);
        frame(dc, e, if shell.prompt.is_some() { pal.accent } else { pal.divider });
        if shell.prompt.is_some() {
            let tip = R::new(scale(12, lay.dpi), scale(48, lay.dpi), lay.w - scale(8, lay.dpi), lay.top.b);
            text(dc, "Enter 확인 · Esc 취소", rc(tip), pal.text_sub, 0);
            return;
        }
        for (b, r) in layout::BUTTONS.iter().zip(lay.buttons.iter()) {
            let enabled = shell.page.is_some() || matches!(b, layout::Btn::Today | layout::Btn::New);
            fill(dc, rc(*r), pal.bg);
            frame(dc, *r, pal.divider);
            text(dc, b.label(), rc(*r), if enabled { pal.text } else { pal.text_faint }, DT_CENTER);
        }
        for (b, r) in layout::NAV_BUTTONS.iter().zip(lay.nav_buttons.iter()) {
            let enabled = match b {
                layout::Btn::Back => shell.can_back() || shell.editor.view.can_zoom_back(),
                layout::Btn::Forward => shell.can_forward(),
                layout::Btn::Favorite => shell.page.is_some(),
                _ => true,
            };
            let active = match b {
                layout::Btn::ToggleLeft => shell.left_open,
                layout::Btn::ToggleRight => shell.right_open,
                layout::Btn::Favorite => shell.page.as_ref().is_some_and(|p| shell.favorites.contains(p)),
                _ => false,
            };
            fill(dc, rc(*r), if active { pal.selected } else { pal.bg });
            frame(dc, *r, pal.divider);
            text(dc, b.label(), rc(*r), if enabled { pal.row_text(active, pal.text) } else { pal.text_faint }, DT_CENTER);
        }
    }
}

unsafe fn draw_left<H: EditHost>(dc: HDC, shell: &Shell<H>, pal: &Palette, lay: &Layout, l: R, pad: i32, f: HFONT, fb: HFONT) {
    unsafe {
        fill(dc, rc(l), pal.bg);
        fill(dc, RECT { left: l.r - 1, top: l.t, right: l.r, bottom: l.b }, pal.divider);
        let head = R::new(l.l, l.t, l.r - 1, l.t + lay.head_h);
        fill(dc, rc(head), pal.head_bg);
        SelectObject(dc, fb);
        text(dc, "페이지", rc(inset(head, pad)), pal.text, 0);
        SelectObject(dc, f);
        for (i, section) in layout::SECTIONS.iter().enumerate() {
            let y = head.b + i as i32 * lay.item_h;
            let row = R::new(l.l, y, l.r - 1, y + lay.item_h);
            let selected = shell.section == *section;
            if selected { fill(dc, rc(row), pal.selected); }
            text(dc, section.label(), rc(inset(row, pad)), pal.row_text(selected, pal.text), 0);
        }
        let saved = SaveDC(dc);
        IntersectClipRect(dc, l.l, lay.list_top(l), l.r, l.b);
        let items = shell.left_list();
        let slots = lay.left_slots() + 1;
        for (slot, item) in items.iter().skip(shell.left_first).take(slots).enumerate() {
            let top = lay.list_top(l) + slot as i32 * lay.item_h;
            let row = R::new(l.l, top, l.r - 1, top + lay.item_h);
            match item {
                LeftItem::Label(s) => {
                    SelectObject(dc, fb);
                    text(dc, s, rc(inset(row, pad)), pal.text_sub, 0);
                    SelectObject(dc, f);
                }
                LeftItem::Page(name) => {
                    let current = shell.page.as_deref() == Some(name.as_str());
                    if current {
                        fill(dc, rc(row), pal.selected);
                    }
                    let mut r = inset(row, pad);
                    r.l += scale(6, lay.dpi);
                    text(dc, name, rc(r), pal.row_text(current, pal.text), 0);
                }
            }
        }
        RestoreDC(dc, saved);
    }
}

unsafe fn draw_right<H: EditHost>(dc: HDC, shell: &Shell<H>, pal: &Palette, lay: &Layout, r: R, pad: i32, f: HFONT, fb: HFONT) {
    unsafe {
        fill(dc, rc(r), pal.bg);
        fill(dc, RECT { left: r.l, top: r.t, right: r.l + 1, bottom: r.b }, pal.divider);
        let head = R::new(r.l + 1, r.t, r.r, r.t + lay.head_h);
        fill(dc, rc(head), pal.head_bg);
        SelectObject(dc, fb);
        text(dc, &format!("카드 {} · 역링크 {}", shell.cards.len(), shell.back.len()), rc(inset(head, pad)), pal.text, 0);
        SelectObject(dc, f);
        if shell.back.is_empty() && shell.cards.is_empty() {
            let row = R::new(r.l + 1, lay.list_top(r), r.r, lay.list_top(r) + lay.item_h * 2);
            text(dc, "Shift+클릭으로 카드를 여세요", rc(inset(row, pad)), pal.text_muted, 0);
            return;
        }
        let saved = SaveDC(dc);
        IntersectClipRect(dc, r.l, lay.list_top(r), r.r, r.b);
        let items = shell.right_list();
        for (slot, item) in items.iter().skip(shell.back_first).take(lay.right_slots() + 1).enumerate() {
            let top = lay.list_top(r) + slot as i32 * lay.item_h;
            let row = R::new(r.l + 1, top, r.r, top + lay.item_h);
            match item {
                RightItem::Card(i) => {
                    fill(dc, rc(row), pal.head_bg);
                    frame(dc, row, pal.divider);
                    let mut title = inset(row, pad);
                    title.r -= scale(28, lay.dpi);
                    text(dc, &shell.cards[*i].target.page, rc(title), pal.accent, 0);
                    text(dc, "×", rc(R::new(title.r, row.t, row.r, row.b)), pal.text, DT_CENTER);
                }
                RightItem::Preview(i) => {
                    let preview = format!("↗ {}", shell.card_preview(*i).replace('\n', " · "));
                    text(dc, &preview, rc(inset(row, pad)), pal.text, 0);
                }
                RightItem::Group(page) => {
                    SelectObject(dc, fb);
                    text(dc, page, rc(inset(row, pad)), pal.accent, 0);
                    SelectObject(dc, f);
                }
                RightItem::Block(i) => {
                    let b = &shell.back[*i];
                    let mut tr = inset(row, pad);
                    tr.l += scale(10, lay.dpi);
                    text(dc, &format!("• {}", b.text), rc(tr), pal.text, 0);
                }
            }
        }
        RestoreDC(dc, saved);
    }
}

unsafe fn draw_results<H: EditHost>(dc: HDC, shell: &Shell<H>, pal: &Palette, lay: &Layout, pad: i32, f: HFONT, fb: HFONT) {
    unsafe {
        let c = lay.center;
        fill(dc, rc(c), pal.bg);
        let head = R::new(c.l, c.t, c.r, c.t + lay.head_h);
        fill(dc, rc(head), pal.head_bg);
        fill(dc, RECT { left: c.l, top: head.b - 1, right: c.r, bottom: head.b }, pal.divider);
        SelectObject(dc, fb);
        let title = if shell.search_mode == crate::shell::SearchMode::Commands { "명령 팔레트" } else { "검색 결과" };
        text(dc, &format!("{title} {}건 — {} · ↑↓ 선택 · Enter 실행 · Esc 닫기", shell.hits.len(), shell.query.trim()), rc(inset(head, pad)), pal.text, 0);
        SelectObject(dc, f);
        if shell.hits.is_empty() {
            let row = R::new(c.l, head.b, c.r, head.b + lay.result_h * 2);
            text(dc, "검색 결과가 없습니다", rc(inset(row, pad)), pal.text_muted, 0);
            return;
        }
        for (slot, i) in (shell.hit_first..shell.hits.len()).take(lay.result_slots() + 1).enumerate() {
            let hit = &shell.hits[i];
            let top = head.b + slot as i32 * lay.result_h;
            let row = R::new(c.l, top, c.r, top + lay.result_h);
            let sel = i == shell.hit_sel;
            if sel {
                fill(dc, rc(row), pal.selected);
            }
            if shell.search_mode == crate::shell::SearchMode::Commands {
                text(dc, &hit.page, rc(inset(row, pad)), pal.row_text(sel, pal.text), 0);
                continue;
            }
            let name_w = scale(150, lay.dpi);
            let mut nr = inset(row, pad);
            nr.r = nr.l + name_w;
            SelectObject(dc, fb);
            text(dc, &hit.page, rc(nr), pal.row_text(sel, pal.accent), 0);
            SelectObject(dc, f);
            let mut br = inset(row, pad);
            br.l = nr.r + scale(8, lay.dpi);
            let body = if hit.block.is_none() && hit.text.is_empty() { "(페이지 이름)" } else { hit.text.as_str() };
            text(dc, body, rc(br), pal.row_text(sel, if hit.block.is_none() { pal.text_muted } else { pal.text }), 0);
        }
    }
}

unsafe fn draw_popup<H: EditHost>(dc: HDC, shell: &Shell<H>, pal: &Palette, lay: &Layout, dpi: u32, f: HFONT) {
    unsafe {
        let Some(p) = &shell.editor.popup else { return };
        let Some(er) = edit_rect_of(shell, lay) else { return };
        let r = layout::popup_rect(lay, er, p.items.len());
        fill(dc, rc(r), pal.bg);
        frame(dc, r, pal.accent);
        SelectObject(dc, f);
        let pad = scale(8, dpi);
        for (i, name) in p.labels.iter().enumerate() {
            let top = r.t + scale(2, dpi) + i as i32 * lay.popup_h;
            let row = R::new(r.l + 1, top, r.r - 1, top + lay.popup_h);
            let sel = i == p.selected;
            if sel {
                fill(dc, rc(row), pal.selected);
            }
            text(dc, name, rc(inset(row, pad)), pal.row_text(sel, pal.text), 0);
        }
    }
}

/// 편집 중 줄의 EDIT 사각형(가운데 목록 기준). 편집 중이 아니거나 화면 밖이면 None.
pub fn edit_rect_of<H: EditHost>(shell: &Shell<H>, lay: &Layout) -> Option<R> {
    let ed = &shell.editor;
    let block = ed.editing_block()?;
    let pos = ed.view.rows.iter().position(|r| r.block == block)?;
    let m = Metrics::new(lay.dpi);
    let page = m.page_rows(lay.center.h());
    if pos < ed.view.first || ed.view.slot_of(pos) > page {
        return None;
    }
    let e = m.row_edit_rect(lay.dpi, &ed.view, pos, lay.center.w());
    Some(R::new(e.left, e.top, e.right, e.bottom))
}
