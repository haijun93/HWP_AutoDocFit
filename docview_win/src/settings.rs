//! 설정 읽기(읽기 전용). 키 규칙은 `docfit_app/thumb_size_setting.py`·`library_exclusions.py` 와 같다.
//! 저장은 이번 단계 범위 밖(다음 카드에서 `docfit_config` 경유).
use crate::json::{self, Value};
use std::path::PathBuf;

/// `ui_thumb_sizes` 설정 키와 화면 이름.
pub const THUMB_KEY: &str = "ui_thumb_sizes";
pub const THUMB_VIEW: &str = "doc_grid";
pub use grid_core::thumb::{MAX_THUMB, MIN_THUMB, clamp_thumb};
pub const DEFAULT_DOC_THUMB: f64 = 190.0;
/// 제외 폴더 설정 키.
pub const EXCLUDED_KEY: &str = "library_excluded_folders";
/// 문서뷰 화면 선택 키('flutter' 기본 | 'native'). 이번 단계는 읽기만(앱 연결은 다음 카드).
pub const UI_KEY: &str = "docview_ui";


/// `docfit_config::settings_path` 와 같은 위치 규칙(APPDATA → LOCALAPPDATA → 홈 → 임시).
pub fn settings_path() -> PathBuf {
    let var = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let root = var("APPDATA")
        .map(|p| p.join("HwpAutoDocFit"))
        .or_else(|| var("LOCALAPPDATA").map(|p| p.join("HwpAutoDocFit")))
        .or_else(|| var("USERPROFILE").or_else(|| var("HOME")).map(|p| p.join(".hwp_auto_docfit")))
        .unwrap_or_else(std::env::temp_dir);
    root.join("settings.json")
}

/// 설정 글(JSON)에서 문서뷰 썸네일 크기. 없거나 형식이 틀리면 기본값.
pub fn thumb_size(text: &str) -> f64 {
    let Ok(root) = json::parse(text) else { return DEFAULT_DOC_THUMB };
    let Some(sizes) = root.get(THUMB_KEY) else { return DEFAULT_DOC_THUMB };
    match sizes.get(THUMB_VIEW) {
        Some(Value::Num(n)) => clamp_thumb(*n, DEFAULT_DOC_THUMB),
        _ => DEFAULT_DOC_THUMB,
    }
}

/// 제외 폴더 목록(빈 글 제외, 대소문자 무시 중복 제거, 처음 순서 유지).
pub fn excluded_folders(text: &str) -> Vec<String> {
    let Ok(root) = json::parse(text) else { return Vec::new() };
    let Some(items) = root.get(EXCLUDED_KEY).and_then(Value::as_array) else { return Vec::new() };
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for v in items {
        if let Some(s) = v.as_str() {
            let s = s.trim();
            if !s.is_empty() && seen.insert(s.replace('/', "\\").trim_end_matches('\\').to_lowercase()) {
                out.push(s.to_string());
            }
        }
    }
    out
}

/// `docview_ui`: 'native' 만 인식하고 그 밖(없음·잘못된 값)은 'flutter'.
pub fn docview_ui(text: &str) -> &'static str {
    match json::parse(text).ok().as_ref().and_then(|r| r.get(UI_KEY)).and_then(Value::as_str) {
        Some("native") => "native",
        _ => "flutter",
    }
}

/// 설정 파일을 글로 읽는다(없으면 빈 객체 글).
pub fn read_text(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|_| "{}".into())
}
