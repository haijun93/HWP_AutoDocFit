# KCLIPSET3 결과 — 몽땅클립보드 설정 창 완성

## 변경 파일

- `kagent/src/settings_window.rs`: 설정 화면 간격과 글꼴을 손보고, 제외 앱을 한 줄씩 표시한다. 입력칸 용도를 표시하고 앱 추가·삭제, 폴더 선택 대화상자·삭제를 연결했다. 키보드 입력으로 단축키 조합을 캡처하고 Win 조합·Alt+F4·Ctrl+Alt+Delete를 저장 전에 거부한다. 동의 전 켜기 비활성, 켜는 중 진행 문구, 15초 제한과 실패 후 재시도를 적용했다. 실행 중인 에이전트에는 파이프 `settings_set`으로 적용하고, 꺼져 있으면 다음 시작부터 적용한다고 안내한다.
- `kagent/src/hotkey.rs`: 예약 조합별 거부 사유와 시험을 추가했다.
- `kagent/src/main.rs`: HKCU Run 등록/해제의 임시 값 이름 경로와 자동 정리 시험을 추가했다.
- `docfit_app/kclip_service.py`: 설정 창 시작 시 실행 중인 에이전트의 파이프 이름을 `--pipe`로 전달한다.
- `docfit_app/settings_schema.py`, `docfit_app/configuration_service.py`: `kclip_settings_mode` 기본값 `dialog`를 추가하고 일반 설정 목록에서 제외한다.
- `desktop_web_ui.py`: 기존 `kclip_window` 브리지에 설정 방식 조회·저장 및 네이티브 창 열기를 추가했다. 새 API 이름은 만들지 않아 `_BrowserApi` 허용 목록을 그대로 썼다.
- `flutter_ui/lib/features/kclip/kclip_dialog.dart`: 기존 대화상자를 유지하면서 `앱 대화상자`/`Windows 창` 선택을 추가했다. native 선택 시 `--settings` 창을 연다.
- `flutter_ui/test/kclip_dialog_test.dart`, `tests/test_bridge_methods.py`, `tests/test_kclip_service.py`: 설정 방식·실행 인수 전달 시험을 추가했다.

## 캡처와 관찰

캡처 파일은 `<임시폴더>/kclipset3-captures/`에 생성했다.

- `kclip-settings-light.png` (96 DPI), `kclip-settings-dark.png` (96 DPI), `kclip-settings-light-150dpi.png` (150%): 이미지 Read 도구로 모두 직접 열었다.
- 150% 캡처에서 `lastpass.exe`와 `example.exe`가 각각 한 줄로 상자 안에 보인다. 캡처 종류 아래 빈 공간을 줄였다. 이름 없는 입력칸에는 “추가할 앱 이름(.exe) 또는 폴더 경로” 레이블이 붙었다. 제목과 보관·동작 구역 제목을 굵게 키웠다. 저장·켜기·닫기 버튼을 본문 하단에 모았고, 자동 실행 체크박스·설명과 겹치지 않는다.
- 캡처는 설정 모델을 읽는 `--capture-settings`의 공용 화면 그리기 경로다. 실제 Win32 자식 컨트롤·IME·시스템 테마 렌더링까지 캡처한 것은 아니다.

## 시험 명령과 결과

```text
cargo test --manifest-path kagent/Cargo.toml --bin kagent --quiet
running 161 tests
test result: ok. 159 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out

cargo test --manifest-path ui_theme/Cargo.toml --lib --quiet
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

python -m unittest tests.test_kclip_service tests.test_bridge_methods
Ran 49 tests
OK

flutter analyze
No issues found! (ran in 25.0s)

flutter test test/kclip_dialog_test.dart
00:23 +23: All tests passed!

flutter build web --release
√ Built build\web

cargo build --manifest-path kagent/Cargo.toml --bin kagent --quiet
성공 (오류 없음)

kagent.exe --capture-settings <임시폴더>
kagent.exe --capture-settings <임시폴더>   [KAGENT_CAPTURE_DPI=144]
kagent.exe --capture-settings <임시폴더> --dark
PNG: kclip-settings-light.png 136,166 bytes; kclip-settings-light-150dpi.png 184,301 bytes; kclip-settings-dark.png 143,596 bytes

git diff --check
성공 (출력 없음)
```

Cargo 실행에서는 `%LOCALAPPDATA%/DocFitTools/mingw64/bin`을 PATH 맨 앞에 두고 `CARGO_TARGET_DIR`만 환경변수로 지정했다. 캡처 설정 데이터와 PNG는 임시 폴더에 두었다. HKCU Run 시험은 `KClipSet3-Test-<PID>`라는 임시 값만 등록·조회·삭제했고, 실제 앱 Run 값은 건드리지 않았다. `hwp-auto-docfit.py`는 수정하지 않았다. 실행 중인 몽땅클립보드·앱을 종료·재시작·설치하지 않았다.

## 사용자 확인이 필요한 부분과 미확인 항목

- 실제 설정 창에서 Ctrl/Alt/Shift 조합 입력, 한글 IME 입력, 고대비 테마, 150%·200% DPI 전환을 확인해야 한다.
- 폴더 선택은 Win32 Shell 폴더 선택 대화상자로 코드를 연결했지만, 실제 데스크톱에서 열고 선택하는 사용자 확인은 하지 않았다.
- 실행 중 다른 프로그램이 점유한 실제 단축키 충돌은 격리 시험에서 발생시키지 않았다. 등록 거부 때 kagent가 돌려준 충돌 상태와 사유 표시만 연결했다.
- 켜기 성공·실패·시간초과는 15초 결과 분류 함수에 가짜 입력을 넣어 종결 상태를 확인했다. 실제 설치·프로세스 시작 수명주기를 대체하는 통합 가짜 런타임 시험은 아니다.
- 자동 실행 등록·해제는 임시 Run 값으로 시험했다. Windows 로그인 후 실제 자동 시작은 사용자 환경에서 확인하지 않았다.
- `--settings --pipe` 전달은 Python 단위 시험, native/dialog 선택은 Flutter 위젯 시험으로 확인했다. 별도 실제 앱 프로세스와 실행 중 agent를 연결한 수동 시험은 하지 않았다.
