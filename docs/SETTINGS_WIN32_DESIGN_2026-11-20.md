# 앱 설정 UI·몽땅클립보드 설정 UI — Rust + Win32 전환 1단계 설계 (SETTINGSWIN1)

작성: 2026-11-20 · 브랜치 `feature/SETTINGSWIN` · 상태: **설계 문서. 코드·설정 변경 없음.**
근거 표기: `파일:줄` = 이번 조사에서 직접 읽은 위치. **미확인** = 읽지 못했거나 시험하지 않은 것(부록 B에 모음, 추측으로 채우지 않음).
조사 대상(전부 읽기 전용): `TODO.md` 마지막 항목, `docs/RUST_MIGRATION_INVENTORY_2026-11-19.md` 12번 행, `docs/DOCK_DESIGN_2026-11-18.md`, `kagent/src/{settings_menu,popup,frame,store,pipe,lifecycle,main}.rs`, `ui_theme/src/*`, `ktm/src/{settings,window}.rs`, `kvf/src/{settings,paths}.rs`, `dock/src/*`, `docfit_app/{settings_schema,configuration_service,settings_service,application_service,kclip_service,update_service,kfind_process}.py`, `docfit_core/{config_store,update_signature}.py`, `flutter_ui/lib/features/{settings,kclip,workspace}/*`, `flutter_ui/lib/main.dart`.

## 0. 요약

- **권장안 한 줄**: 설정 소유자를 Rust 공용 라이브러리(`docfit_config`) 하나로 모으고, 화면은 필요할 때만 뜨는 새 Win32 exe(`ksettings.exe`, 좌측 트리 + 우측 항목 목록 + 검색, 독립 항목은 즉시 적용·서식 항목은 페이지 단위 적용/되돌리기)로 만들며, 몽땅클립보드 켜기·동의는 같은 exe가 `kagent` 생명주기 코드를 **프로세스 안에서 직접** 호출해 제한 시간 있는 상태 기계로 처리한다(Python·Flutter 경유 4단 호출 제거).
- 설정 키: 앱 `settings.json` 최상위 **97개**(화면 노출 83개·내부 14개, 화면상 잎 100개) + 스키마 밖 키 8개 확인 + Rust 앱 파일 키 56개(kagent 24·ktm 23·kvf 8·kfind 1·Dock 0).
- **소유자가 둘 이상인 설정 키: 8개**(⑤ 표). 설정 파일은 **6개**(앱 2·kagent·ktm·kvf·kfind, Dock 자체 파일 없음)로 흩어져 있다(② 표).
- **미확인 항목: 14개**(부록 B).
- 예상 규모: 약 **28~32장**(⑧), 위험 3가지(⑧).
- Python·Flutter 잔류는 **필수 사유가 확인된 것만**(부록 C). 설정 화면·설정 저장·마이그레이션·업데이트 확인은 필수 사유가 없어 모두 제거 대상.

---

## ① 설정 항목 전수 목록과 Flutter가 쓰는 명령

### ①-1 앱 설정(`%APPDATA%\HwpAutoDocFit\settings.json`)

- 기본값 정의: `docfit_app/settings_schema.py:2-140` `DEFAULTS` 최상위 **97개**. 화면·`save_settings` 제외 `EXCLUDED` **14개**(`configuration_service.py:276-277`: 명시 10개 + `_rev` 4개). 나머지 83개가 `fields()`(`configuration_service.py:293-313`)에서 **잎 100개**가 된다(`label_symbols` 4·`symbol_fonts` 12·`table_fonts` 4가 중첩 잎으로 펼쳐짐). 전수표는 **부록 A**.
- 종류: 잎 100개 = boolean 64 · number 21(숫자 문자열) · text 14 · choice 1(`search_engine`, `configuration_service.py:309-312`)(실행해 센 값, 중첩 3종의 잎 포함). **숫자는 문자열로 저장**한다(`"5"`, `"140"` …). Rust 이식 때 이 형식을 바꾸면 Python 엔진이 깨진다.
- 검증(`ConfigurationService.save`, `configuration_service.py:337-391`): 알 수 없는 키 거부 · boolean은 `bool`만 · 그 밖은 문자열 ≤4096자·NUL 금지 · number는 0~1000 유한 소수, 정수 한정 키 5개(`retry_body`·`retry_table`·`punctuation_threshold`·`linespacing_min`·`linespacing_max`), `.size`·`std_title_subtitle_pt`는 >0 · choice는 허용 목록 · **교차 검증**: 최소 줄 간격 ≤ 최대 줄 간격(`:382-389`) · 작업 중(`running`)이면 저장 거부 `JOB_RUNNING`(`:338-339`) · 낙관적 동시성: `revision`(저장 파일 내용의 sha256)이 다르면 `SETTINGS_CHANGED`(`:368-370`).
- 눈에 띄는 비대칭(그대로 이식하면 안 되는 부분): `table_linespacing_min`·`table_spacing_min`은 의미상 숫자인데 `fields()`가 `text`로 분류해(`:300-301` 목록에 없음) 숫자 검증을 받지 않는다. `symbol_fonts`·`table_fonts`는 글꼴 이름을 4096자 문자열로만 검증한다.
- 화면 위치(Flutter): ① **설정 서랍**(`main.dart:3898-4036`, 끝에서 열리는 400px 서랍): 즉시 저장 스위치 9개 `autoclose·verify·integrity_report_file·final_review_file·log_file·unify_result_window·check_updates·always_on_top·developer_mode` + 서식 프로파일 선택 + 화면 테마 + 업데이트 패널 + 링크 4개. ② **전체 설정**(`SettingsDialog`, `settings_dialog.dart` 874줄 + `setting_guides.dart` 936줄): 좌측 분류 10개(`setting_guides.dart:14-34`) + 검색 + 우측 카드, 저장 단추·`restoreDefaults`(`:172-176`)·미리보기(`_typesetPane`, `:761`)·관리 패널 4개(가상 폴더 창·글자 상용구·본문 상용구·앱 정보, `:24-30`). ③ 별도 대화상자: 쓸 메뉴 고르기(`sidebar_menu_settings.dart` 254줄), 준말 관리, 메뉴 표시 방식(`dock_mode_controller.dart` 92줄).
- 안내문 `SettingGuide` **77개**(`setting_guides.dart`), 이 중 일부가 위젯으로 그린 "적용 전·후 예시" 미리보기를 가진다(`preview:`). 이 미리보기는 Win32 이식 비용이 커서 ⑧ 단계·결정 사항에 따로 둔다.
- Rust가 이 파일을 읽는 곳: **Dock만**(`hidden_tabs`, `dock/src/native.rs:421-432`, 파서 `dock/src/lib.rs:206-218`). ktm·kagent·kvf·kfind는 읽지 않는다(grep 확인).
- Python이 읽는 곳: 키 문자열이 등장하는 소스 **32개**(`docfit_engine/{pipeline,formatter_*,page_fit,spacing}.py`·`docfit_ui/legacy.py`·`hwp-auto-docfit.py`·`docfit_app/*`·`tools/*` 등, 근거 스크립트 문자열 검색). 즉 **서식 엔진이 읽는 값**이 대부분이다 → 부록 C.

### ①-2 스키마 밖에서 같은 파일에 저장되는 키(8개 확인)

| 키 | 쓰는 곳 | 비고 |
|---|---|---|
| `hidden_tabs` | `application_service.py:20-23, 276-278` | 0·12·13 강제 표시. **Dock도 읽음**(검증 규칙 이중, ⑤-G) |
| `sidebar_collapsed` | `application_service.py:273-274` | |
| `stage_choices` | `application_service.py:235-250` | 작업 카드 단계 선택 저장 |
| `card_options_rev` | `config_store.py:28` | 마이그레이션이 항상 2로 씀 |
| `kclip_window` | `kclip_window_state.py:8` | 옛 Flutter 스티커 창 위치·크기·항상 위 |
| `external_app_paths` | `external_apps_service.py:30` | |
| `library_excluded_folders` | `library_exclusions.py:9` | 문서뷰·사진뷰 제외 폴더 |
| `last_run_version` | `update_service.py:379-380` | 버전 안내용 |

사진첩·문서 격자 서비스 등 나머지 서비스의 키 전수는 **미확인**(B-8). 저장 코드는 항상 "읽고 → 일부만 바꿔 → 쓰는" 패치 방식이므로, **Rust 라이브러리는 모르는 키를 그대로 보존해야 한다**(이행기 필수 규칙).

### ①-3 Flutter가 설정과 관련해 부르는 명령(Python 브리지 `widget.api(method, args)`)

| 명령 | 호출 위치 | 하는 일 | 설정 키 |
|---|---|---|---|
| `get_settings` / `save_settings(patch, revision)` | `settings_dialog.dart:124,160` | 필드 목록·값·revision / 일괄 저장 | `fields()` 100잎 |
| `get_state`(`hidden_tabs`·`theme_mode`·`menu_display_mode`·`sidebar_collapsed`·`update` 포함) | `main.dart:255` 등 | 시작 때 상태 | 다수 |
| `set_option(key, bool)` | 설정 서랍 스위치 | 9개 키만 허용(`application_service.py:206-207`) | 9 |
| `set_include_spacing`·`set_reset_spacing`·`set_table_spacing`·`set_exclude_tables` | 작업 카드 | 카드 옵션 4개 | 4 |
| `set_theme_mode` | `main.dart:561` | 저장 + kagent에 `settings_set theme` 푸시(`application_service.py:122-128`) | `theme_mode` |
| `set_menu_display_mode` | `dock_mode_controller.dart:33,36,45,69` | 사이드바/Dock 전환 + Dock 실행·폴백 | `menu_display_mode` |
| `set_hidden_tabs`·`set_sidebar_collapsed` | `sidebar_menu_settings.dart:162` | | `hidden_tabs`·`sidebar_collapsed` |
| `hangul_fonts list` | `settings_dialog.dart:73` | 글꼴 목록(한/글 폴더 + Windows) | — |
| `file_rename sendto_set` | `settings_dialog.dart:804` | 탐색기 보내기 메뉴 등록(레지스트리) | `file_rename_sendto` |
| `freeze_logs_open` | `settings_dialog.dart:848` | 오류추적 폴더 열기 | — |
| `idioms`(글자·본문 상용구) | `idiom_panel.dart:74` | 상용구 목록 편집 | **저장 위치 미확인**(B-12) |
| `abbreviations` | `AbbreviationManager`(준말 504줄) | 준말 등록·수정·삭제 | `abbreviations`(EXCLUDED)·`abbreviation_*` |
| `kvf` | `kvf_panel.dart:28` | 가상 폴더 창 켜기·자동 시작·상태 | kvf 파일 |
| `update_*`(상태·확인·시작) | 설정 서랍 `UpdatePanel`(`main.dart:3971`) | 업데이트 | `check_updates_on_start` |
| `kclip`(`status·enable·disable·autostart·settings_get·settings_set·pause·resume·clear·list·…`) | `kclip_dialog.dart:382-424` | 몽땅클립보드 전부 | kagent 파일 |

### ①-4 Rust 앱 설정 키(전수)

**kagent**(`kagent/src/store.rs:19-54`, 저장 JSON 24키 `:102-127`): `max_items`(10~1000) · `max_item_mb`(1~50) · `max_total_mb`(10~2000) · `max_age_days`(1~3650) · `auto_paste` · `sticky_popup` · `always_on_top` · `paused` · `hotkey`(≤40자) · `excluded_apps`(소문자, ≤100개) · `watch_screenshots` · `capture_dirs_off`(≤50) · `timeline_roots`(절대경로, 드라이브 문자·`..` 금지·UNC 금지·≤20) · `timeline_mtime_only` · `popup_x/y/width/height`(창 위치·크기, 자동 저장) · `popup_collapsed` · `excel_target_paste` · `preview_images` · `theme` · `app_path` · `policy_rev`. 검증 범위는 `store.rs:141-200`(`apply`).
- 화면 노출: Flutter 관리창 `_settings`(`kclip_dialog.dart:819-935`)는 `max_items·max_total_mb·max_age_days·sticky_popup·always_on_top·auto_paste·excel_target_paste·preview_images` 8개 + `_captureFolders`(`:937-991`)는 `watch_screenshots`·`capture_dirs_off`. kagent ⚙ 메뉴(`settings_menu.rs:213-258`)는 여기에 `max_item_mb`·`theme`·일시정지·자동 실행·모두 지우기·끄기를 더하고 **폴더별 끄기는 없다**. 어느 화면에도 없는 키: `hotkey`(표시만, `kclip_dialog.dart:1015-1021`)·`excluded_apps`·`timeline_*`(앱 타임머신 화면이 넣음, `timemachine_service.py:836-855`)·`popup_*`·`app_path`.
- 저장 방식: `Settings::save`(`store.rs:211-215`)는 `임시파일 쓰기 → rename`(fsync 없음, 프로세스 간 잠금 없음). 쓰는 사람은 kagent 프로세스 하나뿐이라는 전제(`main.rs:498-515` `update_settings`).

**ktm**(`ktm/src/settings.rs:59-85`, 23키, serde): `theme`("dark"기본)·`ui_theme`("system"기본)·`hotkey`·`recent_queries`(≤30)·`recent_query_times`·`favorite_queries`·`smart_presets`·`default_search_mode`·`max_results`(50/100/200)·`ui_scale`(90/100/110/125)·`result_density`·`open_clicks`·`show_location`·`show_modified`·`include_subfolders`·`excluded_directories`·`version_grouping`·`autostart`·`minimize_to_tray`·`start_hidden`·`max_file_size_mb`·`skip_cloud_network`·`allow_system_folders`. 화면: 이미 Win32 설정 모달(탭 4개 일반·검색·시스템·진단, 설정 17행, `window.rs:2395-2410, 2612-2660`), **값을 누를 때마다 순환 선택**(`window.rs:3202-3270`)하는 방식. 단축키·제외 폴더 편집 화면은 모달 행 목록에 없음("제외 디렉터리 추가" 행은 있음, 동작 세부 미확인). 저장은 `std::fs::write`(`settings.rs:193-200`) 비원자.

**kvf**(`kvf/src/settings.rs:5-21`, 8키): `hotkey`("Win+Shift+F")·`tray`·`sticky`·`x/y/width/height`·`last_box`. 테마와 kagent 단축키는 **kagent 설정 파일을 읽기 전용으로 읽는다**(`settings.rs:30-32`, `paths.rs:7`). 저장은 `write_changed`(`settings.rs:25-28`, 내용이 같으면 쓰지 않음, 비원자). 화면 설정 UI는 없고 앱 Flutter `kvf_panel.dart`(96줄)가 켜기·자동 시작만 다룬다.

**kfind**(`crates/kfind/src/main.rs:45`, `docfit_app/kfind_process.py:50-82`): `%LOCALAPPDATA%\HwpAutoDocFit\kfind\settings.json`에 `fallback`("user"/"task") 1키. Python은 임시파일+`os.replace`(잠금 없음)로 쓰고 Rust는 읽는다.

**Dock**: 자체 설정 파일 **없음**(`dock/src` 전체 grep). 앱 `settings.json`의 `hidden_tabs`만 읽는다. `menu_items(hidden, developer_mode)`의 `developer_mode`는 `native.rs:285`에서 **항상 false로 고정**되어 사이드바(개발자 모드에서 12·13·15 표시)와 다르다.

---

## ② 설정 저장소 구조와 하나로 합치는 안

### ②-1 파일 전수(이번 조사로 확인한 것)

| 파일 | 형식 | 쓰는 쪽 | 읽는 쪽 | 잠금·원자성 |
|---|---|---|---|---|
| `%APPDATA%\HwpAutoDocFit\settings.json` (없으면 `LOCALAPPDATA`, 그것도 없으면 `~/.hwp_auto_docfit`: `formatter_utils.py:317-321`, `backend_main.py:19-20`) | JSON 객체, `indent=2`·UTF-8·한글 그대로(`settings_service.py:64`) | Python `SettingsService`(`settings_service.py:19-76`) | Python 32개 소스 + **Dock** | `RLock`(프로세스 내) + `ProcessLock(settings.json.lock)`(`msvcrt.locking` 1바이트, `process_lock.py:16-32`) + `NamedTemporaryFile → flush → fsync → os.replace`(`:61-67`). 읽기 오류는 `SETTINGS_READ_FAILED`로 **저장을 막는다**(`:36-39`). 변경이 없으면 쓰지 않음(`:55-56`) |
| `%APPDATA%\HwpAutoDocFit\timemachine.json` | JSON | Python `timemachine_service.py:53` | 같음 + `timeline_roots`를 kagent에 푸시(`:836-855`) | 별도(본 조사 미확인) |
| `%LOCALAPPDATA%\HwpAutoDocFit\kagent\settings.json` (`KAGENT_DATA`·`KAGENT_TEST_ROOT`로 변경 가능, `util.rs:13-22`) | JSON(자체 `json.rs`) | **kagent만** | kagent, **kvf(읽기 전용)**, Python(파이프 `settings_get/set`) | rename만 |
| `%LOCALAPPDATA%\HwpAutoDocFit\ktm\settings.json` (`KTM_TEST_ROOT`·캡처 모드는 `.ktm-capture-data`, `window.rs:4039-4050`) | JSON(serde) | ktm만 | ktm | 없음(`fs::write`) |
| `%LOCALAPPDATA%\HwpAutoDocFit\kvf\settings.json` (`KVF_TEST_ROOT`) | JSON(자체) | kvf만 | kvf | 없음 |
| `%LOCALAPPDATA%\HwpAutoDocFit\kfind\settings.json` | JSON | Python(`fallback`)·kfind 설치 | kfind | tmp+replace |

JSON 구현이 **4벌**이다: Python `json`, kagent `json.rs`(295줄), kvf `json.rs`, ktm·dock·kfind의 `serde_json`. 위치가 **두 갈래**(앱 `APPDATA` 로밍 / Rust 앱 `LOCALAPPDATA` 로컬)인 이유는 문서로 확인하지 못했다(B-9).

### ②-2 마이그레이션

- 순수 함수 `migrate_settings`(`docfit_core/config_store.py:10-52`, 43줄)를 `settings_migrations.migrate`가 감싼다(`settings_migrations.py`). 내용: `diagram_editor_enabled` 삭제 · 3개 키 이름 변경·반전(`all_linespacing→all_exclude_linespacing` 등) · `stage_choices.spacing.reset_spacing` 이전 · `*_rev` 판 번호 5종(`label_symbols_rev`·`report_files_rev`·`std_parspace_rev`·`std_supplement_rev`·`linespacing_min_rev`)에 따른 기본값 이전 · 글꼴 오타 `한컴돋음→한컴돋움` · `card_options_rev=2`. **판 번호가 파일 안에 있어 멱등**이고, `save` 때 한 번 더 적용된다(`configuration_service.py:371`).
- kagent는 `policy_rev`(`store.rs:122,126-131`)로 "정책 반영 전 파일(policy_rev 없음)에서 `max_items == 50`이면 500으로" 한 가지만 이전한다. ktm은 `#[serde(default)]`로 새 필드를 기본값 채움(`settings.rs:58-59`)이 전부. kvf는 범위 밖 값 무시.
- **이식 규칙**: 마이그레이션 함수·판 번호 체계를 그대로 Rust로 옮기고, "예전 파일 N종 → Python 결과와 Rust 결과가 바이트 동일" 골든 시험(⑦)을 통과해야 Python 쪽을 지운다.

### ②-3 합치는 안 3가지와 권장

| 안 | 내용 | 장점 | 단점 |
|---|---|---|---|
| **안1 파일 하나** | 6개 파일을 `settings.json` 하나(네임스페이스 `app/kclip/ktm/kvf`)로 | 잠금·백업 한 곳 | 상주 프로세스(kagent·ktm)가 파일 변경을 감시·재로딩해야 함(지금은 메모리 보유 + 자기 저장) · 창 좌표·`app_path` 같은 PC 종속 값이 로밍 폴더로 섞임 · **되돌리기·기존 설치 호환 부담 최대** |
| **안2 파일 유지 + 단일 라이브러리(권장)** | 파일은 그대로 두고, **스키마·검증·마이그레이션·잠금·원자 저장을 Rust 라이브러리 `docfit_config` 하나**가 제공. 앱 설정 파일은 Rust가 소유자, Python은 읽기 전용 로더 | 기존 설치·롤백 호환, 상주 앱 구조 불변, 4벌 JSON 구현 통합 | 라이브러리 의존이 kagent·ktm·kvf·dock에 생김(ui_theme과 같은 경로 크레이트 방식) |
| 안3 설정 데몬 | 설정만 맡는 상주 프로세스 | 소유자 완전 단일 | **상주 자원 추가**(원칙 위반)·시동 순서 의존 |

**권장 = 안2**, 구체 규칙:

1. 라이브러리 `docfit_config`(신규 경로 크레이트, `ui_theme`와 같은 방식으로 각 앱이 `path` 의존): 스키마 표(키·종류·기본값·검증·교차 검증)를 **한 곳**(Rust const 표 + 골든 JSON)에 두고, `load → migrate → merge(DEFAULTS)`, `patch(키 일부) → 검증 → 원자 저장`을 제공.
2. **모르는 키 보존**(스키마 밖 8개 키·미래 키): 읽은 JSON 전체를 들고 있다가 바뀐 키만 갈아 끼운 뒤 전체를 쓴다. 키 순서 보존(`serde_json`의 `preserve_order`).
3. **같은 잠금 프로토콜**: `<파일>.lock`의 0번 바이트 1바이트 배타 잠금(`LockFileEx`). Python `msvcrt.locking`과 서로 배제되는지는 **시험으로 확인해야 한다**(B-1) — 확인 전까지는 이행기에 Python 쓰기를 허용하지 않는 쪽으로 설계한다(아래 4).
4. **이행기 쓰기 규칙**: (가) 설정 창이 쓰는 키(화면 노출 83키 + `theme_mode`·`menu_display_mode`·`hidden_tabs`)는 Rust만 쓴다. (나) Python이 계속 쓰는 키(작업 카드 옵션 `all_*`·`unify_*`·`spacing_*`·`table_spacing`, `stage_choices`, `sidebar_collapsed`, `last_run_version`, `abbreviations`)는 작업 화면이 이식될 때까지 Python이 쓰되 **같은 잠금**을 쓴다. 이 구분이 "소유자가 둘"인 기간을 명시적으로 만든다(⑤-표).
5. **공유 값은 한 키**: 화면 테마는 앱 `theme_mode` 하나를 정본으로 하고 ktm·kagent·kvf는 그것을 **읽기 전용**으로 읽는다(kvf는 이미 kagent 파일을 이렇게 읽음, `kvf/paths.rs:7`). 변경 알림은 이미 있는 `DocFitThemeChanged` 브로드캐스트(`ui_theme/src/lib.rs:334`, `win.rs:124`)를 쓴다. 각 파일의 옛 테마 키는 읽기 호환만 남기고 쓰기는 중단.
6. 상주 앱(kagent)의 설정은 **kagent 프로세스가 계속 쓰기 소유자**이고, 설정 창은 파이프 `settings_set`(검증·정리·창 갱신을 한 번에 하는 기존 길, `pipe.rs:347-354`)으로만 바꾼다. kagent가 꺼져 있으면 클립보드 설정 페이지는 켜기·동의 화면만 보인다(꺼진 동안 바꿀 설정은 기본값이므로 필요 없음, `kclip_dialog.dart:764-815`와 같은 동작).

---

## ③ 창 구조(탭/트리·컨트롤·검색·적용 방식) — 2~3안 비교

### ③-1 현재 두 방식

- Flutter 앱 설정: 좌측 분류 목록 + 검색(2글자부터 실시간) + 우측 카드, **저장 단추 + `revision` 충돌 + 모두 기본값**, 작업 중 잠금.
- 네이티브(kagent): ⚙ 메뉴, **고르면 즉시 반영, 적용 단추 없음**(`popup.rs:1207-1208`). ktm: 탭 4개 + 값 순환 클릭, 즉시 저장.

### ③-2 안

| 안 | 구조 | 적용 방식 | 장점 | 단점 |
|---|---|---|---|---|
| **A 트리+목록(Flutter 그대로)** | 좌 트리(영역→분류) · 우 목록 · 상단 검색 | 하단 [적용][되돌리기][기본값] | 사용자가 익숙한 구조, 교차 검증(줄 간격 최소≤최대)을 한 번에 | 클립보드·검색처럼 "고르면 바로" 쓰던 설정까지 저장 단추가 생김 |
| **B 탭 5개 + 즉시 적용** | 상단 탭(앱·클립보드·검색·폴더·메뉴) · 탭 안 한 줄 목록 | 전부 즉시 | 구현 단순, kagent·ktm 방식과 같음 | 앱 서식 키 60여 개가 한 탭에 몰림 · 즉시 적용은 `retry_body="15"`처럼 **입력 중간값**(1, 15)이 그대로 저장돼 위험 · 교차 검증 순서 의존 |
| **C 트리+목록 + 혼합 적용(권장)** | A의 구조. 최상위 영역 6개: ①보고서·서식 ②작업·결과 ③프로그램·업데이트 ④몽땅클립보드 ⑤몽땅검색·가상폴더 ⑥메뉴·Dock·개발자 | **독립 키(스위치·선택)는 즉시 적용**, **입력칸·서식 묶음(번호·글꼴·간격, 교차 검증이 있는 키)은 페이지 안에서 [적용][되돌리기]** | 클립보드·검색은 지금 동작과 같고, 서식은 지금 Flutter와 같은 안전성 | 두 규칙이 섞여 사용자에게 보여 주는 표시가 필요(변경된 항목에 점·페이지 상단 "변경 3건 [적용][되돌리기]") |

**권장 = C**. 근거: 교차 검증 키(`linespacing_min/max`)와 입력 중 중간값 문제가 있는 값은 일괄 적용이 필요하고(`configuration_service.py:382-389`), 스위치·선택은 지금도 즉시 저장되는 서랍(`main.dart:3898-4036`)·kagent 메뉴(`settings_menu.rs`)와 같다.

- 검색: 지금 규칙을 따른다 — 2글자부터 실시간, 1글자는 Enter(`settings_dialog.dart:36-37` `InstantFilterController`), 검색 대상 = 항목 이름 + 안내 제목·요약(`_matches`, `:204`). 검색 중에는 트리 대신 결과 목록(`_body`, `:791-833`). Rust에서는 77개 안내문을 문자열 표로 가져가 같은 규칙으로 부분 일치.
- 되돌리기: 항목별 "기본값으로"(점 표시) · 페이지 [되돌리기](마지막 저장 값) · 전체 [모두 기본값](현재 `restoreDefaults`와 같음, 저장은 확인 뒤). 모든 되돌리기는 메모리 안 `changed` 맵 조작이라 저장 전에는 파일을 건드리지 않는다.
- 작업 중 잠금: 앱이 문서 작업 중이면 서식 키는 읽기 전용(현 `JOB_RUNNING`). 작업 상태를 아는 소유자는 앱(Flutter→Python)이라 **이행기에는 앱이 `job.lock` 같은 신호를 설정 폴더에 둬야 한다**(현재 신호 수단 미확인 — B-10).

### ③-3 항목 종류별 컨트롤

| 종류 | 키 예 | 컨트롤 |
|---|---|---|
| 스위치(bool) | `stdformat`, `autoclose` … 약 60개 | 스위치 + 이름 + 한 줄 요약 |
| 숫자(문자열 저장) | `retry_body`, `std_parspace_*`, `linespacing_*` | 입력칸 + ± 단추, 검증 실패 시 그 칸 아래 빨간 글(지금 `configuration_service.py` 메시지 재사용) |
| 선택 | `search_engine`, 보관 개수·기간 | 콤보(드롭다운) 또는 라디오 |
| 글꼴(이름+크기) | `symbol_fonts` 12잎, `table_fonts` 4잎 | 편집 가능한 콤보(글꼴 목록 + 직접 입력) + 크기 입력, 목록은 `hangul_fonts`(한/글 폴더 + Windows)를 Rust가 열거 |
| 폴더 | `hwp_font_folder`, `paste_add_folder` | 입력칸 + [찾아보기](IFileOpenDialog) |
| 텍스트 | `title_owner_text` | 입력칸(한글 IME) |
| 라벨 기호 4개 | `label_symbols` | 체크 4개 묶음 |
| 단축키 | kagent·ktm·kvf `hotkey` | **키 캡처 칸**(눌러서 입력) + 충돌 경고. 지금 어느 화면에도 변경 UI가 없음 → **새 기능**(사용자 확인 필요) |
| 목록(편집) | `excluded_apps`, `excluded_directories`, `capture_dirs_off`, `abbreviations`, 상용구 | 체크 목록 / 추가·삭제 목록 |
| 색 | `color`("blue", 표시 색) | 색 선택 콤보 |
| 테마 | `theme_mode` | 라디오 3(시스템·밝게·어둡게) |

---

## ④ 몽땅클립보드 설정·켜기·동의 화면

### ④-1 현재 구조 — `kclip_dialog.dart`(1,342줄)가 하는 일

설정·켜기만이 아니라 **관리창 전체**가 한 파일이다. 기능 목록(줄 번호는 이번에 읽은 범위):

| 기능 | 줄(약) | 이 카드 범위 |
|---|---|---|
| 스티커 창 래퍼(위치·크기·항상 위 저장·복원) | 73-249 | 범위 밖(관리창) |
| 상태·목록·설정 불러오기, FFI(`kclip_ffi.dll`)로 list/get/pin/delete 직접 호출 후 실패 시 Python 중계로 폴백 | 382-456 | 부분(호출 경로) |
| 그림 미리보기·썸네일·표 여부 확인·"음영 없이 복사" | 458-632 | 범위 밖 |
| **`_act`**: 명령 실행·제한 시간(켜기 15초·그 외 30초)·`busy`·켠 뒤 최대 10초 상태 확인·성공 시 `onClose` | 634-689 | **범위** |
| 엑셀 표→한/글 표 변환, 메모 추가, 네이티브 끌어 놓기 | 691-722 | 범위 밖 |
| 모두 지우기 확인 | 724-760 | **범위** |
| **`_off`**: 안내문 + 동의 체크 + "로그인 때 자동 켜기(30초 뒤)" 체크 + [몽땅클립보드 켜기] | 764-817 | **범위** |
| **`_settings`**: 자동 실행 · 보관 개수·최대 MB·보관 일 · 스티커 · 항상 위 · 고르면 바로 붙여넣기 · 엑셀 표 목표 결과 · 그림 미리보기 | 819-935 | **범위** |
| **`_captureFolders`**: 화면캡처 폴더 감시 · 폴더별 끄기 | 937-991 | **범위** |
| **`_on`**: 기록 중 칩 · 단축키 칩(충돌 시 "사용 불가") · 일시정지(30분·1시간) · 모두 지우기 · 끄기 | 993-1060 | **범위** |
| 항목 목록·검색·분류 칩·끌어 놓기 영역 | 1060-1342 | 범위 밖 |

→ 설정·켜기·동의 관련은 약 **450줄**(`:634-689, 724-760, 764-1060`)이고 나머지는 관리창이다. **kagent에는 이미 Win32 빠른 목록(`popup.rs`)이 있으나, Flutter 관리창과의 기능 차(메모·엑셀 변환·드롭·미리보기·분류)는 전수 비교하지 않았다**(B-4). 따라서 이 카드의 전환 범위는 "설정·켜기·동의 + 모두 지우기 확인"으로 정하고, 관리창 목록은 별 카드로 둔다.

### ④-2 켜기 호출 사슬과 '무한 대기' 이력

현재 사슬(모두 코드에서 확인):

```
Flutter _act('enable')                      kclip_dialog.dart:634   제한 15초(:643-646)
 └ Python bridge 'kclip' → KClipService.enable   kclip_service.py:142
    └ subprocess kagent.exe --enable --consent [--autostart]   :127-140  제한 15초(lifecycle_timeout)
       └ kagent lifecycle_command → LifecycleLock(뮤텍스 15초 대기, main.rs:1227-1242)
          └ lifecycle::update → start_and_wait: 20ms × 150 = 3초 (main.rs:1336-1348)
    └ Python status() 반복: 20ms × 75 = 1.5초, **매번 `kagent.exe --status` 프로세스를 새로 띄움**(kclip_service.py:97-125)
 └ Flutter _reload() 최대 200회 × 50ms = 10초, 매번 status(=프로세스 1개) + 목록 + 설정 호출(:428-456, :660-670)
```

- 이력: 사용자 보고(2026-11-20 TODO `638104e8`) "켜는 중…이 끝나지 않고 창이 안 닫힘" → `134beb6f`(대기 10초 + 켜는 중 표시) → `1aef4013`(Python `KCLIP_START_TIMEOUT`·타임아웃 상수 100→75·Flutter 15초 `TimeoutException`·성공 시 `onClose`) → v2.2.83에 포함(`dc55f6bd`). **수정 뒤 실기에서 재발하지 않는지는 이 카드에서 확인하지 않았다**(B-5: 코드 읽기만 함).
- 구조적 약점(코드 사실): ① 제한 시간이 **서로 다른 4곳**(15·15·3·1.5·10초)에 흩어져 있고 서로 맞물리지 않는다 ② 상태 확인이 프로세스 생성 폴링이다 ③ 켜기 성공 판정 기준이 세 곳에서 다르다(`start_and_wait`는 `status.json`의 `pid`가 자식 프로세스와 일치+살아 있음, Python은 `status.running`, Flutter는 `running` 또는 응답의 `running`) ④ 동의문이 **세 곳에 복제**(Python `kclip_service.py:24-27`, Rust `main.rs:1372`·`:1423`).

### ④-3 Win32 설계

**호스트**: `ksettings.exe` 안에서 클립보드 켜기를 직접 실행한다. `kagent/src/lifecycle.rs`는 이미 `std`만 쓰는 순수 모듈(`lifecycle.rs:1-2`)이고 `lifecycle_command`의 설치·해시 검증·원자 교체(`main.rs:1354-1420`)도 `windows-sys`만 쓴다 → 라이브러리로 분리해 `ksettings`와 `kagent --enable` 양쪽이 **같은 코드**를 쓴다(CLI `--enable`은 호환 유지·되돌리기용).

**켜기 창(소개·동의)** 구성: 안내문 · 동의 체크 · 로그인 자동 켜기 체크 · [몽땅클립보드 켜기] · [취소]. 켜기 단추는 `동의 && 번들 있음 && 진행 중 아님`일 때만 활성(`kclip_dialog.dart:797-798`과 같은 조건). `available == false`이면 "이 버전에는 몽땅클립보드 프로그램이 들어 있지 않습니다"를 오류색으로(`:792-796`).

**상태 기계**(UI 스레드는 절대 막지 않음, 작업은 워커 스레드, 결과는 `PostMessage`로):

```
Idle ──[켜기]──> Working(deadline = 시작+15초, [취소] 활성)
  Working 단계: ①해시 검증 ②원자 설치 ③자식 시작 ④준비 대기
     ④ 대기는 WaitForMultipleObjects(자식 핸들, 취소 이벤트, 준비 이벤트, 남은 시간)  ← 폴링·프로세스 재생성 없음
  Working ──준비 확인(pid·버전 일치)──> Done: 사용자 안내 없이 창 닫기(성공 토스트는 kagent가 띄움)
  Working ──자식 조기 종료 / 시간 초과 / 설치 실패──> Failed(코드·진단 표시, 단추 복귀, 창 유지)
  Working ──[취소]/창 닫기──> Cancelling: 우리가 띄운 자식이 아직 준비 전이면 종료, 이미 준비됐으면 그대로 두고 닫기 ──> Idle
```

- **닫히는 흐름 규칙**: 모든 종료 상태는 `Done` 또는 `Failed` 둘 중 하나이며 `Working`에서 영원히 머무는 경로가 없다. 상한은 **한 곳(15초)**: 지금의 15+15+3+1.5+10 중첩을 대체. `Done`이면 항상 창을 닫고 "켰습니다" 알림 후 (사용자 선택으로) 빠른 목록을 연다 — 이는 `c0fb8c4d`(KCLIPOPEN) 동작을 유지.
- 준비 신호: `kagent`가 시동 완료 때 이름 있는 이벤트(예: `Local\HwpAutoDocFit.kagent.ready.<suffix>`)를 켜게 하는 **신규 계약**이다. 현재는 `status.json` 폴링(`main.rs:1337-1344`)이라, 이벤트 방식이 가능한지(`instance_suffix` 시험 격리 포함)는 구현 카드에서 확인(B-6에 포함).
- 오류 코드는 지금 것을 그대로 쓴다: `KCLIP_CONSENT·KCLIP_MISSING·KCLIP_INTEGRITY·KCLIP_START_TIMEOUT·KCLIP_TIMEOUT·KCLIP_COMMAND`(`kclip_service.py`).
- 동의문은 **Rust 상수 한 곳**(현 `main.rs:1372` 문구). Python 복제본은 Python 소비자가 없어지면 삭제.

**클립보드 설정 페이지**(켜진 뒤): 상단 상태줄(기록 중·개수 · 단축키 칩 · 일시정지(30분/1시간/3시간/다시 켤 때까지 — Flutter는 2가지뿐이고 kagent 메뉴는 4가지이므로 **4가지로 합침**, `settings_menu.rs:15-16` ) · 모두 지우기(확인) · 끄기) + 설정 목록(보관 개수·항목 최대 MB·전체 최대 MB·보관 기간 / 자동 실행 / 스티커·항상 위·고르면 붙여넣기·엑셀 목표 결과·그림 미리보기 / 화면캡처 폴더 감시·폴더별 끄기 / 화면 테마는 ⑤에서 공용). 값 선택지는 `settings_menu.rs`의 `MAX_ITEMS`(6)·`MAX_TOTAL_MB`(5)·`MAX_ITEM_MB`(5)·`MAX_AGE_DAYS`(4)를 그대로 쓰되 **선택지에 없는 현재 값은 "현재 N"으로 보이는** 기존 규칙(`settings_menu.rs:226-231`)을 유지한다. 새로 얹는 것은 **단축키 변경·제외 앱 편집**(지금 UI 없음)뿐이며 사용자 확인 후 진행.

**kagent ⚙ 메뉴와의 관계**: 빠른 목록의 ⚙ 메뉴(`popup.rs:1208`)는 그대로 두고 맨 아래에 "전체 설정 열기…"만 추가한다(`ksettings.exe --page clip` 실행, 이미 떠 있으면 활성화). 두 화면은 같은 `Settings::apply`를 지나므로 값이 늘 같다(`settings_menu.rs:3-5` 주석과 같은 원칙).

---

## ⑤ 앱 설정·클립보드·검색(ktm)·가상폴더(kvf)·Dock이 하나의 소유자·공용 부품을 쓰게 하는 설계

### ⑤-1 소유자가 둘 이상인 설정 키 — 8개

| 기호 | 개념 | 저장 위치(소유자) | 현재 문제 | 단일화 안 |
|---|---|---|---|---|
| A | **화면 테마** | 앱 `theme_mode`(Python) · kagent `theme` · ktm `ui_theme` **와 별개 필드 `theme`("dark")** · kvf(kagent 파일을 읽음) | 앱이 저장하면 켜진 kagent에만 푸시(`application_service.py:122-128`, `kclip_service.py:42-53`). ktm은 앱 테마를 모름(`window.rs:230-240`) | 앱 `theme_mode`가 정본, 나머지는 읽기 전용 + `DocFitThemeChanged` 브로드캐스트 |
| B | **항상 위** | 앱 `always_on_top`(앱 창) · kagent `always_on_top`(빠른 목록) · 앱 `kclip_window.always_on_top`(옛 Flutter 스티커 창) | 이름이 같고 뜻이 다른 3곳 | 앱 창 항상 위는 `always_on_top` 유지, 빠른 목록은 이름 구분(`popup.always_on_top`)·옛 `kclip_window` 폐기 |
| C | **단축키** | kagent `hotkey` · ktm `hotkey`("Ctrl+Alt+Space") · kvf `hotkey`("Win+Shift+F") | 3개 파일·3개 검증(kagent ≤40자만, kvf `same_hotkey`로 kagent와 충돌만 검사) | 설정 창에서 한 번에 충돌 검사(kvf `settings.rs` `same_hotkey` 재사용), 저장은 각 앱 |
| D | **몽땅검색 켜기·로그인 자동 실행** | 앱 `mongttang_search`(`settings_schema.py:6`, 끄면 상주 프로그램 종료·자동 실행 해제) + ktm `autostart` | 한쪽만 바꾸면 불일치 | 앱 키 하나가 의도, ktm `autostart`는 그 파생값으로 동기화 |
| E | **kagent 로그인 자동 실행** | 레지스트리 `Run\HwpAutoDocFit.KAgent`(`settings_menu.rs:272-285`) — **UI 3곳**(Flutter 켜기 전·켜진 뒤 체크 2곳 + kagent 메뉴) + CLI `--autostart` | 상태 출처가 레지스트리라 설정 파일에 없음 | 그대로 레지스트리 정본, 설정 창은 읽고 쓰는 UI 하나만 |
| F | **타임머신 조사 폴더** | `timemachine.json` `roots`(앱) → kagent `timeline_roots`(푸시, `timemachine_service.py:836-855`) | 원본 1·사본 1 | 원본은 앱, kagent 값은 파생(읽기 전용 표시) |
| G | **숨길 메뉴 `hidden_tabs`** | 쓰기 Python(`application_service.py:20-23, 276`), 읽기 Dock(`lib.rs:206-218`) | **검증 규칙 이중**(범위 1~15·12·13 제외가 두 곳에 각각 구현) | 규칙을 `docfit_config` 한 곳으로 |
| H | **창 위치·크기** | 앱 `kclip_window`(x,y,width,height) · kagent `popup_*` · kvf `x/y/width/height` | 같은 종류 값이 3 형식 | 창 위치는 "설정"이 아니라 각 앱 상태이므로 설정 창에는 노출하지 않음, 옛 `kclip_window`만 폐기 |

별도 관찰(소유자 수에는 넣지 않음): 앱 `library_excluded_folders`(문서뷰·사진뷰)와 ktm `excluded_directories`(몽땅검색)는 둘 다 "제외 폴더"이지만 기능이 달라 같은 값인지 **미확인**(B-13).

### ⑤-2 하나의 창, 하나의 부품

- 설정 창 하나(`ksettings.exe`)에 영역 6개(③-2)를 두고 영역 ④·⑤는 각각 kagent·ktm/kvf에 **파이프/파일**로 위임한다. ktm의 기존 설정 모달(순환 클릭 17행)은 이 창으로 대체한다(ktm ⚙ 설정 칩은 `ksettings.exe --page search`를 연다) — 다만 ktm 모달은 검색 창 위에서 곧바로 뜨는 장점이 있어 **대체 여부는 사용자 결정**(⑧).
- **`ui_theme` 확장(공용 부품 목록)**. 현재 `ui_theme`은 색(`Palette` 28색)·테마 해석·`apply_window`·브로드캐스트뿐이며 컨트롤이 없다(`ui_theme/src/lib.rs:42-94`, `win.rs`). 지금 kagent·kvf·ktm이 각자 만든 것: 입력/IME 처리 3벌(`popup.rs:1413-1452`, `kvf/window.rs:1525-1559`, ktm은 표준 EDIT `window.rs:4118-4150`), 틀·배율 함수(`kagent/frame.rs`를 kvf가 `#[path]`로 컴파일). 설정 창에 **새로 필요한** 것:

| 부품 | 용도 | 비고 |
|---|---|---|
| 팔레트 추가색 | 입력칸 바탕·테두리·포커스, 비활성 글자, 스위치 on/off 트랙, 오류·경고·성공, 카드 바탕, 섹션 제목 | `Palette`에 필드 추가(고대비 변환 `high_contrast()` 포함, `lib.rs:170`) |
| 공용 컨트롤 크레이트 `ui_kit`(신설, `ui_theme` 의존) | 아래 전부 | `frame.rs`·IME 입력·배율 함수를 여기로 옮김 |
| 스위치·체크·라디오 | 항목 종류별 | 표준 `BUTTON` + 다크 처리 / 직접 그리기 중 선택(③-4) |
| 콤보(드롭다운) | 선택·글꼴 | 표준 `COMBOBOX`는 다크 처리가 번거로움 → 직접 그린 팝업 목록 권장(**미확인**: 표준 콤보의 다크 모드 품질) |
| 숫자 입력 + ± | 숫자 키 | |
| 텍스트 입력(IME) | 한글 | **표준 EDIT 권장**(IME·접근성) — ktm이 이미 씀 |
| 폴더 선택 | `IFileOpenDialog`(폴더 선택 옵션) | |
| 단축키 캡처 칸 | `hotkey` | kvf `hotkey.rs` 파서 재사용 |
| 체크·추가/삭제 목록 | 제외 앱·폴더·준말 | |
| 트리(2단) · 검색칸 · 스크롤 영역(휠·키보드) | 창 구조 | 스크롤 영역은 `popup.rs` 목록 스크롤 로직 재사용 가능성 **미확인** |
| 툴팁 · 변경 표시 점 · 오류 배너 · 확인 상자 | 피드백 | |
| 줄바꿈 안내문(DrawText, 폭 맞춤) | 긴 안내문(`SettingGuide.summary`) | 지금 Flutter 안내문이 길다 |
| 글꼴 생성·DPI 배율 함수 | 고DPI | `frame.rs:120`의 `GetDpiForWindow` 방식을 따름 |

### ⑤-3 앱 안에서 설정이 열리는 길과 Flutter에 남는 연결

현재 진입(확인한 것):

1. 상단 **톱니 아이콘**(`main.dart:1580-1587`) 및 도구 모음 항목 "설정"(`main.dart:432-437`) → 끝 서랍(`settingsDrawer`, `:3898-4036`) → "전체 설정 열기" → `SettingsDialog`(`:3984-4000`).
2. **사이드메뉴 12번**: 라벨은 '설정'이고(`tab_navigation_shell.dart:190`) 아이콘도 설정이나, 사이드바 목록 생성이 **12·13을 제외**한다(`:502`, 페이지도 `SizedBox()`). 즉 사이드메뉴에서 12번을 눌러 여는 경로는 코드상 없다(이전 문서의 "12번=설정"은 라벨 슬롯 기준; **실제 눌림 동작은 미확인**, B-3).
3. **하단 Dock**: Dock `TAB_NAMES[12]`는 '자료함'이고 `route(12)=VirtualFolder`(`dock/src/lib.rs:165-181, 225-229`)로 Flutter의 12='설정'과 **번호 의미가 다르다**. Dock 메뉴에는 설정 항목이 **없다**(`menu_items`·`native.rs` grep).
4. **단축키 창의 설정 메뉴**: kagent 빠른 목록 ⚙(`popup.rs:1208`) · ktm "⚙ 설정" 칩(`window.rs:1273`) · kvf에는 화면 진입 없음(트레이 메뉴만, 미확인).

새 진입(설계):

| 진입 | 방식 |
|---|---|
| Flutter 톱니·서랍 "전체 설정" | `ksettings.exe` 실행(단일 인스턴스 뮤텍스 + 이미 떠 있으면 활성화, Dock과 같은 방식 `docs/DOCK_DESIGN` §단일 인스턴스) |
| Dock | 오른쪽 끝 설정(톱니) 아이콘 또는 우클릭 메뉴 "설정…" 추가 → `ksettings.exe` (Dock 번호 체계와 별개, 번호 불일치는 정리 대상) |
| kagent ⚙ 메뉴 | "전체 설정 열기…" 추가 → `--page clip` |
| ktm ⚙ 설정 칩 | 모달 유지 + "전체 설정…" 링크 → `--page search` (대체는 사용자 결정) |
| 사이드메뉴 12 | 슬롯은 남겨 두되 호출은 `ksettings.exe`로(눌림 동작 확인 후) |

Flutter 쪽 연결을 **없애는 순서**(앞 단계가 끝나야 다음을 지운다):

1. `kclip_dialog.dart`의 설정·켜기·동의(`:634-689, 764-1060`)와 `kclip` 설정 명령(`enable·disable·autostart·settings_get/set·pause·resume·clear` 중 설정 쪽) — 관리창은 남김.
2. 서랍의 몽땅클립보드 항목(`main.dart:417`)을 "설정 열기"로 교체.
3. `SettingsDialog`+`setting_guides`+`kiosk_theme`+`doc_preview`+`kvf_panel`+`update_panel` (약 3,265줄) 삭제, 서랍 링크는 `ksettings.exe` 실행 한 줄로.
4. 서랍의 9개 스위치·테마·프로파일 선택: **메인 작업 화면에서 쓰는 옵션이므로 작업 화면 이식 카드와 함께**(그때까지는 서랍이 Rust 라이브러리를 통해 읽고 쓰도록 연결만 바꿈).
5. 쓸 메뉴 고르기·메뉴 표시 방식(`dock_mode_controller.dart`): Dock 전환 시퀀스(준비 응답·폴백)는 Flutter가 사이드바를 가진 동안은 Flutter가 소유 — 설정 창은 값을 저장하고 전환은 Flutter가 수행하는 **이행기 예외 1건**(DOCK 설계 §4 계약은 "제안"이며 확인된 구현이 아님, B-11).
6. 준말·상용구 관리(`abbreviation_manager`·`idiom_panel` 633줄): 저장 위치 확인 후 마지막에.
7. Python `get_settings`·`save_settings`·`ConfigurationService`·`settings_schema`·`settings_migrations`·`update_service`·`kclip_service`의 설정·켜기 부분 제거(읽기 전용 로더만 남김). 모든 단계에 되돌리기 스위치(`ui.settings=flutter|win32`)를 유지한다.

### ⑤-4 업데이트 확인(`update_service.py` 402줄)의 위치와 서명 검증(U13)

- 위치: 지금 Flutter 서랍의 `UpdatePanel`(`update_panel.dart` 174줄, 상태 `checking/downloading/installing`)과 Python 서비스가 한 쌍. → **설정 창 '프로그램·업데이트' 영역**에 [지금 확인]·진행 표시·"시작할 때 확인"(`check_updates_on_start`)을 두고, 시작 시·주기 확인(`start_periodic_checks`, `update_service.py:386`)과 버전 안내(`version_notice`, `:375-384`)는 상주 프로세스가 없으므로 앱(또는 후속 Rust 호스트)이 호출한다.
- 하는 일(확인): HTTPS GET(허용 호스트 `gitlab.aigov.go.kr`·`api.github.com`·`github.com`·`*.githubusercontent.com`, 포트 443만, `validate_url` `:78-85`) · `latest.json` 또는 GitHub 릴리스 + `SHA256SUMS` · 버전 비교 · SHA-256 검증(`verify_download`) · 설치 스크립트 실행(`powershell_command`, `:351-352`) · 포터블 설치(`portable_install`).
- **U13 답**: 서명 알고리즘은 **Ed25519**(`update_signature.py:1,16`), 서명 대상은 `signature` 키를 뺀 JSON의 정규화 바이트(`sort_keys`·구분자 `(",",":")`·`ensure_ascii=True`·UTF-8, `:3-4`), 공개키 32바이트 base64. **Rust로 가능**하다 — 단 저장소의 어떤 `Cargo.lock`에도 ed25519 크레이트가 없어 **의존성 추가(예: `ed25519-dalek`)가 필요**하고(grep 확인), 정규화 JSON 재현과 Windows CNG 대체 가능성은 확인하지 못했다(B-14). 또 이 기능은 지금 **기본 꺼짐·공개키 비어 있음**(`:19-24`)이라 현 동작은 검증을 하지 않는다 → 이식하더라도 "꺼짐 기본"을 그대로 옮기고, 켜는 시점은 별도 결정.
- 필수 사유 판단: HTTP는 WinHTTP(`windows-sys`), SHA-256은 이미 kagent가 자체 구현(`main.rs:1434-`)·CNG(`BCrypt`)로 가능 → Python·Flutter를 남길 사유 **없음**(제거 대상).
- 확인 필요(범위 밖이지만 기록): 업데이트 원본이 GitLab 경로(`update_service.py:24-27`)다. 팀 규칙은 "GitLab(origin)은 별도 명령 때까지 건드리지 않는다"(`AGENTS.md`)인데 이는 저장소 작업 규칙이고 앱 런타임 접근과의 관계는 문서에 없음(B-7). 이식 카드는 원본 URL을 바꾸지 않는다.

---

## ⑥ 다크·밝게·고대비, 고DPI, 키보드 접근성, 한글 입력

- **테마**: `ui_theme::resolve(pref, system_dark, high_contrast)`(`lib.rs:295`)와 `refresh/apply_window/is_theme_message`(`win.rs:61-124`)를 그대로 쓴다. 고대비면 선택과 무관하게 시스템 색(`Palette::high_contrast`, `lib.rs:170`, kagent 메뉴 주석 `settings_menu.rs:244`). 새 컨트롤이 쓰는 색은 모두 `Palette`에서만 가져오고 **직접 색 상수 금지**(Flutter에서 하드코딩 색을 토큰으로 바꾼 `2b5320b1` DARK1과 같은 원칙). 시험은 `Palette::text_pairs()`·`min_contrast()`(`lib.rs:219,249`)에 새 색 쌍을 더해 대비비 자동 검사.
- **고DPI**: `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)`(`kagent/main.rs:1103`)와 `GetDpiForWindow` 배율(`frame.rs:120`)을 `ui_kit`에 두고, `WM_DPICHANGED`에서 글꼴 재생성. 시험 배율 96·120·144·192(`kvf/vf_capture.rs:1`과 같은 집합).
- **키보드**: Tab·Shift+Tab 순환, 트리 ↑↓·→(펼침), 스위치 Space, 콤보 Alt+↓, 검색 Ctrl+F, Esc = 변경 있으면 확인 후 닫기, Enter = 포커스 항목 동작. 지금 Flutter 설정은 `CloseGuards`(변경 있으면 닫기 확인, `settings_dialog.dart:42-52, 61`)를 쓰므로 같은 동작을 유지.
- **한글 입력**: 표준 `EDIT`는 IME를 기본 제공(ktm 사용 중). 직접 그린 입력칸은 `WM_IME_COMPOSITION`·`WM_IME_ENDCOMPOSITION`·`WM_CHAR` 처리가 필요하며 popup·kvf에 이미 구현이 있다(`popup.rs:1413-1452`, `kvf/window.rs:1525-1559`)가 **복제 3벌**이라 `ui_kit`로 통합. 권장 = 숫자·글꼴·폴더·단축키 등 모든 입력은 표준 EDIT.
- **접근성(UIA)**: kagent·ktm·kvf의 직접 그린 창에는 UIA 제공자 코드가 없다(`WM_GETOBJECT`·`IAccessible` grep 결과 없음). 새 창에서 직접 그린 컨트롤이 스크린리더에 어떻게 보일지는 **미확인**(B-11 포함) — 표준 컨트롤(BUTTON·EDIT·COMBOBOX·TREEVIEW)을 쓰면 기본 제공되고, 직접 그리는 스위치·카드는 UIA 래퍼가 필요하다. **권장**: 표준 컨트롤 우선 + 다크는 `SetWindowTheme("DarkMode_Explorer")`·`WM_CTLCOLOR*`(트리·콤보 품질은 시험 후 판단).

---

## ⑦ 시험 계획과 사용자 화면 확인 지점

**기준선(변경 없음)**: 설정 관련 기존 시험 — Python `tests/test_configuration_service.py`·`test_settings*`·`test_kclip_service.py`·`test_kagent_settings_menu.py`·`test_update_*`·`test_hidden_tabs_setting.py`·`test_search_engine_settings.py` 등, Flutter `settings_dialog_test.dart`·`kclip_dialog_test.dart`·`sidebar_menu_settings_test.dart`·`theme_mode_setting_test.dart`·`update_panel_test.dart`·`search_engine_setting_test.dart`, Rust `kagent/src/settings_menu.rs` 단위 시험 9개(`:286-469`). 전환하는 동안 이 시험들이 같은 결과를 내야 한다(없애는 것은 대체 시험이 통과한 뒤).

1. **설정 왕복(Round-trip)**: 100잎 전부에 대해 "기본값→비기본값→저장→재로드→같은 값", 그리고 알 수 없는 키·스키마 밖 8개 키가 **쓰기 뒤에도 그대로 남는지**. 숫자는 문자열 그대로 `"5"`.
2. **마이그레이션 골든**: 옛 파일 N종(판 번호 없음·`*_rev` 1·옛 이름 키·오타 글꼴·`stage_choices.spacing.reset_spacing` …)을 Python `migrate_settings`와 Rust `migrate`에 넣어 **결과 JSON이 동일**(키 순서 포함). kagent `policy_rev` 이전은 별도 케이스.
3. **Flutter 화면과 저장 결과 차이 0**: 동일 시나리오(100잎 각각 값을 바꾸는 시나리오 스크립트)를 (가) 현 Flutter `SettingsDialog`(기존 하네스 `test_flutter_settings_gui.py`·`scripts/verify_flutter_flow.py`) (나) 새 Win32 창에서 실행하고 **`settings.json` 두 결과를 비교**(바이트 동일을 1차, 키 순서만 다르면 2차로 의미 동일 판정). 같은 방식으로 kagent 파일은 Flutter 관리창 대 새 페이지.
4. **동시 쓰기**: Python 쓰기 프로세스와 Rust 쓰기 프로세스를 동시에 돌려(임시 `APPDATA`) 키 유실 0, 잠금 상호 배제(B-1 해소), 중간에 프로세스를 죽여도 파일이 깨지지 않음(원자성).
5. **검증 동등성**: `configuration_service.save`의 거부 사례(범위 밖 숫자·정수 한정 키 소수·`min>max`·NUL·4097자·알 수 없는 키·작업 중)를 표로 만들어 Rust 검증이 **같은 입력에 같은 결과(코드 포함)**.
6. **켜기 흐름**(④): 가짜 `kagent`로 (성공·즉시 종료·시간 초과·해시 불일치·설치 잠김·취소·창 닫기)를 주입해 **모든 경로가 15초 안에 `Done` 또는 `Failed`로 끝나고 창이 닫히거나 오류가 보임**을 확인(무한 대기 회귀 방지). 실제 `kagent.exe`는 `KAGENT_TEST_ROOT`·`KAGENT_DATA` 격리 폴더에서만.
7. **캡처 모드**: `ksettings.exe --capture-dir <폴더> [--dpi 96|120|144|192] [--scenario <이름>|all] [--dark|--hc]` — ktm(`window.rs:4039-4050`)·kvf(`vf_capture.rs:1`)가 이미 쓰는 방식. 단축키·트레이·단일 인스턴스·실제 설정을 건드리지 않고 임시 폴더만 사용. 산출 PNG를 해상도·테마별로 저장해 **사용자 확인용 + 회귀 비교용**으로 쓴다. 화면 덤프(`ktm/src/view_dump.rs` 방식)로 사각형 좌표 골든도 같이.
8. **자원 측정**: 창을 열고 닫는 20회 반복 후 핸들·메모리 증가 0, 열려 있지 않을 때 **상주 프로세스 0개**(on-demand exe), 시작 시간(콜드/웜) 측정치를 Flutter 설정 열기와 비교해 기록.
9. **한글·접근성**: IME 조합 중 취소·확정, 고대비 켠 상태 캡처, 키보드만으로 전 항목 도달(스크립트).
10. **이 PC 안전**: 모든 시험은 `DOCFIT_FORBID_REAL_DATA=1`(`kfind_process.py:42`)·임시 `APPDATA`/`LOCALAPPDATA`. 한/글 COM이 필요한 시험은 `C:\HWP_AUTODOCFIT\.com-test.lock`(AGENTS.md)이나 이 카드 범위에는 COM 시험이 없다.

**사용자 화면 확인 지점**(각 단계 종료 때 사용자에게 캡처를 보여 승인):
(가) 켜기·동의 창의 첫 모습과 켜기 성공 후 닫히는 동작, 일부러 실패시킨 오류 표시 · (나) 클립보드 설정 페이지 · (다) 앱 설정 창 전체 구조(트리·검색·변경 표시·적용/되돌리기) · (라) 다크·고대비·192DPI 캡처 · (마) 서식 옵션 "적용 전·후 예시" 유지 여부 결정 · (바) Dock·톱니·⚙ 메뉴의 진입 위치 · (사) 업데이트 영역.

---

## ⑧ 단계 계획·카드 수 추정·위험

| 단계 | 내용 | 카드 |
|---|---|---|
| S0 | 이 설계(완료) | 1 |
| S1 | `ui_kit`(frame·IME·배율 이동) + `ui_theme` 팔레트 확장 + 컨트롤 시험 창 + 캡처 모드 | 3~4 |
| S2 | `docfit_config`: 스키마·검증·마이그레이션 골든·잠금·원자 저장·Python 대조 시험 | 3~4 |
| S3 | `ksettings.exe` 골격 + 클립보드 켜기·동의 창 + 클립보드 설정 페이지 + kagent 연동(전체 설정 열기·생명주기 라이브러리화) + Flutter 연결 교체 | 4~5 |
| S4 | 앱 설정 영역 ①~③(보고서·서식 / 작업·결과 / 프로그램) + 글꼴·폴더 컨트롤 + 검색·되돌리기 | 5~6 |
| S4b | "적용 전·후 예시" 미리보기 77종 이식(선택 — 사용자 결정) | 2~3 |
| S5 | 몽땅검색(ktm)·가상폴더(kvf)·메뉴·Dock 영역 + 진입점(Dock·⚙·칩) | 3~4 |
| S6 | 업데이트 확인 Rust(WinHTTP·SHA-256·서명 검증 크레이트·설치) | 3 |
| S7 | 준말·상용구 이식 → Flutter 설정·Python 설정층 제거 + 되돌리기 스위치 정리 | 3~4 |

**합계 약 28~32장**(S4b 포함 시 상단). 모든 단계에 되돌리기 스위치(`ui.settings=flutter|win32`·`menu_display_mode=sidebar`)를 유지하고, 차이 0이 확인되기 전에는 Flutter 화면·Python 설정층을 지우지 않는다.

**위험 3가지**

1. **설정 파일 이행기 동시 쓰기·키 유실** — Python과 Rust가 같은 `settings.json`을 쓰는 기간이 있고, 스키마 밖 키가 8개 이상 있다. 잠금 호환(B-1)이 깨지거나 Rust가 모르는 키를 지우면 사용자의 서식 설정이 사라진다. → 패치 방식 쓰기 + 모르는 키 보존 + 동시 쓰기 시험(⑦-4) + Rust 쓰기 도입 전 설정 파일 사본 자동 백업.
2. **직접 그린 컨트롤의 품질 후퇴(한글 IME·접근성·고DPI·고대비)** — Flutter가 거저 주던 것을 직접 구현해야 한다. UIA 제공자 코드가 저장소에 없다. → 표준 EDIT·표준 컨트롤 우선, `ui_kit` 시험 창으로 먼저 검증(S1), 캡처 회귀(⑦-7), 필요 시 컨트롤별 UIA 래퍼를 S1에서 판단.
3. **프로세스 간 시퀀스 회귀 — '켜기 무한 대기' 재발과 Dock 전환 폴백** — 켜기는 교체·종료·재시작을 포함하고(`lifecycle.rs:13-39`), Dock 전환은 Flutter가 소유한 시퀀스다. 이식하면서 제한 시간·성공 판정이 다시 어긋나면 같은 오류가 재현된다. → 상태 기계로 모든 경로가 단일 15초 상한에서 끝나도록 설계(④-3)하고 실패 주입 시험(⑦-6), Dock 전환은 이행기 예외로 Flutter에 남김.

(그 밖의 큰 불확실성: 미리보기 77종 이식 규모, 사용자 결정 사항 아래.)

**사용자 결정이 필요한 사항**

1. 새 exe `ksettings.exe` 신설(권장) vs `kagent.exe` 내장 — 후자는 클립보드 off일 때 실행이 어색하고 앱 설정까지 kagent가 짊어진다.
2. "적용 전·후 예시" 미리보기(Flutter 위젯 77종)를 Win32로 이식할지·단순 정지 이미지로 대체할지·뺄지.
3. 단축키 변경·제외 앱 편집 같은 **지금 UI가 없는 설정**을 새 창에 만들지.
4. ktm 설정 모달을 새 창으로 대체할지 / 링크만 추가할지.
5. Dock에 설정 진입(아이콘·우클릭)을 둘지.

---

## 부록 A. 앱 `settings.json` 키 전수표(97개)

열 설명: **종류**는 `fields()` 분류(중첩은 잎 개수). **화면 위치(Flutter)**는 `setting_guides.dart`의 분류, 안내문이 없으면 `_categoryOf`(`settings_dialog.dart:184-194`)의 그룹 규칙. **참조 소스**는 키 문자열이 등장하는 소스의 대분류(Py=Python 소스, Fl=Flutter, Rs=Rust) — 읽는 쪽 추정이며 문자열 검색 결과라 `color`처럼 일반어는 잡음이 있음. 소유자가 둘 이상인 키는 비고에 ⑤ 기호.

| 키 | 종류 | 기본값 | 검증 | 화면 위치(Flutter) | 참조 소스 | 비고 |
|---|---|---|---|---|---|---|
| `shell_menu` | boolean | `false` | bool만 | 일반 | Py+Fl |  |
| `menu_display_mode` | (화면 밖) | `"sidebar"` | sidebar·dock(전용 명령) | - | Py+Fl | EXCLUDED(화면·`save_settings` 제외) |
| `mongttang_search` | boolean | `true` | bool만 | 일반 | Py+Fl | 소유자 2 — ⑤-D |
| `file_rename_sendto` | boolean | `false` | bool만 | 프로그램·네트워크(분류 규칙) | Fl |  |
| `rust_rename_preview` | (화면 밖) | `true` | save_settings 불가(내부·전용 경로) | - | Py | EXCLUDED(화면·`save_settings` 제외) |
| `rust_timemachine_scan` | (화면 밖) | `true` | save_settings 불가(내부·전용 경로) | - | Py | EXCLUDED(화면·`save_settings` 제외) |
| `rust_search_hierarchy` | (화면 밖) | `true` | save_settings 불가(내부·전용 경로) | - | Py | EXCLUDED(화면·`save_settings` 제외) |
| `prevent_word_split` | boolean | `true` | bool만 | 문장·줄 정리 | Py+Fl |  |
| `ratio_shrink` | boolean | `false` | bool만 | 문장·줄 정리 | Py+Fl |  |
| `punctuation` | boolean | `true` | bool만 | 문장·줄 정리 | Py+Fl |  |
| `punctuation_threshold` | number | `"5"` | 0~1000 숫자 문자열, 정수 | 문장·줄 정리 | Py+Fl |  |
| `keep_punctuation_set_together` | boolean | `true` | bool만 | 문장·줄 정리 | Py+Fl |  |
| `color_mark_on` | boolean | `false` | bool만 | 문장·줄 정리 | Py+Fl |  |
| `color` | text | `"blue"` | 문자열≤4096·NUL 금지 | 문장·줄 정리 | Py+Fl | 'color'는 일반어라 Rs 잡음 제외 |
| `autoclose` | boolean | `false` | bool만 | 결과·기록 + 설정 서랍 | Py+Fl |  |
| `stdformat` | boolean | `true` | bool만 | 보고서 모양 | Py+Fl |  |
| `verify` | boolean | `false` | bool만 | 결과·기록 + 설정 서랍 | Py+Fl |  |
| `integrity_report_file` | boolean | `false` | bool만 | 결과·기록 + 설정 서랍 | Py+Fl |  |
| `final_review_file` | boolean | `false` | bool만 | 결과·기록 + 설정 서랍 | Py+Fl |  |
| `report_files_rev` | (화면 밖) | `2` | save_settings 불가(내부·전용 경로) | - | Py | EXCLUDED(화면·`save_settings` 제외) |
| `two_pass_processing` | boolean | `false` | bool만 | 문장·줄 정리 | Py+Fl |  |
| `retry_body` | number | `"15"` | 0~1000 숫자 문자열, 정수 | 문장·줄 정리 | Py+Fl |  |
| `retry_table` | number | `"5"` | 0~1000 숫자 문자열, 정수 | 문장·줄 정리 | Py+Fl |  |
| `linespacing_min` | number | `"140"` | 0~1000 숫자 문자열, 정수 | 문장·줄 정리 | Py+Fl |  |
| `linespacing_max` | number | `"200"` | 0~1000 숫자 문자열, 정수 | 문장·줄 정리 | Py+Fl |  |
| `table_linespacing_min` | text | `"100"` | 문자열≤4096·NUL 금지 | 프로그램·네트워크(분류 규칙) | Py |  |
| `table_spacing_min` | text | `"-25"` | 문자열≤4096·NUL 금지 | 프로그램·네트워크(분류 규칙) | Py |  |
| `label_symbols` | 중첩 4개 잎 | `{"□": false, "※": false, "*": false, …` | bool(잎 4개) | 보고서 모양 | Py+Fl |  |
| `label_symbols_rev` | (화면 밖) | `2` | save_settings 불가(내부·전용 경로) | - | Py | EXCLUDED(화면·`save_settings` 제외) |
| `paren_shrink` | boolean | `true` | bool만 | 보고서 모양 | Py+Fl |  |
| `paren_label_bold` | boolean | `true` | bool만 | 보고서 모양 | Py+Fl |  |
| `std_margin` | boolean | `true` | bool만 | 보고서 모양 | Py+Fl |  |
| `std_ratio` | boolean | `true` | bool만 | 보고서 모양 | Py+Fl |  |
| `std_linespacing` | boolean | `true` | bool만 | 보고서 모양 | Py+Fl |  |
| `std_title` | boolean | `true` | bool만 | 보고서 모양 | Py+Fl |  |
| `std_title_auto` | boolean | `true` | bool만 | 보고서 모양 | Py+Fl |  |
| `std_attachment_auto` | boolean | `true` | bool만 | 보고서 모양 | Py+Fl |  |
| `std_midtitle_auto` | boolean | `true` | bool만 | 보고서 모양 | Py+Fl |  |
| `std_midtitle_bold` | boolean | `true` | bool만 | 보고서 모양 | Py+Fl |  |
| `abbreviations` | (화면 밖) | `{}` | save_settings 불가(내부·전용 경로) | - | Py | EXCLUDED(화면·`save_settings` 제외) |
| `abbreviation_defaults` | boolean | `true` | bool만 | 프로그램·네트워크(분류 규칙) | Py |  |
| `abbreviation_source` | (화면 밖) | `"document"` | save_settings 불가(내부·전용 경로) | - | Py | EXCLUDED(화면·`save_settings` 제외) |
| `std_title_bold` | boolean | `true` | bool만 | 보고서 모양 | Py+Fl |  |
| `std_dateinfo` | boolean | `true` | bool만 | 보고서 모양 | Py+Fl |  |
| `std_dateinfo_bold` | boolean | `true` | bool만 | 보고서 모양 | Py+Fl |  |
| `std_symbols` | boolean | `true` | bool만 | 보고서 모양 | Py+Fl |  |
| `std_hanging_indent` | boolean | `true` | bool만 | 보고서 모양 | Py+Fl |  |
| `std_supplement_indent` | boolean | `true` | bool만 | 보고서 모양 | Py+Fl |  |
| `std_symbol_box_bold` | boolean | `false` | bool만 | 보고서 모양 | Py+Fl |  |
| `std_symbol_o_bold` | boolean | `true` | bool만 | 보고서 모양 | Py+Fl |  |
| `std_symbol_dash_bold` | boolean | `false` | bool만 | 보고서 모양 | Py+Fl |  |
| `std_symbol_note_bold` | boolean | `false` | bool만 | 보고서 모양 | Py+Fl |  |
| `std_marker_bold_consistency` | boolean | `true` | bool만 | 보고서 모양 | Py+Fl |  |
| `std_parspace` | boolean | `true` | bool만 | 문단 간격 | Py+Fl |  |
| `std_remove_blank_lines` | boolean | `true` | bool만 | 문단 간격 | Py+Fl |  |
| `std_parspace_chapter` | number | `"15"` | 0~1000 숫자 문자열 | 보고서 모양(분류 규칙) | Py+Fl |  |
| `std_parspace_midtitle` | number | `"15"` | 0~1000 숫자 문자열 | 보고서 모양(분류 규칙) | Py+Fl |  |
| `std_parspace_box` | number | `"15"` | 0~1000 숫자 문자열 | 보고서 모양(분류 규칙) | Py+Fl |  |
| `std_parspace_circle` | number | `"8"` | 0~1000 숫자 문자열 | 보고서 모양(분류 규칙) | Py+Fl |  |
| `std_parspace_dash` | number | `"4"` | 0~1000 숫자 문자열 | 보고서 모양(분류 규칙) | Py+Fl |  |
| `std_parspace_note` | number | `"0"` | 0~1000 숫자 문자열 | 보고서 모양(분류 규칙) | Py+Fl |  |
| `std_parspace_rev` | (화면 밖) | `2` | save_settings 불가(내부·전용 경로) | - | Py | EXCLUDED(화면·`save_settings` 제외) |
| `std_supplement_rev` | (화면 밖) | `2` | save_settings 불가(내부·전용 경로) | - | Py | EXCLUDED(화면·`save_settings` 제외) |
| `std_parspace_return_percent` | number | `"150"` | 0~1000 숫자 문자열 | 문단 간격 | Py+Fl |  |
| `std_title_subtitle_pt` | number | `"15"` | 0~1000 숫자 문자열, >0 | 보고서 모양 | Py+Fl |  |
| `title_owner_text` | text | `""` | 문자열≤4096·NUL 금지 | 보고서 모양 | Py+Fl |  |
| `std_table_header` | boolean | `true` | bool만 | 표 | Py+Fl |  |
| `active_format_profile` | (화면 밖) | `""` | save_settings 불가(내부·전용 경로) | - | Py | EXCLUDED(화면·`save_settings` 제외) |
| `symbol_fonts` | 중첩 12개 잎 | `{"□": {"font": "HY견고딕", "size": "17"}…` | 잎별: .size 숫자>0, 그 외 문자열; label_symbols는 bool | 항목기호 글꼴(분류 규칙) | Py |  |
| `table_fonts` | 중첩 4개 잎 | `{"header": {"font": "한컴돋움", "size": "…` | 잎별: .size 숫자>0, 그 외 문자열; label_symbols는 bool | 항목기호 글꼴(분류 규칙) | Py+Fl |  |
| `hwp_font_folder` | text | `""` | 문자열≤4096·NUL 금지 | 프로그램·네트워크 | Py+Fl |  |
| `paste_add_folder` | text | `""` | 문자열≤4096·NUL 금지 | 결과·기록 | Py+Fl |  |
| `always_on_top` | boolean | `true` | bool만 | 프로그램·네트워크 + 설정 서랍 | Py+Fl+Rs | 소유자 3 — ⑤-B |
| `check_updates_on_start` | boolean | `true` | bool만 | 프로그램·네트워크 | Py+Fl |  |
| `table_spacing` | boolean | `true` | bool만 | 문장·줄 정리 | Py+Fl |  |
| `log_file` | boolean | `false` | bool만 | 결과·기록 + 설정 서랍 | Py+Fl |  |
| `developer_mode` | boolean | `false` | bool만 | 개발자 도구 + 설정 서랍 | Py+Fl |  |
| `freeze_tracing` | boolean | `false` | bool만 | 개발자 도구 | Py+Fl |  |
| `all_include_spacing` | boolean | `true` | bool만 | 작업 카드 기본값 | Py+Fl |  |
| `all_exclude_tables` | boolean | `false` | bool만 | 작업 카드 기본값 | Py+Fl |  |
| `all_exclude_pagefit` | boolean | `false` | bool만 | 작업 카드 기본값 | Py+Fl |  |
| `all_exclude_linespacing` | boolean | `false` | bool만 | 작업 카드 기본값 | Py+Fl |  |
| `unify_exclude_tables` | boolean | `false` | bool만 | 작업 카드 기본값 | Py+Fl |  |
| `unify_exclude_spacing` | boolean | `false` | bool만 | 작업 카드 기본값 | Py+Fl |  |
| `unify_exclude_pagefit` | boolean | `true` | bool만 | 작업 카드 기본값 | Py+Fl |  |
| `unify_keep_layout` | boolean | `true` | bool만 | 작업 카드 기본값 | Py+Fl |  |
| `unify_full_format` | boolean | `false` | bool만 | 작업 카드 기본값 | Py+Fl |  |
| `unify_result_window` | boolean | `false` | bool만 | 결과·기록 + 설정 서랍 | Py+Fl |  |
| `spacing_exclude_reset` | boolean | `false` | bool만 | 문장·줄 정리 | Py+Fl |  |
| `spacing_enabled` | boolean | `true` | bool만 | 프로그램·네트워크(분류 규칙) | Py+Fl |  |
| `spacing_exclude_pagefit` | boolean | `true` | bool만 | 작업 카드 기본값 | Py+Fl |  |
| `spacing_cell_image_to_bg` | boolean | `false` | bool만 | 프로그램·네트워크(분류 규칙) | Py+Fl |  |
| `all_cell_image_to_bg` | boolean | `false` | bool만 | 프로그램·네트워크(분류 규칙) | Py+Fl |  |
| `ui_tk_fallback` | (화면 밖) | `false` | save_settings 불가(내부·전용 경로) | - | Py | EXCLUDED(화면·`save_settings` 제외) |
| `hierarchy_engine` | (화면 밖) | `false` | save_settings 불가(내부·전용 경로) | - | Py | EXCLUDED(화면·`save_settings` 제외) |
| `theme_mode` | (화면 밖) | `"system"` | system·light·dark(전용 명령) | - | Py+Fl | EXCLUDED(화면·`save_settings` 제외); 소유자 4 — ⑤-A |
| `search_engine` | choice | `"kfind"` | kfind·legacy | 프로그램·네트워크 | Py+Fl |  |

화면 밖 키 14개(EXCLUDED): `menu_display_mode`·`rust_rename_preview`·`rust_timemachine_scan`·`rust_search_hierarchy`·`report_files_rev`·`label_symbols_rev`·`abbreviations`·`abbreviation_source`·`std_parspace_rev`·`std_supplement_rev`·`active_format_profile`·`ui_tk_fallback`·`hierarchy_engine`·`theme_mode`. 이 중 `menu_display_mode`·`theme_mode`는 전용 명령(`set_menu_display_mode`·`set_theme_mode`)으로만 바뀐다(검증 `settings_schema.py:147-179`).

## 부록 B. 미확인 항목(14건)

| # | 내용 | 확인 방법 |
|---|---|---|
| B-1 | Python `msvcrt.locking`(1바이트 `LK_LOCK`)과 Rust `LockFileEx`의 상호 배제 호환 | 두 프로세스 동시 쓰기 시험(⑦-4) |
| B-2 | ktm 설정 파일 손상 시 시동 동작(`window.rs:4050`의 `?` 이후 처리) | ktm 시동부 정독 + 손상 파일 시험 |
| B-3 | 사이드메뉴 '12번(설정)'을 눌렀을 때 실제 동작(`tab_navigation_shell.dart:502`가 12·13을 목록에서 제외) | 앱 실행 확인(이번 조사는 읽기 전용) |
| B-4 | Flutter 관리창과 kagent Win32 빠른 목록의 기능 차(메모·엑셀 변환·드롭·분류·미리보기) | 기능 대조표 카드 |
| B-5 | '켜기 무한 대기' 원인이 `1aef4013` 수정으로 해소됐는지·실제 원인 | 실기 재현 시험(`KAGENT_TEST_ROOT` 격리) |
| B-6 | kagent 준비 이벤트 신설 가능 여부·단독 시동(`--setup`류)과 단일 인스턴스 뮤텍스 상호작용(`main.rs` 시동부 미정독) | 구현 카드 |
| B-7 | 업데이트 원본 GitLab 경로와 `AGENTS.md`의 GitLab 금지 지시의 관계 | 사용자 확인 |
| B-8 | 사진첩·문서 격자 등 다른 서비스가 `settings.json`에 넣는 키 전수 | 서비스별 정독 |
| B-9 | 앱은 `APPDATA`(로밍), Rust 앱은 `LOCALAPPDATA`(로컬)를 쓰는 의도·로밍 프로필 환경 영향 | 사용자·배포 환경 확인 |
| B-10 | 문서 작업 중 상태를 설정 창(별도 프로세스)이 알 방법 | 앱 쪽 신호 방식 설계 |
| B-11 | Dock 전환(준비 응답·폴백) 파이프 계약의 실제 구현(DOCK §4 명령 이름은 "제안") 및 직접 그린 컨트롤의 UIA 노출 방식 | Dock 코드 정독·UIA 시제품 |
| B-12 | 상용구(`idioms`)·서식 프로파일(`formats/`) 저장 위치·형식, 서식 프로파일 선택(`profilePicker`)의 이행 범위 | Python 서비스 정독 |
| B-13 | 앱 `library_excluded_folders`와 ktm `excluded_directories`가 같은 개념인지 | 두 기능 정독 |
| B-14 | Ed25519 Rust 대체의 Windows CNG 지원 여부·정규화 JSON 재현 시험 | 크레이트 시제품 |

## 부록 C. Python·Flutter를 남기는 경우와 필수 사유

| 남기는 것 | 필수 사유(근거) | 판정 |
|---|---|---|
| Python **서식 엔진이 설정 값을 읽는 로더**(`settings.json` 읽기 전용) | 서식 처리 엔진이 Python이고 한/글 COM 구동 때문(`docfit_engine/{pipeline,formatter_*,page_fit,spacing}.py`가 키를 읽음, 부록 A 참조 소스 32개 중 엔진 계열). 설정 *소유·검증·저장·마이그레이션*은 필수 사유 없음 | 읽기만 남김, 나머지 제거 |
| Python **작업 카드 옵션 쓰기**(`all_*`·`unify_*`·`spacing_*`·`stage_choices` 등) | 작업 화면이 아직 Flutter·Python이므로 이행기에만. 작업 화면 이식 카드에서 제거 | 이행기 한정 |
| Flutter **설정 서랍 9개 스위치·프로파일 선택·메뉴 표시 방식 시퀀스** | 메인 작업 화면의 일부이고 Dock 전환 시퀀스를 Flutter가 소유하는 동안만 | 이행기 한정(⑤-3 순서 4·5) |
| Flutter **설정 화면 자체**(`SettingsDialog`·서랍 몽땅클립보드 항목·`kclip` 설정부) | 없음 — Win32 컨트롤로 모두 구현 가능 | **제거 대상** |
| Python **`kclip_service` 켜기·상태·설정**, `update_service`, `configuration_service`, `settings_schema`, `settings_migrations`, `settings_service`(쓰기) | 없음 — kagent 생명주기·파이프·WinHTTP·CNG/`sha2`·Ed25519 크레이트로 대체 가능(⑤-4, B-14는 크레이트 선택 문제) | **제거 대상** |
| Python **COM이 필요한 부분**(한/글 글꼴 목록 `hangul_fonts` 등) | 글꼴 목록은 한/글 폴더 + Windows 글꼴 열거이며 COM 불필요로 보이나 `docfit_core.hangul_fonts` 내부를 이번에 읽지 않음 | 미확인(B-12 계열) — 확인 전에는 호출만 유지 |

