//! 견고성: 패닉 격리, 질의 시간 한도·취소, 깨진 파일, 쓰기 잠금, 원자적 교체, 남은 임시 파일.
mod common;
use common::*;
use kfind_core::{Error, FNV_SEED, Index, SearchOptions, SplitMode, Writer, WriterOptions, fnv64, guard};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

fn build(name: &str) -> std::path::PathBuf {
    let dir = tmp_dir(name);
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for d in &corpus(1) {
        add(&mut w, d);
    }
    w.commit().unwrap();
    dir
}

fn segment_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "kfs")).collect();
    v.sort();
    v
}

#[test]
fn panic_is_isolated_and_engine_keeps_working() {
    let dir = build("panic");
    let index = Index::open(&dir).unwrap();
    let before = index.search("예산 집행", &SearchOptions::default()).unwrap().total_docs;
    // 패닉 흉내: 경계 안에서 패닉이 나도 오류 값으로 돌아오고 프로세스·색인은 그대로다.
    let r: kfind_core::Result<()> = guard(|| panic!("흉내 낸 패닉"));
    match r {
        Err(Error::Panic(msg)) => assert!(msg.contains("흉내 낸 패닉")),
        other => panic!("expected Error::Panic, got {other:?}"),
    }
    let r: kfind_core::Result<u32> = guard(|| {
        let v: Vec<u32> = Vec::new();
        Ok(v[3])
    });
    assert!(matches!(r, Err(Error::Panic(_))));
    assert_eq!(index.search("예산 집행", &SearchOptions::default()).unwrap().total_docs, before);
}

#[test]
fn time_limit_and_cancel_stop_a_query() {
    let dir = build("limit");
    let index = Index::open(&dir).unwrap();
    let mut o = SearchOptions { time_limit: Some(Duration::ZERO), ..SearchOptions::default() };
    assert!(matches!(index.search("예산 집행", &o), Err(Error::Timeout)));
    assert!(matches!(index.match_sentences("예산집행계획", &o), Err(Error::Timeout)));
    // 한도를 넉넉히 주면 같은 색인이 정상으로 답한다(한도 초과가 상태를 망가뜨리지 않는다).
    o.time_limit = Some(Duration::from_secs(30));
    assert!(index.search("예산 집행", &o).unwrap().total_docs > 0);

    let flag = Arc::new(AtomicBool::new(true));
    o.cancel = Some(flag.clone());
    assert!(matches!(index.search("예산 집행", &o), Err(Error::Cancelled)));
    flag.store(false, Ordering::Relaxed);
    assert!(index.search("예산 집행", &o).unwrap().total_docs > 0);
}

#[test]
fn time_limit_fires_inside_a_long_scan() {
    // 무한 루프 흉내 대신 긴 확인 작업: 4글자 낱말이 모든 문장에 있어 문장마다 원문 확인을 한다.
    let dir = tmp_dir("longscan");
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    let text = "가나다라마바사 ".repeat(40);
    for doc in 0..400u64 {
        let sents: Vec<kfind_core::Sentence> = (0..50).map(|i| kfind_core::Sentence { para: i, level: 8, text: &text }).collect();
        w.add_document(doc, 0, &sents).unwrap();
    }
    w.commit().unwrap();
    let index = Index::open(&dir).unwrap();
    let unlimited = SearchOptions { time_limit: None, split: SplitMode::Off, ..SearchOptions::default() };
    let started = std::time::Instant::now();
    assert_eq!(index.search("가나다라마바사가나다라", &unlimited).unwrap().total_sentences, 20_000);
    let full = started.elapsed();
    let limited = SearchOptions { time_limit: Some(full / 20), ..unlimited.clone() };
    let started = std::time::Instant::now();
    assert!(matches!(index.search("가나다라마바사가나다라", &limited), Err(Error::Timeout)));
    let cut = started.elapsed();
    assert!(cut < full, "limited query took {cut:?}, unlimited {full:?}");

    // 다른 스레드가 취소 깃발을 세우면 진행 중인 질의가 멈춘다.
    let flag = Arc::new(AtomicBool::new(false));
    let o = SearchOptions { cancel: Some(flag.clone()), ..unlimited };
    let index = Arc::new(index);
    let (i2, o2) = (index.clone(), o.clone());
    let worker = std::thread::spawn(move || {
        let mut cancelled = 0;
        for _ in 0..200 {
            match i2.search("가나다라마바사가나다라", &o2) {
                Err(Error::Cancelled) => {
                    cancelled += 1;
                    break;
                }
                Ok(_) => {}
                Err(e) => panic!("unexpected {e}"),
            }
        }
        cancelled
    });
    std::thread::sleep(full / 2);
    flag.store(true, Ordering::Relaxed);
    assert_eq!(worker.join().unwrap(), 1, "query was not cancelled");
}

#[test]
fn damaged_files_are_reported_not_fatal() {
    let dir = build("damaged");
    let seg = segment_files(&dir).pop().unwrap();
    let good = std::fs::read(&seg).unwrap();
    let manifest = dir.join("MANIFEST.kfm");
    let good_manifest = std::fs::read(&manifest).unwrap();

    // 잘린 조각 파일.
    std::fs::write(&seg, &good[..good.len() / 2]).unwrap();
    assert!(matches!(Index::open(&dir), Err(Error::Corrupt(_))));
    // 머리글 한 바이트가 바뀜.
    let mut bad = good.clone();
    bad[30] ^= 1;
    std::fs::write(&seg, &bad).unwrap();
    assert!(matches!(Index::open(&dir), Err(Error::Corrupt(_))));
    // 쓰레기 파일.
    std::fs::write(&seg, b"not a segment").unwrap();
    assert!(matches!(Index::open(&dir), Err(Error::Corrupt(_))));
    // 본문(비트맵 구역)이 뭉개짐: 열리더라도 verify가 잡고, 질의는 오류나 결과로 끝나며 프로세스는 죽지 않는다.
    let mut bad = good.clone();
    for b in &mut bad[200..4000] {
        *b = 0xFF;
    }
    std::fs::write(&seg, &bad).unwrap();
    if let Ok(index) = Index::open(&dir) {
        assert!(matches!(index.verify(), Err(Error::Corrupt(_))));
        for q in ["예산 집행", "예산집행계획", "budget plan", "주민센터프로그램"] {
            let _ = index.search(q, &SearchOptions::default());
        }
    }
    std::fs::write(&seg, &good).unwrap();
    Index::open(&dir).unwrap().verify().unwrap();

    // 목록 파일: 쓰레기·잘림·한 바이트 바뀜.
    std::fs::write(&manifest, b"garbage").unwrap();
    assert!(matches!(Index::open(&dir), Err(Error::Corrupt(_))));
    std::fs::write(&manifest, &good_manifest[..good_manifest.len() - 5]).unwrap();
    assert!(matches!(Index::open(&dir), Err(Error::Corrupt(_))));
    let mut bad = good_manifest.clone();
    bad[20] ^= 1;
    std::fs::write(&manifest, &bad).unwrap();
    assert!(matches!(Index::open(&dir), Err(Error::Corrupt(_))));
    assert!(matches!(Writer::open(&dir, WriterOptions::default()), Err(Error::Corrupt(_))));

    // 목록에 있는 조각 파일이 없음.
    std::fs::write(&manifest, &good_manifest).unwrap();
    std::fs::remove_file(&seg).unwrap();
    assert!(matches!(Index::open(&dir), Err(Error::Io(_))));
    std::fs::write(&seg, &good).unwrap();
    assert!(Index::open(&dir).unwrap().search("예산", &SearchOptions::default()).unwrap().total_docs > 0);
}

/// 목록 파일의 한 필드를 바꾸고 끝의 해시를 다시 맞춘다.
fn patch_manifest(dir: &std::path::Path, offset: usize, value: u32) {
    let p = dir.join("MANIFEST.kfm");
    let mut b = std::fs::read(&p).unwrap();
    b[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    let n = b.len() - 8;
    let h = fnv64(FNV_SEED, &b[..n]);
    b[n..].copy_from_slice(&h.to_le_bytes());
    std::fs::write(&p, &b).unwrap();
}

#[test]
fn version_mismatch_asks_for_reindex() {
    let dir = build("version");
    // 규칙 버전이 다른 색인: 열기를 거부하고, create로 비우고 다시 만들 수 있다.
    patch_manifest(&dir, 12, 999);
    match Index::open(&dir) {
        Err(Error::RuleVersion { found: 999, expected: 1 }) => {}
        other => panic!("expected RuleVersion, got {:?}", other.map(|i| i.generation())),
    }
    assert!(matches!(Writer::open(&dir, WriterOptions::default()), Err(Error::RuleVersion { .. })));
    patch_manifest(&dir, 8, 77);
    assert!(matches!(Index::open(&dir), Err(Error::FormatVersion { found: 77, .. })));

    let mut w = Writer::create(&dir, WriterOptions::default()).unwrap();
    assert_eq!(w.generation(), 0);
    assert_eq!(Index::open(&dir).unwrap().num_sentences(), 0);
    for d in &corpus(1) {
        add(&mut w, d);
    }
    w.commit().unwrap();
    assert_eq!(segment_files(&dir).len(), 1, "old segment was cleaned up");
    check_all(&Index::open(&dir).unwrap(), &expect(1), "recreated");
}

#[test]
fn second_writer_is_locked_out() {
    let dir = build("lock");
    let w = Writer::open(&dir, WriterOptions::default()).unwrap();
    assert!(matches!(Writer::open(&dir, WriterOptions::default()), Err(Error::Locked)));
    // 읽기는 쓰는 쪽이 있어도 된다.
    assert!(Index::open(&dir).unwrap().num_sentences() > 0);
    drop(w);
    Writer::open(&dir, WriterOptions::default()).unwrap();
}

#[test]
fn leftovers_of_an_interrupted_write_are_ignored_and_cleaned() {
    let dir = build("leftover");
    let before = Index::open(&dir).unwrap().search("예산 집행", &SearchOptions::default()).unwrap().total_docs;
    // 쓰다 죽은 흔적: 임시 조각, 목록에 오르지 못한 조각, 임시 목록.
    let seg = segment_files(&dir).pop().unwrap();
    std::fs::write(dir.join("seg-00000099.kfs.tmp"), b"half written").unwrap();
    std::fs::copy(&seg, dir.join("seg-00000098.kfs")).unwrap();
    std::fs::write(dir.join("MANIFEST.tmp-1-5"), b"half written manifest").unwrap();
    let index = Index::open(&dir).unwrap();
    assert_eq!(index.search("예산 집행", &SearchOptions::default()).unwrap().total_docs, before);
    drop(index);
    let w = Writer::open(&dir, WriterOptions::default()).unwrap();
    let names: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert!(!names.iter().any(|n| n.contains("tmp") || n.contains("00000098") || n.contains("00000099")), "{names:?}");
    drop(w);
    assert_eq!(Index::open(&dir).unwrap().search("예산 집행", &SearchOptions::default()).unwrap().total_docs, before);
}

#[test]
fn readers_always_see_a_complete_generation_while_writer_commits() {
    let dir = tmp_dir("atomic");
    let docs = corpus(1);
    let stop = Arc::new(AtomicBool::new(false));
    let (dir2, stop2) = (dir.clone(), stop.clone());
    // 읽는 스레드: 쓰는 쪽이 커밋·합치기를 되풀이하는 동안 계속 새로 열고 질의한다.
    // 문서를 10개씩 넣으므로 어느 세대든 문서 수는 10의 배수여야 한다(반쯤 쓴 상태를 보면 어긋난다).
    let reader = std::thread::spawn(move || {
        let (mut opens, mut last_generation) = (0u32, 0u64);
        let all = SearchOptions { top: 0, time_limit: None, ..SearchOptions::default() };
        while !stop2.load(Ordering::Relaxed) {
            let index = Index::open(&dir2).expect("open during commit");
            assert!(index.generation() >= last_generation, "generation went backwards");
            last_generation = index.generation();
            let sentences = index.match_sentences("가나", &all).expect("query during commit");
            let mut ids: Vec<u64> = sentences.iter().map(|m| m.doc_id).collect();
            ids.dedup();
            assert_eq!(ids.len() % 10, 0, "saw {} documents at generation {}", ids.len(), index.generation());
            opens += 1;
        }
        opens
    });
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    for (round, chunk) in docs.chunks(10).enumerate() {
        for d in chunk {
            // 모든 문서에 표지 문장을 하나 넣어 문서 수를 셀 수 있게 한다.
            let mut sents: Vec<kfind_core::Sentence> = d.sents.iter().map(|s| kfind_core::Sentence { para: s.para, level: s.level, text: &s.text }).collect();
            sents.push(kfind_core::Sentence { para: 1000, level: 8, text: "가나 표지" });
            w.add_document(d.id, d.date, &sents).unwrap();
        }
        w.commit().unwrap();
        if round % 4 == 3 {
            w.merge_all().unwrap();
        }
    }
    stop.store(true, Ordering::Relaxed);
    let opens = reader.join().expect("reader thread");
    assert!(opens > 0);
    let index = Index::open(&dir).unwrap();
    assert_eq!(index.search("가나", &SearchOptions { top: 0, ..SearchOptions::default() }).unwrap().total_docs, 140);
}
