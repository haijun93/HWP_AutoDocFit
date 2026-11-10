use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const HIERARCHY_VERSION: i64 = 1;

pub fn run(db_path: &str, inputs: &[String]) -> Result<(), String> {
    let mut files = Vec::new();
    for input in inputs { collect(Path::new(input), &mut files)?; }
    files.sort(); files.dedup();
    let mut db = Connection::open(db_path).map_err(|e| e.to_string())?;
    initialize(&mut db)?;
    let total = files.len();
    for (i, path) in files.iter().enumerate() {
        let result = index_one(&mut db, path);
        let (state, count, error) = match result {
            Ok((false, count)) => ("skipped", count, None),
            Ok((true, count)) => ("indexed", count, None),
            Err(error) => ("error", 0, Some(error)),
        };
        let output = crate::json::Value::obj(vec![
            ("path", path.to_string_lossy().to_string().into()),
            ("status", state.into()), ("paragraphs", count.into()),
            ("completed", (i + 1).into()), ("total", total.into()),
            ("error", error.map(crate::json::Value::Str).unwrap_or(crate::json::Value::Null)),
        ]);
        println!("{}", crate::json::to_string(&output));
    }
    Ok(())
}

fn collect(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.is_file() {
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
        if matches!(ext.as_str(), "txt" | "md" | "hwpx" | "docx" | "hwp") {
            let absolute = if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir().map_err(|e|e.to_string())?.join(path) };
            files.push(absolute);
        }
        return Ok(());
    }
    if path.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(path).map_err(|e| e.to_string())?.filter_map(Result::ok).map(|e| e.path()).collect();
        entries.sort(); for entry in entries { collect(&entry, files)?; }
    }
    Ok(())
}

fn initialize(db: &mut Connection) -> Result<(), String> {
    db.execute_batch("PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS content_meta(path TEXT PRIMARY KEY,size INTEGER,mtime REAL,reference_date TEXT,name TEXT,extension TEXT,ctime REAL,hierarchy_schema_version INTEGER NOT NULL DEFAULT 0);
        CREATE TABLE IF NOT EXISTS content_errors(path TEXT PRIMARY KEY,reason TEXT,size INTEGER,mtime REAL);
        CREATE VIRTUAL TABLE IF NOT EXISTS content_paragraph_fts USING fts5(path UNINDEXED,name UNINDEXED,paragraph_index UNINDEXED,level UNINDEXED,level_rank UNINDEXED,body,location_label UNINDEXED,section UNINDEXED,tokenize='trigram');
        CREATE VIRTUAL TABLE IF NOT EXISTS content_fts USING fts5(path UNINDEXED,name UNINDEXED,body,tokenize='trigram');
        CREATE TABLE IF NOT EXISTS content_short_grams(gram TEXT NOT NULL,path TEXT NOT NULL,PRIMARY KEY(gram,path));
        CREATE INDEX IF NOT EXISTS content_short_grams_path ON content_short_grams(path);") .map_err(|e| e.to_string())
}

fn index_one(db: &mut Connection, path: &Path) -> Result<(bool, usize), String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_owned();
    let path = path.to_string_lossy().into_owned();
    let size = meta.len() as i64;
    let mtime = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let old: Option<(i64,f64,i64)> = db.query_row("SELECT size,mtime,hierarchy_schema_version FROM content_meta WHERE path=?", [&path], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e| e.to_string())?;
    let cached = old.is_some_and(|(s,m,v)| s==size && m==mtime && v>=HIERARCHY_VERSION);
    if cached {
        let count: i64 = db.query_row("SELECT count(*) FROM content_paragraph_fts WHERE path=?", [&path], |r|r.get(0)).map_err(|e|e.to_string())?;
        if count > 0 { return Ok((false,count as usize)); }
    }
    let paragraphs = crate::extract::extract_for_index(Path::new(&path))?;
    let texts: Vec<String> = paragraphs.iter().map(|p| p.text.clone()).collect();
    let scheme = hierarchy::compat::marker_scheme::detect_scheme(&texts);
    let levels = if ext == "hwp" {
        crate::legacy_records::classify_texts(&texts)
    } else {
        crate::classify_levels::classify_extracted(&paragraphs, &scheme)
    };
    let body = paragraphs.iter().map(|p|p.text.as_str()).collect::<Vec<_>>().join("\n");
    let date = extract_date(&body);
    let tx = db.transaction().map_err(|e|e.to_string())?;
    for table in ["content_paragraph_fts","content_fts","content_short_grams","content_meta","content_errors"] {
        tx.execute(&format!("DELETE FROM {table} WHERE path=?"), [&path]).map_err(|e|e.to_string())?;
    }
    for (i,p) in paragraphs.iter().enumerate() {
        let level = levels.get(i).map(String::as_str).unwrap_or("body");
        let rank = level_rank(level);
        tx.execute("INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body,location_label,section) VALUES(?,?,?,?,?,?,?,?)",
            params![path,name,i as i64,level,rank,p.text,p.location_label,p.section]).map_err(|e|e.to_string())?;
    }
    tx.execute("INSERT INTO content_fts(path,name,body) VALUES(?,?,?)", params![path,name,body]).map_err(|e|e.to_string())?;
    let chars: Vec<char> = body.to_lowercase().chars().collect();
    let grams: std::collections::HashSet<String> = chars.windows(2)
        .map(|pair| pair.iter().collect::<String>()).collect();
    for gram in grams { tx.execute("INSERT INTO content_short_grams(gram,path) VALUES(?,?)",params![gram,path]).map_err(|e|e.to_string())?; }
    tx.execute("INSERT OR REPLACE INTO content_meta(path,size,mtime,ctime,reference_date,name,extension,hierarchy_schema_version) VALUES(?,?,?,?,?,?,?,?)",params![path,size,mtime,meta.created().ok().and_then(|t|t.duration_since(UNIX_EPOCH).ok()).map(|d|d.as_secs_f64()).unwrap_or(0.0),date,name,ext,HIERARCHY_VERSION]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e|e.to_string())?;
    Ok((true,paragraphs.len()))
}

fn level_rank(level:&str)->i64 { match level {"document_title"=>0,"chapter"=>1,"section"=>2,"topic"=>3,"square"=>4,"circle"=>5,"dash"=>6,"asterisk"=>7,_=>8} }
fn extract_date(text:&str)->Option<String> {
    let re=regex::Regex::new(r"(20\d{2})[-./년 ]\s*(\d{1,2})[-./월 ]\s*(\d{1,2})").ok()?;
    for cap in re.captures_iter(text) { let (y,m,d)=(cap[1].parse::<u32>().ok()?,cap[2].parse::<u32>().ok()?,cap[3].parse::<u32>().ok()?); if let Some(date)=valid_date(y,m,d) { return Some(date); } }
    None
}
fn valid_date(y:u32,m:u32,d:u32)->Option<String> { if !(1..=12).contains(&m)||!(1..=31).contains(&d) {return None} Some(format!("{y:04}-{m:02}-{d:02}")) }
