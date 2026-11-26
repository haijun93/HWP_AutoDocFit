//! Graph snapshot + current document overlay; rendering never reads or writes files.
use crate::{
    edit::EditHost,
    find::{BackItem, Hit},
    shell::{SearchMode, Shell},
};
use outliner_core::references::{RefTarget, References, Resolved};

/// Presentation tokens exclude code. Keep the legacy engine metadata contract intact.
pub(crate) fn block_targets(block: &outliner_core::Block) -> Vec<RefTarget> {
    let mut targets: Vec<_> = outliner_core::references::reference_tokens(&block.text)
        .into_iter()
        .map(|t| t.target)
        .collect();
    for extra in &block.extra {
        targets.extend(
            outliner_core::references::reference_tokens(&extra.line)
                .into_iter()
                .map(|t| t.target),
        );
    }
    if let Some(tags) = block.properties.get("tags") {
        targets.extend(
            tags.split(',')
                .map(|s| {
                    s.trim()
                        .trim_start_matches('#')
                        .trim_start_matches("[[")
                        .trim_end_matches("]]")
                })
                .filter(|s| !s.is_empty())
                .map(|s| RefTarget::Tag(s.into())),
        );
    }
    targets
}

impl<H: EditHost> Shell<H> {
    pub fn refresh_reference_graph(&mut self) {
        self.reference_revision = None;
        self.presentation_revision.clear();
        self.presentation_cache.clear();
        self.graph.clear();
        let Some(store) = &self.store else {
            return;
        };
        for page in &self.pages {
            if self.page.as_ref() == Some(page) {
                continue;
            }
            if let Ok(path) = store.path_of(page) {
                if let Ok((doc, _, _)) = crate::docio::load_file(&path) {
                    self.graph.insert(page.clone(), doc);
                }
            }
        }
        // Mentions depend only on this snapshot and the page name, not on the selection.
        self.refresh_unlinked();
    }

    pub fn refresh_presentations(&mut self) {
        let editing = self.editor.editing_block();
        let page = self.page.as_deref().unwrap_or("");
        let revision = outliner_core::content_digest(&self.editor.markdown());
        if self.presentation_revision != revision {
            self.presentation_cache.clear();
            self.presentation_revision = revision;
        }
        if self.editor.view.rows.iter().any(|r| {
            !self
                .presentation_cache
                .contains_key(&self.editor.view.doc.blocks[r.block].id)
        }) {
            let refs = References::new(&self.graph, page, &self.editor.view.doc);
            for row in &self.editor.view.rows {
                let block = &self.editor.view.doc.blocks[row.block];
                self.presentation_cache.entry(block.id).or_insert_with(|| {
                    // Rows without any reference keep the plain drawing path.
                    if outliner_core::references::reference_tokens(&block.text).is_empty() {
                        Vec::new()
                    } else {
                        refs.render(page, row.block)
                    }
                });
            }
        }
        for row in &mut self.editor.view.rows {
            row.presentation = if editing == Some(row.block) {
                Vec::new()
            } else {
                self.presentation_cache[&self.editor.view.doc.blocks[row.block].id].clone()
            };
            row.lines = if row.presentation.is_empty() {
                self.editor.view.doc.blocks[row.block].text.split('\n').count()
            } else {
                row.presentation.len().max(1)
            };
        }
    }

    pub fn refresh_unlinked(&mut self) {
        self.unlinked.clear();
        let Some(page) = &self.page else {
            return;
        };
        let needle = page.to_lowercase();
        if needle.trim().is_empty() {
            return;
        }
        for (source, doc) in &self.graph {
            if source.eq_ignore_ascii_case(page) {
                continue;
            }
            for (i, b) in doc.blocks.iter().enumerate() {
                let tokens = outliner_core::references::reference_tokens(&b.text);
                if block_targets(b).iter().any(|t| matches!(t, RefTarget::Page(p) | RefTarget::Tag(p) if p.eq_ignore_ascii_case(page))) { continue; }
                // Suppress code and explicit reference labels; only ordinary prose is an unlinked mention.
                let mut prose = b.text.clone();
                for t in tokens.into_iter().rev() {
                    prose.replace_range(t.start..t.end, " ");
                }
                let prose = prose
                    .split('`')
                    .enumerate()
                    .filter(|(i, _)| i % 2 == 0)
                    .map(|(_, s)| s)
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase();
                let found = prose.match_indices(&needle).any(|(at, _)| {
                    let ascii_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
                    !needle.chars().all(ascii_word)
                        || (prose[..at].chars().last().is_none_or(|c| !ascii_word(c))
                            && prose[at + needle.len()..]
                                .chars()
                                .next()
                                .is_none_or(|c| !ascii_word(c)))
                });
                if found {
                    self.unlinked.push(BackItem {
                        page: source.clone(),
                        block: i,
                        text: b.text.chars().take(crate::find::SNIPPET_CHARS).collect(),
                    });
                    if self.unlinked.len() >= crate::find::MAX_HITS {
                        return;
                    }
                }
            }
        }
    }

    /// A typed target keeps block IDs and page names in separate namespaces.
    pub fn follow_target(
        &mut self,
        target: &RefTarget,
        sidebar: bool,
        page_rows: usize,
    ) -> Result<(), String> {
        if let RefTarget::Tag(tag) = target {
            return self.search_tag(tag);
        }
        // Page links keep the existing behaviour (open or create; Shift opens a card).
        if let RefTarget::Page(name) = target {
            return if sidebar {
                self.open_card(name, None)
            } else {
                self.follow_link(name)
            };
        }
        if !self.editor.finish_edit() {
            return Err("편집을 확정하지 못했습니다".into());
        }
        let page = self.page.as_deref().unwrap_or("");
        let refs = References::new(&self.graph, page, &self.editor.view.doc);
        let resolved = refs.resolve(page, target);
        let (page, block) = match resolved {
            Resolved::Found(p, i) => (p.to_owned(), i),
            Resolved::Missing => return Err("참조 블록이 없습니다".into()),
            Resolved::Ambiguous => return Err("중복 ID/이름이라 이동할 수 없습니다".into()),
        };
        // Check the snapshot's target against disk before following its positional block index.
        if self.page.as_deref() != Some(page.as_str()) {
            let store = self.store.as_ref().ok_or("데이터 폴더가 없습니다")?;
            let (fresh, _, _) = crate::docio::load_file(&store.path_of(&page)?)?;
            if self
                .graph
                .get(&page)
                .is_none_or(|old| outliner_core::serialize(old) != outliner_core::serialize(&fresh))
            {
                self.refresh_lists();
                return Err("참조 원문이 바뀌었습니다. 다시 눌러 주세요".into());
            }
        }
        if sidebar {
            return self.open_card(&page, Some(block));
        }
        if self.page.as_deref() != Some(page.as_str()) {
            self.open_page(&page)?;
        }
        self.editor.view.reveal_block(block, page_rows);
        Ok(())
    }

    pub fn search_tag(&mut self, tag: &str) -> Result<(), String> {
        if !self.begin_search(SearchMode::Legacy) {
            return Err("편집을 확정하지 못했습니다".into());
        }
        self.query = format!("#{tag}");
        self.hits.clear();
        for (p, d) in self
            .graph
            .iter()
            .map(|(p, d)| (p.as_str(), d))
            .chain(std::iter::once((
                self.page.as_deref().unwrap_or(""),
                &self.editor.view.doc,
            )))
        {
            for (i, b) in d.blocks.iter().enumerate() {
                if block_targets(b)
                    .iter()
                    .any(|t| matches!(t, RefTarget::Tag(t) if t == tag))
                {
                    self.hits.push(Hit {
                        page: p.into(),
                        block: Some(i),
                        text: b.text.clone(),
                    });
                    if self.hits.len() >= crate::find::MAX_HITS {
                        break;
                    }
                }
            }
            if self.hits.len() >= crate::find::MAX_HITS {
                break;
            }
        }
        self.search_open = true;
        Ok(())
    }

    pub fn goto_unlinked(&mut self, i: usize, page_rows: usize) -> Result<(), String> {
        let item = self.unlinked.get(i).cloned().ok_or("언급이 없습니다")?;
        let store = self.store.as_ref().ok_or("데이터 폴더가 없습니다")?;
        let (fresh, _, _) = crate::docio::load_file(&store.path_of(&item.page)?)?;
        if self
            .graph
            .get(&item.page)
            .is_none_or(|old| outliner_core::serialize(old) != outliner_core::serialize(&fresh))
        {
            self.refresh_lists();
            self.refresh_backlinks();
            return Err("언급 원문이 바뀌었습니다. 다시 눌러 주세요".into());
        }
        // Opening a mention doesn't turn it into a link.
        self.open_page(&item.page)?;
        self.editor.view.reveal_block(item.block, page_rows);
        Ok(())
    }
}
