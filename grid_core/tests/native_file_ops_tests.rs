//! 합성 파일만 사용하며 실제 휴지통·클립보드는 절대 호출하지 않는다.
use grid_core::{file_ops::*, items::Doc};
use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static SERIAL: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "nativeops1-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn file(&self, name: &str) -> Doc {
        let path = self.0.join(name);
        std::fs::write(&path, b"synthetic source").unwrap();
        Doc {
            path: path.to_string_lossy().into_owned(),
            name: name.into(),
            ext: path.extension().unwrap().to_string_lossy().into_owned(),
            size: 16,
            mtime: 0.0,
            cloud: false,
        }
    }
    fn folder(&self) -> PathBuf {
        let p = self.0.join("destination");
        std::fs::create_dir(&p).unwrap();
        p
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let base = std::env::temp_dir().canonicalize().unwrap();
        if self.0.canonicalize().is_ok_and(|p| {
            p.starts_with(base)
                && p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("nativeops1-")
        }) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

#[derive(Default)]
struct Fake {
    recycled: Mutex<Vec<PathBuf>>,
    copied: Arc<Mutex<Vec<PathBuf>>>,
    reject_recycle: bool,
    reject_clipboard: bool,
}
impl Backend for Fake {
    fn recycle(&self, p: &Path) -> Result<(), String> {
        if self.reject_recycle {
            return Err("가짜: 휴지통을 사용할 수 없음".into());
        }
        // 삭제 대신 같은 임시 폴더 안의 가짜 휴지통 파일로 이동한다.
        let target = p.with_extension("fake-recycle");
        assert!(!target.exists());
        std::fs::rename(p, &target).map_err(|e| e.to_string())?;
        self.recycled.lock().unwrap().push(target);
        Ok(())
    }
    fn clipboard(&self, p: &[PathBuf]) -> Result<(), String> {
        if self.reject_clipboard {
            return Err("가짜: 클립보드 잠김".into());
        }
        self.copied.lock().unwrap().extend_from_slice(p);
        Ok(())
    }
}
fn request(op: Operation, docs: Vec<Doc>, destination: Option<PathBuf>, name: &str) -> Request {
    Request {
        op,
        docs,
        destination,
        name: name.into(),
        extensions: vec!["pdf".into(), "jpg".into()],
    }
}
fn run(request: &Request, backend: &impl Backend) -> Vec<FileResult> {
    let mut got = Vec::new();
    execute(request, backend, |r| got.push(r));
    got
}

#[test]
fn copy_collision_uses_numbered_name_and_never_overwrites() {
    let t = Temp::new();
    let source = t.file("한글.pdf");
    let folder = t.folder();
    std::fs::write(folder.join("한글.pdf"), b"existing 1").unwrap();
    std::fs::write(folder.join("한글 (2).pdf"), b"existing 2").unwrap();
    std::fs::create_dir(folder.join("한글 (3).pdf")).unwrap();
    let r = run(
        &request(
            Operation::Copy,
            vec![source.clone()],
            Some(folder.clone()),
            "",
        ),
        &Fake::default(),
    );
    assert!(r[0].error.is_none(), "{:?}", r);
    assert_eq!(r[0].target.as_ref().unwrap().name, "한글 (4).pdf");
    assert_eq!(
        std::fs::read(folder.join("한글.pdf")).unwrap(),
        b"existing 1"
    );
    assert_eq!(
        std::fs::read(folder.join("한글 (2).pdf")).unwrap(),
        b"existing 2"
    );
    assert!(Path::new(&source.path).exists());
}

#[test]
fn move_collision_is_exclusive_and_same_folder_is_refused() {
    let t = Temp::new();
    let source = t.file("a.pdf");
    let folder = t.folder();
    std::fs::write(folder.join("a.pdf"), b"existing").unwrap();
    let r = run(
        &request(
            Operation::Move,
            vec![source.clone()],
            Some(folder.clone()),
            "",
        ),
        &Fake::default(),
    );
    assert!(r[0].error.is_none(), "{:?}", r);
    assert_eq!(r[0].target.as_ref().unwrap().name, "a (2).pdf");
    assert!(!Path::new(&source.path).exists());
    assert_eq!(std::fs::read(folder.join("a.pdf")).unwrap(), b"existing");
    let r = run(
        &request(
            Operation::Move,
            vec![r[0].target.clone().unwrap()],
            Some(folder),
            "",
        ),
        &Fake::default(),
    );
    assert!(r[0].error.as_deref().unwrap().contains("이미"));
}

#[test]
fn rename_keeps_extension_and_collision_preserves_both_files() {
    let t = Temp::new();
    let source = t.file("a.PDF");
    let other = t.file("taken.PDF");
    let r = run(
        &request(Operation::Rename, vec![source.clone()], None, "taken"),
        &Fake::default(),
    );
    assert!(r[0].error.is_some());
    assert!(Path::new(&source.path).exists() && Path::new(&other.path).exists());
    let r = run(
        &request(Operation::Rename, vec![source], None, "renamed.pdf"),
        &Fake::default(),
    );
    assert!(r[0].error.is_none(), "{:?}", r);
    assert_eq!(r[0].target.as_ref().unwrap().name, "renamed.PDF");
}

#[test]
fn invalid_names_never_change_source() {
    let t = Temp::new();
    let source = t.file("a.pdf");
    for name in [
        "",
        " ",
        ".",
        "..",
        "abc.",
        "abc ",
        "../escape",
        "a/b",
        "a\\b",
        "a:b",
        "a?b",
        "a\0b",
        "CON",
        "con.any",
        "LPT1",
        "COM¹",
        "CONOUT$",
    ] {
        assert!(
            renamed_name(Path::new(&source.path), name).is_err(),
            "{name:?}"
        );
    }
    assert!(renamed_name(Path::new(&source.path), &"가".repeat(256)).is_err());
    assert_eq!(
        renamed_name(Path::new(&source.path), "보고서.exe").unwrap(),
        "보고서.exe.pdf"
    );
    assert!(Path::new(&source.path).exists());
}

#[test]
fn refusing_fake_recycle_leaves_source_and_no_fallback_delete_exists() {
    let t = Temp::new();
    let source = t.file("a.pdf");
    let backend = Fake {
        reject_recycle: true,
        ..Default::default()
    };
    let r = run(
        &request(Operation::Delete, vec![source.clone()], None, ""),
        &backend,
    );
    assert!(r[0].error.is_some());
    assert!(Path::new(&source.path).exists());
    assert!(backend.recycled.lock().unwrap().is_empty());
}

#[test]
fn fake_recycle_succeeds_and_bad_files_do_not_stop_following_files() {
    let t = Temp::new();
    let good = t.file("good.jpg");
    let bad = t.file("bad.exe");
    let missing = Doc {
        path: t.0.join("missing.pdf").to_string_lossy().into_owned(),
        ..good.clone()
    };
    let backend = Fake::default();
    let r = run(
        &request(
            Operation::Delete,
            vec![bad.clone(), good.clone(), missing],
            None,
            "",
        ),
        &backend,
    );
    assert!(r[0].error.is_some());
    assert!(r[1].error.is_none());
    assert!(r[2].error.is_some());
    assert!(Path::new(&bad.path).exists());
    assert!(!Path::new(&good.path).exists());
    assert_eq!(backend.recycled.lock().unwrap().len(), 1);
}

#[test]
fn cloud_only_copy_and_clipboard_are_refused_without_hydration() {
    let t = Temp::new();
    let mut cloud = t.file("cloud.pdf");
    cloud.cloud = true;
    let folder = t.folder();
    for op in [Operation::Copy, Operation::Clipboard] {
        let backend = Fake::default();
        let r = run(
            &request(op, vec![cloud.clone()], Some(folder.clone()), ""),
            &backend,
        );
        assert!(r[0].error.as_deref().unwrap().contains("클라우드"));
        assert!(backend.copied.lock().unwrap().is_empty());
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
    }
}

#[test]
fn clipboard_validates_each_file_and_failure_is_reported_for_each_valid_file() {
    let t = Temp::new();
    let good = t.file("a.pdf");
    let bad = t.file("bad.exe");
    let backend = Fake::default();
    let req = request(
        Operation::Clipboard,
        vec![bad.clone(), good.clone()],
        None,
        "",
    );
    let r = run(&req, &backend);
    assert!(r[0].error.is_some() && r[1].error.is_none());
    assert_eq!(*backend.copied.lock().unwrap(), [PathBuf::from(&good.path)]);
    let r = run(
        &req,
        &Fake {
            reject_clipboard: true,
            ..Default::default()
        },
    );
    assert!(r.iter().all(|r| r.error.is_some()));
}

struct CrossVolume {
    fail_source_remove: bool,
    fail_copy: bool,
}
impl Backend for CrossVolume {
    fn recycle(&self, _: &Path) -> Result<(), String> {
        panic!("not recycle")
    }
    fn clipboard(&self, _: &[PathBuf]) -> Result<(), String> {
        panic!("not clipboard")
    }
    fn rename(&self, _: &Path, _: &Path) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::CrossesDevices))
    }
    fn copy(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.fail_copy {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        // 실제 배타 복사 구현을 쓰도록 기본 Backend를 호출한다.
        Fake::default().copy(from, to)
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        if self.fail_source_remove
            && path.file_name().unwrap() == "source.pdf"
            && !path.parent().unwrap().ends_with("destination")
        {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        std::fs::remove_file(path)
    }
}

#[test]
fn cross_volume_move_removes_source_only_after_copy_completes() {
    let t = Temp::new();
    let source = t.file("source.pdf");
    let folder = t.folder();
    let r = run(
        &request(
            Operation::Move,
            vec![source.clone()],
            Some(folder.clone()),
            "",
        ),
        &CrossVolume {
            fail_source_remove: false,
            fail_copy: false,
        },
    );
    assert!(r[0].error.is_none());
    assert!(!Path::new(&source.path).exists());
    assert_eq!(
        std::fs::read(folder.join("source.pdf")).unwrap(),
        b"synthetic source"
    );
}

#[test]
fn cross_volume_copy_or_source_remove_failure_keeps_original_and_rolls_back_copy() {
    for (fail_copy, fail_remove) in [(true, false), (false, true)] {
        let t = Temp::new();
        let source = t.file("source.pdf");
        let folder = t.folder();
        let r = run(
            &request(
                Operation::Move,
                vec![source.clone()],
                Some(folder.clone()),
                "",
            ),
            &CrossVolume {
                fail_source_remove: fail_remove,
                fail_copy,
            },
        );
        assert!(r[0].error.is_some());
        assert!(Path::new(&source.path).exists());
        assert_eq!(std::fs::read_dir(folder).unwrap().count(), 0);
    }
}

#[test]
fn cross_volume_cloud_move_refuses_copy_and_keeps_original() {
    let t = Temp::new();
    let mut source = t.file("source.pdf");
    source.cloud = true;
    let folder = t.folder();
    let r = run(
        &request(
            Operation::Move,
            vec![source.clone()],
            Some(folder.clone()),
            "",
        ),
        &CrossVolume {
            fail_source_remove: false,
            fail_copy: false,
        },
    );
    assert!(r[0].error.as_deref().unwrap().contains("클라우드"));
    assert!(Path::new(&source.path).exists());
    assert_eq!(std::fs::read_dir(folder).unwrap().count(), 0);
}

#[test]
fn background_task_reports_results_without_waiting_for_io_on_calling_thread() {
    use std::sync::mpsc;
    struct Blocking {
        gate: Mutex<mpsc::Receiver<()>>,
    }
    impl Backend for Blocking {
        fn recycle(&self, _: &Path) -> Result<(), String> {
            self.gate.lock().unwrap().recv().unwrap();
            Err("fake blocked recycle".into())
        }
        fn clipboard(&self, _: &[PathBuf]) -> Result<(), String> {
            unreachable!()
        }
    }
    let t = Temp::new();
    let source = t.file("source.pdf");
    let (tx, rx) = mpsc::channel();
    let mut task = Task::start(
        request(Operation::Delete, vec![source.clone()], None, ""),
        Blocking {
            gate: Mutex::new(rx),
        },
    )
    .unwrap();
    assert!(!task.poll()); // worker I/O가 끝나기 전에도 호출은 즉시 돌아온다.
    tx.send(()).unwrap();
    let limit = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !task.poll() {
        assert!(std::time::Instant::now() < limit);
        std::thread::yield_now();
    }
    assert_eq!(task.results.len(), 1);
    assert!(task.results[0].error.is_some());
    assert!(Path::new(&source.path).exists());
}
