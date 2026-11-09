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

fn collect_tab_items(node: &Value, items: &mut Vec<Value>) {
    if local(&node["tag"]) == Some("tabItem") {
        let pos_str = node
            .get("attrib")
            .and_then(|a| a.get("pos"))
            .and_then(Value::as_str);
        let pos_mm = crate::style_inventory::mm::mm(pos_str);
        let mut item = Map::new();
        item.insert("pos_mm".into(), serde_json::json!(pos_mm));
        item.insert("type".into(), attr(node, "type", Value::Null));
        item.insert("leader".into(), attr(node, "leader", Value::Null));
        items.push(Value::Object(item));
    }
    if let Some(children) = node.get("children").and_then(Value::as_array) {
        for child in children {
            collect_tab_items(child, items);
        }
    }
}

pub fn parse_tab(e: &Value) -> Value {
    let mut items = Vec::new();
    collect_tab_items(e, &mut items);

    let auto_left = e
        .get("attrib")
        .and_then(|a| a.get("autoTabLeft"))
        .and_then(Value::as_str)
        == Some("1");
    let auto_right = e
        .get("attrib")
        .and_then(|a| a.get("autoTabRight"))
        .and_then(Value::as_str)
        == Some("1");

    let mut out = Map::new();
    out.insert("id".into(), attr(e, "id", Value::Null));
    out.insert("auto_left".into(), Value::Bool(auto_left));
    out.insert("auto_right".into(), Value::Bool(auto_right));
    out.insert("items".into(), Value::Array(items));
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_tab() {
        let tree = json!({
            "tag": "{http://www.hancom.co.kr/hwpml/2011/head}tabPr",
            "attrib": {
                "id": "0",
                "autoTabLeft": "1",
                "autoTabRight": "0"
            },
            "children": [
                {
                    "tag": "{http://www.hancom.co.kr/hwpml/2011/head}tabItem",
                    "attrib": {
                        "pos": "8000",
                        "type": "LEFT",
                        "leader": "DOT"
                    }
                },
                {
                    "tag": "{http://www.hancom.co.kr/hwpml/2011/head}tabItem",
                    "attrib": {
                        "pos": "16000",
                        "type": "RIGHT"
                    }
                }
            ]
        });

        let res = parse_tab(&tree);
        assert_eq!(res["id"], "0");
        assert_eq!(res["auto_left"], true);
        assert_eq!(res["auto_right"], false);
        let items = res["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["pos_mm"], 28.2);
        assert_eq!(items[0]["type"], "LEFT");
        assert_eq!(items[0]["leader"], "DOT");
        assert_eq!(items[1]["pos_mm"], 56.4);
        assert_eq!(items[1]["type"], "RIGHT");
        assert_eq!(items[1]["leader"], Value::Null);
    }
}
