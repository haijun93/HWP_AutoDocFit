# OUTLINERQA1 — Logseq 기능 동일성 점검 결과

작성: 2026-11-22 카드 기준 · 조사·시험 실행: 2026-11-23 · 브랜치 `feature/OUTLINERQA1`

## 판정 요약

현재 아웃라이너는 Logseq 기능과 동일하지 않다. Rust 엔진은 블록 편집과 Markdown 메타데이터 처리 기능을 넓게 제공하지만, Flutter 탭과 Win32 창이 그 기능을 전부 연결하지 않는다. 다중 블록 선택·드래그 이동·문서 수준 실행 취소/다시 실행은 세 구현 어디에도 완성된 경로가 없다. `outliner_win.exe` 설치 누락은 이 작업에서 확인하지 않았다. 별도 카드 PKGWIN1의 범위다.

이번 점검에서 Flutter의 작은 입력 유실 결함 한 건을 실패 시험으로 재현하고 고쳤다. 입력 중 Tab/Shift+Tab이 텍스트를 저장하지 않고 들여쓰기만 요청하던 동작을 텍스트 저장 후 들여쓰기 순서로 바꿨다.

알려진 Enter 후 focus가 다음 줄로 이동하지 않던 증상은 현재 Flutter 코드가 새 블록을 추가한 뒤 focus를 옮기며(:315-370), 새 합성 위젯 시험에서도 다음 블록 focus를 확인했다. 이 수정은 v2.2.93에서 반영됐다는 카드 설명과 일치한다.

기준은 Logseq 공개 문서와 공식 저장소의 현재 문서/단축키 정의다. Logseq의 Windows 블록 이동은 `Alt+Shift+↑/↓`이며, `Alt+↑/↓`는 블록 선택이다. 따라서 카드의 “Alt+위/아래 이동” 설명은 Logseq 기본 키와 다르다. 이 표는 정확한 Logseq 동작과 카드에 적힌 기대를 구분한다.

## Logseq 기능 기준표

“확인”은 공개 문서에서 기능 또는 표기를 직접 찾았다는 뜻이다. 공개 문서에서 증거를 찾지 못한 세부 동작은 “미확인”으로 남겼다.

| 기능 | 공개 문서에서 확인한 기준 | 근거 |
|---|---|---|
| 블록 트리 | 들여쓰기가 부모·자식 구조를 나타내는 아웃라이너 | [Logseq Markdown Syntax](https://github.com/logseq/logseq/blob/master/docs/logseq-markdown-syntax.md) |
| 새 블록·새 줄 | Enter는 새 블록, Shift+Enter는 같은 블록의 새 줄 | [Logseq tutorial](https://github.com/logseq/docs/blob/master/pages/tutorial.md) |
| 들여쓰기 | Tab / Shift+Tab | [Logseq tutorial](https://github.com/logseq/docs/blob/master/pages/tutorial.md), [공식 단축키 설정](https://github.com/logseq/logseq/blob/master/src/main/frontend/modules/shortcut/config.cljs) |
| 접기·펴기 | 블록 자식 접기/펴기 단축키가 존재 | [공식 단축키 설정](https://github.com/logseq/logseq/blob/master/src/main/frontend/modules/shortcut/config.cljs) |
| 블록 이동 | Windows `Alt+Shift+↑/↓`; `Alt+↑/↓`는 블록 선택 | [공식 단축키 설정](https://github.com/logseq/logseq/blob/master/src/main/frontend/modules/shortcut/config.cljs) |
| 삭제·병합 | Backspace/Delete 삭제 명령 존재. Backspace가 블록 경계에서 병합되는 세부 규칙은 이번 공개 자료에서 미확인 | [공식 단축키 설정](https://github.com/logseq/logseq/blob/master/src/main/frontend/modules/shortcut/config.cljs) |
| 여러 블록 선택 | 블록 선택, 전체 선택, 선택 삭제 명령 존재 | [공식 단축키 설정](https://github.com/logseq/logseq/blob/master/src/main/frontend/modules/shortcut/config.cljs) |
| 드래그 이동 | 블록 드래그 및 Alt 드래그 블록 참조 방식 확인 | [Block references](https://github.com/logseq/docs/blob/master/pages/The%20basics%20of%20block%20references.md) |
| `[[페이지]]`·자동완성 | `[[`로 페이지를 참조하거나 생성하고 검색에서 페이지/블록을 찾음 | [Logseq tutorial](https://github.com/logseq/docs/blob/master/pages/tutorial.md), [Block references](https://github.com/logseq/docs/blob/master/pages/The%20basics%20of%20block%20references.md) |
| `#태그` | 블록/페이지 분류에 태그 사용 | [Logseq tutorial](https://github.com/logseq/docs/blob/master/pages/tutorial.md), [Logseq Markdown](https://github.com/logseq/docs/blob/master/pages/Markdown.md) |
| `((블록 참조))` | `((id))` 형식으로 블록 참조, 검색 및 참조 열기 | [Logseq tutorial](https://github.com/logseq/docs/blob/master/pages/tutorial.md), [Block references](https://github.com/logseq/docs/blob/master/pages/The%20basics%20of%20block%20references.md) |
| 역링크 | Linked References 및 Unlinked References 표시 | [Logseq tutorial](https://github.com/logseq/docs/blob/master/pages/tutorial.md), [DB feature notes](https://github.com/logseq/docs/blob/master/db-version.md) |
| Journals·오늘 | 오늘 일지 자동 생성·Journals 화면에서 접근 | [DB feature notes](https://github.com/logseq/docs/blob/master/db-version.md) |
| 페이지 생성 | `[[페이지]]` 또는 검색에서 생성 | [Logseq tutorial](https://github.com/logseq/docs/blob/master/pages/tutorial.md) |
| 페이지 이름 변경·삭제 | 이번 조사에서 현재 공식 공개 문서의 구체 조작 규칙 미확인 | 미확인 |
| 검색 | 검색창에서 페이지와 블록 검색 | [Block references](https://github.com/logseq/docs/blob/master/pages/The%20basics%20of%20block%20references.md) |
| 속성 | Markdown `key:: value` 표기 확인 | [Logseq Markdown Syntax](https://github.com/logseq/logseq/blob/master/docs/logseq-markdown-syntax.md) |
| 할 일 | 파일 그래프의 `TODO`/`DONE` 표기와 상태 순환 명령 확인. 기본 워크플로는 설정에 따라 다를 수 있음 | [Tasks](https://github.com/logseq/docs/blob/master/pages/Tasks.md), [Logseq Markdown Syntax](https://github.com/logseq/logseq/blob/master/docs/logseq-markdown-syntax.md) |
| Markdown 서식 | 굵게·기울임·취소선·코드·하이라이트 등 표기 지원 | [Logseq Markdown](https://github.com/logseq/docs/blob/master/pages/Markdown.md) |
| 실행 취소·다시 실행 | Undo/Redo 명령 및 단축키 정의 확인 | [공식 단축키 설정](https://github.com/logseq/logseq/blob/master/src/main/frontend/modules/shortcut/config.cljs) |
| 파일 기반 저장 | Markdown 파일 그래프와 Markdown mirror 문법 확인. DB 그래프는 저장 방식이 다름 | [Logseq tutorial](https://github.com/logseq/docs/blob/master/pages/tutorial.md), [Logseq Markdown Syntax](https://github.com/logseq/logseq/blob/master/docs/logseq-markdown-syntax.md) |

## 우리 구현 대조표

지원 판정은 화면에서 사용 가능한가를 기준으로 한다. 엔진 전용 API는 엔진 열에만 지원으로 표기한다. Flutter는 `flutter_ui/lib/features/outliner/outliner_workspace.dart`, Win32는 `outliner_win/src/`, 엔진은 `outliner_core/src/` 기준이다.

| 기능 | Flutter 탭 | Win32 창 | Rust 엔진 | 코드·시험 근거 |
|---|---|---|---|---|
| 블록 트리 | 부분 | 부분 | 부분 | Flutter는 평면 리스트에 indent 여백 및 접힌 하위 필터(:62-73, :565-570). Win32는 가시 행 목록/깊이 계산(`view.rs:24-80`). 엔진은 줄별 indent 모델(`parse_serialize.rs:72-99`). 계층 연산·부모 경계는 세 구현 모두 Markdown 들여쓰기 기반이지 별도 트리 객체가 아님. |
| Enter 새 블록 | 지원 | 지원 | 지원 | Flutter 저장→insert 및 다음 블록 focus(:341-370; 위젯 시험 `outliner_workspace_test.dart:270-335`). Win32 커서 분할(`edit.rs:727-741`). 엔진 grapheme offset 분할(`ops.rs:114-153`). |
| Tab/Shift+Tab | 지원, 이번 수정 | 지원 | 지원 | Flutter 키 핸들러(:619-627), 저장 후 연산(`:384-415`), 회귀 시험(:235-246). Win32(`edit.rs:410-412, 718-725`). 엔진(`ops.rs:76-88`). |
| 접기/펴기 | 부분 | 지원 | 부분 | Flutter는 현재 화면 상태에서만 접고 편집 요청 후 문서를 재파싱하므로 상태가 유지되는지 의심(:62-73, :295-312). Win32 세션 접힘 및 `collapsed:: true`(`view.rs:24-46`, `edit.rs:805-820`). 엔진 편집 API 접힘 flag만 제공하고 Markdown 영구 저장하지 않음(`ops.rs:90-91`). |
| 블록 이동 | 부분 | 부분 | 부분 | Flutter Alt+↑/↓가 한 줄 up/down(:645-657). Win32 Alt+↑/↓ 편집 중 이동이며 하위 포함 형제 이동(`edit.rs:117-120, 686-698`); Logseq 기본 키와는 Alt+Shift 차이. 엔진 up/down은 인접 블록 교환(`ops.rs:94-112`). |
| 삭제·Backspace 병합 | 부분 | 지원 | 지원 | Flutter 빈 칸 삭제와 맨 앞 병합(:659-703), 병합은 공백 없이 텍스트 연결, 마지막 블록 삭제도 허용해 빈 문서 상태가 가능함. Win32 마지막 블록 보호·다음 블록 병합(`edit.rs:742-785`). 엔진 delete/merge(`ops.rs:71-74, 155-176`). |
| 여러 블록 선택 | 미지원 | 미지원 | 미지원 | Flutter/Win32 화면 선택은 한 블록 단위(`layout.rs` 단일 selected index 및 Flutter block field). 엔진 API는 block_id 하나를 받음(`api.rs:104-122`). |
| 드래그 이동 | 미지원 | 미지원 | 미지원 | 두 UI 모두 블록 이동 명령만 있고 드래그 reorder 경로/엔진 범위 연산 없음. |
| `[[페이지]]` 링크 | 부분 | 부분 | 부분 | Flutter 링크 입력 후보·클릭 이동(:713-821), 링크 칩 중심. Win32 링크 강조·클릭 및 후보 팝업(`links.rs`, `edit.rs:567-630`). 엔진 추출·rename 갱신(`parse_serialize.rs:11-70`, `rename.rs:72-230`). |
| 링크 자동완성 | 부분 | 지원 | 미지원 | Flutter는 `[[` 입력 감지 후 최대 6개 페이지 이름 표시(:713-802), 후보 키보드 선택 미확인. Win32 후보 검색·키 이동·확정(`edit.rs:567-630`; `find_links.rs` 시험). 엔진은 autocomplete API 없음. |
| `#태그` | 부분 | 부분 | 지원 | Flutter 원문 입력/보존은 가능하나 전용 태그 UI·필터 없음. Win32 태그 서식 렌더링은 있으나 태그 관리/필터 UI 없음. 엔진 파싱·인덱스(`parse_serialize.rs:11-70`, `index.rs:25-47`). |
| `((블록 참조))` | 미지원 | 미지원 | 지원 | 엔진 추출·해결·역참조(`parse_serialize.rs:11-70`, `index.rs:59-87`, `api.rs:81-103`). 두 화면에서 블록 참조 검색·삽입·점프 UI 없음. |
| 역링크 | 부분 | 지원 | 부분 | Flutter는 현재 페이지 문서 안에서 현재 페이지를 대상으로만 조회(:850-893). Win32는 데이터 폴더의 다른 페이지도 묶어서 제공(`find.rs:79-120`). 엔진 backlinks API는 전달한 문서 한 개만 색인(`index.rs:25-56`). |
| Journals | 부분 | 부분 | 지원 | Flutter 오늘 일지 생성/열기만(:165-177). Win32 오늘 일지와 날짜 이름 변환(`pages.rs:230-257`). 엔진 날짜 이름·목록 API(`api.rs:28-49`). 어제/내일 UI는 없음. |
| 오늘의 일지 | 지원 | 지원 | 지원 | Flutter “오늘의 일지”(:165-177, :453-480), Win32 `journal_today`(`pages.rs:230-257`), 엔진 `daily_page`(`api.rs:28-30`). |
| 페이지 생성/이름 바꾸기/삭제 | 부분 | 지원 | 부분 | Flutter 세 동작 UI(:195-227 및 :475-545); 이름 바꾸기 시 링크 갱신 없음, 삭제는 복구 불가(`outliner_service.py:78-101`). Win32 생성·이름 변경 시 링크 갱신·휴지통 이동(`pages.rs:146-188`, 시험 `pages_store.rs`). 엔진 rename links는 있으나 전체 페이지 lifecycle API는 없음(`rename.rs:263-` 이하). |
| 검색 | 부분 | 부분 | 지원 | Flutter 현재 페이지 블록만 검색하고 `_blocks`를 결과만 남기도록 바꿔 해제 경로가 없음(:417-430). Win32 현재 페이지 포함 모든 페이지 본문 검색(`find.rs:42-77`). 엔진 로컬 검색 및 다중 페이지 query(`search.rs:8-55`). |
| 속성 `key:: value` | 미지원 UI | 부분 | 부분 | Flutter 입력/보존만 되고 속성 표시·편집·검색 UI 없음. Win32 원문 편집은 가능, 속성 관리 UI 미확인. 엔진 속성 추출 및 `prop:key=value` 질의(`parse_serialize.rs:11-70`, `search.rs:8-44`). |
| TODO/DONE | 부분 | 부분 | 부분 | Flutter 완료 토글(:733-751), 엔진 TODO/DONE 순환(`ops.rs:178-199`). Win32 체크박스는 상태 있는 블록만 직접 표시 가능; 미표시 블록은 Ctrl+Enter 토글(`edit.rs:460-462`, `window.rs` 체크 클릭 처리). Logseq의 LATER/NOW/DOING 등 상태 흐름과 동일하지 않음. |
| Markdown 서식 | 부분 | 부분 | 부분 | 세 구현은 Markdown 문법 텍스트를 보존/편집하지만 전용 서식 도구·리치 렌더링을 제공하지 않음. 엔진은 비불릿 줄을 보존(`parse_serialize.rs:101-175`). |
| 실행 취소/다시 실행 | 미지원 | 부분 | 미지원 | Flutter 문서 모델 되돌리기 API 없음; 활성 TextField 안의 로컬 편집 취소만 별개. Win32 EDIT 컨트롤 안의 미확정 입력에 한정될 수 있고 문서 전체 undo history 없음. 엔진에 undo/redo 연산 없음. |
| Markdown 파일 저장 | 지원 | 지원 | 지원 | Flutter 서비스는 `.md` 페이지를 읽고 SHA-256 충돌 확인 후 저장(`outliner_service.py:108-145`, `store.rs:24-72`). Win32 PageStore 기반 파일 저장 및 외부 변경 경고(`pages.rs`, 시험 `save_io.rs`). 엔진 원자 저장/충돌 API(`store.rs:24-125`). |
| 한글 IME | 동작 의심 | 동작 의심 | 미지원 범위 | Flutter 위젯 시험은 완성된 한글 문자열만 넣고 실제 조합 이벤트를 재현하지 않음. Win32 상태 머신 시험은 가짜 호스트; 실 IME는 별도 사용자 실기 필요(`edit.rs:419-565`, `edit_machine.rs`). 엔진은 이미 확정된 UTF-8 문자열 연산만 함. |

### 기능별 시험 자산

| 기능 | 시험 파일/시험 이름 | 확인 범위 |
|---|---|---|
| 트리·접기·펴기 | `outliner_win/tests/layout_and_capture.rs` `folded_children_are_hidden_and_unfold_restores`; Flutter `outliner_workspace_test.dart` 키보드 시나리오 | 가시 행/접힘을 확인. 실제 시각적 계층 조작은 사용자 실기 미확인. |
| Enter·Tab·Shift+Tab | `outliner_win/tests/edit_machine.rs` `enter_splits_at_caret_and_moves_edit_to_new_block`, `tab_and_shift_tab_indent_outdent_and_keep_text`; Flutter `outliner_workspace_test.dart` 키보드 시나리오 및 한글 Enter 시험 | 가짜 Win32 입력 호스트와 Flutter 모의 API의 명령 순서/포커스. 실 IME 아님. |
| 위/아래 이동·삭제·병합 | `outliner_win/tests/edit_machine.rs` `backspace_at_start_merges_with_previous_block`, `backspace_on_empty_block_deletes_and_edits_previous_at_end`; `shell_flow.rs` subtree 이동 시험; `outliner_core/src/tests.rs` 합성 한글 시나리오 | 엔진 편집과 Win32 상태 머신; Flutter 시험은 한 줄 up/down 및 Backspace 요청. |
| 페이지 링크·자동완성 | `outliner_win/tests/find_links.rs`, `shell_flow.rs` `typing_double_bracket_opens_popup_and_filters_as_you_type`, `shell_window.rs` `clicking_a_popup_candidate_inserts_that_page_link` | Win32 후보 필터/확정/링크 이동. Flutter 자동완성은 코드에서 확인했으며 별도 위젯 시험은 없다. |
| 태그·속성·블록 참조 | `outliner_core/src/tests.rs` `tags_are_extracted`, `properties_are_extracted`, `block_references_are_extracted`, `resolve_block_ref_finds_block_by_id_property_and_returns_backlinks`, 합성 한글 시나리오 | 엔진 파싱·색인·질의. 두 UI의 편집·탐색은 미지원이라 화면 시험 없음. |
| 역링크·검색 | `outliner_win/tests/find_links.rs` `backlinks_all_collects_blocks_from_several_pages_excluding_the_page_itself`, `search_all_finds_page_names_then_blocks_across_files`; Flutter `outliner_workspace_test.dart` 역링크 패널 렌더; Rust core search/query 시험 | Win32 전역 범위, Flutter 현재 화면 패널 존재, 엔진 로컬 문서 및 API query. |
| Journals·페이지 관리 | `outliner_win/tests/pages_store.rs` `journal_today_creates_underscore_name_and_converts_flutter_dash_name`, `rename_moves_file_and_updates_links_in_all_pages_but_not_code_or_other_names`, `delete_moves_to_trash_and_restore_brings_it_back`; core journal 시험 | Win32 파일 동작/엔진 이름 계산. Flutter 시험은 오늘 일지 및 페이지 관리 컨트롤 표시를 확인하고 lifecycle 호출 흐름을 모두 누르지는 않는다. |
| TODO/DONE·Markdown 보존 | `outliner_core/src/tests.rs` `todo_status_toggles_through_done_and_back_to_todo`, `round_trip_preserves_unknown_markdown_lines`, `round_trip_preserves_korean_text`; `outliner_win/tests/shell_window.rs` checkbox 시험 | 엔진 상태/왕복 및 Win32 체크 입력; 렌더링 서식 동등성은 검증하지 않음. |
| 파일 저장·충돌 | `outliner_win/tests/save_io.rs`; `outliner_core/src/tests.rs` atomic/checked save 시험; `tests/test_outliner_service.py` 9개 | 저장·충돌·서비스 계약. 합성 경로/파일만 사용. |
| 다중 선택·드래그·undo/redo | 해당 기능 시험 없음 | 현재 코드에서 명령/API/UI 경로를 찾지 못해 미지원으로 판정. |
| 실제 한글 IME | `outliner_win/tests/edit_machine.rs`의 `composing_*` 시험 | 메시지 순서를 가짜 호스트로 검사한 것뿐이다. 실제 IME 확인으로 간주하지 않는다. |

## 합성 동작 시험

- `outliner_core/src/tests.rs:505-552`에 한글 문서로 파싱→메타데이터/역링크→텍스트 입력→커서 위치 분할→Tab/Shift+Tab→접기/펴기→위/아래 이동→삭제→병합→검색/속성 질의→직렬화 연속 시나리오를 추가했다.
- `flutter_ui/test/outliner_workspace_test.dart:190-268`의 기존 위젯 시험은 입력→Enter와 focus→Tab/Shift+Tab→Alt+위/아래→접기/펴기→Backspace 병합/삭제 경로를 확인한다. `:277-337`에는 한글 입력 후 Enter의 API 요청 순서와 새 블록 focus를 확인하는 합성 시험을 추가했다.
- 이 시험은 Flutter UI가 보내는 명령을 모의 API로 확인한다. Python 브리지와 실 IME, 렌더링된 사용자 화면까지 증명하지는 않는다.
- Win32의 기존 자동 시험(`outliner_win/tests/edit_machine.rs`, `shell_flow.rs`, `shell_window.rs`)은 키 상태, 블록 편집, 검색·링크·페이지 흐름을 폭넓게 다룬다. 엔진 parity 시험(`parity.rs`)은 기존 설계에서 허용한 Flutter/Win32 차이를 고정한다. 이번 카드의 새 테스트 변경은 Rust 코드를 바꾸지 않아 Win32 시험 자산을 재실행했다.

## 결함·수정

### 수정: Tab/Shift+Tab 입력 글자 유실

- 재현: Flutter 블록 편집 칸에 `두 번째 한글 수정` 입력 → Tab 또는 Shift+Tab.
- 기대: 입력 문자를 저장한 뒤 블록 indent를 바꾼다.
- 기존 실제: 키 핸들러가 `_edit(indent/outdent)`만 호출했고, 이 경로는 현재 TextField 값을 보내지 않아 마지막 저장 이후 입력이 Markdown에서 빠질 수 있었다.
- 실패 시험: `flutter test --no-pub --plain-name "keyboard shortcuts edit outline blocks" test/outliner_workspace_test.dart`에서 `set_text:2` 요청이 없다는 검증 실패(Actual `-1`).
- 수정: `outliner_workspace.dart:619-627`에서 `_setTextAndEdit(block, operation, value)` 호출. TextField 글자를 먼저 `set_text`로 엔진에 반영한 다음 indent/outdent를 수행한다.
- 회귀 시험: 같은 키보드 시나리오에서 `set_text:2`가 indent와 outdent보다 앞서는지 확인한다.

### 남은 결함 및 카드 후보

1. **기능 동일성 격차(큰 작업)**: 다중 선택, 드래그 재정렬, 문서 단위 undo/redo, `((block ref))` UI, 속성 UI, 인라인 서식 UI를 추가해야 한다. Flutter와 Win32 양쪽에 별도 작업이 필요하다.
2. **검색 UX(큰 작업)**: Flutter 검색은 결과를 `_blocks`에 덮어써 원 목록 복구 수단이 없다. 페이지·블록 전역 검색과 검색 종료 시 복원 경로를 구현해야 한다.
3. **역링크 범위(중간 작업)**: Flutter 탭은 현재 페이지 문서만 조회한다. 다른 페이지를 포함한 전역 linked references가 아니다.
4. **TODO 상태(중간 작업)**: 구현 상태는 TODO/DONE 두 종류다. Logseq 설정 가능한 작업 상태 흐름과 속성/우선순위 관리에 미달한다.
5. **키 바인딩 차이(제품 결정 필요)**: Win32는 Alt+↑/↓로 이동하지만 Logseq 공식 기본값은 Alt+Shift+↑/↓ 이동, Alt+↑/↓ 선택이다. Logseq와 동일한 기본 키로 맞출지, 이 앱의 단순 이동 키를 유지할지 사용자 사용성 검토가 필요하다.
6. **블록 의미 차이**: 엔진/Flutter 이동은 인접 줄 단위, Win32는 하위 블록을 동반하는 형제 이동이다. Win32가 Logseq 트리 이동에 가깝지만 Flutter와 의미가 다르다.
7. **페이지 lifecycle 차이**: Flutter 이름 변경은 참조 갱신이 없고 삭제는 복구 불가. Win32는 참조 갱신 및 휴지통을 사용한다. 이중 화면 사용 시 저장 데이터 동작이 다르다.
8. **ID 안정성**: 엔진은 Markdown 재파싱 때 줄 순서로 블록 ID를 다시 부여한다(`parse_serialize.rs:101-131`). Logseq의 지속 블록 정체성과 동일하지 않고, 외부 블록 참조의 신뢰성에 제약이 있다.
9. **공개 문서에서 미확인**: Logseq 페이지 이름 변경/삭제 세부 동작, Backspace 경계 병합의 정확한 간격/계층 규칙은 이번 자료 조사에서 확정하지 못했다.

## 사용자 실기에서만 확인할 사항

- 실제 Microsoft 한글 IME 조합 중 Enter/Tab/Shift+Tab/Backspace/방향키와 링크 자동완성 후보창 키가 글자를 잃거나 두 번 실행하지 않는지.
- Win32의 한글 조합 확정 동작은 `edit_machine`의 합성 이벤트 시험만으로 검증되지 않는다. IME 종류·TSF 메시지 순서별 확인도 미실시다.
- Win32 설치 실행 파일이 설치본에 포함되는지 및 v2.2.91 패키지 증상 재현은 PKGWIN1이 담당하므로 여기서는 검사하지 않았다.
- 실제 사용자가 말한 “제대로 작동하지 않음”의 미확인 증상은 이 카드만으로 원인 귀속할 수 없다.

## 시험 결과

- `outliner_core`: `cargo test --release --locked` 통과. 출력된 모든 `test result` 줄:

  ```text
  test result: ok. 70 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.53s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.49s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```

- `outliner_win`: `cargo test --release --locked` 통과. 출력된 모든 `test result` 줄:

  ```text
  test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
  test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s
  test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s
  test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.52s
  test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s
  test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
  test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
  test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.72s
  test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.41s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```

- `flutter analyze`: 통과(`No issues found`).
- `flutter test --no-pub test/outliner_workspace_test.dart`: 6개 시험 통과.
- Python: `.venv` 디렉터리가 없는 환경이라 `python -m unittest tests.test_outliner_service`로 대체 실행, `Ran 9 tests`, `OK`. 한/글 COM 시험은 실행하지 않았다.
- 초기 작성 과정의 합성 Rust 시나리오 시험은 테스트 fixture의 계층 깊이와 병합 공백 기대값이 잘못돼 두 번 실패했다. 제품 코드 결함은 아니었고 기대값을 바로잡은 뒤 최종 전체 실행에서 통과했다.
