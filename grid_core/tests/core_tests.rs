//! 공용 부품 직접 시험(문서뷰·사진뷰 시험과 별개): 격자 규칙 두 벌, PNG 인코더, 표시 글자, 줄 파서, 스캔 인자.
use grid_core::fmt::{blend, format_date, format_elapsed, format_size, group_digits};
use grid_core::items::{Collection, Doc, parse_scan_line};
use grid_core::layout::{GridSpec, Metrics};
use grid_core::png;
use grid_core::scan::{klib_args, parent_folder};

#[test]
fn grid_specs_match_flutter_values() {
    assert_eq!((GridSpec::DOC.gap, GridSpec::DOC.pad, GridSpec::DOC.aspect), (14, 16, 0.74));
    assert_eq!((GridSpec::PHOTO.gap, GridSpec::PHOTO.pad, GridSpec::PHOTO.aspect), (10, 16, 0.8));
    // Metrics::new 는 문서뷰 규칙과 같다.
    assert_eq!(Metrics::new(1100, 720, 96, 190.0, false), Metrics::with_spec(GridSpec::DOC, 1100, 720, 96, 190.0, false));
    assert_ne!(Metrics::with_spec(GridSpec::PHOTO, 1100, 720, 96, 190.0, false), Metrics::new(1100, 720, 96, 190.0, false));
}

#[test]
fn png_encoder_makes_valid_header_and_size() {
    let (w, h) = (5usize, 3usize);
    let px: Vec<u8> = (0..w * h * 4).map(|i| (i * 7) as u8).collect();
    let b = png::encode(w, h, &px);
    assert_eq!(&b[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&b[12..16], b"IHDR");
    assert_eq!(u32::from_be_bytes(b[16..20].try_into().unwrap()), 5);
    assert_eq!(u32::from_be_bytes(b[20..24].try_into().unwrap()), 3);
    assert_eq!(&b[b.len() - 8..b.len() - 4], b"IEND");
}

#[test]
fn format_helpers() {
    assert_eq!(format_size(1023), "1023 B");
    assert_eq!(format_date(86_400.0 * 365.0, 0), "1971-01-01");
    assert_eq!(format_date(0.0, -1), "1969-12-31");
    assert_eq!(group_digits(1234567), "1,234,567");
    assert_eq!(group_digits(12), "12");
    assert_eq!(format_elapsed(94), "1:34");
    assert_eq!(format_elapsed(3725), "1:02:05");
    assert_eq!(blend(0x000000, 0xFFFFFF, 0.5), 0x808080);
}

#[test]
fn scan_line_parser_and_args() {
    let d = parse_scan_line(r#"{"path":"D:\\x.jpg","name":"x.jpg","ext":"JPG","size":5,"mtime":2,"mtime_ns":2,"cloud":false}"#).unwrap();
    assert_eq!((d.ext.as_str(), d.size), ("jpg", 5));
    assert!(parse_scan_line("{}").is_err());
    assert_eq!(parent_folder(r"D:\a\b.jpg"), r"D:\a");
    assert_eq!(parent_folder("b.jpg"), "");
    let a = klib_args(&["a", "b"], &[r"C:\".into()], &[]);
    assert_eq!(a, ["scan", "--roots", r"C:\", "--ext", "a,b", "--json-lines"]);
}

#[test]
fn collection_sorts_by_key_once_and_is_stable() {
    let mk = |n: &str, m: f64| Doc { path: format!(r"D:\{n}"), name: n.into(), ext: "jpg".into(), size: 1, mtime: m, cloud: false };
    let mut c = Collection::new(vec![mk("b.jpg", 1.0), mk("a.jpg", 1.0), mk("c.jpg", 2.0)]);
    assert_eq!(c.get(0).unwrap().name, "a.jpg");
    c.set_sort(grid_core::items::SortKey::Modified, false);
    assert_eq!(c.get(0).unwrap().name, "a.jpg", "같은 시각은 경로 순");
    c.toggle_direction();
    assert_eq!(c.get(0).unwrap().name, "c.jpg");
}
