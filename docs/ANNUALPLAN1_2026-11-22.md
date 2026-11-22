# 카드 ANNUALPLAN1 — 업무 연보 남은 개발 현황 조사와 단계별 카드 계획 (2026-11-22)

## 1. 개요

- **목적**: 2026-11-14 사용자 요청에 따른 '업무 연보'(문서 작업 통계, 유사도 기반 주제 묶기, 월·주 통계, 열지도, 타임라인, 버전 줄, 정기 업무 탐지, 인수인계·실적 HWPX 보고서, 엑셀) 기능의 현재 코드·시험·화면 상태를 전수 조사하고, 남은 작업을 시험 가능한 1카드 단위로 체계화한다.
- **기준 브랜치**: `github/main`
- **조사 대상 모듈**:
  - Rust 코어: `kfind/annual/` (`lib.rs`, `norm.rs`, `sim.rs`, `cluster.rs`, `series.rs`, `stats.rs`, `date.rs`, `document_date.rs`, `history.rs`, `output.rs`, `main.rs`)
  - HWPX 부품: `docfit_core/hwpx/` (`__init__.py`, `canvas_hwpx_builder.py`, `canvas_package_adapter.py`), `docfit_core/notebook_hwpx.py`, `docfit_core/report_header.py`, `docfit_core/excel_table_hwpx.py`
  - 기존 설계 문서: `TODO.md`, `docs/ANNUAL_A1_2026-11-14.md`, `docs/ANNUALUI_GAP_2026-11-18.md`, `docs/ANNUAL_REPORT_SCHEMA_2026-11-18.md`

---

## 2. A1~A5 각 단계별 현재 구현 상태

| 단계 | 단계명 및 핵심 내용 | 상태 | 코드 근거 | 시험 근거 | 화면/앱 연결 근거 |
|---|---|:---:|---|---|---|
| **A1** | **연보 분석 엔진**<br>- 파일명 정규화<br>- 제목·중소제목 2글자/낱말 가중 유사도<br>- MinHash+LSH 후보 추리기<br>- Union-Find 주제 묶기<br>- 버전 줄 추출<br>- 정기 업무 탐지(월/분기/반기/연간)<br>- 기초 통계 및 집중 시기 | **구현**<br>(엔진 완성,<br>앱 연동 미구현) | `kfind/annual/src/lib.rs:137` (`analyze`)<br>`norm.rs:27` (`normalize_name`)<br>`sim.rs:194` (`calculate_weighted_similarity`)<br>`cluster.rs:77` (`cluster_topics`)<br>`series.rs:36,72` (`detect_version_lines`, `detect_periodic_task_for_group`)<br>`stats.rs:49` (`calculate_stats`)<br>`document_date.rs:163` (`extract_document_dates`)<br>`history.rs:110` (`detect_periodic_topics`) | `kfind/annual` 단위 테스트 27개 전원 통과<br>- `norm::tests::test_normalize_name_10_cases`<br>- `sim::tests::test_threshold_calibration_20_pairs`<br>- `cluster::tests::test_cluster_topics`<br>- `series::tests::test_periodic_task_monthly`<br>- `stats::tests::test_stats_and_crunch_weeks` | GUI/앱 연동 없음.<br>독립 CLI만 존재:<br>`kfind/annual/src/main.rs:7`<br>(`annual report <JSON> <폴더>`)<br>`crates/kfind` IPC 및 설치 패키지 미포함 |
| **A2** | **타임라인 및 통계 시각화**<br>- 월별 문서수 막대 그래프<br>- 주차×요일 작업 열지도<br>- 주제별 월 타임라인<br>- 타임라인 CSV | **부분**<br>(SVG·CSV 완성,<br>화면/HWPX 미구현) | `kfind/annual/src/output.rs:294` (`monthly_documents_svg`)<br>`output.rs:316` (`weekday_heatmap_svg`)<br>`output.rs:342` (`topic_timeline_svg`)<br>`output.rs:172` (`topic_timeline_csv`)<br>`output.rs:234` (`weekly_topics_csv`)<br>`output.rs:261` (`weekday_heatmap_csv`) | `output::tests` 통과<br>- `chart_svgs_are_balanced_and_empty_input_is_valid`<br>- `monthly_bars_scale_proportionally_and_svg_topic_text_is_escaped`<br>- `timeline_csv_contains_topic_month_rows_by_year`<br>- `weekly_and_weekday_csv_include_headers_and_statistics` | 화면 표시 뷰어/위젯 0건.<br>HWPX 내부 차트/이미지 임베딩 파이프라인 없음. |
| **A3** | **인수인계 보고서**<br>- 주제별 대표 문서 및 비중<br>- 업무 이력 요약<br>- 다음 예정일 및 잔여 업무<br>- 인수인계 마크다운·JSON 출력 | **부분**<br>(MD·JSON 완성,<br>HWPX 미구현) | `kfind/annual/src/output.rs:58` (`render_markdown`)<br>`output.rs:407` (`render_report_json`)<br>`output.rs:646` (`write_outputs` 내 `인수인계_연보.md`)<br>`docs/ANNUAL_REPORT_SCHEMA_2026-11-18.md` | `output::tests` 통과<br>- `markdown_has_sections_and_period_comparison`<br>- `markdown_reports_monthly_counts`<br>- `test_report_json_korean_and_escaping`<br>- `test_write_report_json_atomic_replacement` | 인수인계 **HWPX 보고서 생성기 전무**.<br>화면 내 미리보기 및 파일 생성 UI 없음. |
| **A4** | **실적 보고서 (HWPX)**<br>- 연간 실적 총괄 서식<br>- 월별 추진 실적 표<br>- 업무 비중 차트/열지도 삽입<br>- HWPX 패키징 및 서식 프로필 적용 | **미구현** | `docs/ANNUAL_REPORT_SCHEMA_2026-11-18.md` (스키마 명세만 존재)<br>실제 HWPX 생성 모듈 코드 0건 | HWPX 생성 관련 테스트 0건 | 화면 내 '실적 보고서 생성' UI 및 HWPX 내보내기 대화상자 전무 |
| **A5** | **엑셀(Excel) 및 앱 화면 연결**<br>- 엑셀 호환 데이터 출력<br>- 앱 화면(GUI) 연동<br>- 기간/폴더 선택 대화상자<br>- 결과 시각화 및 내보내기 단추 | **부분**<br>(CSV 6종 완성,<br>.xlsx 및 UI 미구현) | `kfind/annual/src/output.rs:147-292` (UTF-8 BOM CSV 6종: `monthly_topics_csv`, `topic_timeline_csv`, `weekly_topics_csv`, `weekday_heatmap_csv`, `periodic_tasks_csv`, `folder_history_csv`) | `output::tests` 통과<br>- `files_have_utf8_bom`<br>- `weekly_and_weekday_csv_are_written_with_utf8_bom`<br>- `monthly_csv_quotes_comma_quote_newline` | `desktop_web_ui.py`, `flutter_ui/`, `dock/`, `ktm/` 어디에도 연보 메뉴·화면·버튼 연결 코드 0건.<br>단일 통합 워크북(`.xlsx`) 생성기 부재. |

---

## 3. 사용자 요구 문장 체크리스트 및 세부 항목 상태

> **사용자 요구 원문 (2026-11-14)**:
> *"기간 내 문서의 파일명·제목·중제목·소제목 유사도로 주제 묶기 → 월·주 통계 그래프·주간 열지도·주제 타임라인·버전 줄·정기 업무 탐지 → 인수인계·실적 보고서(HWPX)·엑셀, A1 엔진~A5 순"*

| 번호 | 요구 항목 | 상태 | 구현 코드 (파일:줄) | 검증 테스트 (파일:줄) | 비고 |
|:---:|---|:---:|---|---|---|
| 1 | **기간 내 문서 필터링** | **구현** | `kfind/annual/src/lib.rs:144-150`<br>`kfind/annual/src/date.rs:163-176` | `kfind/annual/src/stats.rs:271-291` (`test_period_filter`)<br>`date.rs:247-259` (`test_period_contains`) | 작업일(`modified`) 기준으로 기간 포함 여부 판정 |
| 2 | **파일명·제목·중제목·소제목 유사도로 주제 묶기** | **구현** | `kfind/annual/src/norm.rs:27-86`<br>`kfind/annual/src/sim.rs:194-250`<br>`kfind/annual/src/cluster.rs:77-128` | `kfind/annual/src/norm.rs:140-192`<br>`sim.rs:374-453`<br>`cluster.rs:159-204` | 2글자 조각 + 낱말 가중 Dice(불용어 0.1, 핵심어 1.0), 파일명 0.4 + 제목 0.4 + 중소제목 0.2 가중합 |
| 3 | **월 통계 그래프** | **부분** | `kfind/annual/src/stats.rs:59-71`<br>`kfind/annual/src/output.rs:294-314` | `kfind/annual/src/output.rs:793-810`<br>`output.rs:812-841` | 월별 막대 그래프 SVG 생성 및 월별 CSV 생성 완료. 화면 표시 위젯 및 HWPX 삽입 미구현 |
| 4 | **주 통계 그래프** | **부분** | `kfind/annual/src/stats.rs:73-83`<br>`kfind/annual/src/output.rs:234-259` | `kfind/annual/src/output.rs:982-1010` | 1~53주차 통계 및 주차별 CSV 출력 완료. 주별 독립 SVG 차트 및 화면 표시 미구현 |
| 5 | **주간 열지도 (주차×요일)** | **부분** | `kfind/annual/src/stats.rs:85-103`<br>`kfind/annual/src/output.rs:261-292`<br>`output.rs:316-340` | `kfind/annual/src/output.rs:793-810`<br>`output.rs:982-1010`<br>`stats.rs:313-339` | 주차×요일 열지도 SVG 및 CSV 출력 완료. 화면 위젯 및 HWPX 삽입 미구현 |
| 6 | **주제 타임라인** | **부분** | `kfind/annual/src/output.rs:172-190`<br>`output.rs:342-373`<br>`output.rs:556-613` | `kfind/annual/src/output.rs:793-810`<br>`output.rs:957-980` | 주제별 월 타임라인 SVG, CSV, JSON 출력 완료. 화면 위젯 및 HWPX 삽입 미구현 |
| 7 | **버전 줄 (Version Line)** | **구현** | `kfind/annual/src/series.rs:36-70` | `kfind/annual/src/series.rs:163-189` (`test_version_line`) | 동일 주제 내 `name_norm` 일치 문서 작성일 오름차순 정렬 추적 |
| 8 | **정기 업무 탐지** | **구현** | `kfind/annual/src/series.rs:72-126`<br>`kfind/annual/src/history.rs:110-161` | `kfind/annual/src/series.rs:136-161`<br>`history.rs:282-315` | 3회 이상 작성 간격 중앙값(월간 30±7, 분기 91±15, 반기 182±20, 연간 365±30) 기반 다음 예상일 계산 |
| 9 | **인수인계 보고서** | **부분** | `kfind/annual/src/output.rs:58-145`<br>`output.rs:407-630` | `kfind/annual/src/output.rs:728-761`<br>`output.rs:885-930` | Markdown 및 JSON 데이터 구조 생성 완료. 인수인계 **HWPX 보고서 생성기 미구현** |
| 10 | **실적 보고서 (HWPX)** | **미구현** | `docs/ANNUAL_REPORT_SCHEMA_2026-11-18.md` | 없음 | 데이터 스키마만 존재하고, HWPX 문서 생성·서식 적용·패키징 코드 전무 |
| 11 | **엑셀 통계** | **부분** | `kfind/annual/src/output.rs:654-665` | `kfind/annual/src/output.rs:1012-1038` | Excel 호환 UTF-8 BOM CSV 6종 출력 완료. 단일 통합 워크북(`.xlsx`) 파일 생성기 미구현 |
| 12 | **A1 엔진~A5 순 앱 화면 연결** | **미구현** | `kfind/annual/src/main.rs:1-17` | 없음 | CLI 배치 도구만 존재하며, 데스크톱 UI/창 연결 0건 |

---

## 4. 남은 작업의 단계별 카드 계획 (1카드 = 시험 가능한 작은 단위)

모든 카드는 독립적으로 빌드 및 시험 가능하도록 분할하였다.

### [카드 1] `ANNUAL_A2_WEEKLY_CHART` — 주차별 추이 SVG 차트 추가
- **내용**: 월별 막대 SVG(`monthly_documents_svg`)에 대응하여, 연간 1~53주차 문서 작업량 추이를 시각화하는 `weekly_documents_svg(input: &AnnualInput) -> String` 함수 추가 및 출력 폴더 내 `주차별_문서수.svg` 저장 연결.
- **완료 조건**:
  - `weekly_documents_svg` 구현 및 빈 입력/정상 입력 시 올바른 SVG 태그·치수·스케일 반환.
  - `cargo test --manifest-path kfind/annual/Cargo.toml` 통과.
- **의존**: 기존 `kfind/annual` (A1/A2)
- **예상 규모**: 소 (Rust 약 120줄)
- **워커 종류**: 맥 가능 (순수 Rust)

### [카드 2] `ANNUAL_A4_HWPX_BUILDER` — 업무 연보 인수인계·실적 HWPX 생성기 골격
- **내용**: `docs/ANNUAL_REPORT_SCHEMA_2026-11-18.md`의 JSON(또는 `AnnualInput`)을 읽어 인수인계서 및 실적 보고서 HWPX 문서를 생성하는 모듈 `docfit_core/annual_hwpx.py` 구현.
  - `docfit_core/notebook_hwpx.py`와 `canvas_package_adapter.py`의 구조를 준용하여 기본 서식 HWPX를 복제·변환.
  - 포함 섹션: 보고서 제목, 분석 기간 요약 메타, 상위 주제 통계 표, 월별 작업 통계 표, 정기 업무/현안 표.
- **완료 조건**:
  - `python -m unittest tests/test_annual_hwpx.py` 통과.
  - 생성된 HWPX의 ZIP 구조 무결성, `header.xml`, `section0.xml` 내 유효한 HWPX 네임스페이스 및 태그 구조 확인.
  - 외부 COM 없이 순수 `defusedxml` 및 `zipfile`로 안전하게 빌드.
- **의존**: `docs/ANNUAL_REPORT_SCHEMA_2026-11-18.md`, `docfit_core/hwpx/canvas_package_adapter.py`
- **예상 규모**: 중 (Python 약 350줄)
- **워커 종류**: 맥 가능 (순수 Python)

### [카드 3] `ANNUAL_A4_HWPX_HEATMAP` — HWPX 표 기반 주간 열지도 및 타임라인 렌더러
- **내용**: 한/글 HWPX 내에서 깨짐 없이 안정적으로 동작하도록, 주차×요일 열지도와 타임라인을 '순수 한/글 표(Table)'로 렌더링하는 빌더 구현.
  - 주차×요일 열지도: 53행×7열의 컴팩트 표를 생성하고 작업량 강도에 따라 셀 배경색(`borderFill`의 `fillBrush`)을 단계별(연파랑~진파랑)로 부여.
  - 타임라인: 주요 주제별 1~12월 격자 셀에 작업 유무에 따른 음영 색상 부여.
- **완료 조건**:
  - 열지도/타임라인 표가 포함된 HWPX 문서 생성 및 단위 테스트 통과.
  - 셀 너비·높이가 본문 폭(170mm) 내에 맞게 수학적으로 자동 분할되는지 검증.
- **의존**: `ANNUAL_A4_HWPX_BUILDER`, `docfit_core/excel_table_hwpx.py`
- **예상 규모**: 중 (Python 약 250줄)
- **워커 종류**: 맥 가능 (순수 Python)

### [카드 4] `ANNUAL_A5_XLSX_BUILDER` — 6종 통계 통합 엑셀(.xlsx) 생성기
- **내용**: 현재 6개의 개별 CSV 파일로 분리된 통계 데이터를 1개의 통합 워크북 파일(`업무연보_통계.xlsx`)로 묶어주는 순수 Python 경량 생성기 구현.
  - 시트 구성: 1. 월별통계, 2. 주차별통계, 3. 요일열지도, 4. 주제타임라인, 5. 주기역무, 6. 폴더이력.
  - 외부 무거운 패키지 없이 Python 내장 ZIP+XML 조작(OpenXML 표준 준수) 또는 경량 의존성으로 구현.
- **완료 조건**:
  - `tests/test_annual_xlsx.py` 통과. 생성된 `.xlsx`가 표준 ZIP 패키지이며 각 시트 XML에 정확한 데이터가 포함되었는지 확인.
- **의존**: `kfind/annual` CSV 데이터 사양
- **예상 규모**: 중 (Python 약 250줄)
- **워커 종류**: 맥 가능 (순수 Python)

### [카드 5] `ANNUAL_A1_SCAN_CLI` — 로컬 문서 메타데이터 수집 및 연보 입력 변환기
- **내용**: 지정된 작업 폴더(또는 업무자료함)를 스캔하여 HWPX/HWP/PDF 문서들의 파일 메타데이터(경로, 생성일, 수정일) 및 제목·중소제목을 추출하고 `Doc` 목록(`AnnualInput`) JSON을 생성하는 수집 명령 구현 (`annual scan <폴더> -o input.json`).
  - HWPX 본문 첫 문단/제목 추출기 및 파일 시스템 타임스탬프 연계.
- **완료 조건**:
  - 임시 폴더 내 샘플 문서 5종 스캔 후 `Doc` 목록 JSON 정상 생성 단위 테스트 통과.
- **의존**: `kfind-core`, `kfind/annual`
- **예상 규모**: 중 (Rust 약 250줄)
- **워커 종류**: 맥 가능 (순수 Rust)

### [카드 6] `ANNUAL_A5_UI_DIALOG` — 데스크톱 앱 내 업무 연보 화면 및 내보내기 UI 연결
- **내용**: 사용자가 연보 기능을 직접 호출할 수 있도록 데스크톱 UI에 '업무 연보' 진입점과 실행 대화상자 추가.
  - UI 구성: 분석 대상 폴더 선택, 기간 범위 지정(기본값: 최근 1년), 분석 실행 버튼, 요약 뷰(상위 5대 주제, 집중 주차 안내), 'HWPX 인수인계서 생성', 'HWPX 실적보고서 생성', '엑셀 통합본 저장' 버튼.
- **완료 조건**:
  - UI 컴포넌트 렌더링 및 클릭 이벤트 핸들러 모의(mock) 테스트 통과.
- **의존**: `ANNUAL_A4_HWPX_BUILDER`, `ANNUAL_A5_XLSX_BUILDER`, `ANNUAL_A1_SCAN_CLI`
- **예상 규모**: 중 (Flutter 또는 Win32 Rust 약 300~400줄)
- **워커 종류**:
  - Flutter UI 구현 시: 맥 가능
  - Win32 GDI 경량 창(`annual_win`) 구현 시: PC Rust 워커 필요

### [카드 7] `ANNUAL_A4_WIN_VERIFY` — Windows 한/글 2020 COM 환경 실기 검증
- **내용**: Windows + 한/글 2020 실기 환경에서 생성된 `인수인계서.hwpx` 및 `실적보고서.hwpx`를 실제 한/글로 열람하여 레이아웃 무결성 검증.
  - 검증 항목: 한/글 COM 열기 오류 여부, 쪽 수 및 줄바꿈 적합성, 열지도 표 깨짐 여부, 여백 준수.
- **완료 조건**:
  - 한/글 2020 COM 자동 열기 스크립트 실행 성공(오류 코드 0), 쪽 수 1쪽 이상 산출 확인.
  - `C:\HWP_AUTODOCFIT\.com-test.lock` 파일 잠금 규칙 준수.
- **의존**: `ANNUAL_A4_HWPX_BUILDER`, `ANNUAL_A4_HWPX_HEATMAP`
- **예상 규모**: 소 (Python 검증 스크립트 약 100줄)
- **워커 종류**: **PC Windows 필수 (한/글 2020 COM 필요, 맥 불가)**

---

## 5. 인수인계·실적 보고서 HWPX 출력에 쓸 수 있는 기존 부품과 부족한 것

### 5.1. 쓸 수 있는 기존 부품

1. **`docfit_core/hwpx/canvas_package_adapter.py`**
   - **기능**: COM 없이 원본 HWPX 패키지(`기본 서식.hwpx`)를 읽어 XML 트리를 분해하고, `header.xml`의 스타일 ID(`borderFills`, `charProperties`, `paraProperties`) 충돌 없이 전역 번호를 재할당하며, `section0.xml`에 새로운 문단/표를 안전하게 병합·저장.
   - **활용**: 인수인계서 및 실적 보고서 HWPX 조립의 최하단 무결성 패키징 엔진으로 그대로 사용 가능.

2. **`docfit_core/hwpx/canvas_hwpx_builder.py`**
   - **기능**: `_element`, `_paragraph`, `_table`, `_cell`, `_border` 등 한컴 표준 HWPX XML 노드를 안전하게 생성하는 저수준 함수군.
   - **활용**: 보고서의 제목 문단, 설명 텍스트, 요약 통계 표의 XML 엘리먼트를 생성할 때 직접 호출 가능.

3. **`docfit_core/notebook_hwpx.py`**
   - **기능**: 업무자료함 노트를 독립 HWPX 파일로 완성하는 완전한 파이프라인(`STYLES` 폰트/크기 풀, 인용문, 본문, 비교 표, 임시 파일 기록 후 `defusedxml` 재검증).
   - **활용**: `annual_hwpx.py`를 구현할 때 아키텍처 및 검증 절차의 표준 참조 모델(Blueprint)로 활용.

4. **`docfit_core/report_header.py`**
   - **기능**: 공공기관 및 기업 보고서 머리 서식(제목, 기관명, 보고 일자, 결재란 등) 분석 및 생성 모듈.
   - **활용**: 실적 보고서 1페이지 상단의 정형화된 보고서 머리(결재선, 제목 블록) 생성에 활용.

5. **`docfit_core/excel_table_hwpx.py`**
   - **기능**: 임의 열 수의 표를 한/글 본문 인쇄 폭(`BODY_WIDTH_MM = 170.0mm`)에 맞추어 열 너비 비율을 비례 배분하고 넘침을 방지하는 알고리즘.
   - **활용**: 12개월 월별 통계 표나 7열 요일 표를 본문 폭에 꼭 맞게 자동 조판할 때 재사용.

### 5.2. 부족한 부품 및 구현 필요 사항

1. **업무 연보 전용 HWPX 보고서 서식 프로필 및 템플릿**:
   - 인수인계서 전용 레이아웃(총괄 요약, 담당 업무 목록, 현안 및 다음 일정, 주요 참고 파일 경로).
   - 실적 보고서 전용 레이아웃(연간 실적 총괄, 월별 추진 현황, 주요 성과).
   - 현재는 데이터 스키마(`ANNUAL_REPORT_SCHEMA_2026-11-18.md`)만 정의되어 있고 HWPX 템플릿/레이아웃 매핑 모듈이 전무함.
2. **HWPX 이미지/차트 임베더**:
   - 한/글 2020 HWPX는 SVG 파일을 직접 벡터로 렌더링하지 못하거나 오류를 일으킬 수 있음.
   - SVG를 PNG로 변환하여 HWPX `BinData`에 포함시키거나, 열지도/타임라인을 한/글 '순수 표(Table)'로 변환하여 삽입하는 모듈이 필요함 (안전성을 위해 표 기반 렌더링 권장).
3. **Rust `kfind/annual`과 Python HWPX 빌더 간의 통합 브리지**:
   - Rust CLI가 출력한 JSON을 받아 Python HWPX 생성기를 구동하는 상위 실행 스크립트 또는 Rust 내 직접 패키징 파이프라인 연결 부재.

---

## 6. 사용자 결정이 필요한 사항

1. **보고서 서식 양식 및 디자인 기준**:
   - **(추천) 기본 서식 기반 심플 공공 서식**: `기본 서식.hwpx`의 여백/글꼴(한컴돋움 제목 16pt, 휴먼명조 본문 11pt, 표 머리 `#E7EEF8` 음영, 본문 폭 170mm)을 적용하여 깔끔하고 표준적인 보고서로 자동 생성.
   - **대안**: 사용자가 미리 만들어둔 보고서 양식 템플릿(`.hwpx`) 파일이 있으면 그 템플릿 내 지정 태그(예: `{{월별통계}}`, `{{주요주제}}`)를 치환하는 템플릿 바인딩 방식 채택 여부.
2. **열지도/타임라인 시각화의 HWPX 내부 표현 방식**:
   - **(추천) 순수 한/글 표(Table) 방식**: 주차×요일 열지도와 타임라인을 한/글 네이티브 표로 렌더링하고 셀 배경색을 단계별로 채움. 외부 의존성(PNG 변환기 등)이 일절 없고 한/글 인쇄 시 가장 선명하며 확대해도 깨지지 않음.
   - **대안**: SVG를 PNG 비트맵으로 변환하여 `<hp:pic>` 그림 개체로 삽입하는 방식 (외부 변환 도구 필요).
3. **주제 묶기(클러스터링) 임계치 및 사용자 조정 UI 제공 여부**:
   - 현재 기본 유사도 임계치는 `0.55` (파일명 0.4 + 제목 0.4 + 중소제목 0.2).
   - 사용자 문서 파일명 규칙에 따라 너무 잘게 쪼개지거나 너무 크게 뭉치는 경우가 있을 수 있음.
   - UI에 '주제 묶기 민감도(엄격/보통/느슨)' 옵션을 노출할 것인지, 아니면 내부 자동 임계치(`0.55`)로 고정할 것인지 결정 필요.
4. **최종 산출물 형태**:
   - 현재는 폴더에 마크다운 1건, JSON 1건, SVG 3건, CSV 6건이 개별 파일로 생성됨.
   - 사용자 최종 산출물로 ① '인수인계서.hwpx' + '실적보고서.hwpx' + '통계.xlsx' 3종으로 깔끔하게 정리해 제공할 것인지, ② 원본 중간 데이터(CSV/SVG/JSON)도 함께 보존할 것인지 결정 필요.
5. **UI 진입점 위치**:
   - 독립 경량 창(예: Win32 `annual_win.exe`) 형태 vs 메인 앱(Flutter/Web) 화면 탭 형태 결정 필요.

---

## 7. 확인하지 못한 점 (미확인)

1. **[미확인] Windows 한/글 2020 COM 환경에서의 최종 HWPX 레이아웃 적합성**:
   - 본 작업 환경은 macOS 환경이므로, Windows 한/글 2020 COM을 통한 실제 HWPX 문서 열람, 쪽 나눔, 셀 여백 시각적 확인을 수행할 수 없음 (규칙 준수).
2. **[미확인] 대규모 문서(1만 건 이상) 대상 MinHash+LSH 클러스터링의 메모리/실행 시간**:
   - 현재 테스트는 수십~수백 건의 합성 문서 데이터를 기준으로 통과함. 수만 건 규모의 실제 로컬 문서 색인 대상 실행 시의 레이턴시 및 메모리 사용량은 실기 벤치마크 전까지 미확인.
3. **[미확인] 최종 릴리스 패키지 내 `annual.exe`의 배포 형태**:
   - `build_native.ps1` 및 설치본 스크립트에 `annual.exe`가 단독 등록될지, 혹은 `kfind.exe annual ...`과 같은 하위 서브커맨드로 통합될지 최종 패키징 정책 미확인.
