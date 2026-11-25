//! Shared synthetic corpus for the UNIFIED1 integration tests and measurement tool.
#![allow(dead_code)]
use kfind_core::{Sentence, Writer, WriterOptions};
use kfind_names::{Builder, Names, Options, Searcher};
use ktm::backend::{Answer, Backend, Row, unified};
use ktm::tm_state::ResultKind;
use serde_json::json;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const SEED: u64 = 20261124;
pub const COUNT: u64 = 1000;

pub struct Corpus {
    pub root: PathBuf,
    pub names: Names,
    pub paths: Vec<String>,
}

impl Corpus {
    /// Refuse an existing directory. Synthetic files are always newly created.
    pub fn create(root: &Path) -> Result<Self, String> {
        if root.exists() {
            return Err("합성 코퍼스 경로가 이미 있습니다".into());
        }
        let root = std::path::absolute(root).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(root.join("documents")).map_err(|e| e.to_string())?;
        let mut table =
            std::fs::File::create_new(root.join("doctable.jsonl")).map_err(|e| e.to_string())?;
        let mut writer =
            Writer::open(&root, WriterOptions::default()).map_err(|e| e.to_string())?;
        let mut builder = Builder::new();
        let document_dir = root.join("documents").to_string_lossy().into_owned();
        // A synthetic volume rooted at the corpus directory keeps name-service
        // paths and mmap document-table paths identical, on Windows and Unix.
        let vol = builder.add_volume(&document_dir, 5);
        let mut rng = SEED;
        let mut paths = Vec::new();
        let mut paragraphs = Vec::new();
        for id in 1..=COUNT {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            let name_match = (230..=259).contains(&id) || (480..=509).contains(&id);
            let name = if id == 901 {
                format!("2026 공사 계약-{id:04}.txt")
            } else if name_match {
                format!("예산 집행-{id:04}.txt")
            } else {
                format!("합성문서-{id:04}.txt")
            };
            let text = if id <= 250 {
                "예산 지역별 세부 집행".to_owned()
            } else if id <= 500 {
                "예산집행 현황".to_owned()
            } else if (901..=904).contains(&id) {
                "2026".to_owned()
            } else {
                format!("일반 기록 번호 {rng}")
            };
            let second = if (901..=904).contains(&id) {
                "공사 계약".to_owned()
            } else if id == 905 {
                "2026".to_owned()
            } else if id == 906 {
                "공사 계약".to_owned()
            } else {
                format!("추진 계획 합성 자료 {id}")
            };
            let path = root.join("documents").join(&name);
            std::fs::write(&path, format!("{text}\n{second}\n")).map_err(|e| e.to_string())?;
            let path = path.to_string_lossy().into_owned();
            let level = if id <= 250 { 0 } else { 8 };
            writer
                .add_document(
                    id,
                    id as i64,
                    &[
                        Sentence {
                            para: 0,
                            level,
                            text: &text,
                        },
                        Sentence {
                            para: 1,
                            level: 8,
                            text: &second,
                        },
                    ],
                )
                .map_err(|e| e.to_string())?;
            let entry = json!({"op":"put","key":format!("synthetic:{id}"),"id":id,
                "meta":{"path":path,"date":id,"size":256,"scopes":["tm"],"level_src":"py","level_rule":1}});
            writeln!(table, "{entry}").map_err(|e| e.to_string())?;
            // 2026-11-24T00:00:00Z in FILETIME, only synthetic metadata.
            builder.push_meta(vol, id + 10, 5, &name, 0, 256, 134399520000000000);
            paragraphs.push((path.clone(), 0, text));
            paragraphs.push((path.clone(), 1, second));
            paths.push(path);
        }
        writer.commit().map_err(|e| e.to_string())?;
        table.sync_all().map_err(|e| e.to_string())?;
        let rows: Vec<_> = paragraphs
            .iter()
            .map(|(p, n, t)| (p.as_str(), *n, t.as_str()))
            .collect();
        ktm::paragraphs::create_fixture(&root.join("timeline-content.sqlite"), &rows)?;
        Ok(Self {
            root,
            names: builder.finish(),
            paths,
        })
    }

    pub fn backend(&self) -> Result<Backend, String> {
        Backend::open(&self.root)
            .map(|b| b.with_paragraphs(Some(self.root.join("timeline-content.sqlite"))))
    }

    pub fn names(&self, query: &str, top: usize) -> Result<Answer, String> {
        let (query, _) = unified::names_query(query, None, None, None);
        let view = self.names.view();
        let groups = kfind_names::query::parse_groups(&query);
        let result = Searcher::new()
            .search_groups(
                &view,
                self.names.generation(),
                &groups,
                &Options {
                    limit: top,
                    ..Options::default()
                },
            )
            .map_err(|e| e.to_string())?;
        let rows = kfind_names::query::hits(&view, &result.top)
            .into_iter()
            .map(|hit| Row {
                path: format!("{}\\{}", hit.dir.trim_end_matches('\\'), hit.name),
                name: hit.name,
                parent: hit.dir,
                level: 255,
                date: hit.mtime.unwrap_or(0),
                snippet_tried: true,
                ..Row::default()
            })
            .collect();
        Ok(Answer {
            rows,
            terms: unified::query_terms(&query),
            groups: kfind_core::split_semicolon_groups(&query),
            ..Answer::default()
        })
    }

    pub fn fetch(
        &self,
        backend: &mut Backend,
        query: &str,
        kind: ResultKind,
        top: usize,
    ) -> Result<Answer, String> {
        if kind == ResultKind::Names {
            self.names(query, top)
        } else {
            backend.search_with_scope_ext_limit(
                query,
                kind == ResultKind::Hierarchy,
                None,
                None,
                None,
                ktm::tm_state::MatchMode::AllTerms,
                top,
                None,
            )
        }
    }

    pub fn id_for(&self, row: &Row) -> u64 {
        self.paths
            .iter()
            .position(|path| unified::path_key(path) == unified::path_key(&row.path))
            .map(|i| i as u64 + 1)
            .expect("all rows must belong to the corpus")
    }
}
