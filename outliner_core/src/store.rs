use crate::{parse, serialize, Document, OutlinerError, MAX_DOCUMENT_BYTES};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Reads a UTF-8 Markdown file and parses its contents.
pub fn load(path: &Path) -> Result<Document, OutlinerError> {
    let content = fs::read_to_string(path).map_err(|error| OutlinerError::Io(error.to_string()))?;
    parse(&content)
}

/// Writes through a same-directory temporary file and replaces the destination on success.
pub fn save_atomic(path: &Path, content: &str) -> Result<(), OutlinerError> {
    if content.len() > MAX_DOCUMENT_BYTES {
        return Err(OutlinerError::DocumentTooLarge);
    }
    if path.is_dir() {
        return Err(OutlinerError::Io("저장 경로가 디렉터리입니다".into()));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("outliner.md");
    let mut temp = PathBuf::new();
    let mut file = None;
    for _ in 0..32 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        temp = parent.join(format!(
            ".{name}.outliner-{}-{sequence}.tmp",
            std::process::id()
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
        {
            Ok(opened) => {
                file = Some(opened);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(OutlinerError::Io(error.to_string())),
        }
    }
    let mut file = file.ok_or_else(|| OutlinerError::Io("임시 파일을 만들 수 없습니다".into()))?;
    let write_result = file
        .write_all(content.as_bytes())
        .and_then(|_| file.sync_all());
    drop(file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temp);
        return Err(OutlinerError::Io(error.to_string()));
    }
    #[cfg(windows)]
    if path.exists() {
        let backup = temp.with_extension("backup");
        if let Err(error) = fs::rename(path, &backup) {
            let _ = fs::remove_file(&temp);
            return Err(OutlinerError::Io(error.to_string()));
        }
        if let Err(error) = fs::rename(&temp, path) {
            let restore = fs::rename(&backup, path);
            let _ = fs::remove_file(&temp);
            if let Err(restore_error) = restore {
                return Err(OutlinerError::Io(format!(
                    "{error}; 원본 복구 실패: {restore_error}"
                )));
            }
            return Err(OutlinerError::Io(error.to_string()));
        }
        let _ = fs::remove_file(backup);
        return Ok(());
    }
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(OutlinerError::Io(error.to_string()));
    }
    Ok(())
}

/// Serializes and atomically saves a parsed document.
pub fn save_document(path: &Path, document: &Document) -> Result<(), OutlinerError> {
    save_atomic(path, &serialize(document))
}
