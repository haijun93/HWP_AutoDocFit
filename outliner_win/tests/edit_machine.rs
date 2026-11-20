//! 편집 상태 기계 시험: Enter 분할·Tab·삭제·합치기·Esc 복원, IME 메시지 순서 주입, 엔진 오류 보존.
//! Win32 없이 가짜 `EditHost` 로 "조합 중 글자"와 메시지 순서를 흉내 낸다.
use outliner_win::edit::*;
use outliner_win::view::View;

/// EDIT + IME 를 흉내 낸 가짜. `comp` 는 아직 확정되지 않은 조합 글자(GetWindowText 에는 안 보인다고 가정 —
/// 가장 위험한 경우라서, 확정 없이 읽으면 글자를 잃는다).
#[derive(Default)]
struct Fake {
    text: String,
    caret: usize,
    comp: String,
    sync_complete: bool,
    candidate: bool,
    completes: usize,
    selection_end: Option<usize>,
}

impl Fake {
    /// IME 가 조합을 글에 확정한다(캐럿 앞에 끼워 넣음).
    fn commit(&mut self) {
        let comp = std::mem::take(&mut self.comp);
        let at = self.text.encode_utf16().take(self.caret).count();
        let _ = at;
        let byte: usize = {
            let mut n = 0;
            let mut b = self.text.len();
            for (i, c) in self.text.char_indices() {
                if n >= self.caret {
                    b = i;
                    break;
                }
                n += c.len_utf16();
            }
            b
        };
        self.text.insert_str(byte, &comp);
        self.caret += comp.encode_utf16().count();
    }
}

impl EditHost for Fake {
    fn read_text(&mut self) -> String {
        self.text.clone()
    }
    fn selection(&mut self) -> (usize, usize) {
        (self.caret, self.selection_end.unwrap_or(self.caret))
    }
    fn set_text(&mut self, text: &str, caret: usize) {
        self.text = text.to_string();
        self.caret = caret;
        self.comp.clear();
        self.selection_end = None;
    }
    fn composing(&mut self) -> bool {
        !self.comp.is_empty()
    }
    fn candidate_open(&mut self) -> bool {
        self.candidate
    }
    fn request_complete(&mut self) {
        self.completes += 1;
        if self.sync_complete {
            self.commit();
        }
    }
}

fn editor(md: &str, sync: bool) -> Editor<Fake> {
    let view = View::from_markdown(md).unwrap();
    let base = outliner_core::serialize(&view.doc);
    Editor::new(view, Fake { sync_complete: sync, ..Fake::default() }, None, base)
}

fn texts(ed: &Editor<Fake>) -> Vec<String> {
    ed.view.doc.blocks.iter().map(|b| b.text.clone()).collect()
}

fn start(ed: &mut Editor<Fake>, row: usize) {
    ed.view.set_selected(row, 10);
    assert!(ed.start_edit_selected());
}

#[test]
fn split_utf16_handles_korean_and_surrogates() {
    assert_eq!(split_utf16("안녕하세요", 2, 2), ("안녕".into(), "하세요".into()));
    assert_eq!(split_utf16("안녕하세요", 0, 0), ("".into(), "안녕하세요".into()));
    assert_eq!(split_utf16("안녕", 99, 99), ("안녕".into(), "".into()));
    // 선택 범위는 지우고 나눈다.
    assert_eq!(split_utf16("가나다라", 1, 3), ("가".into(), "라".into()));
    // 😀 는 UTF-16 두 칸 — 한가운데(1)는 앞 경계로.
    assert_eq!(split_utf16("a😀b", 2, 2), ("a".into(), "😀b".into()));
    assert_eq!(split_utf16("a😀b", 3, 3), ("a😀".into(), "b".into()));
}

#[test]
fn enter_splits_at_caret_and_moves_edit_to_new_block() {
    let mut ed = editor("- 첫째\n- 안녕하세요\n- 끝\n", false);
    start(&mut ed, 1);
    ed.host.caret = 2;
    assert_eq!(ed.key(KeyEvent::new(VK_RETURN, 100)), KeyResult::Swallow);
    assert_eq!(texts(&ed), ["첫째", "안녕", "하세요", "끝"]);
    assert_eq!(ed.editing_block(), Some(2));
    assert_eq!((ed.host.text.as_str(), ed.host.caret), ("하세요", 0));
    assert_eq!(ed.view.selected, 2);
    assert_eq!(ed.markdown(), "- 첫째\n- 안녕\n- 하세요\n- 끝\n");
}

#[test]
fn enter_at_end_adds_empty_sibling_with_same_indent() {
    let mut ed = editor("- a\n - b\n", false);
    start(&mut ed, 1);
    ed.key(KeyEvent::new(VK_RETURN, 0));
    assert_eq!(ed.markdown(), "- a\n - b\n - \n");
    assert_eq!(ed.editing_block(), Some(2));
}

#[test]
fn typed_text_is_committed_with_the_command() {
    let mut ed = editor("- 가\n- 나\n", false);
    start(&mut ed, 0);
    ed.host.text = "가가가 수정".into();
    ed.host.caret = ed.host.text.encode_utf16().count();
    ed.key(KeyEvent::new(VK_DOWN, 0)); // ↓ = 확정 후 다음 블록 편집
    assert_eq!(texts(&ed), ["가가가 수정", "나"]);
    assert_eq!(ed.editing_block(), Some(1));
    assert_eq!(ed.host.text, "나");
}

#[test]
fn tab_and_shift_tab_indent_outdent_and_keep_text() {
    let mut ed = editor("- a\n- b\n", false);
    start(&mut ed, 1);
    ed.host.text = "b 수정".into();
    ed.key(KeyEvent::new(VK_TAB, 0));
    assert_eq!(ed.markdown(), "- a\n - b 수정\n");
    assert!(ed.editing());
    let mut sh = KeyEvent::new(VK_TAB, 1);
    sh.shift = true;
    ed.key(sh);
    assert_eq!(ed.markdown(), "- a\n- b 수정\n");
    assert_eq!(ed.host.text, "b 수정", "EDIT 글은 그대로");
}

#[test]
fn indent_error_keeps_typed_text_and_reports() {
    let mut ed = editor("- a\n- b\n", false);
    start(&mut ed, 0);
    ed.host.text = "a2".into();
    ed.key(KeyEvent::new(VK_TAB, 0)); // 첫 블록은 들여쓸 수 없다(엔진 오류)
    assert!(ed.message.as_deref().unwrap_or("").contains("들여쓸 수 없습니다"), "{:?}", ed.message);
    assert_eq!(ed.host.text, "a2", "오류가 나도 EDIT 글은 지우지 않는다");
    assert_eq!(ed.markdown(), "- a2\n- b\n", "글 자체는 확정됨");
    assert!(ed.editing());
}

#[test]
fn backspace_on_empty_block_deletes_and_edits_previous_at_end() {
    let mut ed = editor("- 앞\n- \n- 뒤\n", false);
    start(&mut ed, 1);
    assert_eq!(ed.key(KeyEvent::new(VK_BACK, 0)), KeyResult::Swallow);
    assert_eq!(texts(&ed), ["앞", "뒤"]);
    assert_eq!(ed.editing_block(), Some(0));
    assert_eq!((ed.host.text.as_str(), ed.host.caret), ("앞", 1));
}

#[test]
fn backspace_in_middle_of_text_is_left_to_the_edit_control() {
    let mut ed = editor("- 앞\n- 뒤\n", false);
    start(&mut ed, 1);
    ed.host.caret = 1;
    assert_eq!(ed.key(KeyEvent::new(VK_BACK, 0)), KeyResult::Pass);
    assert_eq!(texts(&ed), ["앞", "뒤"]);
}

#[test]
fn backspace_at_start_merges_with_previous_block() {
    let mut ed = editor("- 앞\n- 뒤\n- 끝\n", false);
    start(&mut ed, 1);
    ed.host.caret = 0;
    ed.key(KeyEvent::new(VK_BACK, 0));
    // 엔진 merge 는 두 글 사이에 공백 1개를 넣는다(Flutter 는 공백 없음 — 차이로 보고).
    assert_eq!(texts(&ed), ["앞 뒤", "끝"]);
    assert_eq!((ed.host.text.as_str(), ed.host.caret), ("앞 뒤", 2), "캐럿은 이어 붙인 자리(공백 뒤)");
    assert_eq!(ed.editing_block(), Some(0));
}

#[test]
fn only_block_is_never_deleted() {
    let mut ed = editor("- \n", false);
    start(&mut ed, 0);
    ed.key(KeyEvent::new(VK_BACK, 0));
    assert_eq!(ed.view.doc.blocks.len(), 1);
}

#[test]
fn esc_discards_uncommitted_typing_and_leaves_document_unchanged() {
    let mut ed = editor("- 원래 글\n", false);
    start(&mut ed, 0);
    ed.host.text = "원래 글에 덧붙인 것".into();
    assert!(ed.dirty(), "입력만 해도 변경 표시");
    assert_eq!(ed.key(KeyEvent::new(VK_ESCAPE, 0)), KeyResult::Swallow);
    assert!(!ed.editing());
    assert_eq!(ed.markdown(), "- 원래 글\n");
    assert!(!ed.dirty());
}

#[test]
fn esc_keeps_what_an_earlier_command_already_committed() {
    let mut ed = editor("- a\n- b\n", false);
    start(&mut ed, 1);
    ed.host.text = "b1".into();
    ed.key(KeyEvent::new(VK_TAB, 0)); // 확정: "b1" + 들여쓰기
    ed.host.text = "b1 더".into();
    ed.key(KeyEvent::new(VK_ESCAPE, 1));
    assert_eq!(ed.markdown(), "- a\n - b1\n", "Esc 는 마지막 확정 이후의 입력만 버린다");
}

// ---------------- IME 순서 주입 ----------------

const RESULT_MD: &str = "- 안녕\n- \n";

fn ime_editor(sync: bool) -> Editor<Fake> {
    let mut ed = editor("- \n", sync);
    start(&mut ed, 0);
    // "안" 은 확정, "녕" 은 조합 중, 캐럿은 끝.
    ed.host.text = "안".into();
    ed.host.caret = 1;
    ed.host.comp = "녕".into();
    ed
}

#[test]
fn composing_enter_completes_first_then_splits_once_sync() {
    let mut ed = ime_editor(true);
    assert_eq!(ed.key(KeyEvent::process(VK_RETURN, 100)), KeyResult::Pass);
    assert_eq!(ed.host.completes, 1);
    assert_eq!(ed.markdown(), RESULT_MD, "확정된 글자가 그대로 남고 빈 블록은 1개");
    assert_eq!(ed.editing_block(), Some(1));
    assert!(!ed.waiting());
}

#[test]
fn composing_enter_waits_for_end_of_composition_when_async() {
    let mut ed = ime_editor(false);
    ed.key(KeyEvent::process(VK_RETURN, 100));
    assert!(ed.waiting(), "F 상태: 조합 종료 대기");
    assert_eq!(ed.markdown(), "- \n", "조합이 끝나기 전에는 글을 읽거나 저장하지 않는다");
    ed.run_pending(120); // 아직 조합 중 + 제한 시간 전 → 계속 대기
    assert!(ed.waiting());
    ed.host.commit(); // WM_IME_ENDCOMPOSITION 도착
    ed.run_pending(130);
    assert_eq!(ed.markdown(), RESULT_MD);
    assert!(!ed.waiting());
}

#[test]
fn confirmation_timeout_runs_the_command_with_what_is_in_the_edit() {
    let mut ed = ime_editor(false);
    ed.key(KeyEvent::process(VK_RETURN, 100));
    ed.run_pending(299);
    assert!(ed.waiting());
    ed.run_pending(300); // 200 ms 경과 — 종료 알림이 끝내 안 와도 명령은 실행(기록용 동작)
    assert!(!ed.waiting());
    assert_eq!(ed.view.doc.blocks.len(), 2);
}

#[test]
fn same_result_for_every_message_order() {
    // A: VK_PROCESSKEY 만(조합 확정은 즉시)
    let mut a = ime_editor(true);
    a.key(KeyEvent::process(VK_RETURN, 100));
    // B: VK_PROCESSKEY 후 확정 종료 알림이 늦게 옴
    let mut b = ime_editor(false);
    b.key(KeyEvent::process(VK_RETURN, 100));
    b.host.commit();
    b.run_pending(105);
    // C: VK_PROCESSKEY → 확정 → 진짜 VK_RETURN 이 20 ms 뒤 한 번 더 옴(중복이어야 함)
    let mut c = ime_editor(true);
    c.key(KeyEvent::process(VK_RETURN, 100));
    assert_eq!(c.key(KeyEvent::new(VK_RETURN, 120)), KeyResult::Swallow);
    // D: 진짜 VK_RETURN 이 조합 중에 도착(TSF 형태) — 확정 요청 후 실행
    let mut d = ime_editor(true);
    assert_eq!(d.key(KeyEvent::new(VK_RETURN, 100)), KeyResult::Swallow);
    // E: 조합이 이미 확정된 뒤 Enter 한 번
    let mut e = editor("- \n", true);
    start(&mut e, 0);
    e.host.text = "안녕".into();
    e.host.caret = 2;
    e.key(KeyEvent::new(VK_RETURN, 100));
    // F: B 와 같지만 진짜 VK_RETURN 이 확정 대기 중에 도착
    let mut f = ime_editor(false);
    f.key(KeyEvent::process(VK_RETURN, 100));
    assert_eq!(f.key(KeyEvent::new(VK_RETURN, 110)), KeyResult::Swallow);
    f.host.commit();
    f.run_pending(112);
    assert_eq!(f.key(KeyEvent::new(VK_RETURN, 118)), KeyResult::Swallow, "실행 직후 오는 진짜 키도 중복");
    for (name, ed) in [("A", &a), ("B", &b), ("C", &c), ("D", &d), ("E", &e), ("F", &f)] {
        assert_eq!(ed.markdown(), RESULT_MD, "순서 {name}");
        assert_eq!(ed.view.doc.blocks.len(), 2, "순서 {name}: 블록 수");
        assert_eq!(ed.editing_block(), Some(1), "순서 {name}");
    }
}

#[test]
fn separate_physical_enters_are_not_deduplicated() {
    let mut ed = editor("- 가나다\n", true);
    start(&mut ed, 0);
    ed.key(KeyEvent::new(VK_RETURN, 1000));
    ed.key(KeyEvent::new(VK_RETURN, 1200));
    ed.key(KeyEvent::new(VK_RETURN, 1400));
    assert_eq!(texts(&ed), ["가나다", "", "", ""], "Enter 3번 = 새 블록 3개, 키 버림 0");
}

#[test]
fn rapid_enters_while_waiting_are_queued_in_order_not_dropped() {
    let mut ed = editor("- \n", false);
    start(&mut ed, 0);
    ed.host.text = "가".into();
    ed.host.caret = 1;
    ed.host.comp = "나".into();
    ed.key(KeyEvent::process(VK_RETURN, 100)); // 대기 시작
    ed.key(KeyEvent::new(VK_RETURN, 200)); // 별개 물리 키(100 ms 뒤) → 큐
    ed.host.commit();
    ed.run_pending(210);
    // 첫 Enter: "가나" | "" , 둘째 Enter: 빈 블록에서 또 하나.
    assert_eq!(texts(&ed), ["가나", "", ""]);
}

#[test]
fn composing_tab_commits_letters_then_indents() {
    let mut ed = editor("- 위\n- \n", true);
    start(&mut ed, 1);
    ed.host.text = "안".into();
    ed.host.caret = 1;
    ed.host.comp = "녕".into();
    ed.key(KeyEvent::process(VK_TAB, 10));
    assert_eq!(ed.markdown(), "- 위\n - 안녕\n");
    assert!(ed.editing());
    // Shift+Tab 도 조합 중에 같은 순서.
    ed.host.comp = "ㅎ".into();
    let mut sh = KeyEvent::process(VK_TAB, 500);
    sh.shift = true;
    ed.key(sh);
    assert_eq!(ed.markdown(), "- 위\n- 안녕ㅎ\n");
}

#[test]
fn composing_arrow_commits_then_moves_to_next_block() {
    let mut ed = editor("- 가\n- 나\n", true);
    start(&mut ed, 0);
    ed.host.text = "가".into();
    ed.host.caret = 1;
    ed.host.comp = "ㄴ".into();
    ed.key(KeyEvent::process(VK_DOWN, 10));
    assert_eq!(texts(&ed), ["가ㄴ", "나"]);
    assert_eq!(ed.editing_block(), Some(1));
}

#[test]
fn composing_backspace_and_esc_belong_to_the_ime() {
    let mut ed = ime_editor(true);
    let before = ed.markdown();
    assert_eq!(ed.key(KeyEvent::process(VK_BACK, 1)), KeyResult::Pass);
    assert_eq!(ed.key(KeyEvent::new(VK_BACK, 2)), KeyResult::Pass, "조합 중 진짜 Backspace 도 IME 소유");
    assert_eq!(ed.key(KeyEvent::process(VK_ESCAPE, 3)), KeyResult::Pass);
    assert_eq!(ed.key(KeyEvent::new(VK_ESCAPE, 4)), KeyResult::Pass);
    assert!(ed.editing(), "조합 중 Esc 가 편집을 끝내면 안 된다");
    assert_eq!(ed.markdown(), before);
    assert_eq!(ed.host.completes, 0);
}

#[test]
fn empty_block_backspace_after_composition_ended_deletes() {
    // 시나리오 7: 조합 중에는 삭제 없음, 조합이 끝난 빈 칸의 다음 Backspace 가 delete.
    let mut ed = editor("- 앞\n- \n", true);
    start(&mut ed, 1);
    ed.host.comp = "ㄱ".into();
    assert_eq!(ed.key(KeyEvent::process(VK_BACK, 1)), KeyResult::Pass);
    assert_eq!(ed.view.doc.blocks.len(), 2);
    ed.host.comp.clear(); // 자모가 모두 지워져 조합 종료
    assert_eq!(ed.key(KeyEvent::new(VK_BACK, 2)), KeyResult::Swallow);
    assert_eq!(texts(&ed), ["앞"]);
}

#[test]
fn candidate_window_keys_are_left_to_the_ime() {
    let mut ed = ime_editor(true);
    ed.host.candidate = true;
    assert_eq!(ed.key(KeyEvent::process(VK_RETURN, 1)), KeyResult::Pass);
    assert_eq!(ed.key(KeyEvent::process(VK_DOWN, 2)), KeyResult::Pass);
    assert_eq!(ed.view.doc.blocks.len(), 1);
    assert_eq!(ed.host.completes, 0);
}

#[test]
fn ime_policy_alternatives_are_switchable() {
    // CompleteOnly: 조합만 확정하고 명령은 버린다(두 번째 Enter 부터 분할).
    let mut ed = ime_editor(true);
    ed.policy.enter = ComposingKey::CompleteOnly;
    ed.key(KeyEvent::process(VK_RETURN, 1));
    assert_eq!((ed.host.completes, ed.view.doc.blocks.len()), (1, 1));
    assert_eq!(ed.host.text, "안녕");
    ed.key(KeyEvent::new(VK_RETURN, 500));
    assert_eq!(ed.markdown(), RESULT_MD);
    // Ignore: 아무것도 안 함.
    let mut ed = ime_editor(true);
    ed.policy.enter = ComposingKey::Ignore;
    assert_eq!(ed.key(KeyEvent::process(VK_RETURN, 1)), KeyResult::Pass);
    assert_eq!((ed.host.completes, ed.view.doc.blocks.len()), (0, 1));
}

#[test]
fn ctrl_s_commits_typing_before_saving() {
    let mut ed = editor("- a\n", false);
    start(&mut ed, 0);
    ed.host.text = "a2".into();
    let mut k = KeyEvent::new(VK_S, 0);
    k.ctrl = true;
    assert_eq!(ed.key(k), KeyResult::Swallow);
    assert_eq!(ed.markdown(), "- a2\n", "저장 전에 EDIT 글이 문서에 들어간다");
    assert_eq!(ed.message.as_deref(), Some("저장할 파일이 없습니다"), "파일 없이는 저장 안 함(시험용 편집기)");
}

#[test]
fn shift_enter_is_swallowed_and_changes_nothing() {
    let mut ed = editor("- a\n", false);
    start(&mut ed, 0);
    let mut k = KeyEvent::new(VK_RETURN, 0);
    k.shift = true;
    assert_eq!(ed.key(k), KeyResult::Swallow);
    assert_eq!(ed.view.doc.blocks.len(), 1);
}

#[test]
fn fold_state_survives_edits_elsewhere() {
    let mut ed = editor("- 가\n - 가1\n- 나\n- 다\n - 다1\n", false);
    ed.view.set_selected(0, 10);
    assert!(ed.view.set_fold(true, 10)); // 가 접음 → 가1 숨김
    assert_eq!(ed.view.rows.len(), 4);
    ed.view.set_selected(1, 10); // 나
    ed.start_edit_selected();
    ed.host.caret = 0;
    ed.host.text = "나".into();
    ed.key(KeyEvent::new(VK_RETURN, 0)); // 앞에 빈 블록: ["", "나"]
    assert!(ed.view.doc.blocks[0].folded, "가 는 여전히 접힘");
    assert_eq!(ed.view.rows.iter().map(|r| r.block).collect::<Vec<_>>(), vec![0, 2, 3, 4, 5]);
    assert_eq!(texts(&ed), ["가", "가1", "", "나", "다", "다1"]);
}

#[test]
fn goto_moves_edit_after_committing() {
    let mut ed = editor("- a\n- b\n- c\n", false);
    start(&mut ed, 0);
    ed.host.text = "a!".into();
    ed.submit(Cmd::Goto(2), 0, false, false);
    assert_eq!(texts(&ed), ["a!", "b", "c"]);
    assert_eq!(ed.editing_block(), Some(2));
    assert_eq!(ed.host.text, "c");
}

#[test]
fn nav_mode_tab_indents_selected_block_without_editing() {
    let mut ed = editor("- a\n- b\n", false);
    ed.view.set_selected(1, 10);
    ed.nav_command(Cmd::Indent);
    assert_eq!(ed.markdown(), "- a\n - b\n");
    assert!(!ed.editing());
}

#[test]
fn sample_document_roundtrips_without_changes_and_edit_marks_dirty() {
    let mut ed = editor(outliner_win::SAMPLE_MD, false);
    assert_eq!(ed.markdown(), outliner_win::SAMPLE_MD, "편집 전 직렬화는 원문 그대로");
    assert!(!ed.dirty());
    start(&mut ed, 0);
    ed.host.text.push('!');
    assert!(ed.dirty());
}
