//! 업무자료함(노트북) 별도 저장소: `<자료함 폴더>\notebook\notebook.db`(SQLite)와 `notebook\tantivy`(본문 색인).
//!
//! klib 연결에 `nb`로 붙인다(ATTACH). 자료함 DB(main)에는 업무자료함 표를 두지 않으므로, 기존 nb_* SQL은 표 이름을
//! 바꾸지 않고도 별도 DB 파일을 읽고 쓴다. 가상 폴더·자료함 공용 색인(`library\tantivy`)과 섞이지 않는다.
//!
//! 처음 실행 때 자료함 DB에 남아 있는 업무자료함 자료를 한 번 옮긴다(`MigrationState`).
//! 옮기기 전에 자료함 DB 전체를 백업하고, 새 파일은 `.partial` 이름으로 다 만든 뒤 마지막에 이름을 바꾼다.
//! 실패하면 만든 파일만 지우고 자료함 DB는 그대로 둔다(다음 실행이나 `nb_migrate`로 다시 시도).
//! 상태는 `notebook\migration.json`(에이전트가 읽는 파일)과 `nb_status` 명령으로 볼 수 있다.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::{params, Connection, OptionalExtension};

use crate::json::Value;
use crate::search::SearchIndex;

pub const DIR: &str = "notebook";
const DB_FILE: &str = "notebook.db";
const INDEX_DIR: &str = "tantivy";
const STATE_FILE: &str = "migration.json";
const LOCK_FILE: &str = "migration.lock";
/// 업무자료함 표(외래키 부모가 앞). 옮기기·떼어 두기 순서.
const TABLES: [&str; 6] = ["notebook", "nb_source", "nb_paragraph", "nb_note", "nb_citation", "nb_note_tag"];

/// 옮기기 상태.
///
/// ```text
/// not_started ─자료 없음─▶ not_needed
/// not_started | failed ─▶ backing_up ─▶ copying ─▶ indexing ─▶ verifying ─▶ switching ─▶ done
///                         (각 단계에서 오류) ─────────────────────────────────────────▶ failed
/// ```
/// 불가능한 전이: 단계 건너뛰기(예: copying→switching), done·not_needed에서 다시 옮기기, failed에서 곧바로 done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MigrationState { NotStarted, NotNeeded, BackingUp, Copying, Indexing, Verifying, Switching, Done, Failed }

impl MigrationState {
    pub fn as_str(self) -> &'static str {
        match self { Self::NotStarted => "not_started", Self::NotNeeded => "not_needed", Self::BackingUp => "backing_up", Self::Copying => "copying",
            Self::Indexing => "indexing", Self::Verifying => "verifying", Self::Switching => "switching", Self::Done => "done", Self::Failed => "failed" }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s { "not_started" => Self::NotStarted, "not_needed" => Self::NotNeeded, "backing_up" => Self::BackingUp, "copying" => Self::Copying,
            "indexing" => Self::Indexing, "verifying" => Self::Verifying, "switching" => Self::Switching, "done" => Self::Done, "failed" => Self::Failed, _ => return None })
    }
    pub fn can_go(self, to: Self) -> bool {
        use MigrationState::*;
        matches!((self, to), (NotStarted, NotNeeded) | (NotStarted, BackingUp) | (Failed, BackingUp) | (BackingUp, Copying) | (Copying, Indexing)
            | (Indexing, Verifying) | (Verifying, Switching) | (Switching, Done))
            || (to == Failed && matches!(self, BackingUp | Copying | Indexing | Verifying | Switching))
    }
    /// 옮기기가 끝나 별도 저장소를 쓰는 상태.
    pub fn settled(self) -> bool { matches!(self, Self::Done | Self::NotNeeded) }
}

/// 옮기기 기록(`migration.json`).
#[derive(Clone, Debug, PartialEq)]
pub struct Migration {
    pub state: MigrationState,
    /// 실패한 단계(state가 failed일 때).
    pub failed_step: Option<MigrationState>,
    pub attempts: i64,
    pub error: String,
    pub backup: String,
    /// 표마다 (옮기기 전 행 수, 옮긴 뒤 행 수).
    pub counts: Vec<(String, i64, i64)>,
    pub indexed_paragraphs: i64,
    /// 옆으로 떼어 둔 옛 표 이름 접두사(자료함 DB 안, 지우지 않음).
    pub legacy_prefix: String,
    pub updated_at: i64,
    /// 단계별 걸린 시간(ms), 마지막 시도.
    pub step_ms: Vec<(String, i64)>,
}

impl Default for Migration {
    fn default() -> Self { Self { state: MigrationState::NotStarted, failed_step: None, attempts: 0, error: String::new(), backup: String::new(), counts: Vec::new(), indexed_paragraphs: 0, legacy_prefix: String::new(), updated_at: 0, step_ms: Vec::new() } }
}

impl Migration {
    fn go(&mut self, to: MigrationState) -> Result<(), String> {
        if !self.state.can_go(to) { return Err(format!("옮기기 상태를 {}에서 {}로 바꿀 수 없습니다.", self.state.as_str(), to.as_str())); }
        // 단계 시간: 들어갈 때 -시작 시각을 적고, 다음 단계로 갈 때 걸린 시간으로 바꾼다.
        let now = crate::util::now_ms() as i64;
        if let Some((_, v)) = self.step_ms.last_mut().filter(|(_, v)| *v < 0) { *v += now; }
        if !matches!(to, MigrationState::Done | MigrationState::Failed | MigrationState::NotNeeded) { self.step_ms.push((to.as_str().to_string(), -now)); }
        if to == MigrationState::Failed { self.failed_step = Some(self.state); } else { self.failed_step = None; self.error.clear(); }
        self.state = to;
        self.updated_at = crate::util::now() as i64;
        Ok(())
    }
    pub fn to_json(&self) -> Value {
        Value::obj(vec![
            ("state", self.state.as_str().into()),
            ("failed_step", self.failed_step.map(|s| Value::from(s.as_str())).unwrap_or(Value::Null)),
            ("attempts", self.attempts.into()), ("error", self.error.clone().into()), ("backup", self.backup.clone().into()),
            ("counts", Value::Arr(self.counts.iter().map(|(t, before, after)| Value::obj(vec![("table", t.clone().into()), ("before", (*before).into()), ("after", (*after).into())])).collect())),
            ("indexed_paragraphs", self.indexed_paragraphs.into()), ("legacy_prefix", self.legacy_prefix.clone().into()), ("updated_at", self.updated_at.into()),
            ("step_ms", Value::obj(self.step_ms.iter().map(|(k, v)| (k.as_str(), Value::from(*v))).collect())),
        ])
    }
    fn from_json(v: &Value) -> Self {
        let counts = v.arr("counts").unwrap_or(&[]).iter().filter_map(|c| Some((c.str("table")?.to_string(), c.i64("before")?, c.i64("after")?))).collect();
        Self {
            state: v.str("state").and_then(MigrationState::parse).unwrap_or(MigrationState::NotStarted),
            failed_step: v.str("failed_step").and_then(MigrationState::parse),
            attempts: v.i64("attempts").unwrap_or(0), error: v.str("error").unwrap_or("").into(), backup: v.str("backup").unwrap_or("").into(),
            counts, indexed_paragraphs: v.i64("indexed_paragraphs").unwrap_or(0), legacy_prefix: v.str("legacy_prefix").unwrap_or("").into(), updated_at: v.i64("updated_at").unwrap_or(0),
            step_ms: match v.get("step_ms") { Some(Value::Obj(items)) => items.iter().filter_map(|(k, v)| match v { Value::Num(n) => Some((k.clone(), *n as i64)), _ => None }).collect(), _ => Vec::new() },
        }
    }
}

/// 열린 별도 저장소. 연결은 자료함 DB 연결(`nb`로 붙임)을 같이 쓴다.
pub struct Store { pub index: Arc<SearchIndex> }

pub fn root_for(library_db: &Path) -> PathBuf { library_db.parent().unwrap_or(Path::new(".")).join(DIR) }
pub fn db_path(root: &Path) -> PathBuf { root.join(DB_FILE) }
pub fn index_path(root: &Path) -> PathBuf { root.join(INDEX_DIR) }

pub fn read_migration(root: &Path) -> Migration {
    std::fs::read_to_string(root.join(STATE_FILE)).ok().and_then(|s| crate::json::parse(&s)).map(|v| Migration::from_json(&v)).unwrap_or_default()
}
fn write_migration(root: &Path, m: &Migration) {
    let tmp = root.join(format!("{STATE_FILE}.tmp"));
    if std::fs::write(&tmp, crate::json::to_string(&m.to_json())).is_ok() { let _ = std::fs::rename(&tmp, root.join(STATE_FILE)); }
}

fn e(err: rusqlite::Error) -> String { err.to_string() }

fn table_exists(c: &Connection, schema: &str, table: &str) -> Result<bool, String> {
    c.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {schema}.sqlite_master WHERE type='table' AND name=?1)"), [table], |r| r.get(0)).map_err(e)
}
fn schema_attached(c: &Connection, schema: &str) -> bool {
    c.prepare("SELECT name FROM pragma_database_list").and_then(|mut s| s.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()).map(|v| v.iter().any(|n| n == schema)).unwrap_or(false)
}
/// 자료함 DB(main)에 남은 업무자료함 자료 행 수(노트북·출처·노트).
fn legacy_rows(c: &Connection) -> Result<i64, String> {
    let mut n = 0;
    for t in ["notebook", "nb_source", "nb_note"] { if table_exists(c, "main", t)? { n += c.query_row(&format!("SELECT count(*) FROM main.{t}"), [], |r| r.get::<_, i64>(0)).map_err(e)?; } }
    Ok(n)
}

fn meta_get(c: &Connection, schema: &str, key: &str) -> Option<String> {
    c.query_row(&format!("SELECT value FROM {schema}.nb_meta WHERE key=?1"), [key], |r| r.get(0)).optional().ok().flatten()
}
fn meta_set(c: &Connection, schema: &str, key: &str, value: &str) -> Result<(), String> {
    c.execute(&format!("INSERT INTO {schema}.nb_meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value"), params![key, value]).map(|_| ()).map_err(e)
}
fn init_meta(c: &Connection, schema: &str) -> Result<(), String> {
    c.execute_batch(&format!("CREATE TABLE IF NOT EXISTS {schema}.nb_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);")).map_err(e)
}

/// 다른 프로세스가 동시에 옮기지 않게 잡는 잠금(프로세스가 죽으면 OS가 푼다).
fn lock(root: &Path) -> Result<std::fs::File, String> {
    #[cfg(windows)]
    { use std::os::windows::fs::OpenOptionsExt; std::fs::OpenOptions::new().write(true).create(true).truncate(false).share_mode(0).open(root.join(LOCK_FILE)).map_err(|_| "다른 프로세스가 업무자료함 자료를 옮기는 중입니다.".to_string()) }
    #[cfg(not(windows))]
    { std::fs::OpenOptions::new().write(true).create(true).truncate(false).open(root.join(LOCK_FILE)).map_err(|e| e.to_string()) }
}

// 시험용: 이 스레드에서 옮기기가 이 단계에 들어서면 실패하게 한다.
#[cfg(test)]
thread_local! { static FAIL_AT: std::cell::Cell<Option<MigrationState>> = const { std::cell::Cell::new(None) }; }
fn step(m: &mut Migration, root: &Path, to: MigrationState) -> Result<(), String> {
    m.go(to)?;
    write_migration(root, m);
    #[cfg(test)]
    if FAIL_AT.with(|f| f.get()) == Some(to) { return Err(format!("시험용 실패({})", to.as_str())); }
    Ok(())
}
/// 표의 외래키 위반 수.
fn fk_violations(c: &Connection, schema: &str, table: &str) -> Result<i64, String> {
    let mut s = c.prepare(&format!("PRAGMA {schema}.foreign_key_check({table})")).map_err(e)?;
    let mut rows = s.query([]).map_err(e)?;
    let mut n = 0;
    while rows.next().map_err(e)?.is_some() { n += 1; }
    Ok(n)
}

fn remove_db_files(path: &Path) {
    for suffix in ["", "-wal", "-shm", "-journal"] { let _ = std::fs::remove_file(PathBuf::from(format!("{}{suffix}", path.display()))); }
}

/// klib 연결에 별도 저장소를 붙인다. 필요하면 옮기기를 하고, 색인이 없거나 낡았으면 DB에서 다시 만든다.
/// 실패하면 (오류, 옮기기 기록)을 돌려준다. 자료함 DB의 자료는 바뀌지 않는다.
pub fn open(c: &Connection, library_db: &Path) -> (Result<Store, String>, Migration) {
    let root = root_for(library_db);
    if let Err(err) = std::fs::create_dir_all(&root) { return (Err(format!("업무자료함 폴더를 만들지 못했습니다: {err}")), Migration::default()); }
    let mut m = read_migration(&root);
    let result = open_inner(c, library_db, &root, &mut m);
    if let Err(err) = &result {
        if m.state != MigrationState::Failed && m.state.can_go(MigrationState::Failed) { let _ = m.go(MigrationState::Failed); }
        if m.error.is_empty() { m.error = err.clone(); }
        if schema_attached(c, "nb") && !m.state.settled() { let _ = c.execute_batch("DETACH DATABASE nb"); }
    }
    write_migration(&root, &m);
    (result, m)
}

fn open_inner(c: &Connection, library_db: &Path, root: &Path, m: &mut Migration) -> Result<Store, String> {
    let db = db_path(root);
    if !db.exists() {
        let _guard = lock(root)?;
        if !db.exists() {
            if legacy_rows(c)? > 0 { migrate(c, library_db, root, m)?; } else { create_empty(root, m)?; }
        }
    }
    attach(c, &db)?;
    // 옮기기가 이름 바꾸기 직후 멈췄으면 옛 표 떼어 두기를 마저 한다. 옛 버전이 다시 만든 빈 표도 떼어 둔다.
    if table_exists(c, "main", "notebook")? || table_exists(c, "main", "nb_source")? {
        let prefix = set_aside_legacy(c)?;
        if m.legacy_prefix.is_empty() { m.legacy_prefix = prefix; }
    }
    if meta_get(c, "nb", "migration_state").as_deref() == Some("switching") { meta_set(c, "nb", "migration_state", "done")?; }
    if m.state == MigrationState::Switching { m.go(MigrationState::Done)?; }
    if !m.state.settled() { m.state = if meta_get(c, "nb", "migrated_from").is_some_and(|v| v != "none") { MigrationState::Done } else { MigrationState::NotNeeded }; m.failed_step = None; m.error.clear(); }
    let index = open_index(c, "nb", &index_path(root))?;
    Ok(Store { index: Arc::new(index) })
}

fn attach(c: &Connection, db: &Path) -> Result<(), String> {
    if !schema_attached(c, "nb") { c.execute("ATTACH DATABASE ?1 AS nb", [db.to_string_lossy().as_ref()]).map_err(e)?; }
    let _ = c.query_row("PRAGMA nb.journal_mode=WAL", [], |_| Ok(()));
    crate::notebook::init_schema_in(c, "nb").map_err(e)?;
    init_meta(c, "nb")
}

/// 옮길 자료가 없을 때: 빈 저장소를 만든다.
fn create_empty(root: &Path, m: &mut Migration) -> Result<(), String> {
    let partial = root.join(format!("{DB_FILE}.partial"));
    remove_db_files(&partial);
    let made = (|| -> Result<(), String> {
        let p = Connection::open(&partial).map_err(e)?;
        crate::notebook::init_schema(&p).map_err(e)?;
        init_meta(&p, "main")?;
        meta_set(&p, "main", "migration_state", "done")?;
        meta_set(&p, "main", "migrated_from", "none")?;
        meta_set(&p, "main", "created_at", &crate::util::now().to_string())?;
        drop(p);
        std::fs::rename(&partial, db_path(root)).map_err(|err| err.to_string())
    })();
    if made.is_err() { remove_db_files(&partial); }
    made?;
    if m.state == MigrationState::NotStarted { m.go(MigrationState::NotNeeded)?; }
    Ok(())
}

/// 표마다 옮길 열(옛 DB에 없는 열은 기본값).
fn columns(c: &Connection, schema: &str, table: &str) -> Result<Vec<String>, String> {
    let mut s = c.prepare(&format!("SELECT name FROM pragma_table_info('{table}','{schema}')")).map_err(e)?;
    s.query_map([], |r| r.get::<_, String>(0)).map_err(e)?.collect::<rusqlite::Result<_>>().map_err(e)
}

fn migrate(c: &Connection, library_db: &Path, root: &Path, m: &mut Migration) -> Result<(), String> {
    m.attempts += 1;
    m.step_ms.clear();
    m.counts.clear();
    m.indexed_paragraphs = 0;
    step(m, root, MigrationState::BackingUp)?;
    let partial = root.join(format!("{DB_FILE}.partial"));
    let index_partial = root.join(format!("{INDEX_DIR}.partial"));
    let result = (|| -> Result<(), String> {
        // 1. 자료함 DB 전체 백업(옮기기 전 상태 그대로).
        let backup_dir = root.join("backup");
        std::fs::create_dir_all(&backup_dir).map_err(|err| err.to_string())?;
        let backup = backup_dir.join(format!("library-before-notebook-split-{}.db", crate::util::now_ms()));
        c.execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()]).map_err(|err| format!("백업 실패: {err}"))?;
        m.backup = backup.to_string_lossy().into_owned();

        // 2. 새 DB(.partial)에 출처·노트 번호 그대로 복사.
        step(m, root, MigrationState::Copying)?;
        remove_db_files(&partial);
        let p = Connection::open(&partial).map_err(e)?;
        crate::notebook::init_schema(&p).map_err(e)?;
        init_meta(&p, "main")?;
        // 옛 자료를 있는 그대로 옮긴다(외래키는 옮긴 뒤 옛 DB와 위반 수가 같은지로 확인).
        p.execute_batch("PRAGMA foreign_keys=OFF").map_err(e)?;
        p.execute("ATTACH DATABASE ?1 AS old", [library_db.to_string_lossy().as_ref()]).map_err(e)?;
        let tx = p.unchecked_transaction().map_err(e)?;
        for t in TABLES {
            if !table_exists(&tx, "old", t)? { m.counts.push((t.to_string(), 0, 0)); continue; }
            let old_cols = columns(&tx, "old", t)?;
            let cols: Vec<String> = columns(&tx, "main", t)?.into_iter().filter(|col| old_cols.contains(col)).collect();
            let list = cols.join(",");
            tx.execute(&format!("INSERT INTO main.{t}({list}) SELECT {list} FROM old.{t}"), []).map_err(|err| format!("{t} 복사 실패: {err}"))?;
            let before: i64 = tx.query_row(&format!("SELECT count(*) FROM old.{t}"), [], |r| r.get(0)).map_err(e)?;
            let after: i64 = tx.query_row(&format!("SELECT count(*) FROM main.{t}"), [], |r| r.get(0)).map_err(e)?;
            m.counts.push((t.to_string(), before, after));
        }
        // 지운 번호를 다시 쓰지 않도록 AUTOINCREMENT 다음 번호도 옮긴다.
        if table_exists(&tx, "old", "sqlite_sequence")? {
            const SEQ: &str = "('notebook','nb_source','nb_note','nb_citation')";
            tx.execute_batch(&format!("UPDATE main.sqlite_sequence SET seq=(SELECT o.seq FROM old.sqlite_sequence o WHERE o.name=main.sqlite_sequence.name) WHERE name IN {SEQ} AND (SELECT o.seq FROM old.sqlite_sequence o WHERE o.name=main.sqlite_sequence.name)>seq;
                INSERT INTO main.sqlite_sequence(name,seq) SELECT o.name,o.seq FROM old.sqlite_sequence o WHERE o.name IN {SEQ} AND NOT EXISTS(SELECT 1 FROM main.sqlite_sequence s WHERE s.name=o.name);")).map_err(e)?;
        }
        for t in TABLES {
            if !table_exists(&tx, "old", t)? { continue; }
            let (before, after) = (fk_violations(&tx, "old", t)?, fk_violations(&tx, "main", t)?);
            if before != after { return Err(format!("{t} 외래키 위반 수가 다릅니다({before} → {after}).")); }
        }
        meta_set(&tx, "main", "migration_state", "switching")?;
        meta_set(&tx, "main", "migrated_from", &library_db.to_string_lossy())?;
        meta_set(&tx, "main", "migrated_at", &crate::util::now().to_string())?;
        meta_set(&tx, "main", "backup", &m.backup)?;
        tx.commit().map_err(e)?;
        p.execute_batch("DETACH DATABASE old").map_err(e)?;

        // 3. 새 색인(.partial)을 새 DB로 만든다.
        step(m, root, MigrationState::Indexing)?;
        let _ = std::fs::remove_dir_all(&index_partial);
        let _ = std::fs::remove_file(&index_partial);
        {
            let ix = SearchIndex::open_or_create_with_threads(&index_partial, 1).map_err(|err| err.to_string())?;
            m.indexed_paragraphs = fill_index(&p, &ix)? as i64;
            ix.flush().map_err(|err| err.to_string())?;
        }

        // 4. 확인: 표마다 행 수가 같고, 색인 문단 수가 DB 문단 수와 같다.
        step(m, root, MigrationState::Verifying)?;
        if let Some((t, before, after)) = m.counts.iter().find(|(_, before, after)| before != after) { return Err(format!("{t} 행 수가 다릅니다({before} → {after}).")); }
        let paragraphs: i64 = p.query_row("SELECT count(*) FROM nb_paragraph", [], |r| r.get(0)).map_err(e)?;
        let docs = SearchIndex::open_or_create(&index_partial).map_err(|err| err.to_string())?.doc_count() as i64;
        if docs != paragraphs { return Err(format!("색인 문단 수({docs})가 DB 문단 수({paragraphs})와 다릅니다.")); }
        let integrity: String = p.query_row("PRAGMA integrity_check", [], |r| r.get(0)).map_err(e)?;
        if integrity != "ok" { return Err(format!("새 DB 검사 실패: {integrity}")); }
        drop(p);

        // 5. 이름 바꾸기(색인 → DB 순). 옛 표 떼어 두기는 open_inner가 붙인 뒤 한 트랜잭션으로 한다.
        step(m, root, MigrationState::Switching)?;
        let index = index_path(root);
        if index.exists() { std::fs::remove_dir_all(&index).map_err(|err| format!("옛 색인 폴더 정리 실패: {err}"))?; }
        std::fs::rename(&index_partial, &index).map_err(|err| format!("색인 폴더 이름 바꾸기 실패: {err}"))?;
        std::fs::rename(&partial, db_path(root)).map_err(|err| format!("DB 이름 바꾸기 실패: {err}"))?;
        // 공용 색인에 남은 업무자료함 문단은 다음 요청 때 지운다(pipe).
        Ok(())
    })();
    if let Err(err) = &result {
        remove_db_files(&partial);
        let _ = std::fs::remove_dir_all(&index_partial);
        if m.state == MigrationState::Switching && !db_path(root).exists() { let _ = std::fs::remove_dir_all(index_path(root)); }
        m.error = err.clone();
        let _ = m.go(MigrationState::Failed);
        m.error = err.clone();
        write_migration(root, m);
    }
    result
}

/// 자료함 DB(main)의 옛 업무자료함 표를 `nbidx_legacy_<시각>_<표>`로 이름만 바꿔 둔다(한 트랜잭션, 지우지 않음).
/// 이렇게 해야 klib 연결의 이름 없는 nb_* 표가 붙인 별도 DB(`nb`)를 가리킨다.
fn set_aside_legacy(c: &Connection) -> Result<String, String> {
    let prefix = format!("nbidx_legacy_{}_", crate::util::now_ms());
    let tx = c.unchecked_transaction().map_err(e)?;
    for t in TABLES.iter().chain(["nb_refresh_paragraph", "nb_refresh", "nb_meta"].iter()) {
        if table_exists(&tx, "main", t)? { tx.execute_batch(&format!("ALTER TABLE main.{t} RENAME TO {prefix}{t}")).map_err(|err| format!("옛 표 떼어 두기 실패({t}): {err}"))?; }
    }
    // 옛 표의 색인 이름이 새 DB 색인 이름과 겹쳐도 되지만(DB가 다름) 찾기 쉽게 그대로 둔다.
    tx.commit().map_err(e)?;
    Ok(prefix)
}

/// 색인을 연다. 처음 만들었거나(폴더가 없었음) 스키마가 바뀌었거나 문단 수가 DB와 다르면 DB로 다시 채운다.
fn open_index(c: &Connection, schema: &str, dir: &Path) -> Result<SearchIndex, String> {
    let existed = dir.join("meta.json").exists();
    let ix = SearchIndex::open_or_create_with_threads(dir, 1).map_err(|err| format!("업무자료함 색인을 열지 못했습니다: {err}"))?;
    let paragraphs: i64 = c.query_row(&format!("SELECT count(*) FROM {schema}.nb_paragraph"), [], |r| r.get(0)).map_err(e)?;
    let dirty = meta_get(c, schema, "index_dirty").is_some();
    if !existed || dirty || ix.needs_rebuild() || ix.doc_count() as i64 != paragraphs {
        ix.clear().map_err(|err| err.to_string())?;
        fill_index(c, &ix)?;
        ix.finish_rebuild().map_err(|err| err.to_string())?;
        if dirty { c.execute(&format!("DELETE FROM {schema}.nb_meta WHERE key='index_dirty'"), []).map_err(e)?; }
    }
    Ok(ix)
}

/// DB의 모든 출처 문단을 색인에 넣는다. 넣은 문단 수.
pub fn fill_index(c: &Connection, ix: &SearchIndex) -> Result<usize, String> {
    let ids: Vec<i64> = { let mut s = c.prepare("SELECT id FROM nb_source ORDER BY id").map_err(e)?; s.query_map([], |r| r.get(0)).map_err(e)?.collect::<rusqlite::Result<_>>().map_err(e)? };
    let batch = ix.batch();
    let mut n = 0;
    for id in ids {
        if let Some(source) = crate::notebook::source_by_id(c, id).map_err(e)? {
            let ps = crate::notebook::paragraphs(c, id).map_err(e)?;
            ix.replace_notebook_source(source.notebook_id, &source, &ps).map_err(|err| err.to_string())?;
            n += ps.len();
        }
    }
    batch.finish().map_err(|err| err.to_string())?;
    Ok(n)
}

/// 공용 색인에서 업무자료함 문단을 지울 차례인지(옮기기 직후 한 번). 지운 뒤 `mark_shared_cleaned`.
pub fn shared_cleanup_pending(c: &Connection) -> bool {
    schema_attached(c, "nb") && meta_get(c, "nb", "migrated_from").is_some_and(|v| v != "none") && meta_get(c, "nb", "shared_index_cleaned").is_none()
}
/// 색인 쓰기가 DB보다 뒤처졌다고 적는다(다음에 열 때 DB로 다시 채움).
pub fn mark_index_dirty(c: &Connection) -> Result<(), String> { meta_set(c, "nb", "index_dirty", &crate::util::now().to_string()) }
pub fn mark_shared_cleaned(c: &Connection) -> Result<(), String> { meta_set(c, "nb", "shared_index_cleaned", &crate::util::now().to_string()) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notebook as nb;

    fn temp(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!("klib-nbstore-{}-{}-{tag}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    fn p(s: i64, o: i64, body: &str) -> nb::Paragraph { nb::Paragraph { source_id: s, ordinal: o, location: String::new(), level: 0, kind: "text".into(), body: body.into() } }

    /// 옛 버전처럼 자료함 DB(main)에 업무자료함 표와 자료를 만든다. 지운 번호가 있어 AUTOINCREMENT가 앞서 있다.
    fn legacy_library(dir: &Path) -> PathBuf {
        let path = dir.join("library.db");
        let c = Connection::open(&path).unwrap();
        c.execute_batch("CREATE TABLE box(id INTEGER PRIMARY KEY AUTOINCREMENT,name TEXT NOT NULL);").unwrap();
        nb::init_schema(&c).unwrap();
        c.execute_batch("DROP TABLE nb_refresh_paragraph; DROP TABLE nb_refresh;").unwrap();
        let n = nb::create(&c, "노트", "").unwrap();
        let s = nb::add_source(&c, n, "txt", "a.txt", "가").unwrap();
        let gone = nb::add_source(&c, n, "txt", "x.txt", "지울 출처").unwrap();
        nb::remove_source(&c, gone).unwrap();
        nb::replace_paragraphs(&c, s, &[p(s, 0, "서울 예산 정책"), p(s, 1, "마포 주민 참여")]).unwrap();
        let note = nb::save_note_with_citations(&c, None, n, "메모", "본문", &[(s, 1, "마포 주민".into())], Some(&["예산".into()])).unwrap();
        assert!(note > 0);
        path
    }

    #[test]
    fn state_machine_rejects_skips_and_restarts() {
        use MigrationState::*;
        for (a, b) in [(NotStarted, BackingUp), (BackingUp, Copying), (Copying, Indexing), (Indexing, Verifying), (Verifying, Switching), (Switching, Done), (Copying, Failed), (Failed, BackingUp), (NotStarted, NotNeeded)] { assert!(a.can_go(b), "{a:?}->{b:?}"); }
        for (a, b) in [(Copying, Switching), (NotStarted, Done), (Done, BackingUp), (NotNeeded, BackingUp), (Failed, Done), (Done, Failed), (NotStarted, Failed)] { assert!(!a.can_go(b), "{a:?}->{b:?}"); }
        for s in [NotStarted, NotNeeded, BackingUp, Copying, Indexing, Verifying, Switching, Done, Failed] { assert_eq!(MigrationState::parse(s.as_str()), Some(s)); }
        let mut m = Migration::default();
        assert!(m.go(Done).is_err());
        m.go(BackingUp).unwrap(); m.go(Failed).unwrap();
        assert_eq!(m.failed_step, Some(BackingUp));
        assert_eq!(Migration::from_json(&m.to_json()), m);
    }

    #[test]
    fn fresh_library_gets_empty_store_without_backup() {
        let dir = temp("fresh");
        let c = Connection::open(dir.join("library.db")).unwrap();
        let (store, m) = open(&c, &dir.join("library.db"));
        let store = store.unwrap();
        assert_eq!(m.state, MigrationState::NotNeeded);
        assert!(m.backup.is_empty());
        assert!(!table_exists(&c, "main", "notebook").unwrap());
        let id = nb::create(&c, "새", "").unwrap();
        assert_eq!(c.query_row("SELECT count(*) FROM nb.notebook WHERE id=?1", [id], |r| r.get::<_, i64>(0)).unwrap(), 1);
        assert_eq!(store.index.doc_count(), 0);
        assert_eq!(read_migration(&root_for(&dir.join("library.db"))).state, MigrationState::NotNeeded);
        drop(store); drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn legacy_data_moves_once_with_backup_same_ids_and_search() {
        let dir = temp("move");
        let lib = legacy_library(&dir);
        let c = Connection::open(&lib).unwrap();
        let (store, m) = open(&c, &lib);
        let store = store.unwrap();
        assert_eq!(m.state, MigrationState::Done, "{m:?}");
        assert_eq!(m.attempts, 1);
        assert!(Path::new(&m.backup).is_file());
        assert!(m.counts.iter().all(|(_, a, b)| a == b));
        assert_eq!(m.counts.iter().find(|(t, ..)| t == "nb_paragraph").unwrap().1, 2);
        // 번호 그대로, 지운 번호는 다시 쓰지 않는다.
        let sources = nb::sources(&c, 1).unwrap();
        assert_eq!(sources.iter().map(|s| s.id).collect::<Vec<_>>(), vec![1]);
        assert_eq!(nb::add_source(&c, 1, "txt", "b.txt", "나").unwrap(), 3);
        assert_eq!(nb::citations(&c, 1).unwrap()[0].0, 1);
        assert_eq!(nb::note_tags(&c, 1).unwrap(), vec!["예산"]);
        // 옛 표는 지우지 않고 이름만 바꿔 둔다. 백업에는 옛 표가 그대로 있다.
        assert!(!table_exists(&c, "main", "notebook").unwrap());
        assert!(table_exists(&c, "main", &format!("{}nb_paragraph", m.legacy_prefix)).unwrap());
        let b = Connection::open(&m.backup).unwrap();
        assert_eq!(b.query_row("SELECT count(*) FROM nb_paragraph", [], |r| r.get::<_, i64>(0)).unwrap(), 2);
        // 별도 색인으로 찾는다.
        assert_eq!(store.index.search_notebook(1, "마포", None, 5).unwrap().iter().map(|h| (h.source_id, h.ordinal)).collect::<Vec<_>>(), vec![(1, 1)]);
        assert!(shared_cleanup_pending(&c));
        mark_shared_cleaned(&c).unwrap();
        assert!(!shared_cleanup_pending(&c));
        drop(store); drop(b); drop(c);
        // 다시 열면 옮기지 않는다.
        let c = Connection::open(&lib).unwrap();
        let (store, m2) = open(&c, &lib);
        assert!(store.is_ok());
        assert_eq!((m2.state, m2.attempts), (MigrationState::Done, 1));
        drop(store); drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn failed_move_leaves_library_untouched_and_retry_succeeds() {
        let dir = temp("fail");
        let lib = legacy_library(&dir);
        let root = root_for(&lib);
        std::fs::create_dir_all(&root).unwrap();
        FAIL_AT.with(|f| f.set(Some(MigrationState::Indexing)));
        let before = std::fs::read(&lib).unwrap();
        let c = Connection::open(&lib).unwrap();
        let (store, m) = open(&c, &lib);
        assert!(store.is_err());
        assert_eq!(m.state, MigrationState::Failed);
        assert_eq!(m.failed_step, Some(MigrationState::Indexing));
        assert!(!m.error.is_empty());
        assert!(!db_path(&root).exists());
        assert!(!root.join(format!("{DB_FILE}.partial")).exists());
        assert!(!schema_attached(&c, "nb"));
        assert!(table_exists(&c, "main", "nb_paragraph").unwrap());
        assert_eq!(c.query_row("SELECT count(*) FROM nb_paragraph", [], |r| r.get::<_, i64>(0)).unwrap(), 2);
        drop(c);
        assert_eq!(std::fs::read(&lib).unwrap(), before, "자료함 DB가 바뀌었다");
        assert_eq!(read_migration(&root).state, MigrationState::Failed);
        assert!(!root.join(format!("{INDEX_DIR}.partial")).exists());
        // 실패 원인이 없어지면 다시 시도해 성공한다.
        FAIL_AT.with(|f| f.set(None));
        let c = Connection::open(&lib).unwrap();
        let (store, m) = open(&c, &lib);
        assert!(store.is_ok(), "{m:?}");
        assert_eq!((m.state, m.attempts), (MigrationState::Done, 2));
        assert_eq!(nb::paragraphs(&c, 1).unwrap().len(), 2);
        drop(store); drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn interrupted_switch_is_finished_and_lost_index_is_rebuilt() {
        let dir = temp("resume");
        let lib = legacy_library(&dir);
        {
            let c = Connection::open(&lib).unwrap();
            let (s, _) = open(&c, &lib);
            drop(s.unwrap());
        }
        let root = root_for(&lib);
        // 이름 바꾸기 직후 멈춘 상태: 새 DB는 switching, 자료함 DB에는 옛 표가 아직 있다.
        let c = Connection::open(&lib).unwrap();
        let prefix = read_migration(&root).legacy_prefix;
        for t in TABLES { c.execute_batch(&format!("ALTER TABLE {prefix}{t} RENAME TO {t}")).unwrap(); }
        let n = Connection::open(db_path(&root)).unwrap();
        meta_set(&n, "main", "migration_state", "switching").unwrap();
        drop(n);
        std::fs::remove_dir_all(index_path(&root)).unwrap();
        let (store, m) = open(&c, &lib);
        let store = store.unwrap();
        assert_eq!(m.state, MigrationState::Done);
        assert!(!table_exists(&c, "main", "notebook").unwrap());
        assert_eq!(meta_get(&c, "nb", "migration_state").as_deref(), Some("done"));
        assert_eq!(store.index.doc_count(), 2);
        drop(store); drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }
}
