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

#[test]
fn search_only_abi_matches_full_response_and_rejects_bad_input() {
    let payload = serde_json::json!({"paragraphs": [
        {"text": "# 제목", "source_path": "x.md"}, {"text": "## 장"}, {"text": "□ 상위"},
        {"text": "칸", "is_table_cell": true}
    ]});
    let input = CString::new(payload.to_string()).unwrap();
    unsafe {
        let full = hierarchy::hierarchy_analyze_json(input.as_ptr());
        let fast = hierarchy::hierarchy_search_levels_json(input.as_ptr());
        assert!(!full.is_null() && !fast.is_null());
        let full_v: serde_json::Value = serde_json::from_str(CStr::from_ptr(full).to_str().unwrap()).unwrap();
        let fast_v: serde_json::Value = serde_json::from_str(CStr::from_ptr(fast).to_str().unwrap()).unwrap();
        hierarchy::hierarchy_json_free(full);
        hierarchy::hierarchy_json_free(fast);
        assert_eq!(fast_v["search_levels"], full_v["search_levels"]);
        assert!(fast_v.get("role_sequence").is_none());
        for bad in ["not json", "{}", "{\"paragraphs\": 3}"] {
            let bad = CString::new(bad).unwrap();
            assert!(hierarchy::hierarchy_search_levels_json(bad.as_ptr()).is_null());
        }
        assert!(hierarchy::hierarchy_search_levels_json(std::ptr::null()).is_null());
    }
}
