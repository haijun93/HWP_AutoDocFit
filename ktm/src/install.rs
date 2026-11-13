//! Explicit per-user install actions. 로그인 자동 실행은 설치할 때 기본 켬
//! (사용자 지시 MG1 추가분), `--no-autostart`로 끈다.
use std::path::{Path, PathBuf};

/// 명령줄 → 차례로 실행할 설치 동작. 설치 명령이 없으면 None(일반 상주 실행).
pub fn plan(args: &[String]) -> Option<Vec<&'static str>> {
    let has = |name: &str| args.iter().any(|a| a == name);
    if has("--uninstall") {
        Some(vec!["autostart-off", "uninstall"])
    } else if has("--install") {
        if has("--no-autostart") {
            Some(vec!["install"])
        } else {
            Some(vec!["install", "autostart-on"])
        }
    } else if has("--autostart-on") {
        Some(vec!["autostart-on"])
    } else if has("--autostart-off") {
        Some(vec!["autostart-off"])
    } else {
        None
    }
}

fn sibling(path: &Path, suffix: &str) -> Result<PathBuf, String> {
    let name = path.file_name().ok_or("설치 파일 이름이 없습니다")?;
    let mut name = name.to_os_string();
    name.push(suffix);
    Ok(path.with_file_name(name))
}

pub fn install_file(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_file() {
        return Err("설치 원본 파일이 없습니다".into());
    }
    if source == destination {
        return Err("설치 원본과 대상은 달라야 합니다".into());
    }
    if destination.exists() && !destination.is_file() {
        return Err("설치 대상이 파일이 아닙니다".into());
    }
    let stage = sibling(destination, &format!(".stage-{}", std::process::id()))?;
    let backup = sibling(destination, ".rollback")?;
    if backup.exists() || stage.exists() {
        return Err("이전 설치의 임시 파일을 확인해 주세요".into());
    }
    if let Some(parent) = destination.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let bytes = std::fs::read(source).map_err(|e| e.to_string())?;
    let result = (|| {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stage)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        if std::fs::read(&stage).map_err(|e| e.to_string())? != bytes {
            return Err("설치 사본 검증 실패".into());
        }
        let replacing = destination.exists();
        if replacing {
            std::fs::rename(destination, &backup).map_err(|e| format!("기존 앱 교체 실패: {e}"))?;
        }
        if let Err(error) = std::fs::rename(&stage, destination) {
            if replacing {
                std::fs::rename(&backup, destination)
                    .map_err(|rollback| format!("설치 실패: {error}; 복구 실패: {rollback}"))?;
            }
            return Err(format!("설치 실패: {error}"));
        }
        if replacing {
            std::fs::remove_file(&backup)
                .map_err(|e| format!("설치는 완료했으나 이전 사본 정리 실패: {e}"))?;
        }
        Ok(())
    })();
    if stage.exists() {
        let _ = std::fs::remove_file(stage);
    }
    result
}

pub fn startup_command(destination: &Path) -> Result<String, String> {
    let value = destination
        .to_str()
        .ok_or("설치 경로가 올바르지 않습니다")?;
    if value.contains(['"', '\0', '\r', '\n']) {
        return Err("설치 경로가 올바르지 않습니다".into());
    }
    Ok(format!("\"{value}\" --hidden"))
}

#[cfg(windows)]
fn autostart(destination: &Path, enabled: bool) -> Result<(), String> {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::*;
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0"
        .encode_utf16()
        .collect();
    let name: Vec<u16> = "HwpAutoDocFit.MongttangSearch\0".encode_utf16().collect();
    let mut handle = std::ptr::null_mut();
    let command = if enabled {
        Some(startup_command(destination)?)
    } else {
        None
    };
    unsafe {
        let code = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut handle,
            std::ptr::null_mut(),
        );
        if code != ERROR_SUCCESS {
            return Err(format!("자동 실행 설정 열기 실패: {code}"));
        }
        let result = if enabled {
            let value: Vec<u16> = command
                .as_ref()
                .unwrap()
                .encode_utf16()
                .chain(Some(0))
                .collect();
            RegSetValueExW(
                handle,
                name.as_ptr(),
                0,
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            )
        } else {
            RegDeleteValueW(handle, name.as_ptr())
        };
        RegCloseKey(handle);
        if result == ERROR_SUCCESS || (!enabled && result == ERROR_FILE_NOT_FOUND) {
            Ok(())
        } else {
            Err(format!("자동 실행 설정 실패: {result}"))
        }
    }
}

#[cfg(not(windows))]
fn autostart(_: &Path, _: bool) -> Result<(), String> {
    Err("Windows에서만 자동 실행을 설정할 수 있습니다".into())
}

pub fn execute(action: &str, source: &Path, destination: &Path) -> Result<(), String> {
    match action.trim_start_matches('-') {
        "install" => install_file(source, destination),
        "uninstall" => {
            // Remove only the named executable; never recursively remove user directories.
            match std::fs::remove_file(destination) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(format!("제거 실패: {e}")),
            }
        }
        "autostart-on" => {
            if !destination.is_file() {
                return Err("자동 실행할 설치 파일이 없습니다".into());
            }
            autostart(destination, true)
        }
        "autostart-off" => autostart(destination, false),
        _ => Err("알 수 없는 설치 명령입니다".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("mg1-install-{}-{nonce}", std::process::id()));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn source(&self) -> PathBuf {
            let path = self.0.join("source.exe");
            std::fs::write(&path, b"new-binary").unwrap();
            path
        }
        fn destination(&self) -> PathBuf {
            self.0.join("ktm.exe")
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            for item in std::fs::read_dir(&self.0).unwrap() {
                let path = item.unwrap().path();
                if path.is_file() {
                    let _ = std::fs::remove_file(path);
                } else {
                    let _ = std::fs::remove_dir(path);
                }
            }
            let _ = std::fs::remove_dir(&self.0);
        }
    }
    #[test]
    fn install_copies_verified_bytes() {
        let t = Temp::new();
        install_file(&t.source(), &t.destination()).unwrap();
        assert_eq!(std::fs::read(t.destination()).unwrap(), b"new-binary");
    }
    #[test]
    fn replacement_removes_backup() {
        let t = Temp::new();
        std::fs::write(t.destination(), b"old").unwrap();
        install_file(&t.source(), &t.destination()).unwrap();
        assert!(!sibling(&t.destination(), ".rollback").unwrap().exists());
    }
    #[test]
    fn missing_source_preserves_old_app() {
        let t = Temp::new();
        std::fs::write(t.destination(), b"old").unwrap();
        assert!(install_file(&t.0.join("missing"), &t.destination()).is_err());
        assert_eq!(std::fs::read(t.destination()).unwrap(), b"old");
    }
    #[test]
    fn same_source_is_rejected() {
        let t = Temp::new();
        let source = t.source();
        assert!(install_file(&source, &source).is_err());
    }
    #[test]
    fn directory_target_is_rejected() {
        let t = Temp::new();
        std::fs::create_dir(t.destination()).unwrap();
        assert!(install_file(&t.source(), &t.destination()).is_err());
    }
    #[test]
    fn stale_backup_is_preserved() {
        let t = Temp::new();
        let backup = sibling(&t.destination(), ".rollback").unwrap();
        std::fs::write(&backup, b"old").unwrap();
        assert!(install_file(&t.source(), &t.destination()).is_err());
        assert_eq!(std::fs::read(backup).unwrap(), b"old");
    }
    #[test]
    fn uninstall_is_idempotent() {
        let t = Temp::new();
        let source = t.source();
        install_file(&source, &t.destination()).unwrap();
        execute("--uninstall", &source, &t.destination()).unwrap();
        execute("--uninstall", &source, &t.destination()).unwrap();
        assert!(source.exists());
    }
    #[test]
    fn startup_command_quotes_paths() {
        assert_eq!(
            startup_command(Path::new("C:/user apps/ktm.exe")).unwrap(),
            "\"C:/user apps/ktm.exe\" --hidden"
        );
        assert!(startup_command(Path::new("bad\"path")).is_err());
    }
    #[test]
    fn install_enables_autostart_by_default() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(plan(&args(&["ktm", "--install"])).unwrap(), ["install", "autostart-on"]);
        assert_eq!(plan(&args(&["ktm", "--install", "--no-autostart"])).unwrap(), ["install"]);
        assert_eq!(plan(&args(&["ktm", "--uninstall"])).unwrap(), ["autostart-off", "uninstall"]);
        assert_eq!(plan(&args(&["ktm", "--autostart-off"])).unwrap(), ["autostart-off"]);
        assert!(plan(&args(&["ktm", "--show"])).is_none());
    }
    #[test]
    fn unknown_action_is_rejected() {
        let t = Temp::new();
        assert!(execute("unknown", &t.source(), &t.destination()).is_err());
    }
}
