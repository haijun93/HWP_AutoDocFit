//! 사진뷰 Rust+Win32 창 1단계(`photoview_win`): 격자 목록 읽기 전용 표시. 격자·스캔·그리기 도우미는 `grid_core`(문서뷰와 공용).
//! 순수 부분(model·scan·settings·synth)은 창 없이 시험하고, 그리기(paint)는 실제 창과 캡처 모드가 같은 함수를 쓴다.
pub mod model;
pub mod panel;
pub mod scan;
pub mod settings;
pub mod synth;

#[cfg(windows)]
pub mod capture;
#[cfg(windows)]
pub mod folder;
#[cfg(windows)]
pub mod paint;
#[cfg(windows)]
pub mod window;

pub use grid_core::{json, layout};
#[cfg(windows)]
pub use grid_core::{gdi as sys, png};
