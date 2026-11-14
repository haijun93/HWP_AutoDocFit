//! kfind-annual: 업무 연보 A1 엔진
//!
//! 로컬 문서의 파일명·제목·중제목·소제목 유사도로 주제를 만들고,
//! 월·주별 통계, 버전 줄, 정기 업무(월·분기·반기·연)를 분석하는 순수 엔진.
//!
//! 라이선스: PolyForm Strict License 1.0.0 (출처 표시 필수, kfind/LICENSE 참고).

pub mod cluster;
pub mod date;
pub mod norm;
pub mod series;
pub mod sim;
pub mod stats;

pub use cluster::{Topic, UnionFind, cluster_topics};
pub use date::{
    Date, IsoWeek, KST_OFFSET_SECS, Period, SECS_PER_DAY, days_to_iso_week, days_to_weekday,
    days_to_ymd, timestamp_to_iso_week, timestamp_to_weekday, ymd_to_days,
};
pub use norm::{extract_extension, normalize_name, strip_extension};
pub use series::{
    PeriodicTask, Periodicity, VersionLine, detect_periodic_task_for_group, detect_version_lines,
    doc_created_at,
};
pub use sim::{
    DocFeatures, calculate_weighted_similarity, compute_minhash_64, extract_bigrams,
    extract_headings_bigrams, jaccard_similarity, lsh_candidate_pairs,
};
pub use stats::{CrunchWeek, Stats, TopTopicSummary, calculate_stats};

use serde::{Deserialize, Serialize};

/// 분석 대상 문서 단위.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Doc {
    /// 원본 파일 경로
    pub path: String,
    /// 생성 시각 (초 단위 유닉스 타임스탬프)
    pub created: i64,
    /// 수정 시각 (초 단위 유닉스 타임스탬프)
    pub modified: i64,
    /// 확장자 (소문자, 점 제외)
    pub ext: String,
    /// 정규화된 파일명 (확장자·날짜·버전·괄호 숫자 제거)
    pub name_norm: String,
    /// 문서 제목 (본문 첫 머리 또는 메타데이터)
    pub title: String,
    /// 중·소제목 목록 (최대 20개)
    pub headings: Vec<String>,
}

impl Doc {
    /// 새 Doc 인스턴스를 생성한다. 확장자와 name_norm은 자동으로 정규화 추출된다.
    pub fn new(
        path: impl Into<String>,
        created: i64,
        modified: i64,
        title: impl Into<String>,
        headings: Vec<String>,
    ) -> Self {
        let path = path.into();
        let ext = extract_extension(&path);
        let name_norm = normalize_name(&path);
        let mut headings = headings;
        headings.truncate(20);

        Self {
            path,
            created,
            modified,
            ext,
            name_norm,
            title: title.into(),
            headings,
        }
    }

    /// 작성일: min(created, modified)
    #[inline]
    pub fn created_at(&self) -> i64 {
        std::cmp::min(self.created, self.modified)
    }

    /// 작업일: modified
    #[inline]
    pub fn modified_at(&self) -> i64 {
        self.modified
    }
}

/// 업무 연보 분석 최종 보고서 구조체 (직렬화 가능한 순수 데이터).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Report {
    /// 분석 수행 시각 (초 단위 타임스탬프)
    pub generated_at: i64,
    /// 적용된 분석 기간 (없으면 전체)
    pub period: Option<Period>,
    /// 적용된 유사도 임계치
    pub threshold: f64,
    /// 입력된 전체 문서 수
    pub total_input_docs: usize,
    /// 기간 필터링 후 실제 분석된 문서 수
    pub analyzed_doc_count: usize,
    /// 분석된 문서들의 원래 인덱스 목록
    pub analyzed_indices: Vec<usize>,
    /// 도출된 주제 목록
    pub topics: Vec<Topic>,
    /// 주제별 버전 줄 목록
    pub version_lines: Vec<VersionLine>,
    /// 탐지된 정기 업무 목록 (월/분기/반기/연간)
    pub periodic_tasks: Vec<PeriodicTask>,
    /// 통계 요약 (주제×월, 주제×ISO주, 주×요일 열지도 수, 상위 주제, 집중 시기)
    pub stats: Stats,
}

/// 기본 유사도 임계치
pub const DEFAULT_SIMILARITY_THRESHOLD: f64 = 0.55;

/// 업무 연보 분석 메인 공개 진입점.
///
/// docs: 입력 문서 목록
/// period: 분석 기간 (None이면 전체 기간)
/// threshold: 주제 묶기 유사도 임계치 (None이면 기본 0.55)
pub fn analyze(docs: &[Doc], period: Option<Period>, threshold: Option<f64>) -> Report {
    let th = threshold.unwrap_or(DEFAULT_SIMILARITY_THRESHOLD);
    let total_input_docs = docs.len();

    // 1단계: 기간 거르기 (작업일 modified 기준)
    let mut analyzed_indices = Vec::new();
    for (i, doc) in docs.iter().enumerate() {
        let work_ts = doc.modified_at();
        if let Some(p) = period {
            if !p.contains(work_ts) {
                continue;
            }
        }
        analyzed_indices.push(i);
    }

    let analyzed_doc_count = analyzed_indices.len();
    if analyzed_doc_count == 0 {
        return Report {
            generated_at: 0,
            period,
            threshold: th,
            total_input_docs,
            analyzed_doc_count: 0,
            analyzed_indices: Vec::new(),
            topics: Vec::new(),
            version_lines: Vec::new(),
            periodic_tasks: Vec::new(),
            stats: Stats::default(),
        };
    }

    // 분석 대상 문서 부분 목록 생성
    let target_docs: Vec<&Doc> = analyzed_indices.iter().map(|&i| &docs[i]).collect();

    // 2단계: 문서 특성(2글자 조각 집합) 추출
    let doc_features: Vec<DocFeatures> = target_docs
        .iter()
        .map(|d| DocFeatures::from_doc(&d.name_norm, &d.title, &d.headings))
        .collect();

    let doc_titles: Vec<String> = target_docs.iter().map(|d| d.title.clone()).collect();
    let doc_name_norms: Vec<String> = target_docs.iter().map(|d| d.name_norm.clone()).collect();
    let created_ts: Vec<i64> = target_docs.iter().map(|d| d.created_at()).collect();
    let modified_ts: Vec<i64> = target_docs.iter().map(|d| d.modified_at()).collect();

    // 3단계: MinHash(64) + LSH(16밴드×4) 후보 추리기
    let signatures: Vec<[u32; 64]> = doc_features
        .iter()
        .map(|f| compute_minhash_64(&f.all_bigrams))
        .collect();
    let candidate_pairs_set = lsh_candidate_pairs(&signatures);
    let candidate_pairs: Vec<(usize, usize)> = candidate_pairs_set.into_iter().collect();

    // 4단계: 유사도 ≥ threshold인 쌍을 Union-Find로 묶어 주제(Topic) 생성
    let topics = cluster_topics(
        &doc_features,
        &doc_titles,
        &doc_name_norms,
        &candidate_pairs,
        th,
    );

    // 5단계: 버전 줄 탐지 (각 주제 안에서 같은 name_norm을 작성일 순으로)
    let mut version_lines = Vec::new();
    for topic in &topics {
        let lines = detect_version_lines(&topic.doc_indices, &doc_name_norms, &created_ts);
        version_lines.extend(lines);
    }

    // 6단계: 정기 업무 탐지 (간격 중앙값 30±7, 91±15, 182±20, 365±30일, 3회 이상)
    let mut periodic_tasks = Vec::new();
    for topic in &topics {
        // 주제 전체 단위 검사
        if let Some(task) = detect_periodic_task_for_group(
            topic.id,
            &topic.rep_title,
            &topic.doc_indices,
            &created_ts,
        ) {
            periodic_tasks.push(task);
        } else {
            // 주제 안의 세부 버전줄 단위 검사
            for vl in &version_lines {
                // 해당 주제에 속한 버전줄만
                if vl.doc_indices.iter().all(|idx| topic.doc_indices.contains(idx)) {
                    if let Some(task) = detect_periodic_task_for_group(
                        topic.id,
                        &vl.name_norm,
                        &vl.doc_indices,
                        &created_ts,
                    ) {
                        periodic_tasks.push(task);
                    }
                }
            }
        }
    }

    // 7단계: 통계 산출 (주제×월, 주제×ISO주, 주×요일 열지도, 상위 주제, 집중 시기)
    let stats = calculate_stats(&topics, &modified_ts, None, KST_OFFSET_SECS);

    Report {
        generated_at: 0,
        period,
        threshold: th,
        total_input_docs,
        analyzed_doc_count,
        analyzed_indices,
        topics,
        version_lines,
        periodic_tasks,
        stats,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_analyze_pipeline() {
        // 복합 시나리오 테스트:
        // 1. 월간 보고서 3건 (주제 1: 월간 정기 업무)
        // 2. 사업 추진 계획서 2건 (주제 2)
        // 3. 다른 연도 문서 1건 (기간 거르기 대상)

        let d1_ts = Date::new(2026, 1, 15).to_timestamp(KST_OFFSET_SECS);
        let d2_ts = Date::new(2026, 2, 15).to_timestamp(KST_OFFSET_SECS);
        let d3_ts = Date::new(2026, 3, 17).to_timestamp(KST_OFFSET_SECS);

        let plan1_ts = Date::new(2026, 2, 1).to_timestamp(KST_OFFSET_SECS);
        let plan2_ts = Date::new(2026, 2, 10).to_timestamp(KST_OFFSET_SECS);

        let old_ts = Date::new(2025, 12, 1).to_timestamp(KST_OFFSET_SECS);

        let docs = vec![
            Doc::new("2026-01-15_월간_업무_보고_v1.hwpx", d1_ts, d1_ts, "1월 업무보고", vec!["주요 실적".into()]),
            Doc::new("2026-02-15_월간_업무_보고_v2.hwpx", d2_ts, d2_ts, "2월 업무보고", vec!["주요 실적".into()]),
            Doc::new("2026-03-17_월간_업무_보고_최종.hwpx", d3_ts, d3_ts, "3월 업무보고", vec!["주요 실적".into()]),
            Doc::new("디지털 혁신 추진 계획서.docx", plan1_ts, plan1_ts, "추진 계획서", vec!["추진 배경".into()]),
            Doc::new("디지털 혁신 추진 계획서_수정본.docx", plan2_ts, plan2_ts, "추진 계획서 수정", vec!["추진 배경".into()]),
            Doc::new("2025년도_과거문서.hwp", old_ts, old_ts, "과거 보고", vec![]),
        ];

        // 2026년도 전체 기간 필터링
        let period_2026 = Period::full_year(2026);
        let report = analyze(&docs, Some(period_2026), Some(0.50));

        // 2025년도 문서는 제외되고 5건만 분석되어야 함
        assert_eq!(report.total_input_docs, 6);
        assert_eq!(report.analyzed_doc_count, 5);

        // 주제는 최소 2개로 분리되어야 함 (월간 업무 보고 계열 vs 디지털 혁신 추진 계열)
        assert!(report.topics.len() >= 2);

        // 월간 정기 업무가 탐지되어야 함
        assert!(!report.periodic_tasks.is_empty());
        let monthly_task = report
            .periodic_tasks
            .iter()
            .find(|t| t.periodicity == Periodicity::Monthly);
        assert!(monthly_task.is_some(), "Monthly recurring task must be detected");

        // 통계 검증: 1, 2, 3월에 작업 기록이 있어야 함
        assert_eq!(report.stats.total_docs, 5);
        assert!(!report.stats.top_topics.is_empty());
    }

    #[test]
    fn test_iso_week_boundary_and_weekday() {
        // 2026년 마지막 주와 2027년 첫 주 경계 검증
        // 2026-12-31은 목요일 (ISO 2026년 53주차)
        let days_2026_12_31 = ymd_to_days(2026, 12, 31);
        let iso_2026_end = days_to_iso_week(days_2026_12_31);
        assert_eq!(iso_2026_end.year, 2026);
        assert_eq!(iso_2026_end.week, 53);

        // 2027-01-01은 금요일 (ISO 2026년 53주차)
        let days_2027_01_01 = ymd_to_days(2027, 1, 1);
        let iso_2027_start = days_to_iso_week(days_2027_01_01);
        assert_eq!(iso_2027_start.year, 2026);
        assert_eq!(iso_2027_start.week, 53);

        // 2027-01-04는 월요일 (ISO 2027년 1주차)
        let days_2027_01_04 = ymd_to_days(2027, 1, 4);
        let iso_2027_week1 = days_to_iso_week(days_2027_01_04);
        assert_eq!(iso_2027_week1.year, 2027);
        assert_eq!(iso_2027_week1.week, 1);
    }
}
