// Copyright (c) 2026 haijun93 (마포구청 공혁준). PolyForm Strict License 1.0.0.
//! QMS-QUERY: 질의 문법 파서 및 검색 엔진 연동 통합 시험
//!
//! - 정확 구문 "…"
//! - 제외어 -낱말 및 -"…"
//! - 파일 형식 ext:hwp,pdf
//! - 폴더 범위 path:폴더
//! - 기간 검색 after:날짜, before:날짜
//! - 기존 ';' 묶음 검색과의 충돌 없음
//! - 파싱 오류 시 검색을 중단하지 않고 낱말로 취급
//! - SearchOptions(phrases, deny, file_types, folder, start_date, end_date) 연결

mod common;
use common::tmp_dir;
use kfind_core::{
    DocumentMeta, Index, SearchOptions, Sentence, Writer, WriterOptions,
};

fn setup_test_index(name: &str) -> Index {
    let dir = tmp_dir(name);
    let mut w = Writer::create(&dir, WriterOptions::default()).expect("create");

    // 문서 1: 2026-05-01, hwp, "기안/2026"
    let meta1 = DocumentMeta {
        path: Some("기안/2026/05_예산계획.hwp".into()),
        file_type: Some("hwp".into()),
    };
    w.add_document_with_meta(
        1,
        20260501,
        &meta1,
        &[
            Sentence { para: 0, level: 0, text: "2026년도 상반기 예산 집행 세부 계획" },
            Sentence { para: 1, level: 1, text: "공사 계약 추진 기본 지침 준수" },
        ],
    )
    .expect("add doc 1");

    // 문서 2: 2026-06-01, pdf, "보고서/2026"
    let meta2 = DocumentMeta {
        path: Some("보고서/2026/06_공사계약보고.pdf".into()),
        file_type: Some("pdf".into()),
    };
    w.add_document_with_meta(
        2,
        20260601,
        &meta2,
        &[
            Sentence { para: 0, level: 0, text: "2026년도 공사 계약 완료 보고서" },
            Sentence { para: 1, level: 1, text: "현장 감독 및 감리 결과 적합 판정" },
        ],
    )
    .expect("add doc 2");

    // 문서 3: 2026-07-01, hwp, "보고서/2026" (보류 포함)
    let meta3 = DocumentMeta {
        path: Some("보고서/2026/07_공사계약보류.hwp".into()),
        file_type: Some("hwp".into()),
    };
    w.add_document_with_meta(
        3,
        20260701,
        &meta3,
        &[
            Sentence { para: 0, level: 0, text: "2026년도 공사 계약 임시 보류 통보" },
            Sentence { para: 1, level: 1, text: "현장 사정으로 계약 집행 일시 중단" },
        ],
    )
    .expect("add doc 3");

    // 문서 4: 2025-12-15, docx, "안내/2025"
    let meta4 = DocumentMeta {
        path: Some("안내/2025/주민센터안내.docx".into()),
        file_type: Some("docx".into()),
    };
    w.add_document_with_meta(
        4,
        20251215,
        &meta4,
        &[
            Sentence { para: 0, level: 0, text: "주민 센터 리모델링 공사 계약 계획 안내" },
        ],
    )
    .expect("add doc 4");

    w.commit().expect("commit");
    Index::open(&dir).expect("open")
}

#[test]
fn wildcard_search_filters_candidates_exactly() {
    let index = setup_test_index("wildcard-search");
    let result = index.search_syntax("공사*", &SearchOptions::default()).expect("wildcard search");
    assert_eq!(result.total_docs, 3);
    assert_eq!(result.total_sentences, 4);

    let result = index.search_syntax("공?사", &SearchOptions::default()).expect("single-char wildcard");
    assert_eq!(result.total_docs, 3);
    assert_eq!(result.total_sentences, 4);
}

#[test]
fn test_query_syntax_exact_phrase_search() {
    let index = setup_test_index("syntax_phrase");

    // "공사 계약" 정확 구문 검색 (문서 1, 2, 3, 4 모두 공사 계약 포함)
    let res = index.search_syntax("\"공사 계약\"", &SearchOptions::default()).expect("search");
    assert_eq!(res.total_docs, 4);

    // "공사 계약 완료" (문서 2에만 연속 일치)
    let res2 = index.search_syntax("\"공사 계약 완료\"", &SearchOptions::default()).expect("search");
    assert_eq!(res2.total_docs, 1);
    assert_eq!(res2.hits[0].doc_id, 2);
}

#[test]
fn test_query_syntax_exclusion() {
    let index = setup_test_index("syntax_exclude");

    // "공사 계약" 중 "보류" 제외 (-보류) -> 문서 3 제외되어 1, 2, 4만 남음
    let res = index.search_syntax("\"공사 계약\" -보류", &SearchOptions::default()).expect("search");
    assert_eq!(res.total_docs, 3);
    let doc_ids: Vec<u64> = res.hits.iter().map(|h| h.doc_id).collect();
    assert!(doc_ids.contains(&1));
    assert!(doc_ids.contains(&2));
    assert!(doc_ids.contains(&4));
    assert!(!doc_ids.contains(&3), "보류가 포함된 문서 3은 제외되어야 함");

    // 제외 구문: -"임시 보류" -> 문서 3 제외
    let res2 = index.search_syntax("공사 -\"임시 보류\"", &SearchOptions::default()).expect("search");
    let doc_ids2: Vec<u64> = res2.hits.iter().map(|h| h.doc_id).collect();
    assert!(!doc_ids2.contains(&3));
}

#[test]
fn test_query_syntax_extension_filter() {
    let index = setup_test_index("syntax_ext");

    // ext:pdf -> 문서 2만 남아야 함
    let res = index.search_syntax("공사 계약 ext:pdf", &SearchOptions::default()).expect("search");
    assert_eq!(res.total_docs, 1);
    assert_eq!(res.hits[0].doc_id, 2);

    // ext:hwp,docx -> 문서 1, 3, 4
    let res2 = index.search_syntax("공사 계약 ext:hwp,docx", &SearchOptions::default()).expect("search");
    assert_eq!(res2.total_docs, 3);
    let doc_ids: Vec<u64> = res2.hits.iter().map(|h| h.doc_id).collect();
    assert!(doc_ids.contains(&1));
    assert!(doc_ids.contains(&3));
    assert!(doc_ids.contains(&4));
    assert!(!doc_ids.contains(&2));
}

#[test]
fn test_query_syntax_folder_scope() {
    let index = setup_test_index("syntax_folder");

    // path:보고서/2026 -> 문서 2, 3
    let res = index.search_syntax("공사 계약 path:보고서/2026", &SearchOptions::default()).expect("search");
    assert_eq!(res.total_docs, 2);
    let doc_ids: Vec<u64> = res.hits.iter().map(|h| h.doc_id).collect();
    assert!(doc_ids.contains(&2));
    assert!(doc_ids.contains(&3));
}

#[test]
fn test_query_syntax_date_filter() {
    let index = setup_test_index("syntax_date");

    // after:2026-06-15 -> 2026-07-01(문서 3)만 해당
    let res = index.search_syntax("공사 계약 after:2026-06-15", &SearchOptions::default()).expect("search");
    assert_eq!(res.total_docs, 1);
    assert_eq!(res.hits[0].doc_id, 3);

    // before:2026-01-01 -> 2025-12-15(문서 4)만 해당
    let res2 = index.search_syntax("공사 계약 before:2026-01-01", &SearchOptions::default()).expect("search");
    assert_eq!(res2.total_docs, 1);
    assert_eq!(res2.hits[0].doc_id, 4);

    // after:2026 before:2026 -> 2026년 전체 (문서 1, 2, 3)
    let res3 = index.search_syntax("공사 계약 after:2026 before:2026", &SearchOptions::default()).expect("search");
    assert_eq!(res3.total_docs, 3);
    let doc_ids: Vec<u64> = res3.hits.iter().map(|h| h.doc_id).collect();
    assert!(doc_ids.contains(&1));
    assert!(doc_ids.contains(&2));
    assert!(doc_ids.contains(&3));
    assert!(!doc_ids.contains(&4));
}

#[test]
fn test_query_syntax_semicolon_group_compatibility() {
    let index = setup_test_index("syntax_group");

    // 세미콜론 묶음 검색과 복합: "2026; 예산" -> 문서 1
    let res = index.search_syntax("2026; 예산 ext:hwp", &SearchOptions::default()).expect("search");
    assert_eq!(res.total_docs, 1);
    assert_eq!(res.hits[0].doc_id, 1);

    // 세미콜론 + 제외어: "2026; 공사 계약 -보류 ext:hwp" -> 문서 1만 남음 (문서 3 보류 제외, 문서 2 pdf 제외)
    let res2 = index.search_syntax("2026; 공사 계약 -보류 ext:hwp", &SearchOptions::default()).expect("search");
    assert_eq!(res2.total_docs, 1);
    assert_eq!(res2.hits[0].doc_id, 1);
}

#[test]
fn test_query_syntax_fault_tolerance_and_recovery() {
    let index = setup_test_index("syntax_fault");

    // 미완성 입력 및 잘못된 문법이 있어도 검색이 중단되지 않고(Err 없이) 일반 낱말로 취급되어 실행되어야 함
    let res = index.search_syntax("공사 ext: - after:invalid-date", &SearchOptions::default());
    assert!(res.is_ok(), "파싱 오류가 있어도 검색이 막히지(Err) 않고 정상 실행되어야 함");

    // 타이핑 중 미완성 따옴표: "예산 -> '예산' 낱말로 취급되어 문서 1 검색됨
    let res2 = index.search_syntax("\"예산", &SearchOptions::default()).expect("search");
    assert_eq!(res2.total_docs, 1);
    assert_eq!(res2.hits[0].doc_id, 1);

    // 2글자 미만 제외어는 무시되고 정상 검색됨
    let res3 = index.search_syntax("공사 계약 -a -및", &SearchOptions::default()).expect("search");
    assert_eq!(res3.total_docs, 4);
}
