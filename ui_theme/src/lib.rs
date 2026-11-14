//! Rust 네이티브 창(kagent 빠른 목록·그림 미리보기, kvf 가상 폴더 창, 앞으로 UI0 공용 부품)의 색 표.
//!
//! - 순수 부분(이 파일): `Palette`(BGR COLORREF 색 묶음) `LIGHT`·`DARK`·`high_contrast`, 사용자 선호
//!   `ThemePref`, 해석 `resolve`, WCAG 대비 계산, 테마 메시지 판정, 그리고 현재 테마 상태 하나(`CURRENT`).
//!   모든 OS에서 `cargo test`로 시험한다.
//! - Windows 부분(`win.rs`): 시스템 앱 모드(AppsUseLightTheme)·고대비 읽기, DWM 어두운 제목줄,
//!   메뉴 어둡게(uxtheme 서수), 테마 변경 알림 방송.
//!
//! 테마 상태 기계(팀 규칙 0절 — 상태는 `CURRENT` 한 곳, 전이는 `store` 한 곳):
//! | 상태 | 사건 | 다음 상태 | 할 일 |
//! |---|---|---|---|
//! | `Theme{pref,kind,palette}` | 사용자 설정 바뀜(pref') | `Theme::derive(pref', 시스템)` | `Change.pref` → 다른 프로세스에 방송, `Change.look` → 다시 그리기 |
//! | 같음 | 시스템 테마·고대비·시스템 색 바뀜(테마 메시지) | `Theme::derive(pref, 시스템)` | `Change.look` → 다시 그리기 |
//! | 같음 | 다른 프로세스 방송(DocFitThemeChanged) | 설정 다시 읽고 위와 같음 | 같음 |
//! 고대비가 켜져 있으면 선호와 상관없이 `HighContrast`(시스템 색 그대로).

use std::sync::Mutex;

#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::*;

/// RGB를 GDI COLORREF(0x00BBGGRR)로.
pub const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

/// 한 테마의 색(모두 COLORREF, BGR). 이름은 그리는 자리 기준.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    /// 창 바탕(목록·선택된 분류 칸·미리보기 바탕).
    pub bg: u32,
    /// 빠른 목록 머리글(상단 메뉴바) 바탕.
    pub head_bg: u32,
    /// 머리글 단추 마우스 올림·누름 바탕.
    pub head_button_hover: u32,
    pub head_button_pressed: u32,
    /// 켜진 토글 단추(항상 위) 바탕과 글리프.
    pub toggle_on_bg: u32,
    pub toggle_on_text: u32,
    /// 닫기 단추 마우스 올림 바탕(빨강)과 그 위 글리프.
    pub close_hover: u32,
    pub on_close_hover: u32,
    /// 머리글 단추 글리프.
    pub head_glyph: u32,
    /// 분류 메뉴바 바탕·마우스 올림·글자·아래 구분선.
    pub tab_bg: u32,
    pub tab_hover: u32,
    pub tab_text: u32,
    pub divider: u32,
    /// 본문 글자(제목·항목·입력).
    pub text: u32,
    /// 빈 목록 안내.
    pub text_muted: u32,
    /// 머리글 도움말.
    pub text_sub: u32,
    /// 자리 표시 글자·항목 종류 꼬리표.
    pub text_faint: u32,
    /// 항목 줄의 고정·지우기 아이콘.
    pub icon: u32,
    /// 강조(선택된 분류 글자·밑줄, 찾기 칸 테두리).
    pub accent: u32,
    /// 빠른 선택 번호(1~9).
    pub number: u32,
    /// 선택된 줄 바탕.
    pub selected: u32,
    /// 선택된 줄의 모든 글자를 이 색으로(고대비 HIGHLIGHTTEXT). None이면 평소 색 그대로.
    pub selected_text: Option<u32>,
    /// 마우스 올린 줄 바탕.
    pub hover: u32,
    /// 창 테두리(DWM 테두리 색·직접 그린 1px 선).
    pub border: u32,
    /// 그림 미리보기 테두리.
    pub preview_border: u32,
    /// kvf 머리글 바탕.
    pub plain_head_bg: u32,
    /// kvf 선택된 영역 밑줄.
    pub accent_line: u32,
}

/// 고대비 팔레트 재료(GetSysColor 값).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SysColors {
    pub window: u32,
    pub window_text: u32,
    pub highlight: u32,
    pub highlight_text: u32,
    pub btn_face: u32,
    pub gray_text: u32,
    pub hotlight: u32,
}

impl Palette {
    /// 밝게: 이전 popup.rs·preview.rs·frame.rs·kvf window.rs 하드코딩 값 그대로(화면 픽셀 동일).
    pub const LIGHT: Palette = Palette {
        bg: 0x00FFFFFF,
        head_bg: 0x00F5EFE8,
        head_button_hover: 0x00E0D8CF,
        head_button_pressed: 0x00CFC6BC,
        toggle_on_bg: 0x00F2DCC4,
        toggle_on_text: 0x00D9731F,
        close_hover: 0x002311E8,
        on_close_hover: 0x00FFFFFF,
        head_glyph: 0x00333333,
        tab_bg: 0x00FAFAFA,
        tab_hover: 0x00EFEFEF,
        tab_text: 0x00444444,
        divider: 0x00E3E3E3,
        text: 0x00222222,
        text_muted: 0x00666666,
        text_sub: 0x00777777,
        text_faint: 0x00888888,
        icon: 0x00555555,
        accent: 0x00D9731F, // #1F73D9
        number: 0x00EB6325, // #2563EB
        selected: 0x00F7E3CF,
        selected_text: None,
        hover: 0x00FBF3EC,
        border: 0x00BDBDBD,
        preview_border: 0x00B4B4B4,
        plain_head_bg: 0x00F4F4F4,
        accent_line: 0x00CC7733,
    };

    /// 어둡게: Flutter 다크 토큰(바탕 #171d26·#0f141a, 글자 #e2e8f0, 보조 #9eabbb, 강조 #6ea2ff) 기준.
    /// 글자/바탕 쌍은 모두 WCAG 4.5:1 이상(`text_pairs` 시험).
    pub const DARK: Palette = Palette {
        bg: rgb(0x17, 0x1D, 0x26),
        head_bg: rgb(0x0F, 0x14, 0x1A),
        head_button_hover: rgb(0x26, 0x30, 0x40),
        head_button_pressed: rgb(0x33, 0x41, 0x55),
        toggle_on_bg: rgb(0x1D, 0x35, 0x57),
        toggle_on_text: rgb(0x6E, 0xA2, 0xFF),
        close_hover: rgb(0xE8, 0x11, 0x23),
        on_close_hover: rgb(0xFF, 0xFF, 0xFF),
        head_glyph: rgb(0xCB, 0xD5, 0xE1),
        tab_bg: rgb(0x1B, 0x22, 0x2C),
        tab_hover: rgb(0x26, 0x30, 0x40),
        tab_text: rgb(0xCB, 0xD5, 0xE1),
        divider: rgb(0x2B, 0x36, 0x44),
        text: rgb(0xE2, 0xE8, 0xF0),
        text_muted: rgb(0xAA, 0xB6, 0xC5),
        text_sub: rgb(0x9E, 0xAB, 0xBB),
        text_faint: rgb(0x94, 0xA3, 0xB8),
        icon: rgb(0xB8, 0xC4, 0xD2),
        accent: rgb(0x6E, 0xA2, 0xFF),
        number: rgb(0x7F, 0xB0, 0xFF),
        selected: rgb(0x1D, 0x35, 0x57),
        selected_text: None,
        hover: rgb(0x21, 0x2B, 0x38),
        border: rgb(0x3A, 0x46, 0x56),
        preview_border: rgb(0x3A, 0x46, 0x56),
        plain_head_bg: rgb(0x0F, 0x14, 0x1A),
        accent_line: rgb(0x6E, 0xA2, 0xFF),
    };

    /// 고대비: OS 고대비 테마 색만 쓴다(사용자가 고른 대비를 그대로 존중).
    pub fn high_contrast(sys: &SysColors) -> Palette {
        Palette {
            bg: sys.window,
            head_bg: sys.btn_face,
            head_button_hover: sys.window,
            head_button_pressed: sys.window,
            toggle_on_bg: sys.highlight,
            toggle_on_text: sys.highlight_text,
            close_hover: sys.highlight,
            on_close_hover: sys.highlight_text,
            head_glyph: sys.window_text,
            tab_bg: sys.btn_face,
            tab_hover: sys.window,
            tab_text: sys.window_text,
            divider: sys.window_text,
            text: sys.window_text,
            text_muted: sys.window_text,
            text_sub: sys.window_text,
            text_faint: sys.gray_text,
            icon: sys.window_text,
            accent: sys.hotlight,
            number: sys.hotlight,
            selected: sys.highlight,
            selected_text: Some(sys.highlight_text),
            hover: sys.btn_face,
            border: sys.window_text,
            preview_border: sys.window_text,
            plain_head_bg: sys.btn_face,
            accent_line: sys.hotlight,
        }
    }

    pub fn of(kind: Kind, sys: &SysColors) -> Palette {
        match kind {
            Kind::Light => Palette::LIGHT,
            Kind::Dark => Palette::DARK,
            Kind::HighContrast => Palette::high_contrast(sys),
        }
    }

    /// 선택된 줄이면 `selected_text`(있을 때), 아니면 주어진 색.
    pub fn row_text(&self, selected: bool, normal: u32) -> u32 {
        match self.selected_text {
            Some(c) if selected => c,
            _ => normal,
        }
    }

    /// 실제로 겹쳐 그리는 (이름, 글자, 바탕) 쌍. 대비 시험·보고에 쓴다.
    pub fn text_pairs(&self) -> Vec<(&'static str, u32, u32)> {
        let p = self;
        vec![
            ("본문/바탕", p.text, p.bg),
            ("본문/머리글", p.text, p.head_bg),
            ("본문/선택 줄", p.row_text(true, p.text), p.selected),
            ("본문/올린 줄", p.text, p.hover),
            ("본문/kvf 머리글", p.text, p.plain_head_bg),
            ("본문/kvf 선택 줄", p.row_text(true, p.text), p.selected),
            ("도움말/머리글", p.text_sub, p.head_bg),
            ("안내/바탕", p.text_muted, p.bg),
            ("흐린 글자/바탕", p.text_faint, p.bg),
            ("흐린 글자/선택 줄", p.row_text(true, p.text_faint), p.selected),
            ("흐린 글자/올린 줄", p.text_faint, p.hover),
            ("분류 글자/분류 바탕", p.tab_text, p.tab_bg),
            ("분류 글자/분류 올림", p.tab_text, p.tab_hover),
            ("강조/바탕(선택 분류)", p.accent, p.bg),
            ("단추 글리프/머리글", p.head_glyph, p.head_bg),
            ("단추 글리프/단추 올림", p.head_glyph, p.head_button_hover),
            ("단추 글리프/단추 누름", p.head_glyph, p.head_button_pressed),
            ("켜진 단추", p.toggle_on_text, p.toggle_on_bg),
            ("닫기 올림", p.on_close_hover, p.close_hover),
            ("번호/바탕", p.number, p.bg),
            ("번호/선택 줄", p.row_text(true, p.number), p.selected),
            ("번호/올린 줄", p.number, p.hover),
            ("아이콘/올린 줄", p.icon, p.hover),
        ]
    }

    /// `text_pairs` 중 가장 낮은 대비.
    pub fn min_contrast(&self) -> f64 {
        self.text_pairs().iter().map(|(_, f, b)| contrast(*f, *b)).fold(f64::INFINITY, f64::min)
    }
}

/// 사용자 선호(kagent settings.json "theme").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemePref {
    System,
    Light,
    Dark,
}

impl ThemePref {
    pub const ALL: [ThemePref; 3] = [ThemePref::System, ThemePref::Light, ThemePref::Dark];

    /// "system"/"light"/"dark", 그 밖의 값은 System.
    pub fn parse(s: &str) -> ThemePref {
        match s {
            "light" => ThemePref::Light,
            "dark" => ThemePref::Dark,
            _ => ThemePref::System,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            ThemePref::System => "system",
            ThemePref::Light => "light",
            ThemePref::Dark => "dark",
        }
    }
    /// 설정 값으로 받아들이는 문자열인지(그 밖의 값은 설정에서 무시).
    pub fn is_valid(s: &str) -> bool {
        matches!(s, "system" | "light" | "dark")
    }
}

/// 실제로 그리는 테마.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Light,
    Dark,
    HighContrast,
}

/// 선호 + 시스템 상태 → 테마. 고대비가 켜져 있으면 무조건 HighContrast.
pub fn resolve(pref: ThemePref, system_dark: bool, high_contrast: bool) -> Kind {
    if high_contrast {
        return Kind::HighContrast;
    }
    match pref {
        ThemePref::Light => Kind::Light,
        ThemePref::Dark => Kind::Dark,
        ThemePref::System if system_dark => Kind::Dark,
        ThemePref::System => Kind::Light,
    }
}

// ---- WCAG 2.x 대비 ----

fn channel(c: u32) -> f64 {
    let v = (c & 0xFF) as f64 / 255.0;
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

/// 상대 휘도(COLORREF BGR).
pub fn luminance(bgr: u32) -> f64 {
    let (r, g, b) = (channel(bgr), channel(bgr >> 8), channel(bgr >> 16));
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// 대비 비(1~21).
pub fn contrast(a: u32, b: u32) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

// ---- 테마 메시지 판정(순수) ----

pub const MSG_SETTINGCHANGE: u32 = 0x001A;
pub const MSG_SYSCOLORCHANGE: u32 = 0x0015;
pub const MSG_THEMECHANGED: u32 = 0x031A;
pub const SPI_SETHIGHCONTRAST_CODE: usize = 0x0043;
/// RegisterWindowMessageW 이름(kagent가 테마 설정을 바꾸면 방송, kvf 등이 받아 다시 읽는다).
pub const CHANGED_MESSAGE_NAME: &str = "DocFitThemeChanged";

/// 테마를 다시 읽어야 하는 메시지인지. `area`는 WM_SETTINGCHANGE lParam 문자열, `registered`는
/// `CHANGED_MESSAGE_NAME` 등록 번호(0이면 없음).
pub fn is_theme_event(msg: u32, wparam: usize, area: Option<&str>, registered: u32) -> bool {
    match msg {
        MSG_SETTINGCHANGE => area == Some("ImmersiveColorSet") || wparam == SPI_SETHIGHCONTRAST_CODE,
        MSG_SYSCOLORCHANGE | MSG_THEMECHANGED => true,
        m => registered != 0 && m == registered,
    }
}

// ---- 현재 테마 상태(프로세스에 하나) ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub pref: ThemePref,
    pub kind: Kind,
    pub palette: Palette,
}

impl Theme {
    pub const INITIAL: Theme = Theme { pref: ThemePref::System, kind: Kind::Light, palette: Palette::LIGHT };

    /// 선호와 시스템 상태로 다음 테마를 만든다(순수 전이).
    pub fn derive(pref: ThemePref, system_dark: bool, high_contrast: bool, sys: &SysColors) -> Theme {
        let kind = resolve(pref, system_dark, high_contrast);
        Theme { pref, kind, palette: Palette::of(kind, sys) }
    }
}

/// 전이 결과: 선호가 바뀜(다른 프로세스에 알릴 일), 보이는 색이 바뀜(다시 그릴 일).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Change {
    pub pref: bool,
    pub look: bool,
}

impl Change {
    pub fn between(old: &Theme, new: &Theme) -> Change {
        Change { pref: old.pref != new.pref, look: old.kind != new.kind || old.palette != new.palette }
    }
}

static CURRENT: Mutex<Theme> = Mutex::new(Theme::INITIAL);

/// 현재 테마.
pub fn theme() -> Theme {
    *CURRENT.lock().unwrap_or_else(|e| e.into_inner())
}

/// 현재 팔레트(그리기 때마다 부른다).
pub fn palette() -> Palette {
    theme().palette
}

/// 상태를 바꾸는 유일한 곳. 바뀐 내용을 돌려준다.
pub fn store(next: Theme) -> Change {
    let mut cur = CURRENT.lock().unwrap_or_else(|e| e.into_inner());
    let change = Change::between(&cur, &next);
    *cur = next;
    change
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_matches_previous_hardcoded_colors() {
        let p = Palette::LIGHT;
        // popup.rs
        assert_eq!(p.accent, 0x00D9731F);
        assert_eq!(p.head_bg, 0x00F5EFE8);
        assert_eq!((p.bg, p.close_hover, p.head_button_pressed, p.head_button_hover), (0x00FFFFFF, 0x002311E8, 0x00CFC6BC, 0x00E0D8CF));
        assert_eq!((p.toggle_on_bg, p.on_close_hover, p.toggle_on_text, p.head_glyph), (0x00F2DCC4, 0x00FFFFFF, 0x00D9731F, 0x00333333));
        assert_eq!((p.text, p.text_sub, p.tab_bg, p.divider, p.tab_hover, p.tab_text), (0x00222222, 0x00777777, 0x00FAFAFA, 0x00E3E3E3, 0x00EFEFEF, 0x00444444));
        assert_eq!((p.text_faint, p.text_muted, p.selected, p.hover, p.number, p.icon), (0x00888888, 0x00666666, 0x00F7E3CF, 0x00FBF3EC, 0x00EB6325, 0x00555555));
        assert_eq!(p.selected_text, None);
        // frame.rs BORDER, preview.rs
        assert_eq!((p.border, p.preview_border), (0x00BDBDBD, 0x00B4B4B4));
        // kvf window.rs
        assert_eq!((p.plain_head_bg, p.accent_line), (0x00F4F4F4, 0x00CC7733));
        // 선택 줄 글자는 바뀌지 않는다.
        assert_eq!(p.row_text(true, 0x00888888), 0x00888888);
    }

    #[test]
    fn rgb_is_bgr_colorref() {
        assert_eq!(rgb(0x1F, 0x73, 0xD9), 0x00D9731F);
        assert_eq!(Palette::DARK.bg, 0x00261D17);
        assert_eq!(Palette::DARK.text, 0x00F0E8E2);
        assert_eq!(Palette::DARK.accent, 0x00FFA26E);
    }

    #[test]
    fn contrast_matches_wcag_reference_values() {
        assert!((contrast(0x00FFFFFF, 0) - 21.0).abs() < 1e-9);
        assert!((contrast(0x00777777, 0x00FFFFFF) - 4.48).abs() < 0.01);
        assert_eq!(contrast(0x00123456, 0x00123456), 1.0);
        assert_eq!(contrast(0x00FFFFFF, 0x00000000), contrast(0, 0x00FFFFFF));
    }

    #[test]
    fn dark_text_pairs_are_at_least_4_5() {
        let p = Palette::DARK;
        for (name, fg, bg) in p.text_pairs() {
            let c = contrast(fg, bg);
            assert!(c >= 4.5, "{name}: {c:.2}");
        }
        assert!(p.min_contrast() >= 4.5);
        // 바탕은 어둡고 글자는 밝다.
        assert!(luminance(p.bg) < 0.05 && luminance(p.text) > 0.7);
    }

    #[test]
    fn high_contrast_uses_system_colors_only() {
        let sys = SysColors { window: 0, window_text: 0x00FFFFFF, highlight: 0x00FFFF00, highlight_text: 0,
                              btn_face: 0, gray_text: 0x0000FF00, hotlight: 0x0000FFFF };
        let p = Palette::high_contrast(&sys);
        assert_eq!((p.bg, p.text, p.selected, p.accent, p.text_faint), (0, 0x00FFFFFF, 0x00FFFF00, 0x0000FFFF, 0x0000FF00));
        assert_eq!(p.row_text(true, p.number), 0);
        assert_eq!(p.row_text(false, p.number), 0x0000FFFF);
        // 검정 바탕 고대비 테마: 겹쳐 그리는 쌍이 모두 읽힌다.
        assert!(p.min_contrast() >= 4.5, "{}", p.min_contrast());
        assert_eq!(Palette::of(Kind::HighContrast, &sys), p);
        assert_eq!(Palette::of(Kind::Dark, &sys), Palette::DARK);
        assert_eq!(Palette::of(Kind::Light, &sys), Palette::LIGHT);
    }

    #[test]
    fn pref_parse_round_trips_and_defaults_to_system() {
        for p in ThemePref::ALL {
            assert_eq!(ThemePref::parse(p.as_str()), p);
            assert!(ThemePref::is_valid(p.as_str()));
        }
        for bad in ["", "Dark", "auto", "high", " light"] {
            assert_eq!(ThemePref::parse(bad), ThemePref::System, "{bad:?}");
            assert!(!ThemePref::is_valid(bad));
        }
    }

    #[test]
    fn resolve_truth_table() {
        use Kind::*;
        use ThemePref as P;
        let cases = [
            (P::System, false, false, Light),
            (P::System, true, false, Dark),
            (P::Light, false, false, Light),
            (P::Light, true, false, Light),
            (P::Dark, false, false, Dark),
            (P::Dark, true, false, Dark),
        ];
        for (pref, dark, hc, want) in cases {
            assert_eq!(resolve(pref, dark, hc), want, "{pref:?} {dark} {hc}");
        }
        for pref in ThemePref::ALL {
            for dark in [false, true] {
                assert_eq!(resolve(pref, dark, true), HighContrast, "고대비는 무조건");
            }
        }
    }

    #[test]
    fn theme_messages() {
        let reg = 0xC123;
        assert!(is_theme_event(MSG_SETTINGCHANGE, 0, Some("ImmersiveColorSet"), reg));
        assert!(is_theme_event(MSG_SETTINGCHANGE, SPI_SETHIGHCONTRAST_CODE, None, reg));
        assert!(!is_theme_event(MSG_SETTINGCHANGE, 0, Some("intl"), reg));
        assert!(!is_theme_event(MSG_SETTINGCHANGE, 0x2F, None, reg));
        assert!(is_theme_event(MSG_SYSCOLORCHANGE, 0, None, reg));
        assert!(is_theme_event(MSG_THEMECHANGED, 0, None, reg));
        assert!(is_theme_event(reg, 0, None, reg));
        assert!(!is_theme_event(0x0400, 0, None, reg));
        assert!(!is_theme_event(0, 0, None, 0), "등록 실패(0)는 아무 메시지와도 맞지 않는다");
    }

    #[test]
    fn derive_and_change_detection() {
        let sys = SysColors::default();
        let a = Theme::derive(ThemePref::System, false, false, &sys);
        assert_eq!(a, Theme::INITIAL);
        let b = Theme::derive(ThemePref::System, true, false, &sys);
        assert_eq!(Change::between(&a, &b), Change { pref: false, look: true });
        let c = Theme::derive(ThemePref::Dark, true, false, &sys);
        assert_eq!(Change::between(&b, &c), Change { pref: true, look: false });
        let d = Theme::derive(ThemePref::Dark, true, true, &sys);
        assert_eq!(d.kind, Kind::HighContrast);
        assert_eq!(Change::between(&c, &d), Change { pref: false, look: true });
        // 고대비 중 시스템 색만 바뀌어도 다시 그린다.
        let sys2 = SysColors { window: 0x00FFFFFF, ..sys };
        let e = Theme::derive(ThemePref::Dark, true, true, &sys2);
        assert!(Change::between(&d, &e).look);
    }

    /// 상태는 `store` 한 곳에서만 바뀐다(전역 상태를 쓰는 시험은 이것 하나).
    #[test]
    fn store_is_the_only_transition() {
        let sys = SysColors::default();
        store(Theme::INITIAL);
        assert_eq!(palette(), Palette::LIGHT);
        let ch = store(Theme::derive(ThemePref::Dark, false, false, &sys));
        assert_eq!(ch, Change { pref: true, look: true });
        assert_eq!(theme().kind, Kind::Dark);
        assert_eq!(palette(), Palette::DARK);
        assert_eq!(store(theme()), Change::default(), "같은 상태면 할 일 없음");
        store(Theme::INITIAL);
    }
}
