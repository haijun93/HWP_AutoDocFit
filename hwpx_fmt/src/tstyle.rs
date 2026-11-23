//! 기본 표 서식(파이썬 `docfit_core/table_style.py` 적용부 + `formatter_tables.기본표서식_hwpx_처리` 의 조사·대상 판정,
//! `format_elements.head_block_tables`). 예시 표 프로필은 설정 JSON(`table_style`)으로 받는다.
use crate::config::unsupported;
use crate::dom::Doc;
use crate::eaw::is_wide;
use crate::fmt3::next_id;
use crate::rules::{attach_kind, is_drawn_table, has_unit_caption, mid_cells, position, py_strip, py_ws, size, title_kind_blank, R};
use crate::run3b::parse_fragment;
use crate::style::unify_marker;
use crate::tbl::{pyint, run_text, HIdx};
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::LazyLock;

// ---------------------------------------------------------------- 프로필

#[derive(Debug, Clone)]
pub struct CharSpec {
    pub height: i64,
    pub bold: bool,
    pub fonts: Vec<(String, String)>,
    pub ratio: Vec<(String, String)>,
    pub spacing: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct CellSpec {
    pub border: String,
    pub char: String,
    pub align: Option<String>,
    pub valign: Option<String>,
    pub line: Option<(String, i64)>,
    pub text_align: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Style {
    pub rows: i64,
    pub cols: i64,
    pub header_rows: i64,
    pub grid: Vec<Vec<usize>>,
    pub cells: Vec<CellSpec>,
    pub borders: Vec<(String, String)>,
    pub chars: Vec<(String, CharSpec)>,
    pub table_border: Option<String>,
    pub in_margin: Vec<(String, String)>,
}

fn bad<T>(what: &str) -> R<T> {
    unsupported(&format!("table_style 프로필 형식: {what}"))
}


fn opt_str(v: &Value, k: &str) -> R<Option<String>> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        _ => bad(&format!("{k} 는 문자열 또는 null 이어야 합니다.")),
    }
}

fn get_int(v: &Value, k: &str) -> R<i64> {
    match v.get(k) {
        Some(Value::Number(n)) if n.is_i64() => Ok(n.as_i64().unwrap()),
        _ => bad(&format!("{k} 는 정수여야 합니다.")),
    }
}

fn str_map(v: Option<&Value>, what: &str) -> R<Vec<(String, String)>> {
    match v {
        None | Some(Value::Null) => Ok(vec![]),
        Some(Value::Object(m)) => m
            .iter()
            .map(|(k, x)| match x {
                Value::String(s) => Ok((k.clone(), s.clone())),
                _ => bad(&format!("{what} 값은 문자열이어야 합니다.")),
            })
            .collect(),
        _ => bad(&format!("{what} 는 객체여야 합니다.")),
    }
}

/// 프로필 JSON → Style. 파이썬 `valid_style` 이 거짓이면 Ok(None)(파이썬은 준말 등록표로 넘어가 None 이 된다).
/// 해석이 파이썬과 갈릴 수 있는 형식 오류는 '미지원 입력'.
pub fn parse_style(v: &Value) -> R<Option<Style>> {
    let obj = match v {
        Value::Null => return Ok(None),
        Value::Object(m) => m,
        _ => return bad("객체여야 합니다."),
    };
    if obj.is_empty() {
        return Ok(None); // 빈 사전은 거짓
    }
    match obj.get("version") {
        Some(Value::Number(n)) if n.is_i64() => {
            if n.as_i64() != Some(1) {
                return Ok(None);
            }
        }
        Some(Value::String(_)) => return Ok(None),
        _ => return bad("version 은 정수여야 합니다."),
    }
    let rows = get_int(v, "rows")?;
    let cols = get_int(v, "cols")?;
    let grid_v = match v.get("grid") {
        Some(Value::Array(a)) => a,
        _ => return bad("grid 는 배열이어야 합니다."),
    };
    let cells_v = match v.get("cells") {
        Some(Value::Array(a)) => a,
        _ => return bad("cells 는 배열이어야 합니다."),
    };
    let borders_v = match v.get("borders") {
        Some(Value::Object(m)) => m,
        _ => return bad("borders 는 객체여야 합니다."),
    };
    let chars_v = match v.get("chars") {
        Some(Value::Object(m)) => m,
        _ => return bad("chars 는 객체여야 합니다."),
    };
    // valid_style: 모양 검사
    if rows < 2 || cols < 2 || grid_v.len() as i64 != rows {
        return Ok(None);
    }
    let mut grid = vec![];
    for row in grid_v {
        let Some(r) = row.as_array() else { return bad("grid 행은 배열이어야 합니다.") };
        if r.len() as i64 != cols {
            return Ok(None);
        }
        let mut out = vec![];
        for i in r {
            let Some(n) = i.as_i64() else { return bad("grid 값은 정수여야 합니다.") };
            if n < 0 || n as usize >= cells_v.len() {
                return Ok(None);
            }
            out.push(n as usize);
        }
        grid.push(out);
    }
    let mut cells = vec![];
    for c in cells_v {
        let (Some(b), Some(ch)) = (c.get("border").and_then(Value::as_str), c.get("char").and_then(Value::as_str)) else {
            return bad("cells 항목에 border/char 문자열이 있어야 합니다.");
        };
        if !borders_v.contains_key(b) || !chars_v.contains_key(ch) {
            return Ok(None);
        }
        let line = match c.get("line") {
            None | Some(Value::Null) => None,
            Some(l) => {
                let t = match l.get("type") {
                    Some(Value::String(s)) => s.clone(),
                    _ => return bad("line.type 은 문자열이어야 합니다."),
                };
                // 파이썬은 `if base.get("line")`: 빈 사전만 거짓
                Some((t, get_int(l, "value")?))
            }
        };
        cells.push(CellSpec {
            border: b.to_string(),
            char: ch.to_string(),
            align: opt_str(c, "align")?,
            valign: opt_str(c, "valign")?,
            line,
            text_align: opt_str(c, "text_align")?,
        });
    }
    let mut borders = vec![];
    for (k, x) in borders_v {
        let Some(s) = x.as_str() else { return bad("borders 값은 문자열이어야 합니다.") };
        borders.push((k.clone(), s.to_string()));
    }
    let mut chars = vec![];
    for (k, x) in chars_v {
        chars.push((
            k.clone(),
            CharSpec {
                height: get_int(x, "height")?,
                bold: match x.get("bold") {
                    None | Some(Value::Null) => false,
                    Some(Value::Bool(b)) => *b,
                    _ => return bad("chars.bold 는 true/false 여야 합니다."),
                },
                fonts: str_map(x.get("fonts"), "chars.fonts")?,
                ratio: str_map(x.get("ratio"), "chars.ratio")?,
                spacing: str_map(x.get("spacing"), "chars.spacing")?,
            },
        ));
    }
    Ok(Some(Style {
        rows,
        cols,
        header_rows: get_int(v, "header_rows")?,
        grid,
        cells,
        borders,
        chars,
        table_border: opt_str(v, "table_border")?.filter(|s| !s.is_empty()),
        in_margin: str_map(v.get("in_margin"), "in_margin")?,
    }))
}

// ---------------------------------------------------------------- 대상 판정

/// `table_style.is_grid_table`
pub fn is_grid_table(d: &Doc, table: usize) -> R<bool> {
    let (rows, cols) = size(d, table)?;
    if rows < 2 || cols < 2 || is_drawn_table(d, table)? {
        return Ok(false);
    }
    let cells = d.cells(table);
    for &tc in &cells {
        if d.iter(tc).any(|i| d.tag(i) == "tbl") {
            return Ok(false);
        }
    }
    let head = crate::tbl::header_row_count(d, table)?;
    let mut starts = vec![];
    for &tc in &cells {
        starts.push(position(d, tc)?.0);
    }
    Ok((head..rows).any(|r| starts.iter().filter(|&&s| s == r).count() >= 2))
}

/// `_기본표서식_대상인가`
pub fn is_target(d: &Doc, table: usize) -> R<bool> {
    if !is_grid_table(d, table)? || has_unit_caption(d, table) {
        return Ok(false);
    }
    if title_kind_blank(d, table, true)?.is_some() {
        return Ok(false);
    }
    if !mid_cells(d, table, true)?.is_empty() {
        return Ok(false);
    }
    Ok(attach_kind(d, table)?.is_none())
}

// ---------------------------------------------------------------- head_block_tables

const SPACES4: [char; 4] = [' ', '\t', '\u{a0}', '\u{3000}'];

fn label_span(cs: &[char], start: usize) -> Option<(usize, usize)> {
    let mut index = start;
    while index < cs.len() && SPACES4.contains(&cs[index]) {
        index += 1;
    }
    if index >= cs.len() || !"(（".contains(cs[index]) {
        return None;
    }
    let mut depth = 0i64;
    for position in index..cs.len() {
        let c = cs[position];
        if c == '\r' || c == '\n' {
            return None;
        }
        if "(（".contains(c) {
            depth += 1;
        } else if ")）".contains(c) {
            depth -= 1;
            if depth == 0 {
                return Some((index, position + 1));
            }
        }
    }
    None
}

fn parenthetical_spans(cs: &[char], protected: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut stack: Vec<(usize, char)> = vec![];
    let mut pairs = vec![];
    for (index, &ch) in cs.iter().enumerate() {
        if ch == '\r' || ch == '\n' {
            stack.clear();
            continue;
        }
        if ch == '(' || ch == '（' {
            stack.push((index, if ch == '(' { ')' } else { '）' }));
        } else if (ch == ')' || ch == '）') && !stack.is_empty() {
            let (start, expected) = *stack.last().unwrap();
            if expected != ch {
                stack.clear();
                continue;
            }
            stack.pop();
            if stack.is_empty() {
                pairs.push((start, index + 1));
            }
        }
    }
    pairs.into_iter().filter(|&(s, e)| !protected.iter().any(|&(os, oe)| s < oe && os < e)).collect()
}

static RE_DIGIT_LEAD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+\.").unwrap());
static RE_COPIES_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+[\s\x1c-\x1f]*부\.|끝\.)$").unwrap());

fn is_body_sentence(text: &str, size: Option<i64>, align: Option<&str>) -> bool {
    let body = py_strip(text);
    if RE_DIGIT_LEAD.is_match(body) && RE_COPIES_END.is_match(body) {
        return false;
    }
    body.chars().filter(|&c| !py_ws(c)).count() >= 12
        && !body.starts_with("붙임")
        && size.unwrap_or(0) < 1800
        && !matches!(align, Some("CENTER") | Some("RIGHT"))
}

/// `paragraph_values` 중 본문 시작 판정에 쓰는 값: (그룹 기호 있음, 글, 글자 크기, 정렬). 글자가 없으면 None.
fn starts_body(d: &Doc, p: usize, idx: &HIdx) -> Option<bool> {
    let runs: Vec<(usize, String)> = d.nodes[p].children.iter().copied().filter(|&r| d.tag(r) == "run").map(|r| (r, run_text(d, r))).collect();
    let text: String = runs.iter().map(|(_, t)| t.as_str()).collect();
    if py_strip(&text).is_empty() {
        return None;
    }
    let align = idx.para(d.attr(p, "paraPrIDRef")).map(|v| v.align.clone());
    let (group, _, raw) = unify_marker(&text);
    let cs: Vec<char> = text.chars().collect();
    let lead = cs.iter().take_while(|c| SPACES4.contains(c)).count();
    let raw_len = raw.chars().count();
    let marker = if raw_len > 0 { Some((lead, lead + raw_len)) } else { None };
    let label = marker.and_then(|m| label_span(&cs, m.1));
    let prot: Vec<(usize, usize)> = [marker, label].into_iter().flatten().collect();
    let asides = parenthetical_spans(&cs, &prot);
    let mut weights: Vec<(i64, usize)> = vec![];
    let mut aside_sizes: Vec<(i64, usize)> = vec![];
    let mut cursor = 0usize;
    let bump = |v: &mut Vec<(i64, usize)>, k: i64| match v.iter_mut().find(|x| x.0 == k) {
        Some(x) => x.1 += 1,
        None => v.push((k, 1)),
    };
    for (run, part) in &runs {
        if let Some(ch) = idx.char(d.attr(*run, "charPrIDRef")) {
            for (offset, letter) in part.chars().enumerate() {
                if py_ws(letter) {
                    continue;
                }
                let position = cursor + offset;
                let in_aside = asides.iter().any(|&(a, b)| a <= position && position < b);
                if !in_aside {
                    bump(&mut weights, ch.size);
                } else {
                    bump(&mut aside_sizes, ch.size);
                }
            }
        }
        cursor += part.chars().count();
    }
    let dominant = |v: &Vec<(i64, usize)>| -> Option<i64> {
        let mut best: Option<(i64, usize)> = None;
        for &(k, c) in v {
            if best.map_or(true, |b| c > b.1) {
                best = Some((k, c));
            }
        }
        best.map(|b| b.0)
    };
    let mut size = dominant(&weights);
    if size.is_none() && !aside_sizes.is_empty() {
        size = dominant(&aside_sizes);
    }
    Some(!group.is_empty() || is_body_sentence(py_strip(&text), size, align.as_deref()))
}

fn largest_text_size(d: &Doc, table: usize, idx: &HIdx) -> i64 {
    let mut best = 0;
    for i in d.iter(table) {
        if d.tag(i) == "run" && !py_strip(&run_text(d, i)).is_empty() {
            let s = idx.char(d.attr(i, "charPrIDRef")).map_or(0, |v| v.size);
            best = best.max(s);
        }
    }
    best
}

/// `head_block_tables(roots, header)`: (구역 위치, 표 요소 번호) 목록. roots 는 구역 번호순.
pub fn head_block_tables(roots: &[&Doc], idx: &HIdx) -> Vec<(usize, usize)> {
    let (mut found, mut before_body, mut body_started) = (vec![], vec![], false);
    for (ri, d) in roots.iter().enumerate() {
        for &p in &d.nodes[0].children {
            if d.tag(p) != "p" {
                continue;
            }
            if let Some(s) = starts_body(d, p, idx) {
                body_started = body_started || s;
            }
            for &run in &d.nodes[p].children {
                if d.tag(run) != "run" {
                    continue;
                }
                for &table in &d.nodes[run].children {
                    if d.tag(table) != "tbl" {
                        continue;
                    }
                    let tables: Vec<(usize, usize)> = d.iter(table).filter(|&x| d.tag(x) == "tbl").map(|x| (ri, x)).collect();
                    if largest_text_size(d, table, idx) >= 1800 {
                        found.extend(tables);
                    } else if !body_started {
                        before_body.extend(tables);
                    }
                }
            }
        }
    }
    if body_started {
        found.extend(before_body);
    }
    found
}

// ---------------------------------------------------------------- 머리글 풀

type PKey = (Option<String>, String, String, i64);

pub struct Pool<'a> {
    pub style: &'a Style,
    borders_g: usize,
    chars_g: usize,
    paras_g: usize,
    fontfaces: HashMap<String, usize>,
    made_borders: HashMap<String, String>,
    made_chars: HashMap<(Option<String>, String, bool), String>,
    made_paras: HashMap<PKey, Option<String>>,
    made_fonts: HashMap<(String, String), Option<String>>,
    parsed: HashMap<String, usize>,
}

fn node_key(d: &Doc, i: usize, out: &mut String) {
    let n = &d.nodes[i];
    out.push('<');
    out.push_str(&n.qname);
    for (k, v) in &n.attrs {
        out.push_str(&format!(" {}={}:{}", k, v.len(), v));
    }
    out.push('|');
    match &n.text {
        Some(t) => out.push_str(&format!("t{}:{}", t.len(), t)),
        None => out.push('-'),
    }
    for &c in &n.children {
        node_key(d, c, out);
    }
    match &n.tail {
        Some(t) => out.push_str(&format!(">t{}:{}", t.len(), t)),
        None => out.push_str(">-"),
    }
}

fn group_by_id(h: &Doc, group: usize, id: Option<&str>) -> Option<usize> {
    h.nodes[group].children.iter().copied().find(|&x| h.attr(x, "id") == id)
}

fn first_tag(h: &Doc, tag: &str) -> Option<usize> {
    (0..h.nodes.len()).find(|&i| h.tag(i) == tag)
}

fn child_by_tag(h: &Doc, e: usize, tag: &str) -> Option<usize> {
    h.child(e, tag)
}

fn append_with_id(h: &mut Doc, group: usize, el: usize) -> R<String> {
    let id = next_id(h, group)?;
    h.set_attr(el, "id", &id);
    h.append_child(group, el);
    let n = h.nodes[group].children.len().to_string();
    h.set_attr(group, "itemCnt", &n);
    Ok(id)
}

const BOLD_BEFORE: [&str; 8] = ["underline", "strikeout", "outline", "shadow", "emboss", "engrave", "supscript", "subscript"];

impl<'a> Pool<'a> {
    pub fn new(h: &Doc, style: &'a Style) -> R<Pool<'a>> {
        let (Some(b), Some(c), Some(p)) = (first_tag(h, "borderFills"), first_tag(h, "charProperties"), first_tag(h, "paraProperties")) else {
            return Err("문서 머리 정보(header.xml)에 서식 목록이 없습니다.".into());
        };
        let mut fontfaces = HashMap::new();
        for i in 0..h.nodes.len() {
            if h.tag(i) == "fontface" {
                fontfaces.insert(h.attr(i, "lang").unwrap_or("").to_lowercase(), i);
            }
        }
        Ok(Pool {
            style,
            borders_g: b,
            chars_g: c,
            paras_g: p,
            fontfaces,
            made_borders: HashMap::new(),
            made_chars: HashMap::new(),
            made_paras: HashMap::new(),
            made_fonts: HashMap::new(),
            parsed: HashMap::new(),
        })
    }

    /// 프로필 테두리 XML(key) → 머리글 문서 안 떨어진 요소(캐시)
    fn parsed_border(&mut self, h: &mut Doc, key: &str) -> R<usize> {
        if let Some(&n) = self.parsed.get(key) {
            return Ok(n);
        }
        let xml = self.style.borders.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).ok_or("테두리 키 없음")?;
        let f = parse_fragment(&xml, h)?;
        let n = h.graft(&f);
        self.parsed.insert(key.to_string(), n);
        Ok(n)
    }

    fn picture_fill(&self, h: &Doc, border_id: Option<&str>) -> Option<usize> {
        let source = group_by_id(h, self.borders_g, border_id)?;
        let fill = child_by_tag(h, source, "fillBrush")?;
        if h.nodes[fill].children.iter().any(|&x| h.tag(x) == "imgBrush") {
            Some(fill)
        } else {
            None
        }
    }

    fn border(&mut self, h: &mut Doc, element: usize) -> R<String> {
        let mut key = String::new();
        node_key(h, element, &mut key);
        if let Some(id) = self.made_borders.get(&key) {
            return Ok(id.clone());
        }
        let id = append_with_id(h, self.borders_g, element)?;
        self.made_borders.insert(key, id.clone());
        Ok(id)
    }

    fn font(&mut self, h: &mut Doc, lang: &str, font_xml: &str) -> R<Option<String>> {
        let key = (lang.to_string(), font_xml.to_string());
        if let Some(v) = self.made_fonts.get(&key) {
            return Ok(v.clone());
        }
        let group = self.fontfaces.get(lang).copied();
        let sample_doc = parse_fragment(font_xml, h)?;
        let Some(group) = group else { return Ok(None) };
        let sample_face = sample_doc.attr(0, "face").map(str::to_string);
        let sample_type = sample_doc.attr(0, "type").unwrap_or("").to_string();
        let found = h.nodes[group]
            .children
            .iter()
            .copied()
            .find(|&f| h.attr(f, "face").map(str::to_string) == sample_face && h.attr(f, "type").unwrap_or("") == sample_type);
        let found = match found {
            Some(f) => f,
            None => {
                let s = h.graft(&sample_doc);
                let id = next_id(h, group)?;
                h.set_attr(s, "id", &id);
                h.append_child(group, s);
                let n = h.nodes[group].children.len().to_string();
                h.set_attr(group, "fontCnt", &n);
                s
            }
        };
        let id = h.attr(found, "id").map(str::to_string);
        self.made_fonts.insert(key, id.clone());
        Ok(id)
    }

    fn char(&mut self, h: &mut Doc, base_id: Option<&str>, spec_key: &str, preserve: bool) -> R<String> {
        let key = (base_id.map(str::to_string), spec_key.to_string(), preserve);
        if let Some(v) = self.made_chars.get(&key) {
            return Ok(v.clone());
        }
        let spec = self.style.chars.iter().find(|(k, _)| k == spec_key).map(|(_, s)| s.clone()).ok_or("글자 서식 키 없음")?;
        let source = match group_by_id(h, self.chars_g, base_id) {
            Some(s) => s,
            None => *h.nodes[self.chars_g].children.first().ok_or("charProperties 가 비어 있습니다.")?,
        };
        let clone = h.clone_subtree(source);
        h.set_attr(clone, "height", &spec.height.to_string());
        if let Some(r) = child_by_tag(h, clone, "fontRef") {
            for (lang, font_xml) in &spec.fonts {
                let font_id = self.font(h, lang, font_xml)?;
                if let Some(fid) = font_id {
                    if h.attr(r, lang).is_some() {
                        h.set_attr(r, lang, &fid);
                    }
                }
            }
        }
        if !preserve {
            for (name, vals) in [("ratio", &spec.ratio), ("spacing", &spec.spacing)] {
                if let Some(t) = child_by_tag(h, clone, name) {
                    if !vals.is_empty() {
                        for (k, v) in vals {
                            if h.attr(t, k).is_some() {
                                h.set_attr(t, k, v);
                            }
                        }
                    }
                }
            }
            if spec.bold && child_by_tag(h, clone, "bold").is_none() {
                let q = match h.nodes[clone].qname.split_once(':') {
                    Some((p, _)) => format!("{p}:bold"),
                    None => "bold".to_string(),
                };
                let bold = h.new_elem(&q, vec![]);
                let at = h.nodes[clone].children.iter().position(|&c| BOLD_BEFORE.contains(&h.tag(c))).unwrap_or(h.nodes[clone].children.len());
                h.insert_child(clone, at, bold);
            }
        }
        let id = append_with_id(h, self.chars_g, clone)?;
        self.made_chars.insert(key, id.clone());
        Ok(id)
    }

    fn para_line(&mut self, h: &mut Doc, base_id: &str, line: &(String, i64)) -> R<String> {
        let key: PKey = (Some(base_id.to_string()), "line".into(), line.0.clone(), line.1);
        if let Some(v) = self.made_paras.get(&key) {
            return Ok(v.clone().unwrap());
        }
        let Some(source) = group_by_id(h, self.paras_g, Some(base_id)) else {
            self.made_paras.insert(key, Some(base_id.to_string()));
            return Ok(base_id.to_string());
        };
        let clone = h.clone_subtree(source);
        let all = h.walk(clone);
        let case = all.iter().copied().find(|&x| h.tag(x) == "case");
        let default = all.iter().copied().find(|&x| h.tag(x) == "default");
        let scopes: Vec<(Option<usize>, i64)> = if case.is_some() { vec![(case, 1), (default, 2)] } else { vec![(Some(clone), 1)] };
        let mut changed = false;
        for (scope, factor) in scopes {
            let Some(scope) = scope else { continue };
            let Some(sp) = h.walk(scope).into_iter().find(|&x| h.tag(x) == "lineSpacing") else { continue };
            let value = (line.1 * if line.0 == "PERCENT" { 1 } else { factor }).to_string();
            if h.attr(sp, "type") != Some(line.0.as_str()) || h.attr(sp, "value") != Some(value.as_str()) {
                h.set_attr(sp, "type", &line.0);
                h.set_attr(sp, "value", &value);
                changed = true;
            }
        }
        let id = if changed { append_with_id(h, self.paras_g, clone)? } else { base_id.to_string() };
        self.made_paras.insert(key, Some(id.clone()));
        Ok(id)
    }

    fn para_align(&mut self, h: &mut Doc, base_id: &str, align: &str) -> R<String> {
        let key: PKey = (Some(base_id.to_string()), "align".into(), align.to_string(), 0);
        if let Some(v) = self.made_paras.get(&key) {
            return Ok(v.clone().unwrap());
        }
        let source = group_by_id(h, self.paras_g, Some(base_id));
        let current = source.and_then(|s| h.walk(s).into_iter().find(|&x| h.tag(x) == "align"));
        let same = match current {
            None => true,
            Some(c) => h.attr(c, "horizontal") == Some(align),
        };
        if same {
            self.made_paras.insert(key, Some(base_id.to_string()));
            return Ok(base_id.to_string());
        }
        let clone = h.clone_subtree(source.unwrap());
        let a = h.walk(clone).into_iter().find(|&x| h.tag(x) == "align").unwrap();
        h.set_attr(a, "horizontal", align);
        let id = append_with_id(h, self.paras_g, clone)?;
        self.made_paras.insert(key, Some(id.clone()));
        Ok(id)
    }
}

// ---------------------------------------------------------------- 적용

fn row_roles(row: i64, row_span: i64, rows: i64, head: i64) -> (&'static str, &'static str, &'static str) {
    let bottom = row + row_span - 1;
    let top = if row < head { "header" } else if row == head { "first" } else { "body" };
    let base = if bottom < head {
        "header"
    } else if row < head {
        "body"
    } else if row == head {
        "first"
    } else if row == rows - 1 {
        "last"
    } else {
        "body"
    };
    let bottom_role = if bottom < head { "header" } else if bottom == rows - 1 { "last" } else { "body" };
    (top, base, bottom_role)
}

fn col_roles(col: i64, col_span: i64, cols: i64) -> (&'static str, &'static str, &'static str) {
    let right = col + col_span - 1;
    let left = if col == 0 { "first" } else { "middle" };
    let base = if col == 0 { "first" } else if right == cols - 1 { "last" } else { "middle" };
    let right_role = if right == cols - 1 { "last" } else { "middle" };
    (left, base, right_role)
}

/// 파이썬 리스트 인덱싱(음수 허용, 범위 밖은 IndexError)
fn pyidx(len: usize, i: i64) -> R<usize> {
    let j = if i < 0 { i + len as i64 } else { i };
    if j < 0 || j as usize >= len {
        return Err("list index out of range".into());
    }
    Ok(j as usize)
}

static RE_DIGIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\p{Nd}").unwrap());
static RE_LETTER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\p{L}\p{Nl}\p{No}]").unwrap());

fn number_like(text: &str) -> bool {
    RE_DIGIT.is_match(text) && RE_LETTER.find_iter(text).count() <= 2
}

fn spread_label(text: &str) -> bool {
    let letters: Vec<char> = text.chars().filter(|&c| !py_ws(c)).collect();
    (2..=6).contains(&letters.len()) && letters.iter().all(|&c| ('가'..='힣').contains(&c))
}

fn fits_one_line(d: &Doc, paragraph: usize, tc: usize, table: usize, height: i64, ratio: i64) -> R<bool> {
    let text = d.own_text(paragraph);
    let text = py_strip(&text);
    if text.is_empty() {
        return Ok(true);
    }
    let size = d.child(tc, "cellSz");
    let margin = if d.attr(tc, "hasMargin") == Some("1") { d.child(tc, "cellMargin") } else { d.child(table, "inMargin") };
    let ival = |s: &str| -> R<i64> { pyint(s)?.ok_or_else(|| format!("invalid literal for int(): {s:?}")) };
    let mut width = match size {
        Some(s) => ival(d.attr(s, "width").unwrap_or("0"))?,
        None => 0,
    };
    match margin {
        Some(m) => width -= ival(d.attr(m, "left").unwrap_or("510"))? + ival(d.attr(m, "right").unwrap_or("510"))?,
        None => width -= 1020,
    }
    let mut ps = crate::style::PySum::new();
    for c in text.chars() {
        if is_wide(c) {
            ps.add_int(height);
        } else {
            ps.add(height as f64 / 2.0);
        }
    }
    let needed = ps.total();
    Ok(needed * ratio as f64 / 100.0 <= width as f64)
}

fn remove_lineseg(d: &mut Doc, p: usize) {
    let rm: Vec<usize> = d.nodes[p].children.iter().copied().filter(|&x| d.tag(x) == "linesegarray").collect();
    for x in rm {
        d.remove_child(p, x);
    }
}

/// `apply_table_style(pool, table)`
pub fn apply_table_style(d: &mut Doc, h: &mut Doc, pool: &mut Pool, table: usize) -> R<()> {
    let style = pool.style;
    let (rows, cols) = size(d, table)?;
    let head = crate::tbl::header_row_count(d, table)?;
    // 예시 표 위치 표(_sample_rows / _sample_cols)
    let (srows, shead) = (style.rows, style.header_rows);
    let first = shead.min(srows - 1);
    let middle = if shead + 1 <= srows - 2 { shead + 1 } else { srows - 1 };
    let top_rows = |r: &str| -> i64 {
        match r {
            "header" => 0,
            "first" => first,
            _ => {
                if shead + 1 < srows { shead + 1 } else { first }
            }
        }
    };
    let base_rows = |r: &str| -> i64 {
        match r {
            "header" => 0,
            "first" => first,
            "body" => middle,
            _ => srows - 1,
        }
    };
    let bottom_rows = |r: &str| -> i64 {
        match r {
            "header" => shead - 1,
            "body" => {
                if srows - 2 >= shead { srows - 2 } else { srows - 1 }
            }
            _ => srows - 1,
        }
    };
    let scols = style.cols;
    let left_cols = |r: &str| -> i64 { if r == "first" { 0 } else if scols >= 2 { 1 } else { 0 } };
    let base_cols = |r: &str| -> i64 {
        match r {
            "first" => 0,
            "middle" => {
                if scols >= 3 { 1 } else { scols - 1 }
            }
            _ => scols - 1,
        }
    };
    let right_cols = |r: &str| -> i64 { if r == "middle" { if scols >= 2 { scols - 2 } else { 0 } } else { scols - 1 } };
    let cell_at = |row: i64, col: i64| -> R<&CellSpec> {
        let r = pyidx(style.grid.len(), row)?;
        let c = pyidx(style.grid[r].len(), col)?;
        Ok(&style.cells[style.grid[r][c]])
    };
    for tc in d.cells(table) {
        let (row, col, row_span, col_span) = position(d, tc)?;
        let (top_role, base_role, bottom_role) = row_roles(row, row_span, rows, head);
        let (left_role, base_col, right_role) = col_roles(col, col_span, cols);
        let base = cell_at(base_rows(base_role), base_cols(base_col))?.clone();
        let edges: [(&str, CellSpec); 4] = [
            ("leftBorder", cell_at(base_rows(base_role), left_cols(left_role))?.clone()),
            ("rightBorder", cell_at(base_rows(base_role), right_cols(right_role))?.clone()),
            ("topBorder", cell_at(top_rows(top_role), base_cols(base_col))?.clone()),
            ("bottomBorder", cell_at(bottom_rows(bottom_role), base_cols(base_col))?.clone()),
        ];
        let bp = pool.parsed_border(h, &base.border)?;
        let composed = h.clone_subtree(bp);
        for (side, src_cell) in &edges {
            let sp = pool.parsed_border(h, &src_cell.border)?;
            let source = child_by_tag(h, sp, side);
            let current = child_by_tag(h, composed, side);
            if let (Some(source), Some(current)) = (source, current) {
                let at = h.index_of(composed, current).unwrap();
                let copy = h.clone_subtree(source);
                h.nodes[composed].children[at] = copy;
            }
        }
        if let Some(pic) = pool.picture_fill(h, d.attr(tc, "borderFillIDRef")) {
            if let Some(cur) = child_by_tag(h, composed, "fillBrush") {
                h.remove_child(composed, cur);
            }
            let copy = h.clone_subtree(pic);
            h.append_child(composed, copy);
        }
        let bid = pool.border(h, composed)?;
        d.set_attr(tc, "borderFillIDRef", &bid);
        if let Some(sub) = d.child(tc, "subList") {
            if let Some(v) = base.valign.as_deref().filter(|s| !s.is_empty()) {
                d.set_attr(sub, "vertAlign", v);
            }
        }
        let spec = style.chars.iter().find(|(k, _)| *k == base.char).map(|(_, s)| s.clone()).ok_or("글자 서식 키 없음")?;
        let ratio = match spec.ratio.iter().find(|(k, _)| k == "hangul") {
            Some((_, v)) => pyint(v)?.ok_or_else(|| format!("invalid literal for int(): {v:?}"))?,
            None => 100,
        };
        let text_align: Option<&str> = if base_role != "header" { base.text_align.as_deref().filter(|s| !s.is_empty()) } else { None };
        for paragraph in d.cell_paras(tc) {
            let runs: Vec<usize> = d.nodes[paragraph].children.iter().copied().filter(|&r| d.tag(r) == "run").collect();
            let mut kinds: Vec<Option<String>> = vec![];
            for &r in &runs {
                let k = d.attr(r, "charPrIDRef").map(str::to_string);
                if !kinds.contains(&k) {
                    kinds.push(k);
                }
            }
            let mixed = kinds.len() > 1;
            for &run in &runs {
                let cid = d.attr(run, "charPrIDRef").map(str::to_string);
                let nid = pool.char(h, cid.as_deref(), &base.char, mixed)?;
                d.set_attr(run, "charPrIDRef", &nid);
            }
            let ptext = d.own_text(paragraph);
            let sentence = text_align.is_some() && !number_like(py_strip(&ptext));
            let mut align: Option<String> = if sentence { text_align.map(str::to_string) } else { base.align.clone() };
            if matches!(align.as_deref(), Some("DISTRIBUTE") | Some("DISTRIBUTE_SPACE")) && !spread_label(&ptext) {
                align = Some("CENTER".into());
            }
            let pid = d.attr(paragraph, "paraPrIDRef").map(str::to_string);
            if let (Some(a), Some(pid)) = (align.as_deref().filter(|s| !s.is_empty()), pid.as_deref()) {
                if sentence || base_role == "header" || fits_one_line(d, paragraph, tc, table, spec.height, ratio)? {
                    let n = pool.para_align(h, pid, a)?;
                    d.set_attr(paragraph, "paraPrIDRef", &n);
                }
            }
            if let Some(line) = &base.line {
                if let Some(pid) = d.attr(paragraph, "paraPrIDRef").map(str::to_string) {
                    let n = pool.para_line(h, &pid, line)?;
                    d.set_attr(paragraph, "paraPrIDRef", &n);
                }
            }
            remove_lineseg(d, paragraph);
        }
    }
    if let Some(tb) = &style.table_border {
        let f = parse_fragment(tb, h)?;
        let n = h.graft(&f);
        let id = pool.border(h, n)?;
        d.set_attr(table, "borderFillIDRef", &id);
    }
    if !style.in_margin.is_empty() {
        if let Some(m) = d.child(table, "inMargin") {
            for (k, v) in &style.in_margin {
                d.set_attr(m, k, v);
            }
        }
    }
    Ok(())
}
