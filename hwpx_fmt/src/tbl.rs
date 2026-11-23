//! RSXML4 공용: 파이썬 int/float 해석, 머리글 색인(`format_elements.HeaderIndex` 중 필요한 값), 쪽 본문 폭,
//! 표 칸 너비(`docfit_core/table_width.py`).
use crate::config::unsupported;
use crate::dom::Doc;
use crate::rules::{clen, is_drawn_table, has_unit_caption, py_strip, py_ws, R};
use crate::style::{py_sum, PySum};
use std::collections::HashMap;

// ---------------------------------------------------------------- 파이썬 숫자 해석

/// 파이썬 `int(str)`: 성공 `Some`, ValueError `None`. 밑줄·ASCII 아닌 숫자·64비트 넘침은 해석 차이가 있어 거부한다.
pub fn pyint(s: &str) -> R<Option<i64>> {
    let t = py_strip(s);
    let body = t.strip_prefix(['+', '-']).unwrap_or(t);
    if !body.is_empty() && body.chars().all(|c| c.is_ascii_digit()) {
        return match t.parse::<i64>() {
            Ok(v) => Ok(Some(v)),
            Err(_) => unsupported(&format!("64비트를 넘는 정수 {s:?}")),
        };
    }
    if t.contains('_') || t.chars().any(|c| !c.is_ascii() && c.is_numeric()) {
        return unsupported(&format!("파이썬 int() 해석이 다를 수 있는 값 {s:?}"));
    }
    Ok(None)
}

/// 속성 값 → `int()`; 속성이 없거나(TypeError) 해석이 안 되면(ValueError) default (table_width/_int).
pub fn int_or(v: Option<&str>, default: i64) -> R<i64> {
    match v {
        None => Ok(default),
        Some(s) => Ok(pyint(s)?.unwrap_or(default)),
    }
}

/// `format_elements._int`: `int(round(float(value)))`, 실패하면 default.
pub fn float_int_or(v: Option<&str>, default: i64) -> R<i64> {
    let Some(s) = v else { return Ok(default) };
    let t = py_strip(s);
    let plain = {
        let b = t.strip_prefix(['+', '-']).unwrap_or(t);
        let (a, rest) = match b.split_once('.') {
            Some((a, r)) => (a, Some(r)),
            None => (b, None),
        };
        let digits = |x: &str| x.chars().all(|c| c.is_ascii_digit());
        digits(a) && rest.map_or(true, digits) && !(a.is_empty() && rest.map_or(true, str::is_empty))
    };
    if plain {
        let f: f64 = t.parse().map_err(|_| format!("실수 해석 실패 {s:?}"))?;
        if !f.is_finite() || f.abs() > 9e15 {
            return unsupported(&format!("너무 큰 값 {s:?}"));
        }
        return Ok(f.round_ties_even() as i64);
    }
    if t.contains('_') || t.chars().any(|c| !c.is_ascii() && c.is_numeric()) || t.parse::<f64>().is_ok() {
        return unsupported(&format!("파이썬 float() 해석이 다를 수 있는 값 {s:?}"));
    }
    Ok(default)
}

// ---------------------------------------------------------------- 머리글 색인

#[derive(Debug, Clone)]
pub struct CharV {
    pub size: i64,
    pub bold: bool,
    pub ratio: i64,
    pub spacing: i64,
}

#[derive(Debug, Clone)]
pub struct ParaV {
    pub align: String,
    pub left: i64,
    pub right: i64,
    pub indent: i64,
}

pub struct HIdx {
    chars: HashMap<Option<String>, CharV>,
    paras: HashMap<Option<String>, ParaV>,
}

fn first_desc(d: &Doc, e: usize, tag: &str) -> Option<usize> {
    d.iter(e).find(|&i| d.tag(i) == tag)
}

impl HIdx {
    pub fn new(h: &Doc) -> R<HIdx> {
        let mut chars = HashMap::new();
        let mut paras = HashMap::new();
        for i in 0..h.nodes.len() {
            let id = h.attr(i, "id").map(str::to_string);
            match h.tag(i) {
                "charPr" => {
                    let lang = |name: &str, def: i64| -> R<i64> {
                        match first_desc(h, i, name) {
                            Some(e) => float_int_or(h.attr(e, "hangul"), def),
                            None => Ok(def),
                        }
                    };
                    chars.insert(
                        id,
                        CharV {
                            size: float_int_or(h.attr(i, "height"), 0)?,
                            bold: first_desc(h, i, "bold").is_some(),
                            ratio: lang("ratio", 100)?,
                            spacing: lang("spacing", 0)?,
                        },
                    );
                }
                "paraPr" => {
                    let align = match first_desc(h, i, "align") {
                        Some(a) => match h.attr(a, "horizontal") {
                            Some(v) if !v.is_empty() => v.to_string(),
                            _ => "JUSTIFY".to_string(),
                        },
                        None => "JUSTIFY".to_string(),
                    };
                    let margin = first_desc(h, i, "margin");
                    let value = |name: &str| -> R<i64> {
                        match margin.and_then(|m| first_desc(h, m, name)) {
                            Some(e) => float_int_or(h.attr(e, "value"), 0),
                            None => Ok(0),
                        }
                    };
                    paras.insert(id, ParaV { align, left: value("left")?, right: value("right")?, indent: value("intent")? });
                }
                _ => {}
            }
        }
        Ok(HIdx { chars, paras })
    }
    pub fn char(&self, id: Option<&str>) -> Option<&CharV> {
        self.chars.get(&id.map(str::to_string))
    }
    pub fn para(&self, id: Option<&str>) -> Option<&ParaV> {
        self.paras.get(&id.map(str::to_string))
    }
}

// ---------------------------------------------------------------- 쪽 본문 폭

/// `_구역_본문폭(section, 기본값)`: 첫 pagePr 의 (쪽 폭 − 좌우 여백). 해석 실패면 default.
pub fn section_width(d: &Doc, default: Option<i64>) -> R<Option<i64>> {
    let Some(pp) = (0..d.nodes.len()).find(|&i| d.tag(i) == "pagePr") else { return Ok(default) };
    let Some(margin) = d.child(pp, "margin") else {
        // 파이썬은 int(폭) 해석 뒤에 margin.get 에서 AttributeError 를 잡는다. 폭 해석이 거부 대상이면 먼저 거부한다.
        let _ = pyint(d.attr(pp, if d.attr(pp, "landscape") == Some("NARROWLY") { "height" } else { "width" }).unwrap_or("0"))?;
        return Ok(default);
    };
    let key = if d.attr(pp, "landscape") == Some("NARROWLY") { "height" } else { "width" };
    let get = |e: usize, k: &str| -> R<Option<i64>> {
        match d.attr(e, k) {
            None => Ok(None),
            Some(s) => pyint(s),
        }
    };
    let (Some(w), Some(l), Some(r)) = (get(pp, key)?, get(margin, "left")?, get(margin, "right")?) else { return Ok(default) };
    Ok(Some(w - l - r))
}

// ---------------------------------------------------------------- 표 칸 너비

const SPACES: [char; 4] = [' ', '\t', '\u{a0}', '\u{3000}'];
const OBJECTS: [&str; 15] =
    ["tbl", "pic", "ole", "equation", "rect", "ellipse", "line", "arc", "polygon", "curve", "connectLine", "container", "textart", "chart", "video"];

pub fn is_space4(c: char) -> bool {
    SPACES.contains(&c)
}

/// `format_elements.run_text`
pub fn run_text(d: &Doc, run: usize) -> String {
    let mut s = String::new();
    for &t in &d.nodes[run].children {
        if d.tag(t) != "t" {
            continue;
        }
        if let Some(x) = &d.nodes[t].text {
            s.push_str(x);
        }
        for &sub in &d.nodes[t].children {
            match d.tag(sub) {
                "fwSpace" | "nbSpace" => s.push(' '),
                "tab" => s.push('\t'),
                "lineBreak" => s.push('\n'),
                _ => {}
            }
            if let Some(x) = &d.nodes[sub].tail {
                s.push_str(x);
            }
        }
    }
    s
}

pub fn paragraph_texts(d: &Doc, tc: usize) -> Vec<String> {
    d.cell_paras(tc)
        .into_iter()
        .map(|p| {
            let t: String = d.nodes[p].children.iter().filter(|&&r| d.tag(r) == "run").map(|&r| run_text(d, r)).collect();
            t.replace('\n', " ").trim_matches(is_space4).to_string()
        })
        .collect()
}

pub fn cell_length(d: &Doc, tc: usize) -> usize {
    paragraph_texts(d, tc).iter().map(|t| clen(t)).max().unwrap_or(0)
}

fn half_width(c: char) -> bool {
    let o = c as u32;
    o < 0x0370 || (0x2000..=0x206F).contains(&o) || (0xFF61..=0xFFDC).contains(&o)
}

fn longest_word(d: &Doc, tc: usize) -> f64 {
    let mut best = 0.0f64;
    for text in paragraph_texts(d, tc) {
        for word in text.split(is_space4).filter(|w| !w.is_empty()) {
            let w: f64 = py_sum(word.chars().map(|c| if half_width(c) { 0.5 } else { 1.0 }));
            if w > best {
                best = w;
            }
        }
    }
    best
}

#[derive(Debug, Clone, Copy)]
pub struct Cell {
    pub tc: usize,
    pub col: i64,
    pub row: i64,
    pub span: i64,
}

/// `_cells`: 칸 주소·크기가 하나라도 없으면 None.
pub fn cells_of(d: &Doc, table: usize) -> R<Option<Vec<Cell>>> {
    let mut out = vec![];
    for tc in d.cells(table) {
        let (addr, span, size) = (d.child(tc, "cellAddr"), d.child(tc, "cellSpan"), d.child(tc, "cellSz"));
        let (Some(addr), Some(_)) = (addr, size) else { return Ok(None) };
        let cs = match span {
            Some(s) => int_or(d.attr(s, "colSpan"), 1)?,
            None => 1,
        };
        out.push(Cell { tc, col: int_or(d.attr(addr, "colAddr"), 0)?, row: int_or(d.attr(addr, "rowAddr"), 0)?, span: cs.max(1) });
    }
    Ok(Some(out))
}

fn row_span(d: &Doc, tc: usize) -> R<i64> {
    match d.child(tc, "cellSpan") {
        Some(s) => Ok(int_or(d.attr(s, "rowSpan"), 1)?.max(1)),
        None => Ok(1),
    }
}

/// `table_width.is_target`
pub fn is_target(d: &Doc, table: usize) -> R<bool> {
    let cols = int_or(d.attr(table, "colCnt"), 0)?;
    let Some(sz) = d.child(table, "sz") else { return Ok(false) };
    if cols < 2 {
        return Ok(false);
    }
    let rel = d.attr(sz, "widthRelTo").filter(|s| !s.is_empty()).unwrap_or("ABSOLUTE");
    if rel != "ABSOLUTE" || is_drawn_table(d, table)? {
        return Ok(false);
    }
    let Some(cells) = cells_of(d, table)? else { return Ok(false) };
    if cells.is_empty() {
        return Ok(false);
    }
    for c in &cells {
        if d.iter(c.tc).skip(1).any(|i| OBJECTS.contains(&d.tag(i))) {
            return Ok(false);
        }
    }
    Ok(cells.iter().any(|c| cell_length(d, c.tc) > 0))
}

pub fn plan_widths(weights: &[f64], floors: &[i64], total: i64) -> Vec<i64> {
    let count = weights.len();
    let sumf: i64 = floors.iter().sum();
    let mut widths = vec![0.0f64; count];
    if sumf >= total {
        let base = if sumf != 0 { sumf } else { count as i64 };
        for (i, &f) in floors.iter().enumerate() {
            widths[i] = (total * if f != 0 { f } else { 1 }) as f64 / base as f64;
        }
    } else {
        let mut fixed: Vec<bool> = vec![false; count];
        loop {
            let rest = total - (0..count).filter(|&i| fixed[i]).map(|i| floors[i]).sum::<i64>();
            let free: Vec<usize> = (0..count).filter(|&i| !fixed[i]).collect();
            let share: f64 = py_sum(free.iter().map(|&i| weights[i]));
            for i in 0..count {
                widths[i] = if fixed[i] {
                    floors[i] as f64
                } else if share != 0.0 {
                    rest as f64 * weights[i] / share
                } else {
                    rest as f64 / free.len() as f64
                };
            }
            let low: Vec<usize> = free.iter().copied().filter(|&i| widths[i] < floors[i] as f64).collect();
            if low.is_empty() {
                break;
            }
            for i in low {
                fixed[i] = true;
            }
        }
    }
    let mut result: Vec<i64> = widths.iter().map(|&w| w.round_ties_even() as i64).collect();
    let mx = *result.iter().max().unwrap();
    let at = result.iter().position(|&x| x == mx).unwrap();
    let diff = total - result.iter().sum::<i64>();
    result[at] += diff;
    result
}

pub fn cap_floors(floors: &[i64], total: i64) -> Vec<i64> {
    let limit = total as f64 * 0.95;
    if floors.iter().sum::<i64>() as f64 <= limit {
        return floors.to_vec();
    }
    let mut ordered = floors.to_vec();
    ordered.sort();
    let mut rest = limit;
    for (n, &value) in ordered.iter().enumerate() {
        let level = rest / (ordered.len() - n) as f64;
        if value as f64 > level {
            return floors.iter().map(|&f| f.min(level as i64)).collect();
        }
        rest -= value as f64;
    }
    floors.to_vec()
}

type Grid = HashMap<i64, HashMap<i64, (usize, i64)>>;

fn grid_of(d: &Doc, cells: &[Cell], rows: i64) -> R<Grid> {
    let mut g: Grid = HashMap::new();
    for c in cells {
        let rs = row_span(d, c.tc)?;
        let mut r = c.row;
        while r < rows.min(c.row + rs) {
            for col in c.col..c.col + c.span {
                g.entry(r).or_default().insert(col, (c.tc, c.span));
            }
            r += 1;
        }
    }
    Ok(g)
}

fn grouped_header(d: &Doc, cells: &[Cell]) -> R<bool> {
    for c in cells {
        if c.row == 0 && c.span > 1 && row_span(d, c.tc)? == 1 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `table_style.header_row_count`
pub fn header_row_count(d: &Doc, table: usize) -> R<i64> {
    let (rows, _) = crate::rules::size(d, table)?;
    let mut spans = vec![];
    for tc in d.cells(table) {
        let (row, _, rspan, _) = crate::rules::position(d, tc)?;
        if row == 0 && rspan < rows {
            spans.push(rspan);
        }
    }
    let mx = spans.iter().copied().max().unwrap_or(1);
    Ok(1.max(mx.min(rows - 1)))
}

pub fn column_ratios(d: &Doc, table: usize) -> R<Option<Vec<f64>>> {
    let Some(cells) = cells_of(d, table)? else { return Ok(None) };
    if cells.is_empty() {
        return Ok(None);
    }
    let cols = int_or(d.attr(table, "colCnt"), 0)?;
    let rows = int_or(d.attr(table, "rowCnt"), 0)?.max(cells.iter().map(|c| c.row).max().unwrap() + 1);
    let grid = grid_of(d, &cells, rows)?;
    let lengths: HashMap<usize, usize> = cells.iter().map(|c| (c.tc, cell_length(d, c.tc))).collect();
    let ratios = |range: &[i64], full: bool| -> Vec<Vec<f64>> {
        let mut found = vec![];
        for &r in range {
            let mut values = vec![];
            let mut ps = PySum::new();
            for c in 0..cols {
                match grid.get(&r).and_then(|m| m.get(&c)) {
                    Some(&(tc, span)) => {
                        let v = lengths[&tc] as f64 / span as f64;
                        values.push(v);
                        ps.add(v);
                    }
                    None => {
                        values.push(0.0);
                        ps.add_int(0);
                    }
                }
            }
            let total = ps.total();
            if total != 0.0 && (!full || values.iter().all(|&v| v != 0.0)) {
                found.push(values.iter().map(|&v| v / total).collect());
            }
        }
        found
    };
    let head = if grouped_header(d, &cells)? { header_row_count(d, table)? } else { 1 };
    let rng = |a: i64, b: i64| -> Vec<i64> { (a..b).collect() };
    let mut found = ratios(&rng(head, rows), true);
    if found.is_empty() && head > 1 {
        found = ratios(&rng(1, head), true);
    }
    if found.is_empty() {
        found = ratios(&rng(head, rows), false);
    }
    if found.is_empty() {
        found = ratios(&[0], true);
    }
    if found.is_empty() {
        found = ratios(&[0], false);
    }
    if found.is_empty() {
        return Ok(None);
    }
    let n = found.len() as f64;
    Ok(Some((0..cols as usize).map(|c| py_sum(found.iter().map(|row| row[c])) / n).collect()))
}

const RESERVE_MIN_SIZE: i64 = 1600;

fn cell_width(d: &Doc, tc: usize) -> R<i64> {
    int_or(d.child(tc, "cellSz").and_then(|s| d.attr(s, "width")), 0)
}

fn plan_with_empty_columns(d: &Doc, cells: &[Cell], cols: i64, weights: &[f64], floors: &[i64], total: i64) -> R<Option<Vec<i64>>> {
    let n = cols as usize;
    let mut original = vec![0i64; n];
    let mut empty = vec![true; n];
    for c in cells {
        if c.span == 1 && 0 <= c.col && c.col < cols {
            original[c.col as usize] = original[c.col as usize].max(cell_width(d, c.tc)?);
        }
        let joined: String = paragraph_texts(d, c.tc).concat();
        if !py_strip(&joined).is_empty() {
            for k in c.col.max(0)..cols.min(c.col + c.span) {
                empty[k as usize] = false;
            }
        }
    }
    let keep: Vec<usize> = (0..n).filter(|&k| empty[k] && original[k] > 0).collect();
    if keep.is_empty() || keep.len() == n || original.iter().sum::<i64>() == 0 {
        return Ok(Some(plan_widths(weights, &cap_floors(floors, total), total)));
    }
    let mut row_widths: HashMap<i64, i64> = HashMap::new();
    let mut coverage: Vec<(i64, Vec<i64>)> = vec![]; // 삽입 순서 유지
    for c in cells {
        let width = cell_width(d, c.tc)?;
        if c.col < 0 || c.col + c.span > cols || width <= 0 {
            return Ok(None);
        }
        *row_widths.entry(c.row).or_insert(0) += width;
        let at = match coverage.iter().position(|(r, _)| *r == c.row) {
            Some(a) => a,
            None => {
                coverage.push((c.row, vec![0; n]));
                coverage.len() - 1
            }
        };
        for k in c.col..c.col + c.span {
            coverage[at].1[k as usize] += 1;
        }
    }
    let complete: Vec<i64> = coverage.iter().filter(|(_, cv)| cv.iter().all(|&x| x == 1)).map(|(r, _)| row_widths[r]).collect();
    if complete.is_empty() {
        return Ok(None);
    }
    let original_total = original.iter().sum::<i64>().max(*complete.iter().max().unwrap());
    let kept: Vec<(usize, i64)> = keep.iter().map(|&k| (k, ((original[k] * total) as f64 / original_total as f64) as i64)).collect();
    let rest = total - kept.iter().map(|x| x.1).sum::<i64>();
    let others: Vec<usize> = (0..n).filter(|k| !kept.iter().any(|x| x.0 == *k)).collect();
    let share0: Vec<f64> = others.iter().map(|&k| weights[k]).collect();
    let ssum = py_sum(share0.iter().copied());
    let share: Vec<f64> = if ssum != 0.0 { share0.iter().map(|&x| x / ssum).collect() } else { vec![1.0 / others.len() as f64; others.len()] };
    let of: Vec<i64> = others.iter().map(|&k| floors[k]).collect();
    let planned = plan_widths(&share, &cap_floors(&of, rest), rest);
    let mut widths = vec![0i64; n];
    for (k, w) in others.iter().zip(planned) {
        widths[*k] = w;
    }
    for (k, w) in kept {
        widths[k] = w;
    }
    Ok(Some(widths))
}

pub fn host_reserve(d: &Doc, paragraph: Option<usize>, table: usize, idx: &HIdx) -> R<i64> {
    let pos = d.child(table, "pos");
    let (Some(p), Some(pos)) = (paragraph, pos) else { return Ok(0) };
    if d.attr(pos, "treatAsChar") != Some("1") {
        return Ok(0);
    }
    let mut reserve = match idx.para(d.attr(p, "paraPrIDRef")) {
        Some(v) => v.left.max(0) + v.right.max(0) + v.indent.max(0),
        None => 0,
    };
    for &run in &d.nodes[p].children {
        if d.tag(run) != "run" {
            continue;
        }
        let (sz, ratio) = match idx.char(d.attr(run, "charPrIDRef")) {
            Some(v) => (v.size, v.ratio),
            None => (1000, 100),
        };
        let size = (sz.max(RESERVE_MIN_SIZE) * ratio) as f64 / 100.0;
        for &child in &d.nodes[run].children {
            if child == table {
                return Ok(reserve);
            }
            if d.tag(child) == "t" {
                let mut text = d.nodes[child].text.clone().unwrap_or_default();
                for &c in &d.nodes[child].children {
                    if let Some(t) = &d.nodes[c].tail {
                        text.push_str(t);
                    }
                }
                let w = py_sum(text.chars().map(|ch| if half_width(ch) && !py_ws(ch) { 0.6 } else { 1.0 }));
                reserve += (w * size) as i64;
            }
        }
    }
    Ok(reserve)
}

fn boundaries(d: &Doc, cells: &[Cell], cols: i64) -> R<Option<HashMap<i64, i64>>> {
    let mut edges: HashMap<i64, i64> = HashMap::new();
    edges.insert(0, 0);
    let mut spans = vec![];
    for c in cells {
        spans.push((c.col, c.col + c.span, cell_width(d, c.tc)?));
    }
    let mut changed = true;
    while changed {
        changed = false;
        for &(left, right, width) in &spans {
            if edges.contains_key(&left) && !edges.contains_key(&right) {
                edges.insert(right, edges[&left] + width);
                changed = true;
            } else if edges.contains_key(&right) && !edges.contains_key(&left) {
                edges.insert(left, edges[&right] - width);
                changed = true;
            }
        }
    }
    if (0..=cols).any(|i| !edges.contains_key(&i)) {
        return Ok(None);
    }
    if spans.iter().any(|&(l, r, w)| edges[&r] - edges[&l] != w) {
        return Ok(None);
    }
    Ok(Some(edges))
}

fn remove_cache(d: &mut Doc, tc: usize) {
    for p in d.cell_paras(tc) {
        let rm: Vec<usize> = d.nodes[p].children.iter().copied().filter(|&x| d.tag(x) == "linesegarray").collect();
        for x in rm {
            d.remove_child(p, x);
        }
    }
}

fn scale_to_fit(d: &mut Doc, table: usize, cells: &[Cell], total: i64) -> R<bool> {
    let size = d.child(table, "sz").ok_or("sz 없음")?;
    let current = int_or(d.attr(size, "width"), 0)?;
    if current == 0 || current <= total {
        return Ok(false);
    }
    let ratio = total as f64 / current as f64;
    let cols = int_or(d.attr(table, "colCnt"), 0)?;
    let edges = boundaries(d, cells, cols)?.map(|e| {
        e.into_iter().map(|(i, x)| (i, if x == current { total } else { (x as f64 * ratio).round_ties_even() as i64 })).collect::<HashMap<_, _>>()
    });
    for c in cells {
        let cell = d.child(c.tc, "cellSz").unwrap();
        let w = match &edges {
            Some(e) => e[&(c.col + c.span)] - e[&c.col],
            None => (int_or(d.attr(cell, "width"), 0)? as f64 * ratio) as i64,
        };
        d.set_attr(cell, "width", &w.to_string());
        remove_cache(d, c.tc);
    }
    d.set_attr(size, "width", &total.to_string());
    if let Some(cap) = d.child(table, "caption") {
        if d.attr(cap, "lastWidth").map_or(false, |s| !s.is_empty()) {
            d.set_attr(cap, "lastWidth", &total.to_string());
        }
    }
    Ok(true)
}

/// `table_width.fit_table`
pub fn fit_table(d: &mut Doc, table: usize, body_width: Option<i64>, idx: &HIdx, reserve: i64) -> R<bool> {
    let Some(body_width) = body_width.filter(|&w| w != 0) else { return Ok(false) };
    if !is_target(d, table)? {
        return Ok(false);
    }
    let cells = cells_of(d, table)?.unwrap();
    let cols = int_or(d.attr(table, "colCnt"), 0)?;
    let out = match d.child(table, "outMargin") {
        Some(o) => int_or(d.attr(o, "left"), 0)? + int_or(d.attr(o, "right"), 0)?,
        None => 0,
    };
    let total = body_width - out - reserve.max(0);
    if total <= cols * 300 {
        return Ok(false);
    }
    if has_unit_caption(d, table) {
        return scale_to_fit(d, table, &cells, total);
    }
    let Some(weights) = column_ratios(d, table)? else { return Ok(false) };
    let in_margin = d.child(table, "inMargin");
    let n = cols as usize;
    let mut floors = vec![600i64; n];
    for c in &cells {
        if c.span != 1 || !(0 <= c.col && c.col < cols) {
            continue;
        }
        let margin = if d.attr(c.tc, "hasMargin") == Some("1") { d.child(c.tc, "cellMargin") } else { in_margin };
        let side = match margin {
            Some(m) => int_or(d.attr(m, "left"), 0)? + int_or(d.attr(m, "right"), 0)?,
            None => 0,
        };
        let mut ch = 0.0f64;
        for p in d.cell_paras(c.tc) {
            for &run in &d.nodes[p].children {
                if d.tag(run) != "run" {
                    continue;
                }
                let v = if py_strip(&run_text(d, run)).is_empty() { None } else { idx.char(d.attr(run, "charPrIDRef")) };
                if let Some(v) = v {
                    let width = (v.size * v.ratio) as f64 / 100.0 * (100 + v.spacing.max(0)) as f64 / 100.0;
                    let w = width * if v.bold { 1.08 } else { 1.0 };
                    if w > ch {
                        ch = w;
                    }
                }
            }
        }
        let k = c.col as usize;
        let f = (longest_word(d, c.tc) * (if ch != 0.0 { ch } else { 1000.0 }) * 1.1) as i64 + side + 200;
        floors[k] = floors[k].max(f);
    }
    let widths = plan_with_empty_columns(d, &cells, cols, &weights, &floors, total)?;
    let Some(widths) = widths else { return Ok(false) };
    if widths.iter().any(|&w| w <= 0) || widths.iter().sum::<i64>() != total || cells.iter().any(|c| c.col < 0 || c.col + c.span > cols) {
        return Ok(false);
    }
    for c in &cells {
        let sz = d.child(c.tc, "cellSz").unwrap();
        let w: i64 = widths[c.col as usize..(c.col + c.span) as usize].iter().sum();
        d.set_attr(sz, "width", &w.to_string());
        remove_cache(d, c.tc);
    }
    let sz = d.child(table, "sz").unwrap();
    d.set_attr(sz, "width", &total.to_string());
    Ok(true)
}
