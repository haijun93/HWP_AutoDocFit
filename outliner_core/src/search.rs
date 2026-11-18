use crate::{Document, OutlinerError, MAX_QUERY_BYTES};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryResult { pub page: String, pub path: Vec<String>, pub body: String, pub status: Option<String> }

/// Evaluates space-separated AND predicates across an ordered collection of named pages.
pub fn query<'a>(pages: impl IntoIterator<Item=(&'a str, &'a Document)>, expression: &str) -> Result<Vec<QueryResult>, OutlinerError> {
    if expression.len() > MAX_QUERY_BYTES { return Err(OutlinerError::InvalidQuery("쿼리가 10 KiB 한도를 초과했습니다".into())); }
    if expression.trim().is_empty() { return Err(OutlinerError::InvalidQuery("쿼리가 비어 있습니다".into())); }
    let mut terms = Vec::new();
    for token in expression.split_whitespace() {
        let (kind, value) = token.split_once(':').ok_or_else(|| OutlinerError::InvalidQuery(format!("잘못된 조건: {token}")))?;
        if value.is_empty() { return Err(OutlinerError::InvalidQuery(format!("값이 비어 있습니다: {token}"))); }
        match kind {
            "status" if value == "TODO|DONE" => terms.push((kind, value)),
            "status" if value == "TODO" || value == "DONE" => terms.push((kind,value)),
            "tag"|"link"|"text" if value.chars().count() >= 2 => terms.push((kind,value)),
            "prop" if value.split_once('=').is_some_and(|(k,v)| !k.is_empty() && !v.is_empty()) => terms.push((kind,value)),
            "tag"|"link"|"text" => return Err(OutlinerError::QueryTooShort),
            "prop"|"status" => return Err(OutlinerError::InvalidQuery(format!("잘못된 조건: {token}"))),
            _ => return Err(OutlinerError::InvalidQuery(format!("알 수 없는 조건: {kind}"))),
        }
    }
    let mut out = Vec::new();
    for (page, doc) in pages {
        let mut stack: Vec<String> = Vec::new();
        for block in &doc.blocks {
            let depth = block.indent;
            stack.truncate(depth);
            stack.push(block.text.clone());
            let matched = terms.iter().all(|(kind,value)| match *kind {
                "status" => block.status.as_deref().is_some_and(|s| *value == "TODO|DONE" || s == *value),
                "tag" => block.tags.iter().any(|s| s.to_lowercase().contains(&value.to_lowercase())),
                "link" => block.page_links.iter().any(|s| s.to_lowercase().contains(&value.to_lowercase())),
                "text" => block.text.to_lowercase().contains(&value.to_lowercase()),
                "prop" => { let (k,v)=value.split_once('=').unwrap(); block.properties.get(k).is_some_and(|x| x.to_lowercase().contains(&v.to_lowercase())) },
                _ => false,
            });
            if matched { out.push(QueryResult{page:page.to_owned(),path:stack.clone(),body:block.text.clone(),status:block.status.clone()}); }
        }
    }
    Ok(out)
}

/// Finds block IDs whose text contains a case-insensitive query of at least two scalars.
pub fn search(document: &Document, query: &str) -> Result<Vec<u64>, OutlinerError> {
    if query.chars().count() < 2 {
        return Err(OutlinerError::QueryTooShort);
    }
    let needle = query.to_lowercase();
    Ok(document
        .blocks
        .iter()
        .filter(|block| block.text.to_lowercase().contains(&needle))
        .map(|block| block.id)
        .collect())
}
