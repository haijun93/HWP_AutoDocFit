//! MG2: 위계 순 상위 N(`Index::search_by`·`rank_by`, `Order::Hierarchy`)이 기본 순서 전체 결과를 위계로 안정 정렬한 뒤
//! 앞 N건을 자른 것과 같은지(미리보기 문장·합계 포함). 조각 하나·여러 조각·지운 문서에서 본다.
mod common;
use common::*;
use kfind_core::{DocHit, Index, Order, Writer, WriterOptions};

fn strip(hits: &[DocHit]) -> Vec<(u64, bool, u8, u32, i64, usize)> {
    hits.iter().map(|h| (h.doc_id, h.adjacent, h.level, h.count, h.date, h.sentences.len())).collect()
}

fn check(index: &Index, label: &str) -> usize {
    let mut checked = 0;
    for e in expect(1).iter().chain(expect(2).iter()) {
        let ctx = format!("[{label}] mode={} query={:?}", e.mode_name, e.query);
        let mut all = index.search(&e.query, &opts(e.mode, 0)).expect("search");
        all.hits.sort_by_key(|h| h.level);
        for top in [0usize, 1, 3, 20] {
            let got = index.search_by(&e.query, &opts(e.mode, top), Order::Hierarchy).expect("search_by");
            let want: Vec<_> = if top == 0 { all.hits.clone() } else { all.hits.iter().take(top).cloned().collect() };
            assert_eq!((got.total_docs, got.total_sentences, got.adjacent_docs), (all.total_docs, all.total_sentences, all.adjacent_docs), "{ctx} top={top}: totals");
            assert_eq!(strip(&got.hits), strip(&want), "{ctx} top={top}: order");
            assert_eq!(got.hits, want, "{ctx} top={top}: previews");
            let ranking = index.rank_by(&e.query, &opts(e.mode, top), Order::Hierarchy).expect("rank_by");
            for (i, h) in want.iter().enumerate() {
                assert_eq!(ranking.sentences(index, i, 2).expect("sentences"), h.sentences, "{ctx}: preview {i}");
            }
            // 기본 순서는 그대로.
            let rel = index.search_by(&e.query, &opts(e.mode, top), Order::Relevance).expect("relevance");
            assert_eq!(rel.hits, index.search(&e.query, &opts(e.mode, top)).unwrap().hits, "{ctx}: relevance unchanged");
            checked += 1;
        }
    }
    checked
}

#[test]
fn hierarchy_top_n_matches_stable_sort() {
    let dir = tmp_dir("hier-single");
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for d in &corpus(1) {
        add(&mut w, d);
    }
    w.commit().unwrap();
    assert!(check(&Index::open(&dir).unwrap(), "single") > 100);

    for d in corpus(1).iter().step_by(3) {
        assert!(w.delete_document(d.id).unwrap());
    }
    for chunk in corpus(2).chunks(11) {
        for d in chunk {
            add(&mut w, d);
        }
        w.commit().unwrap();
    }
    let index = Index::open(&dir).unwrap();
    assert!(index.num_segments() > 1);
    check(&index, "many segments with deletions");
}
