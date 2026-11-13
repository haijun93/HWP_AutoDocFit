//! ui-core: 몽땅클립보드 단축키 창 기술로 만든 공용 화면 부품(UI0, 2026-11-14).
//!
//! 기술(RUSTALL 7절): `windows-sys` Win32 직접, GDI 이중 버퍼, 사용자 틀(WM_NCCALCSIZE·WM_NCHITTEST),
//! Per-Monitor v2 DPI, 직접 그린 한 줄 입력 + IME, 여러 줄은 OS EDIT·RichEdit(Q14), Direct2D는 캔버스 창만(Q15),
//! 상태는 순수 함수(창 없이 시험)로. 화면 스레드에서 막는 호출 금지(TK0).
//!
//! | 모듈 | 부품 |
//! |---|---|
//! | [`window`] | 떠 있는 창·일반 창·모달, 창 종류 등록, 틀 메시지, 작업 영역 안 배치 |
//! | [`frame`] | 크기 조절 판정·DWM 꾸밈·테두리 한 겹·배율·줄 위치·글자 폭 |
//! | [`dpi`] | Per-Monitor v2, WM_DPICHANGED 다시 배치 |
//! | [`theme`] | 라이트·다크·고대비, 시스템 따라가기, WM_SETTINGCHANGE, DARK1 색 토큰 |
//! | [`font`] | 글꼴 캐시(DPI 열쇠) |
//! | [`paint`] | 이중 버퍼·그리기 도움·메모리 화면 |
//! | [`layout`] | 행·열·여백 |
//! | [`list`] | 가상 목록·다중 선택·스크롤 막대·휠 누적 |
//! | [`text_input`] | 한 줄 입력(직접 그림 + IME) |
//! | [`native_edit`] | 여러 줄·폼 입력(OS EDIT·RichEdit) |
//! | [`controls`] | 단추·체크·라디오·탭·드롭다운 |
//! | [`form`] | 설정 폼 |
//! | [`table`] | 표 |
//! | [`preview`] | 그림 미리보기(맞춤·확대·이동) |
//! | [`menu`] | 메뉴 모델 → Win32 메뉴 |
//! | [`focus`] | 키보드 탐색·포커스·접근 키 |
//! | [`a11y`] | 접근성(UIA 제공자) |
//! | [`d2d`] | Direct2D 자리(캔버스 창만) |
//! | [`probe`] | 그리기 결과 기록(시험) |

pub mod a11y;
pub mod controls;
pub mod d2d;
pub mod dpi;
pub mod focus;
pub mod font;
pub mod form;
pub mod frame;
pub mod geom;
pub mod layout;
pub mod list;
pub mod menu;
pub mod native_edit;
pub mod paint;
pub mod preview;
pub mod probe;
pub mod table;
pub mod text_input;
pub mod theme;
pub mod window;
