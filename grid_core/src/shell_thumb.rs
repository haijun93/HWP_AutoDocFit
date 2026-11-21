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
    WorkerPanicked,
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
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                thumbnail_sta(&path, size)
            }))
            .unwrap_or(Err(Error::WorkerPanicked));
            let _ = tx.send(result);
        })
        .map_err(|e| Error::Shell(e.to_string()))?;
    match rx.recv_timeout(Duration::from_secs(15)) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(Error::Timeout),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(Error::WorkerPanicked),
    }
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
    if !absolute.is_absolute() || !absolute.is_file() {
        return Err(Error::Shell("path is not an existing file".into()));
    }
    let mut wide: Vec<u16> = absolute.as_os_str().encode_wide().collect();
    // std::fs::canonicalize may return an extended-length path (\\?\...), which
    // SHCreateItemFromParsingName rejects for otherwise valid shell items.
    const VERBATIM: &[u16] = &[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    const VERBATIM_UNC: &[u16] = &[
        b'\\' as u16,
        b'\\' as u16,
        b'?' as u16,
        b'\\' as u16,
        b'U' as u16,
        b'N' as u16,
        b'C' as u16,
        b'\\' as u16,
    ];
    if wide.starts_with(VERBATIM_UNC) {
        wide.drain(..VERBATIM_UNC.len());
        wide.splice(0..0, [b'\\' as u16, b'\\' as u16]);
    } else if wide.starts_with(VERBATIM) {
        wide.drain(..VERBATIM.len());
    }
    for unit in &mut wide {
        if *unit == b'/' as u16 {
            *unit = b'\\' as u16;
        }
    }
    wide.push(0);
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let initialized = hr.is_ok();
    if hr.is_err() {
        return Err(Error::Shell(format!(
            "COM init HRESULT=0x{:08X}",
            hr.0 as u32
        )));
    }
    let _com = ComGuard(initialized);
    let item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }
        .map_err(|e| {
            Error::Shell(format!(
                "SHCreateItemFromParsingName HRESULT=0x{:08X}",
                e.code().0 as u32
            ))
        })?;
    let factory: IShellItemImageFactory = item.cast().map_err(|e| {
        Error::Shell(format!(
            "IShellItemImageFactory HRESULT=0x{:08X}",
            e.code().0 as u32
        ))
    })?;
    let bitmap = unsafe {
        factory.GetImage(
            SIZE {
                cx: size as i32,
                cy: size as i32,
            },
            SIIGBF_RESIZETOFIT,
        )
    }
    .map_err(|e| Error::Shell(format!("GetImage HRESULT=0x{:08X}", e.code().0 as u32)))?;
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
