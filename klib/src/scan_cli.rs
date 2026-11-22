//! 병렬 사용자 모드 파일 인덱스 스캐너 (`klib scan`).
use std::{
    collections::VecDeque,
    ffi::OsString,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{atomic::{AtomicBool, Ordering}, mpsc, Arc, Mutex},
    thread,
};

#[cfg(windows)]
use std::os::windows::ffi::{OsStrExt, OsStringExt};

const DEFAULT_EXTS: &[&str] = &["hwp", "hwpx", "xls", "xlsx", "ppt", "pptx", "pdf"];
const EXCLUDED_DIRS: &[&str] = &[
    "windows", "program files", "program files (x86)", "programdata", "$recycle.bin",
    "system volume information", "appdata", "node_modules", ".git", ".svn", ".hg",
    "venv", ".venv",
];

#[derive(Debug, Clone, PartialEq)]
struct Item { path: String, name: String, ext: String, size: u64, mtime: f64, mtime_ns: i128, cloud: bool }

#[derive(Default)]
struct Options { roots: Vec<PathBuf>, excludes: Vec<PathBuf>, exts: Vec<String> }

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut o = Options { exts: DEFAULT_EXTS.iter().map(|s| s.to_string()).collect(), ..Options::default() };
    let mut i = 0;
    while i < args.len() {
        let key = args[i].as_str(); i += 1;
        match key {
            "--roots" | "--exclude" | "--ext" => {
                if i >= args.len() { return Err(format!("{key} requires a value")); }
                let mut values = Vec::new();
                while i < args.len() && !args[i].starts_with("--") { values.push(args[i].clone()); i += 1; }
                if values.is_empty() { return Err(format!("{key} requires a value")); }
                match key {
                    "--roots" => o.roots.extend(values.into_iter().map(PathBuf::from)),
                    "--exclude" => o.excludes.extend(values.into_iter().map(PathBuf::from)),
                    _ => o.exts = values.join(",").split(',').map(|x| x.trim().trim_start_matches('.').to_lowercase()).filter(|x| !x.is_empty()).collect(),
                }
            }
            "--json-lines" => {},
            _ => return Err(format!("unknown argument: {key}")),
        }
    }
    if o.roots.is_empty() { return Err("scan requires --roots <folder...>".into()); }
    Ok(o)
}

fn fold(s: &str) -> String { s.to_lowercase() }
fn excluded_name(name: &str) -> bool {
    let s = name.trim_matches(|c: char| c.is_whitespace() || "'\"‘’“”`\u{a0}\u{200b}\u{feff}".contains(c)).to_lowercase();
    s.starts_with("#ieeee")
}
fn excluded_path(path: &Path, base: &Path, custom: &[PathBuf]) -> bool {
    let rel = path.strip_prefix(base).unwrap_or(path);
    if rel.components().any(|c| EXCLUDED_DIRS.contains(&fold(&c.as_os_str().to_string_lossy()).as_str())) { return true; }
    let path_key = path.to_string_lossy().replace('/', "\\").to_lowercase();
    custom.iter().any(|p| {
        let key = p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
        path_key == key || path_key.strip_prefix(&key).is_some_and(|tail| tail.starts_with('\\'))
    })
}

#[cfg(windows)]
fn wide(s: &std::ffi::OsStr) -> Vec<u16> { s.encode_wide().chain(Some(0)).collect() }

#[cfg(windows)]
fn scan_dir(dir: PathBuf, base: PathBuf, exts: Arc<Vec<String>>, excludes: Arc<Vec<PathBuf>>, todo: mpsc::Sender<PathBuf>, out: mpsc::Sender<Item>, cancel: Arc<AtomicBool>, errors: Arc<AtomicBool>) {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{FindFirstFileExW, FindNextFileW, FindExInfoBasic, FindExSearchNameMatch, WIN32_FIND_DATAW, FIND_FIRST_EX_LARGE_FETCH},
    };
    if cancel.load(Ordering::Relaxed) { return; }
    let mut query = dir.clone(); query.push("*");
    let q = wide(query.as_os_str());
    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    let h = unsafe { FindFirstFileExW(q.as_ptr(), FindExInfoBasic, (&mut data as *mut WIN32_FIND_DATAW).cast(), FindExSearchNameMatch, std::ptr::null(), FIND_FIRST_EX_LARGE_FETCH) };
    if h == INVALID_HANDLE_VALUE { errors.store(true, Ordering::Relaxed); return; }
    loop {
        if cancel.load(Ordering::Relaxed) { break; }
        let len = data.cFileName.iter().position(|&x| x == 0).unwrap_or(data.cFileName.len());
        let name_os = OsString::from_wide(&data.cFileName[..len]);
        let name = name_os.to_string_lossy().into_owned();
        if name != "." && name != ".." && !name.starts_with('.') && !excluded_name(&name) {
            let attrs = data.dwFileAttributes;
            let mut path = dir.clone(); path.push(&name_os);
            if attrs & 0x10 != 0 {
                if attrs & 0x400 == 0 && !excluded_path(&path, &base, &excludes) { let _ = todo.send(path); }
            } else if attrs & 0x400 == 0 && !excluded_path(&path, &base, &excludes) {
                let ext = Path::new(&name).extension().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
                if exts.iter().any(|x| x == &ext) {
                    let high = data.ftLastWriteTime.dwHighDateTime as u64;
                    let low = data.ftLastWriteTime.dwLowDateTime as u64;
                    let ticks = (high << 32) | low;
                    let size = ((data.nFileSizeHigh as u64) << 32) | data.nFileSizeLow as u64;
                    let attrs_cloud = attrs & (0x0040_0000 | 0x0004_0000 | 0x0000_1000) != 0;
                    let attrs_recall = attrs & (0x0040_0000 | 0x0004_0000) != 0;
                    let filetime_ns = (ticks as i128 - 116_444_736_000_000_000i128) * 100;
                    let mtime = filetime_ns as f64 / 1e9;
                    let item = Item { path: path.to_string_lossy().into_owned(), name, ext, size,
                        mtime, mtime_ns: filetime_ns, cloud: attrs_cloud || attrs_recall };
                    let _ = out.send(item);
                }
            }
        }
        if unsafe { FindNextFileW(h, &mut data) } == 0 {
            let e = unsafe { GetLastError() }; if e != ERROR_NO_MORE_FILES { errors.store(true, Ordering::Relaxed); }
            break;
        }
    }
    unsafe { CloseHandle(h); }
}

#[cfg(not(windows))]
fn scan_dir(_dir: PathBuf, _base: PathBuf, _exts: Arc<Vec<String>>, _excludes: Arc<Vec<PathBuf>>, _todo: mpsc::Sender<PathBuf>, _out: mpsc::Sender<Item>, _cancel: Arc<AtomicBool>, _errors: Arc<AtomicBool>) {}

fn escape(s: &str) -> String { let mut o = String::with_capacity(s.len()+2); o.push('"'); for c in s.chars() { match c { '"'=>o.push_str("\\\""), '\\'=>o.push_str("\\\\"), '\n'=>o.push_str("\\n"), '\r'=>o.push_str("\\r"), '\t'=>o.push_str("\\t"), c if c < ' '=>o.push_str(&format!("\\u{:04x}", c as u32)), c=>o.push(c) } } o.push('"'); o }
fn emit(item: &Item) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{{\"path\":{},\"name\":{},\"ext\":{},\"size\":{},\"mtime\":{},\"mtime_ns\":{},\"cloud\":{}}}", escape(&item.path), escape(&item.name), escape(&item.ext), item.size, item.mtime, item.mtime_ns, item.cloud)
}

pub fn run(args: &[String]) -> Result<(), String> {
    let opts = parse_args(args)?;
    #[cfg(not(windows))] { let _ = opts; return Err("klib scan requires Windows".into()); }
    #[cfg(windows)] {
        let stdin_cancel = Arc::new(AtomicBool::new(false));
        let scan_errors = Arc::new(AtomicBool::new(false));
        let exts = Arc::new(opts.exts); let excludes = Arc::new(opts.excludes);
        let (out_tx, out_rx) = mpsc::channel::<Item>();
        let queue = Arc::new((Mutex::new(VecDeque::new()), std::sync::Condvar::new()));
        let pending = Arc::new(std::sync::atomic::AtomicUsize::new(opts.roots.len()));
        for root in &opts.roots { queue.0.lock().unwrap().push_back((root.clone(), root.clone())); }
        let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).max(1);
        let mut joins = Vec::new();
        for _ in 0..workers {
            let q = queue.clone(); let pending = pending.clone(); let exts = exts.clone(); let excludes = excludes.clone();
            let cancel = stdin_cancel.clone(); let out = out_tx.clone(); let errors = scan_errors.clone();
            joins.push(thread::spawn(move || loop {
                if cancel.load(Ordering::Relaxed) { break; }
                let next = {
                    let (lock, cv) = &*q;
                    let mut items = lock.lock().unwrap();
                    loop {
                        if let Some(item) = items.pop_front() { break Some(item); }
                        if pending.load(Ordering::Acquire) == 0 { break None; }
                        let (guard, _) = cv.wait_timeout(items, std::time::Duration::from_millis(20)).unwrap();
                        items = guard;
                        if cancel.load(Ordering::Relaxed) { break None; }
                    }
                };
                let Some((path, root)) = next else { break; };
                let (dir_tx, dir_rx) = mpsc::channel();
                scan_dir(path, root.clone(), exts.clone(), excludes.clone(), dir_tx, out.clone(), cancel.clone(), errors.clone());
                for child in dir_rx {
                    pending.fetch_add(1, Ordering::AcqRel);
                    q.0.lock().unwrap().push_back((child, root.clone()));
                    q.1.notify_one();
                }
                if pending.fetch_sub(1, Ordering::AcqRel) == 1 { q.1.notify_all(); }
            }));
        }
        drop(out_tx);
        while let Ok(item) = out_rx.recv() { if emit(&item).is_err() { stdin_cancel.store(true, Ordering::Relaxed); break; } }
        for j in joins { let _ = j.join(); }
        if scan_errors.load(Ordering::Relaxed) { return Err("one or more scan directories could not be read".into()); }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn same_python_hidden_and_temporary_name_rules() {
        assert!(excluded_name("#ieeee-bad.pdf")); assert!(!excluded_name("~$draft.pdf"));
    }
    #[test] fn default_extensions_are_document_only() { assert!(DEFAULT_EXTS.contains(&"pdf")); assert!(!DEFAULT_EXTS.contains(&"jpg")); }
    #[test] fn defaults_match_library_scope_reserved_directories() { for n in ["windows", "program files", "program files (x86)", "programdata", "$recycle.bin", "system volume information", "appdata", "node_modules", ".git", ".svn", ".hg", "venv", ".venv"] { assert!(EXCLUDED_DIRS.contains(&n)); } assert!(!EXCLUDED_DIRS.contains(&"target")); }
    #[test] fn custom_exclusion_is_case_insensitive_and_component_bounded() {
        let root = PathBuf::from(r"C:\root");
        let excluded = vec![PathBuf::from(r"C:\root\Skip")];
        assert!(excluded_path(&PathBuf::from(r"C:\root\skip\child.pdf"), &root, &excluded));
        assert!(!excluded_path(&PathBuf::from(r"C:\root\Skipped\child.pdf"), &root, &excluded));
    }
}
