//! hwpx_fmt 단위·통합 시험(합성 HWPX만 사용). 파이썬과의 차이 0 시험은 scripts/rsxml_parity.py.
use hwpx_fmt::{dom, process, rules, selections_from_json, selections_to_json, survey, zipio, Processor};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

const NS: &str = r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section""#;
const HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8"?><hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head"/>"#;

fn cell(text: &str, row: usize, col: usize) -> String {
    format!(
        r#"<hp:tc><hp:subList><hp:p><hp:run><hp:t>{text}</hp:t></hp:run></hp:p></hp:subList><hp:cellAddr colAddr="{col}" rowAddr="{row}"/><hp:cellSpan colSpan="1" rowSpan="1"/></hp:tc>"#
    )
}
fn table(rows: &[&[&str]]) -> String {
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let trs: String = rows
        .iter()
        .enumerate()
        .map(|(r, row)| format!("<hp:tr>{}</hp:tr>", row.iter().enumerate().map(|(c, t)| cell(t, r, c)).collect::<String>()))
        .collect();
    format!(r#"<hp:tbl rowCnt="{}" colCnt="{cols}">{trs}</hp:tbl>"#, rows.len())
}
fn section(tables: &[String]) -> String {
    let body: String = tables.iter().map(|t| format!("<hp:p><hp:run>{t}</hp:run></hp:p>")).collect();
    format!(r#"<?xml version="1.0" encoding="UTF-8"?><hs:sec {NS}>{body}</hs:sec>"#)
}
fn make_zip(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (n, d) in entries {
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        z.start_file(*n, opts).unwrap();
        z.write_all(d).unwrap();
    }
    z.finish().unwrap().into_inner()
}
fn hwpx(sec: &str) -> Vec<u8> {
    make_zip(&[("mimetype", b"application/hwp+zip".to_vec()), ("Contents/header.xml", HEADER.as_bytes().to_vec()), ("Contents/section0.xml", sec.as_bytes().to_vec())])
}
fn write_tmp(dir: &Path, name: &str, data: &[u8]) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, data).unwrap();
    p
}
fn section_text(path: &Path) -> String {
    let c = zipio::read_hwpx_safely(path).unwrap();
    String::from_utf8(c.into_iter().find(|(n, _)| n == "Contents/section0.xml").unwrap().1).unwrap()
}

#[test]
fn dom_keeps_text_tail_entities_and_drops_comments() {
    let d = dom::parse("<a x=\"1&amp;2\">앞<b/>뒤<!--c--><c>&lt;t&gt;<![CDATA[cd]]></c></a>".as_bytes()).unwrap();
    assert_eq!(d.nodes[0].text.as_deref(), Some("앞"));
    assert_eq!(d.nodes[1].tail.as_deref(), Some("뒤"));
    assert_eq!(d.nodes[2].text.as_deref(), Some("<t>cd"));
    assert_eq!(d.nodes[0].attrs[0].1, "1&2");
    let out = String::from_utf8(d.tostring()).unwrap();
    assert!(out.contains("<a x=\"1&amp;2\">앞<b />뒤<c>&lt;t&gt;cd</c></a>"), "{out}");
    assert_eq!(d.nodes[0].end, 3);
}

#[test]
fn dom_rejects_dtd_invalid_chars_and_malformed() {
    assert!(dom::parse(b"<!DOCTYPE a [<!ENTITY x \"y\">]><a>&x;</a>").is_err());
    assert!(dom::parse("<a>\u{1c}</a>".as_bytes()).is_err());
    assert!(dom::parse(b"<a>&#1;</a>").is_err());
    assert!(dom::parse(b"<a><b></a>").is_err());
    assert!(dom::parse(b"<a/><b/>").is_err());
    assert!(dom::parse(b"").is_err());
}

#[test]
fn date_cell_rule_matches_python_examples() {
    for t in ["’26. 10. 10.(토)", "2026. 10. 10.", "2026년 10월 10일(토) 14:00", "26.10.10.", "10. 6.(화) 10:00 ~"] {
        assert!(rules::is_date_cell(t), "{t}");
    }
    for t in ["관광정책과장: 조희옥 ☎3153-8660", "구분", "", "2026. 10. 10.부터 민원 안내 서비스를 시범 운영하여 대기 시간을 줄임"] {
        assert!(!rules::is_date_cell(t), "{t}");
    }
}

#[test]
fn python_whitespace_set_includes_control_separators() {
    assert!(rules::py_ws('\u{1c}') && rules::py_ws('\u{a0}') && rules::py_ws('\u{3000}'));
    assert!(!rules::py_ws('\u{200b}'));
    assert_eq!(rules::py_strip("\u{3000} 구분 \t"), "구분");
}

#[test]
fn twochar_inserts_one_space_and_keeps_input() {
    let dir = tempfile::tempdir().unwrap();
    let src = write_tmp(dir.path(), "in.hwpx", &hwpx(&section(&[table(&[&["구분", "내용"], &["10", "1부"], &["구분란", "ab"]])])));
    let before = std::fs::read(&src).unwrap();
    let out = dir.path().join("out.hwpx");
    assert_eq!(process(Processor::TwoChar, &src, &out, None).unwrap(), 2);
    let s = section_text(&out);
    assert!(s.contains("<hp:t>구 분</hp:t>") && s.contains("<hp:t>내 용</hp:t>"), "{s}");
    assert!(s.contains("<hp:t>구분란</hp:t>") && s.contains("<hp:t>1부</hp:t>"));
    assert_eq!(std::fs::read(&src).unwrap(), before, "원본은 바뀌면 안 된다");
    assert!(!dir.path().join("out.hwpx.tmp").exists());
}

#[test]
fn twochar_skips_protected_format_and_spaced() {
    let dir = tempfile::tempdir().unwrap();
    // 장식 막대 표(글 칸이 모든 행을 덮고 빈 칸이 있다)는 보호, 붙임 표는 서식 표, '구 분'은 이미 빈칸이 있다
    let bar = r#"<hp:tbl rowCnt="2" colCnt="2"><hp:tr><hp:tc><hp:subList><hp:p><hp:run><hp:t>구분</hp:t></hp:run></hp:p></hp:subList><hp:cellAddr colAddr="0" rowAddr="0"/><hp:cellSpan colSpan="1" rowSpan="2"/></hp:tc><hp:tc><hp:subList><hp:p><hp:run><hp:t></hp:t></hp:run></hp:p></hp:subList><hp:cellAddr colAddr="1" rowAddr="0"/><hp:cellSpan colSpan="1" rowSpan="1"/></hp:tc></hp:tr><hp:tr><hp:tc><hp:subList><hp:p><hp:run><hp:t></hp:t></hp:run></hp:p></hp:subList><hp:cellAddr colAddr="1" rowAddr="1"/><hp:cellSpan colSpan="1" rowSpan="1"/></hp:tc></hp:tr></hp:tbl>"#.to_string();
    let attach = table(&[&["붙임 1", "서류"]]);
    let spaced = table(&[&["구 분", "비고"]]);
    let src = write_tmp(dir.path(), "in.hwpx", &hwpx(&section(&[bar, attach, spaced])));
    let out = dir.path().join("out.hwpx");
    assert_eq!(process(Processor::TwoChar, &src, &out, None).unwrap(), 1); // '비고' 하나뿐
    let s = section_text(&out);
    assert!(s.contains("<hp:t>구분</hp:t>") && s.contains("<hp:t>서류</hp:t>") && s.contains("<hp:t>비 고</hp:t>"), "{s}");
}

#[test]
fn date_cells_become_squeeze_and_rerun_is_noop_copy() {
    let dir = tempfile::tempdir().unwrap();
    let src = write_tmp(dir.path(), "in.hwpx", &hwpx(&section(&[table(&[&["2026. 10. 10.", "구분"]]), table(&[&["x"]])])));
    let sel = survey(Processor::Date, &src).unwrap();
    assert_eq!(sel, vec![("Contents/section0.xml".to_string(), vec![0])]);
    let out = dir.path().join("out.hwpx");
    assert_eq!(process(Processor::Date, &src, &out, Some(&sel)).unwrap(), 1);
    let s = section_text(&out);
    assert_eq!(s.matches("lineWrap=\"SQUEEZE\"").count(), 1, "{s}");
    // 이미 한 줄이면 대상이 없어 원본 바이트 그대로 복사
    let out2 = dir.path().join("out2.hwpx");
    assert_eq!(process(Processor::Date, &out, &out2, None).unwrap(), 0);
    assert_eq!(std::fs::read(&out).unwrap(), std::fs::read(&out2).unwrap());
}

#[test]
fn no_tables_or_targets_copies_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let src = write_tmp(dir.path(), "in.hwpx", &hwpx(&section(&[])));
    let out = dir.path().join("o.hwpx");
    assert_eq!(process(Processor::TwoChar, &src, &out, None).unwrap(), 0);
    assert_eq!(std::fs::read(&src).unwrap(), std::fs::read(&out).unwrap());
}

#[test]
fn refuses_same_input_and_output() {
    let dir = tempfile::tempdir().unwrap();
    let src = write_tmp(dir.path(), "in.hwpx", &hwpx(&section(&[table(&[&["구분"]])])));
    assert!(process(Processor::TwoChar, &src, &src, None).is_err());
}

#[test]
fn selections_json_roundtrip() {
    let s = vec![("Contents/section10.xml".to_string(), vec![1, 2]), ("Contents/section2.xml".to_string(), vec![])];
    let back = selections_from_json(&selections_to_json(&s)).unwrap();
    assert_eq!(back, s, "구역 순서가 유지되어야 한다(preserve_order)");
    assert!(selections_from_json(&serde_json::json!([1])).is_err());
}

#[test]
fn zip_safety_rejects_bad_archives() {
    let dir = tempfile::tempdir().unwrap();
    let sec = section(&[]).into_bytes();
    let h = HEADER.as_bytes().to_vec();
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("traversal", make_zip(&[("Contents/header.xml", h.clone()), ("Contents/section0.xml", sec.clone()), ("../evil.xml", b"x".to_vec())])),
        ("absolute", make_zip(&[("Contents/header.xml", h.clone()), ("Contents/section0.xml", sec.clone()), ("/abs.xml", b"x".to_vec())])),
        ("drive", make_zip(&[("Contents/header.xml", h.clone()), ("Contents/section0.xml", sec.clone()), ("C:/w.xml", b"x".to_vec())])),
        ("no_header", make_zip(&[("Contents/section0.xml", sec.clone())])),
        ("no_section", make_zip(&[("Contents/header.xml", h.clone())])),
        ("not_zip", b"not a zip".to_vec()),
        ("bomb", make_zip(&[("Contents/header.xml", h.clone()), ("Contents/section0.xml", sec.clone()), ("bomb.bin", vec![0u8; 40 * 1024 * 1024])])),
    ];
    for (name, data) in cases {
        let p = write_tmp(dir.path(), &format!("{name}.hwpx"), &data);
        assert!(zipio::read_hwpx_safely(&p).is_err(), "{name} 은 거부해야 한다");
        let out = dir.path().join(format!("{name}.out.hwpx"));
        assert!(process(Processor::TwoChar, &p, &out, None).is_err());
        assert!(!out.exists(), "{name}: 실패한 처리가 출력 파일을 남겼다");
    }
}

#[test]
fn bad_table_attribute_fails_like_python_valueerror() {
    let dir = tempfile::tempdir().unwrap();
    let sec = section(&[r#"<hp:tbl rowCnt="x" colCnt="2"><hp:tr/></hp:tbl>"#.to_string()]);
    let src = write_tmp(dir.path(), "in.hwpx", &hwpx(&sec));
    assert!(survey(Processor::TwoChar, &src).is_err());
}

#[test]
fn mimetype_is_stored_and_order_kept() {
    let dir = tempfile::tempdir().unwrap();
    let src = write_tmp(dir.path(), "in.hwpx", &hwpx(&section(&[table(&[&["구분"]])])));
    let out = dir.path().join("o.hwpx");
    process(Processor::TwoChar, &src, &out, None).unwrap();
    let mut z = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
    assert_eq!(z.by_index(0).unwrap().name().unwrap(), "mimetype");
    assert_eq!(z.by_index(0).unwrap().compression(), zip::CompressionMethod::Stored);
    assert_eq!(z.by_index(1).unwrap().compression(), zip::CompressionMethod::Deflated);
}
