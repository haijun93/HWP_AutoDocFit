//! WIC(Windows Imaging Component)로 그림 파일 썸네일을 만든다(KCAP3). 의존성 없이 운영체제 구성 요소만 쓴다.
//!
//! - JPEG처럼 축소 해독을 지원하는 형식은 `IWICBitmapSourceTransform`으로 1/2·1/4·1/8 크기로 바로 풀고
//!   (전체 해상도를 만들지 않음), 남은 비율만 `boxscale`로 줄인다.
//! - 그 밖(PNG·BMP·GIF 등)은 32비트 BGRA로 바꿔 줄 묶음(`STRIP`줄)씩 복사하며 `boxscale`로 줄인다.
//! - 회전 정보(EXIF 방향 ≠ 1)가 있는 사진은 여기서 만들지 않고 None — 호출 쪽이 탐색기 썸네일(방향 반영)로 간다.
//! COM 인터페이스는 `windows-sys`에 없어 필요한 메서드 표만 직접 적는다(`thumb::shell_thumbnail`과 같은 방식).

use std::ffi::c_void;
use windows_sys::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
use windows_sys::core::{GUID, HRESULT};

use crate::boxscale::{BoxScaler, Layout};

const CLSID_WIC_FACTORY: GUID = GUID::from_u128(0xcacaf262_9370_4615_a13b_9f5539da4c0a);
const IID_WIC_FACTORY: GUID = GUID::from_u128(0xec5ec8a9_c395_4314_9c77_54d7a935ff70);
const IID_SOURCE_TRANSFORM: GUID = GUID::from_u128(0x3b16811b_6a43_4ec9_b713_3d5a0c13b940);
const FMT_BGRA32: GUID = GUID::from_u128(0x6fddc324_4e03_4bfe_b185_3d77768dc90f);
const FMT_BGR32: GUID = GUID::from_u128(0x6fddc324_4e03_4bfe_b185_3d77768dc90e);
const FMT_BGR24: GUID = GUID::from_u128(0x6fddc324_4e03_4bfe_b185_3d77768dc90c);
const FMT_GRAY8: GUID = GUID::from_u128(0x6fddc324_4e03_4bfe_b185_3d77768dc908);
const GENERIC_READ: u32 = 0x8000_0000;
/// 줄 묶음 높이(4K 가로 BGRA 64줄 = 약 1MB).
const STRIP: u32 = 64;

type Ptr = *mut c_void;

#[repr(C)]
struct Unknown {
    query_interface: unsafe extern "system" fn(Ptr, *const GUID, *mut Ptr) -> HRESULT,
    add_ref: usize,
    release: unsafe extern "system" fn(Ptr) -> u32,
}

#[repr(C)]
struct FactoryVtbl {
    base: Unknown,
    create_decoder_from_filename: unsafe extern "system" fn(Ptr, *const u16, *const GUID, u32, i32, *mut Ptr) -> HRESULT,
    _from_stream: usize,
    _from_handle: usize,
    _component_info: usize,
    _decoder: usize,
    _encoder: usize,
    _palette: usize,
    create_format_converter: unsafe extern "system" fn(Ptr, *mut Ptr) -> HRESULT,
    create_bitmap_scaler: unsafe extern "system" fn(Ptr, *mut Ptr) -> HRESULT,
}

#[repr(C)]
struct DecoderVtbl {
    base: Unknown,
    _query_capability: usize,
    _initialize: usize,
    _container_format: usize,
    _decoder_info: usize,
    _copy_palette: usize,
    _metadata_reader: usize,
    _preview: usize,
    _color_contexts: usize,
    _thumbnail: usize,
    _frame_count: usize,
    get_frame: unsafe extern "system" fn(Ptr, u32, *mut Ptr) -> HRESULT,
}

#[repr(C)]
struct WicRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

/// IWICBitmapSource(+ IWICBitmapFrameDecode의 메타데이터 읽기, 변환기·축소기의 Initialize는 따로).
#[repr(C)]
struct SourceVtbl {
    base: Unknown,
    get_size: unsafe extern "system" fn(Ptr, *mut u32, *mut u32) -> HRESULT,
    _pixel_format: usize,
    _resolution: usize,
    _copy_palette: usize,
    copy_pixels: unsafe extern "system" fn(Ptr, *const WicRect, u32, u32, *mut u8) -> HRESULT,
    /// 프레임: GetMetadataQueryReader. 변환기·축소기: Initialize(형식이 달라 아래에서 바꿔 부른다).
    slot8: usize,
}

#[repr(C)]
struct TransformVtbl {
    base: Unknown,
    copy_pixels: unsafe extern "system" fn(Ptr, *const WicRect, u32, u32, *const GUID, i32, u32, u32, *mut u8) -> HRESULT,
    get_closest_size: unsafe extern "system" fn(Ptr, *mut u32, *mut u32) -> HRESULT,
    get_closest_pixel_format: unsafe extern "system" fn(Ptr, *mut GUID) -> HRESULT,
}

#[repr(C)]
struct QueryReaderVtbl {
    base: Unknown,
    _container_format: usize,
    _location: usize,
    get_metadata_by_name: unsafe extern "system" fn(Ptr, *const u16, *mut PropVariant) -> HRESULT,
}

/// PROPVARIANT(64비트 24바이트). 방향 값(VT_UI2)만 읽고, 다른 형식이면 PropVariantClear로 풀어 준다.
#[repr(C)]
struct PropVariant {
    vt: u16,
    _r: [u16; 3],
    val: [u64; 2],
}

#[link(name = "ole32")]
unsafe extern "system" {
    fn PropVariantClear(pv: *mut PropVariant) -> HRESULT;
}

/// COM 객체 소유(놓을 때 Release).
struct Com(Ptr);

impl Com {
    unsafe fn vtbl<T>(&self) -> &T {
        unsafe { &**(self.0 as *mut *const T) }
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { (self.vtbl::<Unknown>().release)(self.0) };
        }
    }
}

fn make(f: impl FnOnce(*mut Ptr) -> HRESULT) -> Option<Com> {
    let mut p: Ptr = std::ptr::null_mut();
    (f(&mut p) >= 0 && !p.is_null()).then_some(Com(p))
}

/// 축소 방식(측정용으로 고를 수 있게 둔다. 에이전트는 `Auto`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// 축소 해독이 되면 그것, 아니면 줄 묶음 박스 축소.
    Auto,
    /// 축소 해독을 쓰지 않고 줄 묶음 박스 축소만.
    #[allow(dead_code)]
    StripsOnly,
    /// WIC 축소기(Fant)로 바로 목표 크기 — 측정 비교용.
    #[allow(dead_code)]
    WicScaler,
}

/// 결과: (가로, 세로, BGRA). 만들 수 없거나 회전 정보가 있으면 None.
pub fn thumb_file(path: &str, max: u32, mode: Mode) -> Option<(u32, u32, Vec<u8>)> {
    ensure_com();
    unsafe { run(path, max, mode) }
}

/// 이 스레드의 COM을 한 번만 켜고 스레드가 끝날 때 끈다. 호출마다 켜고 끄면 마지막 해제 때 WIC·셸 DLL이
/// 내려가 다음 호출이 다시 올리느라 수십~수백 ms가 든다(KCAP3 측정). 이미 다른 방식으로 켜진 스레드면 그대로 쓴다.
pub fn ensure_com() {
    struct Guard(bool);
    impl Drop for Guard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }
    thread_local! { static COM: Guard = Guard(unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) } >= 0); }
    COM.with(|_| ());
}

unsafe fn run(path: &str, max: u32, mode: Mode) -> Option<(u32, u32, Vec<u8>)> {
    unsafe {
        let factory = make(|p| CoCreateInstance(&CLSID_WIC_FACTORY, std::ptr::null_mut(), CLSCTX_INPROC_SERVER, &IID_WIC_FACTORY, p))?;
        let fv = factory.vtbl::<FactoryVtbl>();
        let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        // WICDecodeMetadataCacheOnDemand = 0
        let decoder = make(|p| (fv.create_decoder_from_filename)(factory.0, wide.as_ptr(), std::ptr::null(), GENERIC_READ, 0, p))?;
        let frame = make(|p| (decoder.vtbl::<DecoderVtbl>().get_frame)(decoder.0, 0, p))?;
        let sv = frame.vtbl::<SourceVtbl>();
        let (mut sw, mut sh) = (0u32, 0u32);
        if (sv.get_size)(frame.0, &mut sw, &mut sh) < 0 || sw == 0 || sh == 0 {
            return None;
        }
        if sw > crate::clip::MAX_DIB_DIMENSION || sh > crate::clip::MAX_DIB_DIMENSION
            || (sw as usize).checked_mul(sh as usize)? > crate::clip::MAX_DIB_PIXELS
        {
            return None;
        }
        if orientation(&frame).is_some_and(|o| o != 1) {
            return None;
        }
        let (tw, th) = crate::thumb::fit(sw, sh, max);
        if mode == Mode::Auto {
            if let Some(t) = scaled_decode(&frame, sw, sh, tw, th) {
                return Some(t);
            }
        }
        // 32비트 BGRA로 바꾸는 변환기(이미 BGRA면 그대로 통과).
        let conv = make(|p| (fv.create_format_converter)(factory.0, p))?;
        let conv_init: unsafe extern "system" fn(Ptr, Ptr, *const GUID, i32, Ptr, f64, i32) -> HRESULT =
            std::mem::transmute(conv.vtbl::<SourceVtbl>().slot8);
        if conv_init(conv.0, frame.0, &FMT_BGRA32, 0, std::ptr::null_mut(), 0.0, 0) < 0 {
            return None;
        }
        if mode == Mode::WicScaler {
            let scaler = make(|p| (fv.create_bitmap_scaler)(factory.0, p))?;
            let sc_init: unsafe extern "system" fn(Ptr, Ptr, u32, u32, i32) -> HRESULT =
                std::mem::transmute(scaler.vtbl::<SourceVtbl>().slot8);
            // WICBitmapInterpolationModeFant = 3
            if sc_init(scaler.0, conv.0, tw, th, 3) < 0 {
                return None;
            }
            let mut px = vec![0u8; tw as usize * th as usize * 4];
            let rect = WicRect { x: 0, y: 0, w: tw as i32, h: th as i32 };
            if (scaler.vtbl::<SourceVtbl>().copy_pixels)(scaler.0, &rect, tw * 4, px.len() as u32, px.as_mut_ptr()) < 0 {
                return None;
            }
            // 흰 바탕 합성은 같은 박스 축소(1:1)로.
            return crate::boxscale::scale(&px, tw, th, tw as usize * 4, Layout::Bgra, false, tw, th).map(|b| (tw, th, b));
        }
        let copy = conv.vtbl::<SourceVtbl>().copy_pixels;
        let stride = sw as usize * 4;
        let mut strip = vec![0u8; stride * STRIP.min(sh) as usize];
        let mut scaler = BoxScaler::new(sw, sh, tw, th, Layout::Bgra)?;
        let mut y = 0;
        while y < sh {
            let n = STRIP.min(sh - y);
            let rect = WicRect { x: 0, y: y as i32, w: sw as i32, h: n as i32 };
            let len = stride * n as usize;
            if copy(conv.0, &rect, stride as u32, len as u32, strip.as_mut_ptr()) < 0 {
                return None;
            }
            for line in strip[..len].chunks_exact(stride) {
                scaler.push_row(line);
            }
            y += n;
        }
        scaler.finish().map(|b| (tw, th, b))
    }
}

/// 축소 해독(JPEG의 1/2·1/4·1/8 등): 목표보다 작아지지 않는 가장 작은 크기로 바로 푼다.
unsafe fn scaled_decode(frame: &Com, sw: u32, sh: u32, tw: u32, th: u32) -> Option<(u32, u32, Vec<u8>)> {
    unsafe {
        let tr = make(|p| (frame.vtbl::<Unknown>().query_interface)(frame.0, &IID_SOURCE_TRANSFORM, p))?;
        let tv = tr.vtbl::<TransformVtbl>();
        let mut best = None;
        for f in [8u32, 4, 2] {
            let (mut w, mut h) = (sw.div_ceil(f), sh.div_ceil(f));
            if w < tw || h < th {
                continue;
            }
            if (tv.get_closest_size)(tr.0, &mut w, &mut h) >= 0 && w >= tw && h >= th && w < sw {
                best = Some((w, h));
                break;
            }
        }
        let (w, h) = best?;
        let mut fmt = FMT_BGR24;
        if (tv.get_closest_pixel_format)(tr.0, &mut fmt) < 0 {
            return None;
        }
        let same = |a: &GUID, b: &GUID| (a.data1, a.data2, a.data3, a.data4) == (b.data1, b.data2, b.data3, b.data4);
        let layout = if same(&fmt, &FMT_BGR24) {
            Layout::Bgr
        } else if same(&fmt, &FMT_BGRA32) {
            Layout::Bgra
        } else if same(&fmt, &FMT_BGR32) {
            Layout::Bgrx
        } else if same(&fmt, &FMT_GRAY8) {
            Layout::Gray
        } else {
            return None;
        };
        let stride = (w as usize * layout.bytes()).div_ceil(4) * 4;
        let mut px = vec![0u8; stride * h as usize];
        // WICBitmapTransformRotate0 = 0, 사각형 NULL = 전체
        if (tv.copy_pixels)(tr.0, std::ptr::null(), w, h, &fmt, 0, stride as u32, px.len() as u32, px.as_mut_ptr()) < 0 {
            return None;
        }
        crate::boxscale::scale(&px, w, h, stride, layout, false, tw, th).map(|b| (tw, th, b))
    }
}

/// EXIF 방향(JPEG `/app1/ifd/{ushort=274}`). 없으면 None.
unsafe fn orientation(frame: &Com) -> Option<u16> {
    unsafe {
        let get_reader: unsafe extern "system" fn(Ptr, *mut Ptr) -> HRESULT = std::mem::transmute(frame.vtbl::<SourceVtbl>().slot8);
        let reader = make(|p| get_reader(frame.0, p))?;
        let name: Vec<u16> = "/app1/ifd/{ushort=274}".encode_utf16().chain(Some(0)).collect();
        let mut pv = PropVariant { vt: 0, _r: [0; 3], val: [0; 2] };
        if (reader.vtbl::<QueryReaderVtbl>().get_metadata_by_name)(reader.0, name.as_ptr(), &mut pv) < 0 {
            return None;
        }
        // VT_UI2 = 18
        let r = (pv.vt == 18).then_some(pv.val[0] as u16);
        PropVariantClear(&mut pv);
        r
    }
}
