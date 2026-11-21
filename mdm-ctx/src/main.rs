//! mdm-ctx — 탐색기 오른쪽 버튼 메뉴 '몽땅파일변환' 전달 프로그램.
//! 클릭마다 뜨는 여러 프로세스가 선택 경로를 스풀 폴더에 남기고, 뮤텍스를 잡은 한 프로세스만 대표가 되어
//! 요청이 조용해질 때까지 모은 뒤 앱 `convert` 를 한 번 띄우고 끝난다. 나머지는 바로 종료한다.
#![windows_subsystem = "windows"]
mod logic;

use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, HANDLE};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, OPEN_EXISTING, ReadFile, WriteFile};
use windows_sys::Win32::System::Pipes::PeekNamedPipe;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_SETFOREGROUND, MB_TOPMOST};

fn wide(s: &str) -> Vec<u16> { std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect() }
fn env_ms(name: &str, default: u64) -> u64 { std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default) }
fn now_ms() -> u128 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0) }

/// 데이터 루트. 시험은 MDM_CTX_TEST_ROOT 로 임시 폴더를 쓴다(뮤텍스 이름도 달라진다).
fn root() -> (PathBuf, String) {
    if let Some(test) = std::env::var_os("MDM_CTX_TEST_ROOT") {
        let tag: String = Path::new(&test).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
            .chars().filter(|c| c.is_alphanumeric()).collect();
        return (PathBuf::from(test), format!(".{tag}"));
    }
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    (base.join("HWP_AutoDocFit"), String::new())
}

fn log(root: &Path, text: &str) {
    let _ = fs::create_dir_all(root);
    let line = format!("{} {}\n", now_ms(), text);
    let _ = fs::OpenOptions::new().create(true).append(true).open(root.join("ctx.log"))
        .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
}

fn alert(text: &str) {
    if std::env::var_os("MDM_CTX_TEST_ROOT").is_some() { return; }
    unsafe { MessageBoxW(std::ptr::null_mut(), wide(text).as_ptr(), wide("몽땅파일변환").as_ptr(), MB_ICONERROR | MB_SETFOREGROUND | MB_TOPMOST); }
}

fn write_request(dir: &Path, path: &str, seq: usize) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let name = format!("{:020}_{}_{}", now_ms(), std::process::id(), seq);
    let tmp = dir.join(format!(".{name}.tmp"));
    fs::write(&tmp, path.as_bytes())?;
    fs::rename(&tmp, dir.join(format!("{name}.req")))
}

fn requests(dir: &Path) -> Vec<PathBuf> {
    let mut list: Vec<PathBuf> = fs::read_dir(dir).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    list.retain(|p| p.extension().is_some_and(|e| e == "req"));
    list.sort();
    list
}

/// 낡은 요청(대표가 비정상 종료돼 남은 것)은 새 변환에 섞지 않고 지운다.
fn drop_stale(dir: &Path) {
    for req in requests(dir) {
        let stamp: u128 = req.file_name().and_then(|n| n.to_str()).and_then(|n| n.split('_').next()).and_then(|s| s.parse().ok()).unwrap_or(0);
        if now_ms().saturating_sub(stamp) > 120_000 { let _ = fs::remove_file(req); }
    }
}

struct Mutex(*mut core::ffi::c_void);
impl Drop for Mutex { fn drop(&mut self) { unsafe { CloseHandle(self.0); } } }

fn try_lead(name: &str) -> Option<Mutex> {
    unsafe {
        let handle = CreateMutexW(std::ptr::null(), 0, wide(name).as_ptr());
        if handle.is_null() { return None; }
        if GetLastError() == ERROR_ALREADY_EXISTS { CloseHandle(handle); return None; }
        Some(Mutex(handle))
    }
}

/// 새 요청이 quiet 동안 없거나 상한(시간·개수)에 닿을 때까지 모은다.
fn collect(dir: &Path, quiet: Duration, max_wait: Duration, max_files: usize) -> Vec<PathBuf> {
    let start = Instant::now();
    let mut last = start;
    let mut seen = 0usize;
    loop {
        let count = requests(dir).len();
        if count != seen { seen = count; last = Instant::now(); }
        if seen >= max_files || start.elapsed() >= max_wait || last.elapsed() >= quiet { break; }
        std::thread::sleep(Duration::from_millis(25));
    }
    requests(dir).into_iter().take(max_files).collect()
}

const CONVERT_PIPE: &str = r"\\.\pipe\HwpAutoDocFit.convert";

fn json_string(value: &str, out: &mut String) {
    out.push('"');
    for ch in value.chars() { match ch { '"' => out.push_str("\\\""), '\\' => out.push_str("\\\\"), '\n' => out.push_str("\\n"), '\r' => out.push_str("\\r"), '\t' => out.push_str("\\t"), c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)), c => out.push(c) } }
    out.push('"');
}

fn pipe_payload(target: &str, paths: &[String]) -> Option<String> {
    if paths.is_empty() || paths.len() > 2000 { return None; }
    let mut request = String::from("{\"cmd\":\"convert_open\",\"target\":");
    json_string(target, &mut request); request.push_str(",\"paths\":[");
    for (i, path) in paths.iter().enumerate() { if i != 0 { request.push(','); } json_string(path, &mut request); }
    request.push_str("]}\n");
    (request.len() <= 1_048_576).then_some(request)
}

fn send_to_running_app(target: &str, paths: &[String]) -> bool {
    use std::ptr;
    let Some(request) = pipe_payload(target, paths) else { return false; };
    unsafe {
        let deadline = Instant::now() + Duration::from_secs(2);
        let pipe_name = if std::env::var_os("MDM_CTX_TEST_ROOT").is_some() {
            std::env::var("MDM_CTX_TEST_PIPE").unwrap_or_else(|_| CONVERT_PIPE.to_string())
        } else { CONVERT_PIPE.to_string() };
        let name = wide(&pipe_name);
        let handle: HANDLE = loop {
            let candidate = CreateFileW(name.as_ptr(), 0x8000_0000 | 0x4000_0000, 0, ptr::null(), OPEN_EXISTING, 0, ptr::null_mut());
            if !candidate.is_null() && candidate as isize != -1 { break candidate; }
            let err = GetLastError();
            if err != ERROR_PIPE_BUSY && err != ERROR_FILE_NOT_FOUND || Instant::now() >= deadline { return false; }
            std::thread::sleep(Duration::from_millis(25));
        };
        let mut written = 0;
        let ok = WriteFile(handle, request.as_ptr(), request.len() as u32, &mut written, ptr::null_mut()) != 0 && written as usize == request.len();
        let mut reply = Vec::new();
        while ok && Instant::now() < deadline {
            let mut available = 0;
            if PeekNamedPipe(handle, ptr::null_mut(), 0, ptr::null_mut(), &mut available, ptr::null_mut()) == 0 { break; }
            if available > 0 {
                let mut buf = vec![0u8; available.min(4096) as usize]; let mut got = 0;
                if ReadFile(handle, buf.as_mut_ptr(), buf.len() as u32, &mut got, ptr::null_mut()) == 0 { break; }
                reply.extend_from_slice(&buf[..got as usize]);
                if reply.contains(&b'\n') { break; }
            } else { std::thread::sleep(Duration::from_millis(10)); }
        }
        CloseHandle(handle);
        let response = String::from_utf8_lossy(&reply);
        response.trim_start().starts_with("{\"ok\":true")
    }
}

fn launch(parsed: &logic::Parsed, paths: &[String], root: &Path) {
    let target = parsed.target.as_deref().unwrap_or("");
    if paths.len() <= 2000 && send_to_running_app(target, paths) {
        log(root, &format!("실행 중 앱에 파이프 전달 {}개 → {}", paths.len(), target));
        return;
    }
    log(root, "앱 파이프 연결/응답 실패 또는 크기 초과: 기존 실행 방식으로 폴백");
    let fixed = parsed.app.as_deref().map_or(0, str::len) + 40 + parsed.passthrough.iter().map(|a| a.len() + 3).sum::<usize>();
    let chunks = if target == "pdfm" { vec![paths.to_vec()] } else { logic::chunk_paths(paths, fixed) };
    if target == "pdfm" {
        let total: usize = paths.iter().map(|p| p.chars().count() + 3).sum::<usize>() + fixed;
        if total > logic::MAX_COMMAND_CHARS {
            log(root, &format!("pdfm 경로가 너무 길다: {}개", paths.len()));
            alert("선택한 파일이 너무 많거나 경로가 길어 PDF 하나로 합칠 수 없습니다.\n파일 수를 줄여 다시 선택해 주세요.");
            return;
        }
    }
    for chunk in chunks.into_iter().filter(|c| !c.is_empty()) {
        let mut cmd = std::process::Command::new(parsed.app.as_deref().unwrap_or(""));
        cmd.arg("convert").args(&chunk).arg("--to").arg(target).args(&parsed.passthrough);
        match cmd.spawn() {
            Ok(_) => log(root, &format!("전달 {}개 → {}", chunk.len(), target)),
            Err(e) => { log(root, &format!("앱 실행 실패: {e}")); alert("몽땅파일변환: 앱을 실행하지 못했습니다."); }
        }
    }
}

fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (root, tag) = root();
    let parsed = match logic::parse(&args) {
        Ok(p) => p,
        Err(e) => { log(&root, &format!("인자 오류: {e}")); return 2; }
    };
    let target = parsed.target.clone().unwrap_or_default();
    let safe: String = target.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let spool = root.join("ctx_spool").join(&safe);
    drop_stale(&spool);
    for (i, p) in parsed.paths.iter().enumerate() {
        if let Err(e) = write_request(&spool, p, i) { log(&root, &format!("요청 기록 실패: {e}")); return 1; }
    }
    let mutex_name = format!("Local\\HwpAutoDocFit.ctx.{safe}{tag}");
    let (quiet, max_wait) = (Duration::from_millis(env_ms("MDM_CTX_QUIET_MS", 1500)), Duration::from_millis(env_ms("MDM_CTX_MAX_WAIT_MS", 30_000)));
    // 대표가 끝나는 순간 막 도착한 요청을 놓치지 않도록, 뮤텍스를 놓은 뒤 남은 요청이 있으면 다시 대표를 시도한다.
    loop {
        let Some(guard) = try_lead(&mutex_name) else { return 0 }; // 추종자: 요청만 남기고 종료
        let reqs = collect(&spool, quiet, max_wait, 200);
        let mut paths = Vec::new();
        for req in &reqs {
            if let Ok(text) = fs::read_to_string(req) { paths.push(text); }
            let _ = fs::remove_file(req);
        }
        let valid: Vec<String> = paths.into_iter()
            .filter(|p| logic::supported(p))
            .filter(|p| fs::symlink_metadata(p).map(|m| m.is_file() && !m.file_type().is_symlink()).unwrap_or(false))
            .collect();
        let valid = logic::dedup_sort(valid);
        if valid.is_empty() { log(&root, "전달할 유효한 파일이 없다"); } else { launch(&parsed, &valid, &root); }
        drop(guard);
        if requests(&spool).is_empty() { return 0; }
    }
}

fn main() { std::process::exit(run()); }

#[cfg(test)]
mod pipe_tests {
    use super::pipe_payload;

    #[test]
    fn pipe_payload_enforces_path_count_and_byte_limit() {
        assert!(pipe_payload("pdf", &[]).is_none());
        let many = vec!["a.pdf".to_string(); 2001];
        assert!(pipe_payload("pdf", &many).is_none());
        let huge = vec!["x".repeat(1_048_570) + ".pdf"];
        assert!(pipe_payload("pdf", &huge).is_none());
        let one = pipe_payload("pdf", &["한글\" 파일.pdf".to_string()]).unwrap();
        assert!(one.contains("\\\""));
    }
}
