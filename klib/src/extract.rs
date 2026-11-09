//! Text and HWPX paragraph extraction shared by the CLI and index pipeline.
use crate::json::Value;
use encoding_rs::EUC_KR;
use std::path::Path;
use std::io::Read;
use zip::ZipArchive;

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

/// Extracted paragraph enriched with the shared hierarchy compatibility result.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedParagraph {
    pub paragraph: Paragraph,
    pub level: String,
    pub role: String,
    pub marker: String,
}

/// Run hierarchy::compat directly on extracted paragraphs without a JSON round trip.
pub fn analyze_hierarchy(paragraphs: Vec<Paragraph>) -> Result<Vec<IndexedParagraph>, String> {
    let input: Vec<_> = paragraphs.iter().map(|p| hierarchy::compat::analyze_hierarchy::ParagraphInput {
        text: p.text.clone(), left: 0, indent: 0,
        font: p.style_name.clone().unwrap_or_default(), size_pt: 0.0,
        prev_spacing: None, indent_hwpunit: 0,
    }).collect();
    let results = hierarchy::compat::analyze_hierarchy::analyze_hierarchy_typed(&input);
    if results.len() != paragraphs.len() {
        return Err("hierarchy result paragraph count mismatch".into());
    }
    Ok(paragraphs.into_iter().zip(results).map(|(paragraph, result)| {
        let role = result.role;
        let level = match role.as_str() {
            "제목" => "document_title", "장" => "chapter", "중제목" => "section",
            "소제목" => "topic", "본문" | "내용" | "미분류" => "body",
            "부연설명" => "asterisk", _ => "body",
        }.to_owned();
        IndexedParagraph { paragraph, level, role, marker: result.marker }
    }).collect())
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
        "hwpx" => {
            let mut records = extract_hwpx(path)?;
            // Match Python extract_paragraphs(): legacy HWPX records are fed
            // to extract_hwpx positionally (only emitted, non-cell paragraphs
            // advance the cursor), then their metadata is attached by text FIFO.
            // The positional pass is observable for table-first documents: the
            // first body paragraph receives the first legacy cell record.
            let legacy = crate::legacy_records::run(path)?;
            // Positional HWPX pairing replaces the paragraph text and metadata
            // from the legacy list; table-cell paragraphs themselves are omitted.
            // Reproduce the Python walker cursor semantics directly.
            for (idx, item) in legacy.iter().enumerate() {
                if let Some(record) = records.get_mut(idx) {
                    let legacy_text = item.str("text").unwrap_or("").trim();
                    if !legacy_text.is_empty() {
                        record.text = legacy_text.to_owned();
                        record.hierarchy_level = item.str("level").map(str::to_owned);
                        record.is_table_cell = item.bool("is_table_cell").unwrap_or(false);
                        record.style_name = item.str("style_name").map(str::to_owned);
                        record.outline_level = item.i64("outline_level");
                    }
                }
            }
            Ok(records)
        },
        ext => Err(format!("unsupported: 지원하지 않는 문서 형식: .{ext}")),
    }
}

#[cfg(test)]
mod hierarchy_tests {
    use super::*;

    #[test]
    fn typed_hierarchy_returns_one_result_per_extracted_paragraph() {
        let paragraphs = vec![
            Paragraph::line("제1장 총칙".into(), 1, 1),
            Paragraph::line("1. 목적".into(), 2, 2),
            Paragraph::line("본문 내용".into(), 3, 3),
        ];
        let result = analyze_hierarchy(paragraphs).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].role, "장");
        assert_eq!(result[0].level, "chapter");
        assert_eq!(result[0].marker, "제1장");
    }
}

const MAX_ENTRIES: usize = 10_000;
const MAX_TOTAL_SIZE: u64 = 512 * 1024 * 1024;
const MAX_FILE_SIZE: u64 = 128 * 1024 * 1024;
const MAX_COMPRESSION_RATIO: u64 = 1_000;

fn safe_member_name(name: &str) -> Result<String, String> {
    let name = name.replace('\\', "/");
    let path = std::path::Path::new(&name);
    if path.is_absolute() || name.split('/').any(|part| part == "..")
        || name.as_bytes().get(1) == Some(&b':') && name.as_bytes()[0].is_ascii_alphabetic() {
        return Err(format!("corrupt: 안전하지 않은 HWPX 압축 경로: {name}"));
    }
    Ok(name)
}

fn extract_hwpx(path: &Path) -> Result<Vec<Paragraph>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("corrupt: HWPX 파일 열기 실패: {e}"))?;
    let mut zip = ZipArchive::new(file).map_err(|e| format!("corrupt: HWPX 압축 파일 열기 실패: {e}"))?;
    if zip.len() > MAX_ENTRIES { return Err("corrupt: HWPX 압축 항목 수 초과".into()); }
    let mut names = Vec::new();
    let mut unique = std::collections::HashSet::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| format!("corrupt: HWPX 항목 읽기 실패: {e}"))?;
        let raw_name = entry.name().map_err(|e| format!("corrupt: HWPX 항목 이름 읽기 실패: {e}"))?;
        let name = safe_member_name(&raw_name)?;
        if !unique.insert(name.clone()) { return Err(format!("corrupt: 중복된 HWPX 압축 항목: {name}")); }
        if entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) { return Err(format!("corrupt: 심볼릭 링크 HWPX 항목 거부: {name}")); }
        if entry.encrypted() { return Err("encrypted: 암호화된 HWPX 항목이 포함되어 있음".into()); }
        let size = entry.size();
        if size > MAX_FILE_SIZE { return Err(format!("corrupt: HWPX 단일 항목 크기 초과: {name}")); }
        total = total.saturating_add(size);
        if total > MAX_TOTAL_SIZE { return Err("corrupt: HWPX 전체 크기 제한 초과".into()); }
        let compressed = entry.compressed_size();
        if size > 0 && compressed == 0 || compressed > 0 && size / compressed.max(1) > MAX_COMPRESSION_RATIO {
            return Err(format!("corrupt: HWPX 압축 크기 제한 초과: {name}"));
        }
        names.push(name);
    }
    if names.iter().any(|n| n.to_ascii_lowercase().contains("encrypt")) { return Err("encrypted: 암호화된 HWPX 메타데이터가 발견됨".into()); }
    let mut sections: Vec<String> = names.into_iter().filter(|n| {
        let lower = n.to_ascii_lowercase();
        lower.starts_with("contents/section") && lower.ends_with(".xml")
    }).collect();
    if sections.is_empty() { return Err("corrupt: HWPX 본문 섹션(Contents/section*.xml)을 찾지 못함".into()); }
    sections.sort_by_key(|n| n.chars().filter(char::is_ascii_digit).collect::<String>().parse::<u64>().unwrap_or(0));
    let mut records = Vec::new();
    for section in sections {
        let stem = Path::new(&section).file_stem().and_then(|s| s.to_str()).unwrap_or("section").to_owned();
        let mut entry = zip.by_name(&section).map_err(|e| format!("corrupt: HWPX 섹션 읽기 실패 ({section}): {e}"))?;
        let mut xml = Vec::new();
        entry.read_to_end(&mut xml).map_err(|e| format!("corrupt: HWPX 섹션 읽기 실패 ({section}): {e}"))?;
        let parsed = match extract_section(&xml, &stem) {
            Ok(v) => v,
            Err(e) if e.starts_with("empty:") => continue,
            Err(e) => return Err(e),
        };
        for text in parsed {
            let p_idx = records.iter().filter(|p: &&Paragraph| p.section.as_deref() == Some(stem.as_str())).count() + 1;
            let seq = records.len() + 1;
            records.push(Paragraph { text, location_kind: "paragraph".into(), location_label: format!("{stem} 문단 {p_idx}"), sequence: seq,
                section: Some(stem.clone()), hierarchy_level: None, is_table_cell: false, style_name: None, outline_level: None });
        }
    }
    if records.is_empty() { return Err("empty: 본문 문단 텍스트가 없음".into()); }
    Ok(records)
}

#[derive(Default, Clone)]
struct XmlNode { name: String, content: Vec<XmlContent> }
#[derive(Clone)]
enum XmlContent { Text(String), Node(XmlNode) }

fn extract_section(xml: &[u8], section: &str) -> Result<Vec<String>, String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<XmlNode> = Vec::new();
    let mut root = XmlNode::default();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let qname = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                stack.push(XmlNode { name: qname, ..Default::default() });
            }
            Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                let node = XmlNode { name, ..Default::default() };
                if let Some(parent) = stack.last_mut() { parent.content.push(XmlContent::Node(node)); } else { root.content.push(XmlContent::Node(node)); }
            }
            Ok(Event::Text(e)) => {
                let decoded = e.decode().map_err(|e| format!("corrupt: HWPX 섹션 XML 파싱 실패 ({section}): {e}"))?;
                if let Some(node) = stack.last_mut() { node.content.push(XmlContent::Text(decoded.into_owned())); }
            }
            Ok(Event::CData(e)) => {
                let decoded = e.decode().map_err(|e| format!("corrupt: HWPX 섹션 XML 파싱 실패 ({section}): {e}"))?;
                if let Some(node) = stack.last_mut() { node.content.push(XmlContent::Text(decoded.into_owned())); }
            }
            Ok(Event::GeneralRef(_)) => return Err(format!("corrupt: XML 엔티티 참조 거부 ({section})")),
            Ok(Event::DocType(_)) => return Err(format!("corrupt: DTD 및 외부 엔티티 거부 ({section})")),
            Ok(Event::End(_)) => {
                let node = stack.pop().ok_or_else(|| format!("corrupt: 잘못된 XML 구조 ({section})"))?;
                if let Some(parent) = stack.last_mut() { parent.content.push(XmlContent::Node(node)); } else { root.content.push(XmlContent::Node(node)); }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(format!("corrupt: HWPX 섹션 XML 파싱 실패 ({section}): {e}")),
        }
    }
    if !stack.is_empty() { return Err(format!("corrupt: 닫히지 않은 XML 태그 ({section})")); }
    fn walk(node: &XmlNode, in_table: bool, out: &mut Vec<String>) {
        // Python extractors_hwpx.walk: cell descendants are traversed only to
        // reach the cell and then skipped altogether.
        if matches!(node.name.as_str(), "tc" | "cell") { return; }
        if node.name == "p" {
            let mut content = String::new();
            collect_paragraph(node, true, &mut content);
            // Python falls back to Element.itertext() when its structured
            // HWP text visitor finds nothing (tables and drawing containers).
            if content.trim().is_empty() { content = iter_text(node); }
            let value = content.trim().to_owned();
            if !value.is_empty() { out.push(value); }
            for c in child_nodes(node) { walk(c, in_table, out); }
            return;
        }
        // A table's text is included in its containing paragraph via itertext,
        // but table paragraphs are never emitted as separate records.
        if node.name == "tbl" { return; }
        if in_table && matches!(node.name.as_str(), "tr" | "tc" | "cell") { for c in child_nodes(node) { walk(c, true, out); } return; }
        for c in child_nodes(node) { walk(c, in_table, out); }
    }
    fn collect_paragraph(node: &XmlNode, is_root: bool, out: &mut String) {
        if !is_root && matches!(node.name.as_str(), "p" | "tbl") { return; }
        match node.name.as_str() {
            "lineBreak" | "br" => out.push('\n'), "tab" => out.push('\t'), "fwSpace" | "nbSpace" => out.push(' '),
            "t" => for item in &node.content { match item {
                XmlContent::Text(t) => out.push_str(t),
                XmlContent::Node(c) => { out.push_str(&iter_text(c)); if c.name == "t" { for tail in &c.content { if let XmlContent::Text(t) = tail { out.push_str(t); } } } },
            } },
            _ => for item in &node.content { match item {
                XmlContent::Node(c) => collect_paragraph(c, false, out),
                _ => {},
            } },
        }
    }
    fn iter_text(node: &XmlNode) -> String {
        let mut out = String::new();
        for item in &node.content { match item { XmlContent::Text(t) => out.push_str(t), XmlContent::Node(c) => out.push_str(&iter_text(c)) } }
        out
    }
    fn child_nodes(node: &XmlNode) -> impl Iterator<Item=&XmlNode> { node.content.iter().filter_map(|c| if let XmlContent::Node(n) = c { Some(n) } else { None }) }
    let mut out = Vec::new();
    for child in child_nodes(&root) { walk(child, false, &mut out); }
    Ok(out)
}
