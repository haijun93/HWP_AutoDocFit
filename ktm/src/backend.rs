//! Read-only mapped content queries and a bounded service-pipe alternative.
//! The current published index contains comparison text and positions, not original
//! sentences. `snippet` stays empty rather than presenting a position as a quote.
use kfind_core::{DocHit, Index, SearchOptions};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, String>;
pub const PIPE_NAME: &str = r"\\.\pipe\HwpAutoDocFit.Index";
const LIMIT: usize = 50;

#[derive(Clone, Debug, Default)]
pub struct Row {
    pub doc_id: u64,
    pub path: String,
    pub name: String,
    pub parent: String,
    pub snippet: String,
    pub location: String,
    pub level: u8,
    pub count: u32,
}

pub struct Backend {
    root: PathBuf,
    index: Index,
    table_stamp: Option<(u64, Option<std::time::SystemTime>)>,
    paths: HashMap<u64, String>,
}

impl Backend {
    pub fn open(root: &Path) -> Result<Self> {
        // Index::open treats a missing manifest as empty; the UI needs an explicit
        // unavailable state and must never create the user's index directory.
        if !root.join("MANIFEST.kfm").is_file() {
            return Err("본문 색인이 없습니다. 색인 서비스에서 먼저 색인해 주세요.".into());
        }
        let index = Index::open(root).map_err(|e| e.to_string())?;
        Ok(Self {
            root: root.to_owned(),
            index,
            table_stamp: None,
            paths: HashMap::new(),
        })
    }

    pub fn search(&mut self, query: &str, hierarchy: bool) -> Result<Vec<Row>> {
        self.search_cancel(query, hierarchy, None)
    }

    pub fn search_cancel(
        &mut self,
        query: &str,
        hierarchy: bool,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<Vec<Row>> {
        self.index.reload().map_err(|e| e.to_string())?;
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let opts = SearchOptions {
            top: if hierarchy { 0 } else { LIMIT },
            refs_per_doc: 1,
            cancel,
            ..SearchOptions::default()
        };
        let mut result = if kfind_core::fold(query).chars().count() == 1 {
            self.index.search_character(query, &opts)
        } else {
            self.index.search(query, &opts)
        }
        .map_err(|e| e.to_string())?;
        if hierarchy {
            result.hits.sort_by_key(|h| h.level);
            result.hits.truncate(LIMIT);
        }
        let path = self.root.join("doctable.jsonl");
        let stamp = match std::fs::metadata(&path) {
            Ok(m) => Some((m.len(), m.modified().ok())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.to_string()),
        };
        if stamp != self.table_stamp {
            let paths = read_paths(&path, None)?;
            self.paths = paths;
            self.table_stamp = stamp;
        }
        Ok(result
            .hits
            .iter()
            .map(|h| row(h, self.paths.get(&h.doc_id).map(String::as_str)))
            .collect())
    }
}

fn row(hit: &DocHit, path: Option<&str>) -> Row {
    let path = path.unwrap_or_default().to_owned();
    let p = Path::new(&path);
    let name = p
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("문서 #{} (경로 정보 없음)", hit.doc_id));
    let parent = p
        .parent()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let location = hit
        .sentences
        .first()
        .map(|s| format!("문단 {} · 위계 {} · 원문 미게시", s.para + 1, s.level))
        .unwrap_or_else(|| format!("위계 {} · 원문 미게시", hit.level));
    Row {
        doc_id: hit.doc_id,
        path,
        name,
        parent,
        snippet: String::new(),
        location,
        level: hit.level,
        count: hit.count,
    }
}

/// Replay metadata without a writer/compactor (optionally restricted to IDs).
/// A final incomplete record is ignored exactly as the service does; bytes stay intact.
fn read_paths(path: &Path, ids: Option<&HashSet<u64>>) -> Result<HashMap<u64, String>> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(e.to_string()),
    };
    let mut paths = HashMap::new();
    let mut keys = HashMap::<String, u64>::new();
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    loop {
        line.clear();
        // A document record may include many spans; bound it to the service's
        // per-document input ceiling instead of trusting arbitrary allocations.
        let n = reader
            .by_ref()
            .take(64 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        if n > 64 * 1024 * 1024 {
            return Err("문서 표 기록 크기가 상한을 넘었습니다.".into());
        }
        // Ignore spans and the rest of DocMeta during deserialization so large
        // documents do not allocate a second copy of every sentence position.
        #[derive(serde::Deserialize)]
        struct Record {
            op: String,
            #[serde(default)]
            key: String,
            #[serde(default)]
            id: u64,
            #[serde(default)]
            meta: Meta,
        }
        #[derive(Default, serde::Deserialize)]
        struct Meta {
            #[serde(default)]
            path: String,
        }
        let value: Record = match serde_json::from_slice(&line) {
            Ok(v) => v,
            Err(_) if reader.fill_buf().map_err(|e| e.to_string())?.is_empty() => break,
            Err(e) => return Err(format!("문서 표가 손상되었습니다: {e}")),
        };
        let key = value.key.as_str();
        match value.op.as_str() {
            "put" => {
                if let Some(old) = keys.remove(key) {
                    paths.remove(&old);
                }
                let id = value.id;
                if ids.is_none_or(|ids| ids.contains(&id)) {
                    paths.insert(id, value.meta.path);
                    keys.insert(key.to_owned(), id);
                }
            }
            "del" => {
                if let Some(old) = keys.remove(key) {
                    paths.remove(&old);
                }
            }
            "next" => {}
            _ => return Err("문서 표에 알 수 없는 기록이 있습니다.".into()),
        }
    }
    Ok(paths)
}

/// Blocking alternative; call only on the search worker. The service has a
/// ten-second operation timeout. Responses are length-prefixed and capped here.
pub fn pipe_search(name: &str, query: &str, hierarchy: bool) -> Result<Vec<Row>> {
    if kfind_core::fold(query).chars().count() < 2 {
        return Err(
            "서비스 파이프는 2글자부터 검색합니다. 1글자는 직접 색인 읽기가 필요합니다.".into(),
        );
    }
    if hierarchy {
        return Err("서비스 파이프는 전체 위계 순위 질의를 아직 지원하지 않습니다.".into());
    }
    let request = serde_json::to_vec(&json!({"command":"query", "id":1,
        "text":query,"top":LIMIT,"refs":1}))
    .map_err(|e| e.to_string())?;
    let mut pipe = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(name)
        .map_err(|e| e.to_string())?;
    pipe.write_all(&(request.len() as u32).to_le_bytes())
        .and_then(|_| pipe.write_all(&request))
        .map_err(|e| e.to_string())?;
    let mut prefix = [0; 4];
    pipe.read_exact(&mut prefix).map_err(|e| e.to_string())?;
    let n = u32::from_le_bytes(prefix) as usize;
    if n > 8 * 1024 * 1024 {
        return Err("검색 응답 크기가 상한을 넘었습니다.".into());
    }
    let mut bytes = vec![0; n];
    pipe.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    decode_rows(&bytes)
}

fn decode_rows(bytes: &[u8]) -> Result<Vec<Row>> {
    let value: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if value["ok"] != true {
        return Err(value["error"].as_str().unwrap_or("색인 서비스 오류").into());
    }
    let hits = value["result"]["hits"]
        .as_array()
        .ok_or("검색 응답에 결과가 없습니다.")?;
    hits.iter()
        .map(|h| {
            let hit = DocHit {
                doc_id: h["doc_id"].as_u64().ok_or("문서 번호가 없습니다.")?,
                adjacent: h["adjacent"].as_bool().unwrap_or(false),
                level: h["level"].as_u64().unwrap_or(255) as u8,
                count: h["count"].as_u64().unwrap_or(0) as u32,
                date: h["date"].as_i64().unwrap_or(0),
                sentences: h["sentences"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(1)
                    .map(|s| kfind_core::SentenceRef {
                        ord: s["ord"].as_u64().unwrap_or(0) as u32,
                        para: s["para"].as_u64().unwrap_or(0) as u32,
                        level: s["level"].as_u64().unwrap_or(255) as u8,
                        adjacent: s["adjacent"].as_bool().unwrap_or(false),
                    })
                    .collect(),
            };
            Ok(row(&hit, h["path"].as_str()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kfind_core::{Sentence, Writer, WriterOptions};
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "ktm-backend-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn mapped_queries_character_hierarchy_deleted_and_read_only() {
        let t = Temp::new();
        let mut writer = Writer::open(&t.0, WriterOptions::default()).unwrap();
        writer
            .add_document(
                1,
                10,
                &[Sentence {
                    para: 0,
                    level: 3,
                    text: "예산집행 계획",
                }],
            )
            .unwrap();
        writer
            .add_document(
                2,
                20,
                &[Sentence {
                    para: 1,
                    level: 0,
                    text: "예산집행 추진",
                }],
            )
            .unwrap();
        writer
            .add_document(
                3,
                30,
                &[Sentence {
                    para: 2,
                    level: 0,
                    text: "삭제 예산",
                }],
            )
            .unwrap();
        writer.commit().unwrap();
        writer.delete_document(3).unwrap();
        writer.commit().unwrap();
        let records = concat!(
            "{\"op\":\"put\",\"key\":\"a\",\"id\":1,\"meta\":{\"path\":\"folder/a.hwp\"}}\n",
            "{\"op\":\"put\",\"key\":\"b\",\"id\":2,\"meta\":{\"path\":\"folder/b.hwp\"}}\n",
            "{bad"
        );
        std::fs::write(t.0.join("doctable.jsonl"), records).unwrap();
        let before = std::fs::read(t.0.join("MANIFEST.kfm")).unwrap();
        let mut backend = Backend::open(&t.0).unwrap();
        let hits = backend.search("예산", true).unwrap();
        assert_eq!(hits.iter().map(|r| r.doc_id).collect::<Vec<_>>(), [2, 1]);
        assert_eq!(hits[0].name, "b.hwp");
        assert!(hits[0].snippet.is_empty());
        assert_eq!(backend.search("예", false).unwrap().len(), 2);
        assert_eq!(std::fs::read(t.0.join("MANIFEST.kfm")).unwrap(), before);
        assert_eq!(
            std::fs::read_to_string(t.0.join("doctable.jsonl")).unwrap(),
            records
        );
    }
    #[test]
    fn replay_replacement_deletion_and_missing_index() {
        let t = Temp::new();
        assert!(Backend::open(&t.0).is_err());
        let p = t.0.join("doctable.jsonl");
        std::fs::write(
            &p,
            concat!(
                "{\"op\":\"put\",\"key\":\"a\",\"id\":1,\"meta\":{\"path\":\"old\"}}\n",
                "{\"op\":\"put\",\"key\":\"a\",\"id\":2,\"meta\":{\"path\":\"new\"}}\n",
                "{\"op\":\"del\",\"key\":\"a\"}\n"
            ),
        )
        .unwrap();
        assert!(
            read_paths(&p, Some(&[1, 2].into_iter().collect()))
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn character_scan_honors_cancellation_and_scope() {
        use std::sync::{Arc, atomic::AtomicBool};
        let t = Temp::new();
        let mut writer = Writer::open(&t.0, WriterOptions::default()).unwrap();
        writer
            .add_document(
                1,
                1,
                &[Sentence {
                    para: 0,
                    level: 2,
                    text: "예산집행",
                }],
            )
            .unwrap();
        writer.commit().unwrap();
        let index = Index::open(&t.0).unwrap();
        let opts = SearchOptions {
            cancel: Some(Arc::new(AtomicBool::new(true))),
            ..SearchOptions::default()
        };
        assert!(matches!(
            index.search_character("예", &opts),
            Err(kfind_core::Error::Cancelled)
        ));
        let opts = SearchOptions {
            allow: Some(Arc::new(kfind_core::RoaringTreemap::new())),
            ..SearchOptions::default()
        };
        assert!(index.search_character("예", &opts).unwrap().hits.is_empty());
        assert!(
            index
                .search_character("예산", &SearchOptions::default())
                .is_err()
        );
    }
    #[test]
    fn service_error_and_original_text_absence() {
        assert!(
            decode_rows(br#"{"ok":false,"error":"offline"}"#)
                .unwrap_err()
                .contains("offline")
        );
        let rows = decode_rows(
            br#"{"ok":true,"result":{"hits":[{"doc_id":4,"path":"x.hwp","level":1,"count":2}]}}"#,
        )
        .unwrap();
        assert_eq!(rows[0].name, "x.hwp");
        assert!(rows[0].snippet.is_empty());
        assert!(rows[0].location.contains("원문 미게시"));
    }
}
