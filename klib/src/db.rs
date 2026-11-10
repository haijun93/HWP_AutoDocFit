//! SQLite(rusqlite bundled) 기반 가상 관계형 자료함 DB 모듈.

use std::fs;
use std::path::{Path, PathBuf};
use rusqlite::{params, Connection, OptionalExtension, Result};

use crate::util::{data_dir, is_or_has_reparse_point, now, now_ms, sanitize_filename};


pub struct BoxRow {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub created_at: f64,
    pub updated_at: f64,
    pub export_path: Option<String>,
    pub item_count: i64,
}

pub struct ItemRow {
    pub id: i64,
    pub box_id: i64,
    pub kind: String,
    pub source_path: String,
    pub origin_path: String,
    pub managed_path: String,
    pub display_name: String,
    pub space: String,
    pub x: f64,
    pub y: f64,
    pub created_at: f64,
}

pub struct RelationRow {
    pub id: i64,
    pub box_id: i64,
    pub from_item: i64,
    pub to_item: i64,
    pub label: String,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub(crate) fn connection(&self) -> &Connection { &self.conn }

    pub fn notebook_create(&self,name:&str,description:&str)->Result<i64>{crate::notebook::create(&self.conn,name,description)}
    pub fn notebook_update(&self,id:i64,name:&str,description:&str)->Result<bool>{crate::notebook::update(&self.conn,id,name,description)}
    pub fn notebook_delete(&self,id:i64)->Result<bool>{crate::notebook::delete(&self.conn,id)}
    pub fn notebook_sources(&self,id:i64)->Result<Vec<crate::notebook::Source>>{crate::notebook::sources(&self.conn,id)}
    pub fn notebook_source_paragraphs(&self,notebook_id:i64,source_id:i64)->Result<Vec<crate::notebook::Paragraph>>{crate::notebook::source_paragraphs(&self.conn,notebook_id,source_id)}
    pub fn notebook_notes(&self,notebook_id:i64)->Result<Vec<(i64,String,String,i64)>>{let mut s=self.conn.prepare("SELECT id,title,body,updated_at FROM nb_note WHERE notebook_id=?1 ORDER BY updated_at DESC,id DESC")?;s.query_map([notebook_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?.collect()}
}

impl Database {
    pub fn open(db_path: &str) -> Result<Self> {
        if let Some(parent) = Path::new(db_path).parent() {
            let _ = fs::create_dir_all(parent);
        }
        let conn = Connection::open(db_path)?;
        let db = Database { conn };
        db.init_schema()?;
        crate::notebook::init_schema(&db.conn)?;
        crate::scan::init_schema(&db.conn)?;
        Ok(db)
    }

    pub fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;

             CREATE TABLE IF NOT EXISTS box (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 name TEXT NOT NULL,
                 description TEXT NOT NULL DEFAULT '',
                 created_at REAL NOT NULL,
                 updated_at REAL NOT NULL,
                 export_path TEXT
             );

             CREATE TABLE IF NOT EXISTS item (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 box_id INTEGER NOT NULL REFERENCES box(id) ON DELETE CASCADE,
                 kind TEXT NOT NULL,
                 source_path TEXT NOT NULL DEFAULT '',
                 display_name TEXT NOT NULL,
                 space TEXT NOT NULL DEFAULT 'temp',
                 x REAL NOT NULL DEFAULT 0.0,
                 y REAL NOT NULL DEFAULT 0.0,
                 created_at REAL NOT NULL,
                 UNIQUE (box_id, id)
             );

             CREATE TABLE IF NOT EXISTS relation (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 box_id INTEGER NOT NULL REFERENCES box(id) ON DELETE CASCADE,
                 from_item INTEGER NOT NULL,
                 to_item INTEGER NOT NULL,
                 label TEXT NOT NULL DEFAULT '',
                 FOREIGN KEY (box_id, from_item) REFERENCES item(box_id, id) ON DELETE CASCADE,
                 FOREIGN KEY (box_id, to_item) REFERENCES item(box_id, id) ON DELETE CASCADE
             );

             CREATE INDEX IF NOT EXISTS idx_item_box_space ON item(box_id, space);
             CREATE INDEX IF NOT EXISTS idx_item_display_name ON item(display_name);
             CREATE INDEX IF NOT EXISTS idx_relation_box ON relation(box_id);
             CREATE INDEX IF NOT EXISTS idx_relation_endpoints ON relation(from_item, to_item);",
        )?;
        // 원본 경로만 있던 DB는 스키마만 확장한다. 실제 사본 이전은 최초 사용 때 수행한다.
        let mut stmt = self.conn.prepare("PRAGMA table_info(item)")?;
        let names = stmt.query_map([], |r| r.get::<_, String>(1))?.collect::<Result<Vec<_>>>()?;
        for name in ["origin_path", "managed_path"] {
            if !names.iter().any(|n| n == name) {
                self.conn.execute(&format!("ALTER TABLE item ADD COLUMN {name} TEXT NOT NULL DEFAULT ''"), [])?;
            }
        }
        Ok(())
    }

    // --- Box CRUD ---

    pub fn box_create(&self, name: &str, description: &str) -> Result<i64> {
        let t = now();
        self.conn.execute(
            "INSERT INTO box (name, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![name, description, t, t],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn box_list(&self) -> Result<Vec<BoxRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT b.id, b.name, b.description, b.created_at, b.updated_at, b.export_path,
                    (SELECT COUNT(*) FROM item WHERE box_id = b.id) AS item_count
             FROM box b ORDER BY b.updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(BoxRow {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                export_path: row.get(5)?,
                item_count: row.get(6)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn box_get(&self, id: i64) -> Result<Option<(BoxRow, Vec<ItemRow>, Vec<RelationRow>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, created_at, updated_at, export_path,
                    (SELECT COUNT(*) FROM item WHERE box_id = ?1)
             FROM box WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let b = BoxRow {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
            export_path: row.get(5)?,
            item_count: row.get(6)?,
        };

        // Items
        let mut item_stmt = self.conn.prepare(
            "SELECT id, box_id, kind, source_path, display_name, space, x, y, created_at, origin_path, managed_path
             FROM item WHERE box_id = ?1 ORDER BY id ASC",
        )?;
        let item_rows = item_stmt.query_map(params![id], |r| {
            Ok(ItemRow {
                id: r.get(0)?,
                box_id: r.get(1)?,
                kind: r.get(2)?,
                source_path: r.get(3)?,
                display_name: r.get(4)?,
                space: r.get(5)?,
                x: r.get(6)?,
                y: r.get(7)?,
                created_at: r.get(8)?,
                origin_path: r.get(9)?,
                managed_path: r.get(10)?,
            })
        })?;
        let mut items = Vec::new();
        for it in item_rows {
            items.push(it?);
        }

        // Relations
        let mut rel_stmt = self.conn.prepare(
            "SELECT id, box_id, from_item, to_item, label
             FROM relation WHERE box_id = ?1 ORDER BY id ASC",
        )?;
        let rel_rows = rel_stmt.query_map(params![id], |r| {
            Ok(RelationRow {
                id: r.get(0)?,
                box_id: r.get(1)?,
                from_item: r.get(2)?,
                to_item: r.get(3)?,
                label: r.get(4)?,
            })
        })?;
        let mut relations = Vec::new();
        for rl in rel_rows {
            relations.push(rl?);
        }

        Ok(Some((b, items, relations)))
    }

    pub fn box_rename(&self, id: i64, name: &str) -> Result<bool> {
        let affected = self.conn.execute("UPDATE box SET name = ?1, updated_at = ?2 WHERE id = ?3", params![name, now(), id])?;
        Ok(affected > 0)
    }

    pub fn box_update(&self, id: i64, name: &str, description: &str) -> Result<bool> {
        let t = now();
        let affected = self.conn.execute(
            "UPDATE box SET name = ?1, description = ?2, updated_at = ?3 WHERE id = ?4",
            params![name, description, t, id],
        )?;
        Ok(affected > 0)
    }

    pub fn box_delete(&self, id: i64) -> Result<bool> {
        let affected = self.conn.execute("DELETE FROM box WHERE id = ?1", params![id])?;
        if affected > 0 {
            // 사본 데이터 폴더 정리
            let box_dir = PathBuf::from(data_dir()).join("boxes").join(id.to_string());
            if !is_or_has_reparse_point(&box_dir) { let _ = fs::remove_dir_all(box_dir); }
            let managed = library_root().join(id.to_string());
            if !is_or_has_reparse_point(&managed) { let _ = fs::remove_dir_all(managed); }
        }
        Ok(affected > 0)
    }

    // --- Item CRUD & Operations ---

    fn area_dir(&self, box_id: i64, space: &str) -> std::result::Result<PathBuf, String> {
        if box_id <= 0 || !matches!(space, "core" | "basis" | "reference" | "temp") {
            return Err("잘못된 자료함 영역입니다.".into());
        }
        let dir = library_root().join(box_id.to_string()).join(space);
        if is_or_has_reparse_point(&dir) { return Err("자료함 저장 경로에 링크나 정션이 있습니다.".into()); }
        fs::create_dir_all(&dir).map_err(|e| format!("자료함 영역 생성 실패: {e}"))?;
        Ok(dir)
    }

    fn create_copy(&self, box_id: i64, space: &str, kind: &str, source: &str, name: &str, content: Option<&str>) -> std::result::Result<PathBuf, String> {
        let src = Path::new(source);
        if content.is_none() && src.is_dir() && normalized_path(&library_root().join(box_id.to_string()).join(space))?.starts_with(normalized_path(src)?) {
            return Err("자료함 저장 폴더를 포함하는 원본 폴더는 추가할 수 없습니다.".into());
        }
        let dir = self.area_dir(box_id, space)?;
        let mut filename = if matches!(kind, "file" | "folder") || (content.is_none() && src.is_file()) {
            sanitize_filename(src.file_name().and_then(|n| n.to_str()).unwrap_or(name))
        } else {
            let ext = match kind { "rich_text" => "json", "image" | "capture" => "png", _ => "txt" };
            let clean = sanitize_filename(name);
            if clean.to_ascii_lowercase().ends_with(&format!(".{ext}")) { clean } else { format!("{clean}.{ext}") }
        };
        if filename.is_empty() { filename = "자료.txt".into(); }
        let target = unique_path(&dir, &filename);
        let is_directory = content.is_none() && src.is_dir();
        // create_new/create_dir 성공 후에만 이 작업이 생성한 대상으로 취급한다.
        // 충돌 경합에서 다른 프로그램이 만든 파일을 덮어쓰거나 실패 정리로 지우지 않는다.
        let mut output = if is_directory {
            fs::create_dir(&target).map_err(|e| format!("사본 폴더 예약 실패: {e}"))?;
            None
        } else {
            Some(fs::OpenOptions::new().write(true).create_new(true).open(&target).map_err(|e| format!("사본 파일 예약 실패: {e}"))?)
        };
        let result: std::io::Result<()> = (|| {
            use std::io::Write;
            if content.is_none() && src.exists() {
                if is_or_has_reparse_point(src) { return Err(std::io::Error::other("심볼릭 링크나 정션은 복사할 수 없습니다.")); }
                if src.is_dir() {
                    if dir.canonicalize()?.starts_with(src.canonicalize()?) { return Err(std::io::Error::other("자료함 저장 폴더를 자기 안으로 복사할 수 없습니다.")); }
                    copy_dir_recursive(src, &target)?;
                } else { copy_to_open_file(src, output.as_mut().unwrap())?; }
            } else if matches!(kind, "text" | "rich_text" | "note") {
                output.as_mut().unwrap().write_all(content.unwrap_or(source).as_bytes())?;
            } else if matches!(kind, "image" | "capture") && content.is_some() {
                let bytes = crate::util::base64_decode(content.unwrap()).ok_or_else(|| std::io::Error::other("Base64 디코딩 실패"))?;
                output.as_mut().unwrap().write_all(&bytes)?;
            } else { return Err(std::io::Error::other("원본 파일을 찾을 수 없습니다.")); }
            if let Some(file) = output.as_mut() { file.sync_all()?; }
            Ok(())
        })();
        drop(output);
        if let Err(e) = result { remove_copy(&target); return Err(format!("자료 사본 생성 실패: {e}")); }
        Ok(target)
    }

    fn validate_copy(&self, it: &ItemRow) -> std::result::Result<PathBuf, String> {
        let path = self.validate_copy_location(it)?;
        if !path.exists() { return Err("자료함 복사본을 찾을 수 없습니다.".into()); }
        Ok(path)
    }

    fn validate_copy_location(&self, it: &ItemRow) -> std::result::Result<PathBuf, String> {
        if it.box_id <= 0 || !matches!(it.space.as_str(), "core" | "basis" | "reference" | "temp") {
            return Err("자료함 데이터 폴더 밖의 잘못된 영역입니다.".into());
        }
        let path = PathBuf::from(&it.managed_path);
        let base = library_root().join(it.box_id.to_string()).join(&it.space);
        if path.components().any(|part| matches!(part, std::path::Component::ParentDir)) || it.source_path != it.managed_path || it.managed_path.is_empty() || is_or_has_reparse_point(&path) || is_or_has_reparse_point(&base) {
            return Err("자료함 데이터 폴더 밖 또는 유효하지 않은 복사본 경로입니다.".into());
        }
        let inside = match (base.canonicalize(), path.parent().map(Path::canonicalize)) {
            (Ok(b), Some(Ok(p))) => b == p,
            _ => false,
        };
        if !inside { return Err("자료함 복사본이 없거나 영역 밖의 경로입니다.".into()); }
        Ok(path)
    }

    /// 열기·편집·노트북 사용 직전에만 기존 항목을 이전한다. 실패하면 원본 경로로 대체하지 않는다.
    pub fn item_prepare(&self, id: i64) -> std::result::Result<ItemRow, String> {
        let mut it = self.item_get(id).map_err(|e| e.to_string())?.ok_or("해당 자료 항목을 찾을 수 없습니다.")?;
        if it.kind == "link" { return Ok(it); }
        if !it.managed_path.is_empty() { self.validate_copy(&it)?; return Ok(it); }
        let old = it.source_path.clone();
        // 최초 이전 전 원래 경로를 별도 테이블에 보관한다. 사본 생성/DB 갱신 실패 시 행은 그대로다.
        self.conn.execute_batch("CREATE TABLE IF NOT EXISTS item_path_backup (item_id INTEGER PRIMARY KEY, source_path TEXT NOT NULL, created_at REAL NOT NULL)").map_err(|e| e.to_string())?;
        self.conn.execute("INSERT OR IGNORE INTO item_path_backup VALUES (?1, ?2, ?3)", params![id, old, now()]).map_err(|e| e.to_string())?;
        if !Path::new(&old).exists() && (Path::new(&old).is_absolute() || !matches!(it.kind.as_str(), "text" | "rich_text" | "note")) { return Err("원본 파일을 찾을 수 없습니다.".into()); }
        let path = self.create_copy(it.box_id, &it.space, &it.kind, &old, &it.display_name, None)?;
        let managed = path.to_string_lossy().into_owned();
        let origin = if Path::new(&old).exists() { old } else { String::new() };
        let updated: Result<usize> = (|| {
            let tx = self.conn.unchecked_transaction()?;
            let affected = tx.execute("UPDATE item SET source_path=?1, managed_path=?1, origin_path=?2 WHERE id=?3 AND managed_path=''", params![managed, origin, id])?;
            if affected == 1 { tx.execute("UPDATE nb_source SET origin=?1 WHERE item_id=?2", params![managed, id])?; }
            tx.commit()?;
            Ok(affected)
        })();
        match updated {
            Ok(1) => {},
            Ok(_) => { remove_copy(&path); return self.item_prepare(id); },
            Err(e) => { remove_copy(&path); return Err(e.to_string()); }
        }
        it.source_path = managed.clone(); it.managed_path = managed; it.origin_path = origin;
        Ok(it)
    }

    pub fn item_add(
        &self,
        box_id: i64,
        kind: &str,
        source_path: &str,
        display_name: &str,
        space: &str,
        x: f64,
        y: f64,
        content: Option<&str>,
    ) -> std::result::Result<i64, String> {
        // 1. 종류 및 공간 유효성 검증
        match kind {
            "file" | "folder" | "link" | "text" | "rich_text" | "image" | "capture" | "note" => {}
            _ => return Err(format!("지원하지 않는 자료 종류입니다: {kind}")),
        }
        match space {
            "core" | "basis" | "reference" | "temp" => {}
            _ => return Err(format!("유효하지 않은 공간입니다: {space}")),
        }

        // 2. 대상 box 존재 여부 DB 확인
        let box_exists: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM box WHERE id = ?1)",
            params![box_id],
            |r| r.get(0),
        ).map_err(|e| e.to_string())?;
        if !box_exists {
            return Err(format!("자료함(box_id={box_id})이 존재하지 않습니다."));
        }

        // 3. 심볼릭 링크/정션(상위 경로 포함) 및 파일/폴더 종류·존재 여부 검증 (지적 1 반영)
        if !source_path.is_empty() {
            let p = Path::new(source_path);
            if is_or_has_reparse_point(p) {
                return Err("심볼릭 링크나 정션(또는 그 하위 경로)은 추가할 수 없습니다.".into());
            }
            if kind == "file" && !p.is_file() {
                return Err(format!("원본 파일을 찾을 수 없습니다: {source_path}"));
            }
            if kind == "folder" && !p.is_dir() {
                return Err(format!("원본 폴더를 찾을 수 없습니다: {source_path}"));
            }
        } else if matches!(kind, "file" | "folder") {
            return Err("파일 또는 폴더 경로는 비어있을 수 없습니다.".into());
        }

        let mut actual_source = source_path.to_string();
        let mut created_file_to_cleanup: Option<PathBuf> = None;
        let origin = if kind != "link" && !source_path.is_empty() && Path::new(source_path).exists() {
            // 다른 상자의 사본을 복제해도 최초 출처 기록을 잇는다(외부 입력 경로는 신뢰하지 않는다).
            self.conn.query_row("SELECT origin_path FROM item WHERE managed_path=?1 AND managed_path<>'' LIMIT 1", params![source_path], |r| r.get::<_, String>(0))
                .optional().map_err(|e| e.to_string())?.unwrap_or_else(|| source_path.to_string())
        } else { String::new() };
        if kind != "link" {
            let target = self.create_copy(box_id, space, kind, source_path, display_name, content)?;
            actual_source = target.to_string_lossy().into_owned();
            created_file_to_cleanup = Some(target);
        }
        let managed = if kind == "link" { "" } else { actual_source.as_str() };

        // 5. DB 행 삽입 및 실패 시 생성된 사본 파일 롤백/정리
        let t = now();
        let insert_res = self.conn.execute(
            "INSERT INTO item (box_id, kind, source_path, display_name, space, x, y, created_at, origin_path, managed_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![box_id, kind, actual_source, display_name, space, x, y, t, origin, managed],
        );

        match insert_res {
            Ok(_) => {
                let item_id = self.conn.last_insert_rowid();
                let _ = self.conn.execute("UPDATE box SET updated_at = ?1 WHERE id = ?2", params![t, box_id]);
                Ok(item_id)
            }
            Err(e) => {
                if let Some(created_path) = created_file_to_cleanup {
                    remove_copy(&created_path);
                }
                Err(format!("아이템 DB 등록 실패: {e}"))
            }
        }
    }

    pub fn item_get(&self, id: i64) -> Result<Option<ItemRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, box_id, kind, source_path, display_name, space, x, y, created_at, origin_path, managed_path
             FROM item WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(r) = rows.next()? {
            Ok(Some(ItemRow {
                id: r.get(0)?,
                box_id: r.get(1)?,
                kind: r.get(2)?,
                source_path: r.get(3)?,
                display_name: r.get(4)?,
                space: r.get(5)?,
                x: r.get(6)?,
                y: r.get(7)?,
                created_at: r.get(8)?,
                origin_path: r.get(9)?,
                managed_path: r.get(10)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn item_update(&self, id: i64, display_name: &str) -> Result<bool> {
        let affected = self.conn.execute(
            "UPDATE item SET display_name = ?1 WHERE id = ?2",
            params![display_name, id],
        )?;
        Ok(affected > 0)
    }

    /// 글 자료(text·note)의 klib 사본 파일. 자료함 영역(<box_id>/<space>) 바로 아래 .txt만 허용한다.
    /// 화면의 메모 편집기가 고치는 대상이 사용자 원본이나 임의 경로가 되지 않게 막는다.
    fn item_text_copy(&self, id: i64) -> std::result::Result<(ItemRow, PathBuf), String> {
        let it = self.item_prepare(id)?;
        if !matches!(it.kind.as_str(), "text" | "note") {
            return Err("글 자료(text·note)만 메모 편집기로 열 수 있습니다.".into());
        }
        let path = PathBuf::from(&it.source_path);
        let is_txt = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("txt"));
        if !is_txt || is_or_has_reparse_point(&path) || !path.is_file() {
            return Err("글 자료의 사본 파일을 찾을 수 없습니다.".into());
        }
        self.validate_copy(&it)?;
        Ok((it, path))
    }

    /// 글 자료 내용 읽기(메모 편집기로 열기).
    pub fn item_text_get(&self, id: i64) -> std::result::Result<(ItemRow, String), String> {
        let (it, path) = self.item_text_copy(id)?;
        let bytes = fs::read(&path).map_err(|e| format!("글 사본 읽기 실패: {e}"))?;
        Ok((it, String::from_utf8_lossy(&bytes).into_owned()))
    }

    /// 글 자료 내용(과 이름) 고치기. 임시 파일에 쓴 뒤 바꿔치기해 중간에 실패해도 사본이 반쯤 쓰이지 않게 한다.
    /// 돌려주는 값은 다시 색인할 때 쓰는 고친 뒤의 항목.
    pub fn item_text_set(&self, id: i64, content: &str, display_name: Option<&str>) -> std::result::Result<ItemRow, String> {
        let (mut it, path) = self.item_text_copy(id)?;
        let tmp = path.with_extension(format!("txt.{}.tmp", now_ms()));
        {
            use std::io::Write;
            let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&tmp).map_err(|e| format!("글 사본 임시 파일 생성 실패: {e}"))?;
            if let Err(e) = file.write_all(content.as_bytes()).and_then(|_| file.sync_all()) {
                drop(file); let _ = fs::remove_file(&tmp); return Err(format!("글 사본 쓰기 실패: {e}"));
            }
        }
        if let Err(e) = fs::rename(&tmp, &path) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("글 사본 바꾸기 실패: {e}"));
        }
        let t = now();
        if let Some(name) = display_name.map(str::trim).filter(|n| !n.is_empty()) {
            self.conn.execute("UPDATE item SET display_name = ?1 WHERE id = ?2", params![name, id])
                .map_err(|e| e.to_string())?;
            it.display_name = name.to_string();
        }
        let _ = self.conn.execute("UPDATE box SET updated_at = ?1 WHERE id = ?2", params![t, it.box_id]);
        Ok(it)
    }

    pub fn item_move(&self, id: i64, space: &str, x: f64, y: f64) -> std::result::Result<bool, String> {
        match space {
            "core" | "basis" | "reference" | "temp" => {}
            _ => return Err(format!("유효하지 않은 공간입니다: {space}")),
        }
        if self.item_get(id).map_err(|e| e.to_string())?.is_none() { return Ok(false); }
        let it = self.item_prepare(id)?;
        let mut moved = None;
        if it.kind != "link" && it.space != space {
            let old = self.validate_copy(&it)?;
            let dir = self.area_dir(it.box_id, space)?;
            let new = unique_path(&dir, old.file_name().and_then(|n| n.to_str()).unwrap_or("자료"));
            fs::rename(&old, &new).map_err(|e| format!("영역 사본 이동 실패: {e}"))?;
            moved = Some((old, new));
        }
        let managed = moved.as_ref().map(|(_, new)| new.to_string_lossy().into_owned()).unwrap_or(it.source_path);
        let result: Result<usize> = (|| {
            let tx = self.conn.unchecked_transaction()?;
            let affected = tx.execute(
                "UPDATE item SET space = ?1, x = ?2, y = ?3, source_path=?5, managed_path=CASE WHEN kind='link' THEN '' ELSE ?5 END WHERE id = ?4",
                params![space, x, y, id, managed],
            )?;
            tx.execute("UPDATE nb_source SET origin=?1 WHERE item_id=?2", params![managed, id])?;
            tx.commit()?;
            Ok(affected)
        })();
        if result.is_err() { if let Some((old, new)) = moved { let _ = fs::rename(new, old); } }
        let affected = result.map_err(|e| e.to_string())?;
        Ok(affected > 0)
    }

    pub fn item_delete(&self, id: i64) -> Result<bool> {
        let it = self.item_get(id)?;
        // 등록된 사본만 정리한다. 이전 전 항목의 원본 파일에는 손대지 않는다.
        if let Some(it) = it.as_ref().filter(|it| !it.managed_path.is_empty()) {
            let path = self.validate_copy_location(it).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(e))))?;
            if path.exists() {
                if path.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) }
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            }
        }
        let affected = self.conn.execute("DELETE FROM item WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    pub fn item_delete_source(&self, id: i64) -> Result<Option<String>> {
        let source = self.conn.query_row("SELECT NULLIF(managed_path, '') FROM item WHERE id=?1", params![id], |r| r.get::<_, Option<String>>(0)).optional()?.flatten();
        self.item_delete(id)?;
        Ok(source)
    }

    pub fn item_search(&self, box_id: Option<i64>, query: &str) -> Result<Vec<ItemRow>> {
        let pattern = format!("%{query}%");
        let (sql, params_vec): (&str, Vec<rusqlite::types::Value>) = match box_id {
            Some(bid) => (
                "SELECT id, box_id, kind, source_path, display_name, space, x, y, created_at, origin_path, managed_path
                 FROM item
                 WHERE box_id = ?1 AND (display_name LIKE ?2 COLLATE NOCASE OR source_path LIKE ?2 COLLATE NOCASE OR origin_path LIKE ?2 COLLATE NOCASE)
                 ORDER BY id DESC",
                vec![bid.into(), pattern.into()],
            ),
            None => (
                "SELECT id, box_id, kind, source_path, display_name, space, x, y, created_at, origin_path, managed_path
                 FROM item
                 WHERE display_name LIKE ?1 COLLATE NOCASE OR source_path LIKE ?1 COLLATE NOCASE OR origin_path LIKE ?1 COLLATE NOCASE
                 ORDER BY id DESC",
                vec![pattern.into()],
            ),
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params_vec), |r| {
            Ok(ItemRow {
                id: r.get(0)?,
                box_id: r.get(1)?,
                kind: r.get(2)?,
                source_path: r.get(3)?,
                display_name: r.get(4)?,
                space: r.get(5)?,
                x: r.get(6)?,
                y: r.get(7)?,
                created_at: r.get(8)?,
                origin_path: r.get(9)?,
                managed_path: r.get(10)?,
            })
        })?;
        let mut items = Vec::new();
        for it in rows {
            items.push(it?);
        }
        Ok(items)
    }

    pub fn item_rename(&self, id: i64, name: &str) -> Result<bool> {
        Ok(self.conn.execute("UPDATE item SET display_name=?1 WHERE id=?2", params![name,id])? > 0)
    }

    pub fn relation_update(&self, id: i64, label: &str) -> Result<bool> {
        Ok(self.conn.execute("UPDATE relation SET label = ?1 WHERE id = ?2", params![label, id])? > 0)
    }

    // --- Relation CRUD ---

    pub fn relation_add(&self, box_id: i64, from_item: i64, to_item: i64, label: &str) -> std::result::Result<i64, String> {
        // 1. 자기 자신에 대한 관계선 금지
        if from_item == to_item {
            return Err("자기 자신에 대한 관계선은 허용되지 않습니다.".into());
        }

        // 2. 대상 box 존재 여부 DB 확인 (지적 2 반영)
        let box_exists: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM box WHERE id = ?1)",
            params![box_id],
            |r| r.get(0),
        ).map_err(|e| e.to_string())?;
        if !box_exists {
            return Err(format!("자료함(box_id={box_id})이 존재하지 않습니다."));
        }

        // 3. 양 끝 항목이 모두 지정된 동일 box_id에 존재하는지 검증 (지적 2 반영)
        let from_box: Option<i64> = self.conn.query_row(
            "SELECT box_id FROM item WHERE id = ?1",
            params![from_item],
            |r| r.get(0),
        ).optional().map_err(|e| e.to_string())?;

        let to_box: Option<i64> = self.conn.query_row(
            "SELECT box_id FROM item WHERE id = ?1",
            params![to_item],
            |r| r.get(0),
        ).optional().map_err(|e| e.to_string())?;

        match (from_box, to_box) {
            (Some(b1), Some(b2)) if b1 == box_id && b2 == box_id => {}
            _ => return Err("지정된 항목이 해당 자료함에 존재하지 않습니다.".into()),
        }

        self.conn.execute(
            "INSERT INTO relation (box_id, from_item, to_item, label) VALUES (?1, ?2, ?3, ?4)",
            params![box_id, from_item, to_item, label],
        ).map_err(|e| e.to_string())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn relation_delete(&self, id: i64) -> Result<bool> {
        let affected = self.conn.execute("DELETE FROM relation WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    pub fn relation_list(&self, box_id: i64) -> Result<Vec<RelationRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, box_id, from_item, to_item, label FROM relation WHERE box_id = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![box_id], |r| {
            Ok(RelationRow {
                id: r.get(0)?,
                box_id: r.get(1)?,
                from_item: r.get(2)?,
                to_item: r.get(3)?,
                label: r.get(4)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    // --- 물리 폴더 전환 (Export) ---

    pub fn box_export(&self, box_id: i64, target_dir: &str) -> std::result::Result<usize, String> {
        // 1. 자료함 존재 확인
        let box_exists: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM box WHERE id = ?1)",
            params![box_id],
            |r| r.get(0),
        ).map_err(|e| e.to_string())?;
        if !box_exists {
            return Err(format!("자료함(box_id={box_id})이 존재하지 않습니다."));
        }

        // 2. 대상 폴더 검증 (지적 3 반영: 이미 존재하는 폴더인 경우 빈 폴더인지 검증)
        let target = Path::new(target_dir);
        if is_or_has_reparse_point(target) { return Err("내보내기 대상에 링크나 정션이 있습니다.".into()); }
        let absolute_target = if target.is_absolute() { target.to_path_buf() } else { std::env::current_dir().map_err(|e| e.to_string())?.join(target) };
        let normalized_target = normalized_path(&absolute_target)?;
        // 다른 상자의 저장 폴더도 제외한다. 물리 전환 결과가 상자 삭제 때 함께 지워지면 안 된다.
        for b in self.box_list().map_err(|e| e.to_string())? {
            for root in [library_root().join(b.id.to_string()), library_root().join("boxes").join(b.id.to_string())] {
                let managed_root = normalized_path(&root)?;
                if normalized_target.starts_with(&managed_root) || managed_root.starts_with(&normalized_target) {
                    return Err("자료함 저장 폴더 안이나 상위 폴더로는 내보낼 수 없습니다.".into());
                }
            }
        }
        for it in self.item_search(Some(box_id), "").map_err(|e| e.to_string())? {
            if it.kind == "folder" {
                for source in [&it.source_path, &it.origin_path, &it.managed_path] {
                    if !source.is_empty() && normalized_target.starts_with(normalized_path(Path::new(source))?) {
                        return Err("자료 원본이나 복사본 폴더 안으로는 내보낼 수 없습니다.".into());
                    }
                }
            }
        }
        let target_existed = target.exists();
        if target_existed {
            if !target.is_dir() {
                return Err(format!("내보내기 대상 경로가 디렉터리가 아닙니다: {target_dir}"));
            }
            let mut entries = fs::read_dir(target).map_err(|e| format!("대상 폴더 읽기 실패: {e}"))?;
            if entries.next().is_some() {
                return Err(format!("내보내기 대상 폴더가 비어있지 않습니다: {target_dir}"));
            }
        } else {
            fs::create_dir_all(target).map_err(|e| format!("대상 폴더 생성 실패: {e}"))?;
        }

        // 공간별 4개 하위 폴더 생성
        let core_dir = target.join("1_핵심자료");
        let basis_dir = target.join("2_기초자료");
        let ref_dir = target.join("3_참고자료");
        let temp_dir = target.join("4_임시저장소");

        let cleanup_export = || {
            if !target_existed {
                let _ = fs::remove_dir_all(target);
            } else {
                let _ = fs::remove_dir_all(&core_dir);
                let _ = fs::remove_dir_all(&basis_dir);
                let _ = fs::remove_dir_all(&ref_dir);
                let _ = fs::remove_dir_all(&temp_dir);
            }
        };

        for dir in [&core_dir, &basis_dir, &ref_dir, &temp_dir] {
            if let Err(e) = fs::create_dir_all(dir) {
                cleanup_export();
                return Err(format!("하위 폴더 생성 실패: {e}"));
            }
        }

        let items = match self.item_search(Some(box_id), "") {
            Ok(items) => items,
            Err(e) => { cleanup_export(); return Err(e.to_string()); }
        };

        let mut exported_count = 0;

        for item in items {
            let it = match self.item_prepare(item.id) {
                Ok(it) => it,
                Err(e) => { cleanup_export(); return Err(e); }
            };
            let kind = it.kind;
            let source_path = it.source_path;
            let display_name = it.display_name;
            let space = it.space;

            let dest_folder = match space.as_str() {
                "core" => &core_dir,
                "basis" => &basis_dir,
                "reference" => &ref_dir,
                _ => &temp_dir,
            };

            match kind.as_str() {
                "file" => {
                    let src = Path::new(&source_path);
                    if is_or_has_reparse_point(src) {
                        cleanup_export();
                        return Err(format!("심볼릭 링크나 정션은 내보낼 수 없습니다: {source_path}"));
                    }
                    if !src.is_file() {
                        cleanup_export();
                        return Err(format!("원본 파일을 찾을 수 없습니다: {source_path}"));
                    }
                    let filename = if display_name.is_empty() {
                        src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file.dat".into())
                    } else {
                        sanitize_filename(&display_name)
                    };
                    let dest_path = unique_path(dest_folder, &filename);
                    if dest_path.parent() != Some(dest_folder) {
                        cleanup_export();
                        return Err(format!("잘못된 대상 파일 경로입니다: {}", dest_path.display()));
                    }
                    if let Err(e) = copy_file_verified(src, &dest_path) {
                        cleanup_export();
                        return Err(format!("파일 복사 실패 ({source_path}): {e}"));
                    }
                    exported_count += 1;
                }
                "folder" => {
                    let src = Path::new(&source_path);
                    if is_or_has_reparse_point(src) {
                        cleanup_export();
                        return Err(format!("심볼릭 링크나 정션은 내보낼 수 없습니다: {source_path}"));
                    }
                    if !src.is_dir() {
                        cleanup_export();
                        return Err(format!("원본 폴더를 찾을 수 없습니다: {source_path}"));
                    }
                    let folder_name = if display_name.is_empty() {
                        src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "folder".into())
                    } else {
                        sanitize_filename(&display_name)
                    };
                    let dest_path = unique_path(dest_folder, &folder_name);
                    if dest_path.parent() != Some(dest_folder) {
                        cleanup_export();
                        return Err(format!("잘못된 대상 폴더 경로입니다: {}", dest_path.display()));
                    }
                    if let Err(e) = copy_dir_recursive(src, &dest_path) {
                        cleanup_export();
                        return Err(format!("폴더 복사 실패 ({source_path}): {e}"));
                    }
                    exported_count += 1;
                }
                "text" | "rich_text" | "note" => {
                    let clean_name = sanitize_filename(&display_name);
                    let ext = if kind == "rich_text" { "html" } else { "txt" };
                    let filename = if clean_name.to_ascii_lowercase().ends_with(&format!(".{ext}")) {
                        clean_name
                    } else {
                        format!("{clean_name}.{ext}")
                    };
                    let dest_path = unique_path(dest_folder, &filename);
                    if dest_path.parent() != Some(dest_folder) {
                        cleanup_export();
                        return Err(format!("잘못된 대상 경로입니다: {}", dest_path.display()));
                    }
                    let src = Path::new(&source_path);
                    if kind == "rich_text" && src.is_file() {
                        if is_or_has_reparse_point(src) { cleanup_export(); return Err("reparse point".into()); }
                        if let Err(e) = copy_file_verified(src, &dest_path) { cleanup_export(); return Err(format!("HTML export failed: {e}")); }
                    } else if src.is_file() {
                        if is_or_has_reparse_point(src) {
                            cleanup_export();
                            return Err(format!("심볼릭 링크나 정션은 내보낼 수 없습니다: {source_path}"));
                        }
                        if let Err(e) = copy_file_verified(src, &dest_path) {
                            cleanup_export();
                            return Err(format!("글 사본 복사 실패: {e}"));
                        }
                    } else if let Err(e) = fs::write(&dest_path, &source_path) {
                        cleanup_export();
                        return Err(format!("글 파일 저장 실패: {e}"));
                    }
                    exported_count += 1;
                }
                "image" | "capture" => {
                    let clean_name = sanitize_filename(&display_name);
                    let filename = if clean_name.ends_with(".png") || clean_name.ends_with(".jpg") || clean_name.ends_with(".jpeg") {
                        clean_name
                    } else {
                        format!("{clean_name}.png")
                    };
                    let dest_path = unique_path(dest_folder, &filename);
                    if dest_path.parent() != Some(dest_folder) {
                        cleanup_export();
                        return Err(format!("잘못된 대상 경로입니다: {}", dest_path.display()));
                    }
                    let src = Path::new(&source_path);
                    if is_or_has_reparse_point(src) {
                        cleanup_export();
                        return Err(format!("심볼릭 링크나 정션은 내보낼 수 없습니다: {source_path}"));
                    }
                    if !src.is_file() {
                        cleanup_export();
                        return Err(format!("원본 이미지 파일을 찾을 수 없습니다: {source_path}"));
                    }
                    if let Err(e) = copy_file_verified(src, &dest_path) {
                        cleanup_export();
                        return Err(format!("이미지 복사 실패: {e}"));
                    }
                    exported_count += 1;
                }
                "link" => {
                    let clean_name = sanitize_filename(&display_name);
                    let filename = if clean_name.ends_with(".url") {
                        clean_name
                    } else {
                        format!("{clean_name}.url")
                    };
                    let dest_path = unique_path(dest_folder, &filename);
                    if dest_path.parent() != Some(dest_folder) {
                        cleanup_export();
                        return Err(format!("잘못된 대상 경로입니다: {}", dest_path.display()));
                    }
                    let shortcut_content = format!("[InternetShortcut]\r\nURL={source_path}\r\n");
                    if let Err(e) = fs::write(&dest_path, shortcut_content) {
                        cleanup_export();
                        return Err(format!("바로가기 링크 생성 실패: {e}"));
                    }
                    exported_count += 1;
                }
                _ => {}
            }
        }

        let t = now();
        if let Err(e) = self.conn.execute(
            "UPDATE box SET export_path = ?1, updated_at = ?2 WHERE id = ?3",
            params![target_dir, t, box_id],
        ) {
            cleanup_export();
            return Err(format!("자료함 전환 정보 갱신 실패: {e}"));
        }

        Ok(exported_count)
    }
}

fn normalized_path(path: &Path) -> std::result::Result<PathBuf, String> {
    let full = std::path::absolute(path).map_err(|e| e.to_string())?;
    let mut existing = full.as_path();
    let mut missing = Vec::new();
    // Resolve the nearest existing ancestor, including Windows short names,
    // trailing-dot aliases and junctions, before adding a not-yet-created suffix.
    let resolved = loop {
        match existing.canonicalize() {
            Ok(resolved) => break resolved,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let part = existing.components().next_back().ok_or_else(|| e.to_string())?;
                missing.push(part.as_os_str().to_os_string());
                existing = existing.parent().ok_or_else(|| e.to_string())?;
            },
            Err(e) => return Err(e.to_string()),
        }
    };
    let mut combined = resolved;
    for part in missing.into_iter().rev() { combined.push(part); }
    let mut out = PathBuf::new();
    for part in combined.components() {
        match part { std::path::Component::ParentDir => { out.pop(); }, std::path::Component::CurDir => {}, _ => out.push(part.as_os_str()) }
    }
    #[cfg(windows)]
    {
        let text = out.to_string_lossy().replace('/', "\\").to_lowercase();
        let text = if let Some(unc) = text.strip_prefix("\\\\?\\unc\\") { format!("\\\\{unc}") }
            else { text.strip_prefix("\\\\?\\").unwrap_or(&text).to_string() };
        return Ok(PathBuf::from(text));
    }
    #[cfg(not(windows))]
    Ok(out)
}

fn library_root() -> PathBuf { PathBuf::from(data_dir()) }

fn remove_copy(path: &Path) {
    if is_or_has_reparse_point(path) { return; }
    if path.is_dir() { let _ = fs::remove_dir_all(path); } else { let _ = fs::remove_file(path); }
}

fn copy_file_verified(src: &Path, dst: &Path) -> std::io::Result<()> {
    let mut output = fs::OpenOptions::new().write(true).create_new(true).open(dst)?;
    copy_to_open_file(src, &mut output)
}

fn copy_to_open_file(src: &Path, output: &mut fs::File) -> std::io::Result<()> {
    use std::io::{Read, Write};
    let mut input = fs::File::open(src)?;
    let size = input.metadata()?.len();
    let mut copied = 0;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 { break; }
        output.write_all(&buffer[..read])?;
        copied += read as u64;
    }
    output.sync_all()?;
    if copied != size || input.metadata()?.len() != size { return Err(std::io::Error::other("복사 중 파일 크기가 변경되었습니다.")); }
    Ok(())
}

/// 파일명 충돌 회피: 동일 파일명이 존재할 경우 "이름 (1).ext", "이름 (2).ext" 형태로 새 경로 반환.
fn unique_path(dir: &Path, filename: &str) -> PathBuf {
    let target = dir.join(filename);
    if !target.exists() {
        return target;
    }
    let p = Path::new(filename);
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| filename.to_string());
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();

    let mut counter = 1;
    loop {
        let candidate = dir.join(format!("{stem} ({counter}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

/// 디렉터리 재귀 복사 (링크·정션 발견 시 명시적 오류 반환, 내부 파일명 충돌 회피, 복사 실패 에러 전파)
pub(crate) fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if is_or_has_reparse_point(src) || is_or_has_reparse_point(dst) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("심볼릭 링크나 정션은 내보낼 수 없습니다: {}", src.display()),
        ));
    }
    let norm_src = normalized_path(src).map_err(std::io::Error::other)?;
    let norm_dst = normalized_path(dst).map_err(std::io::Error::other)?;
    if norm_dst.starts_with(&norm_src) || norm_src.starts_with(&norm_dst) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("원본과 같거나 포함 관계인 폴더로 복사할 수 없습니다: {}", dst.display()),
        ));
    }
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        if is_or_has_reparse_point(&path) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("심볼릭 링크나 정션은 내보낼 수 없습니다: {}", path.display()),
            ));
        }
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();
        let safe_name = sanitize_filename(&name_str);
        if path.is_dir() {
            let dest_child = unique_path(dst, &safe_name);
            copy_dir_recursive(&path, &dest_child)?;
        } else {
            let dest_child = unique_path(dst, &safe_name);
            copy_file_verified(&path, &dest_child)?;
        }
    }
    Ok(())
}
