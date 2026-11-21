# THUMBSHELL2 결과 (2026-11-22)

## 완료

- THUMBSHELL1 셸 API 구현과 경로 정규화 변경을 로컬 커밋 `0d8e210f`, `dc35a17a`로 가져왔다. 기존 THUMBSHELL1 결과 문서는 현재 브랜치에 있던 내용을 보존했다.
- Rust release 바이너리는 빌드됐다: `docview_win.exe` 2,650,533 bytes, `photoview_win.exe` 2,134,587 bytes. THUMBSHELL1 이전 동일 조건 바이너리 및 clean CI 빌드 시간 기준 측정치는 없어 의존성의 증가분·CI 시간 변화는 산출하지 못했다.
- `grid_core` 기본 시험: 7 passed, 셸 시험 1 ignored(명시적 셸 핸들러 실측 필요). 각 `test result`는 아래 기록.

## 검증 기록

환경: Windows, Rust 1.99.1. cargo 대상 디렉터리는 임시 경로로 지정. 전체 명령은 요청대로 `--release --locked`.

- `cargo test --manifest-path grid_core/Cargo.toml --release --locked`: lib 0/0, core_tests 7 passed, shell_thumb_tests 0 passed·1 ignored, doc-tests 0/0.
- `cargo test --manifest-path docview_win/Cargo.toml --release --locked`: lib 0/0, main 0/0, capture_tests 5 passed, exclude_tests 11 passed, pure_tests 31 passed. `real_klib_tests` 5개는 `klib.exe`를 찾지 못해 모두 실패했다(기능 실패 판정 아님, 실행 파일을 제공하지 않은 환경 실패).
- `cargo test --manifest-path photoview_win/Cargo.toml --release --locked`: 실행을 시작했으나 이 보고서 작성 시 최종 test result 출력이 수집되지 않았다.

## 미완료 — 이 결과를 기능 완료로 보지 말 것

요청한 작업자 풀(1·2·4·8), 20·200장 cold/warm 측정 및 Pillow 비교를 하지 않았다. 창들에 visible-first 요청, 취소·재우선, 바이트 LRU, UI 메시지 갱신과 3초 실패 처리를 연결하지 않았다. 두 앱에 `--capture-real` 실제 셸 썸네일 경로를 추가하거나 캡처 PNG를 Read로 열어 관찰하지 않았다. 처리기 부재·접근 거부·클라우드 전용 입력은 확인하지 않았다. 현재 구현은 THUMBSHELL1의 요청별 STA 스레드/15초 호출이며 3초 상한이 아니다.

다음 작업은 풀 API를 STA 스레드마다 초기화하도록 정리한 뒤 두 Win32 창 모델·paint에 연결하고, 합성 이미지 전용 측정·실제 셸 캡처를 수행하는 것이다. 사용자 문서나 사진은 열지 않았다.
