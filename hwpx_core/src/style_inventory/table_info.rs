use serde_json::{json, Map, Value};

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

/// Extract structural information, cell fills, margins, and header/body text stats from a table,
/// matching style_inventory._table_info.
///
/// Python reference:
/// def _table_info(tbl, chars):
///     cells = [c for c in tbl.iter() if _tag(c) == "tc"]
///     rows = [r for r in tbl.iter() if _tag(r) == "tr"]
///     sz = _first(tbl, "sz")
///     cm = Counter()
///     for c in cells:
///         m = next((x for x in c if _tag(x) == "cellMargin"), None)
///         if m is not None:
///             cm[tuple(_mm(m.get(k)) for k in ("left", "right", "top", "bottom"))] += 1
///     first = rows[0] if rows else None
///     head_chars, body_chars = Counter(), Counter()
///     for index, row in enumerate(rows):
///         bucket = head_chars if index == 0 else body_chars
///         for run in (r for r in row.iter() if _tag(r) == "run"):
///             text = _run_text(run).strip()
///             if text:
///                 bucket[run.get("charPrIDRef")] += len(text)
///     return {
///         "rows": int(tbl.get("rowCnt", len(rows)) or len(rows)),
///         "cols": int(tbl.get("colCnt", 0) or 0),
///         "width_mm": _mm(sz.get("width")) if sz is not None else None,
///         "table_border_fill": tbl.get("borderFillIDRef"),
///         "cell_border_fills": dict(Counter(c.get("borderFillIDRef") for c in cells)),
///         "cell_margin_mm": list(cm.most_common(1)[0][0]) if cm else None,
///         "header_chars": [cid for cid, _ in head_chars.most_common()],
///         "body_chars": [cid for cid, _ in body_chars.most_common()],
///         "first_row": " | ".join(_text_of(c).strip() for c in first if _tag(c) == "tc")[:120]
///         if first is not None else "",
///     }
pub fn table_info(tbl: &Value, _chars: Option<&Value>) -> Value {
    let mut cells = Vec::new();
    iter_tag(tbl, "tc", &mut cells);

    let mut rows = Vec::new();
    iter_tag(tbl, "tr", &mut rows);

    let sz = crate::style_inventory::first::first(tbl, "sz");

    let mut cm: Vec<([f64; 4], usize)> = Vec::new();
    for c in &cells {
        let mut margin_node = None;
        if let Some(children) = c.get("children").and_then(Value::as_array) {
            for child in children {
                if child
                    .get("tag")
                    .and_then(Value::as_str)
                    .map(crate::style_inventory::tag::tag)
                    == Some("cellMargin")
                {
                    margin_node = Some(child);
                    break;
                }
            }
        }
        if let Some(m) = margin_node {
            let left = crate::style_inventory::mm::mm(get_attr(m, "left"));
            let right = crate::style_inventory::mm::mm(get_attr(m, "right"));
            let top = crate::style_inventory::mm::mm(get_attr(m, "top"));
            let bottom = crate::style_inventory::mm::mm(get_attr(m, "bottom"));
            let margins = [left, right, top, bottom];

            if let Some(entry) = cm.iter_mut().find(|(k, _)| *k == margins) {
                entry.1 += 1;
            } else {
                cm.push((margins, 1));
            }
        }
    }

    let cell_margin_mm = if cm.is_empty() {
        Value::Null
    } else {
        let mut max_count = 0;
        let mut best = [0.0; 4];
        for (margins, count) in &cm {
            if *count > max_count {
                max_count = *count;
                best = *margins;
            }
        }
        json!([best[0], best[1], best[2], best[3]])
    };

    let first = rows.first();
    let mut head_chars: Vec<(Option<String>, usize)> = Vec::new();
    let mut body_chars: Vec<(Option<String>, usize)> = Vec::new();

    for (index, row) in rows.iter().enumerate() {
        let bucket = if index == 0 {
            &mut head_chars
        } else {
            &mut body_chars
        };
        let mut runs = Vec::new();
        iter_tag(row, "run", &mut runs);
        for r in runs {
            let text = crate::style_inventory::run_text::run_text(r);
            let stripped = text.trim();
            if !stripped.is_empty() {
                let cid = get_attr(r, "charPrIDRef").map(|s| s.to_string());
                let len = stripped.chars().count();
                if let Some(entry) = bucket.iter_mut().find(|(k, _)| k == &cid) {
                    entry.1 += len;
                } else {
                    bucket.push((cid, len));
                }
            }
        }
    }

    head_chars.sort_by(|a, b| b.1.cmp(&a.1));
    body_chars.sort_by(|a, b| b.1.cmp(&a.1));

    let header_chars: Vec<Value> = head_chars
        .into_iter()
        .map(|(cid, _)| match cid {
            Some(s) => Value::String(s),
            None => Value::Null,
        })
        .collect();

    let body_chars_val: Vec<Value> = body_chars
        .into_iter()
        .map(|(cid, _)| match cid {
            Some(s) => Value::String(s),
            None => Value::Null,
        })
        .collect();

    let row_cnt_attr = get_attr(tbl, "rowCnt")
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|&n| n > 0);
    let rows_num = row_cnt_attr.unwrap_or(rows.len() as i64);

    let col_cnt_attr = get_attr(tbl, "colCnt")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);

    let width_mm = sz.map(|s| crate::style_inventory::mm::mm(get_attr(s, "width")));

    let mut cell_border_fills = Map::new();
    for c in &cells {
        let key = get_attr(c, "borderFillIDRef").unwrap_or("null").to_string();
        let val = cell_border_fills.entry(&key).or_insert(json!(0));
        if let Some(n) = val.as_i64() {
            *val = json!(n + 1);
        }
    }

    let first_row = if let Some(first_tr) = first {
        let mut parts = Vec::new();
        if let Some(children) = first_tr.get("children").and_then(Value::as_array) {
            for child in children {
                if child
                    .get("tag")
                    .and_then(Value::as_str)
                    .map(crate::style_inventory::tag::tag)
                    == Some("tc")
                {
                    parts.push(crate::style_inventory::text_of::text_of(child).trim().to_string());
                }
            }
        }
        let joined = parts.join(" | ");
        joined.chars().take(120).collect::<String>()
    } else {
        String::new()
    };

    json!({
        "rows": rows_num,
        "cols": col_cnt_attr,
        "width_mm": width_mm,
        "table_border_fill": get_attr(tbl, "borderFillIDRef"),
        "cell_border_fills": cell_border_fills,
        "cell_margin_mm": cell_margin_mm,
        "header_chars": header_chars,
        "body_chars": body_chars_val,
        "first_row": first_row,
    })
}

pub fn table_info_from_value(value: &Value) -> Value {
    let tbl = value.get("tbl").unwrap_or(value);
    let chars = value.get("chars");
    table_info(tbl, chars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_table_info_basic() {
        let tbl = json!({
            "tag": "hp:tbl",
            "attrib": {"rowCnt": "2", "colCnt": "2", "borderFillIDRef": "10"},
            "children": [
                {
                    "tag": "hp:sz",
                    "attrib": {"width": "28346"}
                },
                {
                    "tag": "hp:tr",
                    "children": [
                        {
                            "tag": "hp:tc",
                            "attrib": {"borderFillIDRef": "1"},
                            "children": [
                                {
                                    "tag": "hp:cellMargin",
                                    "attrib": {"left": "283", "right": "283", "top": "141", "bottom": "141"}
                                },
                                {
                                    "tag": "hp:run",
                                    "attrib": {"charPrIDRef": "0"},
                                    "children": [{"tag": "hp:t", "text": "Header"}]
                                }
                            ]
                        }
                    ]
                },
                {
                    "tag": "hp:tr",
                    "children": [
                        {
                            "tag": "hp:tc",
                            "attrib": {"borderFillIDRef": "1"},
                            "children": [
                                {
                                    "tag": "hp:run",
                                    "attrib": {"charPrIDRef": "1"},
                                    "children": [{"tag": "hp:t", "text": "Body text"}]
                                }
                            ]
                        }
                    ]
                }
            ]
        });

        let info = table_info(&tbl, None);
        assert_eq!(info["rows"], 2);
        assert_eq!(info["cols"], 2);
        assert_eq!(info["width_mm"], 100.0);
        assert_eq!(info["table_border_fill"], "10");
        assert_eq!(info["cell_border_fills"]["1"], 2);
        assert_eq!(info["cell_margin_mm"], json!([1.0, 1.0, 0.5, 0.5]));
        assert_eq!(info["header_chars"], json!(["0"]));
        assert_eq!(info["body_chars"], json!(["1"]));
        assert_eq!(info["first_row"], "Header");
    }
}
