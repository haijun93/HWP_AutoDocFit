//! Fixed-seed synthetic input and the opt-in three-artifact bundle.
use crate::{
    Date, Doc, KST_OFFSET_SECS, Period, analyze,
    history::{detect_periodic_topics, folder_histories},
    output::{AnnualInput, write_annual_bundle},
};
use std::{fs, io, path::Path};

pub const DEFAULT_SEED: u64 = 20261125;

pub fn generate_synthetic_docs(seed: u64) -> Vec<Doc> {
    let mut state = seed;
    let mut docs = Vec::new();
    for (folder, title, count) in [
        ("예산", "예산 집행 정산", 12),
        ("보안", "정보보안 취약점 점검", 4),
        ("개발", "업무 자동화 시스템 개발", 24),
        ("평가", "부서 성과 평가", 5),
        ("인프라", "클라우드 전환 구축", 17),
    ] {
        for i in 0..count {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let month = (i * 12 / count + 1) as u32;
            let day = if count <= 12 {
                15
            } else {
                1 + ((state >> 32) % 27) as u32
            };
            for version in 1..=2 {
                let date = Date::new(2026, month, day);
                let ts = date.to_timestamp(KST_OFFSET_SECS) + (9 + version) * 3600;
                docs.push(Doc::new(
                    format!("합성/{folder}/{month:02}_{i:02}_{title}_v{version}.hwpx"),
                    ts,
                    ts,
                    title,
                    vec![format!("{title} 추진 현황"), format!("{title} 후속 계획")],
                ));
            }
        }
    }
    docs
}

pub fn generate_synthetic_annual_input(seed: u64) -> AnnualInput {
    let docs = generate_synthetic_docs(seed);
    let report = analyze(&docs, Some(Period::full_year(2026)), Some(0.55));
    let periodic_topics = detect_periodic_topics(&docs, &report.topics, &[]);
    let folder_histories = folder_histories(&docs, &report.topics, &[], 3);
    AnnualInput {
        report: Some(report),
        periodic_topics,
        folder_histories,
        docs,
        ..AnnualInput::default()
    }
}

pub fn generate_synthetic_annual_artifacts(
    folder: impl AsRef<Path>,
    seed: u64,
) -> io::Result<AnnualInput> {
    let folder = folder.as_ref();
    let input = generate_synthetic_annual_input(seed);
    write_annual_bundle(&input, folder)?;
    fs::write(
        folder.join("원자료/합성_문서.json"),
        serde_json::to_vec_pretty(&input.docs)?,
    )?;
    fs::write(folder.join("원자료/합성_시드.txt"), format!("{seed}\n"))?;
    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_reproducibility_and_derived_counts() {
        let a = generate_synthetic_annual_input(DEFAULT_SEED);
        let b = generate_synthetic_annual_input(DEFAULT_SEED);
        assert_eq!(a, b);
        assert_eq!(a.docs.len(), 124);
        assert_ne!(a.docs, generate_synthetic_docs(DEFAULT_SEED + 1));
        let report = a.report.unwrap();
        assert_eq!(report.analyzed_doc_count, a.docs.len());
        assert_eq!(
            a.folder_histories
                .iter()
                .map(|f| f.document_count)
                .sum::<usize>(),
            a.docs.len()
        );
        assert!(
            report
                .version_lines
                .iter()
                .any(|line| line.doc_indices.len() >= 2)
        );
        assert!(a.periodic_topics.iter().any(|p| p.cadence == "월간"));
    }
    #[test]
    fn generate_synthetic_artifacts() {
        let d = tempfile::tempdir().unwrap();
        generate_synthetic_annual_artifacts(d.path(), DEFAULT_SEED).unwrap();
        for name in [
            "인수인계서.hwpx",
            "실적보고서.hwpx",
            "통계.xlsx",
            "원자료/보고서_데이터.json",
            "원자료/합성_문서.json",
        ] {
            assert!(d.path().join(name).is_file(), "{name}");
        }
    }
}
