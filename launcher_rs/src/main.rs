use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

#[derive(Debug, Clone)]
struct ManifestItem {
    path: String,
    length: u64,
}

#[derive(Debug, Clone)]
struct Manifest {
    version: String,
    payload_hash: String,
    items: Vec<ManifestItem>,
}

fn safe_version(version: &str) -> String {
    version.chars().map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '_' } else { c })
        .collect::<String>().replace(' ', "_")
}

fn cache_path(local_app_data: &Path, version: &str, payload_sha256: &str) -> PathBuf {
    local_app_data.join("HWP_AutoDocFit")
        .join(format!("app_v{}_{}", safe_version(version), &payload_sha256[..12]))
}

fn parse_manifest(text: &str) -> Result<Manifest, String> {
    let mut version = None;
    let mut payload_hash = None;
    let mut items = Vec::new();
    for line in text.lines() {
        let fields: Vec<_> = line.splitn(4, '\t').collect();
        match fields.as_slice() {
            ["V", value] => version = Some((*value).to_owned()),
            ["P", value] => payload_hash = Some((*value).to_owned()),
            ["F", _sha256, length, path] => {
                let length = length.parse::<u64>().map_err(|_| "invalid manifest file length")?;
                validate_relative_path(path)?;
                items.push(ManifestItem { path: path.replace('\\', "/"), length });
            }
            _ => return Err("invalid manifest record".into()),
        }
    }
    let version = version.ok_or("manifest missing version")?;
    let payload_hash = payload_hash.ok_or("manifest missing payload hash")?;
    if payload_hash.len() != 64 || items.is_empty() || !items.iter().any(|i| i.path.eq_ignore_ascii_case("app_main.exe")) {
        return Err("manifest is incomplete".into());
    }
    Ok(Manifest { version, payload_hash, items })
}

fn validate_relative_path(path: &str) -> Result<(), String> {
    let normalized = path.replace('\\', "/");
    if normalized.is_empty() || normalized.starts_with('/') || normalized.contains(':')
        || normalized.split('/').any(|part| part.is_empty() || part == "." || part == "..") {
        return Err(format!("unsafe manifest path: {path}"));
    }
    Ok(())
}

fn cache_is_valid(cache: &Path, manifest: &Manifest) -> bool {
    let marker = match fs::read_to_string(cache.join(".docfit-complete")) { Ok(v) => v, Err(_) => return false };
    if marker.trim() != manifest.payload_hash { return false; }
    manifest.items.iter().all(|item| {
        let path = cache.join(&item.path);
        fs::metadata(path).map(|m| m.is_file() && m.len() == item.length).unwrap_or(false)
    })
}

fn quote_windows_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.chars().any(|c| c.is_whitespace() || c == '"') { return arg.to_owned(); }
    let mut out = String::from("\"");
    let mut slashes = 0;
    for ch in arg.chars() {
        if ch == '\\' { slashes += 1; continue; }
        if ch == '"' {
            out.push_str(&"\\".repeat(slashes * 2 + 1));
            out.push('"');
        } else {
            out.push_str(&"\\".repeat(slashes));
            out.push(ch);
        }
        slashes = 0;
    }
    out.push_str(&"\\".repeat(slashes * 2));
    out.push('"');
    out
}

#[cfg(windows)]
fn windows_argv() -> Vec<String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::System::Environment::GetCommandLineW;
    use windows_sys::Win32::UI::Shell::CommandLineToArgvW;
    unsafe {
        let mut argc = 0;
        let argv = CommandLineToArgvW(GetCommandLineW(), &mut argc);
        if argv.is_null() { return env::args().collect(); }
        let args = std::slice::from_raw_parts(argv, argc as usize).iter().map(|arg| {
            let mut len = 0;
            while *(*arg).add(len) != 0 { len += 1; }
            String::from_utf16_lossy(std::slice::from_raw_parts(*arg, len))
        }).collect();
        LocalFree(argv.cast());
        args
    }
}

fn run() -> Result<i32, String> {
    #[cfg(windows)]
    let args: Vec<String> = windows_argv().into_iter().skip(1).collect();
    #[cfg(not(windows))]
    let args: Vec<String> = env::args().skip(1).collect();
    let manifest_path = env::var_os("DOCFIT_LAUNCHER_MANIFEST").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env::current_exe().unwrap()).with_extension("manifest.txt"));
    let manifest_text = fs::read_to_string(&manifest_path).map_err(|e| format!("cannot read launcher manifest {}: {e}", manifest_path.display()))?;
    let manifest = parse_manifest(&manifest_text)?;
    let local = env::var_os("LOCALAPPDATA").map(PathBuf::from).ok_or("LOCALAPPDATA is not set")?;
    let cache = env::var_os("DOCFIT_LAUNCHER_TEST_ROOT").map(PathBuf::from)
        .unwrap_or_else(|| local.join("HWP_AutoDocFit"));
    let cache = if env::var_os("DOCFIT_LAUNCHER_TEST_ROOT").is_some() { cache.join(cache_path(Path::new(""), &manifest.version, &manifest.payload_hash).file_name().unwrap()) }
        else { cache_path(&local, &manifest.version, &manifest.payload_hash) };
    if cache_is_valid(&cache, &manifest) {
        let app = cache.join("app_main.exe");
        let mut command = Command::new(app);
        command.args(&args).current_dir(&cache);
        if let Some(path) = env::var_os("DOCFIT_LAUNCHER_PATH") { command.env("DOCFIT_LAUNCHER_PATH", path); }
        let status = command.status().map_err(|e| format!("cannot start cached app: {e}"))?;
        return Ok(status.code().unwrap_or(1));
    }
    Err("cache is not ready; payload extraction is not implemented yet".into())
}

fn main() -> ExitCode {
    match run() { Ok(code) => ExitCode::from(code.clamp(0, 255) as u8), Err(error) => { eprintln!("{error}"); ExitCode::FAILURE } }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn cache_path_matches_csharp_launcher_convention() {
        let root = Path::new(r"C:\Users\sample\AppData\Local");
        assert_eq!(cache_path(root, "1.80.10 Alpha 1", HASH), PathBuf::from(r"C:\Users\sample\AppData\Local\HWP_AutoDocFit\app_v1.80.10_Alpha_1_0123456789ab"));
    }

    #[test]
    fn cache_path_sanitizes_windows_invalid_characters() {
        let root = Path::new(r"C:\Users\sample\AppData\Local");
        assert_eq!(cache_path(root, "1.2/a", HASH).file_name().unwrap(), "app_v1.2_a_0123456789ab");
    }

    #[test]
    fn manifest_parsing_and_path_safety() {
        let parsed = parse_manifest(&format!("V\t1.2\nP\t{HASH}\nF\t{HASH}\t3\tapp_main.exe\n")).unwrap();
        assert_eq!(parsed.items[0].length, 3);
        assert!(parse_manifest(&format!("V\t1\nP\t{HASH}\nF\t{HASH}\t3\t../evil\n")).is_err());
    }

    #[test]
    fn windows_quoting_preserves_spaces_quotes_and_trailing_slashes() {
        for arg in ["plain", "has space", "a\"b", "ends with slash\\"] {
            let encoded = quote_windows_arg(arg);
            assert_eq!(decode_windows_arg(&encoded), arg);
        }
    }

    fn decode_windows_arg(encoded: &str) -> String {
        let mut result = String::new();
        let mut chars = encoded.chars().peekable();
        let mut quoted = false;
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                let mut count = 1;
                while chars.peek() == Some(&'\\') { chars.next(); count += 1; }
                if chars.peek() == Some(&'"') {
                    chars.next(); result.push_str(&"\\".repeat(count / 2));
                    if count % 2 == 1 { result.push('"'); } else { quoted = !quoted; }
                } else { result.push_str(&"\\".repeat(count)); }
            } else if ch == '"' { quoted = !quoted; } else { result.push(ch); }
        }
        result
    }
}
