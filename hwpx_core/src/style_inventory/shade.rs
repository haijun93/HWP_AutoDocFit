use serde_json::Value;

pub fn shade(value: Option<&str>) -> Value {
    let normalized = value.filter(|s| !s.is_empty()).unwrap_or("none").to_uppercase();
    if normalized == "NONE" || normalized == "#FFFFFF" { Value::Null } else { Value::String(normalized) }
}
