//! HWP 5.0 Rust 직접 추출 모듈(`hwp5`) 통합 테스트.
//!
//! 검증 범위:
//! 1. 최소 합성 HWP 생성 도우미 (FileHeader, DocInfo, Section0 1개, 문단 3개, 표 1개)
//! 2. 일반 문단 및 표 셀 문단(`is_table_cell`) 분리 및 sequence 검증
//! 3. 레코드 헤더 해석 (크기 0xFFF 및 4바이트 확장 크기)
//! 4. 제어 문자 0~31 처리 (16바이트 확장 제어 스킵, 탭, 줄바꿈, 하이픈, 서러게이트 페어)
//! 5. 압축(Deflate) 및 비압축(Uncompressed) 모드
//! 6. 보안 플래그 (암호 설정, 배포용 문서, DRM) 감지 시 `Err("encrypted")` 반환
//! 7. 잘린 파일 및 손상된 데이터 처리 시 패닉 없이 안전한 `Err` 반환

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use klib::extract::extract_hwp;
use klib::hwp5::{
    build_para_text_bytes, encode_utf16le, extract_hwp5, ParaTextItem, RecordBuilder,
    SyntheticHwp5Builder,
    HWPTAG_CTRL_HEADER, HWPTAG_LIST_HEADER, HWPTAG_PARA_HEADER, HWPTAG_PARA_TEXT, HWPTAG_TABLE,
};

/// 임시 테스트 파일 컨텍스트 (종료 시 자동 삭제)
struct TempHwpFile {
    path: PathBuf,
}

impl TempHwpFile {
    fn new(name: &str, bytes: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!("klib_test_hwp5_{}_{}.hwp", name, std::process::id()));
        let mut file = File::create(&path).expect("임시 HWP 파일 생성");
        file.write_all(bytes).expect("임시 HWP 데이터 쓰기");
        Self { path }
    }
}

impl Drop for TempHwpFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[test]
fn test_hwp5_basic_synthetic_document() {
    // FileHeader, DocInfo, Section0 1개, 문단 3개 및 표 1개로 구성된 최소 HWP 생성
    let mut builder = SyntheticHwp5Builder::new().compressed(true);

    // 문단 1: 일반 텍스트
    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 0).with_data(vec![0u8; 18]).to_bytes())
        .add_record(
            RecordBuilder::new(HWPTAG_PARA_TEXT, 1)
                .with_data(encode_utf16le("제1장 총칙입니다."))
                .to_bytes(),
        );

    // 문단 2: 일반 텍스트
    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 0).with_data(vec![0u8; 18]).to_bytes())
        .add_record(
            RecordBuilder::new(HWPTAG_PARA_TEXT, 1)
                .with_data(encode_utf16le("본 규정은 업무 절차를 규정합니다."))
                .to_bytes(),
        );

    // 문단 3: 표(Table)를 포함하는 문단
    // 3-1. 문단 3 본문
    let para3_text_items = vec![
        ParaTextItem::String("다음은 참고 표입니다: ".to_string()),
        // 표 컨트롤 연결을 나타내는 확장 제어 문자 11 (총 16바이트)
        ParaTextItem::ExtendedControl(11, [0u8; 14]),
    ];
    let para3_bytes = build_para_text_bytes(&para3_text_items);

    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 0).with_data(vec![0u8; 18]).to_bytes())
        .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 1).with_data(para3_bytes).to_bytes());

    // 3-2. 표 컨트롤 레코드 트리
    // HWPTAG_CTRL_HEADER (level 1): 'tbl '
    let mut ctrl_tbl_data = b"tbl ".to_vec();
    ctrl_tbl_data.extend_from_slice(&[0u8; 12]); // 추가 속성
    builder = builder.add_record(RecordBuilder::new(HWPTAG_CTRL_HEADER, 1).with_data(ctrl_tbl_data).to_bytes());

    // HWPTAG_TABLE (level 2): 표 속성
    builder = builder.add_record(RecordBuilder::new(HWPTAG_TABLE, 2).with_data(vec![0u8; 32]).to_bytes());

    // HWPTAG_LIST_HEADER (level 2): 첫 번째 셀 리스트
    builder = builder.add_record(RecordBuilder::new(HWPTAG_LIST_HEADER, 2).with_data(vec![0u8; 16]).to_bytes());

    // 셀 안의 문단: HWPTAG_PARA_HEADER (level 3) & HWPTAG_PARA_TEXT (level 4)
    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 3).with_data(vec![0u8; 18]).to_bytes())
        .add_record(
            RecordBuilder::new(HWPTAG_PARA_TEXT, 4)
                .with_data(encode_utf16le("표 칸 안의 첫 번째 내용입니다."))
                .to_bytes(),
        );

    let hwp_bytes = builder.build().expect("합성 HWP 생성 성공");
    let temp_file = TempHwpFile::new("basic", &hwp_bytes);

    let records = extract_hwp5(&temp_file.path).expect("HWP5 본문 추출 성공");

    // 총 4개 문단 (일반 문단 3개 + 표 셀 문단 1개)
    assert_eq!(records.len(), 4, "문단 수는 총 4개여야 함");

    // 문단 1 검증
    assert_eq!(records[0].text, "제1장 총칙입니다.");
    assert_eq!(records[0].sequence, 1);
    assert_eq!(records[0].is_table_cell, false);
    assert_eq!(records[0].section.as_deref(), Some("Section0"));

    // 문단 2 검증
    assert_eq!(records[1].text, "본 규정은 업무 절차를 규정합니다.");
    assert_eq!(records[1].sequence, 2);
    assert_eq!(records[1].is_table_cell, false);

    // 문단 3 검증 (확장 제어 11이 제거되고 텍스트만 정상 추출)
    assert_eq!(records[2].text, "다음은 참고 표입니다:");
    assert_eq!(records[2].sequence, 3);
    assert_eq!(records[2].is_table_cell, false);

    // 문단 4 검증 (표 셀 안의 문단 -> is_table_cell = true)
    assert_eq!(records[3].text, "표 칸 안의 첫 번째 내용입니다.");
    assert_eq!(records[3].sequence, 4);
    assert_eq!(records[3].is_table_cell, true, "표 셀 문단은 is_table_cell이 true여야 함");
}

#[test]
fn test_hwp5_extended_record_size_0xfff() {
    // 레코드 크기가 0xFFF 이상이거나 강제 확장 헤더를 사용하는 경우 (8바이트 헤더 해석)
    let long_text = "가나다라마바사아자차카타파하 ".repeat(300); // 약 4,500 WCHAR (9,000바이트)
    let text_bytes = encode_utf16le(&long_text);

    let mut builder = SyntheticHwp5Builder::new().compressed(true);
    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 0).with_data(vec![0u8; 18]).to_bytes())
        .add_record(
            RecordBuilder::new(HWPTAG_PARA_TEXT, 1)
                .with_data(text_bytes)
                .force_extended() // 0xFFF + 4바이트 확장 크기 기록
                .to_bytes(),
        );

    let hwp_bytes = builder.build().expect("합성 HWP 생성 성공");
    let temp_file = TempHwpFile::new("extended_size", &hwp_bytes);

    let records = extract_hwp5(&temp_file.path).expect("확장 레코드 크기 HWP5 추출 성공");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].text, long_text.trim());
}

#[test]
fn test_hwp5_control_characters_handling() {
    // 0~31 제어 문자 처리 명세 검증:
    // - 1~3, 11~12, 14~18, 21~23: 확장 제어(16바이트 차지, 8 WCHAR 건너뜀)
    // - 9: 탭 ('\t')
    // - 10: 강제 줄바꿈 ('\n')
    // - 13: 문단 끝 ('\n')
    // - 24: 하이픈 ('-')
    // - 25, 26: 공백 (' ')
    // - 27~31: 공백 (' ')
    // - 서러게이트 페어: 4바이트 유니코드 (예: 훈민정음 특수문자 또는 이모지)
    let items = vec![
        ParaTextItem::String("시작".to_string()),
        ParaTextItem::ExtendedControl(9, [0x20; 14]), // 탭(인라인 컨트롤 8 WCHAR, 규격)
        ParaTextItem::String("탭이후".to_string()),
        ParaTextItem::Control(10), // 줄바꿈
        ParaTextItem::String("줄바꿈이후".to_string()),
        ParaTextItem::ExtendedControl(2, [0xAA; 14]), // 확장 제어 2 (누름틀 등, 16바이트)
        ParaTextItem::String("확장제어2이후".to_string()),
        ParaTextItem::ExtendedControl(15, [0xBB; 14]), // 확장 제어 15 (머리말, 16바이트)
        ParaTextItem::String("머리말이후".to_string()),
        ParaTextItem::Control(24), // 하이픈
        ParaTextItem::Control(25), // 묶음 빈칸
        ParaTextItem::String("끝".to_string()),
        ParaTextItem::Char('🚀'), // 서러게이트 페어 이모지
    ];
    let para_bytes = build_para_text_bytes(&items);

    let mut builder = SyntheticHwp5Builder::new().compressed(false);
    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 0).with_data(vec![0u8; 18]).to_bytes())
        .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 1).with_data(para_bytes).to_bytes());

    let hwp_bytes = builder.build().expect("합성 HWP 생성 성공");
    let temp_file = TempHwpFile::new("ctrl_chars", &hwp_bytes);

    let records = extract_hwp5(&temp_file.path).expect("제어 문자 처리 추출 성공");
    assert_eq!(records.len(), 1);

    let expected = "시작\t탭이후\n줄바꿈이후확장제어2이후머리말이후- 끝🚀";
    assert_eq!(records[0].text, expected);
}

#[test]
fn test_hwp5_uncompressed_document() {
    // 비압축(Uncompressed) HWP5 문서 테스트
    let mut builder = SyntheticHwp5Builder::new().compressed(false);
    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 0).with_data(vec![0u8; 18]).to_bytes())
        .add_record(
            RecordBuilder::new(HWPTAG_PARA_TEXT, 1)
                .with_data(encode_utf16le("비압축 본문 텍스트입니다."))
                .to_bytes(),
        );

    let hwp_bytes = builder.build().expect("비압축 HWP 생성 성공");
    let temp_file = TempHwpFile::new("uncompressed", &hwp_bytes);

    let records = extract_hwp5(&temp_file.path).expect("비압축 HWP5 본문 추출 성공");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].text, "비압축 본문 텍스트입니다.");
}

#[test]
fn test_hwp5_encrypted_and_distributed_flags() {
    // 1. 암호 설정 문서
    let mut builder1 = SyntheticHwp5Builder::new().password_encrypted(true);
    builder1 = builder1
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 0).with_data(vec![0u8; 18]).to_bytes())
        .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 1).with_data(encode_utf16le("암호문")).to_bytes());
    let hwp1 = builder1.build().unwrap();
    let temp1 = TempHwpFile::new("pwd", &hwp1);

    let err1 = extract_hwp5(&temp1.path).unwrap_err();
    assert!(err1.starts_with("encrypted:"), "암호 문서는 encrypted로 반환되어야 함: {err1}");

    // 2. 배포용 문서
    let mut builder2 = SyntheticHwp5Builder::new().distributed(true);
    builder2 = builder2
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 0).with_data(vec![0u8; 18]).to_bytes())
        .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 1).with_data(encode_utf16le("배포문")).to_bytes());
    let hwp2 = builder2.build().unwrap();
    let temp2 = TempHwpFile::new("dist", &hwp2);

    let err2 = extract_hwp5(&temp2.path).unwrap_err();
    assert!(err2.starts_with("encrypted:"), "배포 문서는 encrypted로 반환되어야 함: {err2}");
}

#[test]
fn test_hwp5_corrupt_and_truncated_no_panic() {
    // 1. 빈 파일
    let temp_empty = TempHwpFile::new("empty", &[]);
    assert!(extract_hwp5(&temp_empty.path).is_err());

    // 2. 정상 CFB 생성 후 끝부분을 잘라버린 손상 파일
    let mut builder = SyntheticHwp5Builder::new().compressed(true);
    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 0).with_data(vec![0u8; 18]).to_bytes())
        .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 1).with_data(encode_utf16le("잘릴 본문")).to_bytes());
    let full_bytes = builder.build().unwrap();

    // 중간 지점에서 자름
    let truncated_bytes = &full_bytes[..full_bytes.len() / 2];
    let temp_trunc = TempHwpFile::new("trunc", truncated_bytes);

    let res = extract_hwp5(&temp_trunc.path);
    assert!(res.is_err(), "잘린 파일은 패닉 없이 Err를 반환해야 함");
    let err_msg = res.unwrap_err();
    assert!(err_msg.starts_with("corrupt:"), "손상 오류 코드로 반환되어야 함: {err_msg}");
}

#[test]
fn test_extract_hwp_top_level_fallback() {
    // extract_hwp 최상위 함수 동작 검증
    // 정상 HWP 파일이면 extract_hwp5로 바로 성공
    let mut builder = SyntheticHwp5Builder::new().compressed(true);
    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 0).with_data(vec![0u8; 18]).to_bytes())
        .add_record(
            RecordBuilder::new(HWPTAG_PARA_TEXT, 1)
                .with_data(encode_utf16le("통합 추출 성공"))
                .to_bytes(),
        );

    let hwp_bytes = builder.build().unwrap();
    let temp_file = TempHwpFile::new("top_level", &hwp_bytes);

    let records = extract_hwp(&temp_file.path).expect("최상위 extract_hwp 호출 성공");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].text, "통합 추출 성공");
}

#[test]
fn test_hwp5_control_kinds_classification() {
    // 머리말(head), 각주(fn), 일반 본문 구분 검증
    let mut builder = SyntheticHwp5Builder::new().compressed(true);

    builder = builder.add_record(RecordBuilder::new(HWPTAG_CTRL_HEADER, 0).with_data(b"head\0\0\0\0\0\0\0\0\0\0\0\0".to_vec()).to_bytes());
    builder = builder.add_record(RecordBuilder::new(HWPTAG_LIST_HEADER, 1).with_data(vec![0u8; 16]).to_bytes());
    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 2).with_data(vec![0u8; 18]).to_bytes())
        .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 3).with_data(encode_utf16le("머리말 텍스트")).to_bytes());

    builder = builder.add_record(RecordBuilder::new(HWPTAG_CTRL_HEADER, 0).with_data(b"fn  \0\0\0\0\0\0\0\0\0\0\0\0".to_vec()).to_bytes());
    builder = builder.add_record(RecordBuilder::new(HWPTAG_LIST_HEADER, 1).with_data(vec![0u8; 16]).to_bytes());
    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 2).with_data(vec![0u8; 18]).to_bytes())
        .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 3).with_data(encode_utf16le("각주 설명문")).to_bytes());

    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 0).with_data(vec![0u8; 18]).to_bytes())
        .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 1).with_data(encode_utf16le("일반 본문")).to_bytes());

    let hwp_bytes = builder.build().unwrap();
    let temp = TempHwpFile::new("ctrls", &hwp_bytes);

    let paras = extract_hwp5(&temp.path).expect("추출 성공");
    assert_eq!(paras.len(), 3);
    assert_eq!(paras[0].location_kind, "header");
    assert_eq!(paras[1].location_kind, "footnote");
    assert_eq!(paras[2].location_kind, "paragraph");
}

#[test]
fn test_hwp5_formfeed_char_12_handling() {
    let mut text_bytes = Vec::new();
    text_bytes.extend_from_slice(&encode_utf16le("앞부분"));
    text_bytes.extend_from_slice(&12u16.to_le_bytes()); // 코드 12 (폼 피드)
    text_bytes.extend_from_slice(&encode_utf16le("뒷부분글자"));

    let decoded = klib::hwp5::decode_para_text(&text_bytes);
    assert!(decoded.contains("앞부분"));
    assert!(decoded.contains("뒷부분글자"));
}

#[test]
fn test_hwp5_hidden_comment_and_textbox_classification() {
    let mut builder = SyntheticHwp5Builder::new().compressed(true);

    // 글상자 (b"gso ")
    builder = builder.add_record(RecordBuilder::new(HWPTAG_CTRL_HEADER, 0).with_data(b"gso \0\0\0\0\0\0\0\0\0\0\0\0".to_vec()).to_bytes());
    builder = builder.add_record(RecordBuilder::new(HWPTAG_LIST_HEADER, 1).with_data(vec![0u8; 16]).to_bytes());
    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 2).with_data(vec![0u8; 18]).to_bytes())
        .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 3).with_data(encode_utf16le("글상자 내용문")).to_bytes());

    // 숨은 설명 (b"tcmt")
    builder = builder.add_record(RecordBuilder::new(HWPTAG_CTRL_HEADER, 0).with_data(b"tcmt\0\0\0\0\0\0\0\0\0\0\0\0".to_vec()).to_bytes());
    builder = builder.add_record(RecordBuilder::new(HWPTAG_LIST_HEADER, 1).with_data(vec![0u8; 16]).to_bytes());
    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 2).with_data(vec![0u8; 18]).to_bytes())
        .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 3).with_data(encode_utf16le("숨은 설명 메모")).to_bytes());

    let hwp_bytes = builder.build().unwrap();
    let temp = TempHwpFile::new("gso_tcmt", &hwp_bytes);

    let paras = extract_hwp5(&temp.path).expect("추출 성공");
    assert_eq!(paras.len(), 2);
    assert_eq!(paras[0].location_kind, "textbox");
    assert_eq!(paras[0].text, "글상자 내용문");
    assert_eq!(paras[1].location_kind, "hidden_comment");
    assert_eq!(paras[1].text, "숨은 설명 메모");
}

#[test]
fn test_hwp5_inline_control_code_4_skip() {
    let mut text_bytes = Vec::new();
    text_bytes.extend_from_slice(&encode_utf16le("본문앞"));
    // 코드 4 (1 WCHAR) + 부가정보 (6 WCHAR) + 코드 4 (1 WCHAR) = 총 8 WCHAR
    text_bytes.extend_from_slice(&4u16.to_le_bytes());
    for _ in 0..7 {
        text_bytes.extend_from_slice(&0x1234u16.to_le_bytes()); // 가비지 부가정보
    }
    text_bytes.extend_from_slice(&encode_utf16le("본문뒤"));

    let decoded = klib::hwp5::decode_para_text(&text_bytes);
    assert_eq!(decoded, "본문앞본문뒤");
}

#[test]
fn test_hwp5_cmp4_search_indexing_excludes_hidden_comment() {
    let mut builder = SyntheticHwp5Builder::new().compressed(true);

    // 본문 문단
    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 0).with_data(vec![0u8; 18]).to_bytes())
        .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 1).with_data(encode_utf16le("일반 본문 내용")).to_bytes());

    // 숨은 설명 (b"tcmt")
    builder = builder.add_record(RecordBuilder::new(HWPTAG_CTRL_HEADER, 0).with_data(b"tcmt\0\0\0\0\0\0\0\0\0\0\0\0".to_vec()).to_bytes());
    builder = builder.add_record(RecordBuilder::new(HWPTAG_LIST_HEADER, 1).with_data(vec![0u8; 16]).to_bytes());
    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 2).with_data(vec![0u8; 18]).to_bytes())
        .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 3).with_data(encode_utf16le("비공개 숨은글")).to_bytes());

    let hwp_bytes = builder.build().unwrap();
    let temp = TempHwpFile::new("index_filter", &hwp_bytes);

    // 1. extract_hwp5는 구조 파악을 위해 hidden_comment를 보존
    let raw_paras = extract_hwp5(&temp.path).expect("추출 성공");
    assert_eq!(raw_paras.len(), 2);
    assert_eq!(raw_paras[0].location_kind, "paragraph");
    assert_eq!(raw_paras[1].location_kind, "hidden_comment");

    // 2. extract_for_index 및 extract는 검색 색인 품질(정밀도)을 위해 hidden_comment 제외 (HWP5-CMP4)
    let index_paras = klib::extract::extract_for_index(&temp.path).expect("색인 추출 성공");
    assert_eq!(index_paras.len(), 1);
    assert_eq!(index_paras[0].text, "일반 본문 내용");
    assert_eq!(index_paras[0].location_kind, "paragraph");
}

