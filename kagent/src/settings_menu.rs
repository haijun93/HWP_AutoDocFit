//! 빠른 목록 상단 메뉴바의 '설정' 메뉴(순수 모델, Win32 없음).
//!
//! 관리창(Flutter `kclip_dialog.dart`)과 같은 설정을 빠른 목록에서도 바꾼다. 값은 에이전트의
//! `Settings`(settings.json) 하나에만 저장하고, 바꿀 때는 파이프 `settings_set`과 같은 길
//! (`Settings::apply` 범위 검사 → 저장 → 정리 → 창·감시 갱신)을 지나므로 관리창과 값이 늘 같다.
//! 이 파일은 메뉴 구성·명령 번호·명령 → 설정 변경만 맡고, 메뉴 그리기와 실행은 `popup.rs`가 한다.

use crate::json::Value;
use crate::store::Settings;
use ui_theme::ThemePref;

/// 보관 개수·크기·기간 선택지(관리창 드롭다운과 같은 값 + 항목 하나 최대 크기).
pub const MAX_ITEMS: [i64; 6] = [20, 50, 100, 200, 500, 1000];
pub const MAX_TOTAL_MB: [i64; 5] = [50, 100, 200, 500, 1000];
pub const MAX_ITEM_MB: [i64; 5] = [1, 5, 10, 20, 50];
pub const MAX_AGE_DAYS: [i64; 4] = [7, 30, 90, 365];
/// 일시정지 시간(분). 0 = 다시 켤 때까지.
pub const PAUSE_MINUTES: [u32; 4] = [30, 60, 180, 0];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Number { MaxItems, MaxItemMb, MaxTotalMb, MaxAgeDays }

impl Number {
    const ALL: [Number; 4] = [Number::MaxItems, Number::MaxTotalMb, Number::MaxItemMb, Number::MaxAgeDays];

    fn key(self) -> &'static str {
        match self {
            Number::MaxItems => "max_items",
            Number::MaxItemMb => "max_item_mb",
            Number::MaxTotalMb => "max_total_mb",
            Number::MaxAgeDays => "max_age_days",
        }
    }
    fn code(self) -> usize {
        match self { Number::MaxItems => 1, Number::MaxItemMb => 2, Number::MaxTotalMb => 3, Number::MaxAgeDays => 4 }
    }
    fn from_code(c: usize) -> Option<Number> {
        Number::ALL.into_iter().find(|n| n.code() == c)
    }
    fn label(self) -> &'static str {
        match self {
            Number::MaxItems => "보관 개수",
            Number::MaxItemMb => "항목 하나 최대 크기",
            Number::MaxTotalMb => "전체 최대 크기",
            Number::MaxAgeDays => "보관 기간",
        }
    }
    fn choices(self) -> &'static [i64] {
        match self {
            Number::MaxItems => &MAX_ITEMS,
            Number::MaxItemMb => &MAX_ITEM_MB,
            Number::MaxTotalMb => &MAX_TOTAL_MB,
            Number::MaxAgeDays => &MAX_AGE_DAYS,
        }
    }
    fn unit(self, v: i64) -> String {
        match self {
            Number::MaxItems => format!("{v}개"),
            Number::MaxItemMb | Number::MaxTotalMb => format!("{v}MB"),
            Number::MaxAgeDays if v == 365 => "1년".into(),
            Number::MaxAgeDays => format!("{v}일"),
        }
    }
    /// 지금 설정값(관리창 `settings_get`이 돌려주는 단위와 같음).
    fn current(self, s: &Settings) -> i64 {
        match self {
            Number::MaxItems => s.max_items,
            Number::MaxItemMb => s.max_item_bytes / 1024 / 1024,
            Number::MaxTotalMb => s.max_total_bytes / 1024 / 1024,
            Number::MaxAgeDays => s.max_age_days.round() as i64,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flag { AutoPaste, Sticky, AlwaysOnTop, WatchScreenshots, ExcelTarget, PreviewImages }

impl Flag {
    // 명령 번호는 순서로 정해지므로 새 항목은 끝에 붙인다.
    const ALL: [Flag; 6] = [Flag::Sticky, Flag::AlwaysOnTop, Flag::AutoPaste, Flag::WatchScreenshots, Flag::ExcelTarget, Flag::PreviewImages];

    fn key(self) -> &'static str {
        match self {
            Flag::AutoPaste => "auto_paste",
            Flag::Sticky => "sticky_popup",
            Flag::AlwaysOnTop => "always_on_top",
            Flag::WatchScreenshots => "watch_screenshots",
            Flag::ExcelTarget => "excel_target_paste",
            Flag::PreviewImages => "preview_images",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Flag::AutoPaste => "고르면 바로 붙여넣기",
            Flag::Sticky => "스티커 메모처럼 화면에 유지",
            Flag::AlwaysOnTop => "항상 위",
            Flag::WatchScreenshots => "스크린샷 폴더 감시(캡처 도구·Win+PrtScn·캡처 앱 폴더 자동 인식)",
            Flag::ExcelTarget => "엑셀 표는 목표 결과로 붙이기",
            Flag::PreviewImages => "그림 미리보기",
        }
    }
    fn get(self, s: &Settings) -> bool {
        match self {
            Flag::AutoPaste => s.auto_paste,
            Flag::Sticky => s.sticky_popup,
            Flag::AlwaysOnTop => s.always_on_top,
            Flag::WatchScreenshots => s.watch_screenshots,
            Flag::ExcelTarget => s.excel_target_paste,
            Flag::PreviewImages => s.preview_images,
        }
    }
}

/// 메뉴에서 고를 수 있는 명령.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Command {
    Set(Number, i64),
    Toggle(Flag),
    Autostart(bool),
    /// 일시정지(분, 0 = 다시 켤 때까지).
    Pause(u32),
    /// 화면 테마(시스템 따라가기·밝게·어둡게).
    Theme(ThemePref),
    Resume,
    ClearAll,
    Quit,
}

const ID_AUTOSTART_ON: usize = 100;
const ID_AUTOSTART_OFF: usize = 101;
const ID_RESUME: usize = 110;
const ID_CLEAR: usize = 120;
const ID_QUIT: usize = 121;
const ID_FLAG: usize = 130; // + Flag 순서
const ID_THEME: usize = 160; // + ThemePref::ALL 순서
const ID_PAUSE: usize = 50_000; // + 분(0~1440)
const ID_NUMBER: usize = 10_000; // Number 코드 × 10000 + 값

/// 메뉴 명령 번호(TrackPopupMenu 반환값, 0은 '고르지 않음'이라 쓰지 않는다).
pub fn command_id(c: Command) -> usize {
    match c {
        Command::Set(n, v) => n.code() * ID_NUMBER + v.clamp(0, 9_999) as usize,
        Command::Toggle(f) => ID_FLAG + Flag::ALL.iter().position(|x| *x == f).unwrap_or(0),
        Command::Autostart(true) => ID_AUTOSTART_ON,
        Command::Autostart(false) => ID_AUTOSTART_OFF,
        Command::Pause(m) => ID_PAUSE + m.min(1440) as usize,
        Command::Theme(t) => ID_THEME + ThemePref::ALL.iter().position(|x| *x == t).unwrap_or(0),
        Command::Resume => ID_RESUME,
        Command::ClearAll => ID_CLEAR,
        Command::Quit => ID_QUIT,
    }
}

pub fn command_of(id: usize) -> Option<Command> {
    match id {
        ID_AUTOSTART_ON => Some(Command::Autostart(true)),
        ID_AUTOSTART_OFF => Some(Command::Autostart(false)),
        ID_RESUME => Some(Command::Resume),
        ID_CLEAR => Some(Command::ClearAll),
        ID_QUIT => Some(Command::Quit),
        _ if (ID_FLAG..ID_FLAG + Flag::ALL.len()).contains(&id) => Some(Command::Toggle(Flag::ALL[id - ID_FLAG])),
        _ if (ID_THEME..ID_THEME + ThemePref::ALL.len()).contains(&id) => Some(Command::Theme(ThemePref::ALL[id - ID_THEME])),
        _ if (ID_PAUSE..=ID_PAUSE + 1440).contains(&id) => Some(Command::Pause((id - ID_PAUSE) as u32)),
        _ if (ID_NUMBER..ID_PAUSE).contains(&id) => {
            Number::from_code(id / ID_NUMBER).map(|n| Command::Set(n, (id % ID_NUMBER) as i64))
        }
        _ => None,
    }
}

/// 메뉴 한 줄.
#[derive(Clone, PartialEq, Debug)]
pub enum Entry {
    Item { id: usize, label: String, checked: bool, radio: bool, enabled: bool },
    Separator,
    Sub { label: String, entries: Vec<Entry> },
}

fn item(c: Command, label: impl Into<String>, checked: bool, radio: bool) -> Entry {
    Entry::Item { id: command_id(c), label: label.into(), checked, radio, enabled: true }
}

fn note(label: impl Into<String>) -> Entry {
    Entry::Item { id: 0, label: label.into(), checked: false, radio: false, enabled: false }
}

/// 메뉴에 보이는 상태(설정 파일 밖의 값).
#[derive(Clone, Copy, Debug, Default)]
pub struct Extra {
    /// Windows 로그인 시 자동 실행 등록 여부. None = 읽지 못함.
    pub autostart: Option<bool>,
    /// 시간 일시정지면 다시 기록까지 남은 분.
    pub resume_in_min: Option<u64>,
}

fn theme_label(t: ThemePref) -> &'static str {
    match t {
        ThemePref::System => "시스템 설정 따라가기",
        ThemePref::Light => "밝게",
        ThemePref::Dark => "어둡게",
    }
}

fn pause_label(m: u32) -> String {
    match m {
        0 => "다시 켤 때까지".into(),
        m if m % 60 == 0 => format!("{}시간", m / 60),
        m => format!("{m}분"),
    }
}

/// 설정 메뉴 전체(위에서 아래 순서). 지금 값은 체크 표시, 선택지에 없는 값은 맨 위에 '현재 값'으로 보인다.
pub fn build(s: &Settings, extra: Extra) -> Vec<Entry> {
    let mut out = Vec::new();
    // 기록 상태
    if s.paused {
        let status = match extra.resume_in_min {
            Some(m) => format!("일시정지 중 · {}분 뒤 다시 기록", m.max(1)),
            None => "일시정지 중".into(),
        };
        out.push(note(status));
        out.push(item(Command::Resume, "다시 기록", false, false));
    } else {
        let entries = PAUSE_MINUTES.iter().map(|m| item(Command::Pause(*m), pause_label(*m), false, false)).collect();
        out.push(Entry::Sub { label: "일시정지".into(), entries });
    }
    out.push(Entry::Separator);
    // 보관
    for n in Number::ALL {
        let cur = n.current(s);
        let mut entries = Vec::new();
        if !n.choices().contains(&cur) {
            entries.push(item(Command::Set(n, cur), format!("현재 {}", n.unit(cur)), true, true));
            entries.push(Entry::Separator);
        }
        entries.extend(n.choices().iter().map(|v| item(Command::Set(n, *v), n.unit(*v), *v == cur, true)));
        out.push(Entry::Sub { label: format!("{}: {}", n.label(), n.unit(cur)), entries });
    }
    out.push(Entry::Separator);
    // 켜고 끄는 설정
    for f in Flag::ALL {
        out.push(item(Command::Toggle(f), f.label(), f.get(s), false));
    }
    match extra.autostart {
        Some(on) => out.push(item(Command::Autostart(!on), "Windows 로그인 시 자동 실행", on, false)),
        None => out.push(note("Windows 로그인 시 자동 실행(확인 못 함)")),
    }
    out.push(note(format!("단축키: {} (관리창에서 안내)", s.hotkey)));
    // 화면 테마(고대비가 켜져 있으면 선택과 상관없이 시스템 고대비 색)
    let theme = ThemePref::parse(&s.theme);
    let entries = ThemePref::ALL.iter().map(|t| item(Command::Theme(*t), theme_label(*t), *t == theme, true)).collect();
    out.push(Entry::Sub { label: format!("화면 테마: {}", theme_label(theme)), entries });
    out.push(Entry::Separator);
    out.push(item(Command::ClearAll, "모두 지우기(고정 항목 제외)…", false, false));
    out.push(item(Command::Quit, "몽땅클립보드 끄기…", false, false));
    out
}

/// 설정 파일 값을 바꾸는 명령이면 `settings_set`에 보낼 값(관리창과 같은 키·단위).
/// 일시정지·자동 실행·지우기·끄기는 따로 처리하므로 None.
pub fn settings_patch(s: &Settings, c: Command) -> Option<Value> {
    match c {
        Command::Set(n, v) => Some(Value::obj(vec![(n.key(), v.into())])),
        Command::Toggle(f) => Some(Value::obj(vec![(f.key(), (!f.get(s)).into())])),
        Command::Theme(t) => Some(Value::obj(vec![("theme", t.as_str().into())])),
        _ => None,
    }
}

// ---- Windows 로그인 시 자동 실행(앱 `kagent_client.set_autostart`와 같은 등록) ----

pub const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
pub const RUN_NAME: &str = "HwpAutoDocFit.KAgent";

/// 등록 값: 로그인 때 30초 늦게 시작(앱과 같은 형식).
pub fn autostart_command(exe: &str) -> String {
    format!("\"{exe}\" --delay 30")
}

/// 앱의 `autostart_enabled()`와 같은 판정.
pub fn is_autostart_command(value: &str) -> bool {
    value.to_lowercase().contains("kagent.exe")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(entries: &[Entry]) -> Vec<usize> {
        let mut out = Vec::new();
        for e in entries {
            match e {
                Entry::Item { id, .. } if *id != 0 => out.push(*id),
                Entry::Sub { entries, .. } => out.extend(ids(entries)),
                _ => {}
            }
        }
        out
    }

    fn find<'a>(entries: &'a [Entry], prefix: &str) -> &'a Entry {
        entries.iter().find(|e| match e {
            Entry::Item { label, .. } | Entry::Sub { label, .. } => label.starts_with(prefix),
            Entry::Separator => false,
        }).unwrap_or_else(|| panic!("메뉴에 '{prefix}' 없음"))
    }

    #[test]
    fn every_menu_id_round_trips_and_is_unique() {
        let mut s = Settings::default();
        let mut all = ids(&build(&s, Extra { autostart: Some(false), resume_in_min: None }));
        s.paused = true;
        s.max_items = 77; // 선택지에 없는 값
        all.extend(ids(&build(&s, Extra { autostart: Some(true), resume_in_min: Some(5) })));
        all.sort();
        all.dedup();
        for id in &all {
            let c = command_of(*id).unwrap_or_else(|| panic!("{id} 해석 못 함"));
            assert_eq!(command_id(c), *id);
        }
        // 숫자 명령끼리, 다른 명령과 번호가 겹치지 않는다.
        let cmds: Vec<_> = all.iter().map(|i| command_of(*i).unwrap()).collect();
        for (i, a) in cmds.iter().enumerate() {
            for b in &cmds[i + 1..] {
                assert_ne!(command_id(*a), command_id(*b));
            }
        }
        assert!(all.iter().all(|id| *id > 0 && *id <= 0xFFFF));
        assert_eq!(command_of(0), None);
        assert_eq!(command_of(5 * ID_NUMBER - 1 + 1), Some(Command::Pause(0)));
        assert_eq!(command_of(9 * ID_NUMBER + 5), None);
    }

    #[test]
    fn menu_covers_all_management_window_settings() {
        let s = Settings::default();
        let m = build(&s, Extra { autostart: Some(false), resume_in_min: None });
        for prefix in ["일시정지", "보관 개수: 500개", "전체 최대 크기: 200MB", "항목 하나 최대 크기: 10MB", "보관 기간: 30일",
                       "스티커 메모처럼", "항상 위", "고르면 바로 붙여넣기", "스크린샷 폴더 감시", "엑셀 표는 목표 결과로 붙이기", "그림 미리보기",
                       "Windows 로그인 시 자동 실행", "단축키: Ctrl+Shift+V", "모두 지우기", "몽땅클립보드 끄기"] {
            find(&m, prefix);
        }
        // 켜진 설정은 체크 표시
        assert!(matches!(find(&m, "항상 위"), Entry::Item { checked: true, .. }));
        assert!(matches!(find(&m, "스티커"), Entry::Item { checked: false, .. }));
        assert!(matches!(find(&m, "엑셀 표는 목표 결과로"), Entry::Item { checked: true, .. }), "기본 켬");
        assert!(matches!(find(&m, "그림 미리보기"), Entry::Item { checked: true, .. }), "기본 켬");
        assert!(matches!(find(&m, "Windows 로그인"), Entry::Item { checked: false, .. }));
        // 단축키는 표시만
        assert!(matches!(find(&m, "단축키"), Entry::Item { enabled: false, id: 0, .. }));
    }

    #[test]
    fn number_submenu_marks_current_value_and_shows_unlisted_value() {
        let mut s = Settings::default();
        let m = build(&s, Extra::default());
        let Entry::Sub { entries, .. } = find(&m, "보관 개수") else { panic!() };
        let checked: Vec<_> = entries.iter().filter_map(|e| match e {
            Entry::Item { label, checked: true, radio: true, .. } => Some(label.clone()), _ => None }).collect();
        assert_eq!(checked, ["500개"]); // 기본값 500(사용자 결정 2026-11-07)
        s.max_items = 77;
        s.max_age_days = 365.0;
        let m = build(&s, Extra::default());
        let Entry::Sub { label, entries } = find(&m, "보관 개수") else { panic!() };
        assert_eq!(label, "보관 개수: 77개");
        assert!(matches!(&entries[0], Entry::Item { label, checked: true, .. } if label == "현재 77개"));
        assert!(matches!(find(&m, "보관 기간"), Entry::Sub { label, .. } if label == "보관 기간: 1년"));
    }

    #[test]
    fn pause_section_depends_on_state() {
        let mut s = Settings::default();
        let m = build(&s, Extra::default());
        let Entry::Sub { entries, .. } = find(&m, "일시정지") else { panic!("기록 중이면 일시정지 하위 메뉴") };
        let labels: Vec<_> = entries.iter().filter_map(|e| match e { Entry::Item { label, .. } => Some(label.as_str()), _ => None }).collect();
        assert_eq!(labels, ["30분", "1시간", "3시간", "다시 켤 때까지"]);
        s.paused = true;
        let m = build(&s, Extra { autostart: None, resume_in_min: Some(12) });
        assert!(matches!(find(&m, "일시정지 중"), Entry::Item { label, enabled: false, .. } if label.contains("12분 뒤")));
        assert!(matches!(find(&m, "다시 기록"), Entry::Item { id, .. } if command_of(*id) == Some(Command::Resume)));
        assert!(matches!(find(&m, "Windows 로그인"), Entry::Item { enabled: false, .. }));
    }

    #[test]
    fn patches_use_the_same_keys_and_validation_as_settings_set() {
        let s = Settings::default();
        let apply = |c: Command| {
            let mut t = s.clone();
            t.apply(&settings_patch(&s, c).unwrap());
            t
        };
        assert_eq!(apply(Command::Set(Number::MaxItems, 200)).max_items, 200);
        assert_eq!(apply(Command::Set(Number::MaxTotalMb, 500)).max_total_bytes, 500 * 1024 * 1024);
        assert_eq!(apply(Command::Set(Number::MaxItemMb, 1)).max_item_bytes, 1024 * 1024);
        assert_eq!(apply(Command::Set(Number::MaxAgeDays, 7)).max_age_days, 7.0);
        assert!(!apply(Command::Toggle(Flag::AlwaysOnTop)).always_on_top);
        assert!(apply(Command::Toggle(Flag::Sticky)).sticky_popup);
        assert!(!apply(Command::Toggle(Flag::AutoPaste)).auto_paste);
        assert!(!apply(Command::Toggle(Flag::WatchScreenshots)).watch_screenshots);
        assert!(!apply(Command::Toggle(Flag::ExcelTarget)).excel_target_paste);
        assert!(!apply(Command::Toggle(Flag::PreviewImages)).preview_images);
        // 범위 밖 값은 Settings::apply가 거부(관리창 settings_set과 같은 검사)
        assert_eq!(apply(Command::Set(Number::MaxItems, 5)).max_items, 500);
        assert_eq!(apply(Command::Set(Number::MaxItemMb, 99)).max_item_bytes, 10 * 1024 * 1024);
        assert_eq!(settings_patch(&s, Command::Pause(30)), None);
        assert_eq!(settings_patch(&s, Command::Quit), None);
    }

    #[test]
    fn every_offered_choice_is_accepted_by_settings_apply() {
        for n in Number::ALL {
            for v in n.choices() {
                let mut s = Settings::default();
                s.apply(&settings_patch(&s.clone(), Command::Set(n, *v)).unwrap());
                assert_eq!(n.current(&s), *v, "{n:?} {v}");
            }
        }
    }

    #[test]
    fn existing_flag_ids_are_unchanged_and_excel_target_is_new() {
        // 예전 메뉴 번호(130~133)는 그대로, 새 항목은 134(ExcelTarget), 135(PreviewImages).
        assert_eq!(command_id(Command::Toggle(Flag::Sticky)), 130);
        assert_eq!(command_id(Command::Toggle(Flag::WatchScreenshots)), 133);
        assert_eq!(command_id(Command::Toggle(Flag::ExcelTarget)), 134);
        assert_eq!(command_of(134), Some(Command::Toggle(Flag::ExcelTarget)));
        assert_eq!(command_id(Command::Toggle(Flag::PreviewImages)), 135);
        assert_eq!(command_of(135), Some(Command::Toggle(Flag::PreviewImages)));
    }

    #[test]
    fn theme_submenu_marks_current_choice_and_patches_settings() {
        let mut s = Settings::default();
        let m = build(&s, Extra::default());
        let Entry::Sub { label, entries } = find(&m, "화면 테마") else { panic!("테마 하위 메뉴") };
        assert_eq!(label, "화면 테마: 시스템 설정 따라가기");
        let rows: Vec<_> = entries.iter().filter_map(|e| match e {
            Entry::Item { id, label, checked, radio: true, enabled: true } => Some((command_of(*id), label.as_str(), *checked)),
            _ => None }).collect();
        assert_eq!(rows, [
            (Some(Command::Theme(ThemePref::System)), "시스템 설정 따라가기", true),
            (Some(Command::Theme(ThemePref::Light)), "밝게", false),
            (Some(Command::Theme(ThemePref::Dark)), "어둡게", false),
        ]);
        // 고르면 settings_set과 같은 키·검사를 지난다.
        for t in ThemePref::ALL {
            let mut u = s.clone();
            u.apply(&settings_patch(&s, Command::Theme(t)).unwrap());
            assert_eq!(u.theme, t.as_str());
        }
        s.theme = "dark".into();
        let m = build(&s, Extra::default());
        assert!(matches!(find(&m, "화면 테마"), Entry::Sub { label, .. } if label == "화면 테마: 어둡게"));
        // 번호는 다른 명령과 겹치지 않는다(160~162).
        assert_eq!(command_id(Command::Theme(ThemePref::System)), 160);
        assert_eq!(command_of(163), None);
        assert_eq!(command_of(134), Some(Command::Toggle(Flag::ExcelTarget)));
        assert_eq!(command_of(135), Some(Command::Toggle(Flag::PreviewImages)));
    }

    #[test]
    fn autostart_value_matches_app_format() {
        let cmd = autostart_command("C:\\Users\\u\\AppData\\Local\\HwpAutoDocFit\\kagent\\kagent.exe");
        assert_eq!(cmd, "\"C:\\Users\\u\\AppData\\Local\\HwpAutoDocFit\\kagent\\kagent.exe\" --delay 30");
        assert!(is_autostart_command(&cmd));
        assert!(is_autostart_command("\"C:\\X\\KAGENT.EXE\""));
        assert!(!is_autostart_command("\"C:\\X\\other.exe\""));
    }
}
