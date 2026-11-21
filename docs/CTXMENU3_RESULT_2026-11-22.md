# 몽땅파일변환 앱 파이프 전달 결과 (CTXMENU3, 2026-11-22)

## 구조

`mdm-ctx`는 경로 모음·중복 제거·확장자 검사 뒤 실행 중 앱의 `\\.\pipe\HwpAutoDocFit.convert`에 UTF-8 JSON 한 줄(`cmd=convert_open`, `target`, `paths`)을 보낸다. 연결 또는 성공 응답이 2초 안에 없으면 기존처럼 `convert <경로…> --to <형식> --notify`를 실행한다. 파이프 페이로드는 1MB·2,000개 이하만 보내며 Rust는 `CONVERSION_INPUTS`와 같은 20개 확장자를 확인한다.

앱은 WebView `loaded` 후 `docfit_app.convert_pipe.ConvertPipeReceiver`를 시작한다. 현재 사용자 토큰의 기본 DACL과 원격 클라이언트 거부를 사용하며, 받은 JSON 크기·경로 수·파일 존재·일반 파일·확장자를 다시 확인한다. 이후 `DesktopWebBridge.open_conversion_paths`가 기존 `add_paths`를 호출하고 파일변환 화면과 요청 형식을 선택한다. 자동 변환은 시작하지 않는다. WebView 종료 시 수신 스레드를 깨우고 2초 안에 정리한다.

## 폴백 표

| 상황 | 동작 |
|---|---|
| 앱 실행 중, 정상 연결·`ok:true` 응답 | 한 번 전달, 새 앱 프로세스 없음 |
| 파이프 없음/연결 실패 | 최대 2초 후 기존 앱 실행 방식 |
| 앱이 연결은 받았지만 2초 내 응답 없음/오류 응답 | 기존 앱 실행 방식 |
| 1MB 초과 또는 2,000개 초과 | 파이프 전송 생략 후 기존 앱 실행 방식 |
| 앱 꺼짐 | 기존 앱 실행 방식 |

## 시험 결과

- `cargo test --manifest-path mdm-ctx/Cargo.toml` 단위: `test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s` (경로 개수·크기 상한 포함).
- 같은 명령의 통합(`tests/flow.rs`): `test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.77s`. 가짜 파이프 서버 성공 1회 전달·기존 앱 미실행, 미연결 시 기존 실행 폴백 5 흐름 포함.
- `python -m unittest tests.test_convert_pipe tests.test_mdm_ctx tests.test_bridge_methods`: `Ran 24 tests in 0.732s` / `OK`.

## 사용자 실기 확인 방법

실행 중 앱에서만 확인한다: 파일변환 화면을 연 상태로 `.hwp` 또는 `.pdf`를 탐색기에서 우클릭해 **몽땅파일변환 → 변환**을 선택한다. 앱 새 창/프로세스가 생기지 않고 기존 앱의 파일변환 목록에 해당 파일이 추가되는지 확인한다. 파일 목록에서 형식이 선택됐는지 확인하고 변환은 사용자가 직접 시작한다.

## 미확인

- 실제 탐색기 우클릭에서의 앱 실행 중 전달은 사용자 실기 확인 전이다.
- 설치된 한/글 COM과 원본 문서 변환은 이번 범위에서 실행하지 않았다.
- UI의 `loaded`가 오기 전(초기 로딩 중) 파이프는 아직 열리지 않아 기존 실행 방식으로 폴백할 수 있다.
- 대량 선택(2,000개 경계) 및 1MB 초과 경로 목록은 경계 처리 구현을 했지만 통합 실기 검증은 하지 않았다.
- 실제 사용자 앱과 실제 레지스트리 키는 건드리지 않았다. GitLab(`origin`)과 푸시는 사용하지 않았다.
