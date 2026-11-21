# PDFWIN1 설계 — PDF 편집 Rust + Win32 전환 설계 문서

작성일: 2026-11-22 · 브랜치 `feature/PDFWIN1` · 상태: **설계 문서(코드·설정 변경 없음)**

근거 문서:
- `TODO.md`: 90행(PDF 편집 탭 UI 전면 재디자인 — PDFCraft식 + 드래그앤드롭 파일 추가)
- `docs/DEV_PLAN_2026-11-28.md`: 21행(G7 나머지 메뉴 Rust 이식), 25행(G11 PDF 편집 탭 재디자인 마무리)
- `docs/RUST_MIGRATION_INVENTORY_2026-11-19.md`: 50, 77, 107, 274행(11번 PDF 편집 행)
- `docs/OUTLINER_WIN32_DESIGN_2026-11-20.md`: 전환 설계 문서 형식 및 비교 기준 참고
- `flutter_ui/lib/features/pdf_edit/pdf_edit_workspace.dart`: 화면 기능 전수 (1,202줄)
- `docfit_app/pdf_edit_service.py`: Python 서비스 및 썸네일 공급 (226줄)
- `pdf_edit_core/`: lopdf 기반 PDF 편집 코어 엔진 (Cargo.toml, src/lib.rs 314줄, src/main.rs 155줄, tests/safety.rs 119줄, 총 588줄)
- `desktop_web_ui.py`: `pdf_edit` 브리지 (2383-2395행, 4000행)
- `docview_win/`: 문서뷰 Rust+Win32 격자 가상화 창 구조 (Cargo.toml, src/ 14개 파일, 총 2,858줄)

표기 규칙: 근거는 `파일:줄`. 읽거나 실행해 확인하지 못한 것은 **미확인(U번호)** 으로 적고 9절에 모았다. 추정치는 "추정"이라 적고 산출 근거를 밝혔다.

---

## 0. 요약과 결론

| 항목 | 결론 |
|---|---|
| **권장 창 구조** | **1안: 문서뷰 창(`docview_win`)의 격자 가상화 및 GDI 더블 버퍼링 아키텍처 재사용·확장.**<br>좌측: 점선 드롭존 / 쪽 썸네일 가상 격자(합치기 시 순서 목록), 우측: 고정 작업 패널(약 280px), 상단: 도구 칩 막대. |
| **Python 잔류 사유** | **없음.** 엔진은 이미 Rust 크레이트(`pdf_edit_core`, `lopdf 0.34`, 588줄)로 완성되어 `pdf-edit.exe`로 번들 배포 중이다(`pdf_edit_service.py:23-40`, `HWP_AutoDocFit.spec:17-21`). Python 서비스가 담당하던 사전 분류·검증(`_classify`)과 CLI 호출, 임시 파일 정리(`cleanup`)는 Rust 단독 앱으로 100% 흡수 가능하다(1.3절, 4절). |
| **Flutter 잔류 사유** | **없음.** 인벤토리가 "대체 가능(단독 창 또는 docview_win 확장)"(`RUST_MIGRATION_INVENTORY_2026-11-19.md:107`)으로 판정했다. 화면 1,202줄(`pdf_edit_workspace.dart`)의 모든 요소(드롭존, 도구 칩, 쪽 격자, 선택, 드래그 순서, Undo/Redo, 사이드 패널)는 Win32 네이티브 UI로 100% 치환 가능하다. |
| **쪽 썸네일 공급** | **1순위 권장: Windows 내장 `Windows.Data.Pdf`(WinRT API) 우선 검토.**<br>추가 C 바이너리(PDFium 20MB 등) 없이 번들 크기 0MB, 외부 라이선스 종속 없음. 렌더 실패 시 번호 카드(Numbered Placeholder) 폴백(Flutter 현행과 동일). |
| **가장 큰 위험 3가지** | ① **WinRT `Windows.Data.Pdf` 비동기 FFI 처리 및 환경별 호환성**(3절, U3)<br>② **다중 작업(회전+삭제+순서) 인메모리 파이프라인과 CLI 다회 호출 간 결과 동등성(차이 0) 보장**(5.3절, 6절)<br>③ **Win32 가상 격자에서의 마우스 드래그 순서 재배열 UX 완성도**(5.2절, U2) |
| **규모 추정** | Rust 약 2,800~3,500줄(기존 `docview_win` 격자 재사용분 제외 신규/수정), 카드 약 6~8장(7절). 추정. |

---

## 1. 현재 구조 지도

### 1.1 호출 경로

```
[Flutter UI]
  pdf_edit_workspace.dart (1,202줄)
    ↓ api('pdf_edit', [payload]) (129, 154, 247, 309, 347행)
[Python Bridge & Service]
  desktop_web_ui.py::DesktopWebBridge.pdf_edit (2383-2395행, 4000행 등록)
    ↓ PdfEditService.execute(action, source, output, pages, degrees, sources)
  docfit_app/pdf_edit_service.py (226줄)
    ↓ subprocess.run([executable, action, ...], timeout 30초/300초) (149, 168행)
[Rust CLI Engine]
  pdf-edit.exe (pdf_edit_core/src/main.rs, 155줄)
    ↓ 함수 직접 호출 (delete_pages, reorder, rotate_pages, split, merge, page_info, page_count)
  pdf_edit_core::lib (pdf_edit_core/src/lib.rs, 314줄, lopdf 0.34)
```

- **호출 방식**: Flutter 웹 UI에서 JSON RPC 형태(`widget.api('pdf_edit', [payload])`)로 호출하면 Python `DesktopWebBridge.pdf_edit`이 `PdfEditService`로 위임한다.
- **프로세스 기동**: `PdfEditService`는 요청마다 `subprocess.run`으로 `pdf-edit.exe` 프로세스를 1회 실행한다(`pdf_edit_service.py:149, 168`).
- **상태 보존 방식**: `pdf-edit.exe` 엔진은 상태를 저장하지 않는 순수 CLI 유틸리티다. Flutter가 누적된 편집 작업(`_operations`, `pdf_edit_workspace.dart:41`)을 저장할 때, 중간 단계마다 임시 파일(`.__pdfui_{timestamp}_{index}.pdf`, 364행)을 생성하여 디스크 I/O를 거쳐 체이닝한 뒤 마지막에 `cleanup` 액션(347행)을 호출해 임시 파일을 삭제한다.
- **실행 파일 위치**: `PdfEditService._find_executable()`(`pdf_edit_service.py:23-41`)은 `_internal/resources/pdf-edit.exe`, `resources/pdf-edit.exe`, `pdf_edit_core/target/release/pdf-edit.exe` 순으로 탐색한다.

### 1.2 엔진 CLI 계약 및 형태 (`pdf_edit_core`)

`pdf_edit_core`는 경량 오프라인 PDF 페이지 편집 코어로 `lopdf 0.34`를 사용한다(`pdf_edit_core/Cargo.toml:9`).

| 서브명령 (`main.rs`) | 인자 (`argv`) | 핵심 동작 (`lib.rs`) | 반환/출력 |
|---|---|---|---|
| `count` | `IN` | `page_count(path)` | 표준출력 쪽수 정수 (`println!("{n}")`) |
| `info` | `IN` | `page_info(path)` | 표준출력 JSON: `{"pages":[{"number":1,"width":595.0,"height":842.0,"rotation":0},...]}` |
| `delete` | `IN OUT PAGES` | `delete_pages(in, out, &remove)` | 성공 시 종료코드 0, `OUT` 새 파일 생성 |
| `reorder` | `IN OUT 3,1,2` | `reorder(in, out, &order)` | 성공 시 종료코드 0, `OUT` 새 파일 생성 |
| `rotate` | `IN OUT DEGREES PAGES` | `rotate_pages(in, out, &pages, degrees)` | 성공 시 종료코드 0, `OUT` 새 파일 생성 |
| `split` | `IN DIR 1-2,4-5` | `split(in, dir, &ranges)` | 성공 시 종료코드 0, `DIR/part-1.pdf` 등 생성 |
| `merge` | `OUT IN1 IN2...` | `lopdf::Document::with_version("1.5")` 객체 리매핑 및 합치기 (`main.rs:89-142`) | 성공 시 종료코드 0, `OUT` 새 파일 생성 |

엔진의 안전 규칙(`pdf_edit_core/src/lib.rs`):
- 입력 파일 크기 1GB 초과 시 거부(`lib.rs:28-30`).
- 암호화된 PDF(`doc.is_encrypted()`) 편집 거부(`lib.rs:32-36`).
- 전자서명된 PDF(객체 딕셔너리 `/FT` 또는 `/Type` 값이 `/Sig`) 편집 거부(`lib.rs:37-45`).
- 쪽수 범위 1..=3000 페이지 제한(`lib.rs:47-49`).
- 기존 파일 덮어쓰기 금지(`validate_output`, `persist_noclobber`, `lib.rs:52-62, 75`).
- 부모 노드 순환 탐색 방지(`inherited`, `seen.insert`, `lib.rs:79-93`).

### 1.3 Python 서비스(`pdf_edit_service.py`)의 명령과 분류

`PdfEditService.execute(action, source, output, *, pages, degrees, sources)` (`pdf_edit_service.py:76-162`):

1. **사전 분류 및 유효성 검사 (`_classify`, 43-75행)**:
   - 1GB 크기 제한 검사.
   - 매직 넘버 검사: `%PDF-`로 시작하고 마지막 4096바이트 안에 `%%EOF` 존재 확인.
   - `pypdf.PdfReader`로 암호화(`/Encrypt`), 권한 제한(`/P` != -1), 빈 페이지 문서, 전자서명(`/Annots` 내부 `/Sig`) 사전 검사 및 세분화된 오류 코드(`PDF_ENCRYPTED`, `PDF_RESTRICTED`, `PDF_DAMAGED`, `PDF_SIGNED`) 발생.
2. **액션별 처리**:
   - `info`: `_page_info(source)` 호출 -> `pdf-edit.exe info` 실행 결과 JSON 파싱(164-182행).
   - `thumbnail`: `_thumbnail(source, page_number)` 호출 -> `pypdfium2`로 렌더링 -> PIL JPEG 인코딩 -> `thumbs.ThumbnailCache` 캐싱 -> base64 반환(183-222행).
   - `delete` / `reorder` / `rotate` / `split` / `merge`: 경로 검증(결과 경로가 기존 경로와 동일하거나 이미 존재하면 `OUTPUT_EXISTS` 발생), 인자 정규화 후 `pdf-edit.exe` 실행(95-162행).
   - `cleanup`: 전달받은 임시 파일 목록 중 `.__pdfui_`로 시작하고 `.pdf`로 끝나는 파일 삭제(77-85행).

### 1.4 Flutter 화면(`pdf_edit_workspace.dart`)의 상태와 상호작용

화면 줄 수: 1,202줄 (`flutter_ui/lib/features/pdf_edit/pdf_edit_workspace.dart`).

| 상태 변수 | 줄 | 설명 |
|---|---|---|
| `_tools` | 21-27 | 5종 작업 정의: `delete`(삭제), `rotate`(회전), `split`(분할), `merge`(합치기), `reorder`(순서 변경) |
| `_action` | 32, 501-522 | 현재 선택된 도구 (도구 칩) |
| `_source`, `_output` | 30-31 | 원본 파일 경로 및 결과 파일 경로 |
| `_mergeSources` | 29, 813-858 | 합치기 모드 시 대상 파일 목록 (`ReorderableListView` 순서 제어) |
| `_pageCount`, `_pageOrder` | 36, 39 | 전체 쪽 수 및 현재 쪽 배치 순서 리스트 |
| `_selectedPages` | 40, 431-452 | 선택된 쪽 번호 집합 (`Set<int>`, 단일/Shift 범위/Ctrl 개별) |
| `_operations`, `_redo` | 41-42, 367-430 | 누적 작업 스택 (`_EditOperation(action, pages, degrees)`) 및 실행취소/다시실행 스택 |
| `_thumbnails` | 37, 151-175 | 쪽 번호 -> 썸네일 이미지 데이터 (`Uint8List`, base64 디코딩 결과) |
| `_busy`, `_message` | 34-35, 940-953 | 작업 실행 중 상태(프로그레스바 표시, 버튼 비활성화) 및 결과/오류 메시지 |
| `_completedPath` | 45, 955-981 | 완료된 결과 파일 경로 및 문서뷰 열기 단추 연동 |

### 1.5 기존 시험 자산

| 시험 자산 | 위치 및 줄 수 | 시험 항목 수 | 상태 및 내용 |
|---|---|---|---|
| **Rust 코어 안전성 시험** | `pdf_edit_core/tests/safety.rs` (119줄) | 6개 시험 | 덮어쓰기 방지, 손상/서명/암호화 거부, 계층 트리 회전/순서변경 기하 보존, 유효 분할/삭제 (모두 통과) |
| **Rust 코어 단위 시험** | `pdf_edit_core/src/lib.rs` (314줄) | 3개 시험 | 메타데이터 조회, 유효하지 않은 범위 거부, 순서 및 쪽수 보존 (모두 통과) |
| **Python 서비스 시험** | `tests/test_pdf_edit_service.py` (167줄) | 10개 시험 | 가짜 엔진 호출, 분할/합치기 라우팅, 권한 마커, 암호화/서명/손상 분류, 설치본 탐색 (9개 통과, 1개 macOS `/private` 심볼릭링크 경로 비교 실패) |
| **Flutter 위젯 시험** | `flutter_ui/test/pdf_edit_workspace_test.dart` (415줄) | 10개 시험 | 비활성 탭 드롭 무시, 활성 탭 드롭 반영, 도구 칩 선택, 메타데이터 연동 및 렌더 실패 시 번호 카드 유지, Shift/Ctrl 선택, 누적 작업/Undo/Redo, 드래그/방향키 순서 변경, 합치기 순서 전달, 원본 덮어쓰기 차단, 진행 표시 |

---

## 2. 기능 전수표 (Flutter 기능 → Win32 대응 · 엔진 호출)

Flutter 화면(`pdf_edit_workspace.dart`)의 모든 사용자 접점 및 동작을 Win32 네이티브 대응 요소와 엔진 호출로 1:1 매핑한다.

| 번호 | Flutter 기능 | Flutter 코드 위치 | Win32 네이티브 대응 요소 | 엔진/Rust 코어 직접 호출 |
|---|---|---|---|---|
| 1 | **빈 화면 드롭 영역** | `_dropZone` (550-601) | `WM_PAINT` 점선 사각형(`PS_DOT`), 중앙 아이콘/안내문구, `WM_SETCURSOR` 핸드 포인터 | 없음 (UI 상태 전이) |
| 2 | **외부 파일 드래그앤드롭** | `watchDocumentDropPaths` (53, 63-71), `DragTarget<String>` (550) | `DragAcceptFiles(hwnd, TRUE)`, `WM_DROPFILES` (`HDROP`에서 `.pdf` 파일 경로 추출) | `pdf_edit_core::page_info` (쪽수/크기 메타데이터 즉시 획득) |
| 3 | **파일 선택 대화상자** | `_pickSource` (98-101), `openFile` | `GetOpenFileNameW` (필터: `PDF 파일 (*.pdf)\0*.pdf\0\0`) | `pdf_edit_core::page_info` |
| 4 | **파일 카드 표시 및 제거** | `_fileCard` (621-666) | 상단 영역 패널: 파일명 라벨 + [×] 닫기 단추(클릭 시 초기화) | 리소스 및 메모리 해제 |
| 5 | **도구 칩 선택** | `_ToolChip` (501-522, 991-1040) | 툴바 영역 오너 드로우 단추 5개 (`delete`, `rotate`, `split`, `merge`, `reorder`), 선택 상태 배경/테두리 강조 | 기본 결과 파일명 갱신 (`_편집.pdf`, `_합본.pdf`, `_분할`) |
| 6 | **쪽 썸네일 격자 가상화** | `GridView.builder` (713-770), `SliverGridDelegateWithMaxCrossAxisExtent` | `docview_win` 격자 가상화 엔진(`layout.rs`, `paint.rs` 더블 버퍼링, `SetScrollInfo`) | 가시 범위 내 쪽만 비동기 썸네일 공급 |
| 7 | **쪽 번호 배지 & 번호 카드** | `_PageCard` (1042-1180), 렌더 실패 시 번호 카드 | DIB 썸네일 비트맵 전송(`BitBlt`/`StretchBlt`) + 좌상단 둥근 배지 사각형과 텍스트 출력 | 썸네일 공급자 또는 기본 번호 카드 그리기 |
| 8 | **쪽 선택 (단일/Shift/Ctrl)** | `_selectPage` (431-452), `onTap` (1088-1094) | `WM_LBUTTONDOWN` 시 `GetKeyState(VK_SHIFT)`, `GetKeyState(VK_CONTROL)` 검사 -> `selected_pages` 갱신 -> 부분 재그리기 | 없음 (UI 상태 갱신) |
| 9 | **전체 선택 / 선택 해제** | 689-708행 버튼 | 사이드 패널 또는 상단 바의 [전체 선택] 단추 클릭 시 모든 쪽 선택 토글 | 없음 |
| 10 | **카드 위 호버/선택 단추** | 1148-1172행 (좌회전, 우회전, 삭제 단추) | 선택된 카드 우상단에 미니 단추 3개 오버레이 또는 마우스 호버 영역 히트 테스트 | `_record(rotate)` / `_record(delete)` 스택 적재 |
| 11 | **키보드 방향키 순서 변경** | `_MoveIntent`, 좌우 방향키 (1078-1083, 755-767) | `WM_KEYDOWN` (`VK_LEFT`, `VK_RIGHT`) -> 인접 슬롯과 위치 맞교환 | `_record(reorder)` 스택 적재 |
| 12 | **마우스 드래그 순서 재배열** | `Draggable<int>`, `DragTarget<int>` (1064-1076, 741-754) | 마우스 드래그 시작(`SetCapture`), 이동 중 삽입 위치 가이드 선(Insert Marker) 렌더링, 드롭(`ReleaseCapture`) | `_record(reorder)` 스택 적재 |
| 13 | **작업 스택 누적 & Undo/Redo** | `_record`, `_undo`, `_redoEdit`, `_operations` (367-430) | 메모리 내 `Vec<EditOperation>` 및 Undo/Redo 스택 관리, 단축키 `Ctrl+Z` / `Ctrl+Y` 및 단추 | 메모리 내 쪽 순서 및 가상 뷰 재계산 |
| 14 | **분할 페이지 범위 입력** | `_pages` 텍스트 필드 (777-794), `_parsePages` (76-96) | 우측 패널의 `EDIT` 컨트롤 (`ES_AUTOHSCROLL`), 실시간 구문 검증(`1-3, 5`) 및 에러 텍스트 표시 | `pdf_edit_core::split` 범위 생성 |
| 15 | **회전 각도 선택** | `_degrees` 드롭다운 (797-806, 90/180/270도) | 우측 패널의 `COMBOBOX` 컨트롤 (`CBS_DROPDOWNLIST`) | `rotate` 명령 각도 지정 |
| 16 | **합치기 전용 순서 뷰** | `_mergeEditor` (813-858), `_mergeSources` | 다중 파일 선택(`GetOpenFileNameW` 다중선택 플래그), 리스트뷰(`SysListView32` 또는 오너 드로우 목록) 상하 드래그 순서 변경 | `pdf_edit_core` merge 객체 리매핑 |
| 17 | **저장 위치 변경** | `_pickOutput` (190-197), `getSaveLocation` | `GetSaveFileNameW` (분할 모드는 `IFileDialog` 폴더 선택 모드) | 결과 경로 저장 |
| 18 | **저장 실행 및 안전 가드** | `_run` (211-356), 덮어쓰기 검사, 원본 보호 | 실행 단추 클릭 시: 원본 경로 일치 검사 -> 백그라운드 워커 스레드 기동 -> 진행률 표시 | `pdf_edit_core` 인메모리 파이프라인 수행 후 단일 파일 저장 |
| 19 | **진행률 표시 및 취소** | `LinearProgressIndicator` (908), `_busy` 비활성화 | `PROGRESS_CLASS` 컨트롤 또는 상태바 진행 텍스트, [취소] 단추 활성화 | 워커 스레드 `AtomicBool` 취소 플래그 전달 |
| 20 | **완료 후 파일 열기** | `_completedPath` (955-981), `doc_grid open` | 완료 알림 카드 + [파일 열기] 단추 -> `ShellExecuteW(NULL, L"open", path, ...)` | 셸 기본 뷰어로 열기 |

---

## 3. 쪽 썸네일 공급 (PDF 렌더)

### 3.1 현행 구조와 문제점

- **현행**: Python `pypdfium2`를 사용해 PDF를 렌더링하고, PIL을 통해 JPEG으로 압축 후, 디스크 캐시(`thumbs.ThumbnailCache`)를 거쳐 base64 문자열로 Flutter에 전달한다(`pdf_edit_service.py:183-222`).
- **문제점**:
  1. Python 프로세스와 무거운 `pypdfium2` C++ 바이너리가 번들에 포함되어야 한다.
  2. 메모리 사용량이 크고, base64 IPC 인코딩/디코딩 오버헤드로 인해 쪽수가 많은 PDF에서 격자 로딩 시 버벅임이 발생한다.
  3. 전면 Rust+Win32 전환 시 Python 헤드리스 의존성을 남기게 되는 핵심 병목이다.

### 3.2 세 가지 대안 비교

| 비교 항목 | **대안 A: Windows 내장 `Windows.Data.Pdf`(WinRT)** (권장) | **대안 B: `pdfium-render` (C DLL 번들)** | **대안 C: 순수 Rust 렌더러 (hayagriva 등)** |
|---|---|---|---|
| **번들 크기** | **0 MB** (Windows 10/11 시스템 기본 내장 컴포넌트) | **+15 ~ 25 MB** (`pdfium.dll` 배포 필요) | 약 +2 ~ 5 MB (순수 Rust 컴파일) |
| **외부 라이선스** | **없음** (OS 내장 API) | Apache 2.0 / BSD 3-Clause | MIT / Apache 2.0 |
| **렌더 충실도** | **높음** (Edge/Windows 내장 PDF 엔진, 하드웨어 가속) | **최고** (현행 `pypdfium2`와 100% 동일한 Google PDFium) | **낮음** (복잡한 폰트, 벡터, 특수 색공간 결함 다수) |
| **Rust 연동성** | `windows` 크레이트 (`Data_Pdf`, `Storage_Streams`) FFI 비동기 호출 | C FFI 바인딩 크레이트(`pdfium-render`) 동기 호출 | 순수 Rust crate 호출 |
| **플랫폼 요구사항** | Windows 8.1 / Windows 10 (빌드 15063 Creators Update 이상 권장) | Windows 7 이상 지원 가능 | 플랫폼 독립적 |
| **종합 평가** | **1순위 권장안**: Rust+Win32 전면 전환 원칙(최고 속도·최소 자원·추가 의존성 0)에 완벽히 부합 | 차선책: WinRT 사용 불가 환경 또는 절대적 렌더 일치 필요 시 폴백 | 부적합: 실무 문서 렌더 품질 미달 |

### 3.3 권장안: `Windows.Data.Pdf` 기반 썸네일 아키텍처

1. **WinRT API 파이프라인**:
   ```
   파일 경로 (HSTRING / StorageFile)
     → PdfDocument::LoadFromFileAsync(file)?
     → doc.GetPage(page_index)?
     → InMemoryRandomAccessStream 생성
     → page.RenderToStreamAsync(&stream)?
     → 스트림 데이터를 메모리 DIB / WIC Bitmap으로 디코딩
     → HBITMAP 생성 후 UI 캐시에 보관
   ```
2. **비동기 스레드 풀 격리**:
   - WinRT 비동기 호출(`IAsyncOperation`)은 UI 스레드를 차단하지 않도록 전용 백그라운드 워커 스레드 풀에서 실행한다.
   - `roapi::RoInitialize(RO_INIT_MULTITHREADED)`를 워커 스레드 시작 시 호출한다.
3. **폴백 정책 (안전망)**:
   - 문서가 손상되었거나 WinRT가 지원하지 않는 특수 포맷/구형 OS인 경우, Flutter 현행 동작과 동일하게 **"페이지 번호 카드 (Numbered Card)"**로 즉시 폴백한다(`pdf_edit_workspace_test.dart:92-117` 검증 확인). 화면 조작(순서 변경, 삭제, 회전 등)에는 전혀 지장을 주지 않는다.

---

## 4. 창 구조 비교와 권장안

### 4.1 세 안의 비교

```
+-----------------------------------------------------------------------------------+
|  [도구 칩 막대: 쪽 삭제 | 회전 | 분할 | 합치기 | 순서 변경]           [PDF 추가/변경]   |
+-----------------------------------------------------------+-----------------------+
|                                                           | [작업 설정 패널]      |
|  [중앙 좌측 뷰포트]                                       | 대상: 2, 4 쪽         |
|  - 파일 미선택 시: 점선 드롭존                             |                       |
|  - 단일 파일 모드: 쪽 썸네일 가상화 격자 (GDI 더블 버퍼링)  | [저장 위치 변경]      |
|    * 각 카드: 쪽 번호 배지, 썸네일, 회전 표시            | C:\...\편집본.pdf     |
|    * 호버/선택: 좌회전, 우회전, 삭제 단추                |                       |
|    * 드래그: 순서 변경 삽입 가이드 선                    | [회전 각도: 90°  v]   |
|  - 합치기 모드: 합칠 파일 목록 (순서 재배열 지원)         |                       |
|                                                           | [작업 큐: 회전, 삭제] |
|                                                           | [실행취소] [다시실행] |
|                                                           |                       |
|                                                           | [저장 (진행률 바)]    |
+-----------------------------------------------------------+-----------------------+
|  하단 상태바: 원본 파일 유지 안내 · 완료 시 결과 파일 열기 링크                    |
+-----------------------------------------------------------------------------------+
```

#### A안: `docview_win` (문서뷰) 아키텍처 재사용 및 확장 (권장)
- **구조**: 메인 프레임 창 안에서 좌측 뷰포트는 GDI 직접 그리기(가상 스크롤 + 더블 버퍼링), 우측 패널은 Win32 표준 자식 컨트롤(`EDIT`, `COMBOBOX`, `BUTTON`, `PROGRESS`)로 구성.
- **장점**:
  1. `docview_win` 크레이트에서 이미 검증된 `layout.rs`(SliverGridDelegate 가로세로비 0.74, 여백 16, 간격 14 계산 순수 함수), `paint.rs`(GDI 더블 버퍼링 및 DIB 전송), `ui_theme`(다크/라이트 테마 색상), DPI 스케일링(`scale(v, dpi)`)을 90% 이상 그대로 재사용 가능하다.
  2. 수천 페이지의 대형 PDF라도 화면에 보이는 카드(약 12~24개)만 그리므로 메모리가 5MB 미만으로 유지된다(`docview_win/src/layout.rs:100` `visible_range`).
  3. PDFCraft식 모던 UI(둥근 모서리, 선택 강조 테두리, 카드 위 플로팅 단추, 점선 드롭존)를 GDI+나 커스텀 GDI로 깔끔하게 구현할 수 있다.
- **단점**: 드래그 앤 드롭 재배열 시 마우스 이동에 따른 삽입 가이드 바(Insert Marker) 렌더링 로직을 직접 구현해야 한다.

#### B안: 표준 Win32 컨트롤 조합 (SysListView32 가상 모드)
- **구조**: 좌측 격자를 `SysListView32`(`LVS_ICON | LVS_OWNERDATA`) 컨트롤로 배치.
- **장점**: 윈도우 표준 컨트롤이므로 스크롤과 기본 선택 처리를 OS가 담당.
- **단점**:
  1. 다크 모드 지원(`NM_CUSTOMDRAW`)이 극히 조잡하고 OS 버전마다 다르게 동작한다.
  2. 썸네일 카드 위에 호버 액션 단추(좌회전/우회전/삭제)를 띄우거나 회전 각도 배지를 오버레이하기가 매우 어렵다.
  3. 드래그 재배열 시의 시각적 피드백(PDFCraft식 부드러운 위치 교환) 구현이 제한된다.

#### C안: WebView2 기반 단독 UI
- **구조**: 웹 프론트엔드를 패키징하고 Rust WebView2 호스트(`R-2`)로 래핑.
- **장점**: HTML/CSS/React 등으로 화면을 미려하게 구현 가능.
- **단점**: 메모리 150MB 이상 추가 소모, 시작 시간 지연(수백 ms), 전면 Rust+Win32 경량화 원칙에 정면 배치.

### 4.2 권장안 상세 스펙 (A안 확정)

- **기본 윈도우 크기**: 가로 1,120px, 세로 740px (최소 860 × 520px).
- **영역 분할**:
  - `TOOLBAR_H`: 54px (상단 도구 칩 5개, 파일 추가 단추).
  - `SIDE_PANEL_W`: 280px (우측 작업 설정 영역).
  - `STATUS_H`: 28px (하단 상태 메시지 및 안전 안내 문구).
  - `VIEWPORT`: 좌측 잔여 영역 전체 (드롭존 / 쪽 격자 / 합치기 목록).
- **DPI 스케일링**: `WM_DPICHANGED` 시 `ui_theme` 글꼴 및 `layout::scale`을 통해 모든 영역의 마진, 패딩, 카드 크기를 동적 재계산.

---

## 5. 드래그앤드롭 · 긴 작업 진행 · 취소

### 5.1 외부 파일 드래그앤드롭 (OS 탐색기 → 창)

1. `CreateWindowExW` 직후 `DragAcceptFiles(hwnd, TRUE)` 호출.
2. `WM_DROPFILES` 메시지 수신 시:
   ```rust
   let hdrop = wparam as HDROP;
   let count = DragQueryFileW(hdrop, 0xFFFFFFFF, null_mut(), 0);
   let mut paths = Vec::new();
   for i in 0..count {
       let len = DragQueryFileW(hdrop, i, null_mut(), 0);
       let mut buf = vec![0u16; len as usize + 1];
       DragQueryFileW(hdrop, i, buf.as_mut_ptr(), len + 1);
       paths.push(String::from_utf16_lossy(&buf[..len as usize]));
   }
   DragFinish(hdrop);
   ```
3. 파일 확장자가 `.pdf`인지 검사하고, 현재 액션이 `merge`이면 `merge_sources`에 추가, 그 외에는 단일 원본 파일(`source`)로 등록 후 즉시 `page_info`를 조회한다.

### 5.2 쪽 간 마우스 드래그 순서 재배열

1. **드래그 시작 감지**:
   - `WM_LBUTTONDOWN`: 클릭한 마우스 좌표 `(x, y)`로부터 격자 슬롯 인덱스 `from_idx` 식별. 시작 위치 `(start_x, start_y)` 기록.
   - `WM_MOUSEMOVE`: 마우스 버튼이 눌린 상태에서 `GetSystemMetrics(SM_CXDRAG)`(통상 4px) 이상 이동하면 `dragging = true` 진입, `SetCapture(hwnd)`.
2. **드래그 중 시각 피드백**:
   - 현재 마우스 좌표 아래에 있는 타깃 슬롯 `to_idx` 계산.
   - 드롭 위치에 **삽입 가이드 수직선(Insert Marker, 강조 색상 두께 3px)** 렌더링.
   - 이동 중인 카드는 반투명(알파 블렌딩) 또는 점선 테두리로 표시.
3. **드래그 종료 및 적용**:
   - `WM_LBUTTONUP`: `ReleaseCapture()`. `from_idx != to_idx`이면 쪽 순서 배열(`page_order`)에서 항목을 이동하고, 작업 스택에 `_record(EditOperation::Reorder(new_order))`를 적재한 뒤 `InvalidateRect(hwnd, null, FALSE)`.

### 5.3 긴 작업 진행 · 취소 · 인메모리 최적화

- **임시 파일 제거 및 인메모리 파이프라인**:
  - 현행 Flutter+Python 방식은 회전 후 삭제 같은 복합 작업 시 디스크에 `.__pdfui_...` 임시 파일을 만들고 지운다(`pdf_edit_workspace.dart:358-365`).
  - **Rust 단독 창 전환 시**: 엔진 라이브러리(`pdf_edit_core`)를 직접 링크하므로, 메모리 상에서 `lopdf::Document` 객체 하나를 열어두고 회전 -> 삭제 -> 순서변경을 메모리 상에서 순차 적용한 뒤, **최종 결과 파일 1회만 디스크에 기록**한다(`save`).
  - 디스크 I/O가 획기적으로 줄어들고(수십 배 속도 향상), 임시 파일 누출 위험이 원천 차단된다.
- **백그라운드 스레드 및 취소 플래그**:
  - 대용량(수백 MB, 수천 쪽) PDF 저장 시 UI 스레드 멈춤 방지를 위해 `std::thread::spawn`으로 작업 격리.
  - `Arc<AtomicBool>` 취소 플래그를 두어 사용자가 [취소] 버튼을 누르면 즉시 루프 중단 및 불완전 파일 삭제(`unlink`).
  - 작업 진행 상태는 `PostMessageW(hwnd, WM_APP_PROGRESS, percent, 0)`로 메인 윈도우에 전달하여 프로그레스바 갱신.

---

## 6. 시험 및 차이 0 확인 계획

### 6.1 기존 시험 자산과 동등성

| 기존 테스트 스위트 | 테스트 수 | 검증 내용 | Rust 전환 시 대응 |
|---|---|---|---|
| `pdf_edit_core/tests/safety.rs` | 6개 | 출력 파일 덮어쓰기 방지, 비정상 범위 거부, 암호화/서명 거부, 상속 회전각 보존 | **그대로 유지 및 100% 통과 필수** |
| `pdf_edit_core/src/lib.rs` | 3개 | 메타데이터 조회, 유효 범위 검증 | **그대로 유지 및 100% 통과 필수** |
| `tests/test_pdf_edit_service.py` | 10개 | CLI 계약, 분류기(`_classify`), 썸네일 캐시 | Rust 단위 시험(`pdf_classify_test.rs`)으로 재작성하여 1:1 동등성 검증 |
| `flutter_ui/test/pdf_edit_workspace_test.dart` | 10개 시나리오 | UI 이벤트 순서, 누적 작업, Undo/Redo, 드래그/키보드 순서 | Rust 무두(Headless) UI 시나리오 시험으로 이식 |

### 6.2 Flutter 대비 차이 0 시나리오 매트릭스 (10종)

새 Win32 앱과 기존 Flutter+Python 앱에 동일한 입력 PDF를 인가하여 최종 생성된 PDF의 바이너리/객체 구조를 대조한다.

```
[입력 PDF] ──┬──> [기존 Flutter + Python + pdf-edit.exe] ──> Output_Flutter.pdf
             │
             └──> [새 Rust Win32 (pdf_edit_win)]          ──> Output_Rust.pdf
                                                                   │
                               [lopdf 구조 비교 도구] <────────────┘
                               (페이지 수, 미디어박스, 회전각, 스트림 해시 검증)
```

1. **시나리오 1 (단일 쪽 삭제)**: 3쪽 문서에서 2번 쪽 삭제 -> 결과 2쪽, 순서 [1, 3] 일치 확인.
2. **시나리오 2 (다중 쪽 회전)**: 1, 3번 쪽 90도 회전 -> `/Rotate` 속성 90도 부여 및 누적 각도 일치 확인.
3. **시나리오 3 (쪽 순서 재배열)**: [1, 2, 3] -> [2, 3, 1] 순서 변경 -> 쪽 객체 참조 순서 일치 확인.
4. **시나리오 4 (복합 누적 작업)**: 쪽 1 회전(90°) -> 쪽 2 삭제 -> 쪽 순서 [3, 1] 재배열 -> 누적 결과 일치 확인.
5. **시나리오 5 (Undo / Redo)**: 회전 후 Undo -> 원본 상태 복원 확인, Redo -> 회전 복원 확인.
6. **시나리오 6 (범위 분할)**: 5쪽 문서를 `1-2, 4-5`로 분할 -> `part-1.pdf`(2쪽), `part-2.pdf`(2쪽) 분할 결과 일치 확인.
7. **시나리오 7 (다중 PDF 합치기)**: A.pdf(2쪽) + B.pdf(3쪽) 순서 변경 합치기 -> 5쪽 단일 문서 객체 트리 일치 확인.
8. **시나리오 8 (안전 가드 - 원본 덮어쓰기)**: 결과 경로를 원본과 같게 지정 시 실행 거부 에러 메시지 확인.
9. **시나리오 9 (보안 가드 - 암호화/서명)**: 암호화 또는 서명된 PDF 인가 시 사전 분류 거부 코드 일치 확인.
10. **시나리오 10 (폴백 가드 - 렌더링 실패)**: 손상 폰트/렌더 불가 쪽 발생 시 번호 카드로 정상 표시되는지 확인.

---

## 7. 단계 계획 · 카드 수 추정 · 위험 3가지

### 7.1 단계 계획 (Phase 1 ~ Phase 6)

- **Phase 1 (본 카드: PDFWIN1)**: PDF 편집 Rust+Win32 전환 설계 문서 작성.
- **Phase 2 (PDFWIN2)**: Python `_classify` 로직의 Rust 이식(`pdf_classify.rs`) 및 인메모리 다중 편집 파이프라인(`pdf_edit_core` 확장).
- **Phase 3 (PDFWIN3)**: WinRT `Windows.Data.Pdf` 썸네일 공급 크레이트 프로토타입 및 번호 카드 폴백 구현.
- **Phase 4 (PDFWIN4)**: `docview_win` 아키텍처 기반의 쪽 격자 가상화, 점선 드롭존, 상단 칩 툴바, 사이드 패널 Win32 단독 창 UI 골격 구현.
- **Phase 5 (PDFWIN5)**: 마우스 드래그 순서 재배열(삽입 마커), 키보드 방향키 이동, Undo/Redo 스택, 비동기 작업 진행률/취소, 합치기 뷰 구현.
- **Phase 6 (PDFWIN6)**: Flutter 대비 차이 0 대조 시험(10종 시나리오 통과), 앱 메뉴(메뉴 11) 기본 실행 연결, Flutter 화면/Python 브리지 제거(되돌리기 스위치 제공).

### 7.2 카드 수 추정

- 설계(1) + 코어/분류(1) + 렌더러(1) + UI 메인/격자(1) + 상호작용/합치기(1) + 통합/대조/전환(1)
- **총 6장 (최대 8장)** 예상. 추정.

### 7.3 핵심 위험 3가지와 완화 방안

1. **위험 1: WinRT `Windows.Data.Pdf`의 Rust 연동 비동기 FFI 복잡도 및 구형 Windows 호환성**
   - *영향*: 특정 윈도우 환경에서 썸네일 생성이 실패하거나 충돌 발생 가능.
   - *완화*: 워커 스레드 격리 및 철저한 `Result` 처리. 렌더 실패 시 즉시 **"번호 카드 (Numbered Card)"**로 폴백하도록 설계하여 편집 기능 전체가 중단되는 일을 원천 방지(3.3절).
2. **위험 2: 인메모리 Document 다중 체이닝과 단일 CLI 다회 실행 간의 객체 ID 매핑 차이(차이 0 위험)**
   - *영향*: 회전 후 삭제된 문서의 내부 번호가 어긋나 의도치 않은 쪽이 삭제되거나 순서가 뒤바뀔 위험.
   - *완화*: `_initialOrder` 및 `_pageOrder` 매핑 로직을 `pdf_edit_workspace.dart:273-308`의 ID 트래킹 알고리즘과 수학적으로 동일하게 Rust 구조체로 구현하고 10종 시나리오 대조 도구(6.2절)로 검증.
3. **위험 3: Win32 순수 GDI 환경에서 드래그 재배열 시의 시각적 피드백 구현 난이도**
   - *영향*: 탐색기나 모던 웹 앱에 비해 드래그 반응이 둔탁하거나 화면 깜빡임 발생.
   - *완화*: `docview_win/src/paint.rs`에 구현된 더블 버퍼링 기법을 재사용하고, 전체 다시 그리기 대신 삽입선 영역만 `InvalidateRect`하는 최소 무효화 기법 적용.

---

## 8. 하지 않는 것 (명시적 비목표)

1. **PDF 본문 텍스트/이미지 직접 수정**: 텍스트 타이핑 수정, 이미지 교체, 폰트 임베딩 등은 포함하지 않는다 (페이지 단위 조작에만 집중).
2. **전자서명 재서명 및 암호화 강제 해제**: 보안 무결성을 위반하는 서명 강제 해제나 암호 무력화는 지원하지 않는다 (보안 검사 즉시 거부).
3. **무거운 외부 C++ DLL(PDFium 등) 번들 추가**: 번들 크기를 수십 MB 늘리는 외부 라이브러리는 추가하지 않으며 OS 내장 기능과 폴백 카드를 우선한다.

---

## 9. 미확인 항목 (U번호 목록)

| 번호 | 항목 | 파일:줄 | 미확인 사유 및 조치 계획 |
|---|---|---|---|
| **U1** | `pdf_edit_workspace.dart`의 점선 드롭존 실제 렌더링 스타일 | `TODO.md:90`, `docs/TODO_AUDIT_2026-11-21.md:45` | TODO에는 '점선 테두리'로 요구되었으나 실제 Flutter 코드는 `Border.all(color: c.lineStrong)` 실선으로 구현되어 있음. Win32 전환 시 `PS_DOT` 점선 펜으로 요구사항 충족 예정. |
| **U2** | 저사양 PC에서 가상 격자 드래그앤드롭 시 GDI 마커 지연 시간 | `docview_win/src/window.rs` | Windows 실기 환경이 없는 Mac 환경이므로 실측 불가. Windows 시험 PC에서 마우스 이동 레이턴시 측정 필요. |
| **U3** | WinRT `Windows.Data.Pdf`의 다양한 Windows 10/11 빌드별 예외 케이스 | `pdf_edit_service.py:183-222` | Mac 환경에서 WinRT API를 직접 로드하여 렌더링 시험을 수행할 수 없음(Windows 실기 카드에서 PoC 검증 필요). |
| **U4** | 단일 작업 시 프로세스 기동(`pdf-edit.exe`) 대 라이브러리 직접 링크의 정확한 속도 향상치 | `pdf_edit_service.py:149` | Mac에서는 `cargo test`만 수행 가능하며 프로세스 기동 오버헤드 실측은 Windows 실기 환경 필요. |
| **U5** | 3,000페이지 최대 한계 도달 시 Win32 GDI 스크롤바 정밀도 한계 | `pdf_edit_core/src/lib.rs:48`, `docview_win/src/layout.rs:96` | 3,000쪽 대형 문서 가상 격자 배치 시 스크롤바 정수 오버플로 여부는 코드상 `i32` 범위 내이나 실문서 검증 필요. |
| **U6** | `test_pdf_edit_service.py` 1건 실패 원인 정규화 | `tests/test_pdf_edit_service.py:152` | macOS 환경의 `/var` -> `/private/var` 심볼릭링크 경로 해석 차이로 인한 기지의 OS 종속 실패임 확인(코드 수정 금지 규칙 준수). |
| **U7** | `desktop_web_ui.py` 외 타 진입점(CLI 또는 트레이)에서의 PDF 편집 호출 여부 | `desktop_web_ui.py:2383` | 현재 저장소 전체 검색 시 `pdf_edit` 명령을 호출하는 곳은 `desktop_web_ui.py`와 `pdf_edit_workspace.dart`뿐이며 타 서비스 호출은 없음 확인. |
