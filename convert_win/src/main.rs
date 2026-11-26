#![windows_subsystem = "windows"]
mod logic;
mod task;
#[cfg(windows)]
#[path = "win.rs"]
mod win;
#[cfg(windows)]
fn main() {
    let mut args = std::env::args().skip(1);
    let mut paths=Vec::new();
    let mut capture = None;
    let mut capture_state = None;
    while let Some(a) = args.next() {
        if a == "--capture" {
            capture = args.next();
        } else if a == "--capture-state" {
            capture = args.next(); capture_state = args.next();
        } else { paths.push(a); }
    }
    let result = if let Some(path) = capture {
        if let Some(mode)=capture_state { win::capture_status(std::path::Path::new(&path),&mode).map(|p|p.display().to_string()) }
        else { win::capture(std::path::Path::new(&path)).map(|p| p.display().to_string()) }
    } else {
        win::run(paths).map(|_| String::new())
    };
    if let Err(e) = result {
        let msg: Vec<u16> = format!("{e}\0").encode_utf16().collect();
        let title: Vec<u16> = "몽땅파일변환\0".encode_utf16().collect();
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                std::ptr::null_mut(),
                msg.as_ptr(),
                title.as_ptr(),
                0x10,
            );
        }
        std::process::exit(1);
    }
}
#[cfg(not(windows))]
fn main() {
    eprintln!("convert_win은 Windows 전용입니다.");
}
