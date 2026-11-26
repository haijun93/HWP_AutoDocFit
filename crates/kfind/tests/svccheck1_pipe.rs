//! 설치된 서비스와 다른 파이프/임시 색인에서 새 기능을 실제 왕복한다.
use serde_json::{Value, json};
use std::os::windows::process::CommandExt;
use std::{process::{Child, Command, Stdio}, time::Duration};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::windows::named_pipe::ClientOptions};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
async fn request(pipe: &str, value: Value) -> Value {
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut client = loop {
            match ClientOptions::new().open(pipe) {
                Ok(c) => break c,
                Err(e) if matches!(e.raw_os_error(), Some(2 | 231)) => tokio::time::sleep(Duration::from_millis(10)).await,
                Err(e) => panic!("isolated pipe: {e}"),
            }
        };
        let bytes = serde_json::to_vec(&value).unwrap();
        client.write_u32_le(bytes.len() as u32).await.unwrap();
        client.write_all(&bytes).await.unwrap();
        let len = client.read_u32_le().await.unwrap();
        assert!(len < 1_000_000);
        let mut bytes = vec![0; len as usize];
        client.read_exact(&mut bytes).await.unwrap();
        let response: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(response["ok"], true);
        response["result"].clone()
    }).await.unwrap()
}
#[tokio::test]
async fn names_list_cursor_metadata_and_capabilities_over_isolated_pipe() {
    let index = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    std::fs::write(data.path().join("synthetic.hwpx"), b"synthetic").unwrap();
    std::fs::write(data.path().join("synthetic.jpg"), b"synthetic-photo").unwrap();
    std::fs::write(data.path().join("ignored.log"), b"ignored").unwrap();
    let pipe = format!(r"\\.\pipe\SVCCHECK1-{}", std::process::id());
    let _process = Process(Command::new(env!("CARGO_BIN_EXE_kfind"))
        .args(["--user", "--pipe-name", &pipe, "--index-root"])
        .arg(index.path()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .creation_flags(0x08000000).spawn().unwrap());
    let status = request(&pipe, json!({"command":"names_status"})).await;
    assert_eq!(status["version"], 2);
    assert!(status["features"].as_array().unwrap().contains(&json!("names_list")));
    request(&pipe, json!({"command":"names_build", "roots":[data.path()]})).await;
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let s = request(&pipe, json!({"command":"names_status"})).await;
            if s["state"] == "ready" { break; }
            assert_eq!(s["state"], "building");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    let mut cursor = 0;
    let mut rows = Vec::new();
    loop {
        // 새 필드가 없는 기존 요청도 계속 동작한다.
        let r = request(&pipe, json!({"command":"names_list", "extensions":["hwpx","jpg"], "offset":cursor, "limit":1})).await;
        rows.extend(r["items"].as_array().unwrap().iter().cloned());
        let next = r["next"].as_u64().unwrap();
        assert!(next >= cursor);
        if r["done"] == true { break; }
        assert!(next > cursor);
        cursor = next;
    }
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0]["path"], rows[1]["path"]);
    for row in rows {
        assert!(row["size"].as_u64().unwrap() > 0);
        assert!(row["mtime"].as_f64().unwrap() > 1_700_000_000.0);
        assert!(row["mtime_ns"].as_i64().unwrap() > 0);
        assert_eq!(row["cloud"], false);
    }
}
