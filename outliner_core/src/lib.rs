//! Markdown outliner engine with a line oriented JSON interface.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, path::Path};

pub const MAX_DOCUMENT_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_BLOCKS: usize = 100_000;
pub const MAX_QUERY_BYTES: usize = 10 * 1024;

/// Errors returned by parsing, editing, searching, and storage operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutlinerError {
    DocumentTooLarge,
    TooManyBlocks,
    QueryTooShort,
    InvalidQuery(String),
    Invalid(String),
    Conflict,
    Io(String),
}

impl OutlinerError {
    /// Returns the stable machine readable error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::DocumentTooLarge => "DOCUMENT_TOO_LARGE",
            Self::TooManyBlocks => "TOO_MANY_BLOCKS",
            Self::QueryTooShort => "QUERY_TOO_SHORT",
            Self::InvalidQuery(_) => "INVALID_QUERY",
            Self::Invalid(_) => "INVALID_OPERATION",
            Self::Conflict => "CONFLICT",
            Self::Io(_) => "IO_ERROR",
        }
    }
}

impl fmt::Display for OutlinerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DocumentTooLarge => write!(f, "문서가 10 MiB 한도를 초과했습니다"),
            Self::TooManyBlocks => write!(f, "블록이 100000개 한도를 초과했습니다"),
            Self::QueryTooShort => write!(f, "검색어는 두 글자 이상이어야 합니다"),
            Self::Conflict => write!(f, "파일이 읽은 뒤 변경되었습니다"),
            Self::InvalidQuery(message) | Self::Invalid(message) | Self::Io(message) => {
                write!(f, "{message}")
            }
        }
    }
}

impl From<std::io::Error> for OutlinerError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

/// An editable Markdown bullet and the metadata extracted from its text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Block {
    pub id: u64,
    pub text: String,
    pub indent: usize,
    pub folded: bool,
    pub status: Option<String>,
    pub tags: Vec<String>,
    pub properties: BTreeMap<String, String>,
    pub page_links: Vec<String>,
    pub block_refs: Vec<String>,
    pub line_index: usize,
    /// Hidden `key:: value` continuation lines kept verbatim; they travel with the block.
    #[serde(default)]
    pub extra: Vec<ExtraLine>,
}

/// A property continuation line and how many text continuation lines precede it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtraLine {
    pub after: usize,
    pub line: String,
}

/// Ordered Markdown line nodes; raw lines are retained verbatim.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LineNode {
    Block(u64),
    Raw(String),
}

/// Parsed document with original bytes represented as UTF-8 text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Document {
    pub original: String,
    pub blocks: Vec<Block>,
    pub edited: bool,
    pub sequence: Vec<LineNode>,
    pub line_endings: Vec<String>,
    pub newline: String,
    pub trailing_newline: bool,
}

mod api;
mod history;
mod index;
mod keys;
mod ops;
mod parse_serialize;
mod rename;
mod search;
mod store;

pub use api::handle_request;
pub use history::{UndoHistory, MAX_UNDO_BYTES, MAX_UNDO_DEPTH};
pub use keys::{apply_key, pair_backspace, type_char, Cursor, KeyInput, KeyOutcome};
pub use keys::{move_subtree, DropPlace};
pub use index::{backlinks, build_index, resolve_block_ref, BlockRefResolution, Index};
pub use ops::{edit, edit_many, edit_with_offset};
pub use parse_serialize::{parse, serialize};
pub use rename::{
    collect_markdown_files, rename_page_in_files, update_line_links, update_page_links_text,
    RenameResult,
};
pub use search::search;
pub use search::{query, QueryResult};
pub use store::{content_digest, is_stale, load, save_atomic, save_atomic_checked, save_document};

/// Produces the conventional filename for an ISO date page.
pub fn daily_page(date: &str) -> Result<String, OutlinerError> {
    validate_date(date)?;
    Ok(format!("{}.md", date.replace('-', "_")))
}

/// Logseq-style journal page name using underscores. Input is a Gregorian YYYY-MM-DD date.
pub fn journal_name(date: &str) -> Result<String, OutlinerError> {
    validate_date(date)?;
    Ok(date.replace('-', "_"))
}

/// Converts a journal page name to an ISO date.
pub fn journal_date(name: &str) -> Result<String, OutlinerError> {
    let canonical = normalize_journal_name(name)?;
    Ok(canonical.replace('_', "-"))
}

/// Normalizes either journal spelling, with an optional case-insensitive `.md` suffix.
/// The canonical engine name is `YYYY_MM_DD` without an extension.
pub fn normalize_journal_name(name: &str) -> Result<String, OutlinerError> {
    let stem = name
        .len()
        .checked_sub(3)
        .and_then(|start| name.get(start..).map(|suffix| (start, suffix)))
        .filter(|(_, suffix)| suffix.eq_ignore_ascii_case(".md"))
        .map_or(name, |(start, _)| &name[..start]);
    if stem.len() != 10 {
        return Err(OutlinerError::Invalid(
            "일지 이름은 YYYY_MM_DD 또는 YYYY-MM-DD 형식이어야 합니다".into(),
        ));
    }
    let separator = stem.as_bytes()[4];
    if !matches!(separator, b'_' | b'-') || stem.as_bytes()[7] != separator {
        return Err(OutlinerError::Invalid(
            "일지 이름은 YYYY_MM_DD 또는 YYYY-MM-DD 형식이어야 합니다".into(),
        ));
    }
    let iso = stem.replace('_', "-");
    validate_date(&iso)?;
    Ok(iso.replace('-', "_"))
}

fn validate_date(date: &str) -> Result<(), OutlinerError> {
    if date.len() != 10
        || date.as_bytes()[4] != b'-'
        || date.as_bytes()[7] != b'-'
        || !date
            .bytes()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return Err(OutlinerError::Invalid(
            "날짜는 YYYY-MM-DD여야 합니다".into(),
        ));
    }
    let y: i32 = date[..4].parse().unwrap_or(0);
    let m: u32 = date[5..7].parse().unwrap_or(0);
    let d: u32 = date[8..10].parse().unwrap_or(0);
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let max = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if d == 0 || d > max {
        return Err(OutlinerError::Invalid("유효하지 않은 날짜입니다".into()));
    }
    Ok(())
}

/// Returns the date offset by -1, 0, or +1 day without consulting the system clock.
pub fn journal_relative(date: &str, offset: i32) -> Result<String, OutlinerError> {
    validate_date(date)?;
    if !(-1..=1).contains(&offset) {
        return Err(OutlinerError::Invalid(
            "offset는 -1, 0, 1만 허용합니다".into(),
        ));
    }
    let mut y: i32 = date[..4].parse().unwrap();
    let mut m: i32 = date[5..7].parse().unwrap();
    let mut d: i32 = date[8..].parse().unwrap();
    if offset < 0 {
        d -= 1;
        if d == 0 {
            m -= 1;
            if m == 0 {
                y -= 1;
                m = 12;
            }
            d = month_days(y, m);
        }
    }
    if offset > 0 {
        d += 1;
        if d > month_days(y, m) {
            d = 1;
            m += 1;
            if m == 13 {
                m = 1;
                y += 1;
            }
        }
    }
    Ok(format!("{y:04}-{m:02}-{d:02}"))
}
fn month_days(y: i32, m: i32) -> i32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Lists valid journal names in descending date order.
pub fn journal_list<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut dates: Vec<String> = names
        .into_iter()
        .filter_map(|name| normalize_journal_name(name).ok())
        .collect();
    dates.sort_by(|a, b| b.cmp(a));
    dates.dedup();
    dates
}

/// Loads and parses a UTF-8 Markdown document.
pub fn parse_file(path: &Path) -> Result<Document, OutlinerError> {
    load(path)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod history_tests;
