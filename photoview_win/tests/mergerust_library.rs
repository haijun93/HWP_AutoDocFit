//! v2.2.110 목록 DB와 Rust 사진뷰의 합성 DB 왕복. 사용자 DB는 열지 않는다.
use photoview_win::library::{stored_path, Library};
use rusqlite::{Connection, params};

#[test]
fn old_database_migration_preserves_photo_metadata_and_marks() {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("photo_album.db");
    // 제품은 부모 폴더를 실제 경로로 풀어 저장한다(짧은 이름·정션 임시 폴더에서도 같은 열쇠).
    let path = stored_path(&dir.path().join("synthetic.jpg").to_string_lossy());
    let conn = Connection::open(&database).unwrap();
    conn.execute_batch("CREATE TABLE photos (
        path TEXT PRIMARY KEY, folder TEXT, filename TEXT, size INTEGER, mtime REAL, date_taken TEXT,
        orientation INTEGER DEFAULT 1, favorite INTEGER DEFAULT 0, cloud INTEGER DEFAULT 0);
        CREATE TABLE tags (photo_path TEXT, tag TEXT, PRIMARY KEY(photo_path,tag),
        FOREIGN KEY(photo_path) REFERENCES photos(path) ON DELETE CASCADE);").unwrap();
    conn.execute("INSERT INTO photos VALUES (?1,'folder','synthetic.jpg',123,456.0,'2026-01-02',6,1,0)", [&path]).unwrap();
    conn.execute("INSERT INTO tags VALUES (?1,'original')", [&path]).unwrap();
    drop(conn);

    let library = Library::new(database.clone());
    library.add_tag(&path, "new").unwrap();
    let conn = Connection::open(&database).unwrap();
    let row: (i64, f64, String, i64, i64, i64) = conn.query_row(
        "SELECT size,mtime,date_taken,orientation,favorite,missing FROM photos WHERE path=?1", [&path],
        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).unwrap();
    assert_eq!(row, (123,456.0,"2026-01-02".into(),6,1,0));
    let tags: Vec<String> = conn.prepare("SELECT tag FROM tags WHERE photo_path=?1 ORDER BY tag").unwrap()
        .query_map([&path], |r| r.get(0)).unwrap().collect::<Result<_,_>>().unwrap();
    assert_eq!(tags, ["new", "original"]);
}

#[test]
fn editing_marks_keeps_missing_rows_and_unrelated_list_state() {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("photo_album.db");
    // 제품은 부모 폴더를 실제 경로로 풀어 저장한다(짧은 이름·정션 임시 폴더에서도 같은 열쇠).
    let path = stored_path(&dir.path().join("missing.jpg").to_string_lossy());
    let library = Library::new(database.clone());
    library.add_tag(&path, "keep").unwrap();
    let conn = Connection::open(&database).unwrap();
    conn.execute("UPDATE photos SET missing=1,date_taken='2026-02-03',orientation=8 WHERE path=?1", [&path]).unwrap();
    conn.execute_batch("CREATE TABLE view_sync_state (value TEXT); INSERT INTO view_sync_state VALUES ('keep');").unwrap();
    drop(conn);

    assert!(library.toggle_favorite(&path).unwrap());
    library.add_tag(&path, "another").unwrap();
    let conn = Connection::open(&database).unwrap();
    let row: (i64,String,i64,i64) = conn.query_row(
        "SELECT missing,date_taken,orientation,favorite FROM photos WHERE path=?1", params![path],
        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
    assert_eq!(row,(1,"2026-02-03".into(),8,1));
    assert_eq!(conn.query_row("SELECT value FROM view_sync_state", [], |r| r.get::<_,String>(0)).unwrap(), "keep");
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM tags", [], |r| r.get::<_,i64>(0)).unwrap(), 2);
}
