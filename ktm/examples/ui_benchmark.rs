//! G-KTMUI 키워드 검색 최적화 UI 성능 및 자원 측정 자리
//! 입력 → 첫 그리기 ms 및 쉴 때 메모리 계측 (실행은 총괄)
//! 사용법: cargo run --release --example ui_benchmark -- [OUTPUT_PATH]

use std::path::PathBuf;
use std::time::Instant;

fn get_private_working_set_kb() -> Option<usize> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;

        let mut pmc: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        pmc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        if GetProcessMemoryInfo(
            GetCurrentProcess(),
            (&mut pmc as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
            pmc.cb,
        ) != 0
        {
            return Some(pmc.PrivateUsage / 1024);
        }
    }
    None
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let output_path = args.get(1).map(PathBuf::from);

    println!("=== G-KTMUI UI 성능 및 자원 측정 자리 ===");

    // 1. 순수 레이아웃 및 묶음별 강조 구간 계산 벤치마크 (입력 -> 프레임 구성)
    let queries = ["예산", "2026; 공사 계약", "보고서; 계획; 지출; 검토"];
    let sample_text = "2026년도 상반기 공사 계약 집행 예산 보고서 및 세부 추진 계획";

    println!("[1] 묶음별 강조 구간 및 줄 배치 계산 (입력 -> 첫 그리기 준비)");
    for q in queries {
        let start = Instant::now();
        let groups = ktm::highlight::parse_groups(q);
        let ranges = ktm::highlight::ranges_by_groups(sample_text, &groups);
        let elapsed = start.elapsed();
        println!(
            "  - 질의 '{}': 묶음 {}개, 강조 구간 {}개, 소요: {:.3}µs ({:.4}ms)",
            q,
            groups.len(),
            ranges.len(),
            elapsed.as_micros(),
            elapsed.as_secs_f64() * 1000.0
        );
    }

    // 2. 쉴 때 메모리 측정 (작업 집합)
    println!("\n[2] 메모리 점유 측정");
    if let Some(kb) = get_private_working_set_kb() {
        let mb = kb as f64 / 1024.0;
        println!("  - 현재 프로세스 Private Working Set: {:.2} MB ({} KB)", mb, kb);
        println!("  - 목표: 실사용 메모리 5MB 이하 (만족 여부: {})", if mb <= 5.0 { "PASS" } else { "CHECK" });
    } else {
        println!("  - (Windows 환경 외 또는 카운터 조회 미지원: 기본 측정 생략)");
    }

    if let Some(out) = output_path {
        let json_data = serde_json::json!({
            "target": "G-KTMUI",
            "private_working_set_kb": get_private_working_set_kb(),
            "target_memory_mb": 5.0,
            "target_input_to_paint_ms": 16.0,
            "status": "ready_for_coordinator_evaluation"
        });
        std::fs::write(&out, serde_json::to_string_pretty(&json_data)?)?;
        println!("\n측정 결과 파일 저장 완료: {:?}", out);
    }

    Ok(())
}
