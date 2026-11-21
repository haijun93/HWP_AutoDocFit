# SETTINGSWIN4 결과 — 목록·단축키 키 대응 + Python 설정 서비스 의존 줄이기 설계

작성: 2026-11-22(카드 날짜 기준 파일명 2026-11-21) · 브랜치 `feature/SETTINGSWIN4` · 근거: `docs/SETTINGSWIN3_RESULT_2026-11-21.md`, `docs/SETTINGS_WIN32_DESIGN_2026-11-20.md` ⑤·⑧·부록 A, `docs/DEV_PLAN_2026-11-28.md`(G2)

## 1. 목록·단축키 키 전수 조사와 대응

조사 범위: 앱 `settings.json` 스키마(`DEFAULTS` 97개 최상위) + 파일에 실제로 저장되는 스키마 밖 키(코드에서 `settings.update(...)`·`SETTING_KEY` 로 찾음) + Flutter 설정 화면이 다루는 항목.

| 키 | 실제 모양 | 소유·읽는 곳 | 이번 결정 |
|---|---|---|---|
| **`hidden_tabs`** | 정수 배열(1~15, 12·13 제외, 오름차순) | 쓰기 `application_service.set_hidden_tabs`·읽기 Dock(`dock/src/lib.rs` `parse_hidden_tabs`)·앱 | **목록(메뉴 번호)으로 대응**. 일반 묶음. 패널에 "3  다이어그램"처럼 이름 표시, 번호 목록은 늘 오름차순(▲▼ 무동작) |
| **`library_excluded_folders`** | 문자열 배열(폴더 경로) | `library_exclusions.py`(문서뷰·사진뷰 제외 폴더) | **목록(폴더)으로 대응**. 프로그램·네트워크 묶음. 끝 구분자·대소문자 무시 중복 거부, 예약 문자 거부 |
| `abbreviations` | 객체(준말 → 서식 정보 객체) | `abbreviation_service.py`; 기본 출처는 준말 관리 HWPX 문서(`abbreviation_source="document"`) | **대응하지 않음**(목록이 아님 — 값이 서식 객체. 준말 관리 패널이 정본). 내부 키 그대로 |
| `external_app_paths` | 객체(앱 id → 실행 파일 경로) | `external_apps_service.py` | 대응하지 않음(앱별 경로 대화상자가 정본, "키-경로 표" 컨트롤은 새 종류라 별도 결정) |
| `stage_choices`·`ui_thumb_sizes`·`kclip_window`·`sidebar_collapsed`·`sidebar_mode_*`·`last_run_version`·`card_options_rev` | 화면 상태·카드 상태 | 서비스 각자 | 설정 항목이 아니라 상태 — 대응하지 않음 |
| **단축키(`hotkey`)** | — | **앱 `settings.json` 에 단축키 키가 없다.** kagent·ktm·kvf 각자의 파일(`…\kagent\settings.json` 등)이 소유 | **앱 설정 창에 나올 수 없음**(새 키를 만들지 말라는 지시). 단축키 컨트롤은 몽땅클립보드·몽땅검색 설정 이식(G3·S5)에서 쓴다 |
| 여러 줄 글 | — | 해당 키 없음(`title_owner_text` 도 한 줄) | 컨트롤을 만들지 않음 |
| 글꼴 목록 | — | 저장 키 없음(글꼴 고르기는 설치 글꼴을 그때그때 열거, SETTINGSWIN3) | 해당 없음 |

→ **실제 설정 창에 새로 나타난 것: 목록 2개**(`hidden_tabs`·`library_excluded_folders`). 단축키는 **0개**(위 사유). 키 이름·기본값은 그대로다(둘 다 없을 때의 동작과 같은 빈 목록 `[]`).

### 변경 내용
| 위치 | 변경 |
|---|---|
| `docfit_config/src/lib.rs` | `visible_schema()` 가 배열 기본값을 `"list"` 종류로 분류. `ListKind`(`Text`·`Folder`·`MenuNumber`)·`list_kind(key)`·`is_hideable_menu`·`validate_list`: 메뉴 번호는 정수 1~15·12·13 제외·오름차순·중복 없음(= Python `clean_hidden_tabs` 가 바꾸지 않는 값만), 폴더는 비어 있지 않은 문자열·4096자·NUL 없음·대소문자 무시 중복 없음·최대 1000개 |
| `docs/settings_defaults.json`(골든) | `"hidden_tabs": []`, `"library_excluded_folders": []` 추가 |
| `docfit_app/settings_schema.py` | `LIST_DEFAULTS`·`ALL_DEFAULTS` 추가. **`DEFAULTS` 에는 넣지 않았다** — 서식 엔진 `기본_설정`(=`DEFAULTS` 복사, `hwp-auto-docfit.py:725`)이 통째로 읽고 `설정_저장`으로 되쓰므로, 넣으면 작업 중 바뀐 두 값을 낡은 값으로 덮을 수 있다. Flutter 설정 화면(`fields()`)은 그대로(변화 없음) |
| `scripts/settings_compare.py` | 골든·병합 기대값을 `ALL_DEFAULTS` 로, `list-keys` 사례 추가(기본값·Flutter 화면 비노출·Rust 검증이 Python 정리 규칙과 같은 결론) |
| `settings_win` | `schema.rs`(목록 종류 매핑·`MENU_NAMES`·`list_entry_label`)·`model.rs`(목록 값 정수↔글, 종류별 검사·정렬)·`controls.rs`(`check_list_entry_of`)·`labels.rs`·`guides.rs`·`paint.rs`(패널에 메뉴 이름)·`main.rs`(`--overlay list` 가 `--focus-key` 항목을 연다) |
| 시험 | `tests/test_settings_list_keys.py`(새), `settings_win/tests/control_tests.rs` 5개 추가, `docfit_config` 3개 추가 |

항목 수: **119 = 화면 노출 102 + 내부 17**(묶음별 34+3+14+5+12+10+7+3+12+2+17, 캡처의 트리 숫자와 일치). 이전 117. **시험은 숫자를 고정하지 않는다**(`schema_list_keys()` 가 `visible_schema()` 에서 목록 종류를 찾아 각각 목록 컨트롤·내부 아님·화면 이름 있음·기본값 `[]` 을 확인).

## 2. Python 설정 서비스 의존 줄이기 1단계 설계

별도 문서: **`docs/SETTINGS_PY_DEPENDENCY_DESIGN_2026-11-21.md`**. 요약:
- 읽고 쓰는 지점 표: Python 쓰기 6갈래(W1~W6)·읽기 6갈래(R1~R6)·Rust/기타 소비자(Dock 만 앱 파일을 읽음; kagent·ktm·kvf·kfind 는 각자 파일).
- 호출 방식 비교(CLI·FFI·상주·읽기만 Python): **권장 = `settings-core` CLI**(이미 구현·동등/잠금 시험 통과). 선행 보강: 패치를 표준 입력으로 받기, 실패를 `ServiceError` 로 바꾸는 래퍼.
- 이행: **읽기(그림자 비교) → 쓰기(설정 창만 Rust, 같은 잠금으로 공존) → 제거**. 서식 엔진 읽기 로더(`DEFAULTS`·`migrate`)는 필수 사유가 있어 남김.
- 되돌리기: `settings_ui`(구현됨)·`settings_backend`·`settings_shadow`(설계).
- 최대 위험: 엔진 `설정_저장` 의 통째 되쓰기(이번 카드에서 `LIST_DEFAULTS` 분리로 피함) · 검증 규칙 이중 · 외부 쓰기 뒤 앱 재읽기 연결 없음.

## 3. 캡처 검증(실제 `paint::draw`, 모두 `Read` 로 직접 연 것)

명령 예: `settings_win --capture-dir <폴더> --group general --focus-key hidden_tabs --settings <임시 설정 파일> [--dark] [--dpi 144] [--overlay list]`. 임시 설정 파일에 `hidden_tabs:[3,5,9]`, 폴더 3개를 넣어 읽기만 했다(이 PC 의 실제 설정 파일은 읽지도 않음). PNG 는 재생성 가능하므로 저장소에 넣지 않았다(작업 폴더 스크래치).

| 파일(스크래치 `cap/…`) | 본 것 |
|---|---|
| `general_light`(라이트 96dpi) → 수정 전 | 「일반」 묶음 3번째 「숨길 메뉴 번호」: 줄 "3개: 3, 5, 9" + [편집…](포커스 테두리), 기본값 "(비어 있음)". **안내문이 세 줄로 잘려 "· 6 파일…"에서 끊김** → 수정 |
| `general_dark`(다크 96dpi) → 수정 후 | 같은 줄. 안내문이 두 줄 "왼쪽 메뉴에서 … [편집…]에서 번호와 이름을 볼 수 있어요." 로 끝까지 보임, 글자·단추·포커스 테두리 읽힘, 겹침·잘림 없음. 앞 항목 「퀵몽땅검색 단축키」 '소유자 둘 이상' 칩 정상 |
| `general_overlay`(라이트 목록 편집 패널) | 제목 "목록 편집 · 숨길 메뉴 번호", 입력칸 "새 값 입력 후 Enter" + [추가], 목록 "1 3 다이어그램 / 2 5 붙임사진 편집(선택) / 3 9 사진뷰", [삭제][▲ 위로][▼ 아래로][닫기]. 패널이 목록 영역 안, 겹침·잘림 없음. (하단 "변경 5건"은 캡처 모드의 시연 편집 때문 — 실제 창에서는 0건) |
| `general_overlay_150`(150%=144dpi) | 같은 패널이 비율대로 커짐: 제목·입력칸·행 3개·단추 4개 모두 보임, 잘림·겹침 없음. 왼쪽 트리 숫자·변경 ● 정상 |
| `program_dark`(다크 96dpi, 안내문 수정 전 빌드로 연 것 — 이 묶음은 수정과 무관) | 「프로그램·네트워크」 맨 아래 「문서뷰·사진뷰 제외 폴더」: 안내문 한 줄, 줄 "3개: D:/자료/임시, E…"(말줄임) + [편집…], 기본값 "(비어 있음)". 위 「검색 엔진」(콤보)·체크 줄과 겹침 없음 |
| `program_overlay_dark`(다크 목록 패널) | 제목 "목록 편집 · 문서뷰·사진뷰 제외 폴더", 행 "1 D:/자료/임시 / 2 E:/백업/사진 원본(선택) / 3 C:/Users/Public/Downloads"(긴 경로도 잘리지 않음), 단추 4개. 겹침·잘림 없음 |
| `program_150`(라이트 150%) | 같은 「문서뷰·사진뷰 제외 폴더」 줄: 입력 "3개: D:/자료/임시, E…" + [편집…], 포커스 테두리, 위 항목들과 비율대로. 겹침·잘림 없음 |

**캡처로 찾아 고친 것**: 숨길 메뉴 안내문이 메뉴 이름 13개를 나열해 말줄임으로 끊김 → 요약을 두 줄로 줄이고 이름은 목록 편집 패널 행에 표시(`list_entry_label`).

**열지 않은 파일**(같은 코드·구도 변형, 확인했다고 주장하지 않음): `general_light`(수정 후 다시 만든 것)·`general_150`·`general_overlay_dark`·`program_light`·`program_overlay`·`program_overlay_150`. 고대비는 만들지 않았다.

**알려진 어색함**: 번호 목록 패널의 [▲ 위로][▼ 아래로]는 눌러도 아무 일 없다(번호는 항상 오름차순). 숨기지 않고 둔 것은 패널 구조를 바꾸지 않으려는 선택이다 — 다음 카드에서 비활성 표시로.

## 4. 시험 결과(실행한 명령과 `test result` 줄)

환경: `CARGO_TARGET_DIR=E:\docfit-tmp\target-settingswin4`(환경변수만, 저장소 안 `.cargo/config.toml` 없음), PATH 앞에 `%LOCALAPPDATA%\DocFitTools\mingw64\bin`.

```text
cargo test --locked --manifest-path docfit_config/Cargo.toml
  src/lib.rs unit tests:           test result: ok. 8 passed; 0 failed   (이전 5 + 목록 3)
  (bin·doc-tests):                 test result: ok. 0 passed; 0 failed  ×2
cargo test --locked --manifest-path settings_win/Cargo.toml
  tests/control_tests.rs:          test result: ok. 17 passed; 0 failed  (이전 12 + 목록 5)
  tests/model_tests.rs:            test result: ok. 15 passed; 0 failed
  (lib·bin·doc-tests):             test result: ok. 0 passed; 0 failed  ×4~5
python -m unittest tests.test_settings_list_keys tests.test_configuration_service tests.test_settings_window_service
  tests.test_bridge_methods tests.test_search_engine_settings tests.test_hidden_tabs_setting
  tests.test_library_exclusion_settings tests.test_settings_compare tests.test_core_app_state
                                   Ran 48 tests ... OK (skipped=2: SETTINGS_CORE_EXE 없이 돌린 settings_compare 2개)
SETTINGS_CORE_EXE=<settings-core.exe> python -m unittest tests.test_settings_compare -v
                                   Ran 2 tests ... OK  (동등 사례 + Python/Rust 잠금 상호 배제)
python scripts/settings_compare.py --exe <settings-core.exe>
                                   cases 6, equal True  (기존 5 + list-keys; 골든=ALL_DEFAULTS 일치)
```
- 전체 시험·Flutter 시험은 돌리지 않았다(지시). Flutter 설정 화면은 코드를 바꾸지 않았다(`fields()` 불변).
- `Cargo.lock` 변화 없음(`--locked` 통과).

## 5. 미확인·남은 문제
- **단축키는 앱 설정 창에 나오지 않는다**: 키가 앱 파일에 없다. 단축키 컨트롤은 G3(몽땅클립보드)·S5(몽땅검색·가상폴더)에서 각 앱 파일 키를 쓸 때 연결한다. 시연 항목(`--demo-controls`)은 단축키 캡처용으로 유지.
- 메뉴 번호의 의미: 이름 표는 Flutter `_labels` 순서를 옮긴 것이다. **Dock 의 번호 체계(`TAB_NAMES[12]`=자료함)와 다르다**(설계 ⑤-3 지적) — `hidden_tabs` 는 Flutter 번호 기준으로 저장되며 Dock 이 같은 번호를 어떻게 해석하는지는 이 카드에서 확인하지 않았다.
- 실제 창에서의 목록 편집 동작(입력·Enter·한글 IME·삭제)은 **실기 확인 안 함**(캡처·모델 시험만). SETTINGSWIN3 의 실기 확인 항목 1·3·4 그대로.
- 실제 앱이 `hidden_tabs` 저장을 즉시 반영하는지(앱 메모리 캐시 `_hidden_tabs`, `desktop_web_ui.py:3352`)는 확인하지 않았다 — 새 창에서 저장해도 켜진 앱 사이드바는 다시 시작해야 바뀔 수 있다(설계 문서 ⑤ 위험 3).
- 폴더 목록 항목은 존재 여부를 검사하지 않는다(폴더 컨트롤의 "없는 폴더" 알림도 목록 안에서는 없음).
- Python `read_exclusions` 는 읽을 때 대소문자 무시 정렬한다 — 새 창이 정한 순서는 앱에서 정렬되어 보인다(값 집합은 같음).
- 설계 문서의 '표준 입력 `set`'·`settings_core_client.py`·`settings_backend` 는 **구현하지 않음**(지시: 설계만).

## 6. 실행 환경·git
이 PC 의 실제 설정 파일(`%APPDATA%\HwpAutoDocFit\settings.json`)은 읽지도 쓰지도 않았다(시험·캡처 모두 임시 폴더). `hwp-auto-docfit.py` 는 읽기만 하고 고치지 않았다. GitLab(`origin`)은 건드리지 않았고 **푸시하지 않았다**(로컬 커밋만).
