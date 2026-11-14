// Copyright (c) 2026 haijun93 (마포구청 공혁준). PolyForm Strict License 1.0.0.
//! 문서 단위 보조 비트맵 색인 (SEMI2-docbitmap).
//!
//! 1. 2글자 조각별 문서 번호 Roaring 비트맵
//! 2. 폴더 구간표 (경로 순서 부여, start..=end)
//! 3. 시간 단위 문서 비트맵 (일/월/년 계층 비트맵, 기간 조합)
//! 4. 1등급 비트맵 3종:
//!    - 삭제 표시 (tombstone) 문서 비트맵
//!    - 파일 형식별 (hwp, hwpx, pdf, xlsx, md...) 문서 비트맵
//!    - 문장 역할별 (제목, 표, 본문, 붙임) 문장 비트맵
use std::path::Path;

pub const WORD_PREFIX: u8 = 0x00;
pub const DOC_GRAM_PREFIX: u8 = 0x01;
pub const FILE_TYPE_PREFIX: u8 = 0x02;
pub const ROLE_PREFIX: u8 = 0x03;
pub const DATE_PREFIX: u8 = 0x04;
pub const MONTH_PREFIX: u8 = 0x05;
pub const YEAR_PREFIX: u8 = 0x06;
pub const FOLDER_PREFIX: u8 = 0x07;

/// 검색 범위(단위).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SearchScope {
    /// 기본값: 한 문장 안에 검색어의 모든 낱말이 들어 있어야 일치.
    #[default]
    Sentence,
    /// 문서 단위: 같은 문서 안의 여러 문장에 낱말들이 나뉘어 있어도 일치.
    Document,
    /// 폴더 단위: 같은 폴더 안의 여러 문서에 나뉘어 있어도 일치.
    Folder,
}

/// 문장의 역할(위계·위치 판정 연결).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum SentenceRole {
    Title = 1,
    Table = 2,
    Body = 3,
    Attachment = 4,
}

impl SentenceRole {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(SentenceRole::Title),
            2 => Some(SentenceRole::Table),
            3 => Some(SentenceRole::Body),
            4 => Some(SentenceRole::Attachment),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SentenceRole::Title => "title",
            SentenceRole::Table => "table",
            SentenceRole::Body => "body",
            SentenceRole::Attachment => "attachment",
        }
    }
}

/// 문장 특성(위계, 문단, 내용)을 바탕으로 문장 역할을 자동 판정한다.
pub fn detect_sentence_role(level: u8, _para: u32, text: &str) -> SentenceRole {
    let trimmed = text.trim();
    if trimmed.starts_with("[붙임")
        || trimmed.starts_with("<붙임")
        || trimmed.starts_with("[별표")
        || trimmed.starts_with("<별표")
        || trimmed.starts_with("[첨부")
        || trimmed.starts_with("<첨부")
    {
        return SentenceRole::Attachment;
    }
    // 위계 level 0은 제목으로 판정
    if level == 0 {
        return SentenceRole::Title;
    }
    // 표 안의 셀 텍스트 특성 (예: 짧은 항목 나열, 구분자 등)
    if trimmed.starts_with('|') || trimmed.contains("\t") {
        return SentenceRole::Table;
    }
    SentenceRole::Body
}

/// 폴더별 결과 수 집계 항목.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderCount {
    pub folder: String,
    pub doc_count: u32,
    pub sentence_count: u64,
}

/// 날짜 구간 집계 간격.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DateInterval {
    Year,
    #[default]
    Month,
    Week,
    Day,
}

/// 날짜별 결과 수 집계 항목(막대그래프용).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateCount {
    pub date_key: u32,
    pub label: String,
    pub doc_count: u32,
    pub sentence_count: u64,
}

/// 문서 추가 시 제공할 수 있는 보조 메타데이터.
#[derive(Clone, Debug, Default)]
pub struct DocumentMeta {
    /// 문서 경로 (예: "총무과/2026/계약/공사계약서.hwp"). 폴더 및 파일 형식 자동 추출.
    pub path: Option<String>,
    /// 파일 형식 (예: "hwp", "hwpx", "pdf", "xlsx", "md"). 미지정 시 path에서 추출.
    pub file_type: Option<String>,
}

/// 경로에서 정규화된 폴더 경로를 추출한다.
/// 예: "a/b/c.hwp" -> "a/b", "c.hwp" -> "", "a\\b\\c.hwp" -> "a/b"
pub fn extract_folder(path: &str) -> String {
    let p = path.replace('\\', "/");
    let trimmed = p.trim_matches('/');
    match trimmed.rfind('/') {
        Some(pos) => trimmed[..pos].to_string(),
        None => String::new(),
    }
}

/// 경로 또는 문자열에서 소문자 확장자(파일 형식)를 추출한다.
pub fn extract_file_type(path: &str) -> String {
    let p = Path::new(path);
    p.extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default()
}

/// 날짜(i64)를 (year, month, day)로 정규화한다.
/// - 8자리 정수 (예: 20260115)
/// - Unix 타임스탬프 (초)
/// - 연도 정수 (예: 2026)
pub fn normalize_date(date: i64) -> (u32, u32, u32) {
    if (19000101..=21001231).contains(&date) {
        let y = (date / 10000) as u32;
        let m = ((date / 100) % 100) as u32;
        let d = (date % 100) as u32;
        return (y, m.clamp(1, 12), d.clamp(1, 31));
    }
    if date > 100_000_000 {
        return unix_to_ymd(date);
    }
    if (1900..=2100).contains(&date) {
        return (date as u32, 1, 1);
    }
    (date.max(0) as u32, 1, 1)
}

/// Unix 초 타임스탬프를 UTC (year, month, day)로 변환한다.
pub fn unix_to_ymd(secs: i64) -> (u32, u32, u32) {
    let days = secs.div_euclid(86400);
    // 1970-01-01을 기준으로 일 수를 년/월/일로 환산
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    (y.clamp(1900, 2100) as u32, m, d)
}

pub fn ymd_to_key(y: u32, m: u32, d: u32) -> u32 {
    y * 10000 + m * 100 + d
}

pub fn ym_to_key(y: u32, m: u32) -> u32 {
    y * 100 + m
}
