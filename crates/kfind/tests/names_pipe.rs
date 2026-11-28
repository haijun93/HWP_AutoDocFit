//! FN1 파이프 명령 names_build·names_query·names_status(사용자 모드, 임시 폴더만 순회).
use kfind::pipe::{Handler, decode};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const SID: &str = "S-1-5-21-7";

fn call(h: &Handler, v: Value) -> kfind::Result<Value> {
    h.handle(SID, decode(&serde_json::to_vec(&v).unwrap()).unwrap())
}

fn wait_ready(h: &Handler) -> Value {
    let t = Instant::now();
    loop {
        let s = call(h, json!({"command":"names_status"})).unwrap();
        if s["state"] != "building" {
            return s;
        }
        assert!(t.elapsed() < Duration::from_secs(30), "build timeout");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn user_mode_build_query_status() {
    let idx = tempfile::tempdir().unwrap();
    let h = Handler::new(idx.path().to_path_buf());
    // 만들기 전: 상태 empty, 질의는 상태를 알려 주는 오류.
    let s = call(&h, json!({"command":"names_status"})).unwrap();
    assert_eq!(s["state"], "empty");
    let e = call(&h, json!({"command":"names_query","id":1,"text":"a"})).unwrap_err().to_string();
    assert!(e.contains("unavailable (empty)"), "{e}");
    // 사용자 모드는 roots가 필요하다.
    assert!(call(&h, json!({"command":"names_build"})).is_err());
    assert!(call(&h, json!({"command":"names_build","roots":["relative\\path"]})).is_err());

    let data = tempfile::tempdir().unwrap();
    let r = data.path();
    std::fs::create_dir_all(r.join("업무").join("2026")).unwrap();
    for (d, n) in [("업무\\2026", "예산_집행계획.hwp"), ("업무\\2026", "예산집행 결과.pdf"), ("업무", "회의록.txt"), ("", "readme.md")] {
        std::fs::write(r.join(d).join(n), b"").unwrap();
    }
    let root = r.to_string_lossy().to_string();
    let b = call(&h, json!({"command":"names_build","roots":[root]})).unwrap();
    assert_eq!(b["state"], "building");
    let s = wait_ready(&h);
    assert_eq!(s["state"], "ready", "{s}");
    assert_eq!(s["volumes"][0]["source"], "walk");
    // 뿌리 1 + 폴더 2 + 파일 4
    assert_eq!(s["entries"], 7);

    let q = call(&h, json!({"command":"names_query","id":2,"text":"예산집행"})).unwrap();
    assert_eq!(q["id"], 2);
    assert_eq!(q["total"], 2);
    let names: Vec<&str> = q["items"].as_array().unwrap().iter().map(|i| i["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"예산_집행계획.hwp") && names.contains(&"예산집행 결과.pdf"));
    assert!(q["items"][0]["dir"].as_str().unwrap().ends_with("업무\\2026"));
    // 입력 연장 → 앞 결과 안에서 거르기.
    let q = call(&h, json!({"command":"names_query","id":3,"text":"예산집행 ext:hwp"})).unwrap();
    assert_eq!(q["total"], 1);
    let q = call(&h, json!({"command":"names_query","id":4,"text":"예산집행 ext:hwp 계획"})).unwrap();
    assert_eq!((q["total"].as_u64(), q["narrowed"].as_bool()), (Some(1), Some(true)));
    // QMS2PIPE1: `;` 묶음 교집합(text 만으로, 새 필드 없음 → 구버전 요청 JSON 그대로 호환).
    let q = call(&h, json!({"command":"names_query","id":31,"text":"예산; 계획"})).unwrap();
    assert_eq!(q["total"], 1);
    let q = call(&h, json!({"command":"names_query","id":32,"text":"예산; 결과; ext:pdf"})).unwrap();
    assert_eq!(q["total"], 1);
    let q = call(&h, json!({"command":"names_query","id":33,"text":"예산; 회의"})).unwrap();
    assert_eq!(q["total"], 0);
    let q = call(&h, json!({"command":"names_query","id":34,"text":"예산집행 ext:hwp;pdf"})).unwrap();
    assert_eq!(q["total"], 2);
    assert!(call(&h, json!({"command":"names_query","id":35,"text":"a;b;c;d;e;f;g;h;i"})).is_err());
    let q = call(&h, json!({"command":"names_query","id":5,"text":"path:업무 -예산","sort":"name"})).unwrap();
    let names: Vec<&str> = q["items"].as_array().unwrap().iter().map(|i| i["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["2026", "회의록.txt"]);
    let q = call(&h, json!({"command":"names_query","id":6,"text":"folder:","limit":1})).unwrap();
    assert_eq!((q["total"].as_u64(), q["items"].as_array().unwrap().len()), (Some(3), 1));
    assert!(call(&h, json!({"command":"names_query","id":7,"text":"a","sort":"bogus"})).is_err());
    // Q-F2: 크기·날짜 순. 순회 원천은 파일 크기·고친 시각을 채운다(폴더 크기는 모름 → null, 맨 뒤).
    std::fs::write(r.join("업무").join("2026").join("예산_집행계획.hwp"), vec![0u8; 300]).unwrap();
    std::fs::write(r.join("업무").join("2026").join("예산집행 결과.pdf"), vec![0u8; 10]).unwrap();
    call(&h, json!({"command":"names_build","roots":[r.to_string_lossy()]})).unwrap();
    wait_ready(&h);
    let q = call(&h, json!({"command":"names_query","id":71,"text":"예산집행","sort":"size"})).unwrap();
    let sizes: Vec<u64> = q["items"].as_array().unwrap().iter().map(|i| i["size"].as_u64().unwrap()).collect();
    assert_eq!(sizes, vec![300, 10]);
    assert!(q["items"][0]["mtime"].as_i64().unwrap() > 0);
    let q = call(&h, json!({"command":"names_query","id":72,"text":"업무","sort":"date"})).unwrap();
    assert!(q["items"][0]["size"].is_null());
    // Q-F3: 글자 그대로 "예산_집행"이 든 이름이 먼저.
    let q = call(&h, json!({"command":"names_query","id":73,"text":"예산_집행"})).unwrap();
    assert_eq!((q["total"].as_u64(), q["items"][0]["name"].as_str()), (Some(2), Some("예산_집행계획.hwp")));
    // 다시 만들기: 새 파일이 보인다.
    std::fs::write(r.join("새문서.hwp"), b"").unwrap();
    call(&h, json!({"command":"names_build","roots":[r.to_string_lossy()]})).unwrap();
    assert_eq!(wait_ready(&h)["entries"], 8);
    let q = call(&h, json!({"command":"names_query","id":8,"text":"새문서"})).unwrap();
    assert_eq!(q["total"], 1);
}

/// QMSBUG1: 대상을 하나도 읽지 못한 색인을 `ready`로 두면 모든 파일명 질의가 오류 없이 0건이 된다.
#[test]
fn unreadable_roots_fail_instead_of_becoming_an_empty_ready_index() {
    let idx = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let h = Handler::new(idx.path().to_path_buf());
    let missing = data.path().join("없는폴더").to_string_lossy().into_owned();
    call(&h, json!({"command":"names_build","roots":[missing]})).unwrap();
    let s = wait_ready(&h);
    assert_eq!(s["state"], "failed", "{s}");
    assert!(s["last_error"].as_str().unwrap().contains("unreadable"), "{s}");
    let e = call(&h, json!({"command":"names_query","id":1,"text":"합의각서"})).unwrap_err().to_string();
    assert!(e.contains("unavailable (failed)"), "{e}");

    // 이미 쓸 수 있는 색인이 있으면 실패한 다시 만들기는 옛 색인을 유지한다.
    std::fs::write(data.path().join("합의각서(합성).hwp"), b"").unwrap();
    call(&h, json!({"command":"names_build","roots":[data.path()]})).unwrap();
    assert_eq!(wait_ready(&h)["state"], "ready");
    call(&h, json!({"command":"names_build","roots":[missing]})).unwrap();
    let s = wait_ready(&h);
    assert_eq!(s["state"], "ready", "{s}");
    assert!(s["last_error"].is_string(), "{s}");
    let q = call(&h, json!({"command":"names_query","id":2,"text":"합의각서"})).unwrap();
    assert_eq!(q["total"], 1);

    // 읽을 수 있지만 비어 있는 폴더는 실패가 아니다.
    let empty = tempfile::tempdir().unwrap();
    let h = Handler::new(idx.path().to_path_buf());
    call(&h, json!({"command":"names_build","roots":[empty.path()]})).unwrap();
    assert_eq!(wait_ready(&h)["state"], "ready");
}
