//! KLIBENT1: 표준 XML 엔티티·숫자 참조는 풀고, DTD·사용자 정의/외부 엔티티는 거부한다.
//! 합성 문서만 쓰며 klib 실행 파일의 `extract`·`legacy-records` 명령으로 확인한다.

use std::io::Write;
use std::process::{Command, Output};

fn run(command: &str, name: &str, entries: &[(&str, &str)]) -> Output {
    let path = std::env::temp_dir().join(format!("klib-entity-{}-{command}-{name}", std::process::id()));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (entry, body) in entries {
        zip.start_file(*entry, options).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_klib")).arg(command).arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);
    output
}

fn section(prolog: &str, text: &str) -> String {
    format!(r#"<?xml version="1.0" encoding="UTF-8"?>{prolog}<hs:sec xmlns:hs="s" xmlns:hp="p"><hp:p><hp:run><hp:t>{text}</hp:t></hp:run></hp:p></hs:sec>"#)
}

fn document(prolog: &str, text: &str) -> String {
    format!(r#"<?xml version="1.0" encoding="UTF-8"?>{prolog}<w:document xmlns:w="w"><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body></w:document>"#)
}

const ENCODED: &str = "특수 &amp; &lt; &gt; &quot; &apos; &#44;&#x20;&#xAC00;&#128512; &amp;lt;끝";
const DECODED: &str = "특수 & < > \\\" ' , 가😀 &lt;끝"; // JSON 출력 형태

fn assert_decoded(output: Output, label: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{label}: {}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains(DECODED), "{label}: {stdout}");
}

fn assert_rejected(output: Output, label: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!output.status.success(), "{label}: 거부해야 함: {stdout}");
    assert!(!stdout.contains("비밀"), "{label}: {stdout}");
}

#[test]
fn hwpx_extract_decodes_standard_and_numeric_references() {
    assert_decoded(run("extract", "ok.hwpx", &[("Contents/section0.xml", &section("", ENCODED))]), "extract hwpx");
}

#[test]
fn hwpx_index_walker_decodes_the_same_text() {
    assert_decoded(run("legacy-records", "ok.hwpx", &[("Contents/section0.xml", &section("", ENCODED))]), "legacy-records");
}

#[test]
fn docx_extract_decodes_standard_and_numeric_references() {
    assert_decoded(run("extract", "ok.docx", &[("word/document.xml", &document("", ENCODED))]), "extract docx");
}

#[test]
fn dtd_custom_external_and_illegal_references_stay_rejected() {
    let cases = [
        ("custom", "", "&secret;"),
        ("html", "", "&nbsp;"),
        ("nul", "", "&#0;"),
        ("control", "", "&#x1;"),
        ("surrogate", "", "&#xD800;"),
        ("noncharacter", "", "&#xFFFE;"),
        ("out-of-range", "", "&#x110000;"),
        ("signed", "", "&#+65;"),
        ("dtd", "<!DOCTYPE a>", "본문"),
        ("internal", r#"<!DOCTYPE a [<!ENTITY x "비밀">]>"#, "&x;"),
        ("external", r#"<!DOCTYPE a [<!ENTITY x SYSTEM "file:///비밀">]>"#, "&x;"),
        ("redefined", r#"<!DOCTYPE a [<!ENTITY amp "비밀">]>"#, "&amp;"),
    ];
    for (name, prolog, text) in cases {
        let hwpx = section(prolog, text);
        for command in ["extract", "legacy-records"] {
            assert_rejected(run(command, &format!("{name}.hwpx"), &[("Contents/section0.xml", &hwpx)]), &format!("{command} hwpx {name}"));
        }
        assert_rejected(run("extract", &format!("{name}.docx"), &[("word/document.xml", &document(prolog, text))]), &format!("extract docx {name}"));
    }
}

#[test]
fn docx_attribute_rejects_custom_and_illegal_references() {
    let cases = [
        ("custom", "&secret;"),
        ("html", "&nbsp;"),
        ("nul", "&#0;"),
        ("control", "&#x1;"),
        ("surrogate", "&#xD800;"),
        ("noncharacter", "&#xFFFE;"),
        ("out-of-range", "&#x110000;"),
        ("signed", "&#+65;"),
    ];
    for (name, attr_val) in cases {
        let doc = format!(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="w"><w:body><w:p><w:pPr><w:pStyle w:val="{attr_val}"/></w:pPr><w:r><w:t>본문</w:t></w:r></w:p></w:body></w:document>"#);
        assert_rejected(run("extract", &format!("attr_{name}.docx"), &[("word/document.xml", &doc)]), &format!("extract docx attr {name}"));
    }
}

#[test]
fn docx_attribute_decodes_standard_and_numeric_references() {
    let doc = format!(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="w"><w:body><w:p><w:pPr><w:pStyle w:val="{ENCODED}"/></w:pPr><w:r><w:t>본문</w:t></w:r></w:p></w:body></w:document>"#);
    let output = run("extract", "attr_ok.docx", &[("word/document.xml", &doc)]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "docx attribute decoding failed: {}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("본문"), "extract docx output missing body: {stdout}");
}
