//! 통계 모듈: 주제×월, 주제×ISO주, 주×요일 열지도 수, 상위 주제, 집중 시기(평균+2표준편차 넘는 주).
use std::collections::{BTreeMap, HashMap};
use serde::{Deserialize, Serialize};

use crate::cluster::Topic;
use crate::date::{Date, IsoWeek, KST_OFFSET_SECS, Period, timestamp_to_iso_week, timestamp_to_weekday};

/// 상위 주제 요약.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TopTopicSummary {
    pub topic_id: usize,
    pub topic_name: String,
    pub doc_count: usize,
    pub percentage: f64,
}

/// 집중 시기 (평균 + 2표준편차 초과 주차).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CrunchWeek {
    pub iso_year: i32,
    pub iso_week: u32,
    pub doc_count: u32,
    pub mean: f64,
    pub std_dev: f64,
    pub threshold: f64,
}

/// 종합 통계 보고서 구조체.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    /// 총 문서 수 (기간 필터링 후)
    pub total_docs: usize,
    /// 주제 ID별 월별(1~12월) 문서 수: [1월, 2월, ..., 12월]
    pub topic_by_month: HashMap<usize, [u32; 12]>,
    /// 주제 ID별 ISO 주차(1~53) 문서 수
    pub topic_by_iso_week: HashMap<usize, BTreeMap<u32, u32>>,
    /// 주차(ISO Week) × 요일(0:월요일 ~ 6:일요일) 히트맵
    pub week_day_heatmap: BTreeMap<u32, [u32; 7]>,
    /// 상위 주제 순위
    pub top_topics: Vec<TopTopicSummary>,
    /// 집중 시기 (주별 작업량이 평균+2표준편차를 넘는 주차)
    pub crunch_weeks: Vec<CrunchWeek>,
}

/// 문서별 타임스탬프와 주제 매핑으로부터 통계를 계산한다.
pub fn calculate_stats(
    topics: &[Topic],
    doc_timestamps: &[i64], // modified 기준 (작업일)
    period: Option<Period>,
    offset_secs: i64,
) -> Stats {
    let n = doc_timestamps.len();
    if n == 0 {
        return Stats::default();
    }

    // 문서 -> 주제 ID 매핑 구축
    let mut doc_to_topic: Vec<Option<usize>> = vec![None; n];
    for topic in topics {
        for &doc_idx in &topic.doc_indices {
            if doc_idx < n {
                doc_to_topic[doc_idx] = Some(topic.id);
            }
        }
    }

    let mut filtered_docs_count = 0;
    let mut topic_by_month: HashMap<usize, [u32; 12]> = HashMap::new();
    let mut topic_by_iso_week: HashMap<usize, BTreeMap<u32, u32>> = HashMap::new();
    let mut week_day_heatmap: BTreeMap<u32, [u32; 7]> = BTreeMap::new();
    let mut week_total_counts: BTreeMap<IsoWeek, u32> = BTreeMap::new();
    let mut topic_total_counts: HashMap<usize, usize> = HashMap::new();

    for (i, &ts) in doc_timestamps.iter().enumerate() {
        // 기간 필터링
        if let Some(p) = period {
            if !p.contains(ts) {
                continue;
            }
        }

        filtered_docs_count += 1;
        let date = Date::from_timestamp(ts, offset_secs);
        let iso = timestamp_to_iso_week(ts, offset_secs);
        let weekday = timestamp_to_weekday(ts, offset_secs); // 1=월, 7=일

        // 월별 (month: 1~12 -> index: 0~11)
        let m_idx = (date.month.saturating_sub(1).min(11)) as usize;

        if let Some(topic_id) = doc_to_topic[i] {
            let m_arr = topic_by_month.entry(topic_id).or_insert([0; 12]);
            m_arr[m_idx] += 1;

            let w_map = topic_by_iso_week.entry(topic_id).or_default();
            *w_map.entry(iso.week).or_insert(0) += 1;

            *topic_total_counts.entry(topic_id).or_insert(0) += 1;
        }

        // 요일 히트맵 (weekday 1=월 -> 0, 7=일 -> 6)
        let d_idx = (weekday.saturating_sub(1).min(6)) as usize;
        let heat_arr = week_day_heatmap.entry(iso.week).or_insert([0; 7]);
        heat_arr[d_idx] += 1;

        // 주별 총합 집계
        *week_total_counts.entry(iso).or_insert(0) += 1;
    }

    // 상위 주제 계산
    let mut top_topics = Vec::new();
    for topic in topics {
        let count = topic_total_counts.get(&topic.id).copied().unwrap_or(0);
        let percentage = if filtered_docs_count > 0 {
            (count as f64 / filtered_docs_count as f64) * 100.0
        } else {
            0.0
        };
        top_topics.push(TopTopicSummary {
            topic_id: topic.id,
            topic_name: topic.name.clone(),
            doc_count: count,
            percentage,
        });
    }
    top_topics.sort_by(|a, b| b.doc_count.cmp(&a.doc_count));

    // 집중 시기 계산 (평균 + 2표준편차 넘는 주)
    let mut crunch_weeks = Vec::new();
    if !week_total_counts.is_empty() {
        let counts: Vec<f64> = week_total_counts.values().map(|&c| c as f64).collect();
        let total_sum: f64 = counts.iter().sum();
        let mean = total_sum / counts.len() as f64;

        let variance = counts
            .iter()
            .map(|&c| {
                let diff = c - mean;
                diff * diff
            })
            .sum::<f64>()
            / counts.len() as f64;
        let std_dev = variance.sqrt();
        let threshold = mean + 2.0 * std_dev;

        for (&iso, &cnt) in &week_total_counts {
            let c = cnt as f64;
            if c > threshold {
                crunch_weeks.push(CrunchWeek {
                    iso_year: iso.year,
                    iso_week: iso.week,
                    doc_count: cnt,
                    mean,
                    std_dev,
                    threshold,
                });
            }
        }
    }

    Stats {
        total_docs: filtered_docs_count,
        topic_by_month,
        topic_by_iso_week,
        week_day_heatmap,
        top_topics,
        crunch_weeks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_and_crunch_weeks() {
        let topic = Topic {
            id: 0,
            name: "통합 예산 업무".to_string(),
            rep_title: "예산 업무".to_string(),
            rep_doc_index: 0,
            keywords: vec!["예산".to_string()],
            doc_indices: (0..20).collect(),
        };

        // 10개 주에 걸쳐 평소 1개 문서씩 작업하다가 특정 주(40주차)에 10개 문서 집중 작업
        let mut timestamps = Vec::new();
        // 주차 1~9: 각 주 1건씩
        for week in 1..=9 {
            let days = crate::date::ymd_to_days(2026, 1, 1) + (week - 1) * 7;
            let ts = days * crate::date::SECS_PER_DAY - KST_OFFSET_SECS;
            timestamps.push(ts);
        }
        // 주차 10: 11건 몰림 (이상치 집중 주차)
        for _ in 0..11 {
            let days = crate::date::ymd_to_days(2026, 1, 1) + 9 * 7;
            let ts = days * crate::date::SECS_PER_DAY - KST_OFFSET_SECS;
            timestamps.push(ts);
        }

        let stats = calculate_stats(&[topic], &timestamps, None, KST_OFFSET_SECS);

        assert_eq!(stats.total_docs, 20);
        // 집중 시기가 1개 감지되어야 함 (10번째 주)
        assert!(!stats.crunch_weeks.is_empty());
        assert_eq!(stats.crunch_weeks[0].doc_count, 11);
    }

    #[test]
    fn test_period_filter() {
        let topic = Topic {
            id: 0,
            name: "테스트".to_string(),
            rep_title: "테스트".to_string(),
            rep_doc_index: 0,
            keywords: vec![],
            doc_indices: vec![0, 1],
        };

        let ts1 = Date::new(2026, 3, 1).to_timestamp(KST_OFFSET_SECS);
        let ts2 = Date::new(2026, 7, 1).to_timestamp(KST_OFFSET_SECS);

        // 2026년 1분기만 필터링
        let period = Period::new(
            Date::new(2026, 1, 1).to_timestamp(KST_OFFSET_SECS),
            Date::new(2026, 3, 31).to_timestamp(KST_OFFSET_SECS),
        );

        let stats = calculate_stats(&[topic], &[ts1, ts2], Some(period), KST_OFFSET_SECS);
        assert_eq!(stats.total_docs, 1);
    }
}
