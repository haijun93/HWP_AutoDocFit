//! HWPXFMT5: 구조 서식의 참조 병합과 붙임·한 칸 상자 처리.
//! 지원하지 않는 구조는 종료 코드 4로 돌려 Python 기준 경로에서 처리한다.
use crate::{
    config::{unsupported, Config},
    dom::{self, Doc},
    rules::{self, FormatKind, R},
    zipio, Processor,
};
use serde_json::{json, Value};
use std::{collections::HashMap, path::Path};

type Maps = HashMap<String, HashMap<String, String>>;
fn find(d: &Doc, name: &str) -> R<usize> {
    d.walk(0)
        .into_iter()
        .find(|&n| d.tag(n) == name)
        .ok_or_else(|| format!("서식 그룹 없음: {name}"))
}
fn value(d: &Doc, n: usize, k: &str) -> R<String> {
    d.attr(n, k)
        .map(str::to_owned)
        .ok_or_else(|| format!("속성 없음: {k}"))
}
fn map_id(m: &Maps, group: &str, id: &str) -> R<String> {
    m.get(group)
        .and_then(|x| x.get(id))
        .cloned()
        .ok_or_else(|| format!("서식 참조 없음: {group}/{id}"))
}
fn next_id(d: &Doc, group: usize) -> R<String> {
    let mut max = -1i64;
    for &n in &d.nodes[group].children {
        max = max.max(rules::py_int_pub(d.attr(n, "id").ok_or("id 없음")?)?);
    }
    Ok((max + 1).to_string())
}
fn import(d: &mut Doc, s: &Doc, root: usize) -> usize {
    let order = s.walk(root);
    let map: HashMap<_, _> = order.iter().enumerate().map(|(i, &n)| (n, i)).collect();
    let mut nodes = vec![];
    for n in order {
        let mut node = s.nodes[n].clone();
        node.children = node.children.iter().map(|n| map[n]).collect();
        node.end = 0;
        nodes.push(node);
    }
    for i in (0..nodes.len()).rev() {
        nodes[i].end = nodes[i]
            .children
            .last()
            .map(|&c| nodes[c].end)
            .unwrap_or(i + 1);
    }
    // 예시의 접두사는 대상과 다를 수 있다. 복사한 뿌리에 원래 URI 바인딩을 보존한다.
    for (k, v) in &s.nodes[0].attrs {
        if k == "xmlns" || k.starts_with("xmlns:") {
            if !nodes[0].attrs.iter().any(|(a, _)| a == k) {
                nodes[0].attrs.push((k.clone(), v.clone()));
            }
        }
    }
    let tail = nodes[0].tail.clone();
    let n = d.graft(&Doc { nodes, decl: None });
    d.nodes[n].tail = tail;
    n
}
fn merge(h: &mut Doc, s: &Doc) -> R<Maps> {
    let mut maps = Maps::new();
    let sf = find(s, "fontfaces")?;
    let df = find(h, "fontfaces")?;
    for &face in &s.nodes[sf].children {
        let lang = value(s, face, "lang")?;
        let target = if let Some(n) = h.nodes[df]
            .children
            .iter()
            .copied()
            .find(|&n| h.attr(n, "lang") == Some(&lang))
        {
            n
        } else {
            let n = h.new_elem(
                &s.nodes[face].qname,
                vec![
                    ("lang".into(), lang.clone()),
                    ("fontCnt".into(), "0".into()),
                ],
            );
            for (k, v) in &s.nodes[0].attrs {
                if k == "xmlns" || k.starts_with("xmlns:") {
                    h.set_attr(n, k, v);
                }
            }
            h.append_child(df, n);
            n
        };
        let mut m = HashMap::new();
        for &font in &s.nodes[face].children {
            let same = h.nodes[target].children.iter().copied().find(|&n| {
                h.attr(n, "face") == s.attr(font, "face")
                    && h.attr(n, "type") == s.attr(font, "type")
            });
            let n = if let Some(n) = same {
                n
            } else {
                let id = next_id(h, target)?;
                let n = import(h, s, font);
                h.set_attr(n, "id", &id);
                h.append_child(target, n);
                n
            };
            m.insert(value(s, font, "id")?, value(h, n, "id")?);
        }
        h.set_attr(
            target,
            "fontCnt",
            &h.nodes[target].children.len().to_string(),
        );
        maps.insert(lang.to_lowercase(), m);
    }
    for group in [
        "borderFills",
        "tabProperties",
        "charProperties",
        "paraProperties",
    ] {
        let sg = find(s, group)?;
        let dg = find(h, group)?;
        maps.insert(group.into(), HashMap::new());
        for &item in &s.nodes[sg].children {
            let id = next_id(h, dg)?;
            let clone = import(h, s, item);
            h.set_attr(clone, "id", &id);
            maps.get_mut(group)
                .unwrap()
                .insert(value(s, item, "id")?, id);
            for n in h.walk(clone) {
                for (attr, category) in [
                    ("borderFillIDRef", "borderFills"),
                    ("tabPrIDRef", "tabProperties"),
                ] {
                    if let Some(old) = h.attr(n, attr).map(str::to_owned) {
                        if let Some(new) = maps.get(category).and_then(|m| m.get(&old)) {
                            h.set_attr(n, attr, new);
                        }
                    }
                }
                if h.tag(n) == "fontRef" {
                    for (lang, old) in h.nodes[n].attrs.clone() {
                        h.set_attr(n, &lang, &map_id(&maps, &lang, &old)?);
                    }
                }
            }
            h.append_child(dg, clone);
        }
        h.set_attr(dg, "itemCnt", &h.nodes[dg].children.len().to_string());
    }
    Ok(maps)
}
fn nonempty_paras(d: &Doc, c: usize) -> Vec<usize> {
    d.cell_paras(c)
        .into_iter()
        .filter(|&p| !rules::py_strip(&d.jstr_walk(p)).is_empty())
        .collect()
}
fn update(d: &mut Doc, n: usize, s: &Doc, sn: usize) {
    for (k, v) in &s.nodes[sn].attrs {
        d.set_attr(n, k, v);
    }
}
fn copy_style(d: &mut Doc, t: usize, s: &Doc, st: usize, m: &Maps, box_mode: bool) -> R<()> {
    for attr in [
        "borderFillIDRef",
        "cellSpacing",
        "textWrap",
        "textFlow",
        "pageBreak",
        "repeatHeader",
    ] {
        if let Some(v) = s.attr(st, attr) {
            d.set_attr(
                t,
                attr,
                &if attr == "borderFillIDRef" {
                    map_id(m, "borderFills", v)?
                } else {
                    v.to_owned()
                },
            );
        }
    }
    for name in ["sz", "inMargin", "outMargin"] {
        if let (Some(a), Some(b)) = (d.child(t, name), s.child(st, name)) {
            update(d, a, s, b);
        }
    }
    let sc = s.cells(st);
    let cells = d.cells(t);
    for (i, c) in cells.into_iter().enumerate() {
        let Some(&cs) = sc
            .get(i)
            .or_else(|| if !box_mode { sc.last() } else { None })
        else {
            continue;
        };
        d.set_attr(
            c,
            "borderFillIDRef",
            &map_id(m, "borderFills", &value(s, cs, "borderFillIDRef")?)?,
        );
        d.set_attr(c, "hasMargin", s.attr(cs, "hasMargin").unwrap_or("0"));
        for name in ["cellSz", "cellMargin"] {
            if let (Some(a), Some(b)) = (d.child(c, name), s.child(cs, name)) {
                let old = d.attr(a, "height").map(str::to_owned);
                update(d, a, s, b);
                if name == "cellSz" {
                    if let Some(old) = old {
                        if let (Ok(x), Ok(y)) = (
                            old.parse::<i64>(),
                            d.attr(a, "height").unwrap_or("0").parse::<i64>(),
                        ) {
                            if x > 0 && x < y {
                                d.set_attr(a, "height", &old);
                            }
                        }
                    }
                }
            }
        }
        let sub = d.child(c, "subList").ok_or("subList 없음")?;
        let ss = s.child(cs, "subList").ok_or("예시 subList 없음")?;
        d.set_attr(
            sub,
            "vertAlign",
            s.attr(ss, "vertAlign").unwrap_or("CENTER"),
        );
        let ps = nonempty_paras(d, c);
        let sp = nonempty_paras(s, cs);
        if !ps.is_empty() && sp.is_empty() {
            return Err("예시 본문 없음".into());
        }
        let body = if box_mode && ps.len() != sp.len() {
            sp.iter().copied().max_by_key(|&p| {
                (
                    rules::py_strip(&s.jstr(p)).chars().count(),
                    std::cmp::Reverse(p),
                )
            })
        } else {
            None
        };
        for (i, p) in ps.iter().copied().enumerate() {
            let sample =
                body.unwrap_or(sp[(i + sp.len().saturating_sub(ps.len())).min(sp.len() - 1)]);
            d.set_attr(
                p,
                "paraPrIDRef",
                &map_id(m, "paraProperties", &value(s, sample, "paraPrIDRef")?)?,
            );
            d.set_attr(p, "styleIDRef", "0");
            let runs: Vec<_> = s.nodes[sample]
                .children
                .iter()
                .copied()
                .filter(|&r| s.tag(r) == "run")
                .collect();
            let main = *runs
                .iter()
                .max_by_key(|&&r| (s.jstr(r).chars().count(), std::cmp::Reverse(r)))
                .ok_or("예시 run 없음")?;
            let main = value(s, main, "charPrIDRef")?;
            let quote = runs
                .iter()
                .find(|&&r| matches!(rules::py_strip(&s.jstr(r)), "‘" | "’" | "'"))
                .and_then(|&r| s.attr(r, "charPrIDRef"))
                .unwrap_or(&main);
            for r in d.nodes[p].children.clone() {
                if d.tag(r) == "linesegarray" {
                    d.remove_child(p, r);
                    continue;
                }
                if d.tag(r) != "run" {
                    continue;
                }
                d.set_attr(r, "charPrIDRef", &map_id(m, "charProperties", &main)?);
                if !d.nodes[r]
                    .children
                    .iter()
                    .all(|&n| d.tag(n) == "t" && d.nodes[n].children.is_empty())
                {
                    continue;
                }
                let text = d.jstr_walk(r);
                let mut pieces = vec![];
                let mut acc = String::new();
                for ch in text.chars() {
                    if matches!(ch, '‘' | '’' | '\'') {
                        if !acc.is_empty() {
                            pieces.push(std::mem::take(&mut acc));
                        }
                        pieces.push(ch.to_string());
                    } else {
                        acc.push(ch);
                    }
                }
                if !acc.is_empty() {
                    pieces.push(acc);
                }
                let mut at = d.index_of(p, r).unwrap();
                d.remove_child(p, r);
                let prefix = d.nodes[r]
                    .qname
                    .rsplit_once(':')
                    .map(|(p, _)| format!("{p}:t"))
                    .unwrap_or_else(|| "t".into());
                for piece in pieces {
                    let nr = d.new_elem(&d.nodes[r].qname.clone(), d.nodes[r].attrs.clone());
                    let cp = if matches!(piece.as_str(), "‘" | "’" | "'") {
                        quote
                    } else {
                        &main
                    };
                    d.set_attr(nr, "charPrIDRef", &map_id(m, "charProperties", cp)?);
                    let nt = d.new_elem(&prefix, vec![]);
                    d.nodes[nt].text = Some(piece);
                    d.append_child(nr, nt);
                    d.insert_child(p, at, nr);
                    at += 1;
                }
            }
        }
    }
    Ok(())
}
fn top_tables(d: &Doc) -> Vec<usize> {
    d.nodes[0]
        .children
        .iter()
        .flat_map(|&p| d.nodes[p].children.iter())
        .flat_map(|&r| d.nodes[r].children.iter())
        .copied()
        .filter(|&n| d.tag(n) == "tbl")
        .collect()
}
fn mid_kind(d: &Doc, t: usize) -> R<Value> {
    let cols = rules::py_int_pub(d.attr(t, "colCnt").unwrap_or("0"))?;
    if (cols == 2 || cols == 3) && rules::mid_cells(d, t, false)? == vec![(cols - 1) as usize] {
        return Ok(json!(if cols == 3 { 1 } else { 2 }));
    }
    Ok(if rules::mid_cells(d, t, true)?.is_empty() {
        Value::Null
    } else {
        json!("fit")
    })
}
fn mid_fit(d: &mut Doc, t: usize, h: &Doc, max: i64) -> R<()> {
    let cells = d.cells(t);
    let text_cells = rules::mid_cells(d, t, true)?;
    let Some(size) = d.child(t, "sz") else {
        return Ok(());
    };
    let sizes: Vec<_> = cells.iter().map(|&c| d.child(c, "cellSz")).collect();
    if text_cells.is_empty() || sizes.iter().any(Option::is_none) {
        return Ok(());
    }
    let sizes: Vec<_> = sizes.into_iter().map(Option::unwrap).collect();
    let mut dims = HashMap::new();
    for n in h.walk(0).into_iter().filter(|&n| h.tag(n) == "charPr") {
        let height = crate::tbl::int_or(h.attr(n, "height"), 0)?;
        let height = if height == 0 { 2000 } else { height };
        let ratio = h
            .child(n, "ratio")
            .map(|r| crate::tbl::int_or(h.attr(r, "hangul"), 100))
            .transpose()?
            .unwrap_or(100);
        let spacing = h
            .child(n, "spacing")
            .map(|r| crate::tbl::int_or(h.attr(r, "hangul"), 0))
            .transpose()?
            .unwrap_or(0);
        dims.insert(
            h.attr(n, "id").unwrap_or("").to_owned(),
            (height, ratio, spacing),
        );
    }
    let mut widths: Vec<i64> = sizes
        .iter()
        .map(|&s| crate::tbl::int_or(d.attr(s, "width"), 0))
        .collect::<R<_>>()?;
    let mut minimum = HashMap::new();
    for &i in &text_cells {
        let margin = if d.attr(cells[i], "hasMargin") == Some("1") {
            d.child(cells[i], "cellMargin")
        } else {
            d.child(t, "inMargin")
        };
        let lr = match margin {
            Some(m) => {
                crate::tbl::int_or(d.attr(m, "left"), 0)?
                    + crate::tbl::int_or(d.attr(m, "right"), 0)?
            }
            None => 0,
        };
        let (mut widest, mut largest) = (0, 2000);
        for p in d.cell_paras(cells[i]) {
            let (mut width, mut largest_line) = (0f64, 0);
            for &r in &d.nodes[p].children {
                if d.tag(r) != "run" {
                    continue;
                }
                let (height, ratio, spacing) = dims
                    .get(d.attr(r, "charPrIDRef").unwrap_or(""))
                    .copied()
                    .unwrap_or((2000, 100, 0));
                let text = d.jstr_walk(r);
                if !text.is_empty() {
                    largest_line = largest_line.max(height);
                }
                for c in text.chars() {
                    let factor = if c == ' ' {
                        0.5
                    } else if c.is_ascii() {
                        0.55
                    } else {
                        1.0
                    };
                    width += height as f64 * factor * ratio as f64 / 100.0
                        + height as f64 * spacing as f64 / 100.0;
                }
            }
            widest = widest.max(width as i64);
            largest = largest.max(largest_line);
            for n in d.nodes[p].children.clone() {
                if d.tag(n) == "linesegarray" {
                    d.remove_child(p, n);
                }
            }
        }
        minimum.insert(i, 2 * largest + lr);
        widths[i] = (widest + lr + 600.max(largest / 2)).max(minimum[&i]);
    }
    let total: i64 = widths.iter().sum();
    if max != 0 && total > max {
        let excess = total - max;
        let tw: i64 = text_cells.iter().map(|&i| widths[i]).sum();
        for &i in &text_cells {
            widths[i] = minimum[&i]
                .max(widths[i] - (excess as f64 * widths[i] as f64 / tw as f64) as i64 - 1);
        }
    }
    for (&s, w) in sizes.iter().zip(&widths) {
        d.set_attr(s, "width", &w.to_string());
    }
    d.set_attr(size, "width", &widths.iter().sum::<i64>().to_string());
    Ok(())
}
fn mid_copy(d: &mut Doc, t: usize, s: &Doc, st: usize, m: &Maps, kind: u64) -> R<()> {
    for attr in [
        "borderFillIDRef",
        "cellSpacing",
        "textWrap",
        "textFlow",
        "pageBreak",
        "repeatHeader",
    ] {
        if let Some(v) = s.attr(st, attr) {
            d.set_attr(
                t,
                attr,
                &if attr == "borderFillIDRef" {
                    map_id(m, "borderFills", v)?
                } else {
                    v.into()
                },
            );
        }
    }
    for name in ["inMargin", "outMargin"] {
        if let (Some(a), Some(b)) = (d.child(t, name), s.child(st, name)) {
            update(d, a, s, b);
        }
    }
    let sc = s.cells(st);
    let roles = if kind == 1 {
        sc
    } else {
        vec![sc[0], *sc.last().unwrap()]
    };
    for (cell, cs) in d.cells(t).into_iter().zip(roles) {
        d.set_attr(
            cell,
            "borderFillIDRef",
            &map_id(m, "borderFills", &value(s, cs, "borderFillIDRef")?)?,
        );
        d.set_attr(cell, "hasMargin", s.attr(cs, "hasMargin").unwrap_or("0"));
        if let (Some(a), Some(b)) = (d.child(cell, "cellMargin"), s.child(cs, "cellMargin")) {
            update(d, a, s, b);
        }
        let sub = d.child(cell, "subList").ok_or("subList 없음")?;
        let ss = s.child(cs, "subList").ok_or("예시 subList 없음")?;
        d.set_attr(
            sub,
            "vertAlign",
            s.attr(ss, "vertAlign").unwrap_or("CENTER"),
        );
        let sp = *s.cell_paras(cs).first().ok_or("예시 문단 없음")?;
        let sr = *s.nodes[sp]
            .children
            .iter()
            .find(|&&r| s.tag(r) == "run")
            .ok_or("예시 run 없음")?;
        for p in d.cell_paras(cell) {
            d.set_attr(
                p,
                "paraPrIDRef",
                &map_id(m, "paraProperties", &value(s, sp, "paraPrIDRef")?)?,
            );
            d.set_attr(p, "styleIDRef", "0");
            for r in d.nodes[p].children.clone() {
                if d.tag(r) == "linesegarray" {
                    d.remove_child(p, r);
                } else if d.tag(r) == "run" {
                    d.set_attr(
                        r,
                        "charPrIDRef",
                        &map_id(m, "charProperties", &value(s, sr, "charPrIDRef")?)?,
                    );
                }
            }
        }
    }
    Ok(())
}
fn template(cfg: &Config, key: &str) -> R<Option<(Doc, Doc, usize)>> {
    let Some(v) = cfg
        .format_samples
        .as_ref()
        .and_then(|v| v.get(key))
        .filter(|v| !v.is_null())
    else {
        return Ok(None);
    };
    let h = dom::parse(
        v.get("header_xml")
            .and_then(Value::as_str)
            .ok_or("미지원 입력: 예시 header_xml 없음")?
            .as_bytes(),
    )?;
    let d = dom::parse(
        v.get("table_xml")
            .and_then(Value::as_str)
            .ok_or("미지원 입력: 예시 table_xml 없음")?
            .as_bytes(),
    )?;
    let good = if key == "box" || key == "overview" {
        rules::format_kind(&d, 0)? == Some(FormatKind::Box)
    } else if let Some(k) = key.strip_prefix("attach") {
        rules::attach_kind(&d, 0)? == k.parse::<u8>().ok()
    } else if let Some(k) = key.strip_prefix("midtitle") {
        mid_kind(&d, 0)? == json!(k.parse::<u64>().unwrap_or(0))
    } else if let Some(k) = key.strip_prefix("title") {
        rules::title_kind_blank(&d, 0, false)? == k.parse::<u8>().ok()
    } else {
        false
    };
    if !good {
        return unsupported("예시 표 종류 불일치");
    }
    Ok(Some((h, d, 0)))
}
fn builtin(key: &str, index: usize) -> R<(Doc, Doc, usize)> {
    let v: Value =
        serde_json::from_str(include_str!("../templates.json")).map_err(|e| e.to_string())?;
    let h = dom::parse(v[key]["header"].as_str().unwrap().as_bytes())?;
    let d = dom::parse(v[key]["section"].as_str().unwrap().as_bytes())?;
    let t = *d.tables().get(index).ok_or("내장 표 없음")?;
    Ok((h, d, t))
}
fn check(p: Processor, cfg: &Config) -> R<()> {
    if !cfg.title_owner_text.is_empty() {
        return unsupported("설정 담당자 글 판별 미이식");
    }
    if !matches!(
        p,
        Processor::Attachment | Processor::BoxStyle | Processor::Midtitle | Processor::Title
    ) {
        return unsupported(
            "보고서 머리는 값 재배치·다중 보고서 경계의 동등성 확인 전 Python 경로 사용",
        );
    }
    if matches!(p, Processor::Midtitle | Processor::Title) && cfg.marker_scheme_active {
        return unsupported("활성 기호 체계의 제목 변환 미이식");
    }
    Ok(())
}
pub fn survey(p: Processor, source: &Path, cfg: &Config) -> R<Value> {
    check(p, cfg)?;
    let contents = zipio::read_hwpx_safely(source)?;
    crate::check_other_xml(&contents)?;
    crate::parse_header(&contents)?;
    let box_sample = if p == Processor::BoxStyle {
        template(cfg, "box")?
    } else {
        None
    };
    let mut out = serde_json::Map::new();
    for (name, _, d) in crate::parse_sections(&contents)? {
        if p == Processor::Title {
            out.insert(name, title_targets(&d)?);
            continue;
        }
        if p == Processor::Midtitle {
            for &para in &d.nodes[0].children {
                let text: String = d.nodes[para]
                    .children
                    .iter()
                    .filter(|&&r| d.tag(r) == "run")
                    .flat_map(|&r| d.nodes[r].children.iter().copied())
                    .filter(|&t| d.tag(t) == "t")
                    .map(|t| d.jstr(t))
                    .collect();
                if rules::py_strip(&text).chars().next().map_or(false, |c| {
                    ('Ⅰ'..='ⅿ').contains(&c) || matches!(c, 'I' | 'V' | 'X' | '로')
                }) {
                    return unsupported("일반 로마자 문단의 표 변환은 미확인");
                }
            }
        }
        let ts = top_tables(&d);
        let mut found = vec![];
        for (i, &t) in ts.iter().enumerate() {
            if p == Processor::Attachment {
                if let Some(k) = rules::attach_kind(&d, t)? {
                    found.push(json!([i, k]));
                }
            } else if p == Processor::Midtitle {
                let k = mid_kind(&d, t)?;
                if !k.is_null() {
                    found.push(json!([i, k]));
                }
            } else if box_sample.is_some()
                && rules::format_kind(&d, t)? == Some(FormatKind::Box)
                && !(i > 0 && rules::title_kind_blank(&d, ts[i - 1], true)?.is_some())
                && nonempty_paras(&d, d.cells(t)[0]).len() <= 3
            {
                found.push(json!(i));
            }
        }
        out.insert(name, Value::Array(found));
    }
    Ok(Value::Object(out))
}
pub fn process(
    p: Processor,
    source: &Path,
    target: &Path,
    sel: Option<&Value>,
    cfg: &Config,
) -> R<usize> {
    check(p, cfg)?;
    if std::fs::canonicalize(source).ok() == std::fs::canonicalize(target).ok() {
        return Err("입력과 출력이 같습니다".into());
    }
    let selections = match sel {
        Some(v) => v.clone(),
        None => survey(p, source, cfg)?,
    };
    let obj = selections.as_object().ok_or("선택 목록은 객체")?;
    // 명시 선택 목록도 지원 범위를 우회할 수 없다.
    if matches!(p, Processor::Title | Processor::Midtitle) {
        survey(p, source, cfg)?;
    }
    if obj
        .values()
        .all(|v| v.as_array().map_or(false, Vec::is_empty))
    {
        zipio::copy_file_safely(source, target)?;
        return Ok(0);
    }
    let mut contents = zipio::read_hwpx_safely(source)?;
    crate::check_other_xml(&contents)?;
    let (hi, mut h) = crate::parse_header(&contents)?;
    let mut docs = crate::parse_sections(&contents)?;
    let mut cache: HashMap<String, (Doc, usize, Maps)> = HashMap::new();
    let mut count = 0;
    if p == Processor::Title {
        return title_process(source, target, cfg, &selections);
    }
    for (name, items) in obj {
        let (_, ci, d) = docs
            .iter_mut()
            .find(|(n, _, _)| n == name)
            .ok_or("없는 구역")?;
        let tables = top_tables(d);
        for item in items.as_array().ok_or("선택은 배열")? {
            let (index, key) = if p == Processor::BoxStyle {
                (
                    item.as_u64().ok_or("정수 번호 필요")? as usize,
                    "box".to_string(),
                )
            } else if p == Processor::Midtitle {
                let a = item
                    .as_array()
                    .filter(|a| a.len() == 2)
                    .ok_or("중제목 선택 쌍 필요")?;
                let i = a[0].as_u64().ok_or("정수 번호 필요")? as usize;
                let t = *tables.get(i).ok_or("표 범위 초과")?;
                if mid_kind(d, t)? != a[1] {
                    return Err("중제목 구조 변경".into());
                }
                (
                    i,
                    if a[1] == json!("fit") {
                        "fit".into()
                    } else {
                        format!("midtitle{}", a[1].as_u64().ok_or("유형 번호 필요")?)
                    },
                )
            } else {
                let pair = item
                    .as_array()
                    .filter(|a| a.len() == 2)
                    .ok_or("붙임 선택은 번호/유형 쌍")?;
                let index = pair[0].as_u64().ok_or("정수 번호 필요")? as usize;
                let kind = pair[1].as_u64().ok_or("정수 유형 필요")?;
                let t = *tables.get(index).ok_or("표 범위 초과")?;
                if rules::attach_kind(d, t)?.map(u64::from) != Some(kind) {
                    return Err("붙임 구조 변경".into());
                }
                (index, format!("attach{kind}"))
            };
            let t = *tables.get(index).ok_or("표 범위 초과")?;
            if key == "fit" {
                mid_fit(
                    d,
                    t,
                    &h,
                    crate::tbl::section_width(d, Some(42520))?.unwrap_or(42520),
                )?;
                count += 1;
                continue;
            }
            if !cache.contains_key(&key) {
                if let Some((sh, sd, st)) = template(cfg, &key)? {
                    let maps = merge(&mut h, &sh)?;
                    cache.insert(key.clone(), (sd, st, maps));
                } else if p == Processor::Attachment {
                    if !cache.contains_key("builtin") {
                        let (sh, sd, _) = builtin("overview", 0)?;
                        let maps = merge(&mut h, &sh)?;
                        cache.insert("builtin".into(), (sd, 0, maps));
                    }
                    let (sd, _, maps) = cache["builtin"].clone();
                    let ordinal = key
                        .trim_start_matches("attach")
                        .parse::<usize>()
                        .map_err(|e| e.to_string())?;
                    let st = *sd.tables().get(ordinal).ok_or("내장 붙임 없음")?;
                    cache.insert(key.clone(), (sd, st, maps));
                } else if p == Processor::Midtitle {
                    let sample = template(cfg, "midtitle1")?;
                    if let Some((sh, sd, st)) = sample {
                        let maps = merge(&mut h, &sh)?;
                        cache.insert(key.clone(), (sd, st, maps));
                    } else {
                        if !cache.contains_key("builtin") {
                            let (sh, sd, st) = builtin("midtitle", 0)?;
                            let maps = merge(&mut h, &sh)?;
                            if !cfg.midtitle_number_bold {
                                if let Some(id) =
                                    maps.get("charProperties").and_then(|m| m.get("8"))
                                {
                                    let group = find(&h, "charProperties")?;
                                    if let Some(cp) = h.nodes[group]
                                        .children
                                        .iter()
                                        .copied()
                                        .find(|&n| h.attr(n, "id") == Some(id))
                                    {
                                        for n in h.nodes[cp].children.clone() {
                                            if h.tag(n) == "bold" {
                                                h.remove_child(cp, n);
                                            }
                                        }
                                    }
                                }
                            }
                            cache.insert("builtin".into(), (sd, st, maps));
                        }
                        cache.insert(key.clone(), cache["builtin"].clone());
                    }
                } else {
                    zipio::copy_file_safely(source, target)?;
                    return Ok(0);
                }
            }
            let before = d.jstr_walk(t);
            let (sd, st, maps) = &cache[&key];
            if p == Processor::Midtitle {
                let kind = key
                    .trim_start_matches("midtitle")
                    .parse::<u64>()
                    .map_err(|e| e.to_string())?;
                mid_copy(d, t, sd, *st, maps, kind)?;
                mid_fit(
                    d,
                    t,
                    &h,
                    crate::tbl::section_width(d, Some(42520))?.unwrap_or(42520),
                )?;
            } else {
                copy_style(d, t, sd, *st, maps, p == Processor::BoxStyle)?;
            }
            if before != d.jstr_walk(t) {
                return Err("텍스트 보존 검사 실패".into());
            }
            count += 1;
        }
        contents[*ci].1 = d.tostring();
    }
    contents[hi].1 = h.tostring();
    zipio::write_hwpx_safely(&contents, target)?;
    Ok(count)
}

// 제목 유형 1~3과 그 직후 개요. 1×1/장식 제목 판별은 Python에 남긴다.
fn title_targets(d: &Doc) -> R<Value> {
    let ts = top_tables(d);
    let mut out = vec![];
    let mut previous_title = false;
    let mut previous_host = None;
    let hosts: HashMap<_, _> = d.nodes[0]
        .children
        .iter()
        .enumerate()
        .flat_map(|(i, &p)| {
            d.walk(p)
                .into_iter()
                .filter(|n| ts.contains(n))
                .map(move |n| (n, i))
        })
        .collect();
    for (i, &t) in ts.iter().enumerate() {
        if let Some(k) = rules::title_kind_blank(d, t, false)? {
            let text = d.jstr(d.cells(t)[0]);
            let text = rules::py_strip(&text);
            if !text.is_empty() && text.chars().count() <= 160 && !text.starts_with("붙임") {
                out.push(json!([i, k]));
                previous_title = true;
                previous_host = Some(hosts[&t]);
                continue;
            }
        }
        if previous_title && rules::format_kind(d, t)? == Some(FormatKind::Box) {
            let start = previous_host.unwrap();
            let end = hosts[&t];
            if d.attr(d.nodes[0].children[end], "pageBreak") == Some("1")
                || (end > start
                    && d.nodes[0].children[start + 1..end]
                        .iter()
                        .any(|&p| !blank(d, p) || d.attr(p, "pageBreak") == Some("1")))
            {
                return unsupported("제목과 개요 사이 본문·쪽 나눔이 있는 경우는 미확인");
            }
            previous_title = false;
            continue;
        }
        return unsupported("제목 유형4·5 또는 다른 표가 섞인 문서의 제목 판별은 미확인");
    }
    Ok(Value::Array(out))
}
fn fit_width(d: &mut Doc, t: usize, width: Option<i64>, proportional: bool) -> R<()> {
    let Some(width) = width.filter(|w| *w != 0) else {
        return Ok(());
    };
    let Some(sz) = d.child(t, "sz") else {
        return Ok(());
    };
    let outer = d.child(t, "outMargin");
    let outside = if let Some(n) = outer {
        crate::tbl::int_or(d.attr(n, "left"), 0)? + crate::tbl::int_or(d.attr(n, "right"), 0)?
    } else {
        0
    };
    let old = crate::tbl::int_or(d.attr(sz, "width"), 0)?;
    let target = width - outside;
    if target <= 0 || old <= 0 || target == old {
        return Ok(());
    }
    let merged = d.cells(t).iter().any(|&c| {
        d.child(c, "cellSpan")
            .map_or(false, |s| d.attr(s, "rowSpan").unwrap_or("1") != "1")
    });
    for row in d.nodes[t].children.clone() {
        if d.tag(row) != "tr" {
            continue;
        }
        let sizes: Vec<_> = d.nodes[row]
            .children
            .iter()
            .filter(|&&c| d.tag(c) == "tc")
            .map(|&c| d.child(c, "cellSz"))
            .collect();
        if sizes.is_empty() || sizes.iter().any(Option::is_none) {
            continue;
        }
        let sizes: Vec<_> = sizes.into_iter().map(Option::unwrap).collect();
        let ws: Vec<i64> = sizes
            .iter()
            .map(|&s| crate::tbl::int_or(d.attr(s, "width"), 0))
            .collect::<R<_>>()?;
        let sum: i64 = ws.iter().sum();
        let widest = ws
            .iter()
            .enumerate()
            .max_by_key(|&(i, w)| (*w, std::cmp::Reverse(i)))
            .unwrap()
            .0;
        let new = ws[widest] + target - sum;
        if proportional && !merged && sum > 0 {
            let mut nw: Vec<i64> = ws
                .iter()
                .map(|&w| 1.max((w as f64 * target as f64 / sum as f64).round_ties_even() as i64))
                .collect();
            let delta = target - nw.iter().sum::<i64>();
            *nw.last_mut().unwrap() += delta;
            for (&s, w) in sizes.iter().zip(nw) {
                d.set_attr(s, "width", &w.to_string());
            }
        } else if merged || new < 1000 {
            for (&s, w) in sizes.iter().zip(ws) {
                d.set_attr(
                    s,
                    "width",
                    &1.max((w as f64 * target as f64 / old as f64).round_ties_even() as i64)
                        .to_string(),
                );
            }
        } else {
            d.set_attr(sizes[widest], "width", &new.to_string());
        }
    }
    d.set_attr(sz, "width", &target.to_string());
    Ok(())
}
fn reduced_char(
    h: &mut Doc,
    old: &str,
    height: i64,
    cache: &mut HashMap<String, String>,
) -> R<String> {
    if let Some(id) = cache.get(old) {
        return Ok(id.clone());
    }
    let group = find(h, "charProperties")?;
    let Some(src) = h.nodes[group]
        .children
        .iter()
        .copied()
        .find(|&n| h.attr(n, "id") == Some(old))
    else {
        return Ok(old.into());
    };
    let id = next_id(h, group)?;
    let n = h.clone_subtree(src);
    h.set_attr(n, "id", &id);
    h.set_attr(n, "height", &height.to_string());
    h.append_child(group, n);
    h.set_attr(group, "itemCnt", &h.nodes[group].children.len().to_string());
    cache.insert(old.into(), id.clone());
    Ok(id)
}
fn title_shrink(d: &mut Doc, t: usize, h: &mut Doc, cfg: &Config) -> R<()> {
    if !cfg.title_parenthesis_reduce {
        return Ok(());
    }
    let Some(&cell) = d.cells(t).first() else {
        return Ok(());
    };
    let mut cache = HashMap::new();
    let pattern = regex::Regex::new(r"\([^()]+\)").unwrap();
    for p in d.cell_paras(cell) {
        for r in d.nodes[p].children.clone() {
            if d.tag(r) != "run" || d.nodes[r].children.len() != 1 {
                continue;
            }
            let nt = d.nodes[r].children[0];
            if d.tag(nt) != "t" {
                continue;
            }
            let text = d.nodes[nt].text.clone().unwrap_or_default();
            let spans: Vec<_> = pattern
                .find_iter(&text)
                .map(|m| (m.start(), m.end()))
                .collect();
            if spans.is_empty() {
                continue;
            }
            let old = value(d, r, "charPrIDRef")?;
            let group = find(h, "charProperties")?;
            let cp = h.nodes[group]
                .children
                .iter()
                .copied()
                .find(|&n| h.attr(n, "id") == Some(&old));
            let new = if let Some(cp) = cp {
                let height = crate::tbl::int_or(h.attr(cp, "height"), 1000)?;
                let reduce = if cfg.title_parenthesis_pt == 0.0 {
                    2.0
                } else {
                    cfg.title_parenthesis_pt
                };
                reduced_char(
                    h,
                    &old,
                    100.max(height - (reduce * 100.0).round_ties_even() as i64),
                    &mut cache,
                )?
            } else {
                old.clone()
            };
            let mut at = d.index_of(p, r).unwrap();
            d.remove_child(p, r);
            let mut end = 0;
            let mut pieces = vec![];
            for (a, b) in spans {
                if a > end {
                    pieces.push((&text[end..a], false));
                }
                pieces.push((&text[a..b], true));
                end = b;
            }
            if end < text.len() {
                pieces.push((&text[end..], false));
            }
            for (part, reduce) in pieces {
                let nr = d.new_elem(&d.nodes[r].qname.clone(), d.nodes[r].attrs.clone());
                d.set_attr(nr, "charPrIDRef", if reduce { &new } else { &old });
                let n = d.new_elem(&d.nodes[nt].qname.clone(), d.nodes[nt].attrs.clone());
                d.nodes[n].text = Some(part.into());
                d.append_child(nr, n);
                d.insert_child(p, at, nr);
                at += 1;
            }
        }
    }
    Ok(())
}
fn blank(d: &Doc, p: usize) -> bool {
    d.tag(p) == "p"
        && d.nodes[p].children.iter().all(|&r| {
            d.tag(r) == "linesegarray"
                || (d.tag(r) == "run"
                    && d.nodes[r].children.iter().all(|&t| {
                        d.tag(t) == "t"
                            && d.nodes[t].children.is_empty()
                            && rules::py_strip(d.nodes[t].text.as_deref().unwrap_or("")).is_empty()
                    }))
        })
}
fn title_process(source: &Path, target: &Path, cfg: &Config, sel: &Value) -> R<usize> {
    let mut contents = zipio::read_hwpx_safely(source)?;
    let (hi, mut h) = crate::parse_header(&contents)?;
    let mut docs = crate::parse_sections(&contents)?;
    let mut cache: HashMap<String, (Doc, usize, Maps, bool)> = HashMap::new();
    let mut count = 0;
    for (name, items) in sel.as_object().ok_or("선택 객체 필요")? {
        let (_, ci, d) = docs
            .iter_mut()
            .find(|(n, _, _)| n == name)
            .ok_or("없는 구역")?;
        let tables = top_tables(d);
        let width = crate::tbl::section_width(d, None)?;
        let hosts: HashMap<_, _> = d.nodes[0]
            .children
            .iter()
            .flat_map(|&p| {
                d.walk(p)
                    .into_iter()
                    .filter(|n| tables.contains(n))
                    .map(move |t| (t, p))
            })
            .collect();
        for item in items.as_array().ok_or("선택 배열 필요")? {
            let a = item
                .as_array()
                .filter(|a| a.len() == 2)
                .ok_or("선택 쌍 필요")?;
            let index = a[0].as_u64().ok_or("번호 필요")? as usize;
            let kind = a[1].as_u64().ok_or("유형 필요")?;
            let t = *tables.get(index).ok_or("범위 초과")?;
            if rules::title_kind_blank(d, t, false)?.map(u64::from) != Some(kind) {
                return Err("제목 구조 변경".into());
            }
            let mut end_host = hosts[&t];
            let has_overview = index + 1 < tables.len()
                && rules::format_kind(d, tables[index + 1])? == Some(FormatKind::Box);
            for (table, key) in
                std::iter::once((t, format!("title{kind}"))).chain(if has_overview {
                    Some((tables[index + 1], "overview".into()))
                } else {
                    None
                })
            {
                if !cache.contains_key(&key) {
                    if let Some((sh, sd, st)) = template(cfg, &key)? {
                        let maps = merge(&mut h, &sh)?;
                        cache.insert(key.clone(), (sd, st, maps, true));
                    } else {
                        let group = if key == "title1" || key == "title2" {
                            "title"
                        } else if key == "title3" {
                            "title3"
                        } else {
                            "overview"
                        };
                        let ck = format!("builtin:{group}");
                        if !cache.contains_key(&ck) {
                            let (sh, sd, st) = builtin(group, 0)?;
                            let maps = merge(&mut h, &sh)?;
                            cache.insert(ck.clone(), (sd, st, maps, false));
                        }
                        let (sd, _, maps, _) = cache[&ck].clone();
                        let st = sd.tables()[if key == "title2" { 1 } else { 0 }];
                        cache.insert(key.clone(), (sd, st, maps, false));
                    }
                }
                let (sd, st, maps, custom) = &cache[&key];
                let before = d.jstr_walk(table);
                copy_style(d, table, sd, *st, maps, false)?;
                fit_width(d, table, width, *custom)?;
                if key != "overview" {
                    for cell in d.cells(table).into_iter().skip(1) {
                        let text = d.jstr_walk(cell);
                        if text.chars().count() <= 40 && date_match(&text) {
                            if let Some(sub) = d.child(cell, "subList") {
                                d.set_attr(sub, "lineWrap", "SQUEEZE");
                            }
                        }
                    }
                    if !custom {
                        let pg = find(&h, "paraProperties")?;
                        for p in nonempty_paras(d, d.cells(table)[0]) {
                            let id = value(d, p, "paraPrIDRef")?;
                            if let Some(cp) = h.nodes[pg]
                                .children
                                .iter()
                                .copied()
                                .find(|&n| h.attr(n, "id") == Some(&id))
                            {
                                if let Some(align) = h.child(cp, "align") {
                                    h.set_attr(align, "horizontal", "CENTER");
                                }
                            }
                        }
                    }
                }
                title_shrink(d, table, &mut h, cfg)?;
                if key != "overview" && !*custom {
                    let ps = nonempty_paras(d, d.cells(table)[0]);
                    if ps.len() > 1 {
                        let mut cm = HashMap::new();
                        let height = ((if cfg.title_subtitle_pt == 0.0 {
                            15.0
                        } else {
                            cfg.title_subtitle_pt
                        }) * 100.0)
                            .round_ties_even() as i64;
                        for r in d.nodes[ps[0]].children.clone() {
                            if d.tag(r) == "run" {
                                let old = value(d, r, "charPrIDRef")?;
                                let cg = find(&h, "charProperties")?;
                                if h.nodes[cg].children.iter().any(|&c| {
                                    h.attr(c, "id") == Some(&old)
                                        && h.attr(c, "height") != Some(&height.to_string())
                                }) {
                                    let new = reduced_char(&mut h, &old, height, &mut cm)?;
                                    d.set_attr(r, "charPrIDRef", &new);
                                }
                            }
                        }
                    }
                }
                if before != d.jstr_walk(table) {
                    return Err("제목 텍스트 보존 실패".into());
                }
                if key == "overview" {
                    let a = d.index_of(0, end_host).unwrap();
                    let b = d.index_of(0, hosts[&table]).unwrap();
                    if a < b && d.nodes[0].children[a + 1..b].iter().all(|&p| blank(d, p)) {
                        for p in d.nodes[0].children[a + 1..b].to_vec() {
                            d.remove_child(0, p);
                        }
                    }
                    end_host = hosts[&table];
                }
            }
            let start = d.index_of(0, end_host).unwrap() + 1;
            let mut end = start;
            while end < d.nodes[0].children.len() && blank(d, d.nodes[0].children[end]) {
                end += 1;
            }
            if end > start && end < d.nodes[0].children.len() {
                let text = rules::py_strip(&d.jstr_walk(d.nodes[0].children[end])).to_owned();
                let first = text.chars().next();
                if matches!(first, Some('□' | 'ㅁ' | 'ㅇ' | '○' | '-' | '※' | '*')) {
                    for p in d.nodes[0].children[start..end].to_vec() {
                        d.remove_child(0, p);
                    }
                } else if crate::marker::classify_marker(&text).is_some() {
                    return unsupported("사용자 기호 앞 빈 줄 정리는 미확인");
                }
            }
            count += 1;
        }
        contents[*ci].1 = d.tostring();
    }
    contents[hi].1 = h.tostring();
    zipio::write_hwpx_safely(&contents, target)?;
    Ok(count)
}
fn date_match(text: &str) -> bool {
    regex::Regex::new(r"[’'‘`]?\d{2,4}\s*[.년/\-]\s*\d{1,2}\s*[.월/\-]\s*\d{1,2}|\d{1,2}\s*[.월]\s*\d{1,2}\s*[.일]?\s*\(\s*[월화수목금토일]\s*\)").unwrap().is_match(text)
}
