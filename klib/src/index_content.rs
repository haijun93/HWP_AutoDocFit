use rayon::prelude::*;
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const HIERARCHY_VERSION: i64 = 1;

struct PreparedContent {
    paragraphs: Vec<crate::extract::Paragraph>,
    levels: Vec<String>,
    body: String,
    date: Option<String>,
    grams: HashSet<String>,
}

pub fn run(db_path: &str, inputs: &[String]) -> Result<(), String> {
    let mut files = Vec::new();
    for input in inputs { collect(Path::new(input), &mut files)?; }
    files.sort(); files.dedup();
    let mut db = Connection::open(db_path).map_err(|e| e.to_string())?;
    initialize(&mut db)?;

    let mut cached_meta: HashMap<String, (i64, f64, i64, usize)> = HashMap::new();
    if let Ok(mut stmt) = db.prepare(
        "SELECT m.path, m.size, m.mtime, m.hierarchy_schema_version, COUNT(p.path)
         FROM content_meta m
         LEFT JOIN content_paragraph_fts p ON p.path = m.path
         GROUP BY m.path"
    ) {
        if let Ok(rows) = stmt.query_map([], |row| {
            let path: String = row.get(0)?;
            let size: i64 = row.get(1)?;
            let mtime: f64 = row.get(2)?;
            let version: i64 = row.get(3)?;
            let count: i64 = row.get(4)?;
            Ok((path, (size, mtime, version, count as usize)))
        }) {
            for item in rows.flatten() {
                cached_meta.insert(item.0, item.1);
            }
        }
    }

    let work_items: Vec<_> = files.into_iter().map(|path| {
        let meta = std::fs::metadata(&path);
        let path_str = path.to_string_lossy().into_owned();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_owned();
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
        (path, path_str, name, ext, meta)
    }).collect();

    let prepared: Vec<_> = work_items.into_par_iter().map(|(path, path_str, name, ext, meta_res)| {
        let meta = match meta_res {
            Ok(m) => m,
            Err(e) => return (path, path_str, name, ext, 0, 0.0, 0.0, Err(e.to_string())),
        };
        let size = meta.len() as i64;
        let mtime = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs_f64()).unwrap_or(0.0);
        let ctime = meta.created().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs_f64()).unwrap_or(0.0);

        if let Some(&(s, m, v, count)) = cached_meta.get(&path_str) {
            if s == size && m == mtime && v >= HIERARCHY_VERSION && count > 0 {
                return (path, path_str, name, ext, size, mtime, ctime, Ok(None));
            }
        }

        let content_res = (|| -> Result<PreparedContent, String> {
            let paragraphs = crate::extract::extract_for_index(&path)?;
            let texts: Vec<String> = paragraphs.iter().map(|p| p.text.clone()).collect();
            let scheme = hierarchy::compat::marker_scheme::detect_scheme(&texts);
            let levels = if ext == "hwp" {
                crate::legacy_records::classify_texts(&texts)
            } else {
                crate::classify_levels::classify_extracted(&paragraphs, &scheme)
            };
            let body = paragraphs.iter().map(|p| p.text.as_str()).collect::<Vec<_>>().join("\n");
            let date = extract_date(&body);
            let chars: Vec<char> = body.to_lowercase().chars().collect();
            let grams: HashSet<String> = chars.windows(2)
                .map(|pair| pair.iter().collect::<String>()).collect();

            Ok(PreparedContent {
                paragraphs,
                levels,
                body,
                date,
                grams,
            })
        })();

        (path, path_str, name, ext, size, mtime, ctime, content_res.map(Some))
    }).collect();

    let total = prepared.len();
    for (i, (_path, path_str, name, ext, size, mtime, ctime, res)) in prepared.into_iter().enumerate() {
        let (state, count, error) = match res {
            Ok(None) => {
                let count = cached_meta.get(&path_str).map(|c| c.3).unwrap_or(0);
                ("skipped", count, None)
            }
            Ok(Some(content)) => {
                let save_res = (|| -> Result<usize, String> {
                    let tx = db.transaction().map_err(|e| e.to_string())?;
                    for table in ["content_paragraph_fts", "content_fts", "content_short_grams", "content_meta", "content_errors"] {
                        tx.execute(&format!("DELETE FROM {table} WHERE path=?"), [&path_str]).map_err(|e| e.to_string())?;
                    }
                    for (idx, p) in content.paragraphs.iter().enumerate() {
                        let level = content.levels.get(idx).map(String::as_str).unwrap_or("body");
                        let rank = level_rank(level);
                        tx.execute(
                            "INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body,location_label,section) VALUES(?,?,?,?,?,?,?,?)",
                            params![path_str, name, idx as i64, level, rank, p.text, p.location_label, p.section],
                        ).map_err(|e| e.to_string())?;
                    }
                    tx.execute("INSERT INTO content_fts(path,name,body) VALUES(?,?,?)", params![path_str, name, content.body]).map_err(|e| e.to_string())?;
                    for gram in content.grams {
                        tx.execute("INSERT INTO content_short_grams(gram,path) VALUES(?,?)", params![gram, path_str]).map_err(|e| e.to_string())?;
                    }
                    tx.execute(
                        "INSERT OR REPLACE INTO content_meta(path,size,mtime,ctime,reference_date,name,extension,hierarchy_schema_version) VALUES(?,?,?,?,?,?,?,?)",
                        params![path_str, size, mtime, ctime, content.date, name, ext, HIERARCHY_VERSION],
                    ).map_err(|e| e.to_string())?;
                    tx.commit().map_err(|e| e.to_string())?;
                    Ok(content.paragraphs.len())
                })();

                match save_res {
                    Ok(len) => ("indexed", len, None),
                    Err(e) => ("error", 0, Some(e)),
                }
            }
            Err(e) => ("error", 0, Some(e)),
        };

        let output = crate::json::Value::obj(vec![
            ("path", path_str.into()),
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
        if matches!(ext.as_str(), "txt" | "md" | "hwpx" | "docx" | "hwp" | "xlsx" | "pptx" | "eml" | "pdf") {
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


fn level_rank(level:&str)->i64 { match level {"document_title"=>0,"chapter"=>1,"section"=>2,"topic"=>3,"square"=>4,"circle"=>5,"dash"=>6,"asterisk"=>7,_=>8} }
fn extract_date(text:&str)->Option<String> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"(20\d{2})[-./년 ]\s*(\d{1,2})[-./월 ]\s*(\d{1,2})").unwrap());
    for cap in re.captures_iter(text) { let (y,m,d)=(cap[1].parse::<u32>().ok()?,cap[2].parse::<u32>().ok()?,cap[3].parse::<u32>().ok()?); if let Some(date)=valid_date(y,m,d) { return Some(date); } }
    None
}
fn valid_date(y:u32,m:u32,d:u32)->Option<String> { if !(1..=12).contains(&m)||!(1..=31).contains(&d) {return None} Some(format!("{y:04}-{m:02}-{d:02}")) }
