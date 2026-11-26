//! 즐겨찾기·태그 저장소. 앱 화면(`photo_album_service.PhotoAlbumDb`)과 **같은 SQLite 파일·같은 표**를 쓴다.
//! 읽기만 할 때는 파일을 만들지 않고, 사용자가 즐겨찾기·태그를 바꿀 때만 만든다.
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// `PhotoAlbumDb._init_db` 와 같은 표.
const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS registered_folders (path TEXT PRIMARY KEY, registered_at TEXT, last_scanned_at TEXT);
    CREATE TABLE IF NOT EXISTS photos (
        path TEXT PRIMARY KEY, folder TEXT, filename TEXT, size INTEGER, mtime REAL, date_taken TEXT,
        orientation INTEGER DEFAULT 1, favorite INTEGER DEFAULT 0, cloud INTEGER DEFAULT 0,
        missing INTEGER NOT NULL DEFAULT 0);
    CREATE TABLE IF NOT EXISTS tags (
        photo_path TEXT, tag TEXT, PRIMARY KEY (photo_path, tag),
        FOREIGN KEY (photo_path) REFERENCES photos(path) ON DELETE CASCADE);
    CREATE INDEX IF NOT EXISTS idx_photos_folder ON photos(folder);
    CREATE INDEX IF NOT EXISTS idx_photos_date ON photos(date_taken);
    CREATE INDEX IF NOT EXISTS idx_photos_favorite ON photos(favorite);
    CREATE INDEX IF NOT EXISTS idx_tags_tag ON tags(tag);
";

/// 태그 한 개의 길이 상한(글자).
pub const MAX_TAG: usize = 40;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mark {
    pub favorite: bool,
    pub tags: Vec<String>,
}

/// 목록의 경로와 DB 의 경로를 맞추는 열쇠(Windows 는 대소문자를 가리지 않는다).
pub fn key(path: &str) -> String {
    plain(path).replace('/', "\\").to_lowercase()
}

/// `\\?\` 접두사를 뗀 보통 경로.
fn plain(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
    }
}

/// DB 에 저장하는 경로: 부모 폴더는 실제 경로로 풀고 파일 이름은 받은 그대로 둔다
/// (`PhotoAlbumDb.transfer_photo` 와 같다. 없어진 파일에도 쓸 수 있다).
pub fn stored_path(path: &str) -> String {
    let path = Path::new(path);
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else { return plain(&path.to_string_lossy()) };
    let parent = std::fs::canonicalize(parent)
        .or_else(|_| std::path::absolute(parent))
        .unwrap_or_else(|_| parent.to_path_buf());
    plain(&Path::new(&plain(&parent.to_string_lossy())).join(name).to_string_lossy())
}

/// 앱과 같은 기본 위치(`PhotoAlbumService` → `kagent_client.data_dir()`).
/// 시험 격리 표시(`DOCFIT_FORBID_REAL_DATA`)가 있으면 실제 사용자 폴더는 쓰지 않는다.
pub fn default_db_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("KAGENT_DATA").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir).join("photo_album.db"));
    }
    let local = std::env::var("LOCALAPPDATA").ok().filter(|v| !v.is_empty())?;
    if std::env::var("DOCFIT_FORBID_REAL_DATA").is_ok_and(|v| v == "1")
        && std::env::var("DOCFIT_REAL_LOCALAPPDATA").is_ok_and(|real| key(&real) == key(&local))
    {
        return None;
    }
    Some(Path::new(&local).join("HwpAutoDocFit").join("kagent").join("photo_album.db"))
}

#[derive(Clone, Debug)]
pub struct Library {
    path: PathBuf,
}

type Result<T> = std::result::Result<T, String>;

fn failed(e: impl std::fmt::Display) -> String {
    format!("즐겨찾기·태그 저장소 오류: {e}")
}

impl Library {
    pub fn new(path: PathBuf) -> Library {
        Library { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn writer(&self) -> Result<Connection> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(failed)?;
        }
        let mut conn = Connection::open(&self.path).map_err(failed)?;
        conn.busy_timeout(std::time::Duration::from_secs(3)).map_err(failed)?;
        conn.execute_batch("PRAGMA foreign_keys = ON").map_err(failed)?;
        // Python v2.2.110과 같은 누락 표시. 이전 DB의 태그·즐겨찾기·촬영 정보는 보존한다.
        // 다른 창의 동시 첫 쓰기도 한 트랜잭션에서 직렬화한다.
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(failed)?;
        tx.execute_batch(SCHEMA).map_err(failed)?;
        let columns = tx.prepare("PRAGMA table_info(photos)")
            .and_then(|mut s| s.query_map([], |r| r.get::<_, String>(1))?.collect::<rusqlite::Result<Vec<_>>>())
            .map_err(failed)?;
        if !columns.iter().any(|name| name == "missing") {
            tx.execute("ALTER TABLE photos ADD COLUMN missing INTEGER NOT NULL DEFAULT 0", []).map_err(failed)?;
        }
        tx.commit().map_err(failed)?;
        Ok(conn)
    }

    /// 표시용 전체 목록(열쇠 → 즐겨찾기·태그). 아무 표시도 없는 사진은 담지 않는다.
    pub fn marks(&self) -> Result<HashMap<String, Mark>> {
        if !self.path.is_file() {
            return Ok(HashMap::new());
        }
        let conn = Connection::open_with_flags(&self.path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(failed)?;
        conn.busy_timeout(std::time::Duration::from_secs(3)).map_err(failed)?;
        let mut marks = HashMap::<String, Mark>::new();
        let read = |sql: &str| -> rusqlite::Result<Vec<(String, Option<String>)>> {
            conn.prepare(sql)?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect()
        };
        for (path, _) in read("SELECT path, NULL FROM photos WHERE favorite = 1").map_err(failed)? {
            marks.entry(key(&path)).or_default().favorite = true;
        }
        for (path, tag) in read("SELECT photo_path, tag FROM tags ORDER BY tag").map_err(failed)? {
            if let Some(tag) = tag {
                marks.entry(key(&path)).or_default().tags.push(tag);
            }
        }
        Ok(marks)
    }

    /// 즐겨찾기를 뒤집고 새 값을 돌려준다.
    pub fn toggle_favorite(&self, path: &str) -> Result<bool> {
        let path = stored_path(path);
        let mut conn = self.writer()?;
        let tx = conn.transaction().map_err(failed)?;
        let old: Option<i64> = tx
            .query_row("SELECT favorite FROM photos WHERE path = ?1", [&path], |r| r.get(0))
            .optional()
            .map_err(failed)?;
        let new = match old {
            Some(value) => {
                let new = value == 0;
                tx.execute("UPDATE photos SET favorite = ?1 WHERE path = ?2", params![new as i64, path]).map_err(failed)?;
                new
            }
            None => {
                insert_row(&tx, &path, true).map_err(failed)?;
                true
            }
        };
        tx.commit().map_err(failed)?;
        Ok(new)
    }

    pub fn add_tag(&self, path: &str, tag: &str) -> Result<()> {
        let tag = tag.trim();
        if tag.is_empty() || tag.chars().count() > MAX_TAG || tag.chars().any(char::is_control) {
            return Err(format!("태그는 1~{MAX_TAG}자의 글자여야 합니다"));
        }
        let path = stored_path(path);
        let mut conn = self.writer()?;
        let tx = conn.transaction().map_err(failed)?;
        let exists: Option<i64> = tx.query_row("SELECT 1 FROM photos WHERE path = ?1", [&path], |r| r.get(0)).optional().map_err(failed)?;
        if exists.is_none() {
            insert_row(&tx, &path, false).map_err(failed)?;
        }
        tx.execute("INSERT OR IGNORE INTO tags (photo_path, tag) VALUES (?1, ?2)", params![path, tag]).map_err(failed)?;
        tx.commit().map_err(failed)
    }

    pub fn remove_tag(&self, path: &str, tag: &str) -> Result<()> {
        if !self.path.is_file() {
            return Ok(());
        }
        self.writer()?
            .execute("DELETE FROM tags WHERE photo_path = ?1 AND tag = ?2", params![stored_path(path), tag.trim()])
            .map(|_| ())
            .map_err(failed)
    }

    /// 휴지통으로 보낸 사진의 행·태그를 지운다.
    pub fn delete(&self, path: &str) -> Result<()> {
        if !self.path.is_file() {
            return Ok(());
        }
        let path = stored_path(path);
        let conn = self.writer()?;
        conn.execute("DELETE FROM tags WHERE photo_path = ?1", [&path]).map_err(failed)?;
        conn.execute("DELETE FROM photos WHERE path = ?1", [&path]).map(|_| ()).map_err(failed)
    }

    /// 옮기기·이름 바꾸기·복사한 사진의 정보를 새 경로로. 복사면 원본은 두고 즐겨찾기·태그는 넘기지 않는다.
    pub fn transfer(&self, source: &str, target: &str, keep_source: bool) -> Result<()> {
        if !self.path.is_file() {
            return Ok(());
        }
        let (old, new) = (stored_path(source), stored_path(target));
        let mut conn = self.writer()?;
        let tx = conn.transaction().map_err(failed)?;
        type Row = (Option<i64>, Option<f64>, Option<String>, Option<i64>, Option<i64>, Option<i64>);
        let row: Option<Row> = tx
            .query_row("SELECT size, mtime, date_taken, orientation, favorite, cloud FROM photos WHERE path = ?1", [&old], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
            })
            .optional()
            .map_err(failed)?;
        let Some((size, mtime, date, orientation, favorite, cloud)) = row else { return Ok(()) };
        let tags: Vec<String> = tx
            .prepare("SELECT tag FROM tags WHERE photo_path = ?1")
            .and_then(|mut s| s.query_map([&old], |r| r.get(0))?.collect())
            .map_err(failed)?;
        let removed: &[&String] = if keep_source { &[&new] } else { &[&old, &new] };
        for path in removed {
            tx.execute("DELETE FROM tags WHERE photo_path = ?1", [path]).map_err(failed)?;
            tx.execute("DELETE FROM photos WHERE path = ?1", [path]).map_err(failed)?;
        }
        let (folder, name) = split(&new);
        let favorite = if keep_source { Some(0) } else { favorite };
        tx.execute(
            "INSERT INTO photos (path, folder, filename, size, mtime, date_taken, orientation, favorite, cloud)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![new, folder, name, size, mtime, date, orientation, favorite, cloud],
        )
        .map_err(failed)?;
        if !keep_source {
            for tag in tags {
                tx.execute("INSERT INTO tags (photo_path, tag) VALUES (?1, ?2)", params![new, tag]).map_err(failed)?;
            }
        }
        tx.commit().map_err(failed)
    }
}

fn split(path: &str) -> (String, String) {
    path.rsplit_once(['\\', '/']).map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or_default()
}

/// 앱이 아직 훑지 않은 사진의 행. 앱의 다음 스캔이 파일 정보를 채운다.
fn insert_row(conn: &Connection, path: &str, favorite: bool) -> rusqlite::Result<()> {
    let (folder, name) = split(path);
    let meta = std::fs::metadata(path).ok();
    let size = meta.as_ref().map_or(0, |m| m.len() as i64);
    let mtime = meta
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0.0, |d| d.as_secs_f64());
    // 촬영일 칸은 앱처럼 지역 시각 글자(파일 수정 시각). SQLite 가 시간대를 계산한다.
    let date: String = if mtime > 0.0 {
        conn.query_row("SELECT strftime('%Y-%m-%d %H:%M:%S', ?1, 'unixepoch', 'localtime')", [mtime as i64], |r| r.get(0))?
    } else {
        String::new()
    };
    conn.execute(
        "INSERT INTO photos (path, folder, filename, size, mtime, date_taken, orientation, favorite, cloud)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, 0)",
        params![path, folder, name, size, mtime, date, favorite as i64],
    )
    .map(|_| ())
}

/// 태그별 사진 수(보이는 목록에 있는 것만). 태그 이름 순.
pub fn tag_counts<'a>(marks: &HashMap<String, Mark>, paths: impl Iterator<Item = &'a str>) -> Vec<(String, usize)> {
    let mut counts = BTreeMap::<String, usize>::new();
    for path in paths {
        if let Some(mark) = marks.get(&key(path)) {
            for tag in &mark.tags {
                *counts.entry(tag.clone()).or_default() += 1;
            }
        }
    }
    let mut list: Vec<_> = counts.into_iter().collect();
    list.sort_by_key(|(tag, _)| tag.to_lowercase());
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> (tempfile::TempDir, Library, String) {
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("가 사진.jpg");
        std::fs::write(&photo, b"synthetic").unwrap();
        let library = Library::new(dir.path().join("db").join("photo_album.db"));
        let photo = photo.to_string_lossy().into_owned();
        (dir, library, photo)
    }

    #[test]
    fn reading_never_creates_the_database() {
        let (_dir, library, photo) = temp();
        assert!(library.marks().unwrap().is_empty());
        library.delete(&photo).unwrap();
        library.remove_tag(&photo, "x").unwrap();
        library.transfer(&photo, &photo, false).unwrap();
        assert!(!library.path().exists());
    }

    #[test]
    fn favorite_and_tags_round_trip() {
        let (_dir, library, photo) = temp();
        assert!(library.toggle_favorite(&photo).unwrap());
        library.add_tag(&photo, "  현장 ").unwrap();
        library.add_tag(&photo, "현장").unwrap();
        library.add_tag(&photo, "준공").unwrap();
        let mark = library.marks().unwrap()[&key(&stored_path(&photo))].clone();
        assert_eq!(mark, Mark { favorite: true, tags: vec!["준공".into(), "현장".into()] });
        assert_eq!(tag_counts(&library.marks().unwrap(), [stored_path(&photo).as_str()].into_iter()).len(), 2);
        library.remove_tag(&photo, "준공").unwrap();
        assert!(!library.toggle_favorite(&photo).unwrap());
        assert_eq!(library.marks().unwrap()[&key(&stored_path(&photo))], Mark { favorite: false, tags: vec!["현장".into()] });
        assert!(library.add_tag(&photo, "   ").is_err());
        assert!(library.add_tag(&photo, &"긴".repeat(MAX_TAG + 1)).is_err());
    }

    #[test]
    fn move_keeps_marks_copy_does_not_and_delete_removes() {
        let (dir, library, photo) = temp();
        library.toggle_favorite(&photo).unwrap();
        library.add_tag(&photo, "현장").unwrap();
        let copy = dir.path().join("copy.jpg").to_string_lossy().into_owned();
        library.transfer(&photo, &copy, true).unwrap();
        let marks = library.marks().unwrap();
        assert!(marks.contains_key(&key(&stored_path(&photo))));
        assert!(!marks.contains_key(&key(&stored_path(&copy))), "복사본에는 표시가 없다");

        let moved = dir.path().join("moved.jpg").to_string_lossy().into_owned();
        library.transfer(&photo, &moved, false).unwrap();
        let marks = library.marks().unwrap();
        assert!(!marks.contains_key(&key(&stored_path(&photo))));
        assert_eq!(marks[&key(&stored_path(&moved))], Mark { favorite: true, tags: vec!["현장".into()] });
        library.delete(&moved).unwrap();
        assert!(library.marks().unwrap().is_empty());
    }

    #[test]
    fn keys_ignore_case_separators_and_verbatim_prefix() {
        assert_eq!(key(r"\\?\C:\A\b.JPG"), key("c:/a/B.jpg"));
        assert_eq!(key(r"\\?\UNC\server\share\a.jpg"), r"\\server\share\a.jpg");
    }
}
