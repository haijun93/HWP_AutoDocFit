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

fn int_attr(n: &Value, k: &str, d: i64) -> Value {
    n.get("attrib")
        .and_then(|a| a.get(k))
        .and_then(Value::as_str)
        .unwrap_or("")
        .parse::<i64>()
        .map(Value::from)
        .unwrap_or(Value::from(d))
}

pub fn parse_numbering(e: &Value) -> Value {
    let mut levels = Vec::new();
    if let Some(children) = e.get("children").and_then(Value::as_array) {
        for head in children {
            if local(&head["tag"]) == Some("paraHead") {
                let text_val = head
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let mut level_obj = Map::new();
                level_obj.insert("level".into(), int_attr(head, "level", 0));
                level_obj.insert("format".into(), attr(head, "numFormat", Value::Null));
                level_obj.insert("text".into(), Value::String(text_val.to_string()));
                level_obj.insert("start".into(), attr(head, "start", Value::Null));
                levels.push(Value::Object(level_obj));
            }
        }
    }

    let mut out = Map::new();
    out.insert("id".into(), attr(e, "id", Value::Null));
    out.insert("start".into(), attr(e, "start", Value::Null));
    out.insert("levels".into(), Value::Array(levels));
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_numbering_levels() {
        let tree = json!({
            "tag": "{http://www.hancom.co.kr/hwpml/2011/head}numbering",
            "attrib": {"id": "1", "start": "1"},
            "children": [
                {
                    "tag": "{http://www.hancom.co.kr/hwpml/2011/head}paraHead",
                    "attrib": {"level": "1", "numFormat": "DIGIT", "start": "1"},
                    "text": "^1."
                },
                {
                    "tag": "{http://www.hancom.co.kr/hwpml/2011/head}paraHead",
                    "attrib": {"level": "2", "numFormat": "HANGUL_SYLLABLE"},
                    "text": null
                }
            ]
        });
        let res = parse_numbering(&tree);
        assert_eq!(res["id"], "1");
        assert_eq!(res["start"], "1");
        assert_eq!(res["levels"].as_array().unwrap().len(), 2);
        assert_eq!(res["levels"][0]["level"], 1);
        assert_eq!(res["levels"][0]["format"], "DIGIT");
        assert_eq!(res["levels"][0]["text"], "^1.");
        assert_eq!(res["levels"][1]["level"], 2);
        assert_eq!(res["levels"][1]["text"], "");
        assert_eq!(res["levels"][1]["start"], Value::Null);
    }
}
