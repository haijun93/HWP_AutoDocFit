//! 문서 단위 실행 취소/다시 실행(Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z) 시험. Win32 없이 가짜 `EditHost` 로 한다.
use outliner_win::edit::*;
use outliner_win::view::View;

#[derive(Default)]
struct Fake {
    text: String,
    caret: usize,
    comp: String,
}

impl EditHost for Fake {
    fn read_text(&mut self) -> String {
        self.text.clone()
    }
    fn selection(&mut self) -> (usize, usize) {
        (self.caret, self.caret)
    }
    fn set_text(&mut self, text: &str, caret: usize) {
        self.text = text.to_string();
        self.caret = caret;
        self.comp.clear();
    }
    fn composing(&mut self) -> bool {
        !self.comp.is_empty()
    }
    fn candidate_open(&mut self) -> bool {
        false
    }
    fn request_complete(&mut self) {
        self.text.push_str(&std::mem::take(&mut self.comp));
    }
}

fn editor(md: &str) -> Editor<Fake> {
    let view = View::from_markdown(md).unwrap();
    let base = outliner_core::serialize(&view.doc);
    Editor::new(view, Fake::default(), None, base)
}

fn start(ed: &mut Editor<Fake>, row: usize) {
    ed.view.set_selected(row, 10);
    assert!(ed.start_edit_selected());
}

fn ctrl(vk: u16, shift: bool) -> KeyEvent {
    let mut ev = KeyEvent::new(vk, 0);
    ev.ctrl = true;
    ev.shift = shift;
    ev
}

#[test]
fn undo_and_redo_many_steps_restore_exact_markdown() {
    let mut ed = editor("- 가\n- 나\n");
    let mut states = vec![ed.markdown()];
    start(&mut ed, 0);
    ed.host.text = "한글 닭 앎".into();
    ed.key(KeyEvent::new(VK_RETURN, 0)); // 확정 + 분할 = 한 단계
    states.push(ed.markdown());
    ed.key(KeyEvent::new(VK_TAB, 0)); // 들여쓰기
    states.push(ed.markdown());
    ed.host.text = "새 글".into();
    ed.key(KeyEvent::new(VK_DOWN, 0)); // 글 확정 + 다음 블록으로
    states.push(ed.markdown());
    assert_eq!(ed.history.undo_len(), 3, "명령 하나가 한 단계");

    // 편집 중(확정하지 않은 글 없음)에서도 Ctrl+Z 는 문서 단위로 되돌린다.
    for expected in states.iter().rev().skip(1) {
        assert_eq!(ed.key(ctrl(VK_Z, false)), KeyResult::Swallow);
        assert_eq!(&ed.markdown(), expected);
    }
    assert!(!ed.can_undo());
    assert_eq!(ed.key(ctrl(VK_Z, false)), KeyResult::Swallow, "더 없어도 EDIT 로 새지 않는다");
    assert_eq!(ed.markdown(), states[0]);
    assert!(ed.editing(), "되돌린 뒤에도 편집을 이어 간다");

    for expected in states.iter().skip(1) {
        assert_eq!(ed.key(ctrl(VK_Y, false)), KeyResult::Swallow);
        assert_eq!(&ed.markdown(), expected);
    }
    assert!(!ed.can_redo());
    // Ctrl+Shift+Z 도 다시 실행.
    ed.key(ctrl(VK_Z, false));
    assert_eq!(ed.markdown(), states[2]);
    ed.key(ctrl(VK_Z, true));
    assert_eq!(ed.markdown(), states[3]);
}

#[test]
fn enter_commit_and_split_is_one_undo_step() {
    let mut ed = editor("- 안녕하세요\n");
    start(&mut ed, 0);
    ed.host.caret = 2;
    ed.key(KeyEvent::new(VK_RETURN, 0));
    assert_eq!(ed.markdown(), "- 안녕\n- 하세요\n");
    assert_eq!(ed.history.undo_len(), 1);
    ed.key(ctrl(VK_Z, false));
    assert_eq!(ed.markdown(), "- 안녕하세요\n");
    assert!(ed.editing(), "되돌린 뒤에도 같은 블록 편집을 이어 간다");
    assert_eq!(ed.host.text, "안녕하세요");
}

#[test]
fn unconfirmed_typing_or_composition_passes_ctrl_z_to_the_edit_control() {
    let mut ed = editor("- 하나\n");
    start(&mut ed, 0);
    ed.host.text = "하나 둘".into();
    ed.key(KeyEvent::new(VK_RETURN, 0));
    let after_enter = ed.markdown();
    // 새 블록에서 글을 쓰는 중이면 EDIT 의 글자 되돌리기에 맡긴다(문서는 그대로).
    ed.host.text = "미확정".into();
    assert_eq!(ed.key(ctrl(VK_Z, false)), KeyResult::Pass);
    assert_eq!(ed.key(ctrl(VK_Y, false)), KeyResult::Pass);
    assert_eq!(ed.markdown(), after_enter);
    // 한글 조합 중에도 마찬가지.
    ed.host.text = String::new();
    ed.host.comp = "ㅎ".into();
    assert_eq!(ed.key(ctrl(VK_Z, false)), KeyResult::Pass);
    assert_eq!(ed.markdown(), after_enter);
    assert!(ed.can_undo());
}

#[test]
fn new_edit_clears_redo_and_replace_document_clears_history() {
    let mut ed = editor("- a\n- b\n");
    ed.view.set_selected(1, 10);
    ed.nav_command(Cmd::Indent);
    assert_eq!(ed.markdown(), "- a\n - b\n");
    assert!(ed.history_step(true));
    assert_eq!(ed.markdown(), "- a\n- b\n");
    assert!(ed.can_redo());
    ed.view.set_selected(1, 10);
    ed.nav_command(Cmd::MoveUp);
    assert!(!ed.can_redo(), "새 편집은 다시 실행을 비운다");
    assert!(ed.can_undo());

    let view = View::from_markdown("- 다른 페이지\n").unwrap();
    let base = outliner_core::serialize(&view.doc);
    ed.replace_document(view, None, base);
    assert!(!ed.can_undo() && !ed.can_redo(), "페이지 전환은 이력을 비운다");
}

#[test]
fn list_mode_undo_restores_status_and_move() {
    let original = "- 부모\n - 자식\n- 끝\n";
    let mut ed = editor(original);
    ed.view.set_selected(0, 10);
    ed.nav_command(Cmd::Toggle); // TODO 표시
    let todo = ed.markdown();
    assert_ne!(todo, original);
    ed.view.set_selected(0, 10);
    ed.nav_command(Cmd::MoveDown); // 부모(+자식)를 아래로
    let moved = ed.markdown();
    assert_ne!(moved, todo);
    assert!(ed.history_step(true));
    assert_eq!(ed.markdown(), todo);
    assert!(ed.history_step(true));
    assert_eq!(ed.markdown(), original);
    assert!(ed.history_step(false));
    assert!(ed.history_step(false));
    assert_eq!(ed.markdown(), moved);
}

#[test]
fn undo_without_history_reports_and_keeps_document() {
    let mut ed = editor("- x\n");
    assert!(ed.history_step(true));
    assert_eq!(ed.message.as_deref(), Some("되돌릴 편집이 없습니다"));
    assert!(ed.history_step(false));
    assert_eq!(ed.markdown(), "- x\n");
}
