use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    pub hotkey: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            hotkey: "Ctrl+Alt+Space".into(),
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
}
