use serde_json::{Map, Value};

fn local(v: &Value) -> Option<&str> {
    v.as_str().map(crate::style_inventory::tag::tag)
}

fn first<'a>(node: &'a Value, name: &str) -> Option<&'a Value> {
    if local(&node["tag"]) == Some(name) {
        return Some(node);
    }
    node.get("children")?
        .as_array()?
        .iter()
        .find_map(|c| first(c, name))
}

fn attr(n: &Value, k: &str, d: Value) -> Value {
    n.get("attrib")
        .and_then(|a| a.get(k))
        .cloned()
        .unwrap_or(d)
}

pub fn parse_border_fill(e: &Value) -> Value {
    let mut sides = Map::new();
    let border_keys = [
        ("leftBorder", "left"),
        ("rightBorder", "right"),
        ("topBorder", "top"),
        ("bottomBorder", "bottom"),
        ("diagonal", "diagonal"),
    ];
    for (tag_name, key) in border_keys {
        if let Some(x) = first(e, tag_name) {
            let mut side_info = Map::new();
            side_info.insert("type".into(), attr(x, "type", Value::Null));
            side_info.insert("width".into(), attr(x, "width", Value::Null));
            side_info.insert("color".into(), attr(x, "color", Value::Null));
            sides.insert(key.into(), Value::Object(side_info));
        }
    }

    let win = first(e, "winBrush");
    let grad = first(e, "gradation");

    let fill = if let Some(w) = win {
        let face_color = w.get("attrib").and_then(|a| a.get("faceColor"));
        let face_str = face_color.and_then(Value::as_str);
        if let Some(fc) = face_str {
            if fc != "none" {
                Some(serde_json::json!({
                    "kind": "단색",
                    "color": fc
                }))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let fill = match fill {
        Some(f) => Some(f),
        None => {
            if let Some(g) = grad {
                let mut colors = Vec::new();
                if let Some(children) = g.get("children").and_then(Value::as_array) {
                    for c in children {
                        if local(&c["tag"]) == Some("color") {
                            colors.push(attr(c, "value", Value::Null));
                        }
                    }
                }
                Some(serde_json::json!({
                    "kind": "그러데이션",
                    "colors": colors
                }))
            } else {
                None
            }
        }
    };

    let mut out = Map::new();
    out.insert("id".into(), attr(e, "id", Value::Null));
    out.insert("borders".into(), Value::Object(sides));
    out.insert("fill".into(), fill.unwrap_or(Value::Null));
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_borders_and_solid_fill() {
        let tree = json!({
            "tag": "{http://www.hancom.co.kr/hwpml/2011/head}borderFill",
            "attrib": {"id": "1"},
            "children": [
                {
                    "tag": "{http://www.hancom.co.kr/hwpml/2011/head}leftBorder",
                    "attrib": {"type": "SOLID", "width": "0.1 mm", "color": "#000000"}
                },
                {
                    "tag": "{http://www.hancom.co.kr/hwpml/2011/head}winBrush",
                    "attrib": {"faceColor": "#FF0000"}
                }
            ]
        });
        let res = parse_border_fill(&tree);
        assert_eq!(res["id"], "1");
        assert_eq!(res["borders"]["left"]["type"], "SOLID");
        assert_eq!(res["fill"]["kind"], "단색");
        assert_eq!(res["fill"]["color"], "#FF0000");
    }

    #[test]
    fn parses_gradation_fill_when_winbrush_none() {
        let tree = json!({
            "tag": "{http://www.hancom.co.kr/hwpml/2011/head}borderFill",
            "attrib": {"id": "2"},
            "children": [
                {
                    "tag": "{http://www.hancom.co.kr/hwpml/2011/head}winBrush",
                    "attrib": {"faceColor": "none"}
                },
                {
                    "tag": "{http://www.hancom.co.kr/hwpml/2011/head}gradation",
                    "children": [
                        {"tag": "{http://www.hancom.co.kr/hwpml/2011/head}color", "attrib": {"value": "#000000"}},
                        {"tag": "{http://www.hancom.co.kr/hwpml/2011/head}color", "attrib": {"value": "#FFFFFF"}}
                    ]
                }
            ]
        });
        let res = parse_border_fill(&tree);
        assert_eq!(res["fill"]["kind"], "그러데이션");
        assert_eq!(res["fill"]["colors"], json!(["#000000", "#FFFFFF"]));
    }
}
