//! Resolve only the current process user's published index. No enumeration of
//! other SID directories and no directory/file creation occurs here.
use std::path::{Path, PathBuf};

fn valid_sid(sid: &str) -> bool {
    sid.starts_with("S-1-")
        && sid.len() < 185
        && sid[4..]
            .split('-')
            .all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(windows)]
pub fn current_sid() -> Result<String, String> {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::ConvertSidToStringSidW, GetTokenInformation, TOKEN_QUERY, TOKEN_USER,
            TokenUser,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        // Only the real token handle is owned; GetCurrentProcess is a pseudo-handle.
        let _token = OwnedHandle::from_raw_handle(token);
        let mut needed = 0;
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed);
        if needed < std::mem::size_of::<TOKEN_USER>() as u32 || needed > 1024 * 1024 {
            return Err("사용자 토큰 크기가 올바르지 않습니다.".into());
        }
        let mut buffer = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
        if GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        ) == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let user = &*(buffer.as_ptr() as *const TOKEN_USER);
        let mut text = std::ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let mut len = 0;
        while *text.add(len) != 0 {
            len += 1;
        }
        let sid = String::from_utf16(std::slice::from_raw_parts(text, len));
        LocalFree(text.cast());
        let sid = sid.map_err(|e| e.to_string())?;
        if !valid_sid(&sid) {
            return Err("사용자 SID 형식이 올바르지 않습니다.".into());
        }
        Ok(sid)
    }
}

#[cfg(not(windows))]
pub fn current_sid() -> Result<String, String> {
    Err("Windows 사용자 토큰이 필요합니다.".into())
}

fn index_under(base: &Path, sid: &str) -> PathBuf {
    // Both the service and kfind --user append the authenticated SID to root.
    base.join("HwpAutoDocFit").join("index").join(sid)
}

fn select_index(
    program_data: Option<&Path>,
    local_data: Option<&Path>,
    sid: &str,
    service_ready: bool,
) -> Result<PathBuf, String> {
    if !valid_sid(sid) {
        return Err("사용자 SID 형식이 올바르지 않습니다.".into());
    }
    if service_ready {
        if let Some(base) = program_data {
            return Ok(index_under(base, sid));
        }
    }
    local_data
        .map(|base| index_under(base, sid))
        .ok_or_else(|| "LOCALAPPDATA 경로를 찾을 수 없습니다.".into())
}

pub fn default_index() -> Result<PathBuf, String> {
    let sid = current_sid()?;
    let program = std::env::var_os("ProgramData")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let local = std::env::var_os("LOCALAPPDATA")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let service_ready = program
        .as_deref()
        .is_some_and(|base| index_under(base, &sid).join("MANIFEST.kfm").is_file());
    select_index(program.as_deref(), local.as_deref(), &sid, service_ready)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn own_service_index_wins_only_when_published() {
        let service = Path::new("service-root");
        let local = Path::new("user-root");
        let sid = "S-1-5-21-123-456-789-1001";
        assert_eq!(
            select_index(Some(service), Some(local), sid, true).unwrap(),
            index_under(service, sid)
        );
        assert_eq!(
            select_index(Some(service), Some(local), sid, false).unwrap(),
            index_under(local, sid)
        );
        assert_eq!(
            select_index(None, Some(local), sid, true).unwrap(),
            index_under(local, sid)
        );
        assert!(select_index(None, None, sid, false).is_err());
        assert_eq!(
            select_index(Some(service), None, sid, true).unwrap(),
            index_under(service, sid)
        );
    }
    #[test]
    fn sid_components_cannot_escape_or_select_another_directory() {
        for sid in [
            "",
            "../other",
            "S-1-5/other",
            "S-1-5\\other",
            "S-1--5",
            "S-1-5-",
            "S-1-5\0",
        ] {
            assert!(select_index(None, Some(Path::new("user-root")), sid, false).is_err());
        }
        let a = select_index(None, Some(Path::new("user-root")), "S-1-5-21-1001", false).unwrap();
        let b = select_index(None, Some(Path::new("user-root")), "S-1-5-21-1002", false).unwrap();
        assert_ne!(a, b);
    }
    #[test]
    #[cfg(windows)]
    fn token_api_returns_valid_non_path_sid() {
        let sid = current_sid().unwrap();
        assert!(valid_sid(&sid));
        assert_eq!(Path::new(&sid).components().count(), 1);
        assert_eq!(sid, current_sid().unwrap());
    }
}
