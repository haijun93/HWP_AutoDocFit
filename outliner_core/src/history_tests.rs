use super::*;
use serde_json::{json, Value};

/// 엔진 edit 연산 하나를 적용해 새 마크다운을 돌려준다(이력에는 아직 넣지 않는다).
fn apply(markdown: &str, action: &str, block_id: u64, text: Option<&str>) -> String {
    let mut doc = parse(markdown).unwrap();
    edit(&mut doc, action, block_id, text).unwrap();
    serialize(&doc)
}

fn call(request: Value) -> Value {
    handle_request(request)
}

#[test]
fn undo_then_redo_n_steps_round_trips_exactly() {
    let mut text = "- 가나다\n- 라마바\n".to_string();
    let mut history = UndoHistory::default();
    let mut states = vec![text.clone()];
    let steps: [(&str, u64, Option<&str>); 6] = [
        ("set_text", 1, Some("안녕하세요 [[페이지]]")),
        ("insert", 1, Some("새 블록")),
        ("indent", 2, None),
        ("toggle_status", 1, None),
        ("down", 1, None),
        ("fold", 1, None),
    ];
    for (action, id, value) in steps {
        let next = apply(&text, action, id, value);
        history.record(&text);
        text = next;
        states.push(text.clone());
    }
    // 끝까지 되돌린다: 각 단계가 직전 상태와 바이트 단위로 같다.
    for expected in states.iter().rev().skip(1) {
        text = history.undo(&text).unwrap();
        assert_eq!(&text, expected);
    }
    assert!(!history.can_undo());
    assert!(history.undo(&text).is_none());
    // 다시 끝까지 실행한다.
    for expected in states.iter().skip(1) {
        text = history.redo(&text).unwrap();
        assert_eq!(&text, expected);
    }
    assert!(!history.can_redo());
    assert_eq!(history.undo_len(), 6);
}

#[test]
fn korean_text_and_grapheme_edits_survive_undo() {
    let mut history = UndoHistory::default();
    let base = "- 한글 문서입니다 🇰🇷\n".to_string();
    let edited = apply(&base, "set_text", 1, Some("닭 앎 읽다 각 👨‍👩‍👧"));
    history.record(&base);
    assert_eq!(history.undo(&edited).unwrap(), base);
    assert_eq!(history.redo(&base).unwrap(), edited);
    assert!(edited.contains("👨‍👩‍👧"));
}

#[test]
fn a_new_edit_clears_the_redo_stack() {
    let mut history = UndoHistory::default();
    history.record("a");
    history.record("b");
    let back = history.undo("c").unwrap();
    assert_eq!(back, "b");
    assert!(history.can_redo());
    history.record("x");
    assert!(!history.can_redo());
    assert_eq!(history.redo_len(), 0);
}

#[test]
fn a_grouped_edit_is_one_step() {
    // Enter = set_text + insert 를 한 번의 record 로 묶으면 한 번 되돌림으로 둘 다 사라진다.
    let base = "- 첫째\n".to_string();
    let mut history = UndoHistory::default();
    let step1 = apply(&base, "set_text", 1, Some("첫째 수정"));
    let step2 = apply(&step1, "insert", 1, Some(""));
    history.record(&base);
    assert_eq!(history.undo_len(), 1);
    assert_eq!(history.undo(&step2).unwrap(), base);
    assert!(!history.can_undo());
}

#[test]
fn depth_limit_drops_oldest_steps() {
    let mut history = UndoHistory::with_limits(3, usize::MAX);
    for n in 0..10 {
        history.record(&format!("- {n}\n"));
    }
    assert_eq!(history.undo_len(), 3);
    assert_eq!(history.undo("now").unwrap(), "- 9\n");
    assert_eq!(history.undo("x").unwrap(), "- 8\n");
    assert_eq!(history.undo("x").unwrap(), "- 7\n");
    assert!(history.undo("x").is_none());
    assert_eq!(MAX_UNDO_DEPTH, 200);
    let mut default = UndoHistory::default();
    for n in 0..250 {
        default.record(&format!("- {n}\n"));
    }
    assert_eq!(default.undo_len(), 200);
}

#[test]
fn byte_limit_drops_oldest_and_refuses_oversized_snapshot() {
    let mut history = UndoHistory::with_limits(200, 100);
    for _ in 0..5 {
        history.record(&"가".repeat(10)); // 30 바이트씩
    }
    assert!(history.bytes() <= 100);
    assert_eq!(history.undo_len(), 3);
    // 한도를 넘는 큰 스냅숏은 기록하지 않고 이력을 비운다.
    assert!(!history.record(&"x".repeat(101)));
    assert!(!history.can_undo());
    // redo 쪽까지 합쳐 한도를 지킨다.
    let mut history = UndoHistory::with_limits(200, 100);
    history.record(&"a".repeat(60));
    let _ = history.undo(&"b".repeat(60));
    assert!(history.bytes() <= 100);
}

#[test]
fn page_switch_or_external_change_clears_history() {
    let mut history = UndoHistory::default();
    assert!(!history.bind(Some("A"), Some("v1")));
    history.record("- x\n");
    assert!(!history.bind(Some("A"), Some("v1")));
    assert!(history.can_undo());
    // 자기 저장 뒤 버전 갱신은 비우지 않는다.
    history.rebase(Some("v2"));
    assert!(!history.bind(Some("A"), Some("v2")));
    assert!(history.can_undo());
    // 외부 변경(버전 불일치)이면 비운다.
    assert!(history.bind(Some("A"), Some("v9")));
    assert!(!history.can_undo() && !history.can_redo());
    history.record("- y\n");
    // 다른 페이지면 비운다.
    assert!(history.bind(Some("B"), Some("v9")));
    assert!(!history.can_undo());
}

#[test]
fn api_history_push_undo_redo_and_clear() {
    let before = "- 하나\n";
    let after = "- 하나\n- 둘\n";
    let pushed = call(json!({"id":"p","op":"history","document":after,
        "args":{"action":"push","before":before,"page":"P","version":"v1"}}));
    assert_eq!(pushed["ok"], true, "{pushed}");
    assert_eq!(pushed["id"], "p");
    assert_eq!(pushed["result"]["can_undo"], true);
    assert_eq!(pushed["result"]["can_redo"], false);
    let state = pushed["result"]["history"].clone();

    let undone = call(json!({"op":"history","document":after,
        "args":{"action":"undo","history":state,"page":"P","version":"v1"}}));
    assert_eq!(undone["result"]["markdown"], before);
    assert_eq!(undone["result"]["changed"], true);
    assert_eq!(
        undone["result"]["document"]["blocks"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(undone["result"]["can_redo"], true);
    let state = undone["result"]["history"].clone();

    let redone = call(json!({"op":"history","document":before,
        "args":{"action":"redo","history":state,"page":"P","version":"v1"}}));
    assert_eq!(redone["result"]["markdown"], after);
    assert_eq!(
        redone["result"]["document"]["blocks"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // 되돌릴 것이 없으면 오류가 아니라 changed=false.
    let empty = call(json!({"op":"history","document":after,"args":{"action":"undo"}}));
    assert_eq!(empty["ok"], true);
    assert_eq!(empty["result"]["changed"], false);
    assert_eq!(empty["result"]["markdown"], after);

    // 외부 변경(버전 불일치)은 이력을 비운다.
    let state = redone["result"]["history"].clone();
    let stale = call(json!({"op":"history","document":after,
        "args":{"action":"undo","history":state,"page":"P","version":"v2"}}));
    assert_eq!(stale["result"]["cleared"], true);
    assert_eq!(stale["result"]["changed"], false);
    assert_eq!(stale["result"]["can_undo"], false);

    let bad = call(json!({"op":"history","document":"","args":{"action":"push"}}));
    assert_eq!(bad["ok"], false);
    let unknown = call(json!({"op":"history","document":"","args":{"action":"zap"}}));
    assert_eq!(unknown["error"]["code"], "INVALID_OPERATION");
}

#[test]
fn edit_request_without_history_is_unchanged_and_with_history_records() {
    let doc = "- 가\n";
    let plain = call(
        json!({"op":"edit","document":doc,"args":{"action":"set_text","block_id":1,"text":"나"}}),
    );
    assert_eq!(plain["ok"], true);
    assert!(plain["result"].get("history").is_none());
    assert_eq!(plain["result"]["markdown"], "- 나\n");

    let tracked = call(json!({"op":"edit","document":doc,
        "args":{"action":"set_text","block_id":1,"text":"나","history":null}}));
    assert!(tracked["result"].get("history").is_none());

    let tracked = call(json!({"op":"edit","document":doc,
        "args":{"action":"set_text","block_id":1,"text":"나","history":{"undo":[],"redo":[]}}}));
    assert_eq!(tracked["result"]["history"]["undo"], json!(["- 가\n"]));
    let again = call(json!({"op":"history","document":"- 나\n",
        "args":{"action":"undo","history":tracked["result"]["history"]}}));
    assert_eq!(again["result"]["markdown"], "- 가\n");
}

#[test]
fn multi_block_edit_is_atomic_and_one_history_step() {
    let before = "- a\n- b\n- c\n";
    let moved = call(json!({"op":"edit","document":before,
        "args":{"action":"down","block_ids":[1,2],"history":{"undo":[],"redo":[]}}}));
    assert_eq!(moved["ok"], true, "{moved}");
    assert_eq!(moved["result"]["markdown"], "- c\n- a\n- b\n");
    assert_eq!(moved["result"]["history"]["undo"], json!([before]));
    let undone = call(
        json!({"op":"history","document":moved["result"]["markdown"],
        "args":{"action":"undo","history":moved["result"]["history"]}}),
    );
    assert_eq!(undone["result"]["markdown"], before);

    let indented = call(json!({"op":"edit","document":before,
        "args":{"action":"indent","block_ids":[2,3]}}));
    assert_eq!(indented["result"]["markdown"], "- a\n - b\n - c\n");
    let deleted = call(json!({"op":"edit","document":before,
        "args":{"action":"delete","block_ids":[1,3]}}));
    assert_eq!(deleted["result"]["markdown"], "- b\n");
    let pasted = call(json!({"op":"edit","document":before,
        "args":{"action":"paste","block_ids":[1],"text":"- pasted\n  - child\n"}}));
    assert_eq!(pasted["ok"], true, "{pasted}");
    assert_eq!(pasted["result"]["document"]["blocks"][1]["text"], "pasted");
    assert_eq!(pasted["result"]["document"]["blocks"][2]["indent"], 2);
    let atomic = call(json!({"op":"edit","document":before,
        "args":{"action":"indent","block_ids":[1,2]}}));
    assert_eq!(atomic["ok"], false);
}
