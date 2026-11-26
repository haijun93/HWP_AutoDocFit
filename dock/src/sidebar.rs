//! 사이드바 실행 파일(`sidebar.exe`)의 순수 모델: 메뉴 목록, 메뉴별 실행 대상, 창 상태 기계, 배치 계산.
//! Win32 호출은 `src/bin/sidebar.rs` 에만 둔다.
//!
//! 메뉴마다 창은 하나다. 상태와 전이:
//!   Closed --누름--> Launching(실행 시각) --창 발견--> Open --창 사라짐--> Closed
//!   Launching --창 없이 비정상 종료 또는 시간 초과--> Closed(실패 안내)
//! 불가능한 전이: Launching 중 누름은 새로 실행하지 않는다. Open 에서 누름은 실행하지 않고 앞으로 가져온다.
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// 메뉴 번호는 앱 탭 번호(0~15)와 같고, 설정만 16 이다.
pub const SETTINGS_ENTRY: u8 = 16;
pub const APP_EXE: &str = "HWP_AutoDocFit.exe";
pub const APP_SCRIPT: &str = "hwp-auto-docfit.py";
/// 창이 이 시간 안에 뜨지 않으면 실패로 본다(느린 PC 의 첫 실행을 넉넉히 잡는다).
pub const LAUNCH_TIMEOUT_MS: u64 = 90_000;

/// 앱 왼쪽 메뉴와 같은 이름(`tab_navigation_shell.dart` `_labels`, 12·13 은 `_buildItems`).
pub fn label(id: u8) -> &'static str {
    const LABELS: [&str; 17] = [
        "문서편집", "표템플릿", "표그림판", "다이어그램", "마인드맵", "붙임사진 편집", "파일명 일괄변경",
        "몽땅검색", "문서뷰", "사진뷰", "파일변환", "PDF 편집", "가상 폴더", "업무자료함", "아웃라이너",
        "검색 벤치마크", "설정",
    ];
    LABELS.get(id as usize).copied().unwrap_or("")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: u8,
    /// 묶음 번호. 번호가 바뀌는 곳에 구분선을 긋는다.
    pub group: u8,
}

/// 앱 왼쪽 메뉴 규칙: 묶음 순서는 `SIDEBAR_GROUPS`, 가상 폴더·업무자료함은 '찾기·자료' 끝에 늘 보이고,
/// 검색 벤치마크는 개발자 모드에서만, 문서편집·12·13 은 숨길 수 없다. 설정은 맨 아래.
pub fn entries(hidden: &BTreeSet<u8>, developer_mode: bool) -> Vec<Entry> {
    let mut out = Vec::new();
    for (group, (_, tabs)) in crate::SIDEBAR_GROUPS.iter().enumerate() {
        let group = group as u8;
        for &id in *tabs {
            if (id == 15 && !developer_mode) || (id != 0 && hidden.contains(&id)) {
                continue;
            }
            out.push(Entry { id, group });
        }
        if group == 2 {
            out.extend([12, 13].map(|id| Entry { id, group }));
        }
    }
    out.push(Entry { id: SETTINGS_ENTRY, group: crate::SIDEBAR_GROUPS.len() as u8 });
    out
}

pub fn parse_developer_mode(settings_json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(settings_json)
        .ok()
        .and_then(|v| v.get("developer_mode").and_then(|x| x.as_bool()))
        .unwrap_or(false)
}

/// 기존 앱 화면을 메뉴 하나만 담아 여는 창의 제목. `docfit_app/screen_mode.py` 의 `window_title` 과 같아야 한다.
/// 앱 본 창을 찾는 검색어(앱 이름·"한글편집")는 일부러 넣지 않는다.
pub fn screen_title(id: u8) -> String {
    format!("{} — DocFit", label(id))
}

struct Native {
    exe: &'static str,
    class: &'static str,
}

fn native(id: u8) -> Option<Native> {
    let (exe, class) = match id {
        6 => ("rename_win.exe", "DocFitRenameWin2"),
        7 => ("ktm.exe", "HwpAutoDocFit.MongttangSearch"),
        8 => ("docview_win.exe", "DocViewWinMain"),
        9 => ("photoview_win.exe", "PhotoViewWinMain"),
        10 => ("convert_win.exe", "ConvertWinStage1"),
        14 => ("outliner_win.exe", "OutlinerWinMain"),
        SETTINGS_ENTRY => ("settings_win.exe", "SettingsWinMain"),
        _ => return None,
    };
    Some(Native { exe, class })
}

/// 기존 앱에 이 메뉴의 화면이 있는지(Rust 창이 없을 때의 대신 열기 포함). 몽땅검색·설정은 없다.
pub fn has_app_screen(id: u8) -> bool {
    id <= 15 && id != 7
}

/// 이미 열린 창을 찾는 방법.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Identity {
    Class(&'static str),
    Title(String),
}

/// 이 메뉴의 창일 수 있는 모든 식별자. Rust 창과 대신 연 앱 화면을 함께 찾는다.
pub fn identities(id: u8) -> Vec<Identity> {
    let mut out = Vec::new();
    if let Some(n) = native(id) {
        out.push(Identity::Class(n.class));
    }
    if has_app_screen(id) {
        out.push(Identity::Title(screen_title(id)));
    }
    out
}

/// 기존 앱(Flutter 화면·Python 뒷단)이 있는 곳.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppLocation {
    Installed(PathBuf),
    /// 개발용: 소스 폴더를 Python 으로 실행.
    Source { python: PathBuf, root: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Env {
    /// Rust 창·klib 가 있는 폴더(설치본은 `_internal\resources`, 곧 sidebar.exe 옆).
    pub resources: PathBuf,
    pub settings: PathBuf,
    /// `--app`
    pub app_override: Option<PathBuf>,
    /// `--python` + `--source-root`
    pub source: Option<(PathBuf, PathBuf)>,
}

impl Env {
    /// 지정(개발 소스 → 앱 실행 파일) → 설치본 배치(`{app}\_internal\resources\sidebar.exe`) → 옆 폴더.
    /// 지정한 파일이 없으면 다른 곳을 몰래 쓰지 않는다.
    pub fn app(&self, is_file: &dyn Fn(&Path) -> bool) -> Option<AppLocation> {
        if let Some((python, root)) = &self.source {
            return is_file(&root.join(APP_SCRIPT))
                .then(|| AppLocation::Source { python: python.clone(), root: root.clone() });
        }
        if let Some(exe) = &self.app_override {
            return is_file(exe).then(|| AppLocation::Installed(exe.clone()));
        }
        let installed = self.resources.parent().and_then(Path::parent).map(|d| d.join(APP_EXE));
        installed
            .into_iter()
            .chain([self.resources.join(APP_EXE)])
            .find(|p| is_file(p))
            .map(AppLocation::Installed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spawn {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub envs: Vec<(&'static str, OsString)>,
    pub cwd: Option<PathBuf>,
}

fn pair(args: &mut Vec<OsString>, name: &str, value: &Path) {
    args.push(name.into());
    args.push(value.into());
}

/// 메뉴를 처음 열 때 실행할 것. Rust 창이 있으면 그것(인자는 `docfit_app/*_window_service.py` 와 같다),
/// 없거나 파일이 빠졌으면 기존 앱의 `--screen N`.
pub fn spawn_for(id: u8, env: &Env, is_file: &dyn Fn(&Path) -> bool) -> Result<Spawn, String> {
    if id > SETTINGS_ENTRY {
        return Err("알 수 없는 메뉴입니다.".into());
    }
    if let Some(n) = native(id) {
        let program = env.resources.join(n.exe);
        if is_file(&program) {
            let mut spawn = Spawn { program, args: Vec::new(), envs: Vec::new(), cwd: None };
            match id {
                7 => spawn.args.push("--show".into()),
                SETTINGS_ENTRY => pair(&mut spawn.args, "--settings", &env.settings),
                8 | 9 => {
                    pair(&mut spawn.args, "--settings", &env.settings);
                    let klib = env.resources.join("klib.exe");
                    if is_file(&klib) {
                        pair(&mut spawn.args, "--klib", &klib);
                    }
                }
                14 => {
                    if let Some(folder) = env.settings.parent() {
                        pair(&mut spawn.args, "--data-dir", &folder.join("Outliner"));
                    }
                }
                // 변환 창은 엔진(기존 앱)을 `DOCFIT_APP_EXE` 나 자기 옆에서 찾는다. 설치본 앱은 두 단계 위라 알려 준다.
                10 => {
                    if let Some(AppLocation::Installed(exe)) = env.app(is_file) {
                        spawn.envs.push(("DOCFIT_APP_EXE", exe.into()));
                    }
                }
                _ => {}
            }
            return Ok(spawn);
        }
        if !has_app_screen(id) {
            return Err(format!(
                "{} 창 프로그램({})을 찾을 수 없습니다. 설치 프로그램을 다시 실행해 주세요.",
                label(id),
                n.exe
            ));
        }
    }
    let location = env
        .app(is_file)
        .ok_or_else(|| format!("{} 화면을 열 앱({APP_EXE})을 찾을 수 없습니다.", label(id)))?;
    let (program, mut args, cwd) = match location {
        AppLocation::Installed(exe) => {
            let cwd = exe.parent().map(Path::to_path_buf);
            (exe, Vec::new(), cwd)
        }
        AppLocation::Source { python, root } => {
            (python, vec![OsString::from(root.join(APP_SCRIPT))], Some(root))
        }
    };
    args.push("--screen".into());
    args.push(id.to_string().into());
    Ok(Spawn { program, args, envs: Vec::new(), cwd })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MenuState {
    #[default]
    Closed,
    Launching { since_ms: u64 },
    Open,
}

impl MenuState {
    pub fn name(self) -> &'static str {
        match self {
            MenuState::Closed => "closed",
            MenuState::Launching { .. } => "launching",
            MenuState::Open => "open",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    Spawn,
    /// 실행 중: 한 번 더 눌러도 새로 띄우지 않는다.
    Wait,
    Activate,
}

pub fn on_click(state: MenuState, window_found: bool) -> Click {
    match (window_found, state) {
        (true, _) => Click::Activate,
        (false, MenuState::Launching { .. }) => Click::Wait,
        (false, _) => Click::Spawn,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Exited(i32),
    Timeout,
}

/// 주기 점검. `exit` 는 이 사이드바가 띄운 프로세스가 끝났으면 그 종료 코드.
pub fn observe(state: MenuState, window_found: bool, exit: Option<i32>, now_ms: u64) -> (MenuState, Option<Failure>) {
    if window_found {
        return (MenuState::Open, None);
    }
    match state {
        MenuState::Launching { since_ms } => match exit {
            // 정상 종료(0)는 이미 떠 있는 쪽에 넘긴 경우라 조용히 닫는다.
            Some(0) => (MenuState::Closed, None),
            Some(code) => (MenuState::Closed, Some(Failure::Exited(code))),
            None if now_ms.saturating_sub(since_ms) >= LAUNCH_TIMEOUT_MS => {
                (MenuState::Closed, Some(Failure::Timeout))
            }
            None => (state, None),
        },
        _ => (MenuState::Closed, None),
    }
}

/// 자동 숨김 판단에 쓰는 지금 상황.
#[derive(Clone, Copy, Debug, Default)]
pub struct HideInput {
    pub auto_hide: bool,
    /// 전면 창이 모니터 전체를 덮는다(전체 화면).
    pub fullscreen: bool,
    /// 전면 창이 사이드바 자리와 겹친다.
    pub covers: bool,
    pub pointer_over: bool,
    pub pointer_on_edge: bool,
    /// 사이드바나 그 메뉴가 전면이다.
    pub focused: bool,
}

/// 다음 보임 상태. 자동 숨김이 꺼져 있으면 전체 화면일 때만 숨긴다(Dock 과 같은 규칙).
pub fn next_visible(visible: bool, i: HideInput) -> bool {
    if i.focused || (visible && i.pointer_over) {
        return true;
    }
    if !i.auto_hide {
        return !i.fullscreen;
    }
    if i.pointer_on_edge && !i.fullscreen {
        return true;
    }
    visible && !i.covers && !i.fullscreen
}

/// 세로 배치. 단위는 96dpi 논리 픽셀이고 화면이 낮으면 행 높이를 줄여 모두 보이게 한다.
pub const WIDTH: i32 = 164;
pub const HEADER: i32 = 36;
pub const ROW: i32 = 36;
pub const MIN_ROW: i32 = 24;
pub const GAP: i32 = 9;

/// 각 메뉴 행의 (위, 아래). `height` 는 사이드바에 쓸 수 있는 높이.
pub fn rows(entries: &[Entry], height: i32) -> Vec<(i32, i32)> {
    let breaks = entries.windows(2).filter(|w| w[0].group != w[1].group).count() as i32;
    let free = height - HEADER - breaks * GAP - 6;
    let row = (free / entries.len().max(1) as i32).clamp(MIN_ROW, ROW);
    let mut y = HEADER;
    let mut out = Vec::with_capacity(entries.len());
    for (i, e) in entries.iter().enumerate() {
        if i > 0 && entries[i - 1].group != e.group {
            y += GAP;
        }
        out.push((y, y + row));
        y += row;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> Env {
        Env {
            resources: PathBuf::from("app").join("_internal").join("resources"),
            settings: PathBuf::from("data").join("settings.json"),
            app_override: None,
            source: None,
        }
    }
    fn ids(list: &[Entry]) -> Vec<u8> {
        list.iter().map(|e| e.id).collect()
    }

    #[test]
    fn menu_follows_app_sidebar_order_and_hidden_rules() {
        assert_eq!(ids(&entries(&BTreeSet::new(), false)), [0, 1, 5, 2, 3, 4, 7, 8, 9, 12, 13, 6, 10, 11, 14, 16]);
        assert_eq!(ids(&entries(&BTreeSet::new(), true)), [0, 1, 5, 2, 3, 4, 7, 8, 9, 12, 13, 6, 10, 11, 14, 15, 16]);
        let hidden = BTreeSet::from([0, 7, 8, 9, 12, 13]);
        assert_eq!(ids(&entries(&hidden, false)), [0, 1, 5, 2, 3, 4, 12, 13, 6, 10, 11, 14, 16]);
        assert!(parse_developer_mode(r#"{"developer_mode":true}"#));
        assert!(!parse_developer_mode(r#"{"developer_mode":"true"}"#));
        assert!(!parse_developer_mode("{"));
        assert!(entries(&BTreeSet::new(), true).iter().all(|e| !label(e.id).is_empty()));
    }

    #[test]
    fn native_windows_get_the_same_arguments_as_the_app_services() {
        let e = env();
        let all = |_: &Path| true;
        let docview = spawn_for(8, &e, &all).unwrap();
        assert_eq!(docview.program, e.resources.join("docview_win.exe"));
        assert_eq!(docview.args, [OsString::from("--settings"), e.settings.clone().into(), "--klib".into(),
                                  e.resources.join("klib.exe").into()]);
        assert_eq!(spawn_for(9, &e, &all).unwrap().args, docview.args);
        assert_eq!(spawn_for(14, &e, &all).unwrap().args,
                   [OsString::from("--data-dir"), PathBuf::from("data").join("Outliner").into()]);
        assert_eq!(spawn_for(SETTINGS_ENTRY, &e, &all).unwrap().args,
                   [OsString::from("--settings"), e.settings.clone().into()]);
        assert_eq!(spawn_for(7, &e, &all).unwrap().args, ["--show"]);
        assert!(spawn_for(6, &e, &all).unwrap().args.is_empty());
        let convert = spawn_for(10, &e, &all).unwrap();
        assert!(convert.args.is_empty());
        assert_eq!(convert.envs, [("DOCFIT_APP_EXE", OsString::from(PathBuf::from("app").join(APP_EXE)))]);
    }

    #[test]
    fn menus_without_rust_window_open_the_app_screen() {
        let e = env();
        let app = PathBuf::from("app").join(APP_EXE);
        for id in [0, 1, 2, 3, 4, 5, 11, 12, 13, 15] {
            let s = spawn_for(id, &e, &|_| true).unwrap();
            assert_eq!(s.program, app, "{id}");
            assert_eq!(s.args, [OsString::from("--screen"), id.to_string().into()]);
            assert_eq!(identities(id), [Identity::Title(screen_title(id))]);
        }
    }

    #[test]
    fn missing_rust_window_falls_back_to_app_screen_when_there_is_one() {
        let e = env();
        let app = PathBuf::from("app").join(APP_EXE);
        let only_app = |p: &Path| p == app;
        assert_eq!(spawn_for(9, &e, &only_app).unwrap().args, ["--screen", "9"]);
        for id in [7, SETTINGS_ENTRY] {
            let message = spawn_for(id, &e, &only_app).unwrap_err();
            assert!(message.contains(".exe"), "{message}");
        }
        assert!(spawn_for(0, &e, &|_| false).unwrap_err().contains(APP_EXE));
        assert!(spawn_for(17, &e, &|_| true).is_err());
        assert_eq!(identities(9), [Identity::Class("PhotoViewWinMain"), Identity::Title(screen_title(9))]);
        assert_eq!(identities(7), [Identity::Class("HwpAutoDocFit.MongttangSearch")]);
    }

    #[test]
    fn override_and_source_locations() {
        let mut e = env();
        e.app_override = Some(PathBuf::from("x").join(APP_EXE));
        assert_eq!(spawn_for(0, &e, &|_| true).unwrap().program, PathBuf::from("x").join(APP_EXE));
        assert_eq!(e.app(&|_| false), None);
        e.source = Some((PathBuf::from("python"), PathBuf::from("src")));
        let spawn = spawn_for(3, &e, &|_| true).unwrap();
        assert_eq!(spawn.program, PathBuf::from("python"));
        assert_eq!(spawn.args, [OsString::from(PathBuf::from("src").join(APP_SCRIPT)), "--screen".into(), "3".into()]);
        assert!(spawn_for(10, &e, &|_| true).unwrap().envs.is_empty());
    }

    #[test]
    fn menu_window_state_machine() {
        let launching = MenuState::Launching { since_ms: 1000 };
        assert_eq!(on_click(MenuState::Closed, false), Click::Spawn);
        assert_eq!(on_click(launching, false), Click::Wait);
        assert_eq!(on_click(MenuState::Open, true), Click::Activate);
        assert_eq!(on_click(MenuState::Open, false), Click::Spawn, "닫힌 뒤 점검 전에 눌러도 다시 연다");
        assert_eq!(on_click(MenuState::Closed, true), Click::Activate, "밖에서 연 창도 앞으로");
        assert_eq!(observe(launching, false, None, 2000), (launching, None));
        assert_eq!(observe(launching, true, None, 2000), (MenuState::Open, None));
        assert_eq!(observe(launching, true, Some(0), 2000), (MenuState::Open, None));
        assert_eq!(observe(launching, false, Some(0), 2000), (MenuState::Closed, None));
        assert_eq!(observe(launching, false, Some(2), 2000), (MenuState::Closed, Some(Failure::Exited(2))));
        assert_eq!(observe(launching, false, None, 1000 + LAUNCH_TIMEOUT_MS),
                   (MenuState::Closed, Some(Failure::Timeout)));
        assert_eq!(observe(MenuState::Open, false, Some(1), 9), (MenuState::Closed, None), "닫은 창은 실패가 아니다");
        assert_eq!(observe(MenuState::Closed, false, None, 9), (MenuState::Closed, None));
    }

    #[test]
    fn auto_hide_decisions() {
        let base = HideInput::default();
        assert!(next_visible(true, HideInput { covers: true, ..base }), "꺼져 있으면 계속 떠 있다");
        assert!(!next_visible(true, HideInput { fullscreen: true, covers: true, ..base }));
        let auto = HideInput { auto_hide: true, ..base };
        assert!(next_visible(true, auto));
        assert!(!next_visible(true, HideInput { covers: true, ..auto }));
        assert!(!next_visible(false, auto), "가장자리에 대기 전에는 숨은 채");
        assert!(next_visible(false, HideInput { pointer_on_edge: true, covers: true, ..auto }));
        assert!(next_visible(true, HideInput { pointer_over: true, covers: true, ..auto }));
        assert!(!next_visible(false, HideInput { pointer_over: true, covers: true, ..auto }),
                "숨은 자리 위에 포인터가 있는 것만으로는 다시 뜨지 않는다");
        assert!(next_visible(false, HideInput { focused: true, covers: true, ..auto }));
    }

    #[test]
    fn rows_shrink_to_fit_low_screens() {
        let all = entries(&BTreeSet::new(), true);
        let tall = rows(&all, 1040);
        assert_eq!(tall[0], (HEADER, HEADER + ROW));
        assert!(tall.windows(2).all(|w| w[0].1 <= w[1].0));
        let low = rows(&all, 640);
        assert!(low.last().unwrap().1 <= 640, "{:?}", low.last());
        assert!(low[0].1 - low[0].0 < ROW);
    }
}
