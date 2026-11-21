//! Win32 보조: 와이드 문자열·시간대·고정 드라이브.
use std::path::PathBuf;
use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 지금 시간대의 UTC 대비 오프셋(초). 여름 시간제 반영.
pub fn tz_offset_secs() -> i64 {
    unsafe {
        let mut tz: TIME_ZONE_INFORMATION = std::mem::zeroed();
        let id = GetTimeZoneInformation(&mut tz);
        let extra = match id {
            1 => tz.StandardBias,
            2 => tz.DaylightBias,
            _ => 0,
        };
        -((tz.Bias + extra) as i64) * 60
    }
}

/// 고정 드라이브 루트(`C:\` …). Flutter 기본 범위 "모든 고정 드라이브" 의 Rust 대응.
pub fn fixed_drive_roots() -> Vec<PathBuf> {
    let mask = unsafe { GetLogicalDrives() };
    (0..26u32)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| format!("{}:\\", (b'A' + i as u8) as char))
        .filter(|root| unsafe { GetDriveTypeW(wide(root).as_ptr()) } == 3)
        .map(PathBuf::from)
        .collect()
}
