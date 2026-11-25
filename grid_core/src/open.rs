//! 목록 항목을 시스템 연결 앱으로 열기(DBLOPEN1). 문서뷰·사진뷰 공용.
//! 목록 대상 확장자의 실제 파일만 연다(실행 파일·바로가기 실행 방지). Python `docfit_app/open_listed_file.py` 와 같은 규칙.
use std::path::Path;

/// `extensions` 는 소문자·점 없음.
pub fn check_openable(path: &str, extensions: &[&str]) -> Result<(), String> {
    let p = Path::new(path);
    // 이름의 ':'(NTFS 대체 데이터 스트림)은 확장자 검사를 속일 수 있어 거절한다.
    let stream = cfg!(windows) && p.file_name().is_some_and(|n| n.to_string_lossy().contains(':'));
    let listed = p.extension().and_then(|e| e.to_str()).is_some_and(|e| extensions.iter().any(|x| e.eq_ignore_ascii_case(x)));
    if path.contains('\0') || !p.is_absolute() || stream || !listed {
        return Err("목록 대상 파일이 아니어서 열지 않았습니다".into());
    }
    if !p.is_file() {
        return Err("파일을 찾을 수 없습니다".into());
    }
    Ok(())
}

/// 검사를 통과한 경로만 `opener` 에 넘긴다.
pub fn open_with(path: &str, extensions: &[&str], opener: impl FnOnce(&str) -> Result<(), String>) -> Result<(), String> {
    check_openable(path, extensions)?;
    opener(path)
}

/// `ShellExecuteW` 기본 동작(연결 앱). 셸이 메시지 루프를 돌 수 있으므로 호출하는 창은 상태 참조를 쥐지 않은 채 부른다.
#[cfg(windows)]
pub fn shell_open(owner: isize, path: &str) -> Result<(), String> {
    use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let com = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        // 동사를 비우면(null) 확장자의 기본 동작을 쓴다.
        let code = ShellExecuteW(owner as _, std::ptr::null(), wide.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) as isize;
        if com >= 0 {
            CoUninitialize();
        }
        if code <= 32 { Err(format!("파일을 열지 못했습니다(오류 {code})")) } else { Ok(()) }
    }
}
