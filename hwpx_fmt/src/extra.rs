//! RSXML2 처리기 3종: 개요 표 첫 빈칸(`overview_lead`), 쪽 번호 모양(`page_number`), 자간 초기화(`spacing_reset`).
//! 파이썬 근거: `formatter_apply.py 개요첫칸_hwpx_처리`, `formatter_titles.py 쪽번호_hwpx_처리`,
//! `spacing.py 자간초기화_hwpx_처리` + `docfit_core/spacing_reset.py`.
use crate::config::Config;
use crate::dom::Doc;
use crate::marker::spacing_protect;
use crate::rules::{format_kind, is_user_designed_table, py_strip, py_ws, FormatKind, R};
use std::collections::HashMap;

// ---- 개요 표 첫 빈칸 ------------------------------------------------------

/// 개요첫칸 `대상문단(root)`: 제목 표 바로 뒤 개요 상자 A1 칸의 (앞이 빈칸이 아닌) 글 문단.
pub fn overview_targets(d: &Doc) -> R<Vec<usize>> {
    let mut out = vec![];
    let mut prev: Option<FormatKind> = None;
    for &p in &d.nodes[0].children {
        if d.tag(p) != "p" {
            continue;
        }
        let mut tables = vec![];
        for &r in &d.nodes[p].children {
            if d.tag(r) == "run" {
                for &t in &d.nodes[r].children {
                    if d.tag(t) == "tbl" {
                        tables.push(t);
                    }
                }
            }
        }
        if tables.is_empty() {
            if !py_strip(&d.jstr(p)).is_empty() {
                prev = None; // 글 문단이 끼면 제목 표와 이어진 개요가 아니다
            }
            continue;
        }
        for t in tables {
            let kind = format_kind(d, t)?;
            if kind == Some(FormatKind::Box) && prev == Some(FormatKind::Title) {
                let cell = d.cells(t)[0];
                if let Some(sub) = d.child(cell, "subList") {
                    for &q in &d.nodes[sub].children {
                        if d.tag(q) == "p" {
                            let s = d.jstr(q);
                            if !py_strip(&s).is_empty() && !s.chars().next().map_or(false, py_ws) {
                                out.push(q);
                            }
                        }
                    }
                }
            }
            prev = kind;
        }
    }
    Ok(out)
}

/// 대상 문단의 첫 글자 앞에 빈칸 1칸. 바꾼 문단 수.
pub fn apply_overview(d: &mut Doc) -> R<usize> {
    let targets = overview_targets(d)?;
    let mut count = 0;
    for q in targets {
        // 첫 번째 '비어 있지 않은 글 요소'(run 바로 아래 t)
        let mut found = None;
        'o: for &r in &d.nodes[q].children {
            if d.tag(r) == "run" {
                for &t in &d.nodes[r].children {
                    if d.tag(t) == "t" && !py_strip(d.nodes[t].text.as_deref().unwrap_or("")).is_empty() {
                        found = Some(t);
                        break 'o;
                    }
                }
            }
        }
        let Some(t) = found else { continue };
        let text = d.nodes[t].text.clone().unwrap_or_default();
        let lead_ws = text.chars().next().map_or(false, py_ws);
        let first = py_strip(&text).chars().next();
        if lead_ws || d.jstr(q).chars().next() != first {
            continue; // 첫 글자가 이 글 요소의 맨 앞이 아닌 복잡한 문단은 그대로 둔다
        }
        d.nodes[t].text = Some(format!(" {text}"));
        let caches: Vec<usize> = d.nodes[q].children.iter().copied().filter(|&x| d.tag(x) == "linesegarray").collect();
        for c in caches {
            d.remove_child(q, c);
        }
        count += 1;
    }
    Ok(count)
}

// ---- 쪽 번호 모양 ---------------------------------------------------------

fn page_nums(d: &Doc) -> Vec<usize> {
    (0..d.nodes.len()).filter(|&i| d.tag(i) == "pageNum").collect()
}

/// 모양과 다른 쪽 번호 요소의 순번(쪽번호들 목록 안 번호). 모양이 비면 없음.
pub fn page_number_survey(d: &Doc, cfg: &Config) -> Vec<usize> {
    if cfg.page_number.is_empty() {
        return vec![];
    }
    page_nums(d)
        .into_iter()
        .enumerate()
        .filter(|&(_, x)| cfg.page_number.iter().any(|(k, v)| d.attr(x, k) != Some(v.as_str())))
        .map(|(i, _)| i)
        .collect()
}

pub fn apply_page_number(d: &mut Doc, items: &[usize], cfg: &Config) -> R<usize> {
    let found = page_nums(d);
    let mut count = 0;
    for &index in items {
        let x = *found.get(index).ok_or_else(|| format!("쪽 번호 번호 범위 밖: {index}"))?;
        for (k, v) in &cfg.page_number {
            d.set_attr(x, k, v);
        }
        count += 1;
    }
    Ok(count)
}

// ---- 자간 초기화 ----------------------------------------------------------

#[derive(Default, Debug, Clone, Copy)]
pub struct SpacingStats {
    pub paragraphs: usize,
    pub runs: usize,
    pub split: usize,
    pub skipped: usize,
}

struct Chars {
    group: Option<usize>,
    chars: HashMap<Option<String>, usize>,
    made: HashMap<Option<String>, Option<String>>,
}

fn run_text(d: &Doc, run: usize) -> String {
    let mut s = String::new();
    for &t in &d.nodes[run].children {
        if d.tag(t) == "t" {
            s.push_str(d.nodes[t].text.as_deref().unwrap_or(""));
            for &c in &d.nodes[t].children {
                s.push_str(d.nodes[c].tail.as_deref().unwrap_or(""));
            }
        }
    }
    s
}

fn plain_run(d: &Doc, run: usize) -> bool {
    let k = &d.nodes[run].children;
    k.len() == 1 && d.tag(k[0]) == "t" && d.nodes[k[0]].children.is_empty()
}

impl Chars {
    fn new(h: &Doc) -> Chars {
        let mut group = None;
        let mut chars = HashMap::new();
        for i in 0..h.nodes.len() {
            if group.is_none() && h.tag(i) == "charProperties" {
                group = Some(i);
            }
            if h.tag(i) == "charPr" {
                chars.insert(h.attr(i, "id").map(str::to_string), i);
            }
        }
        Chars { group, chars, made: HashMap::new() }
    }

    fn spacing_nonzero(&self, h: &Doc, id: &Option<String>) -> bool {
        let Some(&c) = self.chars.get(id) else { return false };
        match h.child(c, "spacing") {
            Some(sp) => h.nodes[sp].attrs.iter().any(|(_, v)| !(v == "0" || v.is_empty())),
            None => false,
        }
    }

    fn zero(&mut self, h: &mut Doc, id: &Option<String>) -> R<Option<String>> {
        if let Some(m) = self.made.get(id) {
            return Ok(m.clone());
        }
        let (Some(&source), Some(group)) = (self.chars.get(id), self.group) else { return Ok(id.clone()) };
        let clone = h.clone_subtree(source);
        // 새 번호 = 1 + (group 바로 아래 id 가 숫자인 요소들의 최댓값, 없으면 -1)
        let mut max: i128 = -1;
        for &x in &h.nodes[group].children {
            if let Some(v) = h.attr(x, "id") {
                if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()) {
                    max = max.max(v.parse::<i128>().map_err(|e| e.to_string())?);
                } else if v.chars().any(|c| !c.is_ascii() && c.is_numeric()) {
                    return crate::config::unsupported("글자 모양 번호에 ASCII 가 아닌 숫자가 있습니다(파이썬 isdigit/int 해석 차이).");
                }
            }
        }
        let new_id = (max + 1).to_string();
        h.set_attr(clone, "id", &new_id);
        let kids = h.nodes[clone].children.clone();
        for k in kids {
            if h.tag(k) == "spacing" {
                let n = h.nodes[k].attrs.len();
                for a in 0..n {
                    h.nodes[k].attrs[a].1 = "0".to_string();
                }
            }
        }
        h.append_child(group, clone);
        let cnt = h.nodes[group].children.len().to_string();
        h.set_attr(group, "itemCnt", &cnt);
        self.chars.insert(Some(new_id.clone()), clone);
        self.made.insert(id.clone(), Some(new_id.clone()));
        Ok(Some(new_id))
    }
}

/// reset_spacing: 모든 구역(번호 순서)의 문단 자간을 0 으로. 구역·머리 XML 을 제자리에서 바꾼다.
pub fn reset_spacing(header: &mut Doc, sections: &mut [&mut Doc]) -> R<SpacingStats> {
    let mut chars = Chars::new(header);
    let mut stats = SpacingStats::default();
    for d in sections.iter_mut() {
        // 사용자가 꾸민 표 안의 문단은 건드리지 않는다.
        let mut kept = vec![false; d.nodes.len()];
        for t in 0..d.nodes.len() {
            if d.tag(t) == "tbl" && is_user_designed_table(d, t)? {
                for i in d.iter(t) {
                    kept[i] = true;
                }
            }
        }
        let paras: Vec<usize> = (0..d.nodes.len()).filter(|&i| d.tag(i) == "p" && !kept[i]).collect();
        for p in paras {
            let runs: Vec<usize> = d.nodes[p].children.iter().copied().filter(|&r| d.tag(r) == "run").collect();
            let ids: Vec<Option<String>> = runs.iter().map(|&r| d.attr(r, "charPrIDRef").map(str::to_string)).collect();
            if !ids.iter().any(|i| chars.spacing_nonzero(header, i)) {
                continue;
            }
            let texts: Vec<String> = runs.iter().map(|&r| run_text(d, r)).collect();
            let protect = spacing_protect(&texts.concat());
            let (mut pos, mut changed) = (0usize, 0usize);
            for (k, &run) in runs.iter().enumerate() {
                let (start, end) = (pos, pos + texts[k].chars().count());
                pos = end;
                let cid = &ids[k];
                if !chars.spacing_nonzero(header, cid) {
                    continue;
                }
                if start >= protect {
                    let z = chars.zero(header, cid)?;
                    set_opt_attr(d, run, "charPrIDRef", &z);
                    changed += 1;
                } else if end <= protect {
                    continue;
                } else if plain_run(d, run) {
                    let cut = protect - start;
                    let t = d.nodes[run].children[0];
                    let tail = d.clone_subtree(run);
                    let full: Vec<char> = d.nodes[t].text.clone().unwrap_or_default().chars().collect();
                    let tail_t = d.nodes[tail].children[0];
                    d.nodes[tail_t].text = Some(full[cut.min(full.len())..].iter().collect());
                    d.nodes[t].text = Some(full[..cut.min(full.len())].iter().collect());
                    let z = chars.zero(header, cid)?;
                    set_opt_attr(d, tail, "charPrIDRef", &z);
                    let pos_in_p = d.nodes[p].children.iter().position(|&c| c == run).unwrap();
                    d.insert_child(p, pos_in_p + 1, tail);
                    changed += 1;
                    stats.split += 1;
                } else {
                    stats.skipped += 1;
                }
            }
            if changed > 0 {
                let caches: Vec<usize> = d.nodes[p].children.iter().copied().filter(|&x| d.tag(x) == "linesegarray").collect();
                for c in caches {
                    d.remove_child(p, c);
                }
                stats.paragraphs += 1;
                stats.runs += changed;
            }
        }
    }
    Ok(stats)
}

fn set_opt_attr(d: &mut Doc, i: usize, k: &str, v: &Option<String>) {
    if let Some(v) = v {
        d.set_attr(i, k, v);
    }
}
