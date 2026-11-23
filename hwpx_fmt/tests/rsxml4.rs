//! RSXML4 단위 시험: 표 칸 너비·기본 표 서식(+정렬 재확인)·연결 부호 탭 정렬. 합성 HWPX 만 쓴다.
//! 순수 함수 기대값은 파이썬 원본(`connector_tabs.split_connector`·`table_width.plan_widths`·`cap_floors`·내장 `sum`)이 실제로 돌려준 값이다.
use hwpx_fmt::config::{Config, UNSUPPORTED};
use hwpx_fmt::conn::split_connector;
use hwpx_fmt::dom;
use hwpx_fmt::style::{py_sum, PySum};
use hwpx_fmt::tbl::{cap_floors, plan_widths, pyint, section_width};
use hwpx_fmt::tstyle::parse_style;
use hwpx_fmt::{process_json, survey_json, zipio, Processor};
use serde_json::json;
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
const HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8"?><hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hh:refList><hh:fontfaces itemCnt="1"><hh:fontface lang="HANGUL" fontCnt="1"><hh:font id="0" face="바탕" type="TTF"/></hh:fontface></hh:fontfaces><hh:borderFills itemCnt="1"><hh:borderFill id="1"><hh:leftBorder type="SOLID"/></hh:borderFill></hh:borderFills><hh:tabProperties itemCnt="1"><hh:tabPr id="0" autoTabLeft="0" autoTabRight="0"/></hh:tabProperties><hh:charProperties itemCnt="1"><hh:charPr id="0" height="1000"><hh:fontRef hangul="0"/><hh:ratio hangul="100"/><hh:spacing hangul="0"/></hh:charPr></hh:charProperties><hh:paraProperties itemCnt="1"><hh:paraPr id="0" tabPrIDRef="0"><hh:align horizontal="CENTER" vertical="BASELINE"/></hh:paraPr></hh:paraProperties></hh:refList></hh:head>"#;
fn doc(s: &str) -> Vec<u8> {
    make_zip(&[("mimetype", b"application/hwp+zip".to_vec()), ("Contents/header.xml", HEADER.as_bytes().to_vec()), ("Contents/section0.xml", s.as_bytes().to_vec())])
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
const PAGE: &str = r#"<hp:p><hp:run><hp:secPr><hp:pagePr landscape="WIDELY" width="59528" height="84188"><hp:margin left="8504" right="8504"/></hp:pagePr></hp:secPr></hp:run></hp:p>"#;
fn cell(text: &str, r: usize, c: usize, w: usize) -> String {
    format!(
        r#"<hp:tc borderFillIDRef="1"><hp:subList><hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>{text}</hp:t></hp:run></hp:p></hp:subList><hp:cellAddr colAddr="{c}" rowAddr="{r}"/><hp:cellSpan colSpan="1" rowSpan="1"/><hp:cellSz width="{w}" height="2000"/></hp:tc>"#
    )
}
fn table(rows: &[Vec<String>], width: usize) -> String {
    let trs: String = rows.iter().map(|r| format!("<hp:tr>{}</hp:tr>", r.concat())).collect();
    format!(
        r#"<hp:p><hp:run><hp:tbl rowCnt="{}" colCnt="{}"><hp:sz width="{width}" widthRelTo="ABSOLUTE"/><hp:pos treatAsChar="1"/><hp:outMargin left="0" right="0"/><hp:inMargin left="510" right="510"/>{trs}</hp:tbl></hp:run></hp:p>"#,
        rows.len(),
        rows[0].len()
    )
}
fn para(text: &str) -> String {
    format!(r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>{text}</hp:t></hp:run></hp:p>"#)
}
fn two_col() -> String {
    table(&[vec![cell("가나다라", 0, 0, 3000), cell("마바사아", 0, 1, 3000)], vec![cell("x", 1, 0, 3000), cell("y", 1, 1, 3000)]], 6000)
}
fn widths(xml: &str) -> Vec<u32> {
    let mut v = vec![];
    for part in xml.split(r#"<hp:cellSz width=""#).skip(1) {
        v.push(part.split('"').next().unwrap().parse().unwrap());
    }
    v
}

// ---------------------------------------------------------------- 순수 함수(파이썬 기대값)

#[test]
fn py_sum_is_compensated_like_python_3_12() {
    // 파이썬 3.14: sum([0.1]*10) == 1.0, sum([0.1,0.2,0.3,0.7]) == 1.3 (단순 덧셈은 0.9999999999999999)
    assert_eq!(py_sum(vec![0.1; 10]), 1.0);
    assert_eq!(py_sum(vec![0.1, 0.2, 0.3, 0.7]), 1.3);
    // 정수 항목만이면 정수 합, 앞의 정수는 첫 실수에 더해진다
    let mut s = PySum::new();
    s.add_int(3);
    s.add_int(4);
    assert_eq!(s.total(), 7.0);
    s.add(0.5);
    s.add_int(1);
    assert_eq!(s.total(), 8.5);
}

#[test]
fn split_connector_matches_python() {
    let s = |t: &str| split_connector(t).map(|(a, b, c)| (a, b, c));
    assert_eq!(s("제1장 총칙 ........ 3"), Some(("제1장 총칙".into(), "........".into(), "3".into())));
    assert_eq!(s("가나다    라마바"), Some(("가나다".into(), "    ".into(), "라마바".into())));
    assert_eq!(s("가나다  라마"), None);
    assert_eq!(s("일    시: 2026"), None); // 글자 사이를 띄운 라벨
    assert_eq!(s("제목      (총무과)"), None); // 오른쪽 끝 부서 표시
    assert_eq!(s("가 ..... 1 ..... 2"), None);
    assert_eq!(s("가나다\u{3000}\u{3000}라마"), Some(("가나다".into(), "\u{3000}\u{3000}".into(), "라마".into())));
    assert_eq!(s("가 ----- 나"), Some(("가".into(), "-----".into(), "나".into())));
    assert_eq!(s("...... 가"), None);
    assert_eq!(s("가 ..... "), None);
}

#[test]
fn plan_widths_rounds_half_to_even_like_python() {
    // 파이썬 round: 21260.5 → 21260 (짝수 쪽). 합 보정은 첫 최댓값에.
    assert_eq!(plan_widths(&[0.5, 0.5], &[1000, 1000], 42521), vec![21261, 21260]);
    assert_eq!(plan_widths(&[1.0 / 6.0; 6], &[100; 6], 59530), vec![9920, 9922, 9922, 9922, 9922, 9922]);
    assert_eq!(plan_widths(&[0.1, 0.9], &[5000, 1000], 10000), vec![5000, 5000]);
    assert_eq!(cap_floors(&[5000, 6000, 100], 10000), vec![4700, 4700, 100]);
    assert_eq!(cap_floors(&[100, 200], 10000), vec![100, 200]);
}

#[test]
fn pyint_rejects_ambiguous_forms() {
    assert_eq!(pyint(" 12 ").unwrap(), Some(12));
    assert_eq!(pyint("+7").unwrap(), Some(7));
    assert_eq!(pyint("abc").unwrap(), None);
    assert_eq!(pyint("").unwrap(), None);
    assert!(pyint("５０００").unwrap_err().starts_with(UNSUPPORTED)); // 전각 숫자: 파이썬은 받아들임
    assert!(pyint("5_000").unwrap_err().starts_with(UNSUPPORTED));
}

#[test]
fn section_width_follows_first_pagepr() {
    let d = dom::parse(sec(PAGE).as_bytes()).unwrap();
    assert_eq!(section_width(&d, None).unwrap(), Some(59528 - 8504 * 2));
    let none = dom::parse(sec("<hp:p/>").as_bytes()).unwrap();
    assert_eq!(section_width(&none, None).unwrap(), None);
    assert_eq!(section_width(&none, Some(42520)).unwrap(), Some(42520));
    let land = dom::parse(sec(&PAGE.replace("WIDELY", "NARROWLY")).as_bytes()).unwrap();
    assert_eq!(section_width(&land, None).unwrap(), Some(84188 - 8504 * 2));
}

// ---------------------------------------------------------------- 표 칸 너비

#[test]
fn table_width_splits_by_text_length_and_keeps_input() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = doc(&sec(&format!("{PAGE}{}", two_col())));
    let src = tmp(dir.path(), "in.hwpx", &bytes);
    let dst = dir.path().join("out.hwpx");
    let p = Processor::parse("table_width").unwrap();
    let sel = survey_json(p, &src, &Config::default()).unwrap();
    assert_eq!(sel, json!({"Contents/section0.xml": [0]}));
    let n = process_json(p, &src, &dst, Some(&sel), &Config::default()).unwrap();
    assert_eq!(n, 1);
    let out = entry(&dst, "Contents/section0.xml");
    // 본문 폭 42520, 두 열 글자 수 같음 → 21260 씩, 표 폭 42520
    assert_eq!(&widths(&out)[..2], &[21260, 21260]);
    assert!(out.contains(r#"<hp:sz width="42520""#));
    assert_eq!(std::fs::read(&src).unwrap(), bytes);
}

#[test]
fn table_width_no_pagepr_is_not_a_target_and_copies() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = doc(&sec(&two_col()));
    let src = tmp(dir.path(), "in.hwpx", &bytes);
    let dst = dir.path().join("out.hwpx");
    let p = Processor::parse("표너비_hwpx_처리").unwrap();
    assert_eq!(survey_json(p, &src, &Config::default()).unwrap(), json!({"Contents/section0.xml": []}));
    assert_eq!(process_json(p, &src, &dst, None, &Config::default()).unwrap(), 0);
    assert_eq!(std::fs::read(&dst).unwrap(), bytes);
}

#[test]
fn table_width_skips_title_table() {
    let dir = tempfile::tempdir().unwrap();
    let title = table(&[vec![cell("보고서 제목", 0, 0, 5000)], vec![cell("담당: 홍길동 ☎ 3153-8660", 1, 0, 5000)]], 5000);
    let src = tmp(dir.path(), "in.hwpx", &doc(&sec(&format!("{PAGE}{title}"))));
    let sel = survey_json(Processor::TableWidth, &src, &Config::default()).unwrap();
    assert_eq!(sel, json!({"Contents/section0.xml": []})); // 한 열 표는 대상이 아니다
}

// ---------------------------------------------------------------- 연결 부호 탭

#[test]
fn connector_tab_makes_left_tab_and_new_tab_pr() {
    let dir = tempfile::tempdir().unwrap();
    let body = format!("{PAGE}{}{}{}", para("제1장 총칙 ........ 3"), para("제2장 목적 ........ 12"), para("그냥 글"));
    let src = tmp(dir.path(), "in.hwpx", &doc(&sec(&body)));
    let dst = dir.path().join("out.hwpx");
    let p = Processor::parse("connector_tab").unwrap();
    let sel = survey_json(p, &src, &Config::default()).unwrap();
    assert_eq!(sel, json!({"Contents/section0.xml": [0]}));
    assert_eq!(process_json(p, &src, &dst, Some(&sel), &Config::default()).unwrap(), 2);
    let s = entry(&dst, "Contents/section0.xml");
    assert_eq!(s.matches("<hp:tab ").count(), 2);
    assert!(s.contains(r#"leader="2" type="1""#));
    assert!(s.contains("제1장 총칙<hp:tab"));
    let h = entry(&dst, "Contents/header.xml");
    assert!(h.contains(r#"<hh:tabProperties itemCnt="2">"#));
    assert!(h.contains(r#"leader="DASH""#));
    assert!(h.contains(r#"<hh:paraProperties itemCnt="2">"#)); // 문단 모양 사본 하나
    assert!(h.contains(r#"tabPrIDRef="1""#));
}

#[test]
fn connector_tab_disabled_flag_and_single_paragraph() {
    let dir = tempfile::tempdir().unwrap();
    let src = tmp(dir.path(), "in.hwpx", &doc(&sec(&format!("{PAGE}{}{}", para("가 ..... 1"), para("나 ..... 2")))));
    let off = cfg(r#"{"connector_tab_enabled": false}"#);
    assert_eq!(survey_json(Processor::ConnectorTab, &src, &off).unwrap(), json!({"Contents/section0.xml": []}));
    let one = tmp(dir.path(), "one.hwpx", &doc(&sec(&format!("{PAGE}{}", para("가 ..... 1")))));
    assert_eq!(survey_json(Processor::ConnectorTab, &one, &Config::default()).unwrap(), json!({"Contents/section0.xml": []}));
}

// ---------------------------------------------------------------- 기본 표 서식

fn style_json() -> serde_json::Value {
    json!({
        "version": 1, "source": "t", "rows": 2, "cols": 2, "header_rows": 1,
        "grid": [[0, 1], [2, 3]],
        "cells": [
            {"border": "b0", "char": "c0", "align": "CENTER", "valign": "CENTER", "line": null},
            {"border": "b0", "char": "c0", "align": "CENTER", "valign": "CENTER", "line": null},
            {"border": "b0", "char": "c1", "align": "LEFT", "valign": "CENTER", "line": null},
            {"border": "b0", "char": "c1", "align": "LEFT", "valign": "CENTER", "line": null}
        ],
        "borders": {"b0": "<hh:borderFill xmlns:hh=\"http://www.hancom.co.kr/hwpml/2011/head\" threeD=\"0\"><hh:leftBorder type=\"DOUBLE\"/><hh:rightBorder type=\"DOUBLE\"/><hh:topBorder type=\"DOUBLE\"/><hh:bottomBorder type=\"DOUBLE\"/></hh:borderFill>"},
        "chars": {"c0": {"height": 1300, "bold": true, "fonts": {}, "ratio": {"hangul": "95"}, "spacing": {"hangul": "-3"}},
                  "c1": {"height": 1200, "bold": false, "fonts": {}, "ratio": {}, "spacing": {}}},
        "table_border": null, "in_margin": null
    })
}

#[test]
fn table_style_applies_profile_and_adds_header_entries() {
    let dir = tempfile::tempdir().unwrap();
    let t = table(&[vec![cell("구분", 0, 0, 3000), cell("내용", 0, 1, 3000)], vec![cell("가", 1, 0, 3000), cell("나", 1, 1, 3000)]], 6000);
    let src = tmp(dir.path(), "in.hwpx", &doc(&sec(&format!("{}{}", para("□ 추진 배경"), t))));
    let dst = dir.path().join("out.hwpx");
    let c = Config::from_json(&json!({"table_style": style_json()})).unwrap();
    let p = Processor::parse("table_style").unwrap();
    let sel = survey_json(p, &src, &c).unwrap();
    assert_eq!(sel, json!({"Contents/section0.xml": [0]}));
    assert_eq!(process_json(p, &src, &dst, Some(&sel), &c).unwrap(), 1);
    let h = entry(&dst, "Contents/header.xml");
    assert!(h.contains(r#"<hh:borderFills itemCnt="2">"#));
    assert!(h.contains(r#"<hh:borderFill threeD="0" id="2">"#));
    assert!(h.contains(r#"<hh:charProperties itemCnt="3">"#)); // 머리글 글자 + 본문 글자 사본
    let s = entry(&dst, "Contents/section0.xml");
    assert!(s.contains(r#"<hp:tc borderFillIDRef="2">"#));
    // 정렬 재확인(2차 호출)은 같은 처리기다. 풀은 이번 실행에서 만든 테두리만 재사용하므로(파이썬과 같음) 한 개 더 붙는다
    let p2 = Processor::parse("table_style_align").unwrap();
    let dst2 = dir.path().join("out2.hwpx");
    let sel2 = survey_json(p2, &dst, &c).unwrap();
    process_json(p2, &dst, &dst2, Some(&sel2), &c).unwrap();
    assert!(entry(&dst2, "Contents/header.xml").contains(r#"<hh:borderFills itemCnt="3">"#));
}

#[test]
fn table_style_without_profile_does_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let t = table(&[vec![cell("구분", 0, 0, 3000), cell("내용", 0, 1, 3000)], vec![cell("가", 1, 0, 3000), cell("나", 1, 1, 3000)]], 6000);
    let bytes = doc(&sec(&t));
    let src = tmp(dir.path(), "in.hwpx", &bytes);
    let dst = dir.path().join("out.hwpx");
    let p = Processor::TableStyle;
    assert_eq!(survey_json(p, &src, &Config::default()).unwrap(), json!({"Contents/section0.xml": []}));
    assert_eq!(process_json(p, &src, &dst, None, &Config::default()).unwrap(), 0);
    assert_eq!(std::fs::read(&dst).unwrap(), bytes);
}

#[test]
fn table_style_excludes_head_block_table_before_body() {
    let dir = tempfile::tempdir().unwrap();
    let approve = table(&[vec![cell("담당", 0, 0, 3000), cell("팀장", 0, 1, 3000)], vec![cell("", 1, 0, 3000), cell("", 1, 1, 3000)]], 6000);
    let data = table(&[vec![cell("구분", 0, 0, 3000), cell("내용", 0, 1, 3000)], vec![cell("가", 1, 0, 3000), cell("나", 1, 1, 3000)]], 6000);
    let src = tmp(dir.path(), "in.hwpx", &doc(&sec(&format!("{approve}{}{data}", para("□ 추진 배경")))));
    let c = Config::from_json(&json!({"table_style": style_json()})).unwrap();
    // 결재란(본문 시작 전) 표는 빼고 본문 뒤 표만 대상(요소 순서 번호 1)
    assert_eq!(survey_json(Processor::TableStyle, &src, &c).unwrap(), json!({"Contents/section0.xml": [1]}));
}

#[test]
fn table_style_profile_validation() {
    // 파이썬 valid_style 이 거짓인 모양은 '프로필 없음'(None), 형식 오류는 미지원 입력
    let mut bad_version = style_json();
    bad_version["version"] = json!(2);
    assert!(parse_style(&bad_version).unwrap().is_none());
    let mut bad_rows = style_json();
    bad_rows["rows"] = json!(9);
    assert!(parse_style(&bad_rows).unwrap().is_none());
    assert!(parse_style(&json!({})).unwrap().is_none());
    assert!(parse_style(&style_json()).unwrap().is_some());
    let mut wrong = style_json();
    wrong["rows"] = json!("2");
    assert!(parse_style(&wrong).unwrap_err().starts_with(UNSUPPORTED));
}

#[test]
fn unsupported_inputs_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let src = tmp(dir.path(), "in.hwpx", &doc(&sec(&format!("{PAGE}{}", two_col()))));
    let dst = dir.path().join("out.hwpx");
    let owner = cfg(r#"{"title_owner_text": "담당관"}"#);
    for p in [Processor::TableWidth, Processor::TableStyle, Processor::TableStyleAlign] {
        assert!(survey_json(p, &src, &owner).unwrap_err().starts_with(UNSUPPORTED));
        assert!(process_json(p, &src, &dst, None, &owner).unwrap_err().starts_with(UNSUPPORTED));
    }
    assert!(!dst.exists());
    assert!(Config::from_json(&json!({"connector_tab_enabled": 1})).unwrap_err().starts_with(UNSUPPORTED));
    assert!(Config::from_json(&json!({"table_style": [1]})).unwrap_err().starts_with(UNSUPPORTED));
    let planned = cfg(r#"{"standard_margin_planned": true}"#);
    assert!(survey_json(Processor::ConnectorTab, &src, &planned).unwrap_err().starts_with(UNSUPPORTED));
    assert!(survey_json(Processor::TableWidth, &src, &planned).is_ok()); // 표 칸 너비는 쪽 정보 본문 폭만 쓴다
    assert!(Config::from_json(&json!({"새_설정": 1})).unwrap_err().starts_with(UNSUPPORTED));
    // 속성 값에 전각 숫자가 있으면 파이썬 int() 해석과 갈릴 수 있어 거부
    let weird = table(&[vec![cell("가", 0, 0, 3000), cell("나", 0, 1, 3000)]], 6000).replace(r#"cellSz width="3000""#, r#"cellSz width="３０００""#);
    let src2 = tmp(dir.path(), "w.hwpx", &doc(&sec(&format!("{PAGE}{weird}"))));
    // 조사는 폭 속성을 읽지 않으므로 성공하고, 처리에서 거부한다(출력 파일 없음)
    let sel = survey_json(Processor::TableWidth, &src2, &Config::default()).unwrap();
    let dst2 = dir.path().join("w.out.hwpx");
    assert!(process_json(Processor::TableWidth, &src2, &dst2, Some(&sel), &Config::default()).unwrap_err().starts_with(UNSUPPORTED));
    assert!(!dst2.exists());
}

#[test]
fn processor_names_and_aliases() {
    for n in ["table_width", "표너비_hwpx_처리", "table_style", "기본표서식_hwpx_처리", "table_style_align", "connector_tab", "연결부호탭_hwpx_처리"] {
        assert!(Processor::parse(n).is_some_and(|p| p.is_rsxml4()), "{n}");
    }
    assert!(Processor::parse("date").is_some_and(|p| !p.is_rsxml4()));
}
