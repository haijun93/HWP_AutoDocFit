//! 질의 이력 캐시(KB4): 켜도 결과가 캐시 없이 낸 것·Python 대조 표와 같고, 새 세대에서는 바뀐 조각만 다시
//! 판정하며, 좁히는 입력은 앞 결과 안에서만 판정하고, 멈춘 질의는 담지 않는지.
mod common;
use common::*;
use kfind_core::{Error, Index, QueryCacheOptions, SearchOptions, SplitMode, Writer, WriterOptions};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

fn cached(dir: &std::path::Path) -> Index {
    let mut i = Index::open(dir).unwrap();
    i.set_query_cache(Some(QueryCacheOptions::default()));
    i
}

/// 두 색인이 같은 질의에 같은 답(전체 순위·미리보기·개수·문장 목록)을 내는지.
fn same_answers(a: &Index, b: &Index, q: &str, mode: SplitMode, ctx: &str) {
    let o = opts(mode, 0);
    let (x, y) = (a.search(q, &o).unwrap(), b.search(q, &o).unwrap());
    assert_eq!((&x.words, x.total_sentences, x.total_docs, x.adjacent_docs, &x.hits), (&y.words, y.total_sentences, y.total_docs, y.adjacent_docs, &y.hits), "{ctx} {q:?} search");
    assert_eq!(a.count(q, &o).unwrap(), b.count(q, &o).unwrap(), "{ctx} {q:?} count");
    assert_eq!(a.match_sentences(q, &o).unwrap(), b.match_sentences(q, &o).unwrap(), "{ctx} {q:?} sentences");
    let t = opts(mode, 20);
    assert_eq!(a.search(q, &t).unwrap().hits, b.search(q, &t).unwrap().hits, "{ctx} {q:?} top 20");
}

#[test]
fn cached_index_matches_python_twice() {
    let dir = tmp_dir("qcache-oracle");
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for chunk in corpus(1).chunks(7) {
        for d in chunk {
            add(&mut w, d);
        }
        w.commit().unwrap();
    }
    let index = cached(&dir);
    let exp = expect(1);
    // 처음(채우기)과 두 번째(재사용) 모두 대조 표와 같다.
    check_all(&index, &exp, "cache fill");
    let filled = index.query_cache_stats().unwrap();
    check_all(&index, &exp, "cache reuse");
    check_rank(&index, &exp, "cache rank");
    check_count(&index, &exp, "cache count");
    let s = index.query_cache_stats().unwrap();
    assert!(s.reused > filled.reused, "second pass reused cached segments: {s:?}");
    assert!(s.queries > 0 && s.bytes > 0);
}

#[test]
fn new_generation_reevaluates_only_changed_segments() {
    let dir = tmp_dir("qcache-incremental");
    let (first, second) = (corpus(1), corpus(2));
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for chunk in first.chunks(25) {
        for d in chunk {
            add(&mut w, d);
        }
        w.commit().unwrap();
    }
    let n_segs = w.num_segments() as u64;
    let mut index = cached(&dir);
    let queries: Vec<String> = expect(2).iter().map(|e| e.query.clone()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    for q in &queries {
        index.search(q, &opts(SplitMode::DictLongest, 20)).unwrap();
    }
    // 문서 하나를 고친다 → 새 조각 하나 + 옛 조각의 지운 문장.
    let changed = second.iter().find(|d| first.iter().any(|f| f.id == d.id)).unwrap();
    add(&mut w, changed);
    w.commit().unwrap();
    assert!(index.reload().unwrap());
    let before = index.query_cache_stats().unwrap();
    let plain = Index::open(&dir).unwrap();
    for q in &queries {
        same_answers(&index, &plain, q, SplitMode::DictLongest, "after one change");
    }
    let after = index.query_cache_stats().unwrap();
    // 질의마다(같은 해석이면) 옛 조각은 재사용하고 새 조각 하나만 판정한다.
    assert!(after.reused - before.reused >= n_segs, "{before:?} → {after:?}");
    assert!(after.evaluated - before.evaluated < after.reused - before.reused, "{before:?} → {after:?}");

    // 나머지 변경·합치기를 거쳐도 캐시 없는 색인·대조 표와 같다. 합쳐져 빠진 조각의 판정은 버려진다.
    for d in &first {
        if !second.iter().any(|s| s.id == d.id) {
            w.delete_document(d.id).unwrap();
        }
    }
    let same = |a: &Doc, b: &Doc| a.date == b.date && a.sents.len() == b.sents.len() && a.sents.iter().zip(&b.sents).all(|(x, y)| x.text == y.text && x.para == y.para && x.level == y.level);
    for d in &second {
        if !first.iter().any(|f| f.id == d.id && same(f, d)) {
            add(&mut w, d);
        }
    }
    w.commit().unwrap();
    assert!(index.reload().unwrap());
    check_all(&index, &expect(2), "cached after all changes");
    assert!(w.num_segments() > 1);
    assert!(w.merge_all().unwrap());
    assert!(index.reload().unwrap());
    let s = index.query_cache_stats().unwrap();
    assert_eq!((s.queries, s.bytes), (0, 0), "merged segments dropped from the cache: {s:?}");
    check_all(&index, &expect(2), "cached after merge");
}

#[test]
fn typing_narrows_inside_previous_results() {
    let dir = tmp_dir("qcache-typing");
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for chunk in corpus(1).chunks(30) {
        for d in chunk {
            add(&mut w, d);
        }
        w.commit().unwrap();
    }
    let index = cached(&dir);
    let plain = Index::open(&dir).unwrap();
    // 대조 표의 검색어를 한 글자씩 쳐 나가고(좁히기), 지우고, 낱말을 더하는 입력 흐름.
    let mut typed: Vec<String> = Vec::new();
    for e in expect(1).iter().take(60) {
        let chars: Vec<char> = e.query.chars().collect();
        for n in 1..=chars.len() {
            typed.push(chars[..n].iter().collect());
        }
        typed.push(format!("{} 예산", e.query));
        typed.push(chars[..chars.len().div_ceil(2)].iter().collect());
    }
    for mode in [SplitMode::DictLongest, SplitMode::Off, SplitMode::Bigram] {
        for q in &typed {
            same_answers(&index, &plain, q, mode, &format!("{mode:?}"));
        }
    }
    let s = index.query_cache_stats().unwrap();
    assert!(s.narrowed > 0 && s.reused > 0 && s.evaluated > 0, "{s:?}");
}

#[test]
fn stopped_queries_store_nothing_and_limits_hold() {
    let dir = tmp_dir("qcache-stop");
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for d in &corpus(1) {
        add(&mut w, d);
    }
    w.commit().unwrap();
    let mut index = cached(&dir);
    let cancel = Arc::new(AtomicBool::new(true));
    let o = SearchOptions { cancel: Some(cancel), ..opts(SplitMode::DictLongest, 20) };
    let q = &expect(1)[0].query;
    assert!(matches!(index.search(q, &o), Err(Error::Cancelled)));
    assert_eq!(index.query_cache_stats().unwrap().queries, 0, "cancelled query stored nothing");

    // 아주 작은 상한: 결과는 그대로, 담은 양은 상한 안.
    index.set_query_cache(Some(QueryCacheOptions { max_bytes: 4096, max_queries: 3 }));
    let plain = Index::open(&dir).unwrap();
    for e in expect(1).iter().take(40) {
        same_answers(&index, &plain, &e.query, e.mode, "tiny cache");
        let s = index.query_cache_stats().unwrap();
        assert!(s.bytes <= 4096 && s.queries <= 3, "{s:?}");
    }
    assert!(index.query_cache_stats().unwrap().evicted > 0);
    index.set_query_cache(None);
    assert!(index.query_cache_stats().is_none());
}
