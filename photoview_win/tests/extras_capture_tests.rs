//! 둘째 도구줄·즐겨찾기/태그 표시·크게 보기의 그림(합성 목록·합성 그림만). `PHOTOVIEW_SHOTS=<폴더>` 면 PNG 도 쓴다.
#![cfg(windows)]
use photoview_win::capture::palette_of;
use photoview_win::extras::{Photo, Viewer};
use photoview_win::library::{Mark, key};
use photoview_win::pixels::Image;
use photoview_win::synth::scene;
use std::collections::HashMap;
use std::sync::Arc;
use ui_theme::Kind;

fn shot(name: &str, app: &photoview_win::model::App, photo: &Photo, w: i32, h: i32) -> (Vec<u8>, usize) {
    let pal = palette_of(Kind::Light);
    let shot = grid_core::gdi::render(w, h, |hdc| {
        photoview_win::paint::draw_all(hdc, w, h, app, photo, None, &pal, 96, &HashMap::new())
    })
    .unwrap();
    if let Some(dir) = std::env::var_os("PHOTOVIEW_SHOTS") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        photoview_win::png::write(&dir.join(format!("{name}.png")), shot.width, shot.height, &shot.bgra).unwrap();
    }
    (shot.bgra, shot.drawn)
}

#[test]
fn marks_filters_and_viewer_change_the_picture() {
    let (w, h) = (1100, 720);
    let mut app = scene("list", w, h, 96, 160.0).unwrap();
    app.scroll = 0;
    let mut photo = Photo::default();
    let (plain, _) = shot("plain", &app, &photo, w, h);

    let first = app.coll.get(0).unwrap().path.clone();
    photo.marks.insert(key(&first), Mark { favorite: true, tags: vec!["현장".into(), "준공".into()] });
    let (marked, drawn) = shot("marked", &app, &photo, w, h);
    assert_ne!(plain, marked, "별·태그가 보인다");
    assert!(drawn >= 6);
    shot("marked_min_width", &app, &photo, 820, 480);

    photo.favorites_only = true;
    app.coll.set_only(photo.only(&app.coll.docs));
    assert_eq!(app.coll.len(), 1);
    let (filtered, drawn) = shot("favorites_only", &app, &photo, w, h);
    assert_eq!(drawn, 1);
    assert_ne!(marked, filtered);

    photo.favorites_only = false;
    app.coll.set_only(None);
    let items: Vec<_> = (0..3).map(|i| app.coll.get(i).unwrap().clone()).collect();
    photo.viewer = Viewer::open(items, 0);
    let (loading, drawn) = shot("viewer_loading", &app, &photo, w, h);
    assert_eq!(drawn, 0, "크게 보기 중에는 격자를 그리지 않는다");
    let bgra = (0..600u32 * 400).flat_map(|i| [(i % 600 / 3) as u8, (i / 600 / 2) as u8, 180, 255]).collect();
    photo.viewer.as_mut().unwrap().image = Some(Ok(Arc::new(Image::new(600, 400, bgra).unwrap())));
    let (shown, _) = shot("viewer", &app, &photo, w, h);
    assert_ne!(loading, shown);
    shot("viewer_min_width", &app, &photo, 820, 480);
}
