//! 정확도: Python 전수 기준 구현(tools/oracle.py)이 만든 대조 표와 완전히 같은지.
//! 조각 하나·여러 조각·합친 뒤·증분 갱신·배경 합치기·읽는 쪽 세대 유지까지 같은 표로 확인한다.
mod common;
use common::*;
use kfind_core::{Index, MergePolicy, Writer, WriterOptions};
use std::collections::HashMap;

#[test]
fn single_segment_matches_python() {
    let dir = tmp_dir("single");
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for d in &corpus(1) {
        add(&mut w, d);
    }
    assert_eq!(w.commit().unwrap(), 1);
    let index = Index::open(&dir).unwrap();
    assert_eq!(index.num_segments(), 1);
    index.verify().unwrap();
    let n = check_all(&index, &expect(1), "single");
    assert_eq!(n, 162);
}

#[test]
fn many_segments_then_merged_match_python() {
    let dir = tmp_dir("many");
    let docs = corpus(1);
    let exp = expect(1);
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for chunk in docs.chunks(7) {
        for d in chunk {
            add(&mut w, d);
        }
        w.commit().unwrap();
    }
    assert_eq!(w.num_segments(), 20);
    let mut index = Index::open(&dir).unwrap();
    check_all(&index, &exp, "20 segments");

    assert!(w.merge(&MergePolicy::default()).unwrap());
    assert_eq!(w.num_segments(), 1);
    assert!(index.reload().unwrap());
    index.verify().unwrap();
    check_all(&index, &exp, "merged");
    assert!(!w.merge(&MergePolicy::default()).unwrap(), "nothing left to merge");
    assert!(!index.reload().unwrap());
}

#[test]
fn auto_commit_keeps_results() {
    let dir = tmp_dir("auto");
    let mut w = Writer::open(&dir, WriterOptions { max_pending_sentences: 60 }).unwrap();
    for d in &corpus(1) {
        add(&mut w, d);
    }
    w.commit().unwrap();
    assert!(w.num_segments() > 5, "auto commit made {} segments", w.num_segments());
    check_all(&Index::open(&dir).unwrap(), &expect(1), "auto commit");
}

/// 처음 상태 → 고친 뒤 상태로 가는 변경을 (삭제할 문서, 넣을 문서)로 낸다.
fn diff<'a>(from: &'a [Doc], to: &'a [Doc]) -> (Vec<u64>, Vec<&'a Doc>) {
    let old: HashMap<u64, &Doc> = from.iter().map(|d| (d.id, d)).collect();
    let new: HashMap<u64, &Doc> = to.iter().map(|d| (d.id, d)).collect();
    let deletes: Vec<u64> = from.iter().filter(|d| !new.contains_key(&d.id)).map(|d| d.id).collect();
    let same = |a: &Doc, b: &Doc| a.date == b.date && a.sents.len() == b.sents.len() && a.sents.iter().zip(&b.sents).all(|(x, y)| x.text == y.text && x.para == y.para && x.level == y.level);
    let upserts: Vec<&Doc> = to.iter().filter(|d| old.get(&d.id).is_none_or(|o| !same(o, d))).collect();
    (deletes, upserts)
}

#[test]
fn incremental_updates_with_background_merge_match_python() {
    let dir = tmp_dir("incremental");
    let (first, second) = (corpus(1), corpus(2));
    let (exp1, exp2) = (expect(1), expect(2));
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for chunk in first.chunks(50) {
        for d in chunk {
            add(&mut w, d);
        }
        w.commit().unwrap();
    }
    assert_eq!(w.num_segments(), 3);
    // 읽는 쪽 둘: 하나는 끝까지 처음 세대를 쥐고 있고, 하나는 따라간다.
    let pinned = Index::open(&dir).unwrap();
    let mut follower = Index::open(&dir).unwrap();
    let first_generation = pinned.generation();

    let (deletes, upserts) = diff(&first, &second);
    assert_eq!((deletes.len(), upserts.len()), (20, 44), "fixture: 20 deleted, 24 replaced + 20 new");
    let (del_a, del_b) = deletes.split_at(deletes.len() / 2);
    let (up_a, up_b) = upserts.split_at(upserts.len() / 2);

    // 앞 절반을 반영하고 커밋.
    for id in del_a {
        assert!(w.delete_document(*id).unwrap());
    }
    for d in up_a {
        add(&mut w, d);
    }
    w.commit().unwrap();
    assert!(!w.delete_document(del_a[0]).unwrap(), "already deleted");

    // 배경 합치기를 시작하고, 합치는 동안 뒤 절반을 반영한다(커밋된 것과 대기 중인 것 모두).
    let job = w.plan_merge_all().expect("merge job");
    assert!(w.plan_merge_all().is_none(), "only one merge at a time");
    let handle = std::thread::spawn(move || job.run());
    for id in del_b {
        assert!(w.delete_document(*id).unwrap());
    }
    let (up_b1, up_b2) = up_b.split_at(up_b.len() / 2);
    for d in up_b1 {
        add(&mut w, d);
    }
    w.commit().unwrap();
    for d in up_b2 {
        add(&mut w, d);
    }
    let merged = handle.join().expect("merge thread").expect("merge");
    w.apply_merge(merged).unwrap();

    // 처음 세대를 쥔 읽는 쪽은 조각이 합쳐지고 정리된 뒤에도 처음 상태 그대로 답한다.
    assert_eq!(pinned.generation(), first_generation);
    check_all(&pinned, &exp1, "pinned old generation");
    assert!(follower.reload().unwrap());
    check_all(&follower, &exp2, "after incremental + background merge");
    follower.verify().unwrap();

    // 한 번 더 합쳐 지운 문장을 걷어 내도 같다.
    assert!(w.merge_all().unwrap());
    assert_eq!(w.num_segments(), 1);
    assert!(follower.reload().unwrap());
    check_all(&follower, &exp2, "compacted");
    assert_eq!(follower.num_sentences(), second.iter().map(|d| d.sents.len() as u64).sum::<u64>());

    // 쓰는 쪽을 닫았다 다시 열어도(문서 표를 파일에서 다시 만든다) 이어서 고칠 수 있다.
    drop(w);
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for d in &second {
        assert!(w.delete_document(d.id).unwrap(), "document {} should exist", d.id);
    }
    w.commit().unwrap();
    assert_eq!(w.num_segments(), 0);
    assert!(follower.reload().unwrap());
    assert_eq!(follower.num_sentences(), 0);
    for d in &first {
        add(&mut w, d);
    }
    w.commit().unwrap();
    assert!(follower.reload().unwrap());
    check_all(&follower, &exp1, "rebuilt after deleting everything");

    // 처음부터 새로 만든 색인과도 같다.
    let fresh_dir = tmp_dir("incremental-fresh");
    let mut fresh = Writer::open(&fresh_dir, WriterOptions::default()).unwrap();
    for d in &second {
        add(&mut fresh, d);
    }
    fresh.commit().unwrap();
    check_all(&Index::open(&fresh_dir).unwrap(), &exp2, "fresh build of state 2");
}

#[test]
fn replacing_a_pending_document_keeps_only_the_last() {
    let dir = tmp_dir("pending");
    let docs = corpus(1);
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    // 같은 문서를 커밋 전에 두 번 넣고, 한 문서는 넣었다가 커밋 전에 지운다.
    add(&mut w, &Doc { id: docs[0].id, date: 1, sents: vec![Sent { para: 0, level: 1, text: "버려질 글 가나다라마".into() }] });
    for d in &docs {
        add(&mut w, d);
    }
    add(&mut w, &Doc { id: 777, date: 1, sents: vec![Sent { para: 0, level: 1, text: "가나다라마 임시".into() }] });
    assert!(w.delete_document(777).unwrap());
    w.commit().unwrap();
    let index = Index::open(&dir).unwrap();
    check_all(&index, &expect(1), "pending replace");
    assert_eq!(index.search("가나다라마", &opts(kfind_core::SplitMode::Off, 0)).unwrap().total_docs, 0);
}
