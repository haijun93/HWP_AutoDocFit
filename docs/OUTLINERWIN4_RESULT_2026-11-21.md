# OUTLINERWIN4 결과 — 아웃라이너 Win32 4단계: 나머지 기능 + 앱 연결(설정으로 선택)

작성일: 2026-11-21 · 브랜치 `feature/OUTLINERWIN4` · 설계: [OUTLINER_WIN32_DESIGN_2026-11-20.md](OUTLINER_WIN32_DESIGN_2026-11-20.md) §4·§5 · 앞 카드: [OUTLINERWIN3_RESULT_2026-11-21.md](OUTLINERWIN3_RESULT_2026-11-21.md)

엔진(`outliner_core`)·`hwp-auto-docfit.py` 변경 없음. 새 외부 크레이트 없음(`windows-sys` 기능 2개 `Win32_Security`·`Win32_System_Threading` 만 추가, `Cargo.lock` 불변 — `cargo build --locked` 확인).
**엔진 보강(OUTLINERCORE1: 일지 이름 정규화·커서 분할·링크 갱신·`expected_digest`)은 아직 `github/main` 에 없어서** 이번 카드도 임시 방식(크레이트 안 구현)이다: 커서 분할 = `set_text`+`insert`, 저장 충돌 = SHA-256 비교, 일지 이름 정리 = `pages.rs`. 엔진에 들어오면 각각 한 곳만 바꾸면 된다.

## 1. 변경 파일

### `outliner_win/` (Rust, Win32 없는 로직은 전부 가짜 호스트로 시험 가능)

| 파일 | 내용 |
|---|---|
| `src/pages.rs` (새) | `PageStore`: 데이터 폴더 `.md` 목록·정렬(일반 페이지 → 일지 최신순)·만들기(`- \n`, 중복 오류)·**이름 바꾸기(링크 갱신: 엔진 `rename_page_in_files`)**·**삭제 = `.trash/<이름>.<시각>.md` 로 이동**(+`list_trash`·`restore`)·오늘의 일지(`YYYY_MM_DD`)·옛 `YYYY-MM-DD` 일지 이름 정리(링크 갱신 포함)·최근 페이지(`outliner_recent.json` 형식 그대로, 최대 20). 이름 검증은 `outliner_service.py` 규칙 + Windows 금지 문자. |
| `src/find.rs` (새) | `search_all`(페이지 이름 일치 → 블록 일치, 엔진 `search`, 2글자 미만 빈 결과, 최대 300건), `backlinks_all`(**데이터 폴더 전체**, 엔진 `backlinks`/`build_index`, 대소문자·`2026_11_21`↔`2026-11-21` 동일 취급, 자기 페이지 제외). 저장 안 한 현재 페이지는 메모리 본문으로 대체. |
| `src/links.rs` (새) | `[[` 문맥(`link_context`, UTF-16)·후보(`candidates`: 앞부분 일치 → 포함, 빈 질의는 최근 먼저)·치환(`apply_completion`: `]]` 중복 없이, 캐럿은 `]]` 뒤)·글 속 링크 구간(`segments`·`link_at`). |
| `src/shell.rs` (새) | `Shell<H>`: 현재 페이지·목록·최근·검색·역링크·이름 입력 줄. 열기(저장 후 이동)·새 페이지·이름 바꾸기·삭제(+복원)·`follow_link`(없으면 만듦)·검색 결과/역링크로 이동(접힌 조상 자동 펼침). |
| `src/layout.rs` (새) | 창 배치(위쪽 줄·왼쪽 200px·가운데·오른쪽 250px, 폭이 좁으면 오른쪽 → 왼쪽 순으로 숨김, DPI 비례)와 목록 항목 계산. 그리기와 마우스 판정이 같은 값을 쓴다. |
| `src/paint_shell.rs` (새) | 위쪽 줄(검색 칸·오늘/새 페이지/이름 바꾸기/삭제)·페이지 목록·검색 결과·역링크 패널·자동완성 팝업 그리기. 가운데는 기존 `draw_ext` 를 뷰포트로 옮겨 그대로 사용(한 파일 방식과 같은 그림). |
| `src/clock.rs` (새) | `GetLocalTime` → 일지 이름·휴지통 이름표. |
| `src/edit.rs` | **TODO 토글**(`Cmd::Toggle`, Ctrl+Enter, 편집 중에도 글 확정 후 토글·편집 유지, `toggle_block`), **블록 이동**(`MoveUp/MoveDown`, Alt+↑/↓: 하위 블록을 함께 같은 단계 형제 위로 — 엔진 `up` 인접 교환 반복), **`[[` 자동완성 팝업**(↑↓·Enter·Tab 선택, Esc 로 닫기, 조합 중 Enter 는 조합 확정만), `replace_document`·`finish_edit`. |
| `src/view.rs` | `reveal_block`(접힌 조상 펼치고 선택), `row_of_block`. |
| `src/paint.rs` | `[[링크]]` 를 강조색으로 이어 그리기, `link_spans_x`(클릭 판정용 가로 구간). |
| `src/window.rs` | 페이지 모드 창: 검색·이름 입력 칸(Win32 EDIT 2번째), 단추·목록·역링크·검색 결과·팝업 클릭, 점 클릭 접기·체크 칸 클릭 토글·링크 클릭 이동, Ctrl+클릭(편집 중 EDIT 의 링크), 휠(영역별), Alt+방향키(`WM_SYSKEYDOWN`), Ctrl+F 검색·Ctrl+N 새 페이지, **단일 인스턴스**(`activate_existing_instance`: 두 번째 실행은 기존 창을 앞으로 가져오고 끝). |
| `src/capture.rs`·`src/main.rs` | `--capture-shell`(시연 데이터 폴더로 PNG 4장), `--data-dir`·`--recent-file`, 인수 없으면 페이지 모드. `outliner_win 문서.md` 는 이전과 같은 한 파일 모드. |
| `tests/` | 새: `pages_store.rs`(9)·`find_links.rs`(10)·`shell_flow.rs`(28)·`shell_window.rs`(11)·`common/mod.rs`. |

### 앱 연결

| 파일 | 내용 |
|---|---|
| `docfit_app/settings_schema.py`·`configuration_service.py` | 설정 키 **`outliner_ui`** (`'flutter'` 기본 \| `'native'`), 설정 내보내기 대상에서 제외(`kclip_settings_mode` 와 같은 처리). |
| `docfit_app/outliner_service.py` | 기존 `outliner` 명령에 `ui_mode`(읽기/`value` 로 저장, 'flutter'·'native' 외 거부)와 `open_native`(`outliner_win.exe --data-dir <Outliner 폴더>` 실행) 두 동작만 추가. 기능 로직 없음. |
| `desktop_web_ui.py`·`docfit_app/application_service.py` | `OutlinerService` 에 설정 저장소를 넘기는 한 줄씩. **새 브리지 메서드를 만들지 않아** `_BrowserApi` 허용 목록·`dispatch` 개수는 변경 없음(`outliner` 는 이미 양쪽에 있고, `tests/test_bridge_methods.py` 에 이 경로 시험·허용 목록 확인 추가). |
| `flutter_ui/lib/features/outliner/outliner_ui_gate.dart` (새) | 설정이 `native` 면 창을 열고 탭에는 안내(`창 다시 열기`·`앱 안에서 보기`)만. `flutter` 면 기존 화면 위에 `새 창(Win32)으로 열기` 단추 한 줄(창이 실제로 열릴 때만 설정을 native 로 저장, 실패하면 이유만 알리고 설정 불변). `workspace_adapters.dart` 는 한 줄 교체. |
| `HWP_AutoDocFit.spec`·`.github/workflows/build-windows-exe.yml`·`installer/setup.iss`·`.gitignore` | mdm-ctx 와 같은 방식: CI 에서 `cargo test --release --locked` → `cargo build --release --locked --bin outliner_win` → `resources/outliner_win.exe`, spec 은 있으면 번들, VC++ 런타임 비의존 검사 목록에 추가, 설치 전 `StopHelper('outliner_win.exe')`. `tests/test_distribution_build.py` 에 확인 추가. `.cargo/config.toml` 등 이 PC 전용 경로는 저장소에 넣지 않았다(시험으로 확인). |

## 2. 동작 요약

- **데이터 폴더**: `--data-dir` > 환경변수 `OUTLINER_WIN_DATA` > `%APPDATA%\HwpAutoDocFit\Outliner`(= `outliner_service.py` 와 같은 위치). 앱이 연 창은 항상 `--data-dir` 로 정확한 폴더를 넘긴다. 최근 목록: `--recent-file` > `OUTLINER_WIN_RECENT` > `%LOCALAPPDATA%\HwpAutoDocFit\outliner_recent.json`(Flutter 와 공유).
- **시작**: 오늘의 일지(`2026_11_21.md`)를 열고 없으면 만든다. Flutter 가 만든 `2026-11-21.md` 는 열 때(오늘·링크 클릭·목록 클릭) `2026_11_21.md` 로 정리하고 `[[2026-11-21]]` 링크도 갱신한다(두 이름이 모두 있으면 건드리지 않음).
- **페이지 전환 시 자동 저장**: 다른 페이지를 열거나 이름을 바꾸거나 지우기 전에 편집 중 글을 확정하고 변경이 있으면 저장한다(SHA-256 충돌이면 이동하지 않고 경고 대화상자 + 머리글 안내). Ctrl+S 는 그대로 있다.
- **새 페이지·이름 바꾸기**: 대화상자 대신 **위쪽 입력 줄**을 쓴다(IME 안전: 같은 Win32 EDIT). Enter 확정·Esc 취소, 오류(중복·금지 문자)는 줄을 그대로 두고 안내. 이름 바꾸기: 파일 이동 후 모든 페이지의 `[[옛 이름]]` 갱신(코드 칸·다른 이름은 건드리지 않음 — 엔진 규칙), 현재 페이지를 새 이름으로 다시 읽음.
- **삭제**: 확인 대화상자("삭제하면 `.trash` 폴더로 옮겨지며…") → `.trash/<이름>.<시각>.md` 이동, 오늘 일지를 연다. 복원은 API(`Shell::restore_last`)만 있고 화면 단추는 없다(파일 탐색기에서 되돌릴 수 있음).
- **검색**: 위쪽 칸에 2글자부터 입력 즉시 결과(Enter 불필요). 결과 행 = 페이지 이름 + 블록 글. ↑↓ 선택·Enter 또는 클릭 → 그 페이지의 그 블록으로 이동. Esc 로 닫기.
- **링크**: 글 속 `[[페이지]]` 는 강조색, 클릭하면 이동(없으면 새로 만듦). 편집 중에는 Ctrl+클릭. `[[` 를 입력하면 팝업(최대 8개, 빈 질의는 최근 페이지 먼저). **역링크 패널**은 데이터 폴더 전체에서 현재 페이지를 가리키는 블록을 페이지별로 묶어 보이고 클릭하면 이동(Flutter 는 현재 페이지 본문만 봤다).
- **TODO·접기·이동**: Ctrl+Enter 또는 체크 칸 클릭 = TODO → DONE → TODO. ←/→ 또는 점·삼각형 클릭 = 접기/펼치기. Alt+↑/↓ = 블록(하위 포함)을 같은 단계 형제 위/아래로.

### Flutter 와 다른 점(알려 두는 것)
1. **Ctrl+Enter = TODO 토글**(Flutter 에서는 접기). 접기는 ←/→·점 클릭.
2. **Alt+↑/↓ 는 하위 블록을 함께 옮긴다**(Flutter·엔진 `up/down` 은 한 줄만 교환해 부모와 자식이 어긋났음). 같은 단계 형제가 없으면(부모 밖으로) 안내만 한다.
3. 역링크는 데이터 폴더 전체(자기 페이지 제외), 이름 바꾸기는 링크 갱신, 삭제는 `.trash` — 모두 사용자 확정 사항(TODO 결정 기록).
4. 편집은 Ctrl+S 로 저장하고 페이지를 옮길 때 자동 저장한다(Flutter 는 편집마다 저장).

## 3. 캡처 검증 — PNG 를 직접 열어서 본 것

생성: `outliner_win --capture-dir <폴더> --capture-shell [--dark]` → 실제(화면 밖) 창 + 실제 EDIT, 시연용 임시 데이터 폴더(`<폴더>/data`, 페이지 6개)를 읽어 그린 결과이며 아무것도 저장하지 않는다.
경로: `C:\Users\haiju\AppData\Local\Temp\claude\ow4target\capture4\outliner_shell_{pages,search,popup,backlinks}_{light,dark}_96dpi.png` (1000×600, 저장소 밖; 같은 명령으로 재생성).
8장을 모두 열어 확인했고(밝게 4·어둡게 4), 항목별 관찰:

- **페이지 목록 패널(왼쪽)**: 머리글 "페이지", "최근 페이지"(2026_11_21·회의록·예산 지침) 아래 "전체 페이지 (6)" 에 업무 매뉴얼·예산 지침·주간 업무·참고 자료·회의록·2026_11_21 이 가나다 순으로, 일지는 맨 뒤. 현재 페이지 줄이 선택색(밝게 하늘색·어둡게 짙은 파랑)으로 칠해지고 최근·전체 목록 양쪽에서 같이 강조된다. 글자 읽힘 양호.
- **검색 결과**: 위쪽 칸에 "예산" 입력 → 가운데가 "검색 결과 8건 — 예산" 목록으로 바뀜. 맨 위 행 `예산 지침 (페이지 이름)`, 이어서 예산 지침·주간 업무·참고 자료·회의록(2)·2026_11_21(2)의 블록 글이 페이지 이름(강조색 굵게) + 본문으로 나온다. 첫 행이 선택색. 왼쪽·오른쪽 패널은 그대로 보인다.
- **링크 자동완성 팝업**: "점심 약속 확인 [[" 편집 중, 편집 상자 바로 아래에 테두리(강조색)가 있는 팝업에 후보 6개(2026_11_21·회의록·예산 지침·업무 매뉴얼·주간 업무·참고 자료)가 보이고 첫 항목이 선택색. 머리글 제목 앞에 ● (저장 안 한 변경). 위 줄의 `[[예산 지침]]` 은 강조색 링크로 보인다.
- **역링크 패널(오른쪽)**: 예산 지침 페이지에서 "역링크 (3)" → 주간 업무 `[[예산 지침]] 부서 회신 정리`, 회의록 `[[예산 지침]] 변경 사항 공유`, 2026_11_21 `[[예산 지침]] 검토` 가 페이지 이름(강조색)별로 묶여 나온다(여러 파일). 오늘 일지에서는 "역링크 (1)" 로 회의록의 `다음 일정은 [[2026_11_21]] 일…`(긴 글은 …로 잘림).
- 다크 4장도 같은 구성이며 배경 짙은 남색·글자 밝음·강조 하늘색 링크·팝업/선택 색 모두 읽힌다. 빈 화면·가짜 시안 없음(화소 시험이 목록·패널·팝업 테두리·검색 칸 글자·마젠타 0개를 밝게/어둡게 모두 확인).
- 처음부터 맞게 나온 것은 아니다: 시험을 쓰면서 화소 비교의 색 배치(BGR↔COLORREF)를 틀려 한 번 실패했고(시험 쪽 실수), 한글 이름 `.md` 처리에서 바이트 경계 패닉(`pages.rs`)을 시험이 잡아 고쳤다.

## 4. 시험 (실제 결과 줄)

```
cargo test --release --locked   (outliner_win)
 unittests src\lib.rs            test result: ok. 6 passed
 tests\edit_machine.rs           test result: ok. 30 passed      (기존 유지)
 tests\edit_window.rs            test result: ok. 4 passed       (기존 유지)
 tests\layout_and_capture.rs     test result: ok. 10 passed      (기존 유지)
 tests\save_io.rs                test result: ok. 6 passed       (기존 유지)
 tests\pages_store.rs            test result: ok. 9 passed       (새)
 tests\find_links.rs             test result: ok. 10 passed      (새)
 tests\shell_flow.rs             test result: ok. 28 passed      (새)
 tests\shell_window.rs           test result: ok. 11 passed      (새)
```
outliner_win 합계 **114개 통과, 실패 0**(이전 56 유지 + 새 58). `Cargo.lock` 불변.

- `pages_store`: 만들기·금지 이름·중복, 정렬(일반→일지 최신순, `.trash`·txt 제외), 0바이트 채움, **이름 바꾸기가 여러 파일의 링크를 갱신**(코드 칸·다른 이름 불변, 대소문자만 바꾸기 허용, 실패 시 불변), **삭제가 `.trash` 로 이동·내용 보존·복원**, 일지 `YYYY_MM_DD` 생성·옛 이름 변환+링크 갱신, 최근 목록 왕복·20개 제한·깨진 파일.
- `find_links`: 전체 검색(페이지 이름→블록, 2글자 규칙, 메모리 본문), **역링크(여러 파일)**·대소문자·일지 이름 변형·자기 제외·메모리 본문, 자동완성 문맥·후보 정렬·치환(이모지·`]]` 중복)·링크 구간 UTF-16.
- `shell_flow`(가짜 EDIT): 시작·열기 전 저장·충돌 시 이동 안 함·이름 바꾸기/삭제/복원·입력 줄 오류 유지·링크 따라가기(없으면 생성·옛 일지 정리)·검색 즉시/닫힘·결과 이동(접힌 조상 펼침)·역링크 이동, **TODO 토글**(편집 중/목록/다른 줄 체크 칸), **블록 이동**(하위 동반 위/아래·형제 안·가장자리 오류·접힘 유지·편집 중 Alt 키·조합 중), **자동완성**(`[[` 열기·필터·↑↓ Enter/Tab 선택·Esc 재열림 금지·조합 중 Enter 는 확정만·후보창·선택 영역·Enter 분할 기존 동작).
- `shell_window`(실제 창·EDIT): 배치·좁은 폭, 화소(밝게·어둡게: 왼쪽·오른쪽·가운데·위쪽 줄 글자, 현재 페이지·링크·결과·팝업 테두리, 마젠타 0), 클릭(목록·역링크·단추 새 페이지/이름 바꾸기/오늘·체크 칸·점 접기·링크·검색 결과·팝업 후보), 키.

```
python -m unittest tests.test_outliner_service tests.test_bridge_methods tests.test_distribution_build tests.test_configuration_service tests.test_flutter_settings_gui
Ran 33 tests ... OK
flutter analyze                  → No issues found!
flutter test test/outliner_ui_gate_test.dart test/outliner_workspace_test.dart   → +12: All tests passed!   (새 7 + 기존 5)
flutter test (workspace_adapters·external_apps_sidebar·sidebar_menu_settings·app_info_panel) → +16: All tests passed!
flutter build web --release      → √ Built build\web
```
스모크(실제 실행 파일, 임시 폴더·전용 뮤텍스 접미사·전용 최근 파일): 페이지 모드 창이 뜨고 2초 생존·`2026_11_21.md` 생성, **두 번째 실행은 즉시 종료하고 첫 창은 유지**, 닫으면 정상 종료(제 프로세스만 다룸). 시작 전 실행 중인 `outliner_win` 프로세스는 0개였다.
전체 시험은 돌리지 않았다(지시).

## 5. 사용자 실기로 확인할 것

1. **한글 조합 중 `[[` 입력**: `[[예` 처럼 조합 중인 글자가 있을 때 팝업이 뜨고 후보가 따라가는지, 조합 중 Enter 가 조합 확정만 하고(블록이 나뉘지 않고) 다음 Enter 가 선택하는지, 선택 뒤 캐럿이 `]]` 뒤인지. (U1·U2 미확인 상태는 3단계와 같다.)
2. **검색 칸에 한글 입력**: 조합 중 결과가 따라오는지(조합 글자 포함 여부), 조합 중 Enter/Esc 가 IME 소유로 남는지, 결과에서 ↑↓·Enter.
3. **이름 입력 줄**(새 페이지·이름 바꾸기)에서 한글 입력·Enter 확정·Esc 취소.
4. **Ctrl+클릭으로 편집 중 링크 이동**(키 상태를 시험이 만들 수 없어 자동 시험 없음)과 일반 클릭 이동.
5. 이름 바꾸기 후 **다른 페이지의 링크가 실제로 고쳐지는지**(실제 데이터 사본으로), 삭제 후 `.trash` 폴더 내용·탐색기에서 되돌리기.
6. 앱에서: 설정 `outliner_ui` 를 native 로 바꾼 뒤(탭의 "새 창(Win32)으로 열기" 단추) 사이드메뉴 14번 → 창이 뜨고 탭에 안내만 나오는지, **같은 파일을 Flutter 화면과 창에서 동시에 열었을 때** 나중 저장이 경고를 받는지. 다시 눌러도 창이 하나만 뜨는지.
7. 고DPI(125·150·200 %)·창 폭 줄였을 때 패널 숨김·모니터 간 이동. 폭이 좁아져 오른쪽 패널이 사라지면 역링크를 볼 수 없다(900px 미만).
8. 긴 페이지 목록·검색 결과의 휠 스크롤.

## 6. 미확인·알려진 한계

- **IME 실기 미확인**(U1·U2, 위 1~3). 가짜 호스트 시험은 순서 다양성에 견딤을 보일 뿐 실제 IME 증거가 아니다.
- **CI 의 `cargo test --release --locked`** 는 창을 만드는 시험(`edit_window`·`shell_window`)을 포함한다. 이 PC 에서는 통과하지만 GitHub 실행기(세션·글꼴 환경)에서 같은 결과인지는 확인하지 못했다. 막히면 해당 시험만 CI 에서 제외하면 된다.
- 팝업의 첫 후보에 **현재 페이지**가 포함될 수 있다(빈 질의의 최근 목록 맨 앞). 걸러내지 않았다.
- 일지는 "오늘" 단추뿐이고 어제·내일 이동은 없다(Flutter 도 없었음). 일지 이름은 `YYYY_MM_DD` 만 새로 만든다.
- 이름 바꾸기 링크 갱신은 엔진 규칙(`[[이름]]` 일치, 별칭 `[[이름|표시]]`·블록 참조 `((id))` 는 갱신하지 않음)을 따른다. 링크 갱신이 중간에 실패하면 "이름은 바꿨지만 링크를 갱신하지 못했다"고만 알린다(롤백 없음).
- 휴지통 복원 단추·페이지 목록의 키보드 이동·페이지 목록 클릭 시 한 번에 열림(더블클릭 구분 없음)은 이번 범위 밖. 왼쪽 목록·역링크는 마우스 전용이다.
- 목록 영역 클릭에서 편집 중 조합이 있는 상태로 클릭하면 부모 창이 포커스를 가져가므로 조합이 끊긴다(3단계와 같은 기존 동작).
- 단일 인스턴스는 창 클래스 이름으로 기존 창을 찾는다. 같은 PC 의 다른 사용자 세션과는 구분된다(`Local\` 뮤텍스).
- 설정 `outliner_ui` 를 바꾸는 화면은 Flutter 탭의 한 줄 단추(`새 창(Win32)으로 열기`)와 `앱 안에서 보기` 뿐이다. 설정 창 항목은 만들지 않았다.
- 앱 연결은 이 PC 에서 실제 앱(HwpAutoDocFit)을 띄워 확인하지 않았다(지시: 실행 중 앱 불가침). 브리지·서비스는 단위 시험, Flutter 는 위젯 시험으로만 확인했다.
- `tests.test_tk0_direct_reads` 를 한 번 돌렸을 때 임시 폴더 정리 중 `PermissionError`(사용자 PC 의 상주 프로그램이 임시 sqlite 를 붙잡고 있음)가 났다. 이번 변경과 무관한 환경 문제로 보고 결과에 넣지 않았다.
