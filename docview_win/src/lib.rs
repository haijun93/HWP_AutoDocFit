//! 문서뷰 Rust+Win32 창(`docview_win`): 격자 목록 표시·제외 폴더 편집(DOCVIEWWIN2).
//! 순수 부분(json·model·scan·layout·settings·synth)은 창 없이 시험하고, 그리기(paint)는 실제 창과 캡처 모드가 같은 함수를 쓴다.
pub mod json;
pub mod layout;
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
pub mod png;
#[cfg(windows)]
pub mod sys;
#[cfg(windows)]
pub mod window;
