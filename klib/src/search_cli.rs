//! Long-lived JSONL adapter for Python's optional Rust search backend.
use crate::json::{self, Value};
use crate::search::{SearchIndex, SearchOptions};
use std::io::{self, BufRead, Write};
use std::path::Path;

pub fn run() {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    let mut index: Option<SearchIndex> = None;
    for line in stdin.lock().lines() {
        let reply = match line {
            Ok(line) => handle(&line, &mut index),
            Err(error) => error_value(&error.to_string()),
        };
        let _ = writeln!(stdout, "{}", json::to_string(&reply));
        let _ = stdout.flush();
    }
}

fn handle(line: &str, index: &mut Option<SearchIndex>) -> Value {
    let Some(request) = json::parse(line) else { return error_value("invalid JSON request") };
    let op = request.str("op").unwrap_or("");
    if op == "shutdown" { return Value::obj(vec![("ok", true.into())]); }
    let index_dir = std::env::var("KLIB_TANTIVY_DIR").unwrap_or_else(|_| {
        format!("{}\\tantivy", crate::util::data_dir())
    });
    if index.is_none() {
        match SearchIndex::open_or_create(&index_dir) {
            Ok(opened) => *index = Some(opened),
            Err(error) => return error_value(&error.to_string()),
        }
    }
    let ix = index.as_ref().expect("initialized above");
    match op {
        "index_file" => {
            let Some(path) = request.str("path") else { return error_value("path is required") };
            let name = request.str("name").unwrap_or_else(|| Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(""));
            match crate::content::index_file(ix, path, name, 0) {
                Ok(()) => Value::obj(vec![("ok", true.into()), ("indexed", true.into()), ("error", Value::Null)]),
                Err(error) => error_value(&error),
            }
        }
        "remove_file" => match request.str("path") {
            Some(path) => match crate::content::remove_file(ix, path) {
                Ok(()) => Value::obj(vec![("ok", true.into())]), Err(error) => error_value(&error),
            },
            None => error_value("path is required"),
        },
        "status" => Value::obj(vec![
            ("ok", true.into()), ("document_count", (ix.doc_count() as usize).into()),
            ("index_path", index_dir.into()), ("state", "ready".into()),
        ]),
        "cancel" => Value::obj(vec![("ok", true.into()), ("cancelled", false.into())]),
        "search" => search(ix, &request),
        _ => error_value("unknown operation"),
    }
}

fn search(ix: &SearchIndex, request: &Value) -> Value {
    let query = request.str("query").unwrap_or("");
    let limit = request.i64("limit").unwrap_or(100).clamp(1, 500) as usize;
    let offset = request.i64("offset").unwrap_or(0).max(0) as usize;
    let fetch_limit = limit.saturating_add(offset).saturating_mul(20).min(4000).max(limit);
    let hits = match ix.search(query, SearchOptions { limit: fetch_limit }) {
        Ok(hits) => hits,
        Err(error) => return error_value(&error.to_string()),
    };
    let roots = request.arr("roots").unwrap_or(&[]);
    let hierarchy = request.bool("hierarchy").unwrap_or(false);
    let rows: Vec<Value> = hits.into_iter().filter(|hit| {
        let path = hit.path.to_lowercase();
        let in_roots = roots.is_empty() || roots.iter().any(|root| match root {
            Value::Str(root) => {
                let root = root.to_lowercase().trim_end_matches(|c| c == '\\' || c == '/').to_string();
                path.starts_with(&root) && path.get(root.len()..).is_some_and(|tail| tail.is_empty() || tail.starts_with('\\') || tail.starts_with('/'))
            }, _ => false,
        });
        if !in_roots { return false; }
        let mtime = std::fs::metadata(&hit.path).ok().and_then(|meta| meta.modified().ok())
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()).map(|time| time.as_secs_f64());
        let start = request.num("start");
        let end = request.num("end");
        start.is_none_or(|start| mtime.is_some_and(|mtime| mtime >= start))
            && end.is_none_or(|end| mtime.is_some_and(|mtime| mtime < end))
    }).skip(offset).take(limit).map(|hit| {
        let extension = Path::new(&hit.path).extension().and_then(|ext| ext.to_str()).unwrap_or("").to_lowercase();
        let mtime = std::fs::metadata(&hit.path).ok().and_then(|meta| meta.modified().ok())
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()).map(|time| time.as_secs_f64());
        let mtime_value = mtime.map(Value::from).unwrap_or(Value::Null);
        let snippet = hit.snippets.first().cloned().unwrap_or_default();
        let highlights: Vec<Value> = hit.highlighted.into_iter().map(Value::from).collect();
        let locations = vec![Value::obj(vec![
            ("paragraph_index", 0usize.into()), ("snippet", snippet.clone().into()),
            ("highlights", Value::Arr(highlights.clone())), ("position", Value::obj(vec![
                ("page", hit.page.map(|p| (p as i64).into()).unwrap_or(Value::Null)),
                ("sheet", Value::Null), ("row", Value::Null),
            ])), ("breadcrumb", Value::Arr(Vec::new())), ("breadcrumb_text", "".into()),
        ])];
        if hierarchy {
            Value::obj(vec![
                ("path", hit.path.clone().into()), ("name", hit.name.into()), ("extension", extension.clone().into()),
                ("mtime", mtime_value.clone()), ("folder_path", Path::new(&hit.path).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default().into()),
                ("folder_name", Path::new(&hit.path).parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("").into()),
                ("locations", Value::Arr(locations)), ("breadcrumb", Value::Arr(Vec::new())),
                ("sections", Value::Arr(Vec::new())), ("breadcrumb_text", "".into()),
                ("snippet", snippet.into()), ("highlights", Value::Arr(highlights)),
                ("position", Value::obj(vec![("page", hit.page.map(|p| (p as i64).into()).unwrap_or(Value::Null)), ("sheet", Value::Null), ("row", Value::Null)])),
                ("score", (hit.score as f64).into()),
            ])
        } else {
            Value::obj(vec![
                ("path", hit.path.into()), ("name", hit.name.into()), ("extension", extension.into()),
                ("locations", Value::Arr(locations)), ("snippet", snippet.into()),
                ("highlights", Value::Arr(highlights)), ("score", (hit.score as f64).into()),
                ("total_matches", (hit.paragraph_count as i64).into()),
                ("relevance_rank", 0usize.into()),
            ])
        }
    }).collect();
    Value::obj(vec![("ok", true.into()), ("results", Value::Arr(rows))])
}

fn error_value(message: &str) -> Value {
    Value::obj(vec![("ok", false.into()), ("error", message.into())])
}
