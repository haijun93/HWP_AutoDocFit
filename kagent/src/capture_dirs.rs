//! 화면캡처 폴더 자동 인식과 폴더별 감시 상태(사용자 요청 2026-11-13).
//!
//! 나눔(시험하기 쉽게):
//! - `candidates(env)`: 알려진 폴더·환경 변수만 보고 후보 목록을 만든다(순수 함수, 디스크 안 봄).
//! - `discover(env, exists)`: 실제로 있는 폴더만 남긴다(같은 폴더는 한 번만).
//! - `folder_state(...)`: 폴더 하나의 감시 상태를 정한다(명시적 상태 기계, 아래 표).
//! - `plan(...)`: 설정 + 찾은 폴더 → 폴더별 상태 목록(설정 화면·status.json·감시 시작이 같은 값을 쓴다).
//! - `Env::from_system()`: Windows 알려진 폴더(SHGetKnownFolderPath)와 OneDrive 환경 변수를 읽는다.
//!
//! 감시 상태(폴더 하나):
//! | 상태 | 뜻 | 들어가는 조건 |
//! |---|---|---|
//! | `Watching` | 새 그림 파일을 기록 | 전체 켬 · 기록 중 · 사용자가 이 폴더를 끄지 않음 |
//! | `UserOff` | 사용자가 이 폴더만 끔 | `capture_dirs_off`에 경로가 있음 |
//! | `AllOff` | 화면캡처 폴더 감시 전체 끔 | `watch_screenshots == false` |
//! | `Paused` | 기록 일시정지 중(핸들 해제) | `paused == true` |
//! 우선순위: AllOff > Paused > UserOff > Watching. 폴더가 사라지면 후보에서 빠진다(상태 없음).
//! 상태는 설정·일시정지가 바뀔 때마다 `plan`으로 처음부터 다시 계산한다(숨은 전이 없음).

/// 후보 폴더 하나.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub path: String,
    /// 화면에 보일 출처 이름.
    pub label: &'static str,
    /// 하위 폴더까지 감시(ShareX처럼 월별 폴더에 저장하는 앱).
    pub subtree: bool,
}

/// 후보를 만들 때 쓰는 시스템 값(시험에서는 직접 채운다).
#[derive(Clone, Debug, Default)]
pub struct Env {
    /// FOLDERID_Screenshots — 캡처 도구·Win+PrtSc 저장 위치. OneDrive가 옮겼으면 옮긴 경로가 온다.
    pub known_screenshots: Option<String>,
    pub pictures: Option<String>,
    pub documents: Option<String>,
    pub videos: Option<String>,
    /// OneDrive 뿌리(개인·회사, 환경 변수 OneDrive·OneDriveConsumer·OneDriveCommercial).
    pub onedrive: Vec<String>,
}

pub const LABEL_WINDOWS: &str = "Windows 스크린샷(캡처 도구·Win+PrtSc)";
pub const LABEL_ONEDRIVE: &str = "OneDrive 스크린샷";
pub const LABEL_GAMEBAR: &str = "Xbox Game Bar 캡처";
pub const LABEL_ALCAPTURE: &str = "알캡처";
pub const LABEL_HANCOM: &str = "한컴 캡처";
pub const LABEL_SHAREX: &str = "ShareX";
pub const LABEL_PICPICK: &str = "PicPick";

fn join(root: &str, rel: &str) -> String {
    format!("{}\\{}", root.trim_end_matches(['\\', '/']), rel)
}

/// 같은 폴더 판정용 키(대소문자·구분자·끝 구분자 무시).
pub fn key(path: &str) -> String {
    path.replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// 알려진 위치에서 후보 목록을 만든다(우선순위 순, 중복 제거 전).
pub fn candidates(env: &Env) -> Vec<Candidate> {
    let mut out = Vec::new();
    let mut push = |path: String, label: &'static str, subtree: bool| out.push(Candidate { path, label, subtree });
    if let Some(p) = &env.known_screenshots {
        push(p.clone(), LABEL_WINDOWS, false);
    }
    if let Some(pic) = &env.pictures {
        // 알려진 폴더를 못 읽은 PC(오래된 Windows·정책)와 한국어 이름 폴더.
        push(join(pic, "Screenshots"), LABEL_WINDOWS, false);
        push(join(pic, "스크린샷"), LABEL_WINDOWS, false);
    }
    for od in &env.onedrive {
        for rel in ["Pictures\\Screenshots", "Pictures\\스크린샷", "사진\\스크린샷", "사진\\Screenshots", "그림\\스크린샷"] {
            push(join(od, rel), LABEL_ONEDRIVE, false);
        }
    }
    if let Some(v) = &env.videos {
        // Win+Alt+PrtSc(게임 바)는 동영상\캡처에 PNG를 저장한다(동영상 파일은 확장자로 거른다).
        push(join(v, "Captures"), LABEL_GAMEBAR, false);
        push(join(v, "캡처"), LABEL_GAMEBAR, false);
    }
    for root in [&env.pictures, &env.documents].into_iter().flatten() {
        // 기본 폴더가 판마다 달라 흔한 이름을 모두 본다(실제로 있는 폴더만 감시).
        push(join(root, "ALCapture"), LABEL_ALCAPTURE, false);
        push(join(root, "알캡처"), LABEL_ALCAPTURE, false);
        push(join(root, "한컴 캡처"), LABEL_HANCOM, false);
        push(join(root, "HancomCapture"), LABEL_HANCOM, false);
        push(join(root, "PicPick"), LABEL_PICPICK, false);
    }
    if let Some(doc) = &env.documents {
        push(join(doc, "ShareX\\Screenshots"), LABEL_SHAREX, true);
    }
    out
}

/// 실제로 있는 후보만, 같은 폴더는 처음 것 하나만 남긴다.
pub fn discover(env: &Env, exists: &dyn Fn(&str) -> bool) -> Vec<Candidate> {
    let mut seen = std::collections::HashSet::new();
    candidates(env).into_iter().filter(|c| exists(&c.path) && seen.insert(key(&c.path))).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderState {
    Watching,
    UserOff,
    AllOff,
    Paused,
}

impl FolderState {
    pub fn code(self) -> &'static str {
        match self {
            FolderState::Watching => "watching",
            FolderState::UserOff => "off",
            FolderState::AllOff => "all_off",
            FolderState::Paused => "paused",
        }
    }
}

pub fn folder_state(all_on: bool, paused: bool, user_off: bool) -> FolderState {
    if !all_on {
        FolderState::AllOff
    } else if paused {
        FolderState::Paused
    } else if user_off {
        FolderState::UserOff
    } else {
        FolderState::Watching
    }
}

/// 설정과 찾은 폴더로 폴더별 상태를 정한다. `off`는 소문자 경로 목록(설정 `capture_dirs_off`).
pub fn plan(found: &[Candidate], all_on: bool, paused: bool, off: &[String]) -> Vec<(Candidate, FolderState)> {
    found
        .iter()
        .map(|c| {
            let user_off = off.iter().any(|o| key(o) == key(&c.path));
            (c.clone(), folder_state(all_on, paused, user_off))
        })
        .collect()
}

/// 기록할 그림 파일인지(확장자, 쓰는 중 임시 이름 제외).
pub fn is_capture_image(path: &str) -> bool {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    if name.starts_with('~') || name.starts_with('.') {
        return false;
    }
    let Some(dot) = name.rfind('.') else { return false };
    matches!(name[dot + 1..].to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp")
}

/// 같은 캡처가 클립보드 그림으로 방금(3초 안) 기록됐으면 폴더 파일은 한 번 더 기록하지 않는다.
pub const CLIPBOARD_TWIN_MS: u64 = 3000;

pub fn is_clipboard_twin(now_ms: u64, last_clip_image_ms: u64) -> bool {
    last_clip_image_ms != 0 && now_ms.saturating_sub(last_clip_image_ms) <= CLIPBOARD_TWIN_MS
}

/// 파일이 다 써졌는지: 크기가 0보다 크고 직전 확인과 같으며, PNG면 끝 표시(IEND)까지 있다.
pub fn looks_complete(prev_len: Option<u64>, len: u64, png_tail: Option<&[u8]>) -> bool {
    if len == 0 || prev_len != Some(len) {
        return false;
    }
    match png_tail {
        Some(tail) => tail.ends_with(b"IEND\xAE\x42\x60\x82"),
        None => true,
    }
}

impl Env {
    #[cfg(windows)]
    pub fn from_system() -> Env {
        use windows_sys::Win32::UI::Shell::{FOLDERID_Documents, FOLDERID_Pictures, FOLDERID_Screenshots, FOLDERID_Videos};
        let mut onedrive: Vec<String> = Vec::new();
        for var in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
            if let Ok(p) = std::env::var(var) {
                if !p.is_empty() && !onedrive.iter().any(|o| key(o) == key(&p)) {
                    onedrive.push(p);
                }
            }
        }
        Env {
            known_screenshots: known_folder(&FOLDERID_Screenshots),
            pictures: known_folder(&FOLDERID_Pictures),
            documents: known_folder(&FOLDERID_Documents),
            videos: known_folder(&FOLDERID_Videos),
            onedrive,
        }
    }
}

#[cfg(windows)]
fn known_folder(id: &windows_sys::core::GUID) -> Option<String> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::SHGetKnownFolderPath;
    unsafe {
        let mut p: windows_sys::core::PWSTR = std::ptr::null_mut();
        let hr = SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut p);
        let out = if hr == 0 && !p.is_null() {
            let mut n = 0;
            while *p.add(n) != 0 {
                n += 1;
            }
            Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)))
        } else {
            None
        };
        if !p.is_null() {
            CoTaskMemFree(p as _);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> Env {
        Env {
            known_screenshots: Some("D:\\u\\OneDrive\\사진\\스크린샷".into()),
            pictures: Some("D:\\u\\Pictures".into()),
            documents: Some("D:\\u\\Documents".into()),
            videos: Some("D:\\u\\Videos".into()),
            onedrive: vec!["D:\\u\\OneDrive".into()],
        }
    }

    #[test]
    fn known_folder_comes_first_and_onedrive_duplicate_is_merged() {
        let all = candidates(&env());
        assert_eq!(all[0].path, "D:\\u\\OneDrive\\사진\\스크린샷");
        assert_eq!(all[0].label, LABEL_WINDOWS);
        // OneDrive 후보에도 같은 폴더가 있지만 실제 목록에는 한 번만(알려진 폴더 이름으로) 나온다.
        let exists = |p: &str| p.eq_ignore_ascii_case("d:\\u\\onedrive\\사진\\스크린샷") || p == "D:\\u\\Pictures\\Screenshots";
        let found = discover(&env(), &exists);
        assert_eq!(found.iter().map(|c| c.path.as_str()).collect::<Vec<_>>(),
                   ["D:\\u\\OneDrive\\사진\\스크린샷", "D:\\u\\Pictures\\Screenshots"]);
        assert_eq!(found[0].label, LABEL_WINDOWS);
    }

    #[test]
    fn only_existing_app_folders_are_found_with_their_labels() {
        let exists = |p: &str| ["D:\\u\\Documents\\ALCapture", "D:\\u\\Documents\\ShareX\\Screenshots",
            "D:\\u\\Videos\\Captures"].contains(&p);
        let found = discover(&env(), &exists);
        let got: Vec<_> = found.iter().map(|c| (c.label, c.subtree)).collect();
        assert_eq!(got, [(LABEL_GAMEBAR, false), (LABEL_ALCAPTURE, false), (LABEL_SHAREX, true)]);
    }

    #[test]
    fn no_known_folders_means_no_candidates() {
        assert!(candidates(&Env::default()).is_empty());
        assert!(discover(&Env::default(), &|_| true).is_empty());
    }

    #[test]
    fn key_ignores_case_separators_and_trailing_slash() {
        assert_eq!(key("C:/Users/A/Pictures/Screenshots/"), key("c:\\users\\a\\pictures\\screenshots"));
    }

    #[test]
    fn folder_state_priority_table() {
        use FolderState::*;
        // (전체 켬, 일시정지, 사용자 끔) → 상태
        let table = [
            ((true, false, false), Watching),
            ((true, false, true), UserOff),
            ((true, true, false), Paused),
            ((true, true, true), Paused),
            ((false, false, false), AllOff),
            ((false, true, true), AllOff),
        ];
        for ((all, paused, off), want) in table {
            assert_eq!(folder_state(all, paused, off), want, "{all} {paused} {off}");
        }
    }

    #[test]
    fn plan_applies_user_off_by_path_key() {
        let found = vec![
            Candidate { path: "C:\\A\\Screenshots".into(), label: LABEL_WINDOWS, subtree: false },
            Candidate { path: "C:\\A\\ALCapture".into(), label: LABEL_ALCAPTURE, subtree: false },
        ];
        let p = plan(&found, true, false, &["c:\\a\\alcapture\\".into()]);
        assert_eq!(p.iter().map(|(_, s)| *s).collect::<Vec<_>>(), [FolderState::Watching, FolderState::UserOff]);
        let p = plan(&found, true, true, &[]);
        assert!(p.iter().all(|(_, s)| *s == FolderState::Paused));
    }

    #[test]
    fn capture_image_extensions_and_temp_names() {
        assert!(is_capture_image("C:\\s\\스크린샷 2026-11-13 101010.png"));
        assert!(is_capture_image("C:\\s\\a.JPG"));
        assert!(is_capture_image("x.webp"));
        assert!(!is_capture_image("C:\\s\\clip.mp4"));
        assert!(!is_capture_image("C:\\s\\~tmp123.png"));
        assert!(!is_capture_image("C:\\s\\noext"));
    }

    #[test]
    fn clipboard_twin_window() {
        assert!(!is_clipboard_twin(10_000, 0));
        assert!(is_clipboard_twin(10_000, 8_000));
        assert!(!is_clipboard_twin(10_000, 6_999));
    }

    #[test]
    fn complete_needs_stable_size_and_png_end() {
        assert!(!looks_complete(None, 10, None));
        assert!(!looks_complete(Some(9), 10, None));
        assert!(looks_complete(Some(10), 10, None));
        assert!(!looks_complete(Some(10), 0, None));
        assert!(!looks_complete(Some(10), 10, Some(b"xxxxIDAT")));
        assert!(looks_complete(Some(10), 10, Some(b"\0\0\0\0IEND\xAE\x42\x60\x82")));
    }
}

#[cfg(test)]
mod system_probe {
    /// 이 PC에서 실제로 찾은 폴더를 출력만 한다(읽기 전용, 기본 실행에서 빠짐): `cargo test -- --ignored --nocapture probe`.
    #[test]
    #[ignore]
    fn probe_this_pc() {
        let env = super::Env::from_system();
        for c in super::discover(&env, &|p| std::path::Path::new(p).is_dir()) {
            println!("FOUND {} | {} | subtree={}", c.label, c.path, c.subtree);
        }
        println!("known_screenshots={:?} onedrive={}", env.known_screenshots.is_some(), env.onedrive.len());
    }
}
