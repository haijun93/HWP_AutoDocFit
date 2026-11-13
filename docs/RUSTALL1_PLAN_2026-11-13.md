# RUSTALL1 — 한/글 COM 필수부 외 전부 Rust 전환 계획 (2026-11-13)

> 사용자 지시(2026-11-13): "한컴 COM 관련 필수불가결한 부분을 제외하곤 최고 실행속도 및 최저 리소스 사용을 원칙으로 rust 기반으로 모든 것을 개발해."
> 작성 근거: 작업 폴더 `G:/docfit-tmp/idx`(브랜치 `feature/idle-indexing`, HEAD `832adfe2`) **읽기만**. 코드 수정·커밋·빌드·시험 실행 없음. 줄 수는 `git ls-files` + `wc -l`, COM 참조 수는 정규식 검색(아래 1.1)으로 센 값이다.
> **이 문서의 속도·메모리 이득은 기존 측정 문서 인용이거나 '측정 필요'다. 이번 작업에서 새로 잰 숫자는 없다.**
> **RUSTALL2 보강(2026-11-13)**: 사용자 추가 지시 "사용자 ui도 필수불가결한 것 아니면, 단축키로 호출되는 몽땅클립보드의 ui와 같이 기술을 사용한다." → **7절 UI** 신설, R9·4.2·5절·Q3·Q11 수정, Q13~Q17 추가. 화면 기술 기준은 `kagent/src/popup.rs`(CreateWindowExW + GDI 이중 버퍼, 자체 그리기)·`kagent/src/frame.rs`·`kvf/src/window.rs`. 줄 수는 같은 HEAD(`eeb59d3c`)에서 `wc -l`로 셌다.

## 0. 먼저 알릴 것
1. **요청 문서 `docs/COM_함수별_기준선_2026-11-12.md`는 저장소(모든 브랜치)·`C:\HWP_AUTODOCFIT\v1803\reports`·`G:/docfit-tmp` 어디에도 없다.** `docs/PERF_K_SPACING_COM_ANALYSIS_2026-11-12.md` 7행도 "`docs/benchmarks/COM_기준선_v2.2.58_2026-11-12.md`는 저장소에 없다"고 적고, 그 문서가 인용한 수치(한 번에 적용 322.8초 / COM 269.1초 / 144,274회)만 남아 있다. 이 계획은 그 인용 수치를 쓴다. 기준선 문서 재작성은 R0 단계 카드에 넣었다.
2. **팀 운영 규칙 §6과 이번 지시가 충돌한다.** 규칙: "Rust 전환은 측정에서 그 모듈이 전체 시간의 큰 몫이고 원인이 계산량일 때만 한다." 이번 지시는 COM 필수부 외 전부 Rust. → Q1. 이 계획은 **새 지시를 따르되, 규칙 §5의 "결과 문서가 변경 전과 다르면 채택하지 않는다"와 "느려지면 채택하지 않는다"를 합격 기준으로 그대로 둔다.**
3. **Rust가 자동으로 빠르지는 않다는 반례가 이미 측정돼 있다.** RLEV(2026-11-12): 300문서 본문 색인 앱 Rust 경로 **4.57초**(klib 4.43초) vs Python 경로 **2.99초**. 원인은 계산이 아니라 문서마다 커밋·fsync와 프로세스 경계였다. 그래서 이 계획은 "함수 하나씩 Rust로 바꾸고 Python이 매번 부르는" 방식을 금지하고, **읽기→처리→저장을 Rust 안에서 덩어리째** 하도록 경계를 정한다(3.2).

## 1. 모듈 목록과 분류

### 1.1 범위(숫자)
| 대상 | 파일 | 줄 | 이 계획에서 |
|---|---:|---:|---|
| 앱 Python(아래 분류 대상) | **217** | **82,147** | 전부 분류 |
| `scripts/`·`tools/` 개발 도구 | 119 | 15,699 | 분류 안 함. Python 유지 제안(Q10) — 비교 도구가 Python 기준을 불러야 하므로 |
| `tests/` 시험 | — | 51,540 | 분류 안 함. 유지(비교의 기준) |
| 기존 Rust(`kagent`+`ffi`, `klib`, `hierarchy`, `editor_core`, `kvf`) | — | 8,697 / 9,307 / 2,521 / 4,345 / 1,154 (시험 포함) | B의 근거 |
| Flutter Dart `flutter_ui/lib` | 105 | 45,654 | **RUSTALL2로 대상이 됨**: 화면은 Rust Win32 네이티브로 옮기고(7절) Flutter는 없앤다. `dart:ffi` 연결 2곳(`core/hierarchy_ffi*`, `features/kclip/kclip_ffi*`)도 함께 없어짐 |
| 웹 화면 `desktop_web/`·`diagram_editor/`(package-lock 제외) | 3 / 18 | 494 / 3,559 | 7절: `desktop_web`(Flutter 번들 없을 때 폴백 HTML) 삭제, `diagram_editor`(React 다이어그램·마인드맵)는 E(유지) |
| Node/kordoc·OCR 런타임 | — | — | 대상 아님(Python 아님). Q8 |

COM 참조 수 = 파일 안에서 `HAction|HParameterSet|hwp.(Run|InitScan|GetText|MovePos|SetPos|GetPos|SaveAs|Open|…)|Dispatch(Ex)(|win32com|pythoncom|.HeadCtrl|CreateSet|.CharShape|.ParaShape` 일치 줄 수. 25개 파일이 1 이상. 이 중 Windows 셸 COM(`WScript.Shell`, `win32com.shell` 파일 대화상자, 클립보드)만 쓰는 3개는 한/글 COM이 아니므로 C로 분류했다. 반대로 COM 참조 0이지만 한/글 COM을 쓰는 `hwp_bulk.py`(HWP→HWPX 일괄 변환), 엔진 연결 파일(`runtime.py`·`context.py`·`formatter.py`·`security.py`)은 A로 넣었다.

### 1.2 분류 요약
| 분류 | 뜻 | 파일 | 줄 |
|---|---|---:|---:|
| **A** | 한/글 COM 필수부 — Python 유지 | 26 | 26,286 |
| **B** | Rust 구현이 이미 있음(Python은 중복·대체 경로·브리지) | 34 | 8,515 |
| **C** | Rust로 옮길 것 | 154 | 47,183 |
| ├ C0 | 옮기지 않고 구조 전환으로 **없앨 것**(Tk·pywebview 브리지) | 7 | 10,482 |
| ├ C1 | HWPX XML 읽기·분석·고쳐 쓰기 | 24 | 9,177 |
| ├ C2 | 표·표그림판·엑셀 표 변환 | 20 | 8,442 |
| ├ C3 | 타임머신 나머지·쉬는 시간 색인 | 11 | 2,942 |
| ├ C4 | PDF·파일 변환·외부 엔진 호출 | 9 | 2,519 |
| ├ C5 | 글 규칙·교정·순수 판정 | 43 | 7,033 |
| └ C6 | 앱 서비스·상태·IPC·설정·업데이트 | 40 | 6,588 |
| X | Rust/Flutter 폴더 안 Python 보조 스크립트 | 3 | 163 |
| 합계 | | **217** | **82,147** |

파일별 전체 목록(줄·COM 참조 수·역할)은 **부록 A**.

### 1.3 A를 Python에 남기는 이유
- 한/글 2020 자동화는 `HWPFrame.HwpObject` IDispatch(늦은 바인딩)다. Rust에서도 `windows` 크레이트로 IDispatch를 부를 수는 있지만 ① 형식 라이브러리 없이 이름→DISPID 조회·VARIANT 변환·`HParameterSet` 중첩 객체를 전부 손으로 짜야 하고 ② 호출 시간의 대부분은 한/글 쪽 처리(조판·이동)라 호출자 언어를 바꿔 얻는 몫이 작고(아래 2.1: COM이 전체의 83%) ③ 한/글 무응답·보안 모듈 등록·PID 강제 종료 같은 실측된 함정 대응이 모두 Python에 있다. 팀 규칙 §6("한/글 COM 호출부는 Python을 유지한다")도 같다.
- 다만 **A 안에는 순수 판정 코드가 섞여 있다**(예: `page_fit.py` 5,040줄 중 COM 참조 419줄, 나머지는 판정·반복). 이 부분은 R8에서 함수별 측정 후 Rust로 뺀다(“A→C 추출”). 측정 전에는 몫을 모른다(측정 필요).

### 1.4 B — 이미 있는 Rust와 Python 짝
| Rust | 하는 일 | Python 짝(B) | 현재 상태 |
|---|---|---|---|
| `klib`(pipe·db·search·extract·index_content·tm_scan·rename·notebook·nb_transform·unify_marker, rusqlite+tantivy) | 자료함·노트북·파일이름·타임머신 훑기·본문 추출(HWPX/HWP/DOCX/TXT/MD)·색인·검색, 메모 편집(`edit_*`→editor_core) | `timemachine/content_search`·`scan`·`store`·`extractors*`, `file_rename/*`, `library/*`, `library_scan/service`, `notebook_*` | 명명된 파이프 별도 프로세스. **본문 색인은 Python보다 느림(RLEV 4.57 vs 2.99초)**, 위계 동등성은 RHIER1에서 합성 묶음 맞춤 |
| `hierarchy`(rlib+cdylib+CLI) | 위계 분석·항목기호 체계·검색 위계 | `marker_scheme.py`, `timemachine/hierarchy_path.py`, `hierarchy_bridge.py` | DLL(ctypes)·CLI, `hierarchy_engine=False` 기본(Python이 실사용) |
| `kagent`(+`ffi`=kclip_ffi cdylib) | 몽땅클립보드·타임머신 감시 상주 | `kclip_service.py`, `kclip_window_state.py`, `kagent_*` | 상주 exe. TODO 기록 실측(재측정 안 됨): exe 0.65MB, 유휴 CPU 0%, 개인 메모리 1.8MB |
| `kvf` | 가상 폴더 창 | `kvf_client.py` | 별도 exe(kagent 소스 일부 공유) |
| `editor_core` | 메모 편집 코어 | (Python 짝 없음, klib 파이프 `edit_*`) | Dart 임시 구현과 경계 |

## 2. C의 이득 추정

### 2.1 근거로 쓸 수 있는 기존 측정
| 출처 | 수치 | 이 계획에 주는 뜻 |
|---|---|---|
| PERF_K(요청 인용 기준선) | 한 번에 적용 **322.8초 중 COM 269.1초(83.4%)**, 144,274회 | 주 처리에서 **COM 밖 Python 몫은 최대 53.7초(16.6%)**. 이것이 Rust 전환으로 줄일 수 있는 상한이며, 이 53.7초에는 pywin32 호출 준비 같은 COM 주변 비용도 섞여 있어 실제 몫은 더 작다(측정 필요) |
| PERF2_I1 | COM 호출 148,211→132,667회로 줄여 전체 **387.1→330.1초(−14.7%)**(1회 측정) | 주 처리 속도는 **언어 교체보다 COM 호출 수 감소**가 같은 크기 이상의 효과. Rust 전환과 별개로 계속해야 함 |
| RLEV | 본문 색인 300문서: Rust 4.57초 vs Python 2.99초 | **반례**: 경계·커밋 설계가 틀리면 Rust가 느림 |
| HS2_HSA | 검색 지연 111ms 중 집계 SQL 약 100ms, Python breadcrumb 100회 0.004초 | 검색은 SQL이 병목. 언어 교체 이득 작음(측정 필요), 질의 설계가 핵심 |
| START2 | Python import 벽시계 중앙값 **894.7ms**(249개 모듈) | Python 백엔드를 화면 경로에서 빼면 이 시간이 시작 경로에서 사라짐(추정, Rust DLL 적재 시간 측정 필요) |
| START1 | cold window 6,631ms, warm UI 1,573ms, splash 115ms | 시작 경로 목표치 비교 기준 |
| KAGENT_DESIGN | 기존 앱 전체 약 160~220MB(설계 추정, 미측정) vs kagent 1.8MB(TODO 실측) | 메모리 이득의 방향 근거. 앱 전체 RSS는 **측정 필요** |
| TK_REMOVAL / TK0 | `_tk()` 96→78곳, 기본 대기 120초 | 프리징의 원인은 계산량이 아니라 **단일 Tk 스레드 줄서기**. C0 제거의 이득은 속도보다 멈춤 제거 |

### 2.2 분류별 이득 추정
| 분류 | 속도 이득 | 리소스 이득 | 근거 |
|---|---|---|---|
| C0 | 멈춤(최대 120초 대기) 제거, 시작 경로에서 import 894.7ms 제거(추정) | Python+Tk+pywebview 프로세스 상주 제거. 크기 측정 필요 | TK0, START2 |
| C1 HWPX XML | 측정 필요. XML 파싱·직렬화는 계산량 위주라 Rust 이득이 큰 종류(일반 경험치 수 배~수십 배, 이 PC 미측정) | 문서 DOM 메모리 감소(측정 필요) | 주 처리 Python 몫 ≤ 53.7초 안의 일부 |
| C2 표 | 측정 필요. 표 늘리기·카탈로그 훑기(`_scan_catalogs`) | 측정 필요 | — |
| C3 타임머신·쉬는 시간 | 썸네일(이미지·PDF 렌더) 측정 필요. 색인은 RLEV 반례 해결이 먼저 | 쉬는 시간 작업을 상주 Python 없이 돌림 → 상주 메모리 감소(측정 필요) | RLEV, IDLE1 |
| C4 PDF·변환 | 측정 필요(pypdf 재압축은 계산량 큼) | 측정 필요 | — |
| C5 글 규칙 | 개별로는 작음(문단 수 × 정규식). 대부분 ms 단위로 추정 — 측정 필요 | 작음 | — |
| C6 앱 서비스 | 대부분 I/O·대기. 속도 이득 작음 | **Python 프로세스를 COM 작업자로만 줄이는 데 필수**. 상주 메모리 감소의 본체 | 구조 목표(5절) |

**정리**: 속도로 큰 이득이 기대되는 곳은 C1·C2·C3(썸네일)·C4(PDF) 계산부다. C5·C6은 속도 이득이 작고 **리소스(상주 Python 제거)** 목적이다. 주 처리(서식 적용 전체) 시간은 COM이 83%라 Rust 전환만으로는 최대 16.6%까지만 줄고, 더 줄이려면 **COM으로 하던 일을 HWPX XML 직접 처리로 옮기는 것**(R5, `batch_format.py`가 이미 "단순 본문은 XML 일괄 적용, 컨트롤은 COM" 방식)이 필요하다. 이것은 결과 문서가 달라질 위험이 가장 큰 변경이므로 별도 승인(Q5)을 받는다.

## 3. 옮기는 순서와 경계

### 3.1 순서(이득·위험·의존)
| 단계 | 내용 | 이득 | 위험 | 의존 |
|---|---|---|---|---|
| **R0 기준선·비교 틀** | COM 함수별 기준선 문서 재작성, Python 자체 시간 함수별 프로파일(COM 시간 분리), 앱 RSS·프로세스 수 측정, 공용 비교 도구 `scripts/rust_parity.py` 틀, 설정 `engine.<모듈>=python|shadow|rust` | 이후 모든 판정의 근거 | 없음(코드 동작 불변) | — |
| **R1 공용 크레이트·상주 DLL** | cargo workspace 하나로 묶고(`docfit_rs` 공용: zip·XML 안전 파서·HWPX 모델·JSON 경계), `klib`·`hierarchy`의 중복 추출·위계 코드 통합, `docfit_rs` 하나를 Python(ctypes로 `docfit_rs.dll` 1회 적재)과 Rust 화면 exe(크레이트로 직접 링크, 7절. Flutter가 남아 있는 동안만 dart:ffi)가 함께 쓰게 함 | 프로세스 기동·직렬화 반복 제거(RLEV 원인 해소의 바탕) | 빌드 체계 변경(RHIER1의 target 혼선 경험) | R0 |
| **R2 B 마무리(색인·검색)** | RLEV 반례 해결: 문서 묶음을 DLL 한 호출·한 트랜잭션으로 색인, 위계는 hierarchy 공용 함수. Rust가 Python 이하 시간 + 결과 차이 0이면 Python 중복(`content_search` 색인부·`scan`·`extractors*`) 제거 | 색인 시간(측정 필요), Python 상주 시간 감소 | 검색 결과 순위 변화 | R1, Q4 |
| **R3 C1 HWPX 읽기·분석(읽기 전용)** | `hwpx/__init__`(안전 열기·검사·Markdown), `style_inventory`, `format_elements.analyze_format_elements`, `doc_inspector`, `fidelity/inventory·source_map·verify` | 계산량 큰 분석 | 낮음(읽기 전용, 문서 안 바뀜) | R1 |
| **R4 C2 표(분석→생성)** | `table_catalog`(훑기·내보내기), `table_units`(판정·늘리기·내보내기), `table_builder/style/width/arrows/sketch*`, 엑셀 표 | 측정 필요 | 중간(생성 HWPX 바이트) | R3 |
| **R5 C1 HWPX 고쳐 쓰기** | `xml_hanging_indent.apply_xml_hanging_indent`, `batch_format.apply_batch`, `report_header.apply_header`, `fidelity/patch_hwpx`, `hwpx_idioms`, `abbreviation_doc` | 주 처리 Python 몫 감소 | **높음(결과 문서)** — `format_fidelity_pc.py` 불일치 0 필수 | R3 |
| **R6 C3·C4** | 타임머신 썸네일(이미지·PDF)·series·related·basedate·rules·EML/PDF 추출, 쉬는 시간 스케줄러(`idle_gate/scheduler`), PDF 병합·압축·서식 전달, kordoc 호출 래퍼 | 썸네일·PDF 계산 | PDF 보안·권한값 처리(Q7) | R1 |
| **R7 C5 글 규칙** | `pasted_text`, `writing_aids`, `emphasis`, `continuation`, `blank_lines`, `document_rules/review`, `korean_proofread`, `number_check`, `abbreviations` 등 43개 | 작음 | 중간(문자열 규칙 수백 개, 정규식 의미 차이) | R1 |
| **R8 A→C 추출** | A 파일 안 순수 판정 함수(예: `page_fit_search`, `formatter_analysis`의 판정부)를 함수별 측정 순으로 Rust로, COM 호출은 묶어서 한 번에 넘김 | 주 처리 Python 몫(≤53.7초) 중 측정된 것 | 높음 | R0 프로파일, R5 |
| **R9 C6·C0 구조 전환** | 앱 서비스·상태·작업 수명(`application_service`·`jobs`·설정·업데이트·외부 앱)을 Rust로, **Rust Win32 네이티브 화면(7절, 몽땅클립보드와 같은 기술)이 같은 프로세스에서 Rust 코어를 직접 호출**, Python은 COM 작업자만. `desktop_web_ui.py`·`docfit_ui/*`·Tk·`desktop_web`·Flutter 삭제. 화면 이식은 7.4의 UI 단계(UI1~UI3은 R1 직후 먼저, 주 작업 화면 UI6이 R9와 함께) | 상주 Python·Flutter 엔진·WebView2(상시) 제거, 프리징 구조 제거 | **가장 큼**(화면 전체 경로) | R2~R7, UI0~UI5, Q2·Q3·Q13~Q17 |

순서 이유: R0 없이는 "차이 0"과 "빨라짐"을 판정할 수 없다. R1은 RLEV에서 드러난 경계 비용을 먼저 없애는 바탕이다. R2는 Rust가 이미 있는데 느린 곳이라 첫 실전 검증으로 적합하다. R3(읽기 전용)를 R5(쓰기)보다 앞에 둔 것은 같은 HWPX 모델을 위험 없이 먼저 굳히기 위해서다. R9는 나머지가 다 Rust로 간 뒤에야 Python 백엔드를 COM 작업자로 줄일 수 있어 마지막이다.

### 3.2 덩어리째 Rust 안에서 처리하는 경계(읽기→처리→저장)
- **원칙**: Python·Flutter는 **경로와 옵션만** 넘기고, Rust가 ZIP 열기→XML 파싱→처리→직렬화→원자적 저장(임시 파일 후 교체)까지 한 호출로 한다. 문서 내용(XML 문자열)을 경계 너머로 왕복시키지 않는다.
- **호출 단위 예**
  | 호출 | 입력 | 출력 | 대체하는 Python |
  |---|---|---|---|
  | `hwpx_inspect(path, opts)` | 경로 | 검사 결과 JSON | `hwpx.validate_and_inspect_hwpx` |
  | `hwpx_style_inventory(path)` | 경로 | 스타일 목록 JSON | `style_inventory.analyze_style_inventory` |
  | `hwpx_apply(in, out, ops)` | 경로 2 + 작업 목록(내어쓰기·일괄 서식·머리·패치) | 바뀐 항목 수·경고 JSON | `apply_xml_hanging_indent`, `apply_batch`, `apply_header`, `apply_patch` 연속 호출 |
  | `table_export(catalog, id, edits, out)` | 경로·편집 | 저장 결과 | `table_catalog.export_template`, `table_units.export_resized` |
  | `index_batch(paths[], db)` | 경로 묶음 | 건수·실패 | Python 색인 루프(RLEV: 문서별 커밋 금지, 묶음 1트랜잭션) |
- **COM과 섞이는 곳**: 작업자(Python)가 한/글로 `SaveAs HWPX` → Rust `hwpx_apply` → 한/글로 다시 열기(조판이 필요한 단계만). 이 왕복이 이득인지는 R5에서 문서별로 잰다(열기·저장 비용이 COM 수천 회보다 싼 경우에만).
- **DLL 상주 호출**: `docfit_rs.dll`은 프로세스당 한 번 적재해 계속 둔다. 무거운 상태(글꼴 표·카탈로그 색인·tantivy 리더)는 DLL 안 핸들(`open → handle`, `close`)로 유지해 호출마다 다시 만들지 않는다. 경계는 C ABI `fn(*const c_char json) -> *mut c_char json` + `free` 한 쌍(이미 `hierarchy_analyze_json`/`hierarchy_json_free`로 검증된 모양). 모든 진입점은 `catch_unwind`로 패닉을 오류 JSON으로 바꾼다.
- **스레드·리소스**: 기본 단일 스레드 + 필요할 때만 상한 있는 작업 스레드(코어 수가 아니라 고정 2 등, Q9). 쉬는 시간 작업은 IDLE1의 `IdleJob` 계약(중단 가능·진행 상태 파일)을 Rust 쪽에서 그대로 지킨다.
- **외부 입력 격리**: 사용자 PDF·HWP(OLE) 파서처럼 깨진 입력으로 프로세스가 죽을 위험이 있는 것은 DLL이 아니라 klib처럼 **별도 작은 프로세스**에서 돈다(Q2).
- **XML 보안**: Python의 `defusedxml` 규칙과 같은 정책을 Rust 파서 한 곳(`docfit_rs::xml`)에서 강제 — DOCTYPE·외부 엔티티 거부, ZIP 항목 수·크기·압축비 상한(`hwpx._check_archive_limits` 값 그대로). klib `safe_member_name`·`parse_xml_tree`를 공용으로 올린다(Q12).

## 4. 단계별 비교 도구(Python과 결과 차이 0)와 되돌리기

### 4.1 공용 방식
- **세 모드 설정** `engine.<모듈> = python | shadow | rust`. `shadow`는 둘 다 실행해 **Python 결과를 쓰고** 차이를 `reports/parity/<모듈>.jsonl`에 남긴다. 차이 0 누적 + 측정상 느려지지 않음 → `rust`로 바꾼다.
- **비교 도구** `scripts/rust_parity.py <모듈> --corpus <폴더>`: 같은 입력을 Python 함수와 Rust DLL에 넣어 비교. 비교 단위:
  | 종류 | 비교 방법 |
  |---|---|
  | 분석 결과(JSON) | 키 정렬 후 완전 일치. 실수는 HWPUNIT 정수로 비교(소수 허용 오차 없음) |
  | HWPX 출력 | ① ZIP 항목 목록·순서 ② 각 XML을 정규화(속성 순서·공백) 후 바이트 일치 ③ 정규화 없이 원바이트 일치는 별도 표기(한/글이 순서를 따지는지 미확인 → 원바이트 일치를 목표로) |
  | 서식 결과 문서 | 기존 `scripts/format_fidelity_pc.py` **불일치 0**(릴리스 합격 기준과 동일) |
  | 색인·검색 | 문단별 위계·본문 일치(`scripts/rhier1_diff.py` 방식), 검색 상위 N개 순서 일치 |
  | 썸네일·PDF | 픽셀 차이 0(같은 렌더러일 때) 또는 쪽 수·글자층 대조(`pdf_compress` 기존 검증 방식) |
- **자료 묶음**: 시험 예시 `C:\HWP_AUTODOCFIT\테스트예시`(저장소 밖, 사본으로만), 기존 `tests/` 고정 자료, 항목기호체계 9종. 사용자 원본은 쓰지 않는다.
- **속도 판정**: 측정 1회·다른 작업 없을 때(팀 규칙 §5). 같은 시각에 Python/Rust 번갈아 재고 중앙값 비교(START2 방식).

### 4.2 되돌리기
| 수준 | 방법 |
|---|---|
| 사용자 PC 즉시 | 설정 `engine.<모듈>=python`(Python 코드가 남아 있는 기간 동안). DLL 적재 실패 시 자동으로 Python(현재 `hierarchy_bridge` 동작과 같음) |
| 코드 | 카드 = 커밋 1~3개 단위, `git revert`. 기존 시험은 고치지 않음(팀 규칙 §3) |
| Python 삭제 | `rust` 기본으로 정식 릴리스 1회(Q6) 동안 문제 없을 때만 삭제 카드 실행. 삭제 전 커밋을 태그 없이 브랜치(`archive/py-<모듈>`)로 보존 |
| R9(구조 전환) | 구 경로(pywebview+Flutter 웹+Python 백엔드) 실행 파일을 같은 설치본 안에 한 릴리스 동안 유지하고 런처에서 선택(Q3) |
| UI 화면별 | 설정 `ui.<화면>=flutter|rust`(Flutter가 남아 있는 동안). 단축키·트레이·탭이 이 값을 보고 창을 고름. Rust 창 시작 실패(창 등록·DLL 적재 오류) 시 자동으로 Flutter 경로(7.5) |

## 5. 최종 경계 모습

```
┌──────────── docfit.exe — Rust Win32 네이티브 화면(7절, 몽땅클립보드와 같은 기술) ────────────┐
│  같은 프로세스 ──► docfit_rs (HWPX·표·색인·검색·글 규칙·설정·작업 상태 기계·쉬는 시간)         │
│  [E] 다이어그램·마인드맵 편집기만 열 때 WebView2 자식 창(React diagram_editor)을 띄우고 닫으면 해제 │
│                    │  명명된 파이프(JSON 작업 봉투, 기존 job_envelope 형식)                  │
└────────────────────┼─────────────────────────────────────────────────────────────────────────┘
                     ▼ 필요할 때만 시작, 유휴 시 종료(Q9)
        docfit_com_worker (pythonw, A 26개 파일만: COM 연결·서식 적용·쪽 맞춤·자간·변환)
                     │  pywin32 IDispatch
                     ▼
                한/글 2020 (Hwp.exe)

  별도 상주/단독 exe(이미 Rust): kagent.exe(몽땅클립보드·감시) · kvf.exe(가상 폴더) · klib(외부 입력 격리 프로세스)
```
- **화면 ↔ Rust 코어**: 같은 Rust 프로세스 안 함수 호출(FFI·직렬화 없음). 무거운 일은 작업 스레드에서 돌고 결과는 `PostMessageW`로 화면 스레드에 돌려준다(화면 스레드는 기다리지 않음 — TK0의 줄서기 멈춤 구조가 생기지 않음). 지금의 Flutter 웹 + WebView2 + pywebview + 숨은 Tk 경로는 없어진다(RUSTALL2: Flutter Windows 네이티브 단계도 거치지 않는다).
- **Rust ↔ Python**: Python은 **COM 작업자 하나**. 받는 명령은 "이 문서에 이 단계들을 실행"뿐이고, 결과·진행은 이벤트로 돌려준다(TK 검토 2단계의 "COM 전용 STA 스레드 + 작업 큐"를 프로세스로 분리한 모양). 작업자는 상태를 갖지 않는다 — 파일 목록·옵션·진행 상태 기계는 Rust가 갖고 상태 파일로도 남긴다(팀 규칙 §0의 "읽을 수 있는 상태").
- **작업자 안에서 Rust 쓰기**: 작업자도 같은 `docfit_rs.dll`을 ctypes로 적재해 R5·R8의 HWPX 처리·판정을 부른다(작업자 ↔ Rust는 프로세스 경계 없음).
- **작업자 크기 목표**: A 26,286줄에서 R8 추출 후 줄어든 만큼. import 목록은 `pythoncom`·`win32com`·엔진만(측정 필요). Tk·pywebview·pypdf·Pillow 등은 작업자에서 빠진다.
- **무응답 대책**: 작업자가 응답 없으면 Rust 쪽이 작업자 프로세스와 그 작업자가 띄운 Hwp.exe PID만 종료(`hwp_process.py`의 PID 차집합 방식 유지). 화면은 멈추지 않는다.

## 6. 카드(함수 단위)

카드 머리 형식: `[워커 · 모델 · effort] 과제 (이유)`(팀 규칙 §1). 공통 완료 조건: Rust 단위 시험 + `scripts/rust_parity.py` 차이 0 + 기존 Python 시험 통과 + 1회 측정에서 느려지지 않음. 공통 금지: 기존 시험 수정, 사용자 원본 사용, GitLab 접근, 결과 문서 변경.

### R0 기준선·틀 (코드 동작 불변)
| 카드 | 담당 제안 | 대상 | 완료 조건 |
|---|---|---|---|
| R0-1 | [Codex haijun93 · 기본 · 낮음] 측정 도구 | `docs/COM_함수별_기준선_2026-11-13.md` 재작성: `com_perf` 기록으로 단계별 COM 호출 수·시간 + **Python 자체 시간**(cProfile에서 COM 호출 제외) 함수 상위 50 | 한/글 잠금 잡고 1회, 시연 문서 3종 |
| R0-2 | [Codex haijun93 · 기본 · 낮음] 측정 도구 | 앱 RSS·프로세스 수·시작 시간(현재 구조) 측정 스크립트 | 숫자 표 1장 |
| R0-3 | [Codex ngaytot9 · 상위 · 낮음] 범위 분명 | `scripts/rust_parity.py` 틀(JSON·HWPX 정규화 비교, 결과 jsonl) + 설정 `engine.*` 키(기본 python, 동작 불변) | 자기 시험 통과 |

### R1 공용 크레이트·DLL
| 카드 | 담당 제안 | 대상 | 완료 조건 |
|---|---|---|---|
| R1-1 | [Claude ngaytot9 · 낮음] 다중 파일 구조 | cargo workspace 구성(kagent·klib·hierarchy·editor_core·kvf 빌드 결과 바이트 기능 동일), target 하나 | 기존 Rust 시험 전부 통과, 기존 exe 동작 동일 |
| R1-2 | [Claude ngaytot9 · 낮음] 설계 | `docfit_rs::xml`·`zip` 안전 정책(=`hwpx.open_zip_safely`·`_check_archive_limits`·`_safe_member_name`) | Python 거부 사례 전부 Rust도 거부 |
| R1-3 | [Codex ngaytot9 · 상위 · 낮음] | `docfit_rs.dll` C ABI(json in/out·free·catch_unwind·핸들) + Python 로더(`docfit_app/rs_bridge.py`, 실패 시 Python) | 패닉 주입 시험에서 프로세스 생존 |

### R2 색인·검색(B 마무리)
| 카드 | 담당 제안 | 대상 함수 | 비교 |
|---|---|---|---|
| R2-1 | [Claude ngaytot9 · 낮음] 최적화 | klib `index_content`: 묶음 1트랜잭션·위계 공용 함수 → DLL `index_batch` | RLEV 300문서: Rust ≤ 2.99초 목표, 문단별 위계 차이 0 |
| R2-2 | [Codex ngaytot9 · 상위 · 낮음] | `extractors_ooxml`의 xlsx·pptx(현재 Rust 없음) → `extract.rs` | 문단 텍스트 일치 |
| R2-3 | [Codex haijun93 · 기본 · 낮음] 시험 | `content_search.search` vs klib `search` 상위 N 순서 대조 묶음 | 순서 일치 또는 차이 목록(결정은 Q4) |
| R2-4 | [Codex ngaytot9 · 상위 · 낮음] | 동등 확인 후 Python 중복 경로를 `engine.index=rust` 기본으로(삭제는 Q6 이후) | shadow 차이 0 |

### R3 HWPX 읽기·분석
| 카드 | 대상 함수 | 비교 |
|---|---|---|
| R3-1 | `hwpx.validate_hwpx`·`inspect_hwpx`·`to_markdown`·`compare_documents` | JSON·Markdown 일치 |
| R3-2 | `style_inventory.parse_header`·`analyze_style_inventory`·`inventory_markdown` | JSON 일치 |
| R3-3 | `style_inventory.build_style_sample`(HWPX 생성) | HWPX 정규화·원바이트 |
| R3-4 | `format_elements.char_values`·`para_values`·`border_fill_values`·`table_values`·`hanging_rule` | JSON 일치 |
| R3-5 | `format_elements.analyze_format_elements`·`general_table_agreement`·`head_block_tables` | JSON 일치 |
| R3-6 | `doc_inspector`, `fidelity/inventory`·`source_map`·`verify` | JSON 일치 |
| R3-7 | `style_hierarchy`·`style_learning`·`format_samples`·`release_format_audit` | JSON 일치 |
(R3 담당 제안: R3-1·R3-4·R3-5는 [Claude ngaytot9 · 낮음](HWPX 모델 설계 포함), 나머지 [Codex ngaytot9 · 상위 · 낮음])

### R4 표
| 카드 | 대상 함수 | 비교 |
|---|---|---|
| R4-1 | `table_catalog._scan_catalogs`·`list_catalog`·`_table_preview` | JSON 일치 |
| R4-2 | `table_catalog.export_template`(+`_apply_cell_edits`·`_apply_structure_edits`·`_apply_style_edits`) | HWPX 원바이트 |
| R4-3 | `table_units.detect_plan`·`plan_for`·`compute_catalog_signatures`·`describe_units` | JSON 일치 |
| R4-4 | `table_units.resize_table`·`export_resized`·`generate_org_chart*`·`_fill_weekly` | HWPX 원바이트 |
| R4-5 | `table_builder`·`table_style`·`table_width`·`table_arrows` | HWPX 원바이트 |
| R4-6 | `table_sketch`·`table_sketch_recognize`·`hwp_table_import` | 격자·HWPX 일치 |
| R4-7 | `excel_table*`·`excel_paste_target`·`excel_html_shading`·`cell_image_bg`·`table_import`·`text_table` | 결과 일치 |

### R5 HWPX 고쳐 쓰기(결과 문서 위험 높음)
| 카드 | 대상 함수 | 비교 |
|---|---|---|
| R5-1 | `xml_hanging_indent` 계산부(`char_width_factor`·`prefix_width_hwpunit`·`hanging_indent_pair`·`safe_reference_hanging_indent_pair`) | 값 일치(정수) |
| R5-2 | `xml_hanging_indent.apply_xml_hanging_indent`·`build_hwpx_from_text_file` | HWPX 원바이트 + fidelity 불일치 0 |
| R5-3 | `batch_format.apply_batch` | 동일 |
| R5-4 | `report_header.analyze`·`build_spec`·`apply_header` | 동일 |
| R5-5 | `fidelity/patch_hwpx.apply_patch`·`fidelity/package` | 동일 |
| R5-6 | `hwpx_idioms`·`abbreviation_doc`·`hwp_idioms`(HWP.IDO 직접 쓰기, 백업 유지) | 파일 원바이트 |
| R5-7 | 연속 호출을 `hwpx_apply(in,out,ops)` 한 번으로 묶기 | 단계별 결과와 원바이트 일치 |

### R6~R9 (앞 단계 결과를 본 뒤 상세화)
| 카드 묶음 | 대상(함수 단위로 나눌 목록) |
|---|---|
| R6-1~4 | `thumbs.render/generate`(이미지·ZIP·PDF), `series`·`related`·`basedate`·`rules`, `extractors_eml/pdf`, `idle_gate`·`idle_scheduler`(IdleJob 계약 유지) |
| R6-5~8 | `pdf_merge`, `pdf_compress`(Q7), `pdf_style_transfer`, `kordoc_bridge/runtime`(Node 호출 래퍼만), `file_conversion` 규칙 |
| R7-1~10 | `pasted_text.clean_pasted_text/outline_pasted_text`, `writing_aids`, `emphasis`, `continuation`, `blank_lines`, `document_rules/review`, `korean_proofread`, `number_check`, `abbreviations`, `colon_labels`·`labeled_text`·`marker_spacing`·`marker_prefix`·`roman_heading` 등 — 파일당 1카드, 입력 문단 묶음 대조 |
| R8-1~ | R0-1 프로파일 상위 순: `page_fit_search.search_level`·`classify_reports`, `formatter_analysis` 판정부, `spacing` 후보 계산부 … (COM 호출 순서·횟수 불변을 `com_perf` 기록으로 대조) |
| R9-1~ | 상태 기계(작업·색인·업데이트·한/글 연결)를 Rust로(`application_service`·`jobs`·`job_envelope`·`app_state`·`stage_selection`·`progress_guide`), 설정(`settings_*`·`configuration_service`·`config_store`), 업데이트(`update_service`·`updater`·`update_signature`), 외부 앱·셸 메뉴·파일 대화상자(Windows 셸 COM은 Rust `windows` 크레이트), COM 작업자 분리, **화면은 7절 UI 카드(UI0~UI10)로 Rust Win32 네이티브 전환**, C0·Flutter 삭제 |

## 7. UI — 몽땅클립보드와 같은 Rust Win32 네이티브로

> 지시(2026-11-13): "사용자 ui도 필수불가결한 것 아니면, 단축키로 호출되는 몽땅클립보드의 ui와 같이 기술을 사용한다."
> 기준 기술 = `kagent/src/popup.rs`(1,507줄): `RegisterClassExW`+`CreateWindowExW`, `WM_PAINT`에서 `CreateCompatibleDC/Bitmap`→그리기→`BitBlt`(GDI 이중 버퍼), `CreateFontW` 직접 생성, 자체 그린 머리 단추·분류 탭·목록, `WM_CHAR`+`WM_IME_COMPOSITION` 직접 입력, `TrackPopupMenu` 메뉴, `frame.rs`(187줄, `WM_NCCALCSIZE` 틀 없애기·DWM 테두리·둥근 모서리·6px 크기 조절), Per-Monitor v2 DPI(`main.rs:1016`). `kvf/src/window.rs`(178줄)도 같은 계열에 "Model은 event의 step 한 곳에서만 갱신" 구조. 빌드: `windows-sys 0.59`, `opt-level=3·lto·panic=abort·strip`. 실측(TODO 기록, 재측정 안 됨): kagent exe 0.65MB, 유휴 CPU 0%, 개인 메모리 1.8MB.
> **이번 작업에서 앱을 실행하거나 화면을 캡처하지 않았다.** 위젯 종류는 Dart 소스 검색(`CustomPaint`·`TextField(`·`ReorderableListView`·`Draggable`·`InteractiveViewer`·`Image.memory`·`TabBar`·`DataTable`·`Semantics`·`webview`·`HtmlElementView`)으로 셌다.

### 7.1 화면 목록(전수)
분류: **N** = Rust Win32 네이티브로 옮김, **N0** = 이미 Rust Win32 네이티브(기준), **E** = 그 기술로만 가능해 유지, **D** = 옮기지 않고 없앰(같은 일을 하는 N 화면이 대신함).

| # | 화면 | 파일(줄) | 줄 | 주요 위젯·그리기 | 분류 |
|---|---|---|---:|---|---|
| S1 | 주 작업 화면(상단 탭 셸·단계 보드·파일 목록·카드 옵션) | `main.dart`(4,127), `features/workspace/*`(944), `features/stages/stage_board.dart`(398), `app/*`(232) | 5,701 | TabBar 3, 파일 끌어 놓기, 체크·라디오 카드, 결과 그림(`Image.file/memory`), 대화상자 다수 | N |
| S2 | 진행·작업 로그 | `features/progress/live_progress.dart` | 957 | CustomPaint 4(진행 그림), 로그 목록·복사 | N |
| S3 | 옵션 그림 미리보기 | `features/preview/option_images.dart` | 225 | InteractiveViewer(확대·이동) | N |
| S4 | 문서 변환(드롭·포맷 선택·HWP 일괄·병합 순서) | `features/conversion/*` 5파일 | 2,028 | 놓기 영역, 떠 있는 포맷 선택 창, ReorderableListView(끌어 순서 바꾸기) | N |
| S5 | 서식 관리(프로필 편집·여백 배치·요소 미리보기·표 서식) | `features/formats/*` 5파일 | 1,430 | 설정 폼, 쪽 여백 배치 그림, DataTable 1 | N |
| S6 | 설정(설정 대화상자·안내·관용구·업데이트·가상 폴더·키오스크 테마·문서 미리보기) | `features/settings/*` 7파일 | 3,108 | 탭형 설정 폼, 긴 안내 글, 관용구 표·편집, 업데이트 진행 | N |
| S7 | 준말 등록·관리 | `features/abbreviations/*` | 480 | 검색 목록, 입력 칸, SelectableText | N |
| S8 | 서식 통일 대화상자 3종(영역·스타일 고르기·결과) | `features/style_unify/*` | 530 | 목록·라디오·결과 표 | N |
| S9 | 파일 이름 바꾸기 | `features/file_rename/*`(api 166 포함) | 1,573 | 미리보기 표(현재 이름→새 이름), 규칙 편집 폼 | N |
| S10 | 업무자료함(탐색기 보기·검색·미리보기) | `features/library/*` 8파일(api 1,186 포함) | 4,967 | 아이콘·목록 보기, Draggable 3(끌어 옮기기), CustomPaint 2, 즉시 검색 | N |
| S11 | 노트북 | `features/notebook/*`(api 712 포함) | 2,399 | TabBar 2, 목록, SelectableText | N |
| S12 | 타임머신(목록·썸네일·관련 문서·쉬는 시간 색인 줄) | `features/timemachine/*` 5파일 | 2,770 | 가상 목록, 썸네일(`Image.memory`), 관련 문서 대화상자, 상태 줄 | N |
| S13 | 메모 편집기(큰 글 보기·찾기·Markdown 서식·저장) | `features/editor/*` 10파일 | 3,421 | 여러 줄 편집(TextEditingController 확장), 큰 글 가상 보기(`large_text_view`), 찾기 | N(근거 7.2) |
| S14 | 표 단위(표 격자 미리보기·순서) | `features/table_units/*` | 4,306 | CustomPaint 격자 미리보기, 칸 그림(`Image.memory` 3) | N(Direct2D) |
| S15 | 표그림판(손으로 그린 표 → 표) | `features/table_sketch/*` 5파일 | 7,180 | **그리기 캔버스**(CustomPaint, `table_sketch_painter` 680), 선택·끌기(`table_sketch_selection` 1,350), 인식 결과 | N(Direct2D, 근거 7.2) |
| S16 | 다이어그램·마인드맵 편집기 | Dart 셸 `features/diagrams/*`(585) + `diagram_editor/`(React, 3,559, 시험 6개 파일 포함) | 4,144 | WebView2(`webview_flutter_windows`) 안 자유 도형·자석 연결·층·마인드맵 자동 배치·도형 안 표·칠하기 | **E**(편집기 본체) — Dart 셸 585줄은 Rust 창 셸로(N) |
| S17 | 흐름 편집기 웹판 | `features/flows/*` 6파일 | 661 | `HtmlElementView`(웹 빌드 전용, S16과 같은 편집기) | D(S16로 흡수, `flow_draft`·`edit_save_state` 87줄 모델만 Rust로) |
| S18 | 몽땅클립보드 관리창 | `features/kclip/*` 8파일(ffi 86 포함) | 1,436 | 분류 설정, 보관 설정 드롭다운, 그림 미리보기(`Image.memory` 2) | N(kagent 안으로) |
| S19 | 도구·백엔드 대화상자 | `features/tools/*` | 375 | 단추 목록, 상태 글 | N |
| S20 | 외부 앱 실행 | `features/external_apps/*` | 162 | 메뉴·단추 | N |
| S21 | (화면 아님) Flutter 연결층 | `bridge*.dart`(104), `core/*`(1,256: ipc·api·hierarchy_ffi·drop_events·close_guards·instant_search·models) | 1,360 | — | D(같은 프로세스 Rust 호출로 대체. `instant_search`·`close_guards` 규칙은 Rust로) |
| S22 | 폴백 HTML 화면 | `desktop_web/index.html·app.js·app.css` | 494 | 단계 탭, 카드 옵션 체크 | D |
| S23 | Tk 레거시 GUI(로그·설정·비교·대화상자 `Toplevel` 14곳) | `docfit_ui/legacy.py` | 5,813 | Tk 위젯 | D(Q11) |
| S24 | Tk 준말 대화상자 | `docfit_core/abbreviation_dialog.py` | 195 | Tk | D(S7과 중복) |
| S25 | 초기화 도구 | `hwp-auto-docfit-reset.py` | 436 | Tk 확인 창·체크 | N(작은 창 1개) |
| S26 | 몽땅클립보드 빠른 목록(단축키) | `kagent/src/popup.rs`·`frame.rs`·`settings_menu.rs` | 2,107 | 자체 그린 머리 단추·분류 탭·가상 목록·검색 입력(IME)·메뉴 | N0(기준) |
| S27 | 가상 폴더 창 | `kvf/src/window.rs`(+`vf_state`·`vf_data`·`tray`) | 489 | 자체 그린 탭·목록·IME 입력·트레이 메뉴 | N0 |

**개수**: N **19개 화면**(S1~S15, S18~S20, S25; Dart 43,048줄 + Python Tk 436줄), N0 2개, **E 1개**(S16 편집기 본체, JS 3,559줄), D 4개 화면(S17·S22·S23·S24) + 연결층(S21). 검산: Dart N 43,048 + S16 셸 585 + S17 661 + S21 1,360 = 45,654(1.1과 같음).

### 7.2 E 판정 근거(최소화)
| 화면 | 판정 | 근거 |
|---|---|---|
| S16 다이어그램·마인드맵 편집기 | **E** | 자유 배치 벡터 편집기다: 도형 끌기·크기 손잡이·회전, 자석 연결선(`magnet.js`), 층(`layers.js`), 마인드맵 자동 배치(`mindmap.js`), 도형 안 표(`table.js` 479줄), 칠하기, 도형 안 글 편집(IME 포함), 확대·이동, 되돌리기. 이미 React로 있고 시험 6개 파일(583줄)이 있다. Win32로 다시 짜면 사실상 새 벡터 편집기(도형 안 IME 편집·선택 손잡이·연결선 재배치)를 만드는 일이라 이득(열 때만 쓰는 화면) 대비 위험이 가장 크다. **유지 방법**: Rust 창 안에 WebView2를 **열 때만** 자식으로 만들고 닫으면 해제(Q13). 상시 WebView2·Flutter는 없앰 |
| S15 표그림판 | N(E 아님) | 캔버스이지만 대상이 "격자로 인식될 직선·칸"이라 도형 종류가 적다. Direct2D 직선·사각형·선택 영역·확대만으로 된다. 위험이 커서 이식 순서를 뒤(UI8)로 두고, 동등성 시험(7.5)에서 손그림 입력 재생 결과가 같지 않으면 그 단계에서 E로 돌릴지 다시 묻는다(Q13) |
| S13 메모 편집기 | N(E 아님) | 여러 줄 편집·IME·실행 취소·클립보드·접근성은 Windows 내장 **RichEdit(`msftedit.dll`, MSFTEDIT_CLASS)** 자식 컨트롤이 준다. 큰 글 보기는 이미 Rust인 `editor_core`(4,345줄)와 가상 그리기로. Markdown 서식(굵게·제목)은 RichEdit 서식 범위로 표시 |
| S14 표 격자 미리보기, S3 확대 미리보기, S2 진행 그림 | N | 격자·그림 확대·막대 그리기는 Direct2D(또는 GDI)로 충분 |
| S4 끌어 순서 바꾸기, S10 끌어 옮기기 | N | 자체 목록에서 마우스 끌기 + 탐색기에서 파일 놓기는 OLE `IDropTarget`(`RegisterDragDrop`) |

### 7.3 공용 Rust UI 부품(`docfit_ui` 크레이트, kagent·kvf에서 추출)
| 부품 | 출처(이미 있는 코드) | 새로 할 일 |
|---|---|---|
| 창·틀 | `frame.rs`(`style_frame`·`frame_hit`·`own_border`·DWM 둥근 모서리·테두리 색), popup 캡션 단추 46px·Segoe MDL2 글리프·`WM_MOUSEACTIVATE` 비활성 클릭 | 일반 창(작업 표시줄 있음)·도구 창·모달 대화상자(소유 창 비활성화 + 자체 메시지 루프) 세 모양 |
| DPI | `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)`, `GetDpiForWindow`, `frame::scale` | **`WM_DPICHANGED` 처리 없음**(popup·kvf 모두) → 제안 사각형 적용·글꼴/비트맵 다시 만들기 추가. 100/125/150/200% 시험 |
| 테마 | popup `ACCENT`·`HEAD_BG` 상수(BGR) | `Theme` 구조체 하나(밝음·키오스크(`kiosk_theme.dart` 86줄 값 그대로)·고대비=`SystemParametersInfo(SPI_GETHIGHCONTRAST)`면 `GetSysColor`). 다크는 Flutter에도 없음 → Q16 |
| 글꼴 | popup `CreateFontW(-scale(px),…)` 매 그리기 생성 | (글꼴명·px·굵기·DPI) 캐시, 맑은 고딕/Segoe UI, 아이콘 글리프 글꼴. 말줄임 `DT_END_ELLIPSIS`, 너비 `frame::text_width` |
| 그리기 | popup `PaintGuard` 이중 버퍼(`CreateCompatibleDC/Bitmap`→`BitBlt`), `WM_ERASEBKGND`=1 | `Canvas` 한 겹(GDI 구현 기본, Direct2D 구현은 캔버스 창만). 무효 영역만 다시 그리기 |
| 목록(가상) | popup `row_index`·`VISIBLE`·`ROW_H`·고정/삭제 칸 `row_action`, kvf `top`·`move_selection` | 보이는 줄만 그리기(수만 줄), 다중 선택(Shift/Ctrl), 아이콘 보기(S10), 오른쪽 클릭 메뉴 |
| 스크롤 | popup·kvf는 **휠만**, 스크롤 막대 없음 | 자체 그린 얇은 스크롤 막대(끌기·쪽 단위 클릭), 부드러운 휠(`WHEEL_DELTA` 누적) |
| 한 줄 입력(IME) | popup·kvf `WM_CHAR`+`WM_IME_COMPOSITION`(`GCS_COMPSTR`) 조합 중 글자 표시, kvf 시험(`WM_CHAR '글'`) | 커서 이동·선택·복사/붙여넣기·실행 취소 추가, 조합 창 위치 `ImmSetCompositionWindow`. 검색칸 전용 |
| 여러 줄·폼 입력 | 없음 | **OS `EDIT`·RichEdit 자식 컨트롤**(IME·실행 취소·접근성 내장), `WM_CTLCOLOREDIT`로 테마 색(Q14) |
| 단추·체크·라디오·탭 | popup 머리 단추(`head_button*`)·분류 탭(`tab_layout`: 긴 이름→짧은 이름→같은 폭 말줄임) | 공용 `Button`·`Check`·`Radio`·`Tabs`(마우스 올림·눌림·포커스 고리·사용 안 함 상태) |
| 메뉴 | `TrackPopupMenu(TPM_RETURNCMD)`, `settings_menu.rs`(순수 모델 Entry→명령) | 그대로 공용화 |
| 설정 폼 | `settings_menu.rs`의 "순수 모델 → 그리기 분리" 방식 | `Form` 선언(항목 종류: 체크·선택·숫자·경로·글) → 자동 배치·키보드 이동. S5·S6·S18이 같은 부품 |
| 표 | 없음 | 머리줄·열 너비 끌기·정렬·가상 줄·칸 안 편집(EDIT 자식 띄움). S5·S8·S9·S6 관용구 |
| 미리보기 | 없음(kclip은 Flutter `Image.memory`) | WIC로 PNG·JPG 디코드 → HBITMAP(GDI)/ID2D1Bitmap, 맞춤·확대·끌어 이동(S3·S12·S14) |
| 끌어 놓기 | 없음 | OLE `IDropTarget`(탐색기 파일 놓기), 창 안 끌기는 자체 |
| 접근성 | **popup·kvf 모두 없음**(`WM_GETOBJECT` 0곳) | 자체 그린 요소마다 UIA 제공자(`WM_GETOBJECT`→`UiaReturnRawElementProvider`, 이름·역할·상태·선택·호출), 자식 컨트롤은 OS 기본. 최소 기준 = 현재 Flutter `Semantics` 22곳 + 모든 단추·목록 항목·탭 이름(Q17) |
| 키보드 | popup ←→·Tab 분류 이동, 1~9·↑↓·Enter·Esc, Ctrl+F | 공용 포커스 순서(Tab/Shift+Tab), Alt 접근 키, 기본 단추(Enter)·취소(Esc) |
| 상태·스레드 | kvf `Model`·`Event`·`Effect`(step 한 곳에서만 갱신), `snapshot()` 시험 | 화면마다 같은 구조. 오래 걸리는 일은 작업 스레드 → `PostMessageW(WM_APP+n)`로 결과. 화면 스레드에서 막는 호출 금지(TK0 재발 방지) |

**GDI 대 Direct2D/DirectWrite 선택 기준**
| 기준 | GDI(기본) | Direct2D + DirectWrite |
|---|---|---|
| 쓰는 곳 | 글·목록·폼·탭·메뉴·표(S1~S13, S18~S20, S25 대부분) | 매끈한 선·확대 변환·많은 도형·큰 그림 축소가 있는 캔버스: S15 표그림판, S14 격자 미리보기(확대 시), 필요하면 S3 |
| 속도 | 글·사각형 위주면 충분(popup이 근거). 확대·안티앨리어싱 선은 느리고 거칢 | GPU 가속, 변환·안티앨리어싱 기본. 장치 만들기 첫 지연 있음(측정 필요) |
| 메모리 | 추가 DLL 없음(gdi32는 이미 적재). kagent 개인 메모리 1.8MB(실측) | `d2d1.dll`·`dwrite.dll`·(가속 시) 그래픽 드라이버 적재로 작업 집합 증가(측정 필요) |
| exe 크기 | `windows-sys` Gdi 기능만. kagent 0.65MB(실측) | `windows` 크레이트 D2D·DWrite 기능 추가로 증가(측정 필요) |
| 글자 모양 | GDI ClearType. popup과 같은 모양 | DirectWrite 렌더링은 같은 글꼴이라도 픽셀이 다름 → **한 창 안에서 섞지 않음** |
| 규칙 | 기본값 | 창 단위로만 선택, 해당 창에서 GDI 대비 측정(첫 그리기 ms·개인 메모리·exe 크기) 수치를 카드에 남기고 채택(Q15) |

### 7.4 옮기는 순서(사용 빈도·이득·위험)와 카드
첫 이식은 **자료가 이미 Rust에 있고(Python 없이 동작), 단축키·트레이로 자주 부르며, 지금은 Flutter 앱 기동(START1 cold 6,631ms)을 거쳐야 뜨는 화면**부터 한다.

| 단계 | 화면 | 이유(빈도·이득) | 위험 | 의존 |
|---|---|---|---|---|
| **UI0** 공용 부품 추출 | `docfit_ui` 크레이트(7.3) — kagent popup·kvf가 이것을 쓰도록 바꿈 + `WM_DPICHANGED`·스크롤 막대·UIA 기본 | 이후 모든 화면의 바탕 | 낮음(popup·kvf 스크린샷·`snapshot()` 동일이 조건) | R1-1(workspace) |
| **UI1** S18 몽땅클립보드 관리창 | kagent 안 창으로. 같은 `Settings`·`settings_menu` 모델을 이미 씀 | 단축키 기능의 짝, Flutter 관리창 기동 제거 | 낮음 | UI0 |
| **UI2** S10 업무자료함 | kvf와 같은 자료(`vf_data`)·klib 검색. 매일 쓰는 화면 | 앱 기동 없이 자료함 열기 | 중간(끌어 옮기기·아이콘 보기) | UI0, R2 |
| **UI3** S12 타임머신 | kagent `timeline.rs`가 이미 timeline.db를 씀. 썸네일만 R6 | 상주 kagent에서 바로 열기 | 중간(썸네일) | UI0, R6 썸네일 |
| UI4 | S11 노트북, S13 메모 편집기 | `editor_core`·klib `notebook` 이미 Rust | 중간(RichEdit 서식) | UI0 |
| UI5 | S6 설정, S7 준말, S9 파일 이름, S19 도구, S20 외부 앱, S25 초기화 | 폼·표 부품 검증, 대부분 단순 | 낮음 | UI0, R9 설정부 |
| UI6 | S1 주 작업 화면, S2 진행, S3 미리보기, S4 변환, S5 서식 관리, S8 서식 통일 | 가장 많이 쓰지만 COM 작업자·상태 기계(R9)가 있어야 함 | **높음**(작업 흐름 전체) | R9 |
| UI7 | S14 표 단위 | Direct2D 첫 적용 | 중간 | UI0, R4 |
| UI8 | S15 표그림판 | 가장 큰 Dart 화면(7,180) | **높음**(손그림 인식 입력) | UI7, R4-6 |
| UI9 | S16 다이어그램 셸(E 본체는 WebView2 그대로) | Flutter 의존 끊기 | 중간(WebView2 Rust 호스팅 `webview2-com`) | UI0 |
| UI10 | D 삭제: Flutter 전체(S17·S21 포함), `desktop_web`, Tk(S23·S24) | 상주·번들 크기 감소 | — | UI1~UI9 + 정식 릴리스 1회(Q6·Q11) |

카드 머리 제안: UI0 [Claude ngaytot9 · 낮음] 설계·다중 파일, UI1·UI5 [Codex ngaytot9 · 상위 · 낮음], UI2·UI3·UI4 [Claude ngaytot9 · 낮음], UI6·UI8 [Claude ngaytot9 · 높음](위험 큼), UI7·UI9 [Codex ngaytot9 · 상위 · 낮음], 측정 카드 [Codex haijun93 · 기본 · 낮음].

### 7.5 화면별 동등성 시험과 되돌리기
| 시험 | 방법 | 합격 |
|---|---|---|
| 기능 표 | 화면마다 `docs/ui_parity/<화면>.md`: Dart 소스의 `onPressed`·`onTap`·단축키·메뉴·끌기·대화상자를 전수로 뽑아(스크립트) 한 줄씩 "Flutter 동작 / Rust 동작 / 확인 방법 / 결과" | 모든 줄 통과. 빠진 기능은 사용자 확인 없이 빼지 않음 |
| 상태 시험 | kvf `Model`·`Event`·`snapshot()` 방식: 창 없이 Event 순서 → 상태 비교. 창 시험은 kvf처럼 `SendMessageW(WM_CHAR·WM_IME_*·WM_KEYDOWN)` | Rust 시험 통과 |
| 스크린샷 비교 | 고정 자료·고정 상태로 `PrintWindow` 캡처, DPI 100/150/200% × 밝음/키오스크/고대비 | ① Flutter와는 픽셀 같을 수 없음 → 나란히 보기 한 장으로 배치·글자 잘림·빠진 요소를 사용자가 1회 승인 ② 승인 후 Rust 회귀는 **픽셀 차이 0** |
| 접근성 | UIA 트리 자동 검사(모든 단추·탭·목록 항목에 이름·역할), 내레이터로 주요 흐름 1회 | 빈 이름 0 |
| 성능(1회 측정, 팀 규칙 §5) | 단축키/클릭 → 첫 그리기 ms, 개인 메모리, 유휴 CPU, exe 크기 | Flutter 경로보다 느리거나 무거우면 채택 안 함 |
| 되돌리기 | 설정 `ui.<화면>=flutter|rust`(Flutter 남아 있는 동안), Rust 창 시작 실패 시 자동 Flutter. 코드는 카드 단위 `git revert`. Flutter·Tk 삭제는 정식 릴리스 1회 후, 삭제 전 커밋을 `archive/flutter-ui` 브랜치로 보존 | — |

## 8. 결정 필요(Q)
| Q | 질문 | 선택지 | 권장(답 없으면 이쪽) |
|---|---|---|---|
| Q1 | 팀 규칙 §6("측정상 큰 몫·계산량일 때만 Rust")을 이번 지시로 대체하는가 | 대체 / 유지 | **대체**, 단 "결과 차이 0·느려지지 않음"은 합격 기준으로 유지. 규칙 문서 갱신 필요 |
| Q2 | Rust 코어를 화면 프로세스 안에 둘지, 별도 Rust 호스트 프로세스로 둘지 | 같은 프로세스 / 별도 프로세스 / 혼합 | **혼합**: 기본은 화면 exe 안(최저 리소스, RUSTALL2로 화면도 Rust라 DLL 경계도 없음), 깨진 외부 입력 파서(PDF·HWP OLE)만 별도 프로세스. 작업자(Python)는 `docfit_rs.dll`을 ctypes로 |
| Q3 | **(RUSTALL2 재작성)** 화면을 Flutter 웹+WebView2에서 Rust Win32 네이티브(몽땅클립보드와 같은 기술)로 바꾸는 방식·시점 | ① 화면별 점진(UI1부터, `ui.<화면>` 설정으로 Flutter와 병행) ② R9에서 한 번에 ③ Flutter Windows 네이티브를 거쳐서 | **①**: UI0 공용 부품 → UI1~UI3(자료가 이미 Rust인 몽땅클립보드 관리창·업무자료함·타임머신)은 R1 직후 시작(UI2 검색은 R2, UI3 썸네일은 R6 이후 완성 — 그 전에는 썸네일 없이 목록만), 주 작업 화면(UI6)은 R9와 함께. 화면마다 한 릴리스 병행 후 Flutter 경로 삭제. ③은 지시와 맞지 않아 권하지 않음 |
| Q4 | 본문 색인·검색을 Rust로 바꿀 때 순위가 달라지면 | Python과 같게 맞춤 / 차이 허용 | **같게 맞춤**(RLEV 결정 (다) 대기와 함께) |
| Q5 | COM으로 하던 서식 일을 HWPX XML 직접 처리로 옮기는 것(R5 이후 확장)을 허용하는가 | 허용(fidelity 불일치 0 조건) / 안 함 | **허용**, 단계별 사용자 확인 |
| Q6 | Python 중복 코드를 언제 지우나 | rust 기본 후 즉시 / 정식 릴리스 1회 후 / 계속 보존 | **정식 릴리스 1회 후** |
| Q7 | `pdf_compress`(pypdf, 보안 PDF 권한값 재잠금)를 Rust로 옮기는가 | 옮김 / Python 유지 | **후순위**, 동등 Rust 라이브러리(lopdf 등) 권한 처리 확인 후 |
| Q8 | Node/kordoc·OCR 런타임도 대상인가 | 포함 / 제외 | **제외**(Python 아님). 별도 과제 |
| Q9 | COM 작업자 수명과 Rust 작업 스레드 상한 | 상주 / 필요할 때+유휴 종료 · 스레드 1/2/코어 수 | **필요할 때 시작 + 유휴 5분 종료**, 스레드 상한 2. 첫 작업 지연은 측정 후 보고 |
| Q10 | `scripts/`·`tools/`(119파일 15,699줄)·시험도 Rust로? | 옮김 / 유지 | **유지**(비교 기준) |
| Q11 | **(RUSTALL2 재작성)** Tk 비상 경로(TK 검토 결정 ②, `legacy.py` 5,813줄·`Toplevel` 14곳)와 Tk 준말 대화상자·Tk 초기화 도구를 언제 없애나 | UI6 후 정식 릴리스 1회 뒤 폐기 / 지금 폐기 / 유지 | **UI6(주 작업 화면 Rust 이식) 후 정식 릴리스 1회 뒤 폐기**. 비상 경로 역할(웹·Flutter가 안 뜰 때)은 Python·WebView2·Flutter에 기대지 않는 Rust 창 자체가 맡는다. 초기화 도구(S25)는 Rust 작은 창으로(UI5) |
| Q12 | XML 보안 규칙: `defusedxml` 대신 Rust 파서 정책으로 GitLab·GitHub 보안 검사를 통과시키는 방법 | Semgrep 규칙 추가 / 문서화만 | 보안 검사 설정 확인 필요(모름). GitLab은 건드리지 않음 |
| Q13 | (UI) E 범위: 다이어그램·마인드맵 편집기(React `diagram_editor`)만 WebView2로 남기는가, 표그림판도 E로 두는가 | 다이어그램만 E / 표그림판도 E / E 없음(전부 Win32) | **다이어그램만 E**. Rust 창이 열 때만 WebView2를 만들고 닫으면 해제(상시 WebView2 없음). 표그림판은 N(Direct2D)으로 하되 UI8 동등성 시험 실패 시 다시 묻기 |
| Q14 | (UI) 입력 칸: popup처럼 자체 그리기만 쓸지, OS `EDIT`·RichEdit 자식 컨트롤을 섞을지 | 전부 자체 / 혼합 / 전부 OS 컨트롤 | **혼합**: 한 줄 검색칸은 자체(popup·kvf에서 IME 검증됨), 여러 줄·폼 입력·메모 편집기는 OS `EDIT`/RichEdit(IME·실행 취소·클립보드·접근성 내장, 테마 색은 `WM_CTLCOLOREDIT`) |
| Q15 | (UI) 그리기 기술 | 전부 GDI / 캔버스 창만 Direct2D / 전부 Direct2D | **캔버스 창만 Direct2D**(S14·S15, 필요 시 S3). 나머지 GDI. 첫 Direct2D 창(UI7)에서 첫 그리기 ms·개인 메모리·exe 크기를 GDI와 1회 측정해 확정 |
| Q16 | (UI) 테마 범위 | 현재 Flutter 테마(밝음+키오스크)만 / +고대비 / +다크 모드 | **현재 테마 + 고대비(OS 설정 따름)**. 다크 모드는 지금 Flutter에도 없으므로(`darkTheme`·`Brightness.dark`·`ThemeMode` 검색 0건) 이식 뒤 별도 과제 |
| Q17 | (UI) 프로세스 구성과 접근성 수준 | 화면 exe 1개(상주 kagent에 전부) / 2개(상주 kagent: 몽땅클립보드·업무자료함·타임머신 빠른 창 + `docfit.exe`: 주 작업 화면, 필요할 때 시작) · 접근성 UIA 이름·역할만 / UIA 전체 패턴 | **2개 + kvf는 kagent에 합침**(상주 하나로 단축키 창 모두, 주 작업 화면은 COM 작업자와 함께 필요할 때만). 접근성은 **모든 자체 그린 요소에 UIA 이름·역할·상태 + 목록 선택·단추 호출 패턴**(현재 popup·kvf에는 없음) |

## 9. 이번 작업에서 하지 않은 것
- 코드 수정·커밋·빌드·시험·측정: 하지 않음(지시대로 읽기만).
- `docs/COM_함수별_기준선_2026-11-12.md`: 찾지 못함(0절).
- 함수별 Python 자체 시간: 기존 자료 없음 → 2절의 C1~C5 이득은 대부분 '측정 필요'.
- HANDOVER.md는 555줄 중 1~251행과 목차를 읽었고 252행 이후(개발 환경·함정 상세)는 목차로만 확인했다.
- 분류는 정규식 검색과 파일 첫머리·함수 목록 읽기로 했다. A 파일 안 순수 코드 비율, C 파일이 콜백으로 COM을 받는지 전수 확인은 하지 않았다(R0에서 확인).
- (RUSTALL2) 앱 실행·화면 캡처·Direct2D 시험 창 측정은 하지 않았다. 화면 목록은 폴더·파일 단위이고 대화상자 하나하나(`showDialog` 20파일 77곳, Tk `Toplevel` 14곳)의 기능 표는 7.5대로 각 UI 카드에서 만든다. exe 크기·메모리 수치는 kagent TODO 기록 인용뿐이고 Direct2D 쪽은 전부 '측정 필요'.

## 부록 A. 파일별 분류(217개, 82,147줄)

### A — 26개 파일, 26,286줄

| 파일 | 줄 | COM 참조 | 역할 |
|---|---:|---:|---|
| `docfit_engine/page_fit.py` | 5,040 | 419 | 쪽 맞춤(조판 결과를 COM으로 읽고 고침, COM 참조 419곳) |
| `hwp-auto-docfit.py` | 3,522 | 3 | 부트스트랩·COM 파사드(엔진 함수 설치). 최종: COM 작업자 진입점으로 축소 |
| `docfit_engine/spacing.py` | 2,297 | 118 | 자간·단어 당김(문자 단위 COM, 118곳) |
| `docfit_engine/formatter_apply.py` | 2,198 | 58 | 서식 적용(COM) |
| `docfit_engine/formatter_unify.py` | 1,900 | 52 | 서식 통일(COM) |
| `docfit_engine/formatter_titles.py` | 1,851 | 1 | 제목 처리·HWPX 저장(COM) |
| `docfit_engine/pipeline.py` | 1,268 | 8 | 처리 파이프라인(열기·저장·문서 수 COM) |
| `docfit_engine/formatter_text.py` | 1,153 | 32 | 글자 서식(COM) |
| `docfit_engine/formatter_indent.py` | 951 | 86 | 내어쓰기 적용(COM) |
| `docfit_engine/formatter_utils.py` | 916 | 20 | COM 공용 도우미 |
| `docfit_engine/formatter_tables.py` | 849 | 29 | 표 서식(COM) |
| `docfit_engine/formatter_analysis.py` | 683 | 6 | 문서 분석(COM 스레드 포함, 순수 판정 섞임) |
| `docfit_engine/com_driver.py` | 551 | 10 | COM 연결·호출 래퍼 |
| `docfit_app/table_units_service.py` | 508 | 4 | 표 단위 복사·스냅샷(숨긴 한/글) |
| `docfit_engine/diff_host.py` | 436 | 3 | 비교 호스트(COM 스레드) |
| `docfit_core/hwp_bulk.py` | 386 | 0 | HWP→HWPX 일괄 변환(한/글 COM) |
| `docfit_app/table_sketch_service.py` | 325 | 4 | 표그림판 표 복사(숨긴 한/글) |
| `docfit_core/com_perf.py` | 322 | 2 | COM 호출 계측 |
| `docfit_app/conversion_runner.py` | 251 | 6 | 파일 변환 실행기(한/글 COM) |
| `docfit_engine/context.py` | 177 | 0 | 작업별 엔진 상태·포트 |
| `docfit_engine/formatter.py` | 165 | 0 | 서식 진입점 |
| `docfit_engine/runtime.py` | 164 | 0 | 엔진 실행 문맥 연결 |
| `docfit_core/hwp_process.py` | 118 | 0 | 독립 한/글 PID 기억·강제 종료 |
| `docfit_engine/security.py` | 115 | 0 | 한/글 보안 모듈 등록 |
| `docfit_core/indent_stage_trace.py` | 77 | 5 | 내어쓰기 단계 추적(ParaShape) |
| `docfit_core/table_limits.py` | 63 | 1 | 표 영역 판정(GetPos) |

### B — 34개 파일, 8,515줄

| 파일 | 줄 | COM 참조 | 역할 |
|---|---:|---:|---|
| `docfit_core/timemachine/content_search.py` | 2,340 | 0 | 본문 색인·검색(klib index_content·search, tantivy) |
| `docfit_core/file_rename/rules.py` | 642 | 0 | 파일이름 규칙(klib rename) |
| `docfit_core/timemachine/extractors_ooxml.py` | 427 | 0 | DOCX 등(klib extract_docx; xlsx·pptx는 Rust 없음→C) |
| `docfit_app/library_bridge.py` | 406 | 0 | klib 파이프 브리지 |
| `docfit_app/kclip_service.py` | 403 | 0 | 몽땅클립보드(kagent) |
| `docfit_app/notebook_service.py` | 341 | 0 | 노트북(klib nb_*) |
| `docfit_core/notebook_hwpx.py` | 338 | 0 | 노트북 HWPX 내보내기(klib nb_export_md 일부) |
| `docfit_app/library_service.py` | 313 | 0 | 자료함 서비스(klib 파이프 명령) |
| `docfit_core/timemachine/store.py` | 250 | 0 | 타임머신 저장소(kagent store.rs·klib db) |
| `docfit_app/kagent_app_service.py` | 242 | 0 | kagent 연동 서비스(브리지) |
| `docfit_core/marker_scheme.py` | 234 | 0 | 항목기호 체계(hierarchy compat/marker_scheme) |
| `docfit_app/library_scan.py` | 218 | 0 | 자료함 훑기(klib box_scan) |
| `docfit_core/timemachine/scan.py` | 209 | 0 | 타임머신 훑기(klib tm_scan) |
| `docfit_core/file_rename/executor.py` | 195 | 0 | 이름 바꾸기 실행(klib rename) |
| `docfit_core/library/store.py` | 188 | 0 | 자료함 저장소(klib db) |
| `docfit_app/kagent_client.py` | 188 | 0 | kagent 클라이언트(브리지) |
| `docfit_core/timemachine/hierarchy_path.py` | 170 | 0 | 위계 경로(hierarchy search_levels) |
| `docfit_core/timemachine/extractors_hwpx.py` | 156 | 0 | HWPX 본문(klib extract_hwpx) |
| `docfit_app/hierarchy_bridge.py` | 155 | 0 | hierarchy DLL/CLI 브리지 |
| `docfit_app/library_idle_refresh.py` | 139 | 0 | 자료함 쉬는 시간 재색인(IDLE1) |
| `docfit_core/file_rename/check.py` | 138 | 0 | 이름 검사(klib rename) |
| `docfit_core/timemachine/extractors.py` | 129 | 0 | 본문 추출 분기(klib extract) |
| `docfit_core/timemachine/extractors_hwp.py` | 115 | 0 | HWP 본문(klib extract_hwp) |
| `docfit_core/file_rename/history.py` | 114 | 0 | 이름 이력(klib rename) |
| `docfit_app/kclip_window_state.py` | 97 | 0 | 몽땅클립보드 창 상태(kagent popup) |
| `docfit_core/timemachine/extractors_text.py` | 74 | 0 | TXT·MD(klib extract_text) |
| `docfit_core/file_rename/doc_title.py` | 67 | 0 | 문서 제목 |
| `docfit_app/kvf_client.py` | 57 | 0 | kvf 클라이언트(브리지) |
| `docfit_core/file_rename/model.py` | 54 | 0 | 이름 모델 |
| `docfit_core/file_rename/datetime_rules.py` | 52 | 0 | 날짜 규칙 |
| `docfit_core/file_rename/numbering.py` | 25 | 0 | 번호 |
| `docfit_core/timemachine/extract_text.py` | 18 | 0 | 추출 진입점 |
| `docfit_core/file_rename/__init__.py` | 16 | 0 |  |
| `docfit_core/library/__init__.py` | 5 | 0 |  |

### C0 — 7개 파일, 10,482줄

| 파일 | 줄 | COM 참조 | 역할 |
|---|---:|---:|---|
| `docfit_ui/legacy.py` | 5,813 | 1 | Tk 레거시 GUI(상태·작업 큐). AppState·jobs로 옮긴 뒤 삭제 |
| `desktop_web_ui.py` | 3,984 | 12 | pywebview+숨은 Tk 브리지(_tk 78곳). Flutter↔Rust 직접 FFI·COM 작업자 IPC로 대체 |
| `hwp-auto-docfit-reset.py` | 436 | 0 | 초기화 도구(Tk). Rust 단일 exe 하위 명령으로 |
| `docfit_core/abbreviation_dialog.py` | 195 | 0 | Tk 대화상자. Flutter 화면으로 |
| `docfit_ui/scheduler.py` | 36 | 0 | Tk 소유 스레드 펌프 |
| `docfit_core/flutter_bundle.py` | 17 | 0 | 웹 번들 경로(WebView 폐지 시 불필요) |
| `docfit_ui/__init__.py` | 1 | 0 |  |

### C1 HWPX XML 읽기·분석·고쳐 쓰기 — 24개 파일, 9,177줄

| 파일 | 줄 | COM 참조 | 역할 |
|---|---:|---:|---|
| `docfit_core/format_elements.py` | 1,742 | 0 |  |
| `docfit_core/xml_hanging_indent.py` | 1,030 | 0 |  |
| `docfit_core/style_inventory.py` | 953 | 0 |  |
| `docfit_core/report_header.py` | 897 | 0 |  |
| `docfit_core/batch_format.py` | 441 | 0 |  |
| `docfit_core/hwpx/__init__.py` | 378 | 0 |  |
| `docfit_core/hwpx/canvas_hwpx_builder.py` | 316 | 0 |  |
| `docfit_core/style_unify.py` | 310 | 0 |  |
| `docfit_core/hwpx_idioms.py` | 273 | 0 |  |
| `docfit_core/hwpx/canvas_package_adapter.py` | 273 | 0 |  |
| `docfit_core/abbreviation_doc.py` | 266 | 0 |  |
| `docfit_core/fidelity/package.py` | 261 | 0 |  |
| `docfit_core/style_hierarchy.py` | 246 | 0 |  |
| `docfit_core/hwp_idioms.py` | 236 | 0 |  |
| `docfit_core/style_learning.py` | 225 | 0 |  |
| `docfit_core/fidelity/patch_hwpx.py` | 207 | 0 |  |
| `docfit_core/release_format_audit.py` | 196 | 0 |  |
| `docfit_core/fidelity/inventory.py` | 178 | 0 |  |
| `docfit_core/doc_inspector.py` | 167 | 0 |  |
| `docfit_core/fidelity/source_map.py` | 166 | 0 |  |
| `docfit_core/format_samples.py` | 163 | 0 |  |
| `docfit_core/fidelity/verify.py` | 127 | 0 |  |
| `docfit_core/style_profile_edit.py` | 108 | 0 |  |
| `docfit_core/fidelity/__init__.py` | 18 | 0 |  |

### C2 표·표그림판·엑셀 표 변환 — 20개 파일, 8,442줄

| 파일 | 줄 | COM 참조 | 역할 |
|---|---:|---:|---|
| `docfit_core/table_units.py` | 1,791 | 0 |  |
| `docfit_core/table_catalog.py` | 1,135 | 0 |  |
| `docfit_core/table_builder.py` | 663 | 0 |  |
| `docfit_core/table_style.py` | 647 | 0 |  |
| `docfit_core/table_import.py` | 553 | 0 |  |
| `docfit_core/table_sketch.py` | 521 | 0 |  |
| `docfit_core/table_sketch_recognize.py` | 400 | 0 |  |
| `docfit_core/table_width.py` | 370 | 0 |  |
| `docfit_core/excel_paste_target.py` | 358 | 0 |  |
| `docfit_core/table_arrows.py` | 275 | 0 |  |
| `docfit_core/table_gallery.py` | 262 | 0 |  |
| `docfit_core/connector_tabs.py` | 237 | 0 |  |
| `docfit_core/cell_image_bg.py` | 236 | 0 |  |
| `docfit_core/excel_table.py` | 230 | 0 |  |
| `docfit_core/excel_html_shading.py` | 199 | 0 |  |
| `docfit_core/hwp_table_import.py` | 176 | 0 |  |
| `docfit_core/table_unify.py` | 136 | 0 |  |
| `docfit_core/text_table.py` | 101 | 0 |  |
| `docfit_core/excel_table_hwpx.py` | 83 | 0 |  |
| `docfit_core/table_clipboard.py` | 69 | 0 |  |

### C3 타임머신 나머지·쉬는 시간 색인 — 11개 파일, 2,942줄

| 파일 | 줄 | COM 참조 | 역할 |
|---|---:|---:|---|
| `docfit_app/timemachine_service.py` | 1,076 | 0 |  |
| `docfit_core/timemachine/thumbs.py` | 427 | 0 |  |
| `docfit_app/idle_gate.py` | 364 | 0 |  |
| `docfit_app/idle_scheduler.py` | 335 | 0 |  |
| `docfit_core/timemachine/series.py` | 179 | 0 |  |
| `docfit_core/timemachine/related.py` | 154 | 0 |  |
| `docfit_core/timemachine/extractors_eml.py` | 121 | 0 |  |
| `docfit_core/timemachine/rules.py` | 92 | 0 |  |
| `docfit_core/timemachine/basedate.py` | 92 | 0 |  |
| `docfit_core/timemachine/extractors_pdf.py` | 91 | 0 |  |
| `docfit_core/timemachine/__init__.py` | 11 | 0 |  |

### C4 PDF·파일 변환·외부 엔진 호출 — 9개 파일, 2,519줄

| 파일 | 줄 | COM 참조 | 역할 |
|---|---:|---:|---|
| `docfit_core/kordoc_bridge.py` | 537 | 0 |  |
| `docfit_core/file_conversion.py` | 523 | 0 |  |
| `docfit_core/pdf_style_transfer.py` | 477 | 0 |  |
| `docfit_core/kordoc_runtime.py` | 311 | 0 |  |
| `docfit_core/pdf_merge.py` | 241 | 0 |  |
| `docfit_core/pdf_compress.py` | 194 | 0 |  |
| `docfit_core/session/canvas_commands.py` | 136 | 0 |  |
| `docfit_app/sketch_image_input.py` | 66 | 0 |  |
| `docfit_core/pdfium_guard.py` | 34 | 0 |  |

### C5 글 규칙·교정·순수 판정 — 43개 파일, 7,033줄

| 파일 | 줄 | COM 참조 | 역할 |
|---|---:|---:|---|
| `docfit_core/ai_answer.py` | 538 | 0 |  |
| `docfit_core/writing_aids.py` | 439 | 0 |  |
| `docfit_core/pasted_text.py` | 379 | 0 |  |
| `docfit_core/emphasis.py` | 356 | 0 |  |
| `docfit_core/continuation.py` | 329 | 0 |  |
| `docfit_core/document_rules.py` | 300 | 0 |  |
| `docfit_core/document_review.py` | 284 | 0 |  |
| `docfit_core/progress_guide.py` | 282 | 0 |  |
| `docfit_core/blank_lines.py` | 282 | 0 |  |
| `docfit_core/stage_selection.py` | 245 | 0 |  |
| `docfit_core/colon_labels.py` | 221 | 0 |  |
| `docfit_core/final_evaluation.py` | 219 | 0 |  |
| `docfit_core/abbreviations.py` | 212 | 0 |  |
| `docfit_core/labeled_text.py` | 210 | 0 |  |
| `docfit_core/korean_proofread.py` | 209 | 0 |  |
| `docfit_core/attachment_page.py` | 184 | 0 |  |
| `docfit_core/hangul_fonts.py` | 157 | 0 |  |
| `docfit_core/updater.py` | 156 | 0 |  |
| `docfit_core/outline_ops.py` | 140 | 0 |  |
| `docfit_core/marker_spacing.py` | 135 | 0 |  |
| `docfit_core/save_guard.py` | 133 | 0 |  |
| `docfit_core/char_ranges.py` | 120 | 0 |  |
| `docfit_core/spacing_reset.py` | 118 | 0 |  |
| `docfit_core/attachment_block.py` | 115 | 0 |  |
| `docfit_core/text_edit.py` | 114 | 0 |  |
| `docfit_core/number_check.py` | 105 | 0 |  |
| `docfit_core/mcp_service.py` | 105 | 0 |  |
| `docfit_core/session/command_bus.py` | 99 | 0 |  |
| `docfit_core/ai_prompts.py` | 98 | 0 |  |
| `docfit_core/page_fit_search.py` | 85 | 0 |  |
| `docfit_core/app_state.py` | 83 | 0 |  |
| `docfit_core/session/document_session.py` | 82 | 0 |  |
| `docfit_engine/font_catalog_cache.py` | 79 | 0 |  |
| `docfit_core/config_store.py` | 67 | 0 |  |
| `docfit_core/update_signature.py` | 64 | 0 |  |
| `docfit_core/marker_prefix.py` | 64 | 0 |  |
| `docfit_core/session/style_resolver.py` | 58 | 0 |  |
| `docfit_core/__init__.py` | 53 | 0 |  |
| `docfit_core/asterisk_superscript.py` | 40 | 0 |  |
| `docfit_core/network_policy.py` | 34 | 0 |  |
| `docfit_core/roman_heading.py` | 22 | 0 |  |
| `docfit_core/semantic_units.py` | 11 | 0 |  |
| `docfit_core/session/__init__.py` | 7 | 0 |  |

### C6 앱 서비스·상태·IPC·설정·업데이트 — 40개 파일, 6,588줄

| 파일 | 줄 | COM 참조 | 역할 |
|---|---:|---:|---|
| `docfit_app/file_rename_service.py` | 632 | 2 |  |
| `docfit_app/external_apps_download.py` | 630 | 0 |  |
| `docfit_app/diff_host_service.py` | 452 | 0 |  |
| `docfit_app/update_service.py` | 402 | 0 |  |
| `docfit_app/shell_batch.py` | 397 | 0 |  |
| `docfit_app/application_service.py` | 374 | 0 |  |
| `docfit_app/jobs.py` | 349 | 0 |  |
| `docfit_app/external_apps_service.py` | 344 | 0 |  |
| `docfit_app/flow_service.py` | 301 | 0 |  |
| `docfit_app/freeze_tracer.py` | 257 | 0 |  |
| `docfit_app/native_file_dialog_service.py` | 254 | 1 |  |
| `docfit_app/cli.py` | 232 | 0 |  |
| `docfit_app/sidebar_mode.py` | 186 | 0 |  |
| `docfit_app/settings_schema.py` | 178 | 0 |  |
| `docfit_app/idiom_service.py` | 178 | 0 |  |
| `docfit_app/configuration_service.py` | 156 | 0 |  |
| `docfit_app/abbreviation_service.py` | 142 | 0 |  |
| `docfit_app/shell_menu.py` | 133 | 0 |  |
| `docfit_app/native_table_clipboard.py` | 90 | 8 |  |
| `docfit_app/ipc_server.py` | 89 | 0 |  |
| `docfit_app/excel_table_service.py` | 83 | 0 |  |
| `docfit_app/cell_bg_service.py` | 82 | 0 |  |
| `docfit_app/settings_service.py` | 76 | 0 |  |
| `docfit_app/result_compare.py` | 75 | 0 |  |
| `docfit_app/instance_files.py` | 73 | 0 |  |
| `docfit_app/job_envelope.py` | 57 | 0 |  |
| `docfit_app/table_template_service.py` | 53 | 0 |  |
| `docfit_app/self_test.py` | 52 | 0 |  |
| `docfit_app/clipboard_service.py` | 51 | 0 |  |
| `docfit_app/backend_main.py` | 50 | 0 |  |
| `docfit_app/document_tools_service.py` | 47 | 0 |  |
| `docfit_infra/storage/process_lock.py` | 43 | 0 |  |
| `docfit_app/new_document_service.py` | 28 | 0 |  |
| `docfit_app/api_contract.py` | 18 | 0 |  |
| `docfit_app/app_state.py` | 13 | 0 |  |
| `docfit_app/settings_migrations.py` | 7 | 0 |  |
| `docfit_infra/storage/__init__.py` | 1 | 0 |  |
| `docfit_infra/__init__.py` | 1 | 0 |  |
| `docfit_engine/__init__.py` | 1 | 0 |  |
| `docfit_app/__init__.py` | 1 | 0 |  |

### X — 3개 파일, 163줄

| 파일 | 줄 | COM 참조 | 역할 |
|---|---:|---:|---|
| `kagent/tests/khot_live.py` | 71 | 0 | Rust/Flutter 폴더 안의 시험·도구 스크립트 |
| `flutter_ui/tool/check_arrow_glyphs.py` | 48 | 0 | Rust/Flutter 폴더 안의 시험·도구 스크립트 |
| `hierarchy/tests/measure_ndet.py` | 44 | 0 | Rust/Flutter 폴더 안의 시험·도구 스크립트 |

## 확정(사용자 2026-11-13: "권고안대로 진행해")
Q1~Q17 모두 권고안으로 확정. R0부터 시작.
Q16 변경(사용자 2026-11-13): 고대비만이 아니라 다크 모드 테마 추가(라이트·다크·고대비, 시스템 설정 따라가기).
