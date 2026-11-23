# DOCKFIX1 결과 — 2026-11-23

## 수정 내용

- Dock `route(12)`와 `route(13)`을 모두 Flutter 호스트의 기존 `docfit-open-tab-N` 이벤트 경로로 보낸다. 12는 `WorkspaceExtraPage.library`(가상 폴더), 13은 `WorkspaceExtraPage.notebook`(업무자료함)을 연다.
- Flutter는 이 두 이벤트를 예약 탭으로 버리지 않고 화면 요청으로 처리한다. Dock의 기존 named event 및 Python 수신기·WebView 전달 흐름을 유지하며, 재클릭 닫기 이벤트는 해당 추가 화면을 접는다.
- 과거 `route(13) == Clipboard`를 기대하던 시험을 `MainTab(13)`으로 바꿨다. 업무자료함 아이콘·라벨과 실제 화면이 일치해야 하고, 클립보드는 독 메뉴에 없는 별도 기능이기 때문이다. 12도 실제로는 `kvf.exe --show`를 실행해 Flutter의 가상 폴더 화면과 연결되지 않았으므로 함께 앱 화면 경로로 고쳤다.
- 12와 13이 서로 다른 Flutter 화면을 여는 시험, Rust 경로·이벤트 계약 시험, Python/Rust/Flutter 간 이벤트 이름 시험을 추가했다.

## 클립보드 영향 확인

- Dock의 `open_tab()`에서 `kagent.exe`를 띄우던 경로는 `Route::Clipboard` 한 곳이었고, 오동작 중인 13번 라우트가 유일한 호출점이었다. `kagent.exe` 실행 전용 분기를 제거했다.
- 독 메뉴는 `dock/src/lib.rs::menu_items()`에 정의된 항목만 표시하며 별도 클립보드 항목은 없다. `flutter_ui/lib/main.dart`에는 앱 안 `Ctrl+Shift+V` 몽땅클립보드 화면 진입이 있고, 앱 바깥 단축키는 상주 `kagent`가 담당한다. 두 경로 모두 독 번호 13을 사용하지 않는다.
- 12번은 기존에 `kvf.exe --show`를 별도 실행했으나 앱의 Flutter 가상 폴더 extra page를 열지 못했다. 이제 12 이벤트는 그 extra page를 요청한다.

## 검증

- `cargo test --release --locked -j 2 --manifest-path dock/Cargo.toml` — 통과. 모든 `test result`:

```text
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.40s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

- Python `test_dock_runtime` 및 `test_bridge_methods` — 통과:

```text
Ran 18 tests in 6.429s
OK
```

- 저장소에 `.venv`가 없어 시스템 Python 3.14로 위 Python 시험을 실행했다.
- `flutter analyze` — `No issues found!` (ran in 150.9s).
- 관련 Flutter 회귀시험 `flutter test test/tab_navigation_shell_test.dart test/tab_open_events_test.dart` — `All tests passed!` (12개).
- 최종 `flutter test` 전체 실행 — 통과: 817 passed, 3 skipped (`All tests passed!`).
- 첫 `flutter test` 전체 실행은 새 시험이 가상 폴더 라벨이 한 번만 보인다고 가정해 실패했다. 메뉴 라벨과 extra page 제목이 함께 나타나는 구조를 반영해 시험을 고유 페이지 key·화면 내용 기준으로 수정했다. 관련 시험 12개와 이후 전체 시험이 통과했다.
- 헤드리스 전용 확인: Release `dock.exe --capture-dir <임시 폴더>`가 `dock_light_96.png`와 `dock_light_96_alpha.png`를 생성했다. 일반 모드 실행 및 실행 중인 앱·독 조작은 하지 않았다.
- 한/글 COM 시험은 실행하지 않았다.

## 변경 파일

- `dock/src/lib.rs`, `dock/src/native.rs`
- `docfit_app/dock_runtime.py`, `tests/test_dock_runtime.py`
- `flutter_ui/lib/features/workspace/tab_open_events.dart`
- `flutter_ui/lib/features/workspace/tab_navigation_shell.dart`
- `flutter_ui/test/tab_open_events_test.dart`
- `flutter_ui/test/tab_navigation_shell_test.dart`
