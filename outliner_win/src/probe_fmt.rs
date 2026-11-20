//! IME 탐침 로그 서식(순수 함수, Win32 없이 시험한다).

/// 관심 있는 메시지 이름(없으면 None).
pub fn msg_name(msg: u32) -> Option<&'static str> {
    Some(match msg {
        0x0007 => "WM_SETFOCUS",
        0x0008 => "WM_KILLFOCUS",
        0x0006 => "WM_ACTIVATE",
        0x0100 => "WM_KEYDOWN",
        0x0101 => "WM_KEYUP",
        0x0102 => "WM_CHAR",
        0x0104 => "WM_SYSKEYDOWN",
        0x0105 => "WM_SYSKEYUP",
        0x0106 => "WM_SYSCHAR",
        0x0201 => "WM_LBUTTONDOWN",
        0x0202 => "WM_LBUTTONUP",
        0x010D => "WM_IME_STARTCOMPOSITION",
        0x010E => "WM_IME_ENDCOMPOSITION",
        0x010F => "WM_IME_COMPOSITION",
        0x0281 => "WM_IME_SETCONTEXT",
        0x0282 => "WM_IME_NOTIFY",
        0x0283 => "WM_IME_CONTROL",
        0x0284 => "WM_IME_COMPOSITIONFULL",
        0x0285 => "WM_IME_SELECT",
        0x0286 => "WM_IME_CHAR",
        0x0288 => "WM_IME_REQUEST",
        0x0290 => "WM_IME_KEYDOWN",
        0x0291 => "WM_IME_KEYUP",
        _ => return None,
    })
}

/// `WM_IME_COMPOSITION` 의 lParam 플래그.
pub fn gcs_flags(flags: u32) -> String {
    const TABLE: &[(u32, &str)] = &[
        (0x0001, "GCS_COMPREADSTR"),
        (0x0002, "GCS_COMPREADATTR"),
        (0x0004, "GCS_COMPREADCLAUSE"),
        (0x0008, "GCS_COMPSTR"),
        (0x0010, "GCS_COMPATTR"),
        (0x0020, "GCS_COMPCLAUSE"),
        (0x0080, "GCS_CURSORPOS"),
        (0x0100, "GCS_DELTASTART"),
        (0x0200, "GCS_RESULTREADSTR"),
        (0x0400, "GCS_RESULTREADCLAUSE"),
        (0x0800, "GCS_RESULTSTR"),
        (0x1000, "GCS_RESULTCLAUSE"),
        (0x2000, "CS_INSERTCHAR"),
        (0x4000, "CS_NOMOVECARET"),
    ];
    let names: Vec<&str> = TABLE.iter().filter(|(bit, _)| flags & bit != 0).map(|(_, n)| *n).collect();
    if names.is_empty() { format!("0x{flags:X}(조합 없음/취소)") } else { names.join("|") }
}

/// `WM_IME_NOTIFY` 의 wParam.
pub fn imn_name(w: usize) -> String {
    match w {
        0x0001 => "IMN_CLOSESTATUSWINDOW".into(),
        0x0002 => "IMN_OPENSTATUSWINDOW".into(),
        0x0003 => "IMN_CHANGECANDIDATE".into(),
        0x0004 => "IMN_CLOSECANDIDATE".into(),
        0x0005 => "IMN_OPENCANDIDATE".into(),
        0x0006 => "IMN_SETCONVERSIONMODE".into(),
        0x0008 => "IMN_SETOPENSTATUS".into(),
        0x000F => "IMN_PRIVATE".into(),
        other => format!("IMN_0x{other:X}"),
    }
}

/// 가상 키 이름.
pub fn vk_name(vk: u32) -> String {
    match vk {
        0x08 => "VK_BACK".into(),
        0x09 => "VK_TAB".into(),
        0x0D => "VK_RETURN".into(),
        0x10 => "VK_SHIFT".into(),
        0x11 => "VK_CONTROL".into(),
        0x12 => "VK_MENU(Alt)".into(),
        0x15 => "VK_HANGUL".into(),
        0x19 => "VK_HANJA".into(),
        0x1B => "VK_ESCAPE".into(),
        0x20 => "VK_SPACE".into(),
        0x21 => "VK_PRIOR".into(),
        0x22 => "VK_NEXT".into(),
        0x23 => "VK_END".into(),
        0x24 => "VK_HOME".into(),
        0x25 => "VK_LEFT".into(),
        0x26 => "VK_UP".into(),
        0x27 => "VK_RIGHT".into(),
        0x28 => "VK_DOWN".into(),
        0x2E => "VK_DELETE".into(),
        0x70..=0x87 => format!("VK_F{}", vk - 0x6F),
        0xE5 => "VK_PROCESSKEY".into(),
        0x30..=0x39 | 0x41..=0x5A => format!("'{}'", char::from_u32(vk).unwrap_or('?')),
        other => format!("VK_0x{other:02X}"),
    }
}

/// `WM_KEYDOWN/UP` 의 lParam 분해: 반복 횟수, 스캔 코드, 확장 키, 이전 상태(1=눌려 있었음), 전이(1=떼는 중).
pub fn key_lparam(lp: isize) -> String {
    let lp = lp as u32;
    format!(
        "repeat={} scan=0x{:02X} ext={} prev={} up={}",
        lp & 0xFFFF,
        (lp >> 16) & 0xFF,
        (lp >> 24) & 1,
        (lp >> 30) & 1,
        (lp >> 31) & 1
    )
}

/// 글자 한 개를 `U+XXXX '글'` 로(제어 문자는 이름 없이 코드만).
pub fn char_text(code: u32) -> String {
    match char::from_u32(code) {
        Some(c) if !c.is_control() => format!("U+{code:04X} '{c}'"),
        _ => format!("U+{code:04X}"),
    }
}

/// 한 줄 로그: `[HH:MM:SS.mmm] +경과ms 출처 내용`.
pub fn log_line(clock: (u16, u16, u16, u16), elapsed_ms: f64, src: &str, body: &str) -> String {
    format!(
        "[{:02}:{:02}:{:02}.{:03}] +{:>10.3}ms {:<5} {}",
        clock.0, clock.1, clock.2, clock.3, elapsed_ms, src, body
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_are_decoded() {
        assert_eq!(gcs_flags(0x0008 | 0x0010), "GCS_COMPSTR|GCS_COMPATTR");
        assert_eq!(gcs_flags(0x0800), "GCS_RESULTSTR");
        assert!(gcs_flags(0).contains("조합 없음"));
    }

    #[test]
    fn key_names_and_lparam() {
        assert_eq!(vk_name(0xE5), "VK_PROCESSKEY");
        assert_eq!(vk_name(0x0D), "VK_RETURN");
        assert_eq!(vk_name(0x41), "'A'");
        assert_eq!(vk_name(0x74), "VK_F5");
        // repeat 1, scan 0x1C(Enter), 이전 상태 0, 누르는 중
        let lp = 1 | (0x1C << 16);
        assert_eq!(key_lparam(lp), "repeat=1 scan=0x1C ext=0 prev=0 up=0");
    }

    #[test]
    fn char_text_shows_hangul() {
        assert_eq!(char_text('안' as u32), "U+C548 '안'");
        assert_eq!(char_text(0x0D), "U+000D");
    }

    #[test]
    fn log_line_is_stable() {
        let l = log_line((9, 5, 7, 42), 12.3456, "EDIT", "WM_KEYDOWN");
        assert_eq!(l, "[09:05:07.042] +    12.346ms EDIT  WM_KEYDOWN");
    }

    #[test]
    fn known_messages_have_names() {
        assert_eq!(msg_name(0x010D), Some("WM_IME_STARTCOMPOSITION"));
        assert_eq!(msg_name(0x010F), Some("WM_IME_COMPOSITION"));
        assert_eq!(msg_name(0x010E), Some("WM_IME_ENDCOMPOSITION"));
        assert_eq!(msg_name(0x0001), None);
    }
}
