# Rust 몽땅검색 통합 1단계 설계 메모

작성일: 2026-11-24  
카드: SRCH1B (조사·계약·SQLite 어댑터·스키마 초안)  
기준: 저장소의 지정 파일만 읽어 확인한 정적 코드 조사. 실제 성능 수치는 측정하지 않았다.

## 범위 및 확인 한계

이번 범위는 `TODO.md`의 Rust 검색 통합 절 중 현황 조사, Python `SearchBackend` 계약과 SQLite 어댑터, Rust 스키마 초안이다. 설정 플래그와 측정 스크립트는 다음 SRCH2 범위다. 조사 대상은 `docfit_app/timemachine_service.py`, `docfit_core/timemachine/content_search.py`, `klib/src/search.rs` 및 해당 TODO 절로 제한했다. 기존 IS1·MG2·HIER3D 문서, IPC 구현 파일, `content.rs`, kfind 구현체는 이번 조사에서 열지 않았다. 따라서 Named Pipe wire contract·kfind 내부 변경 감지·Rust 추출기 세부는 확인하지 못했다.

## 현재 구성과 호출 흐름

| 영역 | 확인된 역할 | 저장/호출 경로 | 관찰된 한계 |
|---|---|---|---|
| Python SQLite | `ContentSearchIndex`는 별도 SQLite DB에 본문/문단 FTS, 짧은 조각, 문서 메타데이터와 `content_errors`를 관리한다. 파일별 추출·위계 분석·증분 판정, 본문/위계 검색과 Markdown 내보내기가 이 API를 사용한다. | `TimemachineService`가 `timeline-content.sqlite`를 열어 직접 보유한다. `content_index` 명령, 색인 worker, 파일 제거, 검색 fallback 및 Markdown export가 호출한다. | Rust 인덱스와 공유 저장소가 아니다. service의 다수 경로가 SQLite 전용 메서드와 내부 DB 구조를 직접 참조한다.
| Rust klib/Tantivy | `klib/src/search.rs`의 `SearchIndex`는 문단 단위 Tantivy 색인·검색 라이브러리다. schema v4에는 한국어 bigram 및 자모/초성 필드, path·extension·mtime·page FAST 필드, body와 일부 표시 메타데이터 stored 필드가 있다. | service worker는 선택된 확장자(`txt`, `md`, `hwpx`, `docx`, `hwp`)를 `klib index-content <sqlite 경로> <paths...>` 하위 프로세스에 전달한다. | 기존 schema는 path 기반 문단을 중심으로 하며 제안된 document/node/parent/breadcrumb/content hash 정보를 갖지 않는다. Rust의 Named Pipe 연결을 이 파일만으로 확인할 수 없다.
| kfind | service의 별도 파일명/검색 엔진 클라이언트다. 상태·query 요청을 사용하고 SQLite 색인 문단으로 kfind 색인을 seed/sync한다. | kfind 준비가 되면 검색 결과를 변환하고, 준비 중·near 검색·예외 등은 SQLite legacy 검색으로 돌아간다. `shadow` 설정에서는 두 결과를 비교 기록하고 legacy 결과를 반환한다. | kfind 구현과 그 자체의 디스크 저장 형식 및 파일 변경 감지는 이번 범위에서 조사하지 않았다.

```text
TimemachineService
  ├─ 시작 ──> ContentSearchIndex ──> timeline-content.sqlite
  │             ├─ 파일 추출/문단·위계 저장/오류·상태
  │             ├─ 본문·위계 검색 및 Markdown export
  │             └─ kfind seed용 문단/경로 제공
  ├─ 색인 worker ── 확장자별 분기
  │             ├─ 일부 형식 ──> klib index-content 프로세스 ──> Tantivy (세부 저장 경로 미확인)
  │             └─ 나머지/실패 파일 ──> SQLite 색인·fallback
  └─ 검색 ──> kfind query ── 성공: 변환된 결과
                           └─ 미준비/미지원/오류: SQLite 결과
```

### 중복 저장·변경 감지·취소

SQLite는 문단 FTS와 메타데이터를 직접 보유한다. Rust Tantivy도 `body`와 검색용 토큰 필드를 가지므로 적어도 본문 내용과 토큰 표현은 양쪽에 중복될 수 있다. `TimemachineService`는 SQLite의 `size`, `mtime`, hierarchy schema version으로 재색인 필요 여부를 판단하며, Rust 프로세스에 일부 경로를 전달한다. Rust 파일별 변경 판단 기준은 이번 제한 조사에서 확인하지 못했다. service는 색인 worker에 `threading.Event`를 전달하고, Rust 프로세스 실행 중 취소 시 프로세스 종료를 시도한다. SQLite `ContentSearchIndex` 자체에는 외부 취소 계약이 보이지 않는다.

### 확인된 SQLite API 안정 경계

`content_search`는 검색 결과의 각 항목에 `file_name` 별칭을 추가한다. `content_index`는 기존 색인 응답을 반환하고, `content_index_all/start/status/progress/cancel`은 worker 상태 계약을 제공한다. Markdown export는 `keyword_search`의 `locations`와 `snippet` 필드에 의존한다. `content_errors`는 SQLite 색인 상태의 일부다. 이번 변경은 기존 구현을 고치지 않고 adapter를 얹어 이 형식을 유지한다.

## SearchBackend 계약

계약 메서드는 `index_file(path)`, `remove_file(path)`, `search(query, ...)`, `status(roots=...)`, `cancel(cancel_event)`다. 검색 인수에는 기존의 root, hierarchy/keyword 모드, limit/offset, sort, 날짜 범위를 보존한다. SQLite adapter는 `ContentSearchIndex`를 감싸며 추출·정렬·오류·저장 형식을 재구현하지 않는다. 색인 상세 worker가 쓰는 SQLite 전용 메타데이터 접근은 이번 첫 단계에서 남긴다. 기본 service 인스턴스는 기존 DB와 동일한 `ContentSearchIndex`를 감싼 adapter를 사용한다.

## Rust 스키마 v1 초안

목표 필드:

| 필드 | 용도 | 권장 저장 정책 |
|---|---|---|
| `document_id` | 문서 단위 그룹 키 | FAST/색인. 응답 경로는 원본 메타데이터 저장소에서 조회 가능하면 stored 제외 |
| `node_id` | 위계 노드 식별 | 색인/FAST 필요성 평가, 기본 stored 제외 |
| `parent_id` | 부모 연결 | FAST 또는 term field, 기본 stored 제외 |
| `breadcrumb_id` | 경로 breadcrumb 참조 | FAST/term field, 문자열 본문은 stored하지 않음 |
| `level` | 제목·장·절·항목·본문 우선순위 | FAST, 필요 시 색인, stored 제외 |
| `page` | 페이지 위치 | FAST, stored 제외 |
| `paragraph` | 문단 서수 | FAST, stored 제외 |
| `table` | 표/셀 위치 식별 | FAST/term field, 표시 문자열은 별도 최소 저장소 검토 |
| `ocr` | OCR 생성 여부/상태 | FAST 또는 term field, stored 제외 |
| `confidence` | OCR/추출 신뢰도 | FAST numeric, stored 제외 |
| `content_hash` | 변경 감지 및 중복 방지 | term field 또는 메타데이터, 기본 stored 제외 |
| `schema_version` | 문서/노드 계약 버전 | FAST numeric, stored 제외 |

stored 데이터는 검색 응답을 만들기 위해 꼭 필요한 짧은 표시값만 허용한다. 본문은 Tantivy stored field에 복제하지 않고 추출 결과 캐시/원본 참조에서 필요한 snippet을 구성하는 안을 우선 검토한다. 경로·문서명 저장 필요성은 Python 결과 계약과 후보 결과 후처리를 계측한 뒤 정한다. 이는 설계 초안이며 klib 스키마 변경은 포함하지 않는다.

| 개념 | klib schema v4 현재 | 스키마 v1 초안 차이 |
|---|---|---|
| 문서 키 | `doc_id`는 색인 내부 문서 키처럼 쓰이고, `path`로 파일을 묶음 | 안정적인 `document_id`를 명시해 path와 분리 |
| 위계 | 문단별 `level`; 부모/경로 노드 없음 | `node_id`, `parent_id`, `breadcrumb_id` 추가 |
| 위치 | `page`, 추가로 `sheet`, `row`, `paragraph_ordinal`, `location` | `page`, `paragraph`, `table`을 공통 이름으로 명시하고 workbook 세부는 확장 메타데이터로 분리 검토 |
| OCR/신뢰도 | OCR 표지 및 confidence 필드 없음 | `ocr`, `confidence` 추가 |
| 변경/버전 | 인덱스 전역 `SCHEMA_VERSION=4`; 문서 content hash 없음 | 문서별 `content_hash`, 데이터 계약 `schema_version` 추가 |
| stored | `name`, `title`, `box_ids`, `sheet`, `row`, `body`, notebook 식별/위치 등이 stored; 검색 토큰 대부분 미저장, path·level·mtime·page는 FAST | 본문/대형 메타데이터를 저장하지 않고 결과에 필요한 최소 표시값만 stored |

## 다음 단계 제안

1. SRCH2: 설정 플래그 뼈대(`sqlite` 기본, 미구현 Rust/shadow는 명시적 사유와 SQLite fallback)와 합성 코퍼스 SQLite 기준선 측정.
2. SRCH3: Rust backend 프로세스/IPC 계약과 응답 동일성 시험을 정하고 `index_file/remove_file/search/status/cancel`을 연결.
3. SRCH4: Tantivy 배치 writer·최소 stored field·v1 위계 필드를 단계적으로 도입하고 결과 비교.
4. SRCH5: 변경 감지/hash, 오류·취소·재시작·fallback을 통합하고 대형 코퍼스에서 메모리·지연을 측정.

이번 카드에서 못 한 것: IPC 세부, kfind 내부 구현, `content.rs` 입력 형식, 기존 IS1·MG2·HIER3D 문서 대조, 기준선 측정. 성능 목표의 실제 달성 여부는 측정하지 않았다.
