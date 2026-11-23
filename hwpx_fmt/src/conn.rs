//! 연결 부호 탭 정렬(파이썬 `docfit_core/connector_tabs.py` + `formatter_apply.연결부호탭_hwpx_처리`).
use crate::config::unsupported;
use crate::dom::Doc;
use crate::fmt3::next_id;
use crate::rules::{py_strip, py_ws, R};
use crate::style::{char_em, py_sum};
use crate::tbl::pyint;
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

const PARA_NS: &str = "http://www.hancom.co.kr/hwpml/2011/paragraph";
const HWPUNITCHAR_NS: &str = "http://www.hancom.co.kr/hwpml/2016/HwpUnitChar";
const GAP_EM: f64 = 1.5;

fn is_dash(c: char) -> bool {
    "-\u{2010}\u{2011}\u{2013}\u{2014}\u{2015}".contains(c)
}
fn is_dot(c: char) -> bool {
    ".\u{b7}\u{2219}\u{2022}\u{2025}\u{2026}".contains(c)
}
fn not_ws(c: char) -> bool {
    !py_ws(c)
}

/// `CONNECTOR.finditer(text)` 의 (시작, 끝) 목록(문자 단위). 정규식의 lookbehind/lookahead 를 손으로 푼다.
fn find_connectors(cs: &[char]) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let mut i = 0;
    while i < cs.len() {
        let run = |pred: &dyn Fn(char) -> bool| cs[i..].iter().take_while(|&&c| pred(c)).count();
        let mut hit = None;
        let n = run(&is_dash);
        if n >= 3 {
            hit = Some(i + n);
        }
        if hit.is_none() {
            let n = run(&is_dot);
            if n >= 3 {
                hit = Some(i + n);
            }
        }
        // (?<=\S) {3,}(?=\S) / (?<=\S)　{2,}(?=\S): 앞 글자가 공백이 아니고, 빈칸 묶음 뒤 글자가 공백이 아님
        for (ch, min) in [(' ', 3usize), ('\u{3000}', 2usize)] {
            if hit.is_some() {
                break;
            }
            if i > 0 && not_ws(cs[i - 1]) {
                let n = run(&|c| c == ch);
                if n >= min && i + n < cs.len() && not_ws(cs[i + n]) {
                    hit = Some(i + n);
                }
            }
        }
        match hit {
            Some(e) => {
                out.push((i, e));
                i = e;
            }
            None => i += 1,
        }
    }
    out
}

pub fn kind_of(conn: &str) -> &'static str {
    if py_strip(conn).is_empty() {
        "space"
    } else if conn.chars().next().map_or(false, is_dash) {
        "line"
    } else {
        "dot"
    }
}

static RE_COLON_LABEL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[^\s\x1c-\x1f]{0,3}[\s\x1c-\x1f]*[:：]").unwrap());
static RE_PAREN_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[(（\[][^()（）\[\]]{1,20}[)）\]]$").unwrap());

fn space_connector_ok(left: &str, right: &str) -> bool {
    let r = py_strip(right);
    if RE_COLON_LABEL.is_match(r) {
        return false;
    }
    if RE_PAREN_TAG.is_match(r) {
        return false;
    }
    !(left.contains(':') || left.contains('：'))
}

/// `split_connector`: (왼쪽, 부호, 오른쪽)
pub fn split_connector(text: &str) -> Option<(String, String, String)> {
    let cs: Vec<char> = text.chars().collect();
    let mut found = find_connectors(&cs);
    let s = |a: usize, b: usize| -> String { cs[a..b].iter().collect() };
    if found.len() > 1 && found.iter().all(|&(a, b)| py_strip(&s(a, b)).is_empty()) {
        found = vec![*found.last().unwrap()];
    }
    if found.len() != 1 {
        return None;
    }
    let (a, b) = found[0];
    let (left, conn, right) = (s(0, a), s(a, b), s(b, cs.len()));
    if py_strip(&left).is_empty() || py_strip(&right).is_empty() {
        return None;
    }
    if py_strip(&conn).is_empty() && !space_connector_ok(&left, &right) {
        return None;
    }
    Some((left.trim_end_matches(' ').to_string(), conn, right.trim_start_matches(' ').to_string()))
}

fn text_em(t: &str) -> f64 {
    py_sum(t.chars().map(char_em))
}

#[derive(Debug, Clone)]
pub struct Member {
    pub p: usize,
    pub t: usize,
    pub left: String,
    pub conn: String,
    pub right: String,
}

fn paragraph_t(d: &Doc, p: usize) -> Option<usize> {
    let mut with_text = vec![];
    for &r in &d.nodes[p].children {
        if d.tag(r) != "run" {
            continue;
        }
        for &t in &d.nodes[r].children {
            if d.tag(t) == "t" && (d.nodes[t].text.as_deref().map_or(false, |s| !s.is_empty()) || !d.nodes[t].children.is_empty()) {
                with_text.push(t);
            }
        }
    }
    if with_text.len() != 1 || !d.nodes[with_text[0]].children.is_empty() {
        return None;
    }
    Some(with_text[0])
}

/// `groups_in(container)`
pub fn groups_in(d: &Doc, container: usize) -> Vec<Vec<Member>> {
    let mut out = vec![];
    let mut run: Vec<Member> = vec![];
    for &p in &d.nodes[container].children {
        if d.tag(p) != "p" {
            continue;
        }
        let parts = paragraph_t(d, p).and_then(|t| split_connector(d.nodes[t].text.as_deref().unwrap_or("")).map(|x| (t, x)));
        let has_tbl = d.iter(p).any(|i| d.tag(i) == "tbl");
        match parts {
            Some((t, (l, c, r))) if !has_tbl => {
                if let Some(last) = run.last() {
                    if kind_of(&last.conn) != kind_of(&c) {
                        if run.len() >= 2 {
                            out.push(std::mem::take(&mut run));
                        }
                        run = vec![];
                    }
                }
                run.push(Member { p, t, left: l, conn: c, right: r });
            }
            _ => {
                if run.len() >= 2 {
                    out.push(std::mem::take(&mut run));
                }
                run = vec![];
            }
        }
    }
    if run.len() >= 2 {
        out.push(run);
    }
    out
}

/// root 와 그 안 모든 subList(문서 순서)
pub fn containers(d: &Doc) -> Vec<usize> {
    let mut v = vec![0];
    v.extend((0..d.nodes.len()).filter(|&i| d.tag(i) == "subList"));
    v
}

// ---------------------------------------------------------------- 머리글

fn id_key(d: &Doc, i: usize) -> Option<String> {
    d.attr(i, "id").map(str::to_string)
}

pub struct Head {
    pub tab_group: Option<usize>,
    pub para_group: Option<usize>,
    chars: HashMap<Option<String>, usize>,
    paras: HashMap<Option<String>, usize>,
    made_tabs: HashMap<(i64, String, String), String>,
    made_paras: HashMap<(Option<String>, String), Option<String>>,
    hp_prefix: Option<String>,
}

impl Head {
    pub fn new(h: &Doc) -> Head {
        let mut chars = HashMap::new();
        let mut paras = HashMap::new();
        let (mut tg, mut pg) = (None, None);
        for i in 0..h.nodes.len() {
            match h.tag(i) {
                "paraPr" => {
                    paras.insert(id_key(h, i), i);
                }
                "charPr" => {
                    chars.insert(id_key(h, i), i);
                }
                "tabProperties" if tg.is_none() => tg = Some(i),
                "paraProperties" if pg.is_none() => pg = Some(i),
                _ => {}
            }
        }
        let hp_prefix = crate::run3b::uri_prefixes(h).get(PARA_NS).cloned();
        Head { tab_group: tg, para_group: pg, chars, paras, made_tabs: HashMap::new(), made_paras: HashMap::new(), hp_prefix }
    }

    fn size_pt(&self, h: &Doc, char_id: Option<&str>) -> R<f64> {
        let Some(&c) = self.chars.get(&char_id.map(str::to_string)) else { return Ok(10.0) };
        match pyint(h.attr(c, "height").unwrap_or("1000"))? {
            Some(v) => Ok(v as f64 / 100.0),
            None => Ok(10.0),
        }
    }

    fn margin_left(&self, h: &Doc, para_id: Option<&str>) -> R<i64> {
        let Some(&p) = self.paras.get(&para_id.map(str::to_string)) else { return Ok(0) };
        for i in h.iter(p) {
            if h.tag(i) == "left" {
                return Ok(pyint(h.attr(i, "value").unwrap_or("0"))?.unwrap_or(0));
            }
        }
        Ok(0)
    }

    fn tab_id(&mut self, h: &mut Doc, pos: i64, kind: &str, align: &str) -> R<String> {
        let key = (pos, kind.to_string(), align.to_string());
        if let Some(v) = self.made_tabs.get(&key) {
            return Ok(v.clone());
        }
        let group = self.tab_group.ok_or("tabProperties 없음")?;
        let hh = match h.nodes[group].qname.split_once(':') {
            Some((p, _)) => format!("{p}:"),
            None => String::new(),
        };
        let Some(hp) = self.hp_prefix.clone() else {
            return unsupported("머리글 뿌리에 문단 이름공간(hp) 선언이 없어 탭 요소를 만들 수 없습니다.");
        };
        let new_id = next_id(h, group)?;
        let (leader, _) = leader_of(kind);
        let tab = h.new_elem(&format!("{hh}tabPr"), vec![("id".into(), new_id.clone()), ("autoTabLeft".into(), "0".into()), ("autoTabRight".into(), "0".into())]);
        let switch = h.new_elem(&format!("{hp}:switch"), vec![]);
        let case = h.new_elem(&format!("{hp}:case"), vec![(format!("{hp}:required-namespace"), HWPUNITCHAR_NS.into())]);
        let item1 = h.new_elem(
            &format!("{hh}tabItem"),
            vec![("pos".into(), pos.to_string()), ("type".into(), align.into()), ("leader".into(), leader.into()), ("unit".into(), "HWPUNIT".into())],
        );
        h.append_child(case, item1);
        let default = h.new_elem(&format!("{hp}:default"), vec![]);
        let item2 = h.new_elem(&format!("{hh}tabItem"), vec![("pos".into(), (pos * 2).to_string()), ("type".into(), align.into()), ("leader".into(), leader.into())]);
        h.append_child(default, item2);
        h.append_child(switch, case);
        h.append_child(switch, default);
        h.append_child(tab, switch);
        h.append_child(group, tab);
        let n = h.nodes[group].children.len().to_string();
        h.set_attr(group, "itemCnt", &n);
        self.made_tabs.insert(key, new_id.clone());
        Ok(new_id)
    }

    fn para_with_tab(&mut self, h: &mut Doc, para_id: Option<&str>, tab_id: &str) -> R<String> {
        let key = (para_id.map(str::to_string), tab_id.to_string());
        if let Some(v) = self.made_paras.get(&key) {
            return v.clone().ok_or_else(|| "cannot serialize None".to_string());
        }
        let Some(&source) = self.paras.get(&para_id.map(str::to_string)) else {
            return para_id.map(str::to_string).ok_or_else(|| "paraPrIDRef 없음(파이썬은 직렬화에서 실패)".to_string());
        };
        let group = self.para_group.ok_or("paraProperties 없음")?;
        let clone = h.clone_subtree(source);
        let new_id = next_id(h, group)?;
        h.set_attr(clone, "id", &new_id);
        h.set_attr(clone, "tabPrIDRef", tab_id);
        h.append_child(group, clone);
        let n = h.nodes[group].children.len().to_string();
        h.set_attr(group, "itemCnt", &n);
        self.paras.insert(Some(new_id.clone()), clone);
        self.made_paras.insert(key, Some(new_id.clone()));
        Ok(new_id)
    }
}

fn leader_of(kind: &str) -> (&'static str, &'static str) {
    match kind {
        "dot" => ("DASH", "2"),
        "line" => ("DOT", "3"),
        _ => ("NONE", "0"),
    }
}

#[derive(Debug, Default)]
pub struct Stats {
    pub groups: usize,
    pub paragraphs: usize,
    pub pulled: usize,
}

/// 칸·본문 폭 계산에 쓰는 구역별 정보
pub struct WidthCtx {
    /// 구역(정렬된 순서)마다 본문 폭
    pub body: Vec<i64>,
}

fn t_parent_char(d: &Doc, p: usize, t: usize) -> Option<String> {
    for &r in &d.nodes[p].children {
        if d.tag(r) == "run" && d.nodes[r].children.contains(&t) {
            return d.attr(r, "charPrIDRef").map(str::to_string);
        }
    }
    None
}

/// subList(container)의 칸 폭: 부모가 tc 면 cellSz 폭 − 칸 안 여백, 아니면 0
fn cell_text_width(d: &Doc, container: usize) -> R<i64> {
    let cell = (0..d.nodes.len()).find(|&i| d.tag(i) == "tc" && d.nodes[i].children.contains(&container));
    let Some(cell) = cell else { return Ok(0) };
    let Some(sz) = d.child(cell, "cellSz") else { return Ok(0) };
    let Some(w) = d.attr(sz, "width") else { return Ok(0) };
    let Some(w) = pyint(w)? else { return Ok(0) };
    let mg = d.child(cell, "cellMargin");
    let side = match mg {
        Some(m) => {
            let l = match d.attr(m, "left") {
                Some(s) => pyint(s)?,
                None => Some(0),
            };
            let r = match d.attr(m, "right") {
                Some(s) => pyint(s)?,
                None => Some(0),
            };
            match (l, r) {
                (Some(l), Some(r)) => l + r,
                _ => return Ok(0),
            }
        }
        None => 0,
    };
    Ok(w - side)
}

/// `align_connectors(header, sections, 폭)`. sections 는 구역 정렬 순서의 문서들, body 는 구역별 본문 폭.
pub fn align_connectors(h: &mut Doc, sections: &mut [&mut Doc], body: &[i64]) -> R<Stats> {
    let mut head = Head::new(h);
    if head.tab_group.is_none() || head.para_group.is_none() {
        return Ok(Stats::default());
    }
    let mut stats = Stats::default();
    for (si, d) in sections.iter_mut().enumerate() {
        for container in containers(d) {
            for group in groups_in(d, container) {
                let mut size = f64::NEG_INFINITY;
                for m in &group {
                    let s = head.size_pt(h, t_parent_char(d, m.p, m.t).as_deref())?;
                    if s > size {
                        size = s;
                    }
                }
                let em = size * 100.0;
                let left = group.iter().map(|m| text_em(&m.left)).fold(f64::NEG_INFINITY, f64::max) * em;
                let right = group.iter().map(|m| text_em(&m.right)).fold(f64::NEG_INFINITY, f64::max) * em;
                let mut margin = i64::MIN;
                for m in &group {
                    margin = margin.max(head.margin_left(h, d.attr(m.p, "paraPrIDRef"))?);
                }
                let tw = if container == 0 { body[si] } else { cell_text_width(d, container)? };
                let width = tw - margin;
                let mut pos = (left + GAP_EM * em) as i64;
                if width > 0 && pos as f64 + right > width as f64 {
                    pos = (left + 0.5 * em).max(width as f64 - right) as i64;
                    stats.pulled += 1;
                }
                let kinds: Vec<&str> = {
                    let mut k: Vec<&str> = vec![];
                    for m in &group {
                        let x = kind_of(&m.conn);
                        if !k.contains(&x) {
                            k.push(x);
                        }
                    }
                    k
                };
                let kind = if kinds.len() == 1 { kinds[0] } else { "dot" };
                let tab = head.tab_id(h, pos, kind, "LEFT")?;
                for m in &group {
                    let pid = head.para_with_tab(h, d.attr(m.p, "paraPrIDRef"), &tab)?;
                    d.set_attr(m.p, "paraPrIDRef", &pid);
                    let rm: Vec<usize> = d.nodes[m.p].children.iter().copied().filter(|&x| d.tag(x) == "linesegarray").collect();
                    for x in rm {
                        d.remove_child(m.p, x);
                    }
                    let tq = d.nodes[m.t].qname.clone();
                    let q = match tq.split_once(':') {
                        Some((p, _)) => format!("{p}:tab"),
                        None => "tab".to_string(),
                    };
                    d.nodes[m.t].text = Some(m.left.clone());
                    let el = d.new_elem(&q, vec![("width".into(), "0".into()), ("leader".into(), leader_of(kind).1.into()), ("type".into(), "1".into())]);
                    d.nodes[el].tail = Some(m.right.clone());
                    d.append_child(m.t, el);
                    stats.paragraphs += 1;
                }
                stats.groups += 1;
            }
        }
    }
    Ok(stats)
}
