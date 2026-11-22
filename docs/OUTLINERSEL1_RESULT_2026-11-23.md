# OUTLINERSEL1 결과 — Logseq 키 정렬과 다중 블록 편집 (2026-11-23)

## 키 대응 전·후

| 조작 | 작업 전 | 작업 후 |
|---|---|---|
| `Alt+↑/↓` | Flutter·Win32에서 현재 블록 이동 | 현재 블록부터 인접 블록까지 선택 범위 확장 |
| `Shift+↑/↓` | 입력 칸의 텍스트 선택 동작에 맡김 | 블록 선택 범위 확장 |
| `Esc` | 선택 해제 없음 | 블록 선택 해제 |
| `Alt+Shift+↑/↓` | 연결되지 않음 | 선택 블록을 한 번에 위·아래로 이동 |
| 편집 칸의 텍스트 키 | 텍스트 편집이 우선 | 그대로 우선. Flutter는 조합 중·미확정 글·텍스트 범위 선택 중 블록 키를 처리하지 않고, Win32 편집 중 목록 단축키를 받지 않음 |

화면 도움말과 창 안내에 선택·이동 키를 표시했다. 기존 Alt 이동을 사용하던 사람을 위한 별도 알림은 추가하지 않았다.

## 구현

- `outliner_core`의 기존 `op: "edit"` 요청에 선택 인자 `args.block_ids`를 선택적으로 추가했다. 기존 `block_id` 요청은 유지된다. 다중 명령은 실패 시 문서를 바꾸지 않는 원자적 처리이며, 전달된 `args.history`에는 편집 전 문서를 한 번 기록한다.
- 다중 삭제·들여쓰기·내어쓰기·접기·펴기·이동을 처리한다. `paste`는 선택 끝에 Markdown 텍스트 블록을 넣는다. Flutter와 Win32의 복사·잘라내기는 일반 Markdown 텍스트를 사용한다.
- Flutter 탭과 Win32 창은 선택 범위 배경색을 표시한다. Win32는 Windows 텍스트 클립보드로 복사·잘라내기·붙여넣기를 연결했다.
- Flutter 위젯 시험은 가짜 API를 사용해 키 입력과 `block_ids` 배치를 확인한다. Win32 창 시험은 합성 수정자 키 입력으로 선택·일괄 들여쓰기·한 단계 Undo를 확인한다.

## 제약 및 실기 확인

- 선택은 현재 보이는 블록 사이의 연속 범위다. 마우스 드래그 선택은 구현하지 않았다.
- 붙여넣기는 Markdown 텍스트만 처리한다. 리치 텍스트와 외부 블록 참조 의미는 보존하지 않는다.
- 실제 Windows 한글 IME 조합 중 키 입력은 확인하지 않았다. Flutter composing 상태 검사와 Win32 편집 중 키 전달은 합성 시험에 한정된다.
- Win32의 실제 시스템 클립보드 읽기·쓰기는 실행하지 않았다. 창 시험에서 시스템 클립보드 내용이 바뀌지 않도록 키 선택과 편집만 검증했다.
- 사용자가 보고한 기존 오작동을 실제 사용 환경에서 재현해 원인까지 확인하지는 못했다.

## 시험 결과

초기 Flutter 시험에서 키 처리의 범위 오류와 선택 도구 모음의 좁은 창 넘침이 발견됐다. 도구 모음을 별도 줄에 배치하고, 기존 단일 이동 키 기대를 새 선택·이동 동작에 맞춰 위젯 시험을 갱신했다. Win32 clipboard API는 첫 문서시험 빌드에서 모듈 경로 오류가 나 직접 시스템 API 선언으로 바꿨다. 최종 재실행 결과는 아래와 같다.

- `outliner_core`: `cargo test --release --locked -j 2` 통과. 출력된 모든 결과:

  ```text
  test result: ok. 80 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```

- `outliner_win`: `cargo test --release --locked -j 2` 통과. 출력된 모든 결과:

  ```text
  test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
  test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.65s
  test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
  test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.29s
  test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.17s
  test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
  test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
  test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.36s
  test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.04s
  test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```

- Python: `.venv\Scripts\python.exe`가 없어 시스템 Python으로 `python -m unittest tests.test_outliner_service tests.test_bridge_methods tests.test_package_integrity` 실행. `Ran 28 tests`, `OK`.
- Flutter: `flutter analyze` → `No issues found!`; `flutter test` 전체 → `+816 ~3: All tests passed!`. 아웃라이너 위젯 파일 단독 재실행도 7개 모두 통과.

한/글 COM은 사용하지 않았다. IME 조합 키와 실제 시스템 클립보드는 실기에서만 확인할 수 있어 시험하지 않았다.

