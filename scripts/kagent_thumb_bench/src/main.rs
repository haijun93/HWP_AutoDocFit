//! KCAP3 썸네일 후보 측정. 합성 그림만 쓴다(사용자 파일 사용 금지).
//!
//! 사용:
//!   kagent_thumb_bench gen <폴더>          — 합성 그림 묶음 만들기
//!   kagent_thumb_bench all <폴더>          — 후보마다 새 프로세스로 1회씩 측정해 표(마크다운) 출력
//!   kagent_thumb_bench run <후보> <파일> <출력폴더>  — (all이 부름) 한 후보·한 그림
//! 시간: 첫 호출(ms)과 이어서 4번의 중앙값(ms). 메모리: 프로세스 최대 작업 집합(PeakWorkingSetSize).
//! 화질: 같은 그림의 기준 출력(PNG·DIB=현재 방식 `cur`, JPEG=전체 해독+박스 `ref`)과의 PSNR(dB, ∞=완전히 같음).

#[path = "../../../kagent/src/boxscale.rs"]
mod boxscale;
#[path = "../../../kagent/src/wic.rs"]
mod wic;

/// wic.rs가 쓰는 에이전트 쪽 항목(같은 값).
mod clip {
    pub const MAX_DIB_DIMENSION: u32 = 20_000;
    pub const MAX_DIB_PIXELS: usize = 4096 * 4096;
}
mod thumb {
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
}

use boxscale::Layout;
use std::path::{Path, PathBuf};
use std::time::Instant;

const MAX: u32 = 320;

struct Thumb {
    w: u32,
    h: u32,
    bgra: Vec<u8>,
}

// ---------------- 합성 그림 ----------------

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn range(&mut self, n: u32) -> u32 {
        self.next() % n.max(1)
    }
}

/// 화면 캡처 같은 RGB: 평면 색 창틀·옆 목록·글자 줄(획·안티에일리어스)·단추·아이콘·그림 영역(25%).
fn ui_image(w: u32, h: u32, seed: u32) -> Vec<u8> {
    let s = (h as f32 / 1080.0).max(0.5);
    let px = |v: f32| (v * s) as u32;
    let mut img = vec![0u8; (w * h * 3) as usize];
    let mut rng = Rng(seed);
    let fill = |img: &mut Vec<u8>, x0: u32, y0: u32, x1: u32, y1: u32, c: [u8; 3]| {
        for y in y0.min(h)..y1.min(h) {
            for x in x0.min(w)..x1.min(w) {
                let o = ((y * w + x) * 3) as usize;
                img[o..o + 3].copy_from_slice(&c);
            }
        }
    };
    fill(&mut img, 0, 0, w, h, [243, 243, 243]);
    fill(&mut img, 0, 0, w, px(36.0), [32, 32, 32]); // 제목 막대
    fill(&mut img, 0, px(36.0), w, px(84.0), [251, 251, 251]); // 도구 막대
    let side = px(260.0);
    fill(&mut img, 0, px(84.0), side, h, [232, 234, 237]); // 옆 목록
    fill(&mut img, side + px(16.0), px(100.0), w - px(16.0), h - px(40.0), [255, 255, 255]); // 본문
    fill(&mut img, 0, h - px(28.0), w, h, [0, 120, 212]); // 상태 막대
    // 도구 막대 아이콘
    let mut x = px(12.0);
    while x + px(28.0) < w / 2 {
        let c = [rng.range(200) as u8 + 30, rng.range(200) as u8 + 30, rng.range(200) as u8 + 30];
        fill(&mut img, x, px(46.0), x + px(26.0), px(74.0), c);
        x += px(40.0);
    }
    // 글자 같은 획: 줄마다 단어, 단어마다 세로·가로 획(가장자리 회색 단계).
    let text = |img: &mut Vec<u8>, rng: &mut Rng, x0: u32, x1: u32, y0: u32, y1: u32, line_h: u32, ink: [u8; 3]| {
        let glyph = px(11.0).max(4);
        let mut y = y0;
        while y + line_h < y1 {
            let mut x = x0;
            let end = x0 + (x1 - x0) * (60 + rng.range(40)) / 100;
            while x + glyph < end {
                let word = 2 + rng.range(9);
                for _ in 0..word {
                    for _ in 0..(2 + rng.range(3)) {
                        let gx = x + rng.range(glyph);
                        let gy = y + rng.range(glyph);
                        let (len, horiz) = (2 + rng.range(glyph), rng.range(2) == 0);
                        for k in 0..len {
                            let (xx, yy) = if horiz { (gx + k, gy) } else { (gx, gy + k) };
                            for (dx, dy, a) in [(0, 0, 255u32), (1, 0, 110), (0, 1, 60)] {
                                let (xx, yy) = (xx + dx, yy + dy);
                                if xx < w && yy < h {
                                    let o = ((yy * w + xx) * 3) as usize;
                                    for c in 0..3 {
                                        let bg = img[o + c] as u32;
                                        img[o + c] = ((ink[c] as u32 * a + bg * (255 - a)) / 255) as u8;
                                    }
                                }
                            }
                        }
                    }
                    x += glyph + px(1.0);
                }
                x += glyph / 2 + px(4.0);
            }
            y += line_h;
        }
    };
    text(&mut img, &mut rng, px(16.0), side - px(16.0), px(100.0), h - px(40.0), px(32.0), [40, 40, 40]);
    let (bx0, bx1) = (side + px(40.0), w - px(40.0));
    text(&mut img, &mut rng, bx0, bx1, px(130.0), h / 2, px(26.0), [30, 30, 30]);
    // 단추
    for i in 0..4 {
        let x = bx0 + i * px(150.0);
        fill(&mut img, x, h / 2 + px(10.0), x + px(130.0), h / 2 + px(46.0), [0, 103, 192]);
    }
    // 그림 영역(사진처럼 부드러운 변화 + 잡음)
    let (gx0, gy0, gx1, gy1) = (bx0, h / 2 + px(70.0), bx0 + (bx1 - bx0) / 2, h - px(60.0));
    for y in gy0..gy1.min(h) {
        for x in gx0..gx1.min(w) {
            let o = ((y * w + x) * 3) as usize;
            let fx = (x - gx0) as f32 / (gx1 - gx0) as f32;
            let fy = (y - gy0) as f32 / (gy1 - gy0) as f32;
            let n = rng.range(24) as f32 - 12.0;
            img[o] = (60.0 + 150.0 * fx + 20.0 * (fy * 9.0).sin() + n).clamp(0.0, 255.0) as u8;
            img[o + 1] = (90.0 + 100.0 * fy + 30.0 * (fx * 7.0).cos() + n).clamp(0.0, 255.0) as u8;
            img[o + 2] = (140.0 + 80.0 * (1.0 - fx) + n).clamp(0.0, 255.0) as u8;
        }
    }
    text(&mut img, &mut rng, gx1 + px(30.0), bx1, gy0, gy1, px(26.0), [60, 60, 60]);
    img
}

/// 사진형 RGB: 부드러운 변화 + 결(잡음).
fn photo_image(w: u32, h: u32, seed: u32) -> Vec<u8> {
    let mut rng = Rng(seed);
    let mut img = vec![0u8; (w * h * 3) as usize];
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            let n = rng.range(30) as f32 - 15.0;
            let o = ((y * w + x) * 3) as usize;
            img[o] = (120.0 + 80.0 * (fx * 6.0 + fy * 2.0).sin() + n).clamp(0.0, 255.0) as u8;
            img[o + 1] = (110.0 + 70.0 * (fy * 5.0).cos() * (fx * 3.0).sin() + n).clamp(0.0, 255.0) as u8;
            img[o + 2] = (90.0 + 60.0 * ((fx - 0.5).powi(2) + (fy - 0.5).powi(2)).sqrt() * 4.0 + n).clamp(0.0, 255.0) as u8;
        }
    }
    img
}

fn encode_png(rgb: &[u8], w: u32, h: u32, rgba: bool, fast: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(if rgba { png::ColorType::Rgba } else { png::ColorType::Rgb });
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(if fast { png::Compression::Fast } else { png::Compression::Default });
    let mut wr = enc.write_header().unwrap();
    if rgba {
        let v: Vec<u8> = rgb.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        wr.write_image_data(&v).unwrap();
    } else {
        wr.write_image_data(rgb).unwrap();
    }
    drop(wr);
    out
}

/// 클립보드 CF_DIB처럼: BITMAPINFOHEADER + 32비트 BGRA, 아래 줄부터(알파 0 — 흔한 화면 캡처 DIB).
fn encode_dib(rgb: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut d = Vec::with_capacity(40 + (w * h * 4) as usize);
    for v in [40u32, w, h] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend_from_slice(&1u16.to_le_bytes());
    d.extend_from_slice(&32u16.to_le_bytes());
    d.extend_from_slice(&[0u8; 24]);
    for y in (0..h).rev() {
        for x in 0..w {
            let o = ((y * w + x) * 3) as usize;
            d.extend_from_slice(&[rgb[o + 2], rgb[o + 1], rgb[o], 0]);
        }
    }
    d
}

fn make_images(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    for (name, w, h) in [("1080p", 1920, 1080), ("1440p", 2560, 1440), ("4k", 3840, 2160)] {
        let rgb = ui_image(w, h, 0x1234_5678 ^ w);
        std::fs::write(dir.join(format!("ui_{name}.png")), encode_png(&rgb, w, h, false, false)).unwrap();
        std::fs::write(dir.join(format!("clip_{name}.png")), encode_png(&rgb, w, h, true, true)).unwrap();
        std::fs::write(dir.join(format!("clip_{name}.dib")), encode_dib(&rgb, w, h)).unwrap();
    }
    for (name, w, h) in [("1080p", 1920, 1080), ("12mp", 4000, 3000)] {
        let rgb = photo_image(w, h, 0x9e37_79b9 ^ w);
        let mut out = Vec::new();
        jpeg_encoder::Encoder::new(&mut out, 85).encode(&rgb, w as u16, h as u16, jpeg_encoder::ColorType::Rgb).unwrap();
        std::fs::write(dir.join(format!("photo_{name}.jpg")), out).unwrap();
    }
}

// ---------------- 후보 ----------------

/// 현재 방식(KCAP2까지 `thumb::from_png` + `thumb::downscale`)을 그대로 옮긴 것.
mod current {
    pub fn downscale(src: &[u8], sw: u32, sh: u32, channels: usize, stride: usize, tw: u32, th: u32) -> Option<Vec<u8>> {
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

    pub fn from_png(data: &[u8], max: u32) -> Option<super::Thumb> {
        let mut decoder = png::Decoder::new(data);
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().ok()?;
        let mut buf = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut buf).ok()?;
        let (tw, th) = super::thumb::fit(frame.width, frame.height, max);
        let bgra = downscale(&buf, frame.width, frame.height, frame.color_type.samples(), frame.line_size, tw, th)?;
        Some(super::Thumb { w: tw, h: th, bgra })
    }

    /// 현재 디스크 캐시 형식: RGB PNG(빠른 압축).
    pub fn to_png(t: &super::Thumb) -> Option<Vec<u8>> {
        let rgb: Vec<u8> = t.bgra.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0]]).collect();
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
}

fn layout_of(ct: png::ColorType) -> Option<Layout> {
    Some(match ct {
        png::ColorType::Grayscale => Layout::Gray,
        png::ColorType::GrayscaleAlpha => Layout::GrayA,
        png::ColorType::Rgb => Layout::Rgb,
        png::ColorType::Rgba => Layout::Rgba,
        _ => return None,
    })
}

/// png 크레이트 줄 단위 풀기 + 줄 단위 박스 축소(전체 해상도 버퍼 없음).
fn png_rows(data: &[u8]) -> Option<Thumb> {
    let mut decoder = png::Decoder::new(data);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    if reader.info().interlaced {
        return current::from_png(data, MAX);
    }
    let (w, h) = (reader.info().width, reader.info().height);
    let (ct, _) = reader.output_color_type();
    let (tw, th) = thumb::fit(w, h, MAX);
    let mut s = boxscale::BoxScaler::new(w, h, tw, th, layout_of(ct)?)?;
    while let Some(row) = reader.next_row().ok()? {
        s.push_row(row.data());
    }
    Some(Thumb { w: tw, h: th, bgra: s.finish()? })
}

/// zune-png 전체 풀기 + 박스 축소.
fn zune(data: &[u8]) -> Option<Thumb> {
    use zune_core::bit_depth::BitDepth;
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::DecoderOptions;
    let opts = DecoderOptions::default().png_set_strip_to_8bit(true).set_max_width(20_000).set_max_height(20_000);
    let mut dec = zune_png::PngDecoder::new_with_options(data, opts);
    let px = dec.decode_raw().ok()?;
    let (w, h) = dec.get_dimensions()?;
    if dec.get_depth()? != BitDepth::Eight {
        return None;
    }
    let layout = match dec.get_colorspace()? {
        ColorSpace::Luma => Layout::Gray,
        ColorSpace::LumaA => Layout::GrayA,
        ColorSpace::RGB => Layout::Rgb,
        ColorSpace::RGBA => Layout::Rgba,
        _ => return None,
    };
    let (w, h) = (w as u32, h as u32);
    let (tw, th) = thumb::fit(w, h, MAX);
    let bgra = boxscale::scale(&px, w, h, w as usize * layout.bytes(), layout, false, tw, th)?;
    Some(Thumb { w: tw, h: th, bgra })
}

/// zune-png 전체 풀기 + fast_image_resize(SIMD, 박스 필터). RGB/RGBA만(알파 합성 없음 — 측정용).
fn zune_fir(data: &[u8]) -> Option<Thumb> {
    use fast_image_resize as fr;
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::DecoderOptions;
    let opts = DecoderOptions::default().png_set_strip_to_8bit(true);
    let mut dec = zune_png::PngDecoder::new_with_options(data, opts);
    let px = dec.decode_raw().ok()?;
    let (w, h) = dec.get_dimensions()?;
    let (pt, ch) = match dec.get_colorspace()? {
        ColorSpace::RGB => (fr::PixelType::U8x3, 3),
        ColorSpace::RGBA => (fr::PixelType::U8x4, 4),
        _ => return None,
    };
    let (tw, th) = thumb::fit(w as u32, h as u32, MAX);
    let src = fr::images::Image::from_vec_u8(w as u32, h as u32, px, pt).ok()?;
    let mut dst = fr::images::Image::new(tw, th, pt);
    let mut rs = fr::Resizer::new();
    rs.resize(&src, &mut dst, &fr::ResizeOptions::new().resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::Box))).ok()?;
    let bgra = dst.buffer().chunks_exact(ch).flat_map(|p| [p[2], p[1], p[0], 255]).collect();
    Some(Thumb { w: tw, h: th, bgra })
}

/// jpeg-decoder 축소 해독(DCT 1/2·1/4·1/8) + 박스.
fn jpegdec(data: &[u8]) -> Option<Thumb> {
    let mut d = jpeg_decoder::Decoder::new(data);
    d.read_info().ok()?;
    let info = d.info()?;
    let (tw, th) = thumb::fit(info.width as u32, info.height as u32, MAX);
    let (w, h) = d.scale(tw as u16, th as u16).ok()?;
    let px = d.decode().ok()?;
    let info = d.info()?;
    let layout = match info.pixel_format { jpeg_decoder::PixelFormat::RGB24 => Layout::Rgb, jpeg_decoder::PixelFormat::L8 => Layout::Gray, _ => return None };
    let (w, h) = (w as u32, h as u32);
    let bgra = boxscale::scale(&px, w, h, w as usize * layout.bytes(), layout, false, tw, th)?;
    Some(Thumb { w: tw, h: th, bgra })
}

/// zune-jpeg 전체 해독 + 박스.
fn zunejpg(data: &[u8]) -> Option<Thumb> {
    let mut d = zune_jpeg::JpegDecoder::new(data);
    let px = d.decode().ok()?;
    let (w, h) = d.dimensions()?;
    let (w, h) = (w as u32, h as u32);
    let (tw, th) = thumb::fit(w, h, MAX);
    let bgra = boxscale::scale(&px, w, h, w as usize * 3, Layout::Rgb, false, tw, th)?;
    Some(Thumb { w: tw, h: th, bgra })
}

/// 클립보드 DIB에서 바로(인코드·디코드 없이).
fn dib_box(dib: &[u8]) -> Option<Thumb> {
    let u32_at = |o: usize| u32::from_le_bytes(dib[o..o + 4].try_into().unwrap());
    let (w, hh) = (u32_at(4), u32_at(8) as i32);
    let h = hh.unsigned_abs();
    let stride = (w as usize * 4).div_ceil(4) * 4;
    let (tw, th) = thumb::fit(w, h, MAX);
    // 알파가 전부 0이면 불투명(Bgrx). 측정용 DIB는 알파 0.
    let bgra = boxscale::scale(&dib[40..], w, h, stride, Layout::Bgrx, hh > 0, tw, th)?;
    Some(Thumb { w: tw, h: th, bgra })
}

/// 탐색기 썸네일(현재 코드와 같은 호출, flags만 바꿀 수 있게).
fn shell(path: &str, flags: i32) -> Option<Thumb> {
    use windows_sys::Win32::Foundation::SIZE;
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
    use windows_sys::Win32::UI::Shell::SHCreateItemFromParsingName;
    use windows_sys::core::{GUID, HRESULT};
    #[repr(C)]
    struct FactoryVtbl {
        qi: usize,
        add_ref: usize,
        release: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
        get_image: unsafe extern "system" fn(*mut std::ffi::c_void, SIZE, i32, *mut HBITMAP) -> HRESULT,
    }
    const IID_FACTORY: GUID = GUID::from_u128(0xbcc18b79_ba16_442f_80c4_8a59c30c463b);
    unsafe {
        let init = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        let mut factory: *mut std::ffi::c_void = std::ptr::null_mut();
        let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        let hr = SHCreateItemFromParsingName(wide.as_ptr(), std::ptr::null_mut(), &IID_FACTORY, &mut factory);
        let mut result = None;
        if hr >= 0 && !factory.is_null() {
            let vtbl = &**(factory as *mut *const FactoryVtbl);
            let mut hbmp: HBITMAP = std::ptr::null_mut();
            let size = SIZE { cx: MAX as i32, cy: MAX as i32 };
            if (vtbl.get_image)(factory, size, flags, &mut hbmp) >= 0 && !hbmp.is_null() {
                let mut bm: BITMAP = std::mem::zeroed();
                if GetObjectW(hbmp, std::mem::size_of::<BITMAP>() as i32, &mut bm as *mut _ as _) != 0 && bm.bmWidth > 0 {
                    let (w, h) = (bm.bmWidth as u32, bm.bmHeight as u32);
                    let mut bi: BITMAPINFO = std::mem::zeroed();
                    bi.bmiHeader = BITMAPINFOHEADER {
                        biSize: 40, biWidth: w as i32, biHeight: -(h as i32), biPlanes: 1, biBitCount: 32,
                        biCompression: BI_RGB, ..std::mem::zeroed()
                    };
                    let mut px = vec![0u8; (w * h * 4) as usize];
                    let dc = GetDC(std::ptr::null_mut());
                    let lines = GetDIBits(dc, hbmp, 0, h, px.as_mut_ptr() as _, &mut bi, DIB_RGB_COLORS);
                    ReleaseDC(std::ptr::null_mut(), dc);
                    if lines == h as i32 {
                        let rgba: Vec<u8> = px.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], 255]).collect();
                        let (tw, th) = thumb::fit(w, h, MAX);
                        result = current::downscale(&rgba, w, h, 4, w as usize * 4, tw, th).map(|b| Thumb { w: tw, h: th, bgra: b });
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

// ---------------- 디스크 캐시 형식 ----------------

fn protect(data: &[u8], un: bool) -> Option<Vec<u8>> {
    use windows_sys::Win32::Foundation::{HLOCAL, LocalFree};
    use windows_sys::Win32::Security::Cryptography::*;
    let blob = |d: &[u8]| CRYPT_INTEGER_BLOB { cbData: d.len() as u32, pbData: d.as_ptr() as *mut u8 };
    let input = blob(data);
    let entropy = blob(b"HwpAutoDocFit.kagent.v1");
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    let ok = unsafe {
        if un {
            CryptUnprotectData(&input, std::ptr::null_mut(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        } else {
            CryptProtectData(&input, std::ptr::null(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        }
    };
    (ok != 0).then(|| unsafe {
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData as HLOCAL);
        v
    })
}

fn raw_encode(t: &Thumb) -> Vec<u8> {
    let mut v = Vec::with_capacity(8 + (t.w * t.h * 3) as usize);
    v.extend_from_slice(&t.w.to_le_bytes());
    v.extend_from_slice(&t.h.to_le_bytes());
    v.extend(t.bgra.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]));
    v
}

fn raw_decode(v: &[u8]) -> Option<Thumb> {
    let w = u32::from_le_bytes(v.get(0..4)?.try_into().ok()?);
    let h = u32::from_le_bytes(v.get(4..8)?.try_into().ok()?);
    let px = v.get(8..8 + (w * h * 3) as usize)?;
    Some(Thumb { w, h, bgra: px.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect() })
}

fn disk_encode(fmt: &str, t: &Thumb) -> Vec<u8> {
    match fmt {
        "png" => current::to_png(t).unwrap(),
        "raw" => raw_encode(t),
        "lz4" => lz4_flex::compress_prepend_size(&raw_encode(t)),
        "qoi" => {
            let rgb: Vec<u8> = t.bgra.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0]]).collect();
            qoi::encode_to_vec(&rgb, t.w, t.h).unwrap()
        }
        _ => unreachable!(),
    }
}

fn disk_decode(fmt: &str, v: &[u8]) -> Option<Thumb> {
    match fmt {
        "png" => current::from_png(v, MAX),
        "raw" => raw_decode(v),
        "lz4" => raw_decode(&lz4_flex::decompress_size_prepended(v).ok()?),
        "qoi" => {
            let (h, px) = qoi::decode_to_vec(v).ok()?;
            Some(Thumb { w: h.width, h: h.height, bgra: px.chunks_exact(3).flat_map(|p| [p[2], p[1], p[0], 255]).collect() })
        }
        _ => None,
    }
}

// ---------------- 실행 ----------------

fn peak_kb() -> u64 {
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    unsafe {
        let mut c: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb);
        c.PeakWorkingSetSize as u64 / 1024
    }
}

fn from_tuple(t: Option<(u32, u32, Vec<u8>)>) -> Option<Thumb> {
    t.map(|(w, h, bgra)| Thumb { w, h, bgra })
}

fn run_one(cand: &str, file: &Path, out: &Path) {
    let p = file.to_str().unwrap().replace('/', "\\");
    // 에이전트처럼 스레드의 COM을 한 번 켜 두고 유지한다(호출마다 켜고 끄면 WIC·셸 DLL을 매번 다시 올린다).
    unsafe { windows_sys::Win32::System::Com::CoInitializeEx(std::ptr::null(), windows_sys::Win32::System::Com::COINIT_APARTMENTTHREADED as u32) };
    // 탐색기 캐시에 없는 새 사본(캐시만 조회가 '없음'일 때의 시간).
    let miss_copy = file.with_file_name(format!("miss_{}_{}", std::process::id(), file.file_name().unwrap().to_str().unwrap()));
    if cand == "shell_cache_miss" {
        std::fs::copy(file, &miss_copy).unwrap();
    }
    let mut times = Vec::new();
    let mut thumb = None;
    let mut extra = String::new();
    for _ in 0..5 {
        let t0 = Instant::now();
        let t: Option<Thumb> = match cand {
            "none" => {
                let _ = std::hint::black_box(std::fs::read(&p).unwrap());
                None
            }
            "cur" => current::from_png(&std::fs::read(&p).unwrap(), MAX),
            "png_rows" => png_rows(&std::fs::read(&p).unwrap()),
            "zune" => zune(&std::fs::read(&p).unwrap()),
            "zune_fir" => zune_fir(&std::fs::read(&p).unwrap()),
            "dib_box" => dib_box(&std::fs::read(&p).unwrap()),
            "jpegdec" => jpegdec(&std::fs::read(&p).unwrap()),
            "zunejpg" => zunejpg(&std::fs::read(&p).unwrap()),
            "ref" => from_tuple(wic::thumb_file(&p, MAX, wic::Mode::StripsOnly)),
            "wic_strips" => from_tuple(wic::thumb_file(&p, MAX, wic::Mode::StripsOnly)),
            "wic_auto" => from_tuple(wic::thumb_file(&p, MAX, wic::Mode::Auto)),
            "wic_scaler" => from_tuple(wic::thumb_file(&p, MAX, wic::Mode::WicScaler)),
            "shell" => shell(&p, 0x08),
            "shell_cache" => shell(&p, 0x08 | 0x10),
            "shell_cache_miss" => shell(&miss_copy.to_str().unwrap().replace('/', "\\"), 0x08 | 0x10),
            c if c.starts_with("disk_") => {
                // 입력 그림의 현재 방식 썸네일을 형식별로 봉인해 두고, 읽기(파일 읽기+DPAPI 풀기+해독)만 잰다.
                let fmt = &c[5..];
                let cache = out.join(format!("cache.{fmt}"));
                if !cache.exists() {
                    let t = current::from_png(&std::fs::read(&p).unwrap(), MAX).unwrap();
                    let te = Instant::now();
                    let sealed = protect(&disk_encode(fmt, &t), false).unwrap();
                    let enc_ms = te.elapsed().as_secs_f64() * 1000.0;
                    std::fs::write(&cache, &sealed).unwrap();
                    extra = format!(" write_ms={enc_ms:.2} bytes={}", sealed.len());
                }
                let t0 = Instant::now();
                let r = protect(&std::fs::read(&cache).unwrap(), true).and_then(|v| disk_decode(fmt, &v));
                times.push(t0.elapsed().as_secs_f64() * 1000.0);
                thumb = r.or(thumb);
                continue;
            }
            _ => panic!("후보 없음: {cand}"),
        };
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
        thumb = t.or(thumb);
    }
    let _ = std::fs::remove_file(&miss_copy);
    let first = times[0];
    let mut rest = times[1..].to_vec();
    rest.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med = rest[rest.len() / 2];
    let (w, h) = thumb.as_ref().map(|t| (t.w, t.h)).unwrap_or((0, 0));
    if let Some(t) = &thumb {
        let mut v = Vec::new();
        v.extend_from_slice(&t.w.to_le_bytes());
        v.extend_from_slice(&t.h.to_le_bytes());
        v.extend_from_slice(&t.bgra);
        std::fs::write(out.join(format!("{cand}__{}.thumb", file.file_name().unwrap().to_str().unwrap())), v).unwrap();
    }
    println!("RESULT {cand} {} {w} {h} {first:.2} {med:.2} {}{extra}", file.file_name().unwrap().to_str().unwrap(), peak_kb());
}

fn psnr(a: &[u8], b: &[u8]) -> Option<f64> {
    if a.len() != b.len() || a.len() < 8 || a[..8] != b[..8] {
        return None;
    }
    let (mut se, mut n) = (0f64, 0f64);
    for (pa, pb) in a[8..].chunks_exact(4).zip(b[8..].chunks_exact(4)) {
        for c in 0..3 {
            let d = pa[c] as f64 - pb[c] as f64;
            se += d * d;
            n += 1.0;
        }
    }
    Some(if se == 0.0 { f64::INFINITY } else { 10.0 * (255.0f64 * 255.0 / (se / n)).log10() })
}

fn all(dir: &Path) {
    let me = std::env::current_exe().unwrap();
    let out = dir.join("out");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let plan: Vec<(&str, Vec<&str>, &str)> = vec![
        ("ui_1080p.png", vec!["none", "cur", "png_rows", "zune", "zune_fir", "wic_strips", "wic_scaler", "shell", "shell_cache", "shell_cache_miss"], "cur"),
        ("ui_1440p.png", vec!["none", "cur", "png_rows", "zune", "zune_fir", "wic_strips", "wic_scaler", "shell", "shell_cache", "shell_cache_miss"], "cur"),
        ("ui_4k.png", vec!["none", "cur", "png_rows", "zune", "zune_fir", "wic_strips", "wic_scaler", "shell", "shell_cache", "shell_cache_miss"], "cur"),
        ("clip_1080p.png", vec!["none", "cur", "png_rows", "zune"], "cur"),
        ("clip_4k.png", vec!["none", "cur", "png_rows", "zune"], "cur"),
        ("clip_1080p.dib", vec!["none", "dib_box"], ""),
        ("clip_4k.dib", vec!["none", "dib_box"], ""),
        ("photo_1080p.jpg", vec!["none", "ref", "wic_auto", "jpegdec", "zunejpg", "wic_scaler", "shell", "shell_cache", "shell_cache_miss"], "ref"),
        ("photo_12mp.jpg", vec!["none", "ref", "wic_auto", "jpegdec", "zunejpg", "wic_scaler", "shell", "shell_cache", "shell_cache_miss"], "ref"),
        ("ui_1080p.png", vec!["disk_png", "disk_raw", "disk_lz4", "disk_qoi"], "cur"),
    ];
    println!("| 그림 | 후보 | 크기 | 첫 호출 ms | 중앙값 ms | 최대 작업 집합 MB | PSNR dB(기준) | 비고 |");
    println!("|---|---|---|---:|---:|---:|---:|---|");
    for (img, cands, reference) in plan {
        for cand in cands {
            let o = std::process::Command::new(&me).args(["run", cand, dir.join(img).to_str().unwrap(), out.to_str().unwrap()]).output().unwrap();
            let s = String::from_utf8_lossy(&o.stdout);
            let Some(line) = s.lines().find(|l| l.starts_with("RESULT")) else {
                println!("| {img} | {cand} | 실패 | | | | | {} |", String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or(""));
                continue;
            };
            let f: Vec<&str> = line.split_whitespace().collect();
            // DIB 기준은 같은 그림의 clip PNG 현재 방식.
            let ref_name = if img.ends_with(".dib") { format!("cur__{}.thumb", img.replace(".dib", ".png")) } else { format!("{reference}__{img}.thumb") };
            let q = std::fs::read(out.join(&ref_name)).ok().zip(std::fs::read(out.join(format!("{cand}__{img}.thumb"))).ok())
                .and_then(|(a, b)| psnr(&a, &b));
            let q = match q { Some(v) if v.is_infinite() => "∞".to_string(), Some(v) => format!("{v:.1}"), None => "-".into() };
            println!("| {img} | {cand} | {}×{} | {} | {} | {:.1} | {q} | {} |", f[3], f[4], f[5], f[6], f[7].parse::<f64>().unwrap() / 1024.0, f[8..].join(" "));
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("gen") => make_images(Path::new(&a[2])),
        Some("all") => all(Path::new(&a[2])),
        Some("run") => run_one(&a[2], &PathBuf::from(&a[3]), Path::new(&a[4])),
        _ => eprintln!("사용: gen|all <폴더> / run <후보> <파일> <출력>"),
    }
}
