// Copyright (c) 2026 haijun93 (마포구청 공혁준). PolyForm Strict License 1.0.0.
//! KFSIZE: kfind-core 색인 크기·메모리 측정 및 동일성 검증 시험
mod common;
use common::tmp_dir;
use kfind_core::{
    DocumentMeta, Index, SearchOptions, SearchScope, Sentence, SplitMode, Writer, WriterOptions,
};
use roaring::RoaringBitmap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

struct SectionSizes {
    file_len: u64,
    header_len: usize,
    post: u64,
    dict: u64,
    text: u64,
    text_off: u64,
    para: u64,
    level: u64,
    doc_of: u64,
    docs: u64,
    alpha: u64,
    alpha_ix: u64,
}

fn read_section_sizes(path: &Path) -> SectionSizes {
    let mut f = File::open(path).expect("open segment");
    let mut header = [0u8; 216];
    f.read_exact(&mut header).expect("read header");

    let file_len = u64::from_le_bytes(header[40..48].try_into().unwrap());
    let mut sec = [(0u64, 0u64); 10];
    for i in 0..10 {
        let off = u64::from_le_bytes(header[48 + i * 16..56 + i * 16].try_into().unwrap());
        let len = u64::from_le_bytes(header[56 + i * 16..64 + i * 16].try_into().unwrap());
        sec[i] = (off, len);
    }

    SectionSizes {
        file_len,
        header_len: 216,
        post: sec[0].1,
        dict: sec[1].1,
        text: sec[2].1,
        text_off: sec[3].1,
        para: sec[4].1,
        level: sec[5].1,
        doc_of: sec[6].1,
        docs: sec[7].1,
        alpha: sec[8].1,
        alpha_ix: sec[9].1,
    }
}

fn build_synthetic_corpus(w: &mut Writer, n_docs: usize) {
    let vocab_subjects = [
        "2026년도 정기 예산안", "주민참여예산 제안사업", "지방자치단체 재정운용계획",
        "공공시설물 안전점검", "스마트 도시안전망 구축", "도로 포장 및 교통시설 정비",
        "취약계층 맞춤형 복지서비스", "노인복지관 건립 및 환경개선", "어린이 통학로 교통안전",
        "정보보안 강화대책", "인공지능 기반 행정혁신", "클라우드 업무환경 전환"
    ];
    let vocab_actions = [
        "심의 의결을 거쳐 확정하였다",
        "신속하게 집행하여 현장 민원을 해소한다",
        "철저한 사전 점검을 통해 재난 위험을 예방한다",
        "지역 주민의 다양한 의견을 적극 반영한다",
        "담당 부서와 협의하여 단계별 추진 일정을 마련한다",
        "투명하고 공정한 계약 절차를 준수한다",
        "시스템 안정성과 개인정보 보호를 최우선으로 한다",
        "지속 가능한 친환경 녹색성장을 도모한다"
    ];
    let vocab_notes = [
        "상세 세부 내역은 첨부 계획서를 참조할 것",
        "예산 배정 및 자금 집행은 관련 법령에 의함",
        "분기별 실적 보고 및 자체 성과 평가 실시 예정",
        "미이행 과제에 대해서는 보완 대책을 강구함"
    ];

    for i in 1..=n_docs {
        let meta = DocumentMeta {
            path: Some(format!("dept/division_{}/doc_{:04}.hwp", i % 7, i)),
            file_type: Some(match i % 4 {
                0 => "hwp",
                1 => "hwpx",
                2 => "pdf",
                _ => "txt",
            }.to_string()),
        };

        let s1 = format!("{} 보고 및 세부 추진 계획", vocab_subjects[i % vocab_subjects.len()]);
        let s2 = format!("본 사업은 {} 위해 수립되었으며, {}.", vocab_subjects[(i + 1) % vocab_subjects.len()], vocab_actions[i % vocab_actions.len()]);
        let s3 = format!("주요 추진 과제로는 시설물 정밀 점검, 현장 점검단 구성, 분기별 모니터링 체계가 포함된다.");
        let s4 = format!("관련 규정에 의거하여 {}.", vocab_actions[(i + 3) % vocab_actions.len()]);
        let s5 = format!("또한 주민 협의체 회의를 분기별로 개최하여 진행 상황을 공유한다.");
        let s6 = format!("소요 예산은 총 {} 백만원이며, {}.", 10 + (i * 3) % 500, vocab_actions[(i + 5) % vocab_actions.len()]);
        let s7 = format!("향후 계획: {}.", vocab_notes[i % vocab_notes.len()]);
        let s8 = format!("비고: {}.", vocab_notes[(i + 2) % vocab_notes.len()]);

        let sents = [
            Sentence { para: 0, level: 0, text: &s1 },
            Sentence { para: 1, level: 1, text: &s2 },
            Sentence { para: 1, level: 2, text: &s3 },
            Sentence { para: 2, level: 1, text: &s4 },
            Sentence { para: 2, level: 2, text: &s5 },
            Sentence { para: 3, level: 1, text: &s6 },
            Sentence { para: 4, level: 2, text: &s7 },
            Sentence { para: 4, level: 3, text: &s8 },
        ];

        let date = 20260101 + (i as i64 % 365);
        w.add_document_with_meta(i as u64, date, &meta, &sents).expect("add");
    }
}

#[test]
fn test_measure_synthetic_index_breakdown() {
    let dir = tmp_dir("measure_synthetic_index");
    let mut w = Writer::create(&dir, WriterOptions::default()).expect("create");

    let n_docs = 500;
    build_synthetic_corpus(&mut w, n_docs);
    w.commit().expect("commit");

    let seg_path = dir.join("seg-00000000.kfs");
    let sizes = read_section_sizes(&seg_path);

    println!("\n=== 합성 색인(500 문서, 4000 문장) 구성별 크기 실측 ===");
    println!("총 세그먼트 파일 크기: {} bytes (100.0%)", sizes.file_len);
    println!("  - HEADER:   {:8} bytes ({:.2}%)", sizes.header_len, (sizes.header_len as f64 / sizes.file_len as f64) * 100.0);
    println!("  - POST:     {:8} bytes ({:.2}%) [Roaring 비트맵]", sizes.post, (sizes.post as f64 / sizes.file_len as f64) * 100.0);
    println!("  - DICT:     {:8} bytes ({:.2}%) [FST 사전]", sizes.dict, (sizes.dict as f64 / sizes.file_len as f64) * 100.0);
    println!("  - TEXT:     {:8} bytes ({:.2}%) [문장 본문 부호화]", sizes.text, (sizes.text as f64 / sizes.file_len as f64) * 100.0);
    println!("  - TEXT_OFF: {:8} bytes ({:.2}%) [문장 오프셋 표]", sizes.text_off, (sizes.text_off as f64 / sizes.file_len as f64) * 100.0);
    println!("  - PARA:     {:8} bytes ({:.2}%) [문단 번호]", sizes.para, (sizes.para as f64 / sizes.file_len as f64) * 100.0);
    println!("  - LEVEL:    {:8} bytes ({:.2}%) [위계 순위]", sizes.level, (sizes.level as f64 / sizes.file_len as f64) * 100.0);
    println!("  - DOC_OF:   {:8} bytes ({:.2}%) [문서 번호]", sizes.doc_of, (sizes.doc_of as f64 / sizes.file_len as f64) * 100.0);
    println!("  - DOCS:     {:8} bytes ({:.2}%) [문서 메타 표]", sizes.docs, (sizes.docs as f64 / sizes.file_len as f64) * 100.0);
    println!("  - ALPHA:    {:8} bytes ({:.2}%) [글자 빈도 표]", sizes.alpha, (sizes.alpha as f64 / sizes.file_len as f64) * 100.0);
    println!("  - ALPHA_IX: {:8} bytes ({:.2}%) [글자 오름차순 표]", sizes.alpha_ix, (sizes.alpha_ix as f64 / sizes.file_len as f64) * 100.0);

    let manifest_path = dir.join("manifest");
    if let Ok(m) = std::fs::metadata(&manifest_path) {
        println!("  - manifest: {:8} bytes", m.len());
    }

    assert!(sizes.file_len > 0);
}

#[test]
fn test_bitmap_optimize_roundtrip_identity() {
    // 1. 밀집 구간 (연속)
    let mut bm1 = RoaringBitmap::new();
    for i in 50..5000 {
        bm1.insert(i);
    }
    let mut opt1 = bm1.clone();
    opt1.optimize();
    let mut buf = Vec::new();
    opt1.serialize_into(&mut buf).unwrap();
    let des1 = RoaringBitmap::deserialize_from(&buf[..]).unwrap();
    assert_eq!(bm1, des1, "밀집 구간 비트맵 optimize 후 역직렬화 동일성 실패");

    // 2. 희소 구간 (비연속)
    let mut bm2 = RoaringBitmap::new();
    for i in (0..10000).step_by(17) {
        bm2.insert(i);
    }
    let mut opt2 = bm2.clone();
    opt2.optimize();
    buf.clear();
    opt2.serialize_into(&mut buf).unwrap();
    let des2 = RoaringBitmap::deserialize_from(&buf[..]).unwrap();
    assert_eq!(bm2, des2, "희소 구간 비트맵 optimize 후 역직렬화 동일성 실패");

    // 3. 복합 구간 (혼합 패턴)
    let mut bm3 = RoaringBitmap::new();
    for i in 10..100 { bm3.insert(i); }
    for i in (200..1000).step_by(5) { bm3.insert(i); }
    for i in 3000..5000 { bm3.insert(i); }
    let mut opt3 = bm3.clone();
    opt3.optimize();
    buf.clear();
    opt3.serialize_into(&mut buf).unwrap();
    let des3 = RoaringBitmap::deserialize_from(&buf[..]).unwrap();
    assert_eq!(bm3, des3, "혼합 구간 비트맵 optimize 후 역직렬화 동일성 실패");
}

#[test]
fn test_search_results_identity_with_optimized_postings() {
    let dir = tmp_dir("search_identity_optimized");
    let mut w = Writer::create(&dir, WriterOptions::default()).expect("create");

    let n_docs = 200;
    build_synthetic_corpus(&mut w, n_docs);
    w.commit().expect("commit");

    let index = Index::open(&dir).expect("open index");

    // 다양한 질의 패턴으로 검색 실행 및 결과 일관성 확인
    let test_queries = [
        "예산",
        "안전점검",
        "주민참여예산",
        "추진 계획",
        "정보보안 강화대책",
        "친환경 녹색성장",
    ];

    for q in test_queries {
        for mode in [SplitMode::Off, SplitMode::DictLongest, SplitMode::Bigram] {
            let opts = SearchOptions {
                split: mode,
                top: 20,
                refs_per_doc: 2,
                ..SearchOptions::default()
            };
            let res = index.search(q, &opts).expect("search");
            assert!(res.total_docs > 0, "질의 '{}'에 대한 문서 결과가 있어야 함", q);
            assert!(res.total_sentences > 0, "질의 '{}'에 대한 문장 결과가 있어야 함", q);
            assert!(!res.hits.is_empty(), "질의 '{}' hits가 비어있지 않아야 함", q);

            // 문서 단위 scope 확인
            let doc_opts = SearchOptions {
                scope: SearchScope::Document,
                ..opts.clone()
            };
            let doc_res = index.search(q, &doc_opts).expect("search doc scope");
            assert_eq!(res.total_docs, doc_res.total_docs, "문서 개수는 scope=document와 일치해야 함");
        }
    }

    // 세미콜론 묶음 검색
    let groups_res = index.search_groups("예산; 계획", &SearchOptions::default()).expect("search groups");
    assert!(groups_res.total_docs > 0);
}
