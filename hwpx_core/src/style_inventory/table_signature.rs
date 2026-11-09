use serde_json::{json, Value};
use std::collections::BTreeSet;

fn get_attr<'a>(node: &'a Value, name: &str) -> Option<&'a str> {
    node.get("attrib")
        .and_then(|a| a.get(name))
        .or_else(|| node.get(name))
        .and_then(Value::as_str)
}

fn iter_tag<'a>(node: &'a Value, target: &str, out: &mut Vec<&'a Value>) {
    if node
        .get("tag")
        .and_then(Value::as_str)
        .map(crate::style_inventory::tag::tag)
        == Some(target)
    {
        out.push(node);
    }
    if let Some(children) = node.get("children").and_then(Value::as_array) {
        for child in children {
            iter_tag(child, target, out);
        }
    }
}

fn get_look(cid: Option<&str>, chars: Option<&Value>) -> Value {
    if let (Some(cid_str), Some(chars_obj)) = (cid, chars) {
        if let Some(c) = chars_obj.get(cid_str) {
            if !c.is_null() {
                let hangul = c
                    .get("font")
                    .and_then(|f| f.get("hangul"))
                    .cloned()
                    .unwrap_or(Value::Null);
                let size_pt = c.get("size_pt").cloned().unwrap_or(Value::Null);
                let bold = c.get("bold").cloned().unwrap_or(Value::Null);
                let color = c
                    .get("color")
                    .and_then(Value::as_str)
                    .map(|s| s.to_uppercase())
                    .map(Value::String)
                    .unwrap_or(Value::Null);
                return json!([hangul, size_pt, bold, color]);
            }
        }
    }
    match cid {
        Some(s) => Value::String(s.to_string()),
        None => Value::Null,
    }
}

/// Determine table signature based on dimensions, borders, and first row appearance,
/// matching style_inventory._table_signature.
///
/// Python reference:
/// def _table_signature(tbl, chars=None):
///     chars = chars or {}
///     cells = [c for c in tbl.iter() if _tag(c) == "tc"]
///     rows = [r for r in tbl.iter() if _tag(r) == "tr"]
///     first_row = [c for c in rows[0] if _tag(c) == "tc"] if rows else []
///     def look(cid):
///         c = chars.get(cid)
///         return (c["font"].get("hangul"), c["size_pt"], c["bold"], c["color"].upper()) if c else cid
///     looks = Counter()
///     for r in (r for c in first_row for r in c.iter() if _tag(r) == "run"):
///         looks[look(r.get("charPrIDRef"))] += len(_run_text(r).strip())
///     first_row_chars = _mode(+looks)
///     cell_fills = tuple(sorted({c.get("borderFillIDRef") for c in cells}))
///     sz = _first(tbl, "sz")
///     wide = sz is not None and _mm(sz.get("width")) > 120
///     return (tbl.get("colCnt"), tbl.get("borderFillIDRef"), cell_fills, first_row_chars, wide)
pub fn table_signature(tbl: &Value, chars: Option<&Value>) -> Value {
    let mut cells = Vec::new();
    iter_tag(tbl, "tc", &mut cells);

    let mut rows = Vec::new();
    iter_tag(tbl, "tr", &mut rows);

    let mut first_row = Vec::new();
    if let Some(first_tr) = rows.first() {
        if let Some(children) = first_tr.get("children").and_then(Value::as_array) {
            for child in children {
                if child
                    .get("tag")
                    .and_then(Value::as_str)
                    .map(crate::style_inventory::tag::tag)
                    == Some("tc")
                {
                    first_row.push(child);
                }
            }
        }
    }

    let mut looks: Vec<(Value, usize)> = Vec::new();
    for cell in first_row {
        let mut runs = Vec::new();
        iter_tag(cell, "run", &mut runs);
        for r in runs {
            let text = crate::style_inventory::run_text::run_text(r);
            let stripped = text.trim();
            let len = stripped.chars().count();
            if len == 0 {
                continue;
            }
            let cid = get_attr(r, "charPrIDRef");
            let look_val = get_look(cid, chars);
            if let Some(entry) = looks.iter_mut().find(|(k, _)| k == &look_val) {
                entry.1 += len;
            } else {
                looks.push((look_val, len));
            }
        }
    }

    let mut max_count = 0;
    let mut first_row_chars = Value::Null;
    for (val, count) in &looks {
        if *count > max_count {
            max_count = *count;
            first_row_chars = val.clone();
        }
    }

    let mut fill_set: BTreeSet<Option<String>> = BTreeSet::new();
    for c in &cells {
        fill_set.insert(get_attr(c, "borderFillIDRef").map(|s| s.to_string()));
    }
    let cell_fills: Vec<Value> = fill_set
        .into_iter()
        .map(|opt| match opt {
            Some(s) => Value::String(s),
            None => Value::Null,
        })
        .collect();

    let sz = crate::style_inventory::first::first(tbl, "sz");
    let wide = sz
        .map(|s| crate::style_inventory::mm::mm(get_attr(s, "width")) > 120.0)
        .unwrap_or(false);

    let col_cnt = get_attr(tbl, "colCnt");
    let border_fill = get_attr(tbl, "borderFillIDRef");

    json!([col_cnt, border_fill, cell_fills, first_row_chars, wide])
}

pub fn table_signature_from_value(value: &Value) -> Value {
    let tbl = value.get("tbl").unwrap_or(value);
    let chars = value.get("chars");
    table_signature(tbl, chars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_table_signature_basic() {
        let tbl = json!({
            "tag": "hp:tbl",
            "attrib": {"colCnt": "2", "borderFillIDRef": "1"},
            "children": [
                {
                    "tag": "hp:sz",
                    "attrib": {"width": "35000"}
                },
                {
                    "tag": "hp:tr",
                    "children": [
                        {
                            "tag": "hp:tc",
                            "attrib": {"borderFillIDRef": "2"},
                            "children": [
                                {
                                    "tag": "hp:run",
                                    "attrib": {"charPrIDRef": "0"},
                                    "children": [{"tag": "hp:t", "text": "Header 1"}]
                                }
                            ]
                        },
                        {
                            "tag": "hp:tc",
                            "attrib": {"borderFillIDRef": "2"},
                            "children": [
                                {
                                    "tag": "hp:run",
                                    "attrib": {"charPrIDRef": "0"},
                                    "children": [{"tag": "hp:t", "text": "Header 2"}]
                                }
                            ]
                        }
                    ]
                }
            ]
        });

        let chars = json!({
            "0": {
                "font": {"hangul": "함초롬바탕"},
                "size_pt": 10.0,
                "bold": true,
                "color": "#000000"
            }
        });

        let sig = table_signature(&tbl, Some(&chars));
        assert_eq!(
            sig,
            json!([
                "2",
                "1",
                ["2"],
                ["함초롬바탕", 10.0, true, "#000000"],
                true
            ])
        );
    }
}
