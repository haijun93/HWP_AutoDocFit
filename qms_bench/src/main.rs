use qms_bench::*;
use serde_json::json;
use std::{env, fs, path::PathBuf, time::Duration};

fn main() {
    if let Err(e) = run() {
        eprintln!("{}", json!({"ok":false,"error":e}));
        std::process::exit(2)
    }
}

fn run() -> Result<(), String> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        return Err("usage: qms_bench <sample-queries|run|detect> [args]".into());
    }
    let cmd = args.remove(0);
    match cmd.as_str() {
        "sample-queries" => {
            if args.len() < 3 {
                return Err(
                    "usage: qms_bench sample-queries <filenames.json> <seed> <count>".into(),
                );
            }
            let input = &args[0];
            let seed = args[1].parse::<u64>().map_err(|e| e.to_string())?;
            let count = args[2].parse::<usize>().map_err(|e| e.to_string())?;
            let names: Vec<String> =
                serde_json::from_slice(&fs::read(input).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            println!(
                "{}",
                serde_json::to_string(&sample_queries(&names, seed, count))
                    .map_err(|e| e.to_string())?
            );
        }
        "detect" => {
            let rep = detect_all();
            println!(
                "{}",
                serde_json::to_string(&rep).map_err(|e| e.to_string())?
            );
        }
        "run" => {
            run_benchmark(&args)?;
        }
        _ => return Err(format!("unknown subcommand: {cmd}")),
    }
    Ok(())
}

fn run_benchmark(args: &[String]) -> Result<(), String> {
    let mut adapter_type = "fake".to_string();
    let mut query_set_path: Option<String> = None;
    let mut out_dir = ".".to_string();
    let mut repeats = 10usize;
    let mut warmups = 2usize;
    let mut cold_repeats = 0usize;
    let mut top_n = 1000usize;
    let mut timeout_ms = 10_000u64;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--adapter" => {
                i += 1;
                if i < args.len() {
                    adapter_type = args[i].clone();
                }
            }
            "--query-set" | "-q" => {
                i += 1;
                if i < args.len() {
                    query_set_path = Some(args[i].clone());
                }
            }
            "--out-dir" | "-o" => {
                i += 1;
                if i < args.len() {
                    out_dir = args[i].clone();
                }
            }
            "--repeats" | "-r" => {
                i += 1;
                if i < args.len() {
                    repeats = args[i].parse().unwrap_or(repeats);
                }
            }
            "--warmups" | "-w" => {
                i += 1;
                if i < args.len() {
                    warmups = args[i].parse().unwrap_or(warmups);
                }
            }
            "--cold-repeats" => {
                i += 1;
                if i < args.len() {
                    cold_repeats = args[i].parse().unwrap_or(cold_repeats);
                }
            }
            "--top-n" | "-n" => {
                i += 1;
                if i < args.len() {
                    top_n = args[i].parse().unwrap_or(top_n);
                }
            }
            "--time-limit-ms" | "-t" => {
                i += 1;
                if i < args.len() {
                    timeout_ms = args[i].parse().unwrap_or(timeout_ms);
                }
            }
            arg if !arg.starts_with('-') && query_set_path.is_none() => {
                query_set_path = Some(arg.to_string());
            }
            _ => {}
        }
        i += 1;
    }

    let query_set_path =
        query_set_path.ok_or("missing query set JSON path (--query-set <path>)")?;
    let query_set = load_query_set(&query_set_path)?;

    // 어댑터 생성
    let mut adapter_boxed: Box<dyn QueryAdapter> = match adapter_type.to_lowercase().as_str() {
        "kfind" => Box::new(KfindPipeAdapter::default()),
        "everything" => Box::new(EverythingIpcAdapter::new()?),
        "fake" => Box::new(FakeAdapter {
            name: "fake".into(),
            results: vec!["file1.pdf".into(), "file2.pdf".into()],
            latency_ms: 1.5,
            error: None,
            timed_out: false,
        }),
        other => {
            return Err(format!(
                "unknown adapter type: {other} (choose: kfind, everything, fake)"
            ))
        }
    };

    let start_time = std::time::SystemTime::now();
    let run_id = format!(
        "run-{}-{}",
        adapter_type,
        start_time
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    );

    println!(
        "{}",
        json!({
            "event": "start",
            "run_id": run_id,
            "adapter": adapter_boxed.name(),
            "query_count": query_set.queries.len(),
            "warmups": warmups,
            "repeats": repeats,
            "top_n": top_n
        })
    );

    // 프로세스 자원 카운터 준비
    let mut res_counter = match adapter_type.to_lowercase().as_str() {
        "kfind" => WindowsProcessCounter::for_process_names(&["kfind.exe"]).ok(),
        "everything" => WindowsProcessCounter::for_process_names(&["Everything.exe"]).ok(),
        _ => None,
    };

    let mut pre_samples = Vec::new();
    if let Some(counter) = res_counter.as_mut() {
        if let Ok(sample) = counter.read() {
            pre_samples.push(sample);
        }
    }

    let time_limit = Duration::from_millis(timeout_ms);
    let mut observations = Vec::new();
    let mut stat_rows = Vec::new();
    let mut queries_json = Vec::new();

    for q in &query_set.queries {
        let q_digest = digest(q, false);
        let mut query_latencies = Vec::new();
        let mut query_timeouts = 0usize;
        let mut query_errors = 0usize;
        let mut last_result_count = 0u64;

        for phase in &["warmup", "warm", "cold"] {
            let count = match *phase {
                "warmup" => warmups,
                "warm" => repeats,
                "cold" => cold_repeats,
                _ => 0,
            };

            for repeat_idx in 0..count {
                let call_start = std::time::Instant::now();
                let res = adapter_boxed.query(q, top_n, time_limit);
                let elapsed_ms = call_start.elapsed().as_secs_f64() * 1000.0;
                let actual_latency = elapsed_ms.max(res.latency_ms);

                let status = if res.timed_out {
                    query_timeouts += 1;
                    "timeout"
                } else if res.error.is_some() {
                    query_errors += 1;
                    "error"
                } else {
                    last_result_count = res.result_count;
                    if *phase == "warm" {
                        query_latencies.push(actual_latency);
                    }
                    "ok"
                };

                let obs = Observation {
                    latency_ms: actual_latency,
                    result_count: res.result_count,
                    top_ids: res.top_ids.clone(),
                    status: status.into(),
                    error: res.error.clone(),
                    phase: phase.to_string(),
                    repeat: repeat_idx + 1,
                };

                println!(
                    "{}",
                    json!({
                        "event": "progress",
                        "phase": phase,
                        "query_id": q.id,
                        "repeat": repeat_idx + 1,
                        "latency_ms": actual_latency,
                        "result_count": res.result_count,
                        "status": status,
                        "error": res.error
                    })
                );

                observations.push(obs);
            }
        }

        let st = statistics(&query_latencies);
        stat_rows.push((
            q_digest.clone(),
            st.clone(),
            last_result_count,
            query_timeouts,
            query_errors,
        ));

        queries_json.push(json!({
            "digest": q_digest,
            "statistics": st,
            "result_count": last_result_count,
            "timeouts": query_timeouts,
            "errors": query_errors
        }));
    }

    let mut post_samples = Vec::new();
    if let Some(counter) = res_counter.as_mut() {
        if let Ok(sample) = counter.read() {
            post_samples.push(sample);
        }
    }

    let resource_summary = summarize_samples(&post_samples, false);
    let csv_content = csv_summary(&run_id, &stat_rows);

    let result_json = json!({
        "schema_version": 1,
        "run": {
            "run_id": run_id,
            "adapter": adapter_boxed.name(),
            "query_set_version": query_set.set_version,
            "warmups": warmups,
            "repeats": repeats,
            "top_n": top_n,
            "total_queries": query_set.queries.len()
        },
        "queries": queries_json,
        "resources": resource_summary,
        "summary": {
            "observations_count": observations.len(),
            "completed": true
        }
    });

    let out_dir_path = PathBuf::from(out_dir);
    fs::create_dir_all(&out_dir_path).map_err(|e| e.to_string())?;

    let json_filename = format!("{run_id}.json");
    let csv_filename = format!("{run_id}.csv");
    let json_path = out_dir_path.join(&json_filename);
    let csv_path = out_dir_path.join(&csv_filename);

    write_results(&json_path, &csv_path, &result_json, &csv_content)?;

    println!(
        "{}",
        json!({
            "event": "complete",
            "ok": true,
            "run_id": run_id,
            "json_path": json_path.to_string_lossy(),
            "csv_path": csv_path.to_string_lossy()
        })
    );

    Ok(())
}
