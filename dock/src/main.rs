#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
mod native;
#[cfg(windows)]
fn main() {
    native::run();
}
#[cfg(not(windows))]
fn main() {
    eprintln!("dock is a Windows application");
}
