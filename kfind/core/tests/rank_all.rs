//! KB2: 전체 순위(`Index::rank`, 정수 하나로 묶은 정렬 + 미리보기는 필요한 문서만)가
//! `search`·Python 대조 표와 같은지. 조각 하나·여러 조각·지운 문서가 있는 세대에서 본다.
mod common;
use common::*;
use kfind_core::{Error, Index, Writer, WriterOptions};

#[test]
fn rank_matches_search_on_one_segment() {
    let dir = tmp_dir("rank-single");
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for d in &corpus(1) {
        add(&mut w, d);
    }
    w.commit().unwrap();
    assert_eq!(check_rank(&Index::open(&dir).unwrap(), &expect(1), "rank single"), 162);
}

#[test]
fn rank_matches_search_on_many_segments_with_deletions() {
    let dir = tmp_dir("rank-many");
    let (first, second) = (corpus(1), corpus(2));
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for chunk in first.chunks(7) {
        for d in chunk {
            add(&mut w, d);
        }
        w.commit().unwrap();
    }
    let index = Index::open(&dir).unwrap();
    check_rank(&index, &expect(1), "rank 20 segments");

    // 상태 2로 바꾼다: 지운 문서·고친 문서(옛 문장은 지운 문장 비트맵에 남는다)가 고정 순위표에 섞인다.
    let new_ids: std::collections::HashSet<u64> = second.iter().map(|d| d.id).collect();
    for d in &first {
        if !new_ids.contains(&d.id) {
            assert!(w.delete_document(d.id).unwrap());
        }
    }
    // 바뀐 문서·새 문서만 넣는다(그대로인 문서는 옛 조각에 남아 조각이 여럿이 된다).
    let old: std::collections::HashMap<u64, &Doc> = first.iter().map(|d| (d.id, d)).collect();
    let same = |a: &Doc, b: &Doc| a.date == b.date && a.sents.len() == b.sents.len() && a.sents.iter().zip(&b.sents).all(|(x, y)| x.text == y.text && x.para == y.para && x.level == y.level);
    for d in &second {
        if old.get(&d.id).is_none_or(|o| !same(o, d)) {
            add(&mut w, d);
        }
    }
    w.commit().unwrap();
    let mut index2 = Index::open(&dir).unwrap();
    assert!(index2.num_segments() > 1, "segments {}", index2.num_segments());
    check_rank(&index2, &expect(2), "rank with deletions");

    // 다른 세대의 색인으로 미리보기를 청하면 거부한다.
    let old = index.rank("예산", &opts(kfind_core::SplitMode::Off, 0)).unwrap();
    assert!(!old.result.hits.is_empty());
    assert!(matches!(old.sentences(&index2, 0, 2), Err(Error::Invalid(_))));
    assert!(old.sentences(&index, 0, 2).is_ok());

    w.merge_all().unwrap();
    assert!(index2.reload().unwrap());
    check_rank(&index2, &expect(2), "rank merged");
}
