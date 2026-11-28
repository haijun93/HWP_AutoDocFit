//! 퀵몽땅검색 최종 테마 결정(QMSDARK1). 순수 규칙과 앱 설정 읽기만 두고, 칠하는 일은 창이 한다.
//!
//! 우선순위: 퀵검색 설정 창의 '테마'(`ui_theme` 밝게·어둡게) → 앱 본체 '화면 테마'(`theme_mode`)
//! → Windows 앱 모드. 고대비가 켜져 있으면 `ui_theme::resolve`가 선호와 상관없이 시스템 색을 쓴다.
use std::path::{Path, PathBuf};
use ui_theme::{Kind, Palette, ThemePref, rgb};

/// `quick`은 퀵검색 `ui_theme`, `app`은 앱 `theme_mode`(파일·키가 없으면 None).
pub fn decide(quick: &str, app: Option<&str>) -> ThemePref {
    match ThemePref::parse(quick) {
        ThemePref::System => app.map_or(ThemePref::System, ThemePref::parse),
        own => own,
    }
}

/// 앱 본체 설정 파일(앱·설정 창·Dock과 같은 위치). 읽기만 한다.
pub fn default_app_settings() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("LOCALAPPDATA"))
        .map(|base| PathBuf::from(base).join("HwpAutoDocFit").join("settings.json"))
}

/// 앱 설정의 `theme_mode`. 파일이 없거나 읽지 못하면 None(Windows 앱 모드를 따른다).
pub fn app_theme_mode(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    value.get("theme_mode")?.as_str().map(str::to_owned)
}

/// Ctrl+L이 저장할 값: 지금 보이는 색의 반대. 고대비에서는 바꾸지 않는다.
pub fn toggled(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::Dark => Some("light"),
        Kind::Light => Some("dark"),
        Kind::HighContrast => None,
    }
}

/// 검색어 강조 칸의 (바탕, 글자).
pub fn term_mark(kind: Kind, palette: &Palette) -> (u32, u32) {
    match kind {
        Kind::Light => (0x008A_F0FE, 0x0020_2020),
        Kind::Dark => (rgb(0x7A, 0x67, 0x2E), rgb(0xFF, 0xFF, 0xFF)),
        Kind::HighContrast => (palette.selected, palette.row_text(true, palette.text)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_theme::{SysColors, contrast, resolve};

    #[test]
    fn own_choice_then_app_then_windows() {
        use ThemePref::{Dark, Light, System};
        let cases = [
            ("dark", Some("light"), Dark),
            ("light", Some("dark"), Light),
            ("dark", None, Dark),
            ("system", Some("dark"), Dark),
            ("system", Some("light"), Light),
            ("system", Some("system"), System),
            ("system", Some("blue"), System),
            ("system", None, System),
            ("", Some("dark"), Dark),
        ];
        for (quick, app, want) in cases {
            assert_eq!(decide(quick, app), want, "{quick:?} {app:?}");
        }
    }

    #[test]
    fn windows_mode_and_high_contrast_decide_the_final_kind() {
        let system = decide("system", Some("system"));
        assert_eq!(resolve(system, true, false), Kind::Dark);
        assert_eq!(resolve(system, false, false), Kind::Light);
        for quick in ["light", "dark", "system"] {
            assert_eq!(resolve(decide(quick, Some("light")), true, true), Kind::HighContrast);
        }
    }

    #[test]
    fn app_theme_mode_reads_only_a_string_key() {
        let path = std::env::temp_dir().join(format!("qmsdark1-app-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert_eq!(app_theme_mode(&path), None);
        for (bytes, want) in [
            (&br#"{"theme_mode":"dark","other":1}"#[..], Some("dark")),
            (&b"\xEF\xBB\xBF{\"theme_mode\":\"light\"}"[..], Some("light")),
            (&br#"{"theme_mode":1}"#[..], None),
            (&br#"{}"#[..], None),
            (&b"{"[..], None),
        ] {
            std::fs::write(&path, bytes).unwrap();
            assert_eq!(app_theme_mode(&path).as_deref(), want);
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn toggle_and_marks_follow_the_visible_theme() {
        assert_eq!(toggled(Kind::Dark), Some("light"));
        assert_eq!(toggled(Kind::Light), Some("dark"));
        assert_eq!(toggled(Kind::HighContrast), None);
        for (kind, palette) in [(Kind::Light, Palette::LIGHT), (Kind::Dark, Palette::DARK)] {
            let (bg, fg) = term_mark(kind, &palette);
            assert!(contrast(fg, bg) >= 4.5, "{kind:?}");
        }
        let sys = SysColors { highlight: 0x00FF_FF00, highlight_text: 0, ..SysColors::default() };
        let hc = Palette::high_contrast(&sys);
        assert_eq!(term_mark(Kind::HighContrast, &hc), (sys.highlight, sys.highlight_text));
    }
}
