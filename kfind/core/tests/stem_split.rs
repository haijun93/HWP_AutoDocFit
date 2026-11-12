//! KB3: 붙여 쓴 검색어 나누기 — 조각 낱말이 조사·어미가 붙은 꼴로만 색인에 있을 때(KC1 남은 문제).
mod common;
use common::*;
use kfind_core::{Index, SearchOptions, Sentence, SplitMode, Writer, WriterOptions};

fn build(name: &str, docs: &[(u64, &[&str])]) -> std::path::PathBuf {
    let dir = tmp_dir(name);
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for (id, sents) in docs {
        let v: Vec<Sentence> = sents.iter().enumerate().map(|(i, t)| Sentence { para: i as u32, level: 1, text: t }).collect();
        w.add_document(*id, 0, &v).unwrap();
    }
    w.commit().unwrap();
    dir
}

fn words(index: &Index, q: &str) -> String {
    let r = index.resolve(q, SplitMode::DictLongest).unwrap();
    r.words.iter().map(|w| match &w.pieces { Some(p) => format!("{}={}", w.text, p.join("+")), None => w.text.clone() }).collect::<Vec<_>>().join(" ")
}

#[test]
fn inflected_only_words_still_split() {
    // "예산"·"집행"·"계획"은 어절로 따로 나오지 않고 조사·어미가 붙은 꼴로만 나온다.
    let dir = build("stem-split", &[
        (1, &["예산을 적정하게 집행한다."]),
        (2, &["예산의 집행 내역과 계획을 보고."]),
        (3, &["예산집행 현황."]),
        (4, &["집행부 회의."]),
    ]);
    let index = Index::open(&dir).unwrap();
    assert_eq!(words(&index, "예산집행"), "예산집행=예산+집행");
    assert_eq!(words(&index, "예산집행계획"), "예산집행계획=예산+집행+계획");
    let opts = |split| SearchOptions { top: 0, split, ..SearchOptions::default() };
    let broad = index.search("예산집행", &opts(SplitMode::DictLongest)).unwrap();
    // 붙어 있는 일치(문서 3)가 앞, 이어서 같은 문장에 "예산"과 "집행"이 함께 있는 문서 1·2(나머지가 같아 문서 ID 순). 문서 4는 "예산"이 없다.
    assert_eq!(broad.hits.iter().map(|h| (h.doc_id, h.adjacent)).collect::<Vec<_>>(), [(3, true), (1, false), (2, false)]);
    let strict = index.search("예산집행", &opts(SplitMode::Off)).unwrap();
    assert_eq!(strict.hits.iter().map(|h| h.doc_id).collect::<Vec<_>>(), [3]);
    // 줄기 꼬리 목록에 없는 덧붙임으로만 나온 앞부분은 낱말로 보지 않는다("집행부"의 "부"는 꼬리가 아님).
    assert_eq!(words(&index, "집행부예산"), "집행부예산=집행부+예산");
    assert_eq!(words(&index, "행부예산"), "행부예산");
}

#[test]
fn deleted_inflected_forms_do_not_count() {
    // 줄기로 알아본 근거 어절이 모두 지워지면 더는 나누지 않는다(살아 있는 문장만 낱말 판정에 쓴다).
    let dir = build("stem-split-del", &[(1, &["예산을 쓴다."]), (2, &["집행 보고."])]);
    let index = Index::open(&dir).unwrap();
    assert_eq!(words(&index, "예산집행"), "예산집행=예산+집행");
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    w.delete_document(1).unwrap();
    w.commit().unwrap();
    drop(w);
    let index = Index::open(&dir).unwrap();
    assert_eq!(words(&index, "예산집행"), "예산집행");
}
