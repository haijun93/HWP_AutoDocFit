# KIDX12: Python 색인의 DOCX 문단 추출 함수 위치 및 동작 표

## 1. 개요
Python 본문 색인 파이프라인에서 DOCX 문서의 문단 추출을 담당하는 함수 위치, 관련 모듈, 그리고 구체적인 처리 동작을 정리한다.

## 2. 함수 및 모듈 위치 표

| 단계 / 역할 | 모듈 / 파일 경로 | 함수 / 클래스 | 주요 동작 |
|---|---|---|---|
| **앱 작업자 분배** | `docfit_app/timemachine_service.py` | `_content_index_worker` | 확장자별로 Rust(`klib index-content`) vs Python(`content_index.index_file`) 처리 경로 분배 |
| **Rust 색인 배치 호출** | `docfit_app/timemachine_service.py` | `_run_klib_index` | `klib index-content <db_path> <files...>` 서브프로세스 실행 및 진행 스트림 수신, 실패 시 Python 폴백 반환 |
| **색인 엔진 진입점** | `docfit_core/timemachine/content_search.py` | `ContentIndex.index_file` | 파일 메타데이터(크기, 수정일, 스키마 버전) 확인 후 `extract_document` 호출 및 FTS/트라이그램 DB 적재 |
| **추출 디스패처** | `docfit_core/timemachine/extractors.py` | `extract_document` | 확장자(`ext == "docx"`) 확인 후 `extractors_ooxml.extract_docx` 호출 및 `_apply_hierarchy` 결합 |
| **DOCX 안전 압축 열기** | `docfit_core/timemachine/extractors_ooxml.py` | `_open_zip_safely` | OLE 암호화 복합문서 감지, ZIP 항목 한도(10,000개), 암호화 플래그(0x1), 안전 경로, symlink 차단, 크기(단일 128MB, 총 512MB), 압축률(1,000배) 검사 |
| **DOCX 본문/머리글 문단 추출** | `docfit_core/timemachine/extractors_ooxml.py` | `extract_docx` | `word/document.xml` 및 `word/header*.xml`, `word/footer*.xml`의 `w:p` 태그 순회, itertext 추출 및 공백 trim, `ExtractionRecord` 생성 |
| **위계 결합** | `docfit_core/timemachine/extractors.py` | `_apply_hierarchy` | 전달된 `hierarchy_records`가 있는 경우 텍스트 매칭(FIFO 큐)을 통해 `hierarchy_level`, `is_table_cell`, `style_name`, `outline_level` 결합 |

## 3. `extract_docx` 세부 동작 규격

| 단계 | 처리 항목 | 입력/대상 | 동작 및 예외 처리 | 산출물 / 레코드 속성 |
|---|---|---|---|---|
| **1. 유효성 검사** | ZIP 열기 및 보안 한도 | 파일 경로 (`Path`) | - 파일 미존재 시 `ExtractionError("corrupt")`<br>- OLE 헤더 및 `EncryptedPackage` 포함 시 `ExtractionError("encrypted")`<br>- ZipFile 파싱 실패 시 `ExtractionError("corrupt")`<br>- ZIP 암호화 플래그(bit 0) 설정 시 `ExtractionError("encrypted")`<br>- 항목수 > 10,000, 단일 파일 > 128MB, 전체 크기 > 512MB, 압축비 > 1,000배 초과 시 `ExtractionError("corrupt")`<br>- 경로 탈출(`..`, 절대경로, 드라이브 문자), 심볼릭 링크 시 `ExtractionError("corrupt")` | 유효한 ZipFile 인스턴스 |
| **2. 필수 파일 검사** | `word/document.xml` 존재 여부 | ZIP 엔트리 목록 | `word/document.xml` 미존재 시 `ExtractionError("corrupt", "DOCX 필수 파일(word/document.xml) 누락")` | 필수 파일 확인 완료 |
| **3. 본문 문단 추출** | `word/document.xml` | XML 스트림 (`defusedxml.ElementTree`) | - XML 파싱 실패 시 `ExtractionError("corrupt")`<br>- 로컬 태그명이 `"p"`인 요소 순회<br>- `"".join(elem.itertext()).strip()`로 텍스트 추출<br>- 빈 텍스트는 건너뜀 | `ExtractionRecord`<br>- `text`: 추출된 텍스트<br>- `location_kind`: `"paragraph"`<br>- `location_label`: `f"문단 {p_num}"` (1-based)<br>- `sequence`: `seq` (문서 전체 1-based)<br>- `section`: `"본문"` |
| **4. 머리글/바닥글 문단 추출** | `word/header*.xml`<br>`word/footer*.xml` | ZIP 엔트리 목록 | - 정렬 키: `(1 if "footer" in name else 0, int(num))`<br>- `section_label`: `"머리글"` or `"바닥글"`<br>- `label_prefix`: `f"{section_label} {hf_index}".strip()`<br>- `p` 태그 요소별 itertext 추출 후 공백 trim | `ExtractionRecord`<br>- `text`: 추출된 텍스트<br>- `location_kind`: `"paragraph"`<br>- `location_label`: `f"{label_prefix} 문단 {hf_p_num}"`<br>- `sequence`: `seq`<br>- `section`: `label_prefix` |
| **5. 빈 결과 검사** | 추출 결과 목록 | `records: list[ExtractionRecord]` | 본문 및 머리글/바닥글에서 추출된 문단이 0개인 경우 `ExtractionError("empty", "추출 가능한 DOCX 본문 텍스트가 없음")` 발생 | 비어있지 않은 `list[ExtractionRecord]` 반환 |
