//! 페이지 모드 흐름(`shell.rs`)과 이번 카드의 편집기 기능(TODO 토글·블록 이동·`[[` 자동완성)을 가짜 EDIT 로 시험한다.
mod common;
use common::*;
use outliner_win::edit::*;
use outliner_win::pages::PageStore;
use outliner_win::shell::{PromptKind, Shell};
use std::path::PathBuf;

fn shell_in(tag: &str, pages: &[(&str, &str)]) -> (Shell<Fake>, PathBuf) {
    let d = temp_dir(tag);
    for (n, b) in pages {
        write(&d, &format!("{n}.md"), b);
    }
    let st = PageStore::new(&d, Some(d.join("recent.json")));
    let mut sh = Shell::with_store(Fake::default(), st, fixed_clock).unwrap();
    sh.start().unwrap();
    (sh, d)
}

fn key(vk: u16) -> KeyEvent {
    KeyEvent::new(vk, 1000)
}

fn ctrl(vk: u16) -> KeyEvent {
    KeyEvent { ctrl: true, ..KeyEvent::new(vk, 1000) }
}

fn alt(vk: u16) -> KeyEvent {
    KeyEvent { alt: true, ..KeyEvent::new(vk, 1000) }
}

const TREE: &str = "- A\n  - A1\n  - A2\n- B\n  - B1\n- C\n";

// ---------- 페이지 흐름 ----------

#[test]
fn start_opens_or_creates_todays_journal_with_underscore_name() {
    let (sh, d) = shell_in("start", &[("메모", "- 메모\n")]);
    assert_eq!(sh.page.as_deref(), Some("2026_11_21"));
    assert_eq!(read(&d, "2026_11_21.md"), "- \n");
    assert_eq!(sh.pages, ["메모", "2026_11_21"]);
    assert_eq!(sh.recent, ["2026_11_21"]);
    // 이미 있는 일지는 그대로 연다.
    let (sh2, _) = shell_in("start2", &[("2026_11_21", "- 있던 일지\n")]);
    assert_eq!(sh2.editor.view.doc.blocks[0].text, "있던 일지");
}

#[test]
fn open_page_saves_pending_edit_first_and_tracks_recent() {
    let (mut sh, d) = shell_in("open", &[("가", "- 가 본문\n"), ("나", "- 나 본문\n")]);
    sh.open_page("가").unwrap();
    sh.editor.start_edit_selected();
    sh.editor.host.type_text("가 수정됨"); // 아직 확정 안 된 입력
    sh.open_page("나").unwrap();
    assert_eq!(read(&d, "가.md"), "- 가 수정됨\n", "열기 전에 저장");
    assert_eq!(sh.page.as_deref(), Some("나"));
    assert!(!sh.editor.editing());
    assert_eq!(sh.recent, ["나", "가", "2026_11_21"]);
    assert!(read(&d, "recent.json").contains("\"나\""));
}

#[test]
fn open_page_stays_put_when_file_changed_elsewhere() {
    let (mut sh, d) = shell_in("conflict", &[("가", "- 가\n"), ("나", "- 나\n")]);
    sh.open_page("가").unwrap();
    sh.editor.start_edit_selected();
    sh.editor.host.type_text("내 변경");
    write(&d, "가.md", "- 남이 바꿈\n");
    let err = sh.open_page("나").unwrap_err();
    assert!(err.contains("다른 곳에서 바뀌"), "{err}");
    assert_eq!(sh.page.as_deref(), Some("가"), "페이지를 옮기지 않는다");
    assert_eq!(read(&d, "가.md"), "- 남이 바꿈\n", "남의 변경 보존");
    assert!(sh.editor.alert.is_some());
}

#[test]
fn rename_current_updates_links_everywhere_and_reloads_under_new_name() {
    let (mut sh, d) = shell_in("rename", &[("옛", "- 본문 [[옛]]\n"), ("참조", "- 보라 [[옛]]\n")]);
    sh.open_page("옛").unwrap();
    sh.rename_current("새").unwrap();
    assert_eq!(sh.page.as_deref(), Some("새"));
    assert!(!d.join("옛.md").exists());
    assert_eq!(read(&d, "참조.md"), "- 보라 [[새]]\n");
    assert_eq!(read(&d, "새.md"), "- 본문 [[새]]\n");
    assert_eq!(sh.editor.view.doc.blocks[0].text, "본문 [[새]]", "화면 문서도 갱신된 파일을 다시 읽음");
    assert!(sh.recent.contains(&"새".to_string()) && !sh.recent.contains(&"옛".to_string()));
    assert_eq!(sh.back.iter().map(|b| b.page.as_str()).collect::<Vec<_>>(), ["참조"], "역링크도 새 이름 기준");
    assert!(sh.editor.message.as_deref().unwrap().contains("링크를 고친 파일 2개"));
    // 이름 바꾸기 실패는 현재 페이지를 그대로 둔다.
    assert!(sh.rename_current("참조").is_err());
    assert_eq!(sh.page.as_deref(), Some("새"));
}

#[test]
fn rename_saves_pending_edit_before_renaming() {
    let (mut sh, d) = shell_in("rename_save", &[("옛", "- 본문\n")]);
    sh.open_page("옛").unwrap();
    sh.editor.start_edit_selected();
    sh.editor.host.type_text("고친 본문");
    sh.rename_current("새").unwrap();
    assert_eq!(read(&d, "새.md"), "- 고친 본문\n");
}

#[test]
fn delete_current_moves_to_trash_and_opens_todays_journal() {
    let (mut sh, d) = shell_in("delete", &[("지울", "- 내용\n"), ("남길", "- [[지울]]\n")]);
    sh.open_page("지울").unwrap();
    sh.delete_current().unwrap();
    assert!(!d.join("지울.md").exists());
    let trash = sh.store.as_ref().unwrap().list_trash();
    assert_eq!(trash.len(), 1);
    assert_eq!(std::fs::read_to_string(d.join(".trash").join(&trash[0])).unwrap(), "- 내용\n");
    assert_eq!(sh.page.as_deref(), Some("2026_11_21"));
    assert!(!sh.pages.contains(&"지울".to_string()) && !sh.recent.contains(&"지울".to_string()));
    // 되돌리기
    sh.restore_last().unwrap();
    assert_eq!(sh.page.as_deref(), Some("지울"));
    assert_eq!(read(&d, "지울.md"), "- 내용\n");
    assert!(sh.store.as_ref().unwrap().list_trash().is_empty());
}

#[test]
fn new_page_prompt_flow_keeps_prompt_open_on_error_and_closes_on_success() {
    let (mut sh, d) = shell_in("prompt", &[("있는", "- x\n")]);
    assert_eq!(sh.begin_prompt(PromptKind::NewPage).as_deref(), Some(""));
    assert!(sh.finish_prompt("있는").unwrap_err().contains("이미 있습니다"));
    assert_eq!(sh.prompt, Some(PromptKind::NewPage), "실패하면 줄을 그대로 둔다");
    assert!(sh.finish_prompt("a/b").is_err());
    sh.finish_prompt("새 페이지").unwrap();
    assert_eq!(sh.prompt, None);
    assert_eq!(sh.page.as_deref(), Some("새 페이지"));
    assert_eq!(read(&d, "새 페이지.md"), "- \n");
    // 이름 바꾸기 입력은 현재 이름으로 시작
    assert_eq!(sh.begin_prompt(PromptKind::Rename).as_deref(), Some("새 페이지"));
    sh.cancel_prompt();
    assert_eq!(sh.prompt, None);
}

#[test]
fn follow_link_opens_existing_page_and_creates_missing_one() {
    let (mut sh, d) = shell_in("follow", &[("있는", "- 본문\n")]);
    sh.follow_link("있는").unwrap();
    assert_eq!(sh.page.as_deref(), Some("있는"));
    sh.follow_link("처음 보는 페이지").unwrap();
    assert_eq!(read(&d, "처음 보는 페이지.md"), "- \n");
    assert!(sh.follow_link("a/b").is_err());
    assert_eq!(sh.page.as_deref(), Some("처음 보는 페이지"));
    // 옛 YYYY-MM-DD 일지 링크를 따라가면 이름을 정리해서 연다.
    write(&d, "2026-11-01.md", "- 옛 일지\n");
    sh.follow_link("2026-11-01").unwrap();
    assert_eq!(sh.page.as_deref(), Some("2026_11_01"));
    assert!(d.join("2026_11_01.md").exists() && !d.join("2026-11-01.md").exists());
}

#[test]
fn search_opens_after_two_chars_without_enter_and_closes_below_two() {
    let (mut sh, _d) = shell_in("search", &[("가", "- 사과 하나\n- 배\n"), ("나", "- 사과 둘\n")]);
    sh.set_query("사");
    assert!(!sh.search_open && sh.hits.is_empty());
    sh.set_query("사과");
    assert!(sh.search_open);
    assert_eq!(sh.hits.len(), 2);
    sh.move_hit(true);
    assert_eq!(sh.hit_sel, 1);
    sh.move_hit(true);
    assert_eq!(sh.hit_sel, 1, "끝에서 멈춤");
    sh.move_hit(false);
    sh.move_hit(false);
    assert_eq!(sh.hit_sel, 0);
    sh.set_query("사");
    assert!(!sh.search_open && sh.hits.is_empty());
    sh.set_query("없는낱말들");
    assert!(sh.search_open && sh.hits.is_empty(), "결과 없음도 패널로 알린다");
    sh.close_search();
    assert!(!sh.search_open && sh.query.is_empty());
}

#[test]
fn open_hit_goes_to_the_block_and_unfolds_hidden_ancestors() {
    let (mut sh, _d) = shell_in("hit", &[("긴글", "- 루트\n  - 중간 collapsed:: true\n    - 깊은곳 보물\n- 다른\n")]);
    sh.set_query("보물");
    assert_eq!(sh.hits.len(), 1);
    assert_eq!(sh.hits[0].block, Some(2));
    sh.open_hit(0, 10).unwrap();
    assert_eq!(sh.page.as_deref(), Some("긴글"));
    assert!(!sh.search_open);
    let ed = &sh.editor;
    let row = ed.view.rows.get(ed.view.selected).unwrap();
    assert_eq!(row.block, 2, "해당 블록이 선택됨");
    assert!(!ed.view.doc.blocks[1].folded, "접힌 조상은 펼쳐짐");
    // 페이지 이름 일치는 페이지만 연다.
    sh.set_query("긴글");
    assert_eq!(sh.hits[0].block, None);
    sh.open_hit(0, 10).unwrap();
}

#[test]
fn backlink_panel_lists_other_pages_and_click_goes_to_that_block() {
    let (mut sh, _d) = shell_in(
        "back",
        &[("대상", "- 본문\n"), ("A", "- 일반\n- [[대상]] 첫째\n"), ("B", "- x\n  - y\n  - 또 [[대상]]\n")],
    );
    sh.open_page("대상").unwrap();
    let got: Vec<(&str, usize)> = sh.back.iter().map(|b| (b.page.as_str(), b.block)).collect();
    assert_eq!(got, [("A", 1), ("B", 2)]);
    sh.goto_backlink(1, 10).unwrap();
    assert_eq!(sh.page.as_deref(), Some("B"));
    assert_eq!(sh.editor.view.rows[sh.editor.view.selected].block, 2);
    assert!(sh.back.is_empty(), "B 를 가리키는 링크는 없다");
    assert!(sh.goto_backlink(9, 10).is_err());
}

#[test]
fn save_conflict_alert_is_raised_when_persisting_over_an_external_change() {
    let (mut sh, d) = shell_in("alert", &[("가", "- 가\n")]);
    sh.open_page("가").unwrap();
    sh.editor.start_edit_selected();
    sh.editor.host.type_text("변경");
    write(&d, "가.md", "- 외부\n");
    assert!(sh.persist().is_err());
    assert!(sh.editor.conflict);
}

// ---------- TODO 토글 ----------

#[test]
fn ctrl_enter_toggles_todo_while_editing_and_keeps_the_typed_text_and_session() {
    let (mut sh, _d) = shell_in("toggle", &[("t", "- 일 하나\n- 일 둘\n")]);
    sh.open_page("t").unwrap();
    sh.editor.start_edit_selected();
    sh.editor.host.type_text("일 하나 고침");
    assert_eq!(sh.editor.key(ctrl(VK_RETURN)), KeyResult::Swallow);
    let b = &sh.editor.view.doc.blocks[0];
    assert_eq!((b.status.as_deref(), b.text.as_str()), (Some("TODO"), "일 하나 고침"));
    assert!(sh.editor.editing(), "편집 상태 유지");
    assert_eq!(sh.editor.host.text, "일 하나 고침");
    sh.editor.key(ctrl(VK_RETURN));
    assert_eq!(sh.editor.view.doc.blocks[0].status.as_deref(), Some("DONE"));
    sh.editor.key(ctrl(VK_RETURN));
    assert_eq!(sh.editor.view.doc.blocks[0].status.as_deref(), Some("TODO"), "DONE → TODO");
    assert!(sh.editor.markdown().starts_with("- TODO 일 하나 고침\n"));
    assert_eq!(sh.editor.view.doc.blocks.len(), 2, "블록이 늘지 않는다(Enter 가 아님)");
}

#[test]
fn toggle_works_from_list_and_on_a_row_other_than_the_one_being_edited() {
    let (mut sh, _d) = shell_in("toggle2", &[("t", "- 첫째\n- 둘째\n- 셋째\n")]);
    sh.open_page("t").unwrap();
    sh.editor.view.set_selected(1, 10);
    sh.editor.nav_command(Cmd::Toggle);
    assert_eq!(sh.editor.view.doc.blocks[1].status.as_deref(), Some("TODO"));
    assert_eq!(sh.editor.view.doc.blocks[0].status, None);
    // 0번을 편집 중에 2번의 체크 칸을 누른 것과 같은 호출
    sh.editor.view.set_selected(0, 10);
    sh.editor.start_edit_selected();
    sh.editor.host.type_text("첫째 입력중");
    sh.editor.toggle_block(2);
    assert_eq!(sh.editor.view.doc.blocks[2].status.as_deref(), Some("TODO"));
    assert_eq!(sh.editor.editing_block(), Some(0), "편집은 그 줄에 남는다");
    assert_eq!(sh.editor.view.doc.blocks[0].text, "첫째 입력중", "입력 글도 확정됨");
}

// ---------- 블록 이동 ----------

fn tree_editor() -> Shell<Fake> {
    let (mut sh, _d) = shell_in("move", &[("t", TREE)]);
    sh.open_page("t").unwrap();
    sh
}

fn select_text(sh: &mut Shell<Fake>, text: &str) {
    let row = sh.editor.view.rows.iter().position(|r| r.text == text).unwrap();
    sh.editor.view.set_selected(row, 20);
}

#[test]
fn move_down_carries_children_and_moved_block_stays_selected() {
    let mut sh = tree_editor();
    select_text(&mut sh, "B");
    sh.editor.nav_command(Cmd::MoveDown);
    assert_eq!(sh.editor.markdown(), "- A\n  - A1\n  - A2\n- C\n- B\n  - B1\n");
    let r = &sh.editor.view.rows[sh.editor.view.selected];
    assert_eq!(sh.editor.view.doc.blocks[r.block].text, "B");
    // 맨 아래 형제는 더 못 내려간다(문서 그대로, 안내만)
    let before = sh.editor.markdown();
    sh.editor.nav_command(Cmd::MoveDown);
    assert_eq!(sh.editor.markdown(), before);
    assert!(sh.editor.message.is_some());
}

#[test]
fn move_up_carries_children_over_the_previous_sibling_subtree() {
    let mut sh = tree_editor();
    select_text(&mut sh, "B");
    sh.editor.nav_command(Cmd::MoveUp);
    assert_eq!(sh.editor.markdown(), "- B\n  - B1\n- A\n  - A1\n  - A2\n- C\n");
    sh.editor.nav_command(Cmd::MoveUp);
    assert!(sh.editor.message.is_some(), "맨 위");
    assert!(sh.editor.markdown().starts_with("- B\n"));
}

#[test]
fn move_among_siblings_inside_a_parent_and_errors_at_edges() {
    let mut sh = tree_editor();
    select_text(&mut sh, "A1");
    sh.editor.nav_command(Cmd::MoveDown);
    assert_eq!(sh.editor.markdown(), "- A\n  - A2\n  - A1\n- B\n  - B1\n- C\n");
    let before = sh.editor.markdown();
    sh.editor.nav_command(Cmd::MoveDown); // A1 은 A 의 마지막 자식 — 부모 밖으로 못 나간다
    assert_eq!(sh.editor.markdown(), before);
    select_text(&mut sh, "B1");
    sh.editor.nav_command(Cmd::MoveUp); // B1 위는 부모 B
    assert_eq!(sh.editor.markdown(), before);
    assert!(sh.editor.message.is_some());
}

#[test]
fn move_keeps_folded_state_with_the_block() {
    let mut sh = tree_editor();
    select_text(&mut sh, "A");
    assert!(sh.editor.view.set_fold(true, 20));
    sh.editor.nav_command(Cmd::MoveDown);
    assert_eq!(sh.editor.markdown(), "- B\n  - B1\n- A\n  - A1\n  - A2\n- C\n");
    let a = sh.editor.view.doc.blocks.iter().position(|b| b.text == "A").unwrap();
    assert!(sh.editor.view.doc.blocks[a].folded, "접힘은 A 를 따라간다");
    let b = sh.editor.view.doc.blocks.iter().position(|b| b.text == "B").unwrap();
    assert!(!sh.editor.view.doc.blocks[b].folded);
    assert!(sh.editor.view.rows.iter().all(|r| r.text != "A1"), "A 의 하위는 여전히 숨김");
}

#[test]
fn alt_arrows_move_the_block_while_editing_and_keep_text_and_caret() {
    let mut sh = tree_editor();
    select_text(&mut sh, "B");
    sh.editor.start_edit_selected();
    sh.editor.host.type_text("B 입력중");
    sh.editor.host.sel = (2, 2);
    assert_eq!(sh.editor.key(alt(VK_DOWN)), KeyResult::Swallow);
    assert_eq!(sh.editor.markdown(), "- A\n  - A1\n  - A2\n- C\n- B 입력중\n  - B1\n");
    let nb = sh.editor.editing_block().unwrap();
    assert_eq!(sh.editor.view.doc.blocks[nb].text, "B 입력중");
    assert_eq!(sh.editor.host.text, "B 입력중");
    assert_eq!(sh.editor.host.sel, (2, 2), "캐럿 유지");
    sh.editor.key(alt(VK_UP));
    assert_eq!(sh.editor.markdown(), "- A\n  - A1\n  - A2\n- B 입력중\n  - B1\n- C\n");
}

#[test]
fn alt_move_during_composition_completes_first() {
    let mut sh = tree_editor();
    select_text(&mut sh, "B");
    sh.editor.start_edit_selected();
    sh.editor.host.type_text("B");
    sh.editor.host.composing = true;
    sh.editor.key(alt(VK_DOWN));
    assert_eq!(sh.editor.host.completes, 1);
    assert_eq!(sh.editor.markdown(), "- A\n  - A1\n  - A2\n- C\n- B\n  - B1\n");
}

// ---------- `[[` 자동완성 ----------

fn popup_shell() -> Shell<Fake> {
    let (mut sh, _d) = shell_in(
        "popup",
        &[("회의록", "- x\n"), ("예산 지침", "- x\n"), ("예산 초안", "- x\n"), ("메모", "- 본문 글\n- 둘째\n")],
    );
    sh.open_page("메모").unwrap();
    sh.editor.view.set_selected(0, 10);
    sh.editor.start_edit_selected();
    sh
}

#[test]
fn typing_double_bracket_opens_popup_and_filters_as_you_type() {
    let mut sh = popup_shell();
    sh.editor.host.type_text("참고 [[");
    sh.editor.refresh_popup();
    let p = sh.editor.popup.clone().unwrap();
    assert_eq!(p.ctx.query, "");
    assert!(p.items.len() >= 4, "질의가 비면 모든 페이지(최근 먼저): {:?}", p.items);
    assert_eq!(p.items[0], "메모", "최근 페이지(지금 연 메모)가 먼저");
    sh.editor.host.type_text("참고 [[예산");
    sh.editor.refresh_popup();
    assert_eq!(sh.editor.popup.as_ref().unwrap().items, ["예산 지침", "예산 초안"]);
    sh.editor.host.type_text("참고 [[없는이름");
    sh.editor.refresh_popup();
    assert!(sh.editor.popup.is_none(), "후보가 없으면 닫힘");
    sh.editor.host.type_text("참고 [[예산]] 뒤");
    sh.editor.refresh_popup();
    assert!(sh.editor.popup.is_none(), "링크를 닫으면 닫힘");
}

#[test]
fn arrow_keys_pick_and_enter_inserts_closed_link_without_splitting_block() {
    let mut sh = popup_shell();
    sh.editor.host.type_text("참고 [[예산");
    sh.editor.refresh_popup();
    let blocks = sh.editor.view.doc.blocks.len();
    assert_eq!(sh.editor.key(key(VK_DOWN)), KeyResult::Swallow);
    assert_eq!(sh.editor.popup.as_ref().unwrap().selected, 1);
    assert_eq!(sh.editor.key(key(VK_DOWN)), KeyResult::Swallow);
    assert_eq!(sh.editor.popup.as_ref().unwrap().selected, 0, "끝에서 처음으로 돈다");
    sh.editor.key(key(VK_DOWN));
    assert_eq!(sh.editor.key(key(VK_RETURN)), KeyResult::Swallow);
    assert_eq!(sh.editor.host.text, "참고 [[예산 초안]]");
    let end = "참고 [[예산 초안]]".encode_utf16().count();
    assert_eq!(sh.editor.host.sel, (end, end), "캐럿은 ]] 뒤");
    assert!(sh.editor.popup.is_none());
    assert_eq!(sh.editor.view.doc.blocks.len(), blocks, "Enter 가 블록 분할로 새지 않았다");
    assert!(sh.editor.editing());
}

#[test]
fn tab_also_accepts_and_up_arrow_wraps() {
    let mut sh = popup_shell();
    sh.editor.host.type_text("[[예산");
    sh.editor.refresh_popup();
    sh.editor.key(key(VK_UP));
    assert_eq!(sh.editor.popup.as_ref().unwrap().selected, 1);
    sh.editor.key(key(VK_TAB));
    assert_eq!(sh.editor.host.text, "[[예산 초안]]");
}

#[test]
fn accept_by_index_for_mouse_click_and_cursor_after_existing_closing_brackets() {
    let mut sh = popup_shell();
    sh.editor.host.set_text("[[예산]] 뒤", 4);
    sh.editor.refresh_popup();
    assert!(sh.editor.popup.is_some());
    assert!(sh.editor.accept_popup(Some(1)));
    assert_eq!(sh.editor.host.text, "[[예산 초안]] 뒤", "]] 를 중복하지 않는다");
    assert_eq!(sh.editor.host.sel.0, "[[예산 초안]]".encode_utf16().count());
    assert!(!sh.editor.accept_popup(None), "팝업이 없으면 아무 일 없음");
}

#[test]
fn escape_closes_popup_first_then_stays_closed_until_the_link_changes() {
    let mut sh = popup_shell();
    sh.editor.host.type_text("[[예산");
    sh.editor.refresh_popup();
    assert_eq!(sh.editor.key(key(VK_ESCAPE)), KeyResult::Swallow);
    assert!(sh.editor.popup.is_none());
    assert!(sh.editor.editing(), "첫 Esc 는 팝업만 닫는다(편집은 유지)");
    sh.editor.host.type_text("[[예산 지");
    sh.editor.refresh_popup();
    assert!(sh.editor.popup.is_none(), "같은 [[ 에서는 다시 열지 않는다");
    sh.editor.host.type_text("[[예산 지침]] 그리고 [[회");
    sh.editor.refresh_popup();
    assert!(sh.editor.popup.is_some(), "새 [[ 에서는 열린다");
    sh.editor.key(key(VK_ESCAPE));
    sh.editor.key(key(VK_ESCAPE)); // 팝업 없음 → 편집 취소
    assert!(!sh.editor.editing());
}

#[test]
fn enter_while_composing_with_popup_open_only_completes_composition() {
    let mut sh = popup_shell();
    sh.editor.host.type_text("[[예");
    sh.editor.host.composing = true;
    sh.editor.refresh_popup();
    let blocks = sh.editor.view.doc.blocks.len();
    assert_eq!(sh.editor.key(key(VK_RETURN)), KeyResult::Swallow);
    assert_eq!(sh.editor.host.completes, 1, "조합만 확정");
    assert_eq!(sh.editor.view.doc.blocks.len(), blocks, "블록을 나누지 않는다");
    assert!(sh.editor.popup.is_some(), "다시 Enter 를 눌러야 고른다");
    // VK_PROCESSKEY 로 온 키는 IME 에 맡긴다
    sh.editor.host.composing = true;
    assert_eq!(sh.editor.key(KeyEvent::process(VK_RETURN, 2000)), KeyResult::Pass);
    sh.editor.host.composing = false;
    // 후보창(한자)이 열려 있으면 팝업 키 처리를 건너뛴다
    sh.editor.host.candidate = true;
    assert_eq!(sh.editor.key(key(VK_DOWN)), KeyResult::Pass);
    assert_eq!(sh.editor.popup.as_ref().unwrap().selected, 0);
}

#[test]
fn popup_closes_when_selection_is_not_a_plain_caret_or_when_editing_ends() {
    let mut sh = popup_shell();
    sh.editor.host.type_text("[[예산");
    sh.editor.refresh_popup();
    assert!(sh.editor.popup.is_some());
    sh.editor.host.sel = (0, 3);
    sh.editor.refresh_popup();
    assert!(sh.editor.popup.is_none());
    sh.editor.host.sel = (5, 5);
    sh.editor.refresh_popup();
    assert!(sh.editor.popup.is_some());
    sh.editor.key(key(VK_ESCAPE));
    sh.editor.key(key(VK_ESCAPE));
    assert!(sh.editor.popup.is_none() && !sh.editor.editing());
}

#[test]
fn without_popup_enter_still_splits_and_ctrl_enter_does_not_split() {
    let (mut sh, _d) = shell_in("nopopup", &[("t", "- 가나\n")]);
    sh.open_page("t").unwrap();
    sh.editor.start_edit_selected();
    sh.editor.host.type_text("가나");
    sh.editor.host.sel = (1, 1);
    sh.editor.key(key(VK_RETURN));
    assert_eq!(sh.editor.view.doc.blocks.len(), 2, "기존 Enter 분할은 그대로");
    assert_eq!(sh.editor.view.doc.blocks[1].text, "나");
}

// ---------- OUTLINERWIN5: 새 블록 단추·왼쪽 목록 이름 필터 ----------

#[test]
fn add_block_at_end_appends_same_indent_block_and_starts_editing() {
    let (mut sh, _d) = shell_in("addblk", &[("가", "- A\n  - A1\n")]);
    sh.open_page("가").unwrap();
    // 입력 중인 글은 먼저 확정한다.
    sh.editor.start_edit_selected();
    sh.editor.host.type_text("A 고침");
    assert!(sh.editor.add_block_at_end(10));
    assert_eq!(sh.editor.markdown(), "- A 고침\n  - A1\n  - \n");
    assert_eq!(sh.editor.editing_block(), Some(2));
    assert_eq!((sh.editor.host.text.as_str(), sh.editor.host.sel), ("", (0, 0)));
    // 편집 중이 아니어도 된다(목록 상태에서 단추).
    sh.editor.finish_edit();
    assert!(sh.editor.add_block_at_end(10));
    assert_eq!(sh.editor.view.doc.blocks.len(), 4);
}

#[test]
fn add_block_at_end_reveals_folded_ancestors_and_fails_on_empty_document() {
    let (mut sh, _d) = shell_in("addblk2", &[("가", "- A\n  - A1\n")]);
    sh.open_page("가").unwrap();
    sh.editor.view.set_selected(0, 10);
    sh.editor.view.set_fold(true, 10);
    assert_eq!(sh.editor.view.rows.len(), 1, "A1 은 접혀 숨는다");
    assert!(sh.editor.add_block_at_end(10));
    assert_eq!(sh.editor.view.rows.len(), 3, "접힌 조상을 펼쳐 새 줄이 보인다");
    // 블록이 0개인 문서: 오류 안내만 하고 바뀌지 않는다.
    let view = outliner_win::view::View::from_markdown("").unwrap();
    let mut ed = Editor::new(view, Fake::default(), None, String::new());
    assert!(!ed.add_block_at_end(10));
    assert!(ed.message.is_some() && ed.view.doc.blocks.is_empty());
}

#[test]
fn left_list_filters_all_pages_by_search_text_but_keeps_recent() {
    use outliner_win::layout::LeftItem;
    let (mut sh, _d) = shell_in("leftflt", &[("예산 지침", "- a\n"), ("회의록", "- b\n"), ("주간 업무", "- c\n"), ("Budget", "- d\n")]);
    let names = |sh: &Shell<Fake>| -> Vec<String> {
        sh.left_list().into_iter().filter_map(|i| if let LeftItem::Page(n) = i { Some(n) } else { None }).collect()
    };
    let labels = |sh: &Shell<Fake>| -> Vec<String> {
        sh.left_list().into_iter().filter_map(|i| if let LeftItem::Label(n) = i { Some(n) } else { None }).collect()
    };
    let all = names(&sh);
    assert_eq!(all.len(), 5 + 1, "전체 5개 + 최근(오늘 일지) 1줄");
    sh.set_query("예");
    assert_eq!(labels(&sh)[1], "전체 페이지 (1/5)");
    assert!(names(&sh).contains(&"예산 지침".to_string()) && !names(&sh).contains(&"회의록".to_string()));
    sh.left_first = 3;
    sh.set_query("BUD");
    assert_eq!(sh.left_first, 0, "질의가 바뀌면 맨 위로");
    assert!(names(&sh).contains(&"Budget".to_string()), "대소문자 무시");
    sh.set_query("없는이름");
    assert_eq!(labels(&sh)[1], "전체 페이지 (0/5)");
    assert!(sh.left_list().iter().filter(|i| matches!(i, LeftItem::Page(_))).count() == 1, "최근 줄만 남는다");
    // 이름 입력 줄이 열려 있으면 거르지 않는다.
    sh.begin_prompt(PromptKind::NewPage);
    assert_eq!(names(&sh).len(), 6);
    sh.cancel_prompt();
    sh.close_search();
    assert_eq!(names(&sh), all);
}
