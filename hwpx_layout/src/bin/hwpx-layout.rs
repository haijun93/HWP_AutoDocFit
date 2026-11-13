//! hwpx-layout <파일.hwpx> [--summary]
//! 한/글이 저장한 HWPX의 줄 배치를 JSON으로 낸다. 읽기 시간(초)은 stderr에 적는다.
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.iter().find(|a| !a.starts_with("--")).map(PathBuf::from) else {
        eprintln!("사용: hwpx-layout <파일.hwpx> [--summary]");
        std::process::exit(2);
    };
    let t = Instant::now();
    let layout = match hwpx_layout::read_layout(&path) {
        Ok(l) => l,
        Err(e) => { eprintln!("오류: {e}"); std::process::exit(1); }
    };
    let secs = t.elapsed().as_secs_f64();
    if args.iter().any(|a| a == "--summary") {
        let lines: usize = layout.paras.iter().map(|p| p.lines.len()).sum();
        println!("{}", serde_json::json!({"paragraphs": layout.paras.len(), "lines": lines, "pages": layout.pages, "missing_lineseg": layout.missing, "read_seconds": secs}));
    } else {
        println!("{}", hwpx_layout::to_json(&layout));
    }
    eprintln!("read_seconds={secs:.6}");
}
