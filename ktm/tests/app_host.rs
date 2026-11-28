//! 실제 `ktm --search-host` 프로세스를 합성 색인·격리 파이프로 확인한다. 설치 서비스·사용자 색인을 쓰지 않는다.
#![cfg(windows)]
use serde_json::{Value, json};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

#[path = "../examples/unified_support/mod.rs"]
mod support;

struct Host {
    child: Child,
    input: Option<ChildStdin>,
    events: Receiver<Value>,
}

impl Host {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ktm"))
            .arg("--search-host")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let output = child.stdout.take().unwrap();
        let (tx, events) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines().map_while(Result::ok) {
                let _ = tx.send(serde_json::from_str(&line).unwrap());
            }
        });
        Self { input: child.stdin.take(), child, events }
    }
    fn send(&mut self, value: Value) {
        let input = self.input.as_mut().unwrap();
        writeln!(input, "{value}").unwrap();
        input.flush().unwrap();
    }
    /// 부분 결과를 모으고 마지막 page/error 를 돌려준다.
    fn finish(&self, generation: u64) -> (Vec<Value>, Value) {
        let mut parts = Vec::new();
        loop {
            let event = self.events.recv_timeout(Duration::from_secs(20)).unwrap();
            assert_eq!(event["generation"], generation);
            if event["event"] == "part" {
                parts.push(event);
            } else {
                return (parts, event);
            }
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn host_streams_pages_distinguishes_failures_and_exits_with_stdin() {
    let unique = format!("ktm-app-host-{}", std::process::id());
    let root = std::env::temp_dir().join(&unique);
    let corpus = support::Corpus::create(&root).unwrap();
    let documents = corpus.root.join("documents").to_string_lossy().into_owned();
    // 연관 검색 질의의 200자 제한에 조사 폴더 길이가 섞이면 안 된다.
    let long_root = format!("{documents}/{}", "긴하위폴더".repeat(30));
    let index = corpus.root.clone();
    let pipe = format!(r"\\.\pipe\{unique}");
    let server = pipe.clone();
    let fixture = std::thread::spawn(move || support::serve(corpus, &server));
    let waiting = std::time::Instant::now();
    while !index.join("server-ready").exists() {
        if fixture.is_finished() {
            panic!("합성 파이프 시작 실패: {:?}", fixture.join().unwrap());
        }
        assert!(waiting.elapsed() < Duration::from_secs(20), "합성 파이프 준비 시간 초과");
        std::thread::sleep(Duration::from_millis(10));
    }
    let start = |generation: u64, query: &str, pipe: &str, roots: Vec<&str>| {
        json!({"command":"start","generation":generation,"query":query,"roots":roots,
            "extensions":["txt"],"pipe":pipe,"index_root":index,
            "paragraphs":index.join("timeline-content.sqlite")})
    };
    let mut host = Host::spawn();

    host.send(start(1, "예산집행", &pipe, vec![&documents]));
    let (parts, mut page) = host.finish(1);
    assert_eq!(parts[0]["completed"], "파일명");
    assert_eq!(parts[0]["final_part"], false, "{}", parts[0]["errors"]);
    assert!(parts[0]["answer"]["rows"].as_array().unwrap().len() <= 20);
    assert_eq!(page["event"], "page", "{page}");
    assert_eq!(page["incomplete"], false, "{}", page["errors"]);
    let totals: u64 = page["totals"].as_array().unwrap().iter().map(|n| n.as_u64().unwrap()).sum();
    assert!(totals > 80, "{totals}");
    let first = page["answer"]["rows"].clone();
    let all_totals = page["totals"].clone();
    let mut seen = HashSet::new();
    let mut snippets = 0;
    loop {
        for row in page["answer"]["rows"].as_array().unwrap() {
            assert!(seen.insert(row["path"].as_str().unwrap().to_owned()));
            assert!(row.get("doc_id").is_none() && row.get("id").is_none());
            if row["tier"] != "파일명" && !row["snippet"].as_str().unwrap().is_empty() {
                snippets += 1;
            }
        }
        let Some(next) = page["next_cursor"].as_u64() else { break };
        host.send(json!({"command":"page","generation":1,"cursor":next}));
        page = host.finish(1).1;
    }
    assert_eq!(seen.len() as u64, totals);
    assert!(snippets > 0, "페이지 발췌가 비어 있습니다");
    host.send(json!({"command":"page","generation":1,"cursor":0}));
    assert_eq!(host.finish(1).1["answer"]["rows"], first);

    // 연관 검색(; 묶음)은 두 원천 모두 묶음 계약으로 답한다.
    let mut related = start(2, "2026; 공사 계약", &pipe, vec![&documents]);
    related["related_queries"] = json!(true);
    host.send(related);
    let page = host.finish(2).1;
    assert_eq!(page["errors"], json!([]), "{page}");
    let total: u64 = page["totals"].as_array().unwrap().iter().map(|n| n.as_u64().unwrap()).sum();
    assert_eq!(total, 4, "{page}");
    let mut long = start(3, "2026; 공사 계약", &pipe, vec![&documents, &long_root]);
    long["related_queries"] = json!(true);
    host.send(long);
    let page = host.finish(3).1;
    assert_eq!(page["errors"], json!([]), "{page}");

    // 0건은 검색 못 함과 구분한다.
    host.send(start(4, "존재하지않는낱말", &pipe, vec![&documents]));
    let page = host.finish(4).1;
    assert_eq!(page["totals"], json!([0, 0, 0]), "{page}");
    assert_eq!(page["errors"], json!([]));
    assert_eq!(page["incomplete"], false);
    assert_eq!(page["notice"]["title"], "검색 결과가 없습니다");
    assert_eq!(page["summary"], "0개");

    // 파일명 원천 실패: 위계·본문 결과는 보이고 실패는 따로 알린다.
    let missing = format!(r"\\.\pipe\{unique}-none");
    host.send(start(5, "예산집행", &missing, vec![&documents]));
    let (_, page) = host.finish(5);
    assert_eq!(page["incomplete"], true, "{page}");
    assert_eq!(page["totals"][0], 0, "{page}");
    assert!(page["totals"][1].as_u64().unwrap() + page["totals"][2].as_u64().unwrap() > 0, "{page}");
    assert!(!page["answer"]["rows"].as_array().unwrap().is_empty(), "{page}");
    assert_eq!(page["errors"].as_array().unwrap().len(), 1, "{page}");
    assert!(page["errors"][0].as_str().unwrap().starts_with("파일명"), "{page}");

    // 파일명이 실패하고 하위 단도 0건이면 '0건'이 아니라 '검색 못함'이다.
    host.send(start(6, "존재하지않는낱말", &missing, vec![&documents]));
    let (_, page) = host.finish(6);
    assert_eq!(page["incomplete"], true, "{page}");
    assert_eq!(page["totals"], json!([0, 0, 0]), "{page}");
    assert_eq!(page["summary"], "검색 못함");
    assert_eq!(page["notice"]["title"], "검색을 완료하지 못했습니다");

    // 취소한 세대와 늦게 온 옛 세대 start 는 응답하지 않는다.
    host.send(start(8, "느린검색 예산", &pipe, vec![&documents]));
    host.send(json!({"command":"cancel","generation":8}));
    host.send(start(4, "예산집행", &pipe, vec![&documents]));
    host.send(json!({"command":"page","generation":8,"cursor":0}));
    host.send(start(9, "예산집행", &pipe, vec![&documents]));
    loop {
        let event = host.events.recv_timeout(Duration::from_secs(20)).unwrap();
        // page 요청은 취소 뒤라 응답이 없다.
        assert_eq!(event["generation"], 9, "{event}");
        if event["event"] == "page" {
            break;
        }
    }

    // 조사 폴더가 상위 폴더여도 하위 폴더의 본문 일치를 찾는다.
    let parent = index.to_string_lossy().into_owned();
    host.send(start(11, "예산집행", &pipe, vec![&parent]));
    assert_eq!(host.finish(11).1["totals"], all_totals);

    // 조사 폴더 밖 문서는 결과에 없다.
    let outside = format!("{documents}-none");
    host.send(start(12, "예산집행", &pipe, vec![&outside]));
    assert_eq!(host.finish(12).1["totals"], json!([0, 0, 0]));

    host.input = None;
    for _ in 0..500 {
        if host.child.try_wait().unwrap().is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("표준 입력이 닫혀도 호스트가 끝나지 않았습니다");
}
