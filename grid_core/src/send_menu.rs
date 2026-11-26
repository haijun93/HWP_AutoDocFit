//! 문서뷰·사진뷰 오른쪽 메뉴 '다른 창으로 보내기'. 검사와 전달은 `sidebar.exe --send` 가 한다(보내는 곳이 하나).
use crate::handoff;
use crate::json::Value;
use std::path::{Path, PathBuf};
use std::ptr::null;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub menu: u8,
    pub label: String,
    pub enabled: bool,
}

/// 메뉴 줄. 보낼 수 없으면 흐리게 두고 이유를 함께 적는다. `hidden` 은 숨긴 메뉴 번호(파일 변환·PDF 편집에만 적용).
pub fn entries(menus: &[u8], path: &str, hidden: &[u8], sender_found: bool) -> Vec<Entry> {
    menus
        .iter()
        .filter_map(|&menu| handoff::target(menu))
        .map(|target| {
            let reason = if !sender_found {
                Some("sidebar.exe 없음".to_string())
            } else if matches!(target.menu, 10 | 11) && hidden.contains(&target.menu) {
                Some("숨긴 메뉴".to_string())
            } else {
                handoff::rejection(target.menu, &[path])
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

/// 화면 좌표 `(x, y)` 에 메뉴를 띄우고 고른 곳으로 보낸다. 메뉴가 메시지 루프를 돌므로 창 상태 참조를 쥐지 않고 부른다.
/// 실패하면 안내 문장.
///
/// # Safety
/// `hwnd` 는 이 스레드의 창이어야 한다.
pub unsafe fn show(hwnd: isize, x: i32, y: i32, menus: &[u8], path: &str, settings: &Path) -> Result<(), String> {
    let sender = sender();
    let hidden = hidden_tabs(&std::fs::read_to_string(settings).unwrap_or_default());
    let list = entries(menus, path, &hidden, sender.is_some());
    let chosen = unsafe {
        let popup = CreatePopupMenu();
        for entry in &list {
            let text: Vec<u16> = entry.label.encode_utf16().chain(Some(0)).collect();
            let flags = MF_STRING | if entry.enabled { MF_ENABLED } else { MF_GRAYED };
            AppendMenuW(popup, flags, entry.menu as usize + 1, text.as_ptr());
        }
        let command = TrackPopupMenu(popup, TPM_RETURNCMD | TPM_RIGHTBUTTON, x, y, 0, hwnd as _, null());
        DestroyMenu(popup);
        command
    };
    let (Some(sender), true) = (sender, chosen > 0) else { return Ok(()) };
    let child = std::process::Command::new(sender)
        .arg("--send")
        .arg((chosen - 1).to_string())
        .arg("--path")
        .arg(path)
        .arg("--settings")
        .arg(settings)
        .arg("--notify")
        .spawn()
        .map_err(|e| format!("보내지 못했습니다: {e}"))?;
    // 받는 창이 앞으로 나올 수 있게 전면 권한을 넘긴다.
    unsafe { AllowSetForegroundWindow(child.id()) };
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
        let list = entries(&handoff::DOCVIEW_MENUS, r"C:\a\표.xlsx", &[], true);
        let enabled: Vec<u8> = list.iter().filter(|e| e.enabled).map(|e| e.menu).collect();
        assert_eq!(enabled, [12, 13]);
        assert_eq!(list.len(), 5);
        let pdf = entries(&handoff::DOCVIEW_MENUS, r"C:\a\b.pdf", &[11], true);
        assert!(pdf.iter().any(|e| e.menu == 11 && !e.enabled && e.label.contains("숨긴 메뉴")));
        assert!(entries(&handoff::PHOTOVIEW_MENUS, r"C:\a\b.jpg", &[], true).iter().all(|e| e.enabled));
        assert!(entries(&handoff::PHOTOVIEW_MENUS, r"C:\a\b.jpg", &[], false).iter().all(|e| !e.enabled));
    }
}
