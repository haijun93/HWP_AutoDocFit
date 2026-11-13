#![windows_subsystem = "windows"]
mod window;
fn main() {
    if let Err(error) = window::run() {
        if let Some(path) = std::env::var_os("KTM_ERROR_FILE") {
            let _ = std::fs::write(path, &error);
            std::process::exit(1);
        }
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                std::ptr::null_mut(),
                window::wide(&error).as_ptr(),
                window::wide("몽땅검색").as_ptr(),
                0x10,
            );
        }
    }
}
