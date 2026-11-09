use serde_json::{Map, Value};

/// Parse fontface groups into lang -> font id -> face mappings.
pub fn parse_fonts(header: &Value) -> Value {
    let mut fonts = Map::new();
    let mut stack = vec![header];
    while let Some(node) = stack.pop() {
        if node.get("tag").and_then(Value::as_str).map(crate::style_inventory::tag::tag) == Some("fontface") {
            let lang = node.get("attrib").and_then(|a| a.get("lang")).and_then(Value::as_str).unwrap_or("").to_lowercase();
            let mut entries = Map::new();
            if let Some(children) = node.get("children").and_then(Value::as_array) {
                for font in children {
                    if font.get("tag").and_then(Value::as_str).map(crate::style_inventory::tag::tag) == Some("font") {
                        if let Some(attrs) = font.get("attrib") {
                            if let Some(id) = attrs.get("id").and_then(Value::as_str) {
                                entries.insert(id.to_owned(), attrs.get("face").cloned().unwrap_or(Value::Null));
                            }
                        }
                    }
                }
            }
            fonts.insert(lang, Value::Object(entries));
        }
        if let Some(children) = node.get("children").and_then(Value::as_array) { stack.extend(children.iter().rev()); }
    }
    Value::Object(fonts)
}
