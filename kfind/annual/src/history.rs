//! A3 주기 업무, 폴더 이력, 기간 비교를 위한 순수 분석 함수.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use crate::date::{Date, KST_OFFSET_SECS, SECS_PER_DAY};
use crate::{Doc, Topic};

/// 탐지된 주기 및 다음 예상 시기.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PeriodicPrediction {
    pub topic_id: usize,
    pub topic: String,
    pub cadence: String,
    pub median_interval_days: f64,
    pub occurrence_count: usize,
    pub last_date: Date,
    pub next_expected_date: Date,
    pub doc_indices: Vec<usize>,
}

/// 폴더 단위 대표 업무 이력.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FolderHistory {
    pub folder: String,
    pub document_count: usize,
    pub first_date: Date,
    pub last_date: Date,
    pub representative_topics: Vec<String>,
}

/// 비교할 업무 기간과 그 기간에 속하는 문서 작업일.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparisonPeriod {
    pub start: i64,
    pub end: i64,
}

impl ComparisonPeriod {
    pub const fn new(start: i64, end: i64) -> Self {
        Self { start, end }
    }
    fn contains(self, ts: i64) -> bool {
        self.start <= ts && ts <= self.end
    }
}

/// 두 기간에서 주제별 문서 수와 증감을 나타낸다.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TopicChange {
    pub topic_id: usize,
    pub topic: String,
    pub before_count: usize,
    pub after_count: usize,
    pub delta: isize,
}

fn median(mut xs: Vec<i64>) -> f64 {
    xs.sort_unstable();
    if xs.len() % 2 == 1 {
        xs[xs.len() / 2] as f64
    } else {
        (xs[xs.len() / 2 - 1] as f64 + xs[xs.len() / 2] as f64) / 2.0
    }
}

fn cadence(days: f64) -> Option<&'static str> {
    if (23.0..=37.0).contains(&days) {
        Some("월간")
    } else if (76.0..=106.0).contains(&days) {
        Some("분기")
    } else if (335.0..=395.0).contains(&days) {
        Some("연간")
    } else {
        None
    }
}

fn next_occurrence(last: Date, cadence: &str) -> Date {
    let (year, month) = match cadence {
        "월간" => {
            if last.month == 12 {
                (last.year + 1, 1)
            } else {
                (last.year, last.month + 1)
            }
        }
        "분기" => {
            let total_month = last.year * 12 + last.month as i32 - 1 + 3;
            (
                total_month.div_euclid(12),
                total_month.rem_euclid(12) as u32 + 1,
            )
        }
        "연간" => (last.year + 1, last.month),
        _ => return last,
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    let day = last.day.min(days_in_month);
    Date::new(year, month, day)
}

/// 기존 클러스터에 속한 문서의 날짜를 비교해 주기 업무를 찾는다.
/// A2 날짜가 있으면 이를 사용하고, 없으면 파일 수정일을 KST 날짜로 사용한다.
pub fn detect_periodic_topics(
    docs: &[Doc],
    topics: &[Topic],
    document_dates: &[Option<Date>],
) -> Vec<PeriodicPrediction> {
    let mut out = Vec::new();
    for topic in topics {
        let mut dated: Vec<(i64, usize)> = topic
            .doc_indices
            .iter()
            .filter_map(|&i| {
                let d = docs.get(i)?;
                let date = document_dates
                    .get(i)
                    .copied()
                    .flatten()
                    .unwrap_or_else(|| Date::from_timestamp(d.modified, KST_OFFSET_SECS));
                Some((
                    (date.to_timestamp(KST_OFFSET_SECS) + KST_OFFSET_SECS).div_euclid(SECS_PER_DAY),
                    i,
                ))
            })
            .collect();
        dated.sort_unstable();
        dated.dedup_by_key(|x| x.0);
        if dated.len() < 3 {
            continue;
        }
        let gaps: Vec<i64> = dated.windows(2).map(|w| w[1].0 - w[0].0).collect();
        let med = median(gaps);
        let Some(kind) = cadence(med) else {
            continue;
        };
        let last_day = dated.last().unwrap().0;
        let (ly, lm, ld) = crate::date::days_to_ymd(last_day);
        let last_date = Date::new(ly, lm, ld);
        let next_date = next_occurrence(last_date, kind);
        out.push(PeriodicPrediction {
            topic_id: topic.id,
            topic: topic.rep_title.clone(),
            cadence: kind.to_string(),
            median_interval_days: med,
            occurrence_count: dated.len(),
            last_date,
            next_expected_date: next_date,
            doc_indices: dated.iter().map(|x| x.1).collect(),
        });
    }
    out.sort_by_key(|p| p.topic_id);
    out
}

/// 경로의 상위 폴더별 기간 이력과 문서 수 상위 대표 주제를 계산한다.
pub fn folder_histories(
    docs: &[Doc],
    topics: &[Topic],
    document_dates: &[Option<Date>],
    top_n: usize,
) -> Vec<FolderHistory> {
    let mut topic_for = vec!["기타".to_string(); docs.len()];
    for t in topics {
        for &i in &t.doc_indices {
            if i < topic_for.len() {
                topic_for[i] = t.rep_title.clone();
            }
        }
    }
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, d) in docs.iter().enumerate() {
        let folder = d
            .path
            .rsplit_once(['\\', '/'])
            .map(|x| x.0)
            .unwrap_or("")
            .to_string();
        groups.entry(folder).or_default().push(i);
    }
    groups
        .into_iter()
        .map(|(folder, indices)| {
            let mut dates: Vec<Date> =
                indices
                    .iter()
                    .map(|&i| {
                        document_dates.get(i).copied().flatten().unwrap_or_else(|| {
                            Date::from_timestamp(docs[i].modified, KST_OFFSET_SECS)
                        })
                    })
                    .collect();
            dates.sort();
            let mut counts = HashMap::<String, usize>::new();
            for &i in &indices {
                *counts.entry(topic_for[i].clone()).or_default() += 1;
            }
            let mut ranked: Vec<_> = counts.into_iter().collect();
            ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            FolderHistory {
                folder,
                document_count: indices.len(),
                first_date: dates[0],
                last_date: *dates.last().unwrap(),
                representative_topics: ranked.into_iter().take(top_n).map(|x| x.0).collect(),
            }
        })
        .collect()
}

/// 고정된 주제 클러스터를 기준으로 두 기간의 문서량 증감을 계산한다.
pub fn compare_periods(
    docs: &[Doc],
    topics: &[Topic],
    before: ComparisonPeriod,
    after: ComparisonPeriod,
) -> Vec<TopicChange> {
    let mut rows = Vec::new();
    for t in topics {
        let mut b = 0usize;
        let mut a = 0usize;
        for &i in &t.doc_indices {
            if let Some(d) = docs.get(i) {
                if before.contains(d.modified) {
                    b += 1;
                }
                if after.contains(d.modified) {
                    a += 1;
                }
            }
        }
        rows.push(TopicChange {
            topic_id: t.id,
            topic: t.rep_title.clone(),
            before_count: b,
            after_count: a,
            delta: a as isize - b as isize,
        });
    }
    rows.sort_by_key(|r| r.topic_id);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::Topic;
    fn ts(y: i32, m: u32, d: u32) -> i64 {
        Date::new(y, m, d).to_timestamp(KST_OFFSET_SECS)
    }
    fn topic(id: usize, title: &str, ids: Vec<usize>) -> Topic {
        Topic {
            id,
            name: title.into(),
            rep_title: title.into(),
            rep_doc_index: ids[0],
            keywords: vec![],
            doc_indices: ids,
        }
    }
    fn doc(path: &str, modified: i64) -> Doc {
        Doc::new(path, modified, modified, "", vec![])
    }

    #[test]
    fn periodic_uses_a2_dates_and_falls_back_to_modified() {
        let docs = vec![
            doc("x/a", ts(2020, 1, 1)),
            doc("x/b", ts(2020, 1, 1)),
            doc("x/c", ts(2020, 1, 1)),
        ];
        let topics = vec![topic(0, "연례 보고", vec![0, 1, 2])];
        let dates = vec![
            Some(Date::new(2024, 3, 1)),
            Some(Date::new(2025, 3, 1)),
            Some(Date::new(2026, 3, 1)),
        ];
        let p = detect_periodic_topics(&docs, &topics, &dates);
        assert_eq!(p[0].cadence, "연간");
        assert_eq!(p[0].next_expected_date, Date::new(2027, 3, 1));
        let fallback_docs = vec![
            doc("x/a", ts(2018, 1, 1)),
            doc("x/b", ts(2019, 1, 1)),
            doc("x/c", ts(2020, 1, 1)),
        ];
        let fallback = detect_periodic_topics(&fallback_docs, &topics, &[None, None, None]);
        assert_eq!(fallback[0].last_date, Date::new(2020, 1, 1));
    }
    #[test]
    fn periodic_detects_quarterly_and_monthly_topics() {
        let ds = vec![
            ts(2025, 1, 1),
            ts(2025, 4, 2),
            ts(2025, 7, 2),
            ts(2026, 1, 1),
            ts(2026, 1, 31),
            ts(2026, 3, 2),
        ];
        let docs: Vec<_> = ds
            .iter()
            .enumerate()
            .map(|(i, &d)| doc(&format!("x/{i}"), d))
            .collect();
        let topics = vec![
            topic(1, "분기", vec![0, 1, 2]),
            topic(2, "월간", vec![3, 4, 5]),
        ];
        let p = detect_periodic_topics(&docs, &topics, &[]);
        assert_eq!(
            p.iter().map(|x| x.cadence.as_str()).collect::<Vec<_>>(),
            vec!["분기", "월간"]
        );
    }
    #[test]
    fn folder_history_tracks_dates_and_topics() {
        let docs = vec![
            doc("A/one", ts(2025, 1, 2)),
            doc("A/two", ts(2025, 4, 3)),
            doc("B/three", ts(2025, 2, 1)),
        ];
        let topics = vec![topic(0, "예산", vec![0, 1]), topic(1, "보안", vec![2])];
        let h = folder_histories(&docs, &topics, &[], 2);
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].folder, "A");
        assert_eq!(h[0].document_count, 2);
        assert_eq!(h[0].first_date, Date::new(2025, 1, 2));
        assert_eq!(h[0].last_date, Date::new(2025, 4, 3));
        assert_eq!(h[0].representative_topics, vec!["예산"]);
    }
    #[test]
    fn period_comparison_reports_increase_stable_and_decrease() {
        let docs = vec![
            doc("a/1", ts(2025, 2, 1)),
            doc("a/2", ts(2026, 2, 1)),
            doc("a/3", ts(2026, 7, 1)),
            doc("b/1", ts(2025, 2, 1)),
            doc("b/2", ts(2026, 2, 1)),
            doc("c/1", ts(2025, 2, 1)),
            doc("c/2", ts(2025, 7, 1)),
            doc("c/3", ts(2026, 2, 1)),
        ];
        let topics = vec![
            topic(0, "증가", vec![0, 1, 2]),
            topic(1, "유지", vec![3, 4]),
            topic(2, "감소", vec![5, 6, 7]),
        ];
        let changes = compare_periods(
            &docs,
            &topics,
            ComparisonPeriod::new(ts(2025, 1, 1), ts(2025, 12, 31)),
            ComparisonPeriod::new(ts(2026, 1, 1), ts(2026, 12, 31)),
        );
        assert_eq!(
            changes.iter().map(|x| x.delta).collect::<Vec<_>>(),
            vec![1, 0, -1]
        );
        assert_eq!(changes[0].before_count, 1);
        assert_eq!(changes[0].after_count, 2);
        let json = serde_json::to_string(&changes).unwrap();
        assert!(json.contains("\"delta\":-1"));
    }
}
