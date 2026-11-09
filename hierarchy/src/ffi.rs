//! Stable, allocation-safe C ABI for Flutter and other native clients.
use crate::{analyze_paragraphs, SchemeParagraph, StructureDiagnostic};
use std::{ffi::c_char, ptr, slice, str};

fn escape(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn analyze(input: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    let paragraphs = value.get("paragraphs").and_then(|v| v.as_array()).ok_or("paragraphs must be an array")?;
    let texts: Vec<String> = paragraphs.iter().map(|p| p.get("text").and_then(|v| v.as_str()).unwrap_or("").to_owned()).collect();
    let records: Vec<SchemeParagraph<'_>> = paragraphs.iter().enumerate().map(|(i, p)| SchemeParagraph {
        text: &texts[i], position: i,
        left_indent: p.get("indent").and_then(|v| v.as_i64()).unwrap_or(0).clamp(i32::MIN as i64, i32::MAX as i64) as i32,
        size_hundredth_pt: p.get("font_size").and_then(|v| v.as_f64()).unwrap_or(0.0).mul_add(100.0, 0.5).max(0.0).min(u32::MAX as f64) as u32,
        bold: p.get("bold").and_then(|v| v.as_bool()).unwrap_or(false),
        in_attachment: p.get("in_attachment").and_then(|v| v.as_bool()).unwrap_or(false),
    }).collect();
    let results = analyze_paragraphs(&records);
    let diagnostic = |d: &StructureDiagnostic| -> String { match d {
        StructureDiagnostic::NumberGap { paragraph, expected, found } => format!("number_gap:{}:{}:{}", paragraph, expected, found),
        StructureDiagnostic::LevelGap { paragraph, from, to } => format!("level_gap:{}:{}:{}", paragraph, from, to),
        StructureDiagnostic::OrphanLevel { paragraph, level } => format!("orphan_level:{}:{}", paragraph, level),
        StructureDiagnostic::OrderViolation { paragraph, expected_rank, found_rank } => format!("order_violation:{}:{}:{}", paragraph, expected_rank, found_rank),
        StructureDiagnostic::SameLevelDifferentFamily { rank } => format!("same_level_different_family:{}", rank),
    }};
    let items: Vec<String> = results.iter().map(|r| {
        let parent = r.parent_index.map(|i| i.to_string()).unwrap_or_else(|| "null".into());
        let diagnostics = r.diagnostics.iter().map(|d| escape(&diagnostic(d))).collect::<Vec<_>>().join(",");
        format!("{{\"role\":{},\"kind\":{},\"parent_index\":{},\"confidence\":{},\"diagnostics\":[{}]}}", escape(&format!("{:?}", r.role).to_lowercase()), escape(&format!("{:?}", r.kind).to_lowercase()), parent, r.confidence, diagnostics)
    }).collect();
    Ok(format!("{{\"results\":[{}]}}", items.join(",")))
}

/// Analyze UTF-8 JSON bytes. Returns an owned JSON C string; free it with `hierarchy_free_string`.
#[no_mangle]
pub unsafe extern "C" fn hierarchy_analyze_json(data: *const u8, len: usize) -> *mut c_char {
    if data.is_null() || len > 64 * 1024 * 1024 { return ptr::null_mut(); }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    let result = str::from_utf8(bytes).map_err(|e| e.to_string()).and_then(analyze);
    let json = match result { Ok(v) => v, Err(e) => format!("{{\"error\":{}}}", escape(&e)) };
    match std::ffi::CString::new(json) { Ok(s) => s.into_raw(), Err(_) => ptr::null_mut() }
}

/// Release a string returned by `hierarchy_analyze_json`.
#[no_mangle]
pub unsafe extern "C" fn hierarchy_free_string(value: *mut c_char) {
    if !value.is_null() { drop(unsafe { std::ffi::CString::from_raw(value) }); }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ffi_round_trips_paragraph_results_and_free() {
        let input = r#"{"paragraphs":[{"text":"1. 제목","indent":0,"font_size":12}]}"#;
        let result = unsafe { hierarchy_analyze_json(input.as_ptr(), input.len()) };
        assert!(!result.is_null());
        let output = unsafe { std::ffi::CStr::from_ptr(result).to_str().unwrap().to_owned() };
        assert!(output.contains("\"results\":[{"));
        unsafe { hierarchy_free_string(result); }
    }
}
