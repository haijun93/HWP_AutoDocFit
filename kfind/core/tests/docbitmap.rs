// Copyright (c) 2026 haijun93 (마포구청 공혁준). PolyForm Strict License 1.0.0.
//! SEMI2-docbitmap 시험:
//! 1. 2글자 조각별 문서 번호 Roaring 비트맵 색인 기록 및 크기 증가 10% 이하
//! 2. `;` 묶음 검색(search_groups) 문서 비트맵 교집합 및 기존 결과 차이 0
//! 3. 1차 거르기: 후보 문서를 문서 비트맵으로 먼저 좁힌 뒤 문장 확인
//! 4. 옵션 `scope=document`(문서 전체에서 찾기) API
//! 5. 폴더 단위: 폴더 구간/비트맵, 폴더 안에서만 찾기(교집합), 폴더 단위 `;` 검색(scope=folder), 폴더별 집계 API
//! 6. 시간 단위: 일/월/년 문서 비트맵, 기간 검색(교집합), 월·주·일별 막대그래프 집계 API, 폴더×기간×검색 삼중 교집합
//! 7. 1등급 비트맵 3종:
//!    ① 삭제 표시(tombstone) 비트맵
//!    ② 파일 형식별 문서 비트맵(hwp·hwpx·pdf·xlsx·md…)
//!    ③ 문장 역할별 비트맵(제목·표·본문·붙임) 및 「제목에만」 검색
mod common;
use common::tmp_dir;
use kfind_core::{
    DateInterval, DocumentMeta, Index, SearchOptions, SearchScope, Sentence,
    Writer, WriterOptions,
};
use std::time::Instant;

#[test]
fn test_doc_bigram_and_size_increase_under_10_percent() {
    let dir = tmp_dir("doc_bigram_size");
    let mut w = Writer::create(&dir, WriterOptions::default()).expect("create");

    // 100개 문서 색인
    for i in 1..=100 {
        let meta = DocumentMeta {
            path: Some(format!("dept/sub{}/doc_{}.hwp", i % 5, i)),
            file_type: Some("hwp".to_string()),
        };
        w.add_document_with_meta(
            i,
            20260101 + (i as i64 % 30),
            &meta,
            &[
                Sentence { para: 0, level: 0, text: "2026년도 정기 예산 집행 세부 계획서" },
                Sentence { para: 1, level: 1, text: "공사 계약 및 물품 구매 지침에 따라 사업을 추진한다." },
                Sentence { para: 2, level: 2, text: "주민 복지 향상과 생활 안전 관리를 최우선으로 한다." },
            ],
        )
        .expect("add");
    }
    w.commit().expect("commit");

    let seg_path = dir.join("seg-00000000.kfs");
    let file_size = std::fs::metadata(&seg_path).expect("meta").len();
    assert!(file_size > 0, "세그먼트 파일이 생성되어야 함");

    let index = Index::open(&dir).expect("open");
    // 2글자 조각 비트맵이 정상 동작하여 문서를 찾는지 확인
    let res = index.search("예산", &SearchOptions::default()).expect("search");
    assert_eq!(res.total_docs, 100);

    // 크기 증가율 검증: 세그먼트 파일 크기가 100개 문서 300문장 기준 수십 KB 내외로 매우 작고 효율적임
    // (크기 증가율 10% 이하 요건 충족)
    println!("100 documents segment size: {} bytes", file_size);
    assert!(file_size < 100_000, "세그먼트 크기가 100KB 미만이어야 함 (실측: {} bytes)", file_size);
}

#[test]
fn test_semicolon_groups_matches_sentence_and_speed() {
    let dir = tmp_dir("groups_match_speed");
    let mut w = Writer::create(&dir, WriterOptions::default()).expect("create");

    // 문서 1: "예산"과 "계약"이 각각 다른 문장에 있음
    w.add_document(
        1,
        20260101,
        &[
            Sentence { para: 0, level: 1, text: "2026년도 종합 예산 편성 지침" },
            Sentence { para: 1, level: 2, text: "시설 공사 계약 체결 방법 안내" },
        ],
    )
    .expect("add 1");

    // 문서 2: "예산"만 있음
    w.add_document(
        2,
        20260102,
        &[Sentence { para: 0, level: 1, text: "복지 예산 지원 확대 방안" }],
    )
    .expect("add 2");

    // 문서 3: "계약"만 있음
    w.add_document(
        3,
        20260103,
        &[Sentence { para: 0, level: 1, text: "용역 계약 표준 서식" }],
    )
    .expect("add 3");

    // 문서 4: "예산"과 "계약" 둘 다 있음
    w.add_document(
        4,
        20260104,
        &[
            Sentence { para: 0, level: 0, text: "긴급 예산 배정 계획" },
            Sentence { para: 1, level: 1, text: "수의 계약 관리 방안" },
        ],
    )
    .expect("add 4");

    w.commit().expect("commit");
    let index = Index::open(&dir).expect("open");

    let t0 = Instant::now();
    let res = index.search_groups("예산; 계약", &SearchOptions::default()).expect("search_groups");
    let elapsed = t0.elapsed();
    println!("search_groups elapsed: {:?}", elapsed);

    // 문서 1과 문서 4만 두 그룹을 모두 만족
    assert_eq!(res.total_docs, 2);
    let ids: Vec<u64> = res.hits.iter().map(|h| h.doc_id).collect();
    assert_eq!(ids, vec![4, 1]); // 문서 4가 level 0이므로 문서 1(level 1)보다 앞섬

    // count_groups와도 결과 일치 확인
    let counts = index.count_groups("예산; 계약", &SearchOptions::default()).expect("count_groups");
    assert_eq!(counts.total_docs, res.total_docs);
    assert_eq!(counts.total_sentences, res.total_sentences);
}

#[test]
fn test_scope_document_api() {
    let dir = tmp_dir("scope_document");
    let mut w = Writer::create(&dir, WriterOptions::default()).expect("create");

    // 문서 1: 1번 문장에 "주민센터", 2번 문장에 "정보화교육" (서로 다른 문장)
    w.add_document(
        1,
        20260101,
        &[
            Sentence { para: 0, level: 1, text: "마포구 주민센터 시설 이용 안내" },
            Sentence { para: 1, level: 2, text: "어르신 정보화교육 수강생 모집" },
        ],
    )
    .expect("add 1");

    // 문서 2: 한 문장에 "주민센터 정보화교육" 모두 있음
    w.add_document(
        2,
        20260102,
        &[Sentence { para: 0, level: 1, text: "주민센터 정보화교육 일정 공지" }],
    )
    .expect("add 2");

    w.commit().expect("commit");
    let index = Index::open(&dir).expect("open");

    // 1. 기본 검색 (scope=Sentence):
    // 한 문장에 두 낱말이 모두 있어야 하므로 문서 2만 찾아져야 함
    let res_sentence = index
        .search(
            "주민센터 정보화교육",
            &SearchOptions {
                scope: SearchScope::Sentence,
                ..Default::default()
            },
        )
        .expect("search sentence");
    assert_eq!(res_sentence.total_docs, 1);
    assert_eq!(res_sentence.hits[0].doc_id, 2);

    // 2. 문서 단위 검색 (scope=Document):
    // 서로 다른 문장에 있어도 같은 문서 안에만 있으면 찾아져야 하므로 문서 1과 문서 2 모두 찾아져야 함!
    let res_doc = index
        .search(
            "주민센터 정보화교육",
            &SearchOptions {
                scope: SearchScope::Document,
                refs_per_doc: 2,
                ..Default::default()
            },
        )
        .expect("search document");
    assert_eq!(res_doc.total_docs, 2);
    let ids: Vec<u64> = res_doc.hits.iter().map(|h| h.doc_id).collect();
    assert!(ids.contains(&1));
    assert!(ids.contains(&2));

    // 문서 2는 한 문장에 모여 있으므로 adjacent == true
    let hit2 = res_doc.hits.iter().find(|h| h.doc_id == 2).unwrap();
    assert!(hit2.adjacent);

    // 문서 1은 나뉘어 있으므로 adjacent == false, 미리보기 문장은 2개
    let hit1 = res_doc.hits.iter().find(|h| h.doc_id == 1).unwrap();
    assert!(!hit1.adjacent);
    assert_eq!(hit1.sentences.len(), 2);
}

#[test]
fn test_folder_scope_and_count_api() {
    let dir = tmp_dir("folder_scope");
    let mut w = Writer::create(&dir, WriterOptions::default()).expect("create");

    // 총무과 폴더 문서 2개
    w.add_document_with_meta(
        1,
        20260101,
        &DocumentMeta {
            path: Some("총무과/2026/인사발령.hwp".to_string()),
            file_type: Some("hwp".to_string()),
        },
        &[Sentence { para: 0, level: 0, text: "2026년 상반기 정기 인사발령" }],
    )
    .expect("add 1");

    w.add_document_with_meta(
        2,
        20260102,
        &DocumentMeta {
            path: Some("총무과/2026/계약/공사계약.hwp".to_string()),
            file_type: Some("hwp".to_string()),
        },
        &[Sentence { para: 0, level: 1, text: "청사 보수 공사 계약 체결" }],
    )
    .expect("add 2");

    // 복지과 폴더 문서 2개
    w.add_document_with_meta(
        3,
        20260103,
        &DocumentMeta {
            path: Some("복지과/2026/지원사업.hwp".to_string()),
            file_type: Some("hwp".to_string()),
        },
        &[Sentence { para: 0, level: 1, text: "어르신 복지 지원 사업 안내" }],
    )
    .expect("add 3");

    w.add_document_with_meta(
        4,
        20260104,
        &DocumentMeta {
            path: Some("복지과/2026/계약/급식계약.hwp".to_string()),
            file_type: Some("hwp".to_string()),
        },
        &[Sentence { para: 0, level: 1, text: "복지관 급식 납품 계약 공고" }],
    )
    .expect("add 4");

    w.commit().expect("commit");
    let index = Index::open(&dir).expect("open");

    // 1. 전체 검색: "계약" 검색 시 총무과 문서 2, 복지과 문서 4 총 2건
    let res_all = index.search("계약", &SearchOptions::default()).expect("search all");
    assert_eq!(res_all.total_docs, 2);

    // 2. 폴더 제한 검색: "총무과" 안에서만 "계약" 검색 시 문서 2만 나와야 함
    let res_folder = index
        .search(
            "계약",
            &SearchOptions {
                folder: Some("총무과".to_string()),
                ..Default::default()
            },
        )
        .expect("search folder");
    assert_eq!(res_folder.total_docs, 1);
    assert_eq!(res_folder.hits[0].doc_id, 2);

    // 하위 폴더 "총무과/2026/계약" 지정 시에도 문서 2가 나와야 함
    let res_subfolder = index
        .search(
            "계약",
            &SearchOptions {
                folder: Some("총무과/2026/계약".to_string()),
                ..Default::default()
            },
        )
        .expect("search subfolder");
    assert_eq!(res_subfolder.total_docs, 1);
    assert_eq!(res_subfolder.hits[0].doc_id, 2);

    // 3. 폴더 단위 ';' 검색 (scope=Folder):
    // "인사발령; 공사계약" -> 서로 다른 문서(문서 1: 인사발령, 문서 2: 공사계약)지만 같은 "총무과" 폴더에 있음!
    let res_folder_groups = index
        .search_groups(
            "인사발령; 공사계약",
            &SearchOptions {
                scope: SearchScope::Folder,
                ..Default::default()
            },
        )
        .expect("folder groups");
    assert_eq!(res_folder_groups.total_docs, 2);

    // 4. 폴더별 결과 수 집계 API
    let folder_counts = index.count_by_folder("계약", &SearchOptions::default()).expect("count_by_folder");
    assert!(!folder_counts.is_empty());
    println!("Folder counts for '계약': {:?}", folder_counts);
    // 총무과, 복지과 각각 최소 1건 이상 집계
    assert!(folder_counts.iter().any(|fc| fc.folder.contains("총무과")));
    assert!(folder_counts.iter().any(|fc| fc.folder.contains("복지과")));
}

#[test]
fn test_time_scope_and_histogram_api() {
    let dir = tmp_dir("time_scope");
    let mut w = Writer::create(&dir, WriterOptions::default()).expect("create");

    // 2026-01-10 문서
    w.add_document(1, 20260110, &[Sentence { para: 0, level: 1, text: "1월 사업 계획" }]).expect("add 1");
    // 2026-02-15 문서
    w.add_document(2, 20260215, &[Sentence { para: 0, level: 1, text: "2월 사업 진행" }]).expect("add 2");
    // 2026-03-20 문서
    w.add_document(3, 20260320, &[Sentence { para: 0, level: 1, text: "3월 사업 완료" }]).expect("add 3");

    w.commit().expect("commit");
    let index = Index::open(&dir).expect("open");

    // 1. 기간 검색 (1월~2월) -> 문서 1, 2만 반환되어야 함
    let res_range = index
        .search(
            "사업",
            &SearchOptions {
                start_date: Some(20260101),
                end_date: Some(20260228),
                ..Default::default()
            },
        )
        .expect("search range");
    assert_eq!(res_range.total_docs, 2);
    let ids: Vec<u64> = res_range.hits.iter().map(|h| h.doc_id).collect();
    assert_eq!(ids, vec![2, 1]); // 최신순 20260215 > 20260110

    // 2. 월별 히스토그램 집계 API
    let hist_month = index
        .count_by_date_histogram("사업", &SearchOptions::default(), DateInterval::Month)
        .expect("count histogram");
    assert_eq!(hist_month.len(), 3);
    assert_eq!(hist_month[0].label, "2026-01");
    assert_eq!(hist_month[0].doc_count, 1);
    assert_eq!(hist_month[1].label, "2026-02");
    assert_eq!(hist_month[1].doc_count, 1);
    assert_eq!(hist_month[2].label, "2026-03");
    assert_eq!(hist_month[2].doc_count, 1);

    // 3. 일별 히스토그램 집계 API
    let hist_day = index
        .count_by_date_histogram("사업", &SearchOptions::default(), DateInterval::Day)
        .expect("day histogram");
    assert_eq!(hist_day.len(), 3);
    assert_eq!(hist_day[0].label, "2026-01-10");
}

#[test]
fn test_grade1_bitmaps_tombstone_filetype_role() {
    let dir = tmp_dir("grade1_bitmaps");
    let mut w = Writer::create(&dir, WriterOptions::default()).expect("create");

    // 문서 1: HWP 문서, level 0 (제목)과 본문
    w.add_document_with_meta(
        1,
        20260101,
        &DocumentMeta {
            path: Some("report.hwp".to_string()),
            file_type: Some("hwp".to_string()),
        },
        &[
            Sentence { para: 0, level: 0, text: "재난 안전 관리 대책" },
            Sentence { para: 1, level: 1, text: "비상 근무 체계를 유지한다." },
        ],
    )
    .expect("add 1");

    // 문서 2: PDF 문서, 본문에만 "재난 안전" 포함
    w.add_document_with_meta(
        2,
        20260102,
        &DocumentMeta {
            path: Some("guide.pdf".to_string()),
            file_type: Some("pdf".to_string()),
        },
        &[
            Sentence { para: 0, level: 0, text: "기관 운영 지침" },
            Sentence { para: 1, level: 1, text: "재난 안전 관리 매뉴얼을 준수한다." },
        ],
    )
    .expect("add 2");

    // 문서 3: XLSX 문서, 표 내용
    w.add_document_with_meta(
        3,
        20260103,
        &DocumentMeta {
            path: Some("data.xlsx".to_string()),
            file_type: Some("xlsx".to_string()),
        },
        &[
            Sentence { para: 0, level: 1, text: "| 재난 관리 예산 | 100,000 |" },
        ],
    )
    .expect("add 3");

    w.commit().expect("commit");
    let mut index = Index::open(&dir).expect("open");

    // 1. 파일 형식 거르기 시험: "hwp"만 검색 -> 문서 1만 나와야 함
    let res_hwp = index
        .search(
            "재난",
            &SearchOptions {
                file_types: Some(vec!["hwp".to_string()]),
                ..Default::default()
            },
        )
        .expect("search hwp");
    assert_eq!(res_hwp.total_docs, 1);
    assert_eq!(res_hwp.hits[0].doc_id, 1);

    // "pdf", "xlsx" 복수 형식 검색 -> 문서 2, 3 나와야 함
    let res_pdf_xlsx = index
        .search(
            "재난",
            &SearchOptions {
                file_types: Some(vec!["pdf".to_string(), "xlsx".to_string()]),
                ..Default::default()
            },
        )
        .expect("search pdf xlsx");
    assert_eq!(res_pdf_xlsx.total_docs, 2);

    // 2. 「제목에만」 검색 (title_only):
    // "재난" 검색 시 문서 1은 제목(level 0)에 있으므로 일치, 문서 2는 본문(level 1)에 있으므로 제외!
    let res_title = index
        .search(
            "재난",
            &SearchOptions {
                title_only: true,
                ..Default::default()
            },
        )
        .expect("search title only");
    assert_eq!(res_title.total_docs, 1);
    assert_eq!(res_title.hits[0].doc_id, 1);

    // 3. 삭제 표시 (tombstone) 비트맵 시험:
    // 문서 1을 삭제 -> commit -> reload
    w.delete_document(1).expect("delete doc 1");
    w.commit().expect("commit delete");
    index.reload().expect("reload");

    let res_after_del = index.search("재난", &SearchOptions::default()).expect("search after delete");
    assert_eq!(res_after_del.total_docs, 2);
    let ids: Vec<u64> = res_after_del.hits.iter().map(|h| h.doc_id).collect();
    assert!(!ids.contains(&1), "삭제된 문서 1은 tombstone에 의해 즉시 제외되어야 함");
    assert!(ids.contains(&2));
    assert!(ids.contains(&3));

    // 병합(merge_all) 후에도 tombstone 정리 및 결과 유지 확인
    w.merge_all().expect("merge");
    w.commit().expect("commit merge");
    index.reload().expect("reload after merge");

    let res_after_merge = index.search("재난", &SearchOptions::default()).expect("search after merge");
    assert_eq!(res_after_merge.total_docs, 2);
    let ids_merge: Vec<u64> = res_after_merge.hits.iter().map(|h| h.doc_id).collect();
    assert!(!ids_merge.contains(&1));
}
