use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct FavoriteQuery {
    pub query: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SmartPreset {
    pub name: String,
    pub file_types: Vec<String>,
    pub days: Option<u32>,
    pub exclude_filename: bool,
    pub within_query: String,
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

pub fn relative_time_label(now_unix: u64, then_unix: u64) -> String {
    let age = now_unix.saturating_sub(then_unix);
    if age < 60 {
        "방금".into()
    } else if age < 3600 {
        format!("{}분 전", age / 60)
    } else if age < 86400 {
        format!("{}시간 전", age / 3600)
    } else if age < 7 * 86400 {
        "이번 주".into()
    } else {
        "이전".into()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    /// UNIFIED1: 요청 하나로 세 검색 단을 순차 발행한다. 기존 설정은 꺼짐.
    pub unified_search: bool,
    pub theme: String,
    pub hotkey: String,
    pub recent_queries: Vec<String>,
    #[serde(default)]
    pub recent_query_times: Vec<u64>,
    pub favorite_queries: Vec<FavoriteQuery>,
    pub smart_presets: Vec<SmartPreset>,
    pub ui_theme: String,
    pub default_search_mode: String,
    pub max_results: usize,
    pub ui_scale: u32,
    pub result_density: String,
    pub open_clicks: u8,
    pub show_location: bool,
    pub show_modified: bool,
    pub include_subfolders: bool,
    pub excluded_directories: Vec<String>,
    pub version_grouping: bool,
    pub autostart: bool,
    pub minimize_to_tray: bool,
    pub start_hidden: bool,
    pub max_file_size_mb: u64,
    pub skip_cloud_network: bool,
    pub allow_system_folders: bool,
    /// SEMI1: literal paragraph groups; off preserves the existing quick-search path.
    pub related_queries: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            unified_search: false,
            theme: "dark".into(),
            hotkey: "Ctrl+Alt+Space".into(),
            recent_queries: Vec::new(),
            recent_query_times: Vec::new(),
            favorite_queries: Vec::new(),
            smart_presets: Vec::new(),
            ui_theme: "system".into(),
            default_search_mode: "keyword".into(),
            max_results: 100,
            ui_scale: 100,
            result_density: "comfortable".into(),
            open_clicks: 2,
            show_location: true,
            show_modified: true,
            include_subfolders: true,
            excluded_directories: Vec::new(),
            version_grouping: false,
            autostart: false,
            minimize_to_tray: true,
            start_hidden: false,
            max_file_size_mb: 0,
            skip_cloud_network: true,
            allow_system_folders: false,
            related_queries: false,
        }
    }
}

impl Settings {
    /// Keep user-configurable limits within the values supported by the UI and backend.
    pub fn bounded_max_results(&self) -> usize {
        match self.max_results {
            0..=50 => 50,
            51..=100 => 100,
            101..=200 => 200,
            _ => 200,
        }
    }

    pub fn bounded_ui_scale(&self) -> u32 {
        match self.ui_scale {
            0..=90 => 90,
            91..=100 => 100,
            101..=110 => 110,
            _ => 125,
        }
    }

    pub fn row_height(&self) -> i32 {
        if self.result_density == "compact" {
            56
        } else {
            70
        }
    }

    /// 이름이 있으면 삭제하고, 없으면 추가한다. 반환값은 저장 후 존재 여부다.
    pub fn toggle_smart_preset(&mut self, preset: SmartPreset) -> bool {
        if let Some(i) = self
            .smart_presets
            .iter()
            .position(|p| p.name == preset.name)
        {
            self.smart_presets.remove(i);
            false
        } else {
            self.smart_presets.push(preset);
            true
        }
    }
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
        assert_eq!(settings.max_results, 100);
        assert!(settings.include_subfolders);
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
    fn legacy_settings_without_new_fields_load() {
        let s:Settings=serde_json::from_str(r#"{"theme":"dark","hotkey":"Ctrl+Alt+Space","recent_queries":["a"],"favorite_queries":[],"smart_presets":[]}"#).unwrap();
        assert_eq!(s.max_results, 100);
        assert_eq!(s.ui_theme, "system");
        assert!(s.include_subfolders);
    }
    #[test]
    fn new_settings_fields_roundtrip() {
        let mut s = Settings::default();
        s.ui_theme = "light".into();
        s.max_results = 200;
        s.excluded_directories = vec!["C:\\Temp".into()];
        s.autostart = true;
        let t: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(t, s);
    }
    #[test]
    fn settings_defaults_bound_click_policy() {
        let s = Settings::default();
        assert_eq!(s.open_clicks, 2);
        assert_eq!(s.ui_scale, 100);
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
    #[test]
    fn smart_preset_add_remove_and_roundtrip() {
        let preset = SmartPreset {
            name: "문서 검토".into(),
            file_types: vec!["hwp".into(), "hwpx".into()],
            days: Some(30),
            exclude_filename: true,
            within_query: "결재".into(),
        };
        let mut s = Settings::default();
        assert!(s.toggle_smart_preset(preset.clone()));
        let loaded: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(loaded.smart_presets, vec![preset.clone()]);
        assert!(!s.toggle_smart_preset(preset));
        assert!(s.smart_presets.is_empty());
    }
    #[test]
    fn relative_search_time_uses_age_buckets() {
        assert_eq!(relative_time_label(100, 60), "방금");
        assert_eq!(relative_time_label(3000, 100), "48분 전");
        assert_eq!(relative_time_label(40000, 100), "11시간 전");
        assert_eq!(relative_time_label(400000, 100), "이번 주");
        assert_eq!(relative_time_label(900000, 100), "이전");
    }
    #[test]
    fn max_results_is_bounded_to_supported_steps() {
        for (n, want) in [
            (0, 50),
            (50, 50),
            (51, 100),
            (100, 100),
            (101, 200),
            (999, 200),
        ] {
            let s = Settings {
                max_results: n,
                ..Default::default()
            };
            assert_eq!(s.bounded_max_results(), want);
        }
    }
    #[test]
    fn ui_scale_clamps_and_snaps_to_supported_steps() {
        for (n, want) in [
            (0, 90),
            (90, 90),
            (91, 100),
            (100, 100),
            (101, 110),
            (110, 110),
            (111, 125),
            (900, 125),
        ] {
            let s = Settings {
                ui_scale: n,
                ..Default::default()
            };
            assert_eq!(s.bounded_ui_scale(), want);
        }
    }
    #[test]
    fn density_maps_to_compact_or_comfortable_row_height() {
        assert_eq!(
            Settings {
                result_density: "compact".into(),
                ..Default::default()
            }
            .row_height(),
            56
        );
        assert_eq!(Settings::default().row_height(), 70);
    }
}
