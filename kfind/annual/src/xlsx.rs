//! Minimal OpenXML XLSX workbook generator for annual statistics.
//!
//! Generates 6 sheets:
//! 1. 월별통계 (Monthly topic counts)
//! 2. 주차별통계 (Weekly topic counts)
//! 3. 요일열지도 (Weekday heatmap: 1~53 weeks x Mon~Sun)
//! 4. 주제타임라인 (Topic timeline with monthly grid)
//! 5. 주기업무 (Periodic task detections)
//! 6. 폴더이력 (Folder histories)
//!
//! Complies with ECMA-376 / ISO/IEC 29500 OpenXML without formula dependencies.

use crate::output::AnnualInput;
use std::{fs, io, io::Write, path::Path};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn options() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)
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

fn col_name(mut n: usize) -> String {
    let mut s = String::new();
    loop {
        let rem = n % 26;
        s.insert(0, (b'A' + rem as u8) as char);
        if n < 26 {
            break;
        }
        n = (n / 26) - 1;
    }
    s
}

#[derive(Clone, Debug)]
pub enum CellVal {
    Str(String),
    Int(i64),
    Float(f64),
}

#[derive(Clone, Debug)]
pub struct SheetData {
    pub name: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<CellVal>>,
}

fn sheet_xml(sheet: &SheetData) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\r\n<sheetData>\r\n",
    );
    // Header row (r=1, style=1: bold header with #E7EEF8 background)
    xml.push_str("  <row r=\"1\">\r\n");
    for (c_idx, h) in sheet.headers.iter().enumerate() {
        let c_ref = format!("{}{}", col_name(c_idx), 1);
        xml.push_str(&format!(
            "    <c r=\"{c_ref}\" t=\"inlineStr\" s=\"1\"><is><t xml:space=\"preserve\">{}</t></is></c>\r\n",
            esc(h)
        ));
    }
    xml.push_str("  </row>\r\n");

    // Data rows (r=2..)
    for (r_idx, row) in sheet.rows.iter().enumerate() {
        let row_num = r_idx + 2;
        xml.push_str(&format!("  <row r=\"{row_num}\">\r\n"));
        for (c_idx, val) in row.iter().enumerate() {
            let c_ref = format!("{}{}", col_name(c_idx), row_num);
            match val {
                CellVal::Str(s) => {
                    xml.push_str(&format!(
                        "    <c r=\"{c_ref}\" t=\"inlineStr\" s=\"2\"><is><t xml:space=\"preserve\">{}</t></is></c>\r\n",
                        esc(s)
                    ));
                }
                CellVal::Int(i) => {
                    xml.push_str(&format!("    <c r=\"{c_ref}\" s=\"2\"><v>{i}</v></c>\r\n"));
                }
                CellVal::Float(f) => {
                    xml.push_str(&format!(
                        "    <c r=\"{c_ref}\" s=\"2\"><v>{:.2}</v></c>\r\n",
                        f
                    ));
                }
            }
        }
        xml.push_str("  </row>\r\n");
    }

    xml.push_str("</sheetData>\r\n</worksheet>\r\n");
    xml
}

fn content_types_xml(sheet_count: usize) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\r\n  <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\r\n  <Default Extension=\"xml\" ContentType=\"application/xml\"/>\r\n  <Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>\r\n  <Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>\r\n",
    );
    for i in 1..=sheet_count {
        xml.push_str(&format!(
            "  <Override PartName=\"/xl/worksheets/sheet{i}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>\r\n"
        ));
    }
    xml.push_str("</Types>\r\n");
    xml
}

fn root_rels_xml() -> &'static str {
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\r\n  <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>\r\n</Relationships>\r\n"
}

fn workbook_rels_xml(sheet_count: usize) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\r\n",
    );
    for i in 1..=sheet_count {
        xml.push_str(&format!(
            "  <Relationship Id=\"rId{i}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{i}.xml\"/>\r\n"
        ));
    }
    let style_id = sheet_count + 1;
    xml.push_str(&format!(
        "  <Relationship Id=\"rId{style_id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\r\n</Relationships>\r\n"
    ));
    xml
}

fn workbook_xml(sheets: &[SheetData]) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\r\n  <sheets>\r\n",
    );
    for (i, s) in sheets.iter().enumerate() {
        let sheet_id = i + 1;
        xml.push_str(&format!(
            "    <sheet name=\"{}\" sheetId=\"{sheet_id}\" r:id=\"rId{sheet_id}\"/>\r\n",
            esc(&s.name)
        ));
    }
    xml.push_str("  </sheets>\r\n</workbook>\r\n");
    xml
}

fn styles_xml() -> &'static str {
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\r\n  <fonts count=\"2\">\r\n    <font><sz val=\"10\"/><name val=\"맑은 고딕\"/></font>\r\n    <font><b/><sz val=\"10\"/><color rgb=\"FF1F2937\"/><name val=\"맑은 고딕\"/></font>\r\n  </fonts>\r\n  <fills count=\"3\">\r\n    <fill><patternFill patternType=\"none\"/></fill>\r\n    <fill><patternFill patternType=\"gray125\"/></fill>\r\n    <fill><patternFill patternType=\"solid\"><fgColor rgb=\"FFE7EEF8\"/></patternFill></fill>\r\n  </fills>\r\n  <borders count=\"2\">\r\n    <border><left/><right/><top/><bottom/><diagonal/></border>\r\n    <border>\r\n      <left style=\"thin\"><color rgb=\"FFAAB7C6\"/></left>\r\n      <right style=\"thin\"><color rgb=\"FFAAB7C6\"/></right>\r\n      <top style=\"thin\"><color rgb=\"FFAAB7C6\"/></top>\r\n      <bottom style=\"thin\"><color rgb=\"FFAAB7C6\"/></bottom>\r\n      <diagonal/>\r\n    </border>\r\n  </borders>\r\n  <cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>\r\n  <cellXfs count=\"3\">\r\n    <xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>\r\n    <xf numFmtId=\"0\" fontId=\"1\" fillId=\"2\" borderId=\"1\" xfId=\"0\" applyFont=\"1\" applyFill=\"1\" applyBorder=\"1\"/>\r\n    <xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"1\" xfId=\"0\" applyBorder=\"1\"/>\r\n  </cellXfs>\r\n  <cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>\r\n</styleSheet>\r\n"
}

pub fn build_annual_sheets(input: &AnnualInput) -> Vec<SheetData> {
    let mut sheets = Vec::new();

    // Preserve calendar year rather than summing January across years.
    let mut monthly_rows = Vec::new();
    if let Some(r) = &input.report {
        for t in &r.stats.top_topics {
            if let Some(months) = r.stats.topic_timeline_by_month.get(&t.topic_id) {
                for (&(year, month), &count) in months {
                    monthly_rows.push(vec![
                        CellVal::Str(format!("{year}-{month:02}")),
                        CellVal::Str(t.topic_name.clone()),
                        CellVal::Int(count as i64),
                    ]);
                }
            } else if let Some(months) = r.stats.topic_by_month.get(&t.topic_id) {
                for (month, &count) in months.iter().enumerate() {
                    monthly_rows.push(vec![
                        CellVal::Str(format!("{:02}월 (연도 미상)", month + 1)),
                        CellVal::Str(t.topic_name.clone()),
                        CellVal::Int(count as i64),
                    ]);
                }
            }
        }
    }
    sheets.push(SheetData {
        name: "월별통계".into(),
        headers: vec!["연월".into(), "주제".into(), "문서 수".into()],
        rows: monthly_rows,
    });

    // 2. 주차별통계
    let mut weekly_rows = Vec::new();
    if let Some(r) = &input.report {
        let mut topic_map: std::collections::BTreeMap<(i32, u32), Vec<(String, u32)>> =
            std::collections::BTreeMap::new();
        for (topic_id, week_counts) in &r.stats.topic_by_iso_week {
            let name = r
                .stats
                .top_topics
                .iter()
                .find(|t| t.topic_id == *topic_id)
                .map(|t| t.topic_name.as_str())
                .unwrap_or("미상");
            for (&(year, week), &count) in week_counts {
                topic_map
                    .entry((year, week))
                    .or_default()
                    .push((name.to_string(), count));
            }
        }
        for ((year, week), mut list) in topic_map {
            list.sort();
            for (topic_name, count) in list {
                weekly_rows.push(vec![
                    CellVal::Int(year as i64),
                    CellVal::Int(week as i64),
                    CellVal::Str(topic_name),
                    CellVal::Int(count as i64),
                ]);
            }
        }
    }
    if weekly_rows.is_empty() {
        weekly_rows.push(vec![
            CellVal::Int(0),
            CellVal::Int(0),
            CellVal::Str("자료 없음".into()),
            CellVal::Int(0),
        ]);
    }
    sheets.push(SheetData {
        name: "주차별통계".into(),
        headers: vec![
            "ISO연도".into(),
            "ISO주차".into(),
            "주제".into(),
            "문서 수".into(),
        ],
        rows: weekly_rows,
    });

    // 3. 요일열지도
    let mut heatmap_rows = Vec::new();
    if input.report.is_some() {
        for (&(year, week), counts) in &crate::hwpx::weekly_counts(input) {
            let total: u32 = counts.iter().sum();
            heatmap_rows.push(vec![
                CellVal::Int(year as i64),
                CellVal::Int(week as i64),
                CellVal::Int(counts[0] as i64),
                CellVal::Int(counts[1] as i64),
                CellVal::Int(counts[2] as i64),
                CellVal::Int(counts[3] as i64),
                CellVal::Int(counts[4] as i64),
                CellVal::Int(counts[5] as i64),
                CellVal::Int(counts[6] as i64),
                CellVal::Int(total as i64),
            ]);
        }
    }
    if heatmap_rows.is_empty() {
        heatmap_rows.push(vec![
            CellVal::Int(0),
            CellVal::Int(0),
            CellVal::Int(0),
            CellVal::Int(0),
            CellVal::Int(0),
            CellVal::Int(0),
            CellVal::Int(0),
            CellVal::Int(0),
            CellVal::Int(0),
            CellVal::Int(0),
        ]);
    }
    sheets.push(SheetData {
        name: "요일열지도".into(),
        headers: vec![
            "ISO연도".into(),
            "ISO주차".into(),
            "월".into(),
            "화".into(),
            "수".into(),
            "목".into(),
            "금".into(),
            "토".into(),
            "일".into(),
            "합계".into(),
        ],
        rows: heatmap_rows,
    });

    // Use the same year-separated values as the native timeline, preserving weeks/versions.
    let timeline = crate::hwpx::topic_timeline_table(input);
    sheets.push(SheetData {
        name: "주제타임라인".into(),
        headers: timeline.headers,
        rows: timeline
            .rows
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .enumerate()
                    .map(|(i, c)| {
                        if i >= 3 {
                            CellVal::Int(c.text.parse().unwrap_or(0))
                        } else {
                            CellVal::Str(c.text)
                        }
                    })
                    .collect()
            })
            .collect(),
    });

    // 5. 주기업무
    let mut periodic_rows = Vec::new();
    for p in &input.periodic_topics {
        periodic_rows.push(vec![
            CellVal::Str(p.topic.clone()),
            CellVal::Str(p.cadence.clone()),
            CellVal::Int(p.occurrence_count as i64),
            CellVal::Str(p.last_date.to_string_ymd()),
            CellVal::Str(p.next_expected_date.to_string_ymd()),
        ]);
    }
    if periodic_rows.is_empty() {
        periodic_rows.push(vec![
            CellVal::Str("자료 없음".into()),
            CellVal::Str("-".into()),
            CellVal::Int(0),
            CellVal::Str("-".into()),
            CellVal::Str("-".into()),
        ]);
    }
    sheets.push(SheetData {
        name: "주기업무".into(),
        headers: vec![
            "업무명".into(),
            "주기".into(),
            "발생횟수".into(),
            "최근발생일".into(),
            "다음예상일".into(),
        ],
        rows: periodic_rows,
    });

    // 6. 폴더이력
    let mut folder_rows = Vec::new();
    for f in &input.folder_histories {
        folder_rows.push(vec![
            CellVal::Str(f.folder.clone()),
            CellVal::Int(f.document_count as i64),
            CellVal::Str(f.first_date.to_string_ymd()),
            CellVal::Str(f.last_date.to_string_ymd()),
            CellVal::Str(f.representative_topics.join(", ")),
        ]);
    }
    if folder_rows.is_empty() {
        folder_rows.push(vec![
            CellVal::Str("자료 없음".into()),
            CellVal::Int(0),
            CellVal::Str("-".into()),
            CellVal::Str("-".into()),
            CellVal::Str("-".into()),
        ]);
    }
    sheets.push(SheetData {
        name: "폴더이력".into(),
        headers: vec![
            "폴더경로".into(),
            "문서수".into(),
            "최초작성일".into(),
            "최근작성일".into(),
            "대표주제".into(),
        ],
        rows: folder_rows,
    });

    sheets
}

pub fn write_xlsx(sheets: &[SheetData], path: impl AsRef<Path>) -> io::Result<()> {
    let mut names = std::collections::HashSet::new();
    if sheets.is_empty()
        || sheets.iter().any(|s| {
            s.name.is_empty()
                || s.name.chars().count() > 31
                || s.name.contains(['[', ']', ':', '*', '?', '/', '\\'])
                || !names.insert(s.name.to_lowercase())
                || s.headers.is_empty()
                || s.rows.iter().any(|row| {
                    row.len() != s.headers.len()
                        || row
                            .iter()
                            .any(|c| matches!(c, CellVal::Float(f) if !f.is_finite()))
                })
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid sheet names, dimensions or numeric values",
        ));
    }
    let file = fs::File::create(path)?;
    let mut zip = ZipWriter::new(file);

    // [Content_Types].xml
    zip.start_file("[Content_Types].xml", options())?;
    zip.write_all(content_types_xml(sheets.len()).as_bytes())?;

    // _rels/.rels
    zip.start_file("_rels/.rels", options())?;
    zip.write_all(root_rels_xml().as_bytes())?;

    // xl/_rels/workbook.xml.rels
    zip.start_file("xl/_rels/workbook.xml.rels", options())?;
    zip.write_all(workbook_rels_xml(sheets.len()).as_bytes())?;

    // xl/workbook.xml
    zip.start_file("xl/workbook.xml", options())?;
    zip.write_all(workbook_xml(sheets).as_bytes())?;

    // xl/styles.xml
    zip.start_file("xl/styles.xml", options())?;
    zip.write_all(styles_xml().as_bytes())?;

    // xl/worksheets/sheetN.xml
    for (i, sheet) in sheets.iter().enumerate() {
        let sheet_path = format!("xl/worksheets/sheet{}.xml", i + 1);
        zip.start_file(sheet_path, options())?;
        zip.write_all(sheet_xml(sheet).as_bytes())?;
    }

    zip.finish()?;
    Ok(())
}

pub fn write_xlsx_outputs(input: &AnnualInput, path: impl AsRef<Path>) -> io::Result<()> {
    let sheets = build_annual_sheets(input);
    write_xlsx(&sheets, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn test_col_name() {
        assert_eq!(col_name(0), "A");
        assert_eq!(col_name(25), "Z");
        assert_eq!(col_name(26), "AA");
        assert_eq!(col_name(27), "AB");
    }

    #[test]
    fn test_write_and_verify_xlsx() {
        let dir = tempfile::tempdir().unwrap();
        let xlsx_path = dir.path().join("통계.xlsx");
        let input = AnnualInput::default();
        write_xlsx_outputs(&input, &xlsx_path).unwrap();

        assert!(xlsx_path.is_file());
        let file = fs::File::open(&xlsx_path).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();

        // Check required members
        assert!(zip.by_name("[Content_Types].xml").is_ok());
        assert!(zip.by_name("_rels/.rels").is_ok());
        assert!(zip.by_name("xl/_rels/workbook.xml.rels").is_ok());
        assert!(zip.by_name("xl/workbook.xml").is_ok());
        assert!(zip.by_name("xl/styles.xml").is_ok());

        // 6 sheets check
        for i in 1..=6 {
            let sheet_name = format!("xl/worksheets/sheet{i}.xml");
            let mut sheet_file = zip.by_name(&sheet_name).unwrap();
            let mut content = String::new();
            sheet_file.read_to_string(&mut content).unwrap();
            assert!(!content.is_empty());
            assert!(
                !content.contains("<f>"),
                "Formula is prohibited in statistics XLSX"
            );
        }
    }
}
