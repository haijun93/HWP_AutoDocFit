//! 고정 시드 합성 문서에 대한 직접 검색 오라클 대조.
//! 오라클은 색인 자료구조를 사용하지 않고 모든 문장 원문을 순회한다.
mod common;
use common::tmp_dir;
use kfind_core::{Index, SearchOptions, Sentence, SplitMode, Writer, WriterOptions};
use roaring::RoaringTreemap;
use std::sync::Arc;

#[derive(Clone)]
struct Doc {
    id: u64,
    sentences: Vec<(u8, String)>,
}

// 외부 크레이트 없는 재현 가능한 xorshift 생성기.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize { (self.next() as usize) % n }
}

// 생성 코퍼스는 NFC 한글/ASCII만 사용한다. 글자·숫자 외 문자를 버려 공백과 문장부호를 무시한다.
fn plain(s: &str) -> String {
    s.chars().flat_map(char::to_lowercase).filter(|c| c.is_alphanumeric()).collect()
}

fn direct_docs(docs: &[Doc], groups: &[&str], denied: &str) -> Vec<u64> {
    if groups.is_empty() { return Vec::new(); }
    docs.iter().filter(|doc| {
        !doc.sentences.iter().any(|(_, s)| plain(s).contains(&plain(denied)))
            && groups.iter().all(|group| {
                let terms: Vec<String> = group.split_whitespace().map(plain).filter(|s| s.chars().count() >= 2).collect();
                !terms.is_empty() && doc.sentences.iter().any(|(_, s)| {
                    let sentence = plain(s);
                    terms.iter().all(|term| sentence.contains(term))
                })
            })
    }).map(|doc| doc.id).collect()
}

fn make_corpus() -> Vec<Doc> {
    let mut rng = Rng(0x51A7_2026_1119);
    let words = ["예산", "계획", "공사", "계약", "추진", "결과", "보류", "검토", "사업", "현장", "문서", "안내"];
    let particles = ["", "은", "을", "에서", "으로"];
    (1..=48).map(|id| {
        let sentences = (0..4).map(|_| {
            let n = 2 + rng.below(5);
            let words_in_sentence: Vec<String> = (0..n).map(|_| {
                let word = words[rng.below(words.len())];
                let particle = particles[rng.below(particles.len())];
                format!("{word}{particle}")
            }).collect();
            let separator = [" ", "  ", "\t", "　"][rng.below(4)];
            (rng.below(4) as u8, words_in_sentence.join(separator))
        }).collect();
        Doc { id, sentences }
    }).collect()
}

#[test]
fn group_search_matches_direct_scan_oracle() {
    let docs = make_corpus();
    let dir = tmp_dir("groups-oracle");
    let mut writer = Writer::create(&dir, WriterOptions::default()).expect("create index");
    for doc in &docs {
        let sentences: Vec<_> = doc.sentences.iter().enumerate().map(|(i, (level, text))| Sentence {
            para: i as u32, level: *level, text,
        }).collect();
        writer.add_document(doc.id, 20261119, &sentences).expect("add document");
    }
    writer.commit().expect("commit");
    let index = Index::open(&dir).expect("open");

    let mut rng = Rng(0xC0FF_EE11_2026_19);
    let words = ["예산", "계획", "공사", "계약", "추진", "결과", "보류", "검토", "사업", "현장", "문서", "안내"];
    let mut opts = SearchOptions { top: 0, split: SplitMode::Off, time_limit: None, ..SearchOptions::default() };
    // 제외어 문서 집합도 인덱스가 아닌 원문 순회로 만든다.
    let excluded: RoaringTreemap = docs.iter().filter(|d| d.sentences.iter().any(|(_, s)| plain(s).contains("보류"))).map(|d| d.id).collect();
    opts.deny = Some(Arc::new(excluded));

    let mut queries = vec!["공사; 계약", "계획; 예산", "보류; 검토", "예산; 계획; 공사", "현장; 문서; 안내"]
        .into_iter().map(str::to_string).collect::<Vec<_>>();
    for _ in 0..35 {
        let n = 1 + rng.below(3);
        let groups: Vec<String> = (0..n).map(|_| {
            let terms = 1 + rng.below(2);
            (0..terms).map(|_| words[rng.below(words.len())]).collect::<Vec<_>>().join(" ")
        }).collect();
        queries.push(groups.join(" ; "));
    }

    let mut discrepancies = Vec::new();
    for query in &queries {
        let groups: Vec<&str> = query.split(';').map(str::trim).filter(|g| !g.is_empty()).collect();
        let expected = direct_docs(&docs, &groups, "보류");
        let result = index.search_groups(query, &opts).expect("search_groups");
        let mut actual: Vec<u64> = result.hits.iter().map(|hit| hit.doc_id).collect();
        actual.sort_unstable();
        if actual != expected {
            discrepancies.push(format!("search_groups {query:?}: expected {expected:?}, got {actual:?}"));
        }

        let counts = index.count_groups(query, &opts).expect("count_groups");
        if counts.total_docs != result.total_docs {
            discrepancies.push(format!("count_groups {query:?}: expected {} docs {:?} (oracle/search_groups), got {}", result.total_docs, expected, counts.total_docs));
        }

        if groups.len() > 1 {
            let reversed = groups.iter().rev().copied().collect::<Vec<_>>().join(" ; ");
            let reversed_result = index.search_groups(&reversed, &opts).expect("reversed groups");
            let mut reversed_ids: Vec<u64> = reversed_result.hits.iter().map(|hit| hit.doc_id).collect();
            reversed_ids.sort_unstable();
            if reversed_ids != expected {
                discrepancies.push(format!("reversed groups {query:?}: expected {expected:?}, got {reversed_ids:?}"));
            }
        }
    }

    // 빈 묶음은 무시하고, 중복 묶음은 교집합 집합을 바꾸지 않는다.
    for (query, equivalent) in [(";; 공사 ; ; 계약 ;;", "공사;계약"), ("공사;계약;공사", "공사;계약")] {
        let mut a: Vec<_> = index.search_groups(query, &opts).unwrap().hits.iter().map(|h| h.doc_id).collect();
        let mut b: Vec<_> = index.search_groups(equivalent, &opts).unwrap().hits.iter().map(|h| h.doc_id).collect();
        a.sort_unstable(); b.sort_unstable();
        if a != b { discrepancies.push(format!("empty/duplicate groups {query:?}: got {a:?}, base {b:?}")); }
    }
    if !index.search_groups(";; ;", &opts).unwrap().hits.is_empty() {
        discrepancies.push("empty groups (;; ;) returned documents".to_string());
    }
    assert!(discrepancies.is_empty(), "property discrepancies:\n{}", discrepancies.join("\n"));
}
