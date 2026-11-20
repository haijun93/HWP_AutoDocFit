# OUTLINERWIN1 설계 — 아웃라이너 Rust + Win32 전환 1단계

작성일: 2026-11-20 · 브랜치 `feature/OUTLINERWIN` · 상태: **설계 문서(코드·설정 변경 없음)**

근거 문서: `TODO.md` 103행(아웃라이너 100% 전환 항목), [OUTLINER 설계](OUTLINER_DESIGN_2026-11-18.md), [인벤토리](RUST_MIGRATION_INVENTORY_2026-11-19.md) 14번 행(110행), [DOCK 설계](DOCK_DESIGN_2026-11-18.md).
표기 규칙: 근거는 `파일:줄`. 읽거나 실행해 확인하지 못한 것은 **미확인(U번호)** 으로 적고 9절에 모았다. 추정치는 "추정"이라 적고 근거를 붙였다.

## 0. 요약과 결론

| 항목 | 결론 |
|---|---|
| 권장 창 구조 | **A안: 블록 목록을 직접 그리고(가상 스크롤), 편집 중인 블록 하나에만 EDIT 자식 창을 겹친다.** |
| Python 잔류 사유 | **없음.** 엔진은 이미 Rust 라이브러리(`outliner_core`, `lib.rs`)이므로 네이티브 앱이 직접 링크한다. `outliner_service.py`가 하는 일(경로 검사·SHA-256 충돌 확인·페이지 파일 관리)은 전부 Rust로 옮길 수 있다(4절). 사이드메뉴에서 여는 한 줄만 앱 셸(Flutter+Python)이 살아 있는 동안 거친다(5절). |
| Flutter 잔류 사유 | **없음.** 인벤토리가 "Flutter 필수 아니오"(RUST_MIGRATION_INVENTORY:110)로 판정했고, 화면 881줄은 목록·입력칸·버튼뿐이다(`outliner_workspace.dart`). 캔버스·WebView2 같은 대체 불가 요소가 없다. |
| 가장 큰 위험 | ① 한글 IME 조합 중 명령 처리(3절) ② "차이 0" 기준이 Flutter의 우연한 동작(키 버려짐·병합 시 공백 없음 등)까지 포함하는 문제(6절) ③ 전환기에 두 화면이 같은 `.md`를 동시에 여는 문제(7절) |
| 규모 추정 | Rust 약 3,500~4,500줄(시험 포함), 카드 약 13~16장(7절). 추정. |

## 1. 현재 구조 지도

### 1.1 호출 경로

```
Flutter OutlinerWorkspace (outliner_workspace.dart, 881줄)
  → api('outliner', [payload])  (outliner_workspace.dart:93-103; 응답 {ok, data|error})
  → DesktopWebBridge.outliner  (desktop_web_ui.py:2378-2388)  /  ApplicationService.handle 'outliner' (application_service.py:317-324)
  → OutlinerService.execute     (outliner_service.py:72-134, 149줄)
  → 요청마다 outliner.exe 프로세스 1회 기동 (outliner_service.py:60-61, timeout 30초)
  → outliner_core::handle_request (api.rs:16-160, 표준입력 한 줄 JSON → 표준출력 한 줄 JSON, main.rs:5-22)
```

- Python은 호출마다 `subprocess.run`으로 엔진을 새로 띄우고, 엔진은 요청마다 문서 전체를 다시 파싱한다(`api.rs:69` `parse(document_text)`, 편집 후 `serialize`). 엔진은 상태를 갖지 않는다. 지연은 **미측정(U4)**.
- 엔진 실행 파일은 `resources/outliner.exe`(.spec 17-21행, CI `build-windows-exe.yml:215-225`가 빌드·해시). Python은 `_internal/resources`, `resources`, `outliner_core/target/release` 순으로 찾는다(`outliner_service.py:26-38`).

### 1.2 엔진 요청 형태 (`api.rs`)

요청 `{"id":…, "op":…, "document":"<markdown>", "args":{…}}` → 응답 `{"id":…,"ok":true,"result":…}` 또는 `{"ok":false,"error":{"code","message"}}`(`api.rs:154-159`). 오류 코드는 `lib.rs:23-32`: `DOCUMENT_TOO_LARGE`, `TOO_MANY_BLOCKS`, `QUERY_TOO_SHORT`, `INVALID_QUERY`, `INVALID_OPERATION`, `IO_ERROR`. 한도: 문서 10 MiB, 블록 100,000개(`lib.rs:6-8`).

| op | args | 결과 | 줄 | Python 경유 | Flutter 사용 |
|---|---|---|---|---|---|
| `load` | `path` | `document`, `markdown` | api.rs:50 | `_load` | 예(load·daily·create·rename 뒤) |
| `save` | `path`, `markdown` | `saved` | api.rs:58 | `save` | 예 |
| `edit` | `action`, `block_id`, `text` + `document` | `markdown`, `document` | api.rs:68 | `edit`(op 이름 변환, outliner_service.py:129-133) | 예 |
| `search` | `query` + `document` | `block_ids` | api.rs:75 | `search` | 예(현재 페이지 한정) |
| `backlinks` | `target` + `document` | `block_ids` | api.rs:78 | `backlinks` | 예(자기 페이지 한정, 1.4) |
| `daily_page` | `date` | `filename`(`YYYY-MM-DD.md`) | api.rs:28 | `daily` | 예(오늘 하나만) |
| `parse` / `serialize` | `document` | 문서 / `markdown` | api.rs:65,67 | `parse`만 통과 | 아니오 |
| `query` | `query`, `pages[{page,document}]` | `results` | api.rs:38 | 아니오 | 아니오 |
| `journal_name` / `journal_list` | `date`·`name` / `names` | 이름들 | api.rs:29,34 | 아니오 | 아니오 |
| `resolve_block_ref` (`block_ref`) | `uuid`·`id`·`target` | 해석 결과 | api.rs:81 | 아니오 | 아니오 |
| `rename_page` (`update_links`) | `old_name`,`new_name`,`files`/`path`/`dir`,`preview` | 변경 파일 목록 | api.rs:100 | 아니오 | 아니오 |

`edit`의 `action` 값(`ops.rs:44-184`): `insert`, `delete`, `indent`, `outdent`, `fold`, `unfold`, `up`, `down`, `split`, `merge`, `set_status`, `toggle_status`, `set_text`. 이 중 Flutter가 쓰는 것은 `set_text`·`insert`·`delete`·`indent`·`outdent`·`fold`·`unfold`·`up`·`down`·`toggle_status`다. **`split`·`merge`·`set_status`는 Flutter가 쓰지 않는다.**

오늘 사고(`TODO` 103행, 커밋 3c7e9c45)의 원인은 Python이 `set_text`를 `op`로 바로 보낸 것이었다. 지금은 `op='edit'` + `args.action`(outliner_service.py:129-133, 시험 `tests/test_outliner_service.py:47-66`). Rust가 엔진을 직접 호출하면 이 변환층 자체가 사라진다.

### 1.3 Python 서비스(`outliner_service.py`)의 명령 전체

| payload.action | 동작 | 엔진 호출 | 줄 |
|---|---|---|---|
| `list` | `Outliner/*.md` 이름순 목록 + 메모리 최근 목록(최대 10) | 없음 | 75-77, 146-148 |
| `create` | 같은 이름 있으면 `OUTLINER_PAGE_EXISTS`, 없으면 `- \n` 기록 후 load | load | 78-83 |
| `rename` | **파일 이름만 변경**(링크 갱신 안 함) 후 load | load | 84-93 |
| `delete` | `unlink`(휴지통 아님) | 없음 | 94-101 |
| `daily` | 오늘 날짜 `daily_page` → load(없으면 `- \n` 생성) | daily_page, load | 102-105, 136-149 |
| `load` | 파일 없거나 0바이트면 `- \n` 기록 후 load, SHA-256 `version` 반환 | load | 106-107, 136-149 |
| `save` | 파일의 현재 SHA-256 ≠ 요청 `version` 이면 `OUTLINER_CONFLICT`, 같으면 엔진 `save`(임시 파일+교체, `store.rs:76-146`) 후 새 `version` | save | 108-119 |
| `edit`·`search`·`backlinks`·`parse` | 문서 본문(`markdown`)을 그대로 엔진에 전달 | 같은 이름 | 120-133 |

경로 규칙(`outliner_service.py:40-47`): 페이지 이름은 **파일 이름 한 조각**(경로 구분자·`.`·`..` 불가), 확장자 `.md` 자동 부여, 해석 결과의 부모가 `data_root/Outliner`와 정확히 같아야 함. 데이터 폴더는 `Path(data_root)/'Outliner'`(18행)이고 `data_root`는 설정 저장소 폴더(`desktop_web_ui.py:2384`, `application_service.py:320`). 독립 백엔드는 `%APPDATA%\HwpAutoDocFit`(없으면 `%LOCALAPPDATA%`)를 쓰고(`backend_main.py:20-23`), Dock도 `%APPDATA%\HwpAutoDocFit\settings.json`을 읽는다(`dock/src/native.rs:420-428`). 데스크톱 쪽 `self.gui._준말_서비스().settings._path`의 실제 값은 **미확인(U6)** — 위 두 곳과 같다고 보는 것이 자연스럽지만 읽어 확인하지 않았다.

**저장 충돌 `_versions`**: 딕셔너리는 선언(23행)·기록(118, 145행)·삭제(92, 99행)만 있고 **읽는 곳이 없다.** 실제 충돌 판정은 `save`에서 "클라이언트가 가진 `version`(로드 시점 파일 SHA-256) 대 파일의 현재 SHA-256"(111행)이다. 따라서 Rust로 옮길 때 `_versions`는 이식하지 않아도 동작이 같다. 프로세스 간 파일 잠금은 **저장소 어디에도 없다**(`threading.RLock` 하나, 22행; `outliner_core`에서 `lock` 검색 0건). OUTLINER 설계 17행이 "UI 서비스에서 잠금 정책 제공"이라 했으나 구현되지 않았다(U7).

### 1.4 Flutter 화면(`outliner_workspace.dart`)이 만드는 상태와 명령

| 상태 | 줄 | 설명 |
|---|---|---|
| `_pages` | 47, 105-130 | `list` 결과(전체 페이지) |
| `_recentPages` | 48, 141-163 | 최근 20개. `%LOCALAPPDATA%\HwpAutoDocFit\outliner_recent.json`(28-30, 133-139)에 파일로 따로 저장 — Python의 메모리 최근 10개(outliner_service.py:24)와 **별개**이고 폴더도 다르다(`LOCALAPPDATA` 대 `APPDATA` 추정, U6) |
| `_page`, `_markdown`, `_version`, `_blocks` | 50-53, 180-187 | 현재 페이지·본문·충돌 확인용 SHA-256·블록 목록 |
| `_editors`, `_focusNodes` | 58-59 | 블록 id → TextEditingController/FocusNode. 편집 중이 아닌 블록은 문서 텍스트로 덮어씀(574-578) |
| `_busy`, `_message` | 54-55, 280-293 | 엔진 호출 중 표시와 오류 줄 |
| `_pageQuery`, `_search` | 56-57, 423-426 | 페이지 이름 필터(화면 전용). Enter면 `_searchBlocks` |
| `_linkQuery` | 60, 683-691, 743-774 | `[[` 뒤 글자로 페이지 이름 후보 6개 표시(화면 전용, 엔진 호출 없음) |
| `_visibleBlocks` | 62-73 | 접힌 블록의 하위(들여쓰기가 더 깊은 연속 블록)를 숨김 — **접힘은 화면 계산**, 파일에는 저장되지 않음(엔진 `parse`가 `folded:false`로 시작, `parse_serialize.rs:90`) |

| 사용자 동작 | 호출 순서 (payload.action / operation) | 줄 |
|---|---|---|
| 화면 열기 | `list` → (페이지 없으면) `daily` → `list` | 105-130, 165-178 |
| 페이지 열기 | `load` → `list` | 189-193 |
| 새 페이지 | `create` → `list` | 195-202 |
| 이름 바꾸기 | `rename` → `list` | 231-241 |
| 삭제 | `delete` → `list` → `load`(첫 페이지) 또는 `daily` | 243-278 |
| **Enter** | `edit/set_text`(현재 입력칸 전체) → `edit/insert`(text `''`, 같은 id 뒤) → `save` | 598-604, 315-341 |
| Tab / Shift+Tab | `edit/indent` / `edit/outdent` → `save` (입력칸 텍스트는 **보내지 않음**) | 589-597, 295-313 |
| Ctrl+Enter | `edit/fold` 또는 `unfold` → `save` | 605-614 |
| Alt+↑/↓ | `edit/set_text` → `edit/up` 또는 `down` → `save` | 615-628, 354-385 |
| Backspace(빈 칸) | `edit/set_text` → `edit/delete` → `save` | 631-638 |
| Backspace(칸 맨앞, 앞 블록 있음) | 앞 블록에 `edit/set_text(앞텍스트+현재텍스트)` → 현재 블록 `edit/delete` → `save` (**공백 없이 이어붙임**; 엔진 `merge`는 공백을 넣음, ops.rs:143-145) | 639-673 |
| ☑ 단추 | `edit/set_text` → `edit/toggle_status` → `save` | 702-722 |
| 접기 단추, 들여쓰기·내어쓰기 단추 | `edit/fold`·`unfold`, `edit/indent`·`outdent` → `save` | 543-559, 723-739 |
| 헤더 + 단추 | `edit/insert('')` (마지막 블록 뒤). **블록이 0개면 단추가 비활성**(513행) | 511-517 |
| 검색 Enter | 글자 수 2 미만이면 무시(388행) → `search` → 결과 id로 `_blocks`를 **걸러 냄**(되돌리는 UI 없음, 397-399) | 387-401 |
| 역링크 패널 | 현재 페이지가 바뀔 때마다 `backlinks`(`target` = **현재 페이지 이름**, **현재 페이지 본문**만) → "블록 N" 번호 목록 | 820-880 |
| 링크 칩 클릭 | 블록의 `page_links`마다 `[[이름]]` 칩 → `load` | 775-794 |

관찰(사실, 코드 근거):
1. **역링크는 사실상 미구현이다.** 현재 페이지 본문 안에서 자기 이름을 가리키는 블록만 찾고, 다른 페이지의 참조는 찾지 못한다(849-856). 표시도 "블록 번호"뿐이다.
2. **키가 버려진다.** `_busy`일 때 모든 키 입력을 `handled`로 삼킨다(584행). 엔진 호출이 느리면(요청마다 프로세스 기동) 그 사이 친 글자·Enter가 사라질 수 있다. 오늘 사고의 직접 원인은 요청 형태 불일치(커밋 3c7e9c45)로 확인됐으나, 이 키 버림이 기여했는지는 **미확인(U3)**.
3. Enter는 **커서 위치에서 쪼개지 않는다.** 항상 전체 텍스트를 저장하고 빈 블록을 아래에 만든다(엔진 `split`은 "구분 문자열" 기준이라 커서 분할이 아님, ops.rs:102-108).
4. 접기·들여쓰기·위아래 이동은 **블록 한 줄만** 대상이다(하위 블록 동반 이동 없음, ops.rs:64-101). 들여쓰기는 `indent += 1`, 직렬화는 `" ".repeat(indent)`(parse_serialize.rs:142-ff `format_block`)라 **한 단계가 공백 1칸**이고, 파일에서 읽은 탭은 4칸으로 환산된다(parse_serialize.rs:6-11). Logseq식 2칸·탭과 단계가 어긋날 수 있다. 화면은 `indent × 24px`(538행)로 그린다.
5. 블록 id는 **파싱 때마다 줄 순서대로 새로 매긴다**(`blocks.len()+1`, parse_serialize.rs:121). 편집 응답의 문서를 계속 쓰는 한 일관되지만, 다시 `load`하면 id가 바뀐다.
6. 페이지를 지워 블록이 0개가 되면 엔진 `insert`가 기준 블록이 필요해(ops.rs:37) Flutter에서 새 블록 추가가 막힌다. 다음 `load`가 빈 파일을 `- \n`로 되살린다(outliner_service.py:140-142).

### 1.5 시험 자산 (이식 때 그대로 재사용)

| 자산 | 규모 | 내용 |
|---|---|---|
| `outliner_core/src/tests.rs` | 58개 시험 | 왕복 보존·편집 연산·링크·검색·질의·저널·이름 바꾸기·저장 (이번에 `cargo test` 실행: 58 통과 + 별도 대상 1 통과, 실패 0) |
| `tests/test_outliner_service.py` | 69줄, 시험 4개 | 경로 가둠, 저장 충돌, CLI 계약, `op='edit'` 변환 |
| `flutter_ui/test/outliner_workspace_test.dart` | 265줄 | 화면·최근 페이지·**키보드 시나리오**(190-264) — 6절 "차이 0" 시나리오의 원재료 |

## 2. 창 구조 비교

공통 전제: 블록은 한 줄 텍스트(엔진이 줄 단위 Markdown, `parse_serialize.rs:105-131` `parse`)이며 줄바꿈을 포함할 수 없다. 따라서 편집 컨트롤은 Enter·Shift+Enter가 줄바꿈이 되지 않게 막아야 한다.

### 2.1 세 안

**A안 — 블록 목록 자체 그리기 + 편집 중인 블록에만 EDIT 창 겹치기**
- 메인 창이 `WM_PAINT`에서 보이는 블록만 GDI로 그린다(들여쓰기 선·불릿·접기 삼각형·TODO 체크·`[[링크]]` 강조·`#태그`). 스크롤은 `SetScrollInfo`+수직 막대, 그리는 줄은 화면에 보이는 행만(가상화).
- 블록을 클릭/키로 편집 진입하면 그 블록 사각형에 `EDIT` 자식 창(1개)을 만들거나 옮겨 겹친다. 편집을 떠나면 텍스트를 읽어 엔진에 반영하고 EDIT는 숨긴다(재사용).
- 장점: 창 개수가 **블록 수와 무관**(1개). 수천 블록도 보이는 줄만 그리면 된다. IME 조합창·후보창·한자 변환·밑줄을 **운영체제 EDIT가 처리**한다(직접 그린 입력칸은 조합 글자를 스스로 그려야 한다 — 아래 선례 참고). 선택·복사·붙여넣기·실행 취소(Ctrl+Z)도 EDIT 기본 기능.
- 단점: 직접 그리는 부분의 접근성(화면 낭독기)을 직접 제공해야 한다(2.3). 클릭 시 좌표↔블록 대응, EDIT를 정확히 겹쳐 놓는 레이아웃 계산, 긴 블록의 줄바꿈 높이 계산(그리기와 EDIT가 같은 글꼴·폭이어야 높이가 같음)이 필요하다. 편집 진입·이탈 때 깜빡임이 없도록 해야 한다.

**B안 — 블록마다 컨트롤(EDIT)**
- 블록 하나당 EDIT 자식 창 하나(+접기/체크 단추 창).
- 장점: 가장 단순하다(그리기 코드 적음). 각 EDIT가 스스로 IME·접근성(MSAA 기본 제공)·선택을 처리.
- 단점: 블록 수천 개면 창 수천 개 — 창 생성·레이아웃 비용이 크고, Windows 프로세스당 USER 객체 기본 한도(일반 지식으로 10,000개 수준)에 근접할 수 있다(이 PC 측정은 **미확인, U14**). 스크롤은 창 이동으로 해야 해 끊김 위험. 접힌 블록의 하위를 숨기고 보이는 것만 만드는 가상화를 하면 결국 A안과 같은 좌표 계산이 필요해져 장점이 사라진다. 다크 모드는 `WM_CTLCOLOREDIT`로 가능(ktm 선례 `ktm/src/window.rs:2936`)하나 포커스 이동마다 테두리·밑줄 처리가 따로 필요.

**C안 — 하나의 RichEdit(문서 전체를 한 컨트롤에)**
- 문서 전체를 RichEdit 한 개에 넣고 문단 = 블록으로 본다. 들여쓰기는 문단 들여쓰기(`PARAFORMAT`).
- 장점: 스크롤·선택·IME·접근성·고DPI·큰 문서 성능을 컨트롤이 해결. 코드가 가장 적다(추정 2,000줄 안팎).
- 단점: **엔진의 블록 id·연산과 컨트롤 안의 문단을 일치시키는 것이 어렵다.** 컨트롤 자체의 실행 취소·붙여넣기(여러 줄)·드래그 이동이 엔진 문서를 우회해 블록 구조를 깨뜨릴 수 있다. 접기(문단 숨김)를 RichEdit로 구현하기 어렵고, TODO 체크박스·링크 칩 같은 요소를 넣기 어렵다. Enter를 가로채 엔진을 부르는 방식이면 결국 컨트롤 변경을 엔진과 동기화하는 층이 필요하다(우리 코드에 선례 없음). 다크 모드는 `EM_SETBKGNDCOLOR`·`EM_SETCHARFORMAT`로 가능하나 RichEdit 계열 DLL 사용 선례가 이 저장소에 없다(`ktm`·`kagent`·`dock` 모두 EDIT 또는 직접 그리기만 사용).

### 2.2 비교표

| 항목 | A: 직접 그리기 + EDIT 1개 | B: 블록마다 EDIT | C: RichEdit 1개 |
|---|---|---|---|
| 한글 IME | OS EDIT가 조합·후보창 처리. 상태 기계만 우리가 소유(3절) | 블록마다 EDIT라 동일하나 **포커스 이동 중 조합**이 블록 경계마다 생김 | RichEdit가 처리하나 블록 명령(Enter·Tab) 가로채기 지점이 문단 단위로 모호 |
| 접근성 | 목록 그리기 부분은 직접 제공 필요(UIA/MSAA). **우리 Rust 코드에 선례 없음**(`WM_GETOBJECT`·UIA 검색 0건, U5). 편집 중 블록은 EDIT 기본 지원 | EDIT 기본 지원이 가장 풍부 | RichEdit 기본 지원(문서 한 덩어리로 읽힘) |
| 고DPI | `WM_DPICHANGED`·`scale()` 선례(ktm window.rs:205, 2897). 직접 그리므로 모든 치수 우리가 계산 | 컨트롤마다 크기 재계산 | 컨트롤이 처리 |
| 다크 모드 | `ui_theme`(`apply_window` DWM 다크 속성, `ui_theme/src/win.rs:83`; `Palette`, `ui_theme/src/lib.rs:42`)로 전부 직접 칠함. EDIT는 `WM_CTLCOLOREDIT` | 모든 EDIT에 `WM_CTLCOLOREDIT` | 별도 API 사용 |
| 고대비 | `ui_theme::high_contrast_on`·`SysColors` 선례(win.rs:38-46) | 동일 | 컨트롤별 대응 필요 |
| 블록 수천 개 성능 | 보이는 행만 그림 → 블록 수와 무관. 레이아웃 높이 합산은 블록 수에 비례(캐시 필요) | 창 수천 개: 느림·객체 한도 위험(U14) | 컨트롤이 처리하나 블록 단위 연산 동기화 비용 |
| 구현 줄수(추정) | 3,500~4,500줄 | 2,500~3,200줄 | 2,000~2,800줄 + 동기화층 |
| 엔진 정합성 | 엔진 문서가 유일한 진실, 화면은 투영 | 동일 | 컨트롤 텍스트와 엔진 문서 이중 상태 |

줄수 추정의 근거(비율은 우리 코드의 규모에서 가져옴): 직접 그리는 목록·입력·메뉴를 가진 `kagent/src/popup.rs` 1,886줄(+`frame.rs`), 검색 창 `ktm/src/window.rs` 4,551줄(결과 목록·옵션·모달 다수), Dock 창 `dock/src/native.rs` 443줄 + `lib.rs` 457줄. 아웃라이너는 목록(블록)·편집 1칸·사이드 목록·간단한 대화 상자이므로 popup.rs와 ktm의 중간 규모로 본다. 모델 400 / 그리기 900 / 입력·IME 700 / 명령 600 / 사이드·페이지 관리 600 / 대화 상자·앱 접착 600 / 시험 700 — 합계 4,500줄 상한. **추정이며 측정한 값이 아니다.**

### 2.3 권장: A안

근거:
1. **IME를 운영체제에 맡길 수 있다.** 우리 코드에는 두 가지 선례가 있는데, ktm은 EDIT 컨트롤을 쓰고(`ktm/src/window.rs:4118-4130`) IME 조합을 `ImmGetCompositionStringW(GCS_COMPSTR)`로 *확인만* 한다(260-266, 628-640). kagent의 퀵몽땅클립보드 팝업은 **입력칸을 직접 그려** 조합 글자를 `WM_IME_COMPOSITION`에서 받아 스스로 그린다(`kagent/src/popup.rs:1431-1455`, 그리기 1036행). 후자는 40자 한 줄 검색칸이라 가능했지만, 편집기에서는 후보창 위치(`ImmSetCompositionWindow` 계열 — 우리 코드 검색 0건)·밑줄·커서·선택·붙여넣기를 전부 직접 구현해야 한다. A안은 편집 중 블록만 EDIT에 맡겨 이 부담을 피한다.
2. **성능이 블록 수와 무관**하다(100,000블록까지 허용하는 `lib.rs:7` 한도에서 B안은 불가능).
3. **엔진이 유일한 진실**이어서 "Flutter와 차이 0" 대조(6절)를 이벤트·문서 수준에서 할 수 있다(C안은 이중 상태).
4. 직접 그리기는 우리 코드의 검증된 방식이다: GDI 이중 버퍼·가상 목록(`kagent/src/popup.rs`), 순수 레이아웃 계산+골든 덤프(`ktm/src/layout.rs` 1,758줄·`view_dump.rs` 230줄, `dock/src/lib.rs`의 레이아웃 시험), 캡처 모드(`ktm/src/window_capture.rs`).
5. A안의 약점인 접근성은 **편집 중 EDIT는 기본 제공**, 나머지는 2단계에서 최소 구현(`WM_GETOBJECT`로 목록 항목 노출)을 하고 5절 U5에서 비용을 확인한다. 사용자가 접근성 요구를 높이면 B안을 "보이는 행에만" 적용하는 혼합도 가능하다.

### 2.4 우리 코드에서 재사용할 부분

| 재사용 | 근거 | 아웃라이너에서 |
|---|---|---|
| 조합 중 판별 `ImmGetCompositionStringW(GCS_COMPSTR)>0` | `ktm/src/window.rs:260-266`(`sync_edit_text`), `628-640`(`key`) | 3절 상태 기계의 "조합 중" 판정 함수. **단 ktm은 조합 중이면 키 처리를 *포기*(`return false`)**할 뿐이므로 "확정 후 명령 실행"은 새로 만든다 |
| EDIT 생성·`EM_SETLIMITTEXT`·`WM_CTLCOLOREDIT` 다크 색 | `ktm/src/window.rs:4118-4130`, 2936-2950 | 편집 EDIT 1개 |
| 메시지 루프에서 `WM_KEYDOWN`을 `TranslateMessage` 전에 가로채는 방식 | `ktm/src/window.rs:4521-4527` | Enter·Tab·방향키 선처리 지점 |
| 텍스트 갱신 시 조합을 끊지 않기(`sync_edit_text`) | `ktm/src/window.rs:253-282` | 엔진 응답으로 EDIT 글을 바꿀 때 조합 중이면 건너뜀 |
| 직접 그린 목록·`WM_IME_COMPOSITION` 수신 | `kagent/src/popup.rs:1413-1455` | 조합 문자열 수신 패턴(경고용: 직접 그리기 시 후보창 위치 미처리) |
| 테마(다크·밝게·고대비)·DWM 다크 속성 | `ui_theme/src/lib.rs:42,386`, `win.rs:61-90` | `Palette` 그대로. ktm 도 `ui_theme::refresh` 사용(window.rs:236-246) |
| 모니터 DPI·`WM_DPICHANGED` | `ktm/src/window.rs:205-215, 2897-2909` | 전 치수 `scale()` |
| 단일 인스턴스(뮤텍스+`FindWindow`로 기존 창에 메시지) | `kagent/src/main.rs:1079-1098`, `ktm/src/window.rs:4052-4064` | 5절 |
| 이름 있는 파이프(현재 사용자만, 원격 거부, 한 줄 JSON) | `kagent/src/pipe.rs:63-100`, `dock/src/native.rs:314-391` | 5절(선택) |
| 캡처 모드 | `ktm/src/window_capture.rs:1-31` | 6.4절 |
| 순수 레이아웃 골든 시험 | `dock/src/lib.rs:322` (`dock_layout_dump_edges_dpi_and_workarea` 시험), `ktm/src/view_dump.rs` | 레이아웃·상태 기계를 Win32 없이 시험 |
| 프레임·테두리 | `ktm/src/frame.rs`(261줄), kagent `frame.rs` | 창 외형. 정확한 재사용 범위는 구현 카드에서 확정 |

우리 코드에 **선례가 없는 것**: `ImmNotifyIME`(조합 강제 확정), `ImmSetCompositionWindow`, `WM_GETOBJECT`/UIA 제공, RichEdit. (`grep` 0건)

## 3. 한글 IME 조합 상태 기계

### 3.1 용어와 불확실성

- 조합 중 = `ImmGetCompositionStringW(GCS_COMPSTR)`가 1글자 이상(ktm 선례) **또는** `WM_IME_STARTCOMPOSITION`과 `WM_IME_ENDCOMPOSITION` 사이. 두 판정을 모두 써서 틈(조합 시작 직전·직후 키)을 막는다.
- 한글 IME는 조합 중인 키 입력의 `WM_KEYDOWN`을 `wParam = VK_PROCESSKEY(0xE5)`로 보내는 것이 일반적이고, 원래 키는 `ImmGetVirtualKey`로 알 수 있다. **그러나 Enter·방향키·Tab이 조합 중 (a) VK_PROCESSKEY로만 오는지, (b) 조합을 확정한 *뒤* 실제 `VK_RETURN`이 한 번 더 오는지, (c) 결과 글자(`WM_CHAR`)가 키 입력보다 먼저 오는지는 이 문서 작성 중 확인하지 못했다(U1).** IME 종류(구형/신형 Microsoft 한글 IME, TSF 사용 여부)에 따라 다를 수 있다(U2).
- 따라서 이 상태 기계는 **어느 순서로 와도 같은 결과**가 나오도록 짜고, 2단계 첫 카드에서 실제 순서를 기록하는 탐침 도구를 만들어(6.2절) 가정을 확정한다.

### 3.2 원칙 (오늘 사고 재발 방지)

1. **확정 후 명령**: 명령(Enter·Tab·이동·클릭·페이지 전환·닫기)은 EDIT 안의 글을 읽기 **전에** 조합을 확정한다(`ImmNotifyIME(CPS_COMPLETE)`, 새로 도입, U2). 조합 중 상태에서 `GetWindowTextW`를 호출해 얻은 글을 저장하지 않는다.
2. **저장은 엔진 호출 + 파일 저장이 끝나야 성공**이다. `set_text` 응답을 받기 전에 화면 글을 "저장됨"으로 취급하지 않는다. 엔진 호출이 실패하면 EDIT의 글을 **지우지 않고** 오류를 표시한다.
3. **키를 버리지 않는다.** 명령 처리 중 들어온 키는 큐에 쌓아 순서대로 처리한다(Flutter는 `_busy` 중 버림, outliner_workspace.dart:584). 엔진을 라이브러리로 직접 호출하면 한 번에 마이크로초~밀리초라 큐가 거의 비지만 규칙은 둔다.
4. **한 번의 물리 키 = 한 번의 명령.** Enter가 (a)와 (b)로 두 번 보이더라도 `lParam`의 이전 키 상태·스캔 코드·마지막 처리 시각(50 ms)으로 중복을 제거한다.
5. **조합 중 Backspace·Delete·글자 키·Space·Esc·후보창 조작은 IME 소유**다. 우리 명령으로 해석하지 않는다(블록 삭제·병합은 *조합이 아닌 상태*에서만).
6. 조합 확정 글자를 엔진에 넣을 때 엔진 텍스트 규칙을 지킨다: 줄바꿈(`\r`, `\n`)은 공백으로 바꾼다(블록은 한 줄).

### 3.3 상태

| 상태 | 정의 |
|---|---|
| **N** 비편집 | 편집 EDIT가 숨겨짐. 목록 탐색(↑↓ 선택, Enter=편집 진입) |
| **E** 편집·비조합 | EDIT에 포커스, 조합 없음 |
| **C** 조합 중 | `GCS_COMPSTR` 길이>0 또는 Start~End 사이 |
| **F** 확정 대기 | 명령을 받아 `CPS_COMPLETE`를 요청했고 조합 종료를 기다림. 대기 명령 1개를 가짐. 200 ms 안에 종료 알림이 없으면 E로 간주하고 계속(기록 남김) |
| **X** 실행 중 | 엔진 호출·저장 진행(동기 호출이면 짧음). 들어오는 키는 큐 |

### 3.4 이벤트 × 상태 표

기호: `확정→` = F를 거쳐(이미 E면 생략) 확정된 글을 읽는다. "저장" = 그 글로 `set_text` 후 필요 시 `save`.

| 이벤트 | N | E (비조합) | C (조합 중) |
|---|---|---|---|
| **글자·Space·Backspace(글자 지움)** | 무시 | EDIT가 처리 | IME가 처리(우리는 가로채지 않음) |
| **Enter** | 선택 블록 편집 진입 | 저장 → `insert('')` → 새 블록 편집 진입(EDIT 글 비움). 파일 저장 | `확정→` 저장 → `insert('')` → 새 블록. **한 번에 끝남**(두 번 누르게 하지 않음) |
| **Shift+Enter** | 무시 | 무시(한 줄 규칙, Flutter 단일 줄 입력칸과 동일) | 무시 |
| **Ctrl+Enter** | 접기/펼치기 | 접기/펼치기(글 변경 없으면 `set_text` 생략 가능, Flutter는 보내지 않음 :605-614) | `확정→` 저장 후 접기/펼치기 |
| **Tab / Shift+Tab** | 들여쓰기/내어쓰기 | `indent`/`outdent`(Flutter는 텍스트를 보내지 않으나 **네이티브는 EDIT의 현재 글을 먼저 저장**해 글 유실을 막음 — 차이 등록 필요, 6.3) | `확정→` 저장 → indent/outdent. 편집 위치 유지 |
| **↑ / ↓** | 선택 이동 | 줄이 한 줄이면 이전/다음 블록으로(Flutter 단일 입력칸의 기본 이동에 대응), 여러 줄 래핑이면 캐럿 이동 후 가장자리에서 블록 이동 | `확정→` 저장 → 이전/다음 블록 편집 진입(아래 방향키가 조합 확정에만 쓰이는 IME이면 U1에서 확인해 두 번째 키부터 이동) |
| **Alt+↑ / Alt+↓** | `up`/`down` | 저장 → `up`/`down` → 같은 블록 계속 편집(Flutter :615-628) | `확정→` 저장 → `up`/`down` |
| **Backspace(맨앞·빈칸)** | 무시 | 빈 칸이면 `delete`, 맨앞이면 앞 블록과 합치기(규칙은 4절 표 7) | **IME 소유 — 아무것도 하지 않음** |
| **다른 블록 클릭** | 그 블록 편집 진입 | 저장 → 클릭한 블록의 **id**로 이동(저장으로 블록 목록·높이가 바뀌어도 id로 찾음) | `확정→` 저장 → 이동 |
| **툴바·사이드 단추**(☑·+·접기·페이지 열기·삭제·이름 바꾸기·검색) | 명령 실행 | 저장 → 명령 | `확정→` 저장 → 명령 |
| **Esc** | 창 닫기 아님(선택 해제) | 변경 확정 후 N으로(저장) | **IME 소유**(조합 취소) — 조합이 끝난 뒤의 Esc만 우리가 처리 |
| **창 비활성·닫기·Alt+Tab·페이지 전환** | 저장할 것 없음 | 저장 | `확정→` 저장 → 닫기·전환 진행 |
| **IME 후보창 열림 상태의 Enter·↑↓** | — | — | IME 소유(후보 선택). 후보창이 닫힌 뒤 처음 오는 키부터 우리 처리 |
| **조합 종료 알림**(`WM_IME_ENDCOMPOSITION`) | — | — | C→E. F 상태였으면 대기 명령 실행(`PostMessage`로 자기 창에 보내 결과 글자 처리 후 실행) |
| **엔진 호출 실패** | 오류 줄 표시 | EDIT 글 보존·오류 줄 | 글 보존·오류 줄 |
| **저장 충돌**(`OUTLINER_CONFLICT`) | 안내 | 변경분을 `.conflict` 사본 등으로 보존할지 사용자에게 묻기(정책은 U7) | 글 보존 후 동일 |

### 3.5 명령 실행 순서(확정 후)

```
1. 명령 수신(키/클릭) → 중복 제거(원칙 4)
2. C이면: ImmNotifyIME(CPS_COMPLETE) → F, 대기 명령 보관, 200 ms 타이머
3. 조합 종료(또는 타이머) → PostMessage(WM_APP_RUN_PENDING)
4. 메시지 처리 시: text = GetWindowTextW(EDIT)  (이 시점 글에 확정 글자가 반영돼 있어야 함)
5. 모델에 반영 → 엔진: set_text(id, text) [변경이 있을 때만] → 명령별 edit(...)
6. 성공이면 save(version 확인) → 화면 갱신 → 편집 포커스 이동
7. 큐에 남은 키가 있으면 같은 절차를 반복
```

### 3.6 시험 시나리오 10개 (오늘 사고를 반복하지 않는지)

자동화 열: **자동** = 상태 기계를 순수 Rust로 시험(이벤트 주입, `ImeProbe` 트레이트로 "조합 중 글자"를 가짜 값으로 공급)·Win32 `SendInput`으로 가능. **실기** = 실제 한글 IME가 필요해 사용자 확인.

| # | 시나리오 | 기대 결과 | 자동화 |
|---|---|---|---|
| 1 | 한글 `안녕` 입력 직후(조합 중) Enter | 파일에 `- 안녕` 다음 줄 `- ` (빈 블록 **1개**), 글자 미손실, 새 블록에 포커스 | 상태기계 자동 + **실기 필수**(실제 순서 U1) |
| 2 | 마지막 글자가 미완성 자모(`안녕ㅎ`)인 채 Enter | `ㅎ`까지 포함해 저장 | 실기 필수 |
| 3 | 조합 중 Tab, 조합 중 Shift+Tab | 글자 저장 + 들여쓰기/내어쓰기 1단계, 글자 유지 | 상태기계 자동 + 실기 |
| 4 | 조합 중 ↓ 후 ↑ | 글자 저장되고 인접 블록으로 이동, 돌아오면 글자 그대로 | 실기 필수(IME의 방향키 처리 U1) |
| 5 | 조합 중 Alt+↑ | 글자 저장 후 블록이 위로 이동, 글자 유지 | 실기 |
| 6 | 조합 중 다른 블록 클릭 | 글자 저장, 클릭한 블록 편집 진입(저장으로 높이가 바뀌어도 올바른 블록) | 클릭은 자동, 조합은 실기 |
| 7 | 조합 중 Backspace(자모 지우기) 반복 → 빈 칸이 된 뒤 Backspace | 조합 중에는 블록 삭제·병합 없음, 조합이 끝난 빈 칸에서 다음 Backspace가 `delete` | 상태기계 자동 + 실기 |
| 8 | 조합 중 창 닫기(Alt+F4)·Alt+Tab·페이지 전환 | 글자 저장 후 전환·종료, 저장 실패 시 종료를 막고 알림 | 상태기계 자동 + 실기 |
| 9 | 빠른 연속: `가나다` 입력 중 Enter 3회 연타 | 블록 3개 추가 또는 규칙에 맞는 수, 글자 보존, **키 버림 0** | 영문/`SendInput`으로 자동(큐 검증), 한글은 실기 |
| 10 | 엔진 오류 주입(저장 충돌·읽기 전용 파일)에서 Enter | EDIT 글 보존, 오류 줄, 같은 글로 재시도 가능, 다음 시도 때 중복 블록 생성 없음 | 자동(오류 주입) |

추가 점검(실기): 후보창(한자 변환)이 열린 상태의 Enter·방향키, 영문↔한글 전환 키 직후, 100~200% DPI·다크 모드에서 후보창 위치와 색(U11).

## 4. 기능별 엔진 호출 매핑

"엔진" 열: ✔ 있음 / ✘ 없음 / △ 부분. 네이티브 앱은 `outliner_core`를 **라이브러리로 직접** 호출한다(`pub use`: `parse`, `serialize`, `edit`, `search`, `backlinks`, `query`, `rename_page_in_files`, `save_atomic` 등, `lib.rs:94-104`). 앱 쪽이 문서를 메모리에 유지하고 `edit(&mut document, …)`를 쓰면 요청마다의 재파싱도 없다.

| # | 기능 | 엔진 연산 | 엔진 | Flutter 현재 동작 | 네이티브에서 할 일 |
|---|---|---|---|---|---|
| 1 | 블록 추가(아래) | `insert`(기준 id 뒤, 같은 들여쓰기, ops.rs:45-58) | ✔ | Enter: `set_text`+`insert('')` | 동일 순서. 빈 문서 첫 블록은 `- ` 파일로 시작(1.4절 관찰 6) |
| 2 | 블록 삭제 | `delete`(ops.rs:59) | ✔ | 빈 칸 Backspace | 동일. 하위 블록은 남음(엔진 동작 그대로) |
| 3 | 들여쓰기·내어쓰기 | `indent`·`outdent`(ops.rs:64-77). 첫 블록 들여쓰기·최상위 내어쓰기는 오류 | ✔ | Tab·Shift+Tab·단추. 1단계=공백 1칸(1.4 관찰 4) | 동일 호출. 오류는 조용히 표시 |
| 4 | 접기·펼치기 | `fold`·`unfold`(ops.rs:78-79). 저장되지 않는 세션 값 | ✔ | Ctrl+Enter·단추. 숨김 계산은 화면 쪽(:62-73) | 숨김 계산(`_visibleBlocks`와 같은 규칙)을 Rust 모델에 구현 |
| 5 | 위·아래 이동 | `up`·`down`(ops.rs:80-101). 인접 한 줄 교환, 보존 줄(Raw) 건너뛰기 불가 | ✔ | Alt+↑↓ | 동일 |
| 6 | 분할 | `split`(구분 **문자열** 기준, ops.rs:102-127) | △ | 사용 안 함(Enter도 분할 아님) | **커서 위치 분할은 엔진에 없음.** 정책 선택(U13): ① Flutter와 같이 분할 없이 새 블록(차이 0) ② 커서 뒤 글을 `set_text`(앞)+`insert`(뒤)로 구성(엔진 변경 없이 가능, 차이로 등록) |
| 7 | 병합 | `merge`(다음 블록을 현재에 합침, 사이에 **공백 1개**, ops.rs:128-150) | ✔ | Backspace 맨앞: 앞 블록에 `set_text(앞+현재)`(**공백 없음**)+`delete`(:639-673) | 차이 0이면 Flutter 방식, 엔진 `merge` 사용 시 공백 차이 → 차이 목록에 등록 후 사용자 결정 |
| 8 | TODO 토글 | `toggle_status`(None→TODO→DONE→TODO), `set_status`(ops.rs:151-173) | ✔ | ☑ 단추: `set_text`+`toggle_status` | 동일. 상태 해제(None)는 `set_status('')`뿐 — UI 추가는 선택 |
| 9 | 링크 자동완성 | **없음** — `[[` 뒤 글자를 보고 페이지 이름 목록(`list`)에서 거르는 화면 로직(:683-691, 743-774) | ✘ | 목록 6개, 선택 시 `[[이름]]`로 교체 | Rust로 순수 함수(입력→후보→교체 문자열) 작성·시험. 엔진 변경 불필요 |
| 10 | 역링크 | `backlinks`(한 문서 안, index.rs:201). 여러 페이지는 `collect_markdown_files`(rename.rs:232)+`build_index`(index.rs:176)로 조합 가능 | △ | 현재 페이지 본문에서 자기 이름만 찾음(:849-856, 1.4 관찰 1) | **다른 페이지 참조까지 찾는 새 연산 필요**(엔진 API 추가 또는 앱 쪽 조합). 이는 Flutter 대비 *기능 확장*이므로 차이 0 대조에서 "기존 기능은 같다, 확장은 별도"로 구분 |
| 11 | 검색 | 한 문서 `search`(≥2글자, search.rs:47); 여러 페이지 `query`(`text:`·`tag:`·`link:`·`status:`·`prop:`, search.rs:8, api.rs:38) | ✔(단일)·✔(질의) | 현재 페이지 한정, 결과로 목록을 걸러 냄(되돌리기 UI 없음) | 전체 페이지 검색은 `query`로 가능(Flutter 미사용). 1단계 범위는 Flutter 동작 재현 |
| 12 | 저널 | `daily_page`(api.rs:28, 파일명 `YYYY-MM-DD.md`), `journal_name`·`journal_list`(`YYYY_MM_DD` 이름, lib.rs:131-175) | ✔ | "오늘의 일지" 하나(`daily`). 어제·내일 이동 UI 없음 | **두 이름 규칙이 다르다**: Flutter가 만드는 파일은 `2026-11-20.md`인데 `journal_list`는 `2026_11_20` 형식만 인정(lib.rs:137-144). 정책 결정 필요(U15). 1단계는 Flutter 동작(`daily_page`) 재현 |
| 13 | 페이지 만들기 | 엔진 연산 없음. Python이 `- \n`을 씀(service:78-83) | ✘ | `create` | Rust 앱이 같은 규칙(이름 검증·중복 오류·`- \n`)으로 파일 생성 |
| 14 | 페이지 이름 바꾸기 | 엔진 `rename_page`(링크 갱신·미리보기, api.rs:100)는 있으나 Python은 **파일 이름만** 바꿈(service:84-93) | △ | `rename`(링크 갱신 없음) | 차이 0이면 파일 이름만. 링크까지 갱신하려면 엔진 `rename_page`를 쓰되 차이로 등록(사용자 결정, 7절) |
| 15 | 페이지 삭제 | 엔진 없음. Python이 `unlink`(휴지통 아님, service:94-101) | ✘ | `delete` + 확인 대화상자(:243-278) | 같은 확인 대화상자. 휴지통 사용 여부는 사용자 결정(차이로 등록) |
| 16 | 저장·충돌 | 엔진 `save`(임시 파일+교체, store.rs:76-146). 충돌은 Python의 SHA-256 비교(service:108-119) | △ | 편집마다 `save`, 불일치 시 오류 줄 | 로드 시 SHA-256 보관 → 저장 직전 파일의 현재 SHA-256과 비교 → 불일치면 `OUTLINER_CONFLICT`. 같은 알고리즘(바이트 SHA-256) 유지해야 전환기 두 화면이 서로 충돌을 감지함 |
| 17 | 목록·최근 | 없음(Python `glob`+메모리 10개, Flutter 파일 20개) | ✘ | 두 저장소가 따로 존재(1.3) | `outliner_recent.json` 형식·위치 그대로 읽고 씀(같은 20개) |
| 18 | 블록 참조 `((uuid))` | `resolve_block_ref`(index.rs:210) | ✔ | 사용 안 함 | 1단계 범위 밖(표시만) |

미리 확인된 엔진 한계(변경하지 않고 문서화): 블록 id가 파싱마다 새로 매겨짐, 들여쓰기 1칸 단위, 하위 블록 비동반 이동, 접힘 비저장, 빈 문서에 기준 블록 필요.

## 5. 앱 연동

### 5.1 어떻게 여는가

현재: 사이드메뉴 14번은 Flutter 탭 안의 위젯이다(`tab_navigation_shell.dart:18,88,288`, `workspace_adapters.dart:243,299-315`). Dock은 14번을 `Route::MainTab(14)`로 보고 `docfit-open-tab-14` 윈도 이벤트로 Flutter 앱에 요청한다(`dock/src/lib.rs:220-231`, `dock/src/native.rs:393-418`, Flutter 쪽 `tab_open_events.dart`).

제안(ktm·kagent·Dock 방식을 따름):

| 진입점 | 방법 | 근거 |
|---|---|---|
| **Dock** | `dock::route`에 `Route::Outliner` 추가 → 형제 실행 파일 `outliner-win.exe`(가칭)를 `--show`로 실행. 이미 떠 있으면 새 프로세스가 기존 창에 메시지를 보내고 끝남 | Dock이 ktm을 `--show`로 부르는 방식(`native.rs:395-401`)과 `sibling_exe`(432-434) |
| **사이드메뉴 14번** | Flutter 탭을 "열기" 동작으로 바꾸고 실행 파일을 기동. 앱 셸이 Flutter+Python인 동안은 **브리지 메서드 한 개**(예: `outliner_open`)가 실행을 맡는다(ktm은 `ktm_service.py`가 같은 일을 함, ktm_service.py:1-25) | 앱 셸이 살아 있는 한 불가피한 연결. 아웃라이너 *기능* 코드는 Python에 남지 않음 |
| 실행 파일 위치 | 앱 `resources/outliner-win.exe`(+`.sha256`) — 현재 `outliner.exe`와 같은 번들 규칙(.spec 17-21, CI 215-225) | 기존 패턴 |

**`--enable`·파이프 방식 참고**: 퀵몽땅클립보드는 상주 프로그램이라 `--enable --consent --autostart`로 설치·시작·자동 실행을 등록하고(`kagent/src/main.rs:1060, 1376-1420`, `docfit_app/kclip_service.py:150-190`) 파이프로 상태를 묻는다(`kagent/src/pipe.rs:63-100`). **아웃라이너는 상주할 이유가 없다**(요청 시 창만 뜨면 됨) → `--enable`(동의·자동 실행 등록)은 **쓰지 않는다.** 대신 ① 실행 시 창을 띄우고 ② 닫으면 프로세스 종료 ③ 두 번째 실행은 기존 창을 앞으로 가져오고 종료. 파이프는 Dock의 "이미 열렸는지·`open_page`" 질의가 필요할 때만 선택 도입(첫 단계는 `FindWindow`+`PostMessage`로 충분, kagent `main.rs:1087-1094`와 동일).

### 5.2 단일 인스턴스

`Local\HwpAutoDocFit.Outliner.Singleton` 뮤텍스(Dock `native.rs:35-39`와 같은 이름 규칙) + 창 클래스 `FindWindow`로 기존 창에 `WM_SHOW`(앞으로·페이지 지정은 `WM_COPYDATA`) 전달. 전환기에 Flutter 탭이 같은 파일을 열고 있을 수 있어 7절 위험 3에서 정책을 정한다.

### 5.3 데이터 폴더 공유

- 폴더: `Path(data_root)/Outliner`, `data_root`는 설정 저장소 폴더(`%APPDATA%\HwpAutoDocFit` 추정, U6). 네이티브는 Dock과 같이 `%APPDATA%\HwpAutoDocFit\settings.json`의 폴더를 기준(`dock/src/native.rs:420-428`)으로 삼고, 설정이 없으면 `%APPDATA%\HwpAutoDocFit\Outliner`.
- 규칙 이식(그대로): 페이지 이름은 파일 한 조각만(경로 구분자·`.`·`..` 거부), `.md` 확장 자동, 해석 후 부모가 데이터 폴더와 같아야 함(`outliner_service.py:40-47`), 0바이트·없는 파일은 `- \n`, SHA-256 충돌 확인(108-119).
- 잠금: **기존에 없음**(1.3). 2단계에서 "저장 직전 SHA-256 비교"만 이식하고, 전환기 동시 편집은 7절 위험 3의 정책(둘 중 하나만 편집 허용 또는 충돌 시 사본 보존)으로 처리.
- 최근 페이지: `%LOCALAPPDATA%\HwpAutoDocFit\outliner_recent.json` 그대로 읽고 씀(Flutter와 공유, dart:28-44).

### 5.4 Python 없이 가능한가

- **가능한 것(전부)**: 창, 엔진 호출, 파일 관리, 충돌 확인, Dock에서 열기. Rust 앱은 Python이 없어도 단독 실행된다.
- **남는 한 줄**: 사이드메뉴 14번 클릭을 받는 쪽이 Flutter+Python 셸이다. 이는 **아웃라이너 때문이 아니라 셸 때문**이며 인벤토리의 전체 전환(RUSTALL1)이 끝나면 사라진다. 아웃라이너 쪽에서 제거할 것: `docfit_app/outliner_service.py`(149줄), 브리지 `outliner` 메서드(`desktop_web_ui.py:2378-2388`·`application_service.py:317-324`·허용 목록 `desktop_web_ui.py:3947`), Flutter `features/outliner/`(881줄)·어댑터(`workspace_adapters.dart:243,299-315`)·테스트, `tests/test_outliner_service.py`.
- **주의(혼동 방지)**: `external_apps_service.py:58`의 `outliner`는 **Logseq 외부 앱 다운로드 항목**("mongdangoutliner")이며 이 전환과 별개다. 사이드메뉴의 "외부 앱" 목록(`external_app_launcher.dart:17`)을 어떻게 할지는 사용자 결정 사항(U10).

## 6. 시험 계획

### 6.1 엔진 직접 호출 단위 시험

- 기존: `outliner_core` 58개 시험 그대로 유지(CI `cargo test --release --locked`, build-windows-exe.yml:219).
- 신규(네이티브 앱 crate): **순수 모델 시험** — Win32 없이 `Controller`(상태 기계+모델)에 이벤트를 넣고 문서·요청 로그를 단언. 3.6절 시나리오 1~10의 자동 가능 부분, 접힘 숨김 계산, 링크 자동완성 순수 함수, 경로 검증, 충돌 확인, 최근 페이지 읽기·쓰기(손상 파일 무시, dart:170-189 시험과 동일 기대).
- 사용자 문서는 건드리지 않는다: 모든 시험은 임시 데이터 폴더(`KTM_TEST_ROOT`와 같은 환경 변수 방식, ktm window.rs:4044)에서 수행.

### 6.2 IME 시험

| 범위 | 방법 | 자동화 |
|---|---|---|
| 상태 기계 로직 | `ImeProbe` 트레이트(조합 중 글자·확정 요청)를 가짜로 주입, 이벤트 순서를 (a)(b)(c) 모든 조합으로 재생 | 자동 |
| 영문 키·Enter·Tab·연타 | 실제 창에 `SendInput` (키 버림 0, 중복 Enter 제거 확인) | 자동(전용 창 PC, COM 잠금 불필요) |
| **IME 키 순서 탐침** | 2단계 첫 카드: 창이 받는 `WM_KEYDOWN`/`WM_IME_*`/`WM_CHAR`를 순서·`wParam`·시각과 함께 로그 파일로 기록(`--ime-probe <파일>`). 사용자가 한글 `안녕` 입력 후 Enter·Tab·방향키를 눌러 로그를 남김 | **실기 필수**(U1·U2) |
| 실제 한글 조합 시나리오 3.6절 | 사용자가 체크리스트로 수행 | **실기 필수** |
| 후보창·고DPI·다크 | 100/150/200% DPI, 다크 모드, 후보창 위치·색 | **실기 필수**(U11) |

### 6.3 Flutter 화면과 같은 입력 순서의 결과 대조 도구 설계 ("차이 0")

목표: 같은 **입력 스크립트**를 Flutter 경로와 네이티브 경로에 넣어 **최종 `.md` 바이트·최종 블록 트리·엔진 요청 열**이 같은지 비교.

1. **스크립트 형식**(JSON): `{"pages":{"Home":"- a\n- b\n"}, "steps":[{"focus":2},{"type":"새 글"},{"key":"Enter"},{"key":"Tab"},{"key":"Alt+Up"},{"click_check":2},…]}`. 1.4절의 모든 동작 + 3.6절 시나리오 중 IME 비의존 부분 + 접기·검색·페이지 관리. 목표 40개 이상. 시작 자료는 `outliner_workspace_test.dart:190-264`의 키보드 시나리오.
2. **기준(Reference) 실행**: Flutter 위젯 시험(`flutter test`)에서 `OutlinerWorkspace`를 띄우되 `api`를 **실제 Python `OutlinerService`+실제 `outliner.exe`**에 연결한다(작은 stdio 도우미 `tools/outliner_parity/flutter_api_server.py`가 `OutlinerService.execute`를 JSON 줄로 노출, Dart 시험이 `Process.start`로 사용). 이렇게 해야 "Flutter 입력 처리 → Python 서비스 → 엔진" 전체가 기준이 된다. 각 호출은 요청 로그로 기록.
3. **대상(Target) 실행**: `outliner-win.exe --replay script.json --data-root <임시폴더> --out <결과폴더>`. 같은 스크립트를 같은 이벤트 입력층(순수 컨트롤러)에 넣고 모델이 낸 엔진 호출 로그와 최종 파일을 저장. (Win32 키 경로 전체는 6.2의 `SendInput` 시험이 별도로 보증.)
4. **비교기**: ① 페이지별 최종 `.md` 바이트 동일 ② 최종 블록 트리(JSON) 동일(블록 id 제외·텍스트·들여쓰기·상태) ③ 엔진 요청 열의 (`action`,`block_id`,`text`) 열 동일(저장 횟수 차이는 정규화해 별도 표시). 하나라도 다르면 실패.
5. **허용 차이 등록부** `tools/outliner_parity/exceptions.json`: 의도한 차이만 사유·승인자·날짜와 함께 적고, 등록 없는 차이는 실패. 현재 알려진 후보: Flutter의 `_busy` 키 버림(네이티브는 큐, 3.2-3), Tab에서 글 먼저 저장(3.4), Backspace 병합 공백 규칙(4절 7), 역링크 범위 확장(4절 10), 페이지 이름 바꾸기 링크 갱신(4절 14), Enter 커서 분할(4절 6).
6. 실행 위치: 이 PC는 앱 실행 중일 수 있으므로 시험은 임시 폴더·별도 프로세스에서만 하고, 한/글 COM을 쓰지 않으므로 `.com-test.lock`은 필요 없다.

### 6.4 캡처 모드 (ktm `--capture-dir` 방식)

ktm은 `--capture-dir <폴더> [--dpi 96|120|144|192] [--scenario <이름>|all]`로 단축키·트레이·뮤텍스·검색 스레드 없이 가짜 자료로 창을 장면별 PNG로 저장하고, 가짜 클릭 결과를 `capture-report.txt`에 적는다(`ktm/src/window_capture.rs:1-31`, 설정은 캡처 폴더 안 임시 폴더, `ktm/src/window.rs:4040-4048`). 아웃라이너도 같은 방식:

`outliner-win.exe --capture-dir <폴더> [--dpi …] [--scenario …]` — 장면 목록(안): `01_empty`, `02_blocks_basic`, `03_nested_folded`, `04_todo_done`, `05_editing_overlay`, `06_link_autocomplete`, `07_sidebar_pages_recent`, `08_backlinks`, `09_search_filtered`, `10_dark`, `11_high_contrast`, `12_dpi_144_192`, `13_5000_blocks_scrolled`, `14_conflict_banner`, `15_long_wrapped_block`, `16_korean_ime_composing`(조합 문자열을 가짜로 주입한 모습). 같은 `draw` 함수를 메모리 DC에 그려 PNG를 만들고 `capture-report.txt`에 가짜 클릭 결과를 적는다. EDIT 자식 창은 메모리 DC 캡처에 포함되지 않으므로(편집 오버레이 장면) 캡처 모드는 EDIT 영역을 사각형·가짜 캐럿으로 대체한다 — 이 한계는 구현 때 확인한다(U16).

레이아웃은 `view_dump.rs`·Dock 시험처럼 **순수 함수의 사각형 덤프(골든)** 로도 검사해 Win32 없이 CI에서 회귀를 막는다.

## 7. 단계 계획

카드 수·작업량은 **추정**이다(근거: 1.5·2.2절의 규모 비교).

### 7.1 2단계 — 핵심 입력 먼저 (카드 약 5~6장)

| 카드 | 내용 | 산출물 |
|---|---|---|
| W2-1 | **IME 키 순서 탐침** + 빈 창 + 모델(문서 로드·표시) | `--ime-probe` 로그, 사용자가 한글 입력해 로그 제출 → U1·U2 확정 |
| W2-2 | 블록 목록 그리기(가상 스크롤·들여쓰기·불릿·접기 삼각형)·`ui_theme`·DPI | 캡처 장면 01~04·10~13 |
| W2-3 | 편집 EDIT 겹치기 + **3절 상태 기계** 구현 + 순수 시험 | 시나리오 1~10 자동 부분 통과 |
| W2-4 | 엔진 직접 호출(set_text·insert·delete·indent·outdent)·저장(SHA-256 충돌)·페이지 열기·오늘 일지 | `--replay` 스크립트 초기 10개 |
| W2-5 | 차이 0 비교기와 기준 실행기(6.3) 골격 | 최초 스크립트 통과 |
| W2-6 | 사용자 시험 빌드 | (아래 확인 지점) |

**사용자 화면 확인 지점 ①(W2-1 끝)**: 한글 IME 로그 제출(실기 필수). **확인 지점 ②(W2-6)**: 핵심 입력 체크리스트 — 한글로 문장 입력, Enter 연속, Tab/Shift+Tab, 삭제, 저장 후 다시 열기. Flutter 아웃라이너와 나란히 같은 페이지로 비교. 이 두 번을 통과하기 전에는 3단계로 넘어가지 않는다.

### 7.2 3단계 — 나머지 이식 (카드 약 5~6장)

접기·위아래 이동·TODO·링크 자동완성·사이드 페이지 목록·최근·페이지 만들기/이름 바꾸기/삭제(+확인 대화상자)·검색·역링크 패널·링크 칩·단일 인스턴스·Dock/사이드메뉴 연동(5절). 각 묶음마다 스크립트 추가. **확인 지점 ③**: 기능 전체 체크리스트(사용자). **확인 지점 ④**: 다크 모드·고DPI·고대비·큰 페이지(수천 블록) 실기.

### 7.3 4단계 — 차이 0 확인 후 제거 (카드 약 3~4장)

1. 비교기로 모든 스크립트 차이 0 (등록된 차이 외) + 사용자 "차이 허용 목록" 승인.
2. 기본 진입을 네이티브로 전환하고 **되돌리기 스위치를 둔다**: 설정 `outliner_ui = "native" | "flutter"`(기본 `native`, 문제 시 `flutter`로 즉시 복귀). Dock의 `menu_display_mode=sidebar` 복구 패턴(DOCK 설계 132행)과 같은 방식. 스위치가 있는 동안 Flutter 화면·Python 서비스는 삭제하지 않는다.
3. 사용자가 일정 기간(기준은 사용자 결정) 문제없이 쓴 뒤에만 `features/outliner/`, `outliner_service.py`, 브리지 메서드, 관련 시험을 삭제. 데이터 형식(`.md`)은 변하지 않으므로 되돌려도 자료 손실이 없다.

### 7.4 가장 큰 위험 3가지

1. **한글 IME 조합 중 명령 처리(3절).** 키 순서가 IME·Windows 버전별로 다를 수 있고(U1·U2), 시험의 핵심 부분이 자동화되지 않는다. *완화*: W2-1에서 먼저 실측, 상태 기계를 순서 무관하게 설계, 확정 후에만 글을 읽는 원칙, 사용자 실기를 확인 지점에 고정.
2. **"차이 0"의 정의.** Flutter는 키를 버리고 병합 때 공백을 넣지 않는 등 의도되지 않은 동작이 있어(1.4) 그대로 복제하면 결함을 이식하고, 고치면 차이가 생긴다. 또 역링크는 현재 사실상 미구현이다. *완화*: 허용 차이 등록부(6.3-5)로 모든 의도적 차이를 사유와 함께 사용자가 승인. 4단계에서 기능 확장(역링크·분할)은 "차이 0" 범위 밖으로 분리.
3. **전환기 동시 편집과 데이터 손실.** 파일 잠금이 없고(1.3) Flutter 탭과 새 창이 같은 `.md`를 동시에 열 수 있다. 오늘 사고처럼 저장 실패가 조용히 지나가면 글이 사라진다. *완화*: SHA-256 충돌 확인 이식(원칙 2), 저장 실패 시 EDIT 글 보존·사본 저장, 전환기에는 한쪽이 열려 있으면 다른 쪽을 읽기 전용으로 여는 규칙을 W2-4 카드에서 확정(U7).

작업량 합계(추정): 카드 약 13~16장, Rust 3,500~4,500줄(시험 포함 약 700줄). 이 수치는 측정이 아니며 W2-3 완료 시점의 실제 줄수로 재추정한다.

## 8. 하지 않는 것

- 엔진 `outliner_core`의 동작 변경(이 카드는 문서만). 엔진 한계(블록 id 비영속, 들여쓰기 1칸, 커서 분할 없음, 교차 페이지 역링크 없음)는 사용자 결정 뒤 별도 카드.
- GitLab(`origin`) 접근, 푸시, 실행 중 앱·서비스 조작.
- Direct2D 도입(DOCK 설계 23행과 같이 GDI 이중 버퍼로 충분하다고 본다. 수천 블록 스크롤 성능은 W2-2에서 측정해 재평가).

## 9. 미확인 항목 (총 15개)

| ID | 내용 | 확인 방법 |
|---|---|---|
| U1 | 한글 IME에서 조합 중 Enter·방향키·Tab이 `WM_KEYDOWN`으로 어떤 `wParam`(VK_PROCESSKEY 여부)·순서로 오는지, 확정 글자(`WM_CHAR`)가 키보다 먼저인지 | W2-1 탐침 + 사용자 실기 |
| U2 | `ImmNotifyIME(CPS_COMPLETE)`가 EDIT 글에 즉시(동기) 반영되는지, 신형(TSF) IME에서의 차이 | W2-1 탐침 |
| U3 | 오늘 사고에서 IME·키 버림(`_busy`, dart:584)의 기여 여부(직접 원인은 요청 형태 불일치로 확인됨, 3c7e9c45) | 6.3 기준 실행기로 재현 시험 |
| U4 | 엔진 호출 지연(프로세스 기동 방식 대 라이브러리 직접 호출), 블록 수천 개에서 `edit`(전체 재파싱) 시간 | W2 측정 카드(중앙값·P95) |
| U5 | 자체 그리기 목록의 접근성(UIA/MSAA) 구현 방식·비용 — 우리 코드에 선례 없음 | 구현 카드 전 소규모 시험 |
| U6 | 데스크톱 앱 쪽 설정 저장소 폴더의 실제 값(`self.gui._준말_서비스().settings._path`)과 `%APPDATA%` 대 `%LOCALAPPDATA%` 혼재 | 코드 추적 후 확인 |
| U7 | 전환기 동시 편집 정책(파일 잠금 없음), 충돌 시 사본 보존 방식 | 사용자 결정 |
| U8 | Dock이 설정과 연동된 현재 상태와 14번 라우팅 변경 시 영향(`dock_runtime.py`·`native.rs`가 기본 경로에 연결됐는지) | DOCK 구현 카드 확인 |
| U9 | 사이드메뉴 14번 클릭을 외부 exe 기동으로 바꾸는 Flutter 쪽 구체 변경(ktm은 `ktm_service.py` 경로로 알려졌으나 Flutter 호출부는 읽지 않음) | 구현 카드에서 확인 |
| U10 | `external_apps` 목록의 Logseq "아웃라이너" 항목(external_apps_service.py:58)을 어떻게 할지 | 사용자 결정 |
| U11 | 고DPI·다크 모드·고대비에서 IME 후보창의 위치·색 | 사용자 실기 |
| U13 | Enter 커서 분할·엔진 `split` 활용 여부(Flutter와 다름) | 사용자 결정 |
| U14 | 블록마다 컨트롤(B안)의 한계(USER 객체 한도 등) — 일반 지식이며 이 PC에서 측정하지 않음 | 필요 시 소규모 측정 |
| U15 | 저널 이름 규칙 통일(`YYYY-MM-DD` 대 `YYYY_MM_DD`)과 어제·내일 이동 기능 | 사용자 결정 |
| U16 | 캡처 모드에서 EDIT 자식 창을 포함하는 방법(메모리 DC 한계) | W2-2에서 확인 |

(번호 U12는 결번이다. 개수는 표의 행 수 15개.)
