//! OUTLINEUI1: 합성 자료·가짜 호스트 상태 전이와 실제 Win32 창의 좌표/입력/화소 검증.
mod common;
use common::*;
use outliner_win::capture::seed_demo_pages;
use outliner_win::layout::{self, Btn, LeftItem, LeftSection, RightItem};
use outliner_win::pages::PageStore;
use outliner_win::shell::Shell;
use outliner_win::window::{Harness, WinHost};
use std::sync::Mutex;
use ui_theme::Kind;

static LOCK: Mutex<()> = Mutex::new(());

fn shell(tag: &str) -> (Shell<Fake>, std::path::PathBuf) {
    let dir = temp_dir(tag);
    seed_demo_pages(&dir).unwrap();
    let mut s = Shell::with_store(Fake::default(), PageStore::new(&dir, None), fixed_clock).unwrap();
    s.start().unwrap();
    (s, dir)
}

fn harness(tag: &str, kind: Kind, dpi: u32, w: i32) -> Harness {
    let dir = temp_dir(tag);
    seed_demo_pages(&dir).unwrap();
    let mut s = Shell::with_store(WinHost::new(), PageStore::new(&dir, None), fixed_clock).unwrap();
    s.start().unwrap();
    Harness::new_shell(s, "아웃라이너", kind, dpi, w, outliner_win::paint::scale(720, dpi)).unwrap()
}

#[test]
fn layout_keeps_center_positive_and_remembers_visibility_at_both_dpis() {
    for dpi in [96, 144, 192] {
        let scale = |n| outliner_win::paint::scale(n, dpi);
        for w in [0, 200, 400, 560, 700, 900, 1280] {
            for left in [false, true] {
                for right in [false, true] {
                    let l = layout::compute_visible(scale(w), scale(720), dpi, true, left, right);
                    assert!(l.center.w() >= 0 && l.center.h() >= 0);
                    assert!(l.center.l >= 0 && l.center.r <= scale(w));
                    if let Some(r) = l.left { assert!(left); assert_eq!(r.w(), scale(220)); }
                    if let Some(r) = l.right { assert!(right); assert_eq!(r.w(), scale(280)); }
                    if w >= 400 {
                        for (b, r) in layout::BUTTONS.iter().zip(l.buttons).chain(layout::NAV_BUTTONS.iter().zip(l.nav_buttons)) {
                            assert!(r.l >= 0 && r.r <= scale(w), "{b:?} {r:?}");
                            assert_eq!(l.button_at((r.l + r.r) / 2, (r.t + r.b) / 2), Some(*b));
                        }
                    }
                }
            }
        }
        let wide = layout::compute_visible(scale(1280), scale(720), dpi, true, true, true);
        assert_eq!(wide.center.w(), scale(780));
        for (i, section) in layout::SECTIONS.iter().enumerate() {
            let y = wide.left.unwrap().t + wide.head_h + i as i32 * wide.item_h + 1;
            assert_eq!(wide.section_at(1, y), Some(*section));
            assert_eq!(wide.slot_at(wide.left.unwrap(), y), None);
        }
    }
}

#[test]
fn favorite_journal_recent_sections_filter_without_writing_settings() {
    let (mut s, dir) = shell("ui1_sections");
    let files_before: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    s.toggle_favorite();
    s.select_section(LeftSection::Favorites);
    assert_eq!(s.left_list()[1], LeftItem::Page("2026_11_21".into()));
    s.toggle_favorite();
    assert_eq!(s.left_list().len(), 1);
    s.select_section(LeftSection::Journals);
    assert!(s.left_list().iter().skip(1).all(|i| matches!(i, LeftItem::Page(p) if outliner_win::pages::looks_like_journal(p))));
    s.open_page("회의록").unwrap();
    s.select_section(LeftSection::Recent);
    assert_eq!(s.left_list()[1], LeftItem::Page("회의록".into()));
    s.set_query("예산");
    assert!(s.left_list().iter().skip(1).all(|i| matches!(i, LeftItem::Page(p) if p.contains("예산"))));
    let files_after: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(files_before, files_after);
}

#[test]
fn history_restores_block_and_discards_forward_only_on_new_visit() {
    let (mut s, _) = shell("ui1_history");
    s.editor.view.set_selected(2, 20);
    let block = s.editor.view.rows[s.editor.view.selected].block;
    s.open_page("회의록").unwrap();
    s.open_page("예산 지침").unwrap();
    assert!(s.can_back() && !s.can_forward());
    s.navigate(false, 20).unwrap();
    assert_eq!(s.page.as_deref(), Some("회의록"));
    s.navigate(false, 20).unwrap();
    assert_eq!(s.editor.view.rows[s.editor.view.selected].block, block);
    assert!(!s.can_back() && s.can_forward());
    s.navigate(true, 20).unwrap();
    s.open_page("새 방문").unwrap();
    assert!(!s.can_forward());
    assert_eq!(s.visits.len(), 3);
    s.open_page("새 방문").unwrap();
    assert_eq!(s.visits.len(), 3);
}

#[test]
fn history_save_conflict_or_missing_target_preserves_current_page_and_trail() {
    let (mut s, dir) = shell("ui1_conflict");
    s.open_page("회의록").unwrap();
    s.editor.start_edit_selected();
    s.editor.host.type_text("입력 중 글");
    write(&dir, "회의록.md", "- 외부 변경\n");
    let at = s.visit_at;
    let visits = s.visits.clone();
    assert!(s.navigate(false, 20).is_err());
    assert_eq!(s.page.as_deref(), Some("회의록"));
    assert_eq!(s.visit_at, at);
    assert_eq!(s.visits, visits);
    assert!(s.editor.markdown().contains("입력 중 글"));
    assert_eq!(read(&dir, "회의록.md"), "- 외부 변경\n");
    let (mut s, dir) = shell("ui1_missing");
    s.open_page("회의록").unwrap();
    std::fs::remove_file(dir.join("2026_11_21.md")).unwrap();
    assert!(s.navigate(false, 20).is_err());
    assert_eq!(s.page.as_deref(), Some("회의록"));
    assert!(!dir.join("2026_11_21.md").exists());
}

#[test]
fn cards_deduplicate_close_activate_and_preserve_markdown_and_navigation() {
    let (mut s, dir) = shell("ui1_cards");
    let md = s.editor.markdown();
    let visits = s.visits.clone();
    s.right_open = false;
    s.open_card("회의록", Some(1)).unwrap();
    s.open_card("예산 지침", None).unwrap();
    s.open_card("회의록", Some(1)).unwrap();
    assert!(s.right_open);
    assert_eq!(s.cards.len(), 2);
    assert_eq!(s.cards[0].target.page, "회의록");
    assert!(matches!(s.right_list()[0], RightItem::Card(0)));
    assert_eq!(s.editor.markdown(), md);
    assert_eq!(s.visits, visits);
    s.activate_card(0, 20).unwrap();
    assert_eq!(s.page.as_deref(), Some("회의록"));
    assert_eq!(s.editor.view.rows[s.editor.view.selected].block, 1);
    s.close_card(0);
    assert_eq!(s.cards.len(), 1);
    assert!(s.open_card("없는 페이지", None).is_err());
    assert!(!dir.join("없는 페이지.md").exists());
    assert!(s.open_card("회의록", Some(usize::MAX)).is_err());
    assert!(s.open_card("../탈출", None).is_err());
}

#[test]
fn stale_block_card_refuses_to_jump_to_a_different_block() {
    let (mut s, dir) = shell("ui1_stale_card");
    s.open_card("회의록", Some(1)).unwrap();
    write(&dir, "회의록.md", "- 앞에 추가된 블록\n- 원래 첫 블록\n- 원래 두 번째 블록\n");
    assert!(s.activate_card(0, 20).unwrap_err().contains("블록이 바뀌었습니다"));
    assert_eq!(s.page.as_deref(), Some("2026_11_21"));
    assert_eq!(s.cards.len(), 1);
}

#[test]
fn rename_and_undo_keep_favorites_cards_and_history_consistent() {
    let (mut s, _) = shell("ui1_rename");
    s.open_page("회의록").unwrap();
    s.toggle_favorite();
    s.open_card("회의록", None).unwrap();
    s.rename_current("회의 정리").unwrap();
    assert_eq!(s.favorites, ["회의 정리"]);
    assert_eq!(s.cards[0].target.page, "회의 정리");
    assert!(s.visits.iter().all(|v| v.page != "회의록"));
    s.undo_rename().unwrap();
    assert_eq!(s.favorites, ["회의록"]);
    assert_eq!(s.cards[0].target.page, "회의록");
    assert!(s.visits.iter().all(|v| v.page != "회의 정리"));
}

#[test]
fn real_toolbar_shortcuts_resize_edit_and_shift_click_cards() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = harness("ui1_click", Kind::Light, 96, 1280);
    h.shell().open_page("회의록").unwrap();
    h.layout();
    let old = h.layout_now().center;
    h.editor().start_edit_selected();
    h.layout();
    h.press(Btn::ToggleLeft);
    assert!(h.layout_now().left.is_none());
    assert!(h.layout_now().center.w() > old.w());
    h.press(Btn::ToggleLeft);
    h.editor().finish_edit();
    assert!(h.list_key_modifiers(b'T' as u16, false, false, false));
    assert!(h.list_key_modifiers(b'R' as u16, false, false, false));
    assert!(h.layout_now().right.is_none());
    assert!(h.list_key_modifiers(b'T' as u16, false, false, false));
    assert!(h.list_key_modifiers(b'R' as u16, false, false, false));
    assert!(h.layout_now().right.is_some());
    assert!(h.list_key_modifiers(0xDB, true, false, false));
    assert_eq!(h.shell().page.as_deref(), Some("2026_11_21"));
    assert!(h.list_key_modifiers(0xDD, true, false, false));
    assert_eq!(h.shell().page.as_deref(), Some("회의록"));
    assert!(h.list_key_modifiers(b'F' as u16, true, false, true));
    assert!(h.shell().favorites.contains(&"회의록".into()));
    let l = h.layout_now();
    let items = h.shell().left_list();
    let i = items.iter().position(|i| *i == LeftItem::Page("예산 지침".into())).unwrap();
    h.shift_click_at(30, l.list_top(l.left.unwrap()) + i as i32 * l.item_h + 5);
    assert_eq!(h.shell().cards.len(), 1);
    assert_eq!(h.shell().page.as_deref(), Some("회의록"));
    let r = h.layout_now().right.unwrap();
    h.click_at(r.r - 10, l.list_top(r) + 4, false);
    assert!(h.shell().cards.is_empty());
    let m = outliner_win::paint::Metrics::new(96);
    h.shift_click_at(l.center.l + m.marker_x(0), l.center.t + m.header_h + m.row_h / 2);
    assert_eq!(h.shell().cards.len(), 1);
    assert_eq!(h.shell().cards[0].target.block, Some(0));
    assert!(h.list_key_modifiers(b'K' as u16, true, false, false));
    h.type_search("예산");
    assert!(h.shell().search_open);
}

#[test]
fn real_capture_three_columns_cards_and_themes_at_two_dpis() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dpi in [96, 192] {
        for (kind, name) in [(Kind::Light, "light"), (Kind::Dark, "dark"), (Kind::HighContrast, "hc")] {
            let mut h = harness("ui1_capture", kind, dpi, outliner_win::paint::scale(1280, dpi));
            h.shell().toggle_favorite();
            h.shell().open_card("회의록", Some(0)).unwrap();
            h.shell().open_card("예산 지침", None).unwrap();
            h.layout();
            let l = h.layout_now();
            let shot = h.render().unwrap();
            assert_eq!(shot.width, outliner_win::paint::scale(1280, dpi) as usize);
            assert!(l.left.is_some() && l.right.is_some());
            let count = |rect: layout::R| {
                let mut n = 0;
                for y in rect.t..rect.b { for x in rect.l..rect.r {
                    let i = (y as usize * shot.width + x as usize) * 4;
                    if shot.bgra[i..i+3] != shot.bgra[0..3] { n += 1; }
                }}
                n
            };
            assert!(count(l.center) > 800);
            assert!(count(l.right.unwrap()) > 800);
            if let Some(dir) = std::env::var_os("OUTLINEUI1_CAPTURE_DIR") {
                let path = std::path::PathBuf::from(dir);
                std::fs::create_dir_all(&path).unwrap();
                outliner_win::png::write(&path.join(format!("outlineui1_{name}_{dpi}.png")), shot.width, shot.height, &shot.bgra).unwrap();
            }
        }
    }
}
