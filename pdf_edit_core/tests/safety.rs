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
