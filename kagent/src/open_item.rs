use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

pub use crate::open_rules::safe_web_url;
use crate::open_rules::forbidden;
use crate::util::wide;

pub const TEMP_PREFIX: &str = "kclip-open-";
pub const CLEANUP_MAX_AGE_SECS: u64 = 30 * 24 * 3600; // 30일
const WEB_MARKER: &[u8] = b"\0KAGENT-WEB\0";

/// 임시 파일 이름 규칙: kclip-open-<id>.<ext>
pub fn temp_file_name(id: i64, ext: &str) -> String {
    format!("{TEMP_PREFIX}{id}.{ext}")
}

/// 임시 파일 전체 경로: %TEMP%\kclip-open-<id>.<ext>
pub fn temp_file_path(id: i64, ext: &str) -> PathBuf {
    std::env::temp_dir().join(temp_file_name(id, ext))
}

pub fn classify_web_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if let Some(url) = trimmed.strip_prefix("www.") {
        let normalized = format!("https://{trimmed}");
        let _ = url;
        return safe_web_url(&normalized).map(str::to_owned);
    }
    safe_web_url(trimmed).map(str::to_owned)
}

/// Extract a bounded SourceURL field from CF_HTML bytes.
pub fn source_url_from_html(html: &[u8]) -> Option<String> {
    let header_end = html.iter().position(|&b| b == b'<').unwrap_or(html.len());
    let header = String::from_utf8_lossy(&html[..header_end]);
    if header.contains('\n') && !header.contains("\r\n") { return None; }
    let normalized = header.replace("\r\n", "\n");
    if normalized.lines().any(|line| line.trim_start().starts_with("javascript:") || line.trim_start().starts_with("file:") || line.trim_start().starts_with("ms-")) { return None; }
    let value = normalized.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case("SourceURL").then_some(value.trim())
    })?;
    safe_web_url(value).map(str::to_owned)
}

pub fn encode_web_extra(formats: &[(String, Vec<u8>)], url: &str) -> Vec<u8> {
    let mut out = WEB_MARKER.to_vec();
    out.extend_from_slice(url.as_bytes());
    out.push(0);
    out.extend_from_slice(&crate::clip::pack_formats(formats));
    out
}

pub fn web_url_from_extra(extra: &[u8]) -> Option<String> {
    let rest = extra.strip_prefix(WEB_MARKER)?;
    let end = rest.iter().position(|&b| b == 0)?;
    let url = std::str::from_utf8(&rest[..end]).ok()?;
    safe_web_url(url).map(str::to_owned)
}

pub fn formats_from_extra(extra: &[u8]) -> Vec<(String, Vec<u8>)> {
    if let Some(rest) = extra.strip_prefix(WEB_MARKER) {
        let Some(end) = rest.iter().position(|&b| b == 0) else { return Vec::new() };
        return crate::clip::unpack_formats(&rest[end + 1..]);
    }
    crate::clip::unpack_formats(extra)
}

/// %TEMP% 내의 오래된 kclip-open-* 파일들을 30일 정리 규칙에 따라 삭제한다.
pub fn cleanup_old_temp_files(dir: &Path, max_age_secs: u64) {
    let now = SystemTime::now();
    let max_age = Duration::from_secs(max_age_secs);
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                if let Some(file_name) = p.file_name().and_then(|n| n.to_str()) {
                    if file_name.starts_with(TEMP_PREFIX) {
                        if let Ok(meta) = p.metadata() {
                            if let Ok(modified) = meta.modified() {
                                if let Ok(elapsed) = now.duration_since(modified) {
                                    if elapsed >= max_age {
                                        let _ = std::fs::remove_file(&p);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn cleanup(data_dir: &str) {
    cleanup_old_temp_files(&std::env::temp_dir(), CLEANUP_MAX_AGE_SECS);
    let path = Path::new(data_dir).join("open");
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() { let _ = std::fs::remove_file(p); }
        }
    }
}

fn write_temp_file(id: i64, ext: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir();
    cleanup_old_temp_files(&dir, CLEANUP_MAX_AGE_SECS);
    let path = temp_file_path(id, ext);
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

fn shell_open(hwnd: HWND, path: &str, verb: &str) -> bool {
    let result = unsafe { ShellExecuteW(hwnd, wide(verb).as_ptr(), wide(path).as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
    result as isize > 32
}

fn notify(hwnd: HWND, text: &str) {
    unsafe { windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(hwnd, wide(text).as_ptr(), wide("몽땅클립보드").as_ptr(), windows_sys::Win32::UI::WindowsAndMessaging::MB_OK | windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONINFORMATION); }
}

/// 항목을 연결 프로그램으로 직접 열기.
/// - 웹: http/https 주소만 기본 브라우저로 엶.
/// - 파일: 첫 파일(여러 개면 최대 10개까지) ShellExecute open. 실행 파일(.exe/.bat/.cmd/.ps1/.vbs/.js/.lnk 등) 차단.
/// - 폴더: 탐색기(explore)로 엶.
/// - 글: UTF-8 BOM txt를 %TEMP%/kclip-open-<id>.txt로 저장 후 기본 텍스트 뷰어로 엶.
/// - 그림: %TEMP%/kclip-open-<id>.png로 저장 후 기본 이미지 뷰어로 엶.
pub fn open(hwnd: HWND, id: i64, kind: &str, data: &[u8], extra: &[u8], choose_app: bool) {
    let url = web_url_from_extra(extra).or_else(|| if kind == "text" { classify_web_text(&String::from_utf8_lossy(data)) } else { None });
    if let Some(url) = url {
        if let Some(safe_url) = safe_web_url(&url) {
            if !shell_open(hwnd, safe_url, "open") {
                notify(hwnd, "기본 브라우저에서 웹주소를 열지 못했습니다.");
            }
        } else {
            notify(hwnd, "안전하지 않거나 지원하지 않는 웹주소 형식입니다.");
        }
        return;
    }
    if kind == "files" {
        let paths_str = String::from_utf8_lossy(data);
        let path_list: Vec<&str> = paths_str.split('\n').map(str::trim).filter(|s| !s.is_empty()).take(10).collect();
        if path_list.is_empty() {
            notify(hwnd, "열 파일 또는 폴더 경로가 없습니다.");
            return;
        }
        for path in path_list {
            let p = Path::new(path);
            if !p.exists() {
                notify(hwnd, &format!("경로가 없습니다:\n{path}"));
                continue;
            }
            if p.is_dir() {
                // 폴더: 탐색기로 연다
                if !shell_open(hwnd, path, "explore") && !shell_open(hwnd, path, "open") {
                    notify(hwnd, &format!("폴더를 열지 못했습니다:\n{path}"));
                }
                continue;
            }
            // 보안: 명령 실행 파일(.exe/.bat/.cmd/.ps1/.vbs/.js/.lnk 등)은 열지 않음
            if forbidden(path) {
                notify(hwnd, &format!("보안상 실행 파일은 직접 열 수 없습니다:\n{path}"));
                continue;
            }
            if !shell_open(hwnd, path, if choose_app { "openas" } else { "open" }) {
                notify(hwnd, &format!("항목을 열지 못했습니다:\n{path}"));
            }
        }
        return;
    }
    let (ext, contents) = match kind {
        "image" => ("png", data.to_vec()),
        "text" => {
            let mut b = vec![0xef, 0xbb, 0xbf];
            b.extend_from_slice(data);
            ("txt", b)
        }
        _ => {
            notify(hwnd, "지원하지 않는 항목입니다.");
            return;
        }
    };
    let path = match write_temp_file(id, ext, &contents) {
        Ok(p) => p,
        Err(e) => {
            notify(hwnd, &format!("임시 파일을 만들지 못했습니다:\n{e}"));
            return;
        }
    };
    if !shell_open(hwnd, &path.to_string_lossy(), if choose_app { "openas" } else { "open" }) {
        notify(hwnd, "항목을 열지 못했습니다.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_file_naming_rule() {
        assert_eq!(temp_file_name(42, "txt"), "kclip-open-42.txt");
        assert_eq!(temp_file_name(100, "png"), "kclip-open-100.png");
        let path = temp_file_path(42, "txt");
        assert_eq!(path.file_name().unwrap(), "kclip-open-42.txt");
        assert_eq!(path.parent().unwrap(), std::env::temp_dir().as_path());
    }

    #[test]
    fn utf8_text_is_written_with_bom() {
        let path = temp_file_path(99991, "txt");
        let _ = std::fs::remove_file(&path);
        let res = write_temp_file(99991, "txt", &[0xef, 0xbb, 0xbf, b'k', b'c', b'l', b'i', b'p']);
        assert!(res.is_ok());
        let written = std::fs::read(&path).unwrap();
        assert_eq!(written, [0xef, 0xbb, 0xbf, b'k', b'c', b'l', b'i', b'p']);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn temp_cleanup_30_days_rule() {
        let dir = std::env::temp_dir().join(format!("kclip-test-clean-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);

        let old_file = dir.join("kclip-open-old.txt");
        let new_file = dir.join("kclip-open-new.txt");
        let other_file = dir.join("other-file.txt");

        std::fs::write(&old_file, b"old").unwrap();
        std::fs::write(&new_file, b"new").unwrap();
        std::fs::write(&other_file, b"other").unwrap();

        // max_age_secs = 0 이면 kclip-open-* 파일이 즉시 대상이 됨
        cleanup_old_temp_files(&dir, 0);

        assert!(!old_file.exists(), "kclip-open-old.txt는 삭제되어야 함");
        assert!(!new_file.exists(), "kclip-open-new.txt는 삭제되어야 함");
        assert!(other_file.exists(), "other-file.txt는 kclip 접두사가 아니므로 보존되어야 함");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn web_text_classification_and_scheme_safety() {
        assert_eq!(classify_web_text("  www.example.com/path  ").as_deref(), Some("https://www.example.com/path"));
        assert_eq!(classify_web_text("https://example.com").as_deref(), Some("https://example.com"));
        assert_eq!(classify_web_text("http://example.com").as_deref(), Some("http://example.com"));

        // http/https 이외의 위험한 스킴은 모두 거름
        for bad in [
            "file:///C:/Windows",
            "javascript:alert(1)",
            "ms-settings:",
            "ftp://example.com",
            "data:text/html;base64,PHNjcmlwdD4=",
            "https://a.com\nnext",
            "https://a.com\u{7f}",
        ] {
            assert!(classify_web_text(bad).is_none(), "스킴 거르기 실패: {bad:?}");
        }
        assert!(classify_web_text(&format!("https://{}", "a".repeat(2048))).is_none());
    }

    #[test]
    fn security_forbidden_executable_files_blocked() {
        for p in [
            "C:\\tool.exe",
            "D:\\script.BAT",
            "C:\\Windows\\cmd.CMD",
            "E:\\setup.ps1",
            "F:\\macro.vbs",
            "C:\\evil.js",
            "Desktop\\shortcut.lnk",
        ] {
            assert!(forbidden(p), "실행 파일 차단 검증 실패: {p}");
        }
        assert!(!forbidden("C:\\safe.txt"));
        assert!(!forbidden("D:\\photo.png"));
        assert!(!forbidden("E:\\docs\\guide.pdf"));
    }

    #[test]
    fn cf_html_source_url_is_parsed_and_validated() {
        assert_eq!(source_url_from_html(b"Version:1.0\r\nStartHTML:00000097\r\nSourceURL:https://example.com/a\r\n<html>"), Some("https://example.com/a".into()));
        assert!(source_url_from_html(b"Version:1.0\r\nSourceURL:file:///secret\r\n<html>").is_none());
        assert!(source_url_from_html(b"Version:1.0\r\nSourceURL:https://a.com\njavascript:bad\r\n<html>").is_none());
    }
}
