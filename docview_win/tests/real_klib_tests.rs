//! 실제 `klib scan` 을 자식 프로세스로 돌리는 시험(DOCVIEWWIN2). **건너뛰지 않는다**: klib.exe 가 없으면 실패한다.
//! klib 위치: `DOCFIT_KLIB` → `KLIB_EXE` → 실행 파일 옆·상위의 `klib.exe`/`resources\klib.exe`/`klib\target\…`(`scan::find_klib`).
//! 시험이 만든 임시 폴더 트리만 훑는다(사용자 문서·설정은 건드리지 않는다). 취소 시험만 이 PC 의 첫 고정 드라이브 루트를 읽기 전용으로 훑다 곧 취소한다.
use docview_win::scan::{self, Event, ScanJob};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn real_klib() -> PathBuf {
    scan::find_klib().unwrap_or_else(|| {
        panic!("klib.exe 를 찾지 못했습니다. KLIB_EXE(또는 DOCFIT_KLIB)에 klib.exe 경로를 지정하거나 resources\\klib.exe 를 두세요(CI 는 klib 를 먼저 빌드해 resources 에 복사한다).")
    })
}

/// 임시 폴더 트리(끝나면 지운다).
struct Tree(PathBuf);

impl Tree {
    fn new(tag: &str) -> Tree {
        let dir = std::env::temp_dir().join(format!("docview_win_real_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Tree(dir)
    }
    fn file(&self, rel: &str, body: &[u8]) -> &Tree {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
        self
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn names(docs: &[docview_win::model::Doc]) -> Vec<String> {
    let mut n: Vec<_> = docs.iter().map(|d| d.name.clone()).collect();
    n.sort();
    n
}

fn sample_tree(tag: &str) -> Tree {
    let t = Tree::new(tag);
    t.file("가.hwp", b"hwp")
        .file("나.PDF", b"pdf pdf")
        .file("하위 폴더/다.xlsx", b"x")
        .file("하위 폴더/깊은 폴더/라.pptx", b"slides")
        .file("제외할 폴더/마.hwpx", b"gone")
        .file("제외할 폴더/안쪽/바.xls", b"gone too")
        .file("무시.txt", b"x");
    t
}

#[test]
fn real_klib_scans_a_temp_tree_with_the_document_extensions_only() {
    let exe = real_klib();
    let t = sample_tree("scan");
    let got = scan::scan_blocking(&exe, &[t.0.clone()], &[], Duration::from_secs(60)).expect("실제 klib 스캔");
    assert!(!got.cancelled);
    assert_eq!(names(&got.docs), ["가.hwp", "나.PDF", "다.xlsx", "라.pptx", "마.hwpx", "바.xls"], "지원 확장자 6개, .txt 는 제외");
    assert_eq!(got.visited, 6);
    let d = got.docs.iter().find(|d| d.name == "나.PDF").unwrap();
    assert_eq!(d.ext.to_lowercase(), "pdf");
    assert_eq!(d.size, 7, "실제 파일 크기");
    assert!(d.mtime > 1.5e9, "수정 시각은 실제 값: {}", d.mtime);
    assert!(Path::new(&d.path).is_absolute() && Path::new(&d.path).is_file(), "경로는 실제 파일: {}", d.path);
    assert!(!d.cloud);
}

#[test]
fn real_klib_honors_excluded_folders_passed_with_exclude() {
    let exe = real_klib();
    let t = sample_tree("exclude");
    let excluded = t.0.join("제외할 폴더").display().to_string();
    let got = scan::scan_blocking(&exe, &[t.0.clone()], &[excluded], Duration::from_secs(60)).expect("실제 klib 스캔");
    assert_eq!(names(&got.docs), ["가.hwp", "나.PDF", "다.xlsx", "라.pptx"], "제외 폴더와 그 아래 폴더의 문서는 나오지 않는다");
    // 슬래시 구분자·대소문자가 달라도 같은 폴더로 본다(앱 설정에 사람이 적은 형태).
    let alt = t.0.join("제외할 폴더").display().to_string().replace('\\', "/").to_uppercase();
    let got2 = scan::scan_blocking(&exe, &[t.0.clone()], &[alt], Duration::from_secs(60)).expect("실제 klib 스캔");
    assert_eq!(names(&got2.docs).len(), 4, "대문자·슬래시 형태의 제외 경로도 적용: {:?}", names(&got2.docs));
}

#[test]
fn real_klib_nonexistent_root_is_an_empty_successful_scan() {
    let exe = real_klib();
    let got = scan::scan_blocking(&exe, &[PathBuf::from(r"Z:\docview_win_no_such_root")], &[], Duration::from_secs(30)).expect("없는 루트는 오류가 아니다");
    assert!(got.docs.is_empty());
    assert!(!got.cancelled);
}

#[test]
fn real_klib_error_exit_code_becomes_a_failed_scan() {
    let exe = real_klib();
    // 루트가 하나도 없으면 klib 는 인자 오류로 종료 코드 1 로 끝난다 → 스캔 실패로 알린다.
    let err = scan::scan_blocking(&exe, &[], &[], Duration::from_secs(30)).expect_err("종료 코드 오류는 Err");
    assert!(err.contains("종료 코드"), "종료 코드가 안내에 있어야 한다: {err}");
    // 실행 파일이 없으면 실행 실패.
    let err = scan::scan_blocking(Path::new(r"Z:\없는\klib.exe"), &[PathBuf::from(".")], &[], Duration::from_secs(5)).expect_err("실행 실패");
    assert!(err.contains("실행하지 못했습니다"), "{err}");
}

#[test]
fn real_klib_scan_can_be_cancelled_quickly_and_the_child_exits() {
    let exe = real_klib();
    let root = grid_core::gdi::fixed_drive_roots().into_iter().next().expect("고정 드라이브");
    let mut job = ScanJob::start_klib(&exe, &[root], &[], Arc::new(|| {})).expect("klib 시작");
    std::thread::sleep(Duration::from_millis(300));
    let before: Vec<Event> = job.drain();
    assert!(!before.iter().any(|e| matches!(e, Event::Finished { .. } | Event::Failed(_))), "300ms 만에 드라이브 전체를 끝낼 수는 없다");
    let t0 = Instant::now();
    job.cancel();
    let mut cancelled = None;
    while t0.elapsed() < Duration::from_secs(5) {
        for e in job.drain() {
            match e {
                Event::Finished { cancelled: c } => cancelled = Some(c),
                Event::Failed(m) => panic!("취소는 실패가 아니다: {m}"),
                Event::Batch { .. } => {}
            }
        }
        if cancelled.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(cancelled, Some(true), "취소 요청 뒤 5초 안에 Finished{{cancelled:true}}");
    job.join();
    assert!(t0.elapsed() < Duration::from_secs(5));
}
