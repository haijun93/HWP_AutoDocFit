# DOCKBRIDGE 결과 (2026-11-21)

## 현재 구조와 변경 후 이벤트 경로

| 구간 | 담당 | 동작 |
|---|---|---|
| Dock 클릭 | `dock/src/native.rs` | 탭 인덱스 `N`에 대해 `OpenEventW`/`SetEvent`로 `docfit-open-tab-N`을 신호한다. 활성 탭 재클릭은 `docfit-close-tab-N`을 신호한다. |
| 이벤트 생성·수신 | `docfit_app/dock_runtime.py` | Dock 시작 직전 Python 호스트가 이름 있는 이벤트 32개(open 16, close 16)를 `CreateEventW`로 만든다. 기본 DACL과 세션 한정 이름을 사용하고 `WaitForMultipleObjects` 감시 스레드가 신호를 기다린다. 같은 이름이 이미 있으면 Windows가 돌려준 핸들을 재사용한다. |
| Python → WebView | `desktop_web_ui.py` | `DesktopWebBridge`가 수신기 콜백을 연결한다. 이벤트가 오면 `window.run_js("window.dispatchEvent(new Event('docfit-…-tab-N'))")`를 별도 스레드에서 호출한다. Dock 종료·앱 종료 때 수신 스레드와 핸들을 정리한다. 이 Python 수신기는 Python 호스트가 남아 있는 동안의 임시 다리이며 Rust 호스트가 생기면 대체한다. |
| WebView → Flutter | `flutter_ui/lib/features/workspace/tab_open_events.dart` | 각 인덱스의 DOM 이벤트를 구독해 `tabRequest`에 같은 인덱스를 전달한다. 숨김 탭 및 예약 인덱스 12·13은 열지 않는다. 닫기는 요청된 탭이 현재 선택된 경우에만 접는다. |
| named pipe | `\\.\pipe\HwpAutoDocFit.dock` | 별도 제어·준비 프로토콜이다. Python은 `ready`/`quit` 등 JSON 명령을 사용하고, 이 파이프는 탭 클릭 이벤트 전달에는 관여하지 않는다. |

Flutter의 `N`은 사이드바 그룹 순서가 아니라 `kTabCount == 16`의 탭 인덱스다. 현재 Flutter 수신기는 이 인덱스를 그대로 요청 상태로 전달하므로 별도 재매핑은 필요하지 않았다. 열기·닫기 DOM 이벤트 이름은 Rust의 `tab_event_name`/`tab_close_event_name` 계약과 일치한다.

## 변경 파일

- `docfit_app/dock_runtime.py` — Win32 이벤트 생성, 대기, JS 전달 콜백 및 생명주기 정리.
- `desktop_web_ui.py` — DesktopWebBridge에서 수신기를 연결하고 종료 시 Dock runtime을 정리.
- `tests/test_dock_runtime_events.py` — 가짜 Dock 프로세스와 Win32 named event 기반 E2E 시험. Windows 외 환경에서는 건너뜀.
- `docs/DOCKBRIDGE_RESULT_2026-11-21.md` — 구조, 시험 및 실기 확인 항목.

Flutter 이벤트 매핑 코드는 확인 결과 수정할 필요가 없어 변경하지 않았다.

## 실행한 시험과 결과

명령: `python -m unittest tests.test_dock_runtime_events tests.test_bridge_methods -v`

```text
Ran 10 tests in 0.686s
OK
```

명령: `flutter analyze` (작업 폴더 `flutter_ui`)

```text
No issues found! (ran in 17.1s)
```

명령: `flutter test test/tab_open_events_test.dart` (작업 폴더 `flutter_ui`)

```text
00:00 +2: All tests passed!
```

명령: `flutter build web --release` (작업 폴더 `flutter_ui`)

```text
Compiling lib\main.dart for the Web...                             71.5s
√ Built build\web
```

명령: `$env:PATH = "$env:LOCALAPPDATA\DocFitTools\mingw64\bin;$env:PATH"; $env:CARGO_TARGET_DIR = "$env:TEMP\dockbridge-cargo-target"; cargo test --manifest-path dock\Cargo.toml`

```text
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

전체 시험은 요청대로 실행하지 않았다. Cargo 산출물은 `CARGO_TARGET_DIR` 환경변수에만 두었고 저장소 `.cargo/config.toml`은 추가하지 않았다.

## 사용자 실기 확인

- 실제 앱에서 하단 Dock 아이콘을 눌러 해당 탭으로 전환되는지 확인한다.
- 이미 선택한 탭의 Dock 아이콘을 다시 눌러 탭이 접히는지 확인한다.
- 다른 탭이 선택된 상태에서 닫기 신호가 그 탭을 닫지 않는지 확인한다.

## 미확인

- 사용자 앱을 실행하거나 재시작하지 않았으므로 실제 Dock과 실제 Flutter/WebView 사이의 실기 동작은 확인하지 않았다.
- Dock과 앱이 서로 다른 사용자 계정으로 실행되는 경우는 지원 범위가 아니다. 이벤트는 현재 사용자 기본 DACL을 사용한다.
- 동일 사용자·동일 세션에서 같은 이름 이벤트가 이미 있는 상황은 자동 재사용 E2E 시험으로 확인했다.
