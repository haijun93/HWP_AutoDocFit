// Copyright (c) 2026 haijun93 (마포구청 공혁준). PolyForm Strict License 1.0.0.
//! 연관 검색어(';') 엔진 1단계 시험:
//! - 2묶음 교집합 (문서 단위 교집합, 순위, 미리보기)
//! - 따옴표 안 ';' (글자 취급, 묶음 분리 안 됨)
//! - 빈 묶음 무시 (연속 세미콜론, 양 끝 세미콜론)
//! - ';' 없을 때 기존 동작과 결과 차이 0
//! - 제외어(deny) 문서 전체 기준 적용
mod common;
use common::tmp_dir;
use kfind_core::{Index, SearchOptions, Sentence, SplitMode, Writer, WriterOptions, parse_groups};
use std::sync::Arc;

fn make_test_index(name: &str) -> Index {
    let dir = tmp_dir(name);
    let mut w = Writer::create(&dir, WriterOptions::default()).expect("create index");

    // 문서 1: 1번 문장에 "2026", 2번 문장에 "공사 계약" (서로 다른 문장에 존재)
    // 위계: 1번 문장 level 2, 2번 문장 level 1
    w.add_document(
        1,
        20260101,
        &[
            Sentence { para: 0, level: 2, text: "2026년도 사업 계획을 수립한다." },
            Sentence { para: 1, level: 1, text: "공사 계약 체결 지침을 준수한다." },
        ],
    )
    .expect("add doc 1");

    // 문서 2: 한 문장에 "2026 공사 계약" 모두 존재
    // 위계: level 3, 일치 문장 1개
    w.add_document(
        2,
        20260102,
        &[
            Sentence { para: 0, level: 3, text: "2026년 공사 계약 관련 예산 집행 안내." },
        ],
    )
    .expect("add doc 2");

    // 문서 3: "2026"만 포함 (공사 계약 없음)
    w.add_document(
        3,
        20260103,
        &[
            Sentence { para: 0, level: 0, text: "2026년도 정기 인사 발령." },
        ],
    )
    .expect("add doc 3");

    // 문서 4: "공사 계약"만 포함 (2026 없음)
    w.add_document(
        4,
        20260104,
        &[
            Sentence { para: 0, level: 1, text: "긴급 공사 계약 체결 공고." },
        ],
    )
    .expect("add doc 4");

    // 문서 5: "2026"과 "공사 계약" 모두 포함하되, 제외어 "보류" 포함
    w.add_document(
        5,
        20260105,
        &[
            Sentence { para: 0, level: 1, text: "2026년 공사 계약 건은 보류한다." },
        ],
    )
    .expect("add doc 5");

    w.commit().expect("commit index");
    Index::open(&dir).expect("open index")
}

#[test]
fn parse_groups_rules() {
    // 1. 2묶음 나누기
    assert_eq!(parse_groups("2026; 공사 계약"), ["2026", "공사 계약"]);

    // 2. 따옴표 안 ';'는 글자
    assert_eq!(parse_groups("\"2026; 공사\"; 계약"), ["\"2026; 공사\"", "계약"]);
    assert_eq!(parse_groups("2026; \"공사; 계약\""), ["2026", "\"공사; 계약\""]);

    // 3. 빈 묶음 무시
    assert_eq!(parse_groups(";; 2026 ;; 공사 계약 ;;"), ["2026", "공사 계약"]);
    assert_eq!(parse_groups("; ; "), Vec::<&str>::new());

    // 4. ';' 없을 때
    assert_eq!(parse_groups("2026 공사 계약"), ["2026 공사 계약"]);
    assert_eq!(parse_groups(""), Vec::<&str>::new());
}

#[test]
fn two_groups_intersection_and_ranking() {
    let index = make_test_index("two_groups");
    let opts = SearchOptions {
        top: 20,
        split: SplitMode::DictLongest,
        refs_per_doc: 2,
        ..SearchOptions::default()
    };

    // 질의: "2026; 공사 계약"
    // 문서 1: 묶음 1("2026")은 1번 문장(level 2), 묶음 2("공사 계약")는 2번 문장(level 1) 일치 -> 교집합 포함!
    // 문서 2: 한 문장에 둘 다 일치(level 3) -> 교집합 포함!
    // 문서 3: "2026"만 있으므로 교집합 제외!
    // 문서 4: "공사 계약"만 있으므로 교집합 제외!
    // 문서 5: 둘 다 있으므로 교집합 포함!
    let res = index.search_groups("2026; 공사 계약", &opts).expect("search_groups");

    assert_eq!(res.total_docs, 3, "문서 1, 2, 5 세 문서만 교집합에 포함되어야 함");
    let doc_ids: Vec<u64> = res.hits.iter().map(|h| h.doc_id).collect();
    assert!(doc_ids.contains(&1));
    assert!(doc_ids.contains(&2));
    assert!(doc_ids.contains(&5));
    assert!(!doc_ids.contains(&3));
    assert!(!doc_ids.contains(&4));

    // 순위 확인: 묶음 중 최고 위계(작은 값) 우선
    // 문서 1의 최고 위계 = min(2, 1) = 1
    // 문서 5의 최고 위계 = min(1, 1) = 1
    // 문서 2의 최고 위계 = min(3, 3) = 3
    // 최고 위계가 1인 문서 1과 5가 문서 2(위계 3)보다 반드시 앞에 와야 함!
    let hit1 = res.hits.iter().find(|h| h.doc_id == 1).expect("hit 1");
    let hit2 = res.hits.iter().find(|h| h.doc_id == 2).expect("hit 2");
    let pos1 = res.hits.iter().position(|h| h.doc_id == 1).unwrap();
    let pos2 = res.hits.iter().position(|h| h.doc_id == 2).unwrap();
    assert!(pos1 < pos2, "위계 1인 문서 1이 위계 3인 문서 2보다 앞서야 함");
    assert_eq!(hit1.level, 1);
    assert_eq!(hit2.level, 3);

    // 일치 문장 수 합(sum_count) 확인:
    // 문서 1: 묶음 1(1개) + 묶음 2(1개) = 2개
    // 문서 2: 묶음 1(1개) + 묶음 2(1개) = 2개
    assert_eq!(hit1.count, 2);
    assert_eq!(hit2.count, 2);

    // 미리보기: 묶음마다 최고 문장 1개
    // 문서 1은 묶음 1의 1번 문장(level 2)과 묶음 2의 2번 문장(level 1) 총 2개의 문장이 담겨야 함
    assert_eq!(hit1.sentences.len(), 2);
    assert_eq!(hit1.sentences[0].ord, 0); // 묶음 1 최고 문장
    assert_eq!(hit1.sentences[1].ord, 1); // 묶음 2 최고 문장

    // count_groups도 동일한 결과를 내는지 확인
    let counts = index.count_groups("2026; 공사 계약", &opts).expect("count_groups");
    assert_eq!(counts.total_docs, res.total_docs);
    assert_eq!(counts.total_sentences, res.total_sentences);
}

#[test]
fn no_semicolon_identical_to_existing() {
    let index = make_test_index("no_semicolon");
    let opts = SearchOptions {
        top: 20,
        split: SplitMode::DictLongest,
        refs_per_doc: 2,
        ..SearchOptions::default()
    };

    let query = "2026 공사 계약";
    let single_res = index.search(query, &opts).expect("search");
    let group_res = index.search_groups(query, &opts).expect("search_groups");

    // 세미콜론이 없을 때 search와 search_groups는 결과 차이 0
    assert_eq!(group_res.total_docs, single_res.total_docs);
    assert_eq!(group_res.total_sentences, single_res.total_sentences);
    assert_eq!(group_res.adjacent_docs, single_res.adjacent_docs);
    assert_eq!(group_res.words, single_res.words);
    assert_eq!(group_res.dropped, single_res.dropped);
    assert_eq!(group_res.hits.len(), single_res.hits.len());

    for (gh, sh) in group_res.hits.iter().zip(&single_res.hits) {
        assert_eq!(gh.doc_id, sh.doc_id);
        assert_eq!(gh.level, sh.level);
        assert_eq!(gh.count, sh.count);
        assert_eq!(gh.date, sh.date);
        assert_eq!(gh.adjacent, sh.adjacent);
        assert_eq!(gh.sentences, sh.sentences);
    }

    // rank_groups와 rank도 일치
    let single_rank = index.rank(query, &opts).expect("rank");
    let group_rank = index.rank_groups(query, &opts).expect("rank_groups");
    assert_eq!(group_rank.result.total_docs, single_rank.result.total_docs);
    assert_eq!(group_rank.result.hits.len(), single_rank.result.hits.len());

    // count_groups와 count도 일치
    let single_count = index.count(query, &opts).expect("count");
    let group_count = index.count_groups(query, &opts).expect("count_groups");
    assert_eq!(group_count, single_count);
}

#[test]
fn empty_groups_handling() {
    let index = make_test_index("empty_groups");
    let opts = SearchOptions {
        top: 20,
        split: SplitMode::DictLongest,
        refs_per_doc: 2,
        ..SearchOptions::default()
    };

    // 빈 묶음이 섞여 있어도 "2026; 공사 계약"과 동일한 결과
    let base_res = index.search_groups("2026; 공사 계약", &opts).expect("base");
    let messy_res = index.search_groups(";; 2026 ; ; 공사 계약 ;;", &opts).expect("messy");

    assert_eq!(messy_res.total_docs, base_res.total_docs);
    assert_eq!(messy_res.total_sentences, base_res.total_sentences);
    assert_eq!(messy_res.hits.len(), base_res.hits.len());
    for (m, b) in messy_res.hits.iter().zip(&base_res.hits) {
        assert_eq!(m.doc_id, b.doc_id);
    }

    // 빈 묶음만 있는 경우
    let empty_res = index.search_groups("; ; ;", &opts).expect("empty");
    assert_eq!(empty_res.total_docs, 0);
    assert_eq!(empty_res.hits.len(), 0);
}

#[test]
fn exclude_words_whole_document() {
    let index = make_test_index("exclude");
    // "보류"를 제외어로 지정 (doc_set 이용)
    let deny = index.doc_set("보류").expect("doc_set");
    let opts = SearchOptions {
        top: 20,
        split: SplitMode::DictLongest,
        refs_per_doc: 2,
        deny: Some(Arc::new(deny)),
        ..SearchOptions::default()
    };

    // 문서 5는 "보류"를 포함하므로 결과에서 완전히 제외되어야 함
    let res = index.search_groups("2026; 공사 계약", &opts).expect("search_groups with deny");
    assert_eq!(res.total_docs, 2, "문서 5가 제외되어 문서 1과 문서 2만 남아야 함");
    let doc_ids: Vec<u64> = res.hits.iter().map(|h| h.doc_id).collect();
    assert!(doc_ids.contains(&1));
    assert!(doc_ids.contains(&2));
    assert!(!doc_ids.contains(&5), "제외어 '보류'를 포함한 문서 5는 제외되어야 함");
}
