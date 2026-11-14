use std::ffi::{CStr, CString};

#[test]
fn search_metadata_adds_levels_without_replacing_formatting_roles() {
    let input = CString::new(
        serde_json::json!({"paragraphs": [
            {"text": "# 제목", "source_path": "x.md"}, {"text": "## 장"}
        ]})
        .to_string(),
    )
    .unwrap();
    unsafe {
        let ptr = hierarchy::hierarchy_analyze_json(input.as_ptr());
        assert!(!ptr.is_null());
        let value: serde_json::Value =
            serde_json::from_str(CStr::from_ptr(ptr).to_str().unwrap()).unwrap();
        hierarchy::hierarchy_json_free(ptr);
        assert_eq!(
            value["search_levels"],
            serde_json::json!(["document_title", "chapter"])
        );
        assert_eq!(value["role_sequence"].as_array().unwrap().len(), 2);
    }
}
