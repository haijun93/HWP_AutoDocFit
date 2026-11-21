//! 화면 데이터 모델. 목록·검색·정렬(`grid_core::items`)·표시 글자(`grid_core::fmt`)·스캔 상태(`grid_core::state`)는 공용이고,
//! 이 파일에는 문서뷰 고유 부분(확장자 색·`App`)만 둔다.
pub use grid_core::app::App;
pub use grid_core::fmt::*;
pub use grid_core::items::*;
pub use grid_core::state::*;

/// Flutter `_docExtColor` 와 같은 색(COLORREF, BGR).
pub fn ext_color(ext: &str) -> u32 {
    let rgb = |r: u8, g: u8, b: u8| ui_theme::rgb(r, g, b);
    match ext.to_lowercase().as_str() {
        "hwp" | "hwpx" => rgb(0x25, 0x63, 0xEB),
        "xls" | "xlsx" => rgb(0x16, 0xA3, 0x4A),
        "ppt" | "pptx" => rgb(0xDC, 0x26, 0x26),
        "pdf" => rgb(0xEA, 0x58, 0x0C),
        _ => rgb(0x64, 0x74, 0x8B),
    }
}

pub fn badge_text(ext: &str) -> String {
    if ext.is_empty() { "DOC".into() } else { ext.to_uppercase() }
}

