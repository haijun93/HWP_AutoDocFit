//! Python `_classify` 판정과 Rust `pdf_classify` 판정의 차이 0 대조.
//! 기대값: tests/fixtures/classify/expected.json (tools/gen_classify_fixtures.py 가 pypdf 로 생성).
use pdf_edit_core::pdf_classify::{check_size, classify_path, MAX_PDF_BYTES};
use std::path::PathBuf;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/classify")
}

/// expected.json 은 `"이름": "코드"` 한 줄 형식이라 외부 JSON 의존 없이 읽는다.
fn expected() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(dir().join("expected.json")).unwrap();
    let body = &text[text.find("\"cases\"").unwrap()..];
    let end = body.find("
 }").unwrap();
    body[..end].lines()
        .filter_map(|l| {
            let mut q = l.trim().trim_end_matches(',').split('"').filter(|s| !s.is_empty() && *s != ": ");
            let (k, v) = (q.next()?, q.next()?);
            (k != "cases" && !k.ends_with(": {")).then(|| (k.to_string(), v.to_string()))
        })
        .collect()
}

#[test]
fn rust_classify_matches_python_verdicts_for_every_fixture() {
    let cases = expected();
    assert!(cases.len() >= 25, "기대값 항목이 너무 적음: {}", cases.len());
    let mut diffs = Vec::new();
    for (name, want) in &cases {
        let got = match classify_path(dir().join(format!("{name}.pdf"))) {
            Ok(()) => "OK".to_string(),
            Err(e) => e.code.to_string(),
        };
        println!("{name:28} python={want:16} rust={got}");
        if &got != want {
            diffs.push(format!("{name}: python={want} rust={got}"));
        }
    }
    assert!(diffs.is_empty(), "Python 대비 차이 {}건:\n{}", diffs.len(), diffs.join("\n"));
}

#[test]
fn size_limit_is_one_gib() {
    assert!(check_size(MAX_PDF_BYTES).is_ok());
    assert_eq!(check_size(MAX_PDF_BYTES + 1).unwrap_err().code, "PDF_TOO_LARGE");
}
