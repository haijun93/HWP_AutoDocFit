use serde_json::Value;

fn local(tag: &Value) -> Option<&str> {
    tag.as_str().map(crate::style_inventory::tag::tag)
}

/// Extract text and HWPX spacing tokens with ElementTree-compatible tail handling.
pub fn text_of(tree: &Value) -> String {
    let mut out = String::new();
    let mut stack = vec![tree];
    while let Some(node) = stack.pop() {
        if local(&node["tag"]) == Some("t") {
            if let Some(text) = node.get("text").and_then(Value::as_str) { out.push_str(text); }
            if let Some(children) = node.get("children").and_then(Value::as_array) {
                for child in children {
                    if matches!(local(&child["tag"]), Some("fwSpace" | "nbSpace" | "tab")) { out.push(' '); }
                    if let Some(tail) = child.get("tail").and_then(Value::as_str) { out.push_str(tail); }
                }
            }
        }
        if let Some(children) = node.get("children").and_then(Value::as_array) {
            stack.extend(children.iter().rev());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::text_of;
    use serde_json::json;

    #[test]
    fn keeps_spacing_tokens_and_tails() {
        let tree = json!({"tag":"p","children":[{"tag":"t","text":"A","children":[{"tag":"fwSpace","tail":"B"}]}]});
        assert_eq!(text_of(&tree), "A B");
    }
}
