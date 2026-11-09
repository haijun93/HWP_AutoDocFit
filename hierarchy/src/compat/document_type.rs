//! `docfit_core.style_hierarchy.document_type` 1:1 이식.
use serde_json::Value;
pub fn document_type(roles: &Value, has_midtitle_table: bool) -> String {
    let present: Vec<&str> = roles.as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let mid = has_midtitle_table || present.contains(&"중제목");
    if !present.contains(&"소제목") { "기타".into() }
    else if present.contains(&"장") { if mid { "A형" } else { "기타" }.into() }
    else if mid { "B형".into() } else { "C형".into() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn classifies_role_presence_and_midtitle_table() {
        let cases = [
            (json!([]), false, "기타"),
            (json!(["소제목"]), false, "C형"),
            (json!(["소제목"]), true, "B형"),
            (json!(["장", "소제목"]), false, "기타"),
            (json!(["장", "소제목"]), true, "A형"),
            (json!(["장", "중제목", "소제목"]), false, "A형"),
            (json!(["장", "중제목"]), true, "기타"),
        ];
        for (roles, table, expected) in cases {
            assert_eq!(document_type(&roles, table), expected);
        }
    }
}
