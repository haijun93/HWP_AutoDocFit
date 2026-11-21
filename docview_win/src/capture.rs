//! 캡처 모드: 창을 만들지 않고 `paint::draw` 를 메모리 DC 에 그려 PNG 로 쓴다(`settings_win`·`outliner_win` 과 같은 방식).
use crate::layout::{DEFAULT_H, DEFAULT_W, scale};
use crate::model::App;
use crate::paint::draw;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use ui_theme::{Kind, Palette};
use windows_sys::Win32::Graphics::Gdi::*;

pub struct Shot {
    pub width: usize,
    pub height: usize,
    /// 위에서 아래로의 BGRA.
    pub bgra: Vec<u8>,
    /// 이번 그리기에서 실제로 그린 칸 수.
    pub drawn: usize,
}

pub fn render(app: &App, pal: &Palette, width: i32, height: i32, dpi: u32) -> Result<Shot, String> {
    unsafe {
        let screen = GetDC(null_mut());
        let mem = CreateCompatibleDC(screen);
        let bm = CreateCompatibleBitmap(screen, width, height);
        let old = SelectObject(mem, bm);
        let drawn = draw(mem, width, height, app, pal, dpi);
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
        Ok(Shot { width: width as usize, height: height as usize, bgra: pixels, drawn })
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
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (w, h) = (scale(size.0, dpi), scale(size.1, dpi));
    let shot = render(app, &palette_of(kind), w, h, dpi)?;
    let path = dir.join(format!("docview_{tag}_{}_{}dpi.png", kind_name(kind), dpi));
    crate::png::write(&path, shot.width, shot.height, &shot.bgra)?;
    Ok((path, shot.drawn))
}
