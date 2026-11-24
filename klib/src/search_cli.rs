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
                Ok(changed) => Value::obj(vec![("ok", true.into()), ("indexed", true.into()), ("changed", changed.into()), ("error", Value::Null)]),
                Err(error) => error_value(&error),
            }
        }
        "index_batch" => {
            let Some(paths) = request.arr("paths") else { return error_value("paths is required") };
            if paths.is_empty() || paths.len() > 500 { return error_value("paths must contain 1..500 files") }
            let batch = ix.batch();
            let mut results = Vec::with_capacity(paths.len());
            for path in paths {
                let Some(path) = (match path { Value::Str(path) => Some(path.as_str()), _ => None }) else {
                    results.push(Value::obj(vec![("indexed", false.into()), ("error", "path must be a string".into())]));
                    continue;
                };
                let name = Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or("");
                match crate::content::index_file(ix, path, name, 0) {
                    Ok(changed) => results.push(Value::obj(vec![("indexed", true.into()), ("changed", changed.into()), ("error", Value::Null)])),
                    Err(error) => results.push(Value::obj(vec![("indexed", false.into()), ("error", error.into())])),
                }
            }
            if let Err(error) = batch.finish() { return error_value(&error.to_string()) }
            Value::obj(vec![("ok", true.into()), ("results", Value::Arr(results))])
        }
        "remove_file" => match request.str("path") {
            Some(path) => match crate::content::remove_file(ix, path) {
                Ok(()) => Value::obj(vec![("ok", true.into())]), Err(error) => error_value(&error),
            },
            None => error_value("path is required"),
        },
        "status" => Value::obj(vec![
            ("ok", true.into()), ("document_count", (ix.file_count() as usize).into()),
            ("paragraph_count", (ix.doc_count() as usize).into()),
            ("index_path", index_dir.into()), ("state", "ready".into()),
        ]),
        "cancel" => Value::obj(vec![("ok", true.into()), ("cancelled", false.into())]),
        "search" => search(ix, &request),
        _ => error_value("unknown operation"),
    }
}

fn search(ix: &SearchIndex, request: &Value) -> Value {
    let query = request.str("query").unwrap_or("");
    let limit = request.i64("limit").unwrap_or(100).clamp(1, 5000) as usize;
    let offset = request.i64("offset").unwrap_or(0).max(0) as usize;
    let fetch_limit = limit.saturating_add(offset).saturating_mul(20).min(10000).max(limit);
    let hierarchy = request.bool("hierarchy").unwrap_or(false);
    let hits = if hierarchy && !query.contains('"') && !query.contains(':') && query.split_whitespace().count() > 1 {
        // SQLite 위계 검색은 낱말을 문서 단위로 AND 결합한다. Tantivy 문단 검색을 낱말별로 한 뒤 경로 집합을 교차한다.
        let mut terms = query.split_whitespace();
        let first = match terms.next().map(|term| ix.search(term, SearchOptions { limit: 5000 })) {
            Some(Ok(hits)) => hits,
            Some(Err(error)) => return error_value(&error.to_string()),
            None => Vec::new(),
        };
        let mut common: std::collections::HashSet<String> = first.iter().map(|hit| hit.path.clone()).collect();
        for term in terms {
            let term_hits = match ix.search(term, SearchOptions { limit: 5000 }) {
                Ok(hits) => hits,
                Err(error) => return error_value(&error.to_string()),
            };
            let paths: std::collections::HashSet<String> = term_hits.into_iter().map(|hit| hit.path).collect();
            common.retain(|path| paths.contains(path));
        }
        first.into_iter().filter(|hit| common.contains(&hit.path)).collect()
    } else {
        match ix.search(query, SearchOptions { limit: fetch_limit }) {
            Ok(hits) => hits,
            Err(error) => return error_value(&error.to_string()),
        }
    };
    let roots = request.arr("roots").unwrap_or(&[]);
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
        let locations: Vec<Value> = if hit.locations.is_empty() { vec![Value::obj(vec![
            ("paragraph_index", (hit.paragraph_index as i64).into()), ("snippet", snippet.clone().into()),
            ("level", (hit.level as i64).into()), ("table", hit.table.into()),
            ("highlights", Value::Arr(highlights.clone())), ("position", Value::obj(vec![
                ("page", hit.page.map(|p| (p as i64).into()).unwrap_or(Value::Null)),
                ("sheet", Value::Null), ("row", hit.row.map(|v|(v as i64).into()).unwrap_or(Value::Null)),
            ])), ("breadcrumb", Value::Arr(hit.breadcrumb.iter().cloned().map(Value::from).collect())), ("breadcrumb_text", hit.breadcrumb_text.clone().into()),
        ])] } else { hit.locations.iter().map(|loc| Value::obj(vec![
            ("paragraph_index", (loc.paragraph_index as i64).into()), ("snippet", loc.snippet.clone().into()),
            ("level", (loc.level as i64).into()), ("table", loc.table.into()),
            ("highlights", Value::Arr(highlights.clone())), ("position", Value::obj(vec![
                ("page", loc.page.map(|v|(v as i64).into()).unwrap_or(Value::Null)),
                ("sheet", Value::Null), ("row", loc.row.map(|v|(v as i64).into()).unwrap_or(Value::Null)),
            ])), ("breadcrumb", Value::Arr(loc.breadcrumb.iter().cloned().map(Value::from).collect())), ("breadcrumb_text", loc.breadcrumb.join(" > ").into()),
        ])).collect() };
        let breadcrumb: Vec<Value> = hit.breadcrumb.iter().cloned().map(Value::from).collect();
        if hierarchy {
            Value::obj(vec![
                ("path", hit.path.clone().into()), ("name", hit.name.into()), ("extension", extension.clone().into()),
                ("mtime", mtime_value.clone()), ("folder_path", Path::new(&hit.path).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default().into()),
                ("folder_name", Path::new(&hit.path).parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("").into()),
                ("locations", Value::Arr(locations)), ("breadcrumb", Value::Arr(breadcrumb)),
                ("sections", Value::Arr(Vec::new())), ("breadcrumb_text", hit.breadcrumb_text.into()),
                ("snippet", snippet.into()), ("highlights", Value::Arr(highlights)),
                ("position", Value::obj(vec![("page", hit.page.map(|p| (p as i64).into()).unwrap_or(Value::Null)), ("sheet", Value::Null), ("row", hit.row.map(|v|(v as i64).into()).unwrap_or(Value::Null))])),
                ("paragraph_index", (hit.paragraph_index as i64).into()), ("node_id", hit.node_id.into()),
                ("parent_id", hit.parent_id.map(Value::from).unwrap_or(Value::Null)),
                ("level", (hit.level as i64).into()),
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
