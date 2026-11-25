# 아웃라이너 블록 참조 UI 및 페이지 관리 (OUTLINERREF1) 결과 보고서

**작성 일자**: 2026-11-25
**작업 브랜치**: `feature/OUTLINERREF1`
**작업 상태**: 구현 및 자동 검증 완료; 전체 Python 결과 반영 후 커밋 예정 (푸시하지 않음)

---

## 1. 개요 및 구현 요약

PM 결정(2026-11-23) 4절 우선순위 2 "블록 참조 UI와 페이지 이름 변경·삭제 같은 페이지 관리"를 구현했습니다.

### (1) 엔진 (`outliner_core`)
- **블록 참조 문법 확장**: `((( 블록 id )))` (3중 괄호) 및 `(( 블록 id ))` (2중 괄호) 모두 파싱, 공백 트림 처리 (`parse_serialize.rs`).
- **블록 참조 해결 (`resolve_block_ref`) 및 역참조**:
  - `id:: UUID` 속성뿐만 아니라 숫자 블록 ID (`1`, `2`, ...)로도 타깃 블록을 직접 매칭.
  - 타깃 블록의 ID 및 UUID에 대한 역참조 목록(`backlinks`)을 통합 반환.
- **새 JSON API 오퍼레이션 추가** (`api.rs`):
  - `insert_block_ref`: 대상 블록에 `((target))` 또는 `(((target)))` 블록 참조 삽입 및 `UndoHistory` 기록 연동.
  - `block_backlinks`: 특정 블록을 참조하는 블록 목록 및 텍스트 상세 정보 조회.
  - `check_page_references` (`page_references`): 지정 디렉터리 내 모든 `.md` 파일에서 특정 페이지를 가리키는 `[[페이지]]` 참조 파일 및 블록 탐지.
- **페이지 이름 변경 시 링크 일괄 갱신**: 기존 `rename_page_in_files` 활용 및 실행 취소 가능 지원.

### (2) 파이썬 서비스 어댑터 (`docfit_app/outliner_service.py`)
- `rename`: 파일 이름 변경 시 엔진의 `rename_page` 오퍼레이션을 통해 다른 모든 `.md` 파일 내 `[[옛이름]]`을 `[[새이름]]`으로 일괄 갱신하고, 변경 이력을 보관.
- `undo_rename`: 직전 이름 변경을 되돌리고 링크도 원래대로 원복.
- `check_delete` / `page_references`: 페이지 삭제 전 다른 파일의 참조 여부 검사.
- `delete`: 삭제 시 직접 unlink 대신 `.trash/<page>.<timestamp>.md`로 안전하게 이동(휴지통 보관).
- `restore`: 휴지통에서 삭제된 페이지를 원래 위치로 복원.
- `resolve_block_ref`, `block_ref`, `insert_block_ref`, `block_backlinks` 엔진 연동.

### (3) Win32 네이티브 창 (`outliner_win`)
- **블록 참조 입력 시 자동완성 팝업** (`links.rs`, `edit.rs`):
  - `((` 또는 `(((` 입력 시 문맥 감지 (`block_ref_context`, `any_completion_context`).
  - 현재 문서의 블록 목록에서 검색어 일치 후보 표시 및 선택 시 `((id))` / `(((id)))` 로 완성.
- **역참조 패널 확장** (`find.rs`, `shell.rs`):
  - 페이지 역링크뿐만 아니라 현재 선택된 블록의 역참조(`block_backlinks_all`)를 탐색하여 오른쪽 패널에 함께 표시.
- **페이지 관리 및 실행 취소** (`pages.rs`, `shell.rs`):
  - `check_references`: 삭제 전 참조 검사.
  - `delete_current`: 참조 여부 알림과 함께 `.trash`로 이동(안전 삭제).
  - `restore_last`: 휴지통에서 최근 삭제 파일 복원.
  - `rename_current` & `undo_rename`: 링크 일괄 갱신 및 이름 변경 되돌리기.

### (4) Flutter UI 화면 (`flutter_ui/lib/features/outliner/outliner_workspace.dart`)
- **블록 참조 자동완성 팝업**:
  - `((` 또는 `(((` 입력 시 블록 후보 드롭다운 목록 노출, 선택 시 `((블록 id))` 또는 `(((블록 id)))` 자동완성.
- **역참조 패널 (`_BacklinksPanel`)**:
  - 페이지 역링크와 더불어 현재 선택된 블록을 참조하는 블록 목록(`역참조 (선택 블록)`) 표시.
- **페이지 이름 변경 및 삭제**:
  - 이름 변경 후 링크 갱신 수 표시 및 `[실행 취소]` 스낵바 제공.
  - 페이지 삭제 전 다른 페이지의 참조 여부를 검사하여 참조가 있을 경우 경고 다이얼로그 노출.
  - 삭제 후 `[실행 취소(복원)]` 스낵바로 즉시 복원 가능.

---

## 2. 변경된 파일 목록

1. `outliner_core/src/parse_serialize.rs`: 3중/2중 괄호 블록 참조 파싱 및 공백 트림 처리
2. `outliner_core/src/index.rs`: 숫자 블록 ID 및 UUID 블록 참조 해결, 역참조 통합
3. `outliner_core/src/api.rs`: `insert_block_ref`, `block_backlinks`, `check_page_references` op 추가
4. `outliner_core/src/tests.rs`: 블록 참조 및 페이지 관리 시험, Enter·Tab·Undo·다중선택 회귀 시험 추가
5. `docfit_app/outliner_service.py`: rename 링크 일괄 갱신, undo_rename, check_delete, delete(.trash), restore 구현
6. `tests/test_outliner_service.py`: 페이지 관리 및 블록 참조 연동 단위 시험 추가
7. `outliner_win/src/links.rs`: RefKind 및 `((` / `(((` 블록 참조 문맥·자동완성 추가
8. `outliner_win/src/edit.rs`: 블록 참조 팝업 후보 제공 및 치환 처리
9. `outliner_win/src/pages.rs`: `check_references` 및 `undo_rename` 추가
10. `outliner_win/src/find.rs`: `block_backlinks_all` 추가
11. `outliner_win/src/shell.rs`: 블록 역참조 병합, 페이지 관리 및 되돌리기 연동
12. `outliner_win/src/capture.rs`: OUTLINERWIN5 방식 확장 `capture_ref_set` 하네스 추가
13. `flutter_ui/lib/features/outliner/outliner_workspace.dart`: 블록 참조 후보 팝업, 역참조 패널, 이름 변경/삭제 되돌리기 UI 구현
14. `docs/OUTLINERREF1_PROGRESS.md`: 작업 진행 기록

---

## 3. 검증 및 결과

초기 실행에서 확인된 두 회귀를 바로잡았습니다. 블록 참조 통합 역참조에는 UUID와 숫자 ID로 각각 참조한 블록을 모두 포함하도록 회귀 기대값을 수정했고, 기존 Flutter UI 시험이 찾는 `역링크` 제목을 유지했습니다. 비동기 페이지 삭제 흐름에는 `mounted` 확인을 추가했고, 참조 확인 실패 시 삭제를 진행하지 않도록 했습니다.

### 실제 실행 결과

- `cargo test --release --locked` (`outliner_core`): 라이브러리 **84개 통과**, CLI 통합 **1개 통과**, 실패 0.
- `cargo test --release --locked` (`outliner_win`): **132개 통과**, 실패 0. Enter·Tab/Shift+Tab·Undo·다중 선택 관련 회귀 포함.
- `cargo test --release --locked --test parity` (`outliner_win`): 하네스 5개 통과(닫는 괄호 공백 경계 시험 포함). 동등 시나리오 **42개 차이 0**, 허용 차이 12개는 등록된 기대값대로 고정.
- `flutter analyze`: `No issues found!`.
- `flutter test test/outliner_workspace_test.dart test/outliner_undo_test.dart`: **14개 통과**.
- 전체 Python `python -m unittest discover -s tests`: `Ran 3594 tests in 1438.941s` / `OK (skipped=26)`, 종료 코드 0. 이어서 최신 서비스 수정 상태에서 `python -m unittest tests.test_outliner_service`: `Ran 14 tests` / `OK`.

저장소에 `.venv\Scripts\python.exe`가 없어 시스템 Python 3.14로 전체 시험을 실행했습니다. 전체 시험의 성공 요약 뒤 테스트 패키지 atexit 임시 폴더 정리에서 잠긴 SQLite 파일에 대한 WinError 32가 출력됐습니다(시험 요약은 `OK`, 프로세스 종료 코드 0). 전체 Python 실행은 마지막 서비스 신뢰성 변경 전에 시작됐고, 이 변경 후 서비스 시험 14개를 다시 실행해 통과했습니다. 초기 Flutter 시험에서 발견된 `역링크` 레이블 회귀는 수정 후 재실행해 통과했습니다. `outliner_core` 회귀 시험의 첫 실행에서 드러난 잘못된 기대값 두 곳도 수정하고 전체 Rust 시험을 재실행해 통과했습니다.

### 실행하지 못한 항목

- 한/글 2020 COM 실문서 시험과 네이티브 창의 실제 마우스/IME 확인은 실행하지 않았습니다. 이번 범위의 자동 검증은 하네스와 단위 시험으로 수행했습니다.
- 드래그 이동과 Win32 실행 취소/다시 실행 도구 단추는 범위 밖입니다.

## 4. 후속 작업 설계 메모 (범위 밖 과제)

### (1) 드래그 이동 (Drag & Drop Block Reordering)
- **목표**: 마우스로 블록의 불릿 점(bullet)을 드래그하여 다른 블록의 위/아래 또는 하위 자식으로 이동.
- **엔진 수준 지원**:
  - `outliner_core`의 `ops.rs`에 `move_block(document, source_id, target_id, relation)` 추가 (`relation`: `before`, `after`, `child`).
  - 들여쓰기(`indent`)와 `sequence` 순서를 한 번의 트랜잭션으로 재계산하고 `UndoHistory`에 기록.
- **Win32 창 (`outliner_win`)**:
  - 마우스 `WM_LBUTTONDOWN` 시 불릿 영역(`BULLET_X ~ TEXT_X`) 히트 테스트.
  - 드래그 임계값(4px) 초과 시 드래그 모드 진입, `WM_MOUSEMOVE`에서 삽입 표시선(가로 파란색 선) 그리기.
  - `WM_LBUTTONUP`에서 드롭 위치 계산 후 `move_block` 명령 호출.
- **Flutter (`flutter_ui`)**:
  - `ReorderableListView` 또는 `LongPressDraggable` 위젯 활용.
  - 들여쓰기 단계 유지를 위해 커스텀 드롭 타깃 델리게이트 구현.

### (2) Win32 창 되돌리기/다시 실행 툴바 단추
- **목표**: 상단 툴바 영역에 `[↶ 실행 취소]` 및 `[↷ 다시 실행]` 단추를 배치하여 단축키(Ctrl+Z/Ctrl+Y) 외에도 마우스 클릭으로 이력을 조작할 수 있도록 함.
- **구조 설계**:
  - `outliner_win/src/layout.rs`: `Btn::Undo`, `Btn::Redo` 열거형 항목 추가 및 상단 바 단추 영역(`R`) 계산.
  - `outliner_win/src/paint_shell.rs`: 되돌리기/다시 실행 아이콘 또는 텍스트 렌더링. 비활성화 상태(can_undo == false)에서는 흐린 색(muted)으로 그리기.
  - `outliner_win/src/window.rs`: 단추 클릭 이벤트 수신 시 `shell.editor.history_step(true/false)` 호출 및 화면 다시 그리기.
