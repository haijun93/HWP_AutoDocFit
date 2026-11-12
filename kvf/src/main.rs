#![cfg_attr(not(test), windows_subsystem = "windows")]
#![allow(dead_code)] // V5~V8에서 연결할 공용 함수도 같은 원본으로 컴파일한다.
#[path = "../../kagent/src/sqlite.rs"] mod sqlite;
#[path = "../../kagent/src/json.rs"] mod json;
#[path = "../../kagent/src/util.rs"] mod util;
#[path = "../../kagent/src/lifecycle.rs"] mod lifecycle;
#[path = "../../kagent/src/frame.rs"] mod frame;
#[path = "../../kagent/src/hotkey.rs"] mod hotkey;
#[path = "../../kagent/src/open_rules.rs"] mod open_rules;
mod paths;
mod proc_state;

use windows_sys::Win32::{Foundation::*, System::{LibraryLoader::GetModuleHandleW, Threading::CreateMutexW}, UI::WindowsAndMessaging::*};
use proc_state::{Command, Event, State, Effect};
use std::sync::Mutex;
const SHOW: u32 = WM_APP + 1;
const RELOAD: u32 = WM_APP + 2;
static RUNTIME: Mutex<(State, &'static str)> = Mutex::new((State::NotRunning,"hidden"));
fn status(view: &str) -> std::io::Result<()> {
    let dir = paths::data_dir();
    std::fs::create_dir_all(&dir)?;
    let body = format!("{{\"version\":\"{}\",\"pid\":{},\"hotkey\":\"Win+Shift+F\",\"hotkey_ok\":false,\"view\":\"{}\",\"box\":null,\"items\":0,\"error\":\"\"}}",env!("CARGO_PKG_VERSION"),std::process::id(),view);
    let path = dir.join("status.json");
    if std::fs::read_to_string(&path).ok().as_deref() != Some(&body) { std::fs::write(path,body)?; }
    Ok(())
}
fn dispatch(event: Event) -> bool {
    let mut runtime = RUNTIME.lock().unwrap();
    let (next,effects) = proc_state::step(runtime.0,event,Command::Show);
    runtime.0 = next;
    for effect in effects {
        match effect {
            Effect::SetView(v) => { runtime.1 = v; if status(v).is_err() { return false; } }
            Effect::Cleanup => { let _ = std::fs::remove_file(paths::data_dir().join("status.json")); }
            _ => {}
        }
    }
    true
}
unsafe extern "system" fn wndproc(hwnd: HWND,msg:u32,w:usize,l:isize)->isize {
    match msg {
        SHOW => { if !dispatch(Event::Show) { unsafe { DestroyWindow(hwnd); } } 0 }
        RELOAD => { dispatch(Event::Reload); 0 }
        WM_CLOSE => { dispatch(Event::Quit); unsafe { DestroyWindow(hwnd); } 0 }
        WM_QUERYENDSESSION => 1,
        WM_ENDSESSION if w != 0 => { dispatch(Event::EndSession); unsafe { DestroyWindow(hwnd); } 0 }
        WM_DESTROY => { dispatch(Event::Cleaned); unsafe { PostQuitMessage(0); } 0 }
        _ => unsafe { DefWindowProcW(hwnd,msg,w,l) }
    }
}
fn run() -> Result<(),String> {
    let args = proc_state::parse(&std::env::args().skip(1).collect::<Vec<_>>())?;
    if args.command == Command::Version { println!("{{\"version\":\"{}\"}}",env!("CARGO_PKG_VERSION")); return Ok(()); }
    if !matches!(args.command,Command::Show|Command::Hidden|Command::Quit|Command::Reload) {
        return Err("설치·상태 명령은 V8에서 연결합니다".into());
    }
    let root = std::env::var("KVF_TEST_ROOT").ok();
    let class = util::wide(&paths::object_name("HwpAutoDocFit.KvfMain",root.as_deref()));
    let name = util::wide(&paths::object_name("Local\\HwpAutoDocFit.kvf",root.as_deref()));
    dispatch(Event::Start);
    unsafe {
        let mutex = CreateMutexW(std::ptr::null(),0,name.as_ptr());
        if mutex.is_null() { dispatch(Event::Failed); return Err("뮤텍스 생성 실패".into()); }
        struct Handle(HANDLE);
        impl Drop for Handle { fn drop(&mut self) { unsafe { CloseHandle(self.0); } } }
        let _guard = Handle(mutex);
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let (_,effects) = proc_state::step(State::Starting,Event::Existing,args.command);
            let other = FindWindowW(class.as_ptr(),std::ptr::null());
            for effect in effects {
                if let Effect::Notify(c) = effect {
                    if !other.is_null() { PostMessageW(other,match c { Command::Quit=>WM_CLOSE,Command::Reload=>RELOAD,_=>SHOW },0,0); }
                }
            }
            return Ok(());
        }
        let (state,effects) = proc_state::step(State::Starting,Event::Ready,args.command);
        if state == State::NotRunning { return Ok(()); }
        if args.delay > 0 { std::thread::sleep(std::time::Duration::from_secs(args.delay)); }
        let instance = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSW { lpfnWndProc: Some(wndproc),hInstance:instance,lpszClassName:class.as_ptr(),..std::mem::zeroed() };
        if RegisterClassW(&wc) == 0 { dispatch(Event::Failed); return Err("창 클래스 등록 실패".into()); }
        let hwnd = CreateWindowExW(WS_EX_TOOLWINDOW,class.as_ptr(),util::wide("가상 폴더").as_ptr(),WS_POPUP,0,0,0,0,std::ptr::null_mut(),std::ptr::null_mut(),instance,std::ptr::null());
        if hwnd.is_null() { dispatch(Event::Failed); return Err("주 창 생성 실패".into()); }
        RUNTIME.lock().unwrap().0 = state;
        for effect in effects {
            if let Effect::SetView(v) = effect {
                RUNTIME.lock().unwrap().1 = v;
                if let Err(e) = status(v) { dispatch(Event::Quit); DestroyWindow(hwnd); return Err(e.to_string()); }
            }
        }
        let mut msg: MSG = std::mem::zeroed();
        loop {
            let result = GetMessageW(&mut msg,std::ptr::null_mut(),0,0);
            if result <= 0 {
                if result == -1 { dispatch(Event::Quit); DestroyWindow(hwnd); return Err("메시지 읽기 실패".into()); }
                break;
            }
            TranslateMessage(&msg); DispatchMessageW(&msg);
        }
    }
    Ok(())
}
fn main() { if let Err(e) = run() { eprintln!("{e}"); std::process::exit(1); } }
mod vf_data;
mod vf_state;
