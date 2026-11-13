//! 글꼴 캐시: (글꼴명·논리 px·굵기·품질·DPI)마다 한 번만 `CreateFontW`.
//! UI0 이전 popup은 그릴 때마다 글꼴 6개를 만들고 지웠다. 같은 인자로 만들므로 그려지는 픽셀은 같다.
//! 배율이 바뀌면(WM_DPICHANGED) 새 DPI 열쇠로 새 글꼴을 만들고, [`FontCache::retain_dpi`]로 옛것을 지운다.

use windows_sys::Win32::Graphics::Gdi::{CreateFontW, DeleteObject, HFONT};

/// 맑은 고딕(본문), Segoe MDL2 Assets(캡션 단추 글리프).
pub const FACE_UI: &str = "맑은 고딕";
pub const FACE_GLYPH: &str = "Segoe MDL2 Assets";

/// CreateFontW의 품질 값.
pub const QUALITY_DEFAULT: u32 = 0;
pub const QUALITY_CLEARTYPE: u32 = 5;
/// DEFAULT_CHARSET.
pub const CHARSET_DEFAULT: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FontSpec {
    pub face: &'static str,
    /// 96 DPI 기준 글자 높이(px, 셀 높이가 아니라 글자 높이 = 음수로 넘김).
    pub px: i32,
    pub weight: i32,
    pub quality: u32,
}

impl FontSpec {
    pub const fn new(face: &'static str, px: i32, weight: i32, quality: u32) -> Self {
        Self { face, px, weight, quality }
    }
    /// popup이 쓰던 모양(ClearType).
    pub const fn clear(face: &'static str, px: i32, weight: i32) -> Self {
        Self::new(face, px, weight, QUALITY_CLEARTYPE)
    }
    /// 이 DPI에서의 실제 글자 높이(popup·kvf와 같은 `-(px * dpi / 96)`).
    pub fn height(&self, dpi: u32) -> i32 {
        -crate::geom::scale(self.px, dpi)
    }
}

/// 한 화면 스레드의 글꼴 캐시. 떨어질 때 모두 지운다.
#[derive(Default)]
pub struct FontCache {
    fonts: Vec<(FontSpec, u32, HFONT)>,
}

impl FontCache {
    pub const fn new() -> Self {
        Self { fonts: Vec::new() }
    }

    /// 없으면 만든다. 실패하면 null(그리기 쪽에서 확인).
    pub fn get(&mut self, spec: FontSpec, dpi: u32) -> HFONT {
        let dpi = dpi.max(96);
        if let Some((_, _, f)) = self.fonts.iter().find(|(s, d, _)| *s == spec && *d == dpi) {
            return *f;
        }
        let face: Vec<u16> = spec.face.encode_utf16().chain(std::iter::once(0)).collect();
        let f = unsafe {
            CreateFontW(spec.height(dpi), 0, 0, 0, spec.weight, 0, 0, 0, CHARSET_DEFAULT, 0, 0, spec.quality, 0,
                        face.as_ptr())
        };
        if !f.is_null() {
            self.fonts.push((spec, dpi, f));
        }
        f
    }

    /// 지금 DPI가 아닌 글꼴을 지운다(배율 변경 뒤).
    pub fn retain_dpi(&mut self, dpi: u32) {
        let dpi = dpi.max(96);
        self.fonts.retain(|(_, d, f)| {
            if *d != dpi {
                unsafe { DeleteObject(*f) };
            }
            *d == dpi
        });
    }

    pub fn len(&self) -> usize {
        self.fonts.len()
    }
    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }
}

impl Drop for FontCache {
    fn drop(&mut self) {
        for (_, _, f) in self.fonts.drain(..) {
            unsafe { DeleteObject(f) };
        }
    }
}

// 화면 스레드 하나에서만 쓴다(HFONT는 프로세스 전역 핸들이라 옮겨도 안전).
unsafe impl Send for FontCache {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_reuses_and_drops_other_dpi() {
        let mut c = FontCache::new();
        let body = FontSpec::clear(FACE_UI, 15, 400);
        let a = c.get(body, 96);
        assert!(!a.is_null());
        assert_eq!(c.get(body, 96), a);
        assert_eq!(c.get(body, 0), a, "DPI 0은 96");
        let b = c.get(body, 144);
        assert_ne!(a, b);
        assert_eq!(c.len(), 2);
        c.retain_dpi(144);
        assert_eq!(c.len(), 1);
        assert_eq!(c.get(body, 144), b);
        assert_eq!(body.height(144), -22);
        assert_eq!(FontSpec::clear(FACE_GLYPH, 10, 400).height(120), -12);
    }
}
