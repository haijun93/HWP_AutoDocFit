//! HWP 5.0 Rust 직접 추출기(`hwp5`) vs kordoc(Node.js) 대조 벤치마크 및 정합성 검증 도구.
//!
//! 비교 기준 (HWP5-CMP2):
//! 1. 공백 제거 정규화 전체 글 비교 (문자 일치율, Character Match Rate)
//! 2. 2글자 조각 집합 일치율 (검색 결과 동일성, Bigram Set Similarity / Jaccard & Dice)
//! 3. kordoc 대비 빠진 글 상위 5곳 스니펫 출력
//! 4. 표 셀·머리말/꼬리말·각주 처리 차이 분류 (kordoc의 마크다운 표 행 결합 등)
//!
//! 사용법:
//!   cargo run --example hwp5_vs_kordoc -- <HWP_폴더_또는_파일경로>
//!
//! 라이선스: MIT (로컬 절대 경로 하드코딩 금지)

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use klib::extract::{extract_hwp_kordoc, Paragraph};
use klib::hwp5::extract_hwp5;

/// 문서별 문단 분류 통계
#[derive(Debug, Default, Clone)]
struct ParagraphBreakdown {
    total: usize,
    body: usize,
    table_cell: usize,
    header: usize,
    footer: usize,
    footnote: usize,
    endnote: usize,
    textbox: usize,
}

impl ParagraphBreakdown {
    fn from_paragraphs(paras: &[Paragraph]) -> Self {
        let mut b = Self {
            total: paras.len(),
            ..Default::default()
        };
        for p in paras {
            if p.is_table_cell || p.location_kind == "table_cell" {
                b.table_cell += 1;
            } else {
                match p.location_kind.as_str() {
                    "header" => b.header += 1,
                    "footer" => b.footer += 1,
                    "footnote" => b.footnote += 1,
                    "endnote" => b.endnote += 1,
                    "textbox" => b.textbox += 1,
                    _ => b.body += 1,
                }
            }
        }
        b
    }
}

/// 종합 통계 집계 구조체
#[derive(Default)]
struct SummaryStats {
    total_files: usize,
    both_succeeded: usize,
    rust_only: usize,
    kordoc_only: usize,
    both_failed: usize,
    high_accuracy_count: usize, // 조각 일치율 99.5% 이상 달성 건수
    total_rust_duration_ms: f64,
    total_kordoc_duration_ms: f64,
    sum_char_match_pct: f64,
    sum_bigram_jaccard_pct: f64,
    sum_bigram_dice_pct: f64,
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

/// kordoc 마크다운에서 표 서식 및 주석 문법을 정규화하여 순수 본문 텍스트를 복원한다.
fn clean_kordoc_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let trimmed = line.trim();

        // 1. HTML/마크다운 주석 (<!-- page: 1 --> 등) 제외
        if trimmed.starts_with("<!--") && trimmed.ends_with("-->") {
            continue;
        }

        // 2. 마크다운 표 구분선 행 (예: |---|---| or |:---:|---:|) 제외
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            let inner = &trimmed[1..trimmed.len() - 1];
            let is_divider = inner.split('|').all(|cell| {
                let c = cell.trim();
                !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':' || ch.is_whitespace())
            });
            if is_divider {
                continue;
            }
        }

        // 3. 표 셀 구분 파이프 '|'는 공백으로 치환하여 단어가 붙지 않도록 분리
        let mut cleaned_line = String::with_capacity(line.len());
        for ch in line.chars() {
            if ch == '|' {
                cleaned_line.push(' ');
            } else {
                cleaned_line.push(ch);
            }
        }

        out.push_str(&cleaned_line);
        out.push('\n');
    }
    out
}

/// 공백을 제거한 순수 정규화 텍스트 생성
fn normalize_remove_whitespaces(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// ① 공백 제거 정규화 전체 글 비교 (문자 일치율, Character Multiset Overlap)
fn compute_char_match_rate(norm_rust: &str, norm_kordoc: &str) -> f64 {
    if norm_rust.is_empty() && norm_kordoc.is_empty() {
        return 100.0;
    }
    if norm_rust.is_empty() || norm_kordoc.is_empty() {
        return 0.0;
    }

    let mut hist_r: HashMap<char, usize> = HashMap::new();
    for c in norm_rust.chars() {
        *hist_r.entry(c).or_insert(0) += 1;
    }

    let mut hist_k: HashMap<char, usize> = HashMap::new();
    for c in norm_kordoc.chars() {
        *hist_k.entry(c).or_insert(0) += 1;
    }

    let mut overlap = 0usize;
    for (c, cnt_k) in &hist_k {
        if let Some(cnt_r) = hist_r.get(c) {
            overlap += (*cnt_k).min(*cnt_r);
        }
    }

    let max_len = norm_rust.chars().count().max(norm_kordoc.chars().count());
    if max_len == 0 {
        100.0
    } else {
        (overlap as f64 / max_len as f64) * 100.0
    }
}

/// 2글자 조각(Bigram) 집합 추출 (공백 제외 단락 내 연속 2글자)
fn extract_bigram_set(paras: &[Paragraph]) -> HashSet<[char; 2]> {
    let mut set = HashSet::new();
    for p in paras {
        let chars: Vec<char> = p.text.chars().filter(|c| !c.is_whitespace()).collect();
        if chars.len() >= 2 {
            for i in 0..chars.len() - 1 {
                set.insert([chars[i], chars[i + 1]]);
            }
        }
    }
    set
}

/// kordoc 텍스트로부터 2글자 조각 집합 추출
fn extract_bigram_set_from_raw_text(text: &str) -> HashSet<[char; 2]> {
    let mut set = HashSet::new();
    for line in text.lines() {
        let chars: Vec<char> = line.chars().filter(|c| !c.is_whitespace()).collect();
        if chars.len() >= 2 {
            for i in 0..chars.len() - 1 {
                set.insert([chars[i], chars[i + 1]]);
            }
        }
    }
    set
}

/// ② 2글자 조각 집합 일치율 계산 (Jaccard, Dice, Recall)
struct BigramSimResult {
    jaccard_pct: f64,
    dice_pct: f64,
    recall_pct: f64,
    total_rust: usize,
    total_kordoc: usize,
    intersect: usize,
    missing_in_rust: HashSet<[char; 2]>,
    extra_in_rust: HashSet<[char; 2]>,
}

fn compute_bigram_similarity(
    set_rust: &HashSet<[char; 2]>,
    set_kordoc: &HashSet<[char; 2]>,
) -> BigramSimResult {
    if set_rust.is_empty() && set_kordoc.is_empty() {
        return BigramSimResult {
            jaccard_pct: 100.0,
            dice_pct: 100.0,
            recall_pct: 100.0,
            total_rust: 0,
            total_kordoc: 0,
            intersect: 0,
            missing_in_rust: HashSet::new(),
            extra_in_rust: HashSet::new(),
        };
    }

    let intersect_count = set_rust.intersection(set_kordoc).count();
    let union_count = set_rust.union(set_kordoc).count();

    let jaccard_pct = if union_count > 0 {
        (intersect_count as f64 / union_count as f64) * 100.0
    } else {
        100.0
    };

    let total_sum = set_rust.len() + set_kordoc.len();
    let dice_pct = if total_sum > 0 {
        (2.0 * intersect_count as f64 / total_sum as f64) * 100.0
    } else {
        100.0
    };

    let recall_pct = if !set_kordoc.is_empty() {
        (intersect_count as f64 / set_kordoc.len() as f64) * 100.0
    } else {
        100.0
    };

    let missing_in_rust: HashSet<[char; 2]> =
        set_kordoc.difference(set_rust).copied().collect();
    let extra_in_rust: HashSet<[char; 2]> =
        set_rust.difference(set_kordoc).copied().collect();

    BigramSimResult {
        jaccard_pct,
        dice_pct,
        recall_pct,
        total_rust: set_rust.len(),
        total_kordoc: set_kordoc.len(),
        intersect: intersect_count,
        missing_in_rust,
        extra_in_rust,
    }
}

/// ③ kordoc 대비 빠진 글 상위 5곳 추출 (주변 문맥 포함 스니펫)
fn find_top_missing_snippets(
    kordoc_text: &str,
    missing_bigrams: &HashSet<[char; 2]>,
    max_count: usize,
) -> Vec<String> {
    if missing_bigrams.is_empty() {
        return Vec::new();
    }

    let mut snippets = Vec::new();
    let mut seen_snippets = HashSet::new();

    for line in kordoc_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let chars: Vec<char> = trimmed.chars().collect();
        let mut i = 0usize;
        while i < chars.len() {
            if i + 1 < chars.len() {
                let pair = [chars[i], chars[i + 1]];
                if missing_bigrams.contains(&pair) {
                    // 주변 문맥 추출 (앞뒤 최대 12글자)
                    let start = i.saturating_sub(12);
                    let end = (i + 15).min(chars.len());
                    let snippet: String = chars[start..end].iter().collect();
                    let formatted = format!(
                        "\"...{}...\" (조각: '{}{}')",
                        snippet.trim(),
                        pair[0],
                        pair[1]
                    );

                    if seen_snippets.insert(snippet) {
                        snippets.push(formatted);
                        if snippets.len() >= max_count {
                            return snippets;
                        }
                    }
                    i += 2;
                    continue;
                }
            }
            i += 1;
        }
    }

    snippets
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let target_path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from(".")
    };

    println!("================================================================================");
    println!("  HWP 5.0 Rust 직접 파서 vs kordoc(Node.js) 대조 도구 (HWP5-CMP2)");
    println!("  비교 기준: ① 공백제거 정규화 글 비교 ② 2글자 조각 집합 일치율 ③ 빠진글 상위 5곳");
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

    // 파일 이름순 정렬
    hwp_files.sort();

    println!("발견된 .hwp 문서 수: {}개\n", hwp_files.len());

    println!("| 번호 | 파일명 | 문자 일치율 | 조각 일치율 (Dice/Jaccard) | Rust 문단 (본문/표셀/기타) | kordoc 문단 | Rust 시간 | kordoc 시간 | 배속 | 판정 |");
    println!("|---|---|---:|---:|---:|---:|---:|---:|---:|---|");

    let mut stats = SummaryStats {
        total_files: hwp_files.len(),
        ..Default::default()
    };

    let mut detail_reports = Vec::new();

    for (idx, path) in hwp_files.iter().enumerate() {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        // 1. Rust 직접 파서 실행
        let t0 = Instant::now();
        let res_rust = extract_hwp5(path);
        let rust_elapsed = t0.elapsed();
        let rust_ms = rust_elapsed.as_secs_f64() * 1000.0;

        // 2. kordoc 런타임 실행
        let t1 = Instant::now();
        let res_kordoc = extract_hwp_kordoc(path);
        let kordoc_elapsed = t1.elapsed();
        let kordoc_ms = kordoc_elapsed.as_secs_f64() * 1000.0;

        match (res_rust, res_kordoc) {
            (Ok(rust_paras), Ok(kordoc_paras)) => {
                stats.both_succeeded += 1;
                stats.total_rust_duration_ms += rust_ms;
                stats.total_kordoc_duration_ms += kordoc_ms;

                let breakdown_r = ParagraphBreakdown::from_paragraphs(&rust_paras);

                // 전체 텍스트 병합 및 정규화
                let rust_full_raw: String = rust_paras
                    .iter()
                    .map(|p| p.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                let kordoc_full_raw: String = kordoc_paras
                    .iter()
                    .map(|p| p.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");

                let kordoc_cleaned = clean_kordoc_markdown(&kordoc_full_raw);

                let norm_rust = normalize_remove_whitespaces(&rust_full_raw);
                let norm_kordoc = normalize_remove_whitespaces(&kordoc_cleaned);

                // ① 공백 제거 정규화 전체 글 비교 (문자 일치율)
                let char_match_pct = compute_char_match_rate(&norm_rust, &norm_kordoc);
                stats.sum_char_match_pct += char_match_pct;

                // ② 2글자 조각 집합 일치율 (검색 결과 동일성)
                let set_rust = extract_bigram_set(&rust_paras);
                let set_kordoc = extract_bigram_set_from_raw_text(&kordoc_cleaned);
                let sim_res = compute_bigram_similarity(&set_rust, &set_kordoc);

                stats.sum_bigram_jaccard_pct += sim_res.jaccard_pct;
                stats.sum_bigram_dice_pct += sim_res.dice_pct;

                // 목표 기준: 조각 집합 일치율(Dice 또는 Recall) 99.5% 이상
                let is_pass = sim_res.dice_pct >= 99.5 || sim_res.recall_pct >= 99.5;
                if is_pass {
                    stats.high_accuracy_count += 1;
                }

                let speedup = if rust_ms > 0.0 {
                    format!("{:.1}x", kordoc_ms / rust_ms)
                } else {
                    "-".into()
                };

                let status = if is_pass {
                    "✅ 99.5%이상"
                } else {
                    "⚠️ 미달"
                };

                let rust_counts_str = format!(
                    "{} ({} / {} / {})",
                    breakdown_r.total,
                    breakdown_r.body,
                    breakdown_r.table_cell,
                    breakdown_r.header + breakdown_r.footer + breakdown_r.footnote + breakdown_r.endnote + breakdown_r.textbox
                );

                println!(
                    "| {:>2} | {:<25} | {:>10.2}% | {:>6.2}% / {:>5.2}% | {:>26} | {:>11} | {:>7.2}ms | {:>9.2}ms | {:>6} | {} |",
                    idx + 1,
                    file_name,
                    char_match_pct,
                    sim_res.dice_pct,
                    sim_res.jaccard_pct,
                    rust_counts_str,
                    kordoc_paras.len(),
                    rust_ms,
                    kordoc_ms,
                    speedup,
                    status
                );

                // ③ 빠진 글 상위 5곳 추출 및 상세 리포트 수집
                let missing_snippets = find_top_missing_snippets(
                    &kordoc_cleaned,
                    &sim_res.missing_in_rust,
                    5,
                );

                detail_reports.push((
                    idx + 1,
                    file_name.to_string(),
                    char_match_pct,
                    sim_res,
                    breakdown_r,
                    kordoc_paras.len(),
                    missing_snippets,
                ));
            }
            (Ok(rust_paras), Err(e_kordoc)) => {
                stats.rust_only += 1;
                stats.total_rust_duration_ms += rust_ms;
                let breakdown_r = ParagraphBreakdown::from_paragraphs(&rust_paras);
                let err_brief = e_kordoc.split(':').next().unwrap_or(&e_kordoc);
                println!(
                    "| {:>2} | {:<25} | {:>10} | {:>17} | {:>26} | {:>11} | {:>7.2}ms | {:>11} | {:>6} | 🔵 Rust단독성공 |",
                    idx + 1,
                    file_name,
                    "-",
                    "-",
                    format!("{} ({} / {} / -)", breakdown_r.total, breakdown_r.body, breakdown_r.table_cell),
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
                    "| {:>2} | {:<25} | {:>10} | {:>17} | {:>26} | {:>11} | {:>9} | {:>9.2}ms | {:>6} | 🟡 kordoc단독성공 ({}) |",
                    idx + 1,
                    file_name,
                    "-",
                    "-",
                    "-",
                    kordoc_paras.len(),
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
                    "| {:>2} | {:<25} | {:>10} | {:>17} | {:>26} | {:>11} | {:>9} | {:>11} | {:>6} | ❌ 양쪽실패 ({}/{}) |",
                    idx + 1,
                    file_name,
                    "-",
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

    // 문서별 상세 분석 (빠진 글 상위 5곳 & 문단 처리 차이 분류)
    if !detail_reports.is_empty() {
        println!("\n================================================================================");
        println!("  문서별 상세 대조 분석 및 빠진 글 상위 5곳 (HWP5-CMP2)");
        println!("================================================================================");
        for (num, fname, char_pct, sim, brk, k_len, snippets) in &detail_reports {
            let pass_mark = if sim.dice_pct >= 99.5 || sim.recall_pct >= 99.5 {
                "✅ 통과 (99.5% 이상)"
            } else {
                "⚠️ 미달"
            };

            println!("\n[{}] 파일명: {} ({})", num, fname, pass_mark);
            println!(
                "  * 문자 일치율: {:.2}% | 조각 Dice: {:.2}% | Jaccard: {:.2}% | kordoc 재현율(Recall): {:.2}%",
                char_pct, sim.dice_pct, sim.jaccard_pct, sim.recall_pct
            );
            println!(
                "  * 조각 수: Rust {}개 / kordoc {}개 (공통 교집합 {}개, Rust 미달 조각 {}개)",
                sim.total_rust,
                sim.total_kordoc,
                sim.intersect,
                sim.missing_in_rust.len()
            );
            println!(
                "  * 문단 구성 차이 분류: Rust 총 {}문단 [본문 {}, 표 셀 {}, 머리꼬리 {}, 각주미주 {}, 글상자 {}] vs kordoc {}행",
                brk.total, brk.body, brk.table_cell, brk.header + brk.footer, brk.footnote + brk.endnote, brk.textbox, k_len
            );
            if brk.table_cell > 0 {
                println!(
                    "    (참고: kordoc은 표 셀 {}개를 마크다운 표 행으로 결합하여 문단 수가 적음)",
                    brk.table_cell
                );
            }

            if snippets.is_empty() {
                println!("  * kordoc 대비 빠진 글: 없음 (조각 100% 포괄)");
            } else {
                println!("  * kordoc 대비 빠진 글 상위 {}곳:", snippets.len());
                for (s_idx, s) in snippets.iter().enumerate() {
                    println!("    {}. {}", s_idx + 1, s);
                }
            }

            if !sim.extra_in_rust.is_empty() {
                let sample_extra: Vec<String> = sim
                    .extra_in_rust
                    .iter()
                    .take(3)
                    .map(|pair| format!("'{}{}'", pair[0], pair[1]))
                    .collect();
                println!("  * Rust 추가 조각 예시: {}", sample_extra.join(", "));
            }
        }
    }

    println!("\n================================================================================");
    println!("  종합 요약 보고서 (HWP5-CMP2)");
    println!("================================================================================");
    println!("- 총 검증 대상 파일 수       : {}개", stats.total_files);
    println!("- 양쪽 모두 추출 성공        : {}개", stats.both_succeeded);
    if stats.both_succeeded > 0 {
        let pass_rate = (stats.high_accuracy_count as f64 / stats.both_succeeded as f64) * 100.0;
        let avg_char = stats.sum_char_match_pct / stats.both_succeeded as f64;
        let avg_jaccard = stats.sum_bigram_jaccard_pct / stats.both_succeeded as f64;
        let avg_dice = stats.sum_bigram_dice_pct / stats.both_succeeded as f64;
        println!(
            "  * 조각 일치율 99.5% 이상 달성: {} / {}개 ({:.1}%)",
            stats.high_accuracy_count, stats.both_succeeded, pass_rate
        );
        println!("  * 평균 문자 일치율         : {:.2}%", avg_char);
        println!("  * 평균 조각 Dice 일치율     : {:.2}%", avg_dice);
        println!("  * 평균 조각 Jaccard 일치율  : {:.2}%", avg_jaccard);
    }
    println!("- Rust 단독 성공             : {}개", stats.rust_only);
    println!("- kordoc 단독 성공           : {}개 (암호화/배포용 폴백 대상)", stats.kordoc_only);
    println!("- 양쪽 모두 실패             : {}개", stats.both_failed);

    if stats.both_succeeded > 0 {
        let avg_rust = stats.total_rust_duration_ms / stats.both_succeeded as f64;
        let avg_kordoc = stats.total_kordoc_duration_ms / stats.both_succeeded as f64;
        let total_speedup = if stats.total_rust_duration_ms > 0.0 {
            stats.total_kordoc_duration_ms / stats.total_rust_duration_ms
        } else {
            0.0
        };
        println!("- 평균 소요 시간 (성공 문서 기준):");
        println!("  * Rust 직접 파서           : {:.2} ms / 문서", avg_rust);
        println!("  * kordoc (Node.js)         : {:.2} ms / 문서", avg_kordoc);
        println!("  * 전체 속도 개선율         : 약 {:.1} 배 향상", total_speedup);
    }
    println!("================================================================================");
}
