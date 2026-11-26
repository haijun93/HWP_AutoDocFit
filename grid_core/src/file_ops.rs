//! FILEOPS1 규칙을 따르는 공용 실행기. OS 경계는 Backend로 분리하여 휴지통·클립보드를 가짜로 시험한다.
use crate::items::Doc;
use std::{
    io,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Clipboard,
    Copy,
    Move,
    Rename,
    Delete,
}

#[derive(Clone, Debug)]
pub struct Request {
    pub op: Operation,
    pub docs: Vec<Doc>,
    pub extensions: Vec<String>,
    pub destination: Option<PathBuf>,
    /// 확장자를 제외한 이름. 입력 끝에 같은 확장자가 있으면 한 번 제거한다.
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct FileResult {
    pub source: Doc,
    pub target: Option<Doc>,
    pub error: Option<String>,
}

pub trait Backend: Send + 'static {
    fn recycle(&self, path: &Path) -> Result<(), String>;
    fn clipboard(&self, paths: &[PathBuf]) -> Result<(), String>;
    fn copy(&self, from: &Path, to: &Path) -> io::Result<()> {
        copy_exclusive(from, to)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        rename_exclusive(from, to)
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }
    fn rollback(&self, path: &Path) -> io::Result<()> {
        #[cfg(windows)]
        {
            let mut permissions = std::fs::metadata(path)?.permissions();
            if permissions.readonly() {
                permissions.set_readonly(false);
                std::fs::set_permissions(path, permissions)?;
            }
        }
        self.remove(path)
    }
}

pub fn check_source(doc: &Doc, extensions: &[String]) -> Result<(), String> {
    crate::open::check_openable(
        &doc.path,
        &extensions.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    let metadata = std::fs::symlink_metadata(&doc.path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("링크·폴더는 처리하지 않습니다".into());
    }
    Ok(())
}

pub fn cloud_only(doc: &Doc) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let attributes = std::fs::metadata(&doc.path)
            .map(|m| m.file_attributes())
            .unwrap_or(0);
        if attributes & (0x1000 | 0x40000 | 0x400000) != 0 {
            return true;
        }
    }
    doc.cloud
}

pub fn renamed_name(source: &Path, input: &str) -> Result<String, String> {
    let ext = source
        .extension()
        .and_then(|s| s.to_str())
        .ok_or("확장자가 없습니다")?;
    let suffix = format!(".{ext}");
    let mut stem = input.trim_start();
    if stem.to_lowercase().ends_with(&suffix.to_lowercase()) {
        stem = &stem[..stem.len() - suffix.len()];
    }
    let base = stem
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "CLOCK$"]
        .contains(&base.as_str())
        || ["COM", "LPT"].iter().any(|p| {
            base.strip_prefix(p)
                .is_some_and(|n| n.chars().count() == 1 && "0123456789¹²³".contains(n))
        });
    let name = format!("{stem}{suffix}");
    if stem.trim_matches([' ', '.']).is_empty()
        || stem.ends_with([' ', '.'])
        || reserved
        || stem.chars().any(|c| c < ' ' || "<>:\"/\\|?*".contains(c))
        || name.encode_utf16().count() > 255
    {
        return Err(
            "이름이 올바르지 않습니다(예약 이름·금지 문자·끝 공백/마침표·길이를 확인하세요)".into(),
        );
    }
    Ok(name)
}

#[cfg(windows)]
fn copy_exclusive(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let w = |p: &Path| {
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    if unsafe {
        windows_sys::Win32::Storage::FileSystem::CopyFileW(w(from).as_ptr(), w(to).as_ptr(), 1)
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn copy_exclusive(from: &Path, to: &Path) -> io::Result<()> {
    let mut reader = std::fs::File::open(from)?;
    let mut writer = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)?;
    if let Err(e) = io::copy(&mut reader, &mut writer) {
        drop(writer);
        let _ = std::fs::remove_file(to);
        return Err(e);
    }
    Ok(())
}

#[cfg(windows)]
fn rename_exclusive(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let w = |p: &Path| {
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    // std::fs::rename는 Windows에서도 덮어쓴다. 대체·다른 볼륨 복사 플래그를 주지 않는다.
    if unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileExW(w(from).as_ptr(), w(to).as_ptr(), 0)
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn rename_exclusive(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::hard_link(from, to)?;
    if let Err(e) = std::fs::remove_file(from) {
        let _ = std::fs::remove_file(to);
        return Err(e);
    }
    Ok(())
}

fn collision(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::AlreadyExists || matches!(e.raw_os_error(), Some(80 | 183))
}
fn cross_volume(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::CrossesDevices || e.raw_os_error() == Some(17)
}

fn into_folder(
    doc: &Doc,
    folder: &Path,
    moving: bool,
    backend: &impl Backend,
) -> Result<PathBuf, String> {
    if !folder.is_absolute() || !folder.is_dir() {
        return Err("대상 폴더가 올바르지 않습니다".into());
    }
    let source = Path::new(&doc.path);
    if moving && source.parent().and_then(|p| p.canonicalize().ok()) == folder.canonicalize().ok() {
        return Err("이미 대상 폴더에 있습니다".into());
    }
    if !moving && cloud_only(doc) {
        return Err("클라우드 전용 파일은 내려받은 뒤 복사해 주세요".into());
    }
    let stem = source.file_stem().unwrap().to_string_lossy();
    let ext = source.extension().unwrap().to_string_lossy();
    for n in 1..10000 {
        let name = if n == 1 {
            source.file_name().unwrap().to_os_string()
        } else {
            format!("{stem} ({n}).{ext}").into()
        };
        let target = folder.join(name);
        // 같은 이름의 폴더·링크도 충돌이다. 이후 OS 호출 역시 배타적으로 하여 경합 때 덮어쓰지 않는다.
        if std::fs::symlink_metadata(&target).is_ok() {
            continue;
        }
        let result = if moving {
            backend.rename(source, &target)
        } else {
            backend.copy(source, &target)
        };
        match result {
            Ok(()) => return Ok(target),
            Err(e) if collision(&e) => continue,
            Err(e) if moving && cross_volume(&e) => {
                if cloud_only(doc) {
                    return Err("클라우드 전용 파일의 다른 드라이브 이동은 거절합니다".into());
                }
                match backend.copy(source, &target) {
                    Err(e) if collision(&e) => continue,
                    Err(e) => return Err(e.to_string()),
                    Ok(()) => {
                        if let Err(e) = backend.remove(source) {
                            // 우리가 배타 생성한 사본만 되돌린다. 실패도 숨기지 않는다.
                            let rollback = backend.rollback(&target);
                            return Err(format!(
                                "원본 제거 실패: {e}; 사본 되돌리기: {}",
                                rollback
                                    .map(|_| "완료".to_string())
                                    .unwrap_or_else(|e| e.to_string())
                            ));
                        }
                        return Ok(target);
                    }
                }
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("같은 이름의 파일이 너무 많습니다".into())
}

fn one(request: &Request, doc: &Doc, backend: &impl Backend) -> Result<Option<Doc>, String> {
    check_source(doc, &request.extensions)?;
    let source = Path::new(&doc.path);
    let target = match request.op {
        Operation::Copy | Operation::Move => Some(into_folder(
            doc,
            request
                .destination
                .as_deref()
                .ok_or("대상 폴더가 없습니다")?,
            request.op == Operation::Move,
            backend,
        )?),
        Operation::Rename => {
            let name = renamed_name(source, &request.name)?;
            let target = source.with_file_name(name);
            if target == source {
                return Err("이름이 바뀌지 않았습니다".into());
            }
            backend.rename(source, &target).map_err(|e| e.to_string())?;
            Some(target)
        }
        Operation::Delete => {
            backend.recycle(source)?;
            None
        }
        Operation::Clipboard => unreachable!(),
    };
    Ok(target.map(|p| {
        let mut d = doc.clone();
        d.name = p.file_name().unwrap().to_string_lossy().into_owned();
        d.path = p.to_string_lossy().into_owned();
        d
    }))
}

/// 파일별 결과를 즉시 전달한다. 한 파일의 오류는 나머지 작업을 막지 않는다.
pub fn execute(request: &Request, backend: &impl Backend, mut completed: impl FnMut(FileResult)) {
    if request.op == Operation::Clipboard {
        let mut results: Vec<_> = request
            .docs
            .iter()
            .map(|doc| FileResult {
                source: doc.clone(),
                target: None,
                error: check_source(doc, &request.extensions)
                    .and_then(|_| {
                        if cloud_only(doc) {
                            Err("클라우드 전용 파일은 내려받은 뒤 복사해 주세요".into())
                        } else {
                            Ok(())
                        }
                    })
                    .err(),
            })
            .collect();
        let paths = results
            .iter()
            .filter(|r| r.error.is_none())
            .map(|r| PathBuf::from(&r.source.path))
            .collect::<Vec<_>>();
        if !paths.is_empty() {
            if let Err(e) = backend.clipboard(&paths) {
                for r in &mut results {
                    if r.error.is_none() {
                        r.error = Some(e.clone());
                    }
                }
            }
        }
        for r in results {
            completed(r);
        }
        return;
    }
    for doc in &request.docs {
        let result = if request.op == Operation::Rename && request.docs.len() != 1 {
            Err("이름 변경은 하나만 선택해 주세요".into())
        } else {
            one(request, doc, backend)
        };
        let (target, error) = match result {
            Ok(target) => (target, None),
            Err(e) => (None, Some(e)),
        };
        completed(FileResult {
            source: doc.clone(),
            target,
            error,
        });
    }
}

/// 창은 수신함만 조회한다. 스레드를 기다리는 동안 UI를 막지 않는다.
pub struct Task {
    pub op: Operation,
    pub total: usize,
    pub results: Vec<FileResult>,
    sources: Vec<Doc>,
    receiver: mpsc::Receiver<FileResult>,
    worker: thread::JoinHandle<()>,
}

impl Task {
    pub fn start(request: Request, backend: impl Backend) -> io::Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let (op, total) = (request.op, request.docs.len());
        let sources = request.docs.clone();
        let worker = thread::Builder::new()
            .name("native-file-ops".into())
            .spawn(move || {
                execute(&request, &backend, |r| {
                    let _ = sender.send(r);
                })
            })?;
        Ok(Self {
            op,
            total,
            results: Vec::new(),
            sources,
            receiver,
            worker,
        })
    }
    pub fn poll(&mut self) -> bool {
        self.results.extend(self.receiver.try_iter());
        if !self.worker.is_finished() {
            return false;
        }
        self.results.extend(self.receiver.try_iter());
        if self.results.len() < self.total {
            let completed: std::collections::HashSet<_> =
                self.results.iter().map(|r| r.source.path.clone()).collect();
            for source in &self.sources {
                if !completed.contains(&source.path) {
                    self.results.push(FileResult {
                        source: source.clone(),
                        target: None,
                        error: Some("작업 스레드가 결과를 보내지 못했습니다".into()),
                    });
                }
            }
        }
        true
    }
}
