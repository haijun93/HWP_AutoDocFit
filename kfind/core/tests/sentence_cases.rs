//! 문장 나누기 대조 표(tests/data/sentence_cases.tsv, 200문단): Python 기준 구현(tools/sentence_ref.py)이 낸 구간과
//! Rust `split_sentences`가 같은지.
mod common;
use common::*;
use kfind_core::{SENTENCE_RULE_VERSION, split_sentences};

fn spans_text(text: &str) -> String {
    let sp = split_sentences(text);
    if sp.is_empty() {
        return "-".to_string();
    }
    sp.iter().map(|s| format!("{}:{}", s.start, s.len)).collect::<Vec<_>>().join(" ")
}

#[test]
fn sentence_table_matches_python_reference() {
    let rows = rows("sentence_cases.tsv");
    assert_eq!(rows.len(), 200, "table rows");
    let mut diff = Vec::new();
    for r in &rows {
        let text = from_hex(&r[0]);
        let got = spans_text(&text);
        if got != r[1] {
            diff.push(format!("{} ({}): py={} rs={}", r[0], r[2], r[1], got));
        }
    }
    assert!(diff.is_empty(), "{} differences:\n{}", diff.len(), diff.join("\n"));
}

#[test]
fn spans_cover_text_and_are_trimmed() {
    for r in rows("sentence_cases.tsv") {
        let text = from_hex(&r[0]);
        let mut prev_end = 0;
        for sp in split_sentences(&text) {
            let s = sp.text(&text);
            assert!(sp.byte_start >= prev_end, "overlap in {}", r[0]);
            assert_eq!(s.chars().count() as u32, sp.len);
            assert_eq!(text[..sp.byte_start].chars().count() as u32, sp.start);
            assert!(!s.is_empty() && s.trim() == s, "untrimmed {:?}", s);
            prev_end = sp.byte_end;
        }
    }
}

#[test]
fn sentence_rule_version_is_pinned() {
    assert_eq!(SENTENCE_RULE_VERSION, 1);
}
