use kfind_annual::output::{AnnualInput, write_outputs};
use std::{env, fs, process::ExitCode};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 || args[1] != "report" {
        eprintln!("사용법: annual report <입력 JSON> <출력 폴더>");
        return Err("잘못된 인자".into());
    }
    let json = fs::read_to_string(&args[2])?;
    let input: AnnualInput = serde_json::from_str(&json)?;
    write_outputs(&input, &args[3])?;
    println!("연보 출력 완료: {}", args[3]);
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("오류: {e}");
            ExitCode::FAILURE
        }
    }
}
