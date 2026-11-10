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
        "hwpx" => extract_hwpx(path),
        "hwp" => extract_hwp(path),
        "docx" => extract_docx(path),
        ext => Err(format!("unsupported: 지원하지 않는 문서 형식: .{ext}")),
    }
}

/// Extract HWP5 through the same bundled kordoc Markdown conversion used by Python.
/// HWPX and DOCX retain their independent XML extraction paths above.
fn extract_hwp(path: &Path) -> Result<Vec<Paragraph>, String> {
    use std::{process::{Command, Stdio}, sync::atomic::{AtomicU64, Ordering}, thread, time::{Duration, Instant}};
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    let app_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
    let mut roots = Vec::new();
    if let Some(dir) = app_dir {
        roots.push(dir.join("kordoc-runtime").join("4.24.9"));
        for ancestor in dir.ancestors().skip(1).take(5) {
            roots.push(ancestor.join("kordoc-runtime").join("4.24.9"));
        }
    }
    if let Some(base) = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("APPDATA")) {
        roots.push(Path::new(&base).join("HwpAutoDocFit").join("kordoc-runtime").join("4.24.9"));
    }
    let runtime = roots.into_iter().find_map(|root| {
        let manifest = std::fs::read_to_string(root.join("runtime.json")).ok()?;
        if crate::json::parse(&manifest)?.str("kordoc") != Some("4.24.9") { return None; }
        let node = [root.join("node/node.exe"), root.join("node/node")].into_iter().find(|p| p.is_file())?;
        let cli = root.join("app/node_modules/kordoc/dist/cli.js");
        cli.is_file().then_some((root, node, cli))
    }).ok_or_else(|| "runtime_unavailable: kordoc HWP 변환 런타임을 찾을 수 없음".to_owned())?;

    let temp_root = std::env::temp_dir();
    let (temp_dir, target) = loop {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let candidate = temp_root.join(format!("kidx-hwp-{}-{id}", std::process::id()));
        match std::fs::create_dir(&candidate) {
            Ok(()) => break (candidate.clone(), candidate.join("output.md")),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("corrupt: HWP 임시 폴더 생성 실패: {e}")),
        }
    };
    let mut child = Command::new(&runtime.1)
        .arg(&runtime.2).arg(path).arg("--format").arg("markdown")
        .arg("-o").arg(&target).arg("--silent")
        .env("KORDOC_OFFLINE", "1")
        .env("ORT_DISABLE_TELEMETRY", "1")
        .env("npm_config_offline", "true")
        .env("npm_config_update_notifier", "false")
        .env("NO_UPDATE_NOTIFIER", "1")
        .env("KORDOC_MODEL_CACHE", runtime.0.join("models"))
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().map_err(|e| format!("runtime_unavailable: kordoc 실행 실패: {e}"))?;
    let started = Instant::now();
    while child.try_wait().map_err(|e| format!("extract_failed: kordoc 상태 확인 실패: {e}"))?.is_none() {
        if started.elapsed() > Duration::from_secs(600) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_dir_all(&temp_dir);
            return Err("extract_failed: kordoc 문서 변환 시간 초과".into());
        }
        thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output().map_err(|e| format!("extract_failed: kordoc 결과 읽기 실패: {e}"))?;
    if !output.status.success() && output.status.code() != Some(2) {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let lower = message.to_lowercase();
        let code = if ["암호", "password", "encrypted", "protect"].iter().any(|x| lower.contains(x)) { "encrypted" }
            else if ["손상", "corrupt", "damaged", "invalid", "형식 오류"].iter().any(|x| lower.contains(x)) { "corrupt" }
            else { "extract_failed" };
        let _ = std::fs::remove_dir_all(&temp_dir);
        return Err(format!("{code}: HWP 문서 변환 실패: {message}"));
    }
    let raw = std::fs::read(&target).map_err(|e| {
        let _ = std::fs::remove_dir_all(&temp_dir);
        format!("extract_failed: kordoc 변환 결과를 읽을 수 없음: {e}")
    })?;
    let _ = std::fs::remove_dir_all(&temp_dir);
    let content = String::from_utf8(raw).map_err(|e| format!("decode_error: 변환된 마크다운 디코딩 실패: {e}"))?;
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    if normalized.trim().is_empty() { return Err("empty: HWP 본문 텍스트가 없음".into()); }

    let page_marker = regex::Regex::new(r"(?i)(?:\x0c|<!--\s*page(?:\s*[:#]?\s*(\d+)|\s*break)?\s*-->)").unwrap();
    let matches: Vec<_> = page_marker.find_iter(&normalized).collect();
    let mut records = Vec::new();
    if !matches.is_empty() {
        let mut segments: Vec<(usize, &str)> = Vec::new();
        let mut page = 1usize;
        let mut last_end = 0usize;
        for marker in &matches {
            let chunk = &normalized[last_end..marker.start()];
            if !(last_end == 0 && chunk.trim().is_empty()) { segments.push((page, chunk)); page += 1; }
            if let Some(number) = page_marker.captures(marker.as_str()).and_then(|m| m.get(1)).and_then(|m| m.as_str().parse::<usize>().ok()) { page = number; }
            last_end = marker.end();
        }
        segments.push((page, &normalized[last_end..]));
        for (page, chunk) in segments {
            for line in chunk.lines().map(str::trim).filter(|line| !line.is_empty()) {
                records.push(hwp_record(line.to_owned(), "page", format!("{page}쪽"), records.len() + 1));
            }
        }
    } else {
        let split = regex::Regex::new(r"\n\s*\n").unwrap();
        for (index, para) in split.split(&normalized).map(str::trim).filter(|p| !p.is_empty()).enumerate() {
            records.push(hwp_record(para.to_owned(), "paragraph", format!("문단 {}", index + 1), records.len() + 1));
        }
    }
    if records.is_empty() { return Err("empty: 추출 가능한 HWP 본문이 없음".into()); }
    Ok(records)
}

fn hwp_record(text: String, kind: &str, label: String, sequence: usize) -> Paragraph {
    Paragraph { text, location_kind: kind.into(), location_label: label, sequence,
        section: None, hierarchy_level: None, is_table_cell: false, style_name: None, outline_level: None }
}

/// Extraction used by content indexing, retaining HWPX metadata from the legacy index walker.
pub fn extract_for_index(path: &Path) -> Result<Vec<Paragraph>, String> {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "hwpx" => {
            let mut records = extract_hwpx(path)?;
            let legacy = crate::legacy_records::run(path)?;
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
        }
        "hwp" => extract_hwp(path),
        "docx" => {
            let mut records = extract_docx(path)?;
            let file = std::fs::File::open(path).map_err(|e| format!("corrupt: DOCX 파일 열기 실패: {e}"))?;
            let mut zip = ZipArchive::new(file).map_err(|e| format!("corrupt: DOCX ZIP 열기 실패: {e}"))?;
            let mut xml = Vec::new();
            zip.by_name("word/document.xml").map_err(|e| format!("corrupt: DOCX 본문 읽기 실패: {e}"))?.read_to_end(&mut xml).map_err(|e| format!("corrupt: DOCX 본문 읽기 실패: {e}"))?;
            let root = parse_xml_tree(&xml, "DOCX 본문", "word/document.xml")?;
            fn walk(node: &XmlNode, in_table: bool, out: &mut Vec<Paragraph>) {
                if node.name == "tc" {
                    for child in child_nodes(node) { walk(child, true, out); }
                    return;
                }
                if node.name == "p" {
                    let text = iter_text(node).trim().to_owned();
                    if !text.is_empty() {
                        let mut style = None;
                        let mut outline = None;
                        for ppr in child_nodes(node).filter(|n| n.name == "pPr") {
                            for child in child_nodes(ppr) {
                                if child.name == "pStyle" { style = child.attrs.get("val").cloned(); }
                                if child.name == "outlineLvl" { outline = child.attrs.get("val").and_then(|v| v.parse().ok()); }
                            }
                        }
                        out.push(Paragraph { text, location_kind: "paragraph".into(), location_label: String::new(), sequence: out.len()+1,
                            section: None, hierarchy_level: None, is_table_cell: in_table, style_name: style, outline_level: outline });
                    }
                    for child in child_nodes(node) { walk(child, in_table, out); }
                    return;
                }
                for child in child_nodes(node) { walk(child, in_table, out); }
            }
            let mut metadata = Vec::new();
            walk(&root, false, &mut metadata);
            let mut by_text: std::collections::HashMap<String, std::collections::VecDeque<Paragraph>> = std::collections::HashMap::new();
            for item in metadata { by_text.entry(item.text.trim().to_owned()).or_default().push_back(item); }
            for record in &mut records {
                if let Some(item) = by_text.get_mut(record.text.trim()).and_then(|items| items.pop_front()) {
                    record.is_table_cell = item.is_table_cell;
                    record.style_name = item.style_name;
                    record.outline_level = item.outline_level;
                }
            }
            Ok(records)
        }
        _ => extract(path),
    }
}

/// Rust equivalent of Python `_apply_hierarchy`: match normalized text with
/// per-text FIFO queues, preserving extraction fields on misses.
pub(crate) fn apply_hierarchy(records: Vec<Paragraph>, hierarchy_records: &[Value]) -> Vec<Paragraph> {
    if hierarchy_records.is_empty() { return records; }
    let mut by_text: std::collections::HashMap<String, std::collections::VecDeque<&Value>> = std::collections::HashMap::new();
    for item in hierarchy_records {
        by_text.entry(item.str("text").unwrap_or("").trim().to_owned()).or_default().push_back(item);
    }
    records.into_iter().map(|mut record| {
        if let Some(item) = by_text.get_mut(record.text.trim()).and_then(|items| items.pop_front()) {
            record.hierarchy_level = item.str("level").map(str::to_owned);
            record.is_table_cell = item.bool("is_table_cell").unwrap_or(false);
            record.style_name = item.str("style_name").map(str::to_owned);
            record.outline_level = item.i64("outline_level");
        }
        record
    }).collect()
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
struct XmlNode { name: String, attrs: std::collections::HashMap<String, String>, content: Vec<XmlContent> }
#[derive(Clone)]
enum XmlContent { Text(String), Node(XmlNode) }

fn parse_xml_tree(xml: &[u8], doc_type: &str, context: &str) -> Result<XmlNode, String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<XmlNode> = Vec::new();
    let mut root = XmlNode::default();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let qname = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                let mut node = XmlNode { name: qname, ..Default::default() };
                for attr in e.attributes().with_checks(false).flatten() {
                    let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).into_owned();
                    if let Ok(value) = attr.decode_and_unescape_value(reader.decoder()) { node.attrs.insert(key, value.into_owned()); }
                }
                stack.push(node);
            }
            Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                let mut node = XmlNode { name, ..Default::default() };
                for attr in e.attributes().with_checks(false).flatten() {
                    let key = String::from_utf8_lossy(attr.key.local_name().as_ref()).into_owned();
                    if let Ok(value) = attr.decode_and_unescape_value(reader.decoder()) { node.attrs.insert(key, value.into_owned()); }
                }
                if let Some(parent) = stack.last_mut() { parent.content.push(XmlContent::Node(node)); } else { root.content.push(XmlContent::Node(node)); }
            }
            Ok(Event::Text(e)) => {
                let decoded = e.decode().map_err(|e| format!("corrupt: {doc_type} XML 파싱 실패 ({context}): {e}"))?;
                if let Some(node) = stack.last_mut() { node.content.push(XmlContent::Text(decoded.into_owned())); }
            }
            Ok(Event::CData(e)) => {
                let decoded = e.decode().map_err(|e| format!("corrupt: {doc_type} XML 파싱 실패 ({context}): {e}"))?;
                if let Some(node) = stack.last_mut() { node.content.push(XmlContent::Text(decoded.into_owned())); }
            }
            Ok(Event::GeneralRef(_)) => return Err(format!("corrupt: XML 엔티티 참조 거부 ({context})")),
            Ok(Event::DocType(_)) => return Err(format!("corrupt: DTD 및 외부 엔티티 거부 ({context})")),
            Ok(Event::End(_)) => {
                let node = stack.pop().ok_or_else(|| format!("corrupt: 잘못된 XML 구조 ({context})"))?;
                if let Some(parent) = stack.last_mut() { parent.content.push(XmlContent::Node(node)); } else { root.content.push(XmlContent::Node(node)); }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(format!("corrupt: {doc_type} XML 파싱 실패 ({context}): {e}")),
        }
    }
    if !stack.is_empty() { return Err(format!("corrupt: 닫히지 않은 XML 태그 ({context})")); }
    Ok(root)
}

fn iter_text(node: &XmlNode) -> String {
    let mut out = String::new();
    for item in &node.content { match item { XmlContent::Text(t) => out.push_str(t), XmlContent::Node(c) => out.push_str(&iter_text(c)) } }
    out
}

fn child_nodes(node: &XmlNode) -> impl Iterator<Item=&XmlNode> {
    node.content.iter().filter_map(|c| if let XmlContent::Node(n) = c { Some(n) } else { None })
}

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

const OLE_HEADER: &[u8] = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1";

fn check_ole_encrypted(path: &Path) -> Result<(), String> {
    if let Ok(mut f) = std::fs::File::open(path) {
        let mut header = [0u8; 512];
        if let Ok(n) = f.read(&mut header) {
            let slice = &header[..n];
            if slice.starts_with(OLE_HEADER) {
                let has_encrypted_package = slice.windows(b"EncryptedPackage".len()).any(|w| w == b"EncryptedPackage");
                let has_encryption_info = slice.windows(b"EncryptionInfo".len()).any(|w| w == b"EncryptionInfo");
                if has_encrypted_package || has_encryption_info {
                    return Err("encrypted: 암호화된 Office 복합 문서(OLE)".into());
                }
            }
        }
    }
    Ok(())
}

fn extract_docx(path: &Path) -> Result<Vec<Paragraph>, String> {
    if !path.is_file() {
        return Err(format!("corrupt: 파일을 찾을 수 없음: {}", path.display()));
    }
    check_ole_encrypted(path)?;

    let file = std::fs::File::open(path).map_err(|e| format!("corrupt: 압축 파일 열기 실패: {e}"))?;
    let mut zip = ZipArchive::new(file).map_err(|e| format!("corrupt: 올바른 ZIP 오피스 문서가 아님: {e}"))?;
    if zip.len() > MAX_ENTRIES {
        return Err(format!("corrupt: 압축 항목 수 초과: {}", zip.len()));
    }

    let mut names = Vec::new();
    let mut unique = std::collections::HashSet::new();
    let mut total = 0u64;

    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| format!("corrupt: DOCX 항목 읽기 실패: {e}"))?;
        let raw_name = entry.name().map_err(|e| format!("corrupt: DOCX 항목 이름 읽기 실패: {e}"))?;
        let name = safe_member_name(&raw_name)?;
        if !unique.insert(name.clone()) {
            return Err(format!("corrupt: 중복된 압축 항목: {raw_name}"));
        }
        if entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
            return Err(format!("corrupt: 심볼릭 링크 압축 항목 거부: {raw_name}"));
        }
        if entry.encrypted() {
            return Err(format!("encrypted: 암호화된 ZIP 항목: {raw_name}"));
        }
        let size = entry.size();
        if size > MAX_FILE_SIZE {
            return Err(format!("corrupt: 단일 파일 크기 초과: {raw_name}"));
        }
        total = total.saturating_add(size);
        if total > MAX_TOTAL_SIZE {
            return Err(format!("corrupt: 총 압축 해제 크기 초과: {total}"));
        }
        let compressed = entry.compressed_size();
        if compressed > 0 && size / compressed > MAX_COMPRESSION_RATIO {
            return Err(format!("corrupt: 비정상 압축률 감지: {raw_name}"));
        }
        names.push(name);
    }

    if !names.iter().any(|n| n == "word/document.xml") {
        return Err("corrupt: DOCX 필수 파일(word/document.xml) 누락".into());
    }

    let mut records = Vec::new();
    let mut seq = 1;

    // 1. 본문 문단 추출
    {
        let mut entry = zip.by_name("word/document.xml").map_err(|e| format!("corrupt: DOCX 본문 XML 파싱 실패: {e}"))?;
        let mut xml = Vec::new();
        entry.read_to_end(&mut xml).map_err(|e| format!("corrupt: DOCX 본문 읽기 실패: {e}"))?;
        let doc_root = parse_xml_tree(&xml, "DOCX 본문", "word/document.xml")?;
        let mut body_texts = Vec::new();
        collect_docx_p_texts(&doc_root, &mut body_texts);
        let mut p_num = 1;
        for text in body_texts {
            records.push(Paragraph {
                text,
                location_kind: "paragraph".into(),
                location_label: format!("문단 {p_num}"),
                sequence: seq,
                section: Some("본문".into()),
                hierarchy_level: None,
                is_table_cell: false,
                style_name: None,
                outline_level: None,
            });
            p_num += 1;
            seq += 1;
        }
    }

    // 2. 머리글/바닥글 문단 추출
    let mut hf_names: Vec<String> = names.iter().filter(|n| {
        (n.starts_with("word/header") || n.starts_with("word/footer")) && n.ends_with(".xml")
    }).cloned().collect();

    hf_names.sort_by_key(|n| {
        let is_footer = if n.contains("footer") { 1 } else { 0 };
        let num: u64 = n.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0);
        (is_footer, num)
    });

    for hf_name in hf_names {
        let stem = Path::new(&hf_name).file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let is_header = stem.contains("header");
        let section_label = if is_header { "머리글" } else { "바닥글" };
        let hf_index: String = stem.chars().filter(|c| c.is_ascii_digit()).collect();
        let label_prefix = if hf_index.is_empty() {
            section_label.to_owned()
        } else {
            format!("{section_label} {hf_index}")
        };

        let mut entry = zip.by_name(&hf_name).map_err(|e| format!("corrupt: DOCX 머리글/바닥글 XML 파싱 실패 ({hf_name}): {e}"))?;
        let mut xml = Vec::new();
        entry.read_to_end(&mut xml).map_err(|e| format!("corrupt: DOCX 머리글/바닥글 읽기 실패 ({hf_name}): {e}"))?;
        let hf_root = parse_xml_tree(&xml, "DOCX 머리글/바닥글", &hf_name)?;
        let mut hf_texts = Vec::new();
        collect_docx_p_texts(&hf_root, &mut hf_texts);
        let mut hf_p_num = 1;
        for text in hf_texts {
            records.push(Paragraph {
                text,
                location_kind: "paragraph".into(),
                location_label: format!("{label_prefix} 문단 {hf_p_num}"),
                sequence: seq,
                section: Some(label_prefix.clone()),
                hierarchy_level: None,
                is_table_cell: false,
                style_name: None,
                outline_level: None,
            });
            hf_p_num += 1;
            seq += 1;
        }
    }

    if records.is_empty() {
        return Err("empty: 추출 가능한 DOCX 본문 텍스트가 없음".into());
    }

    Ok(records)
}

fn collect_docx_p_texts(node: &XmlNode, out: &mut Vec<String>) {
    if node.name == "p" {
        let text = iter_text(node);
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            out.push(trimmed.to_owned());
        }
    }
    for child in child_nodes(node) {
        collect_docx_p_texts(child, out);
    }
}
