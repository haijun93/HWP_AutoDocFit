//! 버전 줄(Version Line) 및 정기 업무(Periodic Task) 탐지 모듈.
use std::collections::{BTreeMap, HashMap};
use serde::{Deserialize, Serialize};

use crate::date::{Date, KST_OFFSET_SECS, SECS_PER_DAY, ymd_to_days};

/// 주기 유형.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Periodicity {
    /// 월간 업무 (중앙값 30 ± 7일)
    Monthly,
    /// 분기 업무 (중앙값 91 ± 15일)
    Quarterly,
    /// 반기 업무 (중앙값 182 ± 20일)
    HalfYearly,
    /// 연간 업무 (중앙값 365 ± 30일)
    Annual,
}

impl Periodicity {
    pub fn label(&self) -> &'static str {
        match self {
            Periodicity::Monthly => "월간",
            Periodicity::Quarterly => "분기",
            Periodicity::HalfYearly => "반기",
            Periodicity::Annual => "연간",
        }
    }

    /// 중앙값 일수로부터 주기 유형을 판정한다.
    pub fn from_median_days(days: f64) -> Option<Self> {
        if (23.0..=37.0).contains(&days) {
            Some(Periodicity::Monthly)
        } else if (76.0..=106.0).contains(&days) {
            Some(Periodicity::Quarterly)
        } else if (162.0..=202.0).contains(&days) {
            Some(Periodicity::HalfYearly)
        } else if (335.0..=395.0).contains(&days) {
            Some(Periodicity::Annual)
        } else {
            None
        }
    }
}

/// 같은 정규화 파일명(name_norm)을 가진 문서들의 버전 이력 줄.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VersionLine {
    /// 정규화 파일명
    pub name_norm: String,
    /// 작성일 순으로 정렬된 문서 인덱스 목록
    pub doc_indices: Vec<usize>,
}

/// 탐지된 정기 업무 정보.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PeriodicTask {
    /// 해당 주제 번호
    pub topic_id: usize,
    /// 업무명 (대표 제목 또는 name_norm)
    pub task_name: String,
    /// 주기 유형
    pub periodicity: Periodicity,
    /// 작성 간격 중앙값 (일)
    pub median_interval_days: f64,
    /// 작성 횟수 (3회 이상)
    pub occurrence_count: usize,
    /// 마지막 작성일
    pub last_date: Date,
    /// 다음 예상 작성일
    pub next_expected_date: Date,
    /// 다음 예상일 타임스탬프 (초)
    pub next_expected_ts: i64,
    /// 연관 문서 인덱스 목록
    pub doc_indices: Vec<usize>,
}

/// 문서 작성일 계산: min(created, modified)
#[inline]
pub fn doc_created_at(created: i64, modified: i64) -> i64 {
    std::cmp::min(created, modified)
}

/// 숫자 슬라이스의 중앙값을 계산한다.
fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let len = values.len();
    if len % 2 == 1 {
        values[len / 2]
    } else {
        (values[len / 2 - 1] + values[len / 2]) / 2.0
    }
}

/// 한 주제 안에서 같은 name_norm을 가진 문서들을 작성일 순으로 묶어 버전 줄을 만든다.
pub fn detect_version_lines(
    topic_doc_indices: &[usize],
    name_norms: &[String],
    created_timestamps: &[i64],
) -> Vec<VersionLine> {
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();

    for &idx in topic_doc_indices {
        let name = name_norms[idx].clone();
        groups.entry(name).or_default().push(idx);
    }

    let mut lines = Vec::new();
    for (name_norm, mut indices) in groups {
        // 작성일(min(created, modified)) 기준 오름차순 정렬
        indices.sort_by_key(|&idx| created_timestamps[idx]);
        lines.push(VersionLine {
            name_norm,
            doc_indices: indices,
        });
    }

    // 이름순 정렬
    lines.sort_by(|a, b| a.name_norm.cmp(&b.name_norm));
    lines
}

/// 주어진 문서 인덱스들과 타임스탬프들로부터 정기 업무 여부를 검사한다.
/// 작성 간격의 중앙값이 30±7/91±15/182±20/365±30일이고 3회 이상이면 정기 업무.
pub fn detect_periodic_task_for_group(
    topic_id: usize,
    task_name: &str,
    indices: &[usize],
    created_timestamps: &[i64],
) -> Option<PeriodicTask> {
    if indices.len() < 3 {
        return None;
    }

    // 날짜별로 정렬 및 고유화 (하루에 여러 번 작성된 문서는 1회로 합침)
    let mut date_map: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for &idx in indices {
        let ts = created_timestamps[idx];
        let days = (ts + KST_OFFSET_SECS).div_euclid(SECS_PER_DAY);
        date_map.entry(days).or_default().push(idx);
    }

    if date_map.len() < 3 {
        return None;
    }

    let day_keys: Vec<i64> = date_map.keys().copied().collect();
    let mut intervals: Vec<f64> = Vec::with_capacity(day_keys.len() - 1);
    for w in day_keys.windows(2) {
        let gap = (w[1] - w[0]) as f64;
        intervals.push(gap);
    }

    let med = median(intervals);
    let periodicity = Periodicity::from_median_days(med)?;

    let last_day = *day_keys.last()?;
    let (ly, lm, ld) = crate::date::days_to_ymd(last_day);
    let last_date = Date::new(ly, lm, ld);

    let next_day = last_day + med.round() as i64;
    let (ny, nm, nd) = crate::date::days_to_ymd(next_day);
    let next_expected_date = Date::new(ny, nm, nd);
    let next_expected_ts = next_expected_date.to_timestamp(KST_OFFSET_SECS);

    let mut all_indices: Vec<usize> = Vec::new();
    for list in date_map.values() {
        all_indices.extend(list);
    }

    Some(PeriodicTask {
        topic_id,
        task_name: task_name.to_string(),
        periodicity,
        median_interval_days: med,
        occurrence_count: date_map.len(),
        last_date,
        next_expected_date,
        next_expected_ts,
        doc_indices: all_indices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_line() {
        let name_norms = vec![
            "주간업무보고".to_string(),
            "주간업무보고".to_string(),
            "예산계획서".to_string(),
        ];
        let created_ts = vec![2000, 1000, 1500];
        let indices = vec![0, 1, 2];

        let lines = detect_version_lines(&indices, &name_norms, &created_ts);
        assert_eq!(lines.len(), 2);

        let report_line = lines.iter().find(|l| l.name_norm == "주간업무보고").unwrap();
        // 1000초인 인덱스 1이 인덱스 0보다 먼저 와야 함
        assert_eq!(report_line.doc_indices, vec![1, 0]);
    }

    #[test]
    fn test_periodic_task_monthly() {
        // 매월 약 30일 간격으로 3회 작성된 업무
        let d1 = Date::new(2026, 1, 15).to_timestamp(KST_OFFSET_SECS);
        let d2 = Date::new(2026, 2, 15).to_timestamp(KST_OFFSET_SECS); // 31일 후
        let d3 = Date::new(2026, 3, 17).to_timestamp(KST_OFFSET_SECS); // 30일 후

        let indices = vec![0, 1, 2];
        let created_ts = vec![d1, d2, d3];

        let task = detect_periodic_task_for_group(1, "월간 업무 보고", &indices, &created_ts)
            .expect("Should be detected as monthly");

        assert_eq!(task.periodicity, Periodicity::Monthly);
        assert_eq!(task.occurrence_count, 3);
        assert_eq!(task.last_date, Date::new(2026, 3, 17));
        // 다음 예상일: 3월 17일 + 약 30.5일 -> 4월 16일~17일 부근
        assert_eq!(task.next_expected_date.month, 4);
    }

    #[test]
    fn test_periodic_task_quarterly() {
        // 분기별 약 91일 간격
        let d1 = Date::new(2026, 1, 1).to_timestamp(KST_OFFSET_SECS);
        let d2 = Date::new(2026, 4, 2).to_timestamp(KST_OFFSET_SECS); // 91일
        let d3 = Date::new(2026, 7, 2).to_timestamp(KST_OFFSET_SECS); // 91일

        let indices = vec![0, 1, 2];
        let created_ts = vec![d1, d2, d3];

        let task = detect_periodic_task_for_group(0, "분기 실적 점검", &indices, &created_ts)
            .expect("Should be detected as quarterly");

        assert_eq!(task.periodicity, Periodicity::Quarterly);
        assert_eq!(task.next_expected_date.month, 10);
    }
}
