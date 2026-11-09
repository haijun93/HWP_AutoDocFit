# KIDX1 Rust 본문 색인 전환

> 기준: `github/main` v2.2.48, 작업 브랜치 `feature/klib-index-rust`  
> 측정/검증은 로컬 Windows 환경에서 진행. HWP COM 문서 시험은 실행하지 않음.

## 진행 요약

요청된 다섯 단위를 순서대로 기록한다. 완료하지 않은 단위는 결과를 만들지 않은 이유와 현재 상태를 분명히 남긴다. 각 단위는 구현/검증 후 별도 커밋 및 `github` 우선, `origin` 후순으로 푸시한다.

## 1. 현재 색인 흐름과 기준 측정

### Python 색인 경로

| 단계 | 함수/파일 | 동작 |
|---|---|---|
| 파일 열거/작업 분배 | 타임머신 본문 색인 호출부 (`docfit_core/timemachine`) | 지원 확장자와 파일 상태를 검사하고 `ContentSearchIndex.index_file(path)` 호출 |
| 재색인 판정 | `ContentSearchIndex.index_file` (`content_search.py`) | 크기·mtime·hierarchy schema version, 기존 문단 존재 여부 확인 |
| 형식별 추출 | `extract_paragraphs` → `extract_document` (`extractors.py`, `extractors_text.py`, `extractors_hwpx.py` 등) | TXT/MD/HWPX 외 PDF·Office 등 현재 지원 형식을 각 추출기로 정규화. HWPX는 안전한 ZIP/XML 추출 |
| 기존 위계 입력 준비 | `extract_paragraphs_legacy` | HWPX/DOCX 스타일·개요 정보를 추출해 위치 추출 결과에 합침 |
| 위계 판정 | `classify_paragraph_levels` → `docfit_app.hierarchy_bridge.analyze_levels` | 문단 텍스트를 엔진으로 전달하고 결과를 level/rank로 변환. 미사용/실패 시 규칙 기반 결과 유지 |
| 위치 메타데이터 | `extract_document` 및 `_EXTRACTED_LOCATIONS` | 문단 순번, 위치 라벨, 섹션을 경로별 메모리에 보관 |
| 색인 저장 | `ContentSearchIndex.index_file` | SQLite `content_paragraph_fts`에 문단별 level/rank/body/location 저장, `content_fts`에 합친 본문, `content_short_grams`에 2글자 후보, `content_meta`에 파일 메타데이터 저장 |
| 검색 | `ContentSearchIndex.search`, `keyword_search` | FTS5 후보 집합·SQLite 점수와 후처리로 검색 결과 및 위치를 구성 |

현재 본문 검색 저장소는 SQLite FTS5 trigram이며, `klib`의 Tantivy는 자료함(opt-in library)용 `SearchIndex`다. Python 타임머신 검색 인덱스를 직접 대체하지 않는다.

### 작업 트리에서 확인한 기준 데이터

- `tests/fixtures`: 46개 파일, 총 233,934 bytes (약 228.4 KiB); `.hwpx` 16, `.txt` 12, `.cf_html` 7, `.md` 2, `.json` 2, `.pdf/.xlsx/.pptx/.eml/.cs/.py/.docx` 각 1.
- 합성 300문서 기준 측정은 아직 수행하지 않음.
- 색인 벽시계 시간 및 메모리 측정은 아직 수행하지 않음. 지시한 venv Python 실행 파일은 존재함. fixtures 중 일부는 현재 본문 색인의 비지원 확장자이므로 성공 문서 수와 실패 기록을 함께 측정해야 함.
- 기준 측정을 재현 가능하게 하려면 현재 인덱스용 임시 DB 경로와 fixture·합성 데이터 그룹을 도구 CLI에 명시하고, 성공/오류/문단 수, elapsed time, peak RSS를 기록해야 한다.

### 구현 영향 범위

요청된 Rust CLI 및 Tantivy 문서 색인은 Python의 SQLite FTS5 스키마, 검색어 파싱/순위/스니펫 규칙과 저장 형식이 다르다. 두 경로의 문단 수·레벨·검색 상위 20이 같더라도 기존 Python 앱이 SQLite를 읽으므로, 앱 전환에는 조회 경로의 공유/이중 쓰기 또는 검색 어댑터가 필요하다. 색인 결과 비교만으로 앱에서 색인을 사용할 수 있음이 증명되지는 않는다.

## 2. hierarchy compat 직접 연결

상태: 대기. `github/feature/hier-engine`의 최신 tip은 compat 구현 자체를 포함하지 않고 이전 이식 커밋을 후속 정리로 변경/제거한 상태다. 요청된 `analyze_hierarchy` 1:1 이식본의 파일과 API를 찾아 원격 브랜치의 변경을 정확히 가져오고, 현재 `hierarchy` crate 및 klib 테스트와 충돌 없이 통합해야 한다.

## 3. Rust 문서 추출

상태: 대기. Python 추출은 TXT 인코딩(UTF-8-SIG/CP949/UTF-16), Markdown의 위치·정규화, HWPX의 표/스타일/개요/위치를 보존한다. 기존 `klib/src/content.rs`의 단순 TXT/MD 및 XML 텍스트 추출과는 계약이 다르다. 문단 단위 비교 도구와 fixture 차이 0을 구현/검증해야 한다.

## 4. klib 일괄 색인 명령

상태: 대기. klib에는 Tantivy `SearchIndex` 및 자료함 문단 추출이 있으나 타임머신 인덱스 스키마, 폴더 전체 경로, Python 검색 상위 20 비교에 필요한 통합 명령은 아직 확인되지 않았다.

## 5. 앱 연결 및 전후 측정

상태: 대기. Python 본문 검색 경로의 txt/md/hwpx 분기, 진행률 프로토콜, 오류 fallback, 동일 장치에서의 before/after 시간·메모리를 연결 후 측정해야 한다. Rust 색인기가 생성하는 인덱스를 앱 검색이 읽는 경로가 확인되기 전에는 전환 완료로 표시하지 않는다.

## 실행 기록

- 현재 커밋: `943bab95`; 브랜치 `feature/klib-index-rust`, upstream 상태는 시작 시 `github/main`과 일치.
- 작업 폴더 최초 상태는 clean.
- Cargo 및 테스트 명령, fixture 비교, 실제 성능 측정은 아직 실행하지 않음.
