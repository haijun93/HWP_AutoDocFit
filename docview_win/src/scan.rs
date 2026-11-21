//! `klib scan --json-lines` 문서 줄 파서와 문서뷰용 비동기 스캔 작업. 줄 읽기·취소 기계는 `grid_core::scan` 공용.
//! JSON 줄 형식: `klib/src/scan_cli.rs::emit` — `{"path","name","ext","size","mtime","mtime_ns","cloud"}`(Python `rust_scan.scan` 과 같은 필수 키).
use crate::model::Doc;
use grid_core::scan::ScanSpec;
use std::io;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use grid_core::scan::{find_klib, parent_folder};

/// 문서뷰가 훑는 확장자(Python `DOC_EXTENSIONS` 와 같다).
pub const DOC_EXTENSIONS: [&str; 7] = ["hwp", "hwpx", "xls", "xlsx", "ppt", "pptx", "pdf"];

/// 한 줄 → 문서(공용 `grid_core::items::parse_scan_line`).
pub fn parse_line(line: &str) -> Result<Doc, String> {
    grid_core::items::parse_scan_line(line)
}

const SPEC: ScanSpec<Doc> = ScanSpec { parse: parse_line, folder: |d| parent_folder(&d.path), extensions: &DOC_EXTENSIONS };

pub type Event = grid_core::scan::Event<Doc>;

/// 문서 스캔 작업(공용 `ScanJob<Doc>` 를 감싸 문서 규칙을 고정한다).
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

/// `klib scan` 인자(`rust_scan.scan_default_roots` 와 같은 형식).
pub fn klib_args(roots: &[PathBuf], excludes: &[String]) -> Vec<String> {
    grid_core::scan::klib_args(&DOC_EXTENSIONS, roots, excludes)
}
