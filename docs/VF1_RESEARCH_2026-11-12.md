# VF1 조사: 가상 폴더를 단축키로 부르는 경량 창 (2026-11-12)

작업 폴더 `E:/docfit-tmp/vfp`, 브랜치 `feature/vfolder-popup`, 기준 커밋 `d3173daa`. 코드는 고치지 않았다. 계획은 `docs/VF1_PLAN_2026-11-12.md`.

## 1. 무엇을·왜

사용자 지시(2026-11-12): "가상 폴더는 단축키로 호출되는 몽땅클립보드와 같이 단축키로 호출되어 단독으로 실행될 수 있도록 하고, UI는 몽땅클립보드와 같은 방식, 즉 경량화·최소화·램 상주 가능한 방식으로 설계하여 구현."

지금 가상 폴더는 앱 본체(Flutter 화면 + Python 호스트)를 띄워야만 볼 수 있다. 자료를 꺼내 쓰는 일(찾기 → 열기)은 몽땅클립보드처럼 단축키 한 번으로 끝나야 한다.

## 2. 지금 가상 폴더가 저장되는 곳과 읽고 쓰는 쪽

### 2.1 저장 위치

| 무엇 | 어디 | 근거 |
|---|---|---|
| 자료 폴더 | `%LOCALAPPDATA%\HwpAutoDocFit\library` (시험은 `KLIB_TEST_ROOT`) | `klib/src/util.rs:21` |
| DB | 자료 폴더의 `library.db`, WAL 방식, 외래 키 켬 | `klib/src/main.rs:193`, `klib/src/db.rs:72-73` |
| 상자(가상 폴더) | 표 `box`(id, name, description, created_at, updated_at, export_path) | `klib/src/db.rs:75-82` |
| 항목 | 표 `item`(id, box_id, kind, source_path, display_name, space, x, y, created_at) + 나중에 붙인 열 `origin_path`, `managed_path` | `klib/src/db.rs:84-95`, `113-119` |
| 관계선 | 표 `relation` | `klib/src/db.rs:97-105` |
| 영역 | `item.space` 값 `core`·`basis`·`reference`·`temp` = 화면 이름 핵심자료·기초자료·참고자료·임시저장소 | `klib/src/db.rs:254`, `flutter_ui/lib/features/library/library_screen.dart:101-104` |
| 사본 파일 | `자료 폴더\<box_id>\<space>\<파일 이름>` (`library_root()` = 자료 폴더) | `klib/src/db.rs:257`, `990` |
| 본문 색인 | `자료 폴더\tantivy` (`KLIB_TANTIVY_DIR`로 바꿀 수 있음) | `klib/src/main.rs:39` |

경로 세 가지의 뜻:
- `managed_path`: klib가 만든 사본의 경로. 링크(`kind=link`)는 빈 글자. 사본이 있는 항목은 `source_path == managed_path`여야 한다(`klib/src/db.rs:321`).
- `origin_path`: 사본을 만들 때의 원본 경로(기록용). 원본이 없었으면 빈 글자(`klib/src/db.rs:344`, `410-414`).
- `managed_path`가 빈 글자인 파일 항목 = 예전 DB에서 온, 아직 사본으로 옮기지 않은 항목. 열기 직전에 `item_prepare`가 사본을 만든다. 실패해도 원본 경로로 대신 열지 않는다(`klib/src/db.rs:332-359`).

### 2.2 쓰는 쪽 (모두 klib.exe 한 곳)

| 동작 | 코드 | 원본·사본에 하는 일 |
|---|---|---|
| 항목 추가 | `Database::item_add` `klib/src/db.rs:361-443` → `create_copy` `263-307` | 원본을 읽어 사본을 새로 만든다(`create_new`, 덮어쓰지 않음). 링크·정션 거부(`395`) |
| 영역 이동 | `item_move` `klib/src/db.rs:527-556` | 사본을 다른 영역 폴더로 옮김(`fs::rename`) |
| 삭제 | `item_delete` `klib/src/db.rs:558-570` | 등록된 사본만 지움. 원본은 손대지 않음 |
| 글 자료 고치기 | `item_text_set` `klib/src/db.rs:503-525` | 사본 `.txt`만 |
| 상자 삭제 | `box_delete` `klib/src/db.rs:239-249` | 사본 폴더 통째로 |
| 사본 준비 | `item_prepare` `klib/src/db.rs:333-359` | 예전 항목의 사본을 처음 만들고 표 `item_path_backup`에 원래 경로 보관 |
| 색인 | 파이프 `item_add` 뒤 `content::index_file` `klib/src/pipe.rs:997-1045` | tantivy 색인 갱신 |

### 2.3 klib.exe를 부르는 길

- klib.exe는 상주하지 않는다. 요청을 받으면 뜨고 60초 쉬면 끝난다(`klib/src/main.rs:2`, `203-228`). 한 번에 하나만 뜬다(뮤텍스 `Local\HwpAutoDocFit.klib`, `klib/src/main.rs:168`).
- 파이프 이름 `\\.\pipe\HwpAutoDocFit.klib<시험 접미사>.<사용자 SID>`(`klib/src/pipe.rs:115-119`), 현재 사용자와 시스템만 허용(`klib/src/pipe.rs:325`), 요청은 한 줄 JSON 64KB 이하(`klib/src/pipe.rs:55`), 동시 연결 4개·요청은 하나씩 처리(`klib/src/pipe.rs:124`, `316-318`).
- 읽기 명령: `box_list`(`klib/src/pipe.rs:783`), `box_get`(`829`), `item_get`·`item_prepare`(`1139`), `item_search`(`1372`), `search`(`1047`).
- 앱에서의 경로: Flutter `BridgeLibraryApi`(`flutter_ui/lib/features/library/library_api.dart:416`) → Python `LibraryBridge.request`(`docfit_app/library_bridge.py:98`) → 파이프. klib.exe가 안 떠 있으면 Python이 띄운다(`docfit_app/library_bridge.py:182-196`). 실행 파일 위치는 앱 설치 폴더의 `resources\klib.exe`, 개발 중에는 `klib/target/release/klib.exe`(`docfit_app/library_bridge.py:82-84`).
- 앱 꾸러미는 폴더 방식(PyInstaller `COLLECT`)이고 klib.exe는 `resources`에 들어간다(`HWP_AutoDocFit.spec:12-14`, `34-37`).
- 한 번 실행 방식 `klib.exe --request-json <요청>`도 있다(`klib/src/main.rs:142-164`). 이 길은 뮤텍스·파이프를 쓰지 않고 DB를 직접 연다.

### 2.4 읽는 쪽 (앱 화면)

- 가상 폴더 화면: `flutter_ui/lib/features/library/library_screen.dart`(2,273줄). 영역 이름 `101-104`, 탐색기에서 끌어 넣기 `296-317`(기본 영역은 핵심자료, 탐색기 형식에서 영역을 열어 두었으면 그 영역), 열기 `839` 부근, 삭제 확인 `898`.
- 열기 규칙(Python): `open_library_item` `docfit_app/library_bridge.py:303-357`. `managed_path`가 있으면 그것, 없으면 `source_path`. 링크는 http/https만, `file:`·`javascript:`·`shell:`·`ms-` 차단, 실행 파일은 확인을 받은 뒤에만, 없으면 `MANAGED_MISSING`·`SOURCE_MISSING`.
- 탐색기에서 위치 열기는 `explorer /select,<경로>`를 쓴다(`desktop_web_ui.py:2775`, `2818`).
- 끌어 넣기는 Python 호스트가 받는다(`desktop_web_ui.py:2560`의 `lib_drop_watch`). 그 안쪽 구현은 **미확인**.

## 3. 몽땅클립보드에서 그대로 가져올 것

### 3.1 프로세스·자원 원칙 (`kagent/src/main.rs:3-7`)

- 이벤트로만 깨어난다. 폴링·주기 타이머 없음 → 유휴 CPU 0.
- 창은 메모리에 둔 목록으로 바로 그린다.
- SQLite는 Windows에 들어 있는 `winsqlite3.dll`을 실행 중에 불러 쓴다. 실행 파일에 SQLite를 넣지 않는다(`kagent/src/sqlite.rs:1-2`, `46-83`).
- 네트워크 코드 없음. 의존성은 `windows-sys`·`png`·`regex`뿐(`kagent/Cargo.toml`).
- 실측(문서): 개인 메모리 1.8MB, 감시를 켠 상태 1.95MB·유휴 CPU 0.0%, 실행 파일 0.72MB(`docs/KAGENT_DESIGN.md:27-33`, `39`). 이 세션에서 다시 재지는 않았다.

### 3.2 창 틀·그리기 (`kagent/src/popup.rs`)

| 가져올 것 | 위치 |
|---|---|
| 도구 창(작업 표시줄 단추 없음) `WS_EX_TOOLWINDOW`, `WS_POPUP \| WS_THICKFRAME` | `popup.rs:431-463` |
| 틀 없애기(`WM_NCCALCSIZE`), 테두리 한 겹(DWM 또는 직접 1px), DWM 그림자·둥근 모서리 | `popup.rs:19-22`, `235-267`, `1195` |
| 가장자리 6px 크기 조절, 머리글 끌어 옮기기(순수 함수 `frame_hit`) | `popup.rs:193-233`, `1196-1208` |
| 머리글 단추(같은 폭 46, Segoe MDL2 글리프, 닫기는 빨강) | `popup.rs:136-188` |
| 화면 배율 `scale`, 모니터 작업 영역 안에 놓기, 저장한 위치·크기 되살리기 | `popup.rs:465-468`, `503-543` |
| 숨김과 최소화의 구분(닫기 = 다음에 처음 상태, 최소화 = 상태 유지) | `popup.rs:571-582` |
| 찾기 글자 입력(한글 조합 중 글자 포함), 늦게 온 결과 버리기(`seq`) | `popup.rs:323-326`, `357-401`, `1277-1319` |
| 목록 줄 위치 계산 `row_index`, 분류 단추 배치 `tab_layout` | `popup.rs:102-132`, `272-283` |
| 최소·최대 크기, 위치 저장(`WM_EXITSIZEMOVE`) | `popup.rs:1154-1179` |

`paint`(`popup.rs:750-961`)와 키·마우스 처리(`1218-1460`)는 줄 단위로 읽지 않았다 — 세부는 **미확인**.

### 3.3 단축키·트레이·상태 파일 (`kagent/src/main.rs`)

| 가져올 것 | 위치 |
|---|---|
| 단축키 글자 풀기 `parse_hotkey`(Ctrl·Shift·Alt·Win + 영숫자·F1~F24) | `main.rs:695-717` |
| 등록 `register_hotkey`, 실패 알림 판정 `hotkey_failure_notice`(같은 조합의 연속 실패는 한 번만), 알림 글 `hotkey_failure_body`, 트레이 풍선 `notify_hotkey_failure` | `main.rs:719-754` |
| `WM_HOTKEY` → `popup::show()` | `main.rs:939-942` |
| 트레이 아이콘·말풍선·메뉴 | `main.rs:809-866` |
| 상태 파일 `status.json`(바뀔 때만 쓰기, 임시 파일 뒤 바꿔치기) | `main.rs:404-455` |
| 한 번만 실행(뮤텍스), 이미 떠 있으면 그쪽에 `WM_SHOW`만 보내고 끝 | `main.rs:1021-1045` |
| 설정 `settings.json`(`hotkey` 기본 `Ctrl+Shift+V`, 창 위치·크기, 항상 위, 붙박이) | `kagent/src/store.rs:19-73`, `141-176` |
| 설치·교체 절차(`--install`, `--upgrade-if-stale`, 자료 폴더의 `kagent.exe`로 복사) | `main.rs:1292-1370`, `kagent/src/lifecycle.rs` |
| 열기 `ShellExecuteW`, 실행 파일 확장자 차단 목록, 웹 주소 검사 | `kagent/src/open_item.rs:11`, `17-27`, `85-87`, `130-133` |

### 3.4 그대로 가져오면 안 되는 것

- 붙여넣을 창 기억·포커스 유지(`popup.rs:479-486`, `1184-1193`)와 붙여넣기(`paste_into`, `popup.rs:648`): 가상 폴더 창은 파일을 여는 것이 목적이라 필요 없다.
- 클립보드 분류(`category.rs`)·저장소(`store.rs`)·암호화(`crypt.rs`).
- 열 때마다 `refresh_recent()`가 `status.json`과 트레이를 갱신하는 흐름(`main.rs:391-402`): 가상 폴더 창에는 상주 목록이 없다.

## 4. 새로 필요한 것

1. **library.db 읽기 전용 열기**: `kagent/src/sqlite.rs`에는 읽기·쓰기+만들기 열기만 있다(상수 `sqlite.rs:15-17`). 읽기 전용 열기(`SQLITE_OPEN_READONLY`)가 없다.
2. **klib 자료 폴더 경로**: kagent는 자기 자료 폴더(`...\HwpAutoDocFit\kagent`, `kagent/src/util.rs:12-22`)만 안다. `...\HwpAutoDocFit\library`와 `KLIB_TEST_ROOT`를 아는 함수가 필요하다.
3. **두 번째 단축키**: `register_hotkey`는 id 하나(`HOTKEY_ID = 1`), 실패 기억 하나(`LAST_FAILURE`), 알림 제목이 '몽땅클립보드'로 고정이다(`main.rs:59`, `736`, `748`).
4. **탐색기에서 위치 열기**: `shell_open`은 인수 없이 경로만 연다(`open_item.rs:130`). `explorer.exe /select,"경로"` 호출이 없다.
5. **가상 폴더 창 모듈**: 상자 고르기, 영역 칸, 목록, 찾기, 열기. `popup.rs`의 틀 함수는 모두 비공개라 다른 모듈에서 못 쓴다.
6. **명시적 창 상태**: `popup.rs`의 상태는 `minimized` 플래그 + `IsWindowVisible` + 설정의 `sticky_popup`·`popup_collapsed` + `restore_rect`에 흩어져 있다(암묵적 상태 기계, `popup.rs:317-338`, `473-476`, `571-582`). 새 창은 처음부터 상태 값 하나로 만든다.
7. **상태 파일·명령줄**: 가상 폴더 창 상태를 `status.json`에 싣고, `kagent.exe --vfolder`로 부를 수 있게 한다.
8. (다음 범위) **쓰기 길**: 끌어 넣기 추가는 사본 만들기·링크 거부·색인이 klib 안에 있어 klib.exe가 있어야 한다. kagent 자료 폴더에는 klib.exe가 없다(설치 절차는 kagent.exe만 복사, `main.rs:1300`).

## 5. 확정된 사용자 결정

| 번호 | 결정 | 출처 |
|---|---|---|
| D1 | 가상 폴더를 단축키로 부른다 | 사용자 지시 2026-11-12 |
| D2 | 앱 본체 없이 단독으로 실행된다 | 같음 |
| D3 | UI는 몽땅클립보드와 같은 방식: 경량·최소화·램 상주 가능 | 같음 |
| D4 | 실행 속도와 최소 자원이 가장 중요(몽땅클립보드 원칙) | 사용자 지시 2026-10-31, `kagent/src/main.rs:3` |
| D5 | 암묵적 상태 기계를 드러내고 UX·DX·AX를 모두 올리며 아무것도 깨뜨리지 않는다 | `docs/TEAM_OPERATING_RULES.md` 0절 |
| D6 | 사용자 문서 원본은 고치지 않는다 | `AGENTS.md`, 운영 규칙 6절 |
| D7 | 영역 이름은 핵심자료·기초자료·참고자료·임시저장소 | `library_screen.dart:101-104` |
| D8 | 사본 준비에 실패해도 원본 경로로 대신 열지 않는다 | `klib/src/db.rs:332` |
| D9 | GitLab은 건드리지 않고, 새 기능은 기능 브랜치에서 만든다 | `AGENTS.md` |

## 6. 확인이 필요한 것

사용자 결정이 필요한 것(답이 없으면 괄호 안의 되돌리기 쉬운 쪽으로 간다):

| 번호 | 물음 | 기본값 |
|---|---|---|
| Q1 | 단축키 기본값. 후보 `Win+Shift+F`(권고), `Ctrl+Shift+F`, `Ctrl+Alt+F` | `Win+Shift+F` — 설정 값이라 바꾸기 쉽다 |
| Q2 | 몽땅클립보드를 켜지 않은 사용자에게도 가상 폴더 단축키를 줄 것인가. 주려면 kagent를 '기록 일시정지' 상태로 띄워야 한다 | 이번에는 kagent가 떠 있을 때만 동작. 앱의 동의·켜기 흐름은 바꾸지 않는다 |
| Q3 | 사본을 아직 만들지 않은 예전 항목(`managed_path` 빈 글자)을 창에서 열 때 | 열지 않고 "앱에서 한 번 열어 주세요"라고 알린다(D8과 같은 방향) |
| Q4 | 끌어 넣기 추가(쓰기)를 이번에 넣을 것인가 | 다음 범위. 이번에는 읽기와 열기만 |

실기·측정으로만 알 수 있는 것:

| 번호 | 확인할 것 |
|---|---|
| C1 | `winsqlite3.dll`의 읽기 전용 연결이 WAL 방식 `library.db`를 읽는지(klib.exe가 떠 있을 때·꺼져 있을 때·`-wal` 파일이 남아 있을 때). 안 되면 '읽기·쓰기 열기 + `PRAGMA query_only=ON`'으로 간다 |
| C2 | `Win+Shift+F`를 Windows 11·한/글 2020·Excel이 쓰는지 |
| C3 | 창을 추가한 뒤 kagent 개인 메모리·유휴 CPU·실행 파일 크기 |
| C4 | 항목 1,000개 상자에서 단축키부터 목록 표시까지 걸리는 시간 |

읽지 않아 **미확인**인 것: `popup.rs`의 `paint`(750~961줄)·키와 마우스 처리(1218~1460줄) 세부, `kagent/src/settings_menu.rs` 본문, `kagent/src/pipe.rs`의 서버 본문, `kagent/src/store.rs`의 `Settings` 외 부분, `klib/src/db.rs` 625줄 이후(관계선·내보내기), `klib/src/search.rs`, `klib/src/scan.rs`, `library_screen.dart`의 대부분과 `library_explorer_view.dart`, `docfit_app/kagent_app_service.py`(앱이 kagent 설정을 바꾸는 길), `desktop_web_ui.py`의 가상 폴더 처리부, `kagent/tests/khot_live.py`의 25줄 이후.

이 세션에서 시험을 돌리지 못했다: kagent `cargo test`가 기본 툴체인(gnu)에서는 `-ldwmapi`를 찾지 못해 링크에 실패했고, msvc 툴체인은 Bash에서 `link.exe` 충돌로 실패했으며, PowerShell의 cargo 실행은 권한에서 막혔다. 시험 수는 소스의 `#[test]` 개수로만 셌다: kagent 109개, klib 113개(단위 65 + `klib/tests/test_klib.rs` 48).
