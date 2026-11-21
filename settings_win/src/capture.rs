//! 캡처 모드: 창을 만들지 않고 `paint::draw` 를 메모리 DC 에 그려 PNG 로 쓴다(`outliner_win --capture-dir` 와 같은 방식).
use crate::layout::{DEFAULT_H, DEFAULT_W, scale};
use crate::model::Model;
use crate::paint::{Ui, draw};
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use ui_theme::{Kind, Palette};
use windows_sys::Win32::Graphics::Gdi::*;

pub struct Shot {
    pub width: usize,
    pub height: usize,
    /// 위에서 아래로의 BGRA.
    pub bgra: Vec<u8>,
}

pub fn render(model: &Model, ui: &Ui, pal: &Palette, width: i32, height: i32, dpi: u32) -> Result<Shot, String> {
    unsafe {
        let screen = GetDC(null_mut());
        let mem = CreateCompatibleDC(screen);
        let bm = CreateCompatibleBitmap(screen, width, height);
        let old = SelectObject(mem, bm);
        draw(mem, width, height, model, ui, pal, dpi);
        SelectObject(mem, old);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let got = GetDIBits(screen, bm, 0, height as u32, pixels.as_mut_ptr().cast(), &mut info, DIB_RGB_COLORS);
        DeleteObject(bm);
        DeleteDC(mem);
        ReleaseDC(null_mut(), screen);
        if got == 0 {
            return Err("캡처 실패(GetDIBits)".into());
        }
        Ok(Shot { width: width as usize, height: height as usize, bgra: pixels })
    }
}

pub fn palette_of(kind: Kind) -> Palette {
    Palette::of(kind, &ui_theme::sys_colors())
}

pub fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Light => "light",
        Kind::Dark => "dark",
        Kind::HighContrast => "highcontrast",
    }
}

/// `dir` 에 `settings_<이름>_<테마>_<dpi>dpi.png` 를 쓰고 경로를 돌려준다.
pub fn capture_to(dir: &Path, tag: &str, model: &Model, ui: &Ui, kind: Kind, dpi: u32) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (w, h) = (scale(DEFAULT_W, dpi), scale(DEFAULT_H, dpi));
    let shot = render(model, ui, &palette_of(kind), w, h, dpi)?;
    let path = dir.join(format!("settings_{tag}_{}_{}dpi.png", kind_name(kind), dpi));
    crate::png::write(&path, shot.width, shot.height, &shot.bgra)?;
    Ok(path)
}

/// 시연용 변경(모델 API 로 실제 편집): 체크 하나·숫자 하나·잘못된 숫자 하나. 파일은 건드리지 않는다.
pub fn apply_demo_edits(model: &mut Model) {
    model.toggle("autoclose");
    model.set_text("retry_body", "20");
    model.set_text("punctuation_threshold", "7.5");
}
