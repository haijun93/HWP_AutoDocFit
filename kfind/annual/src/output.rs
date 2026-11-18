//! 사용자용 연보 마크다운과 엑셀 호환 CSV 출력.
use crate::{Date, FolderHistory, PeriodicPrediction, Report, TopicChange};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io, path::Path};

/// SVG chart palette. Keeping every chart color here makes visual changes consistent.
pub mod chart_colors {
    pub const BACKGROUND: &str = "#ffffff";
    pub const TEXT: &str = "#243044";
    pub const GRID: &str = "#d9e0ea";
    pub const BAR: &str = "#3973ac";
    pub const HEAT_LOW: &str = "#edf3f8";
    pub const HEAT_HIGH: &str = "#245b8f";
    pub const TIMELINE: &str = "#3973ac";
}

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

fn xml_escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        .replace('"', "&quot;").replace('\'', "&apos;")
}

fn svg_start(title: &str, description: &str, width: u32, height: u32) -> String {
    format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\" role=\"img\" aria-labelledby=\"chart-title chart-desc\"><title id=\"chart-title\">{}</title><desc id=\"chart-desc\">{}</desc><rect width=\"100%\" height=\"100%\" fill=\"{}\"/><g font-family=\"'Apple SD Gothic Neo', 'Malgun Gothic', 'Nanum Gothic', sans-serif\" fill=\"{}\">",
        xml_escape(title), xml_escape(description), chart_colors::BACKGROUND, chart_colors::TEXT)
}

/// Render total monthly document counts as a standalone UTF-8 SVG bar chart.
pub fn monthly_documents_svg(input: &AnnualInput) -> String {
    const W: u32 = 760; const H: u32 = 360;
    let mut svg = svg_start("월별 문서 수", "1월부터 12월까지 월별 문서 수 막대 그래프", W, H);
    svg.push_str("<text x=\"380\" y=\"28\" text-anchor=\"middle\" font-size=\"18\">월별 문서 수</text>");
    let mut values = [0u64; 12];
    if let Some(report) = &input.report {
        for counts in report.stats.topic_by_month.values() {
            for (i, &count) in counts.iter().enumerate() { values[i] += u64::from(count); }
        }
    }
    let max = values.iter().copied().max().unwrap_or(0);
    for (i, value) in values.iter().enumerate() {
        let x = 52 + i as u32 * 58;
        let bh = if max == 0 { 0 } else { (value * 240 / max) as u32 };
        let y = 300 - bh;
        svg.push_str(&format!("<line x1=\"{x}\" y1=\"60\" x2=\"{x}\" y2=\"300\" stroke=\"{}\"/><rect x=\"{}\" y=\"{y}\" width=\"32\" height=\"{bh}\" fill=\"{}\"/><text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-size=\"11\">{value}</text><text x=\"{}\" y=\"322\" text-anchor=\"middle\" font-size=\"12\">{}월</text>", chart_colors::GRID, x+4, chart_colors::BAR, x+20, if bh == 0 { 292 } else { y.saturating_sub(5) }, x+20, i+1));
    }
    svg.push_str("<line x1=\"40\" y1=\"300\" x2=\"740\" y2=\"300\" stroke=\"#667085\"/></g></svg>");
    svg
}

/// Render each ISO year-week and weekday count as a labeled heatmap cell.
pub fn weekday_heatmap_svg(input: &AnnualInput) -> String {
    const W: u32 = 760;
    let rows = input.report.as_ref().map_or(0, |r| r.stats.week_day_heatmap.len());
    let height = (100 + rows as u32 * 26).max(140);
    let mut svg = svg_start("주차별 요일 열지도", "ISO 연도와 주차별 문서 수를 월요일부터 일요일까지 색으로 표시", W, height);
    svg.push_str("<text x=\"380\" y=\"24\" text-anchor=\"middle\" font-size=\"18\">주차별 요일 열지도</text>");
    let weekdays = ["월", "화", "수", "목", "금", "토", "일"];
    for (i, day) in weekdays.iter().enumerate() {
        svg.push_str(&format!("<text x=\"{}\" y=\"52\" text-anchor=\"middle\" font-size=\"12\">{day}</text>", 158+i*76));
    }
    if let Some(report) = &input.report {
        let max = report.stats.week_day_heatmap.values().flat_map(|v| v.iter()).copied().max().unwrap_or(0);
        for (row, (&(year, week), counts)) in report.stats.week_day_heatmap.iter().enumerate() {
            let y = 62 + row as u32 * 26;
            svg.push_str(&format!("<text x=\"145\" y=\"{}\" text-anchor=\"end\" font-size=\"11\">{year}-W{week:02}</text>", y+16));
            for (col, count) in counts.iter().enumerate() {
                let color = if max == 0 || *count == 0 { chart_colors::HEAT_LOW } else if u64::from(*count)*2 >= u64::from(max) { chart_colors::HEAT_HIGH } else { "#91b4d1" };
                let x = 158 + col as u32 * 76;
                svg.push_str(&format!("<rect x=\"{x}\" y=\"{y}\" width=\"68\" height=\"22\" rx=\"3\" fill=\"{color}\"/><text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-size=\"11\">{count}</text>", x+34, y+15));
            }
        }
    }
    svg.push_str("</g></svg>"); svg
}

/// Render per-topic document counts across calendar months as a timeline.
pub fn topic_timeline_svg(input: &AnnualInput) -> String {
    const W: u32 = 900;
    let topic_count = input.report.as_ref().map_or(0, |r| r.stats.top_topics.len());
    let height = (100 + topic_count as u32 * 42).max(140);
    let mut svg = svg_start("주제별 월 타임라인", "주제별로 월 단위 문서 수를 시간순으로 표시", W, height);
    svg.push_str("<text x=\"450\" y=\"24\" text-anchor=\"middle\" font-size=\"18\">주제별 월 타임라인</text>");
    if let Some(report) = &input.report {
        let mut topics = report.stats.top_topics.clone(); topics.sort_by_key(|t| t.topic_id);
        let month_keys: Vec<(i32,u32)> = report.stats.topic_timeline_by_month.values().flat_map(|m| m.keys().copied()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        let max = report.stats.topic_timeline_by_month.values().flat_map(|m| m.values()).copied().max().unwrap_or(0);
        if !month_keys.is_empty() {
            for (i, &(year, month)) in month_keys.iter().enumerate() {
                let x = 190 + i as u32 * (660 / month_keys.len().max(1) as u32);
                svg.push_str(&format!("<text x=\"{x}\" y=\"52\" text-anchor=\"middle\" font-size=\"10\">{year}-{month:02}</text>"));
            }
        }
        for (row, topic) in topics.iter().enumerate() {
            let y = 65 + row as u32 * 42;
            svg.push_str(&format!("<text x=\"178\" y=\"{}\" text-anchor=\"end\" font-size=\"11\">{}</text>", y+13, xml_escape(&topic.topic_name)));
            if let Some(months) = report.stats.topic_timeline_by_month.get(&topic.topic_id) {
                for (i, key) in month_keys.iter().enumerate() {
                    let count = months.get(key).copied().unwrap_or(0);
                    let x = 190 + i as u32 * (660 / month_keys.len().max(1) as u32);
                    let width = if max == 0 { 0 } else { (count as u64 * 34 / u64::from(max)) as u32 };
                    svg.push_str(&format!("<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"3\"/><circle cx=\"{}\" cy=\"{}\" r=\"4\" fill=\"{}\"/><text x=\"{}\" y=\"{}\" font-size=\"9\">{count}</text>", x, y+9, x+width, y+9, chart_colors::TIMELINE, x+width, y+9, chart_colors::TIMELINE, x+width+4, y+12));
                }
            }
        }
    }
    svg.push_str("</g></svg>"); svg
}

/// Write a complete SVG via a sibling temporary file, then atomically replace the destination.
pub fn write_svg(path: impl AsRef<Path>, svg: &str) -> io::Result<()> {
    let path = path.as_ref();
    let mut temp = path.as_os_str().to_owned(); temp.push(".tmp");
    let temp = std::path::PathBuf::from(temp);
    fs::write(&temp, svg.as_bytes())?;
    if let Err(error) = fs::rename(&temp, path) { let _ = fs::remove_file(&temp); return Err(error); }
    Ok(())
}

/// 보고서와 BOM CSV 6종을 출력 폴더에 쓴다.
pub fn write_outputs(input: &AnnualInput, folder: impl AsRef<Path>) -> io::Result<()> {
    let folder = folder.as_ref();
    fs::create_dir_all(folder)?;
    fs::write(folder.join("인수인계_연보.md"), render_markdown(input))?;
    write_svg(folder.join("월별_문서수.svg"), &monthly_documents_svg(input))?;
    write_svg(folder.join("주차_요일_열지도.svg"), &weekday_heatmap_svg(input))?;
    write_svg(folder.join("주제별_월_타임라인.svg"), &topic_timeline_svg(input))?;
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

    fn assert_well_formed_svg(svg: &str) {
        let mut stack: Vec<&str> = Vec::new();
        let mut rest = svg;
        while let Some(start) = rest.find('<') {
            rest = &rest[start + 1..];
            let end = rest.find('>').expect("closed tag");
            let tag = rest[..end].trim();
            if tag.starts_with('?') || tag.starts_with('!') { rest = &rest[end + 1..]; continue; }
            if let Some(closing) = tag.strip_prefix('/') {
                assert_eq!(stack.pop(), Some(closing), "mismatched XML close tag {tag}");
            } else if !tag.ends_with('/') {
                stack.push(tag.split_whitespace().next().unwrap());
            }
            rest = &rest[end + 1..];
        }
        assert!(stack.is_empty(), "unclosed XML tags: {stack:?}");
        assert!(rest.is_empty() || !rest.contains('<'));
    }

    #[test]
    fn chart_svgs_are_balanced_and_empty_input_is_valid() {
        let input = AnnualInput::default();
        for svg in [monthly_documents_svg(&input), weekday_heatmap_svg(&input), topic_timeline_svg(&input)] {
            assert!(svg.starts_with("<svg "));
            assert!(svg.contains("<title") && svg.contains("<desc"));
            assert_well_formed_svg(&svg);
        }
    }

    #[test]
    fn monthly_bars_scale_proportionally_and_svg_topic_text_is_escaped() {
        let mut input = sample();
        let report = input.report.as_mut().unwrap();
        report.stats.topic_by_month.insert(0, { let mut a = [0; 12]; a[0] = 2; a[1] = 4; a });
        report.stats.top_topics[0].topic_name = "예산 <정산> & \"보고\"".into();
        report.stats.topic_timeline_by_month.insert(0, [((2020, 12), 2), ((2021, 1), 4)].into());
        report.stats.week_day_heatmap.insert((2020, 53), [1, 0, 0, 0, 0, 0, 0]);
        let bars = monthly_documents_svg(&input);
        assert!(bars.contains("y=\"180\" width=\"32\" height=\"120\""));
        assert!(bars.contains("y=\"60\" width=\"32\" height=\"240\""));
        assert_well_formed_svg(&bars);
        let timeline = topic_timeline_svg(&input);
        assert!(timeline.contains("예산 &lt;정산&gt; &amp; &quot;보고&quot;"));
        assert!(timeline.contains("2020-12") && timeline.contains("2021-01"));
        assert_well_formed_svg(&timeline);
        let heatmap = weekday_heatmap_svg(&input);
        assert!(heatmap.contains("2020-W53"));
        assert_well_formed_svg(&heatmap);
    }

    #[test]
    fn svg_file_writer_replaces_destination_from_temporary_file() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("chart.svg");
        fs::write(&path, "old").unwrap();
        write_svg(&path, "<svg>새 차트</svg>").unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "<svg>새 차트</svg>");
    }
}
