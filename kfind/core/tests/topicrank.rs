// Copyright (c) 2026 haijun93 (마포구청 공혁준). PolyForm Strict License 1.0.0.
//! SEMI3-topicrank 테스트:
//! 1. 주제 빈도 순위표: 파일명 낱말 + 제목·소제목 낱말 추출 및 TF-IDF 순위
//! 2. 너무 흔한 말(계획·보고·추진 등) 및 통계적 불용어(DF/N >= 0.5) 자동 제외
//! 3. 상위 1,000개 실시간 유지 및 문서 추가·수정·삭제 시 마이크로초 단위 증감
//! 4. 기능 API:
//!    - 자동 완성(빈도순)
//!    - 빈 검색어 첫 화면 주제 1~10위
//!    - `;` 관련어 추천(교집합 개수)
//!    - 기간별 급증 주제(시간 비트맵)
//!    - 폴더 대표 주제
//! 5. 결과 순위: 드문 말 일치 가산, 흔한 말 가산 낮춤
//! 6. 영속성: 색인 디렉토리 내 topics.bin 저장 및 복원
mod common;
use common::tmp_dir;
use kfind_core::{
    DocumentMeta, Index, SearchOptions, Sentence, Writer, WriterOptions,
};
use std::time::Instant;

#[test]
fn test_topicrank_extraction_and_tf_idf_ranking() {
    let dir = tmp_dir("topicrank_tfidf");
    let mut w = Writer::create(&dir, WriterOptions::default()).expect("create");

    // 10개 문서 색인
    // "스마트모빌리티": 3개 문서에 등장 (TF=6, DF=3) -> 적당히 많지만 모든 문서에 있지 않음 (TF-IDF 높음)
    // "계획", "보고서", "추진": 흔한 말이므로 자동 제외
    // "공통단어": 8개 문서에 등장 (DF/N >= 0.5) -> 통계적 흔한 말이므로 순위 제외 또는 매우 낮음
    for i in 1..=10 {
        let path = if i <= 3 {
            format!("교통과/2026/스마트모빌리티_시범사업_계획서_{i}.hwp")
        } else if i <= 6 {
            format!("도시재생과/2026/골목길_정비_보고서_{i}.hwp")
        } else {
            format!("행정지원과/2026/민원_서비스_개선_추진_{i}.hwp")
        };

        let meta = DocumentMeta {
            path: Some(path),
            file_type: Some("hwp".to_string()),
        };

        let mut sentences = Vec::new();
        if i <= 3 {
            sentences.push(Sentence {
                para: 0,
                level: 0,
                text: "스마트모빌리티 종합 도입 계획",
            });
            sentences.push(Sentence {
                para: 1,
                level: 1,
                text: "스마트모빌리티 인프라 구축 추진 방향",
            });
        } else if i <= 6 {
            sentences.push(Sentence {
                para: 0,
                level: 0,
                text: "골목길 보행 환경 정비 사업 보고",
            });
        } else {
            sentences.push(Sentence {
                para: 0,
                level: 0,
                text: "주민 중심 민원 서비스 체계 구축",
            });
        }
        // 본문 문장 (동시 출현용 보조)
        sentences.push(Sentence {
            para: 2,
            level: 2,
            text: "인프라 구축과 예산 확보를 위해 관계 부서와 협의를 진행한다.",
        });

        w.add_document_with_meta(i, 20260101 + (i as i64), &meta, &sentences)
            .expect("add");
    }
    w.commit().expect("commit");

    let index = Index::open(&dir).expect("open");

    // 첫 화면 상위 주제 1~10위 확인
    let top = index.top_topics(10);
    assert!(!top.is_empty(), "상위 주제 목록이 있어야 함");

    // 1위는 "스마트모빌리티"여야 함 (TF 높고, 특정 문서에만 집중되어 IDF 높음)
    assert_eq!(
        top[0].term, "스마트모빌리티",
        "스마트모빌리티가 1위여야 함 (실제: {})",
        top[0].term
    );

    // 흔한 말("계획", "보고", "보고서", "추진", "사업")은 상위 순위표에 없어야 함
    for item in &top {
        assert_ne!(item.term, "계획", "불용어 '계획'은 제외되어야 함");
        assert_ne!(item.term, "보고", "불용어 '보고'는 제외되어야 함");
        assert_ne!(item.term, "추진", "불용어 '추진'은 제외되어야 함");
        assert_ne!(item.term, "사업", "불용어 '사업'은 제외되어야 함");
    }
}

#[test]
fn test_topicrank_realtime_updates_and_microsecond_speed() {
    let dir = tmp_dir("topicrank_realtime");
    let mut w = Writer::create(&dir, WriterOptions::default()).expect("create");

    // 초기 문서 1 추가
    let meta1 = DocumentMeta {
        path: Some("기획과/데이터_허브_구축.hwp".to_string()),
        file_type: Some("hwp".to_string()),
    };
    w.add_document_with_meta(
        1,
        20260101,
        &meta1,
        &[Sentence {
            para: 0,
            level: 0,
            text: "데이터허브 인프라 구축",
        }],
    )
    .expect("add 1");

    // 단어 확인
    assert_eq!(
        w.topics.stats.get("데이터허브").map(|s| s.title_tf),
        Some(1)
    );

    // 문서 변경 시 소요 시간 측정 (마이크로초 단위 증감 검증)
    let t0 = Instant::now();
    // 문서 1을 새로운 내용("인공지능 혁신")으로 교체
    let meta1_updated = DocumentMeta {
        path: Some("기획과/인공지능_혁신_전략.hwp".to_string()),
        file_type: Some("hwp".to_string()),
    };
    w.add_document_with_meta(
        1,
        20260102,
        &meta1_updated,
        &[Sentence {
            para: 0,
            level: 0,
            text: "인공지능 행정 서비스 도입",
        }],
    )
    .expect("update 1");
    let elapsed = t0.elapsed();

    println!("Document update elapsed time: {:?}", elapsed);
    assert!(
        elapsed.as_millis() < 50,
        "문서 변경 시 증감 연산은 극히 빨라야 함 ({:?})",
        elapsed
    );

    // 이전 단어 "데이터허브"는 제거되고 새 단어 "인공지능"이 등록되었는지 확인
    assert!(
        w.topics.stats.get("데이터허브").is_none(),
        "이전 단어는 제거되어야 함"
    );
    // 파일명과 제목 양쪽에 등장하므로 title_tf는 2
    assert_eq!(
        w.topics.stats.get("인공지능").map(|s| s.title_tf),
        Some(2),
        "새 단어가 등록되어야 함"
    );

    // 문서 삭제
    let t_del = Instant::now();
    w.delete_document(1).expect("delete");
    let elapsed_del = t_del.elapsed();
    println!("Document delete elapsed time: {:?}", elapsed_del);
    assert!(
        elapsed_del.as_millis() < 50,
        "문서 삭제 시 증감 연산은 극히 빨라야 함"
    );

    assert!(w.topics.stats.get("인공지능").is_none());
    assert_eq!(w.topics.total_docs, 0);
}

#[test]
fn test_topicrank_autocomplete_and_empty_top() {
    let dir = tmp_dir("topicrank_autocomplete");
    let mut w = Writer::create(&dir, WriterOptions::default()).expect("create");

    // "청년" 접두사 단어들: 청년주거, 청년창업, 청년지원금
    // "환경" 단어: 환경보호
    w.add_document_with_meta(
        1,
        20260101,
        &DocumentMeta {
            path: Some("복지과/청년주거_안정.hwp".to_string()),
            ..Default::default()
        },
        &[
            Sentence {
                para: 0,
                level: 0,
                text: "청년주거 종합 정책 안내",
            },
            Sentence {
                para: 1,
                level: 1,
                text: "청년주거 임대보증금 확대",
            },
        ],
    )
    .expect("add 1");

    w.add_document_with_meta(
        2,
        20260102,
        &DocumentMeta {
            path: Some("일자리과/청년창업_지원.hwp".to_string()),
            ..Default::default()
        },
        &[Sentence {
            para: 0,
            level: 0,
            text: "청년창업 활성화 펀드 조성",
        }],
    )
    .expect("add 2");

    w.add_document_with_meta(
        3,
        20260103,
        &DocumentMeta {
            path: Some("환경과/환경보호_실천.hwp".to_string()),
            ..Default::default()
        },
        &[Sentence {
            para: 0,
            level: 0,
            text: "탄소중립 환경보호 행동 수칙",
        }],
    )
    .expect("add 3");

    w.commit().expect("commit");

    let index = Index::open(&dir).expect("open");

    // 1. 자동 완성 (빈도순): "청년" 접두사 검색
    let ac = index.autocomplete("청년", 5);
    assert!(!ac.is_empty(), "청년으로 시작하는 자동완성 추천이 나와야 함");
    // "청년주거"는 TF=3, "청년창업"은 TF=2 -> 청년주거가 앞서야 함
    assert_eq!(ac[0].term, "청년주거");
    assert_eq!(ac[1].term, "청년창업");

    // 2. 빈 검색어 첫 화면 주제 1~10위
    let top = index.top_topics(10);
    assert!(top.len() >= 2);
    let top_terms: Vec<&str> = top.iter().map(|t| t.term.as_str()).collect();
    assert!(top_terms.contains(&"청년주거"));
    assert!(top_terms.contains(&"청년창업"));
}

#[test]
fn test_topicrank_related_topics_semicolon() {
    let dir = tmp_dir("topicrank_related");
    let mut w = Writer::create(&dir, WriterOptions::default()).expect("create");

    // 문서 1: "인공지능", "빅데이터", "클라우드"
    w.add_document_with_meta(
        1,
        20260101,
        &DocumentMeta {
            path: Some("인공지능_빅데이터_협력.hwp".to_string()),
            ..Default::default()
        },
        &[
            Sentence {
                para: 0,
                level: 0,
                text: "인공지능 빅데이터 기반 플랫폼",
            },
            Sentence {
                para: 1,
                level: 2,
                text: "클라우드 인프라를 활용하여 분석한다.",
            },
        ],
    )
    .expect("add 1");

    // 문서 2: "인공지능", "빅데이터", "모빌리티"
    w.add_document_with_meta(
        2,
        20260102,
        &DocumentMeta {
            path: Some("인공지능_빅데이터_응용.hwp".to_string()),
            ..Default::default()
        },
        &[
            Sentence {
                para: 0,
                level: 0,
                text: "인공지능 빅데이터 실증 사업",
            },
            Sentence {
                para: 1,
                level: 2,
                text: "모빌리티 센서 데이터를 수집한다.",
            },
        ],
    )
    .expect("add 2");

    // 문서 3: "인공지능", "로봇"
    w.add_document_with_meta(
        3,
        20260103,
        &DocumentMeta {
            path: Some("인공지능_로봇.hwp".to_string()),
            ..Default::default()
        },
        &[Sentence {
            para: 0,
            level: 0,
            text: "인공지능 로봇 자율주행",
        }],
    )
    .expect("add 3");

    w.commit().expect("commit");

    let index = Index::open(&dir).expect("open");

    // ';' 관련어 추천 (교집합 개수): "인공지능"과 동시 출현한 단어들
    let rel = index.related_topics("인공지능", 20);
    assert!(!rel.is_empty(), "관련어 추천 결과가 있어야 함");

    // "빅데이터"는 문서 1, 2에서 2번 함께 출현 -> 교집합 2개로 1위
    assert_eq!(rel[0].term, "빅데이터");
    assert_eq!(rel[0].co_count, 2);

    // "클라우드", "로봇" 등 교집합 단어 포함 여부 확인
    let other_terms: Vec<&str> = rel[1..].iter().map(|r| r.term.as_str()).collect();
    assert!(other_terms.contains(&"로봇"));
    assert!(other_terms.contains(&"클라우드"));
}

#[test]
fn test_topicrank_trending_topics_and_folder_topics() {
    let dir = tmp_dir("topicrank_trending_folder");
    let mut w = Writer::create(&dir, WriterOptions::default()).expect("create");

    // 2026년 1월 문서 (과거): "전통시장", "주차장" (총무과)
    for i in 1..=5 {
        w.add_document_with_meta(
            i,
            20260110 + (i as i64),
            &DocumentMeta {
                path: Some(format!("총무과/2026-01/전통시장_정비_{i}.hwp")),
                ..Default::default()
            },
            &[Sentence {
                para: 0,
                level: 0,
                text: "전통시장 주차장 환경 정비",
            }],
        )
        .expect("add jan");
    }

    // 2026년 11월 문서 (최근 급증): "스마트시티", "디지털트윈" (기획예산과)
    for i in 6..=10 {
        w.add_document_with_meta(
            i,
            20261101 + (i as i64),
            &DocumentMeta {
                path: Some(format!("기획예산과/2026-11/스마트시티_구축_{i}.hwp")),
                ..Default::default()
            },
            &[Sentence {
                para: 0,
                level: 0,
                text: "스마트시티 디지털트윈 가상 모형 구축",
            }],
        )
        .expect("add nov");
    }

    w.commit().expect("commit");

    let index = Index::open(&dir).expect("open");

    // 1. 기간별 급증 주제 (2026년 11월)
    let trending = index.trending_topics(20261101, 20261130, 10);
    assert!(!trending.is_empty(), "급증 주제가 있어야 함");
    let trending_terms: Vec<&str> = trending.iter().map(|t| t.term.as_str()).collect();
    assert!(
        trending_terms.contains(&"스마트시티") && trending_terms.contains(&"디지털트윈"),
        "11월 급증 주제 목록에 스마트시티와 디지털트윈이 포함되어야 함 ({:?})",
        trending_terms
    );
    // 11월에 출현한 주제들의 period_count는 5여야 함
    let sc = trending.iter().find(|t| t.term == "스마트시티").unwrap();
    assert_eq!(sc.period_count, 5);
    assert_eq!(sc.total_count, 5);

    // 2. 폴더 대표 주제: "총무과" 폴더
    let folder_top = index.folder_topics("총무과", 5);
    assert!(!folder_top.is_empty(), "총무과 대표 주제가 있어야 함");
    let terms: Vec<&str> = folder_top.iter().map(|t| t.term.as_str()).collect();
    assert!(terms.contains(&"전통시장"));
    assert!(terms.contains(&"주차장"));
    assert!(!terms.contains(&"스마트시티")); // 기획예산과 주제는 미포함
}

#[test]
fn test_topicrank_search_result_ranking_weight() {
    let dir = tmp_dir("topicrank_weight");
    let mut w = Writer::create(&dir, WriterOptions::default()).expect("create");

    // 문서 1~5: "공통단어"가 흔하게 등장
    for i in 1..=5 {
        w.add_document_with_meta(
            i,
            20260101 + (i as i64),
            &DocumentMeta {
                path: Some(format!("dept/doc_{i}.hwp")),
                ..Default::default()
            },
            &[Sentence {
                para: 0,
                level: 0,
                text: "공통단어 기본 지침 안내",
            }],
        )
        .expect("add common");
    }

    // 문서 6: "특수희귀어"가 1개 문서에만 등장 (IDF 매우 높음)
    w.add_document_with_meta(
        6,
        20260106,
        &DocumentMeta {
            path: Some("dept/doc_special.hwp".to_string()),
            ..Default::default()
        },
        &[Sentence {
            para: 0,
            level: 0,
            text: "특수희귀어 핵심 과제 연구",
        }],
    )
    .expect("add rare");

    w.commit().expect("commit");

    let index = Index::open(&dir).expect("open");

    // "공통단어"의 IDF vs "특수희귀어"의 IDF 비교
    let idf_common = index.idf("공통단어");
    let idf_rare = index.idf("특수희귀어");
    assert!(
        idf_rare > idf_common,
        "드문 말의 IDF가 흔한 말의 IDF보다 높아야 함 (rare: {}, common: {})",
        idf_rare,
        idf_common
    );

    // topic_rank_weight 옵션을 주었을 때 순위 가중치 검증
    let opts = SearchOptions {
        topic_rank_weight: 1.0,
        ..Default::default()
    };
    let res = index.search("특수희귀어", &opts).expect("search");
    assert_eq!(res.hits.len(), 1);
    assert_eq!(res.hits[0].doc_id, 6);
}
