use lopdf::{dictionary, Document, Object};
use pdf_edit_core::*;
use std::path::Path;

fn fixture(path: &Path) {
    let mut doc = Document::with_version("1.5");
    let root = doc.new_object_id();
    let branch = doc.new_object_id();
    let mut kids = Vec::new();
    for n in 1..=3 {
        let id =
            doc.add_object(dictionary! {"Type" => "Page", "Parent" => branch, "TestOrder" => n});
        kids.push(Object::Reference(id));
    }
    doc.objects.insert(branch, Object::Dictionary(dictionary! {"Type" => "Pages", "Parent" => root, "Kids" => kids, "Count" => 3, "Rotate" => 90, "MediaBox" => vec![0.into(),0.into(),100.into(),200.into()], "Resources" => dictionary!{}}));
    doc.objects.insert(root, Object::Dictionary(dictionary! {"Type" => "Pages", "Kids" => vec![Object::Reference(branch)], "Count" => 3}));
    let catalog = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => root});
    doc.trailer.set("Root", catalog);
    doc.save(path).unwrap();
}
#[test]
fn nested_page_tree_reorder_keeps_inherited_geometry_and_order() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let out = dir.path().join("out.pdf");
    fixture(&input);
    reorder(&input, &out, &[3, 1]).unwrap();
    let doc = Document::load(&out).unwrap();
    let pages = doc.get_pages();
    assert_eq!(pages.len(), 2);
    for (n, expected) in [(1, 3), (2, 1)] {
        let page = doc.get_object(pages[&n]).unwrap().as_dict().unwrap();
        assert_eq!(page.get(b"TestOrder").unwrap().as_i64().unwrap(), expected);
        assert_eq!(page.get(b"Rotate").unwrap().as_i64().unwrap(), 90);
        assert_eq!(page.get(b"MediaBox").unwrap().as_array().unwrap().len(), 4);
        assert!(page.has(b"Resources"));
    }
}
#[test]
fn output_never_overwrites_source_or_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    fixture(&input);
    let before = std::fs::read(&input).unwrap();
    assert!(reorder(&input, &input, &[1]).is_err());
    assert!(rotate_pages(&input, &input, &[1], 90).is_err());
    assert_eq!(before, std::fs::read(&input).unwrap());
    let out = dir.path().join("out.pdf");
    std::fs::write(&out, b"existing").unwrap();
    assert!(delete_pages(&input, &out, &[2]).is_err());
    assert_eq!(std::fs::read(&out).unwrap(), b"existing");
}
#[test]
fn rotation_uses_inherited_angle_and_handles_large_angles() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let out = dir.path().join("out.pdf");
    fixture(&input);
    rotate_pages(&input, &out, &[2], -90).unwrap();
    let doc = Document::load(out).unwrap();
    assert_eq!(
        doc.get_object(doc.get_pages()[&2])
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Rotate")
            .unwrap()
            .as_i64()
            .unwrap(),
        0
    );
}
#[test]
fn invalid_delete_and_split_have_no_partial_output() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    fixture(&input);
    let out = dir.path().join("out.pdf");
    for selection in [vec![0], vec![4], vec![1, 2, 3], vec![]] {
        assert!(delete_pages(&input, &out, &selection).is_err());
        assert!(!out.exists());
    }
    let parts = dir.path().join("parts");
    assert!(split(&input, &parts, &[1..=1, 3..=4]).is_err());
    assert!(!parts.exists());
    assert!(split(&input, &parts, &[3..=2]).is_err());
    assert!(!parts.exists());
}
#[test]
fn valid_delete_and_split_keep_expected_pages() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    fixture(&input);
    let out = dir.path().join("out.pdf");
    delete_pages(&input, &out, &[2]).unwrap();
    assert_eq!(page_count(out).unwrap(), 2);
    let paths = split(&input, dir.path().join("parts"), &[1..=2, 3..=3]).unwrap();
    assert_eq!(page_count(&paths[0]).unwrap(), 2);
    assert_eq!(page_count(&paths[1]).unwrap(), 1);
}
#[test]
fn signed_and_encrypted_documents_are_refused_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    let out = dir.path().join("out.pdf");
    fixture(&input);
    let mut doc = Document::load(&input).unwrap();
    doc.add_object(dictionary! {"FT" => "Sig"});
    doc.save(&input).unwrap();
    assert!(reorder(&input, &out, &[1]).is_err());
    assert!(!out.exists());
    fixture(&input);
    let mut doc = Document::load(&input).unwrap();
    let id = doc.add_object(dictionary! {"Filter" => "Standard", "V" => 1, "R" => 2});
    doc.trailer.set("Encrypt", id);
    doc.save(&input).unwrap();
    assert!(rotate_pages(&input, &out, &[1], 90).is_err());
    assert!(!out.exists());
}

/// 암호화(열기 암호 유무 모두)·앞에 군더더기가 있는 PDF 는 어떤 편집 경로로도 사본이 만들어지면 안 된다.
#[test]
fn protected_or_prefixed_pdfs_are_rejected_by_every_edit_path() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/classify");
    let dir = tempfile::tempdir().unwrap();
    let ok = fixtures.join("ok_single.pdf");
    let names = [
        "enc_rc440_nouserpw", "enc_rc4128_nouserpw", "enc_aes128_nouserpw", "enc_aes256_nouserpw",
        "enc_rc440_userpw", "enc_rc4128_userpw", "enc_aes128_userpw", "enc_aes256_userpw",
        "leading_junk",
    ];
    for name in names {
        let input = fixtures.join(format!("{name}.pdf"));
        let out = dir.path().join(format!("{name}.pdf"));
        assert!(pdf_edit_core::page_count(&input).is_err(), "{name}: count");
        assert!(pdf_edit_core::rotate_pages(&input, &out, &[1], 90).is_err(), "{name}: rotate");
        assert!(pdf_edit_core::pipeline::EditSession::open(&input).is_err(), "{name}: open");
        let raw = std::fs::read(&input).unwrap();
        assert!(pdf_edit_core::pipeline::EditSession::from_bytes(&raw).is_err(), "{name}: bytes");
        let status = std::process::Command::new(env!("CARGO_BIN_EXE_pdf-edit"))
            .arg("merge").arg(&out).arg(&ok).arg(&input)
            .status()
            .unwrap();
        assert!(!status.success(), "{name}: merge");
        assert!(!out.exists(), "{name}: output written");
    }
}

/// RUSTSEC-2026-0187: 깊게 중첩된 객체를 읽어도 스택 오버플로로 프로세스가 죽지 않아야 한다.
#[test]
fn deeply_nested_objects_do_not_overflow_the_stack() {
    let nested = "[".repeat(200_000) + &"]".repeat(200_000);
    let objects = [
        "<</Type/Catalog/Pages 2 0 R>>",
        "<</Type/Pages/Kids[3 0 R]/Count 1>>",
        "<</Type/Page/Parent 2 0 R/MediaBox[0 0 10 10]>>",
        nested.as_str(),
    ];
    let mut pdf = String::from("%PDF-1.5\n");
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf += &format!("{} 0 obj\n{body}\nendobj\n", i + 1);
    }
    let xref = pdf.len();
    pdf += "xref\n0 5\n0000000000 65535 f \n";
    for offset in offsets {
        pdf += &format!("{offset:010} 00000 n \n");
    }
    pdf += &format!("trailer\n<</Size 5/Root 1 0 R>>\nstartxref\n{xref}\n%%EOF\n");
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("deep.pdf");
    std::fs::write(&input, pdf).unwrap();
    assert_eq!(pdf_edit_core::page_count(&input).unwrap(), 1);
}
