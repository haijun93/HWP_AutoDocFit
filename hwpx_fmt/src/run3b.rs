//! RSXML3: 빈 줄 서식(`blank_lines`)·강조 낱말(`emphasis`).
//! 파이썬 근거: `formatter_apply.py 빈줄서식_hwpx_처리·강조낱말_hwpx_처리`, `docfit_core/blank_lines.py`·`emphasis.py`.
//! 두 처리기는 서식 프로필(설정 `blank_line_profile`·`emphasis_words`)을 입력으로 받는다. 프로필의 XML 조각(문단 모양·글자 모양·
//! 글꼴·테두리·탭)은 문서의 머리에 복제하며, 이름공간 접두사는 대상 문서의 것으로 바꾼다. 문서에 없는 이름공간이 들어 있으면
//! '미지원 입력'으로 거부한다.
use crate::config::{unsupported, Config};
use crate::dom::{self, Doc};
use crate::fmt3::{body_paras, ctext, first_by_tag, Key};
use crate::rules::{clen, format_kind, py_int_pub, py_strip, py_ws, FormatKind, R};
use crate::style::unify_marker;
use crate::zipio::{self, Contents};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

type Sections = Vec<(String, usize, Doc)>;

fn key(d: &Doc, i: usize, k: &str) -> Key {
    d.attr(i, k).map(|x| x.to_string())
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map_or(true, |f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

fn get<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    v.as_object().and_then(|o| o.get(k))
}

// ---- XML 조각(프로필) ----------------------------------------------------

/// 대상 문서 뿌리의 xmlns 선언 → {이름공간 URI: 접두사}(같은 URI 는 뒤의 선언이 이긴다)
pub(crate) fn uri_prefixes(target: &Doc) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for (k, v) in &target.nodes[0].attrs {
        if let Some(p) = k.strip_prefix("xmlns:") {
            m.insert(v.clone(), p.to_string());
        }
    }
    m
}

/// 프로필의 XML 조각을 읽어 접두사를 대상 문서의 것으로 바꾼다(파이썬은 Clark 이름을 직렬화기가 등록 접두사로 쓴다).
pub(crate) fn parse_fragment(xml: &str, target: &Doc) -> R<Doc> {
    let mut f = dom::parse(xml.as_bytes()).map_err(|e| format!("프로필 XML 조각 오류: {e}"))?;
    let want = uri_prefixes(target);
    // 선언 범위(선행 순회)
    let mut scopes: Vec<(usize, HashMap<String, String>)> = vec![];
    for i in 0..f.nodes.len() {
        while scopes.last().map_or(false, |(end, _)| *end <= i) {
            scopes.pop();
        }
        let mut decl: HashMap<String, String> = scopes.last().map(|(_, m)| m.clone()).unwrap_or_default();
        for (k, v) in &f.nodes[i].attrs {
            if k == "xmlns" {
                decl.insert(String::new(), v.clone());
            } else if let Some(p) = k.strip_prefix("xmlns:") {
                decl.insert(p.to_string(), v.clone());
            }
        }
        scopes.push((f.nodes[i].end, decl.clone()));
        let resolve = |qname: &str, is_attr: bool| -> R<String> {
            let (pre, local) = match qname.split_once(':') {
                Some((p, l)) => (p, l),
                None => {
                    if is_attr || !decl.contains_key("") {
                        return Ok(qname.to_string());
                    }
                    ("", qname)
                }
            };
            let uri = decl.get(pre).ok_or_else(|| format!("프로필 XML 조각의 접두사 {pre:?} 선언이 없습니다."))?;
            match want.get(uri) {
                Some(p) => Ok(format!("{p}:{local}")),
                None => unsupported(&format!("프로필 XML 조각의 이름공간 {uri} 가 대상 문서에 없습니다.")),
            }
        };
        let q = f.nodes[i].qname.clone();
        let nq = resolve(&q, false)?;
        let mut attrs = vec![];
        for (k, v) in f.nodes[i].attrs.clone() {
            if k == "xmlns" || k.starts_with("xmlns:") {
                continue;
            }
            attrs.push((resolve(&k, true)?, v));
        }
        f.nodes[i].local = nq.rsplit(':').next().unwrap_or(&nq).to_string();
        f.nodes[i].qname = nq;
        f.nodes[i].attrs = attrs;
    }
    Ok(f)
}

/// `_clone(template, xml)` / `_clone_like`: 프로필 XML 을 대상 문서의 새 요소로 만든다.
fn clone_from_xml(d: &mut Doc, xml: &Value) -> R<usize> {
    let s = xml.as_str().ok_or("프로필 XML 이 문자열이 아닙니다.")?;
    let f = parse_fragment(s, d)?;
    Ok(d.graft(&f))
}

fn append_with_id(d: &mut Doc, group: usize, el: usize) -> R<String> {
    // 파이썬 _append: 모든 자식의 id 를 int 로 읽는다(숫자가 아니면 중단).
    let mut m: i64 = -1;
    for &x in &d.nodes[group].children {
        let v = d.attr(x, "id").ok_or("id 가 없는 항목이 있어 새 id 를 정할 수 없습니다.")?;
        m = m.max(py_int_pub(v)?);
    }
    let id = (m + 1).to_string();
    d.set_attr(el, "id", &id);
    d.append_child(group, el);
    let n = d.nodes[group].children.len().to_string();
    d.set_attr(group, "itemCnt", &n);
    Ok(id)
}

fn by_id(d: &Doc, group: usize, ident: &Key) -> Option<usize> {
    d.nodes[group].children.iter().copied().find(|&x| key(d, x, "id") == *ident)
}

// ---- 빈 줄 서식 ----------------------------------------------------------

const OBJECTS: [&str; 13] = ["tbl", "pic", "rect", "ellipse", "line", "container", "equation", "ole", "polygon", "curve", "arc", "textart", "connectLine"];

fn itertext(d: &Doc, e: usize, out: &mut String) {
    if let Some(t) = &d.nodes[e].text {
        out.push_str(t);
    }
    for &c in &d.nodes[e].children {
        itertext(d, c, out);
        if let Some(t) = &d.nodes[c].tail {
            out.push_str(t);
        }
    }
}

fn runs(d: &Doc, p: usize) -> Vec<usize> {
    d.nodes[p].children.iter().copied().filter(|&x| d.tag(x) == "run").collect()
}

fn bl_text(d: &Doc, p: usize) -> String {
    let mut s = String::new();
    for r in runs(d, p) {
        for &t in &d.nodes[r].children {
            if d.tag(t) == "t" {
                itertext(d, t, &mut s);
            }
        }
    }
    s
}

fn objects(d: &Doc, p: usize) -> Vec<usize> {
    let mut v = vec![];
    for r in runs(d, p) {
        for &x in &d.nodes[r].children {
            if OBJECTS.contains(&d.tag(x)) {
                v.push(x);
            }
        }
    }
    v
}

fn is_blank(d: &Doc, p: usize) -> bool {
    py_strip(&bl_text(d, p)).is_empty() && objects(d, p).is_empty()
}

/// 서식표_종류판별 의 숫자를 뗀 값(없으면 "table")
fn table_kind(d: &Doc, t: usize) -> R<&'static str> {
    Ok(match format_kind(d, t)? {
        Some(FormatKind::Title) => "title",
        Some(FormatKind::Mid) => "midtitle",
        Some(FormatKind::Attach) => "attach",
        Some(FormatKind::Box) => "box",
        None => "table",
    })
}

fn role(d: &Doc, p: usize) -> R<String> {
    let objs = objects(d, p);
    if let Some(&first) = objs.first() {
        if d.tag(first) != "tbl" {
            return Ok("object".into());
        }
        return Ok(table_kind(d, first)?.to_string());
    }
    let g = unify_marker(py_strip(&bl_text(d, p)));
    Ok(if g.0.is_empty() { "text".into() } else { g.0 })
}

/// `slots`: 구역 바로 아래 빈 줄마다 (문단, 'prev|next|k'), 표만 담은 문단마다 (문단, 'host|역할')
fn slots(d: &Doc) -> R<Vec<(usize, String)>> {
    let paras = body_paras(d);
    let mut roles: Vec<Option<String>> = vec![];
    for &p in &paras {
        roles.push(if is_blank(d, p) { None } else { Some(role(d, p)?) });
    }
    let mut found = vec![];
    for (index, &p) in paras.iter().enumerate() {
        if let Some(r) = &roles[index] {
            let objs = objects(d, p);
            if py_strip(&bl_text(d, p)).is_empty() && objs.len() == 1 && d.tag(objs[0]) == "tbl" {
                found.push((p, format!("host|{r}")));
            }
            continue;
        }
        let before = roles[..index].iter().rev().find_map(|r| r.clone()).unwrap_or_else(|| "start".into());
        let after = roles[index + 1..].iter().find_map(|r| r.clone()).unwrap_or_else(|| "end".into());
        let mut k = 0;
        while index > k && roles[index - k - 1].is_none() {
            k += 1;
        }
        found.push((p, format!("{before}|{after}|{k}")));
    }
    Ok(found)
}

/// `after_tables`: 중제목 표 바로 다음(빈 줄 건너뜀) 글 문단마다 (문단, 'after|역할')
fn after_tables(d: &Doc) -> R<Vec<(usize, String)>> {
    let mut found = vec![];
    let mut last: Option<String> = None;
    for p in body_paras(d) {
        if is_blank(d, p) {
            continue;
        }
        if !objects(d, p).is_empty() && py_strip(&bl_text(d, p)).is_empty() {
            last = Some(role(d, p)?);
            continue;
        }
        if last.as_deref() == Some("midtitle") {
            found.push((p, format!("after|{}", last.as_ref().unwrap())));
        }
        last = None;
    }
    Ok(found)
}

/// `_prev_value`: 문단 모양의 문단 위 간격(case 값 우선)
fn prev_value(d: &Doc, para: usize) -> R<Option<i64>> {
    let nodes = d.walk(para);
    for &c in &nodes {
        if d.tag(c) == "case" {
            for e in d.walk(c) {
                if d.tag(e) == "prev" {
                    let v = d.attr(e, "value").unwrap_or("");
                    return Ok(Some(if v.is_empty() { 0 } else { py_int_pub(v)? }));
                }
            }
        }
    }
    match nodes.iter().copied().find(|&x| d.tag(x) == "prev") {
        Some(e) => {
            let v = d.attr(e, "value").unwrap_or("");
            Ok(Some(if v.is_empty() { 0 } else { py_int_pub(v)? }))
        }
        None => Ok(None),
    }
}

fn set_prev(d: &mut Doc, para: usize, value: i64) -> R<()> {
    fn walk(d: &mut Doc, node: usize, factor: i64, value: i64) {
        for c in d.nodes[node].children.clone() {
            let name = d.tag(c).to_string();
            if name == "prev" {
                d.set_attr(c, "value", &(value * factor).to_string());
            }
            walk(d, c, if name == "default" { 2 } else { factor }, value);
        }
    }
    walk(d, para, 1, value);
    Ok(())
}

fn lookup<'a>(learned: &'a Value, k: &str) -> Option<(String, &'a Value)> {
    let slots = get(learned, "slots").and_then(|v| v.as_object())?;
    if let Some(v) = slots.get(k) {
        return Some((k.to_string(), v));
    }
    if k.starts_with("host|") {
        return None;
    }
    let prefix = format!("{}|", k.rsplit_once('|').map_or(k, |(a, _)| a));
    let mut same: Vec<&String> = slots.keys().filter(|x| x.starts_with(&prefix)).collect();
    same.sort();
    same.last().map(|x| ((*x).clone(), &slots[*x]))
}

/// 글꼴 그룹(언어별 fontface)에서 sample 과 같은 글꼴의 id, 없으면 첫 글꼴을 본뜬 사본을 추가해 그 id. (blank_lines._font_id)
fn font_id_blank(d: &mut Doc, fontfaces: usize, lang: &str, xml: &Value) -> R<Option<String>> {
    let face = d.nodes[fontfaces].children.iter().copied().find(|&x| d.attr(x, "lang").unwrap_or("").to_lowercase() == lang.to_lowercase());
    let sample = parse_fragment(xml.as_str().ok_or("프로필 XML 이 문자열이 아닙니다.")?, d)?;
    let Some(face) = face else { return Ok(None) };
    if d.nodes[face].children.is_empty() {
        return Ok(None);
    }
    let (sf, st) = (sample.attr(0, "face").map(String::from), sample.attr(0, "type").map(String::from));
    if let Some(same) = d.nodes[face].children.iter().copied().find(|&x| d.attr(x, "face").map(String::from) == sf && d.attr(x, "type").map(String::from) == st) {
        return Ok(d.attr(same, "id").map(String::from));
    }
    let clone = d.graft(&sample);
    let mut m: i64 = i64::MIN;
    for &x in &d.nodes[face].children {
        m = m.max(py_int_pub(d.attr(x, "id").ok_or("글꼴 id 가 없습니다.")?)?);
    }
    let id = (m + 1).to_string();
    d.set_attr(clone, "id", &id);
    d.append_child(face, clone);
    let n = d.nodes[face].children.len().to_string();
    d.set_attr(face, "fontCnt", &n);
    Ok(Some(id))
}

fn remove_lineseg(d: &mut Doc, p: usize) {
    let rm: Vec<usize> = d.nodes[p].children.iter().copied().filter(|&x| d.tag(x) == "linesegarray").collect();
    for x in rm {
        d.remove_child(p, x);
    }
}

fn blank_apply(h: &mut Doc, secs: &mut [&mut Doc], learned: &Value) -> R<usize> {
    if !truthy(get(learned, "slots")) && !truthy(get(learned, "after")) {
        return Ok(0);
    }
    let fontfaces = first_by_tag(h, "fontfaces");
    let border_fills = first_by_tag(h, "borderFills");
    let tab_props = first_by_tag(h, "tabProperties");
    let char_props = first_by_tag(h, "charProperties");
    let para_props = first_by_tag(h, "paraProperties");
    let (Some(fontfaces), Some(char_props), Some(para_props)) = (fontfaces, char_props, para_props) else { return Ok(0) };
    let mut count = 0;
    let gaps_v = get(learned, "after").filter(|v| truthy(Some(v)));
    let mut shapes: HashMap<(Key, String), Key> = HashMap::new();
    // 중제목 다음 첫 문장의 문단 위 간격
    for d in secs.iter_mut() {
        for (p, k) in after_tables(d)? {
            let Some(gv) = gaps_v.and_then(|g| g.as_object()).and_then(|o| o.get(&k)) else { continue };
            let gap = gv.as_i64().ok_or_else(|| "after 값이 정수가 아닙니다.".to_string())?;
            let base = key(d, p, "paraPrIDRef");
            if !shapes.contains_key(&(base.clone(), k.clone())) {
                let Some(source) = by_id(h, para_props, &base) else { continue };
                if prev_value(h, source)? == Some(gap) {
                    shapes.insert((base.clone(), k.clone()), base.clone());
                } else {
                    let clone = h.clone_subtree(source);
                    set_prev(h, clone, gap)?;
                    let id = append_with_id(h, para_props, clone)?;
                    shapes.insert((base.clone(), k.clone()), Some(id));
                }
            }
            let to = shapes[&(base.clone(), k.clone())].clone();
            if to != base {
                d.set_attr(p, "paraPrIDRef", to.as_deref().unwrap());
                count += 1;
            }
        }
    }
    let mut made: HashMap<String, (String, String)> = HashMap::new();
    for d in secs.iter_mut() {
        for (p, k) in slots(d)? {
            let Some((slot_key, item)) = lookup(learned, &k) else { continue };
            if !made.contains_key(&slot_key) {
                let para_xml = get(item, "para").ok_or("프로필 슬롯에 para 가 없습니다.")?;
                let char_xml = get(item, "char").ok_or("프로필 슬롯에 char 가 없습니다.")?;
                if h.nodes[para_props].children.is_empty() {
                    return Err("paraProperties 가 비어 있습니다.".into());
                }
                let para = clone_from_xml(h, para_xml)?;
                if truthy(get(item, "border")) && border_fills.is_some() {
                    let f = clone_from_xml(h, get(item, "border").unwrap())?;
                    let fill_id = append_with_id(h, border_fills.unwrap(), f)?;
                    for b in h.walk(para) {
                        if h.tag(b) == "border" {
                            h.set_attr(b, "borderFillIDRef", &fill_id);
                        }
                    }
                }
                if truthy(get(item, "tab")) && tab_props.is_some() {
                    let t = clone_from_xml(h, get(item, "tab").unwrap())?;
                    let id = append_with_id(h, tab_props.unwrap(), t)?;
                    h.set_attr(para, "tabPrIDRef", &id);
                }
                let ch = clone_from_xml(h, char_xml)?;
                let rf = h.nodes[ch].children.iter().copied().find(|&x| h.tag(x) == "fontRef");
                if let Some(rf) = rf {
                    let fonts = match get(item, "fonts") {
                        None => None,
                        Some(Value::Object(o)) => Some(o),
                        Some(_) => return Err("프로필 fonts 가 객체가 아닙니다.".into()),
                    };
                    if let Some(fonts) = fonts {
                        for (lang, xml) in fonts {
                            if let Some(id) = font_id_blank(h, fontfaces, lang, xml)? {
                                h.set_attr(rf, lang, &id);
                            }
                        }
                    }
                }
                if truthy(get(item, "char_border")) && border_fills.is_some() {
                    let f = clone_from_xml(h, get(item, "char_border").unwrap())?;
                    let id = append_with_id(h, border_fills.unwrap(), f)?;
                    h.set_attr(ch, "borderFillIDRef", &id);
                }
                let pid = append_with_id(h, para_props, para)?;
                let cid = append_with_id(h, char_props, ch)?;
                made.insert(slot_key.clone(), (pid, cid));
            }
            let (pid, cid) = made[&slot_key].clone();
            d.set_attr(p, "paraPrIDRef", &pid);
            if !k.starts_with("host|") {
                for r in runs(d, p) {
                    d.set_attr(r, "charPrIDRef", &cid);
                }
            }
            remove_lineseg(d, p);
            count += 1;
        }
    }
    Ok(count)
}

pub fn survey_blank(sections: &Sections, cfg: &Config, m: &mut serde_json::Map<String, Value>) -> R<()> {
    let learned = cfg.blank_line_profile.as_ref().filter(|v| truthy(Some(v)));
    for (n, _, d) in sections {
        let v: Vec<usize> = if learned.is_some() { (0..slots(d)?.len()).collect() } else { vec![] };
        m.insert(n.clone(), serde_json::json!(v));
    }
    Ok(())
}

fn all_empty(sel: &Value) -> R<bool> {
    let obj = sel.as_object().ok_or("selections 는 객체여야 합니다.")?;
    Ok(obj.values().all(|v| v.as_array().map_or(false, |a| a.is_empty())))
}

pub fn process_blank(contents: &mut Contents, mut sections: Sections, sel: &Value, source: &Path, target: &Path, cfg: &Config) -> R<usize> {
    if all_empty(sel)? {
        zipio::copy_file_safely(source, target)?;
        return Ok(0);
    }
    let (hidx, mut h) = crate::parse_header(contents)?;
    let before: Vec<String> = sections.iter().map(|(_, _, d)| d.jstr_walk(0)).collect();
    let learned = cfg.blank_line_profile.clone().filter(|v| truthy(Some(v))).unwrap_or(Value::Object(Default::default()));
    let count = {
        let mut refs: Vec<&mut Doc> = sections.iter_mut().map(|(_, _, d)| d).collect();
        blank_apply(&mut h, &mut refs, &learned)?
    };
    for (k, (_, _, d)) in sections.iter().enumerate() {
        if d.jstr_walk(0) != before[k] {
            return Err("빈 줄 서식 처리 중 글 보존 검사에 실패했습니다.".into());
        }
    }
    for (_, idx, d) in &sections {
        contents[*idx].1 = d.tostring();
    }
    contents[hidx].1 = h.tostring();
    zipio::write_hwpx_safely(contents, target)?;
    Ok(count)
}

// ---- 강조 낱말 -------------------------------------------------------------

const AFTER_BOLD: [&str; 8] = ["underline", "strikeout", "outline", "shadow", "emboss", "engrave", "supscript", "subscript"];

struct EmpInfo {
    chars: HashMap<Key, usize>,
    char_group: Option<usize>,
    faces: HashMap<String, usize>,
}

impl EmpInfo {
    fn new(h: &Doc) -> Self {
        let mut chars = HashMap::new();
        let mut faces = HashMap::new();
        for i in h.walk(0) {
            match h.tag(i) {
                "charPr" => {
                    chars.insert(key(h, i, "id"), i);
                }
                "fontface" => {
                    faces.insert(h.attr(i, "lang").unwrap_or("").to_lowercase(), i);
                }
                _ => {}
            }
        }
        EmpInfo { chars, char_group: first_by_tag(h, "charProperties"), faces }
    }
}

/// emphasis._simple_runs: 글 요소가 하나뿐이고 안에 다른 요소가 없는 run 만. 아니면 None.
fn emp_runs(d: &Doc, p: usize) -> Option<Vec<(usize, String)>> {
    let mut out = vec![];
    for &run in &d.nodes[p].children {
        if d.tag(run) != "run" {
            continue;
        }
        let texts: Vec<usize> = d.nodes[run].children.iter().copied().filter(|&x| d.tag(x) == "t").collect();
        let others = d.nodes[run].children.iter().any(|&x| d.tag(x) != "t");
        if texts.is_empty() {
            if others {
                out.push((run, String::new()));
            }
            continue;
        }
        if texts.len() != 1 || !d.nodes[texts[0]].children.is_empty() {
            return None;
        }
        out.push((run, ctext(d, texts[0])));
    }
    Some(out)
}

fn marker_end_emp(text: &str) -> (String, usize) {
    let (g, _, raw) = unify_marker(py_strip(text));
    let lead = clen(text) - clen(text.trim_start_matches(py_ws));
    (g, if raw.is_empty() { 0 } else { lead + clen(&raw) })
}

fn key_of(text: &str) -> String {
    text.chars().filter(|c| !py_ws(*c)).collect()
}

fn font_id_emp(h: &mut Doc, info: &EmpInfo, lang: &str, xml: &Value) -> R<Option<String>> {
    let Some(&group) = info.faces.get(&lang.to_lowercase()) else { return Ok(None) };
    if h.nodes[group].children.is_empty() {
        return Ok(None);
    }
    let sample = parse_fragment(xml.as_str().ok_or("프로필 XML 이 문자열이 아닙니다.")?, h)?;
    let (sf, st) = (sample.attr(0, "face").map(String::from), sample.attr(0, "type").map(String::from));
    if let Some(same) = h.nodes[group].children.iter().copied().find(|&x| h.attr(x, "face").map(String::from) == sf && h.attr(x, "type").map(String::from) == st) {
        return Ok(h.attr(same, "id").map(String::from));
    }
    let clone = h.graft(&sample);
    let mut m: i64 = i64::MIN;
    for &x in &h.nodes[group].children {
        m = m.max(py_int_pub(h.attr(x, "id").ok_or("글꼴 id 가 없습니다.")?)?);
    }
    let id = (m + 1).to_string();
    h.set_attr(clone, "id", &id);
    h.append_child(group, clone);
    let n = h.nodes[group].children.len().to_string();
    h.set_attr(group, "fontCnt", &n);
    Ok(Some(id))
}

fn max_char_id(h: &Doc, group: usize) -> R<i64> {
    let mut m: i64 = -1;
    for &x in &h.nodes[group].children {
        m = m.max(py_int_pub(h.attr(x, "id").ok_or("글자 모양 id 가 없습니다.")?)?);
    }
    Ok(m)
}

fn json_int(v: Option<&Value>, default: i64) -> R<i64> {
    match v {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Number(n)) => Ok(n.as_i64().unwrap_or_else(|| n.as_f64().map_or(0, |f| f.trunc() as i64))),
        Some(Value::Bool(b)) => Ok(*b as i64),
        _ => Err("프로필 숫자 값이 숫자가 아닙니다.".into()),
    }
}

/// `_styled`: 강조 모양을 입힌 글자 모양의 id(같은 (원본, 항목)은 사본 하나를 공유)
fn styled(h: &mut Doc, info: &mut EmpInfo, base: &Key, entry_idx: usize, entry: &Value, cache: &mut HashMap<(Key, usize), Key>) -> R<Key> {
    let ck = (base.clone(), entry_idx);
    if let Some(v) = cache.get(&ck) {
        return Ok(v.clone());
    }
    let (Some(&source), Some(group)) = (info.chars.get(base), info.char_group) else { return Ok(base.clone()) };
    let clone = h.clone_subtree(source);
    if get(entry, "kind").and_then(|v| v.as_str()) == Some("color") {
        let color = get(entry, "color").and_then(|v| v.as_str()).ok_or("프로필 color 가 문자열이 아닙니다.")?;
        h.set_attr(clone, "textColor", color);
        let id = (1 + max_char_id(h, group)?).to_string();
        h.set_attr(clone, "id", &id);
        h.append_child(group, clone);
        let n = h.nodes[group].children.len().to_string();
        h.set_attr(group, "itemCnt", &n);
        info.chars.insert(Some(id.clone()), clone);
        cache.insert(ck, Some(id.clone()));
        return Ok(Some(id));
    }
    let hv = h.attr(source, "height").unwrap_or("");
    let height = if hv.is_empty() { 1000 } else { py_int_pub(hv)? };
    let delta = json_int(get(entry, "size_delta").filter(|v| truthy(Some(v))), 0)?;
    h.set_attr(clone, "height", &100i64.max(height + delta).to_string());
    if truthy(get(entry, "font")) {
        let rf = h.nodes[clone].children.iter().copied().find(|&x| h.tag(x) == "fontRef");
        let fonts = match get(entry, "fonts") {
            None | Some(Value::Null) => None,
            Some(Value::Object(o)) => Some(o.clone()),
            Some(_) => return Err("프로필 fonts 가 객체가 아닙니다.".into()),
        };
        if let Some(fonts) = fonts {
            for (lang, xml) in &fonts {
                let new = font_id_emp(h, info, lang, xml)?;
                if let (Some(v), Some(rf)) = (new, rf) {
                    h.set_attr(rf, lang, &v);
                }
            }
        }
    }
    let bold = h.nodes[clone].children.iter().copied().find(|&x| h.tag(x) == "bold");
    let want_bold = truthy(get(entry, "bold"));
    if want_bold && bold.is_none() {
        let q = h.nodes[clone].qname.clone();
        let bq = match q.rsplit_once(':') {
            Some((pre, _)) => format!("{pre}:bold"),
            None => "bold".to_string(),
        };
        let idx = h.nodes[clone].children.iter().position(|&x| AFTER_BOLD.contains(&h.tag(x))).unwrap_or(h.nodes[clone].children.len());
        let b = h.new_elem(&bq, vec![]);
        h.insert_child(clone, idx, b);
    } else if !want_bold {
        if let Some(b) = bold {
            h.remove_child(clone, b);
        }
    }
    let id = (1 + max_char_id(h, group)?).to_string();
    h.set_attr(clone, "id", &id);
    h.append_child(group, clone);
    let n = h.nodes[group].children.len().to_string();
    h.set_attr(group, "itemCnt", &n);
    info.chars.insert(Some(id.clone()), clone);
    cache.insert(ck, Some(id.clone()));
    Ok(Some(id))
}

/// `_split`: 글 위치 a·b 에서 run 을 나눠 [a, b) 를 덮는 run 목록
fn split(d: &mut Doc, p: usize, runs: &[(usize, String)], a: usize, b: usize) -> R<Vec<usize>> {
    let mut covered = vec![];
    let mut position = 0;
    for (run, text) in runs {
        let (start, end) = (position, position + clen(text));
        position = end;
        if text.is_empty() || end <= a || start >= b {
            continue;
        }
        let mut cuts: Vec<usize> = vec![start.max(a), end.min(b)].into_iter().filter(|&c| c != start && c != end).collect();
        cuts.sort();
        cuts.dedup();
        let mut pieces = vec![];
        let mut last = start;
        for cut in cuts.into_iter().chain(std::iter::once(end)) {
            pieces.push((last, cut));
            last = cut;
        }
        let index = d.index_of(p, *run).ok_or("run 위치를 찾지 못했습니다.")?;
        let chars: Vec<char> = text.chars().collect();
        let mut made = vec![];
        for (offset, &(s, e)) in pieces.iter().enumerate() {
            let piece = if offset == 0 {
                *run
            } else {
                let c = d.clone_subtree(*run);
                let rm: Vec<usize> = d.nodes[c].children.iter().copied().filter(|&x| d.tag(x) != "t").collect();
                for x in rm {
                    d.remove_child(c, x);
                }
                d.insert_child(p, index + offset, c);
                c
            };
            let t = d.nodes[piece].children.iter().copied().find(|&x| d.tag(x) == "t").ok_or("글 요소가 없습니다.")?;
            d.nodes[t].text = Some(chars[s - start..e - start].iter().collect());
            made.push((piece, s, e));
        }
        for (piece, s, e) in made {
            if s >= a && e <= b {
                covered.push(piece);
            }
        }
    }
    Ok(covered)
}

fn emphasis_apply(h: &mut Doc, secs: &mut [&mut Doc], learned: &[Value]) -> R<usize> {
    if learned.is_empty() {
        return Ok(0);
    }
    for e in learned {
        if !e.is_object() {
            return Err("강조 항목이 객체가 아닙니다.".into());
        }
    }
    let mut info = EmpInfo::new(h);
    if info.char_group.is_none() {
        return Ok(0);
    }
    let mut cache = HashMap::new();
    let mut count = 0;
    for d in secs.iter_mut() {
        for p in body_paras(d) {
            let Some(rs) = emp_runs(d, p) else { continue };
            if rs.is_empty() {
                continue;
            }
            let full: String = rs.iter().map(|(_, t)| t.as_str()).collect();
            let (group, marker_end) = marker_end_emp(&full);
            let fk = key_of(&full);
            let entries: Vec<usize> = learned
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    get(e, "marker").and_then(|v| v.as_str()) == Some(group.as_str())
                        && (!group.is_empty() || get(e, "kind").and_then(|v| v.as_str()) == Some("color"))
                        && match get(e, "context") {
                            None | Some(Value::Null) => true,
                            Some(c) => c.as_str() == Some(fk.as_str()),
                        }
                })
                .map(|(i, _)| i)
                .collect();
            if entries.is_empty() {
                continue;
            }
            let cs: Vec<char> = full.chars().collect();
            let mut spans: Vec<(usize, usize, usize)> = vec![];
            for ei in entries {
                let entry = &learned[ei];
                let text = get(entry, "text").and_then(|v| v.as_str()).ok_or("프로필 text 가 문자열이 아닙니다.")?;
                let tc: Vec<char> = text.chars().collect();
                let find = |from: usize| -> Option<usize> {
                    if from > cs.len() {
                        return None;
                    }
                    (from..=cs.len().saturating_sub(tc.len())).find(|&i| i + tc.len() <= cs.len() && cs[i..i + tc.len()] == tc[..])
                };
                let at = if truthy(get(entry, "whole")) {
                    if py_strip(&full) != text {
                        continue;
                    }
                    find(0)
                } else {
                    find(marker_end)
                };
                let Some(at) = at else { continue };
                if spans.iter().any(|&(x, y, _)| at < y && x < at + tc.len()) {
                    continue;
                }
                spans.push((at, at + tc.len(), ei));
            }
            let any = !spans.is_empty();
            for (a, b, ei) in spans {
                let rs = emp_runs(d, p).unwrap_or_default();
                for piece in split(d, p, &rs, a, b)? {
                    let base = key(d, piece, "charPrIDRef");
                    let nid = styled(h, &mut info, &base, ei, &learned[ei], &mut cache)?;
                    match nid {
                        Some(v) => d.set_attr(piece, "charPrIDRef", &v),
                        None => return Err("charPrIDRef 값이 없어 직렬화할 수 없습니다.".into()),
                    }
                }
                count += 1;
            }
            if any {
                remove_lineseg(d, p);
            }
        }
    }
    Ok(count)
}

pub fn survey_emphasis(sections: &Sections, cfg: &Config, m: &mut serde_json::Map<String, Value>) -> R<()> {
    for (n, _, _) in sections {
        m.insert(n.clone(), serde_json::json!(if cfg.emphasis_words.is_empty() { vec![] } else { vec![0] }));
    }
    Ok(())
}

pub fn process_emphasis(contents: &mut Contents, mut sections: Sections, sel: &Value, source: &Path, target: &Path, cfg: &Config) -> R<usize> {
    if all_empty(sel)? {
        zipio::copy_file_safely(source, target)?;
        return Ok(0);
    }
    let (hidx, mut h) = crate::parse_header(contents)?;
    let before: Vec<String> = sections.iter().map(|(_, _, d)| d.jstr_walk(0)).collect();
    let count = {
        let mut refs: Vec<&mut Doc> = sections.iter_mut().map(|(_, _, d)| d).collect();
        emphasis_apply(&mut h, &mut refs, &cfg.emphasis_words)?
    };
    for (k, (_, _, d)) in sections.iter().enumerate() {
        if d.jstr_walk(0) != before[k] {
            return Err("강조 낱말 처리 중 글 보존 검사에 실패했습니다.".into());
        }
    }
    for (_, idx, d) in &sections {
        contents[*idx].1 = d.tostring();
    }
    contents[hidx].1 = h.tostring();
    zipio::write_hwpx_safely(contents, target)?;
    Ok(count)
}
