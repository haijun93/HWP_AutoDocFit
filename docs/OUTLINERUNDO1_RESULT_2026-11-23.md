# OUTLINERUNDO1 결과 — 아웃라이너 문서 단위 실행 취소/다시 실행 (2026-11-23)

OUTLINERQA1(`docs/OUTLINERQA1_RESULT_2026-11-22.md`)이 Logseq 대비 격차로 꼽은 "문서 단위 실행 취소/다시 실행 없음"을 엔진·Flutter 탭·Win32 창에 추가했다. 한/글 COM·사용자 폴더·사용자 문서는 쓰지 않았고 시험은 합성 마크다운만 썼다.

## 1. 설계와 근거

| 결정 | 근거 |
|---|---|
| 이력 = **직렬화된 마크다운 스냅숏 스택**(역연산 아님) | 편집 연산이 10종이 넘고(set_text·insert·delete·indent·outdent·up/down·split·merge·status…) 역연산을 따로 만들면 연산마다 버그 표면이 늘어난다. 마크다운은 원문 줄(Raw)까지 보존하므로 되돌린 결과가 바이트 단위로 같다. 시험에서 N단계 되돌림/다시 실행이 정확히 이전 문자열과 같음을 확인했다. |
| **한 번의 사용자 명령 = 한 단계**: 명령 직전 마크다운을 한 번만 `record` | Enter 가 만드는 `set_text`+`insert`(Flutter 는 엔진 호출 두 번, Win32 는 한 트랜잭션)가 한 단계가 된다. |
| 한도: 200단계, 스냅숏 합계 32 MiB(Dart 는 16M 글자) | 큰 문서(10 MiB 한도)에서도 메모리가 무한히 늘지 않게 오래된 단계부터 버린다. 스냅숏 하나가 한도보다 크면 기록하지 않고 이력을 비운다(되돌릴 수 없는 상태를 거짓으로 약속하지 않음). |
| 새 편집 → 다시 실행 스택 비움 / 페이지 전환·저장 거절(충돌)·`parse` 실패 → 이력 비움 | 기준 문서가 어긋난 이력을 적용하면 외부 변경을 덮어쓸 수 있다. 안전을 위해 비운다. |
| 엔진은 무상태 → 이력은 호출자가 쥐고 `history` 연산으로 주고받음 | Python 서비스는 요청마다 엔진 프로세스를 띄운다. Win32 는 라이브러리를 직접 부르므로 `UndoHistory` 를 `Editor` 가 쥔다. Flutter 는 매 편집마다 스택 전체를 프로세스 간에 오가게 하지 않으려고 같은 규칙의 Dart 클래스(`OutlinerHistory`)를 쓴다(규칙·한도 동일, 두 구현이 따로 있다는 점이 비용). |
| **입력 중 글자 편집과의 협력**: 확정하지 않은 글(칸 글 ≠ 확정된 블록 글)이 있거나 한글 조합 중이면 Ctrl+Z/Y 는 칸·EDIT 컨트롤 자체의 글자 되돌리기에 맡기고(Flutter `KeyEventResult.ignored`, Win32 `KeyResult::Pass`), 확정할 글이 없으면 문서 단위 이력을 쓴다 | 글자 단위 취소와 블록 단위 취소가 같은 키를 두고 싸우지 않는 가장 단순한 규칙이다. 칸에서 글을 쓰다 Ctrl+Z 를 연타하면 먼저 글자가 되돌려지고, 확정 상태에 이르면 다음 Ctrl+Z 부터 문서 단계가 되돌려진다(Logseq 와 같은 감각). |

접기/펼치기(`fold`)는 세션 상태라 마크다운에 남지 않으므로 이력 단계가 아니다(Logseq 도 접힘은 문서 편집이 아님). 따라서 "편집 연산마다" 대상은 마크다운이 실제로 바뀌는 연산이다.

## 2. 변경 내용

- **엔진** `outliner_core/src/history.rs`: `UndoHistory`(`record`·`undo`·`redo`·`clear`·`bind(page,version)`·`rebase`, 한도 `MAX_UNDO_DEPTH=200`·`MAX_UNDO_BYTES=32MiB`). `api.rs`: 새 연산 `history`(push·undo·redo·clear·rebase·status)와 `edit` 의 선택 인자 `args.history`. `args.history` 가 없으면 `edit` 응답은 이전과 같다(시험으로 확인). 명세: `docs/OUTLINER_ENGINE_API_2026-11-21.md` 하단.
- **Python** `docfit_app/outliner_service.py`: `history` action 추가(`operation`→엔진 `args.action`, `page` 유지). 기존 action 불변.
- **Flutter 탭** `outliner_history.dart`(스택)·`outliner_workspace.dart`: 머리글에 되돌리기·다시 실행 단추(이력이 없으면 비활성), 칸 포커스 중 Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z(macOS 는 ⌘), 칸에 포커스가 없을 때도 같은 키. 두 단추는 포커스를 받지 않아(`ExcludeFocus`) 누르는 순간 칸의 미확정 글이 사라지지 않는다. 미확정 글이 있을 때 되돌리기 단추는 그 글만 확정 상태로 되돌리고 문서는 건드리지 않는다. 다시 실행 단추는 그때 비활성.
- **Win32 창** `outliner_win/src/edit.rs`: `Editor.history`, 모든 편집 명령(`execute`·`finish_edit`·`toggle_block`·`add_block_at_end`·`flush_for_close`)을 `recorded` 로 감싸 마크다운이 바뀌면 한 단계 기록. 편집 중·목록 모드 모두 Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z(`window.rs`). 되돌린 뒤 편집 중이면 같은 블록(없으면 마지막 블록)에서 편집을 이어 간다. 페이지 열기(`replace_document`)·저장 충돌은 이력을 비운다.

## 3. 변경 전후 동작

| 상황 | 변경 전 | 변경 후 |
|---|---|---|
| Enter 로 새 줄을 만든 뒤 Ctrl+Z | 미지원(포커스된 칸의 글자 단위 취소만) | 글 확정+줄 추가가 한 번에 되돌려짐, Ctrl+Y 로 복원 |
| 들여쓰기·내어쓰기·위/아래 이동·삭제·병합·TODO 토글 되돌리기 | 불가 | 가능(단계 200 까지) |
| 되돌린 뒤 새 편집 | — | 다시 실행 비움 |
| 페이지 전환·저장 충돌 | — | 이력 비움 |
| 입력 중 글 Ctrl+Z | 칸 글자 되돌리기 | 같음(문서 이력과 충돌 없음) |

## 4. 시험 결과 (실제 숫자)

| 영역 | 명령 | 결과 |
|---|---|---|
| outliner_core | `cargo test --release --locked` | `test result: ok. 79 passed` / `0 passed` / `ok. 1 passed` / `0 passed`(0 failed). 이 중 새 시험 9개(`history_tests.rs`) |
| outliner_win | `cargo test --release --locked` | 14개 `test result` 줄 모두 ok(`6, 0, 0, 30, 4, 10, 10, 9, 5, 6, 31, 13, 6, 0 passed`, 0 failed). 새 `tests/undo.rs` 6개 포함 |
| Python | `python -m unittest tests.test_outliner_service tests.test_bridge_methods tests.test_package_integrity` | `Ran 28 tests … OK`(새 시험 1개 포함) |
| Flutter | `flutter analyze` | `No issues found!` |
| Flutter | `flutter test test/outliner_undo_test.dart` | 7개 모두 통과(`All tests passed!`) |
| Flutter | `flutter test`(전체) | `+815 ~3: All tests passed!`(815 통과, 3 건너뜀, 실패 0) |

새 시험 내용: 엔진 — 6단계 연속 편집 후 N번 되돌림/다시 실행이 바이트 단위로 같음, 한글·결합 이모지 텍스트, 새 편집이 다시 실행을 비움, 묶음 한 단계, 깊이 한도(200)·바이트 한도·초과 스냅숏 거부, 페이지/버전 변경 시 비움, `history` API 왕복, 구버전 `edit` 불변. Win32 — 키 시험(편집 중 Ctrl+Z/Y/Ctrl+Shift+Z, Enter 분할 한 단계, 미확정 글·조합 중 `Pass`, 새 편집·페이지 교체 시 비움, 목록 모드 TODO·이동 되돌림). Flutter — 한글 입력 후 Enter 가 한 단계로 되돌려지고 Ctrl+Y/Ctrl+Shift+Z 로 복원, 미확정 입력 시 문서 불변, 단추 비활성/활성, 새 편집·페이지 전환·저장 거절 시 이력 비움, `OutlinerHistory` 단위 시험.

## 5. 한계와 확인되지 않은 것

- **실제 한/글 IME 로 직접 확인하지 못했다.** 조합 중 `Pass`(Win32)·`composing` 검사(Flutter)는 가짜 호스트/합성 입력으로만 시험했다. 실제 PC 에서 한글 조합 중 Ctrl+Z 가 EDIT 기본 동작으로 넘어가는지, Win32 EDIT 의 네이티브 Ctrl+Z 가 기대대로 글자 단위로 되돌리는지는 사람이 확인해야 한다.
- Win32 창에는 되돌리기 **단추를 넣지 않았다**(레이아웃·캡처 시험이 단추 7개에 맞춰져 있어 별도 카드 범위로 남김). 단축키만 연결했다.
- Win32 가 이력을 비우는 외부 변경은 저장 시도 때 감지되는 충돌 뿐이다(열어 둔 동안 파일이 바뀌어도 저장하기 전에는 알 수 없음). Flutter 도 저장 거절 때 비운다.
- Flutter 탭은 자동 저장 방식이라 되돌리기마다 파일에 저장된다. 저장이 거절되면 문서는 화면에 되돌려진 채 남고 이력은 비워진다.
- 접힘은 되돌리기 대상이 아니다(위 설계 참조).
- Dart 와 Rust 에 같은 규칙의 이력 구현이 각각 있다(한도 단위만 Dart 는 글자, Rust 는 바이트). 규칙을 바꿀 때 두 곳을 같이 고쳐야 한다.
- 커밋만 했고 푸시하지 않았다.
