# SRCH3 Rust 몽땅검색엔진 통합 결과

기준 커밋: `2cb65faf` (SRCH2 SQLite 기준선), 작업 브랜치 `feature/SRCH3`.

## 구현 상태

- `klib search-content`는 한 프로세스에서 JSONL 요청을 받아 Tantivy 색인·검색을 제공한다. Python `RustBackend`가 `index_file`, `remove_file`, `search`, `status`, `cancel`을 호출한다.
- `search_backend_mode=sqlite`는 기존 SQLite backend를 사용한다. `rust`는 Rust 실패 시 원인을 warning 로그로 남기고 SQLite로 fallback한다. `shadow_compare`는 SQLite 응답을 반환하며 Rust 결과의 상위 20개 차이를 JSONL로 기록한다.
- Rust 응답을 기존 keyword·hierarchy 검색 행 구조로 변환한다. 별도 Tantivy 색인 경로를 사용해 klib 자료함 색인과 분리한다.
- 합성 코퍼스 전용 `scripts/search_compare.py`를 추가했다. 원본 문서는 사용하지 않는다.

## 성능 비교

| 항목 | SQLite SRCH2 기준 | Rust SRCH3 |
|---|---:|---:|
| 초기 색인 1천 문서 | 53.345초 | 272.463초 |
| 초기 색인 1만 문서 | 18분 제한 초과 | 미실행(1천 건 속도 선형 추정 약 45.4분) |
| 증분 색인 10문서 | 5.377초 | 7.158초 |
| 첫 질의 / 반복 중앙값 | 0.7~0.9초 / 9~20ms | 23~39ms / 22~27ms |
| 색인 중 질의 | 중앙값 965ms, 최댓값 1,239ms | 중앙값 316ms, 최댓값 3,608ms |
| 최대 RSS / 인덱스 크기 | 84.1MiB / 41.9MiB (1천 문서) | 83.1MiB / 4.45MiB (1천 문서) |
| 상위 20 일치율(키워드·AND·phrase·위계) | — | 10% / 10% / 0% / 10% |

## 결과 일치도·검증

같은 seed `20261124`의 합성 문서 1천 건과 SRCH2 질의로 비교했다. 네 질의 모두 각 backend에서 20건을 반환했다. 일치율은 두 상위 목록의 교집합을 20으로 나눈 값이다. 전체 누락·추가 파일명은 [SRCH3_MATCH_2026-11-24.json](SRCH3_MATCH_2026-11-24.json)에 기록했다.

| 유형 | 질의 | SQLite/Rust | 일치율 | SQLite 누락 / Rust 추가 |
|---|---|---:|---:|---:|
| 키워드 | `행정` | 20 / 20 | 10% | 18 / 18 |
| AND | `행정 정책` | 20 / 20 | 10% | 18 / 18 |
| phrase | `"행정 절차 지원"` | 20 / 20 | 0% | 20 / 20 |
| 위계 | `행정 정책 문서 안전 지원` | 20 / 20 | 10% | 18 / 18 |

차이는 색인·순위 기준에서 온다. SQLite 키워드 검색은 trigram occurrence 수와 모든 질의어 일치 여부를 사용하지만, Rust는 한글 bigram 토큰에 BM25와 문단 위계 집계를 적용하고 query parser의 기본 OR 결합을 사용한다. phrase 검색도 SQLite trigram 구간과 Tantivy bigram 위치 검색의 의미가 다르다. 위계용 코퍼스는 `.txt` 합성 파일이라 실제 위계 표식이 없어 위계 순위 비교에는 한계가 있다. 이 결과는 두 검색기의 검색 의미를 맞추기 전에 Rust를 기본값으로 승격할 수 없음을 보여 준다.

Rust release 빌드는 성공했다. 첫 MinGW 경로 설정은 `dlltool.exe`를 찾지 못해 실패했고, 설치 경로를 지정해 재시도한 빌드는 성공했다(경고 65건). Python 전체 시험은 `3,590 tests`, `OK (skipped=28)`로 통과했다(843.837초). 시험 종료 때 기존 sandbox cleanup에서 열려 있는 SQLite 파일에 대한 `PermissionError`가 출력됐지만 unittest 종료 코드는 성공이었다. 새 backend 단위 시험 7건도 별도로 통과했다.

## 기본 backend 승격

Rust를 기본값으로 승격할 수 없다. 1천 건 초기 색인이 약 5.1배 느리고, 상위 20 일치율이 0~10%이며, phrase 결과가 전혀 겹치지 않는다. 색인 중 최악 질의 지연도 3.6초로 SQLite의 1.24초보다 길다. 디스크 크기는 약 89% 줄었지만 이를 상쇄하지 못한다. 기본값과 응답은 계속 SQLite로 둔다. Rust 모드는 명시 설정에서만 사용하고, 실패 시 SQLite fallback과 shadow JSONL 비교를 제공한다.

## 미완료·확인 필요

- 1만 문서는 선형 추정 45.4분으로 요청된 18분 제한을 넘어 실행하지 않았다.
- Rust 상태의 `indexed_document_count`는 현재 Tantivy 문단 문서 수를 사용하므로 SQLite의 파일 수와 의미가 다르다. 상태 계약을 맞춰야 한다.
- 색인 단계는 파일마다 writer 커밋을 수행한다. 이번 수치는 배치 writer 최적화를 반영하지 않는다.

## SRCH4 제안

1. 여러 파일을 하나의 `SearchIndex::batch()` 범위로 묶어 초기·증분 색인을 줄인다.
2. Rust query parser에 SQLite와 같은 AND·phrase 의미를 넣고 top-20 일치도를 다시 측정한다.
3. 문단 번호·breadcrumb·위치 정보를 색인하고, 상태의 파일 수·문단 수 및 취소 응답을 SQLite 계약과 맞춘다.
