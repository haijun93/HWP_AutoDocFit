use hwpx_fmt::config::{Config, UNSUPPORTED};
use hwpx_fmt::{process_with, selections_from_json, selections_to_json, survey_with, Processor};
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "사용법:\n  hwpx_fmt <처리기> <입력.hwpx> <출력.hwpx> [--selections 파일.json] [--config 설정.json]   처리\n  hwpx_fmt survey <처리기> <입력.hwpx> [--config 설정.json]                                  조사 모드(대상 목록 JSON 출력)\n처리기: date | twochar | overview_lead | page_number | spacing_reset\n설정(JSON): page_number, title_owner_text, indent_rules, marker_scheme_active (알 수 없는 키·반영 못 하는 값은 '미지원 입력'으로 거부, 종료 코드 4)\n결과는 표준출력 JSON 한 줄. 입력 파일은 바꾸지 않는다.";

fn run(args: &[String]) -> Result<String, String> {
    // 위치 인자와 --selections/--config 분리
    let (mut pos, mut selections, mut config) = (vec![], None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--selections" => selections = Some(it.next().ok_or(USAGE)?.clone()),
            "--config" => config = Some(it.next().ok_or(USAGE)?.clone()),
            _ => pos.push(a.clone()),
        }
    }
    let cfg = match &config {
        Some(p) => Config::from_path(Path::new(p))?,
        None => Config::default(),
    };
    match pos.first().map(String::as_str) {
        Some("survey") if pos.len() == 3 && selections.is_none() => {
            let p = Processor::parse(&pos[1]).ok_or("알 수 없는 처리기")?;
            Ok(selections_to_json(&survey_with(p, Path::new(&pos[2]), &cfg)?).to_string())
        }
        Some(name) if pos.len() == 3 => {
            let p = Processor::parse(name).ok_or("알 수 없는 처리기")?;
            let sel = match &selections {
                Some(f) => {
                    let text = std::fs::read_to_string(f).map_err(|e| e.to_string())?;
                    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                    Some(selections_from_json(&v)?)
                }
                None => None,
            };
            let n = process_with(p, Path::new(&pos[1]), Path::new(&pos[2]), sel.as_ref(), &cfg)?;
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
            ExitCode::from(if e.starts_with(UNSUPPORTED) { 4 } else { 2 })
        }
    }
}
