//! 창끼리 파일 넘기기의 보내는 쪽(`sidebar.exe --send`). Win32 호출(창 찾기·수신함 권한)은 bin 에 있다.
//!
//! 수신함: `%LOCALAPPDATA%\HWP_AutoDocFit\open_requests\screen-N\*.json`. 현재 사용자와 SYSTEM 만 열 수 있게
//! 권한을 걸고, 받는 쪽(`docfit_app/handoff.py`)이 형식·개수·파일 존재를 다시 검사한다. 파일 내용은 넘기지 않는다.
//! 요청 상태: 쓰는 중(.tmp) → 대기(.json) → 가져감(.claimed, 한 번만) → 삭제. 10분 지난 대기는 버린다.

use crate::handoff_rules;
use std::path::{Path, PathBuf};

/// 변환 창(`convert_win.exe`)에 인자로 넘길 수 있는 명령줄 길이(Windows 한도 32767 아래 여유).
pub const MAX_COMMAND_LINE: usize = 30_000;

pub fn mailbox_root() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("HWP_AutoDocFit")
        .join("open_requests")
}

pub fn mailbox(root: &Path, menu: u8) -> PathBuf {
    root.join(format!("screen-{menu}"))
}

/// 형식·개수(CTXMENU1 표)와 실제 일반 파일인지(링크·상대 경로·대체 데이터 스트림 거절) 검사한다.
/// `photo_paste` 는 붙임사진 편집(5)을 열어 클립보드를 붙이는 요청이며 파일이 없다.
pub fn check(menu: u8, paths: &[String], photo_paste: bool, is_file: &dyn Fn(&Path) -> bool) -> Result<(), String> {
    if photo_paste {
        return if menu == 5 && paths.is_empty() { Ok(()) } else { Err("붙임사진 편집 요청을 확인해 주세요".into()) };
    }
    if let Some(reason) = handoff_rules::rejection(menu, paths) {
        return Err(reason);
    }
    for value in paths {
        let path = Path::new(value);
        let stream = cfg!(windows) && path.file_name().is_some_and(|n| n.to_string_lossy().contains(':'));
        if value.contains('\0') || !path.is_absolute() || stream || !is_file(path) {
            return Err(format!("파일을 찾을 수 없거나 넘길 수 없는 경로입니다: {value}"));
        }
    }
    Ok(())
}

/// 파일을 그대로 본다(링크는 따라가지 않는다).
pub fn is_plain_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Rust 변환 창에 파일을 인자로 준다(이미 떠 있으면 그 창이 받는다).
    ConvertArguments,
    /// 수신함에 남기고 앱 화면 창을 연다.
    Mailbox,
}

pub fn route(menu: u8, paths: &[String], convert_exe: bool, app_screen_open: bool) -> Route {
    let length: usize = paths.iter().map(|p| p.encode_utf16().count() + 3).sum();
    if menu == 10 && convert_exe && !app_screen_open && length <= MAX_COMMAND_LINE {
        Route::ConvertArguments
    } else {
        Route::Mailbox
    }
}

/// 요청 하나를 원자적으로 남긴다(.tmp → .json).
pub fn write_request(folder: &Path, paths: &[String], photo_paste: bool, unique: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(folder)?;
    let body = serde_json::json!({ "paths": paths, "photo_paste": photo_paste }).to_string();
    let temporary = folder.join(format!("{unique}.tmp"));
    std::fs::write(&temporary, body)?;
    let done = folder.join(format!("{unique}.json"));
    std::fs::rename(&temporary, &done)?;
    Ok(done)
}

/// `--list` 파일: `{"paths":[...]}`.
pub fn parse_list(text: &str) -> Result<Vec<String>, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| "보낼 목록을 읽지 못했습니다")?;
    value
        .get("paths")
        .and_then(|v| v.as_array())
        .and_then(|list| list.iter().map(|v| v.as_str().map(str::to_owned)).collect())
        .ok_or_else(|| "보낼 목록을 읽지 못했습니다".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn check_rejects_bad_paths_before_anything_is_written() {
        let here = std::env::current_dir().unwrap().join("a.pdf").to_string_lossy().into_owned();
        let yes = |_: &Path| true;
        assert!(check(11, &paths(&[&here]), false, &yes).is_ok());
        assert!(check(11, &paths(&["a.pdf"]), false, &yes).is_err(), "상대 경로");
        assert!(check(11, &paths(&[&here]), false, &|_| false).is_err(), "없는 파일");
        assert!(check(0, &paths(&[&format!("{here}:s.txt")]), false, &yes).is_err() || !cfg!(windows));
        assert!(check(5, &[], true, &yes).is_ok());
        assert!(check(11, &[], true, &yes).is_err());
        assert!(check(5, &paths(&[&here]), false, &yes).is_err(), "붙임사진 편집은 파일을 받지 않는다");
    }

    #[test]
    fn request_is_written_atomically_and_listed_paths_are_strings() {
        let folder = std::env::temp_dir().join(format!("docfit-handoff-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let done = write_request(&folder, &paths(&[r"C:\가 나\a.pdf"]), false, "1-2").unwrap();
        assert_eq!(parse_list(&std::fs::read_to_string(&done).unwrap()).unwrap(), [r"C:\가 나\a.pdf"]);
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1, "임시 파일이 남지 않는다");
        std::fs::remove_dir_all(&folder).unwrap();
        assert!(parse_list(r#"{"paths":[1]}"#).is_err());
        assert!(parse_list("[]").is_err());
    }

    #[test]
    fn conversion_window_takes_arguments_unless_too_long_or_app_screen_is_open() {
        let few = paths(&[r"C:\a.pdf"]);
        assert_eq!(route(10, &few, true, false), Route::ConvertArguments);
        assert_eq!(route(10, &few, false, false), Route::Mailbox);
        assert_eq!(route(10, &few, true, true), Route::Mailbox);
        assert_eq!(route(0, &few, true, false), Route::Mailbox);
        let many = vec![format!(r"C:\{}.pdf", "가".repeat(100)); 400];
        assert_eq!(route(10, &many, true, false), Route::Mailbox);
    }
}
