//! Text and HWPX paragraph extraction shared by the CLI and index pipeline.
use crate::json::Value;
use encoding_rs::EUC_KR;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Paragraph {
    pub text: String,
    pub location_kind: String,
    pub location_label: String,
    pub sequence: usize,
    pub section: Option<String>,
    pub hierarchy_level: Option<String>,
    pub is_table_cell: bool,
    pub style_name: Option<String>,
    pub outline_level: Option<i64>,
}

impl Paragraph {
    fn line(text: String, line: usize, sequence: usize) -> Self {
        Self { text, location_kind: "line".into(), location_label: format!("{line}행"), sequence,
            section: None, hierarchy_level: None, is_table_cell: false, style_name: None, outline_level: None }
    }
    pub fn to_json(&self) -> Value {
        Value::obj(vec![
            ("text", self.text.clone().into()), ("location_kind", self.location_kind.as_str().into()),
            ("location_label", self.location_label.as_str().into()), ("sequence", self.sequence.into()),
            ("section", self.section.clone().map(Value::Str).unwrap_or(Value::Null)),
            ("hierarchy_level", self.hierarchy_level.clone().map(Value::Str).unwrap_or(Value::Null)),
            ("is_table_cell", self.is_table_cell.into()),
            ("style_name", self.style_name.clone().map(Value::Str).unwrap_or(Value::Null)),
            ("outline_level", self.outline_level.map(Value::from).unwrap_or(Value::Null)),
        ])
    }
}

fn decode_text(raw: &[u8]) -> Result<String, String> {
    if raw.starts_with(&[0xef, 0xbb, 0xbf]) {
        return std::str::from_utf8(&raw[3..]).map(str::to_owned).map_err(|e| format!("decode_error: UTF-8 BOM 디코딩 실패: {e}"));
    }
    if raw.starts_with(&[0xff, 0xfe]) {
        if (raw.len() - 2) % 2 != 0 { return Err("decode_error: UTF-16 BOM 디코딩 실패".into()); }
        let units = raw[2..].chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        return char::decode_utf16(units).map(|r| r.map_err(|_| ())).collect::<Result<String, _>>().map_err(|_| "decode_error: UTF-16 BOM 디코딩 실패".into());
    }
    if raw.starts_with(&[0xfe, 0xff]) {
        if (raw.len() - 2) % 2 != 0 { return Err("decode_error: UTF-16 BOM 디코딩 실패".into()); }
        let units = raw[2..].chunks_exact(2).map(|b| u16::from_be_bytes([b[0], b[1]]));
        return char::decode_utf16(units).map(|r| r.map_err(|_| ())).collect::<Result<String, _>>().map_err(|_| "decode_error: UTF-16 BOM 디코딩 실패".into());
    }
    if let Ok(s) = std::str::from_utf8(raw) { return Ok(s.to_owned()); }
    let (decoded, _, had_errors) = EUC_KR.decode(raw);
    if had_errors { Err("decode_error: 텍스트 인코딩 판별 실패(UTF-8/CP949 모두 실패)".into()) } else { Ok(decoded.into_owned()) }
}

fn extract_text(path: &Path) -> Result<Vec<Paragraph>, String> {
    let raw = std::fs::read(path).map_err(|e| format!("corrupt: 파일을 읽을 수 없음: {e}"))?;
    if raw.is_empty() { return Err("empty: 파일 내용이 비어 있음".into()); }
    let normalized = decode_text(&raw)?.replace("\r\n", "\n").replace('\r', "\n");
    let mut records = Vec::new();
    for (idx, line) in normalized.split('\n').enumerate() {
        let stripped = line.trim();
        if !stripped.is_empty() { records.push(Paragraph::line(stripped.to_owned(), idx + 1, records.len() + 1)); }
    }
    if records.is_empty() { return Err("empty: 추출 가능한 텍스트가 없음".into()); }
    Ok(records)
}

pub fn extract(path: &Path) -> Result<Vec<Paragraph>, String> {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "txt" | "md" => extract_text(path),
        "hwpx" => Err("unsupported: HWPX 추출은 다음 단위에서 구현 예정".into()),
        ext => Err(format!("unsupported: 지원하지 않는 문서 형식: .{ext}")),
    }
}
