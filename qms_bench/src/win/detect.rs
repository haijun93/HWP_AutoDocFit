use super::everything::{
    find_everything_window, get_everything_version, is_everything_db_loaded, wide_str,
};
use super::kfind::DEFAULT_KFIND_PIPE_NAME;
use super::process::find_pids_by_name;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use windows_sys::Win32::{
    Foundation::CloseHandle,
    System::{
        Pipes::WaitNamedPipeW,
        Registry::{
            RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
            HKEY_LOCAL_MACHINE, KEY_READ,
        },
        Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION},
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EverythingDetect {
    pub installed: bool,
    pub running: bool,
    pub pids: Vec<u32>,
    pub path: Option<String>,
    pub version: Option<String>,
    pub db_loaded: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KfindDetect {
    pub running: bool,
    pub pids: Vec<u32>,
    pub pipe_name: String,
    pub pipe_exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectReport {
    pub ok: bool,
    pub command: String,
    pub everything: EverythingDetect,
    pub kfind: KfindDetect,
}

pub fn detect_all() -> DetectReport {
    let everything = detect_everything();
    let kfind = detect_kfind();
    DetectReport {
        ok: true,
        command: "detect".into(),
        everything,
        kfind,
    }
}

pub fn detect_everything() -> EverythingDetect {
    let window_info = find_everything_window();
    let mut pids = find_pids_by_name("Everything.exe");
    if let Some((_, pid)) = window_info {
        if !pids.contains(&pid) {
            pids.push(pid);
        }
    }
    let running = !pids.is_empty() || window_info.is_some();

    // 1. 경로 조회: 레지스트리 -> 실행 중 프로세스 -> 표준 위치
    let exe_path = find_everything_path(&pids);

    // 2. 버전 및 DB 로드 상태
    let (version, db_loaded) = if let Some((hwnd, _)) = window_info {
        let ver = get_everything_version(hwnd);
        let loaded = is_everything_db_loaded(hwnd);
        (Some(ver), Some(loaded))
    } else {
        (None, None)
    };

    let installed = exe_path.is_some();

    EverythingDetect {
        installed,
        running,
        pids,
        path: exe_path,
        version,
        db_loaded,
    }
}

pub fn detect_kfind() -> KfindDetect {
    let pids = find_pids_by_name("kfind.exe");
    let running = !pids.is_empty();
    let pipe_name = DEFAULT_KFIND_PIPE_NAME.to_string();
    let pipe_exists = check_pipe_exists(&pipe_name);

    KfindDetect {
        running,
        pids,
        pipe_name,
        pipe_exists,
    }
}

fn check_pipe_exists(pipe_name: &str) -> bool {
    let wide = wide_str(pipe_name);
    unsafe {
        // WaitNamedPipeW with 0 timeout checks if at least one instance is available
        if WaitNamedPipeW(wide.as_ptr(), 0) != 0 {
            return true;
        }
    }
    // 직접 빠른 열기 시도
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(pipe_name)
        .is_ok()
}

fn find_everything_path(pids: &[u32]) -> Option<String> {
    // 1) 레지스트리 조회
    if let Some(p) = find_everything_in_registry() {
        if Path::new(&p).is_file() {
            return Some(p);
        }
    }

    // 2) 실행 중 프로세스 경로 조회
    for &pid in pids {
        if let Some(p) = get_process_image_path(pid) {
            if Path::new(&p).is_file() {
                return Some(p);
            }
        }
    }

    // 3) 표준 설치 위치 조회
    for candidate in get_standard_everything_paths() {
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }

    None
}

fn find_everything_in_registry() -> Option<String> {
    // App Paths\Everything.exe
    let subkey = wide_str(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\Everything.exe");
    if let Some(path) = read_reg_string(HKEY_LOCAL_MACHINE, &subkey, None) {
        return Some(path);
    }
    if let Some(path) = read_reg_string(HKEY_CURRENT_USER, &subkey, None) {
        return Some(path);
    }

    // voidtools\Everything -> InstallLocation or ExePath
    let subkey_void = wide_str(r"SOFTWARE\voidtools\Everything");
    let val_install = wide_str("InstallLocation");
    if let Some(dir) = read_reg_string(HKEY_LOCAL_MACHINE, &subkey_void, Some(&val_install)) {
        let p = PathBuf::from(dir).join("Everything.exe");
        if p.is_file() {
            return Some(p.to_string_lossy().to_string());
        }
    }
    if let Some(dir) = read_reg_string(HKEY_CURRENT_USER, &subkey_void, Some(&val_install)) {
        let p = PathBuf::from(dir).join("Everything.exe");
        if p.is_file() {
            return Some(p.to_string_lossy().to_string());
        }
    }

    None
}

fn read_reg_string(root: HKEY, subkey: &[u16], value_name: Option<&[u16]>) -> Option<String> {
    unsafe {
        let mut hkey: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(root, subkey.as_ptr(), 0, KEY_READ, &mut hkey) != 0 {
            return None;
        }

        let val_ptr = value_name.map(|v| v.as_ptr()).unwrap_or(std::ptr::null());
        let mut buf_bytes = [0u8; 1024];
        let mut size = buf_bytes.len() as u32;
        let mut typ = 0u32;

        let res = RegQueryValueExW(
            hkey,
            val_ptr,
            std::ptr::null_mut(),
            &mut typ,
            buf_bytes.as_mut_ptr(),
            &mut size,
        );
        RegCloseKey(hkey);

        if res == 0 && size > 0 {
            let u16_len = (size as usize) / 2;
            let slice = std::slice::from_raw_parts(buf_bytes.as_ptr() as *const u16, u16_len);
            let null_pos = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
            return Some(String::from_utf16_lossy(&slice[..null_pos]));
        }
    }
    None
}

fn get_process_image_path(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size);
        CloseHandle(handle);

        if ok != 0 && size > 0 {
            return Some(String::from_utf16_lossy(&buf[..size as usize]));
        }
    }
    None
}

fn get_standard_everything_paths() -> Vec<PathBuf> {
    let mut list = Vec::new();
    if let Ok(pf) = std::env::var("ProgramFiles") {
        list.push(PathBuf::from(pf).join("Everything").join("Everything.exe"));
    }
    if let Ok(pfx86) = std::env::var("ProgramFiles(x86)") {
        list.push(
            PathBuf::from(pfx86)
                .join("Everything")
                .join("Everything.exe"),
        );
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        list.push(
            PathBuf::from(local)
                .join("Programs")
                .join("Everything")
                .join("Everything.exe"),
        );
    }
    list
}
