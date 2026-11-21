//! 앱 설정 창 2단계(`settings_win`): 좌측 묶음 트리·우측 항목 목록·검색·저장/되돌리기.
//! 설정 파일은 `docfit_config` 로만 읽고 쓴다(Python 경유 없음). 순수 부분(schema·model·layout)은 창 없이 시험한다.
pub mod controls;
pub mod guides;
pub mod labels;
pub mod layout;
pub mod model;
pub mod schema;

#[cfg(windows)]
pub mod capture;
#[cfg(windows)]
pub mod paint;
#[cfg(windows)]
pub mod png;
#[cfg(windows)]
pub mod window;
