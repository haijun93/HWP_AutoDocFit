use serde_json::{Map, Value};

fn local(v: &Value) -> Option<&str> {
    v.as_str().map(crate::style_inventory::tag::tag)
}

fn attr(n: &Value, k: &str, d: Value) -> Value {
    n.get("attrib")
        .and_then(|a| a.get(k))
        .cloned()
        .unwrap_or(d)
}

fn id_key(n: &Value) -> String {
    n.get("attrib")
        .and_then(|a| a.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

struct HeaderDefs {
    chars: Map<String, Value>,
    paras: Map<String, Value>,
    styles: Map<String, Value>,
    border_fills: Map<String, Value>,
    numberings: Map<String, Value>,
    bullets: Map<String, Value>,
    tabs: Map<String, Value>,
}

fn walk_header(node: &Value, fonts: &Value, defs: &mut HeaderDefs) {
    if let Some(tag_name) = local(&node["tag"]) {
        match tag_name {
            "charPr" => {
                let id = id_key(node);
                let parsed = crate::style_inventory::parse_char::parse_char(node, fonts);
                defs.chars.insert(id, parsed);
            }
            "paraPr" => {
                let id = id_key(node);
                let parsed = crate::style_inventory::parse_para::parse_para(node);
                defs.paras.insert(id, parsed);
            }
            "style" => {
                let id = id_key(node);
                let mut style_obj = Map::new();
                style_obj.insert("id".into(), attr(node, "id", Value::Null));
                style_obj.insert("type".into(), attr(node, "type", Value::Null));
                style_obj.insert("name".into(), attr(node, "name", Value::Null));
                style_obj.insert("eng_name".into(), attr(node, "engName", Value::Null));
                style_obj.insert("para".into(), attr(node, "paraPrIDRef", Value::Null));
                style_obj.insert("char".into(), attr(node, "charPrIDRef", Value::Null));
                style_obj.insert("next".into(), attr(node, "nextStyleIDRef", Value::Null));
                defs.styles.insert(id, Value::Object(style_obj));
            }
            "borderFill" => {
                let id = id_key(node);
                let parsed = crate::style_inventory::parse_border_fill::parse_border_fill(node);
                defs.border_fills.insert(id, parsed);
            }
            "numbering" => {
                let id = id_key(node);
                let parsed = crate::style_inventory::parse_numbering::parse_numbering(node);
                defs.numberings.insert(id, parsed);
            }
            "bullet" => {
                let id = id_key(node);
                let mut bullet_obj = Map::new();
                bullet_obj.insert("id".into(), attr(node, "id", Value::Null));
                bullet_obj.insert("char".into(), attr(node, "char", Value::Null));
                defs.bullets.insert(id, Value::Object(bullet_obj));
            }
            "tabPr" => {
                let id = id_key(node);
                let parsed = crate::style_inventory::parse_tab::parse_tab(node);
                defs.tabs.insert(id, parsed);
            }
            _ => {}
        }
    }

    if let Some(children) = node.get("children").and_then(Value::as_array) {
        for child in children {
            walk_header(child, fonts, defs);
        }
    }
}

pub fn parse_header(header: &Value) -> Value {
    let fonts = crate::style_inventory::parse_fonts::parse_fonts(header);
    let mut defs = HeaderDefs {
        chars: Map::new(),
        paras: Map::new(),
        styles: Map::new(),
        border_fills: Map::new(),
        numberings: Map::new(),
        bullets: Map::new(),
        tabs: Map::new(),
    };

    walk_header(header, &fonts, &mut defs);

    let mut out = Map::new();
    out.insert("fonts".into(), fonts);
    out.insert("chars".into(), Value::Object(defs.chars));
    out.insert("paras".into(), Value::Object(defs.paras));
    out.insert("styles".into(), Value::Object(defs.styles));
    out.insert("border_fills".into(), Value::Object(defs.border_fills));
    out.insert("numberings".into(), Value::Object(defs.numberings));
    out.insert("bullets".into(), Value::Object(defs.bullets));
    out.insert("tabs".into(), Value::Object(defs.tabs));
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_header_comprehensive() {
        let tree = json!({
            "tag": "{http://www.hancom.co.kr/hwpml/2011/head}head",
            "children": [
                {
                    "tag": "{http://www.hancom.co.kr/hwpml/2011/head}fontfaces",
                    "children": [
                        {
                            "tag": "{http://www.hancom.co.kr/hwpml/2011/head}fontface",
                            "attrib": {"lang": "hangul"},
                            "children": [
                                {
                                    "tag": "{http://www.hancom.co.kr/hwpml/2011/head}font",
                                    "attrib": {"id": "0", "face": "함초롬바탕"}
                                }
                            ]
                        }
                    ]
                },
                {
                    "tag": "{http://www.hancom.co.kr/hwpml/2011/head}charProperties",
                    "children": [
                        {
                            "tag": "{http://www.hancom.co.kr/hwpml/2011/head}charPr",
                            "attrib": {"id": "0", "height": "1000", "textColor": "#000000"}
                        }
                    ]
                },
                {
                    "tag": "{http://www.hancom.co.kr/hwpml/2011/head}paraProperties",
                    "children": [
                        {
                            "tag": "{http://www.hancom.co.kr/hwpml/2011/head}paraPr",
                            "attrib": {"id": "0"}
                        }
                    ]
                },
                {
                    "tag": "{http://www.hancom.co.kr/hwpml/2011/head}styles",
                    "children": [
                        {
                            "tag": "{http://www.hancom.co.kr/hwpml/2011/head}style",
                            "attrib": {
                                "id": "0",
                                "type": "PARA",
                                "name": "바탕글",
                                "engName": "Normal",
                                "paraPrIDRef": "0",
                                "charPrIDRef": "0",
                                "nextStyleIDRef": "0"
                            }
                        }
                    ]
                },
                {
                    "tag": "{http://www.hancom.co.kr/hwpml/2011/head}borderFills",
                    "children": [
                        {
                            "tag": "{http://www.hancom.co.kr/hwpml/2011/head}borderFill",
                            "attrib": {"id": "1"}
                        }
                    ]
                },
                {
                    "tag": "{http://www.hancom.co.kr/hwpml/2011/head}numberings",
                    "children": [
                        {
                            "tag": "{http://www.hancom.co.kr/hwpml/2011/head}numbering",
                            "attrib": {"id": "1", "start": "1"}
                        }
                    ]
                },
                {
                    "tag": "{http://www.hancom.co.kr/hwpml/2011/head}bullets",
                    "children": [
                        {
                            "tag": "{http://www.hancom.co.kr/hwpml/2011/head}bullet",
                            "attrib": {"id": "1", "char": "•"}
                        }
                    ]
                },
                {
                    "tag": "{http://www.hancom.co.kr/hwpml/2011/head}tabProperties",
                    "children": [
                        {
                            "tag": "{http://www.hancom.co.kr/hwpml/2011/head}tabPr",
                            "attrib": {"id": "0"}
                        }
                    ]
                }
            ]
        });

        let res = parse_header(&tree);
        assert!(res.get("fonts").is_some());
        assert_eq!(res["fonts"]["hangul"]["0"], "함초롬바탕");
        assert!(res.get("chars").unwrap().get("0").is_some());
        assert!(res.get("paras").unwrap().get("0").is_some());
        assert_eq!(res["styles"]["0"]["name"], "바탕글");
        assert_eq!(res["styles"]["0"]["eng_name"], "Normal");
        assert!(res.get("border_fills").unwrap().get("1").is_some());
        assert!(res.get("numberings").unwrap().get("1").is_some());
        assert_eq!(res["bullets"]["1"]["char"], "•");
        assert!(res.get("tabs").unwrap().get("0").is_some());
    }
}
