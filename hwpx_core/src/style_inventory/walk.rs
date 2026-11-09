use serde_json::Value;

/// Recursively collects all paragraphs as [child, context] pairs in document order,
/// matching style_inventory._walk.
///
/// Python reference:
/// def _walk(element, context, out):
///     for child in element:
///         name = _tag(child)
///         if name == "p":
///             out.append((child, context))
///             _walk(child, context, out)
///         elif name == "tbl":
///             _walk(child, "표", out)
///         elif name in ("header", "footer"):
///             _walk(child, "머리말·꼬리말", out)
///         elif name in ("footNote", "endNote"):
///             _walk(child, "각주·미주", out)
///         elif name in ("rect", "ellipse", "polygon", "curve", "container", "drawText"):
///             _walk(child, "글상자" if context == "본문" else context, out)
///         else:
///             _walk(child, context, out)
pub fn walk_element(element: &Value, context: &str, out: &mut Vec<Value>) {
    let children = match element.get("children").and_then(Value::as_array) {
        Some(c) => c,
        None => return,
    };

    for child in children {
        let tag_name = child
            .get("tag")
            .and_then(Value::as_str)
            .map(crate::style_inventory::tag::tag);

        match tag_name {
            Some("p") => {
                out.push(serde_json::json!([child.clone(), context]));
                walk_element(child, context, out);
            }
            Some("tbl") => {
                walk_element(child, "표", out);
            }
            Some("header") | Some("footer") => {
                walk_element(child, "머리말·꼬리말", out);
            }
            Some("footNote") | Some("endNote") => {
                walk_element(child, "각주·미주", out);
            }
            Some("rect")
            | Some("ellipse")
            | Some("polygon")
            | Some("curve")
            | Some("container")
            | Some("drawText") => {
                let next_ctx = if context == "본문" { "글상자" } else { context };
                walk_element(child, next_ctx, out);
            }
            _ => {
                walk_element(child, context, out);
            }
        }
    }
}

pub fn walk(element: &Value, context: &str) -> Value {
    let mut out = Vec::new();
    walk_element(element, context, &mut out);
    Value::Array(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn walks_simple_paragraphs_in_body() {
        let tree = json!({
            "tag": "sec",
            "children": [
                {"tag": "{ns}p", "children": []},
                {"tag": "{ns}p", "children": []}
            ]
        });
        let result = walk(&tree, "본문");
        assert_eq!(
            result,
            json!([
                [{"tag": "{ns}p", "children": []}, "본문"],
                [{"tag": "{ns}p", "children": []}, "본문"]
            ])
        );
    }

    #[test]
    fn handles_context_transitions() {
        let tree = json!({
            "tag": "sec",
            "children": [
                {"tag": "tbl", "children": [
                    {"tag": "tr", "children": [
                        {"tag": "tc", "children": [
                            {"tag": "p", "children": []}
                        ]}
                    ]}
                ]},
                {"tag": "header", "children": [
                    {"tag": "p", "children": []}
                ]},
                {"tag": "footNote", "children": [
                    {"tag": "p", "children": []}
                ]},
                {"tag": "rect", "children": [
                    {"tag": "p", "children": []}
                ]}
            ]
        });
        let result = walk(&tree, "본문");
        assert_eq!(
            result,
            json!([
                [{"tag": "p", "children": []}, "표"],
                [{"tag": "p", "children": []}, "머리말·꼬리말"],
                [{"tag": "p", "children": []}, "각주·미주"],
                [{"tag": "p", "children": []}, "글상자"]
            ])
        );
    }

    #[test]
    fn preserves_existing_context_inside_shape() {
        let tree = json!({
            "tag": "tbl",
            "children": [
                {"tag": "rect", "children": [
                    {"tag": "p", "children": []}
                ]}
            ]
        });
        let result = walk(&tree, "표");
        assert_eq!(
            result,
            json!([
                [{"tag": "p", "children": []}, "표"]
            ])
        );
    }
}
