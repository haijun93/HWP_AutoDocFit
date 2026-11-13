//! One paired mapped/real-service query, identical root/SID and top-50 result IDs.
use ktm::backend::{Backend, pipe_search};
use std::{path::PathBuf, time::Instant};
fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let root = PathBuf::from(args.get(1).ok_or("compare INDEX PIPE OUTPUT")?);
    let pipe = args.get(2).ok_or("missing pipe")?;
    let start = Instant::now();
    let mut backend = Backend::open(&root)?;
    let mapped = backend.search("예산", false)?;
    let mapped_ms = start.elapsed().as_secs_f64() * 1000.0;
    let start = Instant::now();
    let remote = pipe_search(pipe, "예산", false)?;
    let pipe_ms = start.elapsed().as_secs_f64() * 1000.0;
    let mapped_ids: Vec<_> = mapped.iter().map(|r| r.doc_id).collect();
    let pipe_ids: Vec<_> = remote.iter().map(|r| r.doc_id).collect();
    let result = serde_json::json!({"mapped_open_query_ms":mapped_ms,"pipe_connect_query_ms":pipe_ms,"same_document_order":mapped_ids==pipe_ids,"rows":mapped_ids.len(),"selected":"mmap: supports hierarchy and one-character queries"});
    std::fs::write(args.get(3).ok_or("missing output")?, result.to_string())
        .map_err(|e| e.to_string())?;
    if mapped_ids != pipe_ids {
        return Err("mapped/pipe result order mismatch".into());
    }
    Ok(())
}
