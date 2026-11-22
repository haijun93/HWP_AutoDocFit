//! RSXML2 단위 시험: 개요 첫 빈칸·쪽 번호 모양·자간 초기화, 설정 전달과 '미지원 입력' 거부. 합성 HWPX 만 쓴다.
//! 보호 구간 기대값은 파이썬 `자간보호_글자수`(활성 항목기호 체계 없음, 내어쓰기 규칙 없음)가 실제로 돌려준 값이다.
use hwpx_fmt::config::{Config, UNSUPPORTED};
use hwpx_fmt::{marker, process_with, rules, survey_with, zipio, Processor};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

const NS: &str = r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section""#;

fn make_zip(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (n, d) in entries {
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        z.start_file(*n, opts).unwrap();
        z.write_all(d).unwrap();
    }
    z.finish().unwrap().into_inner()
}
fn sec(body: &str) -> String {
    format!(r#"<?xml version="1.0" encoding="UTF-8"?><hs:sec {NS}>{body}</hs:sec>"#)
}
fn header(charprs: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head"><hh:refList><hh:charProperties itemCnt="2">{charprs}</hh:charProperties></hh:refList></hh:head>"#
    )
}
const CHARPRS: &str = r#"<hh:charPr id="0"><hh:spacing hangul="0" latin="0"/></hh:charPr><hh:charPr id="1"><hh:spacing hangul="-5" latin="-5"/></hh:charPr>"#;
fn doc(h: &str, s: &str) -> Vec<u8> {
    make_zip(&[("mimetype", b"application/hwp+zip".to_vec()), ("Contents/header.xml", h.as_bytes().to_vec()), ("Contents/section0.xml", s.as_bytes().to_vec())])
}
fn tmp(dir: &Path, name: &str, data: &[u8]) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, data).unwrap();
    p
}
fn entry(path: &Path, name: &str) -> String {
    let c = zipio::read_hwpx_safely(path).unwrap();
    String::from_utf8(c.into_iter().find(|(n, _)| n == name).unwrap().1).unwrap()
}
fn cfg(json: &str) -> Config {
    Config::from_json(&serde_json::from_str(json).unwrap()).unwrap()
}
fn one_cell(text_paras: &str) -> String {
    format!(
        r#"<hp:tbl rowCnt="1" colCnt="1"><hp:tr><hp:tc><hp:subList>{text_paras}</hp:subList><hp:cellAddr colAddr="0" rowAddr="0"/><hp:cellSpan colSpan="1" rowSpan="1"/></hp:tc></hp:tr></hp:tbl>"#
    )
}
fn para(text: &str) -> String {
    format!("<hp:p><hp:run><hp:t>{text}</hp:t></hp:run></hp:p>")
}
fn in_p(tbl: &str) -> String {
    format!("<hp:p><hp:run>{tbl}</hp:run></hp:p>")
}
fn title_table() -> String {
    r#"<hp:tbl rowCnt="2" colCnt="2"><hp:tr><hp:tc><hp:subList><hp:p><hp:run><hp:t>보고서 제목</hp:t></hp:run></hp:p></hp:subList><hp:cellAddr colAddr="0" rowAddr="0"/><hp:cellSpan colSpan="2" rowSpan="1"/></hp:tc></hp:tr><hp:tr><hp:tc><hp:subList><hp:p><hp:run><hp:t>2026. 10. 10.</hp:t></hp:run></hp:p></hp:subList><hp:cellAddr colAddr="0" rowAddr="1"/><hp:cellSpan colSpan="1" rowSpan="1"/></hp:tc><hp:tc><hp:subList><hp:p><hp:run><hp:t>담당 과장 홍길동</hp:t></hp:run></hp:p></hp:subList><hp:cellAddr colAddr="1" rowAddr="1"/><hp:cellSpan colSpan="1" rowSpan="1"/></hp:tc></hp:tr></hp:tbl>"#.to_string()
}
const LONG: &str = "개요 본문은 열다섯 글자를 넘는 한 칸 상자 글입니다";

// ---------------------------------------------------------------- 보호 구간(자간 초기화)

#[test]
fn protect_offsets_equal_python_values() {
    // (글, 파이썬 `자간보호_글자수(글) or 0`)
    let table: &[(&str, usize)] = &[
        ("□ 항목 본문", 2),
        ("ㅇ (라벨) 본문 글", 7),
        ("- (방식) : 본문", 9),
        ("- 일시: 2026. 10. 10. 본문", 6),
        ("□ (운영방식)", 8),
        ("ㅇ (운영방식) :", 10),
        ("□", 1),
        ("□   ", 4),
        ("1. 가나다", 3),
        ("(1) 차카타", 4),
        ("Ⅰ. 로마", 0),
        ("※ 참고 사항", 2),
        ("** 별표 둘", 3),
        ("ㅇ 「법령」을 따름", 3),
        ("2026. 10. 10. 행사", 6),
        ("“인용” 글", 0),
        ("(괄호) 본문", 0),
        ("  □ 앞 공백", 4),
        ("본문 글", 0),
        ("ㅇ http: 본문 글", 2),
        ("ㅇ 아주아주 긴 라벨이 열두 글자를 넘는 경우: 본문", 2),
        ("1-1. 하위", 5),
        ("IV. 로마", 0),
        ("❶ 검은 원", 2),
        ("② 원숫자", 2),
        ("(가나다) 설명", 0),
        ("ㅇ (2026) 설명", 2),
        ("ㅇ (열림 본문", 2),
    ];
    for (text, want) in table {
        assert_eq!(marker::spacing_protect(text), *want, "{text:?}");
    }
}

#[test]
fn classify_marker_uses_character_offsets() {
    assert_eq!(marker::classify_marker("  1. 가"), Some(4));
    assert_eq!(marker::classify_marker("1.2.3 가"), Some(5));
    assert_eq!(marker::classify_marker("2026. 10. 10. 행사"), Some(5)); // 파이썬도 "2026." 을 번호로 본다(점 뒤 공백이 있으면 날짜 제외 규칙에 안 걸림)
    assert_eq!(marker::classify_marker("2026.10.10. 행사"), None); // 붙여 쓴 문두 날짜는 번호로 보지 않음
    assert_eq!(marker::classify_marker("1.x"), None); // 뒤가 공백이 아니면 번호가 아님
    assert_eq!(marker::classify_marker("** a"), Some(2));
    assert_eq!(marker::classify_marker("😀 가"), Some(1)); // 코드 포인트 1개(UTF-8 4바이트)
    assert_eq!(marker::classify_marker("가나다"), None);
}

// ---------------------------------------------------------------- 개요 첫 빈칸

#[test]
fn overview_lead_inserts_space_and_drops_linesegarray() {
    let dir = tempfile::tempdir().unwrap();
    let box_cell = one_cell(&format!(
        "<hp:p><hp:run><hp:t>{LONG}</hp:t></hp:run><hp:linesegarray><hp:lineseg textpos=\"0\"/></hp:linesegarray></hp:p>"
    ));
    let s = sec(&format!("{}{}", in_p(&title_table()), in_p(&box_cell)));
    let src = tmp(dir.path(), "in.hwpx", &doc(&header(CHARPRS), &s));
    let before = std::fs::read(&src).unwrap();
    let c = Config::default();
    let sel = survey_with(Processor::OverviewLead, &src, &c).unwrap();
    assert_eq!(sel, vec![("Contents/section0.xml".to_string(), vec![0])]);
    let out = dir.path().join("out.hwpx");
    assert_eq!(process_with(Processor::OverviewLead, &src, &out, None, &c).unwrap(), 1);
    let x = entry(&out, "Contents/section0.xml");
    assert!(x.contains(&format!("<hp:t> {LONG}</hp:t>")), "{x}");
    assert!(!x.contains("linesegarray"), "{x}");
    assert_eq!(std::fs::read(&src).unwrap(), before);
}

#[test]
fn overview_lead_skips_existing_space_and_untitled_boxes() {
    let dir = tempfile::tempdir().unwrap();
    let spaced = one_cell(&para(&format!(" {LONG}")));
    let plain = one_cell(&para(LONG));
    // 제목 뒤 상자인데 이미 빈칸 / 제목 없이 상자만 / 제목과 상자 사이에 글 문단
    for (name, body) in [
        ("spaced", format!("{}{}", in_p(&title_table()), in_p(&spaced))),
        ("untitled", in_p(&plain)),
        ("text_between", format!("{}{}{}", in_p(&title_table()), para("끼어든 글"), in_p(&plain))),
    ] {
        let src = tmp(dir.path(), &format!("{name}.hwpx"), &doc(&header(CHARPRS), &sec(&body)));
        let out = dir.path().join(format!("{name}.out.hwpx"));
        assert_eq!(process_with(Processor::OverviewLead, &src, &out, None, &Config::default()).unwrap(), 0, "{name}");
    }
}

#[test]
fn overview_lead_rejects_owner_text_setting() {
    let dir = tempfile::tempdir().unwrap();
    let src = tmp(dir.path(), "in.hwpx", &doc(&header(CHARPRS), &sec("")));
    let out = dir.path().join("out.hwpx");
    let c = cfg(r#"{"title_owner_text":"담당관"}"#);
    let e = process_with(Processor::OverviewLead, &src, &out, None, &c).unwrap_err();
    assert!(e.starts_with(UNSUPPORTED), "{e}");
    assert!(!out.exists());
    // 같은 설정이라도 반영 여부가 결과에 영향이 없는 날짜 처리기는 받는다
    assert!(survey_with(Processor::Date, &src, &c).is_ok());
}

// ---------------------------------------------------------------- 쪽 번호

#[test]
fn page_number_sets_only_differing_elements_in_given_order() {
    let dir = tempfile::tempdir().unwrap();
    let body = r#"<hp:p><hp:run><hp:ctrl><hp:pageNum pos="TOP_LEFT" keep="1"/><hp:pageNum pos="BOTTOM_CENTER" formatType="DIGIT" sideChar="-"/></hp:ctrl></hp:run></hp:p>"#;
    let src = tmp(dir.path(), "in.hwpx", &doc(&header(CHARPRS), &sec(body)));
    let c = cfg(r#"{"page_number":{"pos":"BOTTOM_CENTER","formatType":"DIGIT","sideChar":"-"}}"#);
    assert_eq!(survey_with(Processor::PageNumber, &src, &c).unwrap(), vec![("Contents/section0.xml".to_string(), vec![0])]);
    let out = dir.path().join("out.hwpx");
    assert_eq!(process_with(Processor::PageNumber, &src, &out, None, &c).unwrap(), 1);
    let x = entry(&out, "Contents/section0.xml");
    // 기존 속성은 제자리에서 바뀌고 없던 속성은 설정 순서대로 뒤에 붙으며 다른 속성(keep)은 남는다
    assert!(x.contains(r#"<hp:pageNum pos="BOTTOM_CENTER" keep="1" formatType="DIGIT" sideChar="-" />"#), "{x}");
}

#[test]
fn page_number_empty_config_is_a_byte_copy() {
    let dir = tempfile::tempdir().unwrap();
    let src = tmp(dir.path(), "in.hwpx", &doc(&header(CHARPRS), &sec(r#"<hp:p><hp:run><hp:pageNum pos="X"/></hp:run></hp:p>"#)));
    let out = dir.path().join("out.hwpx");
    assert_eq!(process_with(Processor::PageNumber, &src, &out, None, &cfg(r#"{"page_number":{}}"#)).unwrap(), 0);
    assert_eq!(std::fs::read(&src).unwrap(), std::fs::read(&out).unwrap());
}

#[test]
fn config_rejects_unknown_keys_and_non_string_values() {
    for j in [r#"{"모르는":1}"#, r#"{"page_number":{"pos":3}}"#, r#"{"page_number":[1]}"#, r#"{"marker_scheme_active":"yes"}"#, r#"{"indent_rules":5}"#, "[1]"] {
        let e = Config::from_json(&serde_json::from_str(j).unwrap()).unwrap_err();
        assert!(e.starts_with(UNSUPPORTED), "{j}: {e}");
    }
    assert!(Config::from_json(&serde_json::from_str(r#"{"page_number":null,"title_owner_text":null,"indent_rules":null}"#).unwrap()).is_ok());
}

// ---------------------------------------------------------------- 자간 초기화

fn run(cid: &str, text: &str) -> String {
    format!(r#"<hp:run charPrIDRef="{cid}"><hp:t>{text}</hp:t></hp:run>"#)
}

#[test]
fn spacing_reset_adds_zero_charpr_splits_marker_run_and_updates_itemcnt() {
    let dir = tempfile::tempdir().unwrap();
    let body = format!(r#"<hp:p>{}</hp:p><hp:p>{}</hp:p><hp:p>{}</hp:p>"#, run("1", "□ 항목 본문"), run("1", "본문 글"), run("0", "이미 0"));
    let src = tmp(dir.path(), "in.hwpx", &doc(&header(CHARPRS), &sec(&body)));
    let out = dir.path().join("out.hwpx");
    let c = Config::default();
    assert_eq!(survey_with(Processor::SpacingReset, &src, &c).unwrap(), vec![("Contents/section0.xml".to_string(), vec![0])]);
    assert_eq!(process_with(Processor::SpacingReset, &src, &out, None, &c).unwrap(), 2);
    let x = entry(&out, "Contents/section0.xml");
    // 첫 문단은 보호 구간("□ ") 뒤에서 둘로 나뉘고 뒤쪽만 새 글자 모양(2)으로, 둘째 문단은 통째로 2로 바뀐다
    assert!(x.contains(r#"<hp:run charPrIDRef="1"><hp:t>□ </hp:t></hp:run><hp:run charPrIDRef="2"><hp:t>항목 본문</hp:t></hp:run>"#), "{x}");
    assert!(x.contains(r#"<hp:run charPrIDRef="2"><hp:t>본문 글</hp:t></hp:run>"#), "{x}");
    assert!(x.contains(r#"<hp:run charPrIDRef="0"><hp:t>이미 0</hp:t></hp:run>"#), "{x}");
    let h = entry(&out, "Contents/header.xml");
    assert!(h.contains(r#"itemCnt="3""#), "{h}");
    assert!(h.contains(r#"<hh:charPr id="2"><hh:spacing hangul="0" latin="0" /></hh:charPr>"#), "{h}");
    assert!(h.contains(r#"<hh:spacing hangul="-5" latin="-5" />"#), "원래 글자 모양은 그대로: {h}");
}

#[test]
fn spacing_reset_without_spacing_is_a_byte_copy_and_keeps_run_with_children() {
    let dir = tempfile::tempdir().unwrap();
    let src = tmp(dir.path(), "a.hwpx", &doc(&header(CHARPRS), &sec(&format!("<hp:p>{}</hp:p>", run("0", "본문")))));
    let out = dir.path().join("a.out.hwpx");
    assert_eq!(process_with(Processor::SpacingReset, &src, &out, None, &Config::default()).unwrap(), 0);
    assert_eq!(std::fs::read(&src).unwrap(), std::fs::read(&out).unwrap());
    // 보호 구간에 걸친 run 이 자식 요소를 가지면 나누지 않고 건너뛴다 → 바꾼 run 없음 → 바이트 복사
    let body = r#"<hp:p><hp:run charPrIDRef="1"><hp:t>□ 본문<hp:fwSpace/>끝</hp:t></hp:run></hp:p>"#;
    let src2 = tmp(dir.path(), "b.hwpx", &doc(&header(CHARPRS), &sec(body)));
    let out2 = dir.path().join("b.out.hwpx");
    assert_eq!(process_with(Processor::SpacingReset, &src2, &out2, None, &Config::default()).unwrap(), 0);
    assert_eq!(std::fs::read(&src2).unwrap(), std::fs::read(&out2).unwrap());
}

#[test]
fn spacing_reset_sections_are_processed_in_numeric_order() {
    // section10 은 압축 안 순서로 앞에 있어도 section2 다음에 처리되어 새 글자 모양 번호가 파이썬(숫자 순서)과 같다
    let dir = tempfile::tempdir().unwrap();
    let hdr = header(r#"<hh:charPr id="0"><hh:spacing hangul="0"/></hh:charPr><hh:charPr id="1"><hh:spacing hangul="-5"/></hh:charPr><hh:charPr id="2"><hh:spacing hangul="4"/></hh:charPr>"#);
    let z = make_zip(&[
        ("mimetype", b"application/hwp+zip".to_vec()),
        ("Contents/header.xml", hdr.into_bytes()),
        ("Contents/section10.xml", sec(&format!("<hp:p>{}</hp:p>", run("2", "열"))).into_bytes()),
        ("Contents/section2.xml", sec(&format!("<hp:p>{}</hp:p>", run("1", "둘"))).into_bytes()),
    ]);
    let src = tmp(dir.path(), "in.hwpx", &z);
    let out = dir.path().join("out.hwpx");
    assert_eq!(process_with(Processor::SpacingReset, &src, &out, None, &Config::default()).unwrap(), 2);
    assert!(entry(&out, "Contents/section2.xml").contains(r#"charPrIDRef="3""#)); // 먼저: id 3
    assert!(entry(&out, "Contents/section10.xml").contains(r#"charPrIDRef="4""#)); // 나중: id 4
}

#[test]
fn spacing_reset_rejects_unsupported_settings_without_output() {
    let dir = tempfile::tempdir().unwrap();
    let src = tmp(dir.path(), "in.hwpx", &doc(&header(CHARPRS), &sec(&format!("<hp:p>{}</hp:p>", run("1", "□ 본문")))));
    for j in [r#"{"indent_rules":{"□":"after_marker"}}"#, r#"{"marker_scheme_active":true}"#] {
        let out = dir_out(dir.path(), j);
        let e = process_with(Processor::SpacingReset, &src, &out, None, &cfg(j)).unwrap_err();
        assert!(e.starts_with(UNSUPPORTED), "{e}");
        assert!(!out.exists());
        assert!(survey_with(Processor::SpacingReset, &src, &cfg(j)).unwrap_err().starts_with(UNSUPPORTED));
    }
    // 비어 있는 설정은 받는다
    assert!(survey_with(Processor::SpacingReset, &src, &cfg(r#"{"indent_rules":{},"marker_scheme_active":false}"#)).is_ok());
}

fn dir_out(dir: &Path, tag: &str) -> PathBuf {
    dir.join(format!("out{}.hwpx", tag.len()))
}

#[test]
fn spacing_reset_leaves_protected_user_designed_tables_alone() {
    let dir = tempfile::tempdir().unwrap();
    // '(단위: 개소)' 캡션이 달린 표 안의 글은 자간을 그대로 둔다
    let tbl = format!(
        r#"<hp:tbl rowCnt="1" colCnt="1"><hp:caption><hp:p><hp:run><hp:t>(단위: 개소)</hp:t></hp:run></hp:p></hp:caption><hp:tr><hp:tc><hp:subList><hp:p>{}</hp:p></hp:subList><hp:cellAddr colAddr="0" rowAddr="0"/><hp:cellSpan colSpan="1" rowSpan="1"/></hp:tc></hp:tr></hp:tbl>"#,
        run("1", "표 안 글")
    );
    let body = format!("{}<hp:p>{}</hp:p>", in_p(&tbl), run("1", "표 밖 글"));
    let src = tmp(dir.path(), "in.hwpx", &doc(&header(CHARPRS), &sec(&body)));
    let out = dir.path().join("out.hwpx");
    assert_eq!(process_with(Processor::SpacingReset, &src, &out, None, &Config::default()).unwrap(), 1);
    let x = entry(&out, "Contents/section0.xml");
    assert!(x.contains(r#"<hp:run charPrIDRef="1"><hp:t>표 안 글</hp:t>"#), "{x}");
    assert!(x.contains(r#"<hp:run charPrIDRef="2"><hp:t>표 밖 글</hp:t>"#), "{x}");
}

#[test]
fn malformed_non_section_xml_fails_the_whole_run_like_python() {
    let dir = tempfile::tempdir().unwrap();
    let z = make_zip(&[
        ("Contents/header.xml", header(CHARPRS).into_bytes()),
        ("Contents/section0.xml", sec("").into_bytes()),
        ("Contents/extra.xml", b"<a><b></a>".to_vec()),
    ]);
    let src = tmp(dir.path(), "in.hwpx", &z);
    for p in [Processor::OverviewLead, Processor::PageNumber, Processor::SpacingReset, Processor::Date, Processor::TwoChar] {
        let out = dir.path().join(format!("{p:?}.out.hwpx"));
        assert!(process_with(p, &src, &out, None, &Config::default()).is_err(), "{p:?}");
        assert!(!out.exists());
    }
}

#[test]
fn format_kind_reports_box_after_title() {
    let d = hwpx_fmt::dom::parse(sec(&format!("{}{}", in_p(&title_table()), in_p(&one_cell(&para(LONG))))).as_bytes()).unwrap();
    let kinds: Vec<_> = d.tables().into_iter().map(|t| rules::format_kind(&d, t).unwrap()).collect();
    assert_eq!(kinds, vec![Some(rules::FormatKind::Title), Some(rules::FormatKind::Box)]);
}

#[test]
fn dom_clone_subtree_copies_tail_and_keeps_original_untouched() {
    let mut d = hwpx_fmt::dom::parse("<a><r x=\"1\"><t>가</t></r>꼬리<z/></a>".as_bytes()).unwrap();
    let r = 1;
    let c = d.clone_subtree(r);
    d.insert_child(0, 1, c);
    d.nodes[c].attrs[0].1 = "2".into();
    let out = String::from_utf8(d.tostring()).unwrap();
    assert!(out.contains("<r x=\"1\"><t>가</t></r>꼬리<r x=\"2\"><t>가</t></r>꼬리<z />") || out.contains("<r x=\"2\"><t>가</t></r>꼬리<r x=\"1\">"), "{out}");
}
