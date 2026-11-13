//! 문서 범위 거르기(KFC1, `SearchOptions::allow`·`deny`): Python 기준 구현(tools/oracle.py)이 만든 범위 대조 표
//! (tests/data/oracle_scope_*.tsv)와 `search`·`rank`·`count`·`match_sentences`가 모두 같은지 본다.
//! 조각 하나·여러 조각·지운 문장이 있는 증분 상태·질의 이력 캐시를 켠 상태를 같은 표로 확인한다.
mod common;
use common::*;
use kfind_core::{DocHit, Index, QueryCacheOptions, RoaringTreemap, SearchOptions, SplitMode, Writer, WriterOptions};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// tools/oracle.py의 NO_SUCH_DOC(어느 상태에도 없는 문서 ID).
const NO_SUCH_DOC: u64 = 999_999_999;

struct ScopeRow {
    mode: SplitMode,
    label: String,
    query: String,
    allow: String,
    deny: String,
    phrases: Vec<String>,
    excludes: Vec<String>,
    words: String,
    dropped: String,
    sentences: u64,
    docs: usize,
    adjacent_docs: usize,
    sid_hash: String,
    rank_hash: String,
    top: String,
}

fn hex_list(s: &str) -> Vec<String> {
    if s == "-" { Vec::new() } else { s.split('|').map(from_hex).collect() }
}

fn scope_rows(state: u32) -> Vec<ScopeRow> {
    rows(&format!("oracle_scope_{state}.tsv"))
        .into_iter()
        .map(|r| ScopeRow {
            mode: match r[0].as_str() {
                "off" => SplitMode::Off,
                "dict" => SplitMode::DictLongest,
                "bigram" => SplitMode::Bigram,
                other => panic!("mode {other}"),
            },
            label: format!("mode={} allow={} deny={} phrases={} excludes={}", r[0], r[2], r[3], r[4], r[5]),
            query: from_hex(&r[1]),
            allow: r[2].clone(),
            deny: r[3].clone(),
            phrases: hex_list(&r[4]),
            excludes: hex_list(&r[5]),
            words: r[6].clone(),
            dropped: r[7].clone(),
            sentences: r[8].parse().expect("sentences"),
            docs: r[9].parse().expect("docs"),
            adjacent_docs: r[10].parse().expect("adjacent"),
            sid_hash: r[11].clone(),
            rank_hash: r[12].clone(),
            top: r[13].clone(),
        })
        .collect()
}

/// 두 상태 문서 ID의 합집합(규칙을 적용할 전체).
fn universe() -> HashSet<u64> {
    corpus(1).iter().chain(corpus(2).iter()).map(|d| d.id).collect()
}

/// tools/oracle.py SCOPE_RULES와 같은 규칙.
fn scope_set(rule: &str, universe: &HashSet<u64>) -> Option<Arc<RoaringTreemap>> {
    let f: fn(u64) -> bool = match rule {
        "-" => return None,
        "mod3" => |d| d % 3 == 0,
        "mod4" => |d| d % 4 == 1,
        "big" => |d| d >= 1 << 40,
        "empty" => |_| false,
        "nosuch" => |d| d == NO_SUCH_DOC,
        other => panic!("scope rule {other}"),
    };
    Some(Arc::new(universe.iter().copied().chain([NO_SUCH_DOC]).filter(|&d| f(d)).collect()))
}

/// 표 한 줄의 질의 옵션. 제외어(KFC2)는 `Index::doc_set`으로 문서 집합을 만들어 제외 집합에 합친다.
fn row_opts(index: &Index, row: &ScopeRow, top: usize, universe: &HashSet<u64>) -> SearchOptions {
    let mut o = opts(row.mode, top);
    o.allow = scope_set(&row.allow, universe);
    o.deny = scope_set(&row.deny, universe);
    o.phrases = row.phrases.clone();
    if !row.excludes.is_empty() {
        let mut deny = o.deny.as_deref().cloned().unwrap_or_default();
        for x in &row.excludes {
            deny |= index.doc_set(x).expect("doc_set");
        }
        o.deny = Some(Arc::new(deny));
    }
    o
}

fn top_of(hits: &[DocHit]) -> String {
    let v: Vec<String> = hits.iter().take(20).map(|h| format!("{}:{}:{}:{}", h.doc_id, h.adjacent as u8, h.level, h.count)).collect();
    if v.is_empty() { "-".to_string() } else { v.join(";") }
}

fn rank_hash(hits: &[DocHit]) -> String {
    hash_u64s(hits.iter().flat_map(|h| [h.doc_id, h.adjacent as u64, h.level as u64, h.count as u64]))
}

/// 범위 대조 표의 모든 줄을 확인한다. 확인한 줄 수를 돌려준다.
fn check_scope(index: &Index, expected: &[ScopeRow], label: &str) -> usize {
    let universe = universe();
    let strip = |hits: &[DocHit]| -> Vec<(u64, bool, u8, u32, i64)> { hits.iter().map(|h| (h.doc_id, h.adjacent, h.level, h.count, h.date)).collect() };
    for e in expected {
        let ctx = format!("[{label}] {} query={:?}", e.label, e.query);
        let o0 = row_opts(index, e, 0, &universe);
        let full = index.search(&e.query, &o0).expect("search");
        let words: Vec<String> = full.words.iter().map(|w| match &w.pieces {
            Some(p) => format!("{}={}", w.text, p.join("+")),
            None => w.text.clone(),
        }).collect();
        assert_eq!(if words.is_empty() { "-".to_string() } else { words.join(" ") }, e.words, "{ctx}: words");
        assert_eq!(if full.dropped.is_empty() { "-".to_string() } else { full.dropped.join(" ") }, e.dropped, "{ctx}: dropped");
        assert_eq!((full.total_sentences, full.total_docs, full.hits.len(), full.adjacent_docs), (e.sentences, e.docs, e.docs, e.adjacent_docs), "{ctx}: totals");
        assert_eq!(rank_hash(&full.hits), e.rank_hash, "{ctx}: rank order");
        assert_eq!(top_of(&full.hits), e.top, "{ctx}: top 20 of full");

        let o20 = row_opts(index, e, 20, &universe);
        let top = index.search(&e.query, &o20).expect("search top");
        assert_eq!(top_of(&top.hits), e.top, "{ctx}: top 20");
        assert_eq!((top.total_docs, top.total_sentences, top.adjacent_docs), (e.docs, e.sentences, e.adjacent_docs), "{ctx}: totals with top");

        let sents = index.match_sentences(&e.query, &o0).expect("match_sentences");
        assert_eq!(sents.len() as u64, e.sentences, "{ctx}: sentence list");
        let sid = hash_u64s(sents.iter().flat_map(|m| [m.doc_id, m.sentence.ord as u64, m.sentence.para as u64, m.sentence.level as u64, m.sentence.adjacent as u64]));
        assert_eq!(sid, e.sid_hash, "{ctx}: sentence set");
        for h in top.hits.iter().take(5) {
            let own: Vec<_> = sents.iter().filter(|m| m.doc_id == h.doc_id).map(|m| m.sentence.clone()).take(2).collect();
            assert_eq!(h.sentences, own, "{ctx}: preview sentences of {}", h.doc_id);
        }

        let c = index.count(&e.query, &o20).expect("count");
        assert_eq!((c.total_sentences, c.total_docs), (e.sentences, e.docs), "{ctx}: count");

        for (top_n, o) in [(0usize, &o0), (20, &o20)] {
            let ranking = index.rank(&e.query, o).expect("rank");
            let base = index.search(&e.query, o).expect("search");
            assert_eq!(strip(&ranking.result.hits), strip(&base.hits), "{ctx} top={top_n}: rank order");
            for (i, h) in base.hits.iter().enumerate().take(10) {
                assert_eq!(ranking.sentences(index, i, 2).expect("sentences"), h.sentences, "{ctx} top={top_n}: preview {i}");
            }
        }
    }
    expected.len()
}

#[test]
fn scope_single_and_many_segments_match_python() {
    let docs = corpus(1);
    let exp = scope_rows(1);
    assert_eq!(exp.len(), 8 * 14 * 3 + 17 * 3, "fixture: 8 scopes x 14 queries x 3 modes + 17 phrase/exclude cases x 3 modes");

    let dir = tmp_dir("scope-single");
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for d in &docs {
        add(&mut w, d);
    }
    w.commit().unwrap();
    assert_eq!(check_scope(&Index::open(&dir).unwrap(), &exp, "single"), 387);

    let dir = tmp_dir("scope-many");
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for chunk in docs.chunks(7) {
        for d in chunk {
            add(&mut w, d);
        }
        w.commit().unwrap();
    }
    assert_eq!(w.num_segments(), 20);
    check_scope(&Index::open(&dir).unwrap(), &exp, "20 segments");
}

/// 지운 문장이 있는 증분 상태: 지운 문서의 ID가 허용 집합에 있어도 다시 나오지 않는다.
#[test]
fn scope_after_incremental_updates_matches_python() {
    let (first, second) = (corpus(1), corpus(2));
    let dir = tmp_dir("scope-incremental");
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for chunk in first.chunks(50) {
        for d in chunk {
            add(&mut w, d);
        }
        w.commit().unwrap();
    }
    let new: HashMap<u64, &Doc> = second.iter().map(|d| (d.id, d)).collect();
    for d in &first {
        if !new.contains_key(&d.id) {
            assert!(w.delete_document(d.id).unwrap());
        }
    }
    // 바뀐 문서·새 문서만 넣는다(그대로인 문서는 처음 조각에 남아 지운 문장 비트맵과 섞인다).
    let old: HashMap<u64, &Doc> = first.iter().map(|d| (d.id, d)).collect();
    let same = |a: &Doc, b: &Doc| a.date == b.date && a.sents.len() == b.sents.len() && a.sents.iter().zip(&b.sents).all(|(x, y)| x.text == y.text && x.para == y.para && x.level == y.level);
    for d in &second {
        if old.get(&d.id).is_none_or(|o| !same(o, d)) {
            add(&mut w, d);
        }
    }
    w.commit().unwrap();
    assert!(w.num_segments() > 1);
    let mut index = Index::open(&dir).unwrap();
    check_scope(&index, &scope_rows(2), "incremental");

    // 질의 이력 캐시를 켜고 범위 없이 먼저 물은 뒤에도(캐시는 범위와 무관한 판정을 담는다) 같다.
    index.set_query_cache(Some(QueryCacheOptions::default()));
    for e in scope_rows(2).iter().filter(|e| e.allow == "-" && e.deny == "nosuch") {
        index.search(&e.query, &opts(e.mode, 20)).unwrap();
    }
    check_scope(&index, &scope_rows(2), "incremental + query cache");
    assert!(index.query_cache_stats().unwrap().reused > 0, "cache was reused across scopes");

    assert!(w.merge_all().unwrap());
    assert!(index.reload().unwrap());
    check_scope(&index, &scope_rows(2), "merged + query cache");
}

/// 범위가 결과를 바꾸지 않는 경우: 허용 집합이 모든 문서면 거르지 않은 것과 같고, 빈 허용 집합은 빈 결과.
#[test]
fn scope_identity_and_empty() {
    let docs = corpus(1);
    let dir = tmp_dir("scope-identity");
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for d in &docs {
        add(&mut w, d);
    }
    w.commit().unwrap();
    let index = Index::open(&dir).unwrap();
    let everything: RoaringTreemap = docs.iter().map(|d| d.id).collect();
    for e in expect(1) {
        let plain = index.search(&e.query, &opts(e.mode, 0)).unwrap();
        let o = SearchOptions { allow: Some(Arc::new(everything.clone())), deny: Some(Arc::new(RoaringTreemap::new())), ..opts(e.mode, 0) };
        let scoped = index.search(&e.query, &o).unwrap();
        assert_eq!((scoped.total_docs, scoped.total_sentences, &scoped.hits), (plain.total_docs, plain.total_sentences, &plain.hits), "{:?}", e.query);
        let empty = SearchOptions { allow: Some(Arc::new(RoaringTreemap::new())), ..opts(e.mode, 0) };
        let none = index.search(&e.query, &empty).unwrap();
        assert_eq!((none.total_docs, none.total_sentences, none.hits.len()), (0, 0, 0));
        assert_eq!(none.words, plain.words, "words are resolved against the whole index");
        assert_eq!(index.count(&e.query, &empty).unwrap().total_docs, 0);
    }
}

/// 제외어 문서 집합(KFC2): 나누지 않은 붙어 있는 일치 문장이 있는 살아 있는 문서와 같다(지운 문서는 빠진다).
#[test]
fn doc_set_is_documents_with_an_adjacent_match() {
    let (first, second) = (corpus(1), corpus(2));
    let dir = tmp_dir("doc-set");
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for d in &first {
        add(&mut w, d);
    }
    w.commit().unwrap();
    let new: HashSet<u64> = second.iter().map(|d| d.id).collect();
    for d in first.iter().filter(|d| !new.contains(&d.id)) {
        w.delete_document(d.id).unwrap();
    }
    w.commit().unwrap();
    let index = Index::open(&dir).unwrap();
    for word in ["예산", "집행", "예산 집행", "예산집행계획", "주민센터", "BUDGET", "e-mail", "1.5배", "가나다라", "프로그램"] {
        let set = index.doc_set(word).unwrap();
        let o = SearchOptions { phrases: vec![word.to_string()], ..opts(SplitMode::DictLongest, 0) };
        let docs: RoaringTreemap = index.match_sentences("", &o).unwrap().iter().map(|m| m.doc_id).collect();
        assert_eq!(set, docs, "{word}");
        assert!(set.iter().all(|d| new.contains(&d)), "{word}: deleted documents are not in the set");
    }
    assert!(index.doc_set("및").is_err() && index.doc_set("…").is_err(), "shorter than 2 chars");
    // 구문 해석은 resolve_with에 드러난다.
    let o = SearchOptions { phrases: vec!["예산 집행".into(), "및".into()], ..opts(SplitMode::DictLongest, 0) };
    let parsed = index.resolve_with("예산집행", &o).unwrap();
    assert_eq!((parsed.words.len(), parsed.words[0].text.as_str(), parsed.words[0].pieces.is_none(), parsed.dropped.as_slice()), (1, "예산집행", true, ["및".to_string()].as_slice()));
}
