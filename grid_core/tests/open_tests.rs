//! DBLOPEN1: 연결 앱 열기 검사와 클릭·Enter 대상. 가짜 opener 만 쓴다(실제 앱 실행 없음).
use grid_core::app::App;
use grid_core::items::Doc;
use grid_core::layout::{GridSpec, Metrics};
use grid_core::open::{check_openable, open_with};
use std::path::PathBuf;

const EXTS: [&str; 2] = ["jpg", "png"];

fn file(name: &str) -> String {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("dblopen1");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, b"synthetic").unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn listed_file_reaches_opener_with_same_path() {
    for name in ["합성 사진.jpg", "UPPER.PNG"] {
        let path = file(name);
        let mut got = Vec::new();
        open_with(&path, &EXTS, |p| {
            got.push(p.to_string());
            Ok(())
        })
        .unwrap();
        assert_eq!(got, [path]);
    }
}

#[test]
fn refused_paths_never_reach_opener() {
    let mut paths: Vec<String> = ["a.exe", "a.bat", "a.lnk", "a.jpg.exe", "noext"].iter().map(|n| file(n)).collect();
    paths.push("relative.jpg".into());
    paths.push(String::new());
    paths.push(format!("{}\0.jpg", file("c.exe")));
    if cfg!(windows) {
        paths.push(format!("{}:hidden.jpg", file("host.exe")));
    }
    for path in paths {
        let r = open_with(&path, &EXTS, |p| panic!("열면 안 되는 경로: {p}"));
        assert!(r.is_err(), "{path:?}");
    }
}

#[test]
fn missing_file_and_opener_error_are_reported() {
    let missing = file("d.jpg").replace("d.jpg", "none.jpg");
    assert_eq!(check_openable(&missing, &EXTS), Err("파일을 찾을 수 없습니다".into()));
    assert_eq!(open_with(&file("e.jpg"), &EXTS, |_| Err("실패".into())), Err("실패".into()));
}

fn app(n: usize) -> App {
    let mut app = App::new(190.0);
    app.coll.replace(
        (0..n)
            .map(|i| Doc { path: format!("C:/s/{i:03}.jpg"), name: format!("{i:03}.jpg"), ext: "jpg".into(), size: 1, mtime: 0.0, cloud: false })
            .collect(),
    );
    app
}

#[test]
fn double_click_target_is_the_pressed_item_even_after_scroll() {
    let mut app = app(100);
    let m = Metrics::with_spec(GridSpec::DOC, 900, 600, 96, app.thumb, false);
    // 아래쪽에 반쯤 걸친 항목을 누르면 첫 클릭이 화면을 스크롤한다.
    let n = (0..100).find(|i| m.item_rect(*i, 0).bottom > m.viewport.bottom).unwrap();
    app.click(&m, Some(n));
    assert!(app.scroll > 0);
    assert_eq!(app.clicked.as_deref(), Some(format!("C:/s/{n:03}.jpg").as_str()));
    // 빈 곳을 누르면 대상이 없다.
    app.click(&m, None);
    assert_eq!(app.clicked, None);
}

#[test]
fn enter_target_is_selection_outside_search() {
    let mut app = app(3);
    let m = app.metrics(900, 600, 96);
    assert_eq!(app.enter_target(), None);
    app.select(&m, Some(1));
    assert_eq!(app.enter_target().as_deref(), Some("C:/s/001.jpg"));
    app.search_focus = true;
    assert_eq!(app.enter_target(), None);
}

#[test]
fn selecting_clears_open_error() {
    let mut app = app(3);
    let m = app.metrics(900, 600, 96);
    app.open_error = "파일을 찾을 수 없습니다".into();
    app.select(&m, Some(2));
    assert!(app.open_error.is_empty());
}
