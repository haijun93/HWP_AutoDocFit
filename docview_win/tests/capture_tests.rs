//! 캡처 모드: 실제 paint 를 메모리 DC 에 그려 PNG 를 만든다(Windows 전용). 실행 파일은 `CARGO_BIN_EXE_docview_win` 기준.
#![cfg(windows)]
use docview_win::capture::{self, Size};
use docview_win::layout::scale;
use docview_win::synth::{SCENES, scene};
use std::path::PathBuf;
use std::process::Command;
use ui_theme::Kind;

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("docview_win_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// PNG 머리 검증 + 가로세로 반환.
fn png_size(path: &std::path::Path) -> (u32, u32, u64) {
    let b = std::fs::read(path).unwrap();
    assert_eq!(&b[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&b[12..16], b"IHDR");
    (u32::from_be_bytes(b[16..20].try_into().unwrap()), u32::from_be_bytes(b[20..24].try_into().unwrap()), b.len() as u64)
}

#[test]
fn every_scene_renders_a_non_blank_picture() {
    for name in SCENES {
        for (kind, dpi) in [(Kind::Light, 96), (Kind::Dark, 144)] {
            let (w, h) = (scale(1100, dpi), scale(720, dpi));
            let app = scene(name, w, h, dpi, 190.0).unwrap();
            let shot = capture::render(&app, &capture::palette_of(kind), w, h, dpi).unwrap();
            assert_eq!((shot.width as i32, shot.height as i32), (w, h));
            let first = &shot.bgra[..4];
            let distinct = shot.bgra.chunks_exact(4).filter(|p| *p != first).count();
            assert!(distinct > shot.bgra.len() / 4 / 50, "{name} {kind:?}: 거의 빈 화면(다른 화소 {distinct})");
            // 가상화: 항목이 수만이어도 실제 그린 칸은 한 화면 분량.
            assert!(shot.drawn <= 40, "{name}: {}칸 그림", shot.drawn);
            if app.coll.len() > 0 {
                assert!(shot.drawn >= 6, "{name}: 칸이 거의 안 그려짐({})", shot.drawn);
            }
        }
    }
}

#[test]
fn light_and_dark_differ() {
    let (w, h) = (1100, 720);
    let app = scene("list", w, h, 96, 190.0).unwrap();
    let a = capture::render(&app, &capture::palette_of(Kind::Light), w, h, 96).unwrap();
    let b = capture::render(&app, &capture::palette_of(Kind::Dark), w, h, 96).unwrap();
    assert_ne!(a.bgra, b.bgra);
}

#[test]
fn selection_and_scan_state_change_the_picture() {
    let (w, h) = (1100, 720);
    let pal = capture::palette_of(Kind::Light);
    let mut app = scene("list", w, h, 96, 190.0).unwrap();
    let a = capture::render(&app, &pal, w, h, 96).unwrap().bgra;
    app.selected = Some(0);
    let b = capture::render(&app, &pal, w, h, 96).unwrap().bgra;
    assert_ne!(a, b, "선택 표시가 그려져야 한다");
    app.scan.state = docview_win::model::ScanState::Running;
    let c = capture::render(&app, &pal, w, h, 96).unwrap().bgra;
    assert_ne!(b, c, "스캔 띠가 그려져야 한다");
    app.thumb = 100.0;
    let d = capture::render(&app, &pal, w, h, 96).unwrap();
    assert!(d.drawn > 18, "썸네일을 줄이면 한 화면에 더 많은 칸이 그려진다({})", d.drawn);
}

#[test]
fn capture_binary_writes_pngs() {
    let dir = temp("cap");
    let out = dir.join("out.txt");
    let exe = env!("CARGO_BIN_EXE_docview_win");
    let st = Command::new(exe)
        .args(["--capture-dir", dir.to_str().unwrap(), "--dark", "--dpi", "144", "--size", "900x600", "--scene", "virtual"])
        .env("DOCVIEW_WIN_OUT", &out)
        .status()
        .unwrap();
    assert!(st.success());
    let line = std::fs::read_to_string(&out).unwrap();
    assert!(line.starts_with("ok "), "{line}");
    let png = dir.join("docview_virtual_dark_144dpi.png");
    let (w, h, bytes) = png_size(&png);
    assert_eq!((w, h), (1350, 900));
    assert!(bytes > 50_000, "PNG 가 너무 작다: {bytes}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn capture_set_writes_all_scenes_and_rejects_bad_args() {
    let dir = temp("set");
    let out = dir.join("out.txt");
    let exe = env!("CARGO_BIN_EXE_docview_win");
    let st = Command::new(exe).args(["--capture-dir", dir.to_str().unwrap(), "--capture-set"]).env("DOCVIEW_WIN_OUT", &out).status().unwrap();
    assert!(st.success());
    let n = std::fs::read_dir(&dir).unwrap().filter(|e| e.as_ref().unwrap().path().extension().is_some_and(|x| x == "png")).count();
    assert_eq!(n, SCENES.len() + 6);
    let _ = std::fs::remove_dir_all(&dir);
    // 알 수 없는 장면·크기는 실패(오류 대화상자 없이 OUT 파일로만 확인: GUI 하위 시스템이라 MessageBox 가 뜨지 않게 제외하고 크기 해석만 시험).
    assert!(capture::parse_size("abc").is_err());
    assert!(capture::parse_size("100x100").is_err());
    let Size(w, h) = capture::parse_size("1280X800").unwrap();
    assert_eq!((w, h), (1280, 800));
    assert!(scene("없는장면", 1100, 720, 96, 190.0).is_err());
}
