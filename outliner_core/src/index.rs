use crate::Document;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// In-memory reverse lookup from page or block target to referring block IDs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Index {
    pub backlinks: BTreeMap<String, Vec<u64>>,
    pub tags: BTreeMap<String, Vec<u64>>,
    pub properties: BTreeMap<String, Vec<(u64, String)>>,
}

/// Result of resolving a block reference `((UUID))`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockRefResolution {
    pub found: bool,
    pub text: String,
    pub content: String,
    pub block_id: Option<u64>,
    pub uuid: String,
    pub backlinks: Vec<u64>,
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

/// Resolves a block reference by looking up the block with property `id:: UUID`
/// or by matching the numeric block ID, and gathering referring block IDs.
/// Missing ID returns an empty result instead of an error.
pub fn resolve_block_ref(document: &Document, uuid: &str) -> BlockRefResolution {
    let trimmed = uuid.trim();
    let target_block = document
        .blocks
        .iter()
        .find(|block| {
            block.properties.get("id").map(String::as_str) == Some(trimmed)
                || block.id.to_string() == trimmed
        });

    let mut backlinks_list = backlinks(document, trimmed);

    if let Some(block) = target_block {
        let block_id_str = block.id.to_string();
        if block_id_str != trimmed {
            for ref_id in backlinks(document, &block_id_str) {
                if !backlinks_list.contains(&ref_id) {
                    backlinks_list.push(ref_id);
                }
            }
        }
        if let Some(prop_id) = block.properties.get("id") {
            if prop_id != trimmed {
                for ref_id in backlinks(document, prop_id) {
                    if !backlinks_list.contains(&ref_id) {
                        backlinks_list.push(ref_id);
                    }
                }
            }
        }
        backlinks_list.sort_unstable();
        backlinks_list.dedup();

        BlockRefResolution {
            found: true,
            text: block.text.clone(),
            content: block.text.clone(),
            block_id: Some(block.id),
            uuid: trimmed.to_string(),
            backlinks: backlinks_list,
        }
    } else {
        backlinks_list.sort_unstable();
        backlinks_list.dedup();

        BlockRefResolution {
            found: false,
            text: String::new(),
            content: String::new(),
            block_id: None,
            uuid: trimmed.to_string(),
            backlinks: backlinks_list,
        }
    }
}

