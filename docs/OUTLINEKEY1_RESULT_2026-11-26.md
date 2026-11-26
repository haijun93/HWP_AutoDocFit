# OUTLINEKEY1 결과 — 아웃라이너 편집 동작을 Logseq 에 맞춰 재구현 (2026-11-26)

설계·동작 표·근거·Logseq 와의 차이: `docs/OUTLINEKEY1_DESIGN_2026-11-26.md`.
Logseq 는 0.10.9 공개 소스를 읽어 동작만 확인했고 코드는 복사하지 않았다. 실제 Logseq 앱을 눌러 본 확인은 하지 않았다.
합성 마크다운만 썼다. 사용자 문서·한/글 COM 은 쓰지 않았다.

## 1. 한 일

- `outliner_core/src/keys.rs`: 키 규칙 순수 함수 `apply_key`(블록·커서·선택·접힘 → 문서·새 커서·선택), 괄호 짝 `type_char`·`pair_backspace`, 엔진 명령 `op: "key"`.
- 파일 형식 확장(설계 §3): `불릿 들여쓰기 + 2칸` 이어지는 줄 = 블록 안 줄바꿈, `키:: 값` 줄은 블록에 붙어 이동. 편집하지 않은 문서는 바이트 그대로.
- Flutter 탭: 편집·선택 키를 `key` 요청으로 교체, 여러 줄 입력 칸, 괄호 짝(Dart 판 `outliner_typing.dart`), 자동완성 키보드 선택, 다른 블록 클릭 시 저장. `outliner_service.py` 에 `key` 연결.
- 네이티브 창: 같은 `apply_key` 를 라이브러리로 직접 호출, 여러 줄 EDIT·줄 높이, 붙여넣기·괄호 짝·선택 키.

### 함께 고친 기존 결함

- 네이티브: Backspace 로 블록을 합친 뒤 뒤따르는 `WM_CHAR(8)` 이 EDIT 에 들어가 이음매 앞 글자를 지울 수 있는 경로(코드로 확인: 삼키는 목록에 8 이 없었음). 실창 시험 추가.
- Flutter: 빈 블록 Backspace 가 하위가 있어도 블록을 지움, 확정하지 않은 글이 다른 블록을 누르면 되돌려질 수 있음.
- 엔진: `id::` 같은 속성 줄이 블록 이동·삭제 때 제자리에 남음.

### 바뀐 기존 동작(사용자가 느끼는 것)

- Esc: 네이티브는 "입력 버림" → 저장하고 블록 선택.
- Backspace 병합: 네이티브의 공백 1칸 삽입 없음.
- 편집 중 Alt+↑/↓: 이동 → 블록 선택. 이동은 Alt+Shift+↑/↓.
- Ctrl+Enter: Flutter 접기 → TODO 순환(없음→TODO→DONE→없음). 접기는 Ctrl+↑/↓·Ctrl+;.
- Shift+Tab: 하위를 데리고 부모의 하위 전체 뒤로 이동(논리적 내어쓰기).

## 2. 시험 결과

마지막 실행 출력(Windows, 로컬 MinGW).

`cargo test --release --locked -j 2` — `outliner_core` (모두 `ok`, 0 failed):

```text
test result: ok. 84 passed   (단위)
test result: ok. 0 passed
test result: ok. 1 passed    (cli_smoke)
test result: ok. 37 passed   (keys)
test result: ok. 0 passed
```

`cargo test --release --locked -j 2` — `outliner_win` (14줄 모두 `ok`, 0 failed, 합 137):

```text
6, 0, 0, 37, 5, 11, 10, 9, 2, 6, 31, 14, 6, 0 passed
```

- Python(저장소 `.venv` 가 없어 시스템 Python): `python -m unittest tests.test_outliner_service tests.test_bridge_methods tests.test_package_integrity` → `Ran 33 tests` `OK`
- `flutter analyze` → `No issues found!`
- `flutter test test/outliner_keys_test.dart test/outliner_workspace_test.dart test/outliner_undo_test.dart test/outliner_ui_gate_test.dart` → `+30: All tests passed!`

- 새 시험: core `tests/keys.rs` 37개(설계 표 번호별, 시험 벡터·엔진 `op:key` 포함), 네이티브 `edit_machine.rs`·`edit_window.rs` 추가 시험, `parity.rs`(Flutter 가 보내는 `key` 요청과 네이티브 키 입력의 결과 대조, 허용 차이 0), Flutter `test/outliner_keys_test.dart`.
- 의도한 동작 변경에 맞춰 바꾼 기존 시험: 네이티브 `edit_machine`·`edit_window`·`shell_flow`·`shell_window`·`parity`, Flutter `outliner_workspace_test`·`outliner_undo_test`. 예전 개별 `edit` 요청 순서를 보던 Flutter 키 시험 1개는 새 파일로 대체했다.
- Flutter 화면 시험은 가짜 엔진이다. 규칙 결과는 Rust 시험이, 화면이 보내는 요청·커서 반영은 Flutter 시험이 맡는다. 실제 `outliner.exe` 를 붙인 화면 시험은 하지 않았다.
- `parity.rs` 는 위 release 실행 뒤 시험 이름·공백만 고쳐 그 파일만 다시 돌렸다(debug, `2 passed`).
- 전체 python·flutter 시험은 돌리지 않았다(릴리스에서 PM).

## 3. 확인 못 한 것

- 실제 Windows 한글 IME 조합 중 Enter/Tab. 합성 입력·가짜 EDIT 로만 "한 번만 들어가고 실행"을 시험했다.
- 실제 시스템 클립보드(네이티브 `WM_PASTE`·Ctrl+C/X), 실제 키보드의 Ctrl+;·Alt 조합.
- 네이티브 여러 줄 블록의 화면 모양(편집 칸 줄 간격은 글꼴 높이, 목록은 28px/줄이라 어긋나 보일 수 있음). 화면 캡처 확인 안 함.
- 설치본·빌드 번들(`resources/outliner.exe`·`outliner_win.exe`)은 갱신하지 않았다. CI(MSVC) 빌드 전에는 Flutter 탭이 옛 엔진을 부르면 `key` 명령이 없어 편집이 실패한다.

## 4. 남은 문제·판단이 필요한 것

- 빈 블록 Enter: 카드 요약은 "내어쓰기"지만 Logseq 소스는 **다음 형제가 없을 때만** 내어쓴다. Logseq 를 따랐다(바꾸려면 `keys.rs` `enter` 의 조건 한 줄).
- 여러 줄 붙여넣기: 목록이 아닌 글도 줄마다 블록(카드). Logseq 는 한 블록 안 줄바꿈이다.
- 제외: DOING/NOW/LATER, `~ * _` 짝, 직접 내어쓰기 설정, 접힘의 파일 저장, 마우스 드래그 선택.
- Flutter 는 명령마다 엔진 프로세스를 띄운다. 처리 중 친 글자가 사라지거나 연타 Enter 가 무시될 수 있다(예전에도 같은 구조, 이번에 Enter 의 호출은 2회→1회).
- 실행 취소 단위는 기존대로 명령 하나(글 확정 포함) = 한 단계.
- 괄호 짝을 넣으면 네이티브 EDIT 의 글자 단위 Ctrl+Z 이력이 끊긴다(`SetWindowText`).
- 네이티브 목록 상태(편집 아님)는 줄이 이미 강조돼 있어, 첫 Alt/Shift+↑↓ 가 바로 다음 줄까지 선택한다(기존 동작 유지).
- 커밋만 했고 푸시하지 않았다.
