//! Windows 그림 입출력(WIC): EXIF 방향을 편 해독, 사본 저장. 원본은 읽기만 하고, 저장은 새 이름에만 한다.
use crate::pixels::Image;
use std::path::{Path, PathBuf};
use windows::{
    Win32::{
        Foundation::{GENERIC_READ, GENERIC_WRITE},
        Graphics::Imaging::*,
        System::{
            Com::StructuredStorage::{IPropertyBag2, PROPBAG2, PROPVARIANT, PropVariantToUInt16},
            Com::*,
            Variant::{VARIANT, VT_R4},
        },
    },
    core::{GUID, PCWSTR, PWSTR},
};

/// 원본 해상도로 해독할 때의 상한(화소 4바이트 → 480MB).
const MAX_PIXELS: u64 = 120_000_000;

fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

/// 이 스레드의 COM 초기화. 이미 다른 방식으로 초기화됐으면 그대로 쓴다.
struct Com(bool);
impl Com {
    fn new() -> Com {
        Com(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok())
    }
}
impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() }
        }
    }
}

fn factory() -> windows::core::Result<IWICImagingFactory> {
    unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
}

/// 클라우드에만 있는 파일은 읽지 않는다(읽으면 내려받는다).
pub fn is_cloud_only(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    // OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS (`file_ops_windows` 와 같은 값).
    std::fs::metadata(path).is_ok_and(|m| m.file_attributes() & 0x0044_1000 != 0)
}

fn orientation(frame: &IWICBitmapFrameDecode) -> u16 {
    unsafe {
        let Ok(reader) = frame.GetMetadataQueryReader() else { return 1 };
        // JPEG, TIFF 순.
        for query in ["/app1/ifd/{ushort=274}", "/ifd/{ushort=274}"] {
            let name: Vec<u16> = query.encode_utf16().chain(Some(0)).collect();
            let mut value = PROPVARIANT::default();
            if reader.GetMetadataByName(PCWSTR(name.as_ptr()), &mut value).is_ok() {
                if let Ok(number) = PropVariantToUInt16(&value) {
                    return number;
                }
            }
        }
        1
    }
}

/// 첫 프레임을 해독해 EXIF 방향을 편다. `max_edge` 를 주면 긴 변이 그 이하가 되게 줄인다(키우지 않는다).
pub fn load(path: &Path, max_edge: Option<u32>) -> Result<Image, String> {
    if is_cloud_only(path) {
        return Err("클라우드에만 있는 파일은 읽지 않습니다".into());
    }
    let _com = Com::new();
    let name = wide(path);
    let decode = || -> windows::core::Result<Result<Image, String>> {
        unsafe {
            let factory = factory()?;
            let decoder = factory.CreateDecoderFromFilename(PCWSTR(name.as_ptr()), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand)?;
            let frame = decoder.GetFrame(0)?;
            let (mut width, mut height) = (0, 0);
            frame.GetSize(&mut width, &mut height)?;
            if width == 0 || height == 0 {
                return Ok(Err("크기가 없는 그림입니다".into()));
            }
            let turned = orientation(&frame);
            let converter = factory.CreateFormatConverter()?;
            converter.Initialize(&frame, &GUID_WICPixelFormat32bppBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)?;
            let longest = width.max(height);
            let source: IWICBitmapSource = match max_edge.filter(|edge| *edge > 0 && longest > *edge) {
                Some(edge) => {
                    width = ((width as u64 * edge as u64 + longest as u64 / 2) / longest as u64).max(1) as u32;
                    height = ((height as u64 * edge as u64 + longest as u64 / 2) / longest as u64).max(1) as u32;
                    let scaler = factory.CreateBitmapScaler()?;
                    scaler.Initialize(&converter, width, height, WICBitmapInterpolationModeFant)?;
                    scaler.into()
                }
                None => converter.into(),
            };
            if width as u64 * height as u64 > MAX_PIXELS {
                return Ok(Err(format!("그림이 너무 큽니다({width}x{height})")));
            }
            let mut pixels = vec![0u8; width as usize * height as usize * 4];
            source.CopyPixels(std::ptr::null(), width * 4, &mut pixels)?;
            Ok(Image::new(width, height, pixels).map(|image| image.oriented(turned)).ok_or_else(|| "그림을 읽지 못했습니다".into()))
        }
    };
    decode().map_err(|e| format!("그림을 열지 못했습니다: {}", e.message()))?
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Jpeg,
    Png,
    Bmp,
    Tiff,
}

impl Format {
    /// 원본 확장자로 정한 사본 형식과 확장자. Windows 에 저장 기능이 없는 형식(webp·heic)은 JPEG 로 쓴다.
    pub fn for_copy(extension: &str) -> (Format, &'static str) {
        match extension.to_ascii_lowercase().as_str() {
            "png" => (Format::Png, "png"),
            "bmp" => (Format::Bmp, "bmp"),
            "tif" => (Format::Tiff, "tif"),
            "tiff" => (Format::Tiff, "tiff"),
            "jpeg" => (Format::Jpeg, "jpeg"),
            _ => (Format::Jpeg, "jpg"),
        }
    }
}

/// `stem{suffix}.ext`, 있으면 `_1`, `_2`… 를 붙인 **새** 파일을 빈 채로 만들어 이름을 차지한다.
pub fn reserve(folder: &Path, stem: &str, suffix: &str, extension: &str) -> Result<PathBuf, String> {
    for n in 0..10_000 {
        let number = if n == 0 { String::new() } else { format!("_{n}") };
        let path = folder.join(format!("{stem}{suffix}{number}.{extension}"));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("사본을 만들지 못했습니다: {e}")),
        }
    }
    Err("같은 이름의 사본이 너무 많습니다".into())
}

fn encode(path: &Path, image: &Image, format: Format, quality: f32) -> windows::core::Result<()> {
    let _com = Com::new();
    let name = wide(path);
    unsafe {
        let factory = factory()?;
        let stream = factory.CreateStream()?;
        stream.InitializeFromFilename(PCWSTR(name.as_ptr()), GENERIC_WRITE.0)?;
        let container: GUID = match format {
            Format::Jpeg => GUID_ContainerFormatJpeg,
            Format::Png => GUID_ContainerFormatPng,
            Format::Bmp => GUID_ContainerFormatBmp,
            Format::Tiff => GUID_ContainerFormatTiff,
        };
        let encoder = factory.CreateEncoder(&container, std::ptr::null())?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
        let mut frame = None;
        let mut options: Option<IPropertyBag2> = None;
        encoder.CreateNewFrame(&mut frame, &mut options)?;
        let frame = frame.ok_or_else(windows::core::Error::empty)?;
        if let (Format::Jpeg, Some(bag)) = (format, options.as_ref()) {
            let mut label: Vec<u16> = "ImageQuality".encode_utf16().chain(Some(0)).collect();
            let property = PROPBAG2 { pstrName: PWSTR(label.as_mut_ptr()), ..Default::default() };
            let mut value = VARIANT::default();
            let inner = &mut *value.Anonymous.Anonymous;
            inner.vt = VT_R4;
            inner.Anonymous.fltVal = quality;
            bag.Write(1, &property, &value)?;
        }
        frame.Initialize(options.as_ref())?;
        frame.SetSize(image.width, image.height)?;
        let opaque = matches!(format, Format::Jpeg | Format::Bmp);
        let wanted = if opaque { GUID_WICPixelFormat24bppBGR } else { GUID_WICPixelFormat32bppBGRA };
        let mut actual = wanted;
        frame.SetPixelFormat(&mut actual)?;
        if actual != wanted {
            return Err(windows::core::Error::empty());
        }
        if opaque {
            let stride = (image.width * 3).next_multiple_of(4);
            let mut rows = vec![0u8; stride as usize * image.height as usize];
            for (row, source) in rows.chunks_exact_mut(stride as usize).zip(image.bgra.chunks_exact(image.width as usize * 4)) {
                for (to, from) in row.chunks_exact_mut(3).zip(source.chunks_exact(4)) {
                    to.copy_from_slice(&from[..3]);
                }
            }
            frame.WritePixels(image.height, stride, &rows)?;
        } else {
            frame.WritePixels(image.height, image.width * 4, &image.bgra)?;
        }
        frame.Commit()?;
        encoder.Commit()
    }
}

/// `reserve` 로 차지한 빈 파일에 저장한다. 실패하면 그 빈 파일을 치운다. `quality` 는 JPEG 에만(0~1).
pub fn save(reserved: &Path, image: &Image, format: Format, quality: f32) -> Result<(), String> {
    encode(reserved, image, format, quality).map_err(|e| {
        let _ = std::fs::remove_file(reserved);
        format!("사본을 저장하지 못했습니다: {}", e.message())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> Image {
        let bgra = (0..w * h).flat_map(|i| [(i % w) as u8, (i / w) as u8, 128, 255]).collect();
        Image::new(w, h, bgra).unwrap()
    }

    #[test]
    fn lossless_formats_round_trip_and_jpeg_stays_close() {
        let dir = tempfile::tempdir().unwrap();
        let image = gradient(61, 40);
        for (format, ext) in [(Format::Png, "png"), (Format::Bmp, "bmp"), (Format::Tiff, "tif"), (Format::Jpeg, "jpg")] {
            let path = reserve(dir.path(), "합성", "_docfit", ext).unwrap();
            save(&path, &image, format, 0.95).unwrap();
            let back = load(&path, None).unwrap();
            assert_eq!((back.width, back.height), (61, 40), "{ext}");
            if format == Format::Jpeg {
                let off: u64 = back.bgra.iter().zip(&image.bgra).map(|(a, b)| a.abs_diff(*b) as u64).sum();
                assert!(off / (image.bgra.len() as u64) < 6, "평균 오차 {off}");
            } else {
                assert_eq!(back.bgra, image.bgra, "{ext}");
            }
        }
    }

    #[test]
    fn long_edge_is_limited_without_enlarging() {
        let dir = tempfile::tempdir().unwrap();
        let path = reserve(dir.path(), "a", "", "png").unwrap();
        save(&path, &gradient(200, 100), Format::Png, 1.0).unwrap();
        let small = load(&path, Some(50)).unwrap();
        assert_eq!((small.width, small.height), (50, 25));
        let same = load(&path, Some(4000)).unwrap();
        assert_eq!((same.width, same.height), (200, 100));
    }

    #[test]
    fn exif_orientation_in_a_real_jpeg_is_applied() {
        let dir = tempfile::tempdir().unwrap();
        let path = reserve(dir.path(), "turned", "", "jpg").unwrap();
        save(&path, &gradient(64, 32), Format::Jpeg, 0.95).unwrap();
        let jpeg = std::fs::read(&path).unwrap();
        for (orientation, size) in [(1u8, (64, 32)), (3, (64, 32)), (6, (32, 64)), (8, (32, 64))] {
            // SOI 바로 뒤에 Orientation 한 항목짜리 Exif APP1 을 끼운다.
            let mut exif = b"Exif\0\0II*\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0".to_vec();
            exif.extend([orientation, 0, 0, 0, 0, 0, 0, 0]);
            let mut bytes = jpeg[..2].to_vec();
            bytes.extend([0xFF, 0xE1]);
            bytes.extend((exif.len() as u16 + 2).to_be_bytes());
            bytes.extend(&exif);
            bytes.extend(&jpeg[2..]);
            std::fs::write(&path, bytes).unwrap();
            let image = load(&path, None).unwrap();
            assert_eq!((image.width, image.height), size, "orientation {orientation}");
        }
    }

    #[test]
    fn reserve_never_reuses_an_existing_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a_edited.jpg"), b"original").unwrap();
        let next = reserve(dir.path(), "a", "_edited", "jpg").unwrap();
        assert_eq!(next.file_name().unwrap(), "a_edited_1.jpg");
        assert_eq!(std::fs::read(dir.path().join("a_edited.jpg")).unwrap(), b"original");
    }

    #[test]
    fn damaged_file_is_an_error_and_unsupported_formats_become_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.jpg");
        std::fs::write(&path, b"not a picture").unwrap();
        assert!(load(&path, Some(100)).is_err());
        assert_eq!(Format::for_copy("HEIC"), (Format::Jpeg, "jpg"));
        assert_eq!(Format::for_copy("webp"), (Format::Jpeg, "jpg"));
        assert_eq!(Format::for_copy("png"), (Format::Png, "png"));
    }
}
