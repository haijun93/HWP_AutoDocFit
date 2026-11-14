//! MG2: 백엔드 한 질의의 단계별 비용(p50/p95, 30회). 합성 색인 폴더와 문단 표만 읽는다.
//! backend_bench INDEX_DIR [CONTENT_DB]
use ktm::backend::Backend;
use std::time::Instant;

fn stats(mut v: Vec<f64>) -> (f64, f64) {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (v[v.len() / 2], v[(v.len() * 95 / 100).min(v.len() - 1)])
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let root = std::path::PathBuf::from(args.get(1).ok_or("backend_bench INDEX_DIR [CONTENT_DB]")?);
    let db = args.get(2).map(std::path::PathBuf::from);
    let t = Instant::now();
    let mut b = Backend::open(&root)?.with_paragraphs(db.clone());
    let open_ms = t.elapsed().as_secs_f64() * 1000.0;
    let t = Instant::now();
    b.warm()?;
    let warm_ms = t.elapsed().as_secs_f64() * 1000.0;
    let mut out = serde_json::Map::new();
    out.insert("open_ms".into(), open_ms.into());
    out.insert("warm_ms".into(), warm_ms.into());
    for (label, q, hier) in [("body_예산", "예산", false), ("body_추진계획", "추진 계획", false), ("hier_예산", "예산", true), ("hier_추진계획", "추진 계획", true), ("one_char_예", "예", false)] {
        let mut total = Vec::new();
        for _ in 0..30 {
            let t = Instant::now();
            let a = b.search_cancel(q, hier, None)?;
            total.push(t.elapsed().as_secs_f64() * 1000.0);
            std::hint::black_box(a);
        }
        let (p50, p95) = stats(total);
        out.insert(label.into(), serde_json::json!({"p50_ms":p50,"p95_ms":p95}));
    }
    // 원문 없는 같은 질의(문단 표 비용 분리).
    let mut plain = Backend::open(&root)?;
    plain.warm()?;
    let mut v = Vec::new();
    for _ in 0..30 {
        let t = Instant::now();
        std::hint::black_box(plain.search_cancel("예산", false, None)?);
        v.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let (p50, p95) = stats(v);
    out.insert("body_예산_no_snippets".into(), serde_json::json!({"p50_ms":p50,"p95_ms":p95}));
    println!("{}", serde_json::Value::Object(out));
    Ok(())
}
