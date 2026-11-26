//! `sidebar.exe` 의 Win32 경계 중 창 UI 밖의 것: 창·프로세스 목록, 전체 종료, 파일 보내기, 로그온 시작, 수신함 권한.
#![allow(unsafe_op_in_unsafe_fn)]

use dock::sidebar::{self, Env, Identity, Spawn};
use dock::{handoff, shutdown};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::ptr::null_mut;
use std::time::{Duration, Instant};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    System::{Console::*, Diagnostics::ToolHelp::*, Registry::*, Threading::*},
    UI::WindowsAndMessaging::*,
};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub unsafe fn window_text(f: unsafe extern "system" fn(HWND, *mut u16, i32) -> i32, hwnd: HWND) -> String {
    let mut buffer = [0u16; 256];
    let n = f(hwnd, buffer.as_mut_ptr(), buffer.len() as i32).max(0) as usize;
    String::from_utf16_lossy(&buffer[..n])
}

// ---- 창·프로세스 ----

pub struct Seen {
    pub hwnd: HWND,
    pub pid: u32,
    pub class: String,
    pub title: String,
    pub owned: bool,
}

unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> i32 {
    if IsWindowVisible(hwnd) != 0 {
        let list = &mut *(data as *mut Vec<Seen>);
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        list.push(Seen {
            hwnd,
            pid,
            class: window_text(GetClassNameW, hwnd),
            title: window_text(GetWindowTextW, hwnd),
            owned: !GetWindow(hwnd, GW_OWNER).is_null(),
        });
    }
    1
}

pub fn visible_windows() -> Vec<Seen> {
    let mut list: Vec<Seen> = Vec::new();
    unsafe {
        EnumWindows(Some(collect), &mut list as *mut _ as LPARAM);
    }
    list
}

pub fn matches(window: &Seen, id: u8) -> bool {
    sidebar::identities(id).iter().any(|identity| match identity {
        Identity::Class(c) => window.class == *c,
        Identity::Title(t) => &window.title == t,
    })
}

pub fn find(windows: &[Seen], id: u8) -> Option<HWND> {
    // 식별자 순서(Rust 창 먼저)를 지킨다.
    sidebar::identities(id).iter().find_map(|identity| {
        windows
            .iter()
            .find(|w| match identity {
                Identity::Class(c) => w.class == *c,
                Identity::Title(t) => &w.title == t,
            })
            .map(|w| w.hwnd)
    })
}

pub fn activate(hwnd: HWND) {
    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd);
    }
}

pub fn process_image(pid: u32) -> Option<PathBuf> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buffer = vec![0u16; 32768];
        let mut size = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut size);
        CloseHandle(process);
        (ok != 0).then(|| PathBuf::from(String::from_utf16_lossy(&buffer[..size as usize])))
    }
}

pub fn process_alive(pid: u32) -> bool {
    unsafe {
        let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if process.is_null() {
            return false;
        }
        let alive = WaitForSingleObject(process, 0) == WAIT_TIMEOUT;
        CloseHandle(process);
        alive
    }
}

/// 실행 파일 이름이 `name` 인 프로세스(대소문자 무시).
fn processes_named(name: &str) -> Vec<u32> {
    let mut found = Vec::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return found;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snapshot, &mut entry);
        while more != 0 {
            let end = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..end]).eq_ignore_ascii_case(name) {
                found.push(entry.th32ProcessID);
            }
            more = Process32NextW(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
    }
    found
}

/// 닫기 요청 한 번. 최소화한 창은 먼저 되돌린다(앱 화면은 보여야 확인 창에 답할 수 있고, 5초 안에 답하지 않으면 강제 종료한다).
pub fn ask_close(hwnd: HWND) {
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        AllowSetForegroundWindow(pid);
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        PostMessageW(hwnd, WM_CLOSE, 0, 0);
    }
}

pub fn closable(window: &Seen) -> shutdown::Window {
    shutdown::Window {
        handle: window.hwnd as isize,
        title: window.title.clone(),
        enabled: unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(window.hwnd) != 0 },
    }
}

/// `--close-all`: 설치 폴더 아래 프로그램의 창에 닫기를 보내고 끝나기를 기다린다. 강제 종료는 하지 않는다.
/// 창 없이 일하는 앱 프로세스(변환 엔진·상주 작업)도 끝날 때까지 기다린다.
pub fn close_all(install_dir: &Path, wait: Duration) -> (i32, String) {
    let own = std::process::id();
    let deadline = Instant::now() + wait;
    let mut asked = BTreeSet::new();
    let mut asked_pids = BTreeSet::new();
    let mut images: BTreeMap<u32, bool> = BTreeMap::new();
    loop {
        let mut inside = |pid: u32| {
            *images
                .entry(pid)
                .or_insert_with(|| process_image(pid).is_some_and(|p| shutdown::under(&p, install_dir)))
        };
        let windows: Vec<Seen> =
            visible_windows().into_iter().filter(|w| !w.owned && w.pid != own && inside(w.pid)).collect();
        let list: Vec<_> = windows.iter().map(closable).collect();
        for handle in shutdown::to_ask(&list, &mut asked) {
            if let Some(w) = windows.iter().find(|w| w.hwnd as isize == handle) {
                asked_pids.insert(w.pid);
                ask_close(w.hwnd);
            }
        }
        let mut busy: BTreeSet<u32> =
            processes_named(sidebar::APP_EXE).into_iter().filter(|p| *p != own && inside(*p)).collect();
        busy.extend(asked_pids.iter().copied().filter(|p| process_alive(*p)));
        if windows.is_empty() && busy.is_empty() {
            return (shutdown::EXIT_CLOSED, String::new());
        }
        if Instant::now() >= deadline {
            let mut lines: Vec<String> = windows
                .iter()
                .map(|w| if w.title.is_empty() { w.class.clone() } else { w.title.clone() })
                .collect();
            let hidden = busy.iter().filter(|p| !windows.iter().any(|w| w.pid == **p)).count();
            if hidden > 0 {
                lines.push(format!("창 없이 작업 중이거나 끝나는 중인 프로그램 {hidden}개"));
            }
            // 확인 창이 다른 창 뒤에 있을 수 있어 앞으로 가져온다.
            if let Some(w) = windows.first() {
                activate(w.hwnd);
            }
            return (shutdown::EXIT_REMAINING, lines.join("\r\n"));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

// ---- 창 열기 ----

/// 메뉴 창을 실행한다. 자식(문서뷰·사진뷰)이 `sidebar.exe --send` 를 같은 앱 위치로 부를 수 있게 환경변수를 준다.
pub fn start(spawn: &Spawn, env: &Env) -> std::io::Result<Child> {
    let mut command = Command::new(&spawn.program);
    // 띄운 창이 부른 쪽의 출력 통로를 물려받으면, 부른 쪽(파이프로 결과를 받는 앱)이 그 창이 닫힐 때까지 기다린다.
    keep_standard_handles_private();
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    command.args(&spawn.args).envs(spawn.envs.iter().map(|(k, v)| (*k, v)));
    if let Ok(own) = std::env::current_exe() {
        command.env("DOCFIT_SIDEBAR_EXE", own);
    }
    command.env("DOCFIT_SIDEBAR_RESOURCES", &env.resources);
    if let Some((python, root)) = &env.source {
        command.env("DOCFIT_SIDEBAR_PYTHON", python).env("DOCFIT_SIDEBAR_SOURCE", root);
    }
    if let Some(app) = &env.app_override {
        command.env("DOCFIT_SIDEBAR_APP", app);
    }
    if let Some(cwd) = &spawn.cwd {
        command.current_dir(cwd);
    }
    command.spawn()
}

fn keep_standard_handles_private() {
    unsafe {
        for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            let handle = GetStdHandle(which);
            if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
                SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0);
            }
        }
    }
}

// ---- 파일 보내기 ----

/// 현재 사용자와 SYSTEM 만 열 수 있게 한다(상속 차단). 다른 계정은 수신함에 요청을 넣을 수 없다.
pub fn restrict_to_current_user(folder: &Path) -> Result<(), String> {
    let failed = |step: &str| format!("수신함 권한을 설정하지 못했습니다({step}, 오류 {})", unsafe { GetLastError() });
    unsafe {
        let mut token = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(failed("token"));
        }
        let mut buffer = [0u64; 64];
        let mut size = 0;
        let ok = GetTokenInformation(token, TokenUser, buffer.as_mut_ptr().cast(), std::mem::size_of_val(&buffer) as u32, &mut size);
        CloseHandle(token);
        if ok == 0 {
            return Err(failed("user"));
        }
        let user = &*(buffer.as_ptr() as *const TOKEN_USER);
        let mut text: *mut u16 = null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {
            return Err(failed("sid"));
        }
        let length = (0..).take_while(|i| *text.add(*i) != 0).count();
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, length));
        LocalFree(text.cast());
        let sddl = wide(&format!("D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)"));
        let mut descriptor = null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut descriptor, null_mut()) == 0 {
            return Err(failed("sddl"));
        }
        let ok = SetFileSecurityW(
            wide(&folder.to_string_lossy()).as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        );
        LocalFree(descriptor);
        if ok == 0 { Err(failed("apply")) } else { Ok(()) }
    }
}

/// 파일(또는 붙임사진 편집 열기 요청)을 `menu` 창에 보낸다. 창이 없으면 연다. 원본은 읽지도 바꾸지도 않는다.
pub fn send(env: &Env, menu: u8, paths: &[String], photo_paste: bool) -> Result<(), String> {
    handoff::check(menu, paths, photo_paste, &handoff::is_plain_file)?;
    let windows = visible_windows();
    let title = sidebar::screen_title(menu);
    let screen = windows.iter().find(|w| w.title == title).map(|w| w.hwnd);
    let is_file = |p: &Path| p.is_file();
    let convert = env.resources.join("convert_win.exe").is_file();
    if handoff::route(menu, paths, convert, screen.is_some()) == handoff::Route::ConvertArguments {
        let mut spawn = sidebar::spawn_for(menu, env, &is_file)?;
        spawn.args.extend(paths.iter().map(Into::into));
        let child = start(&spawn, env).map_err(|e| format!("파일 변환 창을 열지 못했습니다: {e}"))?;
        unsafe { AllowSetForegroundWindow(child.id()) };
        // 이미 떠 있던 창은 자기가 받아 앞으로 나온다.
        return Ok(());
    }
    // 받을 창을 열 수 없으면 요청을 남기지 않는다.
    let spawn = if screen.is_none() { Some(sidebar::spawn_app_screen(menu, env, &is_file)?) } else { None };
    let root = handoff::mailbox_root();
    std::fs::create_dir_all(&root).map_err(|e| format!("수신함을 만들지 못했습니다: {e}"))?;
    restrict_to_current_user(&root)?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let request = handoff::write_request(&handoff::mailbox(&root, menu), paths, photo_paste, &format!("{stamp}-{}", std::process::id()))
        .map_err(|e| format!("보낼 요청을 저장하지 못했습니다: {e}"))?;
    match (screen, spawn) {
        (Some(window), _) => activate(window),
        (None, Some(spawn)) => match start(&spawn, env) {
            Ok(child) => unsafe {
                AllowSetForegroundWindow(child.id());
            },
            Err(e) => {
                let _ = std::fs::remove_file(request);
                return Err(format!("{} 창을 열지 못했습니다: {e}", sidebar::label(menu)));
            }
        },
        (None, None) => {}
    }
    Ok(())
}

// ---- 로그온 때 시작 ----

/// 기본 꺼짐. 상태의 원본은 레지스트리 값 하나다(설정 파일에 따로 두지 않는다).
pub const RUN_VALUE: &str = "HwpAutoDocFitSidebar";

fn run_key() -> Vec<u16> {
    // 시험은 임시 키를 쓴다.
    wide(&std::env::var("DOCFIT_SIDEBAR_RUN_KEY").unwrap_or_else(|_| r"Software\Microsoft\Windows\CurrentVersion\Run".into()))
}

pub fn autostart_enabled() -> bool {
    unsafe {
        let mut size = 0;
        RegGetValueW(HKEY_CURRENT_USER, run_key().as_ptr(), wide(RUN_VALUE).as_ptr(), RRF_RT_REG_SZ, null_mut(), null_mut(), &mut size)
            == ERROR_SUCCESS
    }
}

pub fn set_autostart(command: Option<&str>) -> Result<(), String> {
    unsafe {
        let status = match command {
            Some(command) => {
                let value = wide(command);
                RegSetKeyValueW(HKEY_CURRENT_USER, run_key().as_ptr(), wide(RUN_VALUE).as_ptr(), REG_SZ, value.as_ptr().cast(),
                                (value.len() * 2) as u32)
            }
            None => match RegDeleteKeyValueW(HKEY_CURRENT_USER, run_key().as_ptr(), wide(RUN_VALUE).as_ptr()) {
                ERROR_FILE_NOT_FOUND => ERROR_SUCCESS,
                other => other,
            },
        };
        if status == ERROR_SUCCESS { Ok(()) } else { Err(format!("로그온 시작 설정을 바꾸지 못했습니다(오류 {status})")) }
    }
}

/// 로그온 때 실행할 명령줄. 지금 사이드바가 기본값이 아닌 위치를 받았으면 그대로 옮긴다.
pub fn autostart_command(env: &Env, default_settings: &Path) -> Result<String, String> {
    let own = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut command = format!("\"{}\" --logon", own.display());
    if env.settings != default_settings {
        command.push_str(&format!(" --settings \"{}\"", env.settings.display()));
    }
    if let Some(app) = &env.app_override {
        command.push_str(&format!(" --app \"{}\"", app.display()));
    }
    if let Some((python, root)) = &env.source {
        command.push_str(&format!(" --python \"{}\" --source-root \"{}\"", python.display(), root.display()));
    }
    Ok(command)
}

/// 상주 작업을 맡는 화면 없는 앱 프로세스(`--resident`). 설치본에서만: 개발 실행이 이 PC 의 도우미를 갱신하지 않게.
pub fn start_resident(env: &Env) -> Option<Child> {
    match env.app(&|p| p.is_file())? {
        sidebar::AppLocation::Installed(exe) => {
            let mut command = Command::new(&exe);
            command.args(["--resident", "--parent", &std::process::id().to_string()]);
            // 화면 없는 작업이라 설치본의 시작 그림을 띄우지 않는다.
            command.env("PYINSTALLER_SUPPRESS_SPLASH_SCREEN", "1");
            if let Some(folder) = exe.parent() {
                command.current_dir(folder);
            }
            command.spawn().ok()
        }
        sidebar::AppLocation::Source { .. } => None,
    }
}
