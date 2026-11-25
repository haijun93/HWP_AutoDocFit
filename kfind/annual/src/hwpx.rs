//! Minimal native-table HWPX reports for annual handover and performance summaries.
//!
//! Generates native HWPX tables with:
//! - 5-level shaded heatmap cells for weekly activity
//! - Shaded monthly grid timeline table with version lines
//! - Exact 170mm (48,189 HWPUNIT) width compliance
//! - Pure XML defused packaging without COM dependencies

use crate::{Doc, Report, output::AnnualInput};
use std::{fs, io, io::Write, path::Path};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

pub const BODY_WIDTH_MM: f64 = 170.0;
pub const BODY_WIDTH_HWP: u32 = 48_189;

pub const TITLE_CHAR: u32 = 0;
pub const BODY_CHAR: u32 = 1;

pub const HEADER_BORDER: u32 = 1; // #E7EEF8
pub const BODY_BORDER: u32 = 2; // #FFFFFF
pub const HEAT_LV0: u32 = 3; // #FFFFFF (no activity)
pub const HEAT_LV1: u32 = 4; // #EDF3F8
pub const HEAT_LV2: u32 = 5; // #C9DCED
pub const HEAT_LV3: u32 = 6; // #8EBADB
pub const HEAT_LV4: u32 = 7; // #3973AC

fn options() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)
}

#[derive(Clone, Debug)]
pub struct Cell {
    pub text: String,
    pub border: u32,
}

impl Cell {
    pub fn new(text: impl Into<String>, border: u32) -> Self {
        Self {
            text: text.into(),
            border,
        }
    }

    pub fn default_cell(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            border: BODY_BORDER,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
    pub custom_widths: Option<Vec<u32>>,
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
        esc(text).replace("\n", "<hp:lineBreak/>")
    )
}

fn cell_xml(cell: &Cell, table_id: u32, r: usize, c: usize, width: u32, row_height: u32) -> String {
    format!(
        "<hp:tc name=\"\" header=\"0\" hasMargin=\"1\" protect=\"0\" editable=\"0\" dirty=\"0\" borderFillIDRef=\"{}\"><hp:subList id=\"0\" textDirection=\"HORIZONTAL\" lineWrap=\"BREAK\" vertAlign=\"CENTER\" linkListIDRef=\"0\" linkListNextIDRef=\"0\" textWidth=\"0\" textHeight=\"0\" hasTextRef=\"0\" hasNumRef=\"0\">{}</hp:subList><hp:cellAddr rowAddr=\"{r}\" colAddr=\"{c}\"/><hp:cellSpan rowSpan=\"1\" colSpan=\"1\"/><hp:cellSz width=\"{width}\" height=\"{row_height}\"/><hp:cellMargin left=\"140\" right=\"140\" top=\"90\" bottom=\"90\"/></hp:tc>",
        cell.border,
        p(
            &cell.text,
            if cell.border == HEAT_LV4 {
                2
            } else {
                BODY_CHAR
            },
            table_id * 10_000 + (r as u32) * 100 + c as u32
        )
        .replace(
            "paraPrIDRef=\"0\"",
            if cell.border >= HEAT_LV0 {
                "paraPrIDRef=\"1\""
            } else {
                "paraPrIDRef=\"0\""
            }
        )
    )
}

fn table_xml(table: &Table, object_id: u32) -> String {
    let cols = table.headers.len().max(1);
    let rows = table.rows.len() + 1;

    let widths = if let Some(custom) = &table.custom_widths {
        if custom.len() == cols && custom.iter().sum::<u32>() == BODY_WIDTH_HWP {
            custom.clone()
        } else {
            allocate_widths(cols, table.headers.first().map(String::as_str))
        }
    } else {
        allocate_widths(cols, table.headers.first().map(String::as_str))
    };

    let mut out = format!(
        "<hp:tbl id=\"{object_id}\" zOrder=\"{object_id}\" numberingType=\"TABLE\" textWrap=\"TOP_AND_BOTTOM\" textFlow=\"BOTH_SIDES\" lock=\"0\" dropcapstyle=\"None\" rowCnt=\"{rows}\" colCnt=\"{cols}\" borderFillIDRef=\"2\" repeatHeader=\"1\" pageBreak=\"CELL\" cellSpacing=\"0\" noAdjust=\"0\"><hp:sz width=\"{BODY_WIDTH_HWP}\" height=\"{}\" widthRelTo=\"ABSOLUTE\" heightRelTo=\"ABSOLUTE\" protect=\"0\"/><hp:pos treatAsChar=\"1\" affectLSpacing=\"0\" flowWithText=\"1\" allowOverlap=\"0\" holdAnchorAndSO=\"0\" vertRelTo=\"PARA\" horzRelTo=\"COLUMN\" vertAlign=\"TOP\" horzAlign=\"LEFT\" vertOffset=\"0\" horzOffset=\"0\"/><hp:outMargin left=\"0\" right=\"0\" top=\"0\" bottom=\"0\"/><hp:inMargin left=\"0\" right=\"0\" top=\"0\" bottom=\"0\"/><hp:tr>",
        rows * 2000
    );
    for (i, h) in table.headers.iter().enumerate() {
        let header_cell = Cell::new(h, HEADER_BORDER);
        out.push_str(&cell_xml(&header_cell, object_id, 0, i, widths[i], 2000));
    }
    out.push_str("</hp:tr>");
    for (ri, row) in table.rows.iter().enumerate() {
        out.push_str("<hp:tr>");
        for ci in 0..cols {
            let default_c = Cell::default_cell("");
            let c = row.get(ci).unwrap_or(&default_c);
            out.push_str(&cell_xml(c, object_id, ri + 1, ci, widths[ci], 2000));
        }
        out.push_str("</hp:tr>");
    }
    out.push_str("</hp:tbl>");
    format!(
        "<hp:p id=\"{object_id}\" paraPrIDRef=\"0\" styleIDRef=\"0\" pageBreak=\"0\" columnBreak=\"0\" merged=\"0\"><hp:run charPrIDRef=\"{BODY_CHAR}\">{out}<hp:t/></hp:run></hp:p>"
    )
}

fn allocate_widths(cols: usize, first_header: Option<&str>) -> Vec<u32> {
    let weights: Vec<u32> = match first_header {
        Some("월") => vec![30, 70],
        Some("상위 주제") => vec![60, 20, 20],
        _ => vec![1; cols],
    };
    let weights = if weights.len() == cols {
        weights
    } else {
        vec![1; cols]
    };
    let total_weight: u32 = weights.iter().sum();
    let mut allocated = 0;
    weights
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
        .collect()
}

fn section_xml(title: &str, paragraphs: &[String], tables: &[Table]) -> String {
    let sec_pr = "<hp:p id=\"1\" paraPrIDRef=\"0\" styleIDRef=\"0\" pageBreak=\"0\" columnBreak=\"0\" merged=\"0\"><hp:run charPrIDRef=\"1\"><hp:secPr id=\"\" textDirection=\"HORIZONTAL\" spaceColumns=\"1134\" tabStop=\"8000\" tabStopVal=\"4000\" tabStopUnit=\"HWPUNIT\" outlineShapeIDRef=\"0\" memoShapeIDRef=\"0\" textVerticalWidthHead=\"0\" masterPageCnt=\"0\"><hp:grid lineGrid=\"0\" charGrid=\"0\" wonggojiFormat=\"0\"/><hp:startNum pageStartsOn=\"BOTH\" page=\"0\" pic=\"0\" tbl=\"0\" equation=\"0\"/><hp:visibility hideFirstHeader=\"0\" hideFirstFooter=\"0\" hideFirstMasterPage=\"0\" border=\"SHOW_ALL\" fill=\"SHOW_ALL\" hideFirstPageNum=\"0\" hideFirstEmptyLine=\"0\" showLineNumber=\"0\"/><hp:lineNumberShape restartType=\"0\" countBy=\"0\" distance=\"0\" startNumber=\"0\"/><hp:pagePr landscape=\"WIDELY\" width=\"59528\" height=\"84188\" gutterType=\"LEFT_ONLY\"><hp:margin header=\"3600\" footer=\"3600\" gutter=\"0\" left=\"5669\" right=\"5670\" top=\"3600\" bottom=\"3600\"/></hp:pagePr><hp:footNotePr><hp:autoNumFormat type=\"DIGIT\" userChar=\"\" prefixChar=\"\" suffixChar=\")\" supscript=\"0\"/><hp:noteLine length=\"-1\" type=\"SOLID\" width=\"0.12 mm\" color=\"#000000\"/><hp:noteSpacing betweenNotes=\"283\" belowLine=\"567\" aboveLine=\"850\"/><hp:numbering type=\"CONTINUOUS\" newNum=\"1\"/><hp:placement place=\"EACH_COLUMN\" beneathText=\"0\"/></hp:footNotePr><hp:endNotePr><hp:autoNumFormat type=\"DIGIT\" userChar=\"\" prefixChar=\"\" suffixChar=\")\" supscript=\"0\"/><hp:noteLine length=\"-1\" type=\"SOLID\" width=\"0.12 mm\" color=\"#000000\"/><hp:noteSpacing betweenNotes=\"0\" belowLine=\"567\" aboveLine=\"850\"/><hp:numbering type=\"CONTINUOUS\" newNum=\"1\"/><hp:placement place=\"END_OF_DOCUMENT\" beneathText=\"0\"/></hp:endNotePr><hp:pageBorderFill type=\"BOTH\" borderFillIDRef=\"2\" textBorder=\"PAPER\" headerInside=\"0\" footerInside=\"0\" fillArea=\"PAPER\"><hp:offset left=\"0\" right=\"0\" top=\"0\" bottom=\"0\"/></hp:pageBorderFill></hp:secPr></hp:run></hp:p>";
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
    let mut fonts = String::new();
    for lang in [
        "HANGUL", "LATIN", "HANJA", "JAPANESE", "OTHER", "SYMBOL", "USER",
    ] {
        fonts.push_str(&format!(r#"<hh:fontface lang="{lang}" fontCnt="2"><hh:font id="0" face="휴먼명조" type="TTF" isEmbedded="0"/><hh:font id="1" face="한컴돋움" type="TTF" isEmbedded="0"/></hh:fontface>"#));
    }
    let mut borders = String::new();
    for (i, color) in [
        "#E7EEF8", "#FFFFFF", "#FFFFFF", "#EDF3F8", "#C9DCED", "#8EBADB", "#3973AC",
    ]
    .iter()
    .enumerate()
    {
        borders.push_str(&format!(r#"<hh:borderFill id="{}" threeD="0" shadow="0" centerLine="NONE" breakCellSeparateLine="0"><hh:slash type="NONE"/><hh:backSlash type="NONE"/>"#, i + 1));
        for side in [
            "leftBorder",
            "rightBorder",
            "topBorder",
            "bottomBorder",
            "diagonal",
        ] {
            borders.push_str(&format!(
                r##"<hh:{side} type="SOLID" width="0.12 mm" color="#AAB7C6"/>"##
            ));
        }
        borders.push_str(&format!(r##"<hc:fillBrush><hc:winBrush faceColor="{color}" hatchColor="#000000" alpha="0"/></hc:fillBrush></hh:borderFill>"##));
    }
    let mut chars = String::new();
    for (id, height, font, color) in [
        (0, 1600, 1, "#1F2937"),
        (1, 1100, 0, "#202020"),
        (2, 1100, 0, "#FFFFFF"),
    ] {
        chars.push_str(&format!(r##"<hh:charPr id="{id}" height="{height}" textColor="{color}" shadeColor="none" useFontSpace="0" useKerning="0" symMark="NONE" borderFillIDRef="2"><hh:fontRef hangul="{font}" latin="{font}" hanja="{font}" japanese="{font}" other="{font}" symbol="{font}" user="{font}"/>"##));
        for (tag, value) in [
            ("ratio", 100),
            ("spacing", 0),
            ("relSz", 100),
            ("offset", 0),
        ] {
            chars.push_str(&format!(r#"<hh:{tag} hangul="{value}" latin="{value}" hanja="{value}" japanese="{value}" other="{value}" symbol="{value}" user="{value}"/>"#));
        }
        if id == 0 {
            chars.push_str("<hh:bold/>");
        }
        chars.push_str(r##"<hh:underline type="NONE" shape="SOLID" color="#000000"/><hh:strikeout shape="NONE" color="#000000"/><hh:outline type="NONE"/><hh:shadow type="NONE" color="#B2B2B2" offsetX="10" offsetY="10" alpha="0"/></hh:charPr>"##);
    }
    let mut paras = String::new();
    for (id, align) in [(0, "LEFT"), (1, "CENTER")] {
        paras.push_str(&format!(r#"<hh:paraPr id="{id}" tabPrIDRef="0" condense="0" fontLineHeight="0" snapToGrid="1" suppressLineNumbers="0" checked="0"><hh:align horizontal="{align}" vertical="BASELINE"/><hh:heading type="NONE" idRef="0" level="0"/><hh:breakSetting breakLatinWord="KEEP_WORD" breakNonLatinWord="BREAK_WORD" widowOrphan="0" keepWithNext="0" keepLines="0" pageBreakBefore="0" lineWrap="BREAK"/><hh:autoSpacing eAsianEng="0" eAsianNum="0"/><hh:margin><hc:intent value="0" unit="HWPUNIT"/><hc:left value="0" unit="HWPUNIT"/><hc:right value="0" unit="HWPUNIT"/><hc:prev value="0" unit="HWPUNIT"/><hc:next value="0" unit="HWPUNIT"/></hh:margin><hh:lineSpacing type="PERCENT" value="160" unit="HWPUNIT"/><hh:border borderFillIDRef="2" offsetLeft="0" offsetRight="0" offsetTop="0" offsetBottom="0" connect="0" ignoreMargin="0"/></hh:paraPr>"#));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core" version="1.4" secCnt="1"><hh:beginNum page="1" footnote="1" endnote="1" pic="1" tbl="1" equation="1"/><hh:refList><hh:fontfaces itemCnt="7">{fonts}</hh:fontfaces><hh:borderFills itemCnt="7">{borders}</hh:borderFills><hh:charProperties itemCnt="3">{chars}</hh:charProperties><hh:tabProperties itemCnt="1"><hh:tabPr id="0" autoTabLeft="0" autoTabRight="0"/></hh:tabProperties><hh:paraProperties itemCnt="2">{paras}</hh:paraProperties><hh:styles itemCnt="1"><hh:style id="0" type="PARA" name="바탕글" engName="Normal" paraPrIDRef="0" charPrIDRef="1" nextStyleIDRef="0" langID="1042" lockForm="0"/></hh:styles></hh:refList></hh:head>"#
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
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><hv:HCFVersion xmlns:hv=\"http://www.hancom.co.kr/hwpml/2011/version\" tagetApplication=\"WORDPROCESSOR\" major=\"5\" minor=\"0\" micro=\"5\" buildNumber=\"0\" os=\"1\" xmlVersion=\"1.4\" application=\"Hancom Office Hangul\" appVersion=\"\"/>",
        ),
        (
            "META-INF/container.xml",
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ocf:container xmlns:ocf=\"urn:oasis:names:tc:opendocument:xmlns:container\"><ocf:rootfiles><ocf:rootfile full-path=\"Contents/content.hpf\" media-type=\"application/hwpml-package+xml\"/></ocf:rootfiles></ocf:container>",
        ),
        (
            "META-INF/manifest.xml",
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><odf:manifest xmlns:odf=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\"/>",
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
    z.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><opf:package xmlns:opf=\"http://www.idpf.org/2007/opf/\" version=\"\" unique-identifier=\"\" id=\"\"><opf:metadata><opf:title>Annual report</opf:title><opf:language>ko</opf:language></opf:metadata><opf:manifest><opf:item id=\"header\" href=\"Contents/header.xml\" media-type=\"application/xml\"/><opf:item id=\"section0\" href=\"Contents/section0.xml\" media-type=\"application/xml\"/></opf:manifest><opf:spine><opf:itemref idref=\"header\"/><opf:itemref idref=\"section0\"/></opf:spine></opf:package>")?;
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

pub fn heatmap_level_border(count: u32, max_count: u32) -> u32 {
    if count == 0 || max_count == 0 {
        return HEAT_LV0;
    }
    HEAT_LV0 + ((count as u64 * 4).div_ceil(max_count as u64).min(4) as u32)
}

/// Include idle weeks in the requested period, retaining the ISO year at boundaries.
pub fn weekly_counts(input: &AnnualInput) -> std::collections::BTreeMap<(i32, u32), [u32; 7]> {
    let Some(report) = &input.report else {
        return Default::default();
    };
    let mut counts = report.stats.week_day_heatmap.clone();
    if let Some(period) = report.period {
        let mut ts = period.start;
        while ts <= period.end {
            let week = crate::timestamp_to_iso_week(ts, crate::KST_OFFSET_SECS);
            counts.entry((week.year, week.week)).or_insert([0; 7]);
            if period.end - ts < 7 * 86400 {
                break;
            }
            ts += 7 * 86400;
        }
        if period.start <= period.end {
            let week = crate::timestamp_to_iso_week(period.end, crate::KST_OFFSET_SECS);
            counts.entry((week.year, week.week)).or_insert([0; 7]);
        }
    }
    counts
}

pub fn weekly_heatmap_table(input: &AnnualInput) -> Table {
    // 9 columns: [주차, 월, 화, 수, 목, 금, 토, 일, 합계]
    // Widths: 6,189 + 5,300 * 7 (37,100) + 4,900 = 48,189
    let widths = vec![
        6_189, 5_300, 5_300, 5_300, 5_300, 5_300, 5_300, 5_300, 4_900,
    ];
    let headers = vec![
        "주차".into(),
        "월".into(),
        "화".into(),
        "수".into(),
        "목".into(),
        "금".into(),
        "토".into(),
        "일".into(),
        "합계".into(),
    ];

    let mut rows = Vec::new();
    let mut max_val = 0;

    if let Some(r) = &input.report {
        for counts in r.stats.week_day_heatmap.values() {
            for &c in counts {
                if c > max_val {
                    max_val = c;
                }
            }
        }

        for (&(year, week), counts) in &weekly_counts(input) {
            let total: u32 = counts.iter().sum();
            let mut row = vec![Cell::default_cell(format!("{year} W{week:02}"))];
            for &c in counts {
                let border = heatmap_level_border(c, max_val);
                let text = if c > 0 { c.to_string() } else { "".into() };
                row.push(Cell::new(text, border));
            }
            row.push(Cell::default_cell(total.to_string()));
            rows.push(row);
        }
    }

    if rows.is_empty() {
        let mut row = vec![Cell::default_cell("1주차")];
        for _ in 0..7 {
            row.push(Cell::new("", HEAT_LV0));
        }
        row.push(Cell::default_cell("0"));
        rows.push(row);
    }

    Table {
        headers,
        rows,
        custom_widths: Some(widths),
    }
}

/// One row per topic and calendar year; years are never combined in the monthly grid.
pub fn topic_timeline_table(input: &AnnualInput) -> Table {
    let mut widths = vec![9_000, 6_500, 8_689];
    widths.extend([2_000; 12]);
    let mut headers = vec!["주제·연도".into(), "시작~끝 주".into(), "버전 줄".into()];
    headers.extend((1..=12).map(|m| format!("{m}월")));
    let mut rows = Vec::new();
    if let Some(r) = &input.report {
        let max_count = r
            .stats
            .topic_timeline_by_month
            .values()
            .flat_map(|m| m.values())
            .copied()
            .max()
            .unwrap_or(1);
        for topic in &r.stats.top_topics {
            let mut years = std::collections::BTreeMap::<i32, [u32; 12]>::new();
            if let Some(months) = r.stats.topic_timeline_by_month.get(&topic.topic_id) {
                for (&(year, month), &count) in months {
                    if (1..=12).contains(&month) {
                        years.entry(year).or_default()[month as usize - 1] = count;
                    }
                }
            } else if let Some(months) = r.stats.topic_by_month.get(&topic.topic_id) {
                let year = r
                    .period
                    .map(|p| crate::Date::from_timestamp(p.start, crate::KST_OFFSET_SECS).year)
                    .unwrap_or(0);
                years.insert(year, *months);
            }
            for (year, months) in years {
                let mut docs = r
                    .topics
                    .iter()
                    .find(|t| t.id == topic.topic_id)
                    .map(|t| docs_for_topic(input, r, &t.doc_indices))
                    .unwrap_or_default();
                docs.retain(|d| {
                    crate::Date::from_timestamp(d.modified, crate::KST_OFFSET_SECS).year == year
                });
                docs.sort_by_key(|d| d.modified);
                let week_label = |d: &&Doc| {
                    let week = crate::timestamp_to_iso_week(d.modified, crate::KST_OFFSET_SECS);
                    format!("{} W{:02}", week.year, week.week)
                };
                let period = match (docs.first(), docs.last()) {
                    (Some(first), Some(last)) => {
                        format!("{}~{}", week_label(first), week_label(last))
                    }
                    _ => "문서 날짜 없음".into(),
                };
                let lines = r
                    .version_lines
                    .iter()
                    .filter(|line| {
                        line.doc_indices.iter().any(|i| {
                            r.topics
                                .iter()
                                .any(|t| t.id == topic.topic_id && t.doc_indices.contains(i))
                        })
                    })
                    .collect::<Vec<_>>();
                let history = lines
                    .iter()
                    .filter_map(|line| {
                        let mut editions = docs_for_topic(input, r, &line.doc_indices);
                        editions.retain(|d| {
                            crate::Date::from_timestamp(d.created_at(), crate::KST_OFFSET_SECS).year
                                == year
                        });
                        editions.sort_by_key(|d| d.created_at());
                        if editions.len() < 2 {
                            return None;
                        }
                        let sequence = editions
                            .iter()
                            .map(|d| {
                                let week = crate::timestamp_to_iso_week(
                                    d.created_at(),
                                    crate::KST_OFFSET_SECS,
                                );
                                format!("W{:02}", week.week)
                            })
                            .collect::<Vec<_>>()
                            .join("→");
                        Some(format!("{sequence} ({}판)", editions.len()))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let mut row = vec![
                    Cell::default_cell(format!("{} ({year})", topic.topic_name)),
                    Cell::default_cell(period),
                    Cell::default_cell(if history.is_empty() {
                        "버전 줄 없음".into()
                    } else {
                        history
                    }),
                ];
                row.extend(months.iter().map(|&c| {
                    Cell::new(
                        if c == 0 { String::new() } else { c.to_string() },
                        heatmap_level_border(c, max_count),
                    )
                }));
                rows.push(row);
            }
        }
    }
    if rows.is_empty() {
        let mut row = vec![
            Cell::default_cell("분석된 주제 없음"),
            Cell::default_cell("-"),
            Cell::default_cell("-"),
        ];
        row.extend((0..12).map(|_| Cell::new("", HEAT_LV0)));
        rows.push(row);
    }
    Table {
        headers,
        rows,
        custom_widths: Some(widths),
    }
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
                    Cell::default_cell("분석된 주제 없음"),
                    Cell::default_cell("자료 없음"),
                    Cell::default_cell("문서 정보 없음"),
                ]],
                custom_widths: Some(vec![12_047, 14_456, 21_686]),
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
            Cell::default_cell(t.name.clone()),
            Cell::default_cell(format!(
                "정기 업무: {}\n문서 이력: {}\n담당자 변천: 입력 정보 없음",
                periodic.if_empty("확인된 정기 업무 없음"),
                history.if_empty("확인된 버전 이력 없음")
            )),
            Cell::default_cell(recent_text.if_empty("문서 정보 없음")),
        ]);
    }
    if rows.is_empty() {
        rows.push(vec![
            Cell::default_cell("분석된 주제 없음"),
            Cell::default_cell("-"),
            Cell::default_cell("-"),
        ]);
    }

    let handover_table = Table {
        headers: vec![
            "주제·업무".into(),
            "정기 업무·문서 이력".into(),
            "최근 문서 (최대 3건)".into(),
        ],
        rows,
        custom_widths: Some(vec![12_047, 14_456, 21_686]),
    };

    (vec![summary], vec![handover_table])
}

fn performance(input: &AnnualInput) -> (Vec<String>, Vec<Table>) {
    let Some(r) = input.report.as_ref() else {
        return (
            vec!["분석 보고서가 입력되지 않았습니다.".into()],
            vec![
                Table {
                    headers: vec!["월".into(), "문서 수".into()],
                    rows: (1..=12)
                        .map(|m| {
                            vec![
                                Cell::default_cell(format!("{m}월")),
                                Cell::default_cell("0"),
                            ]
                        })
                        .collect(),
                    custom_widths: Some(vec![14_456, 33_733]),
                },
                Table {
                    headers: vec!["상위 주제".into(), "문서 수".into(), "비중".into()],
                    rows: vec![vec![
                        Cell::default_cell("자료 없음"),
                        Cell::default_cell("0"),
                        Cell::default_cell("0.0%"),
                    ]],
                    custom_widths: Some(vec![28_913, 9_637, 9_639]),
                },
            ],
        );
    };

    let mut months = Vec::new();
    for month in 1..=12 {
        let count: u32 = r.stats.topic_by_month.values().map(|a| a[month - 1]).sum();
        months.push(vec![
            Cell::default_cell(format!("{month}월")),
            Cell::default_cell(count.to_string()),
        ]);
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
                Cell::default_cell(t.topic_name.clone()),
                Cell::default_cell(t.doc_count.to_string()),
                Cell::default_cell(format!("{:.1}%", t.percentage)),
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

    let monthly_table = Table {
        headers: vec!["월".into(), "문서 수".into()],
        rows: months,
        custom_widths: Some(vec![14_456, 33_733]),
    };

    let top_topics_table = Table {
        headers: vec!["상위 주제".into(), "문서 수".into(), "비중".into()],
        rows: if top_rows.is_empty() {
            vec![vec![
                Cell::default_cell("자료 없음"),
                Cell::default_cell("0"),
                Cell::default_cell("0.0%"),
            ]]
        } else {
            top_rows
        },
        custom_widths: Some(vec![28_913, 9_637, 9_639]),
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
        vec![monthly_table, top_topics_table],
    )
}

pub fn render_handover(input: &AnnualInput) -> String {
    let (mut p, mut t) = handover(input);
    p.truncate(1);
    t.truncate(1);
    section_xml("업무 인수인계서", &p, &t)
}

pub fn render_performance(input: &AnnualInput) -> String {
    let (mut p, mut t) = performance(input);
    p.truncate(2);
    t.truncate(2);
    section_xml("업무 실적보고서", &p, &t)
}

pub fn write_hwpx_outputs(input: &AnnualInput, folder: impl AsRef<Path>) -> io::Result<()> {
    let folder = folder.as_ref();
    fs::create_dir_all(folder)?;
    package(&folder.join("인수인계서.hwpx"), &render_handover(input))?;
    package(&folder.join("실적보고서.hwpx"), &render_performance(input))
}

pub fn write_hwpx_outputs_with_charts(
    input: &AnnualInput,
    folder: impl AsRef<Path>,
) -> io::Result<()> {
    let folder = folder.as_ref();
    fs::create_dir_all(folder)?;
    let (p, mut t) = handover(input);
    t.push(topic_timeline_table(input));
    package(
        &folder.join("인수인계서.hwpx"),
        &section_xml("업무 인수인계서", &p, &t),
    )?;
    let (p, mut t) = performance(input);
    t.push(weekly_heatmap_table(input));
    package(
        &folder.join("실적보고서.hwpx"),
        &section_xml("업무 실적보고서", &p, &t),
    )
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
