//! docs/OUTLINEKEY1_DESIGN_2026-11-26.md 동작 표의 번호별 시험. 합성 마크다운만 쓴다.
use outliner_core::{
    apply_key, handle_request, pair_backspace, parse, serialize, type_char, Cursor, KeyInput,
    KeyOutcome, UndoHistory,
};
use serde_json::{json, Value};

fn input(action: &str, block: usize, caret: usize) -> KeyInput {
    KeyInput {
        action: action.into(),
        block: Some(block),
        start: caret,
        end: caret,
        ..KeyInput::default()
    }
}

fn run(md: &str, input: KeyInput) -> KeyOutcome {
    let out = apply_key(&parse(md).unwrap(), &input).unwrap();
    // 모든 결과는 다시 읽어 써도 같아야 한다(파일 왕복).
    assert_eq!(serialize(&parse(&out.markdown).unwrap()), out.markdown);
    out
}

fn key(md: &str, action: &str, block: usize, caret: usize) -> KeyOutcome {
    run(md, input(action, block, caret))
}

fn at(block: usize, offset: usize) -> Option<Cursor> {
    Some(Cursor {
        block,
        start: offset,
        end: offset,
    })
}

fn select(action: &str, selection: &[usize]) -> KeyInput {
    KeyInput {
        action: action.into(),
        selection: selection.to_vec(),
        ..KeyInput::default()
    }
}

// ---- 1. 빈 블록 Enter
#[test]
fn r01_empty_last_child_enter_outdents_one_level() {
    let out = key("- a\n - b\n  - \n", "enter", 2, 0);
    assert_eq!(out.markdown, "- a\n - b\n - \n");
    assert_eq!(out.cursor, at(2, 0));
    let again = key(&out.markdown, "enter", 2, 0);
    assert_eq!(again.markdown, "- a\n - b\n- \n", "한 번 더 → 최상위");
    let top = key(&again.markdown, "enter", 2, 0);
    assert_eq!(top.markdown, "- a\n - b\n- \n- \n", "최상위 빈 블록 = 새 블록");
    assert_eq!(top.cursor, at(3, 0));
}

#[test]
fn r01_empty_block_with_next_sibling_also_outdents() {
    // 사용자 지시: 항상 내어쓰기. 빈 블록은 부모의 하위 전체 뒤로 가고, 뒤 형제는 원래 부모에 남는다.
    let out = key("- a\n - \n - c\n", "enter", 1, 0);
    assert_eq!(out.markdown, "- a\n - c\n- \n");
    assert_eq!(out.cursor, at(2, 0));
}

// ---- 2·3. 자식 있는 블록 / 접힌 블록 끝 Enter
#[test]
fn r02_enter_at_end_of_open_parent_adds_first_child() {
    let out = key("- p\n  - c\n- n\n", "enter", 0, 1);
    assert_eq!(out.markdown, "- p\n  - \n  - c\n- n\n");
    assert_eq!(out.cursor, at(1, 0));
}

#[test]
fn r03_enter_at_end_of_folded_parent_adds_sibling_after_subtree() {
    let mut i = input("enter", 0, 1);
    i.folded = vec![0];
    let out = run("- p\n - c\n- n\n", i);
    assert_eq!(out.markdown, "- p\n - c\n- \n- n\n");
    assert_eq!(out.cursor, at(2, 0));
    assert_eq!(out.folded, [0], "접힘 유지");
}

// ---- 4. 중간 Enter
#[test]
fn r04_enter_in_middle_splits_and_keeps_children_status_and_id() {
    let out = key("- TODO 가나 다라\n  id:: u1\n - 자식\n", "enter", 0, 2);
    assert_eq!(out.markdown, "- TODO 가나\n  id:: u1\n - 다라\n - 자식\n");
    assert_eq!(out.document.blocks[0].properties["id"], "u1");
    assert!(out.document.blocks[1].status.is_none());
    let leaf = key("- abcd\n", "enter", 0, 2);
    assert_eq!(leaf.markdown, "- ab\n- cd\n");
    assert_eq!(leaf.cursor, at(1, 0));
}

#[test]
fn r04_enter_deletes_selected_text_and_handles_surrogates() {
    let mut i = input("enter", 0, 1);
    i.end = 3;
    assert_eq!(run("- a😀b\n", i).markdown, "- a\n- b\n");
}

// ---- 5. 맨 앞 Enter
#[test]
fn r05_enter_at_start_puts_empty_block_above() {
    let out = key("- TODO ab\n - c\n", "enter", 0, 0);
    assert_eq!(out.markdown, "- \n- TODO ab\n - c\n");
    assert_eq!(out.cursor, at(0, 0));
}

// ---- 6. 링크 안 Enter
#[test]
fn r06_enter_inside_reference_moves_past_it_without_split() {
    for md in ["- 앞 [[계획]] 뒤\n", "- 앞 ((계획)) 뒤\n"] {
        let out = key(md, "enter", 0, 5);
        assert!(!out.changed);
        assert_eq!(out.cursor, at(0, 8));
    }
}

// ---- 7. Shift+Enter 와 파일 형식
#[test]
fn r07_newline_stays_in_block_and_round_trips() {
    let out = key("- 첫줄둘째\n - c\n", "newline", 0, 2);
    assert_eq!(out.markdown, "- 첫줄\n  둘째\n - c\n");
    assert_eq!(out.document.blocks.len(), 2);
    assert_eq!(out.document.blocks[0].text, "첫줄\n둘째");
    assert_eq!(out.cursor, at(0, 3));
}

#[test]
fn r07_continuation_format_is_byte_exact_and_keeps_crlf() {
    for source in [
        "- a\r\n  b\r\n  \r\n  c\r\n- d\r\n",
        "- a\n  collapsed:: true\n  본문\n  id:: x\n",
        "\t- tab\n\t  이어짐\n",
        "- a\n  \\- 불릿 아님\n  ```\n  code\n  ```\n",
        "- a\n 한 칸은 보존 줄\n# 제목\n  이것도 보존\n",
    ] {
        let doc = parse(source).unwrap();
        assert_eq!(serialize(&doc), source);
        // 다른 블록을 고친 뒤에도 같은 모양이어야 한다(탭 들여쓰기 정규화 제외).
        if !source.starts_with('\t') {
            let mut i = input("commit", 0, 0);
            i.text = Some(format!("{}!", doc.blocks[0].text));
            let edited = run(source, i);
            assert_eq!(
                edited.markdown.matches('\n').count(),
                source.matches('\n').count()
            );
            let mut back = input("commit", 0, 0);
            back.text = Some(doc.blocks[0].text.clone());
            assert_eq!(run(&edited.markdown, back).markdown, source);
        }
    }
}

#[test]
fn r07_dash_line_typed_inside_block_does_not_become_a_child() {
    let mut i = input("commit", 0, 0);
    i.text = Some("목록\n- 항목\n  - 더".into());
    let out = run("- x\n", i);
    assert_eq!(out.markdown, "- 목록\n  \\- 항목\n    \\- 더\n");
    assert_eq!(out.document.blocks.len(), 1);
    assert_eq!(out.document.blocks[0].text, "목록\n- 항목\n  - 더");
    assert!(parse("- std::vector 사용\n  a::b\n").unwrap().blocks[0].extra.is_empty());
}

// ---- 8. 맨 앞 Backspace
#[test]
fn r08_backspace_merges_without_space_and_places_caret_at_seam() {
    let out = key("- 앞\n- 뒤\n- 끝\n", "backspace", 1, 0);
    assert_eq!(out.markdown, "- 앞뒤\n- 끝\n");
    assert_eq!(out.cursor, at(0, 1));
}

#[test]
fn r08_backspace_moves_children_to_previous_sibling() {
    let out = key("- a\n- b\n - c\n", "backspace", 1, 0);
    assert_eq!(out.markdown, "- ab\n - c\n");
    let empty = key("- a\n- \n - c\n", "backspace", 1, 0);
    assert_eq!(empty.markdown, "- a\n - c\n", "빈 블록이어도 자식은 남는다");
}

#[test]
fn r08_backspace_refuses_when_both_sides_have_children() {
    for (md, block) in [
        ("- a\n - x\n- \n - c\n", 2), // 앞 형제에 자식
        ("- p\n - \n  - c\n", 1),     // 첫 자식(왼쪽 = 부모)
    ] {
        let out = key(md, "backspace", block, 0);
        assert!(!out.changed, "{md}");
        assert!(out.note.is_some());
    }
}

#[test]
fn r08_backspace_goes_to_previous_visible_block() {
    let mut i = input("backspace", 2, 0);
    i.folded = vec![0];
    let out = run("- a\n - hidden\n- b\n", i);
    assert_eq!(out.markdown, "- ab\n - hidden\n");
    assert_eq!(out.cursor, at(0, 1));
    let deep = key("- a\n - deep\n- b\n", "backspace", 2, 0);
    assert_eq!(deep.markdown, "- a\n - deepb\n");
}

#[test]
fn r08_first_and_only_block_rules_and_raw_lines() {
    assert!(!key("- a\n- b\n", "backspace", 0, 0).changed);
    assert!(!key("- \n", "backspace", 0, 0).changed);
    assert_eq!(key("- \n- b\n", "backspace", 0, 0).markdown, "- b\n");
    assert!(!key("- a\n# 제목\n- b\n", "backspace", 1, 0).changed);
    assert_eq!(key("- a\n\n- b\n", "backspace", 1, 0).markdown, "- ab\n\n");
    assert!(!key("- ab\n", "backspace", 0, 1).handled);
}

#[test]
fn r08_merged_away_block_id_is_kept() {
    let out = key("- a\n- b\n  id:: keep-me\n", "backspace", 1, 0);
    assert_eq!(out.markdown, "- ab\n  id:: keep-me\n");
    assert_eq!(out.document.blocks[0].properties["id"], "keep-me");
}

// ---- 9. 끝 Delete
#[test]
fn r09_delete_at_end_merges_next() {
    let out = key("- a\n- b\n - c\n", "delete", 0, 1);
    assert_eq!(out.markdown, "- ab\n - c\n");
    assert_eq!(out.cursor, at(0, 1));
    assert_eq!(key("- p\n - c\n- n\n", "delete", 0, 1).markdown, "- pc\n- n\n");
}

#[test]
fn r09_delete_limits() {
    assert!(!key("- p\n - c\n  - g\n", "delete", 0, 1).changed);
    assert!(!key("- p\n - last\n- n\n", "delete", 1, 4).changed, "다음 형제 없음");
    let mut i = input("delete", 0, 1);
    i.folded = vec![0];
    assert!(!run("- p\n - c\n- n\n - m\n", i.clone()).changed);
    assert_eq!(run("- p\n - c\n- n\n", i).markdown, "- pn\n - c\n");
    assert!(!key("- ab\n- c\n", "delete", 0, 1).handled);
}

// ---- 10. Tab
#[test]
fn r10_tab_indents_subtree_and_keeps_caret() {
    let out = key("- a\n- bb\n - c\n- d\n", "indent", 1, 1);
    assert_eq!(out.markdown, "- a\n - bb\n  - c\n- d\n");
    assert_eq!(out.cursor, at(1, 1));
    assert!(!key("- a\n- b\n", "indent", 0, 0).changed);
    assert!(!key("- p\n - c\n", "indent", 1, 0).changed);
}

#[test]
fn r10_tab_unfolds_new_parent_and_matches_sibling_width() {
    let mut i = input("indent", 2, 0);
    i.folded = vec![0];
    let out = run("- a\n    - x\n- b\n", i);
    assert_eq!(out.markdown, "- a\n    - x\n    - b\n");
    assert!(out.folded.is_empty());
}

// ---- 11. Shift+Tab (논리적)
#[test]
fn r11_logical_outdent_moves_after_parent_subtree() {
    let out = key("- p\n - a\n  - a1\n - b\n- n\n", "outdent", 1, 1);
    assert_eq!(out.markdown, "- p\n - b\n- a\n - a1\n- n\n");
    assert_eq!(out.cursor, at(2, 1));
    assert!(!key("- a\n", "outdent", 0, 0).changed);
}

// ---- 12·13. 방향키
#[test]
fn r12_up_down_move_between_blocks_keeping_column() {
    let md = "- abcd\n- ef\n  ghij\n- k\n";
    assert_eq!(key(md, "down", 0, 3).cursor, at(1, 2), "짧으면 줄 끝");
    assert_eq!(key(md, "up", 2, 1).cursor, at(1, 4), "앞 블록의 마지막 줄 같은 열");
    assert_eq!(key(md, "down", 1, 5).cursor, at(2, 1));
    assert_eq!(key(md, "up", 0, 3).cursor, at(0, 0));
    assert_eq!(key(md, "down", 2, 0).cursor, at(2, 1));
    let mut i = input("down", 0, 0);
    i.folded = vec![0];
    assert_eq!(run("- a\n - h\n- b\n", i).cursor, at(2, 0), "숨은 블록 건너뜀");
}

#[test]
fn r13_left_right_cross_block_boundaries() {
    assert_eq!(key("- ab\n- cd\n", "left", 1, 0).cursor, at(0, 2));
    assert_eq!(key("- ab\n- cd\n", "right", 0, 2).cursor, at(1, 0));
    assert!(!key("- ab\n", "left", 0, 1).handled);
}

// ---- 14. 블록 이동
#[test]
fn r14_move_swaps_siblings_with_their_subtrees() {
    let md = "- a\n - a1\n- b\n - b1\n";
    let up = key(md, "move_up", 2, 1);
    assert_eq!(up.markdown, "- b\n - b1\n- a\n - a1\n");
    assert_eq!(up.cursor, at(0, 1));
    assert_eq!(key(&up.markdown, "move_down", 0, 1).markdown, md);
    assert_eq!(
        key("- a\n\n- b\n", "move_up", 1, 0).markdown,
        "- b\n\n- a\n",
        "보존 줄은 제자리"
    );
    assert!(!key(md, "move_up", 0, 0).changed);
    assert_eq!(key("- a\n- b", "move_up", 1, 0).markdown, "- b\n- a");
}

#[test]
fn r14_move_crosses_into_the_neighbouring_parent() {
    let md = "- A\n - a\n- B\n - b\n";
    assert_eq!(key(md, "move_up", 3, 0).markdown, "- A\n - a\n - b\n- B\n");
    assert_eq!(key(md, "move_down", 1, 0).markdown, "- A\n- B\n - a\n - b\n");
}

// ---- 15. 접기
#[test]
fn r15_collapse_expand_toggle_only_with_children() {
    let out = key("- p\n - c\n", "collapse", 0, 0);
    assert_eq!(out.folded, [0]);
    assert!(!out.changed, "접힘은 파일에 쓰지 않는다");
    let mut i = input("toggle_collapse", 0, 0);
    i.folded = vec![0];
    assert!(run("- p\n - c\n", i).folded.is_empty());
    assert!(key("- leaf\n", "collapse", 0, 0).folded.is_empty());
}

// ---- 16. TODO 순환
#[test]
fn r16_ctrl_enter_cycles_none_todo_done_none() {
    let a = key("- 일\n", "cycle_todo", 0, 1);
    assert_eq!(a.markdown, "- TODO 일\n");
    assert_eq!(a.cursor, at(0, 1));
    let b = key(&a.markdown, "cycle_todo", 0, 1);
    assert_eq!(b.markdown, "- DONE 일\n");
    assert_eq!(key(&b.markdown, "cycle_todo", 0, 1).markdown, "- 일\n");
    let many = run("- a\n- \n- b\n", select("cycle_todo", &[0, 1, 2]));
    assert_eq!(many.markdown, "- TODO a\n- \n- TODO b\n");
}

// ---- 19. 블록 선택 일괄
#[test]
fn r19_delete_selection_takes_children() {
    let out = run("- a\n- b\n - c\n- d\n", select("delete_selection", &[1]));
    assert_eq!(out.markdown, "- a\n- d\n");
    assert_eq!(out.cursor, at(0, 1));
    assert!(out.selection.is_empty());
    assert_eq!(run("- a\n- b\n", select("delete_selection", &[0, 1])).markdown, "- \n");
}

#[test]
fn r19_selection_indent_outdent_move_keep_hierarchy() {
    let md = "- a\n- b\n - c\n- d\n";
    let indented = run(md, select("indent", &[1, 2, 3]));
    assert_eq!(indented.markdown, "- a\n - b\n  - c\n - d\n");
    assert_eq!(indented.selection, [1, 2, 3]);
    assert_eq!(run(&indented.markdown, select("outdent", &[1, 2, 3])).markdown, md);
    assert_eq!(
        run(md, select("move_up", &[1, 3])).markdown,
        "- b\n - c\n- d\n- a\n"
    );
}

#[test]
fn r19_copy_and_cut_are_relative_markdown_without_ids() {
    let md = "- a\n - TODO b\n   더\n   id:: x\n  - c\n";
    let copied = run(md, select("copy", &[1]));
    assert_eq!(copied.clipboard.as_deref(), Some("- TODO b\n  더\n - c"));
    assert!(!copied.changed);
    let cut = run(md, select("cut", &[1]));
    assert_eq!(cut.markdown, "- a\n");
    assert_eq!(cut.clipboard, copied.clipboard);
}

// ---- 20. 여러 줄 붙여넣기
fn paste(md: &str, block: usize, caret: usize, text: &str) -> KeyOutcome {
    let mut i = input("paste", block, caret);
    i.paste = Some(text.into());
    run(md, i)
}

#[test]
fn r20_bullet_list_becomes_a_tree() {
    let out = paste("- 앞\n- 뒤\n", 0, 1, "- 하나\r\n    - TODO 둘\r\n      이어짐\r\n- 셋\r\n");
    assert_eq!(
        out.markdown,
        "- 앞\n- 하나\n - TODO 둘\n   이어짐\n- 셋\n- 뒤\n"
    );
    assert_eq!(out.cursor, at(3, 1));
}

#[test]
fn r20_plain_lines_become_blocks_and_replace_empty_block() {
    let out = paste("- \n", 0, 0, "제목\n  항목 1\n\n  항목 2\n");
    assert_eq!(out.markdown, "- 제목\n - 항목 1\n - 항목 2\n");
}

#[test]
fn r20_paste_into_open_parent_goes_to_children() {
    let out = paste("- p\n - c\n", 0, 1, "x\ny");
    assert_eq!(out.markdown, "- p\n - x\n - y\n - c\n");
    let mut sel = select("paste", &[0]);
    sel.paste = Some("- s".into());
    assert_eq!(run("- p\n - c\n", sel).markdown, "- p\n - c\n- s\n");
}

// ---- 21. 괄호 짝 (Flutter 와 함께 쓰는 시험 벡터)
#[test]
fn r21_typing_vectors() {
    let vectors: Value =
        serde_json::from_str(include_str!("../testdata/typing_rules.json")).unwrap();
    let n = |v: &Value| v.as_u64().unwrap() as usize;
    for v in vectors["type"].as_array().unwrap() {
        let ch = v["char"].as_str().unwrap().chars().next().unwrap();
        let got = type_char(v["text"].as_str().unwrap(), n(&v["start"]), n(&v["end"]), ch);
        let got = got.map_or(Value::Null, |(t, s, e)| json!([t, s, e]));
        assert_eq!(got, v["expect"], "{v}");
    }
    for v in vectors["backspace"].as_array().unwrap() {
        let got = pair_backspace(v["text"].as_str().unwrap(), n(&v["caret"]));
        let got = got.map_or(Value::Null, |(t, c)| json!([t, c]));
        assert_eq!(got, v["expect"], "{v}");
    }
}

// ---- 24. 실행 취소 단위 / 25. 조합 확정 글
#[test]
fn r24_one_command_with_typed_text_is_one_undo_step() {
    let before = "- 가\n";
    let mut i = input("enter", 0, 3);
    i.text = Some("가나다".into());
    let out = run(before, i);
    assert_eq!(out.markdown, "- 가나다\n- \n");
    let mut history = UndoHistory::default();
    history.record(before);
    assert_eq!(history.undo(&out.markdown).as_deref(), Some(before));
}

#[test]
fn engine_key_op_matches_library() {
    let response = handle_request(json!({"id": 1, "op": "key", "document": "- a\n - \n",
        "args": {"action": "enter", "block": 1, "start": 0, "end": 0, "page": "x"}}));
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["result"]["markdown"], "- a\n- \n");
    assert_eq!(response["result"]["cursor"], json!({"block": 1, "start": 0, "end": 0}));
    let bad = handle_request(json!({"id": 2, "op": "key", "document": "- a\n",
        "args": {"action": "enter", "block": 9}}));
    assert_eq!(bad["ok"], false);
}
