use std::env;
use std::path::{Path, PathBuf};

fn safe_version(version: &str) -> String {
    version
        .chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '_' } else { c })
        .collect::<String>()
        .replace(' ', "_")
}

fn cache_path(local_app_data: &Path, version: &str, payload_sha256: &str) -> PathBuf {
    local_app_data
        .join("HWP_AutoDocFit")
        .join(format!("app_v{}_{}", safe_version(version), &payload_sha256[..12]))
}

fn main() {
    let _ = env::args_os();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_path_matches_csharp_launcher_convention() {
        let root = Path::new(r"C:\Users\sample\AppData\Local");
        let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(
            cache_path(root, "1.80.10 Alpha 1", hash),
            PathBuf::from(r"C:\Users\sample\AppData\Local\HWP_AutoDocFit\app_v1.80.10_Alpha_1_0123456789ab")
        );
    }

    #[test]
    fn cache_path_sanitizes_windows_invalid_characters() {
        let root = Path::new(r"C:\Users\sample\AppData\Local");
        let hash = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        assert_eq!(cache_path(root, "1.2/a", hash).file_name().unwrap(), "app_v1.2_a_abcdef012345");
    }
}
