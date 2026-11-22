use hwpx_fmt::{process, selections_from_json, selections_to_json, survey, Processor};
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "사용법:\n  hwpx_fmt <처리기> <입력.hwpx> <출력.hwpx> [--selections 파일.json]   처리(처리기: date | twochar)\n  hwpx_fmt survey <처리기> <입력.hwpx>                                   조사 모드(대상 목록 JSON 출력)\n결과는 표준출력 JSON 한 줄. 입력 파일은 바꾸지 않는다.";

fn run(args: &[String]) -> Result<String, String> {
    match args.first().map(String::as_str) {
        Some("survey") if args.len() == 3 => {
            let p = Processor::parse(&args[1]).ok_or("알 수 없는 처리기")?;
            Ok(selections_to_json(&survey(p, Path::new(&args[2]))?).to_string())
        }
        Some(name) if args.len() == 3 || (args.len() == 5 && args[3] == "--selections") => {
            let p = Processor::parse(name).ok_or("알 수 없는 처리기")?;
            let sel = if args.len() == 5 {
                let text = std::fs::read_to_string(&args[4]).map_err(|e| e.to_string())?;
                let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                Some(selections_from_json(&v)?)
            } else {
                None
            };
            let n = process(p, Path::new(&args[1]), Path::new(&args[2]), sel.as_ref())?;
            Ok(serde_json::json!({ "count": n }).to_string())
        }
        _ => Err(USAGE.into()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(s) => {
            println!("{s}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
    }
}
