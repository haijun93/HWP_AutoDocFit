//! 실패/미지원이 입력과 이미 존재하는 결과를 손상하지 않는지 확인한다.
use hwpx_fmt::{config::Config, process_json, survey_json, Processor};
use std::io::Write;

fn source(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("source.hwpx");
    let file = std::fs::File::create(&path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("mimetype", options).unwrap();
    zip.write_all(b"application/hwp+zip").unwrap();
    zip.start_file("Contents/header.xml", options).unwrap();
    zip.write_all(b"<head><refList/></head>").unwrap();
    zip.start_file("Contents/section0.xml", options).unwrap();
    zip.write_all("<sec><p><run><t>본문</t></run></p></sec>".as_bytes())
        .unwrap();
    zip.finish().unwrap();
    path
}

#[test]
fn unsupported_report_header_never_publishes_partial_output() {
    let dir = tempfile::tempdir().unwrap();
    let src = source(dir.path());
    let target = dir.path().join("target.hwpx");
    std::fs::write(&target, b"existing").unwrap();
    let original = std::fs::read(&src).unwrap();
    let err = process_json(
        Processor::ReportHeader,
        &src,
        &target,
        None,
        &Config::default(),
    )
    .unwrap_err();
    assert!(err.starts_with("미지원 입력:"));
    assert_eq!(std::fs::read(&src).unwrap(), original);
    assert_eq!(std::fs::read(&target).unwrap(), b"existing");
}

#[test]
fn no_profile_box_is_an_exact_package_copy() {
    let dir = tempfile::tempdir().unwrap();
    let src = source(dir.path());
    let target = dir.path().join("copy.hwpx");
    assert_eq!(
        process_json(Processor::BoxStyle, &src, &target, None, &Config::default()).unwrap(),
        0
    );
    assert_eq!(std::fs::read(src).unwrap(), std::fs::read(target).unwrap());
}

#[test]
fn same_file_is_rejected_before_noop_copy() {
    let dir = tempfile::tempdir().unwrap();
    let src = source(dir.path());
    let original = std::fs::read(&src).unwrap();
    assert!(process_json(Processor::Attachment, &src, &src, None, &Config::default()).is_err());
    assert_eq!(std::fs::read(src).unwrap(), original);
}

#[test]
fn roman_paragraph_is_explicitly_unsupported_instead_of_silently_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("roman.hwpx");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for (name, text) in [
        ("mimetype", "application/hwp+zip"),
        ("Contents/header.xml", "<head/>"),
        (
            "Contents/section0.xml",
            "<sec><p><run><t>로1 추진 계획</t></run></p></sec>",
        ),
    ] {
        zip.start_file(name, options).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    assert!(survey_json(Processor::Midtitle, &path, &Config::default())
        .unwrap_err()
        .starts_with("미지원 입력:"));
}
