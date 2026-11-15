//! HWP 파일 고속 병렬 탐색 및 분류 모듈 (`scan-hwp`).
//!
//! 시스템 폴더, 정션/심볼릭링크, 네트워크 드라이브, 클라우드 전용 파일을 제외하고
//! 디렉터리를 병렬로 탐색하여 각 `.hwp` 파일을 머리 정보 기반으로 분류합니다:
//! - 정상 HWP5 (normal)
//! - 암호 문서 (password)
//! - 배포용 문서 (distributed)
//! - DRM 문서 (drm)
//! - 손상된 문서 (corrupt)
//! - 이미 같은 이름의 .hwpx 있음 (existing)
//! - 구형 HWP 3.0 문서 (hwp3)
//!
//! 라이선스: MIT

use std::collections::VecDeque;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::sync::{mpsc, Arc, Condvar, Mutex};

use crate::hwp5::{HwpFileHeader, HWP5_SIGNATURE};
use crate::json::{self, Value};

/// 시스템 및 불필요 디렉터리 제외 목록 (소문자 기준 대조)
pub const SKIP_DIRS: &[&str] = &[
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "$recycle.bin",
    "$windows.~bt",
    "$windows.~ws",
    "$winreagent",
    "system volume information",
    "recovery",
    "appdata",
    "msocache",
    "perflogs",
    "node_modules",
    ".git",
    ".svn",
    "__pycache__",
    "onedrivetemp",
    "config.msi",
    "hnc",
    "hncutils",
    "hancom",
    "hancom office",
    "hancomoffice",
    "서식",
    "서식예시",
    "template",
    "templates",
];

/// 기본 건너뛸 파일명 (소문자)
pub const SKIP_FILES: &[&str] = &[
    "hanwp.hwp",
];

/// OLE 복합 문서 시그니처 (D0 CF 11 E0 A1 B1 1A E1)
pub const OLE_SIGNATURE: &[u8] = &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// HWP 3.0 시그니처
pub const HWP3_SIGNATURE: &[u8] = b"HWP Document File";

/// HWP 분류 종류
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwpKind {
    Normal,
    Password,
    Distributed,
    Drm,
    Corrupt,
    Existing,
    Hwp3,
}

impl HwpKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            HwpKind::Normal => "normal",
            HwpKind::Password => "password",
            HwpKind::Distributed => "distributed",
            HwpKind::Drm => "drm",
            HwpKind::Corrupt => "corrupt",
            HwpKind::Existing => "existing",
            HwpKind::Hwp3 => "hwp3",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            HwpKind::Normal => "정상 HWP5",
            HwpKind::Password => "암호",
            HwpKind::Distributed => "배포용",
            HwpKind::Drm => "DRM",
            HwpKind::Corrupt => "손상",
            HwpKind::Existing => "이미 같은 이름 .hwpx 있음",
            HwpKind::Hwp3 => "HWP3",
        }
    }
}

/// 단일 HWP 파일의 분류 결과
#[derive(Debug, Clone, PartialEq)]
pub struct ScanItem {
    pub path: PathBuf,
    pub kind: HwpKind,
    pub label: &'static str,
    pub size: u64,
    pub hwpx_exists: bool,
    pub file_kind: HwpKind,
    pub version: Option<String>,
    pub flags: Option<u32>,
    pub message: Option<String>,
}

impl ScanItem {
    pub fn to_json(&self) -> Value {
        let mut fields = vec![
            ("path", Value::Str(self.path.to_string_lossy().into_owned())),
            ("kind", Value::Str(self.kind.as_str().to_string())),
            ("label", Value::Str(self.label.to_string())),
            ("size", Value::Num(self.size as f64)),
            ("hwpx_exists", Value::Bool(self.hwpx_exists)),
        ];

        if self.kind == HwpKind::Existing {
            fields.push(("file_kind", Value::Str(self.file_kind.as_str().to_string())));
        }
        if let Some(ref ver) = self.version {
            fields.push(("version", Value::Str(ver.clone())));
        }
        if let Some(flags) = self.flags {
            fields.push(("flags", Value::Num(flags as f64)));
        }
        if let Some(ref msg) = self.message {
            fields.push(("message", Value::Str(msg.clone())));
        }

        Value::obj(fields)
    }
}

/// 심볼릭 링크 또는 Windows 정션(Reparse Point) 여부 판정
pub fn is_symlink_or_reparse(path: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x00000400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

/// 네트워크 드라이브 또는 UNC 경로 여부 판정
pub fn is_network_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    if s.starts_with(r"\\") || s.starts_with("//") {
        return true;
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, DRIVE_REMOTE};
        let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
        let drive_type = unsafe { GetDriveTypeW(wide.as_ptr()) };
        if drive_type == DRIVE_REMOTE {
            return true;
        }
    }
    false
}

/// 디렉터리 건너뛰기 여부 판정
pub fn should_skip_dir(path: &Path, file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    if SKIP_DIRS.contains(&lower.as_str()) || lower.starts_with('$') {
        return true;
    }
    if is_symlink_or_reparse(path) {
        return true;
    }
    #[cfg(windows)]
    {
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            use std::os::windows::fs::MetadataExt;
            let attrs = meta.file_attributes();
            // 시스템 + 숨김 폴더
            if (attrs & 0x04 != 0) && (attrs & 0x02 != 0) {
                return true;
            }
            // 클라우드 전용(온라인 전용)
            if attrs & (0x00400000 | 0x00040000 | 0x00001000) != 0 {
                return true;
            }
        }
    }
    false
}

/// 파일 건너뛰기 여부 판정
pub fn should_skip_file(path: &Path, file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    if SKIP_FILES.contains(&lower.as_str()) {
        return true;
    }
    if is_symlink_or_reparse(path) {
        return true;
    }
    #[cfg(windows)]
    {
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            use std::os::windows::fs::MetadataExt;
            let attrs = meta.file_attributes();
            // 클라우드 전용 파일
            if attrs & (0x00400000 | 0x00040000 | 0x00001000) != 0 {
                return true;
            }
        }
    }
    false
}

/// HWP 파일 머리를 분석하여 분류 정보를 생성한다. 원본 파일은 읽기 전용으로만 접근한다.
pub fn classify_hwp_file(path: &Path) -> ScanItem {
    let mut item = ScanItem {
        path: path.to_path_buf(),
        kind: HwpKind::Normal,
        label: HwpKind::Normal.label(),
        size: 0,
        hwpx_exists: false,
        file_kind: HwpKind::Normal,
        version: None,
        flags: None,
        message: None,
    };

    // 1. 파일 메타데이터 확인
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) => {
            item.kind = HwpKind::Corrupt;
            item.file_kind = HwpKind::Corrupt;
            item.label = HwpKind::Corrupt.label();
            item.message = Some(format!("메타데이터 읽기 실패: {e}"));
            return item;
        }
    };
    item.size = meta.len();

    // 2. 같은 위치에 동일 파일명의 .hwpx 존재 여부 확인
    let hwpx_path = path.with_extension("hwpx");
    let hwpx_upper = path.with_extension("HWPX");
    let hwpx_exists = hwpx_path.is_file() || hwpx_upper.is_file();
    item.hwpx_exists = hwpx_exists;

    // 0바이트 빈 파일 처리
    if item.size == 0 {
        item.file_kind = HwpKind::Corrupt;
        item.message = Some("0바이트 파일".into());
        if hwpx_exists {
            item.kind = HwpKind::Existing;
            item.label = HwpKind::Existing.label();
        } else {
            item.kind = HwpKind::Corrupt;
            item.label = HwpKind::Corrupt.label();
        }
        return item;
    }

    // 3. 파일 머리 읽기 (최대 64KB)
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            item.file_kind = HwpKind::Corrupt;
            item.message = Some(format!("파일 열기 실패: {e}"));
            if hwpx_exists {
                item.kind = HwpKind::Existing;
                item.label = HwpKind::Existing.label();
            } else {
                item.kind = HwpKind::Corrupt;
                item.label = HwpKind::Corrupt.label();
            }
            return item;
        }
    };

    let read_len = (item.size as usize).min(65536);
    let mut buf = vec![0u8; read_len];
    if let Err(e) = file.read_exact(&mut buf) {
        item.file_kind = HwpKind::Corrupt;
        item.message = Some(format!("파일 읽기 실패: {e}"));
        if hwpx_exists {
            item.kind = HwpKind::Existing;
            item.label = HwpKind::Existing.label();
        } else {
            item.kind = HwpKind::Corrupt;
            item.label = HwpKind::Corrupt.label();
        }
        return item;
    }

    if buf.len() < 8 {
        item.file_kind = HwpKind::Corrupt;
        item.message = Some("파일 크기 부족 (8바이트 미만)".into());
        if hwpx_exists {
            item.kind = HwpKind::Existing;
            item.label = HwpKind::Existing.label();
        } else {
            item.kind = HwpKind::Corrupt;
            item.label = HwpKind::Corrupt.label();
        }
        return item;
    }

    // 4. 시그니처 판별
    if buf.starts_with(OLE_SIGNATURE) {
        // OLE 복합 문서
        let header_opt = fast_scan_hwp5_header(&buf).or_else(|| parse_cfb_header(path));
        match header_opt {
            Some(header) => {
                let v = header.version;
                let major = (v >> 24) & 0xFF;
                let minor = (v >> 16) & 0xFF;
                let build = (v >> 8) & 0xFF;
                let rev = v & 0xFF;
                item.version = Some(format!("{major}.{minor}.{build}.{rev}"));
                item.flags = Some(header.flags);

                if header.is_encrypted {
                    item.file_kind = HwpKind::Password;
                } else if header.is_distributed {
                    item.file_kind = HwpKind::Distributed;
                } else if header.is_drm {
                    item.file_kind = HwpKind::Drm;
                } else {
                    item.file_kind = HwpKind::Normal;
                }
            }
            None => {
                item.file_kind = HwpKind::Corrupt;
                item.message = Some("FileHeader 해석 실패 또는 손상된 OLE".into());
            }
        }
    } else if buf.starts_with(HWP3_SIGNATURE) {
        item.file_kind = HwpKind::Hwp3;
    } else {
        // OLE도 아니고 HWP3도 아님: 기관 DRM 문서 (Fasoo, SoftCamp 등)
        item.file_kind = HwpKind::Drm;
        item.message = Some("DRM 암호화 또는 비표준 포맷".into());
    }

    if hwpx_exists {
        item.kind = HwpKind::Existing;
        item.label = HwpKind::Existing.label();
    } else {
        item.kind = item.file_kind;
        item.label = item.file_kind.label();
    }

    item
}

/// 64KB 버퍼 내에서 64/512바이트 섹터 경계에 있는 FileHeader를 고속 탐색
fn fast_scan_hwp5_header(buf: &[u8]) -> Option<HwpFileHeader> {
    let target = HWP5_SIGNATURE;
    let mut pos = 0;
    while pos + 256 <= buf.len() {
        if let Some(rel) = buf[pos..].windows(target.len()).position(|w| w == target) {
            let idx = pos + rel;
            if idx % 64 == 0 && idx + 256 <= buf.len() {
                if let Ok(hdr) = HwpFileHeader::parse(&buf[idx..idx + 256]) {
                    let major = (hdr.version >> 24) & 0xFF;
                    if major >= 5 {
                        return Some(hdr);
                    }
                }
            }
            pos = idx + target.len();
        } else {
            break;
        }
    }
    None
}

/// CFB 라이브러리를 통한 FileHeader 정밀 파싱
fn parse_cfb_header(path: &Path) -> Option<HwpFileHeader> {
    let file = File::open(path).ok()?;
    let mut comp = cfb::CompoundFile::open(file).ok()?;
    let mut stream = if comp.is_stream("/FileHeader") {
        comp.open_stream("/FileHeader").ok()?
    } else if comp.is_stream("FileHeader") {
        comp.open_stream("FileHeader").ok()?
    } else {
        return None;
    };
    let mut bytes = [0u8; 256];
    stream.read_exact(&mut bytes).ok()?;
    HwpFileHeader::parse(&bytes).ok()
}

/// 루트 목록을 순회하여 모든 HWP 파일을 병렬로 탐색·분류하고 콜백을 호출합니다.
pub fn scan_roots_parallel<F>(roots: &[PathBuf], on_item: F)
where
    F: Fn(ScanItem) + Send + Sync + 'static,
{
    let on_item = Arc::new(on_item);
    let queue = Arc::new(Mutex::new(VecDeque::new()));
    let active_workers = Arc::new(AtomicUsize::new(0));
    let cond = Arc::new(Condvar::new());
    let active_mutex = Arc::new(Mutex::new(()));

    // 루트 분류 및 큐 채우기
    for root in roots {
        if is_network_path(root) {
            eprintln!("warning: 네트워크 드라이브는 기본 제외됩니다: {}", root.display());
            continue;
        }
        if root.is_file() {
            let name = root.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.to_lowercase().ends_with(".hwp") && !should_skip_file(root, name) {
                let item = classify_hwp_file(root);
                on_item(item);
            }
        } else if root.is_dir() {
            let name = root.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !should_skip_dir(root, name) {
                queue.lock().unwrap().push_back(root.clone());
            }
        }
    }

    if queue.lock().unwrap().is_empty() {
        return;
    }

    let num_workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(16)
        .max(2);

    let mut handles = Vec::with_capacity(num_workers);

    for _ in 0..num_workers {
        let queue = Arc::clone(&queue);
        let active_workers = Arc::clone(&active_workers);
        let cond = Arc::clone(&cond);
        let active_mutex = Arc::clone(&active_mutex);
        let on_item = Arc::clone(&on_item);

        let handle = std::thread::spawn(move || loop {
            let dir = {
                let mut q = queue.lock().unwrap();
                if let Some(d) = q.pop_front() {
                    active_workers.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Some(d)
                } else {
                    None
                }
            };

            let dir = match dir {
                Some(d) => d,
                None => {
                    let _guard = active_mutex.lock().unwrap();
                    if active_workers.load(std::sync::atomic::Ordering::SeqCst) == 0
                        && queue.lock().unwrap().is_empty()
                    {
                        cond.notify_all();
                        break;
                    }
                    let _ = cond.wait_timeout(_guard, std::time::Duration::from_millis(50));
                    if active_workers.load(std::sync::atomic::Ordering::SeqCst) == 0
                        && queue.lock().unwrap().is_empty()
                    {
                        cond.notify_all();
                        break;
                    }
                    continue;
                }
            };

            // 디렉터리 순회
            if let Ok(rd) = std::fs::read_dir(&dir) {
                let mut subdirs = Vec::new();
                let mut files = Vec::new();

                for entry in rd.flatten() {
                    let path = entry.path();
                    let file_name = entry.file_name();
                    let name_str = file_name.to_string_lossy();

                    let Ok(meta) = entry.metadata() else {
                        continue;
                    };
                    let ft = meta.file_type();

                    if ft.is_dir() {
                        if !should_skip_dir(&path, &name_str) {
                            subdirs.push(path);
                        }
                    } else if ft.is_file() {
                        if name_str.to_lowercase().ends_with(".hwp")
                            && !should_skip_file(&path, &name_str)
                        {
                            files.push(path);
                        }
                    }
                }

                // 발견된 파일들 분류
                for file_path in files {
                    let item = classify_hwp_file(&file_path);
                    on_item(item);
                }

                // 새 하위 디렉터리 큐에 추가
                if !subdirs.is_empty() {
                    let mut q = queue.lock().unwrap();
                    for sd in subdirs {
                        q.push_back(sd);
                    }
                    cond.notify_all();
                }
            }

            active_workers.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            cond.notify_all();
        });

        handles.push(handle);
    }

    for h in handles {
        let _ = h.join();
    }
}

/// 루트 경로들을 탐색하여 모든 ScanItem을 벡터로 수집합니다 (테스트 및 라이브러리용).
pub fn scan_roots(roots: &[PathBuf]) -> Vec<ScanItem> {
    let (tx, rx) = mpsc::channel::<ScanItem>();
    scan_roots_parallel(roots, move |item| {
        let _ = tx.send(item);
    });
    rx.into_iter().collect()
}

/// CLI `scan-hwp` 실행 진입점: 각 HWP 파일의 분류 결과를 JSONL로 표준 출력에 인쇄합니다.
pub fn run(roots: &[PathBuf]) {
    use std::io::Write;
    let (tx, rx) = mpsc::channel::<ScanItem>();
    let print_handle = std::thread::spawn(move || {
        let stdout = std::io::stdout();
        let mut writer = std::io::BufWriter::new(stdout.lock());
        for item in rx {
            let json_val = item.to_json();
            let json_str = json::to_string(&json_val);
            let _ = writeln!(writer, "{json_str}");
        }
        let _ = writer.flush();
    });

    scan_roots_parallel(roots, move |item| {
        let _ = tx.send(item);
    });

    let _ = print_handle.join();
}
