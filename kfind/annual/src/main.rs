use kfind_annual::{
    output::{AnnualInput, write_annual_bundle, write_outputs},
    synthetic::generate_synthetic_annual_artifacts,
};
use std::{env, fs, process::ExitCode};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("사용법:");
        eprintln!("  annual report <입력 JSON> <출력 폴더>");
        eprintln!("  annual bundle <입력 JSON> <출력 폴더> (표·XLSX 포함)");
        eprintln!("  annual synthetic <출력 폴더> [시드]");
        return Err("명령 인자 부족".into());
    }

    match args[1].as_str() {
        "report" | "bundle" => {
            if args.len() != 4 {
                eprintln!("사용법: annual report <입력 JSON> <출력 폴더>");
                return Err("잘못된 인자".into());
            }
            let json = fs::read_to_string(&args[2])?;
            let input: AnnualInput = serde_json::from_str(&json)?;
            if args[1] == "bundle" {
                write_annual_bundle(&input, &args[3])?;
            } else {
                write_outputs(&input, &args[3])?;
            }
            println!("연보 출력 완료: {}", args[3]);
            Ok(())
        }
        "synthetic" => {
            if args.len() < 3 || args.len() > 4 {
                eprintln!("사용법: annual synthetic <출력 폴더> [시드]");
                return Err("잘못된 인자".into());
            }
            let seed = if args.len() == 4 {
                args[3].parse::<u64>()?
            } else {
                kfind_annual::synthetic::DEFAULT_SEED
            };
            generate_synthetic_annual_artifacts(&args[2], seed)?;
            println!("합성 연보 산출물 3종 생성 완료 (시드: {seed}): {}", args[2]);
            Ok(())
        }
        _ => {
            eprintln!("알 수 없는 하위 명령: {}", args[1]);
            eprintln!("사용 가능한 명령: report, bundle, synthetic");
            Err("알 수 없는 명령".into())
        }
    }
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
