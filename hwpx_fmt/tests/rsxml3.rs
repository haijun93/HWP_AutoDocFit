//! RSXML3 단위 시험: 별표 위첨자·붙임 글꼴·콜론 라벨·빈 줄 서식·강조 낱말. 합성 HWPX 만 쓴다.
//! 순수 함수 기대값은 파이썬 원본(`mark_spans`·`label_of`·`plan`·`leading_marker`·`unify_marker`·`find_blocks`)이 실제로 돌려준 값이다.
use hwpx_fmt::config::{Config, UNSUPPORTED};
use hwpx_fmt::fmt3::{find_blocks, label_of, plan, star_spans, stretch};
use hwpx_fmt::style::{char_em, leading_marker, unify_marker};
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
const FONTS: &str = r#"<hh:fontfaces itemCnt="1"><hh:fontface lang="HANGUL" fontCnt="2"><hh:font id="0" face="바탕" type="TTF"/><hh:font id="1" face="견고딕" type="TTF"/></hh:fontface></hh:fontfaces>"#;
fn charpr(id: usize, height: u32, face: usize) -> String {
    format!(
        r#"<hh:charPr id="{id}" height="{height}"><hh:fontRef hangul="{face}" latin="{face}"/><hh:ratio hangul="100" latin="100"/><hh:spacing hangul="0" latin="0"/></hh:charPr>"#
    )
}
fn header(chars: &str, n: usize) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hh:refList>{FONTS}<hh:charProperties itemCnt="{n}">{chars}</hh:charProperties><hh:paraProperties itemCnt="2"><hh:paraPr id="0"><hh:align horizontal="CENTER" vertical="BASELINE"/></hh:paraPr><hh:paraPr id="1"><hh:align horizontal="LEFT" vertical="BASELINE"/></hh:paraPr></hh:paraProperties></hh:refList></hh:head>"#
    )
}
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
fn t(text: &str, cid: usize) -> String {
    format!(r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="{cid}"><hp:t>{text}</hp:t></hp:run></hp:p>"#)
}

// ---------------------------------------------------------------- 순수 함수(파이썬 기대값)

#[test]
fn star_spans_match_python() {
    let cases: Vec<(&str, Vec<(usize, usize)>)> = vec![
        ("ㅇ 단어* 설명", vec![(4, 5)]),
        ("ㅇ 단어** 설명", vec![(4, 6)]),
        ("ㅇ 세 개***", vec![]),
        ("ㅇ a*b", vec![]),
        ("ㅇ 단어*a", vec![]),
        ("ㅇ 단어*(주)", vec![(4, 5)]),
        ("* 주석 본문", vec![]),
        ("** 둘째 주석", vec![]),
        ("ㅇ 앞에 빈칸 *", vec![]),
        ("ㅇ 𠀀* 서로게이트", vec![(3, 4)]),
        ("ㅇ 단어\u{a0}*", vec![]),
        ("1. 번호* 항목", vec![(5, 6)]),
        ("일반 문장*", vec![]),
        ("ㅇ 별*두*번*", vec![(3, 4), (5, 6), (7, 8)]),
    ];
    for (text, want) in cases {
        assert_eq!(star_spans(text), want, "{text:?}");
    }
}

#[test]
fn label_of_match_python() {
    let cases: Vec<(&str, Option<(usize, usize, usize)>)> = vec![
        ("- 공사명: 현수식", Some((2, 5, 5))),
        ("- 일곱글자라벨: 가", Some((2, 8, 8))),
        ("- 사 업 명: 가", Some((2, 7, 7))),
        ("1) 일시: 가", Some((3, 5, 5))),
        ("* 항세기: 가", Some((2, 5, 5))),
        ("** 난세기명: 나", Some((3, 7, 7))),
        ("- 𠀀𠀁: 가", Some((2, 4, 4))),
        ("일시: 가", None),
        ("- 일\u{a0}시: 가", Some((2, 5, 5))),
    ];
    for (text, want) in cases {
        assert_eq!(label_of(text), want, "{text:?}");
    }
}

#[test]
fn leading_and_unify_marker_match_python() {
    let cases: Vec<(&str, (&str, &str), (&str, &str, &str))> = vec![
        ("□ 제목", ("□", "소제목"), ("□", "소제목", "□")),
        ("ㅁ 제목", ("□", "소제목"), ("□", "소제목", "□")),
        ("12. 소제목", ("12.", "소제목"), ("1.", "소제목", "12.")),
        ("제1장 개요", ("제1장", "장"), ("제1장", "장", "제1장")),
        ("가. 항", ("가", "중제목"), ("가.", "중제목", "가")),
        ("① 원", ("①", ""), ("①", "원문자", "①")),
        ("❶ 원", ("-", "내용"), ("-", "내용", "-")),
        ("** 별", ("**", "부연설명"), ("**", "부연설명", "**")),
        ("(1) 가", ("(1)", "부연설명"), ("(1)", "부연설명", "(1)")),
        ("·점", ("", ""), ("", "", "")),
        ("1.「마포」", ("", ""), ("1.", "소제목", "1.")),
        ("일반", ("", ""), ("", "", "")),
    ];
    for (text, lead, uni) in cases {
        let (m, r) = leading_marker(text);
        assert_eq!((m.as_str(), r), lead, "leading {text:?}");
        let (g, role, raw) = unify_marker(text);
        assert_eq!((g.as_str(), role.as_str(), raw.as_str()), uni, "unify {text:?}");
    }
}

#[test]
fn find_blocks_match_python() {
    let s = |x: &str| Some(x.to_string());
    assert_eq!(find_blocks(&[s("ㅇ a"), s("붙임 1. 계획서 1부."), s("끝.")]), vec![(1, 2)]);
    assert_eq!(find_blocks(&[s("붙임 사업계획서 1부.  끝.")]), vec![(0, 0)]);
    assert_eq!(find_blocks(&[s("붙임 1. 가 1부"), s("2. 나 1부"), s("끝.")]), vec![(0, 2)]);
    assert_eq!(find_blocks(&[s("붙임 : 1. 계획서"), s("2. 참고  끝.")]), vec![(0, 1)]);
    assert_eq!(find_blocks(&[s("붙임 1. 가 1부."), None, s("끝.")]), Vec::<(usize, usize)>::new());
}

fn em(s: &str) -> f64 {
    s.chars().map(char_em).sum()
}

#[test]
fn colon_plan_match_python() {
    let l = |s: &str| s.chars().collect::<Vec<char>>();
    assert_eq!(plan(&l("공사명"), em("공사기간"), true), (0, 50, 100));
    assert_eq!(plan(&l("장소"), em("행사일정"), true), (3, 50, 100));
    assert_eq!(plan(&l("일시"), em("장소명"), false), (1, 50, 100));
    assert_eq!(plan(&l("목적"), em("사업추진배경"), true), (7, 50, 100));
    assert_eq!(stretch(&l("가나다"), 4.0, None, true), (50, 100));
    assert_eq!(stretch(&l("가"), 2.0, None, true), (0, 200));
    assert_eq!(stretch(&l("가나"), 8.0, None, false), (50, 100));
}

// ---------------------------------------------------------------- 끝에서 끝까지

#[test]
fn asterisk_superscript_new_charpr_id_and_split() {
    let dir = tempfile::tempdir().unwrap();
    let h = header(&format!("{}{}", charpr(0, 1000, 0), charpr(1, 1000, 0)), 2);
    let src = tmp(dir.path(), "a.hwpx", &doc(&h, &sec(&t("ㅇ 단어* 설명", 1))));
    let c = Config::default();
    assert_eq!(survey_json(Processor::AsteriskSuper, &src, &c).unwrap(), json!({"Contents/section0.xml": [[0, 1]]}));
    let out = dir.path().join("o.hwpx");
    assert_eq!(process_json(Processor::AsteriskSuper, &src, &out, None, &c).unwrap(), 1);
    let hdr = entry(&out, "Contents/header.xml");
    assert!(hdr.contains(r#"<hh:charPr id="2" height="1000">"#), "{hdr}");
    assert!(hdr.contains("<hh:supscript />") && hdr.contains(r#"itemCnt="3""#), "{hdr}");
    let s = entry(&out, "Contents/section0.xml");
    assert!(s.contains(r#"charPrIDRef="1"><hp:t>ㅇ 단어</hp:t>"#) && s.contains(r#"charPrIDRef="2"><hp:t>*</hp:t>"#) && s.contains(r#"charPrIDRef="1"><hp:t> 설명</hp:t>"#), "{s}");
    // 원본 보존
    assert!(entry(&src, "Contents/section0.xml").contains("ㅇ 단어* 설명"));
    // 이미 위첨자인 별표는 건드리지 않음(다시 돌려도 대상 없음)
    let again = dir.path().join("o2.hwpx");
    assert_eq!(survey_json(Processor::AsteriskSuper, &out, &c).unwrap(), json!({"Contents/section0.xml": []}));
    assert_eq!(process_json(Processor::AsteriskSuper, &out, &again, None, &c).unwrap(), 0);
    assert_eq!(std::fs::read(&out).unwrap(), std::fs::read(&again).unwrap(), "대상이 없으면 원본 복사");
}

#[test]
fn attachment_font_clones_charpr_with_goal_font() {
    let dir = tempfile::tempdir().unwrap();
    let h = header(&format!("{}{}", charpr(0, 1000, 0), charpr(1, 1400, 1)), 2);
    let body = format!("{}{}{}", t("ㅇ 본문", 1), t("붙임 1. 계획서 1부.", 0), t("끝.", 0));
    let src = tmp(dir.path(), "a.hwpx", &doc(&h, &sec(&body)));
    let c = Config::default();
    assert_eq!(survey_json(Processor::AttachFont, &src, &c).unwrap(), json!({"Contents/section0.xml": [[1, 2]]}));
    let out = dir.path().join("o.hwpx");
    assert_eq!(process_json(Processor::AttachFont, &src, &out, None, &c).unwrap(), 2);
    let hdr = entry(&out, "Contents/header.xml");
    // 새 글자 모양은 하나(두 문단이 공유), 높이 14pt, 글꼴은 ㅇ 문장의 견고딕(id 1)
    assert!(hdr.contains(r#"<hh:charPr id="2" height="1400"><hh:fontRef hangul="1" latin="0" />"#), "{hdr}");
    assert!(!hdr.contains(r#"id="3""#) || hdr.matches("<hh:charPr ").count() == 3, "{hdr}");
    let s = entry(&out, "Contents/section0.xml");
    assert_eq!(s.matches(r#"charPrIDRef="2""#).count(), 2, "{s}");
    // 설정의 기호 규칙이 우선한다
    let c2 = cfg(r#"{"symbol_rules": [["ㅇ", 0, "바탕", 10]]}"#);
    let out2 = dir.path().join("o2.hwpx");
    assert_eq!(process_json(Processor::AttachFont, &src, &out2, None, &c2).unwrap(), 2);
    let hdr2 = entry(&out2, "Contents/header.xml");
    assert!(hdr2.contains(r#"height="1000""#) && !hdr2.contains(r#"<hh:charPr id="2" height="1400""#), "{hdr2}");
}

#[test]
fn attachment_font_aligns_left_with_copy_on_write() {
    let dir = tempfile::tempdir().unwrap();
    let h = header(&charpr(0, 1000, 0), 1);
    let p = |text: &str, pp: usize| format!(r#"<hp:p paraPrIDRef="{pp}"><hp:run charPrIDRef="0"><hp:t>{text}</hp:t></hp:run></hp:p>"#);
    let body = format!("{}{}{}", t("ㅇ 본문", 0), p("붙임 1. 계획서 1부.   ", 0), p("끝.", 0));
    let src = tmp(dir.path(), "a.hwpx", &doc(&h, &sec(&body)));
    let out = dir.path().join("o.hwpx");
    assert_eq!(process_json(Processor::AttachFont, &src, &out, None, &Config::default()).unwrap(), 2);
    let hdr = entry(&out, "Contents/header.xml");
    // 가운데 정렬 문단 모양(id 0)은 그대로 두고 왼쪽 정렬 사본(id 2)을 새로 만든다. 두 문단이 사본 하나를 공유.
    assert!(hdr.contains(r#"<hh:paraPr id="0"><hh:align horizontal="CENTER""#), "{hdr}");
    assert!(hdr.contains(r#"<hh:paraPr id="2"><hh:align horizontal="LEFT""#) && hdr.contains(r#"itemCnt="3""#), "{hdr}");
    let s = entry(&out, "Contents/section0.xml");
    assert_eq!(s.matches(r#"paraPrIDRef="2""#).count(), 2, "{s}");
    assert!(s.contains("<hp:t>붙임 1. 계획서 1부.</hp:t>"), "끝 공백 제거: {s}");
}

#[test]
fn colon_label_inserts_gap_and_stretches() {
    let dir = tempfile::tempdir().unwrap();
    let h = header(&charpr(0, 1000, 0), 1);
    let body = format!("{}{}", t("- 일시: 가", 0), t("- 장소명: 나", 0));
    let src = tmp(dir.path(), "a.hwpx", &doc(&h, &sec(&body)));
    let c = Config::default();
    assert_eq!(survey_json(Processor::ColonLabel, &src, &c).unwrap(), json!({"Contents/section0.xml": [0]}));
    let out = dir.path().join("o.hwpx");
    assert_eq!(process_json(Processor::ColonLabel, &src, &out, None, &c).unwrap(), 1);
    let s = entry(&out, "Contents/section0.xml");
    // 파이썬 plan('일시', 3em) = (빈칸 1, 자간 50, 장평 100): 글자 사이 빈칸 1칸
    assert!(s.contains("<hp:t>- </hp:t>") && s.contains("<hp:t>일</hp:t>") && s.contains("<hp:t> </hp:t>") && s.contains("<hp:t>시</hp:t>"), "{s}");
    assert!(s.contains("<hp:t>: 가</hp:t>") && s.contains("<hp:t>- 장소명: 나</hp:t>"), "{s}");
    let hdr = entry(&out, "Contents/header.xml");
    assert!(hdr.contains(r#"<hh:spacing hangul="50" latin="50" />"#), "{hdr}");
    // 끄면 조사 결과가 비고 원본 복사
    let off = cfg(r#"{"colon_label_enabled": false}"#);
    assert_eq!(survey_json(Processor::ColonLabel, &src, &off).unwrap(), json!({"Contents/section0.xml": []}));
}

#[test]
fn emphasis_bold_word_creates_charpr_and_splits_run() {
    let dir = tempfile::tempdir().unwrap();
    let h = header(&charpr(0, 1000, 0), 1);
    let src = tmp(dir.path(), "a.hwpx", &doc(&h, &sec(&t("□ 사업 개요 입니다", 0))));
    let c = cfg(r#"{"emphasis_words": [{"marker": "□", "text": "개요", "bold": true, "size_delta": 200}]}"#);
    assert_eq!(survey_json(Processor::Emphasis, &src, &c).unwrap(), json!({"Contents/section0.xml": [0]}));
    let out = dir.path().join("o.hwpx");
    assert_eq!(process_json(Processor::Emphasis, &src, &out, None, &c).unwrap(), 1);
    let hdr = entry(&out, "Contents/header.xml");
    assert!(hdr.contains(r#"<hh:charPr id="1" height="1200">"#) && hdr.contains("<hh:bold />"), "{hdr}");
    let s = entry(&out, "Contents/section0.xml");
    assert!(s.contains("<hp:t>□ 사업 </hp:t>") && s.contains(r#"charPrIDRef="1"><hp:t>개요</hp:t>"#) && s.contains("<hp:t> 입니다</hp:t>"), "{s}");
    // 프로필이 없으면 조사 결과가 비어 있다
    assert_eq!(survey_json(Processor::Emphasis, &src, &Config::default()).unwrap(), json!({"Contents/section0.xml": []}));
}

#[test]
fn blank_lines_applies_learned_slot() {
    let dir = tempfile::tempdir().unwrap();
    let ph = r#"<hh:paraPr xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" id="9"><hh:align horizontal="RIGHT" vertical="BASELINE" /></hh:paraPr>"#;
    let ch = r#"<hh:charPr xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" id="9" height="500"><hh:fontRef hangul="0" latin="0" /></hh:charPr>"#;
    let c = cfg(&serde_json::to_string(&json!({"blank_line_profile": {"version": 1, "slots": {"ㅇ|ㅇ|0": {"para": ph, "char": ch, "fonts": {}}}, "after": {}}})).unwrap());
    let h = header(&charpr(0, 1000, 0), 1);
    let body = format!("{}{}{}", t("ㅇ 하나", 0), t("", 0), t("ㅇ 둘", 0));
    let src = tmp(dir.path(), "a.hwpx", &doc(&h, &sec(&body)));
    assert_eq!(survey_json(Processor::BlankLines, &src, &c).unwrap(), json!({"Contents/section0.xml": [0]}));
    let out = dir.path().join("o.hwpx");
    assert_eq!(process_json(Processor::BlankLines, &src, &out, None, &c).unwrap(), 1);
    let hdr = entry(&out, "Contents/header.xml");
    // 새 문단 모양·글자 모양은 같은 그룹 끝에 붙고 id 는 (최댓값+1): paraPr 2, charPr 1
    assert!(hdr.contains(r#"<hh:paraPr id="2"><hh:align horizontal="RIGHT""#) && hdr.contains(r#"<hh:charPr id="1" height="500">"#), "{hdr}");
    let s = entry(&out, "Contents/section0.xml");
    assert!(s.contains(r#"<hp:p paraPrIDRef="2"><hp:run charPrIDRef="1">"#), "{s}");
}

// ---------------------------------------------------------------- 미지원 입력 거부

#[test]
fn unsupported_inputs_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let h = header(&charpr(0, 1000, 0), 1);
    let src = tmp(dir.path(), "a.hwpx", &doc(&h, &sec(&t("ㅇ 단어*", 0))));
    let out = dir.path().join("o.hwpx");
    for bad in [
        r#"{"unknown": 1}"#,
        r#"{"symbol_rules": {"a": 1}}"#,
        r#"{"symbol_rules": [["ㅇ", 0, 5, 12]]}"#,
        r#"{"symbol_rules": [["ㅇ", 0, "x", "12"]]}"#,
        r#"{"std_symbols": "yes"}"#,
        r#"{"colon_label_enabled": 1}"#,
        r#"{"ratio_reduce": "n"}"#,
        r#"{"blank_line_profile": [1]}"#,
        r#"{"emphasis_words": [1]}"#,
    ] {
        let e = Config::from_json(&serde_json::from_str(bad).unwrap()).unwrap_err();
        assert!(e.starts_with(UNSUPPORTED), "{bad}: {e}");
    }
    // 서식표 판별에 반영하지 못하는 설정(빈 줄 서식)
    let c = cfg(r#"{"title_owner_text": "담당관", "blank_line_profile": {"slots": {"a|b|0": {"para": "<a/>", "char": "<a/>"}}}}"#);
    for p in [Processor::BlankLines] {
        assert!(survey_json(p, &src, &c).unwrap_err().starts_with(UNSUPPORTED));
        assert!(process_json(p, &src, &out, None, &c).unwrap_err().starts_with(UNSUPPORTED));
        assert!(!out.exists());
    }
    // 프로필 XML 의 이름공간이 대상 문서에 없으면 파이썬과 달리 거부한다
    let ec = cfg(r#"{"emphasis_words": [{"marker": "ㅇ", "text": "단어", "font": "x", "bold": false, "size_delta": 0, "fonts": {"hangul": "<q:font xmlns:q=\"urn:other\" id=\"0\" face=\"x\" type=\"TTF\"/>"}}]}"#);
    let e = process_json(Processor::Emphasis, &src, &out, None, &ec).unwrap_err();
    assert!(e.starts_with(UNSUPPORTED), "{e}");
    assert!(!out.exists());
}

#[test]
fn input_file_untouched_and_same_path_refused() {
    let dir = tempfile::tempdir().unwrap();
    let h = header(&charpr(0, 1000, 0), 1);
    let src = tmp(dir.path(), "a.hwpx", &doc(&h, &sec(&t("ㅇ 단어*", 0))));
    let before = std::fs::read(&src).unwrap();
    let e = process_json(Processor::AsteriskSuper, &src, &src, None, &Config::default()).unwrap_err();
    assert!(e.contains("같은 파일"), "{e}");
    assert_eq!(std::fs::read(&src).unwrap(), before);
}
