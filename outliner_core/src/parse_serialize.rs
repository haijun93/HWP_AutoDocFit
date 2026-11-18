use crate::{Block, Document, LineNode, OutlinerError, MAX_BLOCKS, MAX_DOCUMENT_BYTES};
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
        if at + 1 < chars.len() && (chars[at], chars[at + 1]) == ('(', '(') {
            if let Some(end) = (at + 2..chars.len().saturating_sub(1))
                .find(|&i| chars[i] == ')' && chars[i + 1] == ')')
            {
                refs.push(chars[at + 2..end].iter().collect());
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
    })
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
    let mut line_endings = Vec::new();
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
        line_endings.push(ending.to_owned());
        if let Some(block) = parse_block(line, line_index, blocks.len() as u64 + 1) {
            sequence.push(LineNode::Block(block.id));
            blocks.push(block);
            if blocks.len() > MAX_BLOCKS {
                return Err(OutlinerError::TooManyBlocks);
            }
        } else {
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

fn format_block(block: &Block) -> String {
    let mut line = format!("{}- ", " ".repeat(block.indent));
    if let Some(status) = &block.status {
        line.push_str(status);
        line.push(' ');
    }
    line.push_str(&block.text);
    line
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
                    markdown.push_str(&format_block(block));
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
    let (tags, properties, page_links, block_refs) = metadata(&block.text);
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
