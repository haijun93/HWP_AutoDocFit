# KCLIPSET2 진행 결과 — 몽땅클립보드 설정 창 편집·화면

## 변경

- `kagent/src/settings_window.rs`: 설정 화면의 캡처 종류 체크, 제외 앱·폴더 목록 컨트롤, 단축키 입력칸과 파서 검사, 화면 간격·캡처 DPI 대응을 추가했다. 목록 값과 단축키가 유효하지 않으면 저장하지 않고 상태 줄에 이유를 표시한다.
- `kagent/src/store.rs`: `capture_text`, `capture_images`, `capture_files` 설정 키를 추가했다.
- `kagent/src/main.rs`: 세 캡처 허용 설정을 수집 시점에 적용한다.
- `docs/KCLIPSET2_RESULT_2026-11-21.md`: 실행·캡처 관찰·남은 차이를 기록한다.

## 저장과 적용

설정 창을 kagent 내부에서 연 경우 `update_settings`가 저장과 창 갱신을 수행한다. 별도 `--settings` 프로세스는 기존 설정 파일을 저장하며, 실행 중인 다른 kagent 프로세스에 파이프 알림은 보내지 않는다. 이 경우 변경은 다음 시작 때 적용된다. 실행 중 적용이 필요한 외부 창은 status 파일의 `pipe` 이름으로 `settings_set` 요청을 보내야 하며, 이번 단계는 외부 창 연결을 구현하지 않았다.

앱 연결 제안(미구현): Flutter는 kagent 실행 파일을 `--settings --pipe <파이프 이름>` 인수로 실행한다. 새 창은 status 파일에서 읽은 kagent 파이프에 `settings_get`으로 초기 상태를 받고 `settings_set`으로 저장한다. Flutter/Python 코드는 이번 변경에 포함하지 않았다.

## 캡처 관찰

`--capture-settings <폴더>`가 실제 설정 모델을 읽어 그린 PNG이며, 가짜 목업이나 빈 화면이 아니다. 캡처 모드에는 실제 Win32 자식 컨트롤의 스타일·IME가 그려지지 않고 공용 화면 그리기 경로의 캔버스가 쓰인다.

- `<폴더>/kclip-settings-light.png` (96 DPI): 제목, 보관 설정, 동작 설정, 글·그림·파일 체크, 제외 앱 목록, 폴더별 캡처 끄기 목록, 단축키 칸, Windows 로그인 시 자동 실행, 동의 및 하단 버튼이 보였다. 목록과 자동 실행 구분선은 겹치지 않았다.
- `<폴더>/kclip-settings-dark.png` (96 DPI): 같은 항목이 보였다. 체크 표식은 강조색 펜으로 그려져 배경과 구분된다.
- `<폴더>/kclip-settings-light-150dpi.png` (150% DPI): 같은 항목이 보였다. 첫 캡처에는 하단 편집칸·구분선 겹침이 있었고, 좌표를 조정해 캡처를 다시 만들었다. 재생성본에는 겹침이 없다.
- (b) 제외 앱은 목록 상자 안에 여러 줄로 표시되어 `la...` 잘림이 없다. (c) 구분선은 목록·문자와 떨어져 있다. (d) 자동 실행·동의는 체크박스 형태다. (e) 제목 굵기와 구역 제목은 구별된다.

캡처 경로는 작업 폴더 바깥의 임시 `<폴더>` 아래에 두었다. 라이트 96·150%와 다크 96% PNG를 이미지 Read 도구로 열어 직접 확인했다.

## 시험 결과

```text
cargo test --manifest-path kagent/Cargo.toml --bin kagent --quiet
test result: ok. 156 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 16.33s

python -m unittest tests.test_kagent_settings_menu tests.test_kclip_service
Ran 44 tests in 0.669s
OK

cargo build --manifest-path kagent/Cargo.toml --bin kagent --quiet
BUILD_EXIT=0
```

시험 중 실제 kagent 앱을 종료·재시작·설치하지 않았다. 자동 실행 체크는 시험 루트에서 레지스트리 API가 차단되어 있어 임시 Run 키 등록·해제를 실행하지 못했다. 가짜 lifecycle의 성공·실패·15초 시간초과 시험도 아직 추가하지 않았다.

## 남은 작업·실기 확인

- 단축키 입력은 문자열 편집 방식이다. 키 입력을 가로채 조합을 기록하는 캡처 동작과 충돌 조합 등록 결과 피드백은 미완성이다. 기존 parser는 적용했지만 예약 키·실제 다른 프로세스와의 충돌은 UI에서 검증하지 않는다.
- 폴더 목록은 경로 문자열로 추가한다. 폴더 선택 대화상자는 미구현이다.
- kagent 내부 설정 창은 즉시 반영한다. 별도 `--settings` 창은 파일 저장 후 다음 시작에 반영된다. 파이프 알림은 미구현이다.
- 켜기 버튼은 동의 미체크 시 안내를 표시한다. 진행 표시와 가짜 lifecycle 주입 시험은 미구현이다.
- 자동 실행·동의의 체크 상태는 캡처에서 모두 해제 상태만 확인했다. 설정 변경 시 실제 HKCU Run 값 반영은 실기 시험이 필요하다.
- 실제 키 조합·한글 IME, Windows 고대비, 150%·200% 모니터 전환에서 사용자 실기 확인이 필요하다.
