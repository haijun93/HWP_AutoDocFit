//! 사용자용 연보 마크다운과 엑셀 호환 CSV 출력.
use crate::{Date, FolderHistory, PeriodicPrediction, Report, TopicChange};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io, path::Path};

/// 기존 A1~A3 분석 JSON. A3 필드는 단계별 실행을 위해 선택 사항이다.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AnnualInput {
    #[serde(default, alias = "a1", alias = "report")]
    pub report: Option<Report>,
    #[serde(default, alias = "periodic_tasks", alias = "periodic")]
    pub periodic_topics: Vec<PeriodicPrediction>,
    #[serde(default, alias = "folders", alias = "folder_history")]
    pub folder_histories: Vec<FolderHistory>,
    #[serde(default, alias = "comparison", alias = "period_comparison")]
    pub topic_changes: Vec<TopicChange>,
    /// 이전 JSON 입력과의 호환을 위한 선택 필드. 최신 통계는 ISO 연도를 직접 보유한다.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub iso_week_years: BTreeMap<u32, i32>,
}

fn csv_cell(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
fn csv_row(cells: &[String]) -> String {
    cells
        .iter()
        .map(|s| csv_cell(s))
        .collect::<Vec<_>>()
        .join(",")
}
fn date(d: Date) -> String {
    d.to_string_ymd()
}

fn markdown_cell(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace("\r\n", "<br>")
        .replace(['\r', '\n'], "<br>")
}

/// 보고서 마크다운 문자열을 만든다.
pub fn render_markdown(input: &AnnualInput) -> String {
    let mut out = String::from("# 업무 인수인계 연보\n\n");
    if let Some(r) = &input.report {
        out.push_str(&format!(
            "## 분석 개요\n\n- 분석 문서: {}건\n- 입력 문서: {}건\n\n",
            r.analyzed_doc_count, r.total_input_docs
        ));
        out.push_str(
            "## 주제 비중\n\n| 주제 | 문서 수 | 비중 |
|---|---:|---:|\n",
        );
        for t in &r.stats.top_topics {
            out.push_str(&format!(
                "| {} | {} | {:.1}% |\n",
                markdown_cell(&t.topic_name),
                t.doc_count,
                t.percentage
            ));
        }
        out.push_str("\n## 월별 주제 흐름\n\n| 월 | 주제 | 문서 수 |\n|---|---|---:|\n");
        let mut topics = r.stats.top_topics.clone();
        topics.sort_by_key(|t| t.topic_id);
        for month in 1..=12 {
            for t in &topics {
                let count = r
                    .stats
                    .topic_by_month
                    .get(&t.topic_id)
                    .map(|a| a[month - 1])
                    .unwrap_or(0);
                out.push_str(&format!(
                    "| {month}월 | {} | {count} |\n",
                    markdown_cell(&t.topic_name)
                ));
            }
        }
    } else {
        out.push_str("분석 보고서(A1)가 입력에 없습니다.\n\n");
    }
    out.push_str("\n## 주기 업무\n\n");
    if input.periodic_topics.is_empty() {
        out.push_str("분석 결과 없음\n\n");
    } else {
        out.push_str("| 업무 | 주기 | 발생 | 최근 | 다음 예상 |\n|---|---|---:|---|---|\n");
        for p in &input.periodic_topics {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                markdown_cell(&p.topic),
                p.cadence,
                p.occurrence_count,
                date(p.last_date),
                date(p.next_expected_date)
            ));
        }
    }
    out.push_str("\n## 폴더별 이력\n\n");
    if input.folder_histories.is_empty() {
        out.push_str("분석 결과 없음\n\n");
    } else {
        out.push_str("| 폴더 | 문서 수 | 최초 | 최근 | 대표 주제 |\n|---|---:|---|---|---|\n");
        for f in &input.folder_histories {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                markdown_cell(&f.folder),
                f.document_count,
                date(f.first_date),
                date(f.last_date),
                markdown_cell(&f.representative_topics.join(", "))
            ));
        }
    }
    out.push_str("\n## 기간 비교\n\n");
    if input.topic_changes.is_empty() {
        out.push_str("분석 결과 없음\n");
    } else {
        out.push_str("| 주제 | 이전 | 이후 | 증감 |\n|---|---:|---:|---:|\n");
        for c in &input.topic_changes {
            out.push_str(&format!(
                "| {} | {} | {} | {:+} |\n",
                markdown_cell(&c.topic),
                c.before_count,
                c.after_count,
                c.delta
            ));
        }
    }
    out
}

pub fn monthly_topics_csv(input: &AnnualInput) -> String {
    let mut rows = vec![csv_row(&["월".into(), "주제".into(), "문서 수".into()])];
    if let Some(r) = &input.report {
        let mut topics = r.stats.top_topics.clone();
        topics.sort_by_key(|t| t.topic_id);
        for m in 1..=12 {
            for t in &topics {
                let n = r
                    .stats
                    .topic_by_month
                    .get(&t.topic_id)
                    .map(|x| x[m - 1])
                    .unwrap_or(0);
                rows.push(csv_row(&[
                    format!("{m}월"),
                    t.topic_name.clone(),
                    n.to_string(),
                ]));
            }
        }
    }
    rows.join("\r\n") + "\r\n"
}

/// 주제별 연도·월 문서 수를 시간순으로 Excel 호환 CSV로 만든다.
pub fn topic_timeline_csv(input: &AnnualInput) -> String {
    let mut rows = vec![csv_row(&["연도".into(), "월".into(), "주제".into(), "문서 수".into()])];
    if let Some(r) = &input.report {
        let mut topics = r.stats.top_topics.clone();
        topics.sort_by_key(|t| t.topic_id);
        for topic in topics {
            if let Some(months) = r.stats.topic_timeline_by_month.get(&topic.topic_id) {
                for (&(year, month), &count) in months {
                    rows.push(csv_row(&[
                        year.to_string(),
                        format!("{month}월"),
                        topic.topic_name.clone(),
                        count.to_string(),
                    ]));
                }
            }
        }
    }
    rows.join("\r\n") + "\r\n"
}
pub fn periodic_tasks_csv(input: &AnnualInput) -> String {
    let mut rows = vec![csv_row(&[
        "업무".into(),
        "주기".into(),
        "간격(일)".into(),
        "발생 횟수".into(),
        "최근 날짜".into(),
        "다음 예상 날짜".into(),
    ])];
    for p in &input.periodic_topics {
        rows.push(csv_row(&[
            p.topic.clone(),
            p.cadence.clone(),
            format!("{:.1}", p.median_interval_days),
            p.occurrence_count.to_string(),
            date(p.last_date),
            date(p.next_expected_date),
        ]));
    }
    rows.join("\r\n") + "\r\n"
}
pub fn folder_history_csv(input: &AnnualInput) -> String {
    let mut rows = vec![csv_row(&[
        "폴더".into(),
        "문서 수".into(),
        "최초 날짜".into(),
        "최근 날짜".into(),
        "대표 주제".into(),
    ])];
    for f in &input.folder_histories {
        rows.push(csv_row(&[
            f.folder.clone(),
            f.document_count.to_string(),
            date(f.first_date),
            date(f.last_date),
            f.representative_topics.join(", "),
        ]));
    }
    rows.join("\r\n") + "\r\n"
}

/// 주제별 ISO 연도·주차 문서 수를 Excel 호환 CSV로 만든다.
pub fn weekly_topics_csv(input: &AnnualInput) -> String {
    let mut rows = vec![csv_row(&[
        "ISO 연도".into(),
        "ISO 주차".into(),
        "주제".into(),
        "문서 수".into(),
    ])];
    if let Some(r) = &input.report {
        let mut topics = r.stats.top_topics.clone();
        topics.sort_by_key(|t| t.topic_id);
        for topic in topics {
            if let Some(weeks) = r.stats.topic_by_iso_week.get(&topic.topic_id) {
                for (&(year, week), &count) in weeks {
                    rows.push(csv_row(&[
                        year.to_string(),
                        week.to_string(),
                        topic.topic_name.clone(),
                        count.to_string(),
                    ]));
                }
            }
        }
    }
    rows.join("\r\n") + "\r\n"
}

/// ISO 주차별 요일 열지도를 Excel 호환 CSV로 만든다.
pub fn weekday_heatmap_csv(input: &AnnualInput) -> String {
    let mut rows = vec![csv_row(&[
        "ISO 연도".into(),
        "ISO 주차".into(),
        "월요일".into(),
        "화요일".into(),
        "수요일".into(),
        "목요일".into(),
        "금요일".into(),
        "토요일".into(),
        "일요일".into(),
    ])];
    if let Some(r) = &input.report {
        for (&(year, week), counts) in &r.stats.week_day_heatmap {
            let mut cells = vec![year.to_string(), week.to_string()];
            cells.extend(counts.iter().map(u32::to_string));
            rows.push(csv_row(&cells));
        }
    }
    rows.join("\r\n") + "\r\n"
}

/// 보고서와 BOM CSV 6종을 출력 폴더에 쓴다.
pub fn write_outputs(input: &AnnualInput, folder: impl AsRef<Path>) -> io::Result<()> {
    let folder = folder.as_ref();
    fs::create_dir_all(folder)?;
    fs::write(folder.join("인수인계_연보.md"), render_markdown(input))?;
    for (name, csv) in [
        ("월별_주제.csv", monthly_topics_csv(input)),
        ("주제별_타임라인.csv", topic_timeline_csv(input)),
        ("주차별_주제.csv", weekly_topics_csv(input)),
        ("요일_열지도.csv", weekday_heatmap_csv(input)),
        ("주기_업무.csv", periodic_tasks_csv(input)),
        ("폴더_이력.csv", folder_history_csv(input)),
    ] {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(csv.as_bytes());
        fs::write(folder.join(name), bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Stats, TopTopicSummary};
    fn sample() -> AnnualInput {
        let mut stats = Stats::default();
        stats.total_docs = 3;
        stats.top_topics = vec![TopTopicSummary {
            topic_id: 0,
            topic_name: "예산, \"정산\"\n업무".into(),
            doc_count: 2,
            percentage: 66.7,
        }];
        stats.topic_by_month.insert(0, {
            let mut a = [0; 12];
            a[0] = 2;
            a
        });
        AnnualInput {
            report: Some(Report {
                generated_at: 0,
                period: None,
                threshold: 0.5,
                total_input_docs: 3,
                analyzed_doc_count: 3,
                analyzed_indices: vec![],
                topics: vec![],
                version_lines: vec![],
                periodic_tasks: vec![],
                stats,
            }),
            periodic_topics: vec![PeriodicPrediction {
                topic_id: 0,
                topic: "정기, \"점검\"\n업무".into(),
                cadence: "월간".into(),
                median_interval_days: 30.0,
                occurrence_count: 4,
                last_date: Date::new(2026, 1, 2),
                next_expected_date: Date::new(2026, 2, 2),
                doc_indices: vec![],
            }],
            folder_histories: vec![FolderHistory {
                folder: "부서, \"가\"\n폴더".into(),
                document_count: 3,
                first_date: Date::new(2025, 1, 1),
                last_date: Date::new(2026, 1, 1),
                representative_topics: vec!["예산".into()],
            }],
            topic_changes: vec![TopicChange {
                topic_id: 0,
                topic: "예산".into(),
                before_count: 1,
                after_count: 2,
                delta: 1,
            }],
            iso_week_years: BTreeMap::new(),
        }
    }
    #[test]
    fn markdown_has_sections_and_period_comparison() {
        let s = render_markdown(&sample());
        for v in [
            "주제 비중",
            "월별 주제 흐름",
            "주기 업무",
            "폴더별 이력",
            "기간 비교",
            "+1",
        ] {
            assert!(s.contains(v));
        }
    }
    #[test]
    fn markdown_escapes_table_pipes() {
        let mut x = sample();
        x.topic_changes[0].topic = "A|B".into();
        assert!(render_markdown(&x).contains("A\\|B"));
    }
    #[test]
    fn markdown_reports_monthly_counts() {
        assert!(render_markdown(&sample()).contains("| 1월 | 예산, \"정산\"<br>업무 | 2 |"));
    }
    #[test]
    fn timeline_csv_contains_topic_month_rows_by_year() {
        let mut input = sample();
        input.report.as_mut().unwrap().stats.topic_timeline_by_month.insert(
            0,
            [((2025, 1), 2), ((2026, 1), 3), ((2026, 2), 1)].into(),
        );
        let csv = topic_timeline_csv(&input);
        assert!(csv.starts_with("\"연도\",\"월\",\"주제\",\"문서 수\"\r\n"));
        assert!(csv.contains("\"2025\",\"1월\",\"예산, \"\"정산\"\"\n업무\",\"2\""));
        assert!(csv.contains("\"2026\",\"1월\",\"예산, \"\"정산\"\"\n업무\",\"3\""));
        assert!(csv.contains("\"2026\",\"2월\",\"예산, \"\"정산\"\"\n업무\",\"1\""));
    }
    #[test]
    fn monthly_csv_header_and_row() {
        let s = monthly_topics_csv(&sample());
        assert!(s.starts_with("\"월\",\"주제\",\"문서 수\""));
        assert!(s.contains("\"1월\",\"예산, \"\"정산\"\"\n업무\",\"2\""));
    }
    #[test]
    fn monthly_csv_quotes_comma_quote_newline() {
        let s = monthly_topics_csv(&sample());
        assert!(s.contains("\"예산, \"\"정산\"\"\n업무\""));
    }
    #[test]
    fn periodic_csv_contains_dates_and_cadence() {
        let s = periodic_tasks_csv(&sample());
        assert!(s.contains("\"월간\""));
        assert!(s.contains("\"2026-02-02\""));
    }
    #[test]
    fn periodic_csv_escapes_special_text() {
        let s = periodic_tasks_csv(&sample());
        assert!(s.contains("\"정기, \"\"점검\"\"\n업무\""));
    }
    #[test]
    fn folder_csv_escapes_special_text() {
        let s = folder_history_csv(&sample());
        assert!(s.contains("\"부서, \"\"가\"\"\n폴더\""));
    }
    #[test]
    fn folder_csv_has_topic_list() {
        assert!(folder_history_csv(&sample()).contains("\"예산\""));
    }
    #[test]
    fn files_have_utf8_bom() {
        let d = tempfile::tempdir().unwrap();
        write_outputs(&sample(), d.path()).unwrap();
        for n in ["월별_주제.csv", "주제별_타임라인.csv", "주기_업무.csv", "폴더_이력.csv"] {
            let b = fs::read(d.path().join(n)).unwrap();
            assert_eq!(&b[..3], &[0xef, 0xbb, 0xbf]);
        }
    }
    #[test]
    fn output_writes_markdown_and_three_csvs() {
        let d = tempfile::tempdir().unwrap();
        write_outputs(&sample(), d.path()).unwrap();
        for n in [
            "인수인계_연보.md",
            "월별_주제.csv",
            "주제별_타임라인.csv",
            "주차별_주제.csv",
            "요일_열지도.csv",
            "주기_업무.csv",
            "폴더_이력.csv",
        ] {
            assert!(d.path().join(n).is_file());
        }
    }

    #[test]
    fn weekly_and_weekday_csv_include_headers_and_statistics() {
        let mut input = sample();
        let report = input.report.as_mut().unwrap();
        report.stats.topic_by_iso_week.insert(0, [((2025, 3), 1), ((2026, 3), 2)].into());
        report
            .stats
            .week_day_heatmap
            .insert((2025, 3), [1, 0, 0, 0, 0, 0, 0]);
        report.stats.week_day_heatmap.insert((2026, 3), [0, 0, 2, 0, 0, 0, 0]);

        let weekly = weekly_topics_csv(&input);
        assert!(weekly.starts_with("\"ISO 연도\",\"ISO 주차\",\"주제\",\"문서 수\"\r\n"));
        assert!(weekly.contains("\"2025\",\"3\",\"예산, \"\"정산\"\"\n업무\",\"1\""));
        assert!(weekly.contains("\"2026\",\"3\",\"예산, \"\"정산\"\"\n업무\",\"2\""));

        let heatmap = weekday_heatmap_csv(&input);
        assert!(heatmap.starts_with("\"ISO 연도\",\"ISO 주차\",\"월요일\",\"화요일\""));
        assert!(heatmap.contains("\"2025\",\"3\",\"1\",\"0\",\"0\",\"0\",\"0\",\"0\",\"0\""));
        assert!(heatmap.contains("\"2026\",\"3\",\"0\",\"0\",\"2\",\"0\",\"0\",\"0\",\"0\""));
    }

    #[test]
    fn weekly_and_weekday_csv_are_written_with_utf8_bom() {
        let d = tempfile::tempdir().unwrap();
        write_outputs(&sample(), d.path()).unwrap();
        for name in ["주제별_타임라인.csv", "주차별_주제.csv", "요일_열지도.csv"] {
            let bytes = fs::read(d.path().join(name)).unwrap();
            assert_eq!(&bytes[..3], &[0xef, 0xbb, 0xbf]);
        }
    }
}
