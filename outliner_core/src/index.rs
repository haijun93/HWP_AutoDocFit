use crate::Document;
use std::collections::BTreeMap;

/// In-memory reverse lookup from page or block target to referring block IDs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Index {
    pub backlinks: BTreeMap<String, Vec<u64>>,
    pub tags: BTreeMap<String, Vec<u64>>,
    pub properties: BTreeMap<String, Vec<(u64, String)>>,
}

/// Builds page/block backlink, tag, and property indexes for a document.
pub fn build_index(document: &Document) -> Index {
    let mut index = Index::default();
    for block in &document.blocks {
        for target in block.page_links.iter().chain(&block.block_refs) {
            index
                .backlinks
                .entry(target.clone())
                .or_default()
                .push(block.id);
        }
        for tag in &block.tags {
            index.tags.entry(tag.clone()).or_default().push(block.id);
        }
        for (key, value) in &block.properties {
            index
                .properties
                .entry(key.clone())
                .or_default()
                .push((block.id, value.clone()));
        }
    }
    index
}

/// Returns block IDs referring to a page or block target.
pub fn backlinks(document: &Document, target: &str) -> Vec<u64> {
    build_index(document)
        .backlinks
        .remove(target)
        .unwrap_or_default()
}
