//! Win32 보조(와이드 문자열·시간대·고정 드라이브)는 `grid_core::gdi` 로 옮겼다(경로 유지용 재노출).
pub use grid_core::gdi::{fixed_drive_roots, tz_offset_secs, wide};
