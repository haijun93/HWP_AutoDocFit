//! C ABI 규약 시험: Flutter(dart:ffi)와 Python(ctypes)이 쓰는 두 내보내기.
use std::ffi::{CStr, CString};

fn call(input: &str) -> Option<serde_json::Value> {
    let input = CString::new(input).unwrap();
    unsafe {
        let out = hierarchy::hierarchy_analyze_json(input.as_ptr());
        if out.is_null() { return None; }
        let text = CStr::from_ptr(out).to_str().unwrap().to_owned();
        hierarchy::hierarchy_json_free(out);
        serde_json::from_str(&text).ok()
    }
}

#[test]
fn analyze_returns_role_sequence_per_paragraph() {
    let value = call(r#"{"paragraphs":[{"text":"□ 추진 배경","left":0,"indent":0,"font":"","size_pt":15},{"text":"ㅇ 세부 내용","left":0,"indent":0,"font":"","size_pt":15}]}"#).expect("json");
    let roles = value["role_sequence"].as_array().expect("role_sequence");
    assert_eq!(roles.len(), 2);
    assert!(roles.iter().all(|r| r.is_string()));
    assert!(value["document_type"].is_string());
    assert!(value.get("results").is_none());
}

#[test]
fn null_and_invalid_input_return_null() {
    unsafe {
        assert!(hierarchy::hierarchy_analyze_json(std::ptr::null()).is_null());
        hierarchy::hierarchy_json_free(std::ptr::null_mut());
    }
    assert!(call("not json").is_none());
}

#[test]
fn empty_paragraphs_give_empty_role_sequence() {
    let value = call(r#"{"paragraphs":[]}"#).expect("json");
    assert_eq!(value["role_sequence"].as_array().map(Vec::len), Some(0));
}
