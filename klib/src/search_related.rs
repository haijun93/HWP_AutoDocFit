//! Literal paragraph AND -> document intersection. No top-N clipping before intersection.
use super::related_query::{
    RelatedGroup, parse_related_query, related_matches, related_matches_single, related_normalized,
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
            index
                .replace_related(&document(
                    "synthetic/split_sentences.txt",
                    &[(8, "2026 공사 일정. 계약 검토.")],
                ))
                .unwrap();
            index
                .replace_related(&document("synthetic/2026 계약.pdf", &[]))
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
            let filenames = index
                .search_related("2026; 계약 pdf", 0, true)
                .unwrap();
            assert_eq!(filenames.len(), 1);
            assert_eq!(filenames[0].str("path"), Some("synthetic/2026 계약.pdf"));
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
    /// 추출 완료 문단도 공통 v5 필드에 넣는다. sheet/row는 위치 정보로 남긴다.
    pub fn replace_related(&self, request: &J) -> tantivy::Result<()> {
        let path = request
            .str("path")
            .ok_or_else(|| tantivy::TantivyError::InvalidArgument("path required".into()))?;
        let name: String = request.str("name").unwrap_or("").nfc().collect();
        let paragraphs = request
            .arr("paragraphs")
            .ok_or_else(|| tantivy::TantivyError::InvalidArgument("paragraphs required".into()))?;
        let records: Vec<_> = paragraphs.iter().map(|p| {
            let rank = p.i64("level_rank").unwrap_or(8).clamp(0,8) as usize;
            J::obj(vec![("text",p.str("text").unwrap_or("").nfc().collect::<String>().into()),
                ("paragraph_index",p.i64("paragraph_index").unwrap_or(0).into()),
                ("level",p.str("level").unwrap_or(LEVELS[rank]).into()), ("level_rank",rank.into())])
        }).collect();
        let data = J::obj(vec![("name",name.clone().into()), ("mtime",request.num("mtime").unwrap_or(0.0).into()),
            ("size",request.i64("size").unwrap_or(0).into()), ("paragraphs",J::Arr(records))]);
        let related = format!("related-v1:{}",crate::json::to_string(&data));
        let p = Paragraph {path:path.into(),name:name.clone(),box_ids:vec!["0".into()],title:String::new(),
            level:0,body:String::new(),ext:Path::new(path).extension().and_then(|s|s.to_str()).unwrap_or("").into(),
            mtime:(request.num("mtime").unwrap_or(0.0)*1000.0) as i64, page:None,sheet:None,row:None,
            content_hash:String::new(),extractor_version:"semi2-records-1".into()};
        let blocks:Vec<_> = paragraphs.iter().map(|r| {
            let rank=r.i64("level_rank").unwrap_or(8).clamp(0,8);
            let ordinal=r.i64("paragraph_index").unwrap_or(0).max(0) as u64;
            SearchBlock {text:r.str("text").unwrap_or("").nfc().collect(),level:if rank==8 {0} else {(rank+1) as u8},
                ordinal,node_id:format!("{}#{ordinal}",id(path)),..Default::default()}
        }).collect();
        if blocks.iter().all(|b|b.text.trim().is_empty()) {
            // 빈 본문은 검색 토큰을 만들지 않고 파일명·근거 자료만 보존한다.
            return self.write(|w| {
                w.delete_term(tantivy::Term::from_field_text(self.fields.path,path));
                w.add_document(doc!(self.fields.doc_id=>id(path),self.fields.path=>path,self.fields.name=>name.clone(),
                    self.fields.level=>0u64,self.fields.mtime=>p.mtime,self.fields.location=>related.clone(),
                    self.fields.extractor_version=>p.extractor_version.clone()))?;
                Ok(())
            });
        }
        self.add_content_with_related(&p,Some(&blocks),&related)
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
        // 부분 문자열(영문 중간·한 글자·기호 포함)은 토큰 후보만으로 완전성을
        // 보장할 수 없다. 저장 문단을 한 번 읽고 순수 문장 매처를 공유한다.
        let candidates = searcher.search(&tantivy::query::AllQuery, &DocSetCollector)?;
        let mut records = Vec::new();
        let mut native_paths = std::collections::HashSet::new();
        let mut ready_paths = std::collections::HashSet::new();
        for addr in candidates {
            let c = &columns[addr.segment_ord as usize];
            if c.is_notebook(addr.doc_id) { continue; }
            let path = c.path(addr.doc_id)?;
            let d = searcher.doc::<TantivyDocument>(addr)?;
            let location = d.get_first(self.fields.location).and_then(|v|v.as_str()).unwrap_or("");
            if let Some(raw) = location.strip_prefix("related-v1:") {
                let data = crate::json::parse(raw).ok_or_else(||tantivy::TantivyError::InvalidArgument("invalid related metadata".into()))?;
                names.insert(path.clone(), data.str("name").unwrap_or("").into());
                mtimes.insert(path.clone(), data.num("mtime").unwrap_or(0.0));
                sizes.insert(path.clone(), data.i64("size").unwrap_or(0).max(0) as u64);
                for p in data.arr("paragraphs").unwrap_or(&[]) {
                    records.push((path.clone(), p.i64("paragraph_index").map(|n|n as u64),
                        p.i64("level_rank").unwrap_or(8).clamp(0,8) as usize,
                        p.str("level").unwrap_or("body").to_string(), p.str("text").unwrap_or("").nfc().collect::<String>()));
                }
                if data.arr("paragraphs").is_some_and(|p|p.is_empty()) {
                    records.push((path.clone(),None,8,"body".into(),String::new()));
                }
                ready_paths.insert(path.clone());
            } else if d.get_first(self.fields.extractor_version).and_then(|v|v.as_str()).is_some_and(|v|!v.is_empty()) {
                native_paths.insert(path);
            } else {
                let level = if c.level(addr.doc_id)==0 {8} else {usize::from(c.level(addr.doc_id)-1).min(8)};
                names.insert(path.clone(), d.get_first(self.fields.title).and_then(|v|v.as_str()).unwrap_or("").nfc().collect());
                mtimes.insert(path.clone(), d.get_first(self.fields.sheet).and_then(|v|v.as_str()).and_then(|s|s.parse().ok()).unwrap_or(0.0));
                sizes.insert(path.clone(), d.get_first(self.fields.row).and_then(|v|v.as_u64()).unwrap_or(0));
                records.push((path, d.get_first(self.fields.paragraph_ordinal).and_then(|v|v.as_u64()), level,
                    if location.is_empty() {LEVELS[level].into()} else {location.into()},
                    d.get_first(self.fields.body).and_then(|v|v.as_str()).unwrap_or("").nfc().collect()));
            }
        }
        if !native_paths.is_subset(&ready_paths) {
            return Err(tantivy::TantivyError::InvalidArgument("연관 검색 자료가 없는 v5 색인은 재색인이 필요합니다.".into()));
        }
        for group in &groups {
            let mut found = HashMap::<String, Vec<Evidence>>::new();
            for (path, para, level, level_name, text) in &records {
                let name = &names[path];
                if excluded_related(path, name) { continue; }
                if filenames && found.contains_key(path) {
                    continue;
                }
                let body = if filenames {name.clone()} else {text.clone()};
                if !(if filenames {
                    related_matches_single(&body, group)
                } else {
                    related_matches(&body, group)
                }) {
                    continue;
                }
                let para = if filenames { None } else { *para };
                if !filenames && para.is_none() {
                    return Err(tantivy::TantivyError::InvalidArgument(
                        "문단 번호가 없는 색인은 재색인이 필요합니다.".into(),
                    ));
                }
                let level = if filenames {0} else {*level};
                let level_name = if filenames {
                    "filename".into()
                } else {
                    level_name.clone()
                };
                found.entry(path.clone()).or_default().push(Evidence {
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

fn excluded_related(path: &str, name: &str) -> bool {
    let excluded = |s: &str| s.trim_matches(|c: char| c.is_whitespace() || "'\"‘’“”`\u{200b}\u{feff}".contains(c)).to_lowercase().starts_with("#ieeee");
    excluded(name) || excluded(Path::new(path).file_name().and_then(|n|n.to_str()).unwrap_or(""))
        || path.replace('\\',"/").split('/').any(|p|p.eq_ignore_ascii_case("$recycle.bin"))
}
