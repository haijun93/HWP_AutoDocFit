use windows_sys::Win32::{
    Foundation::HWND,
    UI::{
        Input::KeyboardAndMouse::{
            MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey,
            VK_F24, VK_SPACE,
        },
        Shell::{
            NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
        },
        WindowsAndMessaging::*,
    },
};

pub const MESSAGE: u32 = WM_APP + 31;
pub const HOTKEY_ID: i32 = 0x4d47;
pub const SHOW: u32 = 1;
pub const QUIT: u32 = 2;

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

pub fn parse_hotkey(value: &str) -> Result<(u32, u32), String> {
    match value {
        "Ctrl+Alt+Space" => Ok((MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, VK_SPACE as u32)),
        "Ctrl+Alt+Shift+F24" => Ok((
            MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT,
            VK_F24 as u32,
        )),
        _ => Err("지원하지 않는 단축키입니다".into()),
    }
}

pub struct Tray {
    hwnd: HWND,
    registered: bool,
    pub error: Option<String>,
}

impl Tray {
    pub fn new(hwnd: HWND, hotkey: &str) -> Result<Self, String> {
        let mut registered = false;
        let mut error = None;
        unsafe {
            match parse_hotkey(hotkey) {
                Ok((modifiers, key)) => {
                    registered = RegisterHotKey(hwnd, HOTKEY_ID, modifiers, key) != 0;
                    if !registered {
                        error = Some(
                            "몽땅검색 단축키를 등록하지 못했습니다 (다른 앱과 중복 확인)".into(),
                        );
                    }
                }
                Err(message) => error = Some(message),
            }
            let mut data: NOTIFYICONDATAW = std::mem::zeroed();
            data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            data.hWnd = hwnd;
            data.uID = 1;
            data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
            data.uCallbackMessage = MESSAGE;
            data.hIcon = LoadIconW(std::ptr::null_mut(), IDI_APPLICATION);
            let tip = wide("몽땅검색");
            data.szTip[..tip.len()].copy_from_slice(&tip);
            if Shell_NotifyIconW(NIM_ADD, &data) == 0 {
                if registered {
                    UnregisterHotKey(hwnd, HOTKEY_ID);
                }
                return Err("몽땅검색 트레이를 만들지 못했습니다".into());
            }
        }
        Ok(Self {
            hwnd,
            registered,
            error,
        })
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            let mut data: NOTIFYICONDATAW = std::mem::zeroed();
            data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            data.hWnd = self.hwnd;
            data.uID = 1;
            Shell_NotifyIconW(NIM_DELETE, &data);
            if self.registered {
                UnregisterHotKey(self.hwnd, HOTKEY_ID);
            }
        }
    }
}

/// Returns SHOW, QUIT, or zero when dismissed.
pub fn menu(hwnd: HWND) -> u32 {
    unsafe {
        let popup = CreatePopupMenu();
        if popup.is_null() {
            return 0;
        }
        AppendMenuW(
            popup,
            MF_STRING,
            SHOW as usize,
            wide("몽땅검색 열기").as_ptr(),
        );
        AppendMenuW(popup, MF_STRING, QUIT as usize, wide("종료").as_ptr());
        let mut point = std::mem::zeroed();
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
        let selected = TrackPopupMenu(
            popup,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            hwnd,
            std::ptr::null(),
        );
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(popup);
        selected as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_known_hotkeys_are_accepted() {
        assert!(parse_hotkey("Ctrl+Alt+Space").is_ok());
        assert!(parse_hotkey("Ctrl+Alt+Shift+F24").is_ok());
        assert!(parse_hotkey("Space").is_err());
    }
    #[test]
    fn isolated_hotkey_registration_is_released() {
        // Thread hotkey avoids creating a window or touching the production chord.
        unsafe {
            let (modifiers, key) = parse_hotkey("Ctrl+Alt+Shift+F24").unwrap();
            let registered = RegisterHotKey(std::ptr::null_mut(), HOTKEY_ID + 1, modifiers, key);
            if registered != 0 {
                assert_ne!(UnregisterHotKey(std::ptr::null_mut(), HOTKEY_ID + 1), 0);
            }
            assert_ne!(registered, 0, "isolated test chord is already registered");
        }
    }
}
