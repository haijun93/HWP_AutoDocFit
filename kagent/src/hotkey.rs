//! 전역 단축키 글자 해석과 등록 실패 알림 판정(순수 함수).
//! `main.rs`에서 내용 변경 없이 옮겼다(2026-11-12, VF V1).
//! kvf(가상 폴더 창)도 이 파일을 컴파일한다. `std`·`windows-sys`·`crate::util` 말고는 쓰지 말 것.

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN};

pub(crate) fn parse_hotkey(text: &str) -> Option<(u32, u32)> {
    let mut mods = MOD_NOREPEAT;
    let mut vk = 0u32;
    for part in text.split('+').map(|p| p.trim().to_ascii_lowercase()) {
        match part.as_str() {
            "ctrl" | "control" => mods |= MOD_CONTROL,
            "shift" => mods |= MOD_SHIFT,
            "alt" => mods |= MOD_ALT,
            "win" => mods |= MOD_WIN,
            p if p.len() == 1 && p.as_bytes()[0].is_ascii_alphanumeric() => vk = p.to_ascii_uppercase().as_bytes()[0] as u32,
            p if p.starts_with('f') => {
                let n: u32 = p[1..].parse().ok()?;
                if !(1..=24).contains(&n) {
                    return None;
                }
                vk = 0x70 + n - 1;
            }
            _ => return None,
        }
    }
    (vk != 0 && mods != MOD_NOREPEAT).then_some((mods, vk))
}

pub(crate) fn hotkey_rejection_reason(text: &str) -> Option<&'static str> {
    let parts: Vec<_> = text.split('+').map(|p| p.trim().to_ascii_lowercase()).collect();
    if parts.iter().any(|p| p == "win") { return Some("Windows 키 조합은 예약되어 있어 사용할 수 없습니다."); }
    if parts == ["alt", "f4"] || parts == ["f4", "alt"] { return Some("Alt+F4는 창 닫기 예약 키입니다."); }
    if parts.iter().any(|p| p == "del" || p == "delete")
        && parts.iter().any(|p| p == "ctrl") && parts.iter().any(|p| p == "alt") {
        return Some("Ctrl+Alt+Delete는 Windows 보안 예약 키입니다.");
    }
    None
}

/// 성공하면 실패 기억을 초기화하고, 같은 조합의 연속 실패는 한 번만 알린다.
pub(crate) fn hotkey_failure_notice(previous: Option<&str>, text: &str, ok: bool) -> (Option<String>, bool) {
    if ok { (None, false) } else { (Some(text.to_string()), previous != Some(text)) }
}

pub(crate) fn hotkey_failure_body(text: &str, valid: bool) -> String {
    let reason = if valid { "다른 프로그램이 이미 쓰고 있습니다" } else { "쓸 수 없는 조합입니다" };
    format!("{text}는 {reason}. 트레이 아이콘을 눌러 열 수 있고, 설정에서 다른 단축키로 바꿀 수 있습니다.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkey_notice_transitions() {
        let (state, notify) = hotkey_failure_notice(None, "Ctrl+Shift+V", false);
        assert!(notify);
        let (state, notify) = hotkey_failure_notice(state.as_deref(), "Ctrl+Shift+V", false);
        assert!(!notify);
        let (state, notify) = hotkey_failure_notice(state.as_deref(), "Ctrl+Alt+F11", false);
        assert!(notify);
        let (state, notify) = hotkey_failure_notice(state.as_deref(), "Ctrl+Shift+V", false);
        assert!(notify);
        let (state, notify) = hotkey_failure_notice(state.as_deref(), "Ctrl+Shift+V", true);
        assert!(!notify);
        assert!(state.is_none());
        assert!(hotkey_failure_notice(state.as_deref(), "Ctrl+Shift+V", false).1);
        assert_eq!(hotkey_failure_notice(None, "Ctrl+Shift+V", true), (None, false));
    }

    #[test]
    fn invalid_hotkey_notice_reason_and_deduplication() {
        for text in ["V", "Ctrl+Space"] {
            assert!(parse_hotkey(text).is_none());
            let (state, notify) = hotkey_failure_notice(None, text, false);
            assert!(notify);
            assert!(!hotkey_failure_notice(state.as_deref(), text, false).1);
            assert!(hotkey_failure_body(text, false).starts_with(&format!("{text}는 쓸 수 없는 조합입니다.")));
        }
        assert!(hotkey_failure_body("Ctrl+Shift+V", true).starts_with("Ctrl+Shift+V는 다른 프로그램이 이미 쓰고 있습니다."));
    }

    #[test]
    fn hotkeys() {
        assert_eq!(parse_hotkey("Ctrl+Shift+V"), Some((MOD_NOREPEAT | MOD_CONTROL | MOD_SHIFT, 'V' as u32)));
        assert_eq!(parse_hotkey("Alt+F2"), Some((MOD_NOREPEAT | MOD_ALT, 0x71)));
        assert_eq!(parse_hotkey("V"), None);
        assert_eq!(parse_hotkey("Ctrl+Space"), None);
    }

    #[test]
    fn rejects_reserved_hotkey_combinations_with_reasons() {
        assert!(hotkey_rejection_reason("Win+V").is_some());
        assert!(hotkey_rejection_reason("Alt+F4").is_some());
        assert!(hotkey_rejection_reason("Ctrl+Alt+Delete").is_some());
        assert_eq!(hotkey_rejection_reason("Ctrl+Shift+V"), None);
    }
}
