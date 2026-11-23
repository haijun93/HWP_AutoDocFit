//! hwpx_fmt — HWPX 직접 처리기 Rust 이식 프로토타입(RSXML1).
//! 처리기: 표 날짜 칸 한 줄(`date`), 표 두 글자 칸(`twochar`), 개요 표 첫 빈칸(`overview_lead`),
//! 쪽 번호 모양(`page_number`), 자간 초기화(`spacing_reset`).
pub mod config;
pub mod conn;
pub mod eaw;
pub mod dom;
pub mod extra;
pub mod fmt3;
pub mod marker;
pub mod rules;
pub mod run3;
pub mod run3b;
pub mod run4;
pub mod style;
pub mod tbl;
pub mod tstyle;
pub mod zipio;

use config::Config;
use dom::Doc;
use rules::{is_date_cell, is_format_table, is_section_name, protected_tables, py_strip, unprotected_cells, R};
use std::path::Path;
use zipio::Contents;

/// 구역 이름 → 항목 번호 목록(파이썬 selections 와 같은 모양, 구역은 압축 안 순서).
pub type Selections = Vec<(String, Vec<usize>)>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Processor {
    Date,
    TwoChar,
    OverviewLead,
    PageNumber,
    SpacingReset,
    AsteriskSuper,
    AttachFont,
    ColonLabel,
    BlankLines,
    Emphasis,
    TableWidth,
    TableStyle,
    TableStyleAlign,
    ConnectorTab,
}

impl Processor {
    pub fn parse(name: &str) -> Option<Processor> {
        match name {
            "date" | "표날짜칸_hwpx_처리" => Some(Processor::Date),
            "twochar" | "두글자칸_hwpx_처리" => Some(Processor::TwoChar),
            "overview_lead" | "개요첫칸_hwpx_처리" => Some(Processor::OverviewLead),
            "page_number" | "쪽번호_hwpx_처리" => Some(Processor::PageNumber),
            "spacing_reset" | "자간초기화_hwpx_처리" => Some(Processor::SpacingReset),
            "asterisk_superscript" | "별표위첨자_hwpx_처리" => Some(Processor::AsteriskSuper),
            "attachment_font" | "붙임글꼴_hwpx_처리" => Some(Processor::AttachFont),
            "colon_label" | "콜론라벨_hwpx_처리" => Some(Processor::ColonLabel),
            "blank_lines" | "빈줄서식_hwpx_처리" => Some(Processor::BlankLines),
            "emphasis" | "강조낱말_hwpx_처리" => Some(Processor::Emphasis),
            "table_width" | "표너비_hwpx_처리" => Some(Processor::TableWidth),
            "table_style" | "기본표서식_hwpx_처리" => Some(Processor::TableStyle),
            "table_style_align" => Some(Processor::TableStyleAlign),
            "connector_tab" | "연결부호탭_hwpx_처리" => Some(Processor::ConnectorTab),
            _ => None,
        }
    }

    /// RSXML3 처리기(선택 항목이 번호 하나가 아닐 수 있어 survey_json/process_json 으로 부른다).
    /// RSXML4 처리기(표 칸 너비·기본 표 서식·정렬 재확인·연결 부호 탭).
    pub fn is_rsxml4(self) -> bool {
        matches!(self, Processor::TableWidth | Processor::TableStyle | Processor::TableStyleAlign | Processor::ConnectorTab)
    }

    pub fn is_rsxml3(self) -> bool {
        matches!(self, Processor::AsteriskSuper | Processor::AttachFont | Processor::ColonLabel | Processor::BlankLines | Processor::Emphasis)
    }
}

pub fn parse_sections(contents: &Contents) -> R<Vec<(String, usize, Doc)>> {
    let mut v = vec![];
    for (i, (n, data)) in contents.iter().enumerate() {
        if is_section_name(n) {
            v.push((n.clone(), i, dom::parse(data).map_err(|e| format!("{n}: {e}"))?));
        }
    }
    Ok(v)
}

// ---- 표 날짜 칸 한 줄 -------------------------------------------------

/// 표_날짜칸_한줄 이 바꿀 칸의 subList(순수 계산). 이미 한 줄이거나 안쪽 표가 든 칸은 건너뛴다.
fn date_cells(d: &Doc, table: usize) -> R<Vec<usize>> {
    let mut out = vec![];
    for cell in unprotected_cells(d, table)? {
        let Some(sub) = d.child(cell, "subList") else { continue };
        if d.attr(sub, "lineWrap") == Some("SQUEEZE") {
            continue;
        }
        if d.iter(sub).any(|i| d.tag(i) == "tbl") {
            continue;
        }
        let text: String = d.nodes[sub].children.iter().copied().filter(|&q| d.tag(q) == "p").map(|q| d.jstr(q)).collect();
        if is_date_cell(&text) {
            out.push(sub);
        }
    }
    Ok(out)
}

fn survey_date(d: &Doc) -> R<Vec<usize>> {
    let prot = protected_tables(d)?;
    let mut out = vec![];
    for (idx, t) in d.tables().into_iter().enumerate() {
        if !prot.contains(&t) && !date_cells(d, t)?.is_empty() {
            out.push(idx);
        }
    }
    Ok(out)
}

fn apply_date(d: &mut Doc, items: &[usize]) -> R<usize> {
    let tables = d.tables();
    let prot = protected_tables(d)?;
    let mut count = 0;
    for &index in items {
        let t = *tables.get(index).ok_or_else(|| format!("표 번호 범위 밖: {index}"))?;
        if prot.contains(&t) {
            continue;
        }
        let subs = date_cells(d, t)?;
        count += subs.len();
        for s in subs {
            d.set_attr(s, "lineWrap", "SQUEEZE");
        }
    }
    Ok(count)
}

// ---- 표 두 글자 칸 ---------------------------------------------------

fn hangul(c: char) -> bool {
    ('\u{AC00}'..='\u{D7A3}').contains(&c)
}

/// 칸 글 전체가 (앞뒤 공백 허용) 한글 두 글자뿐인가: re.fullmatch(r'\s*[가-힣]{2}\s*')
fn is_two_hangul(s: &str) -> bool {
    let mut it = py_strip(s).chars();
    matches!((it.next(), it.next(), it.next()), (Some(a), Some(b), None) if hangul(a) && hangul(b))
}

/// (글 요소, 첫 글자 뒤 위치(문자 수)) — 파이썬 `대상(root)` 과 같은 순서(같은 글 요소가 두 번 나올 수 있음).
fn twochar_targets(d: &Doc) -> R<Vec<(usize, usize)>> {
    let prot = protected_tables(d)?;
    let mut out = vec![];
    for table in d.tables() {
        if prot.contains(&table) || is_format_table(d, table)? {
            continue;
        }
        for cell in d.cells(table) {
            if !is_two_hangul(&d.jstr(cell)) {
                continue;
            }
            let ts: Vec<usize> = d
                .iter(cell)
                .filter(|&i| d.tag(i) == "t" && !py_strip(d.nodes[i].text.as_deref().unwrap_or("")).is_empty())
                .collect();
            if ts.len() == 1 && d.nodes[ts[0]].children.is_empty() {
                let chars: Vec<char> = d.nodes[ts[0]].text.as_deref().unwrap_or("").chars().collect();
                if let Some(p) = chars.windows(2).position(|w| hangul(w[0]) && hangul(w[1])) {
                    out.push((ts[0], p + 1));
                }
            }
        }
    }
    Ok(out)
}

fn apply_twochar(d: &mut Doc) -> R<usize> {
    let targets = twochar_targets(d)?;
    let mut n = 0;
    for (t, at) in targets {
        let text = d.nodes[t].text.clone().unwrap_or_default();
        let mut s: String = text.chars().take(at).collect();
        s.push(' ');
        s.extend(text.chars().skip(at));
        d.nodes[t].text = Some(s);
        n += 1;
    }
    Ok(n)
}

// ---- 공개 API --------------------------------------------------------

fn survey_doc(p: Processor, d: &Doc, cfg: &Config) -> R<Vec<usize>> {
    match p {
        Processor::Date => survey_date(d),
        Processor::TwoChar => Ok((0..twochar_targets(d)?.len()).collect()),
        Processor::OverviewLead => Ok((0..extra::overview_targets(d)?.len()).collect()),
        Processor::PageNumber => Ok(extra::page_number_survey(d, cfg)),
        Processor::SpacingReset => Ok(vec![0]),
        _ => Err("RSXML3 처리기는 survey_json 으로 부르세요.".into()),
    }
}

/// 설정 중 이 처리기가 반영하지 못하는 값이 있으면 거부한다(조용히 다른 결과를 내지 않는다).
fn check_config(p: Processor, cfg: &Config) -> R<()> {
    match p {
        Processor::TwoChar | Processor::OverviewLead if !cfg.title_owner_text.is_empty() => {
            config::unsupported("제목_담당자_글 설정(title_owner_text)은 서식표 판별에 반영하지 못합니다.")
        }
        Processor::SpacingReset if cfg.indent_rules_nonempty => {
            config::unsupported("내어쓰기_규칙(indent_rules) 설정은 자간 초기화 보호 구간에 반영하지 못합니다.")
        }
        Processor::SpacingReset if cfg.marker_scheme_active => {
            config::unsupported("활성 항목기호 체계(marker_scheme_active)는 자간 초기화 보호 구간에 반영하지 못합니다.")
        }
        _ => Ok(()),
    }
}

/// 구역 번호를 정수로 본 오름차순 키(같으면 압축 안 순서). 파이썬 `sorted(key=int(...))` 와 같다.
pub(crate) fn section_number_key(name: &str) -> (usize, String) {
    let digits = name.trim_start_matches("Contents/section").trim_end_matches(".xml").trim_start_matches('0');
    (digits.len(), digits.to_string())
}

pub fn parse_sections_sorted(contents: &Contents) -> R<Vec<(String, usize, Doc)>> {
    let mut v = parse_sections(contents)?;
    v.sort_by_key(|(n, _, _)| section_number_key(n)); // 안정 정렬
    Ok(v)
}

/// 파이썬 처리기 첫머리: 모든 Contents/*.xml 을 읽어 보므로 구역이 아닌 항목이 깨져 있어도 처리 전체가 실패한다.
pub fn check_other_xml(contents: &Contents) -> R<()> {
    for (n, data) in contents.iter() {
        if n.starts_with("Contents/") && n.ends_with(".xml") && !is_section_name(n) {
            dom::parse(data).map_err(|e| format!("{n}: {e}"))?;
        }
    }
    Ok(())
}

fn ordered_sections(p: Processor, contents: &Contents) -> R<Vec<(String, usize, Doc)>> {
    match p {
        Processor::OverviewLead | Processor::SpacingReset => parse_sections_sorted(contents),
        _ => parse_sections(contents),
    }
}

pub fn parse_header(contents: &Contents) -> R<(usize, Doc)> {
    let i = contents.iter().position(|(n, _)| n == "Contents/header.xml").ok_or("Contents/header.xml 이 없습니다.")?;
    Ok((i, dom::parse(&contents[i].1).map_err(|e| format!("Contents/header.xml: {e}"))?))
}

/// 조사 모드: 구역별 대상 목록(파이썬 처리기를 selections=None 으로 부른 결과).
pub fn survey(p: Processor, source: &Path) -> R<Selections> {
    survey_with(p, source, &Config::default())
}

pub fn survey_with(p: Processor, source: &Path, cfg: &Config) -> R<Selections> {
    if p.is_rsxml3() || p.is_rsxml4() {
        return Err("RSXML3·4 처리기는 survey_json 으로 부르세요.".into());
    }
    check_config(p, cfg)?;
    let contents = zipio::read_hwpx_safely(source)?;
    check_other_xml(&contents)?;
    if p == Processor::SpacingReset {
        parse_header(&contents)?;
    }
    let mut out = vec![];
    for (n, _, d) in ordered_sections(p, &contents)? {
        out.push((n, survey_doc(p, &d, cfg)?));
    }
    Ok(out)
}

pub fn process(p: Processor, source: &Path, target: &Path, selections: Option<&Selections>) -> R<usize> {
    process_with(p, source, target, selections, &Config::default())
}

/// 처리 모드(파이썬 처리기를 selections 와 target 을 주어 부른 것과 같음). 바꾼 칸 수를 돌려준다.
/// selections 가 None 이면 먼저 조사한 결과를 쓴다. 대상이 하나도 없으면 원본을 그대로 복사한다.
/// 두글자 처리는 파이썬처럼 목록 내용이 아니라 '비었는지'만 본다.
pub fn process_with(p: Processor, source: &Path, target: &Path, selections: Option<&Selections>, cfg: &Config) -> R<usize> {
    if p.is_rsxml3() || p.is_rsxml4() {
        return Err("RSXML3·4 처리기는 process_json 으로 부르세요.".into());
    }
    check_config(p, cfg)?;
    if let (Ok(a), Ok(b)) = (std::fs::canonicalize(source), std::fs::canonicalize(target)) {
        if a == b {
            return Err("입력과 출력이 같은 파일입니다(원본 보존).".into());
        }
    }
    let mut contents = zipio::read_hwpx_safely(source)?;
    check_other_xml(&contents)?;
    let sections = ordered_sections(p, &contents)?;
    let mut header = if p == Processor::SpacingReset { Some(parse_header(&contents)?) } else { None };
    let sel: Selections = match selections {
        Some(s) => s.clone(),
        None => {
            let mut v = vec![];
            for (n, _, d) in &sections {
                v.push((n.clone(), survey_doc(p, d, cfg)?));
            }
            v
        }
    };
    if sel.iter().all(|(_, items)| items.is_empty()) {
        zipio::copy_file_safely(source, target)?;
        return Ok(0);
    }
    if p == Processor::SpacingReset {
        let mut docs: Vec<(String, usize, Doc)> = sections;
        let (hidx, hdoc) = header.as_mut().unwrap();
        let stats = {
            let mut refs: Vec<&mut Doc> = docs.iter_mut().map(|(_, _, d)| d).collect();
            extra::reset_spacing(hdoc, &mut refs)?
        };
        if stats.runs == 0 {
            zipio::copy_file_safely(source, target)?;
            return Ok(0);
        }
        for (_, idx, d) in &docs {
            contents[*idx].1 = d.tostring();
        }
        contents[*hidx].1 = hdoc.tostring();
        zipio::write_hwpx_safely(&contents, target)?;
        return Ok(stats.paragraphs);
    }
    if p == Processor::PageNumber {
        // 파이썬은 selections 에 든 구역 이름으로 구역을 찾는다(없으면 KeyError).
        for (n, _) in &sel {
            if !sections.iter().any(|(s, _, _)| s == n) {
                return Err(format!("선택 목록의 구역이 문서에 없습니다: {n}"));
            }
        }
    }
    let mut count = 0;
    for (name, idx, mut d) in sections {
        match p {
            Processor::Date => {
                let Some((_, items)) = sel.iter().find(|(n, _)| *n == name) else { continue };
                count += apply_date(&mut d, items)?;
            }
            Processor::TwoChar => count += apply_twochar(&mut d)?,
            Processor::OverviewLead => count += extra::apply_overview(&mut d)?,
            Processor::PageNumber => {
                // 파이썬은 selections 에 든 구역만 다시 쓴다.
                let Some((_, items)) = sel.iter().find(|(n, _)| *n == name) else { continue };
                count += extra::apply_page_number(&mut d, items, cfg)?;
            }
            _ => unreachable!(),
        }
        contents[idx].1 = d.tostring();
    }
    zipio::write_hwpx_safely(&contents, target)?;
    Ok(count)
}

/// 조사 모드(모든 처리기). 선택 목록을 JSON 으로 돌려준다.
pub fn survey_json(p: Processor, source: &Path, cfg: &Config) -> R<serde_json::Value> {
    if p.is_rsxml4() {
        run4::survey(p, source, cfg)
    } else if p.is_rsxml3() {
        run3::survey(p, source, cfg)
    } else {
        Ok(selections_to_json(&survey_with(p, source, cfg)?))
    }
}

/// 처리 모드(모든 처리기). selections 는 survey_json 이 돌려준 모양의 JSON.
pub fn process_json(p: Processor, source: &Path, target: &Path, sel: Option<&serde_json::Value>, cfg: &Config) -> R<usize> {
    if p.is_rsxml4() {
        run4::process(p, source, target, sel, cfg)
    } else if p.is_rsxml3() {
        run3::process(p, source, target, sel, cfg)
    } else {
        let s = match sel {
            Some(v) => Some(selections_from_json(v)?),
            None => None,
        };
        process_with(p, source, target, s.as_ref(), cfg)
    }
}

pub fn selections_to_json(s: &Selections) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for (n, items) in s {
        m.insert(n.clone(), serde_json::json!(items));
    }
    serde_json::Value::Object(m)
}

pub fn selections_from_json(v: &serde_json::Value) -> R<Selections> {
    let obj = v.as_object().ok_or("selections 는 객체여야 합니다.")?;
    let mut out = vec![];
    for (k, a) in obj {
        let arr = a.as_array().ok_or("selections 값은 배열이어야 합니다.")?;
        let items: Result<Vec<usize>, String> =
            arr.iter().map(|x| x.as_u64().map(|u| u as usize).ok_or_else(|| "정수가 아닌 번호".to_string())).collect();
        out.push((k.clone(), items?));
    }
    Ok(out)
}
