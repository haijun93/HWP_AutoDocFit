# Python 설정 서비스 의존 줄이기 — 1단계 설계 (SETTINGSWIN4)

작성: 2026-11-21 · 브랜치 `feature/SETTINGSWIN4` · **설계만(이 문서에 해당하는 코드 변경 없음)**
근거: `docs/SETTINGS_WIN32_DESIGN_2026-11-20.md` ②·⑤-3·부록 C, `docs/DEV_PLAN_2026-11-28.md`(G2), 이번 조사(`grep` 으로 읽은 위치는 `파일:줄`).
**미확인**은 읽지 못했거나 실행해 보지 못한 것이다.

## 0. 요약

- 앱 `settings.json` 을 건드리는 Python 지점은 **쓰기 5갈래·읽기 6갈래**(아래 ①). 이 중 `docfit_config` 로 바꿀 수 있는 것은 **설정 창이 편집하는 키**(`ConfigurationService` 의 `get_settings`·`save_settings`)이고, 서식 엔진(`설정_불러오기`)·작업 카드 옵션·서비스별 상태 키는 이행기에 Python 이 그대로 읽고 쓴다.
- **권장 호출 방식 = 'settings-core' 명령줄(CLI) 호출, 읽기부터**. 이유: 이미 있고(`docfit_config/src/main.rs`: `get`·`set`·`migrate`·`validate`), 같은 파일을 같은 잠금으로 다루는 시험(`tests/test_settings_compare.py` 의 `test_python_and_rust_process_locks_exclude_each_other`)이 통과하며, Python 쪽 새 의존(DLL 적재·버전 맞물림)이 없다. 호출당 프로세스 1개 비용은 설정 창·서랍을 열 때만 생기는 빈도라 문제가 아니고, **서식 엔진이 매 작업마다 읽는 길에는 쓰지 않는다**(엔진은 계속 Python 로더).
- 이행 순서: **① 읽기 대조(그림자 모드) → ② 설정 창 쓰기만 Rust(Python 쓰기는 같은 잠금으로 공존) → ③ Python 설정 서비스 제거(읽기 전용 로더만 남김)**. 단계마다 `settings_backend`(`python`|`rust`) 되돌리기 스위치.
- 가장 큰 위험: (1) 서식 엔진이 `DEFAULTS` 전체를 읽고 **통째로 되쓴다**(`설정_저장`: `current.update(설정)`)는 점 — 낡은 값 덮어쓰기. (2) 검증 규칙이 두 곳(Python `ConfigurationService`·Rust `validate_patch`) + 설정 창 규칙(더 엄격)으로 나뉜 점. (3) `settings_ui` 기본 전환 시 앱이 외부 쓰기 뒤 설정을 다시 읽는 연결이 없다는 점(SETTINGSWIN3 미확인 7).

## ① 앱 설정 파일을 읽고 쓰는 모든 지점(표)

파일: `%APPDATA%\HwpAutoDocFit\settings.json`(없으면 `LOCALAPPDATA`→`~/.hwp_auto_docfit`; `docfit_config::settings_path()` 가 같은 순서).

### ①-1 Python — 쓰기

| # | 위치 | 하는 일 | 키 | 비고 |
|---|---|---|---|---|
| W1 | `docfit_app/settings_service.py` `SettingsService.update/_update` | **모든 Python 쓰기의 단일 길**: `ProcessLock(settings.json.lock)` → 읽기 → 콜백 → 같으면 안 씀 → 임시 파일+`fsync`+`os.replace` | 전체 | Rust `atomic_write`/`set` 과 같은 잠금 파일·같은 프로토콜(상호 배제 시험 통과) |
| W2 | `configuration_service.py` `ConfigurationService.save` | 설정 창(Flutter) 일괄 저장: 검증 → revision 비교(`SETTINGS_CHANGED`) → 마이그레이션 → 교차 검증(최소≤최대) | `fields()` 잎 100개 | **Rust `docfit_config::set` 와 동일 계약**(`scripts/settings_compare.py` 가 확인). 대체 1순위 |
| W3 | `application_service.py` `dispatch` | `set_option`(9키)·`set_theme_mode`·`set_menu_display_mode`·`set_hidden_tabs`·`set_sidebar_collapsed`·카드 옵션 4개·`stage_choices` | 소수 | 키별 전용 규칙(예 `clean_hidden_tabs`). 앞의 둘은 설정 창과 키가 겹침 |
| W4 | `desktop_web_ui.py:2525,2674,3344,3366,3384,3971-3975` | `kclip_settings_mode`·`always_on_top`·`sidebar_collapsed`·`hidden_tabs`·`theme_mode`·`sidebar_mode_side/y` | 소수 | W3 와 같은 키를 중복 구현(브리지 메서드 + 서비스 둘 다) |
| W5 | 서비스별 직접 쓰기 | `settings_window_service.py:57`(`settings_ui`)·`outliner_service.py:62`(`outliner_ui`)·`library_exclusions.py`(`library_excluded_folders`)·`external_apps_service.py:281,293`(`external_app_paths`)·`thumb_size_setting.py:44`(`ui_thumb_sizes`)·`kclip_window_state.py:60`(`kclip_window`)·`update_service.py:379`(`last_run_version`)·`table_units_service.py:106`·`abbreviation_service.py`(`abbreviations`)·`table_template_service.py:52`·`file_rename_service.py`·`application_service.py:384`(`file_rename_sendto`) | 키 각자 | 모두 W1 을 지난다. **스키마 밖 키**가 대부분 — Rust 는 모르는 키를 보존해야 한다 |
| W6 | `docfit_engine/formatter_utils.py:439` `설정_저장(설정)` | 서식 엔진·옛 Tk 창이 **읽어 둔 설정 전체**를 `current.update(설정)` 로 되씀 | `기본_설정`(=`DEFAULTS` 전체)·`stage_choices`·`abbreviations` | **가장 위험한 쓰기**: 읽은 뒤 다른 곳(설정 창)이 바꾼 값을 낡은 값으로 덮을 수 있다(⑤ 위험 1) |

### ①-2 Python — 읽기

| # | 위치 | 하는 일 | 대체 |
|---|---|---|---|
| R1 | `configuration_service.py` `snapshot` | 기본값 병합+마이그레이션+`revision` → Flutter 설정 창 | Rust `get`(`merged`) — 동등 시험 있음 |
| R2 | `application_service.py:140` `get_state` | `{**DEFAULTS, **migrate(read())}` 로 시작 상태 | 이행기 유지 → 3단계에서 Rust `get` |
| R3 | `docfit_engine/formatter_utils.py:413` `설정_불러오기` | 엔진이 작업마다 읽음(`기본_설정` 키만 + `stage_choices`·`abbreviations`) | **필수 사유 있음(서식 엔진은 Python)** — 읽기 전용 로더로 남김(부록 C). Rust 로 바꾸지 않음 |
| R4 | `desktop_web_ui.py:578,1539,2286,2462,2514,3335,3356,3374` | `developer_mode`·`always_on_top`·`sidebar_collapsed`·`hidden_tabs`·`theme_mode` 읽기 | 키별 getter → 3단계에서 Rust 읽기 한 곳 |
| R5 | `kfind_process.py:82-88`, `timemachine_service.py`(`search_engine`) | `search_engine` 읽기 | 키 한 개 — 읽기 로더 유지 |
| R6 | `docfit_ui/legacy.py:381,1119,1434` | 옛 Tk 창 설정 읽기(`ui_tk_fallback`) | Tk 제거 때 함께 삭제 |

### ①-3 Rust·기타 프로세스가 읽는 파일

| 소비자 | 읽는 파일 | 키 | 쓰기 | 근거 |
|---|---|---|---|---|
| **`settings_win`**(새 창) | 앱 `settings.json` | 전체 | `docfit_config::set` | `settings_win/src/model.rs` |
| **Dock** | 앱 `settings.json` | `hidden_tabs`(읽기 전용) | 없음 | `dock/src/native.rs:339,503-512`, 파서 `dock/src/lib.rs` |
| **kagent** | `%LOCALAPPDATA%\HwpAutoDocFit\kagent\settings.json`(자체 파일) | 24키 | kagent 만(파이프 `settings_set`) | `kagent/src/main.rs:399,1154`, `settings_window.rs:99-104` |
| **ktm** | `…\ktm\settings.json` | 23키 | ktm 만 | `ktm/src/window.rs:4115,4268` |
| **kvf** | `…\kvf\settings.json` + kagent 파일(읽기 전용) | 8키 + 테마·단축키 | kvf 만 | `kvf/src/paths.rs:7`, `tray.rs:15-17`, `window.rs:336,510` |
| **kfind** | `…\kfind\settings.json` | `fallback` | Python(`kfind_process.py`) | `kfind_process.py:49-82` |
| `hwp-auto-docfit.py` | (엔진 경유 R3·W6) | | | 이 카드에서 읽기만 함, 고치지 않음 |

→ **ktm·kagent·kvf·kfind 는 앱 `settings.json` 을 읽지 않는다**(Dock 의 `hidden_tabs` 만 예외). 따라서 "ktm/kagent/kvf/dock 의존"은 파일 소유 문제이지 Python 서비스 의존이 아니다(`kclip_service`·`timemachine_service` 가 파이프로 kagent 설정을 중계하는 것만 Python 경유, 설계 ⑤-1 표 참조). 이 단계의 범위는 **앱 `settings.json` 의 Python 소유권**이다.

## ② `docfit_config` 를 Python 에서 부르는 방법 비교

| 안 | 방식 | 장점 | 단점 | 시험·근거 |
|---|---|---|---|---|
| **A. CLI(권장)** | `subprocess` 로 `settings-core get\|set\|validate\|migrate <파일> [json] [revision]`, 결과는 한 줄 JSON(`{"ok":true,"value":…}`/`{"ok":false,"code":…}`) | 이미 구현·시험됨, 프로세스 격리(Python 이 죽어도 쓰기 원자성 유지), 번들에 exe 하나 추가(`resources/`), 버전 불일치 시 exe 만 교체 | 호출당 프로세스 시작(수십 ms, **미측정**), 큰 값 인자 길이 제한(Windows 명령줄 약 32K자 — `set` 패치를 **표준 입력**으로 받도록 확장 필요) | `tests/test_settings_compare.py`(동등+잠금 상호 배제 통과), `scripts/settings_compare.py` |
| B. FFI(`cdylib`+`ctypes`) | `docfit_config` 를 DLL 로, Python `ctypes` 로 호출 | 호출 빠름, 인자 제한 없음 | DLL 적재·메모리 소유권·PyInstaller 번들·**Rust 패닉이 Python 프로세스를 죽임**, 새 빌드 산출물 | 미시험(이미 `kclip_ffi.dll` 방식이 있어 선례는 있음, 설계 ④-1) |
| C. 파이프/상주 | 설정 서버 상주 | 호출 빠름 | **상주 자원 추가(원칙 위반)**, 시동 순서 의존 | 설계 ②-3 안3 과 같은 이유로 기각 |
| D. Python 이 JSON 만 읽고 Rust 는 쓰기만 | 읽기는 지금 로더 그대로, 쓰기만 CLI | 가장 작은 변경 | Python 쪽 검증·병합 규칙이 남아 이중 규칙 유지 | 2단계(쓰기)에서 사실상 채택 |

**권장**: **A(CLI)** — 단 두 가지를 먼저 고친다: ① `set` 패치를 명령줄이 아니라 **표준 입력**으로 받기(길이 제한·인용 문제 회피; 목록 키가 늘어 패치가 커질 수 있음) ② 호출 래퍼 `docfit_app/settings_core_client.py`(새, 한 파일)가 **모든 실패를 `ServiceError`(기존 오류 코드)로 바꾸고 `python` 스위치로 즉시 되돌릴 수 있게** 한다. FFI(B)는 CLI 호출 비용이 실측으로 문제로 확인될 때만(설정 창 열기·서랍 열기 경로에서 체감 지연이 있을 때).

## ③ 이행 순서

### 1단계 — 읽기(그림자 모드, 사용자 눈에 변화 없음)
1. `settings_core_client.get()` 추가. `ConfigurationService.snapshot()` 이 결과를 **Python 값과 비교만** 한다(그림자): 다르면 로그(`settings-shadow.log`)에만 한 줄, 화면은 Python 값. 앱 실행 중 실제 사용자 파일로 **차이 0 이 쌓이는지** 며칠 관찰.
2. 비교 대상 확대: `revision` 값도 같아야 한다(Python `revision()` = 정렬 JSON sha256, Rust `revision()` 동일 알고리즘 — **한글·키 순서 케이스는 `scripts/settings_compare.py` 에 없음, 추가 필요**).
3. 완료 기준: 실제 설정 파일 N종(이 PC 것은 **읽기만**) + 합성 파일에서 `get`·`migrate` 차이 0.

### 2단계 — 쓰기(설정 창 쓰기만 Rust)
1. 앱 **안**의 Flutter 설정 창 `save_settings` 만 `settings_core_client.set()` 로(스위치 `settings_backend=rust`). `JOB_RUNNING`(작업 중 거부)은 Python 이 앞에서 먼저 확인(작업 상태를 아는 쪽이 Python 이므로).
2. 같은 파일에 Python W1·W3~W6 이 계속 쓰므로 **같은 잠금**(이미 호환 시험 통과)을 쓴다. 쓰기는 모두 "읽고→일부만 바꿔→쓰기"이고 Rust 는 모르는 키를 보존한다(`preserves_unknown…` 시험).
3. **W6(엔진 되쓰기) 대응이 이 단계의 전제**: `설정_저장` 이 `DEFAULTS` 전체를 되쓰지 않도록 **엔진이 실제로 바꾼 키만 패치**로 쓰게 한다(`hwp-auto-docfit.py` 가 아니라 `formatter_utils.py` 쪽 `설정_저장` 한 함수 — 구현 카드에서 `hwp-auto-docfit.py` 불변 규칙과 충돌하는지 먼저 확인, **미확인**). 그 전까지 `settings_win` 과 엔진 동시 사용은 "작업 중 설정 창 저장 금지"(`JOB_RUNNING` 신호, 설계 B-10)로 막는다.
4. 검증 규칙 단일화: Python `ConfigurationService.save` 의 거부 사례 표(설계 ⑦-5)를 Rust 와 같은 입력·같은 코드로 시험(`scripts/settings_compare.py` `validate_cases` 에 이미 9건 — 늘리기). **설정 창 쪽 규칙(폴더 존재·글꼴·260자 등)이 Rust `validate_patch` 보다 엄격한 점**(SETTINGSWIN3 미확인)은 `docfit_config` 로 올려 한 곳에 둔다.
5. 되돌리기: `settings_backend=python` 한 줄로 `ConfigurationService.save` 복귀(파일 형식이 같으므로 데이터 변환 없음).

### 3단계 — 제거
1. `settings_ui` 기본을 native 로 바꾼 뒤(사용자 실기 확인 후) Flutter `SettingsDialog`·`get_settings`·`save_settings`·`ConfigurationService`·`LABELS`·`fields()`·`settings_schema.EXCLUDED` 제거. **`settings_schema.DEFAULTS` 와 `migrate`(`docfit_core/config_store.py`)는 엔진이 읽으므로 남긴다**(읽기 전용 로더, 부록 C).
2. `application_service.dispatch` 의 설정 관련 분기(W3)와 `desktop_web_ui.py` 의 중복 getter/setter(W4·R4)를 Rust 호출 한 곳으로 합침. 이 단계는 "작업 화면 이식 카드"와 같이 가야 안전한 부분이 있다(설계 ⑤-3 순서 4·5).
3. 되돌리기 스위치(`settings_ui`·`settings_backend`)는 **마지막 릴리스까지 유지**하고, 제거는 별도 카드·사용자 승인 후.

## ④ 되돌리기 스위치 정리

| 스위치 | 값 | 효과 | 상태 |
|---|---|---|---|
| `settings_ui` | `flutter`(기본)·`native` | 설정 여는 화면 | **구현됨**(SETTINGSWIN3) |
| `settings_backend`(신설 예정) | `python`(기본)·`rust` | 설정 창 저장·읽기가 쓰는 구현 | 이번엔 설계만. 환경변수 `DOCFIT_SETTINGS_BACKEND` 로도 덮을 수 있게(앱이 켜지지 않을 때 되돌리기용) |
| `settings_shadow`(신설 예정) | on/off | 1단계 그림자 비교 | 설계만 |

## ⑤ 위험

1. **엔진 되쓰기(W6)로 인한 낡은 값 덮어쓰기** — 이번 카드에서 **직접 겪을 뻔했다**: 목록 키를 `DEFAULTS` 에 넣으면 `기본_설정`(=`DEFAULTS` 복사, `hwp-auto-docfit.py:725-726`)에 들어가 엔진이 `hidden_tabs`·`library_excluded_folders` 도 읽고 되쓰게 된다. 그래서 새 키는 `LIST_DEFAULTS`(골든·`ALL_DEFAULTS` 에만)로 따로 두었다. 이 구조적 약점은 2단계에서 `설정_저장` 을 패치 방식으로 바꿔야 해결된다.
2. **검증 규칙 이중·삼중** — Python `ConfigurationService`, Rust `validate_patch`, 설정 창 `explain`. 목록 키는 Python 이 저장 때 거르지 않고 읽을 때 정리(`clean_hidden_tabs`·`read_exclusions`)하므로 Rust 는 "정리해도 바뀌지 않는 값만 받는다"(이번에 `scripts/settings_compare.py` `list-keys` 로 대조).
3. **외부 쓰기 뒤 앱이 다시 읽는 연결 없음** — 새 창이 저장해도 켜져 있는 앱은 메모리 값(`_hidden_tabs`·`_theme_mode` 등 `desktop_web_ui.py:3330-3390` 의 캐시)을 계속 쓴다. 기본 전환 전에 앱이 설정 파일 변경(수정 시각/revision)을 감지해 캐시를 비우는 연결이 필요(**미구현**).
4. **CLI 호출 비용·실패** — exe 가 없거나 백신이 막으면 설정 창이 안 열림 → 래퍼가 `SETTINGS_CORE_MISSING` 으로 바꾸고 `python` 스위치로 폴백. 시작 시간 **미측정**.
5. **스키마 밖 키**(`external_app_paths`·`ui_thumb_sizes`·`kclip_window`·`stage_choices` 등 서비스 소유)를 Rust 가 지우지 않는지 — 보존 시험이 있으나 **키 전수는 미확인**(설계 B-8). 2단계 전에 "실제 사용자 파일 사본으로 `set` 전후 키 집합 비교"를 한다.
6. **롤백 시 형식 호환** — Rust 가 `serde_json::to_vec_pretty`(들여쓰기 2칸·`ensure_ascii` 아님)로 쓰고 Python 은 `indent=2, ensure_ascii=False` — 한글·키 순서는 같지만 **유니코드 정규화·부동소수 표기 차이는 미확인**(값은 거의 문자열·불리언이라 낮음).

## ⑥ 이번 카드에서 한 것과 하지 않은 것

- 한 것: 목록 키 2개의 표현 확장(`docfit_config`·골든·`settings_win`) — `docs/SETTINGSWIN4_RESULT_2026-11-21.md`.
- 하지 않은 것: `settings_core_client.py`·`settings_backend` 스위치·그림자 비교 구현(이 문서는 설계만), `settings-core` 표준 입력 지원, `설정_저장` 패치화.
