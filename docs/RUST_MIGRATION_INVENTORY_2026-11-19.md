# Rust+Win32 전면 전환 재검토 — 기술 인벤토리·판정표·로드맵 (2026-11-19)

> 사용자 지시(2026-11-19): "파이썬과 flutter를 반드시 사용해야 하는 부분을 제외하면 모든 기능은 Rust+Win32 조합으로 개발하여 최고 실행속도, 최저 리소스 사용 원칙으로 진행. 기존에 구현된 모든 기술을 재검토하여 Rust+Win32 기반으로 전환."
> 작성 근거: 작업 폴더 `E:/docfit-tmp/RUSTALL3`(브랜치 `feature/RUSTALL3`, github/main `2931fa30` 기준)를 **읽기만** 했다. 코드 변경·빌드·시험 실행 없음. 줄 수는 `wc -l`, 설치본 크기는 PowerShell 읽기 전용 측정. 확인하지 못한 것은 **미확인**(번호 U1~U17, 12절 모음)으로 적었다.
> 기존 문서와 겹치는 서술은 링크로 대신한다: [RUSTALL1 계획](RUSTALL1_PLAN_2026-11-13.md)(모듈 217개 분류 A/B/C·UI 화면 S1~S27·Q1~Q17), [TK 잔존 목록](TK_INVENTORY_2026-11-18.md), [DOCK 설계 §3 메뉴 이식 판정](DOCK_DESIGN_2026-11-18.md), [COM 호출 설계](COM_CALLS_DESIGN_2026-11-18.md), [HRUST1](HRUST1_RUST_PRIMARY_2026-11-14.md), [OUTLINER 설계](OUTLINER_DESIGN_2026-11-18.md), [HANDOVER.md](../HANDOVER.md). 요청서가 언급한 `docs/COMX1_LINESEG_2026-11-13.md`는 이 저장소에 **없다**(U1).

## 요약(5줄)
1. 규모: Python 약 8.9만 줄(앱 89,291줄) + Dart 5.7만 줄(`flutter_ui/lib` 57,091줄) + 기존 Rust 약 9.0만 줄(19개 크레이트, 시험 포함). 메뉴 0~15 본체는 전부 Flutter 웹(pywebview+WebView2) 위에 있고 Rust 상주 창(ktm·kagent·kvf·dock)은 별도 exe다. 설치본 `_internal` 393 MB 중 Rust 실행물은 약 24.7 MB(6%).
2. 이득이 큰 3개: ① Python+pywebview+WebView2 호스트 제거(시작 경로 import 894.7 ms 근거 RUSTALL1 START2, 번들의 Python 라이브러리 약 143 MB) ② 이미 Rust 엔진이 있는 메뉴(몽땅검색·파일명·아웃라이너·PDF 편집)의 네이티브 창화 ③ 문서뷰·사진뷰·타임머신 썸네일의 Pillow/pypdfium2 경로 Rust화.
3. 어려운 3개: ① 문서편집 주 화면(서식 엔진 `docfit_engine` 20,794줄과 한/글 COM 핫 루프) ② 표그림판(Dart 8,021줄, 손그림 입력) ③ 다이어그램·마인드맵·붙임사진 편집(React 편집기 3,473줄, WebView2 유지가 현실적).
4. 결론: **Python을 완전히 없앨 수는 없다**(한/글 COM 호출부·검증된 서식 엔진은 당분간 Python 헤드리스 워커). 호스트·화면·검색·변환·썸네일은 Rust로 옮길 수 있고, 한/글 COM 자체의 Rust(IDispatch) 이식은 PoC 전까지 **미확인**.
5. 미확인 사항 17건(12절). 측정 시각에 본 앱(`HWP_AutoDocFit.exe`)이 미실행이라 앱 자체의 메모리·CPU 기준선은 **재지 못했다**(8절).

---

## 1. 현재 기술 지도

### 1.1 층 구조(코드로 확인한 호출 경로)
`Flutter 웹 화면(flutter_ui/lib)` → `bridge_web.dart`의 `docfitCall(method, jsonArgs)`(JS interop, 1~9행) → pywebview `js_api=_BrowserApi(bridge)`(`desktop_web_ui.py:4210`) → `DesktopWebBridge`(`desktop_web_ui.py:113`부터, 메서드 약 320개) → `docfit_app/*_service.py` → `docfit_core/*`(순수 규칙)·`docfit_engine/*`(한/글 COM 서식 엔진)·Rust 프로세스(kfind·klib·ktm·kagent·kvf·outliner·pdf-edit·dock, 명명된 파이프/CLI).
- 창 생성: `webview.create_window`(`desktop_web_ui.py:4205`), `webview.start(..., gui="edgechromium", http_server=True)`(`:4339`). 즉 **Python 프로세스가 WebView2를 호스팅**한다.
- 별도 경로(시제품): `flutter_ui/lib/bridge_native.dart`+`core/ipc/native_client.dart`가 `backend/HWP_AutoDocFit_Backend.exe`를 띄워 토큰 인증 JSON 라인 IPC로 부른다. 서버는 `docfit_app/ipc_server.py`(110줄, `PROTOCOL_VERSION=1`, "no GUI or COM dependencies")·`backend_main.py`(58줄). 문서상 시제품이다(`FLUTTER_NATIVE_FEASIBILITY.md`는 Windows 시험 미실시라고 명시). 설치본(8절)에 `HWP_AutoDocFit_Backend.exe`는 없다.
- Tk 잔존: `hwp-auto-docfit.py`(3,517줄) 부트스트랩·`docfit_ui/legacy.py`(5,825줄)·`abbreviation_dialog.py`·`hwp-auto-docfit-reset.py`(436줄). 상세는 [TK 잔존 목록](TK_INVENTORY_2026-11-18.md).

### 1.2 Python(`wc -l` 실측, 2026-11-19)
| 영역 | 줄 | 역할 | 한/글 COM(`win32com`/`pythoncom` import 확인) | 표준 라이브러리 외 의존(확인) |
|---|---:|---|---|---|
| `docfit_engine/` 18개 | 20,794 | 서식 파이프라인(`page_fit` 5,040·`spacing` 2,297·`formatter_apply` 2,198·`formatter_unify` 1,900·`formatter_titles` 1,851·`pipeline` 1,268…), COM 구동 `com_driver.py` 551 | `com_driver`·`diff_host`·`formatter_analysis/apply/text/utils`·`page_fit`·`pipeline` | pywin32 |
| `docfit_core/` | 36,711 | 규칙·HWPX XML·표 분석/생성·타임머신(`timemachine/` 5,447)·파일명 규칙(`file_rename/` 1,303)·PDF 압축/병합·변환·kordoc 브리지·업데이트 | `com_perf.py`, `hwp_bulk_parallel.py`만 import(`hwp_bulk.py`는 import 없이 한/글 호출, RUSTALL1 1.1 참조) | Pillow(`table_catalog`·`thumbs` 등), pypdf(`pdf_compress`·`pdf_merge`·`pdf_extract`), pypdfium2(`pdf_style_transfer`·`extractors_pdf`·`thumbs`), cryptography(`update_signature`), xlrd(사용처 U2), defusedxml, Node.js+kordoc(`kordoc_bridge.py` 629·`kordoc_runtime.py` 311, 선택 기능) |
| `docfit_app/` | 17,580 | 서비스 계층: `photo_album_service` 1,959·`timemachine_service` 1,444·`file_rename_service` 632·`doc_grid_service` 631·`external_apps_download` 630·`table_units_service` 508·`library_bridge` 489·`application_service` 469·`update_service` 402·`qms_bench_service` 376·`pdf_edit_service` 226·`outliner_service` 145·`dock_runtime` 84 등. Rust 프로세스 클라이언트(`kfind_client` 333·`kfind_process` 260·`kagent_client` 197·`kvf_client`) | `conversion_runner`·`file_rename_service`·`native_file_dialog_service`·`native_table_clipboard`·`table_sketch_service`·`table_units_service` | Pillow·pypdfium2(`doc_grid`·`photo_album`·`pdf_edit_service`·`result_compare`·`sketch_image_input`) |
| `docfit_ui/legacy.py` | 5,862 | Tk 레거시 GUI(제거 대상, RUSTALL1 S23) | — | tkinter |
| 최상위 `*.py` 4개 | 8,299 | `desktop_web_ui.py` 4,346(웹 브리지·창 관리)·`hwp-auto-docfit.py` 3,517(부트스트랩·호환 심볼)·`hwp-auto-docfit-reset.py` 436 | `desktop_web_ui`·`hwp-auto-docfit` | pywebview, tkinterdnd2 |
| `docfit_infra/` | 45 | 프로세스 잠금 | — | — |
| **합계** | **89,291** | (참고: `tests/` 371개 파일 57,538줄, `scripts/`+`tools/` 16,850줄은 제외) | | |

- 외부 패키지(`requirements.txt` 확인): pywin32 312(Windows), tkinterdnd2 0.7.0, Pillow 13.0.0, defusedxml 0.7.1, pywebview 6.2.1, pypdfium2 5.14.0, pypdf 6.21.4, cryptography 50.0.2, xlrd 2.0.1. 설치본에는 numpy 2.3.5(`numpy`+`numpy.libs` 27.9 MB)도 들어 있는데 `requirements.txt`에는 없다 — 사유 U3.
- RUSTALL1 분류 재사용: 한/글 COM 필수부(A) 26파일 26,286줄, Rust 이미 있음(B) 34/8,515, 옮길 것(C) 154/47,183(시점 2026-11-13, 총 82,147줄). 이번 총계는 7천 줄 늘었다(차이 분석 안 함).

### 1.3 Flutter(`flutter_ui/lib`, 총 57,091줄)
| 화면(폴더) | 줄 | 메뉴 번호(탭 셸 `_labels`·`workspace_adapters.dart`) | 부르는 백엔드 API(`callApi` 이름, 확인된 것) |
|---|---:|---|---|
| `main.dart`+`app/`+`workspace/`(셸·주 작업 화면) | 4,143+304+1,514 | 0 문서편집 | `get_state`·`add_paths`·`start_job`·`set_*` 등 `DesktopWebBridge` 직접 메서드 |
| `table_units/` | 4,558 | 1 표템플릿 | `table_units` |
| `table_sketch/` | 8,021 | 2 표그림판 | `table_sketch`(`table_sketch_api.dart`에 호출 지점 25곳) |
| `diagrams/`+`flows/`(+React `diagram_editor/` 3,473) | 691+759 | 3 다이어그램·4 마인드맵·5 붙임사진 편집 — 모두 `FlowEditorScreen(initialMode: diagram/mindmap/paint)` | `flow_editor`·`flow_export`(`flow_service`) |
| `file_rename/` | 1,573 | 6 파일명 일괄변경 | `file_rename` |
| `timemachine/` | 3,083 | 7 몽땅검색 | `timemachine` |
| `doc_grid/` | 615 | 8 문서뷰 | `doc_grid`(호출 8곳) |
| `photo_album/` | 3,119 | 9 사진뷰 | `photo_album`(호출 22곳) |
| `conversion/`+`formats/`+`stages/`+`style_unify/` | 2,304+1,526+405+585 | 10 파일변환(변환 탭)·0의 하위 화면 | `start_single_file_conversion`·`hwp_bulk`·`format_profile` 등 |
| `pdf_edit/` | 1,194 | 11 PDF 편집 | `pdf_edit`(호출 6곳) |
| `settings/`+`abbreviations/`+`tools/`+`external_apps/` | 3,265+504+652+162 | 12 설정, 13 사용 방법(탭 이름) 및 앱 메뉴 | `get_settings`·`save_settings`·`idioms`·`external_app` 등 |
| `outliner/` | 881 | 14 아웃라이너 | `outliner` |
| `qms_bench/` | 966 | 15 검색 벤치마크(개발자 모드에서만, `tab_navigation_shell.dart:503`) | `qms_bench` |
| `library/`+`notebook/`+`editor/` | 5,164+2,546+3,456 | (탭 아님) 개발자 모드 부가 페이지 `WorkspaceExtraPage.library/notebook` | `library`·`notebook`·`library_drop` |
| `kclip/` | 1,774 | (탭 아님) 몽땅클립보드 관리창 | `kclip`·`kclip_window` |
| `progress/`·`preview/`·`core/` | 1,083·255·1,878 | 공용 | — |
- 메뉴 번호의 근거: `tab_navigation_shell.dart:10-20` `kTabPaint=5`·`kTabFileRename=6`·`kTabTimemachine=7`·`kTabDocGrid=8`·`kTabPhotoAlbum=9`·`kTabFileConversion=10`·`kTabPdfEdit=11`·`kTabOutliner=14`·`kTabQmsBench=15`·`kTabCount=16`, 이름은 `_labels`(177~194행).
- **중요한 불일치**: [DOCK 설계 §3](DOCK_DESIGN_2026-11-18.md)은 12=자료함·13=업무자료함으로 적었지만 **코드의 12·13은 '설정'·'사용 방법'**이고, 자료함·업무자료함은 탭이 아니라 개발자 모드 부가 페이지다(`tab_navigation_shell.dart:294-295`, `pages` 목록의 12·13 자리는 `SizedBox()`).
- 붙임사진 편집(5)·다이어그램(3)·마인드맵(4)은 같은 `FlowEditorScreen`이며 웹 빌드에서는 `flow_editor_screen_web.dart`(629줄)가 React 편집기를 끼운다.

### 1.4 Rust 크레이트(`Cargo.toml` 19개, `third_party` 제외, `*.rs` 줄 수 실측·시험 포함)
| 크레이트(경로) | 줄 | 역할 | 산출물 | 설치 번들(8절 실측) |
|---|---:|---|---|---|
| `klib` | 14,987 | 자료함·노트북·파일이름 변경·타임머신 훑기·본문 추출·색인(rusqlite+tantivy)·메모 편집 위임 | `klib.exe` | 예(9.36 MB) |
| `ktm` | 14,426 | 퀵몽땅검색 Win32 창(windows-sys 0.59, GDI·IME·UIA) | `ktm.exe` | 예(2.20 MB) |
| `kagent`(+`ffi` 183) | 11,903(ffi 포함) | 퀵몽땅클립보드·타임머신 감시 상주 | `kagent.exe`, `kclip_ffi.dll` | 예(2.44 MB, 0.38 MB) |
| `kfind/core` | 11,392 | 검색 서비스 코어 | lib | kfind에 링크 |
| `crates/kfind` | 3,823 | 검색 서비스 exe | `kfind.exe` | 예(2.94 MB) |
| `kfind/names` | 4,557 | 파일명 색인 | lib | 미확인 U4 |
| `kfind/annual` | 4,081 | 연간 일정·보고서 규칙 | `annual` exe+lib | 미확인 U4(설치본 exe 목록에 없음) |
| `kfind/engine-bench` | 711 | 엔진 비교 | exe | 개발용 |
| `kvf` | 6,969 | 퀵가상폴더 창·트레이 | `kvf.exe` | 예(0.59 MB) |
| `dock` | 911 | 하단 Dock | `dock.exe` | 예(0.44 MB) |
| `hierarchy` | 2,838 | 위계 분석 | `hierarchy.exe`+`hierarchy.dll` | 예(2.52+2.32 MB) |
| `editor_core` | 4,345 | 메모 편집 코어(std 전용) | lib(klib에 링크) | 해당 없음 |
| `outliner_core` | 1,869 | 아웃라이너 엔진 | `outliner.exe` | 예(0.59 MB) |
| `pdf_edit_core` | 588 | PDF 편집 엔진(lopdf 0.34) | `pdf-edit.exe` | 예(0.89 MB) |
| `qms_bench` | 4,429 | 검색 벤치마크 | `qms_bench.exe` | `.spec`는 묶으려 하나(`HWP_AutoDocFit.spec:22`) **설치본 exe 목록에 없음** U5 |
| `ui_theme` | 765 | 공용 색·테마 | lib | 해당 없음 |
| `bench/kb1`·`scripts/kagent_thumb_bench` | 590·666 | 시험 도구 | exe | 아니오 |
- 합계는 약 89,850줄(kagent 안의 ffi 183줄 이중 계산 보정). `ktm`·`kagent`·`dock`·`klib`은 모두 **`windows-sys 0.59`(저수준 FFI)**를 쓰고 `windows` 크레이트 의존은 Cargo.toml에서 찾지 못했다. 한/글 COM(IDispatch)을 부르는 Rust 코드는 저장소에 **없다**(U6).
- `.spec`에 직접 열거된 것은 pdf-edit·outliner·qms_bench·dock·klib·hierarchy(+dll)·kclip_ffi이고, kfind·ktm·kvf·kagent의 번들 경로는 `.spec`에서 찾지 못했다(설치본에는 있음, U7).

---

## 2. 기능·메뉴별 전환 판정표

### 2.1 범례
- 줄 수는 위 1절 실측 기준. 난이도: S(카드 1~2), M(3~6), L(7~15), XL(15 초과 또는 위험 높음).
- 기대 이득은 **방향 근거**이며 이번에 새로 잰 값이 아니다(8절 참조). 수치는 RUSTALL1 §2.1의 기존 측정 인용.
- 권장 순서는 DOCK §3과 비교해 달라진 곳에 설명을 달았다.

### 2.2 메뉴 0~15 판정표
| # | 메뉴 | 현재 구현(Python/Flutter/Rust) | Python이 반드시 필요한가 | Flutter가 반드시 필요한가 | 난이도 | 의존·선행 | 기대 이득 | 위험 | 권장 순서 |
|---:|---|---|---|---|:---:|---|---|---|---:|
| 0 | 문서편집(주 화면·서식 통일·쪽 맞춤·옵션 카드) | Python 약 4만(`docfit_engine` 20.8k+규칙+브리지) / Flutter 약 6~11k / Rust 거의 0(위계 DLL만) | **부분적으로 예**: 한/글 COM 호출부(`com_driver.py:223` `DispatchEx`, `:284` `RegisterModule`, `:269` `CoInitialize`). 늦은 바인딩 IDispatch라 Rust `windows` 크레이트로 호출은 원리상 가능하나 저장소에 선례 0(U6). 순수 판정부(`page_fit` 등)는 Rust 추출 가능(RUSTALL1 §1.3) | 아니오(체크 카드·목록·진행 보드는 Win32 자체 그리기로 가능) | XL | 한/글 COM 워커 분리, 상태 기계 R9, 결과 HWPX 차이 0 비교 도구 | 주 처리는 COM 83.4%(PERF_K: 322.8초 중 269.1초) → 호스트·화면 교체의 **처리 시간 이득은 최대 16.6%**. 시작·유휴 메모리 이득은 큼 | 사용자 문서 결과 변화 위험 최고, COM 직렬·무응답·PID 정리(`hwp_process.py`) 재현 | 최후(DOCK와 같음) |
| 1 | 표템플릿 | Python 약 6.6k(`table_units` 1,858·`table_catalog` 1,189·`table_builder`·`table_units_service` 508 등) / Flutter 4,558 / Rust 0 | 부분: 스냅샷·복사는 **숨긴 한/글 Open→Copy**(HANDOVER v1.80.5). HWPX XML 조작은 Rust 가능(`klib`에 zip·quick-xml 이미 있음) | 아니오(격자 미리보기는 Direct2D/GDI) | L | HWPX 읽기·쓰기 Rust(RUSTALL1 R3·R4), 스냅샷은 COM 워커 | 목록·미리보기 중심, 상주 메모리 이득 중간 | 병합 칸 span·id 재부여 동등성 | 3 |
| 2 | 표그림판 | Python 약 1.2k(`table_sketch` 521·`recognize` 400·service 325) / **Flutter 8,021** / Rust 0 | 부분: 표 복사에 한/글 COM(`table_sketch_service.py`), 인식은 Pillow(`table_sketch_recognize.py`) → Rust `image` 가능(미시험) | 아니오이나 **캔버스 입력 동등성 위험 가장 큼**(RUSTALL1 S15) | XL | `docfit_ui` 공용 캔버스(Direct2D), UI7 선행 | 편집 중에만 떠서 상주 이득 중간 | 포인터·펜·IME 차이로 사용성 저하 | 6(DOCK와 같음) |
| 3 | 다이어그램 | Python 301(`flow_service`)+React 3,473 / Dart 셸 약 1.4k / Rust 0 | 아니오(export만 Python) | **예**: 자유 도형·자석 연결·층·표 삽입(RUSTALL1 S16 'E'). WebView2는 Rust 호스팅 가능(`webview2-com`, RUSTALL1 UI9) | L(셸), XL(편집기 재작성) | Rust WebView2 호스트 | Flutter 엔진 제거 효과만 | 편집기 동등성 | 7(셸) |
| 4 | 마인드맵 | 3과 같은 편집기 | 아니오 | 예(3과 공유) | L | 3과 같음 | 3과 같음 | 3과 같음 | 7 |
| 5 | 붙임사진 편집(그림판) | 3과 같은 `FlowEditorScreen(initialMode:'paint')`+`photoPasteTrigger`(`workspace_adapters.dart:100`) | 아니오(사진 수신 `request_photo_edit`) | 예(펜·도형·텍스트 상자). 기능 범위는 U8 | L | 3과 같음 | — | — | 7 이후 |
| 6 | 파일명 일괄변경 | Python 약 1.9k(`file_rename_service` 632+`docfit_core/file_rename` 1,303) / Flutter 1,573 / Rust `klib rename` | **거의 아니오**: 규칙·미리보기·실행취소는 순수 로직+Win32 파일 API. 셸 COM 일부(`file_rename_service.py`)는 Win32 셸 API로 가능 | 아니오 | M | `klib rename` 동등성 확인(RUSTALL1 B), 공용 목록·폼 부품 | 앱 기동(START1 cold 6,631 ms) 없이 열림 | 규칙 편집 폼·미리보기 표 동등성 | 2 |
| 7 | 몽땅검색(타임머신) | Python 약 6.9k(`timemachine/` 5,447+service 1,444) / Flutter 3,083 / **Rust ktm 14,426+kfind+klib+kagent** | 썸네일(`thumbs.py` Pillow·pypdfium2)·본문 추출 일부만 Python. 나머지는 Rust 대체물 존재 | 아니오(ktm 창이 이미 존재) | M | 메뉴를 ktm 호출로 연결(DOCK 설계), 썸네일 Rust(RUSTALL1 R6), 색인 경계 재설계(RLEV 반례) | 시작 경로에서 Flutter·Python 제외 | ktm와 Flutter 화면 기능 차이 U9 | 1 |
| 8 | 문서뷰(썸네일 격자) | Python 631(`doc_grid_service`)+Pillow/pypdfium2 / Flutter 615 / Rust 0 | 썸네일이 Python 의존. Rust PDF 렌더 크레이트 채택 여부 U10 | 아니오 | M | Rust 썸네일 엔진(R6) | 이미지 메모리·CPU(미측정) | PDF/HWP 렌더 품질 차이 | 4 |
| 9 | 사진뷰 | Python 1,959(`photo_album_service`: EXIF·dHash·그룹 메타)+Pillow / Flutter 3,119 / Rust 0 | 디코드·EXIF가 Pillow. Rust `image`·EXIF 크레이트로 대체 가능(미시험) | 아니오(큰 사진 격자·시트는 GDI+/D2D) | L | Rust 이미지 부품 | 큰 폴더 처리 CPU(미측정) | HEIC·RAW 등 포맷 차이 U11 | 5 |
| 10 | 파일변환 | Python 약 2.5k(`file_conversion` 575·`hwp_bulk` 685·`conversion_runner` 258·`pdf_*` 약 600·kordoc 940) / Flutter 2,304 / Rust 0 | **예**: HWP→HWPX·HWP→PDF는 한/글 COM(`hwp_bulk.py`, `HANCOM_PDF_DIRECT`), Office 입력·kordoc은 Node 외부 엔진(`kordoc_bridge.py`), pypdf 재압축·병합 | 아니오 | L | COM 워커, PDF 처리 Rust(`lopdf`는 `pdf_edit_core`가 이미 사용) | 변환 시간은 한/글이 지배 → UI 상주 이득만 | 보안 PDF 권한 재잠금 동등성(`PDF_COMPRESSION_DESIGN.md`) | 8 |
| 11 | PDF 편집 | Python 226(`pdf_edit_service`)+pypdfium2 / Flutter 1,194 / **Rust `pdf-edit.exe`(`pdf_edit_core` 588줄, lopdf)** | 렌더는 pypdfium2(Python). 편집 엔진은 이미 Rust(호출 방식 U12) | 아니오(페이지 목록·순서 변경) | M | Rust 렌더(U10)와 같은 부품 | 중간 | 페이지 렌더 일치 | 8 |
| 12 | 설정(탭 이름) | Python 약 0.8k(`configuration_service` 166·`settings_schema` 225·마이그레이션·`update_service` 402) / Flutter 3,265+준말 504 | 아니오(JSON·레지스트리·서명 검증은 Rust 가능. 서명 알고리즘 U13) | 아니오(폼·긴 안내문) | M | 설정 저장소 단일 소유자 결정(DOCK 미확인 항목) | 작음 | 설정 키 호환 | 3~4 |
| 13 | 사용 방법 | Flutter 중심(내용 소스 U14) | 아니오 | 아니오 | S | — | 작음 | — | 3 |
| 14 | 아웃라이너 | Python 145(`outliner_service`) / Flutter 881 / **Rust `outliner.exe`(1,869줄)** | 아니오 | 아니오 | M | OUTLINER2([OUTLINER 설계](OUTLINER_DESIGN_2026-11-18.md)) | 엔진은 이미 Rust, 창만 필요 | 잠금·편집 의미 동등성 | 3 |
| 15 | 검색 벤치마크 | Python 376(`qms_bench_service`) / Flutter 966 / **Rust `qms_bench` 4,429** | 아니오 | 아니오(개발자 모드 전용) | S | 설치본에 `qms_bench.exe` 없음(U5) | 거의 없음 | 낮음 | 마지막(또는 CLI 유지) |
| — | 자료함(개발자 부가) | Python 약 1.2k(`library_*`)+`docfit_core/library` 193 / Flutter 5,164 / Rust klib·kvf | 아니오 | 아니오(RUSTALL1 S10, UI2) | L | kvf 자료 모델 | 큼(매일 쓰는 화면) | 끌어 옮기기·아이콘 보기 | 9 |
| — | 업무자료함(노트북) | Python 369(`notebook_service`)+`notebook_hwpx` 338 / Flutter 2,546+메모 편집 3,456 / Rust klib `notebook`·`editor_core` 4,345 | 아니오 | 메모 편집기(큰 글·서식)는 RichEdit/자체 렌더 결정 필요(RUSTALL1 7.2) | L | UI4 | 중간 | 서식 저장 호환 | 9 |

### 2.3 백그라운드·보조 기능 판정
| 기능 | 현재(근거) | Python 필요? | 난이도 | 전환 방향·이득 |
|---|---|---|:---:|---|
| 업데이트 확인·설치 | `update_service.py` 402·`updater.py` 156·`update_signature.py`(cryptography). GitLab/GitHub `latest.json`, Setup `/SILENT`(HANDOVER v1.80.6) | 아니오(HTTP·해시·서명 검증·프로세스 교체는 Rust 가능). 서명 알고리즘 U13 | M | 작은 Rust 업데이터(런처). GitLab 접속 금지 지시(AGENTS.md)와 현 구현의 엔드포인트 관계는 U15 |
| 설정·마이그레이션 | `settings_schema`·`settings_migrations`·`configuration_service` | 아니오 | M | JSON 스키마 Rust 구현, 소유자 한 곳 |
| 색인 서비스 시작 | `kfind_process.py` 260·`kfind_client.py` 333·`library_kfind_sync.py` 236 | 아니오 | S | 런처/호스트가 kfind를 직접 시동 |
| 쉬는 시간 색인 | `idle_scheduler.py` 335·`idle_gate.py` 454·`*_idle_refresh.py` | 아니오 | M | kagent/kfind 안으로(감시기 이미 상주) |
| 몽땅클립보드 연동 | `kclip_service.py` 426·`kagent_client.py` 197·`kagent_app_service.py` 258 | 아니오 | S | Python 클라이언트 삭제, 관리창은 kagent 안으로(RUSTALL1 UI1) |
| 퀵가상폴더 연동 | `kvf_client.py`·kvf 상주 | 아니오 | S | 동일 |
| Dock 연동 | `dock_runtime.py` 84(파이프 `\\.\pipe\HwpAutoDocFit.dock`) | 아니오 | S | 호스트가 직접 |
| 로그·진단 | `freeze_tracer.py` 257·`desktop_web_ui.py`의 `get_log`/`open_log`·`self_test.py`·`com_perf.py` 322 | `com_perf`는 COM 프록시라 Python 유지 | S~M | 로그 뷰어는 Rust 창, 계측은 워커에 |
| 셸 메뉴(탐색기 우클릭)·배치 | `shell_menu.py`·`shell_batch.py` 397 | 아니오(레지스트리·Win32) | S | Rust CLI |
| 파일 끌어 놓기 | `_bind_file_drop`(`desktop_web_ui.py:4004`)·tkinterdnd2 | tkinterdnd2는 Python 전용 | M | Rust OLE 드롭(호스트에서) |
| 창 관리(항상 위·크기·사이드바 모드) | `_Win32WindowApi`(`desktop_web_ui.py:3728`~, ctypes Win32 호출) | 아니오(이미 Win32 API) | S | Rust 호스트로 그대로 |
| 새 문서·외부 앱 실행 | `new_document_service.py`·`external_apps_*` | 새 문서는 한/글 COM(`_open_blank_hwp_document`) | S | 외부 앱은 Rust, 새 문서는 COM 워커 |

---

## 3. 한/글 COM의 Rust 이식 가능성(조사 범위와 한계)
- 확인된 사실: 한/글 호출은 `HwpFrame.HwpObject`(늦은 바인딩 IDispatch)이고 `DispatchEx`→실패 시 `Dispatch` 순으로 만들며(`com_driver.py:223`), 보안 모듈은 `RegisterModule`로 등록(`:284`, 레지스트리 `HKCU\Software\HNC\HwpAutomation\Modules`, `format_samples.py:24`), 스레드는 `pythoncom.CoInitialize`(`:269`) 즉 STA 직렬이다. 한/글 무응답·PID 정리는 `docfit_core/hwp_process.py`가 한다. 호출 밀도는 `com_perf.py` ComProxy가 속성 읽기까지 센다([COM 호출 설계](COM_CALLS_DESIGN_2026-11-18.md)).
- Rust로 가능한가: IDispatch(`GetIDsOfNames`/`Invoke`)·VARIANT·SAFEARRAY·STA 메시지 펌프는 `windows` 크레이트로 구현 가능(원리). **그러나** `HParameterSet` 중첩 객체(`CreateSet`·`SetItem`)와 한/글의 이벤트 동작은 저장소에 Rust 시험이 전혀 없어 호환·성능을 **확인 못 했다**(U6).
- 이득 평가: COM 시간의 대부분이 한/글 프로세스 내부이므로(COM 83.4%, RUSTALL1 §2.1) 호출자를 Python→Rust로 바꿔 얻는 몫은 호출 1회당 마샬링 비용 수준일 가능성이 크다. 크기는 **측정 필요**(첫 카드 5번 PoC).
- 따라서 로드맵은 COM 호출부를 마지막 단계(Python 워커 유지 → 선택적 Rust PoC)에 둔다.

---

## 4. 전환 로드맵 제안

### 4.1 단계
| 단계 | 내용 | 산출물 | 합격 기준(팀 규칙: 결과 차이 0·느려지지 않음) | 되돌리기 스위치 | 사용자에게 보이는 위험 | 카드 수(추정) |
|---|---|---|---|---|---|---:|
| **R-0 기준선** | 앱 자체(Python+WebView2) 메모리·CPU·시작 시간 재측정(8절 공백 해소), 메뉴별 기능 목록(`docs/ui_parity`) 스크립트 추출 | 측정 스크립트·기준 문서 | 같은 PC 비교(CI 시작 속도 기준은 폐지, 회귀는 같은 PC 비교) | 없음(읽기) | 없음 | 4 |
| **R-1 비-GUI Python 제거** | Python이 하던 Rust 프로세스 시동·클라이언트(`kfind_process`·`kagent_client`·`kvf_client`·`dock_runtime`)·셸 메뉴·업데이트·로그 뷰어를 Rust 런처/호스트로 | `launcher` 확장 또는 신규 Rust 호스트 exe | 기능 표 전부 통과 | `ui.host=python|rust` | 시작·종료 방식 변화 | 8~12 |
| **R-2 Rust WebView2 호스트** | pywebview+`DesktopWebBridge`를 대체하는 Rust 창(WebView2, `docfitCall` 호환 JS 주입). Python은 `ipc_server.py` 헤드리스 워커(이미 존재, 토큰 인증 JSON IPC)로 격하. Flutter 웹은 그대로. 약 320개 브리지 메서드 중 COM 없는 것부터 Rust가 직접 처리 | Rust 호스트+워커 프로토콜 확장 | 메서드별 응답 JSON 차이 0(녹화·재생 비교 도구) | 설정 한 줄로 기존 pywebview 경로 | 창 틀·끌어 놓기·IME 차이 | 15~25 |
| **R-3 네이티브 창 이식(화면별)** | 7→6→14→1→(13·12)→8→9→11→10→자료함·노트북→(2)→0. 각 창은 `docfit_ui` 공용 부품(kagent popup·kvf 기반)과 ktm/Dock 창 라우팅 재사용. 3·4·5는 WebView2(React) 단독 창 유지 | 화면별 Rust 창 | 화면 동등성 표+스크린샷 픽셀 차이 0(승인 후)+UIA 이름+자원 측정 | `ui.<화면>=flutter|rust` 화면별 | 사용자 습관 변화 최대 | 화면당 5~15, 총 70~110 |
| **R-4 엔진 Rust화** | 썸네일·이미지(R6), HWPX XML(R3·R5), 표(R4), PDF 처리, 서식 판정부(A→C 추출) | klib 확장 | 결과 HWPX/PDF 차이 0 | 엔진 스위치(RUSTALL1 4.2) | 결과 문서 변경 위험(전 단계 중 최고) | 30~50 |
| **R-5 COM 호출부 Rust PoC** | Rust IDispatch로 `Open`·`GetText`·`Run` 소수 호출, Python과 호출 시간·결과 비교 | PoC 보고서 | 차이 0이고 호출당 시간이 줄 때만 채택 | Python 워커 유지 | 없음(PoC) | 3~5 |
| **R-6 Flutter·Tk 제거** | 모든 화면 이식 뒤 정식 릴리스 1회(RUSTALL1 Q6·Q11) 거쳐 삭제, `archive/flutter-ui`로 보존 | 번들 축소 | 번들·상주 측정 | 브랜치 되돌리기 | — | 5 |
- 총 카드 대략 **140~210개**(RUSTALL1이 R0~R9+UI0~UI10을 이미 카드화해 중복이 있는 추정이며 확정 아님).

### 4.2 검증 방법(공통)
1. 기능 동등: 화면별 `onPressed/onTap/단축키/메뉴` 전수 표(스크립트 추출), RUSTALL1 §7.5 방식.
2. 결과 동등: 같은 사본 입력으로 Python과 Rust 결과(HWPX XML 정규화 비교·PDF 텍스트층·썸네일 해시)를 비교, 차이 0. 사용자 원본은 건드리지 않고 사본만.
3. 응답 동등(R-2): 브리지 메서드 호출·응답 녹화를 두 호스트에 재생해 JSON diff 0.
4. 자원: 같은 PC에서 시작→첫 그리기 ms, Private WS, 유휴 CPU, exe 크기를 1회씩 측정(반복 회차 없이 1회, 측정 중 시험·빌드 금지, 사전 PC 정리).
5. COM이 닿는 시험은 `C:\HWP_AUTODOCFIT\.com-test.lock` 필수(AGENTS.md).

### 4.3 지금 바로 착수할 수 있는 첫 카드 5개
| # | 카드 | 범위 | 시험 방법 |
|---:|---|---|---|
| 1 | **앱 기준선 측정 스크립트**(R-0) | 설치본 `HWP_AutoDocFit.exe`를 실행해 시작→첫 화면 시간, 유휴 60초 Working Set·Private·CPU, 하위 `msedgewebview2` 합계를 `tools/measure_app_baseline.ps1`(신규)로 1회 기록. 코드 동작 불변 | 1회 측정, PC·Windows 빌드 병기, 측정 전 PC 정리 |
| 2 | **브리지 메서드 호출 표 자동 추출**(R-0) | Dart `callApi('이름', …)`와 `DesktopWebBridge` 메서드 약 320개를 스크립트로 대조해 "Rust 처리 가능/COM 필요" 열을 추가, 이번 수작업 표(1.3·2.2절)를 기계 검증 | 스크립트 결과의 메서드 수가 `grep` 합계와 일치 |
| 3 | **몽땅검색(메뉴 7)→ktm 연결**(DOCK3 1순위) | 7번을 Flutter `TimemachineScreen` 대신 ktm 창 호출로 전환하는 플래그 `ui.timemachine=ktm` 추가(기본 flutter). 기능 차이 표(U9) 작성 | 20회 연속 열기·닫기, 시작 ms 비교, 플래그 off 시 기존과 동일 |
| 4 | **파일명 일괄변경 동등성 도구**(메뉴 6) | `klib rename`과 `docfit_core/file_rename/rules.py`(642줄)의 결과 비교 도구(규칙 전수 입력→새 이름 목록 diff). 화면은 건드리지 않음 | 같은 입력 코퍼스 1,000건 새 이름·충돌 처리 diff 0 |
| 5 | **한/글 COM Rust PoC**(R-5 선행 조사) | `windows` 크레이트로 `HwpFrame.HwpObject` 생성→`RegisterModule`→`Open`→텍스트 읽기→`Quit`만(읽기 전용 사본), Python과 소요시간 비교. **COM 잠금 파일 필수** | 같은 사본에서 텍스트 동일, 호출당 시간 기록, 한/글 프로세스 누수 0 |
- 보조 6번 후보: Rust WebView2 호스트 최소 PoC — `docfitCall`을 받는 창에 `desktop_web` 정적 번들을 띄우고 `get_state` 하나만 Python 워커로 전달.

---

## 5. Python·Flutter를 계속 써야 하는 부분과 최소화 방법
| 항목 | 계속 쓰는 근거 | 최소화 방법 |
|---|---|---|
| 한/글 COM 호출부와 서식 엔진 `docfit_engine/*`(약 2.1만 줄) | ① Rust 선례 0(U6) ② 사용자 문서 결과 동등성이 최우선(팀 규칙 §5) ③ 무응답·보안 모듈·PID 정리 함정이 Python에 집약(`hwp_process.py`, `com_driver.py`) ④ COM 시간 83%는 한/글 몫이라 이득 작음 | Python은 **헤드리스 워커(`ipc_server.py`)로만** 상주, 필요할 때 시작·유휴 시 종료. 순수 판정부는 함수 단위 측정 뒤 Rust로 추출(A→C). R-5 PoC로 판정 |
| 한/글 COM 전용 기능(`hwp_bulk`, 표 스냅샷·복사, 새 문서 열기, 변환의 HWP 경로) | 같은 이유 | 같은 워커. 한/글을 부르지 않는 후반 처리(PDF 병합·압축·썸네일)는 Rust로 이동 |
| Node.js+kordoc(Office·OCR 입력 변환) | Python이 아니라 외부 런타임. 대체 Rust 엔진 없음(U16) | 선택 기능으로 격리 유지, 설치 번들 제외 가능성 검토 |
| 다이어그램·마인드맵·붙임사진 편집(React 3,473줄) | 자유 도형 캔버스, 기존 결론(DOCK §3·RUSTALL1 S16 'E') | Flutter 셸 제거 후 **WebView2 단독 창**(Rust 호스트가 띄움). 편집 중에만 프로세스 존재 |
| 표그림판(Dart 8,021줄) | 손그림 입력 동등성 위험 | 처음엔 Flutter(웹) 단독 창으로 두고 R-3 후반에 Direct2D 재작성 여부를 자원 측정 후 판정 |
| Flutter 웹 번들(`desktop_web` 75.6 MB) | R-2 직후에도 모든 메뉴를 그대로 호스팅해야 하므로 | 화면 이식이 끝나는 대로 메뉴별로 제외, 마지막에 삭제(R-6). `menu_display_mode`(사이드바/Dock) 스위치와 병행 |
| pywin32·pywebview·tkinterdnd2·Pillow·numpy | 위 Python 워커·Flutter 호스트가 쓰는 동안 | 호스트 교체(R-2)로 pywebview·tkinterdnd2·tk 제거(`_tcl_data`·`_tk_data` 등 약 4.3 MB 포함), 이미지 처리 Rust화로 Pillow·numpy 제거 후보(numpy 사용처 U3) |
| `tests/`(57,538줄)·`scripts/`·`tools/` | 비교 기준(Python이 기준 결과를 만든다) | 변경하지 않음(RUSTALL1 Q10) |

---

## 6. 규모 비교(방향 확인용)
| 항목 | 값 | 출처 |
|---|---:|---|
| 앱 Python : Dart : Rust 줄 수 | 89,291 : 57,091 : 약 89,850 | 1절 `wc -l` |
| 설치본 `_internal` 중 Rust 실행물 | 약 24.7 MB / 393.3 MB(6.3%) | 8절 |
| 설치본 `_internal` 중 Flutter 웹(`desktop_web`) | 75.6 MB(19.2%) | 8절 |
| 설치본 `_internal` 중 `resources` | 174.6 MB(44.4%) — Rust 24.7 MB 외 약 150 MB 내역 미확인(U17) | 8절 |
| Python 런타임·라이브러리 등 나머지 | 약 143 MB(`_internal`−`desktop_web`−`resources`, 산술) | 8절 계산 |

---

## 7. 권장 순서의 근거
1. 이미 Rust 엔진·창이 있는 것부터(7·6·14, 이어서 1·11): 위험이 낮고 시작 경로 이득이 크다(RUSTALL1 §7.4 기준 유지).
2. 호스트(R-2)는 화면 이식과 **병행 가능**: Rust 호스트가 있어야 Flutter를 단계적으로 줄일 수 있다(일부 메뉴는 Rust 창, 일부는 WebView2 안).
3. 문서편집 0번은 COM·서식 결과 위험이 가장 커서 마지막. 처리 시간 이득이 작으므로 화면·호스트 이득(상주 메모리·시작)만 보고 판단.
4. RLEV 반례(본문 색인 Rust 4.57초 vs Python 2.99초)처럼 **경계 설계가 틀리면 Rust도 느리다** → 모든 이식 카드 합격 기준에 "느려지면 채택 안 함"을 둔다.

---

## 8. 현재 자원 기준선(이 PC 실측)
측정 시각: 2026-11-19 22:40~22:41(+09:00). 방법: PowerShell `Get-ChildItem -Recurse`(크기·개수), `Get-Process`·`Get-CimInstance Win32_Process`(읽기 전용). 프로세스는 건드리지 않았다.

### 8.1 설치본 `C:\Users\haiju\AppData\Local\Programs\HWP_AutoDocFit\`
| 항목 | 파일 수 | 바이트 |
|---|---:|---:|
| 폴더 전체 | 2,017 | 422,163,579 (약 402.6 MiB) |
| `HWP_AutoDocFit.exe`(PyInstaller 부트로더) | 1 | 11,604,081 |
| `_internal\` | 2,014 | 393,256,955 |
| ├ `desktop_web\`(Flutter 웹 번들) | 88 | 75,583,927 |
| ├ `resources\`(Rust exe·자원 등) | 209 | 174,627,938 |
| ├ `PIL\` | 122 | 28,293,769 |
| ├ `numpy.libs\`+`numpy\` | 2+13 | 20,978,768+6,939,136 |
| ├ `cryptography\` | 112 | 10,386,031 |
| ├ `pypdfium2_raw\`+`pypdfium2\` | 5+22 | 7,538,184+174,172 |
| ├ `pythonwin\`·`win32\`·`win32com\`·`pywin32_system32\` | 2·9·1·4 | 6,692,432·579,584·545,792·1,657,856 |
| └ `_tcl_data\`+`_tk_data\`+`tcl8\`+`tkinterdnd2\` | 832+89+5+133 | 3,162,778+851,433+276,516+3,701,048 |
| `unins000.exe`/`unins000.dat` | 2 | 4,385,192 / 12,917,351 |

Rust 실행물(모두 `_internal\resources\`)

| 파일 | 바이트 |
|---|---:|
| `klib.exe` | 9,363,968 |
| `kfind.exe` | 2,942,464 |
| `hierarchy.exe` / `hierarchy.dll` | 2,516,992 / 2,319,872 |
| `kagent.exe` / `kclip_ffi.dll` | 2,444,800 / 375,808 |
| `ktm.exe` | 2,202,624 |
| `pdf-edit.exe` | 892,416 |
| `kvf.exe` / `outliner.exe` / `dock.exe` | 593,408 / 587,264 / 442,880 |
| **합계(11개)** | **24,682,496** |
- 비교: Python 본체 부트로더 11.6 MB + 라이브러리 약 143 MB + Flutter 웹 75.6 MB 대비 Rust 전체 24.7 MB. `qms_bench.exe`는 설치본에서 찾지 못했다(U5).

### 8.2 실행 중 프로세스(측정 시각 기준)
| 대상 | 상태 | 개수 | Working Set | Private | CPU 시간 |
|---|---|---:|---:|---:|---:|
| `HWP_AutoDocFit.exe`(Python+WebView 본체) | **미실행** | 0 | — | — | — |
| `HWP_AutoDocFit_Backend` | 미실행 | 0 | — | — | — |
| `kfind.exe`(경로 `...\_internal\resources\kfind.exe` 확인) | 실행 | 1 | 44,314,624 B(약 42.3 MiB) | 40,714,240 B | 20.8초 |
| `ktm`·`kagent`·`kvf`·`dock` | 미실행 | 0 | — | — | — |
| `msedgewebview2` | 6개(합계 WS 202,833,920 B, CPU 262.6초) | 6 | — | — | — |
- **해석 주의(중요)**: 확인한 `msedgewebview2`의 명령줄은 `--webview-exe-name=SearchHost.exe`(Windows 검색 UI)로 **본 앱의 하위 프로세스가 아니다**(6개 전부를 확인하지는 않고 1개 명령줄만 읽음, U17과 별개의 한계). 본 앱이 미실행이므로 이 합계를 본 앱 기준선으로 쓰지 않는다. `python` 5개(WS 421 MB)는 다른 프로젝트 가상환경 4개와 이 저장소 `unittest` 1개이며 본 앱이 아니다. `flutter_tester` 9개(각 78~106 MB)도 다른 작업의 시험 프로세스로 보인다. 따라서 **본 앱(Python+WebView2)의 메모리·CPU 기준선은 이번에 측정하지 못했다**(첫 카드 1번의 목적).
- 읽은 값 중 의미 있는 것은 `kfind` 상주 약 40.7 MB private(색인을 올려 둔 상태)이다. "Rust 상주는 작다(kagent 1.8 MB, TODO 기록)"는 기존 설명이 **kfind에는 해당하지 않는다**. 색인 규모·시점에 따라 달라지는 1회 측정값이다.
- 기존 문서 인용(이번 측정 아님): START1 cold window 6,631 ms·warm UI 1,573 ms·splash 115 ms, START2 Python import 894.7 ms(249개 모듈), KAGENT_DESIGN 기존 앱 전체 약 160~220 MB(설계 추정), kagent 개인 메모리 1.8 MB(TODO 기록, 재측정 안 됨).

---

## 9. 전환 후 기대 이득(방향) 요약
| 항목 | 현재 | 전환 후 기대 | 근거 상태 |
|---|---|---|---|
| 시작 경로 | Python import 894.7 ms+창 생성+Flutter 로드(cold 6.6초) | Rust 호스트는 import 제거. WebView2는 화면이 남는 동안 유지 | 추정(Rust 호스트 시작 미측정) |
| 상주 메모리 | 본 앱 160~220 MB 추정(미측정) | Rust 창 상주 수 MB~수십 MB(kagent 1.8 MB, kfind 40.7 MB 실측) | 미측정(R-0에서 확정) |
| 설치 크기 | 402.6 MiB | Python 라이브러리 약 143 MB+Flutter 웹 75.6 MB 제거 시 크게 축소 여지(산술, `resources` 내역 U17에 따라 변동) | 산술 |
| 서식 처리 시간 | COM 83.4% | 최대 16.6%만 개선 여지(PERF_K 인용) | 인용 |
| 색인·검색 | HS2: 집계 SQL이 병목 | 언어 교체 이득 작음, 질의 설계가 핵심 | 인용 |

---

## 10. 위험과 대응(사용자에게 보이는 것)
- 화면 동등성: 위젯 동작·IME·접근성 차이 → 화면별 기능 표·UIA 검사·스크린샷 1회 승인(RUSTALL1 §7.5).
- 호스트 교체: 파일 끌어 놓기·창 틀·DPI·메시지 큐가 달라질 수 있음 → `ui.host` 스위치로 pywebview 경로 유지.
- COM 워커 분리: 한/글 무응답 시 워커 재시작·PID 정리 규칙 재사용(`hwp_process.py`). 한 번에 한 COM 시험.
- 되돌리기 설계: 화면별 `ui.<화면>`, 호스트 `ui.host`, 엔진 스위치, Flutter·Tk 삭제는 정식 릴리스 1회 뒤 보존 브랜치와 함께.
- 병행 규칙: 알파·감마 폐지 → `feature/…` 브랜치에서 시험 통과 후 main, 푸시는 `github`에만(GitLab 금지, AGENTS.md).

## 11. DOCK·RUSTALL1과의 차이(정정 제안)
1. DOCK §3 표의 12·13 행은 코드와 불일치(1.3절).
2. DOCK §8 미확인이던 "PDF 편집 Rust 엔진 접점"은 `pdf_edit_core`(588줄, lopdf)·`pdf-edit.exe`가 존재하고 설치본에 번들됨을 확인(호출 세부는 U12).
3. DOCK는 메뉴 5를 "자유 펜 캔버스로 보임"이라 추정했으나 코드는 `FlowEditorScreen(initialMode:'paint')`로 3·4와 같은 React 편집기를 쓴다(`workspace_adapters.dart:100`). 3·4·5는 한 묶음(WebView2 유지 후보)이다.
4. RUSTALL1의 Python 줄 수(82,147)는 2026-11-13 시점이고 지금은 89,291이다.

## 12. 미확인 사항(17건)
- U1 `docs/COMX1_LINESEG_2026-11-13.md` 파일 없음(요청서가 참조).
- U2 `xlrd` 실제 사용 파일·기능(grep에서 용도 미추적).
- U3 `numpy`가 설치본에 번들된 이유(requirements 밖, 간접 의존 추정).
- U4 `kfind/names`·`kfind/annual`의 설치본 포함 여부(annual exe는 설치본 exe 목록에 없음).
- U5 `qms_bench.exe`가 `.spec`에는 있으나 설치본에 없는 이유(조건부 번들?).
- U6 Rust에서 한/글 IDispatch 호출의 동작·성능(저장소에 선례 0, `windows` 크레이트 미사용).
- U7 `.spec`에서 kfind·ktm·kvf·kagent 번들 경로 확인 실패(설치본에는 존재).
- U8 메뉴 5(붙임사진 편집)의 실제 편집 기능 범위와 사용 빈도.
- U9 ktm 창과 Flutter `TimemachineScreen`의 기능 차이 목록.
- U10 Rust PDF/HWP 썸네일 렌더 엔진 선택과 품질 동등성.
- U11 사진뷰 지원 포맷(HEIC·RAW 등)과 Rust 대체 크레이트의 동등성.
- U12 `pdf_edit_service.py`가 `pdf-edit.exe`를 부르는 방식·기능 범위(CLI/파이프, pypdfium2 렌더와의 분담).
- U13 업데이트 서명(`update_signature.py`)의 알고리즘과 Rust 크레이트 대체 가능 여부.
- U14 '사용 방법'(13) 화면의 내용 소스 파일.
- U15 `update_service.py`의 GitLab 접속 경로와 "GitLab 전면 중단" 지시(AGENTS.md)의 관계.
- U16 kordoc/Node 외부 엔진을 Rust로 대체할 수 있는지(후보 없음).
- U17 `_internal\resources`(174.6 MB) 중 Rust 24.7 MB를 뺀 약 150 MB의 내용(폰트·서식 예시·kordoc 등으로 추정, 미측정).
