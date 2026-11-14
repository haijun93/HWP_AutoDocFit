//! 수명 명령(`--install`·`--disable`·`--uninstall`·`--quit`·`--status`·`--version`) 실행.
//! 결과는 표준 출력에 JSON 한 줄(앱 `docfit_app/ktm_client.py`가 읽음). 창·트레이·색인을 열지 않는다.
use std::path::Path;
use std::ptr::null;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HWND, WAIT_OBJECT_0},
    System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    UI::WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId, PostMessageW},
};

/// 떠 있는 몽땅검색에게 "완전히 끝내기"를 부탁하는 메시지(숨기기와 다름).
pub const QUIT: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 23;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// 이 사용자 세션에서 떠 있는 몽땅검색 창과 그 프로세스 번호.
pub fn running() -> Option<(HWND, u32)> {
    unsafe {
        let h = FindWindowW(wide(super::window::CLASS).as_ptr(), null());
        if h.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, &mut pid);
        Some((h, pid))
    }
}

/// 떠 있으면 끝내고 프로세스가 사라질 때까지(최대 5초) 기다린다. exe 교체 전에 꼭 부른다.
fn quit() -> Result<(), String> {
    let Some((h, pid)) = running() else {
        return Ok(());
    };
    unsafe {
        let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        PostMessageW(h, QUIT, 0, 0);
        if process.is_null() {
            // 기다릴 손잡이가 없으면 창이 사라지는 것으로 판단한다.
            for _ in 0..250 {
                if running().is_none() {
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            return Err("몽땅검색을 끄지 못했습니다(5초 초과)".into());
        }
        let waited = WaitForSingleObject(process, 5000);
        CloseHandle(process);
        if waited == WAIT_OBJECT_0 {
            Ok(())
        } else {
            Err("몽땅검색을 끄지 못했습니다(5초 초과)".into())
        }
    }
}

/// 설치본을 숨긴 채(단축키만 등록) 따로 시작한다.
fn start(destination: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    let mut command = std::process::Command::new(destination);
    command
        .arg("--hidden")
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // 시험(KTM_TEST_ROOT)에서는 같은 격리 폴더를 쓰게 그대로 물려준다.
    // 상주할 자식이 이 명령의 출력 파이프를 물려받으면 출력을 읽는 앱이 EOF를 영원히 기다린다.
    // CreateProcess가 상속 가능한 손잡이를 모두 넘기므로 우리 표준 손잡이의 상속 표시를 먼저 끈다.
    unsafe {
        use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};
        use windows_sys::Win32::System::Console::{
            GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        };
        for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            let handle = GetStdHandle(which);
            if !handle.is_null() && handle as isize != -1 {
                SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0);
            }
        }
    }
    command.spawn().map_err(|e| format!("몽땅검색 시작 실패: {e}"))?;
    for _ in 0..150 {
        if running().is_some() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Err("몽땅검색이 3초 안에 뜨지 않았습니다".into())
}

pub fn status(data: &Path, destination: &Path) -> serde_json::Value {
    let pid = running().map(|r| r.1);
    let file = if pid.is_some() {
        std::fs::read(data.join("status.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            // 지난 실행이 남긴 파일(다른 pid)은 지금 상태가 아니다.
            .filter(|v| v.get("pid").and_then(|p| p.as_u64()) == pid.map(u64::from))
            .unwrap_or(serde_json::Value::Null)
    } else {
        serde_json::Value::Null
    };
    ktm::install::report(pid, destination, ktm::install::autostart_command().as_deref(), file)
}

/// 수명 명령이면 실행하고 JSON 한 줄을 찍은 뒤 true. 일반 상주 실행이면 false.
pub fn run(args: &[String], data: &Path, source: &Path, destination: &Path) -> bool {
    let has = |name: &str| args.iter().any(|a| a == name);
    let output = if has("--version") {
        serde_json::json!({"ok": true, "name": "몽땅검색", "version": env!("CARGO_PKG_VERSION")})
    } else if has("--status") {
        status(data, destination)
    } else if let Some(actions) = ktm::install::plan(args) {
        let mut done = Vec::new();
        let mut failure = None;
        for action in actions {
            let result = match action {
                "quit" => quit(),
                "start" => start(destination),
                other => ktm::install::execute(other, source, destination),
            };
            match result {
                Ok(()) => done.push(action),
                Err(e) => {
                    failure = Some(format!("{action}: {e}"));
                    break;
                }
            }
        }
        let mut value = status(data, destination);
        value["actions"] = serde_json::json!(done);
        if let Some(error) = failure {
            value["ok"] = false.into();
            value["error"] = error.into();
        }
        value
    } else {
        return false;
    };
    println!("{output}");
    if output["ok"] != true {
        std::process::exit(1);
    }
    true
}
