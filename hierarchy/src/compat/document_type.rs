use serde_json::Value;
pub fn document_type(roles: &Value, has_midtitle_table: bool) -> String {
    let present: Vec<&str> = roles.as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let mid = has_midtitle_table || present.contains(&"중제목");
    if !present.contains(&"소제목") { "기타".into() }
    else if present.contains(&"장") { if mid { "A형" } else { "기타" }.into() }
    else if mid { "B형".into() } else { "C형".into() }
}
