//! 비교형 규칙 시험 표(tests/data/fold_cases.tsv): Python 기준 구현이 낸 기대값과 Rust `fold`가 같은지.
mod common;
use common::*;
use kfind_core::{FOLD_RULE_VERSION, UNICODE_VERSION, fold, parse_query, tokens};

#[test]
fn fold_table_matches_python_reference() {
    let rows = rows("fold_cases.tsv");
    assert!(rows.len() >= 30, "table has {} rows", rows.len());
    for r in &rows {
        let input = from_hex(&r[0]);
        assert_eq!(fold(&input), from_hex(&r[1]), "{} ({})", r[0], r[2]);
        assert_eq!(tokens(&input).concat(), fold(&input), "tokens of {}", r[0]);
    }
}

#[test]
fn composed_and_decomposed_hangul_are_equal() {
    // 완성형과 조합형(첫가끝)은 같은 비교형이 된다.
    let composed = "예산 집행 계획";
    let decomposed = "\u{110b}\u{1168}\u{1109}\u{1161}\u{11ab} \u{110c}\u{1175}\u{11b8}\u{1112}\u{1162}\u{11bc} \u{1100}\u{1168}\u{1112}\u{116c}\u{11a8}";
    assert_ne!(composed, decomposed);
    assert_eq!(fold(composed), fold(decomposed));
    assert_eq!(parse_query(composed), parse_query(decomposed));
}

#[test]
fn rule_version_is_pinned() {
    assert_eq!(FOLD_RULE_VERSION, 1);
    assert_eq!(UNICODE_VERSION, "16.0.0");
}
