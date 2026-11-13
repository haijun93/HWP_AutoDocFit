//! 그림 항목 미리보기용 축소 썸네일(사용자 요청 2026-11-13).
//!
//! - 마우스·키보드로 그림 항목을 가리킬 때(앱의 `thumb` 요청 포함) 없으면 바로 만든다. 그림·화면캡처 항목을
//!   기록하면 `prewarm`이 쉬는 때 미리 만들어 둔다(KCAP2, 첫 미리보기 50ms 목표).
//! - 메모리 캐시(최근 것부터, 합계 `MEM_CAP` 바이트)와 디스크 캐시(`thumbs\`, DPAPI 암호화, 합계 `DISK_CAP`).
//!   디스크 캐시는 에이전트를 다시 켠 뒤 큰 그림을 다시 풀지 않게 하려는 것이며, 클립보드 내용 보호 원칙에
//!   맞춰 평문으로 두지 않는다. 모두 지우기 때 함께 지운다.
//! - 저장한 그림(PNG)은 직접 풀어 줄이고, 파일 항목(화면캡처 폴더·탐색기 복사)은 PNG면 직접, 그 밖(JPG 등)은
//!   Windows 탐색기 썸네일(IShellItemImageFactory, 시스템 썸네일 캐시 활용)로 만든다.
//! 순수 계산(크기 맞춤·축소·캐시 한도)은 따로 시험한다.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// 썸네일 긴 변 최대(물리 픽셀).
pub const MAX_SIDE: u32 = 320;
/// 메모리 캐시 합계 상한(약 4MB — 16:9 썸네일 17장 정도).
pub const MEM_CAP: usize = 4 * 1024 * 1024;
/// 디스크 캐시 합계 상한.
pub const DISK_CAP: u64 = 16 * 1024 * 1024;

/// 위에서 아래로 쌓인 BGRA(알파는 흰 바탕에 합성해 255).
#[derive(Clone, Debug, PartialEq)]
pub struct Thumb {
    pub w: u32,
    pub h: u32,
    pub bgra: Vec<u8>,
}

/// 비율을 지키며 긴 변이 `max`를 넘지 않게(작은 그림은 키우지 않음).
pub fn fit(w: u32, h: u32, max: u32) -> (u32, u32) {
    if w == 0 || h == 0 {
        return (0, 0);
    }
    if w <= max && h <= max {
        return (w, h);
    }
    if w >= h {
        (max, ((h as u64 * max as u64 + w as u64 / 2) / w as u64).max(1) as u32)
    } else {
        (((w as u64 * max as u64 + h as u64 / 2) / h as u64).max(1) as u32, max)
    }
}

/// 영역 평균 축소: 채널 1~4개(회색·회색+알파·RGB·RGBA) → 흰 바탕 합성 BGRA. `stride`는 원본 줄 바이트.
pub fn downscale(src: &[u8], sw: u32, sh: u32, channels: usize, stride: usize, tw: u32, th: u32) -> Option<Vec<u8>> {
    if sw == 0 || sh == 0 || tw == 0 || th == 0 || !(1..=4).contains(&channels) {
        return None;
    }
    let need = stride.checked_mul(sh as usize - 1)?.checked_add(sw as usize * channels)?;
    if src.len() < need {
        return None;
    }
    let mut out = vec![0u8; tw as usize * th as usize * 4];
    for ty in 0..th {
        let y0 = (ty as u64 * sh as u64 / th as u64) as u32;
        let y1 = (((ty + 1) as u64 * sh as u64).div_ceil(th as u64) as u32).clamp(y0 + 1, sh);
        for tx in 0..tw {
            let x0 = (tx as u64 * sw as u64 / tw as u64) as u32;
            let x1 = (((tx + 1) as u64 * sw as u64).div_ceil(tw as u64) as u32).clamp(x0 + 1, sw);
            let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
            for y in y0..y1 {
                let line = &src[y as usize * stride..];
                for x in x0..x1 {
                    let p = &line[x as usize * channels..x as usize * channels + channels];
                    let (pr, pg, pb, pa) = match channels {
                        1 => (p[0], p[0], p[0], 255),
                        2 => (p[0], p[0], p[0], p[1]),
                        3 => (p[0], p[1], p[2], 255),
                        _ => (p[0], p[1], p[2], p[3]),
                    };
                    // 흰 바탕 합성: c*a + 255*(255-a)
                    let a = pa as u64;
                    r += (pr as u64 * a + 255 * (255 - a)) / 255;
                    g += (pg as u64 * a + 255 * (255 - a)) / 255;
                    b += (pb as u64 * a + 255 * (255 - a)) / 255;
                    n += 1;
                }
            }
            let o = (ty as usize * tw as usize + tx as usize) * 4;
            out[o..o + 4].copy_from_slice(&[(b / n) as u8, (g / n) as u8, (r / n) as u8, 255]);
        }
    }
    Some(out)
}

/// PNG → 썸네일. 원본 크기 한도는 클립보드와 같다(가로·세로 2만, 1,600만 화소).
pub fn from_png(data: &[u8], max: u32) -> Option<Thumb> {
    let mut decoder = png::Decoder::new(data);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let info = decoder.read_header_info().ok()?;
    let (w, h) = (info.width, info.height);
    if w == 0 || h == 0 || w > crate::clip::MAX_DIB_DIMENSION || h > crate::clip::MAX_DIB_DIMENSION
        || (w as usize).checked_mul(h as usize)? > crate::clip::MAX_DIB_PIXELS
    {
        return None;
    }
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).ok()?;
    let (tw, th) = fit(frame.width, frame.height, max);
    let bgra = downscale(&buf, frame.width, frame.height, frame.color_type.samples(), frame.line_size, tw, th)?;
    Some(Thumb { w: tw, h: th, bgra })
}

/// 썸네일 → PNG(RGB). 디스크 캐시와 앱 전달용.
pub fn to_png(t: &Thumb) -> Option<Vec<u8>> {
    let mut rgb = Vec::with_capacity(t.w as usize * t.h as usize * 3);
    for px in t.bgra.chunks_exact(4) {
        rgb.extend_from_slice(&[px[2], px[1], px[0]]);
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, t.w, t.h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(&rgb).ok()?;
    }
    Some(out)
}

// ---- 메모리 캐시 ----

/// 최근 쓴 것이 뒤. 합계 바이트가 상한을 넘으면 앞에서부터 버린다.
#[derive(Default)]
pub struct MemCache {
    items: VecDeque<(String, Arc<Thumb>)>,
    bytes: usize,
    cap: usize,
}

impl MemCache {
    pub fn new(cap: usize) -> Self {
        MemCache { items: VecDeque::new(), bytes: 0, cap }
    }
    pub fn get(&mut self, key: &str) -> Option<Arc<Thumb>> {
        let pos = self.items.iter().position(|(k, _)| k == key)?;
        let item = self.items.remove(pos)?;
        let t = item.1.clone();
        self.items.push_back(item);
        Some(t)
    }
    pub fn contains(&self, key: &str) -> bool {
        self.items.iter().any(|(k, _)| k == key)
    }
    pub fn put(&mut self, key: String, t: Arc<Thumb>) {
        if let Some(pos) = self.items.iter().position(|(k, _)| *k == key) {
            if let Some((_, old)) = self.items.remove(pos) {
                self.bytes -= old.bgra.len();
            }
        }
        if t.bgra.len() > self.cap {
            return;
        }
        self.bytes += t.bgra.len();
        self.items.push_back((key, t));
        while self.bytes > self.cap {
            let Some((_, old)) = self.items.pop_front() else { break };
            self.bytes -= old.bgra.len();
        }
    }
    pub fn clear(&mut self) {
        self.items.clear();
        self.bytes = 0;
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

/// 디스크 캐시 정리 계획: (이름, 크기, 수정 시각) 목록에서 합계가 상한 이하가 될 때까지 오래된 것부터 지울 이름.
pub fn disk_evictions(mut files: Vec<(String, u64, u64)>, cap: u64) -> Vec<String> {
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort_by_key(|f| f.2);
    let mut out = Vec::new();
    for (name, size, _) in files {
        if total <= cap {
            break;
        }
        total -= size;
        out.push(name);
    }
    out
}

/// 디스크 캐시 파일 이름(내용 지문, 경로·내용은 드러나지 않음).
pub fn disk_name(fingerprint: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in fingerprint.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}.thumb")
}

/// 미리보기를 만들 그림 파일 확장자(탐색기 썸네일로 만들 수 있는 흔한 형식).
pub fn is_image_path(path: &str) -> bool {
    matches!(crate::category::extension(path).as_deref(),
             Some("png" | "jpg" | "jpeg" | "jpe" | "gif" | "bmp" | "webp" | "tif" | "tiff" | "heic" | "heif" | "avif"))
}

/// 파일 항목에서 미리보기할 첫 그림 경로.
pub fn first_image_path(data: &[u8]) -> Option<String> {
    String::from_utf8_lossy(data)
        .split(['\n', '\0'])
        .map(str::trim)
        .find(|p| !p.is_empty() && is_image_path(p))
        .map(str::to_owned)
}

/// 메모리 캐시 키: 항목 번호와 크기(번호가 다시 쓰여도 다른 내용이면 다른 키).
pub fn mem_key(id: i64, size: i64) -> String {
    format!("{id}/{size}")
}

// ---- 시스템 연결(에이전트 실행 중에만) ----

static MEM: Mutex<Option<MemCache>> = Mutex::new(None);
/// 캐시 세대: 모두 지우기마다 하나 늘린다. 지우기 전에 시작한 생성 결과는 캐시에 넣지 않는다
/// (미리 만들기·즉시 생성이 지우기와 겹쳐도 지운 뒤에 썸네일이 되살아나지 않게).
static EPOCH: AtomicU64 = AtomicU64::new(0);

fn with_mem<R>(f: impl FnOnce(&mut MemCache) -> R) -> R {
    let mut guard = MEM.lock().unwrap();
    f(guard.get_or_insert_with(|| MemCache::new(MEM_CAP)))
}

pub fn cached(id: i64, size: i64) -> Option<Arc<Thumb>> {
    with_mem(|m| m.get(&mem_key(id, size)))
}

pub fn is_cached(id: i64, size: i64) -> bool {
    with_mem(|m| m.contains(&mem_key(id, size)))
}

fn disk_dir() -> String {
    format!("{}\\thumbs", crate::app().dir)
}

/// 메모리·디스크 캐시를 모두 지운다(모두 지우기·끄기 때).
pub fn clear_all() {
    EPOCH.fetch_add(1, Ordering::SeqCst);
    crate::prewarm::clear();
    with_mem(|m| m.clear());
    let _ = std::fs::remove_dir_all(disk_dir());
}

fn disk_get(name: &str) -> Option<Thumb> {
    let sealed = std::fs::read(format!("{}\\{name}", disk_dir())).ok()?;
    let png = crate::crypt::unprotect(&sealed)?;
    from_png(&png, MAX_SIDE)
}

fn disk_put(name: &str, t: &Thumb, epoch: u64) {
    let Some(sealed) = to_png(t).and_then(|p| crate::crypt::protect(&p)) else { return };
    if EPOCH.load(Ordering::SeqCst) != epoch {
        return;
    }
    let dir = disk_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let tmp = format!("{dir}\\{name}.tmp");
    if std::fs::write(&tmp, &sealed).is_err() || std::fs::rename(&tmp, format!("{dir}\\{name}")).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return;
    }
    let files: Vec<(String, u64, u64)> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| {
                    let m = e.metadata().ok()?;
                    let t = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64;
                    Some((e.file_name().to_string_lossy().into_owned(), m.len(), t))
                })
                .collect()
        })
        .unwrap_or_default();
    for old in disk_evictions(files, DISK_CAP) {
        let _ = std::fs::remove_file(format!("{dir}\\{old}"));
    }
}

/// 항목 번호로 썸네일을 구한다(메모리 → 디스크 → 새로 만들기). 그림 항목이 아니면 None.
/// 창 스레드에서 부르지 말 것(파일·복호화·풀기가 들어간다) — 미리보기 작업 스레드·파이프 스레드에서 부른다.
pub fn get(id: i64, size: i64) -> Option<Arc<Thumb>> {
    if let Some(t) = cached(id, size) {
        return Some(t);
    }
    let epoch = EPOCH.load(Ordering::SeqCst);
    // 저장 그림은 DB의 내용 지문으로 디스크 캐시를 먼저 본다(큰 그림을 복호화하지 않고).
    let (kind, hash) = crate::app().store.lock().unwrap().kind_hash(id).ok().flatten()?;
    if kind == "image" {
        if let Some(t) = disk_get(&disk_name(&hash)) {
            return Some(remember(id, size, t, epoch));
        }
    }
    let (kind, data, _) = crate::app().store.lock().unwrap().payload(id).ok().flatten()?;
    let (fingerprint, source) = match kind.as_str() {
        "image" => (hash, Source::Png(data)),
        "files" => {
            let path = first_image_path(&data)?;
            let meta = std::fs::metadata(&path).ok()?;
            let mtime = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis();
            (format!("file:{path}:{}:{mtime}", meta.len()), Source::File(path))
        }
        _ => return None,
    };
    let name = disk_name(&fingerprint);
    let thumb = match disk_get(&name) {
        Some(t) => t,
        None => {
            let t = match source {
                Source::Png(bytes) => from_png(&bytes, MAX_SIDE)?,
                Source::File(path) => from_file(&path, MAX_SIDE)?,
            };
            disk_put(&name, &t, epoch);
            t
        }
    };
    Some(remember(id, size, thumb, epoch))
}

fn remember(id: i64, size: i64, thumb: Thumb, epoch: u64) -> Arc<Thumb> {
    let t = Arc::new(thumb);
    with_mem(|m| {
        if EPOCH.load(Ordering::SeqCst) == epoch {
            m.put(mem_key(id, size), t.clone());
        }
    });
    t
}

enum Source {
    Png(Vec<u8>),
    File(String),
}

/// 그림 파일 → 썸네일. PNG는 직접(큰 파일은 거절), 그 밖은 탐색기 썸네일.
pub fn from_file(path: &str, max: u32) -> Option<Thumb> {
    if path.to_ascii_lowercase().ends_with(".png") {
        if std::fs::metadata(path).ok()?.len() <= 50 * 1024 * 1024 {
            if let Some(t) = std::fs::read(path).ok().and_then(|b| from_png(&b, max)) {
                return Some(t);
            }
        }
    }
    shell_thumbnail(path, max)
}

/// Windows 탐색기 썸네일(IShellItemImageFactory::GetImage). COM 초기화는 이 함수 안에서 한다.
fn shell_thumbnail(path: &str, max: u32) -> Option<Thumb> {
    use windows_sys::Win32::Foundation::SIZE;
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW,
        HBITMAP, ReleaseDC,
    };
    use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
    use windows_sys::Win32::UI::Shell::SHCreateItemFromParsingName;
    use windows_sys::core::{GUID, HRESULT};

    #[repr(C)]
    struct FactoryVtbl {
        query_interface: usize,
        add_ref: usize,
        release: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
        get_image: unsafe extern "system" fn(*mut std::ffi::c_void, SIZE, i32, *mut HBITMAP) -> HRESULT,
    }
    const IID_FACTORY: GUID = GUID::from_u128(0xbcc18b79_ba16_442f_80c4_8a59c30c463b);
    const SIIGBF_THUMBNAILONLY: i32 = 0x08;

    unsafe {
        let init = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        let mut factory: *mut std::ffi::c_void = std::ptr::null_mut();
        let wide = crate::util::wide(path);
        let hr = SHCreateItemFromParsingName(wide.as_ptr(), std::ptr::null_mut(), &IID_FACTORY, &mut factory);
        let mut result = None;
        if hr >= 0 && !factory.is_null() {
            let vtbl = &**(factory as *mut *const FactoryVtbl);
            let mut hbmp: HBITMAP = std::ptr::null_mut();
            let size = SIZE { cx: max as i32, cy: max as i32 };
            if (vtbl.get_image)(factory, size, SIIGBF_THUMBNAILONLY, &mut hbmp) >= 0 && !hbmp.is_null() {
                let mut bm: BITMAP = std::mem::zeroed();
                if GetObjectW(hbmp, std::mem::size_of::<BITMAP>() as i32, &mut bm as *mut _ as _) != 0
                    && bm.bmWidth > 0 && bm.bmHeight > 0
                {
                    let (w, h) = (bm.bmWidth as u32, bm.bmHeight as u32);
                    let mut bi: BITMAPINFO = std::mem::zeroed();
                    bi.bmiHeader = BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w as i32, biHeight: -(h as i32),
                        biPlanes: 1, biBitCount: 32, biCompression: BI_RGB, ..std::mem::zeroed()
                    };
                    let mut px = vec![0u8; w as usize * h as usize * 4];
                    let dc = GetDC(std::ptr::null_mut());
                    let lines = GetDIBits(dc, hbmp, 0, h, px.as_mut_ptr() as _, &mut bi, DIB_RGB_COLORS);
                    ReleaseDC(std::ptr::null_mut(), dc);
                    if lines == h as i32 {
                        // 탐색기 썸네일의 알파는 0일 때가 많아 불투명으로 본다. BGRA → RGBA로 바꿔 같은 축소를 쓴다.
                        let rgba: Vec<u8> = px.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], 255]).collect();
                        let (tw, th) = fit(w, h, max);
                        result = downscale(&rgba, w, h, 4, w as usize * 4, tw, th).map(|bgra| Thumb { w: tw, h: th, bgra });
                    }
                }
                DeleteObject(hbmp);
            }
            (vtbl.release)(factory);
        }
        if init >= 0 {
            CoUninitialize();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_keeps_ratio_and_never_enlarges() {
        assert_eq!(fit(1920, 1080, 320), (320, 180));
        assert_eq!(fit(1080, 1920, 320), (180, 320));
        assert_eq!(fit(100, 50, 320), (100, 50));
        assert_eq!(fit(5000, 1, 320), (320, 1));
        assert_eq!(fit(0, 10, 320), (0, 0));
    }

    #[test]
    fn downscale_averages_and_composites_on_white() {
        // 2×1 RGB(검정·흰색) → 1×1 회색
        let out = downscale(&[0, 0, 0, 255, 255, 255], 2, 1, 3, 6, 1, 1).unwrap();
        assert_eq!(out, [127, 127, 127, 255]);
        // 완전 투명 빨강 → 흰색, 불투명 빨강 → BGRA 빨강
        assert_eq!(downscale(&[255, 0, 0, 0], 1, 1, 4, 4, 1, 1).unwrap(), [255, 255, 255, 255]);
        assert_eq!(downscale(&[255, 0, 0, 255], 1, 1, 4, 4, 1, 1).unwrap(), [0, 0, 255, 255]);
        // 줄 끝 여백(stride)이 있어도 맞게 읽는다.
        assert_eq!(downscale(&[9, 0, 7, 0], 1, 2, 1, 2, 1, 2).unwrap(), [9, 9, 9, 255, 7, 7, 7, 255]);
        assert!(downscale(&[0; 3], 2, 2, 3, 6, 1, 1).is_none(), "짧은 버퍼");
        assert!(downscale(&[0; 3], 1, 1, 5, 5, 1, 1).is_none(), "채널 수");
    }

    fn png_of(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut wr = enc.write_header().unwrap();
            wr.write_image_data(&vec![200u8; (w * h * 4) as usize]).unwrap();
        }
        out
    }

    #[test]
    fn png_thumbnail_roundtrip_and_speed() {
        let src = png_of(1920, 1080);
        let t0 = std::time::Instant::now();
        let t = from_png(&src, MAX_SIDE).unwrap();
        let ms = t0.elapsed().as_millis();
        assert_eq!((t.w, t.h), (320, 180));
        assert_eq!(t.bgra.len(), 320 * 180 * 4);
        let back = from_png(&to_png(&t).unwrap(), MAX_SIDE).unwrap();
        assert_eq!(back, t);
        // 디버그 빌드에서도 한도 안(목표 50ms는 릴리스 기준, 여기서는 큰 회귀만 잡는다).
        assert!(ms < 2000, "1920×1080 썸네일 {ms}ms");
        assert!(from_png(b"not png", MAX_SIDE).is_none());
    }

    #[test]
    fn file_thumbnails_png_direct_and_bmp_through_shell() {
        let dir = std::env::temp_dir().join(format!("kagent-thumb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("캡처 1.png");
        std::fs::write(&png, png_of(640, 360)).unwrap();
        let t = from_file(png.to_str().unwrap(), MAX_SIDE).unwrap();
        assert_eq!((t.w, t.h), (320, 180));
        // 24비트 BMP 400×200(파랑) — PNG가 아니므로 탐색기 썸네일 경로를 탄다.
        let (w, h) = (400u32, 200u32);
        let stride = (w * 3).div_ceil(4) * 4;
        let mut bmp = b"BM".to_vec();
        bmp.extend_from_slice(&(54 + stride * h).to_le_bytes());
        bmp.extend_from_slice(&[0, 0, 0, 0]);
        bmp.extend_from_slice(&54u32.to_le_bytes());
        for v in [40u32, w, h] {
            bmp.extend_from_slice(&v.to_le_bytes());
        }
        bmp.extend_from_slice(&1u16.to_le_bytes());
        bmp.extend_from_slice(&24u16.to_le_bytes());
        bmp.extend_from_slice(&[0u8; 24]);
        for _ in 0..h {
            let mut line: Vec<u8> = (0..w).flat_map(|_| [255u8, 0, 0]).collect();
            line.resize(stride as usize, 0);
            bmp.extend_from_slice(&line);
        }
        let bmp_path = dir.join("캡처 2.bmp");
        std::fs::write(&bmp_path, &bmp).unwrap();
        let t = from_file(bmp_path.to_str().unwrap(), MAX_SIDE).expect("탐색기 썸네일");
        assert!(t.w <= MAX_SIDE && t.h <= MAX_SIDE && t.w > t.h, "{}×{}", t.w, t.h);
        let mid = ((t.h / 2 * t.w + t.w / 2) * 4) as usize;
        assert!(t.bgra[mid] > 200 && t.bgra[mid + 2] < 60, "가운데는 파랑: {:?}", &t.bgra[mid..mid + 4]);
        assert!(from_file(dir.join("없음.jpg").to_str().unwrap(), MAX_SIDE).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mem_cache_is_lru_and_bounded_by_bytes() {
        let t = |n: usize| Arc::new(Thumb { w: 1, h: 1, bgra: vec![0; n] });
        let mut c = MemCache::new(10);
        c.put("a".into(), t(4));
        c.put("b".into(), t(4));
        assert!(c.get("a").is_some()); // a가 최근
        c.put("c".into(), t(4)); // 12 > 10 → 가장 오래된 b를 버린다
        assert!(c.contains("a") && c.contains("c") && !c.contains("b"));
        assert_eq!(c.bytes(), 8);
        c.put("a".into(), t(2)); // 같은 키 교체
        assert_eq!((c.len(), c.bytes()), (2, 6));
        c.put("big".into(), t(11)); // 상한보다 큰 것은 넣지 않음
        assert!(!c.contains("big"));
        c.clear();
        assert_eq!((c.len(), c.bytes()), (0, 0));
    }

    #[test]
    fn disk_evictions_remove_oldest_until_under_cap() {
        let files = vec![("new".into(), 5, 30), ("old".into(), 5, 10), ("mid".into(), 5, 20)];
        assert_eq!(disk_evictions(files.clone(), 10), ["old"]);
        assert_eq!(disk_evictions(files.clone(), 4), ["old", "mid", "new"]);
        assert!(disk_evictions(files, 15).is_empty());
    }

    #[test]
    fn image_paths_and_keys() {
        assert_eq!(first_image_path("C:\\a.txt\nC:\\b.JPG\nC:\\c.png".as_bytes()).as_deref(), Some("C:\\b.JPG"));
        assert_eq!(first_image_path(b"C:\\a.txt").as_deref(), None);
        assert_eq!(first_image_path("C:\\x\\y.png\0".as_bytes()).as_deref(), Some("C:\\x\\y.png"));
        assert_ne!(mem_key(1, 10), mem_key(1, 11));
        assert!(disk_name("image:1:abc").ends_with(".thumb"));
        assert!(!disk_name("C:\\secret\\name.png").contains("secret"));
    }
}
