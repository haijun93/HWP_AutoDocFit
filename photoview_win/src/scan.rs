//! 사진 스캔: `klib scan --ext <사진 확장자> --json-lines` 줄 파서(문서와 같은 줄 형식)와 사진뷰용 비동기 스캔 작업.
//! 줄 형식: `klib/src/scan_cli.rs::emit` — `{"path","name","ext","size","mtime","mtime_ns","cloud"}`. 읽기·취소 기계는 `grid_core::scan` 공용.
use crate::model::Doc;
use grid_core::scan::ScanSpec;
use std::io;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use grid_core::scan::{find_klib, parent_folder};

/// 사진뷰가 훑는 확장자(`photo_album_service.PHOTO_EXTENSIONS` 와 같다).
pub const PHOTO_EXTENSIONS: [&str; 8] = ["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff", "heic"];

/// 한 줄 → 사진 항목.
pub fn parse_line(line: &str) -> Result<Doc, String> {
    grid_core::items::parse_scan_line(line)
}

const SPEC: ScanSpec<Doc> = ScanSpec { parse: parse_line, folder: |d| parent_folder(&d.path), extensions: &PHOTO_EXTENSIONS };

pub type Event = grid_core::scan::Event<Doc>;

/// 사진 스캔 작업(공용 `ScanJob<Doc>` 에 사진 규칙을 고정).
pub struct ScanJob(grid_core::scan::ScanJob<Doc>);

impl Deref for ScanJob {
    type Target = grid_core::scan::ScanJob<Doc>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for ScanJob {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl ScanJob {
    pub fn from_lines<I>(lines: I, killer: Option<Box<dyn Fn() + Send + Sync>>, notify: Arc<dyn Fn() + Send + Sync>) -> ScanJob
    where
        I: Iterator<Item = io::Result<String>> + Send + 'static,
    {
        ScanJob(grid_core::scan::ScanJob::from_lines(SPEC, lines, killer, notify))
    }

    pub fn start_klib(exe: &Path, roots: &[PathBuf], excludes: &[String], notify: Arc<dyn Fn() + Send + Sync>) -> Result<ScanJob, String> {
        grid_core::scan::ScanJob::start_klib(SPEC, exe, roots, excludes, notify).map(ScanJob)
    }
}

/// `klib scan` 인자(`rust_scan.scan_default_roots` 와 같은 형식, 확장자만 사진).
pub fn klib_args(roots: &[PathBuf], excludes: &[String]) -> Vec<String> {
    grid_core::scan::klib_args(&PHOTO_EXTENSIONS, roots, excludes)
}

/// 동기 스캔 결과(시험·캡처 모드용): 모든 문서와 취소 여부.
#[derive(Debug)]
pub struct Scanned {
    pub docs: Vec<Doc>,
    pub cancelled: bool,
    /// 마지막 배치가 알린 훑은 수.
    pub visited: usize,
}

/// `klib scan` 을 실제로 돌려 끝날 때까지 기다린다. 오류(자식 종료 코드·잘못된 줄·시간 초과)는 `Err` 로 돌려준다.
/// `limit` 안에 끝나지 않으면 자식 프로세스를 죽이고 오류로 끝낸다.
pub fn scan_blocking(exe: &Path, roots: &[PathBuf], excludes: &[String], limit: std::time::Duration) -> Result<Scanned, String> {
    let mut job = ScanJob::start_klib(exe, roots, excludes, Arc::new(|| {}))?;
    let out = collect(&job, limit);
    if out.is_err() {
        job.cancel();
    }
    job.join();
    out
}

/// 이벤트를 끝까지(또는 `limit` 까지) 모은다.
pub fn collect(job: &ScanJob, limit: std::time::Duration) -> Result<Scanned, String> {
    use grid_core::scan::Event as E;
    let start = std::time::Instant::now();
    let mut out = Scanned { docs: Vec::new(), cancelled: false, visited: 0 };
    loop {
        for e in job.drain() {
            match e {
                E::Batch { docs, visited, .. } => {
                    out.docs.extend(docs);
                    out.visited = visited;
                }
                E::Finished { cancelled } => {
                    out.cancelled = cancelled;
                    return Ok(out);
                }
                E::Failed(msg) => return Err(msg),
            }
        }
        if start.elapsed() > limit {
            return Err(format!("스캔이 {}초 안에 끝나지 않았습니다.", limit.as_secs()));
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
