use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct FavoriteQuery {
    pub query: String,
    #[serde(default)]
    pub name: String,
}

impl FavoriteQuery {
    pub fn new(query: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            name: name.into(),
        }
    }
}

pub fn push_recent_query(recent: &mut Vec<String>, query: &str) {
    let q = query.trim();
    if q.is_empty() {
        return;
    }
    if let Some(pos) = recent.iter().position(|r| r == q) {
        recent.remove(pos);
    }
    recent.insert(0, q.to_string());
    if recent.len() > 30 {
        recent.truncate(30);
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    pub hotkey: String,
    pub recent_queries: Vec<String>,
    pub favorite_queries: Vec<FavoriteQuery>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            hotkey: "Ctrl+Alt+Space".into(),
            recent_queries: Vec::new(),
            favorite_queries: Vec::new(),
        }
    }
}

impl Settings {
    pub fn push_recent_query(&mut self, query: &str) {
        push_recent_query(&mut self.recent_queries, query);
    }

    pub fn toggle_favorite(&mut self, query: &str) -> bool {
        let q = query.trim();
        if q.is_empty() {
            return false;
        }
        if let Some(pos) = self.favorite_queries.iter().position(|f| f.query == q) {
            self.favorite_queries.remove(pos);
            false
        } else {
            self.favorite_queries.push(FavoriteQuery {
                query: q.to_string(),
                name: q.to_string(),
            });
            true
        }
    }

    pub fn is_favorite(&self, query: &str) -> bool {
        let q = query.trim();
        self.favorite_queries.iter().any(|f| f.query == q)
    }

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
    fn file_save_load_and_missing_default() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("mg1-settings-{}-{nonce}.json", std::process::id()));
        assert_eq!(Settings::load(&path).unwrap(), Settings::default());
        let settings = Settings {
            theme: "light".into(),
            ..Settings::default()
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), settings);
        std::fs::write(&path, b"{").unwrap();
        assert!(Settings::load(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn recent_queries_max_30_and_duplicate_to_front() {
        let mut s = Settings::default();
        for i in 1..=35 {
            s.push_recent_query(&format!("query {i}"));
        }
        assert_eq!(s.recent_queries.len(), 30);
        assert_eq!(s.recent_queries[0], "query 35");
        assert_eq!(s.recent_queries[29], "query 6");

        // 중복 검색어 삽입 시 맨 앞으로 이동
        s.push_recent_query("query 10");
        assert_eq!(s.recent_queries.len(), 30);
        assert_eq!(s.recent_queries[0], "query 10");
        assert_eq!(s.recent_queries[1], "query 35");

        // 빈 문자열이나 공백은 무시
        s.push_recent_query("   ");
        assert_eq!(s.recent_queries.len(), 30);
        assert_eq!(s.recent_queries[0], "query 10");
    }
    #[test]
    fn favorite_queries_toggle_and_roundtrip() {
        let mut s = Settings::default();
        assert!(!s.is_favorite("보고서"));
        assert!(s.toggle_favorite("보고서"));
        assert!(s.is_favorite("보고서"));
        assert_eq!(s.favorite_queries.len(), 1);
        assert_eq!(s.favorite_queries[0].query, "보고서");
        assert_eq!(s.favorite_queries[0].name, "보고서");

        // 토글 다시 하면 제거
        assert!(!s.toggle_favorite("보고서"));
        assert!(!s.is_favorite("보고서"));
        assert!(s.favorite_queries.is_empty());

        // 빈 검색어는 무시
        assert!(!s.toggle_favorite("  "));
        assert!(s.favorite_queries.is_empty());

        // roundtrip 보존
        s.toggle_favorite("예산 결산");
        s.push_recent_query("최근 검색 1");
        let json = serde_json::to_string(&s).unwrap();
        let loaded: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded, s);
    }
}
