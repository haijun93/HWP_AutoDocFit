//! 화면 데이터 모델. 목록·검색·정렬·표시 글자·스캔 상태·`App` 은 `grid_core` 공용이고, 여기에는 사진 고유 부분(확장자 색·배지)만 둔다.
pub use grid_core::app::App;
pub use grid_core::fmt::*;
pub use grid_core::items::*;
pub use grid_core::state::*;

/// 사진 확장자별 색(COLORREF, BGR). 같은 계열 사진은 같은 색으로 묶는다.
pub fn photo_color(ext: &str) -> u32 {
    let rgb = |r: u8, g: u8, b: u8| ui_theme::rgb(r, g, b);
    match ext.to_lowercase().as_str() {
        "jpg" | "jpeg" => rgb(0x25, 0x63, 0xEB),
        "png" => rgb(0x16, 0xA3, 0x4A),
        "heic" => rgb(0x7C, 0x3A, 0xED),
        "webp" => rgb(0xEA, 0x58, 0x0C),
        "tif" | "tiff" => rgb(0xDB, 0x27, 0x77),
        _ => rgb(0x64, 0x74, 0x8B),
    }
}

pub fn badge_text(ext: &str) -> String {
    if ext.is_empty() { "IMG".into() } else { ext.to_uppercase() }
}
