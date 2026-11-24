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
| 초기 색인 1천 문서 | 53.345초 | 측정 대기 |
| 초기 색인 1만 문서 | 18분 제한 초과 | 측정 대기 |
| 증분 색인 10문서 | 5.377초 | 측정 대기 |
| 첫 질의 / 반복 중앙값 | 0.7~0.9초 / 9~20ms | 측정 대기 |
| 색인 중 질의 | 중앙값 965ms, 최댓값 1,239ms | 측정 대기 |
| 최대 RSS / 인덱스 크기 | 84.1MiB / 41.9MiB (1천 문서) | 측정 대기 |
| 결과 일치율(키워드·AND·phrase·위계) | — | 측정 대기 |

## 결과 일치도·검증

Rust 결과와 SQLite 상위 20개 비교는 같은 seed `20261124`의 합성 문서로 실행한다. 누락·추가 경로를 질의별로 기록한다. Rust 빌드 및 실제 측정 결과는 실행 완료 뒤 이 문서에 채운다. 전체 Python 시험 결과도 실행 완료 뒤 기록한다.

## 기본 backend 승격

현재는 승격을 권하지 않는다. SRCH3 성능과 결과 일치 측정이 끝나지 않았고, 결과 변환은 SQLite의 위계·문단 위치 정보를 완전히 재현하지 못할 수 있다. 기본값 `sqlite`와 그 경로는 유지한다. 승격 판단 전 Rust 1천 문서 측정, 동일 조건 결과 비교, fallback/shadow 전체 시험을 확인해야 한다.

## 미완료·확인 필요

- Rust release 빌드, 1천·1만 문서 벤치마크, 결과 일치도 및 전체 Python 시험: 측정 대기.
- 현재 색인 단계는 파일마다 writer 커밋을 수행하므로 배치 writer의 이득을 반영하지 않는다.

## SRCH4 제안

1. 여러 파일을 하나의 `SearchIndex::batch()` 범위로 묶고, 파일별 오류가 있어도 완료된 변경만 안전하게 커밋하는 배치 writer 경로를 추가한다.
2. 경로·확장자·수정 시각·위계 수준 외에 문단 번호, 섹션 breadcrumb, 위치/쪽 정보를 색인에 보존해 SQLite 응답 변환과 상위 20개 일치도를 개선한다.
3. SQLite/Rust 양쪽의 취소·상태 응답을 일치시키고, 색인 중 검색 지연·RSS를 반복 측정한다.
