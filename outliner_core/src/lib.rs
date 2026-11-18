//! Markdown outliner engine with a line oriented JSON interface.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, path::Path};

pub const MAX_DOCUMENT_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_BLOCKS: usize = 100_000;

/// Errors returned by parsing, editing, searching, and storage operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutlinerError {
    DocumentTooLarge,
    TooManyBlocks,
    QueryTooShort,
    Invalid(String),
    Io(String),
}

impl OutlinerError {
    /// Returns the stable machine readable error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::DocumentTooLarge => "DOCUMENT_TOO_LARGE",
            Self::TooManyBlocks => "TOO_MANY_BLOCKS",
            Self::QueryTooShort => "QUERY_TOO_SHORT",
            Self::Invalid(_) => "INVALID_OPERATION",
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
            Self::Invalid(message) | Self::Io(message) => write!(f, "{message}"),
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
mod index;
mod ops;
mod parse_serialize;
mod search;
mod store;

pub use api::handle_request;
pub use index::{backlinks, build_index, Index};
pub use ops::edit;
pub use parse_serialize::{parse, serialize};
pub use search::search;
pub use store::{load, save_atomic, save_document};

/// Produces the conventional filename for an ISO date page.
pub fn daily_page(date: &str) -> Result<String, OutlinerError> {
    let bytes = date.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
        || date[5..7]
            .parse::<u8>()
            .map_or(true, |m| !(1..=12).contains(&m))
        || date[8..10]
            .parse::<u8>()
            .map_or(true, |d| !(1..=31).contains(&d))
    {
        return Err(OutlinerError::Invalid(
            "날짜는 YYYY-MM-DD여야 합니다".into(),
        ));
    }
    Ok(format!("{date}.md"))
}

/// Loads and parses a UTF-8 Markdown document.
pub fn parse_file(path: &Path) -> Result<Document, OutlinerError> {
    load(path)
}

#[cfg(test)]
mod tests;
