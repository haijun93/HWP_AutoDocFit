//! kvf 설치·교체 효과. 전이 순서는 공용 lifecycle::update를 그대로 사용한다.
//! NotInstalled → Installed → Resident; 종료 실패는 교체 금지, 시작 실패는 rollback.
use crate::{lifecycle, util::{self,wide}};
use std::path::Path;
use windows_sys::Win32::{Foundation::*,System::Threading::*,UI::WindowsAndMessaging::*};
const RUN_KEY:&str=r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_NAME:&str="HwpAutoDocFit.Kvf";
fn autostart_command(target:&Path)->String{format!("\"{}\" --hidden --delay 30",target.display())}
fn verified(source:&Path)->Result<Vec<u8>,String>{
 let expected=std::fs::read_to_string(source.with_file_name(format!("{}.sha256",source.file_name().ok_or("source name")?.to_string_lossy()))).map_err(|_|"integrity: missing sidecar")?;
 let expected=expected.trim().to_ascii_lowercase();
 let data=std::fs::read(source).map_err(|e|e.to_string())?;
 if expected.len()!=64 || !expected.bytes().all(|c|c.is_ascii_hexdigit()) || sha256_bytes(&data).as_deref()!=Some(&expected){return Err("integrity: hash mismatch".into())}Ok(data)
}
#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegOpenKeyExW(key: isize, sub: *const u16, options: u32, sam: u32, out: *mut isize) -> u32;
    fn RegQueryValueExW(key: isize, name: *const u16, reserved: *mut u32, kind: *mut u32, data: *mut u8, len: *mut u32) -> u32;
    fn RegSetValueExW(key: isize, name: *const u16, reserved: u32, kind: u32, data: *const u8, len: u32) -> u32;
    fn RegDeleteValueW(key: isize, name: *const u16) -> u32;
    fn RegCloseKey(key: isize) -> u32;
}
const HKEY_CURRENT_USER: isize = 0x8000_0001u32 as i32 as isize;
const KEY_QUERY_VALUE: u32 = 0x0001;
const KEY_SET_VALUE: u32 = 0x0002;
const REG_SZ: u32 = 1;

fn open_run_key(sam: u32) -> Option<isize> {
    let mut key = 0isize;
    let r = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), 0, sam, &mut key) };
    (r == 0).then_some(key)
}

/// 자동 실행 등록 여부. 레지스트리를 읽지 못하면 None.
pub fn autostart_enabled() -> Option<bool> {
    if std::env::var_os("KVF_TEST_ROOT").is_some() { return None; }
    let key = open_run_key(KEY_QUERY_VALUE)?;
    let name = wide(RUN_NAME);
    let mut buf = vec![0u16; 2048];
    let mut len = (buf.len() * 2) as u32;
    let mut kind = 0u32;
    let r = unsafe { RegQueryValueExW(key, name.as_ptr(), std::ptr::null_mut(), &mut kind, buf.as_mut_ptr() as *mut u8, &mut len) };
    unsafe { RegCloseKey(key) };
    match r {
        0 => {
            let n = (len as usize / 2).min(buf.len());
            let value = String::from_utf16_lossy(&buf[..n]);
            Some(kind == REG_SZ && value.trim_end_matches('\0') == autostart_command(&crate::paths::data_dir().join("kvf.exe")))
        }
        2 => Some(false), // ERROR_FILE_NOT_FOUND: 등록 없음
        _ => None,
    }
}

/// 설치된 kvf 사본을 30초 늦게 시작하도록 등록한다.
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    if std::env::var_os("KVF_TEST_ROOT").is_some() {
        return Err("시험 실행에서는 자동 실행 등록을 바꾸지 않습니다.".into());
    }
    let key = open_run_key(KEY_SET_VALUE | KEY_QUERY_VALUE).ok_or("자동 실행 등록 위치를 열지 못했습니다.")?;
    let name = wide(RUN_NAME);
    let r = if enabled {
        let value = wide(&autostart_command(&crate::paths::data_dir().join("kvf.exe")));
        unsafe { RegSetValueExW(key, name.as_ptr(), 0, REG_SZ, value.as_ptr() as *const u8, (value.len() * 2) as u32) }
    } else {
        match unsafe { RegDeleteValueW(key, name.as_ptr()) } { 2 => 0, r => r }
    };
    unsafe { RegCloseKey(key) };
    if r == 0 { Ok(()) } else { Err(format!("자동 실행 설정을 바꾸지 못했습니다(Windows 오류 {r}).")) }
}

struct LifecycleLock(windows_sys::Win32::Foundation::HANDLE);
impl LifecycleLock {
    fn acquire(root: &std::path::Path) -> Result<Self, String> {
        use windows_sys::Win32::System::Threading::{CreateMutexW, WaitForSingleObject};
        let hash = root.to_string_lossy().encode_utf16().fold(0xcbf29ce484222325u64,
            |h, c| (h ^ c as u64).wrapping_mul(0x100000001b3));
        let name = wide(&format!("Local\\HwpAutoDocFit.kvf.update.{hash:016x}"));
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if handle.is_null() { return Err("update mutex".into()); }
        let result = unsafe { WaitForSingleObject(handle, 15000) };
        if result != 0 && result != 0x80 {
            unsafe { CloseHandle(handle); }
            return Err("update busy".into());
        }
        Ok(Self(handle))
    }
}
impl Drop for LifecycleLock {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::System::Threading::ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}

fn lifecycle_status(root: &std::path::Path) -> Option<crate::json::Value> {
    std::fs::read_to_string(root.join("status.json")).ok().and_then(|s| crate::json::parse(&s))
}
fn resident_pid()->Option<u32>{unsafe{
 let root=std::env::var("KVF_TEST_ROOT").ok();
 let class=wide(&crate::paths::object_name("HwpAutoDocFit.KvfMain",root.as_deref()));
 let hwnd=FindWindowW(class.as_ptr(),std::ptr::null());
 if hwnd.is_null(){return None}let mut pid=0;GetWindowThreadProcessId(hwnd,&mut pid);(pid!=0).then_some(pid)
}}
fn lifecycle_alive(status: &crate::json::Value) -> bool {
    use windows_sys::Win32::System::Threading::{OpenProcess, GetExitCodeProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    let Some(pid) = status.num("pid").filter(|n| *n > 0.0 && *n <= u32::MAX as f64) else { return false; };
    if resident_pid()!=Some(pid as u32){return false;}
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32);
        if handle.is_null() { return GetLastError() == 5; } // 접근 거부는 실행 중으로 취급한다.
        let mut code = 0;
        let queried = GetExitCodeProcess(handle, &mut code);
        CloseHandle(handle);
        queried == 0 || code == 259
    }
}
fn lifecycle_error(root: &std::path::Path, error: &str) -> crate::json::Value {
    use std::io::Write;
    eprintln!("kvf update: {error}");
    if let Ok(mut log) = std::fs::OpenOptions::new().create(true).append(true).open(root.join("upgrade.log")) {
        let _ = writeln!(log, "{} {error}", util::now());
    }
    crate::json::Value::obj(vec![("ok", false.into()), ("upgraded", false.into()), ("reason", error.into())])
}

struct NativeLifecycle<'a> {
    root: &'a std::path::Path,
    target: &'a std::path::Path,
    current: &'a [u8],
    old: Option<&'a [u8]>,
    pending: Option<StartedProcess>,
}
struct StartedProcess { handle: HANDLE, pid: u32 }
impl Drop for StartedProcess { fn drop(&mut self) { unsafe { CloseHandle(self.handle); } } }
impl StartedProcess {
    fn finished(&self) -> bool { unsafe { WaitForSingleObject(self.handle, 0) == 0 } }
}
fn spawn_hidden(target: &Path) -> Result<StartedProcess, String> {
    // 출력 파이프를 포함한 부모 핸들을 상주 프로세스에 물려주지 않는다.
    // JSON 호출자가 상주 프로세스 종료까지 EOF를 기다리는 일을 방지한다.
    let application = wide(&target.to_string_lossy());
    let mut command = wide(&format!("\"{}\" --hidden", target.display()));
    unsafe {
        let mut startup: STARTUPINFOW = std::mem::zeroed();
        startup.cb = std::mem::size_of_val(&startup) as u32;
        startup.dwFlags = STARTF_USESHOWWINDOW | STARTF_USESTDHANDLES;
        startup.wShowWindow = SW_HIDE as u16;
        let mut process: PROCESS_INFORMATION = std::mem::zeroed();
        if CreateProcessW(application.as_ptr(), command.as_mut_ptr(), std::ptr::null(),
            std::ptr::null(), 0, CREATE_NO_WINDOW, std::ptr::null(), std::ptr::null(),
            &startup, &mut process) == 0 { return Err(format!("spawn: {}", GetLastError())); }
        CloseHandle(process.hThread);
        Ok(StartedProcess { handle: process.hProcess, pid: process.dwProcessId })
    }
}
fn atomic_agent_write(target: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    let temp = target.with_extension(format!("{}.new", std::process::id()));
    let result = (|| {
        std::fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
        use std::io::Write;
        let mut file=std::fs::File::create(&temp).map_err(|e|e.to_string())?;
        file.write_all(bytes).and_then(|_|file.sync_all()).map_err(|e|e.to_string())?;
        drop(file);
        for attempt in 0..21 {
            match std::fs::rename(&temp, target) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied && attempt < 20 =>
                    std::thread::sleep(std::time::Duration::from_millis(100)),
                Err(error) => return Err(error.to_string()),
            }
        }
        unreachable!()
    })();
    let _ = std::fs::remove_file(temp);
    result
}
impl lifecycle::Runtime for NativeLifecycle<'_> {
    fn quit_and_wait(&mut self) -> Result<(), String> {
        let status = lifecycle_status(self.root).filter(lifecycle_alive);
        if status.is_none() {
            if resident_pid().is_some(){return Err("kvf status unavailable; refusing replacement".into())}
            if let Some(child) = self.pending.as_mut() {
                if !child.finished() {
                    return Err("new agent has no status; previous binary kept in kvf.previous".into());
                }
            }
            return Ok(());
        }
        let status = status.unwrap();
        let root=std::env::var("KVF_TEST_ROOT").ok();
        let class=wide(&crate::paths::object_name("HwpAutoDocFit.KvfMain",root.as_deref()));
        unsafe {
            let hwnd=FindWindowW(class.as_ptr(),std::ptr::null());
            let mut pid=0; GetWindowThreadProcessId(hwnd,&mut pid);
            if hwnd.is_null() || status.num("pid")!=Some(pid as f64){return Err("invalid kvf identity".into())}
            let process=OpenProcess(0x00100000,0,pid);
            if process.is_null(){return Err("cannot wait for kvf".into())}
            let posted=PostMessageW(hwnd,WM_CLOSE,0,0);
            let result=if posted!=0{WaitForSingleObject(process,5000)}else{u32::MAX};
            CloseHandle(process);
            if result==0{return Ok(())}
        }
        Err("still_running".into())
    }
    fn install(&mut self) -> Result<(), String> {
        // 종료·복구가 불가능한 경우에도 이전 실행 파일을 별도 보존한다.
        if let Some(old) = self.old { atomic_agent_write(&self.root.join("kvf.previous"), old)?; }
        atomic_agent_write(self.target, self.current)
    }
    fn start_and_wait(&mut self) -> Result<(), String> {
        let child = spawn_hidden(self.target)?;
        for _ in 0..150 {
            if let Some(status) = lifecycle_status(self.root) {
                if status.num("pid") == Some(child.pid as f64) && lifecycle_alive(&status) { return Ok(()); }
            }
            if child.finished() { return Err("kvf exited before ready".into()); }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        self.pending = Some(child);
        Err("agent start timeout".into())
    }
    fn rollback(&mut self) -> Result<(), String> {
        if let Some(old) = self.old { atomic_agent_write(self.target, old) }
        else { std::fs::remove_file(self.target).map_err(|e| e.to_string()) }
    }
}

pub fn lifecycle_command(command: &str, args: &[String]) -> crate::json::Value {
    use crate::json::Value;
    let root = crate::paths::data_dir();
    let _guard = match LifecycleLock::acquire(&root) {
        Ok(guard) => guard,
        Err(error) => return lifecycle_error(&root, &error),
    };
    let target = root.join("kvf.exe");
    let source = std::env::current_exe().ok();
    let bundled = source.as_deref().filter(|p| p.is_file());
    match command {
        "--status" => {
            let status = std::fs::read_to_string(root.join("status.json")).ok()
                .and_then(|s| crate::json::parse(&s)).unwrap_or_else(|| Value::obj(vec![]));
            let running = lifecycle_alive(&status);

            return Value::obj(vec![("ok", true.into()), ("available", bundled.is_some().into()),
                ("running", running.into()), ("status", status), ("autostart", autostart_enabled().unwrap_or(false).into())]);
        }
        "--install" | "--enable" | "--upgrade-if-stale" => {
            let Some(source) = bundled else { return Value::obj(vec![("ok", false.into()), ("reason", "no_bundle".into())]); };
            if command=="--upgrade-if-stale" && !target.exists() && !lifecycle_status(&root).as_ref().is_some_and(lifecycle_alive){return Value::obj(vec![("ok",true.into()),("upgraded",false.into()),("restarted",false.into()),("reason","not_installed".into())])}
            if args.iter().any(|a|a=="--autostart") && std::env::var_os("KVF_TEST_ROOT").is_some(){return lifecycle_error(&root,"시험 실행에서는 자동 실행 등록을 바꾸지 않습니다.")}
            let current=match verified(source){Ok(b)=>b,Err(e)=>return lifecycle_error(&root,&e)};
            let old = match std::fs::read(&target) {
                Ok(bytes) => Some(bytes),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return lifecycle_error(&root, &error.to_string()),
            };
            let status = lifecycle_status(&root);
            let was_running = status.as_ref().is_some_and(lifecycle_alive);
            if command == "--upgrade-if-stale" && old.is_none() && !was_running {
                return Value::obj(vec![("ok", true.into()), ("upgraded", false.into()), ("reason", "not_installed".into())]);
            }
            // 디스크 파일이 이미 새것이어도 실행 중인 프로세스는 구버전일 수 있다.
            // 같은 버전으로 다시 빌드한 기존 배포판도 파일 차이로 감지한다.
            let stale = lifecycle::stale(old.as_deref() != Some(current.as_slice()), was_running,
                status.as_ref().and_then(|s| s.str("version")), env!("CARGO_PKG_VERSION"));
            let mut runtime = NativeLifecycle { root: &root, target: &target, current: &current, old: old.as_deref(), pending: None };
            match lifecycle::update(&mut runtime, was_running, stale, command == "--enable") {
                Ok((upgraded, restarted)) => {
                    let _ = std::fs::remove_file(root.join("kvf.previous"));
                    if command == "--enable" {
                        if std::env::var_os("KVF_TEST_ROOT").is_none() { if let Err(error) = set_autostart(args.iter().any(|a| a == "--autostart")) {
                            return lifecycle_error(&root, &error);
                        }}
                    }
                    Value::obj(vec![("ok", true.into()), ("upgraded", upgraded.into()),
                        ("restarted", restarted.into()), ("installed", target.to_string_lossy().into_owned().into()),
                        ("version", env!("CARGO_PKG_VERSION").into())])
                }
                Err(error) => lifecycle_error(&root, &error),
            }
        }

        "--disable" => {
            let mut rt=NativeLifecycle{root:&root,target:&target,current:&[],old:None,pending:None};
            if let Err(e)=lifecycle::Runtime::quit_and_wait(&mut rt){return lifecycle_error(&root,&e)}
            if std::env::var_os("KVF_TEST_ROOT").is_none(){if let Err(e)=set_autostart(false){return lifecycle_error(&root,&e)}}
            Value::obj(vec![("ok",true.into()),("running",false.into()),("autostart",false.into())])
        }
        _ => return Value::obj(vec![("ok", false.into())]),
    }
}

fn sha256_file(path: &std::path::Path) -> Option<String> {
    sha256_bytes(&std::fs::read(path).ok()?)
}
fn sha256_bytes(data:&[u8])->Option<String>{
    let mut h = [0x6a09e667u32,0xbb67ae85,0x3c6ef372,0xa54ff53a,0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19];
    const K:[u32;64]=[0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2];
    let mut msg=data.to_vec(); let bits=(msg.len() as u64)*8; msg.push(0x80); while msg.len()%64!=56 {msg.push(0)} msg.extend_from_slice(&bits.to_be_bytes());
    for c in msg.chunks_exact(64) { let mut w=[0u32;64]; for i in 0..16 {w[i]=u32::from_be_bytes(c[i*4..i*4+4].try_into().ok()?)} for i in 16..64 {let a=w[i-15];let b=w[i-2];w[i]=w[i-16].wrapping_add(a.rotate_right(7)^a.rotate_right(18)^(a>>3)).wrapping_add(w[i-7]).wrapping_add(b.rotate_right(17)^b.rotate_right(19)^(b>>10));} let(mut a,mut b,mut c,mut d,mut e,mut f,mut g,mut z)=(h[0],h[1],h[2],h[3],h[4],h[5],h[6],h[7]); for i in 0..64 {let t1=z.wrapping_add(e.rotate_right(6)^e.rotate_right(11)^e.rotate_right(25)).wrapping_add((e&f)^(!e&g)).wrapping_add(K[i]).wrapping_add(w[i]);let t2=(a.rotate_right(2)^a.rotate_right(13)^a.rotate_right(22)).wrapping_add((a&b)^(a&c)^(b&c));z=g;g=f;f=e;e=d.wrapping_add(t1);d=c;c=b;b=a;a=t1.wrapping_add(t2);} for(i,v)in[a,b,c,d,e,f,g,z].iter().enumerate(){h[i]=h[i].wrapping_add(*v)} }
    Some(h.iter().map(|v|format!("{v:08x}")).collect())
}

#[cfg(test)] mod tests {
 use super::*;
 use crate::json::Value;
 fn fixture(name:&str)->std::path::PathBuf{
  let root=std::path::PathBuf::from("E:/docfit-tmp").join(format!("vfv8-{name}-{}",std::process::id()));
  std::fs::create_dir_all(&root).unwrap();root
 }
 #[test]fn run_value_targets_installed_copy(){assert_eq!(autostart_command(Path::new(r"E:\한글 폴더\kvf.exe")),"\"E:\\한글 폴더\\kvf.exe\" --hidden --delay 30");assert_ne!(RUN_NAME,"HwpAutoDocFit.KAgent");}
 #[test]fn sha_standard_vectors(){assert_eq!(sha256_bytes(b"abc").unwrap(),"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");assert_eq!(sha256_bytes(b"").unwrap(),"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");}
 #[test]fn rejects_missing_malformed_and_mismatched_hash(){let root=fixture("hash");let src=root.join("kvf.exe");std::fs::write(&src,b"abc").unwrap();assert!(verified(&src).is_err());for h in ["bad".to_string(),"0".repeat(64)]{std::fs::write(root.join("kvf.exe.sha256"),h).unwrap();assert!(verified(&src).is_err());}std::fs::write(root.join("kvf.exe.sha256"),sha256_bytes(b"abc").unwrap().to_uppercase()).unwrap();assert_eq!(verified(&src).unwrap(),b"abc");std::fs::remove_dir_all(root).unwrap();}
 #[test]fn atomic_replace_preserves_other_files(){let root=fixture("atomic");let target=root.join("kvf.exe");std::fs::write(root.join("settings.json"),b"settings").unwrap();atomic_agent_write(&target,b"old").unwrap();atomic_agent_write(&target,b"new").unwrap();assert_eq!(std::fs::read(&target).unwrap(),b"new");assert_eq!(std::fs::read(root.join("settings.json")).unwrap(),b"settings");assert_eq!(std::fs::read_dir(&root).unwrap().count(),2);std::fs::remove_dir_all(root).unwrap();}
 #[test]fn locked_replacement_keeps_old_and_backup(){use std::os::windows::fs::OpenOptionsExt;let root=fixture("locked");let target=root.join("kvf.exe");std::fs::write(&target,b"old").unwrap();let lock=std::fs::OpenOptions::new().read(true).share_mode(1).open(&target).unwrap();let mut rt=NativeLifecycle{root:&root,target:&target,current:b"new",old:Some(b"old"),pending:None};assert!(lifecycle::Runtime::install(&mut rt).is_err());assert_eq!(std::fs::read(&target).unwrap(),b"old");assert_eq!(std::fs::read(root.join("kvf.previous")).unwrap(),b"old");drop(lock);std::fs::remove_dir_all(root).unwrap();}
 #[test]fn native_rollback_restores_previous_bytes(){let root=fixture("rollback");let target=root.join("kvf.exe");std::fs::write(&target,b"old").unwrap();let mut rt=NativeLifecycle{root:&root,target:&target,current:b"new",old:Some(b"old"),pending:None};lifecycle::Runtime::install(&mut rt).unwrap();assert_eq!(std::fs::read(&target).unwrap(),b"new");lifecycle::Runtime::rollback(&mut rt).unwrap();assert_eq!(std::fs::read(&target).unwrap(),b"old");std::fs::remove_dir_all(root).unwrap();}
 #[test]fn test_root_blocks_registry_and_absent_upgrade_is_noop(){let _env=util::TestEnvironment::lock();let root=fixture("isolated");unsafe{std::env::set_var("KVF_TEST_ROOT",&root)}assert!(set_autostart(true).is_err());assert!(set_autostart(false).is_err());assert_eq!(autostart_enabled(),None);let v=lifecycle_command("--upgrade-if-stale",&[]);assert_eq!(v.str("reason"),Some("not_installed"));assert!(!root.join("kvf.exe").exists());assert_eq!(lifecycle_command("--disable",&[]).bool("ok"),Some(true));unsafe{std::env::remove_var("KVF_TEST_ROOT")}std::fs::remove_dir_all(root).unwrap();}
 #[test]fn status_does_not_trust_reused_pid(){let _env=util::TestEnvironment::lock();let root=fixture("pid");unsafe{std::env::set_var("KVF_TEST_ROOT",&root)}let v=Value::obj(vec![("pid",(std::process::id() as i64).into())]);assert!(!lifecycle_alive(&v));unsafe{std::env::remove_var("KVF_TEST_ROOT")}std::fs::remove_dir_all(root).unwrap();}
}
