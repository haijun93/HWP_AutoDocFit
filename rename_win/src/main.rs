#![windows_subsystem = "windows"]
#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    let mut args = std::env::args().skip(1);
    let (mut capture, mut selftest, mut dpi, mut data_dir) = (None, None, 96u32, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--capture" => capture = args.next(),
            "--selftest" => selftest = args.next(),
            "--data-dir" => data_dir = args.next(),
            "--dpi" => dpi = args.next().and_then(|v| v.parse().ok()).unwrap_or(96),
            _ => {
                fail(&format!("알 수 없는 인자: {a}"), 2);
            }
        }
    }
    if selftest.is_some() && data_dir.is_none() {
        let t = std::env::temp_dir().join(format!("renamewin5-selftest-data-{}", std::process::id()));
        data_dir = Some(t.to_string_lossy().into_owned());
    }
    if let Err(e) = win::set_data_dir(data_dir) {
        fail(&e, 1);
    }
    let result = if let Some(path) = capture {
        win::capture(std::path::Path::new(&path), dpi).map(|_| ())
    } else if let Some(path) = selftest {
        match win::selftest(std::path::Path::new(&path)) {
            Ok(true) => Ok(()),
            Ok(false) => std::process::exit(1),
            Err(e) => Err(e),
        }
    } else {
        win::run()
    };
    if let Err(e) = result {
        fail(&e, 1);
    }
}

#[cfg(windows)]
fn fail(msg: &str, code: i32) -> ! {
    let m: Vec<u16> = format!("{msg}\0").encode_utf16().collect();
    let t: Vec<u16> = "파일명 일괄변경\0".encode_utf16().collect();
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
            std::ptr::null_mut(),
            m.as_ptr(),
            t.as_ptr(),
            0x10,
        );
    }
    std::process::exit(code);
}

#[cfg(not(windows))]
fn main() {
    eprintln!("rename_win은 Windows 전용입니다.");
}
