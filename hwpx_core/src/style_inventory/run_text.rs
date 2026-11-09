use serde_json::Value;

/// Concatenate direct t children from a run, matching style_inventory._run_text.
pub fn run_text(run: &Value) -> String {
    let mut out = String::new();
    if let Some(children) = run.get("children").and_then(Value::as_array) {
        for child in children {
            if child.get("tag").and_then(Value::as_str).map(crate::style_inventory::tag::tag) == Some("t") {
                out.push_str(&crate::style_inventory::text_of::text_of(child));
            }
        }
    }
    out
}
