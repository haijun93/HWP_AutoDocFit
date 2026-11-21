//! 설정 읽기(읽기 전용). 키 규칙은 `docfit_app/thumb_size_setting.py`·`library_exclusions.py` 와 같다.
//! 저장은 이번 단계 범위 밖(다음 카드에서 `docfit_config` 경유).
use grid_core::json::{self, Value};
use std::path::PathBuf;

/// `ui_thumb_sizes` 설정 키와 화면 이름.
pub const THUMB_KEY: &str = "ui_thumb_sizes";
pub const THUMB_VIEW: &str = "photo_album";
pub use grid_core::thumb::{MAX_THUMB, MIN_THUMB, clamp_thumb};
pub const DEFAULT_PHOTO_THUMB: f64 = 160.0;
/// 제외 폴더 설정 키.
pub const EXCLUDED_KEY: &str = "library_excluded_folders";
/// 사진뷰 화면 선택 키('flutter' 기본 | 'native'). 앱 연결은 PHOTOWIN2.
pub const UI_KEY: &str = "photoview_ui";


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

/// 설정 글(JSON)에서 사진뷰 썸네일 크기(`ui_thumb_sizes.photo_album`). 없거나 형식이 틀리면 기본값.
pub fn thumb_size(text: &str) -> f64 {
    let Ok(root) = json::parse(text) else { return DEFAULT_PHOTO_THUMB };
    let Some(sizes) = root.get(THUMB_KEY) else { return DEFAULT_PHOTO_THUMB };
    match sizes.get(THUMB_VIEW) {
        Some(Value::Num(n)) => clamp_thumb(*n, DEFAULT_PHOTO_THUMB),
        _ => DEFAULT_PHOTO_THUMB,
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

/// `photoview_ui`: 'native' 만 인식하고 그 밖(없음·잘못된 값)은 'flutter'.
pub fn photoview_ui(text: &str) -> &'static str {
    match json::parse(text).ok().as_ref().and_then(|r| r.get(UI_KEY)).and_then(Value::as_str) {
        Some("native") => "native",
        _ => "flutter",
    }
}

/// 설정 파일을 글로 읽는다(없으면 빈 객체 글).
pub fn read_text(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|_| "{}".into())
}

// ---------- 앱 설정 읽기·저장(PHOTOWIN2): 설정 소유자 `docfit_config` 경유 ----------

/// 창이 시작할 때 한 번 읽은 앱 설정 묶음.
#[derive(Clone, Debug, PartialEq)]
pub struct Loaded {
    /// `ui_thumb_sizes.photo_album`(100~240, 없으면 160).
    pub thumb: f64,
    /// `library_excluded_folders`(`klib scan --exclude` 로 넘긴다).
    pub excluded: Vec<String>,
    /// 읽은 시점의 파일 revision. 저장 때 그대로 보내 그 사이 앱이 쓴 변경을 덮어쓰지 않게 한다.
    pub revision: String,
    /// `photoview_ui`('flutter'|'native').
    pub ui: &'static str,
    /// 설정 파일을 읽지 못했을 때의 이유(기본값으로 계속한다).
    pub error: Option<String>,
}

/// 설정 파일을 `docfit_config` 로 읽는다(없으면 기본값, 깨졌으면 기본값 + 이유). 파일은 바꾸지 않는다.
pub fn load(path: &std::path::Path) -> Loaded {
    match docfit_config::get(path) {
        Ok(merged) => {
            let text = merged.to_string();
            // revision 은 `docfit_config::set` 이 검사하는 값(원본 파일 기준)과 같아야 한다.
            let revision = docfit_config::read_json(path).map(|v| docfit_config::revision(&v)).unwrap_or_default();
            Loaded { thumb: thumb_size(&text), excluded: excluded_folders(&text), revision, ui: photoview_ui(&text), error: None }
        }
        Err(e) => Loaded { thumb: DEFAULT_PHOTO_THUMB, excluded: Vec::new(), revision: String::new(), ui: "flutter", error: Some(format!("설정을 읽지 못해 기본값을 씁니다({})", e.message)) },
    }
}

/// 폴더 경로의 비교 키(Python `library_exclusions._key` 와 같은 규칙: 구분자·대소문자·끝 구분자 무시).
pub fn folder_key(s: &str) -> String {
    let t = s.trim().replace('/', "\\");
    let t = if t.len() > 3 { t.trim_end_matches('\\').to_string() } else { t };
    t.to_lowercase()
}

/// 제외 폴더 목록 정렬(Python `sorted(..., key=str.casefold)`).
pub fn sort_folders(items: &mut [String]) {
    items.sort_by_key(|s| s.to_lowercase());
}

/// 저장 실패의 종류.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    /// 읽은 뒤 다른 곳(앱)이 설정 파일을 바꿨다 — 아무것도 쓰지 않았다.
    Conflict,
    Failed(String),
}

/// 제외 폴더 목록을 저장한다(`docfit_config::set` 의 목록 검증·원자 저장·revision 검사). 성공하면 새 revision.
pub fn save_excluded(path: &std::path::Path, items: &[String], revision: &str) -> Result<String, SaveError> {
    let patch = serde_json::json!({ EXCLUDED_KEY: items, "__revision": revision });
    match docfit_config::set(path, &patch) {
        Ok(_) => docfit_config::read_json(path).map(|v| docfit_config::revision(&v)).map_err(|e| SaveError::Failed(e.message)),
        Err(e) if e.code == "SETTINGS_CHANGED" => Err(SaveError::Conflict),
        Err(e) => Err(SaveError::Failed(e.message)),
    }
}
