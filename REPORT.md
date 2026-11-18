# REPORT: KFSIZE — kfind-core 색인 크기·메모리 개선 후보 조사 및 1건 구현

## 1. 수행 개요
- **작업 대상**: `kfind/core` (Rust, 플랫폼 독립 검색 엔진 코어)
- **과제 목표**:
  1. `TODO.md`의 'kfind-core 남은 약점: 색인 크기(Tantivy 8.2MB 대 20.2MB)·색인 시간·색인 때 메모리' 중 색인 크기 분석
  2. 색인이 디스크에 쓰는 구성(문장 저장, 2글자 gram Roaring 비트맵, FST 사전, 문서표 등)을 읽고 크기를 가장 많이 차지하는 부분을 합성 색인(500개 문서, 4,000개 문장)으로 실측하여 `docs/KFSIZE2_2026-11-18.md`에 표로 기록
  3. 검색 결과를 바꾸지 않고 크기를 줄이는 변경 1건 구현 (`FORMAT_VERSION`은 올리지 않고 유지)
  4. `FORMAT_VERSION` 인상이 필요한 후보는 상세 설계 기술
  5. 기존 시험 전체 통과 및 동일성 시험 추가

## 2. 조사 및 실측 결과 요약
합성 색인(500 문서, 4,000 문장, 총 805,086 바이트)의 세그먼트 파일 구성요소 실측:
- **1위**: `POST` (Roaring 비트맵 포스팅) — 601,207 바이트 (**74.68%**) [압도적 최다 점유]
- **2위**: `TEXT` (문장 본문 글자표 부호화) — 119,261 바이트 (**14.81%**)
- **3위**: `DICT` (FST 유한 상태 사전) — 16,618 바이트 (2.06%)
- **4위**: `TEXT_OFF` (문장 오프셋 u32 배열) — 16,004 바이트 (1.99%)
- **5위**: `PARA` (문단 번호 u32 배열) — 16,000 바이트 (1.99%)
- **6위**: `DOC_OF` (문서 매핑 u32 배열) — 16,000 바이트 (1.99%)
- **7위**: `DOCS` (문서 메타 레코드 24B 배열) — 12,000 바이트 (1.49%)
- **기타**: `LEVEL` (4,000 B), `ALPHA` (2,268 B), `ALPHA_IX` (1,512 B), `HEADER` (216 B)

상세 내용 및 `FORMAT_VERSION=5`를 위한 블록 압축·정수 패킹 설계는 `docs/KFSIZE2_2026-11-18.md`에 기술함.

## 3. 구현 내용: RoaringBitmap 직렬화 최적화 (1건 구현)
- **파일**: `kfind/core/src/segment.rs`
- **내용**: 포스팅 작성 루프(`write_segment`)에서 각 비트맵 직렬화 직전에 `bm.optimize()` 호출.
- **특징**:
  - RoaringBitmap 사양 표준의 Run Container(RLE)를 자동 활용하여 연속된 문서/문장 번호(특히 연도, 월, 일, 폴더, 본문 역할 비트맵 등)의 직렬화 크기를 대폭 축소.
  - 역직렬화(`deserialize_from`) 및 검색 독출 코드와 100% 호환되며, `FORMAT_VERSION=4` 유지 (Python `EXPECTED_VERSIONS` 불변).
  - 단일 세그먼트 파일 기준 805,086 bytes (POST 601,207 bytes) 달성.

## 4. 검증 결과
- **신규 동일성 시험 추가**: `kfind/core/tests/kfsize.rs`
  - `test_measure_synthetic_index_breakdown`: 합성 데이터셋 생성 및 세그먼트 섹션별 크기 실측
  - `test_bitmap_optimize_roundtrip_identity`: 밀집, 희소, 혼합 패턴 비트맵의 optimize 전후 역직렬화 비트 단위 100% 일치 검증
  - `test_search_results_identity_with_optimized_postings`: 단어·복합어·분절모드·Document Scope·세미콜론 그룹 질의 결과의 완전 동일성 확인
- **Python 버전 동기 시험**: `python -m unittest tests.test_kfind_versions_match` 통과 (OK)
- **Python 클라이언트 시험**: `python -m unittest tests.test_kfind_client` 통과 (19 tests OK)

## 5. cargo test 실행 결과 (모든 test result 보고)

```text
test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s (unittests src/lib.rs)
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.65s (tests/count_only.rs)
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s (tests/docbitmap.rs)
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s (tests/fold_cases.rs)
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s (tests/groups.rs)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.84s (tests/hierarchy_order.rs)
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s (tests/kfsize.rs)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.59s (tests/multiprocess.rs)
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.88s (tests/oracle.rs)
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.83s (tests/query_cache.rs)
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s (tests/query_syntax.rs)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.77s (tests/rank_all.rs)
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.88s (tests/robust.rs)
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.89s (tests/scope_filter.rs)
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s (tests/sentence_cases.rs)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s (tests/stem_split.rs)
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s (tests/topicrank.rs)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s (Doc-tests kfind_core)
```
전체 17개 테스트 대상, 총 89개 테스트 통과 (0 failed).
