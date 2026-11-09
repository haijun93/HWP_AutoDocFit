use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};
use rayon::prelude::*;
use sha2::Digest;

const MUTEX_SUFFIX: &str = "HWP_AutoDocFit_SingleInstance_Mutex";

struct Trace {
    started: Instant,
    stage: Instant,
    enabled: bool,
}

impl Trace {
    fn new() -> Self {
        Self { started: Instant::now(), stage: Instant::now(), enabled: env::var_os("HWP_LAUNCHER_TRACE").as_deref() == Some(std::ffi::OsStr::new("1")) }
    }
    fn mark(&mut self, name: &str) {
        if self.enabled {
            eprintln!("HWP_LAUNCHER_TRACE stage={} elapsed_ms={} total_ms={}", name, self.stage.elapsed().as_millis(), self.started.elapsed().as_millis());
        }
        self.stage = Instant::now();
    }
}

#[derive(Debug, Clone)]
struct ManifestItem {
    path: String,
    length: u64,
    sha256: String,
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
                items.push(ManifestItem { path: path.replace('\\', "/"), length, sha256: (*_sha256).to_owned() });
            }
            _ => return Err("invalid manifest record".into()),
        }
    }
    let version = version.ok_or("manifest missing version")?;
    let payload_hash = payload_hash.ok_or("manifest missing payload hash")?;
    if payload_hash.len() != 64 || items.is_empty() || !items.iter().any(|i| i.path.eq_ignore_ascii_case("app_main.exe")) {
        return Err("manifest is incomplete".into());
    }
    if items.iter().any(|i| i.sha256.len() != 64) { return Err("manifest contains invalid file SHA-256".into()); }
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

fn sha256_file(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 { break; }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn extract_payload(payload: &Path, final_cache: &Path, manifest: &Manifest) -> Result<(), String> {
    use std::collections::{HashMap, HashSet};
    use std::io::Cursor;
    let payload_hash = sha256_file(payload)?;
    if !payload_hash.eq_ignore_ascii_case(&manifest.payload_hash) { return Err("payload SHA-256 mismatch".into()); }
    let parent = final_cache.parent().ok_or("cache path has no parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let staging = parent.join(format!(".staging-{}", uuid_token()));
    fs::create_dir(&staging).map_err(|e| e.to_string())?;
    let result = (|| {
        let expected: HashMap<String, &ManifestItem> = manifest.items.iter()
            .map(|item| (item.path.to_lowercase(), item)).collect();
        let mut written = HashSet::new();
        let archive_bytes = fs::read(payload).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(Cursor::new(archive_bytes)).map_err(|e| e.to_string())?;
        let mut entries = Vec::with_capacity(archive.len());
        for index in 0..archive.len() {
            let entry = archive.by_index(index).map_err(|e| e.to_string())?;
            let name = entry.name().replace('\\', "/");
            validate_relative_path(&name)?;
            let key = name.to_lowercase();
            if expected.get(&key).is_none() || !written.insert(key.clone()) { return Err(format!("unexpected or duplicate payload path: {name}")); }
            entries.push((index, name, key));
        }
        let extracted: Result<Vec<String>, String> = entries.par_iter().map(|(index, name, key)| {
            let item = expected.get(key).unwrap();
            let source = fs::File::open(payload).map_err(|e| e.to_string())?;
            let mut archive = zip::ZipArchive::new(source).map_err(|e| e.to_string())?;
            let mut entry = archive.by_index(*index).map_err(|e| e.to_string())?;
            let output = staging.join(name);
            if let Some(parent) = output.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
            let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&output).map_err(|e| e.to_string())?;
            let mut digest = sha2::Sha256::new();
            let mut size = 0u64;
            let mut buffer = [0u8; 1024 * 1024];
            loop {
                let count = entry.read(&mut buffer).map_err(|e| e.to_string())?;
                if count == 0 { break; }
                size += count as u64;
                if size > item.length { return Err(format!("payload file too large: {name}")); }
                digest.update(&buffer[..count]);
                file.write_all(&buffer[..count]).map_err(|e| e.to_string())?;
            }
            let actual = format!("{:x}", digest.finalize());
            if size != item.length || !actual.eq_ignore_ascii_case(&item.sha256) { return Err(format!("file SHA-256 mismatch: {name}")); }
            Ok(name.clone())
        }).collect();
        let extracted = extracted?;
        if extracted.len() != entries.len() { return Err("parallel extraction incomplete".into()); }
        if written.len() != expected.len() { return Err("payload is missing manifest files".into()); }
        fs::write(staging.join(".docfit-complete"), &manifest.payload_hash).map_err(|e| e.to_string())?;
        let mut attempt = 0;
        loop {
            let trash = parent.join(format!(".trash-{}", uuid_token()));
            if final_cache.exists() { fs::rename(final_cache, &trash).map_err(|e| e.to_string())?; }
            match fs::rename(&staging, final_cache) {
                Ok(()) => { let _ = fs::remove_dir_all(trash); break; }
                Err(error) if attempt < 25 => {
                    if trash.exists() { let _ = fs::rename(trash, final_cache); }
                    attempt += 1;
                    std::thread::sleep(Duration::from_millis(40));
                    if attempt == 25 { return Err(error.to_string()); }
                }
                Err(error) => return Err(error.to_string()),
            }
        }
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_dir_all(&staging); }
    result
}

fn uuid_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    format!("{:x}-{:x}-{:x}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos(), rand_token())
}

fn cleanup_old_caches(root: &Path, current_name: &str, current_version: &str) {
    let current_version = version_tuple(current_version);
    let Ok(entries) = fs::read_dir(root) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue; };
        if !name.starts_with("app_v") || name == current_name || !path.is_dir() { continue; }
        let Some(version) = name.strip_prefix("app_v").and_then(|v| v.split('_').next()) else { continue; };
        let candidate = version_tuple(version);
        if candidate.is_empty() || candidate > current_version { continue; }
        // Running app files are locked on Windows; a successful rename safely marks only unused caches.
        let trash = root.join(format!(".trash-{}", uuid_token()));
        if fs::rename(&path, &trash).is_ok() { let _ = fs::remove_dir_all(trash); }
    }
}

fn version_tuple(version: &str) -> Vec<u64> {
    version.split('.').map(|part| part.parse::<u64>().unwrap_or(u64::MAX)).collect()
}

#[cfg(windows)]
fn show_splash(version: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DefWindowProcW, RegisterClassW, ShowWindow, WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    unsafe extern "system" fn proc(hwnd: windows_sys::Win32::Foundation::HWND, msg: u32, wp: usize, lp: isize) -> isize {
        DefWindowProcW(hwnd, msg, wp, lp)
    }
    unsafe {
        let class: Vec<u16> = OsStr::new("DocFitRustSplash").encode_wide().chain(Some(0)).collect();
        let title: Vec<u16> = OsStr::new(&format!("HWP_AutoDocFit v{version} 준비 중")).encode_wide().chain(Some(0)).collect();
        let hinst = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: hinst, lpszClassName: class.as_ptr(), ..std::mem::zeroed() };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(0, class.as_ptr(), title.as_ptr(), WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            100, 100, 520, 170, std::ptr::null_mut(), std::ptr::null_mut(), hinst, std::ptr::null());
        if !hwnd.is_null() { ShowWindow(hwnd, 5); }
    }
}

#[cfg(not(windows))]
fn show_splash(_version: &str) {}
fn rand_token() -> u64 { use std::hash::{Hash, Hasher}; let mut h = std::collections::hash_map::DefaultHasher::new(); std::thread::current().id().hash(&mut h); h.finish() }

#[allow(dead_code)]
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

#[cfg(windows)]
struct SingleInstance {
    handle: windows_sys::Win32::Foundation::HANDLE,
    name: String,
}

#[cfg(windows)]
impl Drop for SingleInstance {
    fn drop(&mut self) { unsafe { windows_sys::Win32::Foundation::CloseHandle(self.handle); } }
}

#[cfg(windows)]
fn acquire_single_instance() -> Result<Option<SingleInstance>, String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    let suffix = if env::var_os("DOCFIT_LAUNCHER_TEST_ROOT").is_some() { "_Test" } else { "" };
    let create = |prefix: &str| unsafe {
        let name = format!("{prefix}{MUTEX_SUFFIX}{suffix}");
        let wide: Vec<u16> = OsStr::new(&name).encode_wide().chain(Some(0)).collect();
        let handle = CreateMutexW(std::ptr::null(), 1, wide.as_ptr());
        (handle, name, GetLastError())
    };
    let (mut handle, mut name, mut error) = create("Global\\");
    if handle.is_null() {
        (handle, name, error) = create("Local\\");
    }
    if handle.is_null() { return Err(format!("cannot create single-instance mutex ({error})")); }
    if error == ERROR_ALREADY_EXISTS {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(handle); }
        return Ok(None);
    }
    Ok(Some(SingleInstance { handle, name }))
}

#[cfg(windows)]
fn handoff_mutex(instance: &SingleInstance, child: &mut std::process::Child) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
    let event_name = format!("Local\\HWP_AutoDocFit_Handoff_{}", std::process::id());
    let wide: Vec<u16> = OsStr::new(&event_name).encode_wide().chain(Some(0)).collect();
    let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, wide.as_ptr()) };
    if event.is_null() { return Err("cannot create mutex handoff event".into()); }
    let mut interval = Duration::from_millis(100);
    let deadline = Instant::now() + Duration::from_secs(45);
    let result = loop {
        if unsafe { WaitForSingleObject(event, 0) } == 0 { break Ok(()); }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break Err(format!("app exited before mutex handoff: {status}"));
        }
        if Instant::now() >= deadline { break Err("app did not acknowledge mutex handoff within 45 seconds".into()); }
        std::thread::sleep(interval);
        interval = (interval + Duration::from_millis(100)).min(Duration::from_millis(500));
    };
    if result.is_ok() { unsafe { windows_sys::Win32::System::Threading::ReleaseMutex(instance.handle); } }
    unsafe { windows_sys::Win32::Foundation::CloseHandle(event); }
    result
}

#[cfg(not(windows))]
fn handoff_mutex(_instance: &(), _child: &mut std::process::Child) -> Result<(), String> { Ok(()) }

fn run() -> Result<i32, String> {
    let mut trace = Trace::new();
    #[cfg(windows)]
    let args: Vec<String> = windows_argv().into_iter().skip(1).collect();
    #[cfg(not(windows))]
    let args: Vec<String> = env::args().skip(1).collect();
    let manifest_path = env::var_os("DOCFIT_LAUNCHER_MANIFEST").map(PathBuf::from)
        .unwrap_or_else(|| { let mut p = env::current_exe().unwrap(); p.set_file_name(format!("{}.manifest.txt", p.file_stem().unwrap().to_string_lossy())); p });
    let manifest_text = fs::read_to_string(&manifest_path).map_err(|e| format!("cannot read launcher manifest {}: {e}", manifest_path.display()))?;
    let manifest = parse_manifest(&manifest_text)?;
    trace.mark("manifest-check");
    let self_test = args.first().map(|a| a == "--self-test").unwrap_or(false);
    #[cfg(windows)]
    let instance = if self_test || env::var_os("DOCFIT_LAUNCHER_TEST_ROOT").is_some() || args.first().map(|a| !a.starts_with('-')).unwrap_or(false) {
        None
    } else { acquire_single_instance()? };
    if let Some(expected_version) = manifest_text.lines().find_map(|line| line.strip_prefix("V\t")) {
        if expected_version != manifest.version { return Err("manifest version mismatch".into()); }
    }
    let local = env::var_os("LOCALAPPDATA").map(PathBuf::from).ok_or("LOCALAPPDATA is not set")?;
    let cache = env::var_os("DOCFIT_LAUNCHER_TEST_ROOT").map(PathBuf::from)
        .unwrap_or_else(|| local.join("HWP_AutoDocFit"));
    let cache = if env::var_os("DOCFIT_LAUNCHER_TEST_ROOT").is_some() { cache.join(cache_path(Path::new(""), &manifest.version, &manifest.payload_hash).file_name().unwrap()) }
        else { cache_path(&local, &manifest.version, &manifest.payload_hash) };
    trace.mark("cache-path");
    if !cache_is_valid(&cache, &manifest) {
        show_splash(&manifest.version);
        let payload = env::var_os("DOCFIT_LAUNCHER_PAYLOAD").map(PathBuf::from)
            .unwrap_or_else(|| { let mut p = env::current_exe().unwrap(); p.set_file_name(format!("{}.payload.zip", p.file_stem().unwrap().to_string_lossy())); p });
        extract_payload(&payload, &cache, &manifest)?;
    }
    trace.mark("manifest-validation");
    if cache_is_valid(&cache, &manifest) {
        let app = cache.join("app_main.exe");
        #[cfg(windows)]
        if instance.is_none() && !self_test {
            // A second ordinary GUI launch focuses the app via its existing instance path.
            return Ok(0);
        }
        let mut command = Command::new(app);
        let child_args = args.clone();
        command.args(&child_args).current_dir(&cache);
        for (key, value) in env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("DOCFIT_")) {
            command.env(key, value);
        }
        if let Some(path) = env::var_os("DOCFIT_LAUNCHER_PATH") { command.env("DOCFIT_LAUNCHER_PATH", path); }
        #[cfg(windows)]
        if let Some(instance) = instance.as_ref().filter(|_| !self_test) {
            let event_name = format!("Local\\HWP_AutoDocFit_Handoff_{}", std::process::id());
            command.env("DOCFIT_LAUNCHER_HANDOFF_EVENT", &event_name);
            command.env("DOCFIT_LAUNCHER_MUTEX_NAME", &instance.name);
            let mut child = command.spawn().map_err(|e| format!("cannot start cached app: {e}"))?;
            trace.mark("child-start");
            handoff_mutex(instance, &mut child)?;
            trace.mark("handoff-wait");
            if let Some(cache_root) = cache.parent() { cleanup_old_caches(cache_root, cache.file_name().unwrap().to_str().unwrap_or(""), &manifest.version); }
            return Ok(0);
        }
        #[cfg(not(windows))]
        let _instance = ();
        trace.mark("child-start");
        let status = command.status().map_err(|e| format!("cannot start cached app: {e}"))?;
        trace.mark("child-exit");
        #[cfg(windows)]
        if let Some(cache_root) = cache.parent() { cleanup_old_caches(cache_root, cache.file_name().unwrap().to_str().unwrap_or(""), &manifest.version); }
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
