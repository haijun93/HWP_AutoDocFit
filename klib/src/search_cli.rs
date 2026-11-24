//! SEMI1 opt-in JSONL adapter. Requires an explicit isolated index directory.
use crate::json::{self, Value as J};
use crate::search::SearchIndex;
use std::io::{self, BufRead, Write};

fn error(text: &str) -> J {
    J::obj(vec![("ok", false.into()), ("error", text.into())])
}

pub fn run(dir: &str) -> Result<(), String> {
    let ix = SearchIndex::open_or_create(dir).map_err(|e| e.to_string())?;
    let input = io::stdin();
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in input.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let response = match json::parse(&line) {
            Some(request) => handle(&ix, &request),
            None => error("invalid JSON"),
        };
        writeln!(output, "{}", json::to_string(&response)).map_err(|e| e.to_string())?;
        output.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn handle(ix: &SearchIndex, request: &J) -> J {
    match request.str("op").unwrap_or("") {
        "status" => match ix.related_status(request.arr("roots").unwrap_or(&[])) {
            Ok(status) => J::obj(vec![("ok", true.into()), ("status", status)]),
            Err(e) => error(&e.to_string()),
        },
        "replace_many" => {
            let Some(documents) = request.arr("documents") else {
                return error("documents required");
            };
            let batch = ix.batch();
            for document in documents {
                if let Err(e) = ix.replace_related(document) {
                    return error(&e.to_string());
                }
            }
            match batch.finish() {
                Ok(()) => J::obj(vec![("ok", true.into())]),
                Err(e) => error(&e.to_string()),
            }
        }
        "replace" => match ix.replace_related(request) {
            Ok(()) => J::obj(vec![("ok", true.into()), ("indexed", true.into())]),
            Err(e) => error(&e.to_string()),
        },
        "remove" => match request.str("path").map(|p| ix.delete_path(p)) {
            Some(Ok(())) => J::obj(vec![("ok", true.into())]),
            Some(Err(e)) => error(&e.to_string()),
            None => error("path required"),
        },
        "search" if request.bool("related_groups") == Some(true) => {
            let hits = match ix.search_related(
                request.str("query").unwrap_or(""),
                0,
                request.bool("filenames").unwrap_or(false),
            ) {
                Ok(hits) => hits,
                Err(e) => return error(&e.to_string()),
            };
            let roots = request.arr("roots").unwrap_or(&[]);
            let offset = request.i64("offset").unwrap_or(0).max(0) as usize;
            let limit = request.i64("limit").unwrap_or(100).clamp(1, 500) as usize;
            let mut rows: Vec<_> = hits
                .into_iter()
                .filter(|hit| {
                    let path = hit
                        .str("path")
                        .unwrap_or("")
                        .replace('\\', "/")
                        .to_lowercase();
                    let allowed = roots.is_empty()
                        || roots.iter().any(|r| {
                            let J::Str(root) = r else {
                                return false;
                            };
                            let root = root
                                .replace('\\', "/")
                                .to_lowercase()
                                .trim_end_matches('/')
                                .to_string();
                            path == root
                                || path.strip_prefix(&root).is_some_and(|s| s.starts_with('/'))
                        });
                    let mtime = hit.num("mtime").unwrap_or(0.0);
                    allowed
                        && request.num("start").is_none_or(|s| mtime >= s)
                        && request.num("end").is_none_or(|e| mtime < e)
                })
                .skip(offset)
                .take(limit)
                .collect();
            for (i, row) in rows.iter_mut().enumerate() {
                if let J::Obj(fields) = row {
                    if let Some((_, rank)) = fields.iter_mut().find(|(k, _)| k == "relevance_rank")
                    {
                        *rank = (offset + i + 1).into();
                    }
                }
            }
            J::obj(vec![("ok", true.into()), ("results", J::Arr(rows))])
        }
        _ => error("supported: replace, remove, search with related_groups=true"),
    }
}
