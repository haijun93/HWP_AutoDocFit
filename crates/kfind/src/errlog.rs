//! 서비스 오류 파일 로그(용량 상한·1세대 회전). 서비스에서는 stderr 가 사라지므로 데이터 폴더에 남긴다.
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
pub const LOG_NAME: &str = "kfind-service.log";
pub const MAX_BYTES: u64 = 256 * 1024;
/// 색인 폴더(`<데이터>/index`)의 부모 아래 `logs`를, 부모가 없으면 색인 폴더 아래 `logs`를 쓴다.
pub fn log_dir_for(index_root: &Path) -> PathBuf {
    index_root.parent().unwrap_or(index_root).join("logs")
}
/// 한 줄을 덧붙인다. 파일이 `max_bytes` 이상이면 `<이름>.1`로 바꾸고(기존 .1 은 덮어씀) 새로 시작한다.
pub fn append(dir: &Path, line: &str, max_bytes: u64) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let path = dir.join(LOG_NAME);
    if fs::metadata(&path).map(|m| m.len() >= max_bytes).unwrap_or(false) {
        let old = dir.join(format!("{LOG_NAME}.1"));
        let _ = fs::remove_file(&old);
        fs::rename(&path, &old)?;
    }
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "[{secs}] {}", line.replace(['\r', '\n'], " "))
}
/// 오류를 stderr 와 파일에 모두 남긴다(실패해도 무시).
pub fn report(dir: &Path, msg: &str) {
    eprintln!("{msg}");
    let _ = append(dir, msg, MAX_BYTES);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn appends_and_flattens_newlines() {
        let t = tempfile::tempdir().unwrap();
        append(t.path(), "a\nb", MAX_BYTES).unwrap();
        append(t.path(), "c", MAX_BYTES).unwrap();
        let s = fs::read_to_string(t.path().join(LOG_NAME)).unwrap();
        assert_eq!(s.lines().count(), 2);
        assert!(s.contains("a b") && s.contains("c"));
    }
    #[test]
    fn rotates_and_stays_bounded() {
        let t = tempfile::tempdir().unwrap();
        for i in 0..200 {
            append(t.path(), &format!("line {i} {}", "x".repeat(40)), 1024).unwrap();
        }
        let cur = fs::metadata(t.path().join(LOG_NAME)).unwrap().len();
        let old = fs::metadata(t.path().join(format!("{LOG_NAME}.1"))).unwrap().len();
        assert!(cur < 1024 + 100 && old < 1024 + 100, "{cur} {old}");
        assert!(!t.path().join(format!("{LOG_NAME}.2")).exists());
    }
    #[test]
    fn log_dir_is_sibling_of_index() {
        assert_eq!(log_dir_for(Path::new("base/index")), Path::new("base/logs"));
    }
}
