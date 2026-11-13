//! 단축키 창(빠른 목록)·관리창이 함께 쓰는 화면 테마 상태(UI1).
//!
//! 상태 = 사용자 선택(`Settings.theme_mode`: system·light·dark) × 시스템(앱 다크 모드·고대비) → [`Mode`].
//! 결정은 `ui_core::theme::resolve`(순수 함수, 고대비가 무엇보다 먼저). 바뀌는 때:
//! - 설정 `theme_mode`가 바뀜(`apply_settings`) — 앱이 관리창을 열 때 자기 테마를 함께 보낸다.
//! - 창이 받은 WM_SETTINGCHANGE("ImmersiveColorSet")·WM_SYSCOLORCHANGE·WM_THEMECHANGED(`on_message`).
//!
//! 바뀌었는지(true)만 돌려주고, 다시 그리기는 부른 쪽이 한다(빠른 목록·관리창 둘 다 무효화).

use std::sync::Mutex;

use ui_core::theme::{AppColors, Choice, ClipPalette, Mode, SystemColors, Watcher};

struct Look {
    watcher: Option<Watcher>,
    /// 시험에서 고정한 테마(그리기 해시를 PC 설정과 떼어 놓으려고).
    forced: Option<Mode>,
}

static LOOK: Mutex<Look> = Mutex::new(Look { watcher: None, forced: None });

fn with<R>(f: impl FnOnce(&mut Look) -> R) -> R {
    let mut g = LOOK.lock().unwrap();
    if g.watcher.is_none() {
        let choice = crate::APP.get().map(|a| Choice::parse(&a.settings.lock().unwrap().theme_mode)).unwrap_or_default();
        g.watcher = Some(Watcher::new(choice, ui_core::theme::system()));
    }
    f(&mut g)
}

/// 지금 그릴 테마.
pub fn mode() -> Mode {
    with(|l| l.forced.unwrap_or_else(|| l.watcher.map(|w| w.mode).unwrap_or(Mode::Light)))
}

/// 설정의 테마 선택을 반영. 바뀌었으면 true.
pub fn apply_choice(theme_mode: &str) -> bool {
    let choice = Choice::parse(theme_mode);
    with(|l| l.watcher.as_mut().and_then(|w| w.set_choice(choice)).is_some())
}

/// 창 프로시저의 시스템 알림. 테마가 바뀌었으면 true.
///
/// # Safety
/// WM_SETTINGCHANGE면 `lp`는 0이거나 유효한 문자열 포인터여야 한다(Windows가 보장).
pub unsafe fn on_message(msg: u32, lp: isize) -> bool {
    with(|l| l.watcher.as_mut().and_then(|w| unsafe { w.on_message(msg, lp) }).is_some())
}

fn system_colors(mode: Mode) -> SystemColors {
    if mode == Mode::HighContrast { SystemColors::read() } else { SystemColors::BLACK }
}

/// 빠른 목록 색(라이트는 UI0 이전 상수와 같은 값).
pub fn clip_palette() -> ClipPalette {
    let m = mode();
    ClipPalette::for_mode(m, &system_colors(m))
}

/// 관리창 단추·체크 색(DARK1 토큰).
pub fn app_colors() -> AppColors {
    let m = mode();
    AppColors::for_mode(m, &system_colors(m))
}

/// 사용자 틀의 DWM 테두리 색·다크 제목 표시줄을 지금 테마로.
///
/// # Safety
/// `hwnd`는 살아 있는 최상위 창이어야 한다.
pub unsafe fn frame_for(hwnd: windows_sys::Win32::Foundation::HWND) {
    let pal = clip_palette();
    unsafe {
        ui_core::frame::style_frame_with(hwnd, pal.border);
        ui_core::frame::set_dark_frame(hwnd, mode() == Mode::Dark);
    }
}

/// 시험 전용: 테마 고정(None = 풀기).
#[cfg(test)]
pub fn force(mode: Option<Mode>) {
    with(|l| l.forced = mode);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forced_mode_controls_palettes_and_light_equals_ui0_constants() {
        let _g = crate::look::tests_lock();
        force(Some(Mode::Light));
        assert_eq!(clip_palette(), ClipPalette::LIGHT);
        force(Some(Mode::Dark));
        assert_ne!(clip_palette(), ClipPalette::LIGHT);
        assert!(app_colors().is_dark());
        force(Some(Mode::HighContrast));
        assert_eq!(mode(), Mode::HighContrast);
        force(Some(Mode::Light));
    }
}

/// 테마를 고정하는 시험끼리 겹치지 않게(창 시험과 같은 순서 잠금).
#[cfg(test)]
pub fn tests_lock() -> std::sync::MutexGuard<'static, ()> {
    static L: Mutex<()> = Mutex::new(());
    L.lock().unwrap_or_else(|e| e.into_inner())
}
