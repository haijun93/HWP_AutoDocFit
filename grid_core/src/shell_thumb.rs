//! Windows Explorer thumbnail provider via IShellItemImageFactory.
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};
use windows::{
    Win32::{
        Foundation::SIZE,
        Graphics::Gdi::{
            BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, GetDC, GetDIBits,
            GetObjectW, HBITMAP, ReleaseDC,
        },
        System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize},
        UI::Shell::{
            IShellItem, IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_RESIZETOFIT,
        },
    },
    core::{Interface, PCWSTR},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

#[derive(Debug)]
pub enum Error {
    InvalidSize,
    PathEncoding,
    Timeout,
    Shell(String),
    NoPixels,
    Oversized,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

/// Uses an isolated STA worker with a 15 s caller timeout. A hung shell extension can leave that
/// worker blocked; Windows offers no safe way to abort an in-process COM call.
pub fn thumbnail(path: &Path, size: u32) -> Result<Bitmap, Error> {
    if size == 0 || size > 4096 {
        return Err(Error::InvalidSize);
    }
    let path = path.to_path_buf();
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("shell-thumb-sta".into())
        .spawn(move || {
            let _ = tx.send(unsafe { thumbnail_sta(&path, size) });
        })
        .map_err(|e| Error::Shell(e.to_string()))?;
    rx.recv_timeout(Duration::from_secs(15))
        .map_err(|_| Error::Timeout)?
}

struct ComGuard(bool);
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe {
                CoUninitialize();
            }
        }
    }
}
struct BitmapGuard(HBITMAP);
impl Drop for BitmapGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::DeleteObject(self.0.into());
        }
    }
}

unsafe fn thumbnail_sta(path: &PathBuf, size: u32) -> Result<Bitmap, Error> {
    let absolute = path
        .canonicalize()
        .map_err(|e| Error::Shell(format!("path: {e}")))?;
    let mut wide: Vec<u16> = absolute.as_os_str().encode_wide().collect();
    wide.push(0);
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let initialized = hr.is_ok();
    if hr.is_err() && hr.0 != 0x80010106u32 as i32 {
        return Err(Error::Shell(format!("COM init: {hr}")));
    }
    let _com = ComGuard(initialized);
    let item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }
        .map_err(|e| Error::Shell(format!("item: {e}")))?;
    let factory: IShellItemImageFactory = item
        .cast()
        .map_err(|e| Error::Shell(format!("image factory: {e}")))?;
    let bitmap = unsafe {
        factory.GetImage(
            SIZE {
                cx: size as i32,
                cy: size as i32,
            },
            SIIGBF_RESIZETOFIT,
        )
    }
    .map_err(|e| Error::Shell(format!("GetImage: {e}")))?;
    let _bitmap = BitmapGuard(bitmap);
    let mut object = BITMAP::default();
    if unsafe {
        GetObjectW(
            bitmap.into(),
            std::mem::size_of::<BITMAP>() as i32,
            Some((&mut object as *mut BITMAP).cast()),
        )
    } == 0
    {
        return Err(Error::NoPixels);
    }
    let (w, h) = (object.bmWidth as u32, object.bmHeight.unsigned_abs());
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let dc = unsafe { GetDC(None) };
    if dc.is_invalid() {
        return Err(Error::NoPixels);
    }
    if w == 0
        || h == 0
        || w > 16384
        || h > 16384
        || (w as usize).saturating_mul(h as usize).saturating_mul(4) > 256 * 1024 * 1024
    {
        unsafe {
            ReleaseDC(None, dc);
        }
        return Err(Error::Oversized);
    }
    info.bmiHeader.biWidth = w as i32;
    info.bmiHeader.biHeight = -(h as i32);
    let mut pixels = vec![0u8; w as usize * h as usize * 4];
    let rows = unsafe {
        GetDIBits(
            dc,
            bitmap,
            0,
            h,
            Some(pixels.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        )
    };
    unsafe {
        ReleaseDC(None, dc);
    }
    if rows == 0 {
        return Err(Error::NoPixels);
    }
    Ok(Bitmap {
        width: w,
        height: h,
        bgra: pixels,
    })
}

use std::os::windows::ffi::OsStrExt;
