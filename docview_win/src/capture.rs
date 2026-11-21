//! 캡처 모드: 창을 만들지 않고 `paint::draw` 를 메모리 DC 에 그려 PNG 로 쓴다(`settings_win`·`outliner_win` 과 같은 방식).
use crate::layout::{DEFAULT_H, DEFAULT_W, scale};
use crate::model::App;
use crate::paint::draw_with_panel;
use std::path::{Path, PathBuf};
use ui_theme::{Kind, Palette};

pub use grid_core::gdi::{Shot, kind_name, palette_of};

pub fn render(app: &App, pal: &Palette, width: i32, height: i32, dpi: u32) -> Result<Shot, String> {
    render_with_panel(app, None, pal, width, height, dpi)
}

/// `render` + 제외 폴더 편집 패널(있으면).
pub fn render_with_panel(app: &App, panel: Option<&crate::panel::ExcludePanel>, pal: &Palette, width: i32, height: i32, dpi: u32) -> Result<Shot, String> {
    grid_core::gdi::render(width, height, |hdc| draw_with_panel(hdc, width, height, app, panel, pal, dpi))
}

/// 화면 크기(96 DPI 기준 논리 크기, 기본 1100x720).
#[derive(Clone, Copy)]
pub struct Size(pub i32, pub i32);

impl Default for Size {
    fn default() -> Self {
        Size(DEFAULT_W, DEFAULT_H)
    }
}

pub fn parse_size(s: &str) -> Result<Size, String> {
    let (w, h) = s.split_once(['x', 'X']).ok_or("--size 는 WxH 형식입니다")?;
    let (w, h): (i32, i32) = (w.parse().map_err(|_| "--size 가로가 숫자가 아닙니다")?, h.parse().map_err(|_| "--size 세로가 숫자가 아닙니다")?);
    if !(400..=4000).contains(&w) || !(300..=3000).contains(&h) {
        return Err("--size 범위: 400x300 ~ 4000x3000".into());
    }
    Ok(Size(w, h))
}

/// `dir` 에 `docview_<장면>_<테마>_<dpi>dpi.png` 를 쓰고 (경로, 그린 칸 수) 를 돌려준다.
pub fn capture_to(dir: &Path, tag: &str, app: &App, kind: Kind, dpi: u32, size: Size) -> Result<(PathBuf, usize), String> {
    capture_with_panel(dir, tag, app, None, kind, dpi, size)
}

/// `capture_to` + 제외 폴더 편집 패널(있으면).
pub fn capture_with_panel(dir: &Path, tag: &str, app: &App, panel: Option<&crate::panel::ExcludePanel>, kind: Kind, dpi: u32, size: Size) -> Result<(PathBuf, usize), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (w, h) = (scale(size.0, dpi), scale(size.1, dpi));
    let shot = render_with_panel(app, panel, &palette_of(kind), w, h, dpi)?;
    let path = dir.join(format!("docview_{tag}_{}_{}dpi.png", kind_name(kind), dpi));
    crate::png::write(&path, shot.width, shot.height, &shot.bgra)?;
    Ok((path, shot.drawn))
}
