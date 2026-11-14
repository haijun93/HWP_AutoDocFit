//! HWP 5.0 Rust 직접 추출기(`hwp5`) vs kordoc(Node.js) 대조 벤치마크 및 정합성 검증 도구.
//!
//! 사용법:
//!   cargo run --example hwp5_vs_kordoc -- <HWP_폴더_또는_파일경로>
//!
//! 인자가 없으면 현재 디렉토리 및 상위 디렉토리의 .hwp 파일을 자동으로 탐색합니다.
//! 각 문서에 대해:
//! 1. Rust 직접 파서(`extract_hwp5`) 소요 시간 및 문단 수
//! 2. kordoc 런타임(`extract_hwp_kordoc`) 소요 시간 및 문단 수
//! 3. 문단 텍스트 일치율 및 불일치 문단 수
//! 4. 결과 요약 테이블(Markdown 표)을 출력합니다.
//!
//! 라이선스: MIT

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use klib::extract::extract_hwp_kordoc;
use klib::hwp5::extract_hwp5;

#[derive(Default)]
struct SummaryStats {
    total_files: usize,
    both_succeeded: usize,
    rust_only: usize,
    kordoc_only: usize,
    both_failed: usize,
    perfect_match_count: usize,
    total_rust_duration_ms: f64,
    total_kordoc_duration_ms: f64,
}

fn collect_hwp_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_hwp_files(&path, out);
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext.eq_ignore_ascii_case("hwp") {
                        out.push(path);
                    }
                }
            }
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let target_path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        // 기본값: 현재 폴더 또는 상위 테스트 디렉토리
        PathBuf::from(".")
    };

    println!("================================================================================");
    println!("  HWP 5.0 Rust 직접 파서 vs kordoc(Node.js) 정합성 및 성능 대조 도구");
    println!("  대상 경로: {}", target_path.display());
    println!("================================================================================");

    let mut hwp_files = Vec::new();
    if target_path.is_file() {
        hwp_files.push(target_path.clone());
    } else if target_path.is_dir() {
        collect_hwp_files(&target_path, &mut hwp_files);
    } else {
        eprintln!("오류: 지정된 경로를 찾을 수 없습니다: {}", target_path.display());
        return;
    }

    if hwp_files.is_empty() {
        println!("지정된 경로에 .hwp 파일이 없습니다.");
        return;
    }

    println!("발견된 .hwp 문서 수: {}개\n", hwp_files.len());

    println!("| 번호 | 파일명 | Rust 문단 | kordoc 문단 | 불일치 | Rust 시간 | kordoc 시간 | 배속 | 상태 |");
    println!("|---|---|---:|---:|---:|---:|---:|---:|---|");

    let mut stats = SummaryStats {
        total_files: hwp_files.len(),
        ..Default::default()
    };

    for (idx, path) in hwp_files.iter().enumerate() {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");

        // 1. Rust 직접 파서 실행 및 시간 측정
        let t0 = Instant::now();
        let res_rust = extract_hwp5(path);
        let rust_elapsed = t0.elapsed();
        let rust_ms = rust_elapsed.as_secs_f64() * 1000.0;

        // 2. kordoc 런타임 실행 및 시간 측정
        let t1 = Instant::now();
        let res_kordoc = extract_hwp_kordoc(path);
        let kordoc_elapsed = t1.elapsed();
        let kordoc_ms = kordoc_elapsed.as_secs_f64() * 1000.0;

        match (res_rust, res_kordoc) {
            (Ok(rust_paras), Ok(kordoc_paras)) => {
                stats.both_succeeded += 1;
                stats.total_rust_duration_ms += rust_ms;
                stats.total_kordoc_duration_ms += kordoc_ms;

                let r_len = rust_paras.len();
                let k_len = kordoc_paras.len();

                // 문단 텍스트 불일치 수 계산
                let min_len = r_len.min(k_len);
                let mut mismatch = 0usize;
                for i in 0..min_len {
                    if rust_paras[i].text.trim() != kordoc_paras[i].text.trim() {
                        mismatch += 1;
                    }
                }
                mismatch += r_len.abs_diff(k_len);

                if mismatch == 0 {
                    stats.perfect_match_count += 1;
                }

                let speedup = if rust_ms > 0.0 {
                    format!("{:.1}x", kordoc_ms / rust_ms)
                } else {
                    "-".into()
                };

                let status = if mismatch == 0 {
                    "✅ 완전일치"
                } else {
                    "⚠️ 일부차이"
                };

                println!(
                    "| {:>2} | {:<25} | {:>9} | {:>11} | {:>6} | {:>7.2}ms | {:>9.2}ms | {:>6} | {} |",
                    idx + 1,
                    file_name,
                    r_len,
                    k_len,
                    mismatch,
                    rust_ms,
                    kordoc_ms,
                    speedup,
                    status
                );
            }
            (Ok(rust_paras), Err(e_kordoc)) => {
                stats.rust_only += 1;
                stats.total_rust_duration_ms += rust_ms;
                let err_brief = e_kordoc.split(':').next().unwrap_or(&e_kordoc);
                println!(
                    "| {:>2} | {:<25} | {:>9} | {:>11} | {:>6} | {:>7.2}ms | {:>11} | {:>6} | 🔵 Rust단독성공 |",
                    idx + 1,
                    file_name,
                    rust_paras.len(),
                    "-",
                    "-",
                    rust_ms,
                    err_brief,
                    "-"
                );
            }
            (Err(e_rust), Ok(kordoc_paras)) => {
                stats.kordoc_only += 1;
                stats.total_kordoc_duration_ms += kordoc_ms;
                let err_brief = e_rust.split(':').next().unwrap_or(&e_rust);
                println!(
                    "| {:>2} | {:<25} | {:>9} | {:>11} | {:>6} | {:>9} | {:>9.2}ms | {:>6} | 🟡 kordoc단독성공 ({}) |",
                    idx + 1,
                    file_name,
                    "-",
                    kordoc_paras.len(),
                    "-",
                    err_brief,
                    kordoc_ms,
                    "-",
                    err_brief
                );
            }
            (Err(e_rust), Err(e_kordoc)) => {
                stats.both_failed += 1;
                let r_brief = e_rust.split(':').next().unwrap_or(&e_rust);
                let k_brief = e_kordoc.split(':').next().unwrap_or(&e_kordoc);
                println!(
                    "| {:>2} | {:<25} | {:>9} | {:>11} | {:>6} | {:>9} | {:>11} | {:>6} | ❌ 양쪽실패 ({}/{}) |",
                    idx + 1,
                    file_name,
                    "-",
                    "-",
                    "-",
                    r_brief,
                    k_brief,
                    "-",
                    r_brief,
                    k_brief
                );
            }
        }
    }

    println!("\n================================================================================");
    println!("  종합 요약 보고서");
    println!("================================================================================");
    println!("- 총 검증 대상 파일 수 : {}개", stats.total_files);
    println!("- 양쪽 모두 추출 성공  : {}개", stats.both_succeeded);
    println!("  * 텍스트 완전 일치   : {}개", stats.perfect_match_count);
    println!("- Rust 단독 성공       : {}개", stats.rust_only);
    println!("- kordoc 단독 성공     : {}개 (암호화/배포용 등 폴백 대상)", stats.kordoc_only);
    println!("- 양쪽 모두 실패       : {}개", stats.both_failed);

    if stats.both_succeeded > 0 {
        let avg_rust = stats.total_rust_duration_ms / stats.both_succeeded as f64;
        let avg_kordoc = stats.total_kordoc_duration_ms / stats.both_succeeded as f64;
        let total_speedup = if stats.total_rust_duration_ms > 0.0 {
            stats.total_kordoc_duration_ms / stats.total_rust_duration_ms
        } else {
            0.0
        };
        println!("- 평균 소요 시간 (성공 문서 기준):");
        println!("  * Rust 직접 파서   : {:.2} ms / 문서", avg_rust);
        println!("  * kordoc (Node.js) : {:.2} ms / 문서", avg_kordoc);
        println!("  * 전체 속도 개선율 : 약 {:.1} 배 향상", total_speedup);
    }
    println!("================================================================================");
}
