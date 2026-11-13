//! Read-only mapped content queries and a bounded service-pipe alternative.
//! The published index holds comparison text and positions only (no original text).
//! Snippets come from the app's SQLite paragraph table (Q-A6), read-only, and only for
//! rows about to be shown; without that table `snippet` stays empty.
use crate::paragraphs::Paragraphs;
use kfind_core::{DocHit, Index, Order, SearchOptions};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, String>;
pub const PIPE_NAME: &str = r"\\.\pipe\HwpAutoDocFit.Index";
const LIMIT: usize = 50;
/// 검색 직후 원문을 함께 가져올 앞 결과 수(기본 창 크기에서 보이는 줄 + 여유). 나머지는 스크롤할 때.
pub const SNIPPET_PREFETCH: usize = 8;
/// 발췌: 첫 일치 앞 글자 수, 최대 글자 수.
const EXCERPT_BEFORE: usize = 16;
const EXCERPT_MAX: usize = 120;

/// 검색 한 번의 결과: 줄과 강조 낱말(검색어 낱말 + 엔진이 나눈 조각).
#[derive(Clone, Debug, Default)]
pub struct Answer {
    pub rows: Vec<Row>,
    pub terms: Vec<String>,
}

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
    /// 첫 일치 문장의 문단 번호(원문 찾기용).
    pub para: Option<u32>,
    /// 원문 찾기를 이미 해 봤는지(없어도 다시 찾지 않음).
    pub snippet_tried: bool,
}

pub struct Backend {
    root: PathBuf,
    index: Index,
    table_stamp: Option<(u64, Option<std::time::SystemTime>)>,
    paths: HashMap<u64, String>,
    para_path: Option<PathBuf>,
    paragraphs: Option<Paragraphs>,
    para_tried: bool,
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
            para_path: None,
            paragraphs: None,
            para_tried: false,
        })
    }

    /// 원문 문단 표 위치(없으면 원문 없이 위치만 보여 줌).
    pub fn with_paragraphs(mut self, path: Option<PathBuf>) -> Self {
        self.para_path = path;
        self
    }

    /// 시작할 때 한 번: 문서 표 읽기, 문단 표 열기, 질의 한 번으로 매핑 쪽을 데운다(창 표시 경로에서 무거운 일 0).
    pub fn warm(&mut self) -> Result<()> {
        self.search_cancel("문서", false, None)?;
        self.search_cancel("문서", true, None)?;
        self.open_paragraphs();
        if let Some(p) = self.paragraphs.as_mut() {
            p.body("", 0)?;
        }
        Ok(())
    }

    fn open_paragraphs(&mut self) {
        if !self.para_tried {
            self.para_tried = true;
            self.paragraphs = self.para_path.as_deref().and_then(|p| Paragraphs::open(p).ok());
        }
    }

    pub fn has_paragraphs(&mut self) -> bool {
        self.open_paragraphs();
        self.paragraphs.is_some()
    }

    /// 아직 찾아보지 않은 줄의 원문 발췌를 채운다(문단 표가 없으면 아무것도 안 함).
    pub fn fill_snippets(&mut self, rows: &mut [Row], terms: &[String]) {
        self.open_paragraphs();
        let Some(p) = self.paragraphs.as_mut() else { return };
        for row in rows.iter_mut().filter(|r| !r.snippet_tried) {
            row.snippet_tried = true;
            let Some(para) = row.para else { continue };
            if row.path.is_empty() {
                continue;
            }
            if let Ok(Some(body)) = p.body(&row.path, para) {
                row.snippet = crate::highlight::excerpt(&body, terms, EXCERPT_BEFORE, EXCERPT_MAX);
            }
        }
    }

    pub fn search(&mut self, query: &str, hierarchy: bool) -> Result<Vec<Row>> {
        self.search_cancel(query, hierarchy, None).map(|a| a.rows)
    }

    pub fn search_cancel(
        &mut self,
        query: &str,
        hierarchy: bool,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<Answer> {
        self.index.reload().map_err(|e| e.to_string())?;
        let query = query.trim();
        if query.is_empty() {
            return Ok(Answer::default());
        }
        let opts = SearchOptions {
            top: LIMIT,
            refs_per_doc: 1,
            cancel,
            ..SearchOptions::default()
        };
        // 1글자 훑기는 이미 위계 먼저 정렬한다(모든 일치가 붙어 있는 일치).
        let order = if hierarchy { Order::Hierarchy } else { Order::Relevance };
        let result = if kfind_core::fold(query).chars().count() == 1 {
            self.index.search_character(query, &opts)
        } else {
            self.index.search_by(query, &opts, order)
        }
        .map_err(|e| e.to_string())?;
        let mut terms: Vec<String> = query.split_whitespace().map(str::to_owned).collect();
        for w in &result.words {
            terms.push(w.text.clone());
            terms.extend(w.pieces.iter().flatten().cloned());
        }
        if terms.is_empty() || kfind_core::fold(query).chars().count() == 1 {
            terms.push(kfind_core::fold(query));
        }
        terms.sort();
        terms.dedup();
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
        let mut rows: Vec<Row> = result
            .hits
            .iter()
            .map(|h| row(h, self.paths.get(&h.doc_id).map(String::as_str)))
            .collect();
        let n = rows.len().min(SNIPPET_PREFETCH);
        self.fill_snippets(&mut rows[..n], &terms);
        Ok(Answer { rows, terms })
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
        .map(|s| format!("문단 {} · 위계 {}", s.para + 1, s.level))
        .unwrap_or_else(|| format!("위계 {}", hit.level));
    Row {
        doc_id: hit.doc_id,
        path,
        name,
        parent,
        snippet: String::new(),
        location,
        level: hit.level,
        count: hit.count,
        para: hit.sentences.first().map(|s| s.para),
        snippet_tried: false,
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
    let bytes = pipe_call(name, &json!({"command":"query", "id":1, "text":query,"top":LIMIT,"refs":1}))?;
    decode_rows(&bytes)
}

/// 길이 접두 JSON 요청 하나 → 응답 바이트(상한 8MB).
pub fn pipe_call(name: &str, request: &Value) -> Result<Vec<u8>> {
    let request = serde_json::to_vec(request).map_err(|e| e.to_string())?;
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
    Ok(bytes)
}

/// 파일명 정렬(Q-F2): 파이프 `names_query`의 `sort` 값과 화면 이름.
pub const NAME_SORTS: [(&str, &str); 4] = [("relevance", "관련도"), ("name", "이름순"), ("size", "크기순"), ("date", "날짜순")];

/// 파일명 탭: 서비스가 메모리에 쥔 드라이브 전체 이름 색인에 `names_query`(서비스 SID 범위, 다른 사용자 프로필 제외).
pub fn pipe_names(name: &str, query: &str, sort: &str) -> Result<Vec<Row>> {
    let bytes = pipe_call(name, &json!({"command":"names_query","id":1,"text":query,"limit":LIMIT,"sort":sort}))?;
    decode_names(&bytes)
}

fn decode_names(bytes: &[u8]) -> Result<Vec<Row>> {
    let value: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if value["ok"] != true {
        let e = value["error"].as_str().unwrap_or("색인 서비스 오류");
        return Err(if e.contains("unavailable") {
            format!("파일명 색인을 준비하는 중입니다({e}).")
        } else {
            e.to_owned()
        });
    }
    let items = value["result"]["items"].as_array().ok_or("파일명 응답에 결과가 없습니다.")?;
    Ok(items
        .iter()
        .map(|i| {
            let name = i["name"].as_str().unwrap_or_default().to_owned();
            let dir = i["dir"].as_str().unwrap_or_default().to_owned();
            let path = if dir.is_empty() {
                name.clone()
            } else if dir.ends_with('\\') {
                format!("{dir}{name}")
            } else {
                format!("{dir}\\{name}")
            };
            let is_dir = i["is_dir"].as_bool().unwrap_or(false);
            let mut info = Vec::new();
            if is_dir {
                info.push("폴더".to_owned());
            } else if let Some(s) = i["size"].as_u64() {
                info.push(format_size(s));
            }
            if let Some(t) = i["mtime"].as_i64().and_then(format_filetime) {
                info.push(t);
            }
            Row { path, name, parent: dir, location: info.join(" · "), level: 255, snippet_tried: true, ..Row::default() }
        })
        .collect())
}

/// 크기 글: 1023 B · 1.5 KB · 12.3 MB · 1.2 GB.
pub fn format_size(n: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut k = 0;
    while v >= 1024.0 && k < U.len() - 1 {
        v /= 1024.0;
        k += 1;
    }
    if k == 0 { format!("{n} B") } else { format!("{v:.1} {}", U[k]) }
}

/// FILETIME(100ns, 1601-01-01 UTC) → 지역 시각 "2026-11-14 09:05". 범위 밖이면 None.
#[cfg(windows)]
pub fn format_filetime(t: i64) -> Option<String> {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
    if t <= 0 {
        return None;
    }
    unsafe {
        let ft = FILETIME { dwLowDateTime: t as u32, dwHighDateTime: (t >> 32) as u32 };
        let mut utc: SYSTEMTIME = std::mem::zeroed();
        let mut local: SYSTEMTIME = std::mem::zeroed();
        if FileTimeToSystemTime(&ft, &mut utc) == 0 || SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) == 0 {
            return None;
        }
        Some(format!("{:04}-{:02}-{:02} {:02}:{:02}", local.wYear, local.wMonth, local.wDay, local.wHour, local.wMinute))
    }
}

#[cfg(not(windows))]
pub fn format_filetime(_t: i64) -> Option<String> {
    None
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
    fn snippets_come_from_paragraph_table_only_for_prefetched_rows() {
        let t = Temp::new();
        let mut writer = Writer::open(&t.0, WriterOptions::default()).unwrap();
        let mut table = String::new();
        for id in 1..=12u64 {
            writer
                .add_document(id, id as i64, &[Sentence { para: 2, level: (id % 3) as u8, text: "예산 집행 계획" }])
                .unwrap();
            table.push_str(&format!(
                "{{\"op\":\"put\",\"key\":\"k{id}\",\"id\":{id},\"meta\":{{\"path\":\"D:\\\\d\\\\{id}.hwp\"}}}}\n"
            ));
        }
        writer.commit().unwrap();
        std::fs::write(t.0.join("doctable.jsonl"), table).unwrap();
        let db = t.0.join("timeline-content.sqlite");
        let bodies: Vec<(String, String)> = (1..=12)
            .map(|id| (format!("D:\\d\\{id}.hwp"), format!("머리말 {id} 이 문서는 아주 긴 머리글을 가진 시험 문단이다\n올해 예산 집행 계획을 세운다 {id}")))
            .collect();
        let rows: Vec<(&str, u32, &str)> = bodies.iter().map(|(p, b)| (p.as_str(), 2, b.as_str())).collect();
        crate::paragraphs::create_fixture(&db, &rows).unwrap();
        let mut backend = Backend::open(&t.0).unwrap().with_paragraphs(Some(db.clone()));
        backend.warm().unwrap();
        let mut answer = backend.search_cancel("예산집행", true, None).unwrap();
        assert_eq!(answer.rows.len(), 12);
        assert!(answer.terms.iter().any(|w| w == "예산") && answer.terms.iter().any(|w| w == "집행"));
        // 위계 순(0 → 1 → 2).
        assert!(answer.rows.windows(2).all(|w| w[0].level <= w[1].level));
        for r in &answer.rows[..SNIPPET_PREFETCH] {
            assert!(r.snippet.contains("예산 집행 계획"), "{:?}", r.snippet);
            assert!(r.snippet.starts_with('…'));
        }
        assert!(answer.rows[SNIPPET_PREFETCH..].iter().all(|r| r.snippet.is_empty() && !r.snippet_tried));
        let terms = answer.terms.clone();
        backend.fill_snippets(&mut answer.rows[SNIPPET_PREFETCH..], &terms);
        assert!(answer.rows.iter().all(|r| !r.snippet.is_empty()));
        // 문단 표가 없으면 원문 없이 위치만.
        let mut plain = Backend::open(&t.0).unwrap().with_paragraphs(Some(t.0.join("none.sqlite")));
        let a = plain.search_cancel("예산", false, None).unwrap();
        assert!(a.rows.iter().all(|r| r.snippet.is_empty() && r.location.starts_with("문단 3")));
        assert!(!t.0.join("none.sqlite").exists());
    }
    #[test]
    fn names_rows_size_date_and_errors() {
        let rows = decode_names(
            br#"{"ok":true,"result":{"items":[{"name":"a.hwp","dir":"C:\\x","is_dir":false,"size":1536,"mtime":133758000000000000},{"name":"x","dir":"C:","is_dir":true,"size":null,"mtime":null},{"name":"C:","dir":"","is_dir":true}]}}"#,
        )
        .unwrap();
        assert_eq!(rows[0].path, "C:\\x\\a.hwp");
        assert!(rows[0].location.starts_with("1.5 KB · 2024-"), "{}", rows[0].location);
        assert_eq!(rows[1].path, "C:\\x");
        assert_eq!(rows[1].location, "폴더");
        assert_eq!(rows[2].path, "C:");
        assert!(rows.iter().all(|r| r.snippet.is_empty() && r.snippet_tried));
        let e = decode_names(br#"{"ok":false,"error":"names index unavailable (building)"}"#).unwrap_err();
        assert!(e.contains("준비하는 중"));
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_filetime(0), None);
        assert!(pipe_names(r"\\.\pipe\ktm-none-test-pipe", "예산", "size").is_err());
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
        assert_eq!(rows[0].location, "위계 1");
        assert_eq!(rows[0].para, None);
    }
}
