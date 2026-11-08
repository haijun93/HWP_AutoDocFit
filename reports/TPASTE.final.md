# 카드 TPASTE 표그림판: 한/글에서 복사한 표 붙여넣기 구현 완료 보고

- **작업 브랜치**: `feature/tpaste` (작업 디렉터리: `C:/HWP_AUTODOCFIT/v1803/tpaste`)
- **작업 일시**: 2026-11-08
- **관련 요구사항**: `TODO.md` '## [완료·2.0.5] 표그림판 — 한/글 HWPX의 표를 복사해 표그림판에…' 절의 0·1번

---

## 1. 개요 및 작업 요약

한/글 2020 문서에서 표를 복사(Ctrl+C)하여 표그림판에 붙여넣기(단추 클릭 또는 Ctrl+V)할 수 있도록 실기 조사와 기능 구현을 완료하였습니다.
클립보드에 등록되는 11종의 포맷 중 가장 가볍고 정보 손실이 없는 `HTML Format`을 최우선으로 채택하였으며, 순수 파이썬 변환 함수(`docfit_core.table_import.import_html_table`)를 구현하였습니다.
UI 측면에서는 기존 작업 내용이 실수로 덮어써지는 사고를 방지하기 위해 기존 '한/글 문서에서 표 가져오기' 흐름을 재사용하여 **파란 선 겹쳐 보기(overlay preview) → 적용/취소 선택 배너 → 되돌리기(Ctrl+Z) 1단계 보장** 구조를 완성하였습니다.

---

## 2. 세부 진행 단계 및 결과

### 2.1. [0번] Windows PC 한/글 표 클립보드 포맷 조사
- **조사 스크립트 작성**: `scripts/inspect_hwp_table_clipboard.py`
  - 한/글 COM 잠금(`C:\HWP_AUTODOCFIT\.com-test.lock`, 원자적 생성 및 해제, 오래된 stale 락 자동 감지) 적용.
  - 실행 중인 사용자 한/글 프로세스를 절대 건드리지 않고, 독립된 `DispatchEx("HwpFrame.HwpObject")` 인스턴스로 격리 실행.
  - 2×2 표(배경색 `#E2EFDA`, 이중선/굵은선 테두리, 텍스트 정렬, 굵게 서식 포함)를 로드하여 복사(`Copy`).
  - 사용 가능한 모든 클립보드 형식 열거 및 메타데이터/내용 추출 후 인스턴스 및 락 정리.
- **결과 기록**: `docs/TABLE_PASTE_FORMATS.md`
  - 총 11개 포맷 감지 (`DataObject`, `Hwp_Native_Info`, `Hwp Native`, `DOCX Format`, `Rich Text Format`, `HTML Format`, `CF_UNICODETEXT`, `CF_TEXT`, `Ole Private Data`, `CF_LOCALE`, `CF_OEMTEXT`).
  - `HTML Format`이 셀 폭/높이(pt), 셀 병합(`colspan`/`rowspan`), 4변 테두리 굵기/선종류/색상, 셀 배경색, 텍스트 정렬/굵게 서식을 완벽하게 보존하고 있음을 확인.
  - COM 기동 없이 표준 라이브러리(`html.parser`)만으로 가장 가볍고 빠르게 처리할 수 있어 최우선 읽기 포맷으로 선정.

### 2.2. [1번 & 변환기] 순수 파이썬 HTML 표 → sketch 변환기 구현
- **모듈**: `docfit_core/table_import.py`
  - `import_html_table(html, cell_mm=5.0) -> tuple[dict, dict]` 순수 함수 구현.
  - `html.parser.HTMLParser`를 사용한 표준 파서 기반(`_HTMLTableParser`).
  - pt 단위를 격자 칸 수(`cell_mm`)로 변환 시 행은 올림(글자 겹침 방지), 열은 반올림 적용.
  - 선 굵기(`thin`, `thick`, `double`), 선 모양(`solid`, `dash`, `dot`), 선 색상, 셀 배경색(`fills`), 텍스트 및 정렬(`texts`) 변환.
  - 반환 규격: `(sketch, info)` 튜플 (`sketch`: grid, lines, texts, images, fills / `info`: rows, cols, warnings).
- **서비스 및 기존 호환**:
  - `docfit_core/hwp_table_import.py`: `sketch_from_html`이 `import_html_table`을 호출하도록 단일화.
  - `docfit_app/table_sketch_service.py`: `paste_hwp`에서 `import_html_table`을 호출하여 `warnings`를 포함한 응답 반환.

### 2.3. 표그림판 UI 및 사용자 흐름 구현
- **파일**: `flutter_ui/lib/features/table_sketch/table_sketch_screen.dart`
  - 상단 툴바에 **'붙여넣기'** 단추 (`sketchPasteHwp`, tooltip: `한/글 표 붙여넣기 (Ctrl+V)`) 배치.
  - `Ctrl+V` 단축키 입력(`_pasteKey`) 연동:
    - 클립보드에 한/글 표가 있으면 한/글 표 붙여넣기 겹쳐 보기 실행.
    - 클립보드가 변경되지 않았거나 한/글 표가 없고 그림판 내부 복사본이 있으면 내부 복사본 붙여넣기 실행.
  - **겹쳐 보기 및 안전 흐름**:
    - 붙여넣기 성공 시 바로 덮어쓰지 않고 `_recognized = found;`, `_pastingHwp = true;` 설정.
    - 캔버스 위에 파란 선 오버레이(`_buildRecognizedOverlay`) 표시.
    - 상단에 안내 배너(`_buildRecognizeBanner`): "붙여넣은 표: N행 × M열 (파란 선) / 적용하면 지금 그림이 바뀝니다. 되돌리기로 돌아갈 수 있어요." 및 `[취소]` / `[적용]` 단추 노출.
    - `[취소]` 클릭 시 겹쳐 보기를 해제하고 원래 편집 화면 유지.
    - `[적용]` 클릭 시 스냅샷 저장(`_saveSnapshot`) 후 캔버스에 적용, 이후 `Ctrl+Z`로 언제든 이전 상태로 되돌리기 가능.

---

## 3. 검증 및 테스트 결과

1. **Python 관련 테스트**:
   - 실행: `& "C:/Users/haiju/OneDrive/바탕 화면/HWP_AutoDocFit/mapo-agent-1/.venv/Scripts/python.exe" -m unittest tests/test_table_import.py tests/test_hwp_table_import.py tests/test_table_sketch.py`
   - 결과: **53개 테스트 전부 PASS (OK)**.
2. **Flutter 위젯 테스트**:
   - 신규 테스트 작성: `flutter_ui/test/table_sketch_paste_test.dart` (단추 클릭, 겹쳐 보기, 취소, 적용, 되돌리기, Ctrl+V 단축키, 클립보드 빈 상태 안내 3건 검증).
   - 전체 테스트 실행: `cmd /c "%LOCALAPPDATA%\DocFitTools\flutter\bin\flutter.bat test --no-pub"`
   - 결과: **498개 테스트 전체 PASS**.
3. **Flutter 코드 분석 (Analyze)**:
   - 실행: `cmd /c "%LOCALAPPDATA%\DocFitTools\flutter\bin\flutter.bat analyze --no-pub"`
   - 결과: **No issues found! (0건)**.
4. **규칙 준수 확인**:
   - 작업 폴더 `C:\HWP_AUTODOCFIT\v1803\tpaste` 내에서만 작업 완료.
   - XML 파싱은 `defusedxml`만 사용.
   - `hwp-auto-docfit.py`는 수정하지 않음 (CRLF 및 원본 무결성 유지).
   - `kagent` 변경 없음.
   - 원격 푸시 미실시 (커밋만 진행).
