use serde_json::{Map, Value};

const LANGS: &[&str] = &["hangul", "latin", "hanja", "japanese", "other", "symbol", "user"];

/// Read per-language attributes, converting valid integers while retaining invalid strings.
pub fn lang_values(element: &Value, cast: &str) -> Result<Value, String> {
    let attrs = element.get("attrib").and_then(Value::as_object);
    let mut out = Map::new();
    if let Some(attrs) = attrs {
        for lang in LANGS {
            if let Some(Value::String(raw)) = attrs.get(*lang) {
                let value = if cast == "str" { Value::String(raw.clone()) }
                    else if cast == "int" { raw.parse::<i64>().map(Value::from).unwrap_or_else(|_| Value::String(raw.clone())) }
                    else { return Err("unsupported cast".into()) };
                out.insert((*lang).to_owned(), value);
            }
        }
    }
    Ok(Value::Object(out))
}
