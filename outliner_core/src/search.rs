use crate::{Document, OutlinerError};

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
