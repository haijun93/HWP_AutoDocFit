//! 인메모리 다중 편집 파이프라인 시험: 임시 파일 체이닝 결과와 동일한지, 임시 파일이 없는지,
//! 실패 시 문서가 보존되는지, CLI 합치기와 쪽 순서가 같은지.
use lopdf::{dictionary, Document, Object};
use pdf_edit_core::pipeline::{EditOp, EditSession};
use pdf_edit_core::*;
use std::path::Path;

/// n쪽 문서. 각 쪽: TestOrder=번호, MediaBox 폭=100+번호. 모든 쪽은 부모 Pages 노드의
/// Rotate(90)·Resources 를 상속한다(상속 속성 보존 확인용).
fn fixture(path: &Path, n: usize, tag: i64) {
    let mut doc = Document::with_version("1.5");
    let root = doc.new_object_id();
    let mut kids = Vec::new();
    for i in 1..=n {
        let id = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => root, "TestOrder" => tag * 100 + i as i64,
            "MediaBox" => vec![0.into(), 0.into(), (100 + i as i64).into(), 200.into()]
        });
        kids.push(Object::Reference(id));
    }
    doc.objects.insert(root, Object::Dictionary(dictionary! {"Type" => "Pages", "Kids" => kids, "Count" => n as i64, "Rotate" => 90, "Resources" => dictionary!{}}));
    let catalog = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => root});
    doc.trailer.set("Root", catalog);
    doc.save(path).unwrap();
}

/// (TestOrder, 폭, 유효 Rotate) 목록.
fn summary(path: &Path) -> Vec<(i64, i64, i64)> {
    summary_doc(&Document::load(path).unwrap())
}
fn summary_doc(doc: &Document) -> Vec<(i64, i64, i64)> {
    let mut out = Vec::new();
    for (_, id) in doc.get_pages() {
        let mut cur = id;
        let (mut rot, mut mb) = (None, None);
        let order = doc.get_object(id).unwrap().as_dict().unwrap().get(b"TestOrder").unwrap().as_i64().unwrap();
        loop {
            let d = doc.get_object(cur).unwrap().as_dict().unwrap();
            if rot.is_none() { rot = d.get(b"Rotate").ok().and_then(|v| v.as_i64().ok()); }
            if mb.is_none() { mb = d.get(b"MediaBox").ok().and_then(|v| v.as_array().ok()).map(|a| a[2].as_i64().unwrap()); }
            match d.get(b"Parent").ok().and_then(|p| p.as_reference().ok()) { Some(p) => cur = p, None => break }
        }
        out.push((order, mb.unwrap_or(-1), rot.unwrap_or(0).rem_euclid(360)));
    }
    out
}

#[test]
fn in_memory_chain_equals_temp_file_chaining() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    fixture(&input, 5, 1);
    // 기존 방식: 단계마다 임시 파일.
    let (s1, s2, s3, s4) = (dir.path().join("s1.pdf"), dir.path().join("s2.pdf"), dir.path().join("s3.pdf"), dir.path().join("s4.pdf"));
    rotate_pages(&input, &s1, &[1, 3], 90).unwrap();
    delete_pages(&s1, &s2, &[2]).unwrap();
    reorder(&s2, &s3, &[4, 3, 1]).unwrap();
    rotate_pages(&s3, &s4, &[2], -90).unwrap();
    // 새 방식: 메모리에서 연달아 적용하고 한 번만 저장.
    let out = dir.path().join("mem.pdf");
    let mut session = EditSession::open(&input).unwrap();
    session
        .apply_all(&[
            EditOp::Rotate { pages: vec![1, 3], degrees: 90 },
            EditOp::Delete { pages: vec![2] },
            EditOp::Reorder { order: vec![4, 3, 1] },
            EditOp::Rotate { pages: vec![2], degrees: -90 },
        ])
        .unwrap();
    session.save(&out).unwrap();
    assert_eq!(summary(&out), summary(&s4));
    assert_eq!(summary(&out), vec![(105, 105, 90), (104, 104, 0), (101, 101, 180)]);
    assert_eq!(page_count(&out).unwrap(), 3);
}

#[test]
fn design_scenario_four_rotate_delete_reorder() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    fixture(&input, 3, 2);
    let mut s = EditSession::open(&input).unwrap();
    s.apply(&EditOp::Rotate { pages: vec![1], degrees: 90 }).unwrap();
    s.apply(&EditOp::Delete { pages: vec![2] }).unwrap();
    s.apply(&EditOp::Reorder { order: vec![2, 1] }).unwrap();
    let out = dir.path().join("out.pdf");
    s.save(&out).unwrap();
    assert_eq!(summary(&out), vec![(203, 103, 90), (201, 101, 180)]);
    let info = s.page_info().unwrap();
    assert_eq!(info.iter().map(|p| p.number).collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(info[1].rotation, 180);
}

#[test]
fn only_final_file_is_written_and_source_is_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    fixture(&input, 4, 3);
    let before = std::fs::read(&input).unwrap();
    let out = dir.path().join("out.pdf");
    let mut s = EditSession::open(&input).unwrap();
    s.apply_all(&[
        EditOp::Rotate { pages: vec![2], degrees: 180 },
        EditOp::Delete { pages: vec![1] },
        EditOp::Reorder { order: vec![3, 2, 1] },
    ])
    .unwrap();
    // 저장 전에는 디렉터리에 원본뿐(임시 파일 없음).
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    s.save(&out).unwrap();
    let mut names = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, vec!["in.pdf", "out.pdf"]);
    assert_eq!(std::fs::read(&input).unwrap(), before);
    // 기존 파일·원본 덮어쓰기 금지.
    assert!(s.save(&out).is_err());
    assert!(s.save(&input).is_err());
    assert_eq!(std::fs::read(&input).unwrap(), before);
}

#[test]
fn failed_operation_leaves_document_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    fixture(&input, 3, 4);
    let mut s = EditSession::open(&input).unwrap();
    let before = summary_doc(&Document::load_mem(&s.to_bytes().unwrap()).unwrap());
    let before_info = s.page_info().unwrap();
    for bad in [
        EditOp::Rotate { pages: vec![1, 9], degrees: 90 },
        EditOp::Rotate { pages: vec![1, 1], degrees: 90 },
        EditOp::Rotate { pages: vec![1], degrees: 45 },
        EditOp::Rotate { pages: vec![], degrees: 90 },
        EditOp::Delete { pages: vec![1, 2, 3] },
        EditOp::Delete { pages: vec![0] },
        EditOp::Reorder { order: vec![] },
        EditOp::Reorder { order: vec![1, 1] },
        EditOp::Reorder { order: vec![4] },
    ] {
        assert!(s.apply(&bad).is_err(), "{bad:?}");
    }
    // 직렬화 바이트는 저장할 때마다 xref 객체 번호가 달라지므로 문서 내용(쪽 순서·크기·회전)으로 비교한다.
    assert_eq!(summary_doc(&Document::load_mem(&s.to_bytes().unwrap()).unwrap()), before);
    assert_eq!(s.page_info().unwrap(), before_info);
    assert_eq!(s.page_count(), 3);
    // apply_all 은 실패 위치를 알려주고 앞 작업을 유지한다.
    let r = s.apply_all(&[
        EditOp::Rotate { pages: vec![1], degrees: 90 },
        EditOp::Delete { pages: vec![7] },
    ]);
    assert_eq!(r.unwrap_err().0, 1);
    assert_eq!(s.page_info().unwrap()[0].rotation, 180);
}

#[test]
fn from_bytes_applies_same_safety_rules() {
    assert!(EditSession::from_bytes(b"not a pdf").is_err());
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    fixture(&input, 2, 5);
    let raw = std::fs::read(&input).unwrap();
    let mut s = EditSession::from_bytes(&raw).unwrap();
    s.apply(&EditOp::Delete { pages: vec![1] }).unwrap();
    let bytes = s.to_bytes().unwrap();
    assert_eq!(EditSession::from_bytes(&bytes).unwrap().page_count(), 1);
}

#[test]
fn merge_matches_cli_merge_page_order_and_keeps_inherited_attributes() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.pdf"), dir.path().join("b.pdf"));
    fixture(&a, 2, 6);
    fixture(&b, 3, 7);
    let cli_out = dir.path().join("cli.pdf");
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_pdf-edit"))
        .args(["merge", cli_out.to_str().unwrap(), b.to_str().unwrap(), a.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success());
    let mem_out = dir.path().join("mem.pdf");
    let mut merged = EditSession::merge(vec![EditSession::open(&b).unwrap(), EditSession::open(&a).unwrap()]).unwrap();
    assert_eq!(merged.page_count(), 5);
    merged.save(&mem_out).unwrap();
    let orders = |p: &Path| summary(p).iter().map(|t| t.0).collect::<Vec<_>>();
    // 쪽 순서는 CLI 와 동일.
    assert_eq!(orders(&mem_out), orders(&cli_out));
    assert_eq!(orders(&mem_out), vec![701, 702, 703, 601, 602]);
    // 상속된 MediaBox/Rotate 는 인메모리 합치기에서 보존된다.
    assert_eq!(summary(&mem_out)[0], (701, 101, 90));
    // 합친 뒤 곧바로 이어서 편집할 수 있다(임시 파일 없음).
    merged.apply(&EditOp::Delete { pages: vec![1] }).unwrap();
    merged.apply(&EditOp::Rotate { pages: vec![1], degrees: 90 }).unwrap();
    assert_eq!(merged.page_info().unwrap()[0].rotation, 180);
    assert!(EditSession::merge(vec![EditSession::open(&a).unwrap()]).is_err());
}

#[test]
fn cli_merge_loses_inherited_geometry_documented_difference() {
    // 기존 CLI 합치기는 Parent 를 새 루트로 바꿀 때 상속 속성을 복사하지 않아 MediaBox 가 사라진다.
    // 이 시험은 그 현행 동작을 기록한다(수정 대상은 PDFWIN6 의 CLI 교체 단계).
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.pdf"), dir.path().join("b.pdf"));
    fixture(&a, 1, 8);
    fixture(&b, 1, 9);
    let cli_out = dir.path().join("cli.pdf");
    assert!(std::process::Command::new(env!("CARGO_BIN_EXE_pdf-edit"))
        .args(["merge", cli_out.to_str().unwrap(), a.to_str().unwrap(), b.to_str().unwrap()])
        .status().unwrap().success());
    // 쪽 자신의 MediaBox 는 있지만 상속하던 Rotate(90)는 사라진다.
    assert_eq!(summary(&cli_out), vec![(801, 101, 0), (901, 101, 0)]);
}
