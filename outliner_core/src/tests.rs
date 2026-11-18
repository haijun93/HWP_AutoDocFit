use super::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static TEST_ID: AtomicU64 = AtomicU64::new(0);
fn temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "outliner-{label}-{}-{}.md",
        std::process::id(),
        TEST_ID.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn round_trip_preserves_empty_document() {
    let doc = parse("").unwrap();
    assert_eq!(serialize(&doc), "");
}
#[test]
fn round_trip_preserves_blank_lines() {
    let source = "\n- item\n\nend\n";
    assert_eq!(serialize(&parse(source).unwrap()), source);
}
#[test]
fn round_trip_preserves_mixed_tabs_and_spaces() {
    let source = "\t- tab\n  \t- mixed\n";
    assert_eq!(serialize(&parse(source).unwrap()), source);
}
#[test]
fn round_trip_preserves_unknown_markdown_lines() {
    let source = "# heading\nparagraph\n> quote\n";
    assert_eq!(serialize(&parse(source).unwrap()), source);
}
#[test]
fn round_trip_preserves_korean_text() {
    let source = "- 한글 문장\n";
    assert_eq!(serialize(&parse(source).unwrap()), source);
}
#[test]
fn round_trip_preserves_deep_indentation() {
    let source = "            - deep\n";
    assert_eq!(serialize(&parse(source).unwrap()), source);
}
#[test]
fn round_trip_preserves_collapsed_property_line() {
    let source = "- parent\n  collapsed:: true\n";
    assert_eq!(serialize(&parse(source).unwrap()), source);
}
#[test]
fn round_trip_preserves_crlf_and_trailing_newline() {
    let source = "- one\r\n\r\n- two\r\n";
    assert_eq!(serialize(&parse(source).unwrap()), source);
}
#[test]
fn round_trip_preserves_missing_final_newline() {
    let source = "- final";
    assert_eq!(serialize(&parse(source).unwrap()), source);
}
#[test]
fn edit_normalizes_only_changed_block_after_raw_lines() {
    let mut doc = parse("intro\r\n- old\r\nend\r\n").unwrap();
    edit(&mut doc, "set_text", 1, Some("new")).unwrap();
    assert_eq!(serialize(&doc), "intro\r\n- new\r\nend\r\n");
}

#[test]
fn edit_preserves_mixed_line_endings_on_raw_lines() {
    let mut doc = parse("intro\r\n- old\nunknown\r\n").unwrap();
    edit(&mut doc, "set_text", 1, Some("new")).unwrap();
    assert_eq!(serialize(&doc), "intro\r\n- new\nunknown\r\n");
}
#[test]
fn first_block_cannot_move_up() {
    let mut doc = parse("- a\n- b\n").unwrap();
    assert!(edit(&mut doc, "up", 1, None).is_err());
}
#[test]
fn last_block_cannot_move_down() {
    let mut doc = parse("- a\n- b\n").unwrap();
    assert!(edit(&mut doc, "down", 2, None).is_err());
}
#[test]
fn first_block_cannot_be_indented() {
    let mut doc = parse("- a\n").unwrap();
    assert!(edit(&mut doc, "indent", 1, None).is_err());
}
#[test]
fn top_level_block_cannot_be_outdented() {
    let mut doc = parse("- a\n").unwrap();
    assert!(edit(&mut doc, "outdent", 1, None).is_err());
}
#[test]
fn nested_block_can_be_outdented() {
    let mut doc = parse("- a\n  - b\n").unwrap();
    edit(&mut doc, "outdent", 2, None).unwrap();
    assert_eq!(doc.blocks[1].indent, 1);
}
#[test]
fn insertion_after_last_block_adds_new_block() {
    let mut doc = parse("- a\n").unwrap();
    edit(&mut doc, "insert", 1, Some("b")).unwrap();
    assert_eq!(doc.blocks.len(), 2);
    assert!(serialize(&doc).contains("- b"));
}
#[test]
fn deletion_of_last_block_keeps_remaining_block() {
    let mut doc = parse("- a\n- b\n").unwrap();
    edit(&mut doc, "delete", 2, None).unwrap();
    assert_eq!(doc.blocks.len(), 1);
}
#[test]
fn folded_block_can_move_down() {
    let mut doc = parse("- parent\n  - child\n- next\n").unwrap();
    edit(&mut doc, "fold", 1, None).unwrap();
    edit(&mut doc, "down", 1, None).unwrap();
    assert!(doc.blocks.iter().find(|b| b.id == 1).unwrap().folded);
}
#[test]
fn page_links_are_extracted() {
    assert_eq!(
        parse("- [[페이지]]\n").unwrap().blocks[0].page_links,
        ["페이지"]
    );
}
#[test]
fn block_references_are_extracted() {
    assert_eq!(
        parse("- ((block-7))\n").unwrap().blocks[0].block_refs,
        ["block-7"]
    );
}
#[test]
fn tags_are_extracted() {
    assert_eq!(parse("- work #태그\n").unwrap().blocks[0].tags, ["태그"]);
}
#[test]
fn properties_are_extracted() {
    assert_eq!(
        parse("- item kind:: task\n").unwrap().blocks[0].properties["kind"],
        "task"
    );
}
#[test]
fn todo_marker_is_extracted() {
    assert_eq!(
        parse("- TODO item\n").unwrap().blocks[0].status.as_deref(),
        Some("TODO")
    );
}
#[test]
fn done_marker_is_extracted() {
    assert_eq!(
        parse("- DONE item\n").unwrap().blocks[0].status.as_deref(),
        Some("DONE")
    );
}
#[test]
fn todo_status_toggles_through_done_and_back_to_todo() {
    let mut doc = parse("- task\n").unwrap();
    edit(&mut doc, "toggle_status", 1, None).unwrap();
    assert_eq!(doc.blocks[0].status.as_deref(), Some("TODO"));
    edit(&mut doc, "toggle_status", 1, None).unwrap();
    assert_eq!(doc.blocks[0].status.as_deref(), Some("DONE"));
    edit(&mut doc, "toggle_status", 1, None).unwrap();
    assert_eq!(doc.blocks[0].status.as_deref(), Some("TODO"));
}
#[test]
fn backlink_index_includes_page_and_block_references() {
    let doc = parse("- [[page]] ((42))\n").unwrap();
    assert_eq!(backlinks(&doc, "page"), [1]);
    assert_eq!(backlinks(&doc, "42"), [1]);
}
#[test]
fn index_collects_tag_and_property_values() {
    let index = build_index(&parse("- #tag state:: open\n").unwrap());
    assert_eq!(index.tags["tag"], [1]);
    assert_eq!(index.properties["state"][0].1, "open");
}
#[test]
fn daily_page_uses_iso_filename() {
    assert_eq!(daily_page("2026-11-18").unwrap(), "2026-11-18.md");
}
#[test]
fn daily_page_rejects_invalid_month() {
    assert!(daily_page("2026-13-01").is_err());
}
#[test]
fn daily_page_rejects_invalid_shape() {
    assert!(daily_page("yesterday").is_err());
}
#[test]
fn search_supports_two_scalar_partial_match() {
    let doc = parse("- 가나다라마바사\n").unwrap();
    assert_eq!(search(&doc, "나다").unwrap(), [1]);
}
#[test]
fn search_is_case_insensitive() {
    let doc = parse("- Rust Engine\n").unwrap();
    assert_eq!(search(&doc, "rU").unwrap(), [1]);
}
#[test]
fn search_rejects_one_scalar_query() {
    assert_eq!(
        search(&parse("- item\n").unwrap(), "x"),
        Err(OutlinerError::QueryTooShort)
    );
}
#[test]
fn parser_rejects_document_over_limit() {
    let input = "x".repeat(MAX_DOCUMENT_BYTES + 1);
    assert_eq!(parse(&input), Err(OutlinerError::DocumentTooLarge));
}
#[test]
fn parser_rejects_more_than_maximum_blocks() {
    let input = "- x\n".repeat(MAX_BLOCKS + 1);
    assert_eq!(parse(&input), Err(OutlinerError::TooManyBlocks));
}
#[test]
fn malformed_bullet_like_line_is_preserved_as_raw() {
    let source = "-\n -\ttext\n";
    assert_eq!(serialize(&parse(source).unwrap()), source);
}
#[test]
fn failed_edit_keeps_document_unchanged() {
    let mut doc = parse("- a\n").unwrap();
    let before = doc.clone();
    assert!(edit(&mut doc, "delete", 99, None).is_err());
    assert_eq!(doc, before);
}
#[test]
fn split_and_merge_keep_text_and_metadata_consistent() {
    let mut doc = parse("- alpha beta #tag\n").unwrap();
    edit(&mut doc, "split", 1, Some(" ")).unwrap();
    assert_eq!(doc.blocks.len(), 2);
    edit(&mut doc, "merge", 1, None).unwrap();
    assert_eq!(doc.blocks[0].tags, ["tag"]);
}
#[test]
fn atomic_save_replaces_existing_file() {
    let path = temp_path("replace");
    fs::write(&path, "old").unwrap();
    save_atomic(&path, "new").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    fs::remove_file(path).unwrap();
}
#[test]
fn atomic_save_failure_preserves_original_file() {
    let path = temp_path("failure");
    fs::write(&path, "original").unwrap();
    let result = save_atomic(&path.join("child.md"), "replacement");
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "original");
    fs::remove_file(path).unwrap();
}

// Retained first-stage coverage to prevent weakening the original behaviors.
#[test]
fn round_trip_preserves_empty_korean_deep_indent_and_unknown_lines() {
    for source in [
        "",
        "한글\n- 항목\n    - 깊은 항목\n일반 문단\n",
        "\t- 탭 자식\n  - 공백 혼합\n```\n- 코드\n```\n",
    ] {
        let doc = parse(source).unwrap();
        assert_eq!(serialize(&doc), source);
    }
}
#[test]
fn extracts_links_tags_properties_backlinks_and_search() {
    let doc = parse("- [[페이지]] #태그 kind:: task ((42))\n- unrelated\n").unwrap();
    let block = &doc.blocks[0];
    assert_eq!(block.page_links, ["페이지"]);
    assert_eq!(block.tags, ["태그"]);
    assert!(block.properties.contains_key("kind"));
    assert_eq!(backlinks(&doc, "페이지"), [1]);
    assert_eq!(backlinks(&doc, "42"), [1]);
    assert_eq!(search(&doc, "관계"), Ok(vec![]));
    assert_eq!(search(&doc, "태"), Err(OutlinerError::QueryTooShort));
}
#[test]
fn edit_operations_cover_insert_delete_indent_fold_move_split_merge() {
    let mut doc = parse("- one\n- two\n- three\n").unwrap();
    edit(&mut doc, "insert", 1, Some("new")).unwrap();
    edit(&mut doc, "indent", 2, None).unwrap();
    edit(&mut doc, "fold", 2, None).unwrap();
    assert!(doc.blocks.iter().find(|b| b.id == 2).unwrap().folded);
    edit(&mut doc, "up", 3, None).unwrap();
    let split_id = doc.blocks.iter().find(|b| b.text == "one").unwrap().id;
    edit(&mut doc, "split", split_id, Some("n")).unwrap();
    edit(&mut doc, "merge", split_id, None).unwrap();
    let tail = doc.blocks.last().unwrap().id;
    edit(&mut doc, "delete", tail, None).unwrap();
    assert!(!doc.blocks.is_empty());
    assert!(doc.edited);
}
#[test]
fn rejects_limits_and_malformed_operations() {
    let too_big = "x".repeat(MAX_DOCUMENT_BYTES + 1);
    assert_eq!(parse(&too_big), Err(OutlinerError::DocumentTooLarge));
    let doc = parse("- x\n").unwrap();
    assert_eq!(search(&doc, "x"), Err(OutlinerError::QueryTooShort));
    assert!(daily_page("yesterday").is_err());
    let mut doc = parse("- x\n").unwrap();
    assert!(edit(&mut doc, "delete", 999, None).is_err());
}
