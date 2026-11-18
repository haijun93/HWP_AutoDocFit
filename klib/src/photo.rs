//! 사진 태그·검색을 위한 플랫폼 독립 엔진 규칙.

use rusqlite::{params, Connection, Result};

/// 태그 목록을 정리한다. 공백 태그를 버리고 앞뒤 공백을 제거하며 중복을 제거한다.
pub fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut normalized: Vec<String> = tags
        .iter()
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
        .map(str::to_owned)
        .collect();
    normalized.sort();
    normalized.dedup();
    normalized
}

/// 사진 메타데이터에 사진뷰 검색어가 일치하는지 확인한다.
/// 검색어는 파일명·경로·태그에서 부분 문자열로, 촬영일에서 날짜 접두사로 찾는다.
pub fn matches_search(filename: &str, path: &str, date_taken: &str, tags: &[String], query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    filename.to_lowercase().contains(&query)
        || path.to_lowercase().contains(&query)
        || date_taken.to_lowercase().contains(&query)
        || tags.iter().any(|tag| tag.to_lowercase().contains(&query))
}

/// 기존 업무자료 스키마와 분리된 새 테이블만 추가한다.
pub fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS photo_tags (photo_path TEXT NOT NULL, tag TEXT NOT NULL, PRIMARY KEY(photo_path, tag)); CREATE INDEX IF NOT EXISTS idx_photo_tags_tag ON photo_tags(tag);")
}

pub fn get_tags(conn: &Connection, path: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT tag FROM photo_tags WHERE photo_path=?1 ORDER BY tag")?;
    stmt.query_map([path], |row| row.get(0))?.collect()
}

pub fn add_tag(conn: &Connection, path: &str, tag: &str) -> Result<Vec<String>> {
    let tag = tag.trim();
    if !tag.is_empty() { conn.execute("INSERT OR IGNORE INTO photo_tags(photo_path,tag) VALUES(?1,?2)", params![path, tag])?; }
    get_tags(conn, path)
}

pub fn remove_tag(conn: &Connection, path: &str, tag: &str) -> Result<Vec<String>> {
    conn.execute("DELETE FROM photo_tags WHERE photo_path=?1 AND tag=?2", params![path, tag.trim()])?;
    get_tags(conn, path)
}

pub fn list_by_tag(conn: &Connection, tag: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT photo_path FROM photo_tags WHERE tag=?1 ORDER BY photo_path")?;
    stmt.query_map([tag.trim()], |row| row.get(0))?.collect()
}

pub fn search(conn: &Connection, query: &str, tag: Option<&str>) -> Result<Vec<(String, Vec<String>)>> {
    let needle = format!("%{}%", query.trim().to_lowercase());
    let paths = {
        let mut stmt = conn.prepare("SELECT DISTINCT p.photo_path FROM photo_tags p WHERE (?1='' OR LOWER(p.photo_path) LIKE ?2 OR EXISTS (SELECT 1 FROM photo_tags q WHERE q.photo_path=p.photo_path AND LOWER(q.tag) LIKE ?2)) AND (?3 IS NULL OR EXISTS (SELECT 1 FROM photo_tags t WHERE t.photo_path=p.photo_path AND LOWER(t.tag)=LOWER(?3))) ORDER BY p.photo_path")?;
        stmt.query_map(params![query.trim(), needle, tag], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>>>()?
    };
    paths.into_iter().map(|path| Ok((path.clone(), get_tags(conn, &path)?))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_trim_drop_empty_and_deduplicate() {
        let input = vec![" 풍경 ".into(), "".into(), "여행".into(), "풍경".into(), "  ".into()];
        assert_eq!(normalize_tags(&input), vec!["여행", "풍경"]);
    }

    #[test]
    fn search_matches_filename_date_and_tag_case_insensitively() {
        let tags = vec!["현장 점검".to_string()];
        assert!(matches_search("IMG_100.JPG", "D:/photos/IMG_100.JPG", "2026-08-15 12:00:00", &tags, "img_100"));
        assert!(matches_search("IMG_100.JPG", "D:/photos/IMG_100.JPG", "2026-08-15 12:00:00", &tags, "2026-08"));
        assert!(matches_search("IMG_100.JPG", "D:/photos/IMG_100.JPG", "2026-08-15 12:00:00", &tags, "점검"));
        assert!(!matches_search("IMG_100.JPG", "D:/photos/IMG_100.JPG", "2026-08-15 12:00:00", &tags, "휴가"));
    }

    #[test]
    fn empty_search_matches_every_photo() {
        assert!(matches_search("a.jpg", "photos/a.jpg", "", &[], "  "));
    }

    #[test]
    fn sqlite_tags_crud_search_and_tag_listing() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        assert_eq!(add_tag(&conn, "photos/a.jpg", " trip ").unwrap(), vec!["trip"]);
        assert_eq!(add_tag(&conn, "photos/a.jpg", "trip").unwrap(), vec!["trip"]);
        add_tag(&conn, "photos/a.jpg", "summer").unwrap();
        add_tag(&conn, "photos/b.jpg", "trip").unwrap();
        assert_eq!(list_by_tag(&conn, "trip").unwrap(), vec!["photos/a.jpg", "photos/b.jpg"]);
        assert_eq!(search(&conn, "SUM", None).unwrap()[0].0, "photos/a.jpg");
        assert_eq!(search(&conn, "", Some("trip")).unwrap().len(), 2);
        assert_eq!(remove_tag(&conn, "photos/a.jpg", "trip").unwrap(), vec!["summer"]);
    }
}
