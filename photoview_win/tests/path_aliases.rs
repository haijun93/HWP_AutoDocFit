//! PHOTOPATH1: 조회 때문에 기존 행/표시를 옮기거나 지우지 않는다. 합성 DB만 사용한다.
use photoview_win::library::{Library, resolved_key};
use rusqlite::Connection;

#[test]
fn case_alias_writes_reuse_the_original_row() {
    let dir = tempfile::tempdir().unwrap();
    let photo = dir.path().join("Synthetic.JPG");
    std::fs::write(&photo, b"synthetic").unwrap();
    let db = dir.path().join("synthetic.db");
    let library = Library::new(db.clone());
    let path = photo.to_string_lossy();
    library.add_tag(&path, "keep").unwrap();
    #[cfg(windows)]
    let alias = path.to_lowercase();
    #[cfg(not(windows))]
    let alias = path.to_string();
    library.add_tag(&alias, "new").unwrap();
    assert!(library.toggle_favorite(&alias).unwrap());
    library.remove_tag(&alias, "new").unwrap();
    let conn = Connection::open(db).unwrap();
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM photos", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    assert_eq!(library.marks().unwrap()[&resolved_key(&path)].tags, ["keep"]);
}

#[test]
fn reads_preserve_existing_duplicate_rows_and_union_marks() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("folder");
    std::fs::create_dir(&folder).unwrap();
    let photo = folder.join("synthetic.jpg");
    std::fs::write(&photo, b"synthetic").unwrap();
    let db = dir.path().join("synthetic.db");
    let library = Library::new(db.clone());
    let path = photo.to_string_lossy().to_string();
    library.add_tag(&path, "shared").unwrap();
    let alias = folder.join(".").join("synthetic.jpg").to_string_lossy().to_string();
    let conn = Connection::open(&db).unwrap();
    conn.execute("INSERT INTO photos(path,favorite,orientation,missing) VALUES (?1,1,6,1)", [&alias]).unwrap();
    conn.execute("INSERT INTO tags VALUES (?1,'shared'),(?1,'legacy')", [&alias]).unwrap();
    let before = std::fs::read(&db).unwrap();
    let marks = library.marks().unwrap();
    assert!(marks[&resolved_key(&path)].favorite);
    assert_eq!(marks[&resolved_key(&path)].tags, ["legacy", "shared"]);
    assert_eq!(std::fs::read(&db).unwrap(), before);
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM photos", [], |r| r.get::<_, i64>(0)).unwrap(), 2);
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM tags", [], |r| r.get::<_, i64>(0)).unwrap(), 3);
}

#[test]
fn different_folders_with_the_same_basename_stay_distinct() {
    let dir = tempfile::tempdir().unwrap();
    let library = Library::new(dir.path().join("synthetic.db"));
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    let a = a.join("missing.jpg").to_string_lossy().to_string();
    let b = b.join("missing.jpg").to_string_lossy().to_string();
    library.add_tag(&a, "a").unwrap();
    library.add_tag(&b, "b").unwrap();
    let marks = library.marks().unwrap();
    assert_eq!(marks.len(), 2);
    assert_eq!(marks[&resolved_key(&a)].tags, ["a"]);
    assert_eq!(marks[&resolved_key(&b)].tags, ["b"]);
}
