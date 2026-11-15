//! 퀵몽땅검색 결과 CSV/TSV 내보내기 모듈 (QMS-EXPORT).
//!
//! - UTF-8 BOM (`\u{FEFF}`) 포함으로 Excel에서 한글 깨짐 방지
//! - CSV: RFC 4180 호환 (쉼표, 쌍따옴표, 줄바꿈 필드 인용 및 `"` -> `""` 2중 이스케이프)
//! - TSV: 탭 구분자 및 개행/따옴표 안전 이스케이프
//! - 상한: 화면 상한과 무관하게 최대 1,000건 (`EXPORT_LIMIT`)
//! - 열 구성:
//!   - 본문 탭: 번호, 파일명, 경로, 형식, 수정일, 일치 문장
//!   - 파일명 탭: 번호, 파일명, 경로, 형식, 수정일, 크기
//! - 저장 위치: 사용자 문서 폴더의 '퀵몽땅검색_결과_날짜시각.csv' (또는 `.tsv`)
//! - 외부 네트워크 전송 없음 (순수 로컬 파일 저장)

use crate::backend::Row;
use crate::tm_state::Tab;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// 검색 결과 내보내기 최대 건수.
pub const EXPORT_LIMIT: usize = 1000;

/// UTF-8 바이트 순서 표식 (BOM).
pub const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// 내보내기 형식: CSV 또는 TSV.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Csv,
    Tsv,
}

impl ExportFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Csv => "csv",
            ExportFormat::Tsv => "tsv",
        }
    }

    pub fn mime_or_desc(&self) -> &'static str {
        match self {
            ExportFormat::Csv => "CSV (Excel 호환, UTF-8 BOM)",
            ExportFormat::Tsv => "TSV (탭 구분 텍스트, UTF-8 BOM)",
        }
    }
}

/// 내보내기 대상 탭 종류 (본문/위계 vs 파일명).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    Body,
    Names,
}

impl From<Tab> for ExportKind {
    fn from(tab: Tab) -> Self {
        match tab {
            Tab::Names => ExportKind::Names,
            Tab::Body | Tab::Hierarchy => ExportKind::Body,
        }
    }
}

/// 내보내기 결과 레코드 DTO.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ExportRecord {
    pub index: usize,
    pub filename: String,
    pub path: String,
    pub format: String,
    pub modified: String,
    pub extra: String,
}

/// `Row` 슬라이스로부터 최대 `EXPORT_LIMIT`(1,000건)개의 `ExportRecord` DTO 목록을 생성한다.
pub fn rows_to_records(rows: &[Row], kind: ExportKind) -> Vec<ExportRecord> {
    rows.iter()
        .take(EXPORT_LIMIT)
        .enumerate()
        .map(|(i, row)| row_to_record(i + 1, row, kind))
        .collect()
}

fn row_to_record(index: usize, row: &Row, kind: ExportKind) -> ExportRecord {
    let filename = if !row.name.is_empty() {
        row.name.clone()
    } else {
        Path::new(&row.path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    };

    let p = Path::new(&row.path);
    let format = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_uppercase())
        .unwrap_or_else(|| {
            if row.location.contains("폴더") {
                "폴더".to_string()
            } else {
                String::new()
            }
        });

    let modified = resolve_modified(p, &row.location);

    let extra = match kind {
        ExportKind::Body => {
            if !row.snippet.is_empty() {
                row.snippet.clone()
            } else {
                row.location.clone()
            }
        }
        ExportKind::Names => {
            // 파일명 탭의 location 예: "12.3 KB · 2026-11-14 09:05" 또는 "폴더"
            if let Some(pos) = row.location.find(" · ") {
                row.location[..pos].trim().to_string()
            } else if !row.location.is_empty() {
                row.location.trim().to_string()
            } else {
                String::new()
            }
        }
    };

    ExportRecord {
        index,
        filename,
        path: row.path.clone(),
        format,
        modified,
        extra,
    }
}

/// 파일의 수정 시각을 읽어 "YYYY-MM-DD HH:MM:SS" 형식으로 변환한다. 실패 시 location 문자열의 날짜를 사용한다.
fn resolve_modified(path: &Path, fallback_location: &str) -> String {
    if let Ok(meta) = std::fs::metadata(path) {
        if let Ok(mtime) = meta.modified() {
            let (y, m, d, hh, mm, ss) = system_time_to_kst(mtime);
            return format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02}");
        }
    }
    // location에 "2026-11-14 09:05" 등의 형식이 있는 경우 추출
    if let Some(pos) = fallback_location.rfind(" · ") {
        let tail = fallback_location[pos + 3..].trim();
        if tail.len() >= 10 && tail.contains('-') {
            return tail.to_string();
        }
    }
    String::new()
}

/// `SystemTime`을 한국 표준시(KST: UTC+9) (year, month, day, hour, min, sec)로 변환한다.
pub fn system_time_to_kst(t: SystemTime) -> (u32, u32, u32, u32, u32, u32) {
    let dur = t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let sec = dur.as_secs() as i64 + 9 * 3600;
    let (y, m, d) = kfind_core::unix_to_ymd(sec);
    let day_sec = sec.rem_euclid(86400);
    let hh = (day_sec / 3600) as u32;
    let mm = ((day_sec % 3600) / 60) as u32;
    let ss = (day_sec % 60) as u32;
    (y, m, d, hh, mm, ss)
}

/// CSV 필드 이스케이프 (RFC 4180):
/// 필드에 쉼표(`,`), 쌍따옴표(`"`), 개행(`\r`, `\n`)이 포함되어 있으면
/// 필드 전체를 쌍따옴표로 감싸고 내부 `"`는 `""`로 2중 치환한다.
pub fn escape_csv_field(val: &str) -> String {
    if val.contains(['"', ',', '\r', '\n']) {
        let escaped = val.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        val.to_string()
    }
}

/// TSV 필드 이스케이프:
/// 필드에 탭(`\t`), 쌍따옴표(`"`), 개행(`\r`, `\n`)이 포함되어 있으면
/// 필드 전체를 쌍따옴표로 감싸고 내부 `"`는 `""`로 치환한다 (Excel 호환).
pub fn escape_tsv_field(val: &str) -> String {
    if val.contains(['"', '\t', '\r', '\n']) {
        let escaped = val.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        val.to_string()
    }
}

/// 헤더 열 이름 배열 반환.
pub fn header_columns(kind: ExportKind) -> [&'static str; 6] {
    match kind {
        ExportKind::Body => ["번호", "파일명", "경로", "형식", "수정일", "일치 문장"],
        ExportKind::Names => ["번호", "파일명", "경로", "형식", "수정일", "크기"],
    }
}

/// 레코드 목록을 문자열로 직렬화한다 (CSV 또는 TSV, CRLF 줄바꿈, UTF-8 BOM 포함).
pub fn serialize_to_string(format: ExportFormat, kind: ExportKind, records: &[ExportRecord]) -> String {
    let mut out = String::from("\u{FEFF}");
    let cols = header_columns(kind);

    match format {
        ExportFormat::Csv => {
            // 헤더
            for (i, col) in cols.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&escape_csv_field(col));
            }
            out.push_str("\r\n");

            // 데이터 행 (최대 1,000건 보장)
            for rec in records.iter().take(EXPORT_LIMIT) {
                out.push_str(&rec.index.to_string());
                out.push(',');
                out.push_str(&escape_csv_field(&rec.filename));
                out.push(',');
                out.push_str(&escape_csv_field(&rec.path));
                out.push(',');
                out.push_str(&escape_csv_field(&rec.format));
                out.push(',');
                out.push_str(&escape_csv_field(&rec.modified));
                out.push(',');
                out.push_str(&escape_csv_field(&rec.extra));
                out.push_str("\r\n");
            }
        }
        ExportFormat::Tsv => {
            // 헤더
            for (i, col) in cols.iter().enumerate() {
                if i > 0 {
                    out.push('\t');
                }
                out.push_str(&escape_tsv_field(col));
            }
            out.push_str("\r\n");

            // 데이터 행 (최대 1,000건 보장)
            for rec in records.iter().take(EXPORT_LIMIT) {
                out.push_str(&rec.index.to_string());
                out.push('\t');
                out.push_str(&escape_tsv_field(&rec.filename));
                out.push('\t');
                out.push_str(&escape_tsv_field(&rec.path));
                out.push('\t');
                out.push_str(&escape_tsv_field(&rec.format));
                out.push('\t');
                out.push_str(&escape_tsv_field(&rec.modified));
                out.push('\t');
                out.push_str(&escape_tsv_field(&rec.extra));
                out.push_str("\r\n");
            }
        }
    }
    out
}

/// 레코드 목록을 바이트(UTF-8 BOM 포함)로 직렬화한다.
pub fn serialize_to_bytes(format: ExportFormat, kind: ExportKind, records: &[ExportRecord]) -> Vec<u8> {
    serialize_to_string(format, kind, records).into_bytes()
}

/// 사용자 문서 폴더 경로를 구한다.
pub fn documents_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(userprofile) = std::env::var_os("USERPROFILE") {
            let doc = PathBuf::from(&userprofile).join("Documents");
            if doc.is_dir() {
                return doc;
            }
            let doc_ko = PathBuf::from(&userprofile).join("문서");
            if doc_ko.is_dir() {
                return doc_ko;
            }
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let doc = PathBuf::from(home).join("Documents");
            if doc.is_dir() {
                return doc;
            }
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// 내보내기 파일명 생성: '퀵몽땅검색_결과_날짜시각.확장자'
/// (예: `퀵몽땅검색_결과_20261116_064526.csv`)
pub fn generate_filename(format: ExportFormat, now: SystemTime) -> String {
    let (y, m, d, hh, mm, ss) = system_time_to_kst(now);
    format!(
        "퀵몽땅검색_결과_{y:04}{m:02}{d:02}_{hh:02}{mm:02}{ss:02}.{}",
        format.extension()
    )
}

/// 지정된 디렉터리와 현재 시각을 기준으로 전체 저장 경로를 생성한다.
pub fn generate_export_path(dir: &Path, format: ExportFormat, now: SystemTime) -> PathBuf {
    dir.join(generate_filename(format, now))
}

/// 결과를 파일로 저장하고 생성된 파일 경로를 반환한다.
pub fn export_to_file(
    format: ExportFormat,
    kind: ExportKind,
    rows: &[Row],
    target_dir: Option<&Path>,
    now: Option<SystemTime>,
) -> std::io::Result<PathBuf> {
    let records = rows_to_records(rows, kind);
    let bytes = serialize_to_bytes(format, kind, &records);
    let dir_buf;
    let base_dir = match target_dir {
        Some(d) => d,
        None => {
            dir_buf = documents_dir();
            &dir_buf
        }
    };
    if !base_dir.is_dir() {
        std::fs::create_dir_all(base_dir)?;
    }
    let now = now.unwrap_or_else(SystemTime::now);
    let path = generate_export_path(base_dir, format, now);
    std::fs::write(&path, bytes)?;
    Ok(path)
}

/// 저장된 파일이 있는 폴더를 열고(가능한 경우 파일 선택), 성공 여부를 반환한다.
pub fn open_export_folder(saved_file: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let path_str = saved_file.to_string_lossy();
        crate::open::open(&path_str, true)
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("-R")
            .arg(saved_file)
            .spawn();
        Ok(())
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        if let Some(parent) = saved_file.parent() {
            let _ = std::process::Command::new("xdg-open")
                .arg(parent)
                .spawn();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_results_export() {
        let empty_rows: Vec<Row> = Vec::new();
        let records = rows_to_records(&empty_rows, ExportKind::Body);
        assert_eq!(records.len(), 0);

        let csv = serialize_to_string(ExportFormat::Csv, ExportKind::Body, &records);
        assert!(csv.starts_with('\u{FEFF}'));
        let lines: Vec<&str> = csv.trim_start_matches('\u{FEFF}').trim_end().split("\r\n").collect();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "번호,파일명,경로,형식,수정일,일치 문장");

        let tsv = serialize_to_string(ExportFormat::Tsv, ExportKind::Names, &records);
        assert!(tsv.starts_with('\u{FEFF}'));
        let tsv_lines: Vec<&str> = tsv.trim_start_matches('\u{FEFF}').trim_end().split("\r\n").collect();
        assert_eq!(tsv_lines.len(), 1);
        assert_eq!(tsv_lines[0], "번호\t파일명\t경로\t형식\t수정일\t크기");
    }

    #[test]
    fn test_escape_table_15_cases() {
        // 이스케이프 15건 이상 표 시험: 특수문자, 쉼표, 쌍따옴표, 줄바꿈, 탭, 수식 등
        let test_cases = [
            ("순수 영문", "Simple English Text", "Simple English Text", "Simple English Text"),
            ("순수 한글", "정상적인 한글 문장입니다.", "정상적인 한글 문장입니다.", "정상적인 한글 문장입니다."),
            ("쉼표 포함", "가, 나, 다", "\"가, 나, 다\"", "가, 나, 다"),
            ("쌍따옴표 포함", "그는 \"안녕\"이라 말했다.", "\"그는 \"\"안녕\"\"이라 말했다.\"", "\"그는 \"\"안녕\"\"이라 말했다.\""),
            ("LF 줄바꿈", "첫 줄\n둘째 줄", "\"첫 줄\n둘째 줄\"", "\"첫 줄\n둘째 줄\""),
            ("CRLF 줄바꿈", "첫 줄\r\n둘째 줄", "\"첫 줄\r\n둘째 줄\"", "\"첫 줄\r\n둘째 줄\""),
            ("탭 포함", "열1\t열2", "열1\t열2", "\"열1\t열2\""),
            ("쉼표+따옴표", "\"강조\", 쉼표", "\"\"\"강조\"\", 쉼표\"", "\"\"\"강조\"\", 쉼표\""),
            ("쉼표+줄바꿈", "줄1, 항목\n줄2", "\"줄1, 항목\n줄2\"", "\"줄1, 항목\n줄2\""),
            ("따옴표+줄바꿈", "\"인용문\"\n다음 줄", "\"\"\"인용문\"\"\n다음 줄\"", "\"\"\"인용문\"\"\n다음 줄\""),
            ("쉼표+따옴표+줄바꿈", "\"A, B\"\r\n\"C, D\"", "\"\"\"A, B\"\"\r\n\"\"C, D\"\"\"", "\"\"\"A, B\"\"\r\n\"\"C, D\"\"\""),
            ("단일 따옴표만", "\"", "\"\"\"\"", "\"\"\"\""),
            ("연속 따옴표", "\"\"", "\"\"\"\"\"\"", "\"\"\"\"\"\""),
            ("앞뒤 공백+따옴표", "  \"앞뒤공백\"  ", "\"  \"\"앞뒤공백\"\"  \"", "\"  \"\"앞뒤공백\"\"  \""),
            ("수식 기호 및 쉼표", "=SUM(A1:A10), \"합계\"", "\"=SUM(A1:A10), \"\"합계\"\"\"", "\"=SUM(A1:A10), \"\"합계\"\"\""),
            ("복합 한글 특수문자", "[특보] \"서울, 경기\n주의보\"", "\"[특보] \"\"서울, 경기\n주의보\"\"\"", "\"[특보] \"\"서울, 경기\n주의보\"\"\""),
        ];

        assert!(test_cases.len() >= 15, "15건 이상이어야 함 (실제: {})", test_cases.len());

        for (desc, input, expected_csv, expected_tsv) in &test_cases {
            let escaped_csv = escape_csv_field(input);
            assert_eq!(&escaped_csv, expected_csv, "CSV 이스케이프 실패: {desc}");

            let escaped_tsv = escape_tsv_field(input);
            assert_eq!(&escaped_tsv, expected_tsv, "TSV 이스케이프 실패: {desc}");
        }
    }

    #[test]
    fn test_korean_paths_and_special_chars() {
        let rows = vec![
            Row {
                doc_id: 1,
                path: "C:\\사용자\\문서\\보고서(최종, 2026).hwpx".to_string(),
                name: "보고서(최종, 2026).hwpx".to_string(),
                parent: "C:\\사용자\\문서".to_string(),
                snippet: "본문 일치 문장: \"중요 결재\" 내용 확인.".to_string(),
                location: "문단 3 · 위계 1".to_string(),
                ..Row::default()
            },
            Row {
                doc_id: 2,
                path: "D:\\공유 폴더\\2026년\\기안서\t샘플.pdf".to_string(),
                name: "기안서\t샘플.pdf".to_string(),
                parent: "D:\\공유 폴더\\2026년".to_string(),
                snippet: "첫 줄\n둘째 줄 일치 내용".to_string(),
                location: "문단 1 · 위계 0".to_string(),
                ..Row::default()
            },
        ];

        let records = rows_to_records(&rows, ExportKind::Body);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].format, "HWPX");
        assert_eq!(records[1].format, "PDF");

        let csv = serialize_to_string(ExportFormat::Csv, ExportKind::Body, &records);
        assert!(csv.contains("\"C:\\사용자\\문서\\보고서(최종, 2026).hwpx\""));
        assert!(csv.contains("\"본문 일치 문장: \"\"중요 결재\"\" 내용 확인.\""));
        assert!(csv.contains("\"첫 줄\n둘째 줄 일치 내용\""));

        let tsv = serialize_to_string(ExportFormat::Tsv, ExportKind::Body, &records);
        assert!(tsv.contains("\"기안서\t샘플.pdf\""));
    }

    #[test]
    fn test_export_limit_1000() {
        let mut rows = Vec::new();
        for i in 1..=1500 {
            rows.push(Row {
                doc_id: i as u64,
                path: format!("C:\\문서\\doc_{i:04}.hwp"),
                name: format!("doc_{i:04}.hwp"),
                parent: "C:\\문서".to_string(),
                snippet: format!("일치 문장 {i}"),
                location: format!("문단 {i} · 위계 0"),
                ..Row::default()
            });
        }

        let records = rows_to_records(&rows, ExportKind::Body);
        assert_eq!(records.len(), 1000, "1000건으로 상한 제한되어야 함");
        assert_eq!(records[0].index, 1);
        assert_eq!(records[999].index, 1000);
        assert_eq!(records[999].filename, "doc_1000.hwp");

        let csv = serialize_to_string(ExportFormat::Csv, ExportKind::Body, &records);
        let lines: Vec<&str> = csv.trim_start_matches('\u{FEFF}').trim_end().split("\r\n").collect();
        // 헤더 1줄 + 데이터 1000줄 = 1001줄
        assert_eq!(lines.len(), 1001);
        assert_eq!(lines[1], "1,doc_0001.hwp,C:\\문서\\doc_0001.hwp,HWP,,일치 문장 1");
        assert_eq!(lines[1000], "1000,doc_1000.hwp,C:\\문서\\doc_1000.hwp,HWP,,일치 문장 1000");
    }

    #[test]
    fn test_file_save_and_path_generation() {
        let temp_dir = std::env::temp_dir().join(format!("ktm-export-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);

        let rows = vec![Row {
            doc_id: 10,
            path: "C:\\테스트\\문서.hwpx".to_string(),
            name: "문서.hwpx".to_string(),
            parent: "C:\\테스트".to_string(),
            snippet: "일치 내용".to_string(),
            location: "문단 1 · 위계 0".to_string(),
            ..Row::default()
        }];

        let test_time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1794725126); // 2026년 특정 시각
        let saved_path = export_to_file(
            ExportFormat::Csv,
            ExportKind::Body,
            &rows,
            Some(&temp_dir),
            Some(test_time),
        ).unwrap();

        assert!(saved_path.is_file());
        let filename = saved_path.file_name().unwrap().to_string_lossy();
        assert!(filename.starts_with("퀵몽땅검색_결과_"));
        assert!(filename.ends_with(".csv"));

        let bytes = std::fs::read(&saved_path).unwrap();
        assert!(bytes.starts_with(UTF8_BOM));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
