//! 문서뷰(`docview_win`)·사진뷰(`photoview_win`) 공용 부품. 창과 무관한 순수 부분은 모든 OS 에서 시험한다.
//! - `json`: 읽기 전용 JSON 파서(설정·`klib scan` JSON 줄)
//! - `png`: 무압축 PNG 인코더(캡처)
//! - `fmt`: 크기·날짜·자리 쉼표·경과 표기, 색 섞기
//! - `state`: 스캔 상태 표시값과 키보드 이동
//! - `items`: 목록 항목(`Doc`)·검색·정렬(`Collection`)
//! - `layout`: 격자 배치·가상화·스크롤바·도구줄(`GridSpec` 로 문서뷰/사진뷰 규칙 구분)
//! - `open`: 목록 항목을 연결 앱으로 열기(대상 확장자 검사)
//! - `selection`·`file_ops`: 경로 기반 다중 선택·배경 파일 작업과 파일별 결과
//! - `native_ops`(Windows): 오른쪽 메뉴·확인/이름 입력/결과창·단축키·실행 상태
//! - `scan`: 줄 소스(자식 프로세스)를 읽는 비동기 작업·취소
//! - `gdi`(Windows): 그리기 도우미·메모리 DC 캡처·드라이브·시간대
pub mod app;
pub mod fmt;
pub mod handoff;
pub mod items;
pub mod json;
pub mod layout;
pub mod open;
pub mod png;
pub mod scan;
pub mod state;
pub mod thumb;
pub mod selection;
pub mod file_ops;

#[cfg(windows)]
pub mod native_ops;

#[cfg(windows)]
mod file_ops_windows;

#[cfg(windows)]
pub mod shell_thumb;

#[cfg(windows)]
pub mod gdi;

#[cfg(windows)]
pub mod send_menu;
