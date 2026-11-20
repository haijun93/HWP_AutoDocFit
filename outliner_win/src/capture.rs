//! 캡처 모드: 창을 만들지 않고 `paint::draw` 를 메모리 DC에 그려 화소/PNG 로 돌려준다
//! (`ktm --capture-dir` 와 같은 방식).
use crate::paint::draw;
use crate::view::View;
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

/// 지정한 팔레트로 `view` 를 그려 화소를 돌려준다.
pub fn render(view: &View, pal: &Palette, width: i32, height: i32, dpi: u32, title: &str) -> Result<Shot, String> {
    unsafe {
        let screen = GetDC(null_mut());
        let mem = CreateCompatibleDC(screen);
        let bm = CreateCompatibleBitmap(screen, width, height);
        let old = SelectObject(mem, bm);
        draw(mem, width, height, view, pal, dpi, title);
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

/// `dir` 에 `outliner_<테마>_<dpi>dpi.png` 를 쓰고 경로를 돌려준다.
pub fn capture_to(dir: &Path, view: &View, kind: Kind, dpi: u32, title: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (w, h) = (crate::paint::scale(720, dpi), crate::paint::scale(520, dpi));
    let shot = render(view, &palette_of(kind), w, h, dpi, title)?;
    let path = dir.join(format!("outliner_{}_{}dpi.png", kind_name(kind), dpi));
    crate::png::write(&path, shot.width, shot.height, &shot.bgra)?;
    Ok(path)
}
