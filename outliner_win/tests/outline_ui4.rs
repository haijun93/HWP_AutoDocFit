mod common;
use common::*;
use outliner_core::references::RefTarget;
use outliner_win::{
    layout::RightItem,
    pages::PageStore,
    paint::{Metrics, reference_spans_x, scale},
    shell::{PromptKind, Shell},
    window::{Harness, WinHost},
};
use std::{path::PathBuf, sync::Mutex};
use ui_theme::Kind;

static LOCK: Mutex<()> = Mutex::new(());
const MAIN: &str = "- 인라인 😀 ((stable)) #업무 #[[장기 계획]]\r\n  owner:: 합성 담당자\r\n- {{embed ((stable))}}\r\n- {{embed [[회의록]]}}\r\n";
const OTHER: &str = "- 대상 본문 #업무\n  id:: stable\n  - 하위 내용\n- 참고 예산\n- [[예산]] 연결\n- `예산` 코드\n";

fn data() -> PathBuf {
    let d = temp_dir("ui4");
    write(&d, "예산.md", MAIN);
    write(&d, "회의록.md", OTHER);
    d
}
fn shell() -> (Shell<Fake>, PathBuf) {
    let d = data();
    let mut s = Shell::with_store(
        Fake::default(),
        PageStore::new(d.clone(), None),
        fixed_clock,
    )
    .unwrap();
    s.open_page("예산").unwrap();
    (s, d)
}
fn harness(dpi: u32, kind: Kind) -> (Harness, PathBuf) {
    let d = data();
    let mut s =
        Shell::with_store(WinHost::new(), PageStore::new(d.clone(), None), fixed_clock).unwrap();
    s.open_page("예산").unwrap();
    (
        Harness::new_shell(
            s,
            "OUTLINEUI4 합성",
            kind,
            dpi,
            scale(1280, dpi),
            scale(780, dpi),
        )
        .unwrap(),
        d,
    )
}

#[test]
fn linked_and_unlinked_are_separate_and_opening_mentions_never_links_text() {
    let (mut s, d) = shell();
    assert!(s.back.iter().any(|b| b.page == "회의록" && b.block == 3));
    assert_eq!(s.unlinked.len(), 1);
    assert_eq!(s.unlinked[0].block, 2);
    assert!(
        s.right_list()
            .iter()
            .any(|r| matches!(r, RightItem::Unlinked(0)))
    );
    s.goto_unlinked(0, 20).unwrap();
    assert_eq!(s.page.as_deref(), Some("회의록"));
    assert_eq!(s.editor.view.rows[s.editor.view.selected].block, 2);
    assert_eq!(read(&d, "회의록.md"), OTHER);
    assert_eq!(read(&d, "예산.md"), MAIN);
}

#[test]
fn presentation_and_tag_search_do_not_write_or_create_files() {
    let (mut s, d) = shell();
    let count = std::fs::read_dir(&d).unwrap().count();
    s.refresh_presentations();
    let shown: String = s.editor.view.rows[0]
        .presentation
        .iter()
        .flatten()
        .map(|s| s.text.as_str())
        .collect();
    assert!(shown.contains("대상 본문") && !shown.contains("((stable))"));
    assert!(s.editor.view.rows[1].lines >= 3);
    s.search_tag("업무").unwrap();
    assert_eq!(s.hits.len(), 2);
    s.search_tag("장기 계획").unwrap();
    assert_eq!(s.hits.len(), 1);
    assert_eq!(s.editor.markdown(), MAIN);
    assert!(!s.editor.dirty());
    assert_eq!(read(&d, "예산.md"), MAIN);
    assert_eq!(std::fs::read_dir(&d).unwrap().count(), count);
}

#[test]
fn typed_block_target_follows_real_block_and_shift_opens_readonly_card() {
    let (mut s, d) = shell();
    s.follow_target(&RefTarget::Block("stable".into()), true, 20)
        .unwrap();
    assert_eq!(s.page.as_deref(), Some("예산"));
    assert_eq!(s.cards[0].target.page, "회의록");
    assert_eq!(s.cards[0].target.block, Some(0));
    s.follow_target(&RefTarget::Block("stable".into()), false, 20)
        .unwrap();
    assert_eq!(s.page.as_deref(), Some("회의록"));
    assert_eq!(s.editor.view.rows[s.editor.view.selected].block, 0);
    assert_eq!(read(&d, "회의록.md"), OTHER);
}

#[test]
fn code_references_and_tags_are_literal_in_sidebar_and_exact_tag_search() {
    let (mut s, d) = shell();
    write(
        &d,
        "코드.md",
        "- `[[예산]] #업무`\n- 일반 #업무\n- property\n  owner:: [[예산]]\n",
    );
    s.refresh_lists();
    s.refresh_backlinks();
    assert!(!s.back.iter().any(|b| b.page == "코드" && b.block == 0));
    assert!(s.back.iter().any(|b| b.page == "코드" && b.block == 2));
    s.search_tag("업무").unwrap();
    assert!(
        !s.hits
            .iter()
            .any(|b| b.page == "코드" && b.block == Some(0))
    );
    assert!(
        s.hits
            .iter()
            .any(|b| b.page == "코드" && b.block == Some(1))
    );
}

#[test]
fn missing_duplicate_and_stale_references_never_create_or_jump_to_wrong_file() {
    let (mut s, d) = shell();
    assert!(
        s.follow_target(&RefTarget::Block("absent".into()), false, 20)
            .is_err()
    );
    write(&d, "중복.md", "- 중복\n  id:: stable\n");
    s.refresh_lists();
    assert!(
        s.follow_target(&RefTarget::Block("stable".into()), false, 20)
            .unwrap_err()
            .contains("중복")
    );
    std::fs::remove_file(d.join("중복.md")).unwrap();
    s.refresh_lists();
    write(&d, "회의록.md", &format!("- 앞에 삽입\n{OTHER}"));
    assert!(
        s.follow_target(&RefTarget::Block("stable".into()), false, 20)
            .unwrap_err()
            .contains("바뀌었습니다")
    );
    assert_eq!(s.page.as_deref(), Some("예산"));
    s.follow_target(&RefTarget::Block("stable".into()), false, 20)
        .unwrap();
    assert_eq!(s.editor.view.rows[s.editor.view.selected].block, 1);
    assert!(!d.join("absent.md").exists());
}

#[test]
fn property_prompt_confirm_cancel_validation_and_single_undo_preserve_source() {
    let (mut s, d) = shell();
    assert!(s.begin_prompt(PromptKind::Property).is_some());
    s.cancel_prompt();
    assert_eq!(s.editor.markdown(), MAIN);
    s.begin_prompt(PromptKind::Property).unwrap();
    assert!(s.finish_prompt("잘못된 입력").is_err());
    assert_eq!(s.prompt, Some(PromptKind::Property));
    s.finish_prompt("owner:: 새 담당자 😀").unwrap();
    assert_eq!(s.prompt, None);
    assert_eq!(
        s.editor.view.doc.blocks[0].properties["owner"],
        "새 담당자 😀"
    );
    assert_eq!(read(&d, "예산.md"), MAIN);
    assert!(s.editor.history_step(true));
    assert_eq!(s.editor.markdown(), MAIN);
    assert!(s.editor.history_step(false));
    assert!(s.editor.markdown().contains("owner:: 새 담당자 😀\r\n"));
    assert!(s.editor.save());
    assert!(read(&d, "예산.md").contains("owner:: 새 담당자 😀\r\n"));
    assert_eq!(read(&d, "회의록.md"), OTHER);
}

#[test]
fn property_prompt_during_composition_waits_without_mutating_document() {
    let (mut s, d) = shell();
    s.editor.start_edit_selected();
    s.editor.host.type_text("조합 중");
    s.editor.host.composing = true;
    assert!(s.begin_prompt(PromptKind::Property).is_none());
    assert!(s.prompt.is_none());
    assert_eq!(read(&d, "예산.md"), MAIN);
}

#[test]
fn actual_win32_reference_click_and_second_line_embed_click_share_geometry() {
    let _guard = LOCK.lock().unwrap();
    let (mut h, _) = harness(96, Kind::Light);
    h.layout();
    let m = Metrics::new(96);
    let lay = h.layout_now();
    let row = h.editor().view.rows[0].clone();
    let spans = reference_spans_x(&row, 0, m.text_x(0), 96);
    let (l, r, target) = spans
        .iter()
        .find(|(_, _, t)| matches!(t, RefTarget::Block(_)))
        .unwrap();
    assert_eq!(target, &RefTarget::Block("stable".into()));
    h.shift_click_at(
        lay.center.l + (l + r) / 2,
        lay.center.t + m.header_h + m.row_h / 2,
    );
    assert_eq!(h.shell().cards[0].target.block, Some(0));
    h.click_at(
        lay.center.l + (l + r) / 2,
        lay.center.t + m.header_h + m.row_h / 2,
        false,
    );
    assert_eq!(h.shell().page.as_deref(), Some("회의록"));
    let selected = h.editor().view.selected;
    assert_eq!(h.editor().view.rows[selected].block, 0);
    h.shell().open_page("예산").unwrap();
    h.layout();
    let row = h.editor().view.rows[1].clone();
    let slot = h.editor().view.slot_of(1);
    let spans = reference_spans_x(&row, 1, m.text_x(0), 96);
    let (l, r, _) = spans
        .iter()
        .find(|(_, _, t)| matches!(t, RefTarget::Tag(_)))
        .unwrap();
    h.click_at(
        lay.center.l + (l + r) / 2,
        lay.center.t + m.header_h + (slot + 1) as i32 * m.row_h + m.row_h / 2,
        false,
    );
    assert!(h.shell().search_open);
    assert_eq!(h.shell().query, "#업무");
    assert_eq!(h.shell().hits.len(), 2);
}

#[test]
fn editing_uses_raw_reference_text_and_refresh_uses_live_target_after_undo() {
    let (mut s, _) = shell();
    s.refresh_presentations();
    s.editor.start_edit_selected();
    s.refresh_presentations();
    assert!(s.editor.view.rows[0].presentation.is_empty());
    assert!(s.editor.host.text.contains("((stable))"));
    s.editor.finish_edit();
    s.refresh_presentations();
    assert!(!s.editor.view.rows[0].presentation.is_empty());
    assert_eq!(s.editor.markdown(), MAIN);
    s.editor.set_property(1, "id", "stable").unwrap();
    s.refresh_presentations();
    assert!(
        s.editor.view.rows[0]
            .presentation
            .iter()
            .flatten()
            .any(|span| span.state == outliner_core::references::RefState::Ambiguous)
    );
    assert!(s.editor.history_step(true));
    s.refresh_presentations();
    assert_eq!(s.editor.markdown(), MAIN);
    assert!(
        !s.editor.view.rows[0]
            .presentation
            .iter()
            .flatten()
            .any(|span| span.state == outliner_core::references::RefState::Ambiguous)
    );
}

#[test]
fn actual_win32_property_row_opens_input_and_enter_sets_property() {
    let _guard = LOCK.lock().unwrap();
    let (mut h, d) = harness(96, Kind::Dark);
    h.layout();
    let items = h.shell().right_list();
    let slot = items
        .iter()
        .position(|r| matches!(r, RightItem::Property(k, _) if k == "owner"))
        .unwrap();
    let lay = h.layout_now();
    let right = lay.right.unwrap();
    h.click_at(
        right.l + 30,
        lay.list_top(right) + slot as i32 * lay.item_h + 10,
        false,
    );
    assert_eq!(h.shell().prompt, Some(PromptKind::Property));
    assert_eq!(h.search_text(), "owner:: 합성 담당자");
    h.type_search("owner:: 새 담당자");
    h.confirm_prompt();
    assert_eq!(
        h.editor().view.doc.blocks[0].properties["owner"],
        "새 담당자"
    );
    assert_eq!(read(&d, "예산.md"), MAIN);
}

#[test]
fn actual_win32_three_themes_two_dpis_reference_embed_properties_are_nonblank() {
    let _guard = LOCK.lock().unwrap();
    let destination = std::env::var_os("OUTLINEUI4_CAPTURE_DIR").map(PathBuf::from);
    for (tag, kind) in [
        ("light", Kind::Light),
        ("dark", Kind::Dark),
        ("hc", Kind::HighContrast),
    ] {
        for dpi in [96, 192] {
            let (mut h, d) = harness(dpi, kind);
            h.layout();
            let shot = h.render().unwrap();
            assert!(shot.bgra.windows(4).any(|p| p != &shot.bgra[..4]));
            assert_eq!(h.editor().markdown(), MAIN);
            assert_eq!(read(&d, "예산.md"), MAIN);
            if let Some(dir) = &destination {
                std::fs::create_dir_all(dir).unwrap();
                outliner_win::png::write(
                    &dir.join(format!("references_{tag}_{dpi}.png")),
                    shot.width,
                    shot.height,
                    &shot.bgra,
                )
                .unwrap();
            }
        }
    }
}

#[test]
fn leaving_the_page_cancels_a_pending_property_prompt() {
    let (mut s, d) = shell();
    s.begin_prompt(PromptKind::Property).unwrap();
    s.open_page("회의록").unwrap();
    assert_eq!(s.prompt, None);
    assert_eq!(s.editor.markdown(), OTHER);
    assert_eq!(read(&d, "회의록.md"), OTHER);
}

#[test]
fn rows_without_references_keep_the_plain_drawing_path() {
    let (mut s, _) = shell();
    s.open_page("회의록").unwrap();
    s.refresh_presentations();
    let plain = s.editor.view.rows.iter().find(|r| r.text == "참고 예산").unwrap();
    assert!(plain.presentation.is_empty());
    assert_eq!(plain.lines, 1);
}
