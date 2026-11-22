//! hwpx_fmt — HWPX 직접 처리기 Rust 이식 프로토타입(RSXML1).
//! 처리기: 표 날짜 칸 한 줄(`date`), 표 두 글자 칸(`twochar`).
pub mod dom;
pub mod rules;
pub mod zipio;

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
}

impl Processor {
    pub fn parse(name: &str) -> Option<Processor> {
        match name {
            "date" | "표날짜칸_hwpx_처리" => Some(Processor::Date),
            "twochar" | "두글자칸_hwpx_처리" => Some(Processor::TwoChar),
            _ => None,
        }
    }
}

fn parse_sections(contents: &Contents) -> R<Vec<(String, usize, Doc)>> {
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

fn survey_doc(p: Processor, d: &Doc) -> R<Vec<usize>> {
    match p {
        Processor::Date => survey_date(d),
        Processor::TwoChar => Ok((0..twochar_targets(d)?.len()).collect()),
    }
}

/// 조사 모드: 구역별 대상 목록(파이썬 처리기를 selections=None 으로 부른 결과).
pub fn survey(p: Processor, source: &Path) -> R<Selections> {
    let contents = zipio::read_hwpx_safely(source)?;
    let mut out = vec![];
    for (n, _, d) in parse_sections(&contents)? {
        out.push((n, survey_doc(p, &d)?));
    }
    Ok(out)
}

/// 처리 모드(파이썬 처리기를 selections 와 target 을 주어 부른 것과 같음). 바꾼 칸 수를 돌려준다.
/// selections 가 None 이면 먼저 조사한 결과를 쓴다. 대상이 하나도 없으면 원본을 그대로 복사한다.
/// 두글자 처리는 파이썬처럼 목록 내용이 아니라 '비었는지'만 본다.
pub fn process(p: Processor, source: &Path, target: &Path, selections: Option<&Selections>) -> R<usize> {
    if let (Ok(a), Ok(b)) = (std::fs::canonicalize(source), std::fs::canonicalize(target)) {
        if a == b {
            return Err("입력과 출력이 같은 파일입니다(원본 보존).".into());
        }
    }
    let mut contents = zipio::read_hwpx_safely(source)?;
    let sections = parse_sections(&contents)?;
    let sel: Selections = match selections {
        Some(s) => s.clone(),
        None => {
            let mut v = vec![];
            for (n, _, d) in &sections {
                v.push((n.clone(), survey_doc(p, d)?));
            }
            v
        }
    };
    if sel.iter().all(|(_, items)| items.is_empty()) {
        zipio::copy_file_safely(source, target)?;
        return Ok(0);
    }
    let mut count = 0;
    for (name, idx, mut d) in sections {
        match p {
            Processor::Date => {
                let Some((_, items)) = sel.iter().find(|(n, _)| *n == name) else { continue };
                count += apply_date(&mut d, items)?;
            }
            Processor::TwoChar => count += apply_twochar(&mut d)?,
        }
        contents[idx].1 = d.tostring();
    }
    zipio::write_hwpx_safely(&contents, target)?;
    Ok(count)
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
