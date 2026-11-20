//! 실제 mdm-ctx.exe 를 띄워 모음·전달을 시험한다. 가짜 앱(examples/fake_app)이 받은 인자를 파일에 남긴다.
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

fn fake_app() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    exe.parent().unwrap().parent().unwrap().join("examples").join("fake_app.exe")
}
fn setup(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mdmctx_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("files")).unwrap();
    dir
}
fn make(dir: &Path, name: &str) -> String {
    let p = dir.join("files").join(name);
    std::fs::write(&p, b"x").unwrap();
    p.to_string_lossy().into_owned()
}
fn spawn(dir: &Path, target: &str, paths: &[&str], quiet: u64) -> Child {
    Command::new(env!("CARGO_BIN_EXE_mdm-ctx"))
        .arg("--app").arg(fake_app()).arg("convert").args(paths).arg("--to").arg(target).arg("--notify")
        .env("MDM_CTX_TEST_ROOT", dir).env("MDM_FAKE_OUT", dir.join("out.txt")).env("MDM_CTX_QUIET_MS", quiet.to_string())
        .spawn().unwrap()
}
/// 가짜 앱은 대표가 끝난 뒤에도 잠깐 돌므로 `want` 건이 찍힐 때까지(최대 5초) 기다린 뒤 호출별 인자 목록을 돌려준다.
fn calls_wait(dir: &Path, want: usize) -> Vec<Vec<String>> {
    for _ in 0..100 {
        if calls(dir).len() >= want { break; }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    std::thread::sleep(std::time::Duration::from_millis(300)); // 예상보다 많이 찍히는지도 잡는다
    calls(dir)
}
fn calls(dir: &Path) -> Vec<Vec<String>> {
    let text = std::fs::read_to_string(dir.join("out.txt")).unwrap_or_default();
    let mut out = vec![];
    let mut cur = vec![];
    for line in text.lines() {
        if line == "--END--" { out.push(std::mem::take(&mut cur)); } else { cur.push(line.to_string()); }
    }
    out
}

#[test]
fn single_korean_space_path() {
    let dir = setup("one");
    let p = make(&dir, "공 백 문서.hwp");
    assert!(spawn(&dir, "pdf", &[&p], 100).wait().unwrap().success());
    let c = calls_wait(&dir, 1);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0], vec!["convert".to_string(), p, "--to".into(), "pdf".into(), "--notify".into()]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn many_processes_make_one_call_sorted() {
    let dir = setup("many");
    let names: Vec<String> = (1..=30).map(|i| format!("문서{i}.pdf")).collect();
    let paths: Vec<String> = names.iter().map(|n| make(&dir, n)).collect();
    let mut kids: Vec<Child> = paths.iter().map(|p| spawn(&dir, "pdfm", &[p], 600)).collect();
    for k in kids.iter_mut() { assert!(k.wait().unwrap().success()); }
    let c = calls_wait(&dir, 1);
    assert_eq!(c.len(), 1, "대표 1개만 전달해야 함: {c:?}");
    let got: Vec<&String> = c[0].iter().filter(|a| a.ends_with(".pdf")).collect();
    assert_eq!(got.len(), 30);
    assert!(got[0].ends_with("문서1.pdf") && got[1].ends_with("문서2.pdf") && got[9].ends_with("문서10.pdf"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn filters_duplicates_missing_and_unsupported() {
    let dir = setup("filter");
    let a = make(&dir, "a.hwp");
    let bad = make(&dir, "run.exe");
    let missing = dir.join("files").join("없음.pdf").to_string_lossy().into_owned();
    let mut k = spawn(&dir, "pdf", &[&a, &bad, &missing, &a], 100);
    assert!(k.wait().unwrap().success());
    let c = calls_wait(&dir, 1);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].iter().filter(|x| x.ends_with(".hwp")).count(), 1, "{c:?}");
    assert!(!c[0].iter().any(|x| x.ends_with(".exe") || x.ends_with("없음.pdf")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn nothing_valid_launches_nothing() {
    let dir = setup("none");
    let bad = make(&dir, "run.exe");
    assert!(spawn(&dir, "pdf", &[&bad], 50).wait().unwrap().success());
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(calls(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn separate_targets_do_not_mix() {
    let dir = setup("targets");
    let a = make(&dir, "a.hwp");
    let b = make(&dir, "b.hwp");
    let mut k1 = spawn(&dir, "pdf", &[&a], 300);
    let mut k2 = spawn(&dir, "hwpx", &[&b], 300);
    k1.wait().unwrap();
    k2.wait().unwrap();
    assert_eq!(calls_wait(&dir, 2).len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}
