//! Minimal native-table HWPX reports for annual handover and performance summaries.
use crate::{Doc, Report, output::AnnualInput};
use std::{fs, io, io::Write, path::Path};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

pub const BODY_WIDTH_MM: f64 = 170.0;
const BODY_WIDTH_HWP: u32 = 48_189;
const TITLE_CHAR: u32 = 0;
const BODY_CHAR: u32 = 1;
const HEADER_BORDER: u32 = 1;
const BODY_BORDER: u32 = 2;
fn options() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)
}

#[derive(Clone)]
struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}
fn esc(s: &str) -> String {
    s.chars()
        .filter(|c| matches!(*c as u32, 0x09 | 0x0A | 0x0D | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF))
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&apos;".into(),
            _ => c.to_string(),
        })
        .collect()
}
fn p(text: &str, char_id: u32, id: u32) -> String {
    format!(
        "<hp:p id=\"{id}\" paraPrIDRef=\"0\" styleIDRef=\"0\" pageBreak=\"0\" columnBreak=\"0\" merged=\"0\"><hp:run charPrIDRef=\"{char_id}\"><hp:t>{}</hp:t></hp:run></hp:p>",
        esc(text)
    )
}
fn cell(
    text: &str,
    table_id: u32,
    r: usize,
    c: usize,
    width: u32,
    border: u32,
    row_height: u32,
) -> String {
    format!(
        "<hp:tc name=\"\" header=\"0\" hasMargin=\"1\" protect=\"0\" editable=\"0\" dirty=\"0\" borderFillIDRef=\"{border}\"><hp:subList id=\"0\" textDirection=\"HORIZONTAL\" lineWrap=\"BREAK\" vertAlign=\"CENTER\" linkListIDRef=\"0\" linkListNextIDRef=\"0\" textWidth=\"0\" textHeight=\"0\" hasTextRef=\"0\" hasNumRef=\"0\">{}</hp:subList><hp:cellAddr rowAddr=\"{r}\" colAddr=\"{c}\"/><hp:cellSpan rowSpan=\"1\" colSpan=\"1\"/><hp:cellSz width=\"{width}\" height=\"{row_height}\"/><hp:cellMargin left=\"140\" right=\"140\" top=\"90\" bottom=\"90\"/></hp:tc>",
        p(
            text,
            BODY_CHAR,
            table_id * 10_000 + (r as u32) * 100 + c as u32
        )
    )
}
fn table_xml(table: &Table, object_id: u32) -> String {
    let cols = table.headers.len().max(1);
    let rows = table.rows.len() + 1;
    let weights: Vec<u32> = match table.headers.first().map(String::as_str) {
        Some("월") => vec![30, 70],
        Some("상위 주제") => vec![60, 20, 20],
        _ => vec![25, 30, 45],
    };
    let weights = if weights.len() == cols {
        weights
    } else {
        vec![1; cols]
    };
    let total_weight: u32 = weights.iter().sum();
    let mut allocated = 0;
    let widths = weights
        .iter()
        .enumerate()
        .map(|(i, weight)| {
            let width = if i + 1 == cols {
                BODY_WIDTH_HWP - allocated
            } else {
                BODY_WIDTH_HWP * weight / total_weight
            };
            allocated += width;
            width
        })
        .collect::<Vec<_>>();
    let mut out = format!(
        "<hp:tbl id=\"{object_id}\" rowCnt=\"{rows}\" colCnt=\"{cols}\" borderFillIDRef=\"2\" repeatHeader=\"1\" pageBreak=\"CELL\" cellSpacing=\"0\" noAdjust=\"0\"><hp:sz width=\"{BODY_WIDTH_HWP}\" height=\"0\" widthRelTo=\"ABSOLUTE\" heightRelTo=\"ABSOLUTE\" protect=\"0\"/><hp:pos treatAsChar=\"1\" affectLSpacing=\"0\" flowWithText=\"1\" allowOverlap=\"0\" holdAnchorAndSO=\"0\" vertRelTo=\"PARA\" horzRelTo=\"COLUMN\" vertAlign=\"TOP\" horzAlign=\"LEFT\" vertOffset=\"0\" horzOffset=\"0\"/><hp:inMargin left=\"0\" right=\"0\" top=\"0\" bottom=\"0\"/><hp:outMargin left=\"0\" right=\"0\" top=\"0\" bottom=\"0\"/><hp:tr>"
    );
    for (i, h) in table.headers.iter().enumerate() {
        out.push_str(&cell(h, object_id, 0, i, widths[i], HEADER_BORDER, 700));
    }
    out.push_str("</hp:tr>");
    for (ri, row) in table.rows.iter().enumerate() {
        out.push_str("<hp:tr>");
        for ci in 0..cols {
            out.push_str(&cell(
                row.get(ci).map(String::as_str).unwrap_or(""),
                object_id,
                ri + 1,
                ci,
                widths[ci],
                BODY_BORDER,
                700,
            ));
        }
        out.push_str("</hp:tr>");
    }
    out.push_str("</hp:tbl>");
    format!(
        "<hp:p id=\"{object_id}\" paraPrIDRef=\"0\" styleIDRef=\"0\" pageBreak=\"0\" columnBreak=\"0\" merged=\"0\"><hp:run charPrIDRef=\"{BODY_CHAR}\">{out}</hp:run></hp:p>"
    )
}

fn section_xml(title: &str, paragraphs: &[String], tables: &[Table]) -> String {
    let sec_pr = "<hp:p id=\"1\" paraPrIDRef=\"0\" styleIDRef=\"0\" pageBreak=\"0\" columnBreak=\"0\" merged=\"0\"><hp:run charPrIDRef=\"1\"><hp:secPr id=\"\" textDirection=\"HORIZONTAL\" spaceColumns=\"1134\" tabStop=\"8000\" tabStopVal=\"4000\" tabStopUnit=\"HWPUNIT\" outlineShapeIDRef=\"1\" memoShapeIDRef=\"0\" textVerticalWidthHead=\"0\" masterPageCnt=\"0\"><hp:grid lineGrid=\"0\" charGrid=\"0\" wonggojiFormat=\"0\"/><hp:startNum pageStartsOn=\"BOTH\" page=\"0\" pic=\"0\" tbl=\"0\" equation=\"0\"/><hp:visibility hideFirstHeader=\"0\" hideFirstFooter=\"0\" hideFirstMasterPage=\"0\" border=\"SHOW_ALL\" fill=\"SHOW_ALL\" hideFirstPageNum=\"0\" hideFirstEmptyLine=\"0\" showLineNumber=\"0\"/><hp:lineNumberShape restartType=\"0\" countBy=\"0\" distance=\"0\" startNumber=\"0\"/><hp:pagePr landscape=\"WIDELY\" width=\"59528\" height=\"84188\" gutterType=\"LEFT_ONLY\"><hp:margin header=\"3600\" footer=\"3600\" gutter=\"0\" left=\"5669\" right=\"5669\" top=\"3600\" bottom=\"3600\"/></hp:pagePr><hp:footNotePr><hp:autoNumFormat type=\"DIGIT\" userChar=\"\" prefixChar=\"\" suffixChar=\")\" supscript=\"0\"/><hp:noteLine length=\"-1\" type=\"SOLID\" width=\"0.12 mm\" color=\"#000000\"/><hp:noteSpacing betweenNotes=\"283\" belowLine=\"567\" aboveLine=\"850\"/><hp:numbering type=\"CONTINUOUS\" newNum=\"1\"/><hp:placement place=\"EACH_COLUMN\" beneathText=\"0\"/></hp:footNotePr><hp:endNotePr><hp:autoNumFormat type=\"DIGIT\" userChar=\"\" prefixChar=\"\" suffixChar=\")\" supscript=\"0\"/><hp:noteLine length=\"-1\" type=\"SOLID\" width=\"0.12 mm\" color=\"#000000\"/><hp:noteSpacing betweenNotes=\"0\" belowLine=\"567\" aboveLine=\"850\"/><hp:numbering type=\"CONTINUOUS\" newNum=\"1\"/><hp:placement place=\"END_OF_DOCUMENT\" beneathText=\"0\"/></hp:endNotePr><hp:pageBorderFill type=\"BOTH\" borderFillIDRef=\"2\" textBorder=\"PAPER\" headerInside=\"0\" footerInside=\"0\" fillArea=\"PAPER\"><hp:offset left=\"0\" right=\"0\" top=\"0\" bottom=\"0\"/></hp:pageBorderFill></hp:secPr></hp:run></hp:p>";
    let mut body = String::from(sec_pr);
    body.push_str(&p(title, TITLE_CHAR, 2));
    for (i, para) in paragraphs.iter().enumerate() {
        body.push_str(&p(para, BODY_CHAR, 3 + i as u32));
    }
    for (i, table) in tables.iter().enumerate() {
        body.push_str(&table_xml(table, 10 + i as u32));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><hs:sec xmlns:ha=\"http://www.hancom.co.kr/hwpml/2011/app\" xmlns:hp=\"http://www.hancom.co.kr/hwpml/2011/paragraph\" xmlns:hs=\"http://www.hancom.co.kr/hwpml/2011/section\" xmlns:hc=\"http://www.hancom.co.kr/hwpml/2011/core\" xmlns:hh=\"http://www.hancom.co.kr/hwpml/2011/head\">{body}</hs:sec>"
    )
}
fn header_xml() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><hh:head xmlns:hh=\"http://www.hancom.co.kr/hwpml/2011/head\" xmlns:hc=\"http://www.hancom.co.kr/hwpml/2011/core\" version=\"1.5.0.0\" secCnt=\"1\"><hh:beginNum page=\"1\" footnote=\"1\" endnote=\"1\" picture=\"1\" table=\"1\" equation=\"1\"/><hh:refList><hh:fontfaces itemCnt=\"1\"><hh:fontface lang=\"HANGUL\" fontCnt=\"2\"><hh:font id=\"0\" face=\"휴먼명조\" type=\"TTF\" isEmbedded=\"0\"/><hh:font id=\"1\" face=\"한컴돋움\" type=\"TTF\" isEmbedded=\"0\"/></hh:fontface></hh:fontfaces><hh:borderFills itemCnt=\"2\"><hh:borderFill id=\"1\" threeD=\"0\" shadow=\"0\" centerLine=\"NONE\" breakCellSeparateLine=\"0\"><hh:slash type=\"NONE\"/><hh:backSlash type=\"NONE\"/><hh:leftBorder type=\"SOLID\" width=\"0.12mm\" color=\"#AAB7C6\"/><hh:rightBorder type=\"SOLID\" width=\"0.12mm\" color=\"#AAB7C6\"/><hh:topBorder type=\"SOLID\" width=\"0.12mm\" color=\"#AAB7C6\"/><hh:bottomBorder type=\"SOLID\" width=\"0.12mm\" color=\"#AAB7C6\"/><hh:diagonal type=\"SOLID\" width=\"0.1mm\" color=\"#AAB7C6\"/><hh:fillBrush><hh:winBrush faceColor=\"#E7EEF8\" hatchColor=\"#000000\" alpha=\"0\"/></hh:fillBrush></hh:borderFill><hh:borderFill id=\"2\" threeD=\"0\" shadow=\"0\" centerLine=\"NONE\" breakCellSeparateLine=\"0\"><hh:slash type=\"NONE\"/><hh:backSlash type=\"NONE\"/><hh:leftBorder type=\"SOLID\" width=\"0.12mm\" color=\"#AAB7C6\"/><hh:rightBorder type=\"SOLID\" width=\"0.12mm\" color=\"#AAB7C6\"/><hh:topBorder type=\"SOLID\" width=\"0.12mm\" color=\"#AAB7C6\"/><hh:bottomBorder type=\"SOLID\" width=\"0.12mm\" color=\"#AAB7C6\"/><hh:diagonal type=\"SOLID\" width=\"0.1mm\" color=\"#AAB7C6\"/><hh:fillBrush><hh:winBrush faceColor=\"#FFFFFF\" hatchColor=\"#000000\" alpha=\"0\"/></hh:fillBrush></hh:borderFill></hh:borderFills><hh:charProperties itemCnt=\"2\"><hh:charPr id=\"0\" height=\"1600\" textColor=\"#1F2937\" shadeColor=\"#FFFFFF\" useFontSpace=\"0\" useKerning=\"0\"><hh:fontRef hangul=\"1\" latin=\"1\"/><hh:ratio hangul=\"100\" latin=\"100\"/><hh:spacing hangul=\"0\" latin=\"0\"/><hh:relSz hangul=\"100\" latin=\"100\"/><hh:offset hangul=\"0\" latin=\"0\"/><hh:bold/><hh:underline type=\"NONE\" shape=\"SOLID\" color=\"#000000\"/><hh:strikeout shape=\"NONE\" color=\"#000000\"/><hh:outline type=\"NONE\"/><hh:shadow type=\"NONE\"/></hh:charPr><hh:charPr id=\"1\" height=\"1100\" textColor=\"#202020\" shadeColor=\"#FFFFFF\" useFontSpace=\"0\" useKerning=\"0\"><hh:fontRef hangul=\"0\" latin=\"0\"/><hh:ratio hangul=\"100\" latin=\"100\"/><hh:spacing hangul=\"0\" latin=\"0\"/><hh:relSz hangul=\"100\" latin=\"100\"/><hh:offset hangul=\"0\" latin=\"0\"/><hh:underline type=\"NONE\" shape=\"SOLID\" color=\"#000000\"/><hh:strikeout shape=\"NONE\" color=\"#000000\"/><hh:outline type=\"NONE\"/><hh:shadow type=\"NONE\"/></hh:charPr></hh:charProperties><hh:paraProperties itemCnt=\"1\"><hh:paraPr id=\"0\" align=\"JUSTIFY\" lineSpacingType=\"PERCENT\" lineSpacing=\"160\"/></hh:paraProperties></hh:refList><hh:styles itemCnt=\"1\"><hh:style id=\"0\" type=\"PARA\" name=\"Normal\" engName=\"Normal\" paraPrIDRef=\"0\" charPrIDRef=\"1\" nextStyleIDRef=\"0\" langID=\"1042\" lockForm=\"0\"/></hh:styles></hh:head>"
    )
}
fn package(path: &Path, section: &str) -> io::Result<()> {
    let f = fs::File::create(path)?;
    let mut z = ZipWriter::new(f);
    z.start_file(
        "mimetype",
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
    )?;
    z.write_all(b"application/hwp+zip")?;
    for (name, content) in [
        (
            "version.xml",
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ha:version version=\"1.5.0.0\" xmlns:ha=\"http://www.hancom.co.kr/hwpml/2011/app\"/>",
        ),
        (
            "META-INF/container.xml",
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ocf:container xmlns:ocf=\"urn:oasis:names:tc:opendocument:xmlns:container\"><ocf:rootfiles><ocf:rootfile full-path=\"Contents/content.hpf\" media-type=\"application/hwpml-package+xml\"/></ocf:rootfiles></ocf:container>",
        ),
        (
            "META-INF/manifest.xml",
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><opf:manifest xmlns:opf=\"http://www.idpf.org/2007/opf\"><opf:item id=\"header\" href=\"Contents/header.xml\" media-type=\"application/xml\"/><opf:item id=\"section0\" href=\"Contents/section0.xml\" media-type=\"application/xml\"/></opf:manifest>",
        ),
    ] {
        z.start_file(name, options())?;
        z.write_all(content.as_bytes())?;
    }
    z.start_file("Contents/header.xml", options())?;
    z.write_all(header_xml().as_bytes())?;
    z.start_file("Contents/section0.xml", options())?;
    z.write_all(section.as_bytes())?;
    z.start_file("Contents/content.hpf", options())?;
    z.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><opf:package xmlns:opf=\"http://www.idpf.org/2007/opf\" version=\"3.0\"><opf:metadata/><opf:manifest><opf:item id=\"header\" href=\"header.xml\" media-type=\"application/xml\"/><opf:item id=\"section0\" href=\"section0.xml\" media-type=\"application/xml\"/></opf:manifest><opf:spine><opf:itemref idref=\"section0\"/></opf:spine></opf:package>")?;
    z.finish()?;
    Ok(())
}
fn report_period(r: &Report) -> String {
    r.period
        .map(|p| {
            format!(
                "{} ~ {}",
                crate::Date::from_timestamp(p.start, crate::KST_OFFSET_SECS).to_string_ymd(),
                crate::Date::from_timestamp(p.end, crate::KST_OFFSET_SECS).to_string_ymd()
            )
        })
        .unwrap_or_else(|| "전체 기간".into())
}
fn docs_for_topic<'a>(input: &'a AnnualInput, r: &Report, indices: &[usize]) -> Vec<&'a Doc> {
    indices
        .iter()
        .filter_map(|&i| {
            r.analyzed_indices
                .get(i)
                .and_then(|&original| input.docs.get(original))
        })
        .collect()
}
fn handover(input: &AnnualInput) -> (Vec<String>, Vec<Table>) {
    let Some(r) = input.report.as_ref() else {
        return (
            vec!["분석 보고서가 입력되지 않았습니다.".into()],
            vec![Table {
                headers: vec![
                    "주제·업무".into(),
                    "정기 업무·문서 이력".into(),
                    "최근 문서 (최대 3건)".into(),
                ],
                rows: vec![vec![
                    "분석된 주제 없음".into(),
                    "자료 없음".into(),
                    "문서 정보 없음".into(),
                ]],
            }],
        );
    };
    let summary = format!(
        "분석 기간 {} 동안 문서 {}건을 분석하여 주제 {}개와 정기 업무 {}건을 확인했습니다.",
        report_period(r),
        r.analyzed_doc_count,
        r.topics.len(),
        r.periodic_tasks.len()
    );
    let mut rows = Vec::new();
    for t in &r.topics {
        let periodic = r
            .periodic_tasks
            .iter()
            .filter(|x| x.topic_id == t.id)
            .map(|x| format!("{} ({})", x.task_name, x.periodicity.label()))
            .collect::<Vec<_>>()
            .join(", ");
        let history = r
            .version_lines
            .iter()
            .filter(|line| line.doc_indices.iter().any(|i| t.doc_indices.contains(i)))
            .map(|line| line.name_norm.clone())
            .collect::<Vec<_>>()
            .join(", ");
        let mut recent = docs_for_topic(input, r, &t.doc_indices);
        recent.sort_by_key(|d| std::cmp::Reverse(d.modified));
        let recent_text = recent
            .iter()
            .take(3)
            .map(|d| format!("{} — {}", d.title, d.path))
            .collect::<Vec<_>>()
            .join("\n");
        rows.push(vec![
            t.name.clone(),
            format!(
                "정기 업무: {}\n문서 이력: {}\n담당자 변천: 입력 정보 없음",
                periodic.if_empty("확인된 정기 업무 없음"),
                history.if_empty("확인된 버전 이력 없음")
            ),
            recent_text.if_empty("문서 정보 없음"),
        ]);
    }
    if rows.is_empty() {
        rows.push(vec!["분석된 주제 없음".into(), "-".into(), "-".into()]);
    }
    (
        vec![summary],
        vec![Table {
            headers: vec![
                "주제·업무".into(),
                "정기 업무·문서 이력".into(),
                "최근 문서 (최대 3건)".into(),
            ],
            rows,
        }],
    )
}
trait EmptyFallback {
    fn if_empty(self, fallback: &str) -> String;
}
impl EmptyFallback for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.into()
        } else {
            self
        }
    }
}
fn performance(input: &AnnualInput) -> (Vec<String>, Vec<Table>) {
    let Some(r) = input.report.as_ref() else {
        return (
            vec!["분석 보고서가 입력되지 않았습니다.".into()],
            vec![
                Table {
                    headers: vec!["월".into(), "문서 수".into()],
                    rows: (1..=12)
                        .map(|m| vec![format!("{m}월"), "0".into()])
                        .collect(),
                },
                Table {
                    headers: vec!["상위 주제".into(), "문서 수".into(), "비중".into()],
                    rows: vec![vec!["자료 없음".into(), "0".into(), "0.0%".into()]],
                },
            ],
        );
    };
    let mut months = Vec::new();
    for month in 1..=12 {
        let count: u32 = r.stats.topic_by_month.values().map(|a| a[month - 1]).sum();
        months.push(vec![format!("{month}월"), count.to_string()]);
    }
    let mut topics = r.stats.top_topics.clone();
    topics.sort_by(|a, b| {
        b.doc_count
            .cmp(&a.doc_count)
            .then_with(|| a.topic_id.cmp(&b.topic_id))
    });
    let top_rows = topics
        .iter()
        .take(10)
        .map(|t| {
            vec![
                t.topic_name.clone(),
                t.doc_count.to_string(),
                format!("{:.1}%", t.percentage),
            ]
        })
        .collect::<Vec<_>>();
    let sentence = if let Some(t) = topics.first() {
        format!(
            "가장 많은 문서가 확인된 주제는 ‘{}’이며, {}건({:.1}%)입니다.",
            t.topic_name, t.doc_count, t.percentage
        )
    } else {
        "분석된 주제가 없습니다.".into()
    };
    (
        vec![
            format!(
                "분석 기간 {} · 분석 문서 {}건",
                report_period(r),
                r.analyzed_doc_count
            ),
            sentence,
        ],
        vec![
            Table {
                headers: vec!["월".into(), "문서 수".into()],
                rows: months,
            },
            Table {
                headers: vec!["상위 주제".into(), "문서 수".into(), "비중".into()],
                rows: if top_rows.is_empty() {
                    vec![vec!["자료 없음".into(), "0".into(), "0.0%".into()]]
                } else {
                    top_rows
                },
            },
        ],
    )
}
pub fn render_handover(input: &AnnualInput) -> String {
    let (p, t) = handover(input);
    section_xml("업무 인수인계서", &p, &t)
}
pub fn render_performance(input: &AnnualInput) -> String {
    let (p, t) = performance(input);
    section_xml("업무 실적보고서", &p, &t)
}
pub fn write_hwpx_outputs(input: &AnnualInput, folder: impl AsRef<Path>) -> io::Result<()> {
    let folder = folder.as_ref();
    fs::create_dir_all(folder)?;
    package(&folder.join("인수인계서.hwpx"), &render_handover(input))?;
    package(&folder.join("실적보고서.hwpx"), &render_performance(input))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Period, Stats, TopTopicSummary, Topic, VersionLine};
    use std::io::Read;

    fn sample() -> AnnualInput {
        let ts = crate::Date::new(2026, 2, 1).to_timestamp(crate::KST_OFFSET_SECS);
        let doc = Doc::new(
            "합성/2026_월간_업무.hwpx",
            ts,
            ts,
            "2월 업무보고",
            vec!["주요 실적".into()],
        );
        let mut stats = Stats::default();
        stats.total_docs = 1;
        stats
            .topic_by_month
            .insert(0, [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        stats.top_topics.push(TopTopicSummary {
            topic_id: 0,
            topic_name: "월간 업무 보고".into(),
            doc_count: 1,
            percentage: 100.0,
        });
        AnnualInput {
            report: Some(Report {
                generated_at: 0,
                period: Some(Period::full_year(2026)),
                threshold: 0.55,
                total_input_docs: 1,
                analyzed_doc_count: 1,
                analyzed_indices: vec![0],
                topics: vec![Topic {
                    id: 0,
                    name: "월간 업무 보고".into(),
                    rep_title: "월간 업무 보고".into(),
                    rep_doc_index: 0,
                    keywords: vec![],
                    doc_indices: vec![0],
                }],
                version_lines: vec![VersionLine {
                    name_norm: "월간 업무".into(),
                    doc_indices: vec![0],
                }],
                periodic_tasks: vec![],
                stats,
            }),
            docs: vec![doc],
            ..AnnualInput::default()
        }
    }

    #[test]
    fn package_layout_text_and_python_validator() {
        let d = tempfile::tempdir().unwrap();
        crate::output::write_outputs(&sample(), d.path()).unwrap();
        let empty = d.path().join("empty");
        crate::output::write_outputs(&AnnualInput::default(), &empty).unwrap();
        for (name, expected_tables) in [("인수인계서.hwpx", 1), ("실적보고서.hwpx", 2)] {
            let path = d.path().join(name);
            let f = fs::File::open(&path).unwrap();
            let mut zip = zip::ZipArchive::new(f).unwrap();
            assert_eq!(zip.by_index(0).unwrap().name().unwrap(), "mimetype");
            assert_eq!(
                zip.by_index(0).unwrap().compression(),
                CompressionMethod::Stored
            );
            for member in [
                "version.xml",
                "META-INF/container.xml",
                "META-INF/manifest.xml",
                "Contents/header.xml",
                "Contents/section0.xml",
                "Contents/content.hpf",
            ] {
                assert!(zip.by_name(member).is_ok(), "missing {member}");
            }
            let mut section = String::new();
            zip.by_name("Contents/section0.xml")
                .unwrap()
                .read_to_string(&mut section)
                .unwrap();
            assert!(section.contains("월간 업무 보고"));
            assert_eq!(section.matches("<hp:tbl ").count(), expected_tables);
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let validator = root.join("scripts/validate_annual_hwpx.py");
        let status = std::process::Command::new("python")
            .arg(validator)
            .arg(d.path().join("인수인계서.hwpx"))
            .arg(d.path().join("실적보고서.hwpx"))
            .arg(empty.join("인수인계서.hwpx"))
            .arg(empty.join("실적보고서.hwpx"))
            .current_dir(root)
            .status()
            .expect("Python HWPX validator must run");
        assert!(status.success(), "Python HWPX validator failed");
        assert!(d.path().join("원자료/월별_주제.csv").is_file());
    }
}
