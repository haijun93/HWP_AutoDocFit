//! RSXML3 처리기 3종: 별표 위첨자(`asterisk_superscript`), 붙임 글꼴(`attachment_font`), 콜론 라벨 정렬(`colon_label`).
//! 파이썬 근거: `formatter_apply.py` 별표위첨자_hwpx_처리·붙임글꼴_hwpx_처리·콜론라벨_hwpx_처리,
//! `docfit_core/asterisk_superscript.py`·`attachment_block.py`·`colon_labels.py`.
//! 서식 ID 규칙은 파이썬과 같다: 새 charPr/paraPr/font 는 그룹 끝에 붙이고 id = (숫자 id 최댓값 + 1), itemCnt/fontCnt 갱신,
//! 같은 원본·같은 변경은 사본 하나를 공유(캐시), 원본 정의는 건드리지 않는다(copy-on-write).
//! 오프셋은 모두 문자(코드 포인트) 수다. 수정은 children 목록으로만 하고 iter() 구간에 기대지 않는다.
use crate::config::Config;
use crate::dom::Doc;
use crate::marker::classify_marker;
use crate::rules::{clen, py_int_pub, py_strip, py_ws, R};
use crate::style::{char_em, leading_marker, py_round};
use regex::Regex;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

pub type Key = Option<String>;

fn s(d: &Doc, i: usize, k: &str) -> Key {
    d.attr(i, k).map(|x| x.to_string())
}

pub fn ctext(d: &Doc, t: usize) -> String {
    d.nodes[t].text.clone().unwrap_or_default()
}

pub fn body_paras(d: &Doc) -> Vec<usize> {
    d.nodes[0].children.iter().copied().filter(|&c| d.tag(c) == "p").collect()
}

const SETTING_CTRLS: [&str; 8] = ["colPr", "secPr", "pageNum", "pageNumCtrl", "pageHiding", "newNum", "header", "footer"];

/// `_문단_일반글`: 표·그림·필드가 없는 일반 글 문단이면 (run, 글 요소) 목록, 아니면 None.
pub fn general_texts(d: &Doc, p: usize) -> Option<Vec<(usize, usize)>> {
    general_texts_opt(d, p, false)
}

/// flat 이면 글 요소 안 자식(탭)을 허용한다(탭을 문자로 펼친 판정용 사본과 같다).
fn general_texts_opt(d: &Doc, p: usize, flat: bool) -> Option<Vec<(usize, usize)>> {
    let mut out = vec![];
    for &run in &d.nodes[p].children {
        let rt = d.tag(run);
        if rt == "linesegarray" {
            continue;
        }
        if rt != "run" {
            return None;
        }
        for &c in &d.nodes[run].children {
            match d.tag(c) {
                "t" => {
                    if !flat && !d.nodes[c].children.is_empty() {
                        return None;
                    }
                    out.push((run, c));
                }
                "secPr" => {}
                "ctrl" => {
                    if d.nodes[c].children.iter().any(|&x| !SETTING_CTRLS.contains(&d.tag(x))) {
                        return None;
                    }
                }
                _ => return None,
            }
        }
    }
    Some(out)
}

fn is_digit_id(v: Option<&str>) -> bool {
    let v = v.unwrap_or("");
    !v.is_empty() && v.chars().all(|c| c.is_ascii_digit())
}

/// `str(max([int(x.get('id')) for x in group if (x.get('id') or '').isdigit()] + [-1]) + 1)`
pub fn next_id(d: &Doc, group: usize) -> R<String> {
    let mut m: i64 = -1;
    for &x in &d.nodes[group].children {
        if is_digit_id(d.attr(x, "id")) {
            m = m.max(d.attr(x, "id").unwrap().parse::<i64>().map_err(|e| e.to_string())?);
        }
    }
    Ok((m + 1).to_string())
}

pub fn first_by_tag(d: &Doc, tag: &str) -> Option<usize> {
    d.walk(0).into_iter().find(|&i| d.tag(i) == tag)
}

fn all_by_tag(d: &Doc, tag: &str) -> Vec<usize> {
    d.walk(0).into_iter().filter(|&i| d.tag(i) == tag).collect()
}

fn remove_lineseg(d: &mut Doc, p: usize) {
    let rm: Vec<usize> = d.nodes[p].children.iter().copied().filter(|&x| d.tag(x) == "linesegarray").collect();
    for x in rm {
        d.remove_child(p, x);
    }
}

// ============================================================ 별표 위첨자

fn marker_end_ast(text: &str) -> Option<usize> {
    if let Some(e) = classify_marker(text) {
        return Some(e);
    }
    let (m, _) = leading_marker(text);
    if !m.is_empty() {
        return Some(clen(text) - clen(text.trim_start_matches(py_ws)) + clen(&m));
    }
    None
}

/// `mark_spans`: 문장 속 위첨자로 만들 별표 (시작, 끝)(문자 수). 정규식 `(?<=[^\s*])(\*{1,2})(?![*A-Za-z0-9])` 와 같다.
pub fn star_spans(text: &str) -> Vec<(usize, usize)> {
    let Some(me) = marker_end_ast(text) else { return vec![] };
    let cs: Vec<char> = text.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == '*' && i > 0 && !py_ws(cs[i - 1]) && cs[i - 1] != '*' {
            let mut j = i;
            while j < cs.len() && cs[j] == '*' {
                j += 1;
            }
            let run = j - i;
            // 1~2개짜리 별표 묶음만, 뒤에 영문·숫자가 오지 않을 때(3개 이상은 앞 두 개 뒤가 별표라 실패)
            if run <= 2 && !(j < cs.len() && cs[j].is_ascii_alphanumeric()) {
                if i >= me {
                    out.push((i, j));
                }
                i = j;
                continue;
            }
            i = j;
            continue;
        }
        i += 1;
    }
    out
}

type Piece = (usize, usize, usize, usize); // (run, t, lo, hi)

fn star_plan(d: &Doc, p: usize) -> Vec<Piece> {
    let Some(texts) = general_texts(d, p) else { return vec![] };
    if texts.is_empty() {
        return vec![];
    }
    let full: String = texts.iter().map(|&(_, t)| ctext(d, t)).collect();
    let spans = star_spans(&full);
    if spans.is_empty() {
        return vec![];
    }
    let mut out = vec![];
    let mut offset = 0;
    for (run, t) in texts {
        let len = clen(&ctext(d, t));
        for &(st, en) in &spans {
            let (lo, hi) = (st.max(offset), en.min(offset + len));
            if lo < hi {
                out.push((run, t, lo - offset, hi - offset));
            }
        }
        offset += len;
    }
    out
}

pub fn super_ids(h: &Doc) -> HashSet<Key> {
    all_by_tag(h, "charPr").into_iter().filter(|&x| h.nodes[x].children.iter().any(|&c| h.tag(c) == "supscript")).map(|x| s(h, x, "id")).collect()
}

fn already(d: &Doc, pc: &Piece, sup: &HashSet<Key>) -> bool {
    sup.contains(&s(d, pc.0, "charPrIDRef"))
}

/// 조사: 문단마다 (번호, 아직 위첨자가 아닌 별표 수).
pub fn asterisk_survey(d: &Doc, h: &Doc) -> Vec<(usize, usize)> {
    let sup = super_ids(h);
    let mut out = vec![];
    for (i, p) in body_paras(d).into_iter().enumerate() {
        let n = star_plan(d, p).iter().filter(|pc| !already(d, pc, &sup)).count();
        if n > 0 {
            out.push((i, n));
        }
    }
    out
}

/// `_위첨자_글자모양`
fn super_char(h: &mut Doc, char_id: &Key, cache: &mut HashMap<Key, Key>) -> R<Key> {
    if let Some(v) = cache.get(char_id) {
        return Ok(v.clone());
    }
    let group = first_by_tag(h, "charProperties").ok_or("charProperties 가 없습니다.")?;
    let source = h.nodes[group].children.iter().copied().find(|&x| s(h, x, "id") == *char_id);
    let Some(source) = source else {
        cache.insert(char_id.clone(), char_id.clone());
        return Ok(char_id.clone());
    };
    if h.nodes[source].children.iter().any(|&c| h.tag(c) == "supscript") {
        cache.insert(char_id.clone(), char_id.clone());
        return Ok(char_id.clone());
    }
    let clone = h.clone_subtree(source);
    let subs: Vec<usize> = h.nodes[clone].children.iter().copied().filter(|&x| h.tag(x) == "subscript").collect();
    for x in subs {
        h.remove_child(clone, x);
    }
    let q = h.nodes[source].qname.clone();
    let sup_q = match q.rsplit_once(':') {
        Some((pre, _)) => format!("{pre}:supscript"),
        None => "supscript".to_string(),
    };
    let sup = h.new_elem(&sup_q, vec![]);
    h.append_child(clone, sup);
    let new_id = next_id(h, group)?;
    h.set_attr(clone, "id", &new_id);
    h.append_child(group, clone);
    let n = h.nodes[group].children.len().to_string();
    h.set_attr(group, "itemCnt", &n);
    cache.insert(char_id.clone(), Some(new_id.clone()));
    Ok(Some(new_id))
}

fn run_texts_joined(d: &Doc, p: usize) -> String {
    let mut out = String::new();
    for &r in &d.nodes[p].children {
        if d.tag(r) == "run" {
            out.push_str(&d.jstr_walk(r));
        }
    }
    out
}

/// 처리: selections = {구역: [(문단 번호, 개수)]}. 바꾼 글 조각 수.
pub fn asterisk_apply(d: &mut Doc, h: &mut Doc, items: &[(usize, usize)], sup: &HashSet<Key>, cache: &mut HashMap<Key, Key>) -> R<usize> {
    let body = body_paras(d);
    let mut count = 0;
    for &(index, _) in items {
        let p = *body.get(index).ok_or("문단 번호 범위 밖")?;
        // 같은 글 요소끼리 묶는다(처음 나온 순서)
        let mut order: Vec<usize> = vec![];
        let mut by_text: HashMap<usize, Vec<Piece>> = HashMap::new();
        for pc in star_plan(d, p) {
            if !by_text.contains_key(&pc.1) {
                order.push(pc.1);
            }
            by_text.entry(pc.1).or_default().push(pc);
        }
        let before = run_texts_joined(d, p);
        for t in order {
            let pieces = &by_text[&t];
            let run = pieces[0].0;
            let rk = &d.nodes[run].children;
            if rk.len() != 1 || d.tag(rk[0]) != "t" || !d.nodes[t].children.is_empty() {
                continue; // 다른 요소가 섞인 글 묶음은 건드리지 않는다.
            }
            if sup.contains(&s(d, run, "charPrIDRef")) {
                continue;
            }
            let text: Vec<char> = ctext(d, t).chars().collect();
            let mut sorted = pieces.clone();
            sorted.sort_by_key(|x| x.2);
            let mut cuts: Vec<(usize, usize, bool)> = vec![];
            let mut position = 0;
            for &(_, _, lo, hi) in &sorted {
                cuts.push((position, lo, false));
                cuts.push((lo, hi, true));
                position = hi;
            }
            cuts.push((position, text.len(), false));
            let mut at = d.index_of(p, run).ok_or("run 위치를 찾지 못했습니다.")?;
            let run_q = d.nodes[run].qname.clone();
            let run_a = d.nodes[run].attrs.clone();
            let t_q = d.nodes[t].qname.clone();
            let t_a = d.nodes[t].attrs.clone();
            let orig_id = s(d, run, "charPrIDRef");
            d.remove_child(p, run);
            for (lo, hi, mark) in cuts.into_iter().filter(|c| c.0 < c.1) {
                let pr = d.new_elem(&run_q, run_a.clone());
                if mark {
                    let nid = super_char(h, &orig_id, cache)?;
                    match nid {
                        Some(v) => d.set_attr(pr, "charPrIDRef", &v),
                        None => return Err("charPrIDRef 값이 없어 직렬화할 수 없습니다.".into()),
                    }
                    count += 1;
                }
                let tt = d.new_elem(&t_q, t_a.clone());
                d.nodes[tt].text = Some(text[lo..hi].iter().collect());
                d.append_child(pr, tt);
                d.insert_child(p, at, pr);
                at += 1;
            }
        }
        if run_texts_joined(d, p) != before {
            return Err("별표 위첨자 적용 중 문장 글이 바뀌어 중단했습니다.".into());
        }
        remove_lineseg(d, p);
    }
    Ok(count)
}

// ============================================================ 붙임 글꼴

static RE_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\s\x1c-\x1f]*붙임").unwrap());
static RE_ITEM_PRE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t\u{a0}\u{3000}]*\d+\.").unwrap());
static RE_HEADER_PRE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t\u{a0}\u{3000}]*붙임[ \t\u{a0}\u{3000}]*[:：]?[ \t\u{a0}\u{3000}]*\d+\.").unwrap());

fn la_ok(rest: &str) -> bool {
    // (?=\s|$|「)
    match rest.chars().next() {
        None => true,
        Some(c) => py_ws(c) || c == '「',
    }
}

fn is_numbered_item(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    if let Some(m) = RE_ITEM_PRE.find(text) {
        if la_ok(&text[m.end()..]) {
            return true;
        }
    }
    if let Some(m) = RE_HEADER_PRE.find(text) {
        let rest = &text[m.end()..];
        // (?=\s|$|「).*$ : '.' 은 줄바꿈을 못 먹고 $ 는 끝 또는 끝 줄바꿈 앞
        let body = rest.strip_suffix('\n').unwrap_or(rest);
        if la_ok(rest) && !body.contains('\n') {
            return true;
        }
    }
    false
}

fn is_start(t: &Option<String>) -> bool {
    matches!(t, Some(x) if !x.is_empty() && RE_START.is_match(x))
}
fn is_end(t: &str) -> bool {
    py_rstrip(t).ends_with("끝.")
}
fn py_rstrip(t: &str) -> &str {
    t.trim_end_matches(py_ws)
}

/// `find_blocks`: 붙임 묶음의 (시작, 끝)(끝 포함)
pub fn find_blocks(texts: &[Option<String>]) -> Vec<(usize, usize)> {
    let mut blocks = vec![];
    let mut i = 0;
    while i < texts.len() {
        if !is_start(&texts[i]) {
            i += 1;
            continue;
        }
        let mut end = None;
        for j in i..texts.len().min(i + 30) {
            let Some(text) = &texts[j] else { break };
            if text.is_empty() || py_strip(text).is_empty() || !(py_rstrip(text).ends_with('.') || is_numbered_item(text)) {
                break;
            }
            if is_end(text) {
                end = Some(j);
                break;
            }
        }
        match end {
            None => i += 1,
            Some(e) => {
                blocks.push((i, e));
                i = e + 1;
            }
        }
    }
    blocks
}

/// 글꼴 이름 표 {언어(소문자): {id: 이름}}. 같은 언어의 fontface 가 여럿이면 뒤의 것이 앞의 것을 덮는다(dict 컴프리헨션).
fn face_table(h: &Doc) -> HashMap<String, HashMap<Key, Key>> {
    let mut m = HashMap::new();
    for ff in all_by_tag(h, "fontface") {
        let lang = h.attr(ff, "lang").unwrap_or("").to_lowercase();
        let mut t = HashMap::new();
        for &f in &h.nodes[ff].children {
            t.insert(s(h, f, "id"), s(h, f, "face"));
        }
        m.insert(lang, t);
    }
    m
}

fn char_map(h: &Doc) -> HashMap<Key, usize> {
    let mut m = HashMap::new();
    for c in all_by_tag(h, "charPr") {
        m.insert(s(h, c, "id"), c);
    }
    m
}

/// (언어별 글꼴 이름 집합, 크기 ×100 정수)
fn char_face_size(h: &Doc, faces: &HashMap<String, HashMap<Key, Key>>, ch: usize) -> R<(std::collections::BTreeSet<Key>, i64)> {
    let mut names = std::collections::BTreeSet::new();
    if let Some(r) = h.nodes[ch].children.iter().copied().find(|&x| h.tag(x) == "fontRef") {
        for (lang, fid) in &h.nodes[r].attrs {
            let n = faces.get(lang.as_str()).and_then(|t| t.get(&Some(fid.clone()))).cloned().flatten();
            names.insert(n);
        }
    }
    Ok((names, py_int_pub(h.attr(ch, "height").unwrap_or("0"))?))
}

#[derive(Clone, Debug)]
pub struct Goal {
    face: Key,
    size: f64,
}

/// `_본문기호_글꼴`
fn goal_font(h: &Doc, paras: &[(usize, usize)], docs: &[&Doc], cfg: &Config) -> R<Option<Goal>> {
    if cfg.std_symbols {
        for (m, face, size) in &cfg.symbol_rules {
            if m == "ㅇ" {
                return Ok(Some(Goal { face: face.clone(), size: size.unwrap() }));
            }
        }
    }
    let faces = face_table(h);
    let chars = char_map(h);
    let mut tally: Vec<((Key, i64), usize)> = vec![];
    for &(di, p) in paras {
        let d = docs[di];
        let Some(texts) = general_texts(d, p) else { continue };
        if texts.is_empty() {
            continue;
        }
        let joined: String = texts.iter().map(|&(_, t)| ctext(d, t)).collect();
        if leading_marker(&joined).0 != "ㅇ" {
            continue;
        }
        for &run in &d.nodes[p].children {
            if d.tag(run) != "run" {
                continue;
            }
            let length: usize = d.nodes[run].children.iter().filter(|&&t| d.tag(t) == "t").map(|&t| clen(&ctext(d, t))).sum();
            let ch = chars.get(&s(d, run, "charPrIDRef")).copied();
            if length > 0 {
                if let Some(ch) = ch {
                    let size = py_int_pub(h.attr(ch, "height").unwrap_or("0"))?;
                    let r = h.nodes[ch].children.iter().copied().find(|&x| h.tag(x) == "fontRef").ok_or("fontRef 가 없습니다.")?;
                    let face = faces.get("hangul").and_then(|t| t.get(&s(h, r, "hangul"))).cloned().flatten();
                    let key = (face, size);
                    match tally.iter_mut().find(|(k, _)| *k == key) {
                        Some(e) => e.1 += length,
                        None => tally.push((key, length)),
                    }
                }
            }
        }
    }
    let mut best: Option<&((Key, i64), usize)> = None;
    for e in &tally {
        if best.map_or(true, |b| e.1 > b.1) {
            best = Some(e);
        }
    }
    Ok(best.map(|((f, sz), _)| Goal { face: f.clone(), size: *sz as f64 / 100.0 }))
}

/// 탭만 있는 글 요소를 문자로 펼친 문단 글. 표·필드 등은 None. (`paragraph_text`)
fn paragraph_text(d: &Doc, p: usize) -> Option<String> {
    for i in d.walk(p) {
        if d.tag(i) == "t" && !d.nodes[i].children.is_empty() && d.nodes[i].children.iter().any(|&c| d.tag(c) != "tab") {
            return None;
        }
    }
    let texts = general_texts_opt(d, p, true)?;
    let mut out = String::new();
    for (_, t) in texts {
        out.push_str(&ctext(d, t));
        for &c in &d.nodes[t].children {
            out.push('\t');
            out.push_str(d.nodes[c].tail.as_deref().unwrap_or(""));
        }
    }
    Some(out)
}

fn runs_with_text(d: &Doc, p: usize) -> Vec<usize> {
    d.nodes[p]
        .children
        .iter()
        .copied()
        .filter(|&r| d.tag(r) == "run" && d.nodes[r].children.iter().any(|&t| d.tag(t) == "t" && !ctext(d, t).is_empty()))
        .collect()
}

pub struct AttachCtx {
    goal: Option<Goal>,
    faces: HashMap<String, HashMap<Key, Key>>,
    chars: HashMap<Key, usize>,
    paras_by_id: HashMap<Key, usize>,
    para_group: Option<usize>,
    para_cache: HashMap<Key, Key>,
}

fn rstrip_st(x: &str) -> &str {
    x.trim_end_matches([' ', '\t'])
}

impl AttachCtx {
    pub fn new(h: &Doc, docs: &[&Doc], cfg: &Config) -> R<AttachCtx> {
        let mut paras: Vec<(usize, usize)> = vec![];
        for (di, d) in docs.iter().enumerate() {
            for p in body_paras(d) {
                paras.push((di, p));
            }
        }
        let goal = goal_font(h, &paras, docs, cfg)?;
        let mut paras_by_id = HashMap::new();
        for x in all_by_tag(h, "paraPr") {
            paras_by_id.insert(s(h, x, "id"), x);
        }
        Ok(AttachCtx {
            goal,
            faces: face_table(h),
            chars: char_map(h),
            paras_by_id,
            para_group: first_by_tag(h, "paraProperties"),
            para_cache: HashMap::new(),
        })
    }

    fn needs_layout_change(&self, h: &Doc, d: &Doc, p: usize) -> bool {
        let style = self.paras_by_id.get(&s(d, p, "paraPrIDRef")).copied();
        let align = style.and_then(|st| h.nodes[st].children.iter().copied().find(|&x| h.tag(x) == "align"));
        let text = paragraph_text(d, p);
        let a = align.map_or(false, |a| h.attr(a, "horizontal") != Some("LEFT"));
        let b = text.as_deref().map_or(false, |t| t != rstrip_st(t));
        a || b
    }

    fn needs_change(&self, h: &Doc, d: &Doc, p: usize) -> R<bool> {
        let Some(goal) = &self.goal else { return Ok(false) };
        for run in runs_with_text(d, p) {
            if let Some(&ch) = self.chars.get(&s(d, run, "charPrIDRef")) {
                let (names, size) = char_face_size(h, &self.faces, ch)?;
                let want: std::collections::BTreeSet<Key> = std::iter::once(goal.face.clone()).collect();
                if names != want || (size as f64 / 100.0 - goal.size).abs() > 0.001 {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    pub fn survey(&self, h: &Doc, d: &Doc) -> R<Vec<(usize, usize)>> {
        let paras = body_paras(d);
        let texts: Vec<Option<String>> = paras.iter().map(|&p| paragraph_text(d, p)).collect();
        let mut out = vec![];
        for (st, en) in find_blocks(&texts) {
            let mut any = false;
            for &p in &paras[st..=en] {
                if self.needs_change(h, d, p)? || self.needs_layout_change(h, d, p) {
                    any = true;
                    break;
                }
            }
            if any {
                out.push((st, en));
            }
        }
        Ok(out)
    }
}

/// `_글꼴_확보`: 모든 언어 글꼴 그룹에 face 가 있게 하고 {언어(소문자): ID}를 돌려준다.
fn ensure_font(h: &mut Doc, face: &Key, cache: &mut HashMap<Key, HashMap<String, Key>>) -> R<HashMap<String, Key>> {
    if let Some(v) = cache.get(face) {
        return Ok(v.clone());
    }
    let mut result = HashMap::new();
    for ff in all_by_tag(h, "fontface") {
        let mut found = h.nodes[ff].children.iter().copied().find(|&f| s(h, f, "face") == *face);
        if found.is_none() {
            let first = *h.nodes[ff].children.first().ok_or("fontface 가 비어 있습니다.")?;
            let nf = h.clone_subtree(first);
            match face {
                Some(v) => h.set_attr(nf, "face", v),
                None => return Err("글꼴 이름이 없어 직렬화할 수 없습니다.".into()),
            }
            let nid = next_id(h, ff)?;
            h.set_attr(nf, "id", &nid);
            h.append_child(ff, nf);
            let n = h.nodes[ff].children.len().to_string();
            h.set_attr(ff, "fontCnt", &n);
            found = Some(nf);
        }
        let lang = h.attr(ff, "lang").unwrap_or("").to_lowercase();
        result.insert(lang, s(h, found.unwrap(), "id"));
    }
    cache.insert(face.clone(), result.clone());
    Ok(result)
}

pub struct AttachState {
    font_cache: HashMap<Key, HashMap<String, Key>>,
    char_cache: HashMap<Key, Key>,
}

impl AttachState {
    pub fn new() -> Self {
        AttachState { font_cache: HashMap::new(), char_cache: HashMap::new() }
    }
}

fn apply_layout(ctx: &mut AttachCtx, h: &mut Doc, d: &mut Doc, p: usize) -> R<()> {
    let old = s(d, p, "paraPrIDRef");
    let style = ctx.paras_by_id.get(&old).copied();
    if let (Some(style), Some(pg)) = (style, ctx.para_group) {
        let align = h.nodes[style].children.iter().copied().find(|&x| h.tag(x) == "align");
        if let Some(a) = align {
            if h.attr(a, "horizontal") != Some("LEFT") {
                if !ctx.para_cache.contains_key(&old) {
                    let clone = h.clone_subtree(style);
                    let mut m: i64 = -1;
                    for k in ctx.paras_by_id.keys() {
                        if is_digit_id(k.as_deref()) {
                            m = m.max(k.as_ref().unwrap().parse::<i64>().map_err(|e| e.to_string())?);
                        }
                    }
                    let new_id = (m + 1).to_string();
                    h.set_attr(clone, "id", &new_id);
                    let al = h.nodes[clone].children.iter().copied().find(|&x| h.tag(x) == "align").ok_or("align 이 없습니다.")?;
                    h.set_attr(al, "horizontal", "LEFT");
                    h.append_child(pg, clone);
                    let n = h.nodes[pg].children.len().to_string();
                    h.set_attr(pg, "itemCnt", &n);
                    ctx.paras_by_id.insert(Some(new_id.clone()), clone);
                    ctx.para_cache.insert(old.clone(), Some(new_id));
                }
                match ctx.para_cache.get(&old).cloned().flatten() {
                    Some(v) => d.set_attr(p, "paraPrIDRef", &v),
                    None => return Err("paraPrIDRef 값이 없어 직렬화할 수 없습니다.".into()),
                }
            }
        }
    }
    // 여러 run 에 걸친 끝 공백만 제거한다.
    let mut ts: Vec<usize> = vec![];
    for &r in &d.nodes[p].children {
        for &x in &d.nodes[r].children {
            if d.tag(x) == "t" {
                ts.push(x);
            }
        }
    }
    for &t in ts.iter().rev() {
        let kids: Vec<usize> = d.nodes[t].children.clone();
        for &c in kids.iter().rev() {
            let tail = rstrip_st(d.nodes[c].tail.as_deref().unwrap_or("")).to_string();
            let keep = !tail.is_empty() || d.tag(c) != "tab";
            d.nodes[c].tail = Some(tail);
            if keep {
                return Ok(());
            }
            d.remove_child(t, c);
        }
        let txt = rstrip_st(d.nodes[t].text.as_deref().unwrap_or("")).to_string();
        let nonempty = !txt.is_empty();
        d.nodes[t].text = Some(txt);
        if nonempty {
            break;
        }
    }
    Ok(())
}

/// 선택된 묶음 (시작, 끝) 목록 적용. 바꾼 문단 수.
pub fn attach_apply(ctx: &mut AttachCtx, st: &mut AttachState, h: &mut Doc, d: &mut Doc, items: &[(usize, usize)]) -> R<usize> {
    let body = body_paras(d);
    let group = first_by_tag(h, "charProperties").ok_or("charProperties 가 없습니다.")?;
    let mut count = 0;
    for &(start, end) in items {
        let hi = (end + 1).min(body.len());
        let lo = start.min(hi);
        for &p in &body[lo..hi] {
            let before = d.jstr(p);
            let runs = if ctx.goal.is_some() { runs_with_text(d, p) } else { vec![] };
            for run in runs {
                let goal = ctx.goal.clone().unwrap();
                let old = s(d, run, "charPrIDRef");
                if !st.char_cache.contains_key(&old) {
                    let source_char = ctx.chars.get(&old).copied();
                    let same = match source_char {
                        None => true,
                        Some(sc) => {
                            let (names, size) = char_face_size(h, &ctx.faces, sc)?;
                            let want: std::collections::BTreeSet<Key> = std::iter::once(goal.face.clone()).collect();
                            names == want && (size as f64 / 100.0 - goal.size).abs() < 0.001
                        }
                    };
                    if same {
                        st.char_cache.insert(old.clone(), old.clone());
                    } else {
                        let sc = source_char.unwrap();
                        let clone = h.clone_subtree(sc);
                        h.set_attr(clone, "height", &py_round(goal.size * 100.0).to_string());
                        let ids = ensure_font(h, &goal.face, &mut st.font_cache)?;
                        let r = h.nodes[clone].children.iter().copied().find(|&x| h.tag(x) == "fontRef").ok_or("fontRef 가 없습니다.")?;
                        let langs: Vec<String> = h.nodes[r].attrs.iter().map(|(k, _)| k.clone()).collect();
                        for lang in langs {
                            if let Some(v) = ids.get(&lang.to_lowercase()) {
                                match v {
                                    Some(v) => h.set_attr(r, &lang, v),
                                    None => return Err("글꼴 id 가 없어 직렬화할 수 없습니다.".into()),
                                }
                            }
                        }
                        let new_id = next_id(h, group)?;
                        h.set_attr(clone, "id", &new_id);
                        h.append_child(group, clone);
                        let n = h.nodes[group].children.len().to_string();
                        h.set_attr(group, "itemCnt", &n);
                        ctx.chars.insert(Some(new_id.clone()), clone);
                        ctx.faces = face_table(h);
                        st.char_cache.insert(old.clone(), Some(new_id));
                    }
                }
                match st.char_cache.get(&old).cloned().flatten() {
                    Some(v) => d.set_attr(run, "charPrIDRef", &v),
                    None => return Err("charPrIDRef 값이 없어 직렬화할 수 없습니다.".into()),
                }
            }
            if d.jstr(p) != before {
                return Err("붙임 글꼴 통일 중 문장 글이 바뀌어 중단했습니다.".into());
            }
            apply_layout(ctx, h, d, p)?;
            remove_lineseg(d, p);
            count += 1;
        }
    }
    Ok(count)
}

// ============================================================ 콜론 라벨 정렬

static RE_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([\s\x1c-\x1f]*(?:(?:\*{1,2}|[-‐–□■ㅇ○◦●•·∙※▪▶◆◇☞])|\d{1,2}[.)]|[가-하][.)]|\(\d{1,2}\)|[①-⑳])[\s\x1c-\x1f]*)([^:：\s\x1c-\x1f][^:：]{0,7}?)[\s\x1c-\x1f]*([:：])").unwrap()
});
static RE_LONG_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\s\x1c-\x1f]*(?:(?:\*{1,2}|[-‐–□■ㅇ○◦●•·∙※▪▶◆◇☞])|\d{1,2}[.)]|[가-하][.)]|\(\d{1,2}\)|[①-⑳])[\s\x1c-\x1f]*[^:：\s\x1c-\x1f][^:：]{6,19}[:：]").unwrap()
});
const MAX_LABEL: usize = 7;
const MAX_SPACING: i64 = 50;
const MAX_RATIO: i64 = 200;
const SPACE_EM: f64 = 0.5;

fn cp(text: &str, byte: usize) -> usize {
    text[..byte].chars().count()
}

/// `label_of`: (라벨 시작, 라벨 끝, 콜론 위치)(문자 수). 라벨은 공백 제외 7자 미만.
pub fn label_of(text: &str) -> Option<(usize, usize, usize)> {
    let c = RE_LABEL.captures(text)?;
    let g2 = c.get(2).unwrap();
    let g3 = c.get(3).unwrap();
    let label = g2.as_str();
    if clen(&label.replace(' ', "")) >= MAX_LABEL || py_strip(label).is_empty() {
        return None;
    }
    let st = cp(text, g2.start());
    Some((st, st + clen(py_rstrip(label)), cp(text, g3.start())))
}

fn sum_em(cs: &[char]) -> f64 {
    let mut t = 0.0;
    for &c in cs {
        t += char_em(c);
    }
    t
}

/// `stretch`: (자간 %, 장평 %)
pub fn stretch(label: &[char], target_em: f64, spaced_base: Option<f64>, allow_ratio: bool) -> (i64, i64) {
    let base = sum_em(label);
    let inner = if label.is_empty() { 0.0 } else { sum_em(&label[..label.len() - 1]) };
    let current = spaced_base.unwrap_or(base);
    if label.is_empty() || current >= target_em - 1e-6 {
        return (0, 100);
    }
    let gap = target_em - current;
    if inner > 0.0 && gap <= MAX_SPACING as f64 / 100.0 * inner {
        return (py_round(gap / inner * 100.0), 100);
    }
    if !allow_ratio {
        return (if inner > 0.0 { MAX_SPACING } else { 0 }, 100);
    }
    let spacing = if inner > 0.0 { MAX_SPACING } else { 0 };
    let fixed = current - base;
    (spacing, MAX_RATIO.min(py_round((target_em - fixed) / (base + spacing as f64 / 100.0 * inner) * 100.0)))
}

/// `plan`: (글자 사이 빈칸 수, 자간 %, 장평 %)
pub fn plan(label: &[char], target_em: f64, allow_ratio: bool) -> (usize, i64, i64) {
    let base = sum_em(label);
    if label.is_empty() || base >= target_em - 1e-6 {
        return (0, 0, 100);
    }
    let n = label.len() as i64;
    let gaps = n - 1;
    let target_n = py_round(target_em);
    let extra = target_em - base;
    let mut spaces: i64 = 0;
    let inner = sum_em(&label[..label.len() - 1]);
    if gaps > 0 && !((n % 2 != target_n % 2) && extra <= MAX_SPACING as f64 / 100.0 * inner) {
        spaces = 1.max((extra / (gaps as f64 * SPACE_EM)) as i64);
        while spaces > 1 && spaces as f64 * gaps as f64 * SPACE_EM > extra - 1e-6 {
            spaces -= 1;
        }
    }
    let mut spaced = base + spaces as f64 * gaps as f64 * SPACE_EM;
    if !allow_ratio && gaps > 0 {
        while target_em - spaced > MAX_SPACING as f64 / 100.0 * inner + 1e-6 {
            spaces += 1;
            spaced = base + spaces as f64 * gaps as f64 * SPACE_EM;
        }
    }
    let (spacing, ratio) = stretch(label, target_em, Some(spaced), allow_ratio);
    (spaces as usize, spacing, ratio)
}

/// `_simple_runs`: 모든 run 이 글 요소 하나(안에 다른 요소 없음)만 가질 때 [(run, 글)], 아니면 None.
fn simple_runs_colon(d: &Doc, p: usize) -> Option<Vec<(usize, String)>> {
    let mut out = vec![];
    for &run in &d.nodes[p].children {
        if d.tag(run) != "run" {
            continue;
        }
        let k = &d.nodes[run].children;
        if k.is_empty() {
            continue;
        }
        if k.iter().any(|&x| d.tag(x) != "t") || k.len() != 1 || !d.nodes[k[0]].children.is_empty() {
            return None;
        }
        out.push((run, ctext(d, k[0])));
    }
    if out.iter().any(|(_, t)| !t.is_empty()) {
        Some(out)
    } else {
        None
    }
}

pub struct ColonGroup {
    items: Vec<(usize, String, (usize, usize, usize))>, // (문단, 글, 라벨 위치)
}

/// `groups_in(container)`: 같은 부모 안에서 '항목기호 + 짧은 라벨 + 콜론' 문단이 2개 이상 이어진 묶음.
pub fn groups_in(d: &Doc, container: usize) -> Vec<ColonGroup> {
    let mut out = vec![];
    let mut run: Vec<(usize, String, (usize, usize, usize))> = vec![];
    for &p in &d.nodes[container].children {
        if d.tag(p) != "p" {
            continue;
        }
        let text = simple_runs_colon(d, p).map(|rs| rs.into_iter().map(|(_, t)| t).collect::<String>());
        let found = text.as_deref().and_then(label_of);
        let has_tbl = d.iter(p).any(|i| d.tag(i) == "tbl");
        if let (Some(f), Some(t)) = (found, &text) {
            if !has_tbl {
                run.push((p, t.clone(), f));
                continue;
            }
        }
        if !run.is_empty() && text.as_deref().map_or(false, |t| RE_LONG_LABEL.is_match(t)) {
            continue;
        }
        if run.len() >= 2 {
            out.push(ColonGroup { items: std::mem::take(&mut run) });
        }
        run = vec![];
    }
    if run.len() >= 2 {
        out.push(ColonGroup { items: run });
    }
    out
}

/// 구역의 콜론 묶음 수(조사). 컨테이너 = 뿌리 + 모든 subList
pub fn colon_containers(d: &Doc) -> Vec<usize> {
    let mut v = vec![0];
    v.extend(d.iter(0).filter(|&i| d.tag(i) == "subList"));
    v
}

pub fn colon_survey(d: &Doc) -> Vec<usize> {
    let n: usize = colon_containers(d).into_iter().map(|c| groups_in(d, c).len()).sum();
    (0..n).collect()
}

pub struct ColonChars {
    group: Option<usize>,
    chars: HashMap<Key, usize>,
    made: HashMap<(Key, i64, i64), Key>,
}

impl ColonChars {
    pub fn new(h: &Doc) -> Self {
        ColonChars { group: first_by_tag(h, "charProperties"), chars: char_map(h), made: HashMap::new() }
    }

    fn stretched(&mut self, h: &mut Doc, cid: &Key, spacing: i64, ratio: i64) -> R<Key> {
        let key = (cid.clone(), spacing, ratio);
        if let Some(v) = self.made.get(&key) {
            return Ok(v.clone());
        }
        let (Some(src), Some(group)) = (self.chars.get(cid).copied(), self.group) else { return Ok(cid.clone()) };
        let clone = h.clone_subtree(src);
        let new_id = (1 + {
            let mut m: i64 = -1;
            for &x in &h.nodes[group].children {
                if is_digit_id(h.attr(x, "id")) {
                    m = m.max(h.attr(x, "id").unwrap().parse::<i64>().map_err(|e| e.to_string())?);
                }
            }
            m
        })
        .to_string();
        h.set_attr(clone, "id", &new_id);
        let kids = h.nodes[clone].children.clone();
        for ch in kids {
            let tag = h.tag(ch).to_string();
            if tag == "spacing" {
                let attrs: Vec<(String, String)> = h.nodes[ch].attrs.clone();
                for (lang, v) in attrs {
                    let n = py_int_pub(&v)?;
                    h.set_attr(ch, &lang, &(-50i64).max(MAX_SPACING.min(n + spacing)).to_string());
                }
            } else if tag == "ratio" {
                let attrs: Vec<(String, String)> = h.nodes[ch].attrs.clone();
                for (lang, v) in attrs {
                    let n = py_int_pub(&v)?;
                    let r = py_round((n * ratio) as f64 / 100.0);
                    h.set_attr(ch, &lang, &50i64.max(MAX_RATIO.min(r)).to_string());
                }
            }
        }
        h.append_child(group, clone);
        let n = h.nodes[group].children.len().to_string();
        h.set_attr(group, "itemCnt", &n);
        self.chars.insert(Some(new_id.clone()), clone);
        self.made.insert(key, Some(new_id.clone()));
        Ok(Some(new_id))
    }
}

/// `align_colons` 한 컨테이너 몫. 늘린 문단 수와 묶음 수를 (stretched, groups)에 더한다.
pub fn colon_apply_group(d: &mut Doc, h: &mut Doc, ch: &mut ColonChars, g: &ColonGroup, allow_ratio: bool, stats: &mut (usize, usize)) -> R<()> {
    let target = g
        .items
        .iter()
        .map(|(_, t, (s0, e0, _))| {
            let cs: Vec<char> = t.chars().collect();
            sum_em(&cs[*s0..*e0])
        })
        .fold(f64::NEG_INFINITY, f64::max);
    let mut done = false;
    for (p, t, (st, en, _)) in &g.items {
        let cs: Vec<char> = t.chars().collect();
        let label = &cs[*st..*en];
        let (spaces, spacing, ratio) = plan(label, target, allow_ratio);
        if (spaces, spacing, ratio) == (0, 0, 100) {
            continue;
        }
        let Some(runs) = simple_runs_colon(d, *p) else { continue };
        let gap: String = " ".repeat(spaces);
        let mut letters: Vec<(char, Key)> = vec![];
        for (run, text) in &runs {
            let id = s(d, *run, "charPrIDRef");
            for c in text.chars() {
                letters.push((c, id.clone()));
            }
        }
        let mut pieces: Vec<(String, Key)> = vec![];
        for (i, (c, cid)) in letters.iter().enumerate() {
            if *st <= i && i + 1 < *en {
                pieces.push((c.to_string(), ch.stretched(h, cid, spacing, ratio)?));
                if !gap.is_empty() {
                    pieces.push((gap.clone(), cid.clone()));
                }
            } else if i + 1 == *en {
                pieces.push((c.to_string(), ch.stretched(h, cid, 0, ratio)?));
            } else {
                pieces.push((c.to_string(), cid.clone()));
            }
        }
        let mut merged: Vec<(String, Key)> = vec![];
        for (piece, cid) in pieces {
            if let Some(last) = merged.last_mut() {
                if last.1 == cid {
                    last.0.push_str(&piece);
                    continue;
                }
            }
            merged.push((piece, cid));
        }
        let template = runs[0].0;
        let index = d.index_of(*p, template).ok_or("run 위치를 찾지 못했습니다.")?;
        for (old, _) in &runs {
            d.remove_child(*p, *old);
        }
        for (offset, (piece, cid)) in merged.into_iter().enumerate() {
            let nr = d.clone_subtree(template);
            let t0 = d.nodes[nr].children[0];
            d.nodes[t0].text = Some(piece);
            match cid {
                Some(v) => d.set_attr(nr, "charPrIDRef", &v),
                None => return Err("charPrIDRef 값이 없어 직렬화할 수 없습니다.".into()),
            }
            d.insert_child(*p, index + offset, nr);
        }
        remove_lineseg(d, *p);
        stats.0 += 1;
        done = true;
    }
    if done {
        stats.1 += 1;
    }
    Ok(())
}

pub fn selections_value(items: Vec<(String, Vec<Value>)>) -> Value {
    let mut m = serde_json::Map::new();
    for (n, v) in items {
        m.insert(n, json!(v));
    }
    Value::Object(m)
}
