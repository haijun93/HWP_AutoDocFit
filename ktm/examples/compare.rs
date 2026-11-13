//! MG2: 같은 색인(같은 사용자 SID 폴더)에 대해 메모리 매핑 직접 읽기와 실제 서비스 파이프(kfind --user)를 비교한다.
//! compare INDEX_DIR PIPE OUTPUT [NAMES_ROOT]
//! - 처음 한 번(열기·연결 포함)과 30회 p50/p95. 같은 질의의 상위 50 문서 순서가 같아야 한다.
//! - NAMES_ROOT를 주면 서비스에 names_build{roots}를 보내고 파일명 질의 지연도 잰다(파일명은 파이프만).
//!   (names_build·names_status는 id 필드를 받지 않는다: 알 수 없는 필드면 서비스가 응답 없이 끊음)
use ktm::backend::{Backend, pipe_call, pipe_names, pipe_search};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Instant};

const RUNS: usize = 30;

fn stats(mut v: Vec<f64>) -> Value {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    json!({"p50_ms": v[v.len() / 2], "p95_ms": v[(v.len() * 95 / 100).min(v.len() - 1)]})
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let root = PathBuf::from(args.get(1).ok_or("compare INDEX PIPE OUTPUT [NAMES_ROOT]")?);
    let pipe = args.get(2).ok_or("missing pipe")?;
    let output = args.get(3).ok_or("missing output")?;
    let mut out = serde_json::Map::new();

    let t = Instant::now();
    let mut backend = Backend::open(&root)?;
    let first = backend.search("예산", false)?;
    out.insert("mapped_first_open_query_ms".into(), ms(t).into());
    let t = Instant::now();
    let remote_first = pipe_search(pipe, "예산", false)?;
    out.insert("pipe_first_connect_query_ms".into(), ms(t).into());
    let ids = |rows: &[ktm::backend::Row]| rows.iter().map(|r| r.doc_id).collect::<Vec<_>>();
    let same_first = ids(&first) == ids(&remote_first);

    let mut same = same_first;
    for q in ["예산", "추진 계획"] {
        let (mut a, mut b) = (Vec::new(), Vec::new());
        for _ in 0..RUNS {
            let t = Instant::now();
            let m = backend.search(q, false)?;
            a.push(ms(t));
            let t = Instant::now();
            let p = pipe_search(pipe, q, false)?;
            b.push(ms(t));
            same &= ids(&m) == ids(&p);
        }
        out.insert(format!("mapped_{q}"), stats(a));
        out.insert(format!("pipe_{q}"), stats(b));
    }
    // 위계·1글자는 매핑만(파이프 query에는 위계 순서·1글자 훑기가 없다).
    let mut h = Vec::new();
    for _ in 0..RUNS {
        let t = Instant::now();
        backend.search("예산", true)?;
        h.push(ms(t));
    }
    out.insert("mapped_hierarchy_예산".into(), stats(h));
    out.insert("same_document_order".into(), same.into());
    out.insert("rows".into(), first.len().into());

    if let Some(names_root) = args.get(4) {
        let t = Instant::now();
        let r: Value = serde_json::from_slice(&pipe_call(pipe, &json!({"command":"names_build","roots":[names_root]}))?)
            .map_err(|e| e.to_string())?;
        if r["ok"] != true {
            return Err(format!("names_build: {r}"));
        }
        loop {
            let s: Value = serde_json::from_slice(&pipe_call(pipe, &json!({"command":"names_status"}))?)
                .map_err(|e| e.to_string())?;
            if s["result"]["state"] != "building" {
                out.insert("names_state".into(), s["result"]["state"].clone());
                out.insert("names_entries".into(), s["result"]["entries"].clone());
                break;
            }
            if t.elapsed().as_secs() > 60 {
                return Err("names_build timeout".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        out.insert("names_build_ms".into(), ms(t).into());
        for (q, sort) in [("합성문서", "relevance"), ("합성문서-0", "size"), ("txt", "date")] {
            let mut v = Vec::new();
            let mut n = 0;
            for _ in 0..RUNS {
                let t = Instant::now();
                n = pipe_names(pipe, q, sort)?.len();
                v.push(ms(t));
            }
            let mut s = stats(v);
            s["rows"] = n.into();
            out.insert(format!("pipe_names_{q}_{sort}"), s);
        }
    }
    let faster = |a: &str, b: &str| out[a]["p50_ms"].as_f64() < out[b]["p50_ms"].as_f64();
    let mapped_wins = faster("mapped_예산", "pipe_예산") && faster("mapped_추진 계획", "pipe_추진 계획");
    out.insert(
        "selected".into(),
        (if mapped_wins { "mmap (본문·위계 기본), 파일명은 파이프" } else { "pipe" }).into(),
    );
    std::fs::write(output, Value::Object(out).to_string()).map_err(|e| e.to_string())?;
    if !same {
        return Err("mapped/pipe result order mismatch".into());
    }
    Ok(())
}
