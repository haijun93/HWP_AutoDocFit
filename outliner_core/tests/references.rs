use outliner_core::{parse, references::*, serialize};
use std::collections::BTreeMap;

fn texts(lines: &[Vec<DisplaySpan>]) -> String {
    lines
        .iter()
        .map(|l| l.iter().map(|s| s.text.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn tokens_distinguish_targets_embeds_tags_and_unicode_ranges() {
    let text = "😀 [[페이지]] ((uuid)) (((legacy))) {{embed ((uuid))}} {{embed [[페이지]]}} #업무 #[[여러 단어]]";
    let tokens = reference_tokens(text);
    assert_eq!(tokens.len(), 7);
    assert_eq!(
        tokens.iter().map(|t| t.embed).collect::<Vec<_>>(),
        [false, false, true, true, true, false, false]
    );
    assert_eq!(tokens[6].target, RefTarget::Tag("여러 단어".into()));
    assert_eq!(&text[tokens[3].start..tokens[3].end], "{{embed ((uuid))}}");
}

#[test]
fn code_and_incomplete_or_invalid_references_remain_literal() {
    for text in [
        "`((id))`",
        "```\n[[page]]\n```",
        "`` #tag ``",
        "((",
        "[[ ]]",
        "{{embed ((id)) extra}}",
    ] {
        let tokens = reference_tokens(text);
        if text.starts_with("{{") {
            assert!(tokens.iter().all(|t| !t.embed));
        } else {
            assert!(tokens.is_empty(), "{text}");
        }
    }
}

#[test]
fn inline_reference_uses_live_text_without_mutation_or_id_creation() {
    let input = "- 😀 ((stable))\r\n- 대상 본문\r\n  id:: stable\r\n";
    let doc = parse(input).unwrap();
    let graph = BTreeMap::new();
    let refs = References::new(&graph, "현재", &doc);
    assert_eq!(texts(&refs.render("현재", 0)), "😀 ↗ 대상 본문");
    assert_eq!(serialize(&doc), input);
    assert!(!doc.edited);
}

#[test]
fn missing_and_duplicate_ids_are_explicit_and_never_pick_a_random_block() {
    let doc = parse("- ((absent)) ((dup))\n- A\n  id:: dup\n- B\n  id:: dup\n").unwrap();
    let graph = BTreeMap::new();
    let refs = References::new(&graph, "p", &doc);
    assert_eq!(
        refs.resolve("p", &RefTarget::Block("dup".into())),
        Resolved::Ambiguous
    );
    let states: Vec<_> = refs.render("p", 0)[0]
        .iter()
        .filter(|s| s.target.is_some())
        .map(|s| s.state)
        .collect();
    assert_eq!(states, [RefState::Missing, RefState::Ambiguous]);
}

#[test]
fn permanent_ids_cross_pages_while_numeric_ids_stay_local() {
    let other = parse("- 다른 페이지\n  id:: cross\n- 두번째\n").unwrap();
    let doc = parse("- ((cross)) ((2))\n").unwrap();
    let graph = BTreeMap::from([("other".into(), other)]);
    let refs = References::new(&graph, "here", &doc);
    assert_eq!(
        refs.resolve("here", &RefTarget::Block("cross".into())),
        Resolved::Found("other", 0)
    );
    assert_eq!(
        refs.resolve("here", &RefTarget::Block("2".into())),
        Resolved::Missing
    );
    assert_eq!(
        refs.resolve("other", &RefTarget::Block("2".into())),
        Resolved::Missing
    );
}

#[test]
fn journal_aliases_resolve_and_duplicate_page_aliases_are_explicit() {
    let doc = parse("- {{embed [[2026_11_26]]}}\n").unwrap();
    let mut graph = BTreeMap::from([("2026-11-26".into(), parse("- 일지\n").unwrap())]);
    assert_eq!(
        References::new(&graph, "p", &doc).resolve("p", &RefTarget::Page("2026_11_26".into())),
        Resolved::Found("2026-11-26", 0)
    );
    graph.insert("2026_11_26".into(), parse("- 중복 일지\n").unwrap());
    assert_eq!(
        References::new(&graph, "p", &doc).resolve("p", &RefTarget::Page("2026_11_26".into())),
        Resolved::Ambiguous
    );
}

#[test]
fn block_embed_expands_descendants_but_excludes_siblings() {
    let doc =
        parse("- {{embed ((root))}}\n- 부모\n  id:: root\n  - 자식\n    - 손자\n- 형제\n").unwrap();
    let graph = BTreeMap::new();
    let refs = References::new(&graph, "p", &doc);
    let shown = texts(&refs.render("p", 0));
    assert!(shown.contains("부모") && shown.contains("자식") && shown.contains("손자"));
    assert!(!shown.contains("형제"));
    assert!(shown.lines().any(|s| s == "  부모"));
    assert!(shown.lines().any(|s| s == "    자식"));
    assert!(shown.lines().any(|s| s == "      손자"));
}

#[test]
fn page_embed_and_cross_page_cycle_are_bounded() {
    let a = parse("- {{embed [[b]]}}\n").unwrap();
    let b = parse("- {{embed [[a]]}}\n").unwrap();
    let graph = BTreeMap::from([("b".into(), b)]);
    let refs = References::new(&graph, "a", &a);
    let result = refs.render("a", 0);
    assert!(result.iter().flatten().any(|s| s.state == RefState::Cycle));
    assert!(result.len() < MAX_DISPLAY_LINES);
}

#[test]
fn expansion_has_depth_line_and_character_limits() {
    let body = (0..100)
        .map(|i| format!("- line {i}\n"))
        .collect::<String>();
    let a = parse("- {{embed [[large]]}}\n").unwrap();
    let graph = BTreeMap::from([("large".into(), parse(&body).unwrap())]);
    let result = References::new(&graph, "a", &a).render("a", 0);
    assert!(result.len() <= MAX_DISPLAY_LINES);
    assert!(result.iter().flatten().any(|s| s.state == RefState::Limit));
    let huge = parse(&format!("- {}\n", "한".repeat(5000))).unwrap();
    let empty = BTreeMap::new();
    let result = References::new(&empty, "a", &huge).render("a", 0);
    assert!(texts(&result).chars().count() <= MAX_DISPLAY_CHARS);
    assert!(texts(&result).ends_with('…'));
    let mut graph = BTreeMap::new();
    for i in 1..20 {
        graph.insert(
            format!("p{i}"),
            parse(&format!("- {{{{embed [[p{}]]}}}}\n", i + 1)).unwrap(),
        );
    }
    let a = parse("- {{embed [[p1]]}}\n").unwrap();
    assert!(References::new(&graph, "a", &a)
        .render("a", 0)
        .iter()
        .flatten()
        .any(|s| s.state == RefState::Limit));
}

#[test]
fn recursive_inline_reference_terminates_with_cycle_label() {
    let doc = parse("- ((b))\n  id:: a\n- ((a))\n  id:: b\n").unwrap();
    let graph = BTreeMap::new();
    let shown = texts(&References::new(&graph, "p", &doc).render("p", 0));
    assert!(shown.contains("순환 참조"));
}

#[test]
fn bracket_tags_and_property_values_participate_in_indexes() {
    let doc = parse("- #[[여러 단어]] #업무\n  tags:: [[장기 계획]], 재정\n  owner:: [[담당자]]\n  source:: ((uuid))\n").unwrap();
    assert!(doc.blocks[0].tags.contains(&"여러 단어".into()));
    assert!(doc.blocks[0].tags.contains(&"장기 계획".into()));
    assert!(doc.blocks[0].tags.contains(&"재정".into()));
    assert!(doc.blocks[0].page_links.contains(&"담당자".into()));
    assert!(doc.blocks[0].block_refs.contains(&"uuid".into()));
}

#[test]
fn property_edit_preserves_crlf_full_value_order_and_other_properties() {
    let input =
        "- 본문\r\n  owner:: 이전 값\r\n  id:: fixed\r\n  collapsed:: true\r\n- 다른 블록\r\n";
    let mut doc = parse(input).unwrap();
    set_property(&mut doc, 1, "owner", "새 담당자 😀").unwrap();
    let after = serialize(&doc);
    assert!(after.contains("owner:: 새 담당자 😀\r\n"));
    assert!(after.contains("id:: fixed\r\n") && after.contains("collapsed:: true\r\n"));
    assert!(after.ends_with("- 다른 블록\r\n"));
    assert_eq!(
        parse(&after).unwrap().blocks[0].properties["owner"],
        "새 담당자 😀"
    );
}

#[test]
fn invalid_property_edits_leave_document_identical() {
    let mut doc = parse("- 본문\n").unwrap();
    let before = doc.clone();
    for (id, k, v) in [(1, "bad key", "v"), (1, "ok", "a\nb"), (9, "ok", "v")] {
        assert!(set_property(&mut doc, id, k, v).is_err());
        assert_eq!(doc, before);
    }
}

#[test]
fn page_names_with_parentheses_stay_links_and_a_lone_backtick_is_literal() {
    let tokens = reference_tokens("it`s [[회의(1차)]] #[[예산 (안)]]");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].target, RefTarget::Page("회의(1차)".into()));
    assert_eq!(tokens[1].target, RefTarget::Tag("예산 (안)".into()));
}

#[test]
fn fenced_code_spanning_lines_is_not_expanded() {
    let doc = parse("- ```
  ((stable)) [[a]]
  ```
- 대상
  id:: stable
").unwrap();
    let graph = BTreeMap::new();
    let lines = References::new(&graph, "p", &doc).render("p", 0);
    assert_eq!(texts(&lines), "```
((stable)) [[a]]
```");
    assert!(lines.iter().flatten().all(|s| s.target.is_none()));
}

#[test]
fn ordinary_links_keep_their_source_text_and_collapsed_marker_stays_hidden() {
    let doc = parse("- [[ 새 페이지 ]] collapsed:: true
").unwrap();
    let graph = BTreeMap::new();
    let lines = References::new(&graph, "p", &doc).render("p", 0);
    assert_eq!(texts(&lines).trim_end(), "[[ 새 페이지 ]]");
    assert!(lines.iter().flatten().all(|s| s.state == RefState::Found));
}

#[test]
fn property_edit_replaces_in_place_and_refuses_properties_written_in_the_body() {
    let mut doc = parse("- 본문
  a:: 1
  b:: 2
").unwrap();
    set_property(&mut doc, 1, "a", "3").unwrap();
    assert_eq!(serialize(&doc), "- 본문
  a:: 3
  b:: 2
");
    let mut doc = parse("- title:: 제목
").unwrap();
    let before = doc.clone();
    assert!(set_property(&mut doc, 1, "title", "다른 제목").is_err());
    assert_eq!(doc, before);
}
