# Tk·legacy 잔존 사용처 목록 (2026-11-18)

> 조사만 수행. 코드·버전·README·TODO는 변경하지 않았고 시험도 실행하지 않았다. `git grep`으로 현재 작업 트리를 검색하고, 런타임 진입점 일부를 읽었다. Windows에서 실제 폴백 전환·COM·화면 동작은 확인하지 않았다.

## 범위와 기존 결정

- 구조적 배경, `_tk()` 프리징 원인 및 완화안은 [TK 제거 타당성 검토(2026-11-09)](TK_REMOVAL_REVIEW_2026-11-09.md)를 참조한다.
- Rust 전환 순서와 사용자 확정 Q11(UI6 후 정식 릴리스 1회 뒤 Tk 비상 경로 폐기)은 [RUSTALL1 계획(2026-11-13)](RUSTALL1_PLAN_2026-11-13.md)을 참조한다.
- 아래 분류는 두 문서의 결정을 다시 제안하지 않고, 현 시점 검색 결과의 잔존 위치와 제거 선행조건을 정리한다.

분류: **(a) 제거 가능**은 현 검색 범위에서 기능 이전 후 제거 대상으로 확인됨, **(b) 조건부**는 대체 경로·동등성·호출자 확인이 선행되어야 함, **(c) 유지 필요**는 현재 목적상 유지할 근거가 있음. 확인할 수 없는 동작은 **미확인**으로 표기한다.

## 1. 런타임 tkinter import 및 초기화

| 위치 | 사용 | 분류 | 근거·조건 |
|---|---|---|---|
| `hwp-auto-docfit.py:255,260-265` | `simpledialog`, `tk`, `ttk`, `messagebox`, `filedialog`; `TkinterDnD` | **(b) 조건부** | 메인 부트스트랩이 legacy GUI와 Tk 복구 화면을 초기화한다. UI6 및 정식 릴리스 후 Tk 경로 폐기 전에는 제거 불가. `tkinterdnd2`까지 대체됐는지는 이 조사에서 미확인. |
| `docfit_core/abbreviation_dialog.py:5,7` | 준말 관리용 Tk `Toplevel`·위젯과 파일/메시지 대화상자 | **(b) 조건부** | Flutter 화면으로 옮긴다는 RUSTALL1 C0 분류가 있으나, 현재 호출자 전수·기능 동등성 확인은 미확인. |
| `hwp-auto-docfit-reset.py:17-18,430` | 복구/초기화 GUI와 `tk.Tk()` | **(a) 제거 가능** | RUSTALL1 C0는 Rust 단일 exe 하위 명령 대체를 제안한다. Rust 대체 명령이 준비된 뒤 제거. `--silent` CLI는 별도 경로이며 이 GUI 제거만으로 없애지 않는다. |

## 2. legacy.py 의존

### 런타임 연결 지점

| 위치 | 참조 | 분류 | 근거·조건 |
|---|---|---|---|
| `hwp-auto-docfit.py:1252` | `_engine_resolve('docfit_ui.legacy', '창_맨앞으로', ...)` | **(b) 조건부** | Tk 창 활성화 보조 기능. Tk 메인/폴백 제거와 함께 대체 여부 확인 필요. |
| `hwp-auto-docfit.py:3272,3275,3278` | `색_rgb`, `LiveStageBoard`, `HwpAutoDocFitGUI` 동적 로드 | **(b) 조건부** | GUI 클래스 및 사용자 화면 핵심. UI6 이전 제거 불가, 화면 이전 및 출시 검증 이후 제거 대상. |
| `hwp-auto-docfit.py:3284,3287,3290` | 새 창 전면화, 웹 모드에서 Tk 숨김/복원 | **(b) 조건부** | Tk↔WebView2 전환에 직접 연결. 폴백 종료 경로 교체 후 제거 대상. |
| `docfit_ui/legacy.py:1` | 모듈 자체가 compatibility session 정의 모음이라고 설명 | **(b) 조건부** | 코드 파일의 실사용 참조는 위 동적 로드 지점이다. UI6·출시 조건을 만족한 뒤 파일 제거 가능. |
| `desktop_web_ui.py:1171` | `legacy.py` 변수 둘이 반대값으로 동기화된다는 주석 | **(a) 제거 가능** | 설명 주석. 참조된 옵션 동기화가 다른 경로로 이관되면 주석/호환 흔적 제거 가능. 실제 동기화 구현의 전수 여부는 미확인. |

### 테스트·문서에서의 참조

| 위치 | 성격 | 분류 | 근거·조건 |
|---|---|---|---|
| `tests/test_review_regressions.py:15`, `tests/test_run_signature.py:18` | legacy 모듈 등록 및 AST 구조를 직접 전제 | **(b) 조건부** | 제품 legacy 제거 후 테스트를 새 UI/엔진 계약으로 대체해야 한다. |
| `TODO.md:135` | 완료 전 항목으로 제거 계획 기록 | 조사 범위 밖 | 사용자가 지정한 TODO 금지에 따라 수정하지 않았다. |
| `docs/DEVELOPMENT.md:150`, `docs/FLUTTER_PARITY.md:11,33-58`, `docs/UI_DIALOG_FEATURE_TABLE_2026-11-14.md:112-120` 등 | 구조 설명·기능 대조표 | **(b) 조건부** | 코드 제거 시 설명의 최신성 확인이 필요하다. 이번 문서에서는 반복 서술하지 않는다. |

그 밖의 `docs/`, `reports/`, `scripts/` 검색 결과는 과거 계획·기록·설명이다. 현재 런타임 import/호출이라는 증거로 취급하지 않았다. 제품 동작이 그 문서에만 기록된 경우 업데이트 필요 여부는 미확인이다.

## 3. Tk 폴백 진입점과 숨은 Tk 호출

### 실제 GUI 폴백 분기

| 위치 | 호출/조건 | 분류 | 근거·조건 |
|---|---|---|---|
| `hwp-auto-docfit.py:3440-3449` (Tk 직접 시작 `3442`) | `HWP_AUTODOCFIT_UI=tk` 또는 `webview` 미설치면 Tk UI를 직접 열고 `mainloop()` | **(b) 조건부** | 명시적 복구 폴백. RUSTALL1 Q11에 따라 UI6·정식 릴리스 1회 뒤 폐기 결정. |
| `hwp-auto-docfit.py:3454-3466` (Tk 스레드 초기화 `3456`) | WebView2 경로에서도 백그라운드 Tk root·GUI·mainloop 생성 | **(b) 조건부** | `desktop_web_ui.PendingGui` 및 `_tk()`의 기반. Tk 없는 백엔드/상태·작업 큐로 이관 완료 후 제거. |
| `hwp-auto-docfit.py:3468-3489` | 웹 UI 시작 실패 시 `tk_준비_확인()`, `기존화면_안내`, Tk 창 복원 및 스레드 대기 | **(b) 조건부** | 사용자에게 기존 화면을 제공하는 실제 복구 경로. Rust UI 복구 역할이 준비되기 전 제거 불가. UI6 이후 제거 방향은 확정, 실제 대체 구현은 미확인. |
| `hwp-auto-docfit-reset.py:421-432` (생성 `430`) | `--silent`이 아닌 대화형 실행에서 Tk 창 시작 | **(a) 제거 가능** | Rust 복구 명령/창 대체 이후 제거 가능. |

### `desktop_web_ui.py`의 숨은 Tk 큐 왕복

`desktop_web_ui.py:297`은 `_tk(callback, timeout=120, ...)` 구현이다. 검색된 `_tk(` 행은 아래와 같다. **호출부 전부는 (b) 조건부**이며, UI/서비스 상태를 Tk 객체에서 읽고 쓰는 부분과 실제 Tk 창을 여는 부분이 섞여 있다. 호출별 API 의미를 전수 매핑하지 않아 개별 분류는 미확인이다. Tk 런타임 제거와 함께 큐·콜백을 새 상태/작업 API로 이관한 뒤 정리한다.

```text
desktop_web_ui.py:292,297,317,327,329,378,550,564,567,680,923,938,942-943,
976,986,1026,1085,1106,1129,1151,1204,1218,1264,1379,1417,1426,1495,
1536,1542,1548,1556,1565,1575,1668,1686,1700,1704,1755,1806,1827,1906,
2037,2105,2320,2444,2479,2499,2507,2520,2978,2981,2985,2988,2992,
2996,3014,3039,3044,3054,3068,3076,3124,3140,3149,3169,3222,3262,
3349,3360,3364,3368,3373,3391,3397,3411,3428,3438,3484,3505,3896,
4065,4069
```

위 목록 중 `317,327,329,378`은 `_tk()` 구현/추적 내부이고 `297`은 정의다. 나머지는 호출 위치다. 직접 Tk 창/위젯 동작이 명시된 예는 설정·텍스트 추가·세부 작업·로그 창(`1536-1556`), 결과 파일/비교창(`2996,3076`), 서식 편집/수정(`3349,3368`)이다. 다른 호출은 목록·옵션·작업·서비스 등으로 보이나, 각 콜백 안쪽에서 Tk 위젯을 건드리는지는 미확인이다.

## 4. 비제품 코드의 tkinter 참조

| 위치 | 성격 | 분류 | 근거·조건 |
|---|---|---|---|
| `scripts/tk0_verify_pc.py:39,69,140,145` | PC 검증 전용 Tk 생성 및 `_tk()` 점검 스크립트 | **(a) 제거 가능** | Tk 런타임을 없앤 뒤 검증 스크립트도 새 경로 검증으로 바꾸거나 폐기한다. 이번에는 실행하지 않음. |
| `tests/gui_support.py:4`; `tests/test_abbreviation_dialog.py:9,32`; `tests/test_app_icon.py:4,38`; `tests/test_button_state_guard.py:5`; `tests/test_format_detail_dialog.py:6-7,40,91`; `tests/test_saved_time_dialog.py:4,39`; `tests/test_style_review_dialog.py:4-5,14` | Tk 화면 단위 테스트 보조/생성 | **(b) 조건부** | 해당 Tk UI가 제품에서 제거되면 대체 UI/API 검증으로 전환하거나 불필요한 테스트만 제거한다. 일부는 Windows GUI 환경 의존. |
| `tests/test_desktop_request_lifecycle.py`, `tests/test_desktop_browser_api.py`, `tests/test_tk0_*`, `tests/test_freeze_tracer.py` 등 | `_tk()` 큐·추적 계약 시험 | **(b) 조건부** | 큐/추적 계약이 Tk 없는 백엔드로 이전되는 경우 테스트도 새 계약에 맞춰야 한다. |
| `tests/test_native_ipc.py:73`, `tests/test_pending_gui.py:70`, `tests/test_tk0_direct_reads.py`, `tests/test_v2_t04_t19.py:244`, `tests/test_v2_text_input.py:9` | Tk를 불러오지 않음/방지하는 회귀 검사 | **(c) 유지 필요** | Tk 제거가 완료될 때까지 경계 회귀 방지용. 완료 뒤에도 비-Tk 불변 조건이 유효하면 유지할 수 있다. |

이 외 `git grep` 결과 중 `begin_tk`, 문자열 `without_tk`, 테스트 더미 함수 이름은 Tk import/실제 GUI 초기화와 구분했다. 실제 import 전체 목록은 1절 및 이 절에 기재했다.

## 5. 제거 순서 제안

1. **UI6·화면 동등성·정식 릴리스 조건 충족**: Q11 결정의 선행조건이다. 숨은 Tk가 현재 WebView2 화면의 상태·큐 역할도 하므로 우선 대체한다. 이 문서에서는 해당 조건의 충족 여부를 판정하지 않았다.
2. **숨은 Tk 연결 절단**: `desktop_web_ui.py`의 `_tk()` 큐 왕복, legacy GUI 로드 지점, Tk 초기화·숨김·복원·실패 안내를 Tk 없는 상태/작업 API와 확정된 복구 UI로 치환한다. `desktop_web_ui.py` Tk 전용 직접 창 호출은 각 화면/API 대응을 확인하면서 정리한다.
3. **메인 부트스트랩 Tk 제거**: `hwp-auto-docfit.py:255-265` Tk 관련 import와 `3440-3489` 분기를 제거하고 `tkinterdnd2` 의존·번들 설정도 잔존 여부를 확인한다. 폴백 실패 시 동작은 대체 UI로 검토한다.
4. **legacy 및 준말 창 정리**: `legacy.py` 동적 로드 대상과 테스트 AST/모듈 전제를 제거한다. `abbreviation_dialog.py`는 Flutter 대체 기능과 호출자 확인 전에는 유지한다.
5. **독립 복구 창 및 보조 도구**: `hwp-auto-docfit-reset.py` 대화형 Tk 경로를 Rust 하위 명령 대체 후 제거한다. `scripts/tk0_verify_pc.py`는 대체 검증기로 바꾸거나 폐기한다.
6. **테스트·의존성·문서 정리**: Tk GUI/큐 전용 시험을 대체·삭제하고, 비-Tk 회귀 시험은 의미가 남는 범위에서 유지한다. `tkinterdnd2` 패키지·PyInstaller hidden import/spec·설치 안내·문서를 검색해 불필요한 항목만 정리한다.

## 6. 미확인 항목

- Windows 실기에서 `HWP_AUTODOCFIT_UI=tk`, WebView2 미설치 및 WebView 시작 실패 폴백을 실행하지 않았다.
- Flutter 준말 관리 대체 화면의 기능 동등성, `abbreviation_dialog.py` 호출자 전수, 모든 `_tk()` 콜백의 Tk 위젯 직접 접근 여부는 미확인.
- `tkinterdnd2`가 메인 Tk UI 외 어디서 필수인지, 패키징된 환경에서 Tk DLL/의존성이 어떤 경로로 포함되는지 미확인.
- `legacy.py`를 참조하는 테스트 외부의 동적 플러그인/사용자 스크립트 존재 여부는 저장소 grep만으로는 확인할 수 없다.
