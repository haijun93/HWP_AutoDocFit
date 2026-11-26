//! 문서뷰·사진뷰 오른쪽 메뉴의 '다른 창으로 보내기' 줄(파일 작업 메뉴 아래에 붙는다). 검사와 전달은 `sidebar.exe --send` 가 한다(보내는 곳이 하나).
use crate::handoff;
use crate::json::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub menu: u8,
    pub label: String,
    pub enabled: bool,
}

/// 메뉴 줄. 보낼 수 없으면 흐리게 두고 이유를 함께 적는다. `hidden` 은 숨긴 메뉴 번호(파일 변환·PDF 편집에만 적용).
pub fn entries<S: AsRef<str>>(menus: &[u8], paths: &[S], hidden: &[u8], sender_found: bool) -> Vec<Entry> {
    menus
        .iter()
        .filter_map(|&menu| handoff::target(menu))
        .map(|target| {
            let reason = if !sender_found {
                Some("sidebar.exe 없음".to_string())
            } else if matches!(target.menu, 10 | 11) && hidden.contains(&target.menu) {
                Some("숨긴 메뉴".to_string())
            } else {
                handoff::rejection(target.menu, paths)
            };
            let verb = if matches!(target.menu, 12 | 13) { target.label.to_string() } else { format!("{}(으)로 보내기", target.label) };
            Entry {
                menu: target.menu,
                enabled: reason.is_none(),
                label: match reason {
                    Some(reason) => format!("{verb} — {reason}"),
                    None => verb,
                },
            }
        })
        .collect()
}

/// 설정의 `hidden_tabs`. 읽지 못하면 숨긴 것 없음.
pub fn hidden_tabs(settings_json: &str) -> Vec<u8> {
    crate::json::parse(settings_json)
        .ok()
        .and_then(|root| {
            root.get("hidden_tabs").and_then(Value::as_array).map(|list| {
                list.iter().filter_map(|v| match v { Value::Num(n) => Some(*n as u8), _ => None }).collect()
            })
        })
        .unwrap_or_default()
}

/// 사이드바가 띄웠으면 환경변수, 아니면 창 프로그램 옆의 sidebar.exe(설치본 배치).
pub fn sender() -> Option<PathBuf> {
    std::env::var_os("DOCFIT_SIDEBAR_EXE")
        .map(PathBuf::from)
        .into_iter()
        .chain(std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("sidebar.exe"))))
        .find(|p| p.is_file())
}

/// 지금 선택에 대한 보내기 줄(`sidebar.exe` 유무·숨긴 메뉴를 읽는다). 메뉴는 `native_ops::menu` 가 띄운다.
pub fn lines(menus: &[u8], paths: &[String], settings: &Path) -> Vec<Entry> {
    let hidden = hidden_tabs(&std::fs::read_to_string(settings).unwrap_or_default());
    entries(menus, paths, &hidden, sender().is_some())
}

/// 명령줄에 직접 싣는 경로 길이 상한(Windows 한도 32767 아래 여유). 넘으면 `--list` 파일로 넘긴다.
const MAX_ARGUMENT_UNITS: usize = 30_000;

fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' | '\\' => out.extend(['\\', c]),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `sidebar.exe` 인자와, 만들었다면 지워야 할 목록 파일.
fn arguments(menu: u8, paths: &[String], settings: &Path, list_folder: &Path) -> std::io::Result<(Vec<OsString>, Option<PathBuf>)> {
    let mut args: Vec<OsString> = vec!["--send".into(), menu.to_string().into()];
    // 경로마다 `--path` 와 따옴표·공백만큼 더 든다.
    let length: usize = paths.iter().map(|p| p.encode_utf16().count() + 10).sum();
    let list = if length <= MAX_ARGUMENT_UNITS {
        for path in paths {
            args.extend(["--path".into(), path.into()]);
        }
        None
    } else {
        use std::io::Write;
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let file = list_folder.join(format!("docfit-send-{}-{stamp}.json", std::process::id()));
        let body: Vec<String> = paths.iter().map(|p| json_string(p)).collect();
        let mut stream = std::fs::OpenOptions::new().write(true).create_new(true).open(&file)?;
        stream.write_all(format!("{{\"paths\":[{}]}}", body.join(",")).as_bytes())?;
        args.extend(["--list".into(), file.clone().into()]);
        Some(file)
    };
    args.extend(["--settings".into(), settings.into(), "--notify".into()]);
    Ok((args, list))
}

/// 고른 곳으로 보낸다. 받는 창을 기다리지 않는다. 실패하면 안내 문장.
pub fn send(menu: u8, paths: &[String], settings: &Path) -> Result<(), String> {
    let sender = sender().ok_or("sidebar.exe 가 없어 보내지 못했습니다")?;
    let failed = |e: std::io::Error| format!("보내지 못했습니다: {e}");
    let (args, list) = arguments(menu, paths, settings, &std::env::temp_dir()).map_err(failed)?;
    let mut child = match std::process::Command::new(sender).args(args).spawn() {
        Ok(child) => child,
        Err(e) => {
            if let Some(file) = list {
                let _ = std::fs::remove_file(file);
            }
            return Err(failed(e));
        }
    };
    // 받는 창이 앞으로 나올 수 있게 전면 권한을 넘긴다.
    unsafe { AllowSetForegroundWindow(child.id()) };
    if let Some(file) = list {
        // 보내기 프로세스가 목록을 읽고 끝난 뒤 지운다.
        std::thread::spawn(move || {
            let _ = child.wait();
            let _ = std::fs::remove_file(file);
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_tabs_are_read_from_settings() {
        assert_eq!(hidden_tabs(r#"{"hidden_tabs":[10,11]}"#), [10, 11]);
        assert!(hidden_tabs("{").is_empty());
    }

    #[test]
    fn unsupported_targets_stay_visible_but_disabled_with_a_reason() {
        let list = entries(&handoff::DOCVIEW_MENUS, &[r"C:\a\표.xlsx"], &[], true);
        let enabled: Vec<u8> = list.iter().filter(|e| e.enabled).map(|e| e.menu).collect();
        assert_eq!(enabled, [12, 13]);
        assert_eq!(list.len(), 5);
        let pdf = entries(&handoff::DOCVIEW_MENUS, &[r"C:\a\b.pdf"], &[11], true);
        assert!(pdf.iter().any(|e| e.menu == 11 && !e.enabled && e.label.contains("숨긴 메뉴")));
        assert!(entries(&handoff::PHOTOVIEW_MENUS, &[r"C:\a\b.jpg"], &[], true).iter().all(|e| e.enabled));
        assert!(entries(&handoff::PHOTOVIEW_MENUS, &[r"C:\a\b.jpg"], &[], false).iter().all(|e| !e.enabled));
    }

    #[test]
    fn every_selected_file_is_checked() {
        let mixed = [r"C:\a\b.pdf", r"C:\a\c.hwp"];
        let list = entries(&handoff::DOCVIEW_MENUS, &mixed, &[], true);
        let enabled: Vec<u8> = list.iter().filter(|e| e.enabled).map(|e| e.menu).collect();
        assert_eq!(enabled, [0, 10, 12, 13], "PDF 가 아닌 파일이 섞이면 PDF 편집은 막는다");
        let many = vec![r"C:\a\b.pdf"; 201];
        let list = entries(&handoff::DOCVIEW_MENUS, &many, &[], true);
        assert!(list.iter().any(|e| e.menu == 12 && !e.enabled && e.label.contains("200개")));
    }

    #[test]
    fn short_selection_goes_on_the_command_line_and_long_one_into_a_list_file() {
        let folder = std::env::temp_dir().join(format!("docfit-send-menu-test-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let two = vec![r"C:\가 나\a.pdf".to_string(), r"C:\b.pdf".to_string()];
        let (args, list) = arguments(11, &two, Path::new("s.json"), &folder).unwrap();
        assert!(list.is_none());
        assert_eq!(args.iter().filter(|a| *a == "--path").count(), 2);
        assert_eq!(args[3], OsString::from(&two[0]));

        let many: Vec<String> = (0..1000).map(|i| format!(r"C:\{}\{i}.pdf", "가".repeat(40))).collect();
        let (args, list) = arguments(0, &many, Path::new("s.json"), &folder).unwrap();
        let list = list.unwrap();
        assert!(!args.iter().any(|a| a == "--path"));
        assert!(args.windows(2).any(|w| w[0] == "--list" && w[1] == *list.as_os_str()));
        let root = crate::json::parse(&std::fs::read_to_string(&list).unwrap()).unwrap();
        let read: Vec<&str> = root.get("paths").and_then(Value::as_array).unwrap().iter()
            .filter_map(|v| match v { Value::Str(s) => Some(s.as_str()), _ => None }).collect();
        assert_eq!(read, many);
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
