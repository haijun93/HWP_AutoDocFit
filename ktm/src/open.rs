//! Validate an indexed path before handing it to the desktop shell.
use std::path::{Path, PathBuf};

pub fn validate_path(path: &str) -> Result<(), String> {
    let bytes = path.as_bytes();
    if path.contains('\0')
        || bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !matches!(bytes[2], b'\\' | b'/')
        || path[2..].contains(':')
        || path
            .split(['\\', '/'])
            .skip(1)
            .any(|part| part.ends_with([' ', '.']))
    {
        return Err("로컬 드라이브의 절대 경로만 열 수 있습니다".into());
    }
    let extension = path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .rsplit('.')
        .next()
        .unwrap_or("")
        .trim_end_matches([' ', '.'])
        .to_ascii_lowercase();
    const BLOCKED: &[&str] = &[
        "exe",
        "com",
        "bat",
        "cmd",
        "ps1",
        "psm1",
        "psd1",
        "vbs",
        "vbe",
        "js",
        "jse",
        "wsf",
        "wsh",
        "msi",
        "msp",
        "scr",
        "pif",
        "cpl",
        "hta",
        "lnk",
        "url",
        "reg",
        "scf",
        "appref-ms",
    ];
    if BLOCKED.contains(&extension.as_str()) {
        return Err("실행 파일과 바로가기는 검색창에서 열 수 없습니다".into());
    }
    Ok(())
}

pub fn target(path: &str, folder: bool) -> Result<PathBuf, String> {
    validate_path(path)?;
    let path = Path::new(path);
    if !path.is_file() {
        return Err("문서 파일이 없거나 접근할 수 없습니다".into());
    }
    if folder {
        path.parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| "상위 폴더가 없습니다".into())
    } else {
        Ok(path.to_path_buf())
    }
}

pub fn open(path: &str, folder: bool) -> Result<(), String> {
    let target = target(path, folder)?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
        let wide: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        let verb: Vec<u16> = "open\0".encode_utf16().collect();
        let code = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                wide.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        } as isize;
        if code <= 32 {
            return Err(format!("문서를 열 수 없습니다 (Windows {code})"));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = target;
        Err("Windows에서만 열 수 있습니다".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_nonlocal_and_relative_paths() {
        for path in [
            "file.hwp",
            "C:file.hwp",
            "\\\\host\\x.hwp",
            "https://x",
            "C:\\x.hwp:evil",
            "C:\\x\0.hwp",
            "C:\\evil.exe.",
            "C:\\evil.exe ",
        ] {
            assert!(validate_path(path).is_err(), "{path:?}");
        }
    }
    #[test]
    fn blocks_execution_and_shortcuts_case_insensitively() {
        for extension in ["EXE", "cmd", "Ps1", "lnk", "url", "hta", "reg", "msi"] {
            assert!(validate_path(&format!("C:\\docs\\보고서.{extension}")).is_err());
        }
    }
    #[test]
    fn allows_document_paths() {
        for path in [
            "C:\\docs\\보고서.hwpx",
            "D:/docs/계획.pdf",
            "C:\\docs\\실행.exe.hwpx",
        ] {
            assert!(validate_path(path).is_ok());
        }
    }
    #[test]
    fn missing_document_is_an_error() {
        assert!(target("Z:\\missing-mg1-document.hwpx", false).is_err());
    }
}
