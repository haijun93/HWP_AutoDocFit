//! 표 판별 규칙. 파이썬 원본과 같은 규칙(근거 위치는 docs/RSXML1_RESULT_2026-11-22.md 표 참조).
use crate::dom::Doc;
use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

pub type R<T> = Result<T, String>;

/// 파이썬 str.isspace()/re `\s` 와 같은 공백(유니코드 White_Space + U+001C~001F).
pub fn py_ws(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}
pub fn py_strip(s: &str) -> &str {
    s.trim_matches(py_ws)
}
fn nows(s: &str) -> String {
    s.chars().filter(|c| !py_ws(*c)).collect()
}
pub fn clen(s: &str) -> usize {
    s.chars().count()
}

/// 파이썬 int(str) 대응(공백 허용, 부호, 숫자). 실패하면 오류(파이썬은 ValueError 로 중단).
fn py_int(s: &str) -> R<i64> {
    let t = py_strip(s);
    let body = t.strip_prefix(['+', '-']).unwrap_or(t);
    if body.is_empty() || !body.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("정수가 아닌 값: {s:?}"));
    }
    t.parse::<i64>().map_err(|e| e.to_string())
}

macro_rules! lazy_re {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pat).unwrap());
    };
}
// [\s\x1c-\x1f] = 파이썬 \s
lazy_re!(RE_DATE, r"[’'‘`]?\d{2,4}[\s\x1c-\x1f]*[.년/\-][\s\x1c-\x1f]*\d{1,2}[\s\x1c-\x1f]*[.월/\-][\s\x1c-\x1f]*\d{1,2}|\d{1,2}[\s\x1c-\x1f]*[.월][\s\x1c-\x1f]*\d{1,2}[\s\x1c-\x1f]*[.일]?[\s\x1c-\x1f]*\([\s\x1c-\x1f]*[월화수목금토일][\s\x1c-\x1f]*\)");
lazy_re!(RE_DATE_REST, r"\([\s\x1c-\x1f]*[월화수목금토일][\s\x1c-\x1f]*\)|[월화수목금토일]요일|\d{1,2}[\s\x1c-\x1f]*:[\s\x1c-\x1f]*\d{2}|[.\s\x1c-\x1f~\-]");
lazy_re!(RE_BULLET_LEAD_WS, r"^[\s\x1c-\x1f]*[□ㅁㅇ○※*\-]");
lazy_re!(RE_BULLET, r"^[□ㅁㅇ○※*\-]");
lazy_re!(RE_ATTACH, r"^붙임(?:[\s\x1c-\x1f]*\d+)?(?:[\s\x1c-\x1f]|$)");
lazy_re!(RE_MID_NUM, r"^(?:[Ⅰ-ⅿ]+|[IVX]{1,4})[\s\x1c-\x1f]*[.．]?$");
lazy_re!(RE_MID_NUM_DIGIT, r"^(?:[Ⅰ-ⅿ]+|[IVX]{1,4}|\d{1,2})[\s\x1c-\x1f]*[.．]?$");
lazy_re!(RE_TITLE_DATE, r"[0-9]{2,4}[\s\x1c-\x1f]*[.년/\-][\s\x1c-\x1f]*[0-9]{1,2}");
lazy_re!(RE_OWNER_A, r"담당|과장|팀장|☎|전화|부서|작성");
lazy_re!(RE_OWNER_B, r"담당|과장|팀장|본부장|부장|☎|전화|내선|부서|작성|\d{2,4}-\d{4}");
lazy_re!(RE_SECTION, r"^Contents/section\d+\.xml$");

pub fn is_section_name(n: &str) -> bool {
    RE_SECTION.is_match(n)
}

impl Doc {
    pub fn tag(&self, i: usize) -> &str {
        &self.nodes[i].local
    }
    pub fn iter(&self, i: usize) -> std::ops::Range<usize> {
        i..self.nodes[i].end
    }
    pub fn attr(&self, i: usize, k: &str) -> Option<&str> {
        self.nodes[i].attrs.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())
    }
    pub fn set_attr(&mut self, i: usize, k: &str, v: &str) {
        if let Some(a) = self.nodes[i].attrs.iter_mut().find(|(a, _)| a == k) {
            a.1 = v.to_string();
        } else {
            self.nodes[i].attrs.push((k.to_string(), v.to_string()));
        }
    }
    pub fn child(&self, i: usize, name: &str) -> Option<usize> {
        self.nodes[i].children.iter().copied().find(|&c| self.tag(c) == name)
    }
    /// 제목_셀들: 표 바로 아래 tr 의 바로 아래 tc
    pub fn cells(&self, table: usize) -> Vec<usize> {
        let mut v = vec![];
        for &row in &self.nodes[table].children {
            if self.tag(row) == "tr" {
                for &c in &self.nodes[row].children {
                    if self.tag(c) == "tc" {
                        v.push(c);
                    }
                }
            }
        }
        v
    }
    /// 제목_문자열: 모든 t 요소의 text 를 이음(tail 제외).
    pub fn jstr(&self, e: usize) -> String {
        let mut s = String::new();
        for i in self.iter(e) {
            if self.tag(i) == "t" {
                if let Some(t) = &self.nodes[i].text {
                    s.push_str(t);
                }
            }
        }
        s
    }
    /// table_style._text: t 의 글자와 t 안 요소 뒤 글자(tail)까지.
    pub fn own_text(&self, e: usize) -> String {
        let mut s = String::new();
        for i in self.iter(e) {
            if self.tag(i) == "t" {
                if let Some(t) = &self.nodes[i].text {
                    s.push_str(t);
                }
                for &c in &self.nodes[i].children {
                    if let Some(t) = &self.nodes[c].tail {
                        s.push_str(t);
                    }
                }
            }
        }
        s
    }
    /// 제목_문단들: subList 바로 아래 p
    pub fn cell_paras(&self, cell: usize) -> Vec<usize> {
        match self.child(cell, "subList") {
            None => vec![],
            Some(s) => self.nodes[s].children.iter().copied().filter(|&p| self.tag(p) == "p").collect(),
        }
    }
    pub fn tables(&self) -> Vec<usize> {
        (0..self.nodes.len()).filter(|&i| self.tag(i) == "tbl").collect()
    }
}

// ---------------- docfit_core/table_style.py ----------------

fn size(d: &Doc, t: usize) -> R<(i64, i64)> {
    Ok((py_int(d.attr(t, "rowCnt").unwrap_or("0"))?, py_int(d.attr(t, "colCnt").unwrap_or("0"))?))
}

/// (행, 열, 행 병합 수, 열 병합 수)
fn position(d: &Doc, tc: usize) -> R<(i64, i64, i64, i64)> {
    let addr = d.child(tc, "cellAddr");
    let span = d.child(tc, "cellSpan");
    let g = |e: Option<usize>, k: &str, def: &str| -> R<i64> {
        match e {
            Some(e) => py_int(d.attr(e, k).unwrap_or(def)),
            None => py_int(def),
        }
    };
    let row = if addr.is_some() { g(addr, "rowAddr", "0")? } else { 0 };
    let col = if addr.is_some() { g(addr, "colAddr", "0")? } else { 0 };
    let rows = if span.is_some() { g(span, "rowSpan", "1")? } else { 1 };
    let cols = if span.is_some() { g(span, "colSpan", "1")? } else { 1 };
    Ok((row, col, rows.max(1), cols.max(1)))
}

const ARROWS: &str = "→⇒⇨➔➜➡➞➝▶▷►⇩↓⬇▼▽⟶⟹";

fn is_arrow_cell(d: &Doc, tc: usize) -> bool {
    let t = nows(&d.own_text(tc));
    !t.is_empty() && t.chars().all(|c| ARROWS.contains(c))
}

fn is_design_table(d: &Doc, t: usize) -> R<bool> {
    let (rows, cols) = size(d, t)?;
    let cells = d.cells(t);
    if rows < 2 || cols < 2 || cells.is_empty() {
        return Ok(false);
    }
    let texts: Vec<(usize, String)> = cells.iter().map(|&c| (c, nows(&d.own_text(c)))).collect();
    if texts.iter().all(|(_, s)| !s.is_empty()) || !texts.iter().any(|(_, s)| !s.is_empty()) {
        return Ok(false);
    }
    for (tc, text) in &texts {
        if !text.is_empty() {
            let (row, _, span, _) = position(d, *tc)?;
            if row != 0 || span < rows.max(1) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn is_diagram_table(d: &Doc, t: usize) -> R<bool> {
    let cells = d.cells(t);
    if cells.len() < 8 {
        return Ok(false);
    }
    let texts: Vec<String> = cells.iter().map(|&c| nows(&d.own_text(c))).collect();
    let filled = texts.iter().filter(|s| !s.is_empty()).count();
    let mut first = vec![];
    for (i, &c) in cells.iter().enumerate() {
        if position(d, c)?.0 == 0 {
            first.push(&texts[i]);
        }
    }
    let n = cells.len();
    Ok((filled as f64) <= (n as f64) * 0.35
        && n - filled >= 6
        && first.iter().filter(|s| !s.is_empty()).count() * 2 < first.len())
}

fn is_flow_table(d: &Doc, t: usize) -> bool {
    d.cells(t).into_iter().any(|c| is_arrow_cell(d, c))
}

fn has_unit_caption(d: &Doc, t: usize) -> bool {
    match d.child(t, "caption") {
        Some(c) => nows(&d.own_text(c)).contains("단위"),
        None => false,
    }
}

/// is_user_designed_table = is_drawn_table(design|diagram|flow) or has_unit_caption
pub fn is_user_designed_table(d: &Doc, t: usize) -> R<bool> {
    Ok(is_design_table(d, t)? || is_diagram_table(d, t)? || is_flow_table(d, t) || has_unit_caption(d, t))
}

/// protected_tables: 사용자가 꾸민 표와 그 안 모든 표의 요소 번호.
pub fn protected_tables(d: &Doc) -> R<HashSet<usize>> {
    let mut p = HashSet::new();
    for t in 0..d.nodes.len() {
        if d.tag(t) == "tbl" && !p.contains(&t) && is_user_designed_table(d, t)? {
            for i in d.iter(t) {
                if d.tag(i) == "tbl" {
                    p.insert(i);
                }
            }
        }
    }
    Ok(p)
}

/// iter_unprotected_cells: 보호 표 경계에서 멈추며 칸을 문서 순서로.
pub fn unprotected_cells(d: &Doc, table: usize) -> R<Vec<usize>> {
    let mut out = vec![];
    let mut pending = vec![table];
    while let Some(e) = pending.pop() {
        if d.tag(e) == "tbl" && is_user_designed_table(d, e)? {
            continue;
        }
        if d.tag(e) == "tc" {
            out.push(e);
        }
        for &c in d.nodes[e].children.iter().rev() {
            pending.push(c);
        }
    }
    Ok(out)
}

// ---------------- docfit_engine/formatter_titles.py ----------------

/// 날짜칸인가
pub fn is_date_cell(text: &str) -> bool {
    let s = py_strip(text);
    if s.is_empty() || clen(s) > 40 {
        return false;
    }
    let Some(m) = RE_DATE.find(s) else { return false };
    let rest = format!("{}{}", &s[..m.start()], &s[m.end()..]);
    let rest = RE_DATE_REST.replace_all(&rest, "");
    clen(&rest) <= 12
}

fn opaque(d: &Doc, c: usize, names: &[&str]) -> bool {
    d.iter(c).any(|i| names.contains(&d.tag(i)))
}

fn mid_cells(d: &Doc, table: usize, digit_ok: bool) -> R<Vec<usize>> {
    let cells = d.cells(table);
    let (rows, cols) = size(d, table)?;
    if rows != 1 || cols < 2 || cells.len() as i64 != cols {
        return Ok(vec![]);
    }
    for &c in &cells {
        if opaque(d, c, &["tbl", "pic", "ole", "rect", "fieldBegin"]) {
            return Ok(vec![]);
        }
        if let Some(sp) = d.child(c, "cellSpan") {
            if d.attr(sp, "colSpan").unwrap_or("1") != "1" || d.attr(sp, "rowSpan").unwrap_or("1") != "1" {
                return Ok(vec![]);
            }
        }
    }
    let texts: Vec<String> = cells.iter().map(|&c| py_strip(&d.jstr(c)).to_string()).collect();
    let re = if digit_ok { &*RE_MID_NUM_DIGIT } else { &*RE_MID_NUM };
    if !re.is_match(&texts[0]) {
        return Ok(vec![]);
    }
    let glan: Vec<usize> = texts.iter().enumerate().filter(|(i, t)| *i > 0 && !t.is_empty()).map(|(i, _)| i).collect();
    if !glan.iter().any(|i| *i == 1 || *i == 2) {
        return Ok(vec![]);
    }
    if glan.iter().any(|&i| clen(&texts[i]) > 80 || RE_BULLET.is_match(&texts[i]) || texts[i].starts_with("붙임")) {
        return Ok(vec![]);
    }
    Ok(glan)
}

fn mid_kind(d: &Doc, table: usize) -> R<Option<u8>> {
    let cols = py_int(d.attr(table, "colCnt").unwrap_or("0"))?;
    if cols != 2 && cols != 3 {
        return Ok(None);
    }
    if mid_cells(d, table, false)? != vec![(cols - 1) as usize] {
        return Ok(None);
    }
    Ok(Some(if cols == 3 { 1 } else { 2 }))
}

fn attach_kind(d: &Doc, table: usize) -> R<Option<u8>> {
    let cells = d.cells(table);
    let (rows, cols) = (py_int(d.attr(table, "rowCnt").unwrap_or("0"))?, py_int(d.attr(table, "colCnt").unwrap_or("0"))?);
    if rows != 1 || (cols != 2 && cols != 3) || cells.len() as i64 != cols {
        return Ok(None);
    }
    let first = py_strip(&d.jstr(cells[0])).to_string();
    let last = py_strip(&d.jstr(*cells.last().unwrap())).to_string();
    if !RE_ATTACH.is_match(&first) || last.is_empty() {
        return Ok(None);
    }
    if cols == 3 && !py_strip(&d.jstr(cells[1])).is_empty() {
        return Ok(None);
    }
    Ok(Some(if cols == 3 { 1 } else { 2 }))
}

fn title_kind(d: &Doc, table: usize) -> R<Option<u8>> {
    let cells = d.cells(table);
    if cells.len() != 2 && cells.len() != 3 {
        return Ok(None);
    }
    let rc = size(d, table)?;
    let three_rows = cells.len() == 3 && rc == (3, 1);
    if (cells.len() == 2 && rc != (2, 1)) || (cells.len() == 3 && rc != (2, 2) && !three_rows) {
        return Ok(None);
    }
    for (i, &c) in cells.iter().enumerate() {
        let blocked: &[&str] = if i == 0 { &["tbl", "ole", "rect", "fieldBegin"] } else { &["tbl", "pic", "ole", "rect", "fieldBegin"] };
        if opaque(d, c, blocked) {
            return Ok(None);
        }
    }
    let ps: Vec<usize> = d.cell_paras(cells[0]).into_iter().filter(|&p| !py_strip(&d.jstr(p)).is_empty()).collect();
    if ps.len() != 1 && ps.len() != 2 {
        return Ok(None);
    }
    if ps.iter().any(|&p| RE_BULLET_LEAD_WS.is_match(&d.jstr(p))) {
        return Ok(None);
    }
    if cells.len() == 2 || three_rows {
        for &c in &cells[1..] {
            // _설정_담당자_글인가: 설정 값(제목_담당자_글)이 기본값 ''이면 항상 거짓
            if !RE_OWNER_B.is_match(&d.jstr(c)) {
                return Ok(None);
            }
        }
        return Ok(Some(3));
    }
    match d.child(cells[0], "cellSpan") {
        Some(sp) if d.attr(sp, "colSpan") == Some("2") => {}
        _ => return Ok(None),
    }
    let date = d.jstr(cells[1]);
    let owner = d.jstr(cells[2]);
    if !RE_TITLE_DATE.is_match(&date) {
        return Ok(None);
    }
    if !RE_OWNER_A.is_match(&owner) {
        return Ok(None);
    }
    Ok(Some(if ps.len() == 2 { 2 } else { 1 }))
}

fn overview_table(d: &Doc, table: usize) -> R<bool> {
    let cells = d.cells(table);
    if cells.len() != 1 || size(d, table)? != (1, 1) {
        return Ok(false);
    }
    let text = py_strip(&d.jstr(cells[0])).to_string();
    Ok(!text.is_empty() && clen(&text) >= 15 && !RE_BULLET.is_match(&text) && !text.starts_with("붙임"))
}

/// 서식표_종류판별: 서식 표(제목·중제목·붙임·한 칸 상자)이면 true.
pub fn is_format_table(d: &Doc, table: usize) -> R<bool> {
    if title_kind(d, table)?.is_some() {
        let cells = d.cells(table);
        let title = py_strip(&d.jstr(cells[0])).to_string();
        if !title.is_empty() && clen(&title) <= 160 && !title.starts_with("붙임") {
            return Ok(true);
        }
    }
    if mid_kind(d, table)?.is_some() || attach_kind(d, table)?.is_some() {
        return Ok(true);
    }
    overview_table(d, table)
}
