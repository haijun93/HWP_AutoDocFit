//! New command only; existing names_query remains unchanged.
use kfind::pipe::{Handler, decode};
use serde_json::json;
use std::time::{Duration, Instant};

#[test]
fn names_related_command_carries_evidence() {
    let index = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::write(data.path().join("2026 공사 계약 R&D.txt"), "").unwrap();
    std::fs::write(data.path().join("2026 공사계약 RD.txt"), "").unwrap();
    let handler = Handler::new(index.path().into());
    let call = |v| {
        handler.handle(
            "S-1-5-21-7",
            decode(&serde_json::to_vec(&v).unwrap()).unwrap(),
        )
    };
    call(json!({"command":"names_build","roots":[data.path()]})).unwrap();
    let started = Instant::now();
    loop {
        if call(json!({"command":"names_status"})).unwrap()["state"] == "ready" {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(10));
    }
    let result = call(
        json!({"command":"names_related","id":1,"text":";2026;; \"공사 계약\";R&D;","limit":1}),
    )
    .unwrap();
    assert_eq!(result["total"], 1);
    assert_eq!(result["related_groups"], true);
    assert_eq!(
        result["items"][0]["group_matches"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(result["items"][0]["group_matches"][2]["filename"], true);
}
