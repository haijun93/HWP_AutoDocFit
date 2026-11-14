//! 다른 프로세스에서 같은 색인을 mmap으로 읽는 시험(KB3): 쓰는 프로세스가 커밋(목록 파일 원자적 교체)·합치기
//! (조각 파일 지우기)를 되풀이하는 동안, 읽는 프로세스 둘이 계속 열고·다시 읽고·질의한다.
//!
//! 읽는 프로세스는 이 시험 실행 파일 자신을 `child_reader` 시험 하나만 돌게 띄운 것이다(환경 변수로 폴더를 받는다).
//! 환경 변수가 없으면 `child_reader`는 아무것도 하지 않는다(보통 실행에서는 통과만 한다).
mod common;
use common::*;
use kfind_core::{Index, SearchOptions, Sentence, Writer, WriterOptions};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const ENV_DIR: &str = "KFIND_MP_DIR";
const STOP: &str = "STOP-READERS";

fn all() -> SearchOptions {
    SearchOptions { top: 0, time_limit: None, ..SearchOptions::default() }
}

/// 표지 낱말 "가나"가 든 문서 수와 세대. 쓰는 쪽은 10개씩 넣으므로 어느 세대든 10의 배수여야 한다.
fn marker_docs(index: &Index) -> usize {
    let found = index.search("가나", &all()).expect("query during commit");
    let n = found.total_docs;
    assert_eq!(n % 10, 0, "saw {n} documents at generation {} (half-written state?)", index.generation());
    // 순위 경로와 개수 경로도 같은 세대에서 같아야 한다.
    assert_eq!(index.count("가나", &all()).expect("count").total_docs, n);
    n
}

/// 읽는 프로세스 본체. 끝나면 "MP-READER opens=.. reloads=.. generations=.. pinned_ok=1"을 출력한다.
#[test]
fn child_reader() {
    let Ok(dir) = std::env::var(ENV_DIR) else { return };
    let dir = Path::new(&dir);
    // 처음 연 세대를 끝까지 쥐고 있는다: 쓰는 쪽이 그 조각을 합쳐 목록에서 빼고 지우려 해도 결과가 같아야 한다.
    let pinned = loop {
        match Index::open(dir) {
            Ok(i) if i.generation() > 0 => break i,
            Ok(_) => std::thread::sleep(Duration::from_millis(1)),
            Err(e) => panic!("open: {e}"),
        }
    };
    let pinned_docs = marker_docs(&pinned);
    let pinned_full = pinned.search("예산 집행", &all()).expect("pinned query");
    let mut live = Index::open(dir).expect("open");
    let (mut opens, mut reloads, mut generations, mut last_gen) = (0u32, 0u32, 0u32, live.generation());
    let mut last_docs = marker_docs(&live);
    let deadline = Instant::now() + Duration::from_secs(120);
    while !dir.join(STOP).exists() {
        assert!(Instant::now() < deadline, "reader ran too long");
        // ① 새로 열기
        let fresh = Index::open(dir).expect("open during commit");
        assert!(fresh.generation() >= last_gen, "generation went backwards: {} < {last_gen}", fresh.generation());
        let n = marker_docs(&fresh);
        opens += 1;
        // ② 쥐고 있던 색인 다시 읽기(이미 연 조각은 다시 쓰고, 지워진 조각만 놓는다)
        if live.reload().expect("reload during commit") {
            reloads += 1;
            generations += 1;
        }
        assert!(live.generation() >= last_gen);
        let m = marker_docs(&live);
        // 같은 세대면 같은 답, 세대가 오르면 문서 수는 줄지 않는다(쓰는 쪽은 넣기만 한다).
        if live.generation() == fresh.generation() {
            assert_eq!(m, n);
        }
        assert!(m >= last_docs, "documents went backwards: {m} < {last_docs}");
        last_gen = live.generation();
        last_docs = m;
    }
    // 쥐고 있던 옛 세대는 처음과 똑같이 답한다(조각 파일이 목록에서 빠졌어도 매핑은 살아 있다).
    assert_eq!(marker_docs(&pinned), pinned_docs);
    let again = pinned.search("예산 집행", &all()).expect("pinned query at end");
    assert_eq!((again.total_sentences, again.total_docs, again.hits), (pinned_full.total_sentences, pinned_full.total_docs, pinned_full.hits));
    live.reload().expect("final reload");
    println!("MP-READER opens={opens} reloads={reloads} generations={generations} final_generation={} final_docs={} pinned_generation={} pinned_ok=1", live.generation(), marker_docs(&live), pinned.generation());
}

#[test]
fn other_processes_read_while_writer_replaces_atomically() {
    let dir = tmp_dir("multiprocess");
    let docs = corpus(1);
    let add_round = |w: &mut Writer, chunk: &[Doc]| {
        for d in chunk {
            let mut sents: Vec<Sentence> = d.sents.iter().map(|s| Sentence { para: s.para, level: s.level, text: &s.text }).collect();
            sents.push(Sentence { para: 1000, level: 8, text: "가나 표지" });
            w.add_document(d.id, d.date, &sents).unwrap();
        }
    };
    let mut w = Writer::open(&dir, WriterOptions::default()).unwrap();
    add_round(&mut w, &docs[..10]);
    w.commit().unwrap();
    let exe = std::env::current_exe().unwrap();
    let readers: Vec<_> = (0..2)
        .map(|_| {
            Command::new(&exe)
                .args(["--exact", "child_reader", "--nocapture", "--test-threads", "1"])
                .env(ENV_DIR, &dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn reader process")
        })
        .collect();
    // 읽는 프로세스가 첫 세대를 쥘 시간을 준 뒤 커밋·합치기를 되풀이한다. 합치기는 읽는 쪽이 매핑한 조각을
    // 목록에서 빼고 지운다. 지우기가 거부되면(옛 Windows 동작) 다음 정리로 미뤄지고, 이름만 지워지면(POSIX 지우기,
    // 이 PC Windows 11에서 확인) 매핑은 그대로 살아 있다 — 어느 쪽이든 쥐고 있던 세대의 답은 같아야 한다.
    std::thread::sleep(Duration::from_millis(300));
    let mut commits = 0;
    for _pass in 0..3 {
        for (round, chunk) in docs[10..].chunks(10).enumerate() {
            add_round(&mut w, chunk);
            w.commit().unwrap();
            commits += 1;
            if round % 3 == 2 {
                w.merge_all().unwrap();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    w.merge_all().unwrap();
    // 기록용: 디스크에 남은 조각 파일 수(지우기가 미뤄졌으면 목록보다 많다).
    let on_disk = std::fs::read_dir(&dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("seg-")).count();
    eprintln!("MP-WRITER commits={commits} segment_files_on_disk={on_disk} listed={}", w.num_segments());
    std::fs::write(dir.join(STOP), b"").unwrap();
    for child in readers {
        let out = child.wait_with_output().expect("reader process");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "reader process failed:\n{stdout}\n{}", String::from_utf8_lossy(&out.stderr));
        // 시험 틀이 "test child_reader ... "를 같은 줄 앞에 찍으므로 줄 안에서 찾는다.
        let line = stdout.lines().find_map(|l| l.find("MP-READER").map(|i| &l[i..])).unwrap_or_else(|| panic!("no reader summary:\n{stdout}"));
        eprintln!("{line} (writer commits {commits})");
        // 읽는 쪽이 실제로 여러 세대를 거쳐 보았어야 시험이 뜻이 있다.
        let reloads: u32 = line.split("reloads=").nth(1).and_then(|s| s.split(' ').next()).and_then(|s| s.parse().ok()).unwrap();
        assert!(reloads >= 2, "reader saw too few generations: {line}");
        assert!(line.contains("final_docs=140"), "{line}");
    }
    std::fs::remove_file(dir.join(STOP)).unwrap();
    // 읽는 프로세스가 모두 끝난 뒤 다음 커밋에서 미뤄 둔 옛 조각 파일이 정리된다.
    add_round(&mut w, &docs[..10]);
    w.commit().unwrap();
    let index = Index::open(&dir).unwrap();
    assert_eq!(marker_docs(&index), 140);
    let seg_files = std::fs::read_dir(&dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("seg-")).count();
    assert_eq!(seg_files, index.num_segments(), "stale segment files left after readers exited");
}
