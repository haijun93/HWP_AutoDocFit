# OUTLINEKEY1 설계 — 아웃라이너 편집 동작을 Logseq 블록 편집기에 맞춘다 (2026-11-26)

## 0. 근거

Logseq 데스크톱의 **파일 그래프 판 0.10.9 공개 소스를 읽고 동작만 확인**했다(AGPL — 코드는 복사하지 않았다. 규칙은 새로 썼다).
`https://github.com/logseq/logseq/blob/0.10.9/` 아래:

| 약칭 | 파일 |
|---|---|
| [E] | `src/main/frontend/handler/editor.cljs` — `keydown-new-block`, `insert-new-block!`, `compute-fst-snd-block-text`, `outliner-insert-block!`, `keydown-backspace-handler`, `delete-block!`, `keydown-delete-handler`, `delete-concat`, `keydown-up-down-handler`, `keydown-arrow-handler`, `shortcut-select-up-down`, `select-block-up-down`, `escape-editing`, `select-parent`, `cycle-todo!`, `autopair`, `keydown-not-matched-handler`, `paste-blocks` |
| [K] | `src/main/frontend/modules/shortcut/config.cljs` — 키 배치 |
| [O] | `src/main/frontend/modules/outliner/core.cljs` — `indent-outdent-blocks`, `move-blocks-up-down`, 트리 `-del` |
| [P] | `src/main/frontend/handler/paste.cljs` — `paste-copied-text`, `paste-segmented-text` |
| [M] | `src/main/frontend/util/marker.cljs` — `cycle-marker-state` |

실제 Logseq 앱을 띄워 눌러 본 확인은 하지 않았다. 아래 "확인" 열의 `[E]` 등은 소스 근거이고, `PM 가정`은 확인 못 한 것이다.

## 1. 동작 표

"현재" = 이 카드 전(Flutter `outliner_workspace.dart` / 네이티브 `outliner_win/src/edit.rs`).

| # | 상황 | Logseq 동작 (확인) | 현재 동작 | 바꿀 동작 |
|---|---|---|---|---|
| 1 | 빈 블록에서 Enter | 내용이 비고 **다음 형제가 없고** 부모가 블록이면 한 단계 내어쓰기. 최상위이거나 다음 형제가 있으면 새 블록 [E `keydown-new-block`] | 항상 같은 단계 새 블록 | Logseq 와 같게. ※ 카드 요약 "빈 블록 Enter = 내어쓰기"보다 좁다: 다음 형제가 있는 빈 블록은 새 블록(Logseq 확인 결과를 따름) |
| 2 | 자식이 있고 펼친 블록 끝에서 Enter | 새 블록 = 첫 자식 [E `outliner-insert-block!`] | 같은 단계에 삽입(자식을 빼앗음) | 첫 자식 |
| 3 | 접힌 블록 끝에서 Enter | 새 블록 = 하위 전체 뒤의 형제 [E] | 하위 앞에 삽입 | 하위 뒤 형제 |
| 4 | 중간 Enter | 앞부분이 원래 블록(자식·속성·id 유지), 뒷부분(앞 공백 제거)이 새 블록. 위치는 2·3 규칙. TODO 표식은 원래 블록에만 [E] | 같은 단계 분할, 네이티브만 구현(Flutter 는 분할 안 함) | Logseq 와 같게 |
| 5 | 맨 앞 Enter(뒤에 글 있음) | 위에 빈 형제 블록, 커서는 그 빈 블록 [E `insert-new-block-before-block-aux!`] | 아래에 새 블록(Flutter) / 빈 앞+글 분할(네이티브) | 위에 빈 블록 |
| 6 | `[[이름]]`·`((id))` 안에서 Enter | 링크를 연다(새 블록 아님) [E `thing-at-point`] | 새 블록 | 분할하지 않고 닫는 괄호 뒤로 커서(링크 열기는 기존 Ctrl+클릭·칩 — **차이**) |
| 7 | Shift+Enter | 블록 안 줄바꿈 [K `:editor/new-line`] | Flutter 새 블록, 네이티브 무시 | 블록 안 줄바꿈(파일 형식 확장 §3) |
| 8 | 맨 앞 Backspace | 앞의 **보이는** 블록과 글 이어붙임(공백 없음), 커서는 이음매. 자식은 앞 형제로 옮김. **자식이 있고** 왼쪽(앞 형제 또는 부모)도 자식이 있으면 아무 일 없음. 문서 첫 블록은 비어 있을 때만 삭제, 마지막 한 블록은 유지. 지워지는 블록이 참조되면 그 id 유지 [E `delete-block!`, O `-del`] | Flutter 직전 블록(접힘 무시)과 합침·빈 블록은 자식 있어도 삭제, 네이티브 공백 1칸 | Logseq 와 같게. 두 블록 사이에 **빈 줄이 아닌 보존 줄**(제목 등)이 있으면 합치지 않음(파일 보존) |
| 9 | 끝에서 Delete | 다음 블록 = 접혔거나 자식 없으면 **다음 형제**, 아니면 첫 자식. 접힘+다음 형제에 자식, 또는 첫 자식에 자식이 있으면 아무 일 없음 [E `delete-concat`] | 미구현 | Logseq 와 같게(8과 같은 보존 줄 규칙·id 이어받기) |
| 10 | Tab | 앞 형제의 마지막 자식이 됨(자식 동반), 앞 형제가 접혔으면 펼침, 커서 유지. 앞 형제가 없으면 아무 일 없음 [O `indent-outdent-blocks`] | 그 블록 한 줄만 +1(자식 안 따라감) | Logseq 와 같게(네이티브는 안내 문구만 표시) |
| 11 | Shift+Tab | 기본값은 "직접 내어쓰기"(뒤 형제가 자식이 됨), 설정 `logical-outdenting?` 이면 **논리적 내어쓰기**: 부모의 하위 전체 뒤 형제로 이동, 뒤 형제는 부모에 남음 [O] | 한 줄만 -1 | **논리적 내어쓰기**(카드 지시. Logseq 의 선택 설정과 같다) |
| 12 | ↑ / ↓ | 블록 첫(끝) 줄이면 앞(뒤) 보이는 블록으로, 열 위치 유지. 더 없으면 맨 앞(끝)으로 [E `keydown-up-down-handler`] | Flutter 없음 / 네이티브 항상 이동, 커서 끝 | Logseq 와 같게 |
| 13 | 맨 앞 ← / 맨 끝 → | 앞 블록 끝 / 다음 블록 앞 [E `keydown-arrow-handler`] | 없음 | 같게 |
| 14 | Alt+Shift+↑/↓ (mac ⌘⇧) | 형제와 자리바꿈(자식 동반). 첫 자식이 위로 → 부모의 앞 형제의 마지막 자식. 마지막 자식이 아래로 → 부모의 다음 형제의 첫 자식 [K, O `move-blocks-up-down`] | 형제 안에서만 / Flutter 선택이 있어야 | 같게 |
| 15 | 접기·펼치기 | Ctrl+↑ 접기, Ctrl+↓ 펼치기, Ctrl+; 토글 [K] | Ctrl+Enter(Flutter) | Ctrl+↑/↓/; (Ctrl+Enter 는 TODO 로) |
| 16 | Ctrl+Enter | TODO 순환: 없음 → TODO → DOING → DONE → 없음(`:todo` 방식) [M] | Flutter 접기, 네이티브 TODO↔DONE | 없음 → TODO → DONE → 없음. ※ DOING·NOW·LATER 는 현재 파일 파서가 상태로 다루지 않아 **제외(차이)**. 체크 칸 클릭은 TODO↔DONE 그대로 |
| 17 | 편집 중 Esc | 저장하고 그 블록 선택 [E `escape-editing`] | Flutter 선택 해제, 네이티브 **입력 버림** | 글 확정 + 블록 선택(데이터 손실 없음) |
| 18 | 블록 선택 확장 | 편집 중 Alt+↑/↓ = 현재 블록 선택. Shift+↑/↓ 는 첫/끝 줄이면 블록 선택, 아니면 글 선택. 선택 상태에서 Alt/Shift+↑/↓ 로 늘리고 반대 방향이면 줄임 [E] | Alt·Shift+↑ 가 바로 두 블록 선택 | 같게. 편집 중 Ctrl+A 는 전체 글 → 한 번 더 누르면 블록 선택. Ctrl+Shift+A 전체 블록 |
| 19 | 선택 상태의 키 | ↑↓ 한 블록 선택 이동, Enter 편집(끝), Esc 해제, Delete/Backspace 하위까지 삭제, Tab/Shift+Tab, Ctrl+C/X/V(계층 유지) [E, K] | 선택한 줄만(자식 제외) | 하위 포함. 복사 글은 `- ` 마크다운(상대 들여쓰기, `id::` 속성 제외) |
| 20 | 여러 줄 붙여넣기 | `- ` 목록이면 블록 트리. 빈 줄로 나뉜 글은 문단별 블록. **목록이 아닌 한 줄바꿈 글은 한 블록 안 줄바꿈** [P] | Flutter 줄이 칸에 섞임, 네이티브 공백으로 | 여러 줄 = 블록 트리(들여쓰기→계층). ※ 목록이 아닌 글도 줄마다 블록(카드 지시, **Logseq 와 차이**). 한 블록으로 넣기 = Ctrl+Shift+V(Logseq 와 같은 키). 빈 블록에 붙이면 그 블록을 대체 |
| 21 | `[` `(` `{` `` ` `` 입력 | 짝 자동 입력, 닫는 글자 앞에서 같은 글자를 치면 넘어감, 빈 짝 안 Backspace 는 둘 다 삭제. `(` 는 글 처음·공백·줄바꿈·`]`·`(` 뒤에서만 [E `autopair*`] | 없음 | 같게. ※ Logseq 의 `~ * _ ^ = / +` 짝·선택 감싸기는 공문 입력(`3~5` 등)에 방해되어 **제외(차이)** |
| 22 | `[[` `((` 자동완성 | Enter 선택, ↑↓ 이동, Esc 닫기 [E] | Flutter 마우스만 | Flutter 도 키보드. 짝이 있어도 치환 정확 |
| 23 | 다른 블록을 마우스로 클릭 | 벗어난 블록 저장 [E] (blur 저장은 PM 가정 포함) | Flutter 는 확정 안 한 글이 되돌려질 수 있음 | 벗어날 때 글 확정 |
| 24 | 실행 취소 단위 | 명령(트랜잭션) 단위. Tab 은 글 저장과 이동이 따로 두 단계 [E] | 명령 하나(글 확정 포함) 한 단계(OUTLINERUNDO1) | **현재 유지**(명령 하나 = 한 단계, Logseq 와 일부 차이) |
| 25 | 한글 IME 조합 중 Enter/Tab 등 | (PM 가정) 조합 확정 후 동작 | 조합 확정 후 실행 | 유지 |

## 2. 구조 — 규칙은 `outliner_core` 한 곳

- 새 모듈 `outliner_core/src/keys.rs`: 순수 함수 `apply_key(문서, 입력) → 결과`.
  - 입력: 동작 이름, 편집 중 블록(문서 순서 번호)·커서(UTF-16 시작·끝)·편집 칸의 현재 글, 선택한 블록 번호들, 접힌 블록 번호들, 붙여넣을 글.
  - 결과: 새 문서(마크다운을 다시 파싱한 정규 문서), 처리 여부, 변경 여부, 새 커서, 새 선택, 접힌 번호, 안내 문구, 클립보드 글.
- 엔진 JSON: `op: "key"`. Flutter 는 Python 서비스 경유로, 네이티브는 라이브러리 직접 호출로 **같은 함수**를 쓴다.
- 글자 입력(괄호 짝)은 키마다 프로세스를 띄울 수 없어 Flutter 에 Dart 판을 두고, **같은 시험 벡터 파일**
  `outliner_core/testdata/typing_rules.json` 을 Rust·Dart 시험이 함께 돌려 두 구현이 같음을 고정한다.
  ↑↓ 의 "첫/끝 줄" 판정은 화면(줄바꿈) 사정이라 각 화면이 하고, 어느 블록·어느 열로 갈지는 core 가 정한다.

## 3. 파일 형식 확장 — 블록 안 줄바꿈 (Logseq 와 같은 "이어지는 줄")

```
- 첫 줄
  둘째 줄            ← 불릿의 들여쓰기 + 공백 2칸 = 같은 블록의 다음 줄
  id:: 1234          ← 속성 줄(`키:: 값`): 화면에는 안 보이고 블록을 따라다닌다
 - 자식
```

- 읽기: 불릿 **바로 뒤에 이어지는** 줄 중 `불릿 앞 공백 + 2칸`으로 시작하고 불릿(`- `)이 아닌 줄만 흡수한다. 중간에 다른 줄이 끼면 그 뒤는 지금처럼 보존 줄이다.
- 속성 줄(`^[글자·숫자·_·-·.]+::( |$)`)은 `Block.extra` 에 원문 그대로 두고 `properties` 에 읽어 넣는다. 지금까지는 보존 줄이라 이동·삭제 때 제자리에 남아 **다른 블록에 id 가 붙을 수 있었는데**, 이제 블록과 함께 움직인다.
- 쓰기: 이어지는 줄은 `들여쓰기 + 2칸` 으로 쓴다. 줄 맨 앞(공백 뒤)이 `- ` 인 글은 자식 불릿으로 오해되지 않게 `\- ` 로 적고 읽을 때 되돌린다.
- 편집하지 않은 문서는 지금처럼 **바이트 단위로 그대로** 저장한다. CRLF·BOM 없는 UTF-8 유지.
- 영향: 이미 `불릿 + 2칸` 들여쓴 일반 줄이 있는 문서는 그 줄이 블록의 글로 보인다(Logseq 에서도 같은 뜻). 편집 후 저장할 때 탭 들여쓰기는 지금처럼 공백으로 바뀐다.
- 사용자가 Shift+Enter 뒤에 `이름:: 값` 꼴의 줄을 쓰면 속성으로 숨는다(Logseq 와 같음).

## 4. 하지 않는 것

DOING/NOW/LATER 상태, 블록 확대(zoom), 슬래시 명령, 마우스 드래그 선택, `* _ ~` 짝, 직접 내어쓰기 설정, 접힘의 파일 저장(`collapsed::` 쓰기).
