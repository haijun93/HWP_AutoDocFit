//! KB2: "개수만" 경로(`Index::count`)가 Python 대조 표의 문장 수·문서 수와 같은지.
//! 조각 하나·여러 조각·지운 문서가 있는 세대·합친 뒤에서 본다.
mod common;
use common::*;
use kfind_core::{Error, Index, SearchOptions, SplitMode, Writer, WriterOptions};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

#[test]
fn count_matches_python_on_one_segment() {
    let dir = tmp_dir("count-single");
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for d in &corpus(1) {
        add(&mut w, d);
    }
    w.commit().unwrap();
    assert_eq!(check_count(&Index::open(&dir).unwrap(), &expect(1), "count single"), 162);
}

#[test]
fn count_matches_python_with_deletions_and_after_merge() {
    let dir = tmp_dir("count-many");
    let (first, second) = (corpus(1), corpus(2));
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for chunk in first.chunks(7) {
        for d in chunk {
            add(&mut w, d);
        }
        w.commit().unwrap();
    }
    check_count(&Index::open(&dir).unwrap(), &expect(1), "count 20 segments");
    let new_ids: std::collections::HashSet<u64> = second.iter().map(|d| d.id).collect();
    for d in &first {
        if !new_ids.contains(&d.id) {
            assert!(w.delete_document(d.id).unwrap());
        }
    }
    let old: std::collections::HashMap<u64, &Doc> = first.iter().map(|d| (d.id, d)).collect();
    let same = |a: &Doc, b: &Doc| a.date == b.date && a.sents.len() == b.sents.len() && a.sents.iter().zip(&b.sents).all(|(x, y)| x.text == y.text && x.para == y.para && x.level == y.level);
    for d in &second {
        if old.get(&d.id).is_none_or(|o| !same(o, d)) {
            add(&mut w, d);
        }
    }
    w.commit().unwrap();
    let mut index = Index::open(&dir).unwrap();
    assert!(index.num_segments() > 1);
    check_count(&index, &expect(2), "count with deletions");
    w.merge_all().unwrap();
    assert!(index.reload().unwrap());
    check_count(&index, &expect(2), "count merged");
}

#[test]
fn count_obeys_cancel() {
    let dir = tmp_dir("count-cancel");
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for d in &corpus(1) {
        add(&mut w, d);
    }
    w.commit().unwrap();
    let index = Index::open(&dir).unwrap();
    let flag = Arc::new(AtomicBool::new(true));
    let o = SearchOptions { split: SplitMode::Off, cancel: Some(flag), ..SearchOptions::default() };
    assert!(matches!(index.count("예산", &o), Err(Error::Cancelled)));
}
