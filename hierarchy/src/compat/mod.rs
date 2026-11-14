//! `docfit_core.style_hierarchy`와 함수 단위 계약을 공유하는 포팅 계층.
pub mod canonical_marker;
pub mod normalize_leading_dot;
pub mod display_role;
pub mod stored_role;
pub mod document_type;
pub mod leading_marker;
pub mod most_common;
pub mod analyze_hierarchy;
pub mod hierarchy_summary;
pub mod marker_scheme;
pub mod lead_regex;

use std::io::{Read, Write};

pub fn run_fn(name: &str) -> Result<(), String> {
    let mut raw = Vec::new();
    std::io::stdin().read_to_end(&mut raw).map_err(|e| e.to_string())?;
    let input: serde_json::Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    let result = match name {
        "canonical_marker" => serde_json::json!(canonical_marker::canonical_marker(input["marker"].as_str().unwrap_or(""))),
        "normalize_leading_dot" => serde_json::json!(normalize_leading_dot::normalize_leading_dot(input["text"].as_str().unwrap_or(""))),
        "display_role" => serde_json::json!(display_role::display_role(input["role"].as_str().unwrap_or(""))),
        "stored_role" => serde_json::json!(stored_role::stored_role(input["label"].as_str().unwrap_or(""))),
        "document_type" => serde_json::json!(document_type::document_type(&input["roles"], input["has_midtitle_table"].as_bool().unwrap_or(false))),
        "leading_marker" => serde_json::to_value(leading_marker::leading_marker(input["text"].as_str().unwrap_or(""))).map_err(|e| e.to_string())?,
        "most_common" | "_most_common" => input["values"].as_array().and_then(|v| most_common::most_common(v)).cloned().unwrap_or(input["default"].clone()),
        "analyze_hierarchy" => analyze_hierarchy::analyze_hierarchy(&input["paragraphs"]),
        "analyze_hierarchy_stages" => analyze_hierarchy::analyze_hierarchy_stages(&input),
        "hierarchy_summary" => serde_json::json!(hierarchy_summary::hierarchy_summary(&input["analysis"])),
        "detect_scheme" => {
            let texts: Vec<String> = input["texts"].as_array().into_iter().flatten().map(|x| x.as_str().unwrap_or("").to_owned()).collect();
            marker_scheme::scheme_json(&marker_scheme::detect_scheme(&texts))
        },
        "classify_marker" => serde_json::Value::Array(input["texts"].as_array().into_iter().flatten().map(|t| serde_json::to_value(marker_scheme::classify_marker(t.as_str().unwrap_or(""))).unwrap_or(serde_json::Value::Null)).collect()),
        "_ordinal" => serde_json::Value::Array(input["items"].as_array().into_iter().flatten().map(|item| serde_json::to_value(marker_scheme::ordinal(item["family"].as_str().unwrap_or(""), item["token"].as_str().unwrap_or(""))).unwrap_or(serde_json::Value::Null)).collect()),
        _ => return Err(format!("unknown compat function: {name}")),
    };
    serde_json::to_writer(std::io::stdout(), &result).map_err(|e| e.to_string())?;
    std::io::stdout().write_all(b"\n").map_err(|e| e.to_string())
}
