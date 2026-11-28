//! 새 전수 명령만 검사한다. 설치 서비스·사용자 색인은 사용하지 않는다.
use kfind::pipe::{Handler, decode};
use serde_json::json;
use std::time::{Duration, Instant};

#[test]
fn snapshot_exceeds_old_service_limit_and_preserves_literal_group_evidence() {
    let index = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    for i in 0..1201 {
        std::fs::write(data.path().join(format!("2026 공사 계약-{i:04}.txt")), "").unwrap();
    }
    std::fs::write(data.path().join("2026 교차금지.txt"), "공사 계약").unwrap();
    let handler = Handler::new(index.path().into());
    let call = |value| {
        handler.handle(
            "S-1-5-21-7",
            decode(&serde_json::to_vec(&value).unwrap()).unwrap(),
        )
    };
    call(json!({"command":"names_build","roots":[data.path()]})).unwrap();
    let start = Instant::now();
    while call(json!({"command":"names_status"})).unwrap()["state"] != "ready" {
        assert!(start.elapsed() < Duration::from_secs(30));
        std::thread::sleep(Duration::from_millis(10));
    }
    let old =
        call(json!({"command":"names_query","id":1,"text":"2026; 공사 계약 file:","limit":9000}))
            .unwrap();
    assert_eq!(old["items"].as_array().unwrap().len(), 1000);
    for related in [false, true] {
        let result = call(
            json!({"command":"names_snapshot","id":2,"text":"2026; 공사 계약","related":related}),
        )
        .unwrap();
        assert_eq!(result["complete"], true);
        assert_eq!(result["total"], 1201);
        assert_eq!(result["items"].as_array().unwrap().len(), 1201);
        if related {
            assert_eq!(result["related_groups"], true);
            assert!(
                result["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|row| row["group_matches"].as_array().unwrap().len() == 2)
            );
        }
    }
}
