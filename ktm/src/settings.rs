use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct RecentDoc {
    pub path: String,
    pub name: String,
    pub location: String,
    pub modified: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    pub hotkey: String,
    /// 마지막 창 위치 (x, y). 화면 위쪽 1/4 기본 위치 대신 기억된 위치로 복원.
    pub window_pos: Option<(i32, i32)>,
    /// 최근 검색어 기기 저장 여부 (끌 수 있음).
    pub save_history: bool,
    /// 최근 검색어 (최대 5개).
    pub recent_queries: Vec<String>,
    /// 최근 연 문서 (최대 10개).
    pub recent_docs: Vec<RecentDoc>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            hotkey: "Ctrl+Alt+Space".into(),
            window_pos: None,
            save_history: true,
            recent_queries: Vec::new(),
            recent_docs: Vec::new(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("설정 읽기 오류: {e}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("설정 읽기 오류: {e}")),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, bytes).map_err(|e| format!("설정 저장 오류: {e}"))
    }

    /// 최근 검색어 추가 (중복 제거 후 맨 앞에 추가, 최대 5개 유지).
    pub fn add_recent_query(&mut self, query: &str) {
        if !self.save_history {
            return;
        }
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        self.recent_queries.retain(|item| item != q);
        self.recent_queries.insert(0, q.to_string());
        if self.recent_queries.len() > 5 {
            self.recent_queries.truncate(5);
        }
    }

    /// 최근 연 문서 추가 (중복 제거 후 맨 앞에 추가, 최대 10개 유지).
    pub fn add_recent_doc(&mut self, doc: RecentDoc) {
        self.recent_docs.retain(|item| item.path != doc.path);
        self.recent_docs.insert(0, doc);
        if self.recent_docs.len() > 10 {
            self.recent_docs.truncate(10);
        }
    }
}

pub fn load(path: &Path) -> Result<Settings, String> {
    Settings::load(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn omitted_fields_have_safe_defaults() {
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings, Settings::default());
        let settings: Settings = serde_json::from_str(r#"{"theme":"light"}"#).unwrap();
        assert_eq!(settings.hotkey, "Ctrl+Alt+Space");
    }
    #[test]
    fn settings_roundtrip() {
        let settings = Settings {
            theme: "high-contrast".into(),
            ..Settings::default()
        };
        assert_eq!(
            serde_json::from_slice::<Settings>(&serde_json::to_vec(&settings).unwrap()).unwrap(),
            settings
        );
    }
    #[test]
    fn malformed_settings_are_reported() {
        assert!(serde_json::from_str::<Settings>("{").is_err());
    }
    #[test]
    fn recent_queries_and_docs_limit() {
        let mut s = Settings::default();
        for i in 0..10 {
            s.add_recent_query(&format!("검색어 {i}"));
        }
        assert_eq!(s.recent_queries.len(), 5);
        assert_eq!(s.recent_queries[0], "검색어 9");

        s.save_history = false;
        s.add_recent_query("저장 안 됨");
        assert_eq!(s.recent_queries.len(), 5);
        assert_eq!(s.recent_queries[0], "검색어 9");

        for i in 0..15 {
            s.add_recent_doc(RecentDoc {
                path: format!("C:\\doc_{i}.hwp"),
                name: format!("문서 {i}.hwp"),
                location: "C:\\".into(),
                modified: "2026-11-14".into(),
            });
        }
        assert_eq!(s.recent_docs.len(), 10);
        assert_eq!(s.recent_docs[0].name, "문서 14.hwp");
    }
}
