use std::os::windows::process::CommandExt;
use std::{
    process::{Child, Command, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::ClientOptions,
};
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[tokio::test]
async fn executable_user_mode_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let sid = kfind::pipe::current_sid().unwrap();
    let mut writer =
        kfind_core::Writer::create(&dir.path().join(&sid), Default::default()).unwrap();
    writer
        .add_document(
            77,
            0,
            &[kfind_core::Sentence {
                para: 2,
                level: 1,
                text: "예산 집행 계획",
            }],
        )
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    let name = format!(r"\\.\pipe\kfind-exe-test-{}", std::process::id());
    let mut process = Process(
        Command::new(env!("CARGO_BIN_EXE_kfind"))
            .arg("--user")
            .arg("--index-root")
            .arg(dir.path())
            .arg("--pipe-name")
            .arg(&name)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .creation_flags(0x08000000)
            .spawn()
            .unwrap(),
    );
    let mut client = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            match ClientOptions::new().open(&name) {
                Ok(client) => break client,
                Err(e) if matches!(e.raw_os_error(), Some(2 | 231)) => {
                    assert!(
                        process.0.try_wait().unwrap().is_none(),
                        "user mode exited before binding"
                    );
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Err(e) => panic!("pipe connect: {e}"),
            }
        }
    })
    .await
    .unwrap();
    let bytes = br#"{"command":"query","id":7,"text":"\uC608\uC0B0","top":20}"#;
    client.write_u32_le(bytes.len() as u32).await.unwrap();
    client.write_all(bytes).await.unwrap();
    let length = client.read_u32_le().await.unwrap();
    assert!(length < 1_000_000);
    let mut reply = vec![0; length as usize];
    client.read_exact(&mut reply).await.unwrap();
    let reply: serde_json::Value = serde_json::from_slice(&reply).unwrap();
    assert_eq!(reply["ok"], true);
    assert_eq!(reply["result"]["hits"][0]["doc_id"], 77);
}

/// 요청 하나 = 연결 하나. 파이프가 바쁘면(231) 잠깐 기다려 다시 연다.
async fn call(name: &str, request: &serde_json::Value) -> std::io::Result<serde_json::Value> {
    let mut client = loop {
        match ClientOptions::new().open(name) {
            Ok(c) => break c,
            Err(e) if e.raw_os_error() == Some(231) => tokio::time::sleep(Duration::from_millis(5)).await,
            Err(e) => return Err(e),
        }
    };
    let bytes = serde_json::to_vec(request).unwrap();
    client.write_u32_le(bytes.len() as u32).await?;
    client.write_all(&bytes).await?;
    let length = client.read_u32_le().await?;
    assert!(length < 16_000_000);
    let mut reply = vec![0; length as usize];
    client.read_exact(&mut reply).await?;
    Ok(serde_json::from_slice(&reply).unwrap())
}

/// KFS1: 빈 색인 폴더로 띄운 실행 파일(--user)에 실제 파이프로 문서를 넣고(64KB 넘는 문서는 조각으로) 찾고 지운다.
#[tokio::test]
async fn executable_user_mode_ingest_roundtrip() {
    use serde_json::json;
    let dir = tempfile::tempdir().unwrap();
    let name = format!(r"\\.\pipe\kfind-exe-ingest-{}", std::process::id());
    let mut process = Process(
        Command::new(env!("CARGO_BIN_EXE_kfind"))
            .arg("--user")
            .arg("--index-root")
            .arg(dir.path())
            .arg("--pipe-name")
            .arg(&name)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .creation_flags(0x08000000)
            .spawn()
            .unwrap(),
    );
    let status = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            match call(&name, &json!({"command":"status"})).await {
                Ok(v) => break v,
                Err(e) if matches!(e.raw_os_error(), Some(2 | 231)) => {
                    assert!(process.0.try_wait().unwrap().is_none(), "user mode exited before binding");
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Err(e) => panic!("pipe connect: {e}"),
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(status["ok"], true);
    assert_eq!(status["result"]["mode"], "user");
    assert_eq!(status["result"]["index_available"], false);
    assert_eq!(status["result"]["docs"], 0);

    // 64KB 넘는 문서: 문단 300개(약 120KB)를 4조각으로.
    let filler = "가나다라마바사아자차카타파하 ".repeat(9);
    let paras: Vec<_> = (0..300u32)
        .map(|p| {
            let t = if p == 250 { format!("{filler}끝에 있는 예산 집행 문장.") } else { filler.clone() };
            json!({"p":p,"lv":if p == 0 { 0 } else { 8 },"t":t})
        })
        .collect();
    let head = json!({"command":"ingest","doc_key":"big","path":r"D:\사본\큰문서.hwpx","size":123,"mtime":7,"date":1_700_000_000_000i64,
        "ref_date":"2026-11-13","scopes":["tm","box:9"],"level_src":"py","level_rule":4});
    let whole = {
        let mut v = head.clone();
        v["paras"] = json!(paras);
        v
    };
    assert!(serde_json::to_vec(&whole).unwrap().len() > 65_536);
    assert!(call(&name, &whole).await.is_err(), "unsplit request over 64KB closes the connection");
    let chunks: Vec<_> = paras.chunks(75).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        let mut v = head.clone();
        v["paras"] = json!(chunk);
        v["part"] = json!(i);
        v["parts"] = json!(chunks.len());
        assert!(serde_json::to_vec(&v).unwrap().len() < 65_536);
        let r = call(&name, &v).await.unwrap();
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(r["result"]["indexed"], i + 1 == chunks.len());
    }
    let small = json!({"command":"ingest","doc_key":"memo","path":"","size":0,"mtime":0,"date":5,"scopes":["nb:3:src:1"],
        "level_src":"rs","level_rule":0,"paras":[{"p":0,"lv":2,"t":"예산 집행 메모. ※"}]});
    assert_eq!(call(&name, &small).await.unwrap()["ok"], true);

    let q = call(&name, &json!({"command":"query","id":1,"text":"예산 집행","scope":"box:9","refs":3})).await.unwrap();
    assert_eq!(q["ok"], true, "{q}");
    let hit = &q["result"]["hits"][0];
    assert_eq!(q["result"]["total_docs"], 1);
    assert_eq!(hit["doc_key"], "big");
    assert_eq!(hit["path"], r"D:\사본\큰문서.hwpx");
    let s = &hit["sentences"][0];
    assert_eq!(s["para"], 250);
    // 문단 250 = 채움 글(마침표 없음) + "끝에 있는 … 문장." → 한 문장. 앞에 문장 하나("간단. ")를 둔 문단으로 위치를 따로 본다.
    let text = format!("{filler}끝에 있는 예산 집행 문장.");
    assert_eq!((s["start"].as_u64(), s["len"].as_u64()), (Some(0), Some(text.trim_end().chars().count() as u64)));
    let small2 = json!({"command":"ingest","doc_key":"pos","path":"pos.txt","size":1,"mtime":1,"date":1,"scopes":["box:9"],
        "level_src":"py","level_rule":4,"paras":[{"p":4,"lv":3,"t":"간단. ※ 그리고 예산 집행 문장."}]});
    assert_eq!(call(&name, &small2).await.unwrap()["ok"], true);
    let q = call(&name, &json!({"command":"query","id":9,"text":"예산 집행","scope":"box:9","refs":3})).await.unwrap();
    let pos = q["result"]["hits"].as_array().unwrap().iter().find(|h| h["doc_key"] == "pos").unwrap();
    let s = &pos["sentences"][0];
    let t = "간단. ※ 그리고 예산 집행 문장.";
    let got: String = t.chars().skip(s["start"].as_u64().unwrap() as usize).take(s["len"].as_u64().unwrap() as usize).collect();
    assert_eq!((s["para"].as_u64(), s["level"].as_u64(), got.as_str()), (Some(4), Some(3), "※ 그리고 예산 집행 문장."));
    assert_eq!(call(&name, &json!({"command":"remove","doc_keys":["pos"]})).await.unwrap()["result"]["removed"], 1);

    let c = call(&name, &json!({"command":"count","id":2,"text":"예산 집행"})).await.unwrap();
    assert_eq!((c["result"]["total_docs"].as_u64(), c["result"]["total_sentences"].as_u64()), (Some(2), Some(2)));
    let st = call(&name, &json!({"command":"stale","level_rule":4})).await.unwrap();
    assert_eq!(st["result"], json!({"doc_keys":["memo"],"more":false}));
    // 같은 파일 판의 py 문서를 rs가 덮는 것은 거절.
    let mut down = head.clone();
    down["level_src"] = json!("rs");
    down["paras"] = json!([]);
    let r = call(&name, &down).await.unwrap();
    assert_eq!(r["ok"], false);
    assert!(r["error"].as_str().unwrap().starts_with("rejected:"), "{r}");

    let r = call(&name, &json!({"command":"remove","doc_keys":["big"]})).await.unwrap();
    assert_eq!(r["result"]["removed"], 1);
    let q = call(&name, &json!({"command":"query","id":3,"text":"예산 집행"})).await.unwrap();
    assert_eq!(q["result"]["hits"][0]["doc_key"], "memo");
    assert_eq!(q["result"]["total_docs"], 1);
    let status = call(&name, &json!({"command":"status"})).await.unwrap();
    assert_eq!(status["result"]["docs"], 1);
    assert_eq!(status["result"]["level_rule_counts"], json!({"rs":1}));
    assert_eq!(status["result"]["uploads"], 0);
}
