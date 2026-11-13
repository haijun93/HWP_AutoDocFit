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
