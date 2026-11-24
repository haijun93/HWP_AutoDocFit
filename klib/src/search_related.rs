//! Literal paragraph AND -> document intersection. No top-N clipping before intersection.
use super::related_query::{
    RelatedGroup, parse_related_query, related_matches, related_normalized,
};
use super::*;
use crate::json::Value as J;
use tantivy::collector::DocSetCollector;
use unicode_normalization::UnicodeNormalization;

struct Evidence {
    para: Option<u64>,
    level: usize,
    level_name: String,
    body: String,
}

const LEVELS: [&str; 9] = [
    "document_title",
    "chapter",
    "section",
    "topic",
    "square",
    "circle",
    "dash",
    "asterisk",
    "body",
];
const LABELS: [&str; 9] = [
    "문서 제목",
    "장",
    "절",
    "주제·소제목",
    "ㅁ",
    "ㅇ",
    "-",
    "*",
    "본문",
];

fn excerpt(body: &str, group: &RelatedGroup) -> (String, Vec<J>) {
    let chars: Vec<char> = body.chars().collect();
    let lower = related_normalized(body);
    let mut spans = Vec::new();
    // Map lowercased byte offsets back to original character offsets (İ can expand).
    let mut boundaries = Vec::new();
    let mut byte = 0;
    for (i, c) in chars.iter().enumerate() {
        for lc in c.to_lowercase() {
            boundaries.push((byte, i));
            byte += lc.len_utf8();
        }
    }
    boundaries.push((byte, chars.len()));
    for term in &group.terms {
        let t = related_normalized(term);
        for (a, _) in lower.match_indices(&t) {
            let b = a + t.len();
            let start = boundaries
                .iter()
                .rev()
                .find(|(p, _)| *p <= a)
                .map(|(_, i)| *i);
            let end = boundaries
                .iter()
                .rev()
                .find(|(p, _)| *p < b)
                .map(|(_, i)| *i + 1);
            if let (Some(start), Some(end)) = (start, end) {
                spans.push((start, end));
            }
        }
    }
    spans.sort_unstable();
    spans.dedup();
    let first = spans.first().map_or(0, |s| s.0);
    let start = first.saturating_sub(70);
    let end = chars.len().min(first + 180);
    (
        chars[start..end].iter().collect(),
        spans
            .into_iter()
            .filter(|&(a, b)| a >= start && b <= end)
            .map(|(a, b)| J::Arr(vec![(a - start).into(), (b - start).into()]))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document(path: &str, blocks: &[(usize, &str)]) -> J {
        J::obj(vec![
            ("path", path.into()),
            ("name", path.into()),
            (
                "paragraphs",
                J::Arr(
                    blocks
                        .iter()
                        .enumerate()
                        .map(|(i, (rank, text))| {
                            J::obj(vec![
                                ("paragraph_index", i.into()),
                                ("text", (*text).into()),
                                ("level_rank", (*rank).into()),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }
    #[test]
    fn grouped_intersection_rank_evidence_and_uncapped_candidates() {
        let dir = std::env::temp_dir().join(format!(
            "semi1-klib-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        {
            let index = SearchIndex::open_or_create(&dir).unwrap();
            let batch = index.batch();
            for i in 0..650 {
                index
                    .replace_related(&document(&format!("synthetic/{i:04}.txt"), &[(8, "2026")]))
                    .unwrap();
            }
            index
                .replace_related(&document(
                    "synthetic/title.md",
                    &[(0, "2026"), (8, "공사 계약")],
                ))
                .unwrap();
            index
                .replace_related(&document(
                    "synthetic/count.txt",
                    &[(8, "2026"), (8, "공사 계약"), (8, "공사 계약")],
                ))
                .unwrap();
            index
                .replace_related(&document(
                    "synthetic/split.txt",
                    &[(8, "2026"), (8, "공사"), (8, "계약")],
                ))
                .unwrap();
            batch.finish().unwrap();
            let hits = index.search_related("2026; 공사 계약", 0, false).unwrap();
            assert_eq!(hits.len(), 2);
            assert_eq!(hits[0].str("path"), Some("synthetic/title.md"));
            assert_eq!(hits[1].i64("total_matches"), Some(3));
            assert_eq!(
                hits[0].arr("group_matches").unwrap()[1]
                    .arr("matches")
                    .unwrap()[0]
                    .i64("paragraph_index"),
                Some(1)
            );
            assert_eq!(
                index
                    .search_related("2026; 공사 계약", 1, false)
                    .unwrap()
                    .len(),
                1
            );
            assert!(
                index
                    .search_related("2026; 없는검색어", 0, false)
                    .unwrap()
                    .is_empty()
            );
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

impl SearchIndex {
    pub fn related_status(&self, roots: &[J]) -> tantivy::Result<J> {
        let searcher = self.reader.searcher();
        let columns = Columns::open(&searcher)?;
        let all = searcher.search(&tantivy::query::AllQuery, &DocSetCollector)?;
        let mut paths = std::collections::HashSet::new();
        for addr in all {
            let path = columns[addr.segment_ord as usize].path(addr.doc_id)?;
            if roots.is_empty()
                || roots.iter().any(|r| match r {
                    J::Str(root) => within_root(&path, root),
                    _ => false,
                })
            {
                paths.insert(path);
            }
        }
        Ok(J::obj(vec![
            ("needs_reindex_count", 0usize.into()),
            ("needs_reindex", false.into()),
            ("reindex_guide", J::Null),
            ("has_index", (!paths.is_empty()).into()),
            ("indexed_document_count", paths.len().into()),
            ("persistent_error_count", 0usize.into()),
        ]))
    }
    /// SEMI1 전용 색인에 공통 추출 결과를 넣는다. 번호의 빈 구간도 보존한다.
    pub fn replace_related(&self, request: &J) -> tantivy::Result<()> {
        let path = request
            .str("path")
            .ok_or_else(|| tantivy::TantivyError::InvalidArgument("path required".into()))?;
        let name: String = request.str("name").unwrap_or("").nfc().collect();
        let paragraphs = request
            .arr("paragraphs")
            .ok_or_else(|| tantivy::TantivyError::InvalidArgument("paragraphs required".into()))?;
        self.write(|writer| {
            writer.delete_term(tantivy::Term::from_field_text(self.fields.path,path));
            for p in paragraphs {
                let text: String = p.str("text").unwrap_or("").nfc().collect();
                let rank = p.i64("level_rank").unwrap_or(8).clamp(0,8);
                let level = if rank == 8 { 0 } else { rank+1 };
                writer.add_document(doc!(self.fields.doc_id=>id(path),self.fields.path=>path,
                    self.fields.name=>name.clone(),self.fields.title=>name.clone(),
                    self.fields.level=>level as u64,self.fields.body_bi=>text.clone(),self.fields.body=>text,
                    self.fields.location=>p.str("level").unwrap_or(LEVELS[rank as usize]),
                    self.fields.sheet=>request.num("mtime").unwrap_or(0.0).to_string(),
                    self.fields.row=>request.i64("size").unwrap_or(0).max(0) as u64,
                    self.fields.ext=>Path::new(path).extension().and_then(|s|s.to_str()).unwrap_or(""),
                    self.fields.mtime=>request.i64("mtime_us").unwrap_or(0),
                    self.fields.paragraph_ordinal=>p.i64("paragraph_index").unwrap_or(0).max(0) as u64))?;
            }
            // 파일명이 있지만 본문이 없는 문서도 이름 검색에서 찾는다.
            if paragraphs.is_empty() {
                writer.add_document(doc!(self.fields.doc_id=>id(path),self.fields.path=>path,
                    self.fields.name=>name,self.fields.level=>0u64,
                    self.fields.sheet=>request.num("mtime").unwrap_or(0.0).to_string(),
                    self.fields.row=>request.i64("size").unwrap_or(0).max(0) as u64,
                    self.fields.mtime=>request.i64("mtime_us").unwrap_or(0)))?;
            }
            Ok(())
        })
    }

    /// Separate opt-in API. `limit=0` returns all documents, for verification/adapters.
    pub fn search_related(
        &self,
        query: &str,
        limit: usize,
        filenames: bool,
    ) -> tantivy::Result<Vec<J>> {
        let groups = parse_related_query(query).map_err(tantivy::TantivyError::InvalidArgument)?;
        if groups.is_empty() {
            return Ok(Vec::new());
        }
        let searcher = self.reader.searcher();
        let columns = Columns::open(&searcher)?;
        let mut maps = Vec::<HashMap<String, Vec<Evidence>>>::new();
        let mut names = HashMap::<String, String>::new();
        let mut mtimes = HashMap::<String, f64>::new();
        let mut sizes = HashMap::<String, u64>::new();
        for group in &groups {
            let field = if filenames {
                self.fields.name
            } else {
                self.fields.body_bi
            };
            let mut parser = QueryParser::for_index(&self.index, vec![field]);
            parser.set_conjunction_by_default();
            let anchor = group
                .terms
                .iter()
                .max_by_key(|s| s.chars().count())
                .expect("nonempty group");
            let expression = format!("\"{}\"", anchor.replace('\\', "\\\\").replace('"', "\\\""));
            // Candidate retrieval only. Literal AND below removes tokenizer false positives.
            let q: Box<dyn Query> = if anchor.chars().any(|c| c.is_alphanumeric()) {
                parser
                    .parse_query(&expression)
                    .map_err(|e| tantivy::TantivyError::InvalidArgument(e.to_string()))?
            } else {
                Box::new(tantivy::query::AllQuery)
            };
            let candidates = searcher.search(q.as_ref(), &DocSetCollector)?;
            let mut found = HashMap::<String, Vec<Evidence>>::new();
            for addr in candidates {
                let c = &columns[addr.segment_ord as usize];
                if c.is_notebook(addr.doc_id) {
                    continue;
                }
                let path = c.path(addr.doc_id)?;
                if filenames && found.contains_key(&path) {
                    continue;
                }
                let d = searcher.doc::<TantivyDocument>(addr)?;
                let name = d
                    .get_first(self.fields.name)
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let raw = if filenames {
                    name
                } else {
                    d.get_first(self.fields.body)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                };
                let body: String = raw.nfc().collect();
                if !related_matches(&body, group) {
                    continue;
                }
                let para = if filenames {
                    None
                } else {
                    d.get_first(self.fields.paragraph_ordinal)
                        .and_then(|v| v.as_u64())
                };
                if !filenames && para.is_none() {
                    return Err(tantivy::TantivyError::InvalidArgument(
                        "문단 번호가 없는 색인은 재색인이 필요합니다.".into(),
                    ));
                }
                let raw_level = c.level(addr.doc_id);
                let level = if filenames {
                    0
                } else if raw_level == 0 {
                    8
                } else {
                    usize::from(raw_level - 1).min(8)
                };
                names.insert(path.clone(), name.nfc().collect());
                mtimes.insert(
                    path.clone(),
                    d.get_first(self.fields.sheet)
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0),
                );
                sizes.insert(
                    path.clone(),
                    d.get_first(self.fields.row)
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0),
                );
                let level_name = if filenames {
                    "filename".into()
                } else {
                    d.get_first(self.fields.location)
                        .and_then(|v| v.as_str())
                        .unwrap_or(LEVELS[level])
                        .to_string()
                };
                found.entry(path).or_default().push(Evidence {
                    para,
                    level,
                    level_name,
                    body,
                });
            }
            maps.push(found);
        }
        let mut paths: Vec<_> = maps[0]
            .keys()
            .filter(|path| maps[1..].iter().all(|m| m.contains_key(*path)))
            .cloned()
            .collect();
        let rank = |path: &String| {
            (
                maps.iter()
                    .flat_map(|m| &m[path])
                    .map(|e| e.level)
                    .min()
                    .unwrap_or(8),
                std::cmp::Reverse(maps.iter().map(|m| m[path].len()).sum::<usize>()),
                related_normalized(path),
            )
        };
        paths.sort_by_key(rank);
        if limit > 0 {
            paths.truncate(limit);
        }
        let mut out = Vec::new();
        for (rank, path) in paths.into_iter().enumerate() {
            let mut info = Vec::new();
            let mut all = Vec::new();
            let mut best = (usize::MAX, u64::MAX, String::new(), Vec::new());
            let mut total = 0;
            let mut counts = Vec::new();
            for (i, (group, map)) in groups.iter().zip(&mut maps).enumerate() {
                let mut ev = map.remove(&path).expect("intersection");
                ev.sort_by_key(|e| (e.level, e.para));
                total += ev.len();
                counts.push((group.query.clone(), ev.len().into()));
                let mut matches = Vec::new();
                for e in &ev {
                    let (snippet, highlights) = excerpt(&e.body, group);
                    if (e.level, e.para.unwrap_or(0)) < (best.0, best.1) {
                        best = (
                            e.level,
                            e.para.unwrap_or(0),
                            snippet.clone(),
                            highlights.clone(),
                        );
                    }
                    let evidence = J::obj(vec![
                        (
                            "paragraph_index",
                            e.para.map(|p| (p as usize).into()).unwrap_or(J::Null),
                        ),
                        ("level", e.level_name.clone().into()),
                        (
                            "level_rank",
                            if filenames {
                                J::from(-1i64)
                            } else {
                                e.level.into()
                            },
                        ),
                        ("snippet", snippet.into()),
                        ("highlights", J::Arr(highlights)),
                    ]);
                    matches.push(evidence.clone());
                    let mut location = evidence;
                    if let J::Obj(fields) = &mut location {
                        fields.extend([
                            (
                                "position".into(),
                                J::obj(vec![
                                    ("page", J::Null),
                                    ("sheet", J::Null),
                                    ("row", J::Null),
                                    (
                                        "label",
                                        e.para
                                            .map(|p| format!("문단 {}", p + 1))
                                            .unwrap_or_else(|| "파일명".into())
                                            .into(),
                                    ),
                                ]),
                            ),
                            ("breadcrumb".into(), J::Arr(vec![])),
                            ("breadcrumb_text".into(), "".into()),
                        ]);
                    }
                    all.push(location);
                }
                info.push(J::obj(vec![
                    ("group_index", i.into()),
                    ("query", group.query.clone().into()),
                    (
                        "terms",
                        J::Arr(group.terms.iter().cloned().map(J::from).collect()),
                    ),
                    ("match_count", ev.len().into()),
                    ("matches", J::Arr(matches)),
                ]));
            }
            let folder = Path::new(&path).parent().unwrap_or(Path::new(""));
            out.push(J::obj(vec![
                ("path", path.clone().into()),
                ("name", names.remove(&path).unwrap_or_default().into()),
                ("snippet", best.2.into()),
                ("highlights", J::Arr(best.3)),
                ("score", total.into()),
                ("total_matches", total.into()),
                ("match_counts", J::Obj(counts)),
                ("mtime", J::from(mtimes.get(&path).copied().unwrap_or(0.0))),
                (
                    "size",
                    J::from(sizes.get(&path).copied().unwrap_or(0) as i64),
                ),
                (
                    "best_level",
                    if filenames {
                        "파일명"
                    } else {
                        LABELS[best.0]
                    }
                    .into(),
                ),
                (
                    "position",
                    J::obj(vec![
                        ("page", J::Null),
                        ("sheet", J::Null),
                        ("row", J::Null),
                    ]),
                ),
                ("breadcrumb", J::Arr(vec![])),
                ("breadcrumb_text", "".into()),
                ("context_hits", 0usize.into()),
                ("relevance_rank", (rank + 1).into()),
                ("group_matches", J::Arr(info)),
                (
                    "extension",
                    Path::new(&path)
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .into(),
                ),
                ("folder_path", folder.to_string_lossy().into_owned().into()),
                (
                    "folder_name",
                    folder
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .into(),
                ),
                ("locations", J::Arr(all)),
                ("sections", J::Arr(vec![])),
            ]));
        }
        Ok(out)
    }
}

pub(super) fn within_root(path: &str, root: &str) -> bool {
    let path = path.replace('\\', "/").to_lowercase();
    let root = root.replace('\\', "/").to_lowercase();
    let root = root.trim_end_matches('/');
    path == root || path.strip_prefix(root).is_some_and(|s| s.starts_with('/'))
}
