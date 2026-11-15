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
    build_para_text_bytes, encode_utf16le, extract_hwp5, extract_hwp5_preview,
    ImageFormat, ParaTextItem, Preview, RecordBuilder, SyntheticHwp5Builder,
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

// =========================================================================
// DVF1: HWP 5.0 미리보기(PrvImage, PrvText) 추출 통합 시험
// =========================================================================

const SAMPLE_PNG_BYTES: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15c4";
const SAMPLE_GIF_BYTES: &[u8] = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\xff\xff\xff\x00\x00\x00!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;";
const SAMPLE_BMP_BYTES: &[u8] = b"BM\x3a\x00\x00\x00\x00\x00\x00\x00\x36\x00\x00\x00\x28\x00\x00\x00\x01\x00\x00\x00\x01\x00\x00\x00\x01\x00\x18\x00\x00\x00\x00\x00\x04\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\xff\xff\xff\x00";

#[test]
fn test_dvf1_preview_png_and_text() {
    let text = "문서 미리보기 첫 번째 문장입니다. 몽땅사진첩 연동.";
    let hwp_bytes = SyntheticHwp5Builder::new()
        .prv_image(SAMPLE_PNG_BYTES.to_vec())
        .prv_text(text)
        .build()
        .expect("합성 HWP 생성 성공");

    let temp = TempHwpFile::new("dvf1_png", &hwp_bytes);
    let preview: Preview = extract_hwp5_preview(&temp.path).expect("미리보기 추출 성공");

    assert!(preview.has_image());
    assert_eq!(preview.image_format, Some(ImageFormat::Png));
    assert_eq!(preview.format(), Some(ImageFormat::Png));
    assert_eq!(preview.image.as_deref(), Some(SAMPLE_PNG_BYTES));
    assert_eq!(preview.text, text);
    assert!(!preview.is_empty());
}

#[test]
fn test_dvf1_preview_gif_and_bmp_formats() {
    // 1. GIF 형식
    let gif_hwp = SyntheticHwp5Builder::new()
        .prv_image(SAMPLE_GIF_BYTES.to_vec())
        .prv_text("GIF 미리보기")
        .build()
        .unwrap();
    let temp_gif = TempHwpFile::new("dvf1_gif", &gif_hwp);
    let preview_gif = extract_hwp5_preview(&temp_gif.path).expect("GIF 미리보기 추출");
    assert_eq!(preview_gif.image_format, Some(ImageFormat::Gif));
    assert_eq!(preview_gif.image.as_deref(), Some(SAMPLE_GIF_BYTES));
    assert_eq!(preview_gif.text, "GIF 미리보기");

    // 2. BMP 형식
    let bmp_hwp = SyntheticHwp5Builder::new()
        .prv_image(SAMPLE_BMP_BYTES.to_vec())
        .prv_text("BMP 미리보기")
        .build()
        .unwrap();
    let temp_bmp = TempHwpFile::new("dvf1_bmp", &bmp_hwp);
    let preview_bmp = extract_hwp5_preview(&temp_bmp.path).expect("BMP 미리보기 추출");
    assert_eq!(preview_bmp.image_format, Some(ImageFormat::Bmp));
    assert_eq!(preview_bmp.image.as_deref(), Some(SAMPLE_BMP_BYTES));
    assert_eq!(preview_bmp.text, "BMP 미리보기");
}

#[test]
fn test_dvf1_preview_empty_when_no_streams() {
    // PrvImage, PrvText 모두 없는 정상 문서 -> 둘 다 없으면 빈 값 반환
    let hwp_bytes = SyntheticHwp5Builder::new().build().unwrap();
    let temp = TempHwpFile::new("dvf1_empty", &hwp_bytes);

    let preview = extract_hwp5_preview(&temp.path).expect("빈 미리보기 추출 성공");
    assert!(preview.is_empty());
    assert!(!preview.has_image());
    assert!(!preview.has_text());
    assert_eq!(preview.image, None);
    assert_eq!(preview.image_format, None);
    assert_eq!(preview.text, "");
}

#[test]
fn test_dvf1_preview_text_max_1024_chars_and_bom_strip() {
    // 1,024자 초과 텍스트 (1,500자)
    let long_text = "가나다라마바사".repeat(250); // 7 * 250 = 1750자
    assert!(long_text.chars().count() > 1024);

    let hwp_bytes = SyntheticHwp5Builder::new()
        .prv_text(&long_text)
        .build()
        .unwrap();
    let temp = TempHwpFile::new("dvf1_long_text", &hwp_bytes);

    let preview = extract_hwp5_preview(&temp.path).expect("미리보기 추출");
    assert_eq!(preview.text.chars().count(), 1024);
    let expected: String = long_text.chars().take(1024).collect();
    assert_eq!(preview.text, expected);

    // UTF-16LE BOM(0xFEFF) 포함 텍스트 검증
    let mut bom_bytes = vec![0xFF, 0xFE]; // UTF-16LE BOM
    bom_bytes.extend_from_slice(&encode_utf16le("BOM 제외 테스트"));
    let bom_hwp = SyntheticHwp5Builder::new()
        .prv_text_bytes(bom_bytes)
        .build()
        .unwrap();
    let temp_bom = TempHwpFile::new("dvf1_bom", &bom_hwp);
    let preview_bom = extract_hwp5_preview(&temp_bom.path).expect("BOM 미리보기 추출");
    assert_eq!(preview_bom.text, "BOM 제외 테스트");
}

#[test]
fn test_dvf1_preview_encrypted_and_distributed_docs() {
    // 암호·배포용 문서는 내용 스트림을 열지 않고 PrvImage/PrvText만 시도(실패해도 오류 대신 빈 값).
    // 1. 암호 문서에 미리보기가 있는 경우
    let enc_with_preview = SyntheticHwp5Builder::new()
        .password_encrypted(true)
        .prv_image(SAMPLE_PNG_BYTES.to_vec())
        .prv_text("암호화 문서 미리보기 글")
        .build()
        .unwrap();
    let temp_enc1 = TempHwpFile::new("dvf1_enc_with", &enc_with_preview);
    let preview_enc1 = extract_hwp5_preview(&temp_enc1.path).expect("암호 문서 미리보기 추출 성공");
    assert_eq!(preview_enc1.image_format, Some(ImageFormat::Png));
    assert_eq!(preview_enc1.text, "암호화 문서 미리보기 글");

    // 2. 암호 문서에 미리보기가 없는 경우 -> 오류 대신 빈 값
    let enc_no_preview = SyntheticHwp5Builder::new()
        .password_encrypted(true)
        .build()
        .unwrap();
    let temp_enc2 = TempHwpFile::new("dvf1_enc_none", &enc_no_preview);
    let preview_enc2 = extract_hwp5_preview(&temp_enc2.path).expect("암호 문서 빈 미리보기 성공");
    assert!(preview_enc2.is_empty());

    // 3. 배포용 문서에 미리보기가 있는 경우
    let dist_with_preview = SyntheticHwp5Builder::new()
        .distributed(true)
        .prv_image(SAMPLE_PNG_BYTES.to_vec())
        .prv_text("배포용 문서 미리보기")
        .build()
        .unwrap();
    let temp_dist1 = TempHwpFile::new("dvf1_dist_with", &dist_with_preview);
    let preview_dist1 = extract_hwp5_preview(&temp_dist1.path).expect("배포용 미리보기 추출 성공");
    assert_eq!(preview_dist1.image_format, Some(ImageFormat::Png));
    assert_eq!(preview_dist1.text, "배포용 문서 미리보기");

    // 4. 배포용 문서에 미리보기가 없는 경우 -> 오류 대신 빈 값
    let dist_no_preview = SyntheticHwp5Builder::new()
        .distributed(true)
        .build()
        .unwrap();
    let temp_dist2 = TempHwpFile::new("dvf1_dist_none", &dist_no_preview);
    let preview_dist2 = extract_hwp5_preview(&temp_dist2.path).expect("배포용 빈 미리보기 성공");
    assert!(preview_dist2.is_empty());
}

#[test]
fn test_dvf1_preview_corrupt_defense_sector_cycle() {
    // 손상 파일 방어: 섹터 순환 감지
    let mut hwp_bytes = SyntheticHwp5Builder::new()
        .prv_image(SAMPLE_PNG_BYTES.to_vec())
        .prv_text("섹터 순환 테스트")
        .build()
        .unwrap();

    // CFB 헤더에서 섹터 크기 및 첫 번째 FAT 섹터 위치 확인
    let sector_shift = u16::from_le_bytes([hwp_bytes[30], hwp_bytes[31]]) as usize;
    let sector_size = 1usize << sector_shift;
    let fat_sec_id = u32::from_le_bytes([hwp_bytes[76], hwp_bytes[77], hwp_bytes[78], hwp_bytes[79]]) as usize;
    let fat_offset = (fat_sec_id + 1) * sector_size;

    // FAT 테이블 내에서 섹터 4 -> 섹터 5 -> 섹터 4 순환 루프 강제 주입
    let sec4_offset = fat_offset + 4 * 4;
    let sec5_offset = fat_offset + 5 * 4;
    if sec5_offset + 4 <= hwp_bytes.len() {
        hwp_bytes[sec4_offset..sec4_offset + 4].copy_from_slice(&5u32.to_le_bytes());
        hwp_bytes[sec5_offset..sec5_offset + 4].copy_from_slice(&4u32.to_le_bytes());
    }

    let temp = TempHwpFile::new("dvf1_cycle", &hwp_bytes);
    let res = extract_hwp5_preview(&temp.path);
    assert!(res.is_err(), "섹터 순환 손상 파일은 Err를 반환해야 함");
    let err_msg = res.unwrap_err();
    assert!(
        err_msg.contains("순환") || err_msg.contains("cycle"),
        "에러 메시지에 섹터 순환 감지 내용이 포함되어야 함: {err_msg}"
    );
}

#[test]
fn test_dvf1_preview_corrupt_defense_stream_size_4mb_limit() {
    // 손상 파일 방어: 스트림 크기 상한 4MB 초과 조작
    let mut hwp_bytes = SyntheticHwp5Builder::new()
        .prv_image(SAMPLE_PNG_BYTES.to_vec())
        .prv_text("4MB 초과 방어")
        .build()
        .unwrap();

    // 디렉토리 엔트리에서 "PrvImage" 스트림의 크기 필드(120..128)를 5MB(5,242,880바이트)로 조작
    let prv_name_utf16 = encode_utf16le("PrvImage");
    let mut found = false;
    let mut pos = 512;
    while pos + 128 <= hwp_bytes.len() {
        if &hwp_bytes[pos..pos + prv_name_utf16.len()] == &prv_name_utf16[..] {
            // 크기 필드(120..128) 조작
            let oversized: u64 = 5 * 1024 * 1024;
            hwp_bytes[pos + 120..pos + 128].copy_from_slice(&oversized.to_le_bytes());
            found = true;
            break;
        }
        pos += 128;
    }
    assert!(found, "디렉토리 엔트리에서 PrvImage 탐색 성공해야 함");

    let temp = TempHwpFile::new("dvf1_oversized", &hwp_bytes);
    let res = extract_hwp5_preview(&temp.path);
    assert!(res.is_err(), "4MB 초과 스트림은 Err를 반환해야 함");
    let err_msg = res.unwrap_err();
    assert!(
        err_msg.contains("4MB") || err_msg.contains("제한을 초과"),
        "에러 메시지에 4MB 크기 제한 초과 내용이 포함되어야 함: {err_msg}"
    );
}

#[test]
fn test_dvf1_preview_corrupt_truncated_file() {
    // 완전히 잘린 파일 (헤더 100바이트만 존재)
    let temp = TempHwpFile::new("dvf1_truncated", &[0u8; 100]);
    let res = extract_hwp5_preview(&temp.path);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("corrupt"));
}

#[test]
fn test_dvf1_preview_cli_command() {
    let text = "CLI 하위 명령 연동 시험 텍스트";
    let hwp_bytes = SyntheticHwp5Builder::new()
        .prv_image(SAMPLE_PNG_BYTES.to_vec())
        .prv_text(text)
        .build()
        .unwrap();

    let temp_hwp = TempHwpFile::new("dvf1_cli", &hwp_bytes);
    let out_png_path = std::env::temp_dir().join(format!("klib_test_out_{}.png", std::process::id()));
    let _ = std::fs::remove_file(&out_png_path);

    let klib_bin = env!("CARGO_BIN_EXE_klib");
    let output = std::process::Command::new(klib_bin)
        .args(["preview", temp_hwp.path.to_str().unwrap(), out_png_path.to_str().unwrap()])
        .output()
        .expect("klib preview CLI 실행 성공");

    assert!(output.status.success(), "CLI 반환 코드가 0이어야 함");
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    assert!(stdout_str.contains("\"has_image\":true"));
    assert!(stdout_str.contains("\"format\":\"png\""));
    assert!(stdout_str.contains(text));

    // 출력 파일이 실제로 작성되었고 원본 이미지와 일치하는지 검증
    assert!(out_png_path.is_file(), "출력 PNG 파일이 생성되어야 함");
    let written_bytes = std::fs::read(&out_png_path).expect("출력 PNG 파일 읽기");
    assert_eq!(written_bytes, SAMPLE_PNG_BYTES);

    let _ = std::fs::remove_file(&out_png_path);
}

#[test]
fn test_kordoc23_synthetic_hwp_identity_and_preview() {
    // KORDOC23 2단계: 합성 HWP 생성 및 결과 동일성 검증
    let text1 = "제1조(목적) 이 조례는 주민 복리 증진을 목적으로 한다.";
    let text2 = "제2조(정의) 이 조례에서 사용하는 용어의 뜻은 다음과 같다.";
    let cell_text = "별표 1 관련 산출 내역";

    let mut builder = SyntheticHwp5Builder::new()
        .compressed(true)
        .prv_image(SAMPLE_PNG_BYTES.to_vec())
        .prv_text(&format!("{text1}\n{text2}"));

    // 문단 1
    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 0).with_data(vec![0u8; 18]).to_bytes())
        .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 1).with_data(encode_utf16le(text1)).to_bytes());

    // 문단 2
    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 0).with_data(vec![0u8; 18]).to_bytes())
        .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 1).with_data(encode_utf16le(text2)).to_bytes());

    // 표 문단
    let para3_items = vec![
        ParaTextItem::String("참고 표: ".to_string()),
        ParaTextItem::ExtendedControl(11, [0u8; 14]),
    ];
    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 0).with_data(vec![0u8; 18]).to_bytes())
        .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 1).with_data(build_para_text_bytes(&para3_items)).to_bytes());

    let mut ctrl_tbl = b"tbl ".to_vec();
    ctrl_tbl.extend_from_slice(&[0u8; 12]);
    builder = builder.add_record(RecordBuilder::new(HWPTAG_CTRL_HEADER, 1).with_data(ctrl_tbl).to_bytes());
    builder = builder.add_record(RecordBuilder::new(HWPTAG_TABLE, 2).with_data(vec![0u8; 32]).to_bytes());
    builder = builder.add_record(RecordBuilder::new(HWPTAG_LIST_HEADER, 2).with_data(vec![0u8; 16]).to_bytes());
    builder = builder
        .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 3).with_data(vec![0u8; 18]).to_bytes())
        .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 4).with_data(encode_utf16le(cell_text)).to_bytes());

    let hwp_bytes = builder.build().expect("합성 HWP 빌드 성공");
    let temp = TempHwpFile::new("kordoc23_synth", &hwp_bytes);

    // 1. 本文 문단 추출 동일성 검증
    let records = extract_hwp(&temp.path).expect("HWP 본문 추출 성공");
    assert_eq!(records.len(), 4, "문단 총 4개(본문 3 + 표 셀 1)");
    assert_eq!(records[0].text, text1);
    assert_eq!(records[1].text, text2);
    assert_eq!(records[2].text, "참고 표:");
    assert_eq!(records[3].text, cell_text);
    assert!(records[3].is_table_cell, "표 셀 문단 표시");

    // 2. preview 추출 동일성 검증
    let out_png = std::env::temp_dir().join(format!("klib_kordoc23_out_{}.png", std::process::id()));
    let _ = std::fs::remove_file(&out_png);
    let (has_image, format, prev_text) = klib::extract::preview_hwp(&temp.path, &out_png).expect("미리보기 추출");
    assert!(has_image);
    assert_eq!(format.as_deref(), Some("png"));
    assert!(prev_text.contains("제1조(목적)"));
    assert!(out_png.is_file());
    assert_eq!(std::fs::read(&out_png).unwrap(), SAMPLE_PNG_BYTES);
    let _ = std::fs::remove_file(&out_png);
}

