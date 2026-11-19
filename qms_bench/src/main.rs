use qms_bench::*;
use serde_json::json;
use std::{env, fs, time::Duration};

fn main() {
    if let Err(e) = run() {
        eprintln!("{}", json!({"ok":false,"error":e}));
        std::process::exit(2)
    }
}
fn run() -> Result<(), String> {
    let mut a = env::args().skip(1);
    let Some(cmd) = a.next() else {
        return Err("usage: qms_bench <sample-queries|run|detect> [args]".into());
    };
    match cmd.as_str() {
        "sample-queries" => {
            let input = a.next().ok_or("missing filenames JSON")?;
            let seed = a
                .next()
                .ok_or("missing seed")?
                .parse::<u64>()
                .map_err(|e| e.to_string())?;
            let count = a
                .next()
                .ok_or("missing count")?
                .parse::<usize>()
                .map_err(|e| e.to_string())?;
            let names: Vec<String> =
                serde_json::from_slice(&fs::read(input).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            println!(
                "{}",
                serde_json::to_string(&sample_queries(&names, seed, count))
                    .map_err(|e| e.to_string())?
            );
        }
        "run" => {
            let set = a.next().ok_or("missing query set JSON path")?;
            let q = load_query_set(set)?;
            println!(
                "{}",
                json!({"ok":true,"command":"run","status":"skeleton","query_count":q.queries.len()})
            );
        }
        "detect" => println!(
            "{}",
            json!({"ok":true,"command":"detect","status":"skeleton","platform_adapters":[]})
        ),
        _ => return Err(format!("unknown subcommand: {cmd}")),
    }
    Ok(())
}
#[allow(dead_code)]
fn _duration(ms: u64) -> Duration {
    Duration::from_millis(ms)
}
