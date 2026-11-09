use serde_json::Value;

/// Filters direct run children of a paragraph, matching style_inventory._paragraph_runs.
///
/// Python reference:
/// def _paragraph_runs(para):
///     return [run for run in para if _tag(run) == "run"]
pub fn paragraph_runs(para: &Value) -> Value {
    let mut runs = Vec::new();
    if let Some(children) = para.get("children").and_then(Value::as_array) {
        for child in children {
            if child
                .get("tag")
                .and_then(Value::as_str)
                .map(crate::style_inventory::tag::tag)
                == Some("run")
            {
                runs.push(child.clone());
            }
        }
    }
    Value::Array(runs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_direct_run_children() {
        let para = json!({
            "tag": "hp:p",
            "children": [
                {"tag": "hp:run", "attrib": {"charPrIDRef": "0"}, "children": []},
                {"tag": "hp:linesegarray", "children": []},
                {"tag": "{http://www.hancom.co.kr/hwpml/2011/paragraph}run", "attrib": {"charPrIDRef": "1"}, "children": []}
            ]
        });
        let result = paragraph_runs(&para);
        assert_eq!(
            result,
            json!([
                {"tag": "hp:run", "attrib": {"charPrIDRef": "0"}, "children": []},
                {"tag": "{http://www.hancom.co.kr/hwpml/2011/paragraph}run", "attrib": {"charPrIDRef": "1"}, "children": []}
            ])
        );
    }

    #[test]
    fn returns_empty_for_no_runs() {
        let para = json!({
            "tag": "hp:p",
            "children": [
                {"tag": "hp:linesegarray", "children": []}
            ]
        });
        assert_eq!(paragraph_runs(&para), json!([]));
    }
}
