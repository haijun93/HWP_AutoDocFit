//! Windows 전용: GDI 그리기 도우미(칸·글·선·단추)·메모리 DC 캡처·시간대·고정 드라이브. 문서뷰·사진뷰가 같이 쓴다.
use crate::layout::{Rect, scale};
use std::path::PathBuf;
use std::ptr::null_mut;
use ui_theme::{Kind, Palette};
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};

pub const FACE: &str = "맑은 고딕";

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 지금 시간대의 UTC 대비 오프셋(초). 여름 시간제 반영.
pub fn tz_offset_secs() -> i64 {
    unsafe {
        let mut tz: TIME_ZONE_INFORMATION = std::mem::zeroed();
        let id = GetTimeZoneInformation(&mut tz);
        let extra = match id {
            1 => tz.StandardBias,
            2 => tz.DaylightBias,
            _ => 0,
        };
        -((tz.Bias + extra) as i64) * 60
    }
}

/// 고정 드라이브 루트(`C:\` …). Flutter 기본 범위 "모든 고정 드라이브" 의 Rust 대응.
pub fn fixed_drive_roots() -> Vec<PathBuf> {
    let mask = unsafe { GetLogicalDrives() };
    (0..26u32)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| format!("{}:\\", (b'A' + i as u8) as char))
        .filter(|root| unsafe { GetDriveTypeW(wide(root).as_ptr()) } == 3)
        .map(PathBuf::from)
        .collect()
}

pub struct Fonts {
    pub normal: HFONT,
    pub bold: HFONT,
    pub small: HFONT,
    pub badge: HFONT,
    pub title: HFONT,
}

pub unsafe fn make_font(px: i32, weight: i32) -> HFONT {
    unsafe {
        CreateFontW(-px, 0, 0, 0, weight, 0, 0, 0, DEFAULT_CHARSET as u32, OUT_DEFAULT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32, CLEARTYPE_QUALITY as u32, (DEFAULT_PITCH | FF_DONTCARE) as u32, wide(FACE).as_ptr())
    }
}

impl Fonts {
    pub unsafe fn new(dpi: u32) -> Fonts {
        unsafe {
            Fonts {
                normal: make_font(scale(14, dpi), 400),
                bold: make_font(scale(13, dpi), 700),
                small: make_font(scale(12, dpi), 400),
                badge: make_font(scale(11, dpi), 700),
                title: make_font(scale(17, dpi), 700),
            }
        }
    }
    pub unsafe fn free(&self) {
        unsafe {
            for f in [self.normal, self.bold, self.small, self.badge, self.title] {
                DeleteObject(f);
            }
        }
    }
}

pub fn rc(r: &Rect) -> RECT {
    RECT { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

pub unsafe fn fill(hdc: HDC, r: &Rect, color: u32) {
    unsafe {
        let b = CreateSolidBrush(color);
        FillRect(hdc, &rc(r), b);
        DeleteObject(b);
    }
}

pub unsafe fn boxed(hdc: HDC, r: &Rect, fill_c: u32, border: u32, border_w: i32, radius: i32) {
    unsafe {
        let pen = CreatePen(PS_SOLID, border_w.max(1), border);
        let brush = CreateSolidBrush(fill_c);
        let (op, ob) = (SelectObject(hdc, pen), SelectObject(hdc, brush));
        RoundRect(hdc, r.left, r.top, r.right, r.bottom, radius, radius);
        SelectObject(hdc, op);
        SelectObject(hdc, ob);
        DeleteObject(pen);
        DeleteObject(brush);
    }
}

pub unsafe fn line(hdc: HDC, x1: i32, y1: i32, x2: i32, y2: i32, color: u32, w: i32) {
    unsafe {
        let pen = CreatePen(PS_SOLID, w.max(1), color);
        let old = SelectObject(hdc, pen);
        MoveToEx(hdc, x1, y1, null_mut());
        LineTo(hdc, x2, y2);
        SelectObject(hdc, old);
        DeleteObject(pen);
    }
}

pub unsafe fn text(hdc: HDC, font: HFONT, r: &Rect, s: &str, color: u32, flags: u32) {
    unsafe {
        let old = SelectObject(hdc, font);
        SetBkMode(hdc, TRANSPARENT as i32);
        SetTextColor(hdc, color);
        let w = wide(s);
        let mut r = rc(r);
        DrawTextW(hdc, w.as_ptr(), -1, &mut r, flags | DT_NOPREFIX);
        SelectObject(hdc, old);
    }
}

pub unsafe fn text_width(hdc: HDC, font: HFONT, s: &str) -> i32 {
    unsafe {
        let old = SelectObject(hdc, font);
        let w = wide(s);
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        DrawTextW(hdc, w.as_ptr(), -1, &mut r, DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX);
        SelectObject(hdc, old);
        r.right - r.left
    }
}

pub const ONE_LINE: u32 = DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS;
pub const CENTER: u32 = DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS;

pub unsafe fn button(hdc: HDC, f: &Fonts, r: &Rect, label: &str, enabled: bool, primary: bool, pal: &Palette, dpi: u32) {
    unsafe {
        let (bg, fg, border) = if primary && enabled {
            (pal.accent, pal.bg, pal.accent)
        } else {
            (pal.bg, if enabled { pal.text } else { pal.text_faint }, pal.divider)
        };
        boxed(hdc, r, bg, border, 1, scale(8, dpi));
        text(hdc, f.normal, r, label, fg, CENTER);
    }
}

pub struct Shot {
    pub width: usize,
    pub height: usize,
    /// 위에서 아래로의 BGRA.
    pub bgra: Vec<u8>,
    /// 그리기 함수가 돌려준 값(이번에 그린 칸 수).
    pub drawn: usize,
}

/// 메모리 DC 에 `draw` 로 그린 화면을 BGRA 로 읽는다(캡처 모드 공용).
pub fn render(width: i32, height: i32, draw: impl FnOnce(HDC) -> usize) -> Result<Shot, String> {
    unsafe {
        let screen = GetDC(null_mut());
        let mem = CreateCompatibleDC(screen);
        let bm = CreateCompatibleBitmap(screen, width, height);
        let old = SelectObject(mem, bm);
        let drawn = draw(mem);
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
