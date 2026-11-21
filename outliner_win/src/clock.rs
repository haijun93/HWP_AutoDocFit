//! 지역 시각(`GetLocalTime`) — 오늘의 일지 이름·휴지통 이름표에만 쓴다.
use crate::shell::Now;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;

pub fn now() -> Now {
    let t = unsafe {
        let mut t = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    Now {
        date_iso: format!("{:04}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay),
        stamp: format!("{:04}{:02}{:02}-{:02}{:02}{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond),
    }
}
