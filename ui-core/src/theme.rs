//! 테마(라이트·다크·고대비, 시스템 따라가기). DARK1(`flutter_ui/lib/core/theme/app_theme.dart`)의 색 토큰을
//! 값 그대로 옮겼다 — Flutter 화면과 Rust 화면이 같은 색을 쓴다.
//!
//! 상태 기계(순수 함수 [`resolve`]): 테마 = (사용자 선택 [`Choice`]) × (시스템 밝기) × (시스템 고대비).
//! - 고대비가 켜져 있으면 선택과 상관없이 고대비 색(시스템 색 그대로).
//! - 아니면 system → 시스템 밝기, light → 라이트, dark → 다크.
//!
//! 시스템 설정이 바뀌면 WM_SETTINGCHANGE("ImmersiveColorSet")·WM_SYSCOLORCHANGE·WM_THEMECHANGED가 온다.
//! [`Watcher::on_message`]가 그때만 시스템 값을 다시 읽어 바뀐 테마를 돌려준다.

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::Graphics::Gdi::{
    COLOR_3DFACE, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_HOTLIGHT, COLOR_WINDOW, COLOR_WINDOWTEXT,
    GetSysColor,
};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SPI_GETHIGHCONTRAST, SystemParametersInfoW, WM_SETTINGCHANGE, WM_SYSCOLORCHANGE, WM_THEMECHANGED,
};

/// 0xRRGGBB 색(화면 코드는 [`Rgb::bgr`]로 GDI COLORREF를 얻는다).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb(pub u32);

impl Rgb {
    /// GDI COLORREF(0x00BBGGRR).
    pub const fn bgr(self) -> u32 {
        let v = self.0;
        ((v & 0xFF) << 16) | (v & 0xFF00) | ((v >> 16) & 0xFF)
    }
    pub const fn from_bgr(c: u32) -> Self {
        Rgb(((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF))
    }
    fn channel(self, shift: u32) -> f64 {
        let c = ((self.0 >> shift) & 0xFF) as f64 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    }
    /// WCAG 상대 휘도.
    pub fn luminance(self) -> f64 {
        0.2126 * self.channel(16) + 0.7152 * self.channel(8) + 0.0722 * self.channel(0)
    }
}

/// WCAG 대비(1~21).
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (x, y) = (a.luminance(), b.luminance());
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

/// 사용자 테마 선택(설정 키 `theme_mode`, Flutter `AppThemeChoice`와 같은 값).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Choice {
    #[default]
    System,
    Light,
    Dark,
}

impl Choice {
    pub fn key(self) -> &'static str {
        match self {
            Choice::System => "system",
            Choice::Light => "light",
            Choice::Dark => "dark",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Choice::System => "시스템 설정 따르기",
            Choice::Light => "밝게",
            Choice::Dark => "어둡게",
        }
    }
    /// 모르는 값은 system(Flutter `AppThemeChoice.parse`와 같음).
    pub fn parse(v: &str) -> Self {
        match v {
            "light" => Choice::Light,
            "dark" => Choice::Dark,
            _ => Choice::System,
        }
    }
}

/// 실제로 그릴 테마.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
    HighContrast,
}

/// 시스템 상태(밝기·고대비).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct System {
    pub dark: bool,
    pub high_contrast: bool,
}

/// 테마 결정(순수 함수).
pub fn resolve(choice: Choice, sys: System) -> Mode {
    if sys.high_contrast {
        return Mode::HighContrast;
    }
    match choice {
        Choice::Light => Mode::Light,
        Choice::Dark => Mode::Dark,
        Choice::System if sys.dark => Mode::Dark,
        Choice::System => Mode::Light,
    }
}

/// 시스템이 앱 다크 모드인지(HKCU …\Themes\Personalize\AppsUseLightTheme = 0). 값이 없으면 라이트.
pub fn system_dark() -> bool {
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0".encode_utf16().collect();
    let name: Vec<u16> = "AppsUseLightTheme\0".encode_utf16().collect();
    let mut value: u32 = 1;
    let mut size = 4u32;
    let rc = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(),
                     &mut value as *mut u32 as _, &mut size)
    };
    rc == ERROR_SUCCESS && value == 0
}

/// 시스템 고대비가 켜져 있는지.
pub fn system_high_contrast() -> bool {
    unsafe {
        let mut hc: HIGHCONTRASTW = std::mem::zeroed();
        hc.cbSize = std::mem::size_of::<HIGHCONTRASTW>() as u32;
        SystemParametersInfoW(SPI_GETHIGHCONTRAST, hc.cbSize, &mut hc as *mut _ as _, 0) != 0
            && hc.dwFlags & HCF_HIGHCONTRASTON != 0
    }
}

pub fn system() -> System {
    System { dark: system_dark(), high_contrast: system_high_contrast() }
}

/// 테마에 영향을 주는 시스템 알림인지(순수 함수: 메시지와 WM_SETTINGCHANGE의 영역 이름).
pub fn is_theme_message(msg: u32, area: Option<&str>) -> bool {
    match msg {
        WM_SYSCOLORCHANGE | WM_THEMECHANGED => true,
        WM_SETTINGCHANGE => matches!(area, Some("ImmersiveColorSet") | Some("WindowsThemeElement")) || area.is_none(),
        _ => false,
    }
}

/// WM_SETTINGCHANGE LPARAM의 영역 이름(없으면 None).
///
/// # Safety
/// `lp`는 0이거나 NUL로 끝나는 UTF-16 문자열 포인터여야 한다.
pub unsafe fn setting_area(lp: isize) -> Option<String> {
    if lp == 0 {
        return None;
    }
    unsafe {
        let p = lp as *const u16;
        let mut n = 0;
        while *p.add(n) != 0 && n < 256 {
            n += 1;
        }
        Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)))
    }
}

/// 테마 감시(창 하나에 하나): 선택이나 시스템이 바뀌면 새 Mode를 돌려준다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Watcher {
    pub choice: Choice,
    pub system: System,
    pub mode: Mode,
}

impl Watcher {
    pub fn new(choice: Choice, system: System) -> Self {
        Self { choice, system, mode: resolve(choice, system) }
    }
    /// 선택 변경(설정 화면). 바뀌면 Some(새 Mode).
    pub fn set_choice(&mut self, choice: Choice) -> Option<Mode> {
        self.choice = choice;
        self.update(self.system)
    }
    /// 시스템 값 변경(순수). 바뀌면 Some(새 Mode).
    pub fn update(&mut self, system: System) -> Option<Mode> {
        self.system = system;
        let next = resolve(self.choice, system);
        (next != self.mode).then(|| {
            self.mode = next;
            next
        })
    }
    /// 창 프로시저에서 부른다: 테마 알림이면 시스템을 다시 읽는다.
    ///
    /// # Safety
    /// WM_SETTINGCHANGE면 `lp`는 0이거나 유효한 문자열 포인터여야 한다(Windows가 보장).
    pub unsafe fn on_message(&mut self, msg: u32, lp: isize) -> Option<Mode> {
        let area = if msg == WM_SETTINGCHANGE { unsafe { setting_area(lp) } } else { None };
        if !is_theme_message(msg, area.as_deref()) {
            return None;
        }
        self.update(system())
    }
}

/// 고대비 테마에 쓰는 Windows 시스템 색(Flutter `SystemColors`와 같은 일곱 가지).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemColors {
    pub window: Rgb,
    pub window_text: Rgb,
    pub highlight: Rgb,
    pub highlight_text: Rgb,
    pub button_face: Rgb,
    pub gray_text: Rgb,
    pub hotlight: Rgb,
}

impl SystemColors {
    pub fn read() -> Self {
        let c = |i| Rgb::from_bgr(unsafe { GetSysColor(i) });
        Self {
            window: c(COLOR_WINDOW),
            window_text: c(COLOR_WINDOWTEXT),
            highlight: c(COLOR_HIGHLIGHT),
            highlight_text: c(COLOR_HIGHLIGHTTEXT),
            button_face: c(COLOR_3DFACE),
            gray_text: c(COLOR_GRAYTEXT),
            hotlight: c(COLOR_HOTLIGHT),
        }
    }
    /// 흔한 고대비 검정 테마(시험용).
    pub const BLACK: SystemColors = SystemColors {
        window: Rgb(0x000000),
        window_text: Rgb(0xFFFFFF),
        highlight: Rgb(0x1AEBFF),
        highlight_text: Rgb(0x000000),
        button_face: Rgb(0x000000),
        gray_text: Rgb(0x3FF23F),
        hotlight: Rgb(0xFFFF00),
    };
}

/// 작업 방식 카드 색(문서편집 작업 카드).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModeColors {
    pub surface: Rgb,
    pub selected_surface: Rgb,
    pub accent: Rgb,
}

/// 앱 전체 색 토큰(Flutter `AppColors`와 같은 이름·값).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppColors {
    pub mode: Mode,
    pub canvas: Rgb,
    pub surface: Rgb,
    pub surface_muted: Rgb,
    pub surface_subtle: Rgb,
    pub line: Rgb,
    pub line_strong: Rgb,
    pub ink: Rgb,
    pub text_strong: Rgb,
    pub text: Rgb,
    pub text_body: Rgb,
    pub text_secondary: Rgb,
    pub muted: Rgb,
    pub faint: Rgb,
    pub accent: Rgb,
    pub accent_deep: Rgb,
    pub accent_fill: Rgb,
    pub on_accent: Rgb,
    pub accent_soft: Rgb,
    pub mint: Rgb,
    pub mint_soft: Rgb,
    pub danger: Rgb,
    pub danger_deep: Rgb,
    pub danger_soft: Rgb,
    pub success: Rgb,
    pub success_soft: Rgb,
    pub warning: Rgb,
    pub warning_soft: Rgb,
    pub highlight: Rgb,
    pub on_highlight: Rgb,
    /// spacing·unify·all·file_format 순.
    pub modes: [ModeColors; 4],
}

const fn mc(surface: u32, selected: u32, accent: u32) -> ModeColors {
    ModeColors { surface: Rgb(surface), selected_surface: Rgb(selected), accent: Rgb(accent) }
}

impl AppColors {
    pub const LIGHT: AppColors = AppColors {
        mode: Mode::Light,
        canvas: Rgb(0xf6f8fb),
        surface: Rgb(0xffffff),
        surface_muted: Rgb(0xf8fafc),
        surface_subtle: Rgb(0xf1f5f9),
        line: Rgb(0xe2e8f0),
        line_strong: Rgb(0xcbd5e1),
        ink: Rgb(0x172b4d),
        text_strong: Rgb(0x0f172a),
        text: Rgb(0x1e293b),
        text_body: Rgb(0x334155),
        text_secondary: Rgb(0x475569),
        muted: Rgb(0x64748b),
        faint: Rgb(0x94a3b8),
        accent: Rgb(0x2563eb),
        accent_deep: Rgb(0x1d4ed8),
        accent_fill: Rgb(0x2563eb),
        on_accent: Rgb(0xffffff),
        accent_soft: Rgb(0xedf3ff),
        mint: Rgb(0x287c72),
        mint_soft: Rgb(0xf3f8f7),
        danger: Rgb(0xdc2626),
        danger_deep: Rgb(0xb91c1c),
        danger_soft: Rgb(0xfef2f2),
        success: Rgb(0x15803d),
        success_soft: Rgb(0xdff3ee),
        warning: Rgb(0xe0a526),
        warning_soft: Rgb(0xfff7eb),
        highlight: Rgb(0xffeb3b),
        on_highlight: Rgb(0x0f172a),
        modes: [
            mc(0xf0f5ff, 0xe3edff, 0x3b6fb6),
            mc(0xeff8f5, 0xdef1ea, 0x2f8273),
            mc(0xfff7eb, 0xffedcf, 0xa86a1f),
            mc(0xf4f1ff, 0xeae4ff, 0x7656a8),
        ],
    };

    pub const DARK: AppColors = AppColors {
        mode: Mode::Dark,
        canvas: Rgb(0x0f141a),
        surface: Rgb(0x171d26),
        surface_muted: Rgb(0x1b222c),
        surface_subtle: Rgb(0x212a35),
        line: Rgb(0x2e3846),
        line_strong: Rgb(0x435064),
        ink: Rgb(0xe8eef6),
        text_strong: Rgb(0xf1f5f9),
        text: Rgb(0xe2e8f0),
        text_body: Rgb(0xcbd5e1),
        text_secondary: Rgb(0xb4c0cf),
        muted: Rgb(0x9eabbb),
        faint: Rgb(0x8e9bad),
        accent: Rgb(0x6ea2ff),
        accent_deep: Rgb(0x93bbff),
        accent_fill: Rgb(0x2563eb),
        on_accent: Rgb(0xffffff),
        accent_soft: Rgb(0x1c2a44),
        mint: Rgb(0x5cc4b5),
        mint_soft: Rgb(0x16302c),
        danger: Rgb(0xff7b7b),
        danger_deep: Rgb(0xff9c9c),
        danger_soft: Rgb(0x3a1d20),
        success: Rgb(0x4ade80),
        success_soft: Rgb(0x15302a),
        warning: Rgb(0xf0b94a),
        warning_soft: Rgb(0x3a2e14),
        highlight: Rgb(0x8a7300),
        on_highlight: Rgb(0xffffff),
        modes: [
            mc(0x172235, 0x1d2d4a, 0x8fb4ee),
            mc(0x152a26, 0x1b3832, 0x6cc3b2),
            mc(0x2e2415, 0x3d2f17, 0xe6b36e),
            mc(0x241e36, 0x2f2747, 0xbca6ec),
        ],
    };

    /// Windows 고대비: 시스템 색 그대로(Flutter `AppColors.highContrast`와 같은 대응).
    pub fn high_contrast(s: &SystemColors) -> AppColors {
        let m = ModeColors { surface: s.window, selected_surface: s.highlight, accent: s.hotlight };
        let t = s.window_text;
        AppColors {
            mode: Mode::HighContrast,
            canvas: s.window,
            surface: s.window,
            surface_muted: s.window,
            surface_subtle: s.button_face,
            line: t,
            line_strong: t,
            ink: t,
            text_strong: t,
            text: t,
            text_body: t,
            text_secondary: t,
            muted: t,
            faint: s.gray_text,
            accent: s.hotlight,
            accent_deep: s.hotlight,
            accent_fill: s.highlight,
            on_accent: s.highlight_text,
            accent_soft: s.window,
            mint: t,
            mint_soft: s.window,
            danger: t,
            danger_deep: t,
            danger_soft: s.window,
            success: t,
            success_soft: s.window,
            warning: t,
            warning_soft: s.window,
            highlight: s.highlight,
            on_highlight: s.highlight_text,
            modes: [m; 4],
        }
    }

    pub fn for_mode(mode: Mode, sys: &SystemColors) -> AppColors {
        match mode {
            Mode::Light => Self::LIGHT,
            Mode::Dark => Self::DARK,
            Mode::HighContrast => Self::high_contrast(sys),
        }
    }

    pub fn is_dark(&self) -> bool {
        self.canvas.luminance() < 0.5
    }
}

/// 키오스크(큰 글씨) 화면 색(Flutter `KioskColors`와 같은 값).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KioskColors {
    pub blue: Rgb,
    pub blue_fill: Rgb,
    pub on_blue: Rgb,
    pub blue_soft: Rgb,
    pub ink: Rgb,
    pub sub: Rgb,
    pub faint: Rgb,
    pub line: Rgb,
    pub canvas: Rgb,
    pub card: Rgb,
    pub green: Rgb,
    pub red: Rgb,
    pub disabled_fill: Rgb,
    pub secondary_fill: Rgb,
}

impl KioskColors {
    pub const LIGHT: KioskColors = KioskColors {
        blue: Rgb(0x3182f6), blue_fill: Rgb(0x3182f6), on_blue: Rgb(0xffffff), blue_soft: Rgb(0xe8f3ff),
        ink: Rgb(0x191f28), sub: Rgb(0x4e5968), faint: Rgb(0x8b95a1), line: Rgb(0xe5e8eb),
        canvas: Rgb(0xf2f4f6), card: Rgb(0xffffff), green: Rgb(0x03b26c), red: Rgb(0xf04452),
        disabled_fill: Rgb(0xd1d6db), secondary_fill: Rgb(0xf2f4f6),
    };
    pub const DARK: KioskColors = KioskColors {
        blue: Rgb(0x6ea8ff), blue_fill: Rgb(0x2563eb), on_blue: Rgb(0xffffff), blue_soft: Rgb(0x1c2a44),
        ink: Rgb(0xe8eef6), sub: Rgb(0xb4c0cf), faint: Rgb(0x8e9bad), line: Rgb(0x2e3846),
        canvas: Rgb(0x0f141a), card: Rgb(0x171d26), green: Rgb(0x4ade80), red: Rgb(0xff7b7b),
        disabled_fill: Rgb(0x2e3846), secondary_fill: Rgb(0x212a35),
    };
    pub fn high_contrast(s: &SystemColors) -> KioskColors {
        KioskColors {
            blue: s.hotlight, blue_fill: s.highlight, on_blue: s.highlight_text, blue_soft: s.window,
            ink: s.window_text, sub: s.window_text, faint: s.gray_text, line: s.window_text,
            canvas: s.window, card: s.window, green: s.window_text, red: s.window_text,
            disabled_fill: s.button_face, secondary_fill: s.button_face,
        }
    }
}

/// 단축키 창(몽땅클립보드 빠른 목록·가상 폴더) 색. GDI COLORREF(BGR) 그대로 둔다.
/// `LIGHT`는 UI0 이전 popup·kvf의 상수와 한 값도 다르지 않다(그리기 해시로 확인).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipPalette {
    pub window: u32,
    pub head: u32,
    pub head_hover: u32,
    pub head_pressed: u32,
    pub head_on: u32,
    pub close_hover: u32,
    pub on_close_hover: u32,
    pub accent: u32,
    pub glyph: u32,
    pub title: u32,
    pub hint: u32,
    pub tab_bar: u32,
    pub tab_line: u32,
    pub tab_hover: u32,
    pub tab_text: u32,
    pub placeholder: u32,
    pub empty_text: u32,
    pub selected_row: u32,
    pub hover_row: u32,
    pub shortcut: u32,
    pub tag: u32,
    pub row_action: u32,
    /// kvf 머리글·분류 표시줄.
    pub vf_head: u32,
    pub vf_text: u32,
    pub vf_tab_mark: u32,
    pub border: u32,
}

impl ClipPalette {
    pub const LIGHT: ClipPalette = ClipPalette {
        window: 0x00FFFFFF,
        head: 0x00F5EFE8,
        head_hover: 0x00E0D8CF,
        head_pressed: 0x00CFC6BC,
        head_on: 0x00F2DCC4,
        close_hover: 0x002311E8,
        on_close_hover: 0x00FFFFFF,
        accent: 0x00D9731F,
        glyph: 0x00333333,
        title: 0x00222222,
        hint: 0x00777777,
        tab_bar: 0x00FAFAFA,
        tab_line: 0x00E3E3E3,
        tab_hover: 0x00EFEFEF,
        tab_text: 0x00444444,
        placeholder: 0x00888888,
        empty_text: 0x00666666,
        selected_row: 0x00F7E3CF,
        hover_row: 0x00FBF3EC,
        shortcut: 0x00EB6325,
        tag: 0x00888888,
        row_action: 0x00555555,
        vf_head: 0x00f4f4f4,
        vf_text: 0x00222222,
        vf_tab_mark: 0x00cc7733,
        border: crate::frame::BORDER,
    };

    /// 앱 색 토큰에서 만든 팔레트(다크·고대비용, UI1부터 단축키 창에 연결).
    pub fn from_app(c: &AppColors) -> ClipPalette {
        ClipPalette {
            window: c.surface.bgr(),
            head: c.surface_subtle.bgr(),
            head_hover: c.line.bgr(),
            head_pressed: c.line_strong.bgr(),
            head_on: c.accent_soft.bgr(),
            close_hover: if c.mode == Mode::HighContrast { c.accent_fill.bgr() } else { 0x002311E8 },
            on_close_hover: if c.mode == Mode::HighContrast { c.on_accent.bgr() } else { 0x00FFFFFF },
            accent: c.accent.bgr(),
            glyph: c.text_body.bgr(),
            title: c.text.bgr(),
            hint: c.muted.bgr(),
            tab_bar: c.surface_muted.bgr(),
            tab_line: c.line.bgr(),
            tab_hover: c.surface_subtle.bgr(),
            tab_text: c.text_body.bgr(),
            placeholder: c.faint.bgr(),
            empty_text: c.muted.bgr(),
            selected_row: if c.mode == Mode::HighContrast { c.highlight.bgr() } else { c.accent_soft.bgr() },
            hover_row: c.surface_muted.bgr(),
            shortcut: c.accent.bgr(),
            tag: c.muted.bgr(),
            row_action: c.text_secondary.bgr(),
            vf_head: c.surface_subtle.bgr(),
            vf_text: c.text.bgr(),
            vf_tab_mark: c.accent.bgr(),
            border: c.line_strong.bgr(),
        }
    }

    pub fn for_mode(mode: Mode, sys: &SystemColors) -> ClipPalette {
        match mode {
            Mode::Light => Self::LIGHT,
            _ => Self::from_app(&AppColors::for_mode(mode, sys)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_state_machine_all_combinations() {
        for choice in [Choice::System, Choice::Light, Choice::Dark] {
            for dark in [false, true] {
                // 고대비는 무엇보다 먼저.
                assert_eq!(resolve(choice, System { dark, high_contrast: true }), Mode::HighContrast);
            }
        }
        let sys = |dark| System { dark, high_contrast: false };
        assert_eq!(resolve(Choice::System, sys(false)), Mode::Light);
        assert_eq!(resolve(Choice::System, sys(true)), Mode::Dark);
        assert_eq!(resolve(Choice::Light, sys(true)), Mode::Light);
        assert_eq!(resolve(Choice::Dark, sys(false)), Mode::Dark);
    }

    #[test]
    fn choice_keys_round_trip_and_unknown_is_system() {
        for c in [Choice::System, Choice::Light, Choice::Dark] {
            assert_eq!(Choice::parse(c.key()), c);
        }
        assert_eq!(Choice::parse("blue"), Choice::System);
        assert_eq!(Choice::Dark.label(), "어둡게");
    }

    #[test]
    fn watcher_reports_only_changes() {
        let mut w = Watcher::new(Choice::System, System::default());
        assert_eq!(w.mode, Mode::Light);
        assert_eq!(w.update(System::default()), None);
        assert_eq!(w.update(System { dark: true, high_contrast: false }), Some(Mode::Dark));
        assert_eq!(w.set_choice(Choice::Light), Some(Mode::Light));
        assert_eq!(w.update(System { dark: true, high_contrast: true }), Some(Mode::HighContrast));
        assert_eq!(w.set_choice(Choice::Dark), None, "고대비 중 선택 변경은 화면을 바꾸지 않음");
        assert_eq!(w.update(System { dark: false, high_contrast: false }), Some(Mode::Dark));
    }

    #[test]
    fn theme_messages() {
        assert!(is_theme_message(WM_SETTINGCHANGE, Some("ImmersiveColorSet")));
        assert!(!is_theme_message(WM_SETTINGCHANGE, Some("intl")));
        assert!(is_theme_message(WM_SYSCOLORCHANGE, None));
        assert!(is_theme_message(WM_THEMECHANGED, None));
        assert!(!is_theme_message(0x0200, None));
        let s: Vec<u16> = "ImmersiveColorSet\0".encode_utf16().collect();
        assert_eq!(unsafe { setting_area(s.as_ptr() as isize) }.as_deref(), Some("ImmersiveColorSet"));
        assert_eq!(unsafe { setting_area(0) }, None);
    }

    #[test]
    fn bgr_conversion() {
        assert_eq!(Rgb(0x1F73D9).bgr(), 0x00D9731F, "popup ACCENT");
        assert_eq!(Rgb::from_bgr(0x00D9731F), Rgb(0x1F73D9));
    }

    /// 다크: 모든 글자 토큰이 모든 바탕 토큰 위에서 WCAG AA 4.5:1 이상(DARK1 기준).
    #[test]
    fn dark_text_tokens_meet_aa_on_every_surface() {
        let c = AppColors::DARK;
        let texts = [c.ink, c.text_strong, c.text, c.text_body, c.text_secondary, c.muted, c.faint, c.accent,
                     c.danger, c.success, c.warning, c.mint];
        let grounds = [c.canvas, c.surface, c.surface_muted, c.surface_subtle];
        for t in texts {
            for g in grounds {
                let r = contrast(t, g);
                assert!(r >= 4.5, "{t:?} on {g:?} = {r:.2}");
            }
        }
        assert!(contrast(c.on_accent, c.accent_fill) >= 4.5);
        assert!(contrast(c.on_highlight, c.highlight) >= 4.5);
        assert!(contrast(AppColors::LIGHT.on_accent, AppColors::LIGHT.accent_fill) >= 4.5);
    }

    #[test]
    fn high_contrast_uses_system_colors_only() {
        let s = SystemColors::BLACK;
        let c = AppColors::high_contrast(&s);
        assert_eq!(c.canvas, s.window);
        assert_eq!(c.text, s.window_text);
        assert_eq!(c.accent_fill, s.highlight);
        assert!(c.is_dark());
        let k = KioskColors::high_contrast(&s);
        assert_eq!(k.card, s.window);
        let p = ClipPalette::for_mode(Mode::HighContrast, &s);
        assert_eq!(p.window, s.window.bgr());
        assert_eq!(p.selected_row, s.highlight.bgr());
        assert_eq!(ClipPalette::for_mode(Mode::Light, &s), ClipPalette::LIGHT);
        assert!(contrast(Rgb::from_bgr(ClipPalette::for_mode(Mode::Dark, &s).title),
                         Rgb::from_bgr(ClipPalette::for_mode(Mode::Dark, &s).window)) >= 4.5);
    }

    #[test]
    fn system_queries_do_not_panic() {
        let _ = system();
        let _ = SystemColors::read();
    }
}
