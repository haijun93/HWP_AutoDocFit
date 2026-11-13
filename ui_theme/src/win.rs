//! Windows 부분: 시스템 테마 읽기, 창 제목줄·메뉴 어둡게, 테마 변경 알림.

use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{
    COLOR_BTNFACE, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_HOTLIGHT, COLOR_WINDOW, COLOR_WINDOWTEXT,
    GetSysColor,
};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, PostMessageW, RegisterWindowMessageW, SPI_GETHIGHCONTRAST, SystemParametersInfoW, WM_SETTINGCHANGE,
};

use crate::{Change, Kind, Palette, SysColors, Theme, ThemePref, is_theme_event, store, theme};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 앱 모드가 어둡게인지(HKCU\...\Themes\Personalize AppsUseLightTheme DWORD 0). 값이 없으면 밝게.
pub fn system_dark() -> bool {
    let key = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let name = wide("AppsUseLightTheme");
    let mut value: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    let rc = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(),
                     &mut value as *mut u32 as _, &mut size)
    };
    rc == 0 && value == 0
}

/// 고대비가 켜져 있는지.
pub fn high_contrast_on() -> bool {
    let mut hc = HIGHCONTRASTW { cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32, dwFlags: 0,
                                 lpszDefaultScheme: std::ptr::null_mut() };
    let ok = unsafe { SystemParametersInfoW(SPI_GETHIGHCONTRAST, hc.cbSize, &mut hc as *mut _ as _, 0) };
    ok != 0 && hc.dwFlags & HCF_HIGHCONTRASTON != 0
}

/// 지금 시스템 색.
pub fn sys_colors() -> SysColors {
    unsafe {
        SysColors {
            window: GetSysColor(COLOR_WINDOW),
            window_text: GetSysColor(COLOR_WINDOWTEXT),
            highlight: GetSysColor(COLOR_HIGHLIGHT),
            highlight_text: GetSysColor(COLOR_HIGHLIGHTTEXT),
            btn_face: GetSysColor(COLOR_BTNFACE),
            gray_text: GetSysColor(COLOR_GRAYTEXT),
            hotlight: GetSysColor(COLOR_HOTLIGHT),
        }
    }
}

/// 선호에 시스템 상태를 더한 테마(상태는 바꾸지 않는다).
pub fn current(pref: ThemePref) -> (Kind, Palette) {
    let t = Theme::derive(pref, system_dark(), high_contrast_on(), &sys_colors());
    (t.kind, t.palette)
}

/// 테마 상태 전이(창 스레드에서 부른다): 시스템 상태를 읽어 `store`하고 메뉴 모드를 맞춘다.
pub fn refresh(pref: ThemePref) -> Change {
    let next = Theme::derive(pref, system_dark(), high_contrast_on(), &sys_colors());
    let change = store(next);
    set_menu_mode(next.kind);
    change
}

/// 지금 선호 그대로 시스템만 다시 읽는다(테마 메시지를 받았을 때).
pub fn refresh_system() -> Change {
    refresh(theme().pref)
}

const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
const DWMWA_USE_IMMERSIVE_DARK_MODE_OLD: u32 = 19;

/// 창 제목줄·틀을 테마에 맞춘다(어둡게만 켬, 고대비·밝게는 끔). Windows 10 1809~1909는 속성 19.
pub fn apply_window(hwnd: HWND, kind: Kind) {
    if hwnd.is_null() {
        return;
    }
    let dark: BOOL = (kind == Kind::Dark) as BOOL;
    unsafe {
        let size = std::mem::size_of::<BOOL>() as u32;
        if DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const BOOL as _, size) < 0 {
            DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE_OLD, &dark as *const BOOL as _, size);
        }
        if let Some(allow) = uxtheme().allow_window {
            allow(hwnd, dark != 0);
        }
    }
}

fn changed_message() -> u32 {
    static ID: OnceLock<u32> = OnceLock::new();
    *ID.get_or_init(|| unsafe { RegisterWindowMessageW(wide(crate::CHANGED_MESSAGE_NAME).as_ptr()) })
}

/// 테마를 다시 읽어야 하는 메시지인지(WM_SETTINGCHANGE ImmersiveColorSet·고대비, WM_SYSCOLORCHANGE,
/// WM_THEMECHANGED, DocFitThemeChanged).
pub fn is_theme_message(msg: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
    let area = if msg == WM_SETTINGCHANGE && lparam != 0 { read_wide(lparam as *const u16, 64) } else { None };
    is_theme_event(msg, wparam, area.as_deref(), changed_message())
}

fn read_wide(p: *const u16, max: usize) -> Option<String> {
    let mut v = Vec::new();
    for i in 0..max {
        let c = unsafe { *p.add(i) };
        if c == 0 {
            return Some(String::from_utf16_lossy(&v));
        }
        v.push(c);
    }
    None
}

/// 테마 설정이 바뀌었다고 모든 최상위 창에 알린다(kvf 등은 받으면 설정을 다시 읽는다).
pub fn broadcast_changed() {
    let id = changed_message();
    if id != 0 {
        unsafe { PostMessageW(HWND_BROADCAST, id, 0, 0) };
    }
}

// ---- 메뉴 어둡게(문서화되지 않은 uxtheme 서수; 없으면 조용히 건너뜀) ----

type SetPreferredAppMode = unsafe extern "system" fn(i32) -> i32;
type FlushMenuThemes = unsafe extern "system" fn();
type AllowDarkModeForWindow = unsafe extern "system" fn(HWND, bool) -> bool;

#[derive(Clone, Copy)]
struct UxTheme {
    set_mode: Option<SetPreferredAppMode>,
    flush: Option<FlushMenuThemes>,
    allow_window: Option<AllowDarkModeForWindow>,
}

fn uxtheme() -> UxTheme {
    static UX: OnceLock<UxTheme> = OnceLock::new();
    *UX.get_or_init(|| unsafe {
        let module = LoadLibraryW(wide("uxtheme.dll").as_ptr());
        if module.is_null() {
            return UxTheme { set_mode: None, flush: None, allow_window: None };
        }
        let get = |ordinal: usize| GetProcAddress(module, ordinal as *const u8);
        UxTheme {
            set_mode: get(135).map(|f| std::mem::transmute::<_, SetPreferredAppMode>(f)),
            flush: get(136).map(|f| std::mem::transmute::<_, FlushMenuThemes>(f)),
            allow_window: get(133).map(|f| std::mem::transmute::<_, AllowDarkModeForWindow>(f)),
        }
    })
}

/// 서수 135 SetPreferredAppMode 값: Default 0, ForceDark 2, ForceLight 3.
pub fn menu_mode(kind: Kind) -> i32 {
    match kind {
        Kind::Dark => 2,
        Kind::Light => 3,
        Kind::HighContrast => 0,
    }
}

fn set_menu_mode(kind: Kind) {
    let ux = uxtheme();
    unsafe {
        if let Some(set) = ux.set_mode {
            set(menu_mode(kind));
        }
        if let Some(flush) = ux.flush {
            flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{WM_SYSCOLORCHANGE, WM_THEMECHANGED};

    #[test]
    fn message_constants_match_windows() {
        assert_eq!(crate::MSG_SETTINGCHANGE, WM_SETTINGCHANGE);
        assert_eq!(crate::MSG_SYSCOLORCHANGE, WM_SYSCOLORCHANGE);
        assert_eq!(crate::MSG_THEMECHANGED, WM_THEMECHANGED);
        assert_eq!(crate::SPI_SETHIGHCONTRAST_CODE,
                   windows_sys::Win32::UI::WindowsAndMessaging::SPI_SETHIGHCONTRAST as usize);
    }

    #[test]
    fn registered_message_and_setting_change_string() {
        let id = changed_message();
        assert!(id >= 0xC000, "RegisterWindowMessage 범위");
        assert!(is_theme_message(id, 0, 0));
        let area = wide("ImmersiveColorSet");
        assert!(is_theme_message(WM_SETTINGCHANGE, 0, area.as_ptr() as LPARAM));
        let other = wide("Environment");
        assert!(!is_theme_message(WM_SETTINGCHANGE, 0, other.as_ptr() as LPARAM));
        assert!(!is_theme_message(WM_SETTINGCHANGE, 0, 0));
    }

    #[test]
    fn system_reads_do_not_fail() {
        // 값은 이 PC 설정에 따라 다르다. 읽기가 막히지 않고 결과가 해석 규칙과 맞는지만 본다.
        let (kind, palette) = current(ThemePref::Light);
        if high_contrast_on() {
            assert_eq!(kind, Kind::HighContrast);
        } else {
            assert_eq!((kind, palette), (Kind::Light, Palette::LIGHT));
            assert_eq!(current(ThemePref::Dark), (Kind::Dark, Palette::DARK));
        }
        let _ = system_dark();
        assert_eq!(menu_mode(Kind::Dark), 2);
        assert_eq!(menu_mode(Kind::Light), 3);
        assert_eq!(menu_mode(Kind::HighContrast), 0);
    }
}
