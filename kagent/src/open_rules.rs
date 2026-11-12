//! 항목을 열 때의 규칙(순수 함수): 실행 파일 확장자 차단, 안전한 웹 주소 판정.
//! `open_item.rs`에서 내용 변경 없이 옮겼다(2026-11-12, VF V1).
//! kvf(가상 폴더 창)도 이 파일을 컴파일한다. `std`·`windows-sys`·`crate::util` 말고는 쓰지 말 것.

use std::path::Path;

const FORBIDDEN: &[&str] = &["exe", "com", "bat", "cmd", "ps1", "psm1", "vbs", "vbe", "js", "jse", "wsf", "wsh", "msc", "scr", "pif", "cpl", "hta", "lnk", "url", "reg", "msi", "msp", "dll"];
pub const MAX_WEB_URL_CHARS: usize = 2048;

/// Accept only bounded HTTP(S) URLs without control characters or whitespace.
pub fn safe_web_url(url: &str) -> Option<&str> {
    if url.is_empty() || url.chars().count() > MAX_WEB_URL_CHARS
        || url.chars().any(|c| c.is_control() || c.is_whitespace())
    { return None; }
    let rest = if let Some(rest) = url.strip_prefix("http://") { rest }
        else if let Some(rest) = url.strip_prefix("https://") { rest }
        else { return None };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.starts_with('.') || authority.ends_with('.') { return None; }
    Some(url)
}

fn extension(path: &str) -> Option<String> {
    Path::new(path).extension().and_then(|s| s.to_str()).map(str::to_ascii_lowercase)
}

pub(crate) fn forbidden(path: &str) -> bool {
    extension(path).is_some_and(|ext| FORBIDDEN.contains(&ext.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_extensions_are_blocked_case_insensitively() {
        for p in ["C:\\a.exe", "C:\\b.BAT", "C:\\c.cmd", "C:\\d.ps1", "C:\\e.vbs", "C:\\f.js", "C:\\g.lnk"] { assert!(forbidden(p), "{p}"); }
        assert!(!forbidden("C:\\safe.txt"));
    }
}
