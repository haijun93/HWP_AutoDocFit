//! Explicit per-user install actions. 로그인 자동 실행은 설치할 때 기본 켬
//! (사용자 지시 MG1 추가분), `--no-autostart`로 끈다.
use std::path::{Path, PathBuf};

/// 명령줄 → 차례로 실행할 설치 동작. 설치 명령이 없으면 None(일반 상주 실행).
/// `quit`(떠 있는 몽땅검색 끄기)·`start`(설치본을 숨긴 채 시작)는 Win32가 필요해 실행 파일 쪽이 맡는다.
/// 수명 상태: 꺼짐(설치 없음) → `--install` → 켜짐(설치·자동 실행·실행 중) → `--disable` → 꺼짐(설치본만 남음)
/// → `--uninstall` → 설치 없음. 교체 전에는 항상 먼저 끈다(실행 중인 exe는 지울 수 없음).
/// `disabled`: 사용자가 끈 표시(`--disable`이 남기는 [`disabled_marker`])가 있는지.
/// `--respect-disabled`(앱 시작·설치 프로그램이 쓰는 갱신)는 끈 사용자에게 다시 켜지 않고 설치본만 바꾼다.
pub fn plan(args: &[String], disabled: bool) -> Option<Vec<&'static str>> {
    let has = |name: &str| args.iter().any(|a| a == name);
    if has("--uninstall") {
        Some(vec!["quit", "autostart-off", "uninstall", "enable-mark"])
    } else if has("--install") {
        if disabled && has("--respect-disabled") {
            Some(vec!["quit", "install"])
        } else if has("--no-autostart") {
            Some(vec!["enable-mark", "quit", "install", "start"])
        } else {
            Some(vec!["enable-mark", "quit", "install", "autostart-on", "start"])
        }
    } else if has("--disable") {
        Some(vec!["disable-mark", "quit", "autostart-off"])
    } else if has("--quit") {
        Some(vec!["quit"])
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
    let key: Vec<u16> = RUN_KEY.encode_utf16().chain(Some(0)).collect();
    let name: Vec<u16> = RUN_NAME.encode_utf16().chain(Some(0)).collect();
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

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
pub const RUN_NAME: &str = "HwpAutoDocFit.MongttangSearch";

/// HKCU Run에 등록된 몽땅검색 명령(없으면 None).
#[cfg(windows)]
pub fn autostart_command() -> Option<String> {
    use windows_sys::Win32::System::Registry::*;
    let key: Vec<u16> = RUN_KEY.encode_utf16().chain(Some(0)).collect();
    let name: Vec<u16> = RUN_NAME.encode_utf16().chain(Some(0)).collect();
    let mut buffer = vec![0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    let code = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if code != 0 {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1).min(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..len]))
}

#[cfg(not(windows))]
pub fn autostart_command() -> Option<String> {
    None
}

/// 자동 실행 명령이 이 설치본을 가리키는지(경로 대소문자 무시).
pub fn autostart_points_to(command: Option<&str>, destination: &Path) -> bool {
    match (command, startup_command(destination)) {
        (Some(c), Ok(expected)) => c.eq_ignore_ascii_case(&expected),
        _ => false,
    }
}

#[cfg(not(windows))]
fn autostart(_: &Path, _: bool) -> Result<(), String> {
    Err("Windows에서만 자동 실행을 설정할 수 있습니다".into())
}

/// 앱 설정 화면이 읽는 상태 한 줄(`--status`, 설치 명령 결과 공통).
/// `status`는 실행 중인 몽땅검색이 쓰는 status.json(없으면 Null).
pub fn report(
    running_pid: Option<u32>,
    destination: &Path,
    autostart: Option<&str>,
    status: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "name": "몽땅검색",
        "version": env!("CARGO_PKG_VERSION"),
        "running": running_pid.is_some(),
        "pid": running_pid,
        "installed": destination.is_file(),
        "installed_path": destination.to_string_lossy(),
        "autostart": autostart_points_to(autostart, destination),
        "disabled": disabled_marker(destination).is_file(),
        "autostart_command": autostart,
        "hotkey_error": status.get("hotkey_error").and_then(|v| v.as_str()).filter(|s| !s.is_empty()),
        "status": status,
    })
}

/// 사용자가 몽땅검색을 끈 표시 파일(설치본 옆). 있으면 자동 갱신이 다시 켜지 않는다.
pub fn disabled_marker(destination: &Path) -> PathBuf {
    destination.with_file_name("ktm.disabled")
}

pub fn execute(action: &str, source: &Path, destination: &Path) -> Result<(), String> {
    match action.trim_start_matches('-') {
        "install" => install_file(source, destination),
        "disable-mark" => {
            let marker = disabled_marker(destination);
            if let Some(parent) = marker.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("끔 표시 실패: {e}"))?;
            }
            std::fs::write(&marker, b"off").map_err(|e| format!("끔 표시 실패: {e}"))
        }
        "enable-mark" => match std::fs::remove_file(disabled_marker(destination)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("끔 표시 지우기 실패: {e}")),
        },
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
        assert_eq!(
            plan(&args(&["ktm", "--install"]), false).unwrap(),
            ["enable-mark", "quit", "install", "autostart-on", "start"]
        );
        assert_eq!(
            plan(&args(&["ktm", "--install", "--no-autostart"]), false).unwrap(),
            ["enable-mark", "quit", "install", "start"]
        );
        assert_eq!(
            plan(&args(&["ktm", "--uninstall"]), false).unwrap(),
            ["quit", "autostart-off", "uninstall", "enable-mark"]
        );
        assert_eq!(plan(&args(&["ktm", "--autostart-off"]), false).unwrap(), ["autostart-off"]);
        assert!(plan(&args(&["ktm", "--show"]), false).is_none());
        assert!(plan(&args(&["ktm", "--hidden"]), false).is_none());
    }
    #[test]
    fn disable_and_quit_keep_the_installed_copy() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(plan(&args(&["ktm", "--disable"]), false).unwrap(), ["disable-mark", "quit", "autostart-off"]);
        assert_eq!(plan(&args(&["ktm", "--quit"]), false).unwrap(), ["quit"]);
        // 교체·제거 계획은 파일을 건드리기 전에 언제나 먼저 끈다(실행 중인 exe는 지울 수 없음).
        for flag in ["--install", "--uninstall", "--disable"] {
            let steps = plan(&args(&["ktm", flag]), false).unwrap();
            let quit = steps.iter().position(|s| *s == "quit").unwrap();
            assert!(steps.iter().position(|s| *s == "install" || *s == "uninstall").is_none_or(|i| i > quit));
        }
    }
    #[test]
    fn respect_disabled_only_updates_the_copy() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let refresh = args(&["ktm", "--install", "--respect-disabled"]);
        assert_eq!(plan(&refresh, true).unwrap(), ["quit", "install"]);
        assert_eq!(plan(&refresh, false).unwrap(), ["enable-mark", "quit", "install", "autostart-on", "start"]);
        // 사람이 설정에서 다시 켜면(--respect-disabled 없음) 끔 표시를 지우고 켠다.
        assert_eq!(plan(&args(&["ktm", "--install"]), true).unwrap()[0], "enable-mark");
    }
    #[test]
    fn disable_marker_round_trip() {
        let t = Temp::new();
        let exe = t.destination();
        execute("disable-mark", &t.source(), &exe).unwrap();
        assert!(disabled_marker(&exe).is_file());
        assert_eq!(report(None, &exe, None, serde_json::Value::Null)["disabled"], true);
        execute("enable-mark", &t.source(), &exe).unwrap();
        execute("enable-mark", &t.source(), &exe).unwrap();
        assert!(!disabled_marker(&exe).exists());
    }
    #[test]
    fn autostart_match_is_exact_command() {
        let exe = Path::new("C:/Users/a/AppData/Local/HwpAutoDocFit/ktm/bin/ktm.exe");
        let command = startup_command(exe).unwrap();
        assert!(autostart_points_to(Some(&command), exe));
        assert!(autostart_points_to(Some(&command.to_uppercase()), exe));
        assert!(!autostart_points_to(Some("\"C:/other/ktm.exe\" --hidden"), exe));
        assert!(!autostart_points_to(None, exe));
    }
    #[test]
    fn report_reflects_install_autostart_and_hotkey_error() {
        let t = Temp::new();
        let exe = t.destination();
        let r = report(None, &exe, None, serde_json::Value::Null);
        assert_eq!((r["running"].as_bool(), r["installed"].as_bool(), r["autostart"].as_bool()), (Some(false), Some(false), Some(false)));
        assert!(r["hotkey_error"].is_null());
        std::fs::write(&exe, b"x").unwrap();
        let command = startup_command(&exe).unwrap();
        let status = serde_json::json!({"pid": 7, "hotkey_error": "몽땅검색 단축키를 등록하지 못했습니다"});
        let r = report(Some(7), &exe, Some(&command), status);
        assert_eq!((r["running"].as_bool(), r["installed"].as_bool(), r["autostart"].as_bool()), (Some(true), Some(true), Some(true)));
        assert_eq!(r["pid"], 7);
        assert_eq!(r["hotkey_error"], "몽땅검색 단축키를 등록하지 못했습니다");
        assert_eq!(r["name"], "몽땅검색");
    }
    #[test]
    fn quit_and_start_are_not_file_actions() {
        // Win32가 필요한 단계는 실행 파일 쪽에서만 처리하고 여기서는 거부한다(실수로 무시되지 않게).
        let t = Temp::new();
        assert!(execute("quit", &t.source(), &t.destination()).is_err());
        assert!(execute("start", &t.source(), &t.destination()).is_err());
    }
    #[test]
    fn unknown_action_is_rejected() {
        let t = Temp::new();
        assert!(execute("unknown", &t.source(), &t.destination()).is_err());
    }
}
