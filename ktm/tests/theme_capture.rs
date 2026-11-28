//! QMSDARK1: 실제 `ktm.exe`를 화면 캡처 모드로 실행해, 설정 조합별로 창에 칠해진 색을 확인한다.
//! 캡처 모드는 단축키·트레이·단일 인스턴스·검색 스레드를 만들지 않는다. 설정 파일은 모두 임시 폴더의
//! 합성 자료이고(`--app-settings`), 설치본·사용자 설정은 읽지도 쓰지도 않는다.
#![cfg(windows)]
use std::path::{Path, PathBuf};
use ui_theme::Palette;

const YELLOW_MARK: u32 = 0x008A_F0FE;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qmsdark1-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".ktm-capture-data")).unwrap();
    dir
}

/// 캡처 모드가 쓰는 압축 없는 RGB PNG를 읽어 COLORREF(BGR) 화소로 돌려준다.
fn pixels(path: &Path) -> (usize, Vec<u32>) {
    let png = std::fs::read(path).unwrap();
    let be = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
    let (width, height) = (be(16), be(20));
    let mut at = 8;
    while &png[at + 4..at + 8] != b"IDAT" {
        at += 12 + be(at);
    }
    let z = &png[at + 8..at + 8 + be(at)];
    let (mut raw, mut i) = (Vec::new(), 2);
    loop {
        let last = z[i] == 1;
        let len = u16::from_le_bytes([z[i + 1], z[i + 2]]) as usize;
        raw.extend_from_slice(&z[i + 5..i + 5 + len]);
        i += 5 + len;
        if last {
            break;
        }
    }
    assert_eq!(raw.len(), height * (width * 3 + 1));
    let out = raw
        .chunks(width * 3 + 1)
        .flat_map(|row| row[1..].chunks(3).map(|c| ui_theme::rgb(c[0], c[1], c[2])))
        .collect();
    (width, out)
}

struct Shot {
    dir: PathBuf,
    width: usize,
    pixels: Vec<u32>,
}

impl Shot {
    fn at(&self, x: usize, y: usize) -> u32 {
        self.pixels[y * self.width + x]
    }
    fn count(&self, color: u32) -> usize {
        self.pixels.iter().filter(|p| **p == color).count()
    }
    fn report(&self) -> String {
        std::fs::read_to_string(self.dir.join("capture-report_96dpi.txt")).unwrap()
    }
}

impl Drop for Shot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn capture(name: &str, quick: &str, app: Option<&str>) -> Shot {
    let dir = temp(name);
    std::fs::write(dir.join(".ktm-capture-data/settings.json"), quick).unwrap();
    let app_path = dir.join("app-settings.json");
    if let Some(app) = app {
        std::fs::write(&app_path, app).unwrap();
    }
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_ktm"))
        .env_remove("KTM_TEST_ROOT")
        .env("KTM_ERROR_FILE", dir.join("error.txt"))
        .arg("--capture-dir")
        .arg(&dir)
        .args(["--scenario", "03_results", "--app-settings"])
        .arg(&app_path)
        .status()
        .unwrap();
    assert!(status.success(), "{name}: {}", std::fs::read_to_string(dir.join("error.txt")).unwrap_or_default());
    let (width, pixels) = pixels(&dir.join("03_results_96dpi.png"));
    Shot { dir, width, pixels }
}

/// 창 바탕(목록 오른쪽 빈 곳)과 검색 입력칸 안쪽이 모두 기대한 팔레트인지.
fn assert_look(shot: &Shot, dark: bool, why: &str) {
    let want = if dark { Palette::DARK } else { Palette::LIGHT };
    assert_eq!(shot.at(shot.width / 2, 390), want.bg, "{why}: 목록 바탕");
    assert_eq!(shot.at(shot.width - 40, 54), want.bg, "{why}: 검색 입력칸 바탕");
    assert_eq!(shot.at(200, 3), want.head_bg, "{why}: 머리글 바탕");
}

#[test]
fn saved_choices_decide_the_window_colors() {
    if ui_theme::high_contrast_on() {
        return; // 고대비 PC에서는 시스템 색만 쓴다(ktm::theme 단위 시험).
    }
    let system = ui_theme::system_dark();
    let cases: [(&str, &str, Option<&str>, bool); 8] = [
        // 설정 창에서 '어둡게'를 골랐는데 옛 Ctrl+L 값 "light"가 남아 있음(보고된 결함).
        ("quick-dark", r#"{"theme":"light","ui_theme":"dark"}"#, None, true),
        ("quick-light", r#"{"theme":"dark","ui_theme":"light"}"#, Some(r#"{"theme_mode":"dark"}"#), false),
        // 앱 화면 테마를 따른다.
        ("app-dark", r#"{"theme":"light","ui_theme":"system"}"#, Some(r#"{"theme_mode":"dark"}"#), true),
        ("app-light", r#"{"theme":"dark","ui_theme":"system"}"#, Some(r#"{"theme_mode":"light"}"#), false),
        // 아무도 고르지 않았으면 Windows 앱 모드.
        ("windows", r#"{"theme":"dark","ui_theme":"system"}"#, Some(r#"{"theme_mode":"system"}"#), system),
        ("no-files", "{}", None, system),
        // ui_theme가 생기기 전 파일의 "light"는 옮겨 온다.
        ("legacy-light", r#"{"theme":"light"}"#, Some(r#"{"theme_mode":"dark"}"#), false),
        // 앱 설정이 BOM으로 저장돼 있어도 읽는다.
        ("app-bom", "{}", Some("\u{feff}{\"theme_mode\":\"dark\"}"), true),
    ];
    for (name, quick, app, dark) in cases {
        assert_look(&capture(name, quick, app), dark, name);
    }
}

#[test]
fn dark_window_has_no_bright_fixed_color_and_changes_apply() {
    if ui_theme::high_contrast_on() {
        return;
    }
    let shot = capture("dark-pixels", r#"{"ui_theme":"dark"}"#, None);
    assert_eq!(shot.count(YELLOW_MARK), 0, "검색어 강조가 밝게 테마의 고정 노랑을 쓴다");
    assert_eq!(shot.count(0x00FF_FFFF) * 50 / shot.pixels.len(), 0, "흰 면이 남아 있다");
    // 실행 중 테마 알림·설정 창 클릭·Ctrl+L 이 바로 반영되는지는 창 안에서 확인해 보고서에 적는다.
    let report = shot.report();
    for name in ["PASS 테마 알림 반영", "PASS 설정 창 테마 즉시 반영", "PASS Ctrl+L 테마 저장"] {
        assert!(report.contains(name), "{name} 없음:\n{report}");
    }
}
