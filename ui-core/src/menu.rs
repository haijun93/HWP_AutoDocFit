//! 메뉴 모델(순수) → Win32 팝업 메뉴. kagent `settings_menu.rs`의 `Entry`와 popup `build_menu`를 옮겼다.

use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CheckMenuRadioItem, CreatePopupMenu, DestroyMenu, HMENU, MF_BYCOMMAND, MF_CHECKED, MF_GRAYED,
    MF_POPUP, MF_SEPARATOR, MF_STRING, TPM_RETURNCMD, TrackPopupMenu,
};

/// 메뉴 한 줄.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    Separator,
    Item { id: usize, label: String, checked: bool, radio: bool, enabled: bool },
    Sub { label: String, entries: Vec<Entry> },
}

impl Entry {
    pub fn item(id: usize, label: impl Into<String>) -> Entry {
        Entry::Item { id, label: label.into(), checked: false, radio: false, enabled: true }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 메뉴 모델을 Win32 메뉴로 만든다. 하위 메뉴는 부모 메뉴가 없앨 때 함께 없어진다.
///
/// # Safety
/// 돌려받은 메뉴는 `DestroyMenu`로 없애야 한다.
pub unsafe fn build(entries: &[Entry]) -> HMENU {
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return menu;
        }
        for e in entries {
            match e {
                Entry::Separator => { AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null()); }
                Entry::Item { id, label, checked, radio, enabled } => {
                    let flags = MF_STRING | if *checked && !*radio { MF_CHECKED } else { 0 }
                        | if *enabled { 0 } else { MF_GRAYED };
                    AppendMenuW(menu, flags, *id, wide(label).as_ptr());
                    if *checked && *radio {
                        CheckMenuRadioItem(menu, *id as u32, *id as u32, *id as u32, MF_BYCOMMAND);
                    }
                }
                Entry::Sub { label, entries } => {
                    let sub = build(entries);
                    if !sub.is_null() {
                        AppendMenuW(menu, MF_STRING | MF_POPUP, sub as usize, wide(label).as_ptr());
                    }
                }
            }
        }
        menu
    }
}

/// 메뉴를 화면 좌표 pt에 띄우고 고른 명령(없으면 0)을 돌려준다. 메뉴는 없앤다.
///
/// # Safety
/// `owner`는 이 스레드의 창이어야 한다.
pub unsafe fn track(owner: HWND, entries: &[Entry], pt: POINT, flags: u32) -> usize {
    unsafe {
        let m = build(entries);
        if m.is_null() {
            return 0;
        }
        let c = TrackPopupMenu(m, TPM_RETURNCMD | flags, pt.x, pt.y, 0, owner, std::ptr::null());
        DestroyMenu(m);
        c.max(0) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetMenuItemCount, GetMenuState, MF_BYPOSITION};

    #[test]
    fn build_reflects_model() {
        let entries = vec![
            Entry::item(1, "열기"),
            Entry::Separator,
            Entry::Item { id: 2, label: "켜기".into(), checked: true, radio: false, enabled: true },
            Entry::Item { id: 3, label: "막힘".into(), checked: false, radio: false, enabled: false },
            Entry::Sub { label: "하위".into(), entries: vec![Entry::item(4, "가"), Entry::Item { id: 5, label: "나".into(), checked: true, radio: true, enabled: true }] },
        ];
        unsafe {
            let m = build(&entries);
            assert!(!m.is_null());
            assert_eq!(GetMenuItemCount(m), 5);
            assert_ne!(GetMenuState(m, 2, MF_BYPOSITION) & MF_CHECKED, 0);
            assert_ne!(GetMenuState(m, 3, MF_BYPOSITION) & MF_GRAYED, 0);
            assert_ne!(GetMenuState(m, 4, MF_BYPOSITION) & MF_POPUP, 0);
            DestroyMenu(m);
        }
    }
}
