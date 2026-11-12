//! kvf 전용 경로. kagent 자료와 커널 객체를 공유하지 않는다.
use std::path::PathBuf;

pub fn data_dir() -> PathBuf { root("KVF_TEST_ROOT", "kvf") }
pub fn library_dir() -> PathBuf { root("KLIB_TEST_ROOT", "library") }
fn root(key: &str, leaf: &str) -> PathBuf {
    std::env::var_os(key).map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("LOCALAPPDATA 없음"))
            .join("HwpAutoDocFit").join(leaf)
    })
}
pub fn object_name(base: &str, test_root: Option<&str>) -> String {
    match test_root {
        None => base.into(),
        Some(root) => {
            let hash = root.encode_utf16().fold(0xcbf29ce484222325u64, |h,c| (h ^ c as u64).wrapping_mul(0x100000001b3));
            format!("{base}.test.{hash:016x}")
        }
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn objects_are_isolated() {
        assert_eq!(object_name("kvf", None), "kvf");
        assert_ne!(object_name("kvf", Some("a")), object_name("kvf", Some("b")));
        assert_eq!(object_name("kvf", Some("a")), object_name("kvf", Some("a")));
    }
}
