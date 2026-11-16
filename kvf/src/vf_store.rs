//! KVF 전용 가상 항목·작업 저널. 참조 경로만 저장하며 파일 시스템 항목을 조작하지 않는다.
use crate::sqlite::{Arg, Connection};
use std::path::Path;

pub const STORE_VERSION: i64 = 1;
pub const JOURNAL_LIMIT: i64 = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtualItem {
    pub id: String,
    pub location: String,
    pub source_ref: String,
    pub display_name: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalEntry {
    pub operation_id: String,
    pub kind: String,
    pub item_id: String,
    pub from_location: Option<String>,
    pub to_location: Option<String>,
    pub display_name: String,
    pub source_ref: String,
    pub undone: bool,
    pub real_copy: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RealCopyEntry {
    pub operation_id: String,
    pub path: String,
    pub source: String,
    pub length: u64,
    pub modified_ns: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportReport {
    pub added: Vec<String>,
    pub duplicates: Vec<String>,
    pub errors: Vec<String>,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        Self::open_at(path.to_str().ok_or("가상 저장소 경로 오류")?)
    }
    fn open_at(path: &str) -> Result<Self, String> {
        let conn = Connection::open(path)?;
        conn.exec("PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS kvf_meta(key TEXT PRIMARY KEY,value INTEGER NOT NULL); INSERT OR IGNORE INTO kvf_meta(key,value) VALUES('schema_version',0);")?;
        let mut version = 0;
        conn.query(
            "SELECT value FROM kvf_meta WHERE key='schema_version'",
            &[],
            |r| version = r.int(0),
        )?;
        if version > STORE_VERSION {
            return Err(format!("지원하지 않는 가상 저장소 버전: {version}"));
        }
        if version < 1 {
            conn.exec("BEGIN IMMEDIATE; CREATE TABLE IF NOT EXISTS virtual_item(id TEXT PRIMARY KEY,location TEXT NOT NULL,source_ref TEXT NOT NULL,display_name TEXT NOT NULL,created_at INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS virtual_item_location ON virtual_item(location); CREATE TABLE IF NOT EXISTS operation_journal(operation_id TEXT PRIMARY KEY,kind TEXT NOT NULL,item_id TEXT NOT NULL,from_location TEXT,to_location TEXT,display_name TEXT NOT NULL,undone INTEGER NOT NULL DEFAULT 0,created_at INTEGER NOT NULL,redone INTEGER NOT NULL DEFAULT 0,source_ref TEXT NOT NULL DEFAULT ''); UPDATE kvf_meta SET value=1 WHERE key='schema_version'; COMMIT;")?;
        }
        conn.exec("CREATE TABLE IF NOT EXISTS real_copy_journal(operation_id TEXT PRIMARY KEY,path TEXT NOT NULL,source TEXT NOT NULL,length INTEGER NOT NULL,modified_ns TEXT NOT NULL,undone INTEGER NOT NULL DEFAULT 0)")?;
        let mut has_redone = false;
        conn.query("PRAGMA table_info(operation_journal)", &[], |r| {
            if r.text(1) == "redone" {
                has_redone = true;
            }
        })?;
        if !has_redone {
            conn.exec(
                "ALTER TABLE operation_journal ADD COLUMN redone INTEGER NOT NULL DEFAULT 0",
            )?;
        }
        let mut has_source_ref = false;
        conn.query("PRAGMA table_info(operation_journal)", &[], |r| {
            if r.text(1) == "source_ref" {
                has_source_ref = true;
            }
        })?;
        if !has_source_ref {
            conn.exec(
                "ALTER TABLE operation_journal ADD COLUMN source_ref TEXT NOT NULL DEFAULT ''",
            )?;
            conn.exec("UPDATE operation_journal SET source_ref=COALESCE((SELECT source_ref FROM virtual_item WHERE virtual_item.id=operation_journal.item_id),'')")?;
        }
        let mut has_from_name = false;
        conn.query("PRAGMA table_info(operation_journal)", &[], |r| {
            if r.text(1) == "from_display_name" {
                has_from_name = true;
            }
        })?;
        if !has_from_name {
            conn.exec("ALTER TABLE operation_journal ADD COLUMN from_display_name TEXT NOT NULL DEFAULT ''")?;
            conn.exec("UPDATE operation_journal SET from_display_name=display_name")?;
        }
        Ok(Self { conn })
    }

    pub fn version(&self) -> Result<i64, String> {
        let mut version = 0;
        self.conn.query(
            "SELECT value FROM kvf_meta WHERE key='schema_version'",
            &[],
            |r| version = r.int(0),
        )?;
        Ok(version)
    }

    pub fn add_reference(
        &self,
        id: &str,
        location: &str,
        source_ref: &str,
        display_name: &str,
        created_at: i64,
    ) -> Result<(), String> {
        if id.is_empty() || location.is_empty() || source_ref.is_empty() {
            return Err("가상 항목 ID·위치·경로 참조는 필수입니다".into());
        }
        self.conn.run("INSERT INTO virtual_item(id,location,source_ref,display_name,created_at) VALUES(?,?,?,?,?)", &[Arg::Text(id),Arg::Text(location),Arg::Text(source_ref),Arg::Text(display_name),Arg::Int(created_at)])?;
        Ok(())
    }

    /// Validates independently so one inaccessible path cannot block its siblings.
    pub fn import_paths(
        &self,
        location: &str,
        paths: &[String],
        created_at: i64,
    ) -> Result<ImportReport, String> {
        let mut report = ImportReport {
            added: vec![],
            duplicates: vec![],
            errors: vec![],
        };
        for (index, raw) in paths.iter().enumerate() {
            let path = Path::new(raw);
            let metadata = match std::fs::metadata(path) {
                Ok(m) => m,
                Err(e) => {
                    report.errors.push(format!("{raw}: {e}"));
                    continue;
                }
            };
            if !metadata.is_file() && !metadata.is_dir() {
                report
                    .errors
                    .push(format!("{raw}: 파일 또는 폴더가 아닙니다"));
                continue;
            }
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| raw.clone());
            let mut duplicate = false;
            self.conn.query(
                "SELECT id FROM virtual_item WHERE location=? AND source_ref=?",
                &[Arg::Text(location), Arg::Text(raw)],
                |_| duplicate = true,
            )?;
            if duplicate {
                report.duplicates.push(raw.clone());
                continue;
            }
            let id = format!(
                "drop-{created_at}-{index}-{}",
                raw.bytes()
                    .fold(0u64, |h, b| h.wrapping_mul(131).wrapping_add(b as u64))
            );
            let inserted=self.conn.run("INSERT INTO virtual_item(id,location,source_ref,display_name,created_at) VALUES(?,?,?,?,?)",&[Arg::Text(&id),Arg::Text(location),Arg::Text(raw),Arg::Text(&name),Arg::Int(created_at)]);
            match inserted {
                Ok(_) => report.added.push(raw.clone()),
                Err(e) => report.errors.push(format!("{raw}: {e}")),
            }
        }
        Ok(report)
    }

    pub fn real_copy(
        &self,
        operation_id: &str,
        sources: &[String],
        target: &Path,
        location: &str,
        created_at: i64,
    ) -> Result<Vec<PathBuf>, String> {
        if sources.is_empty() {
            return Err("복사할 항목이 없습니다".into());
        }
        if !target.is_dir() {
            return Err("대상 폴더에 접근할 수 없습니다".into());
        }
        let mut made: Vec<(PathBuf, String)> = vec![];
        let result = (|| {
            for (i, source) in sources.iter().enumerate() {
                let src = Path::new(source);
                let meta = std::fs::metadata(src).map_err(|e| format!("{source}: {e}"))?;
                if !meta.is_file() && !meta.is_dir() {
                    return Err(format!("{source}: 지원하지 않는 항목입니다"));
                }
                let name = src
                    .file_name()
                    .ok_or("원본 이름을 읽을 수 없습니다")?
                    .to_string_lossy();
                let dest = unique_fs_path(target, &name);
                copy_tree(src, &dest)?;
                made.push((dest.clone(), source.clone()));
                let id = format!("{operation_id}:{i}");
                let display = made
                    .last()
                    .unwrap()
                    .0
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                let md = std::fs::metadata(&made.last().unwrap().0).map_err(|e| e.to_string())?;
                let modified = modified_ns(&md);
                let len = tree_size(&made.last().unwrap().0)?;
                self.conn.exec("BEGIN IMMEDIATE")?;
                let row = (|| {
                    self.conn.run("INSERT INTO virtual_item(id,location,source_ref,display_name,created_at) VALUES(?,?,?,?,?)",&[Arg::Text(&id),Arg::Text(location),Arg::Text(&dest.to_string_lossy()),Arg::Text(&display),Arg::Int(created_at)])?;
                    let op = format!("{operation_id}:{i}");
                    self.conn.run("INSERT INTO operation_journal(operation_id,kind,item_id,from_location,to_location,display_name,undone,created_at,source_ref) VALUES(?,'real-copy',?,NULL,?,?,0,strftime('%s','now'),?)",&[Arg::Text(&op),Arg::Text(&id),Arg::Text(location),Arg::Text(&display),Arg::Text(&dest.to_string_lossy())])?;
                    self.conn.run("INSERT INTO real_copy_journal(operation_id,path,source,length,modified_ns) VALUES(?,?,?,?,?)",&[Arg::Text(&op),Arg::Text(&dest.to_string_lossy()),Arg::Text(source),Arg::Int(len as i64),Arg::Text(&modified.to_string())])
                })();
                if let Err(e) = row {
                    let _ = self.conn.exec("ROLLBACK");
                    return Err(e);
                }
                self.conn.exec("COMMIT")?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            for (p, _) in &made {
                let _ = std::fs::remove_dir_all(p);
                let _ = std::fs::remove_file(p);
            }
            let _ = self.conn.run(
                "DELETE FROM virtual_item WHERE id LIKE ?",
                &[Arg::Text(&format!("{operation_id}:%"))],
            );
            let _ = self.conn.run(
                "DELETE FROM operation_journal WHERE operation_id LIKE ?",
                &[Arg::Text(&format!("{operation_id}:%"))],
            );
            let _ = self.conn.run(
                "DELETE FROM real_copy_journal WHERE operation_id LIKE ?",
                &[Arg::Text(&format!("{operation_id}:%"))],
            );
            return Err(e);
        }
        Ok(made.into_iter().map(|(p, _)| p).collect())
    }

    pub fn undo_real_copy(&self, operation_id: &str, confirm_changed: bool) -> Result<(), String> {
        let mut entries = vec![];
        self.conn.query("SELECT operation_id,path,length,modified_ns FROM real_copy_journal WHERE operation_id=? OR operation_id LIKE ?",&[Arg::Text(operation_id),Arg::Text(&format!("{operation_id}:%"))],|r|entries.push((r.text(0),PathBuf::from(r.text(1)),r.int(2) as u64,r.text(3))))?;
        if entries.is_empty() {
            return Err("실제 복사 작업을 찾을 수 없습니다".into());
        }
        for (_, p, len, mtime) in &entries {
            if !p.exists() {
                continue;
            }
            let md = std::fs::metadata(p).map_err(|e| e.to_string())?;
            if tree_size(p)? != *len || modified_ns(&md).to_string() != *mtime {
                if !confirm_changed {
                    return Err(format!(
                        "복사 후 변경된 파일입니다. 삭제 확인이 필요합니다: {}",
                        p.display()
                    ));
                }
            }
        }
        for (op, p, _, _) in entries {
            if p.is_dir() {
                std::fs::remove_dir_all(&p)
            } else {
                std::fs::remove_file(&p)
            }
            .map_err(|e| e.to_string())?;
            self.conn.run(
                "DELETE FROM virtual_item WHERE source_ref=?",
                &[Arg::Text(&p.to_string_lossy())],
            )?;
            self.conn.run(
                "UPDATE real_copy_journal SET undone=1 WHERE operation_id=?",
                &[Arg::Text(&op)],
            )?;
            self.conn.run(
                "UPDATE operation_journal SET undone=1 WHERE operation_id=?",
                &[Arg::Text(&op)],
            )?;
        }
        Ok(())
    }

    pub fn real_copy_entries(&self, operation_id: &str) -> Result<Vec<RealCopyEntry>, String> {
        let mut out = vec![];
        self.conn.query("SELECT operation_id,path,source,length,modified_ns FROM real_copy_journal WHERE operation_id=? OR operation_id LIKE ?",&[Arg::Text(operation_id),Arg::Text(&format!("{operation_id}:%"))],|r|out.push(RealCopyEntry{operation_id:r.text(0),path:r.text(1),source:r.text(2),length:r.int(3) as u64,modified_ns:r.text(4).parse().unwrap_or(0)}))?;
        Ok(out)
    }

    pub fn list(&self, location: &str) -> Result<Vec<VirtualItem>, String> {
        let mut items = Vec::new();
        self.conn.query("SELECT id,location,source_ref,display_name,created_at FROM virtual_item WHERE location=? ORDER BY created_at,id", &[Arg::Text(location)], |r| items.push(VirtualItem{id:r.text(0),location:r.text(1),source_ref:r.text(2),display_name:r.text(3),created_at:r.int(4)}))?;
        Ok(items)
    }

    pub fn move_virtual(&self, operation_id: &str, item_id: &str, to: &str) -> Result<(), String> {
        self.conn.exec("BEGIN IMMEDIATE")?;
        let result = (|| {
            let mut before = None;
            self.conn.query(
                "SELECT location,display_name,source_ref FROM virtual_item WHERE id=?",
                &[Arg::Text(item_id)],
                |r| before = Some((r.text(0), r.text(1), r.text(2))),
            )?;
            let (from, name, reference) = before.ok_or("가상 항목을 찾을 수 없습니다")?;
            if from == to {
                return Err("같은 가상 위치로 이동할 수 없습니다".into());
            }
            let display = unique_name(&self.conn, to, &name, None)?;
            self.conn.run(
                "UPDATE virtual_item SET location=?,display_name=? WHERE id=?",
                &[Arg::Text(to), Arg::Text(&display), Arg::Text(item_id)],
            )?;
            self.conn.run("INSERT INTO operation_journal(operation_id,kind,item_id,from_location,to_location,display_name,undone,created_at,source_ref,from_display_name) VALUES(?,'move',?,?,?,?,0,strftime('%s','now'),?,?)", &[Arg::Text(operation_id),Arg::Text(item_id),Arg::Text(&from),Arg::Text(to),Arg::Text(&display),Arg::Text(&reference),Arg::Text(&name)])?;
            Ok(())
        })();
        match result {
            Ok(()) => self.conn.exec("COMMIT"),
            Err(e) => {
                let _ = self.conn.exec("ROLLBACK");
                Err(e)
            }
        }
    }

    pub fn move_many(
        &self,
        operation_id: &str,
        item_ids: &[String],
        to: &str,
    ) -> Result<(), String> {
        self.conn.exec("BEGIN IMMEDIATE")?;
        let result = (|| {
            if item_ids.is_empty() {
                return Err("이동할 가상 항목이 없습니다".into());
            }
            for item_id in item_ids {
                let mut before = None;
                self.conn.query(
                    "SELECT location,display_name,source_ref FROM virtual_item WHERE id=?",
                    &[Arg::Text(item_id)],
                    |r| before = Some((r.text(0), r.text(1), r.text(2))),
                )?;
                let (from, name, reference) = before.ok_or("가상 항목을 찾을 수 없습니다")?;
                if from == to {
                    return Err("같은 가상 위치로 이동할 수 없습니다".into());
                }
                let display = unique_name(&self.conn, to, &name, None)?;
                self.conn.run(
                    "UPDATE virtual_item SET location=?,display_name=? WHERE id=?",
                    &[Arg::Text(to), Arg::Text(&display), Arg::Text(item_id)],
                )?;
                let op = format!("{operation_id}:{item_id}");
                self.conn.run("INSERT INTO operation_journal(operation_id,kind,item_id,from_location,to_location,display_name,undone,created_at,source_ref,from_display_name) VALUES(?,'move',?,?,?,?,0,strftime('%s','now'),?,?)",&[Arg::Text(&op),Arg::Text(item_id),Arg::Text(&from),Arg::Text(to),Arg::Text(&display),Arg::Text(&reference),Arg::Text(&name)])?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.conn.exec("COMMIT"),
            Err(e) => {
                let _ = self.conn.exec("ROLLBACK");
                Err(e)
            }
        }
    }

    pub fn copy_many(
        &self,
        operation_id: &str,
        item_ids: &[String],
        to: &str,
        created_at: i64,
    ) -> Result<(), String> {
        self.conn.exec("BEGIN IMMEDIATE")?;
        let result = (|| {
            if item_ids.is_empty() {
                return Err("복제할 가상 항목이 없습니다".into());
            }
            for (n, item_id) in item_ids.iter().enumerate() {
                let mut source = None;
                self.conn.query(
                    "SELECT source_ref,location,display_name FROM virtual_item WHERE id=?",
                    &[Arg::Text(item_id)],
                    |r| source = Some((r.text(0), r.text(1), r.text(2))),
                )?;
                let (reference, from, name) =
                    source.ok_or("복제할 가상 항목을 찾을 수 없습니다")?;
                let copy_id = format!("{operation_id}:{n}");
                let display = unique_name(&self.conn, to, &name, None)?;
                self.conn.run("INSERT INTO virtual_item(id,location,source_ref,display_name,created_at) VALUES(?,?,?,?,?)",&[Arg::Text(&copy_id),Arg::Text(to),Arg::Text(&reference),Arg::Text(&display),Arg::Int(created_at)])?;
                let op = format!("{operation_id}:{n}");
                self.conn.run("INSERT INTO operation_journal(operation_id,kind,item_id,from_location,to_location,display_name,undone,created_at,source_ref) VALUES(?,'copy',?,?,?, ?,0,strftime('%s','now'),?)",&[Arg::Text(&op),Arg::Text(&copy_id),Arg::Text(&from),Arg::Text(to),Arg::Text(&display),Arg::Text(&reference)])?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.conn.exec("COMMIT"),
            Err(e) => {
                let _ = self.conn.exec("ROLLBACK");
                Err(e)
            }
        }
    }

    pub fn virtual_copy(
        &self,
        operation_id: &str,
        source_id: &str,
        copy_id: &str,
        to: &str,
        name: &str,
        created_at: i64,
    ) -> Result<(), String> {
        self.conn.exec("BEGIN IMMEDIATE")?;
        let result = (|| {
            let mut source = None;
            self.conn.query(
                "SELECT source_ref,location,display_name FROM virtual_item WHERE id=?",
                &[Arg::Text(source_id)],
                |r| source = Some((r.text(0), r.text(1), r.text(2))),
            )?;
            let (reference, from, source_name) =
                source.ok_or("복제할 가상 항목을 찾을 수 없습니다")?;
            let display = unique_name(
                &self.conn,
                to,
                if name.is_empty() { &source_name } else { name },
                None,
            )?;
            self.conn.run("INSERT INTO virtual_item(id,location,source_ref,display_name,created_at) VALUES(?,?,?,?,?)", &[Arg::Text(copy_id),Arg::Text(to),Arg::Text(&reference),Arg::Text(&display),Arg::Int(created_at)])?;
            self.conn.run("INSERT INTO operation_journal(operation_id,kind,item_id,from_location,to_location,display_name,undone,created_at,source_ref) VALUES(?,'copy',?,?,?, ?,0,strftime('%s','now'),?)", &[Arg::Text(operation_id),Arg::Text(copy_id),Arg::Text(&from),Arg::Text(to),Arg::Text(&display),Arg::Text(&reference)])?;
            Ok(())
        })();
        match result {
            Ok(()) => self.conn.exec("COMMIT"),
            Err(e) => {
                let _ = self.conn.exec("ROLLBACK");
                Err(e)
            }
        }
    }

    pub fn journal(&self) -> Result<Vec<JournalEntry>, String> {
        let mut entries = Vec::new();
        self.conn.query("SELECT operation_id,kind,item_id,from_location,to_location,display_name,source_ref,undone FROM operation_journal ORDER BY created_at DESC,rowid DESC LIMIT 100", &[], |r| entries.push(JournalEntry{operation_id:r.text(0),kind:r.text(1),item_id:r.text(2),from_location:(!r.text(3).is_empty()).then(||r.text(3)),to_location:(!r.text(4).is_empty()).then(||r.text(4)),display_name:r.text(5),source_ref:r.text(6),undone:r.int(7)!=0,real_copy:r.text(1)=="real-copy"}))?;
        Ok(entries)
    }

    pub fn undo(&self, operation_id: &str) -> Result<(), String> {
        self.conn.exec("BEGIN IMMEDIATE")?;
        let result = (|| {
            let mut entries = Vec::new();
            self.conn.query("SELECT kind,item_id,from_location,to_location,undone,redone,from_display_name FROM operation_journal WHERE operation_id=? OR operation_id LIKE ? ORDER BY rowid", &[Arg::Text(operation_id),Arg::Text(&format!("{operation_id}:%"))], |r| entries.push((r.text(0),r.text(1),r.text(2),r.text(3),r.int(4)!=0,r.int(5)!=0,r.text(6))))?;
            if entries.is_empty() {
                return Err("작업 기록을 찾을 수 없습니다".into());
            }
            if entries.iter().any(|e| e.4 || e.5) {
                return Err("이미 되돌린 작업입니다".into());
            }
            for (kind, id, from, to, _, _, from_name) in entries {
                if kind == "move" {
                    self.conn.run("UPDATE virtual_item SET location=?,display_name=? WHERE id=? AND location=?", &[Arg::Text(&from),Arg::Text(&from_name),Arg::Text(&id),Arg::Text(&to)])?;
                } else if kind == "copy" {
                    self.conn
                        .run("DELETE FROM virtual_item WHERE id=?", &[Arg::Text(&id)])?;
                } else if kind == "real-copy" {
                    return Err("실제 복사는 안전 확인을 거쳐 되돌려야 합니다".into());
                } else {
                    return Err("지원하지 않는 작업 종류입니다".into());
                }
            }
            self.conn.run(
                "UPDATE operation_journal SET undone=1 WHERE operation_id=? OR operation_id LIKE ?",
                &[
                    Arg::Text(operation_id),
                    Arg::Text(&format!("{operation_id}:%")),
                ],
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => self.conn.exec("COMMIT"),
            Err(e) => {
                let _ = self.conn.exec("ROLLBACK");
                Err(e)
            }
        }
    }

    pub fn redo(&self, operation_id: &str) -> Result<(), String> {
        self.conn.exec("BEGIN IMMEDIATE")?;
        let result = (|| {
            let mut entries = Vec::new();
            self.conn.query("SELECT kind,item_id,from_location,to_location,undone,redone,display_name,source_ref FROM operation_journal WHERE operation_id=? OR operation_id LIKE ? ORDER BY rowid", &[Arg::Text(operation_id),Arg::Text(&format!("{operation_id}:%"))], |r| entries.push((r.text(0),r.text(1),r.text(2),r.text(3),r.int(4)!=0,r.int(5)!=0,r.text(6),r.text(7))))?;
            if entries.is_empty() {
                return Err("작업 기록을 찾을 수 없습니다".into());
            }
            if entries.iter().any(|e| !e.4 || e.5) {
                return Err("다시 실행할 수 없는 작업입니다".into());
            }
            for (kind, id, from, to, _, _, name, source_ref) in entries {
                if kind == "move" {
                    self.conn.run("UPDATE virtual_item SET location=?,display_name=? WHERE id=? AND location=?", &[Arg::Text(&to),Arg::Text(&name),Arg::Text(&id),Arg::Text(&from)])?;
                } else if kind == "copy" {
                    self.conn.run("INSERT INTO virtual_item(id,location,source_ref,display_name,created_at) VALUES(?,?,?,?,strftime('%s','now'))", &[Arg::Text(&id),Arg::Text(&to),Arg::Text(&source_ref),Arg::Text(&name)])?;
                } else {
                    return Err("지원하지 않는 작업 종류입니다".into());
                }
            }
            self.conn.run("UPDATE operation_journal SET undone=0,redone=1 WHERE operation_id=? OR operation_id LIKE ?", &[Arg::Text(operation_id),Arg::Text(&format!("{operation_id}:%"))])?;
            Ok(())
        })();
        match result {
            Ok(()) => self.conn.exec("COMMIT"),
            Err(e) => {
                let _ = self.conn.exec("ROLLBACK");
                Err(e)
            }
        }
    }
}

use std::path::PathBuf;
fn unique_fs_path(target: &Path, name: &str) -> PathBuf {
    let first = target.join(name);
    if !first.exists() {
        return first;
    }
    let stem = Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.into());
    let ext = Path::new(name)
        .extension()
        .map(|x| format!(".{}", x.to_string_lossy()))
        .unwrap_or_default();
    for n in 2..=99999 {
        let p = target.join(format!("{stem} ({n}){ext}"));
        if !p.exists() {
            return p;
        }
    }
    target.join(format!("{stem} (복사본){ext}"))
}
fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    if src.is_dir() {
        std::fs::create_dir(dst).map_err(|e| e.to_string())?;
        for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
            let p = entry.map_err(|e| e.to_string())?.path();
            copy_tree(&p, &dst.join(p.file_name().ok_or("파일 이름이 없습니다")?))?
        }
    } else {
        std::fs::copy(src, dst).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn tree_size(p: &Path) -> Result<u64, String> {
    let m = std::fs::metadata(p).map_err(|e| e.to_string())?;
    if m.is_file() {
        return Ok(m.len());
    }
    let mut n = 0;
    for e in std::fs::read_dir(p).map_err(|e| e.to_string())? {
        n += tree_size(&e.map_err(|e| e.to_string())?.path())?;
    }
    Ok(n)
}
fn modified_ns(m: &std::fs::Metadata) -> u128 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
mod qvf_e4_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    struct T(PathBuf);
    impl T {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "qvf-e4-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn path(&self, n: &str) -> PathBuf {
            self.0.join(n)
        }
    }
    impl Drop for T {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn e4_01_imports_single_file() {
        let t = T::new();
        std::fs::write(t.path("a.txt"), "a").unwrap();
        let s = Store::open_at(":memory:").unwrap();
        let r = s
            .import_paths("p1", &[t.path("a.txt").to_string_lossy().into()], 1)
            .unwrap();
        assert_eq!(r.added.len(), 1);
    }
    #[test]
    fn e4_02_imports_folder_as_one_reference() {
        let t = T::new();
        std::fs::create_dir(t.path("folder")).unwrap();
        let s = Store::open_at(":memory:").unwrap();
        assert_eq!(
            s.import_paths("p1", &[t.path("folder").to_string_lossy().into()], 1)
                .unwrap()
                .added
                .len(),
            1
        );
    }
    #[test]
    fn e4_03_reports_missing_path_and_keeps_success() {
        let t = T::new();
        std::fs::write(t.path("ok"), "ok").unwrap();
        let s = Store::open_at(":memory:").unwrap();
        let r = s
            .import_paths(
                "p1",
                &[
                    t.path("missing").to_string_lossy().into(),
                    t.path("ok").to_string_lossy().into(),
                ],
                1,
            )
            .unwrap();
        assert_eq!(r.errors.len(), 1);
        assert_eq!(r.added.len(), 1);
    }
    #[test]
    fn e4_04_warns_same_virtual_location_duplicate() {
        let t = T::new();
        std::fs::write(t.path("a"), "a").unwrap();
        let s = Store::open_at(":memory:").unwrap();
        let p = t.path("a").to_string_lossy().into_owned();
        s.import_paths("p1", &[p.clone()], 1).unwrap();
        assert_eq!(s.import_paths("p1", &[p], 2).unwrap().duplicates.len(), 1);
    }
    #[test]
    fn e4_05_same_path_can_be_referenced_in_other_location() {
        let t = T::new();
        std::fs::write(t.path("a"), "a").unwrap();
        let s = Store::open_at(":memory:").unwrap();
        let p = t.path("a").to_string_lossy().into_owned();
        s.import_paths("p1", &[p.clone()], 1).unwrap();
        assert_eq!(s.import_paths("p2", &[p], 2).unwrap().added.len(), 1);
    }
    #[test]
    fn e4_06_multi_import_is_independent() {
        let t = T::new();
        for n in ["a", "b"] {
            std::fs::write(t.path(n), n).unwrap()
        }
        let s = Store::open_at(":memory:").unwrap();
        assert_eq!(
            s.import_paths(
                "p",
                &[
                    t.path("a").to_string_lossy().into(),
                    t.path("b").to_string_lossy().into()
                ],
                1
            )
            .unwrap()
            .added
            .len(),
            2
        );
    }
    #[test]
    fn e4_07_import_preserves_external_bytes() {
        let t = T::new();
        std::fs::write(t.path("a"), "immutable").unwrap();
        let s = Store::open_at(":memory:").unwrap();
        s.import_paths("p", &[t.path("a").to_string_lossy().into()], 1)
            .unwrap();
        assert_eq!(std::fs::read_to_string(t.path("a")).unwrap(), "immutable");
    }
    #[test]
    fn e4_08_import_stores_original_reference() {
        let t = T::new();
        std::fs::write(t.path("한 글.txt"), "x").unwrap();
        let p = t.path("한 글.txt").to_string_lossy().into_owned();
        let s = Store::open_at(":memory:").unwrap();
        s.import_paths("p", &[p.clone()], 1).unwrap();
        assert_eq!(s.list("p").unwrap()[0].source_ref, p);
    }
    #[test]
    fn e4_09_real_copy_creates_new_bytes() {
        let t = T::new();
        let src = t.path("src");
        let dst = t.path("dst");
        std::fs::create_dir(&src).unwrap();
        std::fs::create_dir(&dst).unwrap();
        std::fs::write(src.join("a"), "copy").unwrap();
        let s = Store::open_at(":memory:").unwrap();
        s.real_copy(
            "op",
            &[src.join("a").to_string_lossy().into()],
            &dst,
            "p",
            1,
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(dst.join("a")).unwrap(), "copy");
    }
    #[test]
    fn e4_10_real_copy_registers_virtual_result() {
        let t = T::new();
        let src = t.path("src");
        let dst = t.path("dst");
        std::fs::create_dir(&dst).unwrap();
        std::fs::write(&src, "a").unwrap();
        let s = Store::open_at(":memory:").unwrap();
        s.real_copy("op", &[src.to_string_lossy().into()], &dst, "p", 1)
            .unwrap();
        assert_eq!(s.list("p").unwrap().len(), 1);
    }
    #[test]
    fn e4_11_real_copy_collision_gets_number() {
        let t = T::new();
        let src = t.path("a.txt");
        let dst = t.path("dst");
        std::fs::create_dir(&dst).unwrap();
        std::fs::write(&src, "new").unwrap();
        std::fs::write(dst.join("a.txt"), "old").unwrap();
        let s = Store::open_at(":memory:").unwrap();
        s.real_copy("op", &[src.to_string_lossy().into()], &dst, "p", 1)
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(dst.join("a (2).txt")).unwrap(),
            "new"
        );
        assert_eq!(std::fs::read_to_string(dst.join("a.txt")).unwrap(), "old");
    }
    #[test]
    fn e4_12_real_copy_journals_each_result() {
        let t = T::new();
        let src = t.path("a");
        let dst = t.path("dst");
        std::fs::create_dir(&dst).unwrap();
        std::fs::write(&src, "a").unwrap();
        let s = Store::open_at(":memory:").unwrap();
        s.real_copy("op", &[src.to_string_lossy().into()], &dst, "p", 1)
            .unwrap();
        assert_eq!(s.real_copy_entries("op").unwrap().len(), 1);
    }
    #[test]
    fn e4_13_undo_real_copy_removes_unchanged_result() {
        let t = T::new();
        let src = t.path("a");
        let dst = t.path("dst");
        std::fs::create_dir(&dst).unwrap();
        std::fs::write(&src, "a").unwrap();
        let s = Store::open_at(":memory:").unwrap();
        s.real_copy("op", &[src.to_string_lossy().into()], &dst, "p", 1)
            .unwrap();
        s.undo_real_copy("op", false).unwrap();
        assert!(!dst.join("a").exists());
        assert!(s.list("p").unwrap().is_empty());
    }
    #[test]
    fn e4_14_undo_refuses_changed_result_without_confirmation() {
        let t = T::new();
        let src = t.path("a");
        let dst = t.path("dst");
        std::fs::create_dir(&dst).unwrap();
        std::fs::write(&src, "a").unwrap();
        let s = Store::open_at(":memory:").unwrap();
        s.real_copy("op", &[src.to_string_lossy().into()], &dst, "p", 1)
            .unwrap();
        std::fs::write(dst.join("a"), "changed content").unwrap();
        assert!(s.undo_real_copy("op", false).is_err());
        assert!(dst.join("a").exists());
    }
    #[test]
    fn e4_15_undo_can_remove_changed_result_after_explicit_confirmation() {
        let t = T::new();
        let src = t.path("a");
        let dst = t.path("dst");
        std::fs::create_dir(&dst).unwrap();
        std::fs::write(&src, "a").unwrap();
        let s = Store::open_at(":memory:").unwrap();
        s.real_copy("op", &[src.to_string_lossy().into()], &dst, "p", 1)
            .unwrap();
        std::fs::write(dst.join("a"), "changed").unwrap();
        s.undo_real_copy("op", true).unwrap();
        assert!(!dst.join("a").exists());
    }
    #[test]
    fn e4_16_real_copy_never_modifies_source() {
        let t = T::new();
        let src = t.path("a");
        let dst = t.path("dst");
        std::fs::create_dir(&dst).unwrap();
        std::fs::write(&src, "source").unwrap();
        let s = Store::open_at(":memory:").unwrap();
        s.real_copy("op", &[src.to_string_lossy().into()], &dst, "p", 1)
            .unwrap();
        assert_eq!(std::fs::read_to_string(src).unwrap(), "source");
    }
    #[test]
    fn e4_17_real_copy_handles_folder_tree() {
        let t = T::new();
        let src = t.path("folder");
        let dst = t.path("dst");
        std::fs::create_dir(&src).unwrap();
        std::fs::create_dir(&dst).unwrap();
        std::fs::write(src.join("child"), "child").unwrap();
        let s = Store::open_at(":memory:").unwrap();
        s.real_copy("op", &[src.to_string_lossy().into()], &dst, "p", 1)
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(dst.join("folder/child")).unwrap(),
            "child"
        );
    }
    #[test]
    fn e4_18_partial_failure_cleans_earlier_results() {
        let t = T::new();
        let src = t.path("a");
        let dst = t.path("dst");
        std::fs::create_dir(&dst).unwrap();
        std::fs::write(&src, "a").unwrap();
        let s = Store::open_at(":memory:").unwrap();
        assert!(
            s.real_copy(
                "op",
                &[
                    src.to_string_lossy().into(),
                    t.path("missing").to_string_lossy().into()
                ],
                &dst,
                "p",
                1
            )
            .is_err()
        );
        assert!(!dst.join("a").exists());
        assert!(s.list("p").unwrap().is_empty());
    }
    #[test]
    fn e4_19_confirmed_copy_requires_sources() {
        let t = T::new();
        let dst = t.path("dst");
        std::fs::create_dir(&dst).unwrap();
        let s = Store::open_at(":memory:").unwrap();
        assert!(s.real_copy("op", &[], &dst, "p", 1).is_err());
    }
    #[test]
    fn e4_20_import_empty_batch_is_noop() {
        let s = Store::open_at(":memory:").unwrap();
        assert!(s.import_paths("p", &[], 1).unwrap().added.is_empty());
    }
}

fn unique_name(
    conn: &Connection,
    location: &str,
    name: &str,
    except: Option<&str>,
) -> Result<String, String> {
    let exists = |candidate: &str| -> Result<bool, String> {
        let mut found = false;
        conn.query(
            "SELECT id FROM virtual_item WHERE location=? AND display_name=?",
            &[Arg::Text(location), Arg::Text(candidate)],
            |r| {
                if except != Some(r.text(0).as_str()) {
                    found = true;
                }
            },
        )?;
        Ok(found)
    };
    if !exists(name)? {
        return Ok(name.to_string());
    }
    let (stem, ext) = name
        .rsplit_once('.')
        .filter(|(s, _)| !s.is_empty())
        .map(|(s, e)| (s.to_string(), format!(".{e}")))
        .unwrap_or((name.to_string(), String::new()));
    for n in 2..=9999 {
        let candidate = format!("{stem} ({n}){ext}");
        if !exists(&candidate)? {
            return Ok(candidate);
        }
    }
    Err("겹치는 이름 번호를 정할 수 없습니다".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Temp(PathBuf);
    use std::path::PathBuf;
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "kvf-store-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn db(&self) -> PathBuf {
            self.0.join("virtual.db")
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn initializes_and_migrates_version_zero() {
        let s = Store::open_at(":memory:").unwrap();
        assert_eq!(s.version().unwrap(), 1);
    }
    #[test]
    fn reference_and_journal_roundtrip_and_undo() {
        let s = Store::open_at(":memory:").unwrap();
        s.add_reference("a", "one", "C:/문서/원본.txt", "원본.txt", 1)
            .unwrap();
        s.move_virtual("op1", "a", "two").unwrap();
        assert_eq!(s.list("two").unwrap()[0].source_ref, "C:/문서/원본.txt");
        assert_eq!(s.journal().unwrap()[0].operation_id, "op1");
        s.undo("op1").unwrap();
        assert_eq!(s.list("one").unwrap().len(), 1);
        assert_eq!(s.list("two").unwrap().len(), 0);
    }
    #[test]
    fn virtual_copy_copies_reference_without_touching_file() {
        let s = Store::open_at(":memory:").unwrap();
        s.add_reference("a", "one", "X:/원본/자료.txt", "자료.txt", 1)
            .unwrap();
        s.virtual_copy("op2", "a", "b", "two", "자료 (2).txt", 2)
            .unwrap();
        let copied = &s.list("two").unwrap()[0];
        assert_eq!(copied.source_ref, "X:/원본/자료.txt");
        s.undo("op2").unwrap();
        assert!(s.list("two").unwrap().is_empty());
    }
    #[test]
    fn missing_item_move_rolls_back() {
        let s = Store::open_at(":memory:").unwrap();
        assert!(s.move_virtual("bad", "none", "two").is_err());
        assert!(s.journal().unwrap().is_empty());
    }

    #[test]
    fn move_collision_adds_number_and_preserves_reference() {
        let s = Store::open_at(":memory:").unwrap();
        s.add_reference("a", "one", "C:/원본/자료.txt", "자료.txt", 1)
            .unwrap();
        s.add_reference("b", "two", "C:/다른/자료.txt", "자료.txt", 1)
            .unwrap();
        s.move_virtual("m", "a", "two").unwrap();
        let moved = s
            .list("two")
            .unwrap()
            .into_iter()
            .find(|item| item.id == "a")
            .unwrap();
        assert_eq!(moved.display_name, "자료 (2).txt");
        assert_eq!(moved.source_ref, "C:/원본/자료.txt");
    }
    #[test]
    fn copy_collision_adds_number() {
        let s = Store::open_at(":memory:").unwrap();
        s.add_reference("a", "one", "C:/자료.txt", "자료.txt", 1)
            .unwrap();
        s.add_reference("b", "two", "C:/기존.txt", "자료.txt", 1)
            .unwrap();
        s.virtual_copy("c", "a", "copy", "two", "", 2).unwrap();
        assert_eq!(
            s.list("two")
                .unwrap()
                .iter()
                .find(|i| i.id == "copy")
                .unwrap()
                .display_name,
            "자료 (2).txt"
        );
    }
    #[test]
    fn repeated_collision_uses_next_free_number() {
        let s = Store::open_at(":memory:").unwrap();
        for (id, name) in [("a", "자료.txt"), ("b", "자료.txt"), ("c", "자료 (2).txt")] {
            s.add_reference(id, "one", "C:/자료.txt", name, 1).unwrap();
        }
        s.move_virtual("m", "a", "two").unwrap();
        s.move_virtual("n", "b", "two").unwrap();
        assert_eq!(
            s.list("two")
                .unwrap()
                .iter()
                .map(|i| i.display_name.as_str())
                .collect::<Vec<_>>(),
            vec!["자료.txt", "자료 (2).txt"]
        );
    }
    #[test]
    fn multi_move_is_undone_as_one_operation() {
        let s = Store::open_at(":memory:").unwrap();
        for id in ["a", "b"] {
            s.add_reference(id, "one", "C:/자료.txt", id, 1).unwrap();
        }
        s.move_many("batch", &["a".into(), "b".into()], "two")
            .unwrap();
        s.undo("batch").unwrap();
        assert_eq!(s.list("one").unwrap().len(), 2);
        assert!(s.list("two").unwrap().is_empty());
    }
    #[test]
    fn multi_copy_is_undone_as_one_operation() {
        let s = Store::open_at(":memory:").unwrap();
        for id in ["a", "b"] {
            s.add_reference(id, "one", "C:/자료.txt", id, 1).unwrap();
        }
        s.copy_many("batch", &["a".into(), "b".into()], "two", 2)
            .unwrap();
        assert_eq!(s.list("two").unwrap().len(), 2);
        s.undo("batch").unwrap();
        assert!(s.list("two").unwrap().is_empty());
    }
    #[test]
    fn move_redo_restores_location() {
        let s = Store::open_at(":memory:").unwrap();
        s.add_reference("a", "one", "C:/a", "a", 1).unwrap();
        s.move_many("m", &["a".into()], "two").unwrap();
        s.undo("m").unwrap();
        s.redo("m").unwrap();
        assert_eq!(s.list("two").unwrap().len(), 1);
    }
    #[test]
    fn undo_collision_move_restores_original_display_name() {
        let s = Store::open_at(":memory:").unwrap();
        s.add_reference("a", "one", "C:/a", "a", 1).unwrap();
        s.add_reference("b", "two", "C:/b", "a", 1).unwrap();
        s.move_virtual("m", "a", "two").unwrap();
        assert_eq!(
            s.list("two")
                .unwrap()
                .iter()
                .find(|item| item.id == "a")
                .unwrap()
                .display_name,
            "a (2)"
        );
        s.undo("m").unwrap();
        assert_eq!(s.list("one").unwrap()[0].display_name, "a");
    }
    #[test]
    fn copy_redo_restores_virtual_record() {
        let s = Store::open_at(":memory:").unwrap();
        s.add_reference("a", "one", "C:/a", "a", 1).unwrap();
        s.copy_many("c", &["a".into()], "two", 2).unwrap();
        s.undo("c").unwrap();
        s.redo("c").unwrap();
        assert_eq!(s.list("two").unwrap()[0].source_ref, "C:/a");
    }
    #[test]
    fn journal_survives_reopen_and_restores_undo_state() {
        let t = Temp::new();
        {
            let s = Store::open(&t.db()).unwrap();
            s.add_reference("a", "one", "C:/a", "a", 1).unwrap();
            s.move_virtual("m", "a", "two").unwrap();
        }
        let s = Store::open(&t.db()).unwrap();
        assert_eq!(s.journal().unwrap()[0].operation_id, "m");
        s.undo("m").unwrap();
    }
    #[test]
    fn failed_multi_operation_rolls_back_every_item() {
        let s = Store::open_at(":memory:").unwrap();
        s.add_reference("a", "one", "C:/a", "a", 1).unwrap();
        assert!(
            s.move_many("m", &["a".into(), "missing".into()], "two")
                .is_err()
        );
        assert_eq!(s.list("one").unwrap().len(), 1);
        assert!(s.list("two").unwrap().is_empty());
        assert!(s.journal().unwrap().is_empty());
    }
    #[test]
    fn same_location_move_is_rejected_without_journal() {
        let s = Store::open_at(":memory:").unwrap();
        s.add_reference("a", "one", "C:/a", "a", 1).unwrap();
        assert!(s.move_virtual("m", "a", "one").is_err());
        assert!(s.journal().unwrap().is_empty());
    }
    #[test]
    fn empty_batch_is_rejected() {
        let s = Store::open_at(":memory:").unwrap();
        assert!(s.move_many("m", &[], "two").is_err());
        assert!(s.copy_many("c", &[], "two", 1).is_err());
    }
    #[test]
    fn undo_then_redo_state_is_recorded() {
        let s = Store::open_at(":memory:").unwrap();
        s.add_reference("a", "one", "C:/a", "a", 1).unwrap();
        s.move_many("m", &["a".into()], "two").unwrap();
        s.undo("m").unwrap();
        s.redo("m").unwrap();
        assert!(!s.journal().unwrap()[0].undone);
    }
}
