//! `klib scan --json-lines` 결과 파서와 비동기 스캔 작업(진행·취소). 창과 무관해 시험할 수 있다.
//! JSON 줄 형식: `klib/src/scan_cli.rs::emit` — `{"path","name","ext","size","mtime","mtime_ns","cloud"}`(Python `rust_scan.scan` 과 같은 필수 키).
use crate::json::{self, Value};
use crate::model::Doc;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// 문서뷰가 훑는 확장자(Python `DOC_EXTENSIONS` 와 같다).
pub const DOC_EXTENSIONS: [&str; 7] = ["hwp", "hwpx", "xls", "xlsx", "ppt", "pptx", "pdf"];

/// 한 줄 → 문서. 필수 키(path·name·ext·size·mtime·cloud)가 없거나 형식이 틀리면 오류(Python 파서와 같은 거절 기준).
pub fn parse_line(line: &str) -> Result<Doc, String> {
    let v = json::parse(line)?;
    let Value::Obj(_) = v else { return Err("JSON 줄이 객체가 아닙니다".into()) };
    let text = |k: &str| v.get(k).ok_or_else(|| format!("필수 키 없음: {k}")).and_then(|x| x.as_str().map(str::to_string).ok_or_else(|| format!("{k} 는 글이어야 합니다")));
    let num = |k: &str| v.get(k).ok_or_else(|| format!("필수 키 없음: {k}")).and_then(|x| x.as_f64().ok_or_else(|| format!("{k} 는 숫자여야 합니다")));
    let path = text("path")?;
    let name = text("name")?;
    let ext = text("ext")?.trim_start_matches('.').to_lowercase();
    let size = num("size")?;
    let mtime = num("mtime")?;
    let cloud = v.get("cloud").ok_or("필수 키 없음: cloud")?.as_bool().ok_or("cloud 는 참/거짓이어야 합니다")?;
    if path.is_empty() || name.is_empty() {
        return Err("path·name 이 비어 있습니다".into());
    }
    if size < 0.0 || !size.is_finite() {
        return Err("size 가 음수입니다".into());
    }
    Ok(Doc { path, name, ext, size: size as u64, mtime, cloud })
}

/// 경로의 부모 폴더(Windows 구분자 기준).
pub fn parent_folder(path: &str) -> String {
    path.rsplit_once(['\\', '/']).map(|(p, _)| p.to_string()).unwrap_or_default()
}

#[derive(Debug)]
pub enum Event {
    /// 새로 훑은 문서들과 지금까지 훑은 수·현재 폴더.
    Batch { docs: Vec<Doc>, visited: usize, folder: String },
    Finished { cancelled: bool },
    Failed(String),
}

const BATCH_ROWS: usize = 500;
const BATCH_TIME: Duration = Duration::from_millis(80);

/// 줄 수신기를 끝까지(또는 취소·오류까지) 읽어 이벤트로 보낸다. 마지막에 `Finished`/`Failed` 를 정확히 한 번 보낸다.
/// 줄이 드물게 와도(다음 줄이 80ms 넘게 안 와도) 모인 것은 80ms 마다 비워 진행 표시가 멈추지 않는다.
pub fn pump(lines: &Receiver<io::Result<String>>, cancel: &AtomicBool, tx: &Sender<Event>, notify: &dyn Fn()) {
    let send = |e: Event| {
        let _ = tx.send(e);
        notify();
    };
    // 실패해도 그때까지 모인 문서는 먼저 보낸다(종료 코드 오류는 모든 줄 뒤에 온다).
    let fail = |batch: &mut Vec<Doc>, visited: usize, folder: &str, msg: String| {
        if !batch.is_empty() {
            send(Event::Batch { docs: std::mem::take(batch), visited, folder: folder.to_string() });
        }
        send(Event::Failed(msg));
    };
    let (mut batch, mut visited, mut folder) = (Vec::new(), 0usize, String::new());
    let mut last = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        match lines.recv_timeout(BATCH_TIME) {
            Ok(Ok(line)) => {
                if line.trim().is_empty() {
                    continue;
                }
                match parse_line(&line) {
                    Ok(doc) => {
                        visited += 1;
                        folder = parent_folder(&doc.path);
                        batch.push(doc);
                    }
                    Err(e) => return fail(&mut batch, visited, &folder, format!("잘못된 스캔 줄: {e}")),
                }
            }
            Ok(Err(e)) => return fail(&mut batch, visited, &folder, format!("스캔 출력을 읽지 못했습니다: {e}")),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if !batch.is_empty() && (batch.len() >= BATCH_ROWS || last.elapsed() >= BATCH_TIME) {
            send(Event::Batch { docs: std::mem::take(&mut batch), visited, folder: folder.clone() });
            last = Instant::now();
        }
    }
    if !batch.is_empty() {
        send(Event::Batch { docs: batch, visited, folder });
    }
    send(Event::Finished { cancelled: cancel.load(Ordering::Relaxed) });
}

/// 비동기 스캔 한 번. `cancel()` 은 즉시 돌아오고, 끝은 `rx` 의 `Finished{cancelled:true}` 로 온다.
pub struct ScanJob {
    pub rx: Receiver<Event>,
    pub started: Instant,
    cancel: Arc<AtomicBool>,
    killer: Option<Box<dyn Fn() + Send + Sync>>,
    join: Option<JoinHandle<()>>,
}

impl ScanJob {
    /// 임의 줄 소스(시험·합성 모델)로 시작한다. `killer` 는 취소 때 소스를 깨우는 동작(자식 프로세스 종료 등).
    pub fn from_lines<I>(lines: I, killer: Option<Box<dyn Fn() + Send + Sync>>, notify: Arc<dyn Fn() + Send + Sync>) -> ScanJob
    where
        I: Iterator<Item = io::Result<String>> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        // 읽기 스레드: 블로킹 읽기를 맡고, 취소되면 더 보내지 않고 끝난다(자식 프로세스는 killer 가 죽여 EOF 로 깨운다).
        let (line_tx, line_rx) = mpsc::sync_channel::<io::Result<String>>(1024);
        let rc = cancel.clone();
        std::thread::spawn(move || {
            for line in lines {
                if rc.load(Ordering::Relaxed) || line_tx.send(line).is_err() {
                    break;
                }
            }
        });
        let c = cancel.clone();
        let join = std::thread::spawn(move || pump(&line_rx, &c, &tx, &*notify));
        ScanJob { rx, started: Instant::now(), cancel, killer, join: Some(join) }
    }

    /// `klib scan` 을 자식 프로세스로 실행한다.
    pub fn start_klib(exe: &Path, roots: &[PathBuf], excludes: &[String], notify: Arc<dyn Fn() + Send + Sync>) -> Result<ScanJob, String> {
        let mut cmd = std::process::Command::new(exe);
        cmd.args(klib_args(roots, excludes));
        cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).stdin(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = cmd.spawn().map_err(|e| format!("klib 를 실행하지 못했습니다({}): {e}", exe.display()))?;
        let stdout = child.stdout.take().ok_or("klib 출력 파이프가 없습니다")?;
        let child = Arc::new(Mutex::new(child));
        let kill = child.clone();
        let reap = child.clone();
        let lines = BufReader::new(stdout).lines();
        // 줄 소스가 끝나면(정상 종료·취소로 죽임) 종료 코드를 확인해 이상이면 오류 줄로 알린다.
        let source = lines.chain(std::iter::from_fn(move || {
            let status = reap.lock().ok()?.wait().ok()?;
            if status.success() { None } else { Some(Err(io::Error::other(format!("klib 종료 코드 {:?}", status.code())))) }
        }).take(1));
        Ok(ScanJob::from_lines(
            source,
            Some(Box::new(move || {
                if let Ok(mut c) = kill.lock() {
                    let _ = c.kill();
                }
            })),
            notify,
        ))
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(k) = &self.killer {
            k();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// 쌓인 이벤트를 모두 꺼낸다.
    pub fn drain(&self) -> Vec<Event> {
        self.rx.try_iter().collect()
    }

    /// 스레드가 끝날 때까지 기다린다(시험·종료용).
    pub fn join(&mut self) {
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// `klib scan` 인자(`rust_scan.scan_default_roots` 와 같은 형식).
pub fn klib_args(roots: &[PathBuf], excludes: &[String]) -> Vec<String> {
    let mut a = vec!["scan".to_string(), "--roots".to_string()];
    a.extend(roots.iter().map(|r| r.display().to_string()));
    a.push("--ext".into());
    a.push(DOC_EXTENSIONS.join(","));
    a.push("--json-lines".into());
    if !excludes.is_empty() {
        a.push("--exclude".into());
        a.extend(excludes.iter().cloned());
    }
    a
}

/// klib 실행 파일을 찾는다: `DOCFIT_KLIB` → 이 실행 파일 옆·상위 폴더의 `klib.exe`·`resources`·`klib/target`.
pub fn find_klib() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("DOCFIT_KLIB").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
    }
    let exe = std::env::current_exe().ok()?;
    let name = if cfg!(windows) { "klib.exe" } else { "klib" };
    for dir in exe.parent()?.ancestors().take(6) {
        for rel in ["", "resources", "klib/target/release", "klib/target/debug"] {
            let c = dir.join(rel).join(name);
            if c.is_file() {
                return Some(c);
            }
        }
    }
    None
}
