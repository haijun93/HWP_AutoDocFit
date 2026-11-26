use outliner_core::{DropPlace::*, move_subtree, parse, serialize, UndoHistory};

fn moved(text: &str, from: usize, to: usize, place: outliner_core::DropPlace) -> String {
    let mut doc = parse(text).unwrap();
    let (a, b) = (doc.blocks[from].id, doc.blocks[to].id);
    assert!(move_subtree(&mut doc, a, b, place).unwrap());
    let result = serialize(&doc);
    assert_eq!(serialize(&parse(&result).unwrap()), result);
    result
}

#[test]
fn before_and_after_take_the_entire_source_and_target_subtrees() {
    let text = "- 가\n  - 가의 자식\n- 나\n  - 나의 자식\n- 다\n";
    let after = "- 나\n  - 나의 자식\n- 가\n  - 가의 자식\n- 다\n";
    assert_eq!(moved(text, 0, 2, After), after);
    assert_eq!(moved(text, 2, 0, Before), after);
}

#[test]
fn child_uses_existing_indent_and_preserves_relative_depth_status_and_properties() {
    let text = "- TODO 가\r\n  id:: abc\r\n  - 자식\r\n- 나\r\n    - 기존 자식\r\n";
    let mut doc = parse(text).unwrap();
    doc.blocks[0].folded = true;
    doc.blocks[2].folded = true;
    let id = doc.blocks[0].id;
    let target = doc.blocks[2].id;
    assert!(move_subtree(&mut doc, id, target, Child).unwrap());
    assert!(!doc.blocks[0].folded);
    let i = doc.blocks.iter().position(|b| b.id == id).unwrap();
    assert_eq!((doc.blocks[i].indent, doc.blocks[i + 1].indent), (4, 6));
    assert!(doc.blocks[i].folded);
    assert_eq!(doc.blocks[i].status.as_deref(), Some("TODO"));
    assert_eq!(doc.blocks[i].properties.get("id").map(String::as_str), Some("abc"));
    let output = serialize(&doc);
    assert!(!output.replace("\r\n", "").contains('\n'));
    assert_eq!(output.matches("id:: abc").count(), 1);
}

#[test]
fn after_an_ancestor_outdents_at_the_same_sequence_boundary() {
    assert_eq!(moved("- 부모\n  - 자식\n    - 손자\n- 끝", 1, 0, After), "- 부모\n- 자식\n  - 손자\n- 끝");
}

#[test]
fn child_of_leaf_uses_one_space_and_can_move_from_both_sides() {
    assert_eq!(moved("- 가\n  - 자식\n- 나\n", 0, 2, Child), "- 나\n - 가\n   - 자식\n");
    assert_eq!(moved("- 가\n- 나\n", 1, 0, Child), "- 가\n - 나\n");
}

#[test]
fn self_descendant_unknown_target_and_syntax_boundary_are_atomic() {
    for (text, from, to) in [
        ("- 가\n  - 자식\n- 나\n", 0, 0),
        ("- 가\n  - 자식\n- 나\n", 0, 1),
        ("- 가\n# 보존 제목\n- 나\n", 0, 1),
    ] {
        for place in [Before, After, Child] {
            let mut doc = parse(text).unwrap();
            let before = doc.clone();
            let (a, b) = (doc.blocks[from].id, doc.blocks[to].id);
            assert!(move_subtree(&mut doc, a, b, place).is_err());
            assert_eq!(doc, before);
        }
    }
    let mut doc = parse("- 가\n- 나\n").unwrap();
    let before = doc.clone();
    assert!(move_subtree(&mut doc, 1, 999, Before).is_err());
    assert_eq!(doc, before);
}

#[test]
fn adjacent_drop_is_a_noop_and_does_not_mark_edited() {
    for (from, to, place) in [(1, 0, After), (0, 1, Before)] {
        let mut doc = parse("- 가\n- 나\n").unwrap();
        let before = doc.clone();
        let (a, b) = (doc.blocks[from].id, doc.blocks[to].id);
        assert!(!move_subtree(&mut doc, a, b, place).unwrap());
        assert_eq!(doc, before);
    }
}

#[test]
fn missing_final_newline_mixed_endings_and_raw_blanks_remain_valid() {
    let text = "# 머리\r\n- 가\r\n  - 자식\n\r\n- 나";
    let result = moved(text, 0, 2, After);
    assert!(result.starts_with("# 머리\r\n"));
    assert!(!result.ends_with('\n'));
    assert!(result.contains("\r\n\r\n"));
    assert_eq!(parse(&result).unwrap().blocks.len(), 3);
}

#[test]
fn drop_is_one_history_snapshot_and_restores_exact_original() {
    let text = "- TODO 가\r\n  id:: a\r\n  - 손자\r\n- 나";
    let after = moved(text, 0, 2, Child);
    let mut history = UndoHistory::default();
    history.record(text);
    assert_eq!(history.undo(&after).as_deref(), Some(text));
    assert!(!history.can_undo());
    assert_eq!(history.redo(text).as_deref(), Some(after.as_str()));
}
