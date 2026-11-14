//! HWP 5.0 (이진 바이너리 포맷) 본문 및 표 텍스트 Rust 직접 추출 모듈.
//!
//! 한컴 "한글 문서 파일 형식 5.0" 공개 규격 기반 구현:
//! 1. 복합 문서(Compound File Binary, CFB / OLE) 구조 해석
//! 2. `FileHeader` 스트림 검증 (시그니처, 버전, 압축/암호/배포용 플래그)
//! 3. `BodyText/Section*` 스트림 추출 및 raw Deflate 압축 해제
//! 4. 레코드 헤더(Tag ID, Level, Size, 0xFFF 확장 크기) 파싱
//! 5. HWPTAG_PARA_HEADER(66) 및 HWPTAG_PARA_TEXT(67) 제어 문자(0~31) 처리
//!    - 1~8, 11, 14~23: 확장 및 인라인 제어 문자 (16바이트 차지, 8 WCHAR 건너뜀)
//!    - 9: 탭('\t'), 10: 줄바꿈('\n'), 12: 폼 피드('\n'), 13: 문단 끝('\n')
//!    - 24: 하이픈('-'), 25~31: 공백류 문자
//! 6. 컨트롤(HWPTAG_CTRL_HEADER) 및 문단 리스트(HWPTAG_LIST_HEADER) 안의 문단 감지
//!    - 표 셀(tbl): `is_table_cell = true`, `location_kind = "table_cell"`
//!    - 머리말(head)/꼬리말(foot), 각주(fn)/미주(en), 글상자(gso 등), 숨은글(tcmt), 메모(memo), 덧말(tdut) 등 세분화 분류
//!
//! 라이선스: MIT

use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::Path;

use cfb::CompoundFile;
use flate2::read::{DeflateDecoder, ZlibDecoder};

use crate::extract::Paragraph;

/// HWP 5.0 시그니처 ("HWP Document File")
pub const HWP5_SIGNATURE: &[u8] = b"HWP Document File";

/// FileHeader 고정 크기 (256 바이트)
pub const FILE_HEADER_SIZE: usize = 256;

/// DoS 방지를 위한 최대 압축 해제 크기 (섹션당 128MB, 전체 512MB)
pub const MAX_SECTION_DECOMPRESSED_SIZE: u64 = 128 * 1024 * 1024;
pub const MAX_TOTAL_DECOMPRESSED_SIZE: u64 = 512 * 1024 * 1024;

/// HWP 5.0 주요 레코드 태그 ID
pub const HWPTAG_BEGIN: u32 = 0x010;
pub const HWPTAG_DOCUMENT_PROPERTIES: u32 = HWPTAG_BEGIN; // 16
pub const HWPTAG_ID_MAPPINGS: u32 = HWPTAG_BEGIN + 1; // 17
pub const HWPTAG_BIN_DATA: u32 = HWPTAG_BEGIN + 2; // 18
pub const HWPTAG_FACE_NAME: u32 = HWPTAG_BEGIN + 3; // 19
pub const HWPTAG_BORDER_FILL: u32 = HWPTAG_BEGIN + 4; // 20
pub const HWPTAG_CHAR_SHAPE: u32 = HWPTAG_BEGIN + 5; // 21
pub const HWPTAG_TAB_DEF: u32 = HWPTAG_BEGIN + 6; // 22
pub const HWPTAG_NUMBERING: u32 = HWPTAG_BEGIN + 7; // 23
pub const HWPTAG_BULLET: u32 = HWPTAG_BEGIN + 8; // 24
pub const HWPTAG_PARA_SHAPE: u32 = HWPTAG_BEGIN + 9; // 25
pub const HWPTAG_STYLE: u32 = HWPTAG_BEGIN + 10; // 26
pub const HWPTAG_DOC_DATA: u32 = HWPTAG_BEGIN + 11; // 27
pub const HWPTAG_DISTRIBUTE_DOC_DATA: u32 = HWPTAG_BEGIN + 12; // 28

// 본문(Section) 레코드 태그
pub const HWPTAG_PARA_HEADER: u32 = HWPTAG_BEGIN + 50; // 66
pub const HWPTAG_PARA_TEXT: u32 = HWPTAG_BEGIN + 51; // 67
pub const HWPTAG_PARA_CHAR_SHAPE: u32 = HWPTAG_BEGIN + 52; // 68
pub const HWPTAG_PARA_LINE_SEG: u32 = HWPTAG_BEGIN + 53; // 69
pub const HWPTAG_PARA_RANGE_TAG: u32 = HWPTAG_BEGIN + 54; // 70
pub const HWPTAG_CTRL_HEADER: u32 = HWPTAG_BEGIN + 55; // 71
pub const HWPTAG_LIST_HEADER: u32 = HWPTAG_BEGIN + 56; // 72
pub const HWPTAG_PAGE_DEF: u32 = HWPTAG_BEGIN + 57; // 73
pub const HWPTAG_FOOTNOTE: u32 = HWPTAG_BEGIN + 58; // 74
pub const HWPTAG_PAGE_BORDER_FILL: u32 = HWPTAG_BEGIN + 59; // 75
pub const HWPTAG_SHAPE_COMPONENT: u32 = HWPTAG_BEGIN + 60; // 76
pub const HWPTAG_TABLE: u32 = HWPTAG_BEGIN + 61; // 77

/// FileHeader 플래그 비트 정의
pub const FLAG_COMPRESSED: u32 = 0x01; // 압축 여부 (Deflate)
pub const FLAG_PASSWORD_ENCRYPTED: u32 = 0x02; // 암호 설정 여부
pub const FLAG_DISTRIBUTED: u32 = 0x04; // 배포용 문서 여부
pub const FLAG_SCRIPT: u32 = 0x08; // 스크립트 저장 여부
pub const FLAG_DRM: u32 = 0x10; // DRM 보안 문서 여부
pub const FLAG_XML_TEMPLATE: u32 = 0x20; // XML 템플릿 문서 여부
pub const FLAG_HISTORY: u32 = 0x40; // 문서 이력 관리 여부
pub const FLAG_SIGNATURE: u32 = 0x80; // 전자 서명 여부

/// HWP 5.0 FileHeader 정보
#[derive(Debug, Clone, PartialEq)]
pub struct HwpFileHeader {
    pub version: u32,
    pub flags: u32,
    pub is_compressed: bool,
    pub is_encrypted: bool,
    pub is_distributed: bool,
    pub is_drm: bool,
}

impl HwpFileHeader {
    /// 256바이트 FileHeader 버퍼로부터 헤더 구조체를 파싱한다.
    pub fn parse(buf: &[u8]) -> Result<Self, String> {
        if buf.len() < FILE_HEADER_SIZE {
            return Err("corrupt: FileHeader 크기가 256바이트 미만입니다".into());
        }

        // 시그니처 검사 (첫 17바이트 "HWP Document File")
        if &buf[..HWP5_SIGNATURE.len()] != HWP5_SIGNATURE {
            return Err("unsupported: HWP 5.0 시그니처가 일치하지 않습니다".into());
        }

        // 파일 버전 (오프셋 32..36, uint32 little-endian)
        let version = u32::from_le_bytes(buf[32..36].try_into().unwrap());
        let major = (version >> 24) & 0xFF;
        if major < 5 {
            return Err(format!("unsupported: 지원하지 않는 HWP 버전입니다 (major={major})"));
        }

        // 속성 플래그 (오프셋 36..40, uint32 little-endian)
        let flags = u32::from_le_bytes(buf[36..40].try_into().unwrap());
        let is_compressed = (flags & FLAG_COMPRESSED) != 0;
        let is_encrypted = (flags & FLAG_PASSWORD_ENCRYPTED) != 0;
        let is_distributed = (flags & FLAG_DISTRIBUTED) != 0;
        let is_drm = (flags & FLAG_DRM) != 0;

        Ok(Self {
            version,
            flags,
            is_compressed,
            is_encrypted,
            is_distributed,
            is_drm,
        })
    }
}

/// 단일 HWP 5.0 레코드 헤더
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecordHeader {
    pub tag_id: u32,
    pub level: u32,
    pub size: usize,
    pub header_size: usize,
}

impl RecordHeader {
    /// 레코드 버퍼의 시작 위치에서 헤더(4바이트 또는 8바이트)를 파싱한다.
    pub fn parse(buf: &[u8]) -> Result<Self, String> {
        if buf.len() < 4 {
            return Err("corrupt: 레코드 헤더를 읽기에 바이트가 부족합니다".into());
        }

        let header_val = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let tag_id = header_val & 0x3FF; // 하위 10비트
        let level = (header_val >> 10) & 0x3FF; // 다음 10비트
        let size_field = (header_val >> 20) & 0xFFF; // 상위 12비트

        if size_field == 0xFFF {
            // 크기가 0xFFF (4095)이면 바로 뒤 4바이트가 실제 확장 크기
            if buf.len() < 8 {
                return Err("corrupt: 레코드 확장 크기(4바이트)를 읽기에 바이트가 부족합니다".into());
            }
            let extended_size = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
            Ok(Self {
                tag_id,
                level,
                size: extended_size,
                header_size: 8,
            })
        } else {
            Ok(Self {
                tag_id,
                level,
                size: size_field as usize,
                header_size: 4,
            })
        }
    }
}

/// HWP 5.0 파일에서 본문 문단 및 표 셀 문단을 Rust로 직접 추출한다.
///
/// # 반환값
/// - `Ok(Vec<Paragraph>)`: 추출된 문단 목록
/// - `Err("encrypted: ...")`: 암호화되거나 배포용 문서로 보호되어 있어 kordoc 대체 필요
/// - `Err("empty: ...")`: 본문 내용이 비어 있음
/// - `Err("corrupt: ...")`: 손상되었거나 파싱할 수 없는 파일
/// - `Err("unsupported: ...")`: HWP 5.0 미지원 버전
pub fn extract_hwp5(path: &Path) -> Result<Vec<Paragraph>, String> {
    let file = File::open(path).map_err(|e| format!("corrupt: HWP 파일 열기 실패: {e}"))?;
    let mut comp = CompoundFile::open(file).map_err(|e| format!("corrupt: CFB 복합 문서 파싱 실패: {e}"))?;

    // 1. FileHeader 스트림 읽기 및 검증
    let header_data = read_cfb_stream(&mut comp, "/FileHeader")
        .or_else(|_| read_cfb_stream(&mut comp, "FileHeader"))
        .map_err(|e| format!("corrupt: FileHeader 스트림 읽기 실패: {e}"))?;

    let file_header = HwpFileHeader::parse(&header_data)?;

    // 배포용 문서 또는 암호 설정 문서, DRM 보안 문서는 kordoc 폴백을 위해 Err("encrypted: ...") 반환
    if file_header.is_encrypted || file_header.is_distributed || file_header.is_drm {
        return Err("encrypted: 암호화되거나 배포용으로 보호된 HWP 문서".into());
    }

    // 2. BodyText 하위의 Section0, Section1, ... 스트림 목록 탐색 및 번호순 정렬
    let section_paths = find_section_streams(&comp)?;
    if section_paths.is_empty() {
        return Err("corrupt: HWP 본문 섹션(BodyText/Section*)을 찾을 수 없습니다".into());
    }

    let mut records: Vec<Paragraph> = Vec::new();
    let mut total_decompressed_bytes = 0u64;

    // 3. 각 Section 스트림 순회 파싱
    for (sec_index, sec_path) in section_paths.iter().enumerate() {
        let section_name = format!("Section{sec_index}");
        let raw_stream = read_cfb_stream(&mut comp, sec_path)
            .map_err(|e| format!("corrupt: {sec_path} 섹션 스트림 읽기 실패: {e}"))?;

        // 압축 해제 (raw Deflate)
        let decompressed = if file_header.is_compressed {
            decompress_section(&raw_stream, &section_name)?
        } else {
            raw_stream
        };

        total_decompressed_bytes = total_decompressed_bytes.saturating_add(decompressed.len() as u64);
        if total_decompressed_bytes > MAX_TOTAL_DECOMPRESSED_SIZE {
            return Err("corrupt: HWP 본문 전체 압축 해제 크기 제한(512MB)을 초과했습니다".into());
        }

        // 섹션 내부 레코드 해석 및 문단 추출
        extract_paragraphs_from_section(&decompressed, &section_name, &mut records)?;
    }

    if records.is_empty() {
        return Err("empty: 추출 가능한 HWP 본문이 없음".into());
    }

    Ok(records)
}

/// CFB 파일 내에서 지정된 경로의 스트림 데이터를 읽어온다.
fn read_cfb_stream<R: Read + std::io::Seek>(comp: &mut CompoundFile<R>, path: &str) -> Result<Vec<u8>, String> {
    let mut stream = comp.open_stream(path).map_err(|e| format!("스트림 '{path}' 열기 실패: {e}"))?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).map_err(|e| format!("스트림 '{path}' 읽기 실패: {e}"))?;
    Ok(buf)
}

/// CFB 내에서 Section 스트림 경로들을 찾아 숫자 순서대로 정렬하여 반환한다.
fn find_section_streams<R: Read + std::io::Seek>(comp: &CompoundFile<R>) -> Result<Vec<String>, String> {
    let mut sections = Vec::new();

    // cfb 엔트리 순회
    for entry in comp.walk() {
        if entry.is_stream() {
            let path_str = entry.path().to_string_lossy().to_string();
            let norm = path_str.replace('\\', "/");
            let lower = norm.to_ascii_lowercase();

            // /BodyText/Section0, BodyText/Section0 등 매칭
            if (lower.contains("bodytext/section") || lower.contains("/section")) && !lower.ends_with(".xml") {
                sections.push(norm);
            }
        }
    }

    // 섹션 번호로 정렬
    sections.sort_by_key(|s| {
        let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
        digits.parse::<u64>().unwrap_or(0)
    });

    Ok(sections)
}

/// Section 스트림 데이터를 raw Deflate(또는 zlib 폴백)로 압축 해제한다.
fn decompress_section(data: &[u8], section_name: &str) -> Result<Vec<u8>, String> {
    if data.is_empty() {
        return Ok(Vec::new());
    }

    let mut decompressed = Vec::new();
    let mut decoder = DeflateDecoder::new(data);

    // 섹션당 최대 크기 제한으로 DoS 방지
    let mut limited = (&mut decoder).take(MAX_SECTION_DECOMPRESSED_SIZE + 1);
    let read_res = limited.read_to_end(&mut decompressed);

    if read_res.is_err() {
        // 일부 비표준 툴로 생성되어 zlib 헤더가 포함된 경우 폴백 시도
        decompressed.clear();
        let mut zlib_decoder = ZlibDecoder::new(data);
        let mut zlib_limited = (&mut zlib_decoder).take(MAX_SECTION_DECOMPRESSED_SIZE + 1);
        if zlib_limited.read_to_end(&mut decompressed).is_err() {
            return Err(format!("corrupt: {section_name} 압축 해제(Deflate) 실패"));
        }
    }

    if decompressed.len() as u64 > MAX_SECTION_DECOMPRESSED_SIZE {
        return Err(format!("corrupt: {section_name} 압축 해제 크기(128MB) 제한 초과"));
    }

    Ok(decompressed)
}

/// HWP 5.0 컨트롤 종류 열거형
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwpCtrlKind {
    None,
    Table,         // b"tbl " (표)
    Header,        // b"head" (머리말)
    Footer,        // b"foot" (꼬리말)
    Footnote,      // b"fn  " (각주)
    Endnote,       // b"en  " (미주)
    TextBox,       // b"gso " 등 (글상자/도형 컨테이너)
    HiddenComment, // b"tcmt" (숨은 설명 / 숨은 글)
    Memo,          // b"memo" (메모)
    Dutmal,        // b"tdut" (덧말)
}

/// 레코드 트리 추적을 위한 스택 엔트리
#[derive(Debug, Clone)]
struct HierarchyNode {
    level: u32,
    kind: HwpCtrlKind,
}

/// 섹션의 바이너리 레코드들을 순회하며 문단(Paragraph)을 추출한다.
fn extract_paragraphs_from_section(
    data: &[u8],
    section_name: &str,
    records: &mut Vec<Paragraph>,
) -> Result<(), String> {
    let mut offset = 0usize;
    let total_len = data.len();

    // 계층 추적 스택 (표 및 컨트롤 중첩 파악)
    let mut stack: Vec<HierarchyNode> = Vec::new();
    let mut current_para_kind = HwpCtrlKind::None;

    while offset < total_len {
        // 남은 바이트가 4바이트 미만이면 헤더를 읽을 수 없음
        if offset + 4 > total_len {
            return Err(format!("corrupt: {section_name} 헤더가 잘려 있습니다"));
        }

        let record_header = RecordHeader::parse(&data[offset..])?;
        let header_len = record_header.header_size;
        let data_len = record_header.size;

        let record_data_start = offset + header_len;
        let record_data_end = record_data_start + data_len;

        if record_data_end > total_len {
            return Err(format!("corrupt: {section_name} 레코드 데이터가 잘려 있습니다 (tag={}, size={})", record_header.tag_id, data_len));
        }

        let record_bytes = &data[record_data_start..record_data_end];
        let record_level = record_header.level;

        // 계층 레벨 스택 갱신: 현재 레벨보다 크거나 같은 이전 노드들을 pop
        while let Some(top) = stack.last() {
            if top.level >= record_level {
                stack.pop();
            } else {
                break;
            }
        }

        // 현재 스택 상의 가장 안쪽 유효 컨트롤 종류 판별
        let current_kind = stack.iter().rev().find_map(|node| {
            if node.kind != HwpCtrlKind::None {
                Some(node.kind)
            } else {
                None
            }
        }).unwrap_or(HwpCtrlKind::None);

        match record_header.tag_id {
            HWPTAG_CTRL_HEADER => {
                // 첫 4바이트가 컨트롤 ID
                if record_bytes.len() >= 4 {
                    let ctrl_id = &record_bytes[0..4];
                    let kind = match ctrl_id {
                        b"tbl " => HwpCtrlKind::Table,
                        b"head" => HwpCtrlKind::Header,
                        b"foot" => HwpCtrlKind::Footer,
                        b"fn  " => HwpCtrlKind::Footnote,
                        b"en  " => HwpCtrlKind::Endnote,
                        b"gso " | b"$rec" | b"rect" | b"$lin" | b"line" | b"$ell" | b"elli"
                        | b"$arc" | b"arc " | b"$cur" | b"curv" | b"$pol" | b"poly"
                        | b"text" | b"tbox" => HwpCtrlKind::TextBox,
                        b"tcmt" => HwpCtrlKind::HiddenComment,
                        b"memo" => HwpCtrlKind::Memo,
                        b"tdut" => HwpCtrlKind::Dutmal,
                        _ => current_kind,
                    };
                    stack.push(HierarchyNode {
                        level: record_level,
                        kind,
                    });
                }
            }
            HWPTAG_LIST_HEADER => {
                // 표 셀의 문단 리스트 헤더 등: 부모 컨트롤의 kind 계승
                stack.push(HierarchyNode {
                    level: record_level,
                    kind: current_kind,
                });
            }
            HWPTAG_PARA_HEADER => {
                // 새 문단 시작: 현재 컨트롤 종류 기록
                current_para_kind = current_kind;
            }
            HWPTAG_PARA_TEXT => {
                // 문단 텍스트 디코딩 및 제어 문자 처리
                let text = decode_para_text(record_bytes);
                let trimmed = text.trim();

                if !trimmed.is_empty() {
                    let seq = records.len() + 1;
                    let (location_kind, is_table_cell) = match current_para_kind {
                        HwpCtrlKind::Table => ("table_cell".to_string(), true),
                        HwpCtrlKind::Header => ("header".to_string(), false),
                        HwpCtrlKind::Footer => ("footer".to_string(), false),
                        HwpCtrlKind::Footnote => ("footnote".to_string(), false),
                        HwpCtrlKind::Endnote => ("endnote".to_string(), false),
                        HwpCtrlKind::TextBox => ("textbox".to_string(), false),
                        HwpCtrlKind::HiddenComment => ("hidden_comment".to_string(), false),
                        HwpCtrlKind::Memo => ("memo".to_string(), false),
                        HwpCtrlKind::Dutmal => ("dutmal".to_string(), false),
                        HwpCtrlKind::None => ("paragraph".to_string(), false),
                    };

                    records.push(Paragraph {
                        text: trimmed.to_owned(),
                        location_kind,
                        location_label: format!("문단 {seq}"),
                        sequence: seq,
                        section: Some(section_name.to_owned()),
                        hierarchy_level: None,
                        is_table_cell,
                        style_name: None,
                        outline_level: None,
                    });
                }
            }
            _ => {
                // 기타 서식/컨트롤 레코드
            }
        }

        offset = record_data_end;
    }

    Ok(())
}

/// HWPTAG_PARA_TEXT 바이너리에서 UTF-16LE 텍스트를 디코딩하고 제어 문자(0~31)를 처리한다.
///
/// 제어 문자(0~31) 처리 규격:
/// - 1~8, 11, 14~23: 확장 컨트롤 및 인라인 컨트롤 (16바이트 = 8 WCHAR 건너뜀, 12는 폼 피드로 제외).
/// - 9: 탭 ('\t')
/// - 10, 12, 13: 줄바꿈 / 폼 피드 ('\n')
/// - 24: 하이픈 ('-')
/// - 25, 26: 묶음 빈칸, 고정폭 빈칸 (' ')
/// - 27~31: 특수 공백/문자 (' ')
/// - 0: 사용 불가 제어 문자 (1 WCHAR 건너뜀)
/// - 일반 문자: UTF-16LE 서러게이트 페어 및 유니코드 복원
pub fn decode_para_text(bytes: &[u8]) -> String {
    let n_wchars = bytes.len() / 2;
    let mut out = String::with_capacity(n_wchars);
    let mut i = 0usize;

    while i < n_wchars {
        let b0 = bytes[i * 2];
        let b1 = bytes[i * 2 + 1];
        let code = u16::from_le_bytes([b0, b1]);

        match code {
            // 1~8, 11, 14~23: 확장 컨트롤 및 인라인 컨트롤 (16바이트 = 8 WCHAR)
            // - 확장 컨트롤: 1..=3, 11, 14..=18, 21..=23
            // - 인라인 컨트롤: 4..=8 (필드 끝 등), 19, 20
            // (12는 폼 피드 인라인 줄바꿈이므로 제외)
            1..=8 | 11 | 14..=23 => {
                i = (i + 8).min(n_wchars);
            }
            // 9: 탭
            9 => {
                out.push('\t');
                i += 1;
            }
            // 10: 줄바꿈, 12: 폼 피드, 13: 문단 끝
            10 | 12 | 13 => {
                out.push('\n');
                i += 1;
            }
            // 24: 하이픈
            24 => {
                out.push('-');
                i += 1;
            }
            // 25, 26: 공백 (묶음 빈칸, 고정폭 빈칸)
            25 | 26 => {
                out.push(' ');
                i += 1;
            }
            // 27~31: 일반 문자 1개 크기 공백
            27..=31 => {
                out.push(' ');
                i += 1;
            }
            // 0: 사용 불가 문자 1 WCHAR 건너뜀
            0 => {
                i += 1;
            }

            // 일반 유니코드 문자
            _ => {
                if (0xD800..=0xDBFF).contains(&code) {
                    // High surrogate
                    if i + 1 < n_wchars {
                        let low_b0 = bytes[(i + 1) * 2];
                        let low_b1 = bytes[(i + 1) * 2 + 1];
                        let low = u16::from_le_bytes([low_b0, low_b1]);
                        if (0xDC00..=0xDFFF).contains(&low) {
                            if let Some(ch) = char::decode_utf16([code, low]).next().and_then(Result::ok) {
                                out.push(ch);
                            }
                            i += 2;
                            continue;
                        }
                    }
                    i += 1;
                } else if (0xDC00..=0xDFFF).contains(&code) {
                    // 고립된 low surrogate
                    i += 1;
                } else {
                    if let Some(ch) = char::from_u32(code as u32) {
                        out.push(ch);
                    }
                    i += 1;
                }
            }
        }
    }

    out
}

// =========================================================================
// 합성 HWP 5.0 생성기 (테스트 및 검증 지원용)
// =========================================================================

/// 테스트용 레코드 빌더
pub struct RecordBuilder {
    tag_id: u32,
    level: u32,
    data: Vec<u8>,
    force_extended_header: bool,
}

impl RecordBuilder {
    pub fn new(tag_id: u32, level: u32) -> Self {
        Self {
            tag_id,
            level,
            data: Vec::new(),
            force_extended_header: false,
        }
    }

    pub fn with_data(mut self, data: Vec<u8>) -> Self {
        self.data = data;
        self
    }

    pub fn force_extended(mut self) -> Self {
        self.force_extended_header = true;
        self
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let size = self.data.len();

        if size >= 0xFFF || self.force_extended_header {
            // 크기 필드에 0xFFF 설정 후 뒤에 4바이트 확장 크기 기록
            let header = (self.tag_id & 0x3FF) | ((self.level & 0x3FF) << 10) | (0xFFF << 20);
            out.extend_from_slice(&header.to_le_bytes());
            out.extend_from_slice(&(size as u32).to_le_bytes());
        } else {
            let header = (self.tag_id & 0x3FF) | ((self.level & 0x3FF) << 10) | (((size as u32) & 0xFFF) << 20);
            out.extend_from_slice(&header.to_le_bytes());
        }

        out.extend_from_slice(&self.data);
        out
    }
}

/// UTF-16LE 바이트열을 생성한다.
pub fn encode_utf16le(s: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(s.len() * 2);
    for u in s.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    bytes
}

/// 제어 문자(0~31)를 포함하는 텍스트 바이트열을 조립한다.
pub fn build_para_text_bytes(items: &[ParaTextItem]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for item in items {
        match item {
            ParaTextItem::String(s) => {
                bytes.extend_from_slice(&encode_utf16le(s));
            }
            ParaTextItem::Char(c) => {
                let mut buf = [0u16; 2];
                let encoded = c.encode_utf16(&mut buf);
                for u in encoded {
                    bytes.extend_from_slice(&u.to_le_bytes());
                }
            }
            ParaTextItem::ExtendedControl(code, extra_14_bytes) => {
                bytes.extend_from_slice(&code.to_le_bytes());
                if extra_14_bytes.len() == 14 {
                    bytes.extend_from_slice(extra_14_bytes);
                } else {
                    let mut padded = [0u8; 14];
                    let copy_len = extra_14_bytes.len().min(14);
                    padded[..copy_len].copy_from_slice(&extra_14_bytes[..copy_len]);
                    bytes.extend_from_slice(&padded);
                }
            }
            ParaTextItem::Control(code) => {
                bytes.extend_from_slice(&code.to_le_bytes());
            }
        }
    }
    bytes
}

#[derive(Debug, Clone)]
pub enum ParaTextItem {
    String(String),
    Char(char),
    ExtendedControl(u16, [u8; 14]),
    Control(u16),
}

/// 테스트용 합성 HWP 5.0 CFB 바이너리 생성기
pub struct SyntheticHwp5Builder {
    version: u32,
    flags: u32,
    section_records: Vec<Vec<u8>>,
}

impl SyntheticHwp5Builder {
    pub fn new() -> Self {
        Self {
            version: 0x05000000, // 5.0.0.0
            flags: FLAG_COMPRESSED, // 기본값: 압축
            section_records: Vec::new(),
        }
    }

    pub fn version(mut self, ver: u32) -> Self {
        self.version = ver;
        self
    }

    pub fn compressed(mut self, yes: bool) -> Self {
        if yes {
            self.flags |= FLAG_COMPRESSED;
        } else {
            self.flags &= !FLAG_COMPRESSED;
        }
        self
    }

    pub fn password_encrypted(mut self, yes: bool) -> Self {
        if yes {
            self.flags |= FLAG_PASSWORD_ENCRYPTED;
        } else {
            self.flags &= !FLAG_PASSWORD_ENCRYPTED;
        }
        self
    }

    pub fn distributed(mut self, yes: bool) -> Self {
        if yes {
            self.flags |= FLAG_DISTRIBUTED;
        } else {
            self.flags &= !FLAG_DISTRIBUTED;
        }
        self
    }

    pub fn add_record(mut self, record_bytes: Vec<u8>) -> Self {
        self.section_records.push(record_bytes);
        self
    }

    /// CFB 바이트 버퍼를 완성하여 반환한다.
    pub fn build(self) -> Result<Vec<u8>, String> {
        let cursor = Cursor::new(Vec::new());
        let mut comp = CompoundFile::create(cursor).map_err(|e| format!("CFB 생성 실패: {e}"))?;

        // 1. FileHeader 스트림 (256바이트)
        let mut file_header = [0u8; FILE_HEADER_SIZE];
        file_header[..HWP5_SIGNATURE.len()].copy_from_slice(HWP5_SIGNATURE);
        file_header[32..36].copy_from_slice(&self.version.to_le_bytes());
        file_header[36..40].copy_from_slice(&self.flags.to_le_bytes());

        {
            let mut s = comp.create_stream("/FileHeader").map_err(|e| format!("FileHeader 생성 실패: {e}"))?;
            s.write_all(&file_header).map_err(|e| format!("FileHeader 쓰기 실패: {e}"))?;
        }

        // 2. DocInfo 스트림 (최소한의 DocumentProperties)
        {
            let mut doc_info = Vec::new();
            let doc_props = RecordBuilder::new(HWPTAG_DOCUMENT_PROPERTIES, 0)
                .with_data(vec![0u8; 32])
                .to_bytes();
            doc_info.extend_from_slice(&doc_props);

            let mut s = comp.create_stream("/DocInfo").map_err(|e| format!("DocInfo 생성 실패: {e}"))?;
            s.write_all(&doc_info).map_err(|e| format!("DocInfo 쓰기 실패: {e}"))?;
        }

        // 3. BodyText 스토리지 생성 및 Section0 스트림 쓰기
        comp.create_storage("/BodyText").map_err(|e| format!("BodyText 스토리지 생성 실패: {e}"))?;

        let mut section_data = Vec::new();
        for rec in self.section_records {
            section_data.extend_from_slice(&rec);
        }

        let is_comp = (self.flags & FLAG_COMPRESSED) != 0;
        let final_section_bytes = if is_comp {
            use flate2::write::DeflateEncoder;
            use flate2::Compression;
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&section_data).map_err(|e| format!("섹션 압축 실패: {e}"))?;
            encoder.finish().map_err(|e| format!("섹션 압축 마감 실패: {e}"))?
        } else {
            section_data
        };

        {
            let mut s = comp.create_stream("/BodyText/Section0").map_err(|e| format!("Section0 생성 실패: {e}"))?;
            s.write_all(&final_section_bytes).map_err(|e| format!("Section0 쓰기 실패: {e}"))?;
        }

        let inner = comp.into_inner();
        Ok(inner.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;

    struct TempTestHwp {
        path: PathBuf,
    }

    impl TempTestHwp {
        fn new(name: &str, bytes: &[u8]) -> Self {
            let path = std::env::temp_dir().join(format!("klib_test_{name}_{}.hwp", std::process::id()));
            let mut file = File::create(&path).expect("임시 HWP 생성");
            file.write_all(bytes).expect("임시 HWP 쓰기");
            Self { path }
        }
    }

    impl Drop for TempTestHwp {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    #[test]
    fn test_hwp5_basic_and_table_cell() {
        let mut builder = SyntheticHwp5Builder::new().compressed(true);

        // 본문 문단 1
        builder = builder
            .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 0).with_data(vec![0u8; 18]).to_bytes())
            .add_record(
                RecordBuilder::new(HWPTAG_PARA_TEXT, 1)
                    .with_data(encode_utf16le("본문 첫 번째 문단입니다."))
                    .to_bytes(),
            );

        // 본문 문단 2 (표 포함)
        let para2_text_items = vec![
            ParaTextItem::String("표 안내: ".to_string()),
            ParaTextItem::ExtendedControl(11, [0u8; 14]),
        ];
        let para2_bytes = build_para_text_bytes(&para2_text_items);
        builder = builder
            .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 0).with_data(vec![0u8; 18]).to_bytes())
            .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 1).with_data(para2_bytes).to_bytes());

        // 표 컨트롤 및 셀 문단
        builder = builder.add_record(RecordBuilder::new(HWPTAG_CTRL_HEADER, 1).with_data(b"tbl \0\0\0\0\0\0\0\0\0\0\0\0".to_vec()).to_bytes());
        builder = builder.add_record(RecordBuilder::new(HWPTAG_TABLE, 2).with_data(vec![0u8; 32]).to_bytes());
        builder = builder.add_record(RecordBuilder::new(HWPTAG_LIST_HEADER, 2).with_data(vec![0u8; 16]).to_bytes());
        builder = builder
            .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 3).with_data(vec![0u8; 18]).to_bytes())
            .add_record(
                RecordBuilder::new(HWPTAG_PARA_TEXT, 4)
                    .with_data(encode_utf16le("표 셀 내용입니다."))
                    .to_bytes(),
            );

        let hwp_bytes = builder.build().expect("HWP 생성");
        let temp = TempTestHwp::new("basic", &hwp_bytes);

        let paras = extract_hwp5(&temp.path).expect("추출 성공");
        assert_eq!(paras.len(), 3);
        assert_eq!(paras[0].text, "본문 첫 번째 문단입니다.");
        assert_eq!(paras[0].location_kind, "paragraph");
        assert!(!paras[0].is_table_cell);

        assert_eq!(paras[1].text, "표 안내:");
        assert_eq!(paras[1].location_kind, "paragraph");
        assert!(!paras[1].is_table_cell);

        assert_eq!(paras[2].text, "표 셀 내용입니다.");
        assert_eq!(paras[2].location_kind, "table_cell");
        assert!(paras[2].is_table_cell);
    }

    #[test]
    fn test_hwp5_control_kinds_classification() {
        let mut builder = SyntheticHwp5Builder::new().compressed(true);

        // 1. 머리말 (b"head")
        builder = builder.add_record(RecordBuilder::new(HWPTAG_CTRL_HEADER, 0).with_data(b"head\0\0\0\0\0\0\0\0\0\0\0\0".to_vec()).to_bytes());
        builder = builder.add_record(RecordBuilder::new(HWPTAG_LIST_HEADER, 1).with_data(vec![0u8; 16]).to_bytes());
        builder = builder
            .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 2).with_data(vec![0u8; 18]).to_bytes())
            .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 3).with_data(encode_utf16le("머리말 텍스트")).to_bytes());

        // 2. 각주 (b"fn  ")
        builder = builder.add_record(RecordBuilder::new(HWPTAG_CTRL_HEADER, 0).with_data(b"fn  \0\0\0\0\0\0\0\0\0\0\0\0".to_vec()).to_bytes());
        builder = builder.add_record(RecordBuilder::new(HWPTAG_LIST_HEADER, 1).with_data(vec![0u8; 16]).to_bytes());
        builder = builder
            .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 2).with_data(vec![0u8; 18]).to_bytes())
            .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 3).with_data(encode_utf16le("각주 설명문")).to_bytes());

        // 3. 본문 문단
        builder = builder
            .add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 0).with_data(vec![0u8; 18]).to_bytes())
            .add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 1).with_data(encode_utf16le("일반 본문")).to_bytes());

        let hwp_bytes = builder.build().expect("HWP 생성");
        let temp = TempTestHwp::new("ctrls", &hwp_bytes);

        let paras = extract_hwp5(&temp.path).expect("추출 성공");
        assert_eq!(paras.len(), 3);
        assert_eq!(paras[0].location_kind, "header");
        assert!(!paras[0].is_table_cell);

        assert_eq!(paras[1].location_kind, "footnote");
        assert!(!paras[1].is_table_cell);

        assert_eq!(paras[2].location_kind, "paragraph");
        assert!(!paras[2].is_table_cell);
    }

    #[test]
    fn test_hwp5_formfeed_char_12_handling() {
        // 문자 코드 12(Form Feed)가 8 WCHAR 스킵되지 않고 정상 처리되는지 검증
        let mut text_bytes = Vec::new();
        // "앞부분"
        text_bytes.extend_from_slice(&encode_utf16le("앞부분"));
        // 코드 12 (0x000C, 폼 피드)
        text_bytes.extend_from_slice(&12u16.to_le_bytes());
        // "뒷부분" (만약 12가 8 WCHAR로 스킵되었다면 뒷부분 7글자가 삭제되었을 것임)
        text_bytes.extend_from_slice(&encode_utf16le("뒷부분글자"));

        let decoded = decode_para_text(&text_bytes);
        assert!(decoded.contains("앞부분"));
        assert!(decoded.contains("뒷부분글자"));
        assert_eq!(decoded.replace('\n', " "), "앞부분 뒷부분글자");
    }

    #[test]
    fn test_hwp5_extended_record_size_0xfff() {
        let mut builder = SyntheticHwp5Builder::new().compressed(false);
        builder = builder.add_record(RecordBuilder::new(HWPTAG_PARA_HEADER, 0).with_data(vec![0u8; 18]).to_bytes());

        let large_string = "가".repeat(5000);
        let large_bytes = encode_utf16le(&large_string);
        builder = builder.add_record(RecordBuilder::new(HWPTAG_PARA_TEXT, 1).with_data(large_bytes).to_bytes());

        let hwp_bytes = builder.build().expect("HWP 생성");
        let temp = TempTestHwp::new("large", &hwp_bytes);

        let paras = extract_hwp5(&temp.path).expect("추출 성공");
        assert_eq!(paras.len(), 1);
        assert_eq!(paras[0].text.chars().count(), 5000);
    }

    #[test]
    fn test_hwp5_security_flags() {
        // 암호 문서
        let encrypted_bytes = SyntheticHwp5Builder::new()
            .password_encrypted(true)
            .build()
            .unwrap();
        let temp_enc = TempTestHwp::new("enc", &encrypted_bytes);
        let res_enc = extract_hwp5(&temp_enc.path);
        assert!(res_enc.is_err());
        assert!(res_enc.unwrap_err().contains("encrypted"));

        // 배포용 문서
        let dist_bytes = SyntheticHwp5Builder::new()
            .distributed(true)
            .build()
            .unwrap();
        let temp_dist = TempTestHwp::new("dist", &dist_bytes);
        let res_dist = extract_hwp5(&temp_dist.path);
        assert!(res_dist.is_err());
        assert!(res_dist.unwrap_err().contains("encrypted"));
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

        let hwp_bytes = builder.build().expect("HWP 생성");
        let temp = TempTestHwp::new("gso_tcmt", &hwp_bytes);

        let paras = extract_hwp5(&temp.path).expect("추출 성공");
        assert_eq!(paras.len(), 2);
        assert_eq!(paras[0].location_kind, "textbox");
        assert_eq!(paras[0].text, "글상자 내용문");
        assert_eq!(paras[1].location_kind, "hidden_comment");
        assert_eq!(paras[1].text, "숨은 설명 메모");
    }

    #[test]
    fn test_hwp5_inline_control_code_4_skip() {
        // 코드 4(필드 끝 등 인라인 컨트롤)는 8 WCHAR(16바이트)를 차지하므로,
        // 코드 4 뒤의 7 WCHAR 가비지 정보가 정상적으로 건너뛰어지는지 검증
        let mut text_bytes = Vec::new();
        text_bytes.extend_from_slice(&encode_utf16le("본문앞"));
        // 코드 4 (1 WCHAR) + 부가정보 (6 WCHAR) + 코드 4 (1 WCHAR) = 총 8 WCHAR
        text_bytes.extend_from_slice(&4u16.to_le_bytes());
        for _ in 0..7 {
            text_bytes.extend_from_slice(&0x1234u16.to_le_bytes()); // 가비지 부가정보
        }
        text_bytes.extend_from_slice(&encode_utf16le("본문뒤"));

        let decoded = decode_para_text(&text_bytes);
        assert_eq!(decoded, "본문앞본문뒤");
    }
}
