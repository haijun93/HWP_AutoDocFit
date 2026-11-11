//! 본문 색인 다시 만들기.
//!
//! 색인 폴더의 스키마 버전이 지금 코드와 다르면 `SearchIndex::open_or_create`가 옛 색인을 버리고
//! `needs_rebuild()`를 켠다. 여기서는 DB(sqlite)에 남아 있는 원본 목록(자료함 항목·노트북 출처)으로
//! 색인을 처음부터 다시 채우고, 다 끝난 뒤에만 버전 표시를 쓴다(`finish_rebuild`).
//! 중간에 죽으면 버전 표시가 없으므로 다음 실행 때 처음부터 다시 한다.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::search::SearchIndex;

/// 이만큼씩 묶어 커밋한다(진행 중에도 이미 넣은 문서는 검색되고, writer 메모리가 한없이 늘지 않는다).
const CHUNK: usize = 200;

static RUNNING: AtomicBool = AtomicBool::new(false);
static DONE: AtomicUsize = AtomicUsize::new(0);
static TOTAL: AtomicUsize = AtomicUsize::new(0);
static FAILED: AtomicUsize = AtomicUsize::new(0);
static LAST_ERROR: Mutex<String> = Mutex::new(String::new());

#[derive(Debug, Default, PartialEq)]
pub struct Report {
    /// 다시 색인하려 한 원본 파일 수(경로 기준, 여러 자료함에 있어도 한 번).
    pub files: usize,
    /// 그중 읽지 못한 파일 수(원본이 없어졌거나 본문이 없는 파일). 재색인을 막지 않는다.
    pub failed: usize,
    pub notebook_sources: usize,
}

/// DB의 원본 목록으로 색인을 다시 채우고 버전 표시를 쓴다. 호출한 스레드에서 끝까지 돈다.
pub fn rebuild(index: &SearchIndex, c: &rusqlite::Connection) -> Result<Report, String> {
    let text = |e: rusqlite::Error| e.to_string();
    // 같은 경로가 여러 자료함에 있으면 한 문서에 자료함 id를 모두 싣는다(이름은 나중 항목 것).
    let mut files: Vec<(String, String, Vec<String>)> = Vec::new();
    {
        let mut by_path: HashMap<String, usize> = HashMap::new();
        let mut s = c.prepare("SELECT source_path,display_name,box_id FROM item WHERE kind IN ('file','text','rich_text','note') AND source_path<>'' ORDER BY id").map_err(text)?;
        let rows = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))).map_err(text)?;
        for row in rows {
            let (path, name, box_id) = row.map_err(text)?;
            let box_id = box_id.to_string();
            match by_path.get(&path) {
                Some(&i) => { files[i].1 = name; if !files[i].2.contains(&box_id) { files[i].2.push(box_id); } }
                None => { by_path.insert(path.clone(), files.len()); files.push((path, name, vec![box_id])); }
            }
        }
    }
    let sources: Vec<i64> = {
        let mut s = c.prepare("SELECT id FROM nb_source ORDER BY id").map_err(text)?;
        s.query_map([], |r| r.get(0)).map_err(text)?.collect::<rusqlite::Result<_>>().map_err(text)?
    };
    TOTAL.store(files.len() + sources.len(), Ordering::Relaxed);
    DONE.store(0, Ordering::Relaxed);
    FAILED.store(0, Ordering::Relaxed);
    let mut report = Report { files: files.len(), ..Report::default() };
    for chunk in files.chunks(CHUNK) {
        let batch = index.batch();
        for (path, name, boxes) in chunk {
            if crate::content::index_file_in_boxes(index, path, name, boxes).is_err() {
                report.failed += 1;
                FAILED.fetch_add(1, Ordering::Relaxed);
            }
            DONE.fetch_add(1, Ordering::Relaxed);
        }
        batch.finish().map_err(|e| e.to_string())?;
    }
    for chunk in sources.chunks(CHUNK) {
        let batch = index.batch();
        for &id in chunk {
            if let Some(source) = crate::notebook::source_by_id(c, id).map_err(text)? {
                let paragraphs = crate::notebook::paragraphs(c, id).map_err(text)?;
                index.replace_notebook_source(source.notebook_id, &source, &paragraphs).map_err(|e| e.to_string())?;
                report.notebook_sources += 1;
            }
            DONE.fetch_add(1, Ordering::Relaxed);
        }
        batch.finish().map_err(|e| e.to_string())?;
    }
    index.finish_rebuild().map_err(|e| e.to_string())?;
    Ok(report)
}

/// 프로세스에서 한 번만: 색인을 열어 보고, 다시 만들어야 하면 배경 스레드에서 다시 만든다.
/// 배경 스레드는 DB를 읽기 전용 연결로 따로 열어, 요청 처리(DB 잠금)를 막지 않는다.
pub fn kick(db: &Mutex<crate::db::Database>) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let path = db.lock().ok().and_then(|g| g.connection().path().map(str::to_string)).unwrap_or_default();
        if path.is_empty() { return; }
        // 색인을 열어 판단하기 전까지도 '색인 중'으로 알린다(막 버린 색인의 빈 결과를 정상 결과로 오해하지 않게).
        RUNNING.store(true, Ordering::SeqCst);
        let spawned = std::thread::Builder::new().name("klib-reindex".into()).spawn(move || {
            let result = (|| {
                let index = crate::search_index()?;
                if !index.needs_rebuild() { return Ok(()); }
                let c = rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| e.to_string())?;
                let _ = c.busy_timeout(std::time::Duration::from_secs(5));
                rebuild(index, &c).map(|_| ())
            })();
            if let Err(e) = result { *LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()) = e; }
            RUNNING.store(false, Ordering::SeqCst);
        });
        if spawned.is_err() { RUNNING.store(false, Ordering::SeqCst); }
    });
}

pub fn is_running() -> bool { RUNNING.load(Ordering::SeqCst) }

/// 한 번 실행 방식(`--request-json`)에서 끝나기 전에 기다린다(중간에 끝나면 다음 실행이 처음부터 다시 해야 한다).
pub fn wait() { while is_running() { std::thread::sleep(std::time::Duration::from_millis(20)); } }

/// (상태, 끝낸 수, 전체 수, 읽지 못한 수, 마지막 오류). 상태: "rebuilding" | "failed" | "ready".
pub fn status() -> (&'static str, usize, usize, usize, String) {
    let error = LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let state = if is_running() { "rebuilding" } else if !error.is_empty() { "failed" } else { "ready" };
    (state, DONE.load(Ordering::Relaxed), TOTAL.load(Ordering::Relaxed), FAILED.load(Ordering::Relaxed), error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::{Paragraph, SearchOptions};

    fn temp(tag: &str) -> std::path::PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!("klib-reindex-{}-{}-{tag}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// 옛 스키마 색인이 남아 있는 폴더를 새 코드로 열면 옛 색인을 버리고, DB의 원본 목록으로 다시 채운 뒤 검색이 된다.
    #[test]
    fn old_schema_index_is_rebuilt_from_db_and_searchable() {
        let root = temp("rebuild");
        let (a, b, gone) = (root.join("가.txt"), root.join("나.md"), root.join("없어진 파일.txt"));
        std::fs::write(&a, "서울 예산 정책\n마포 주민 참여").unwrap();
        std::fs::write(&b, "# 부산 계획\n부산 예산 편성").unwrap();
        let path = |p: &std::path::Path| p.to_string_lossy().into_owned();
        let db = crate::db::Database::open(&path(&root.join("library.db"))).unwrap();
        let c = db.connection();
        let (box1, box2) = (db.box_create("첫째", "").unwrap(), db.box_create("둘째", "").unwrap());
        for (box_id, kind, source, name) in [(box1, "file", &a, "가.txt"), (box2, "file", &a, "가.txt"), (box1, "note", &b, "나.md"), (box1, "file", &gone, "없어진 파일.txt")] {
            c.execute("INSERT INTO item(box_id,kind,source_path,display_name,created_at) VALUES(?1,?2,?3,?4,0)", rusqlite::params![box_id, kind, path(source), name]).unwrap();
        }
        // 본문 색인 대상이 아닌 종류(링크 등)는 다시 색인하지 않는다.
        c.execute("INSERT INTO item(box_id,kind,source_path,display_name,created_at) VALUES(?1,'link','https://example.invalid/서울','링크',0)", [box1]).unwrap();
        let nb = crate::notebook::create(c, "노트", "").unwrap();
        let source = crate::notebook::add_source(c, nb, "txt", "origin", "출처 자료").unwrap();
        crate::notebook::replace_paragraphs(c, source, &[crate::notebook::Paragraph { source_id: source, ordinal: 0, location: "1".into(), level: 0, kind: "text".into(), body: "대전 교통 대책".into() }]).unwrap();

        let dir = root.join("tantivy");
        {
            let old = SearchIndex::open_legacy_for_test(&dir).unwrap();
            old.add_or_update(&Paragraph { path: path(&a), box_ids: vec![box1.to_string()], name: "가.txt".into(), title: String::new(), level: 0, body: "옛날 색인 내용".into(), ext: "txt".into(), mtime: 1, page: None, sheet: None, row: None }).unwrap();
        }
        let index = SearchIndex::open_or_create(&dir).unwrap();
        assert!(index.needs_rebuild());
        assert!(index.search("옛날", SearchOptions { limit: 5 }).unwrap().is_empty());

        let report = rebuild(&index, c).unwrap();
        assert_eq!(report, Report { files: 3, failed: 1, notebook_sources: 1 });
        assert!(!index.needs_rebuild());
        let hits = index.search("서울", SearchOptions { limit: 5 }).unwrap();
        assert_eq!(hits.iter().map(|h| h.path.clone()).collect::<Vec<_>>(), [path(&a)]);
        assert_eq!(hits[0].box_ids, [box1.to_string(), box2.to_string()]);
        assert_eq!(index.search_box(box2, "서울", 5).unwrap().len(), 1);
        assert_eq!(index.search("부산", SearchOptions { limit: 5 }).unwrap().iter().map(|h| h.path.clone()).collect::<Vec<_>>(), [path(&b)]);
        assert!(index.search("옛날", SearchOptions { limit: 5 }).unwrap().is_empty());
        assert_eq!(index.search_notebook(nb, "대전", None, 5).unwrap().len(), 1);
        drop(index);

        // 다시 열면 버전 표시가 맞으므로 그대로 쓴다.
        let index = SearchIndex::open_or_create(&dir).unwrap();
        assert!(!index.needs_rebuild());
        assert_eq!(index.search("서울", SearchOptions { limit: 5 }).unwrap().len(), 1);
        assert_eq!(index.search_notebook(nb, "대전", None, 5).unwrap().len(), 1);
        drop(index);
        drop(db);
        let _ = std::fs::remove_dir_all(root);
    }
}
