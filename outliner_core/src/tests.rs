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

#[test]
fn resolve_block_ref_finds_block_by_id_property_and_returns_backlinks() {
    let source = "- 타깃 블록 id:: 6a8274be-9759-4d37-8894-399a0937a008\n- 참조자 블록 1 ((6a8274be-9759-4d37-8894-399a0937a008))\n- 무관한 블록\n- 참조자 블록 2 ((6a8274be-9759-4d37-8894-399a0937a008))\n";
    let doc = parse(source).unwrap();
    let res = resolve_block_ref(&doc, "6a8274be-9759-4d37-8894-399a0937a008");
    assert!(res.found);
    assert_eq!(res.block_id, Some(1));
    assert!(res.text.contains("타깃 블록"));
    assert_eq!(res.backlinks, vec![2, 4]);
}

#[test]
fn resolve_block_ref_with_missing_id_returns_empty_result_instead_of_error() {
    let source = "- 일반 블록 1\n- 일반 블록 2\n";
    let doc = parse(source).unwrap();
    let res = resolve_block_ref(&doc, "non-existent-uuid-1234");
    assert!(!res.found);
    assert_eq!(res.text, "");
    assert_eq!(res.content, "");
    assert_eq!(res.block_id, None);
    assert!(res.backlinks.is_empty());
}

#[test]
fn rename_page_updates_korean_page_names() {
    let source = "- [[프로젝트 안내]] 시작\n- [[다른문서]]\n- [[프로젝트 안내]] 재참조\n";
    let (updated, changed) = update_page_links_text(source, "프로젝트 안내", "프로젝트 가이드");
    assert!(changed);
    assert_eq!(
        updated,
        "- [[프로젝트 가이드]] 시작\n- [[다른문서]]\n- [[프로젝트 가이드]] 재참조\n"
    );
}

#[test]
fn rename_page_preserves_aliases() {
    let source = "- [[옛이름]] 변경대상\n- [[옛이름|별칭]] 유지대상\n- [[별칭|옛이름]] 유지대상\n- [별칭]([[옛이름]]) 유지대상\n";
    let (updated, changed) = update_page_links_text(source, "옛이름", "새이름");
    assert!(changed);
    assert_eq!(
        updated,
        "- [[새이름]] 변경대상\n- [[옛이름|별칭]] 유지대상\n- [[별칭|옛이름]] 유지대상\n- [별칭]([[옛이름]]) 유지대상\n"
    );
}

#[test]
fn rename_page_preserves_nested_links() {
    let source = "- [[옛이름]] 변경대상\n- [[[[옛이름]]]] 중첩유지\n- [[[옛이름]]] 중첩유지\n- [[외부 [[옛이름]]]] 중첩유지\n";
    let (updated, changed) = update_page_links_text(source, "옛이름", "새이름");
    assert!(changed);
    assert_eq!(
        updated,
        "- [[새이름]] 변경대상\n- [[[[옛이름]]]] 중첩유지\n- [[[옛이름]]] 중첩유지\n- [[외부 [[옛이름]]]] 중첩유지\n"
    );
}

#[test]
fn rename_page_preserves_links_inside_code_blocks_and_inline_code() {
    let source = "\
- 정상링크 [[프로젝트]]
- 인라인코드 `[[프로젝트]]` 유지
```rust
// 코드블록 안의 [[프로젝트]] 유지
let link = \"[[프로젝트]]\";
```
- 다시 정상링크 [[프로젝트]]
";
    let (updated, changed) = update_page_links_text(source, "프로젝트", "새프로젝트");
    assert!(changed);
    let expected = "\
- 정상링크 [[새프로젝트]]
- 인라인코드 `[[프로젝트]]` 유지
```rust
// 코드블록 안의 [[프로젝트]] 유지
let link = \"[[프로젝트]]\";
```
- 다시 정상링크 [[새프로젝트]]
";
    assert_eq!(updated, expected);
}

#[test]
fn rename_page_in_files_atomic_replacement_and_preview() {
    let dir = temp_path("rename-test-dir");
    fs::create_dir_all(&dir).unwrap();
    let file1 = dir.join("page1.md");
    let file2 = dir.join("page2.md");
    let file3 = dir.join("page3.md");

    fs::write(&file1, "- [[옛페이지]] 링크\n").unwrap();
    fs::write(&file2, "- 다른 페이지 링크 [[다른곳]]\n").unwrap();
    fs::write(&file3, "- 또 다른 [[옛페이지]] 링크\n").unwrap();

    let paths = vec![file1.clone(), file2.clone(), file3.clone()];

    // 1. Preview mode: files are NOT modified
    let preview_res = rename_page_in_files(&paths, "옛페이지", "새페이지", true).unwrap();
    assert!(preview_res.preview);
    assert_eq!(preview_res.count, 2);
    assert_eq!(
        preview_res.affected_files,
        vec![file1.display().to_string(), file3.display().to_string()]
    );
    assert_eq!(
        fs::read_to_string(&file1).unwrap(),
        "- [[옛페이지]] 링크\n"
    );
    assert_eq!(
        fs::read_to_string(&file3).unwrap(),
        "- 또 다른 [[옛페이지]] 링크\n"
    );

    // 2. Execution mode: atomically replaces target files
    let exec_res = rename_page_in_files(&paths, "옛페이지", "새페이지", false).unwrap();
    assert!(!exec_res.preview);
    assert_eq!(exec_res.count, 2);
    assert_eq!(
        fs::read_to_string(&file1).unwrap(),
        "- [[새페이지]] 링크\n"
    );
    assert_eq!(
        fs::read_to_string(&file2).unwrap(),
        "- 다른 페이지 링크 [[다른곳]]\n"
    );
    assert_eq!(
        fs::read_to_string(&file3).unwrap(),
        "- 또 다른 [[새페이지]] 링크\n"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn api_handles_resolve_block_ref_and_rename_page() {
    use serde_json::json;

    // 1. resolve_block_ref via JSON API
    let req = json!({
        "id": "ref-1",
        "op": "resolve_block_ref",
        "document": "- 대상 id:: my-uuid-123\n- 참조 ((my-uuid-123))\n",
        "args": {"uuid": "my-uuid-123"}
    });
    let res = handle_request(req);
    assert_eq!(res["ok"], true);
    assert_eq!(res["result"]["found"], true);
    assert_eq!(res["result"]["block_id"], 1);
    assert_eq!(res["result"]["backlinks"], json!([2]));

    // Missing UUID returns empty result
    let req_missing = json!({
        "id": "ref-2",
        "op": "resolve_block_ref",
        "document": "- 아무내용\n",
        "args": {"uuid": "no-such-uuid"}
    });
    let res_missing = handle_request(req_missing);
    assert_eq!(res_missing["ok"], true);
    assert_eq!(res_missing["result"]["found"], false);
    assert_eq!(res_missing["result"]["text"], "");
    assert_eq!(res_missing["result"]["backlinks"], json!([]));

    // 2. rename_page via JSON API (document string mode)
    let req_rename = json!({
        "id": "ren-1",
        "op": "rename_page",
        "document": "- [[오래된페이지]] 내용\n",
        "args": {"old_name": "오래된페이지", "new_name": "새로운페이지"}
    });
    let res_rename = handle_request(req_rename);
    assert_eq!(res_rename["ok"], true);
    assert_eq!(
        res_rename["result"]["markdown"],
        "- [[새로운페이지]] 내용\n"
    );
    assert_eq!(res_rename["result"]["changed"], true);
}

