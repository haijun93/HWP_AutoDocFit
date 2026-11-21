//! Windows Explorer thumbnail provider via IShellItemImageFactory.
use std::{
    collections::{HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, mpsc},
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

/// A completed asynchronous request. Errors are strings so callers can pass results across UI
/// boundaries without depending on the shell crate's error internals.
pub struct Completed {
    pub path: PathBuf,
    pub result: Result<Bitmap, String>,
}

struct QueueState {
    pending: VecDeque<PathBuf>,
    active: HashSet<PathBuf>,
    known: HashSet<PathBuf>,
    stopped: bool,
}

/// Bounded STA-backed thumbnail workers. `set_visible` replaces queued work and preserves the
/// order supplied by the view, so newly visible cells go first and old off-screen work is dropped.
pub struct ThumbnailService {
    queue: Arc<(Mutex<QueueState>, Condvar)>,
    completed: Mutex<mpsc::Receiver<Completed>>,
}

impl ThumbnailService {
    pub fn new(workers: usize, notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        let queue = Arc::new((
            Mutex::new(QueueState {
                pending: VecDeque::new(),
                active: HashSet::new(),
                known: HashSet::new(),
                stopped: false,
            }),
            Condvar::new(),
        ));
        let (tx, rx) = mpsc::channel();
        for index in 0..workers.clamp(1, 8) {
            let queue = Arc::clone(&queue);
            let tx = tx.clone();
            let notify = Arc::clone(&notify);
            let _ = std::thread::Builder::new()
                .name(format!("shell-thumb-{index}"))
                .spawn(move || {
                    // Keep this worker in an STA for its lifetime. `thumbnail_sta` balances an
                    // additional CoInitializeEx reference around each individual shell call.
                    let worker_init = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
                    let _worker_com = ComGuard(worker_init.is_ok());
                    loop {
                        let path = {
                            let (lock, wake) = &*queue;
                            let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
                            while state.pending.is_empty() && !state.stopped {
                                state = wake.wait(state).unwrap_or_else(|e| e.into_inner());
                            }
                            if state.stopped {
                                return;
                            }
                            let path = state.pending.pop_front().expect("checked above");
                            state.active.insert(path.clone());
                            path
                        };
                        let result = if worker_init.is_ok() {
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                                thumbnail_sta(&path, 256)
                            }))
                            .unwrap_or(Err(Error::WorkerPanicked))
                            .map_err(|e| e.to_string())
                        } else {
                            Err(format!("COM init HRESULT=0x{:08X}", worker_init.0 as u32))
                        };
                        {
                            let (lock, _) = &*queue;
                            let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
                            state.active.remove(&path);
                        }
                        let _ = tx.send(Completed { path, result });
                        notify();
                    }
                });
        }
        drop(tx);
        Self {
            queue,
            completed: Mutex::new(rx),
        }
    }

    pub fn set_visible(&self, ordered_paths: impl IntoIterator<Item = PathBuf>) {
        let (lock, wake) = &*self.queue;
        let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
        let old_pending = std::mem::take(&mut state.pending);
        for old in old_pending {
            state.known.remove(&old);
        }
        let mut next = VecDeque::new();
        let mut visible = HashSet::new();
        for path in ordered_paths {
            if visible.insert(path.clone())
                && !state.known.contains(&path)
                && !state.active.contains(&path)
            {
                state.known.insert(path.clone());
                next.push_back(path);
            }
        }
        state.pending = next;
        wake.notify_all();
    }

    pub fn drain(&self) -> Vec<Completed> {
        let rx = self.completed.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = Vec::new();
        while let Ok(result) = rx.try_recv() {
            out.push(result);
        }
        out
    }

    pub fn forget(&self, path: &Path) {
        let (lock, _) = &*self.queue;
        let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
        if !state.active.contains(path) {
            state.known.remove(path);
        }
    }
}

impl Drop for ThumbnailService {
    fn drop(&mut self) {
        let (lock, wake) = &*self.queue;
        lock.lock().unwrap_or_else(|e| e.into_inner()).stopped = true;
        wake.notify_all();
        // Threads are detached: an in-process shell extension can hang indefinitely and Windows
        // provides no safe way to abort its COM call during window shutdown.
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

/// Uses an isolated STA worker with a 3 s caller timeout. A hung shell extension can leave that
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
    match rx.recv_timeout(Duration::from_secs(3)) {
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
