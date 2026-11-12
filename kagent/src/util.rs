//! 작은 도움 함수들.
//! kvf(가상 폴더 창)도 이 파일을 컴파일한다. `std`·`windows-sys`·`crate::util` 말고는 쓰지 말 것.

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// %LOCALAPPDATA%\HwpAutoDocFit\kagent (KAGENT_DATA로 바꿀 수 있음 — 시험용).
pub fn data_dir() -> String {
    if let Ok(root) = std::env::var("KAGENT_TEST_ROOT") {
        return root;
    }
    if let Ok(dir) = std::env::var("KAGENT_DATA") {
        return dir;
    }
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    format!("{base}\\HwpAutoDocFit\\kagent")
}

/// 시험 실행은 기본 인스턴스와 충돌하지 않는 별도 커널 객체 이름을 쓴다.
pub fn instance_suffix() -> Option<String> {
    std::env::var("KAGENT_TEST_ROOT").ok().map(|root| {
        let hash = root.encode_utf16().fold(0xcbf29ce484222325u64, |h, c| (h ^ c as u64).wrapping_mul(0x100000001b3));
        format!(".test.{hash:016x}")
    })
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// 표준 base64 풀기(패딩 필수, 공백·다른 글자가 있으면 None).
pub fn unbase64(text: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    }
    let bytes = text.as_bytes();
    if bytes.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let chunks = bytes.len() / 4;
    for (i, chunk) in bytes.chunks(4).enumerate() {
        let pad = chunk.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && i + 1 != chunks) {
            return None;
        }
        let mut n = 0u32;
        for &c in &chunk[..4 - pad] {
            n = n << 6 | val(c)?;
        }
        n <<= 6 * pad as u32;
        let b = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&b[..3 - pad]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_is_the_private_data_directory_and_changes_instance_name() {
        let _env = TestEnvironment::lock();
        let root = std::env::temp_dir().join(format!("kagent-root-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        unsafe { std::env::set_var("KAGENT_TEST_ROOT", &root); }
        assert_eq!(data_dir(), root.to_string_lossy());
        let first = instance_suffix().unwrap();
        unsafe { std::env::set_var("KAGENT_TEST_ROOT", root.join("other")); }
        assert_ne!(first, instance_suffix().unwrap());
        unsafe { std::env::remove_var("KAGENT_TEST_ROOT"); }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn b64() {
        assert_eq!(super::base64(b""), "");
        assert_eq!(super::base64(b"f"), "Zg==");
        assert_eq!(super::base64(b"fo"), "Zm8=");
        assert_eq!(super::base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn unbase64_round_trips_and_rejects_bad_input() {
        for data in [&b""[..], b"f", b"fo", b"foo", b"foobar", &[0u8, 255, 128, 7, 0][..]] {
            assert_eq!(super::unbase64(&super::base64(data)).as_deref(), Some(data));
        }
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(super::unbase64(&super::base64(&all)).unwrap(), all);
        assert_eq!(super::unbase64("Zg="), None); // 길이
        assert_eq!(super::unbase64("Z==g"), None); // 패딩 위치
        assert_eq!(super::unbase64("Zg==Zg=="), None); // 중간 패딩
        assert_eq!(super::unbase64("Zm9v YmFy"), None);
    }
}

// Environment variables are process-wide; restore them even when a test panics.
#[cfg(test)]
static TEST_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[cfg(test)]
pub struct TestEnvironment {
    _guard: std::sync::MutexGuard<'static, ()>,
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
}
#[cfg(test)]
impl TestEnvironment {
    pub fn lock() -> Self {
        let guard = TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        Self { _guard: guard, saved: ["KAGENT_TEST_ROOT", "KAGENT_DATA"].into_iter()
            .map(|key| (key, std::env::var_os(key))).collect() }
    }
}
#[cfg(test)]
impl Drop for TestEnvironment {
    fn drop(&mut self) {
        for (key, value) in &self.saved {
            unsafe { match value { Some(v) => std::env::set_var(key, v), None => std::env::remove_var(key) } }
        }
    }
}
