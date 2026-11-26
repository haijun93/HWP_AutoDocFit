use crate::{Block, Document, ExtraLine, LineNode, OutlinerError, MAX_BLOCKS, MAX_DOCUMENT_BYTES};
use std::collections::BTreeMap;

fn indent_width(prefix: &str) -> usize {
    prefix
        .chars()
        .map(|character| if character == '\t' { 4 } else { 1 })
        .sum()
}

fn metadata(
    text: &str,
) -> (
    Vec<String>,
    BTreeMap<String, String>,
    Vec<String>,
    Vec<String>,
) {
    let chars: Vec<char> = text.chars().collect();
    let (mut tags, mut pages, mut refs) = (Vec::new(), Vec::new(), Vec::new());
    let mut at = 0;
    while at < chars.len() {
        if chars[at..].starts_with(&['#', '[', '[']) {
            if let Some(end) = (at + 3..chars.len().saturating_sub(1)).find(|&i| chars[i] == ']' && chars[i + 1] == ']') {
                let name: String = chars[at + 3..end].iter().collect();
                if !name.trim().is_empty() {
                    tags.push(name.trim().to_owned());
                    pages.push(name.trim().to_owned());
                    at = end + 2;
                    continue;
                }
            }
        }
        if chars[at] == '#' {
            let end = (at + 1..chars.len())
                .find(|&i| !(chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '-'))
                .unwrap_or(chars.len());
            if end > at + 1 {
                tags.push(chars[at + 1..end].iter().collect());
                at = end;
                continue;
            }
        }
        if at + 1 < chars.len() && (chars[at], chars[at + 1]) == ('[', '[') {
            if let Some(end) = (at + 2..chars.len().saturating_sub(1))
                .find(|&i| chars[i] == ']' && chars[i + 1] == ']')
            {
                pages.push(chars[at + 2..end].iter().collect());
                at = end + 2;
                continue;
            }
        }
        if at + 2 < chars.len() && chars[at] == '(' && chars[at + 1] == '(' && chars[at + 2] == '(' {
            if let Some(end) = (at + 3..chars.len().saturating_sub(2))
                .find(|&i| chars[i] == ')' && chars[i + 1] == ')' && chars[i + 2] == ')')
            {
                let content: String = chars[at + 3..end].iter().collect();
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    refs.push(trimmed.to_string());
                }
                at = end + 3;
                continue;
            }
        }
        if at + 1 < chars.len() && (chars[at], chars[at + 1]) == ('(', '(') {
            if let Some(end) = (at + 2..chars.len().saturating_sub(1))
                .find(|&i| chars[i] == ')' && chars[i + 1] == ')')
            {
                let content: String = chars[at + 2..end].iter().collect();
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    refs.push(trimmed.to_string());
                }
                at = end + 2;
                continue;
            }
        }
        at += 1;
    }
    let mut properties = BTreeMap::new();
    let words: Vec<&str> = text.split_whitespace().collect();
    for (index, part) in words.iter().enumerate() {
        if let Some(key) = part.strip_suffix("::") {
            if !key.is_empty() {
                properties.insert(
                    key.into(),
                    words.get(index + 1).copied().unwrap_or("").into(),
                );
            }
        } else if let Some((key, value)) = part.split_once("::") {
            if !key.is_empty() {
                properties.insert(key.into(), value.into());
            }
        }
    }
    (tags, properties, pages, refs)
}

fn parse_block(line: &str, index: usize, id: u64) -> Option<Block> {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let indent = indent_width(&line[..line.len() - trimmed.len()]);
    let mut text = trimmed.strip_prefix("- ")?.to_owned();
    let status = if let Some(rest) = text.strip_prefix("TODO ") {
        text = rest.into();
        Some("TODO".into())
    } else if let Some(rest) = text.strip_prefix("DONE ") {
        text = rest.into();
        Some("DONE".into())
    } else {
        None
    };
    let (tags, properties, page_links, block_refs) = metadata(&text);
    Some(Block {
        id,
        text,
        indent,
        folded: false,
        status,
        tags,
        properties,
        page_links,
        block_refs,
        line_index: index,
        extra: Vec::new(),
    })
}

/// `key:: value` (or a bare `key::`). `std::vector` is not a property.
pub(crate) fn is_property_line(line: &str) -> bool {
    line.split_once("::").is_some_and(|(key, rest)| {
        !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
            && (rest.is_empty() || rest.starts_with(' '))
    })
}

fn unescape_continuation(line: &str) -> String {
    let rest = line.trim_start_matches([' ', '\t']);
    match rest.strip_prefix('\\') {
        Some(after) if after.starts_with("- ") => {
            format!("{}{after}", &line[..line.len() - rest.len()])
        }
        _ => line.to_owned(),
    }
}

fn escape_continuation(line: &str) -> String {
    let rest = line.trim_start_matches([' ', '\t']);
    if rest.starts_with("- ") {
        format!("{}\\{rest}", &line[..line.len() - rest.len()])
    } else {
        line.to_owned()
    }
}

/// Parses Markdown while retaining every unrecognized line and original newline style.
pub fn parse(input: &str) -> Result<Document, OutlinerError> {
    if input.len() > MAX_DOCUMENT_BYTES {
        return Err(OutlinerError::DocumentTooLarge);
    }
    let newline = if input.contains("\r\n") { "\r\n" } else { "\n" }.to_string();
    let trailing_newline = input.ends_with('\n');
    let mut blocks = Vec::new();
    let mut sequence = Vec::new();
    let mut line_endings: Vec<String> = Vec::new();
    // 이어지는 줄(§ OUTLINEKEY1 파일 형식 확장)을 받을 수 있는 블록의 접두사: 불릿 앞 공백 + 2칸.
    let mut open_prefix: Option<String> = None;
    let mut text_lines = 0;
    for (line_index, raw_line) in input.split_inclusive('\n').enumerate() {
        let (line, ending) = if let Some(without_lf) = raw_line.strip_suffix('\n') {
            if let Some(without_cr) = without_lf.strip_suffix('\r') {
                (without_cr, "\r\n")
            } else {
                (without_lf, "\n")
            }
        } else {
            (raw_line, "")
        };
        if let Some(prefix) = &open_prefix {
            let continuation = line
                .strip_prefix(prefix.as_str())
                .filter(|rest| !rest.trim_start_matches([' ', '\t']).starts_with("- "));
            if let (Some(rest), Some(block)) = (continuation, blocks.last_mut()) {
                let block: &mut Block = block;
                if is_property_line(rest) {
                    block.extra.push(ExtraLine {
                        after: text_lines,
                        line: rest.to_owned(),
                    });
                } else {
                    block.text.push('\n');
                    block.text.push_str(&unescape_continuation(rest));
                    text_lines += 1;
                }
                refresh_metadata(block);
                if let Some(last) = line_endings.last_mut() {
                    *last = ending.to_owned();
                }
                continue;
            }
        }
        line_endings.push(ending.to_owned());
        if let Some(block) = parse_block(line, line_index, blocks.len() as u64 + 1) {
            let trimmed = line.trim_start_matches([' ', '\t']);
            open_prefix = Some(format!("{}  ", &line[..line.len() - trimmed.len()]));
            text_lines = 0;
            sequence.push(LineNode::Block(block.id));
            blocks.push(block);
            if blocks.len() > MAX_BLOCKS {
                return Err(OutlinerError::TooManyBlocks);
            }
        } else {
            open_prefix = None;
            sequence.push(LineNode::Raw(line.to_owned()));
        }
    }
    Ok(Document {
        original: input.into(),
        blocks,
        edited: false,
        sequence,
        line_endings,
        newline,
        trailing_newline,
    })
}

fn format_block(block: &Block, newline: &str) -> String {
    let mut parts = block.text.split('\n');
    let mut out = format!("{}- ", " ".repeat(block.indent));
    if let Some(status) = &block.status {
        out.push_str(status);
        out.push(' ');
    }
    out.push_str(parts.next().unwrap_or(""));
    let prefix = " ".repeat(block.indent + 2);
    let mut extra = block.extra.iter().peekable();
    let mut written = 0;
    let push = |out: &mut String, line: &str| {
        out.push_str(newline);
        out.push_str(&prefix);
        out.push_str(line);
    };
    for part in parts {
        while let Some(item) = extra.next_if(|item| item.after <= written) {
            push(&mut out, &item.line);
        }
        push(&mut out, &escape_continuation(part));
        written += 1;
    }
    for item in extra {
        push(&mut out, &item.line);
    }
    out
}

/// Serializes a document, returning untouched source exactly when no edit occurred.
pub fn serialize(document: &Document) -> String {
    if !document.edited {
        return document.original.clone();
    }
    let mut markdown = String::new();
    for (index, node) in document.sequence.iter().enumerate() {
        match node {
            LineNode::Raw(raw) => markdown.push_str(raw),
            LineNode::Block(id) => {
                if let Some(block) = document.blocks.iter().find(|block| block.id == *id) {
                    markdown.push_str(&format_block(block, &document.newline));
                }
            }
        }
        markdown.push_str(
            document
                .line_endings
                .get(index)
                .map(String::as_str)
                .unwrap_or(&document.newline),
        );
    }
    markdown
}

pub(crate) fn refresh_metadata(block: &mut Block) {
    let (mut tags, mut properties, mut page_links, mut block_refs) = metadata(&block.text);
    for item in &block.extra {
        if let Some((key, value)) = item.line.split_once("::") {
            properties.insert(key.to_owned(), value.trim().to_owned());
            for token in crate::references::reference_tokens(value) {
                use crate::references::RefTarget;
                let (list, name) = match token.target {
                    RefTarget::Page(name) => (&mut page_links, name),
                    RefTarget::Block(name) => (&mut block_refs, name),
                    RefTarget::Tag(name) => (&mut tags, name),
                };
                if !list.contains(&name) { list.push(name); }
            }
            if key == "tags" {
                for name in value.split(',').map(|s| s.trim().trim_start_matches('#').trim_start_matches("[[").trim_end_matches("]]")) {
                    if !name.is_empty() && !tags.iter().any(|t| t == name) { tags.push(name.to_owned()); }
                }
            }
        }
    }
    block.tags = tags;
    block.properties = properties;
    block.page_links = page_links;
    block.block_refs = block_refs;
}

pub(crate) fn block_from_text(text: &str, id: u64, indent: usize) -> Block {
    let mut line = format!("{}- {text}", " ".repeat(indent));
    let mut block = parse_block(&line, 0, id).expect("constructed bullet line");
    block.indent = indent;
    line.clear();
    block
}
