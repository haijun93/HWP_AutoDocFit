//! 그리기 결과 기록(시험용): 이중 버퍼를 화면에 옮기기 직전의 픽셀을 해시로 남긴다.
//! `arm()`을 부른 뒤의 다음 그리기 한 번만 계산한다(평소에는 원자 변수 하나 읽기뿐).
//! UI0에서 kagent·kvf를 ui-core로 옮길 때 "화면 차이 0"을 이 해시로 확인했다.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Graphics::Gdi::{
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BitBlt, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GdiFlush, HDC, SRCCOPY, SelectObject,
};

static ARMED: AtomicBool = AtomicBool::new(false);
static LAST: Mutex<Option<Frame>> = Mutex::new(None);

/// 기록한 한 장: 크기와 픽셀(BGRX 32비트) FNV-1a 해시.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: i32,
    pub height: i32,
    pub hash: u64,
}

/// 다음 그리기를 기록하도록 켠다.
pub fn arm() {
    *LAST.lock().unwrap() = None;
    ARMED.store(true, Ordering::SeqCst);
}

/// 마지막으로 기록한 그리기(없으면 None).
pub fn take() -> Option<Frame> {
    LAST.lock().unwrap().take()
}

/// FNV-1a 64비트(순수 함수).
pub fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x100000001b3))
}

/// 그리기 직후(BitBlt 전) 부른다. 켜져 있을 때만 픽셀을 복사해 해시한다.
///
/// # Safety
/// `mem`은 w×h 비트맵이 선택된 유효한 메모리 DC여야 한다.
pub unsafe fn record(mem: HDC, w: i32, h: i32) {
    if !ARMED.swap(false, Ordering::SeqCst) || w <= 0 || h <= 0 {
        return;
    }
    unsafe {
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(mem, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        if dib.is_null() || bits.is_null() {
            return;
        }
        let dc = CreateCompatibleDC(mem);
        let old = SelectObject(dc, dib);
        BitBlt(dc, 0, 0, w, h, mem, 0, 0, SRCCOPY);
        GdiFlush();
        let len = (w as usize) * (h as usize) * 4;
        let hash = fnv1a(std::slice::from_raw_parts(bits as *const u8, len));
        SelectObject(dc, old);
        DeleteDC(dc);
        DeleteObject(dib);
        *LAST.lock().unwrap() = Some(Frame { width: w, height: h, hash });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_known_values() {
        assert_eq!(fnv1a(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a(b"a"), 0xaf63dc4c8601ec8c);
    }

    #[test]
    fn record_is_noop_unless_armed() {
        unsafe { record(std::ptr::null_mut(), 10, 10) };
        assert_eq!(take(), None);
    }
}
