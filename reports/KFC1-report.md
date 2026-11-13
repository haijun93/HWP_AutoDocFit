# KFC1 문서 범위 거르기(allow·deny) 보고 (2026-11-13)

브랜치 `feature/instant-search`, 크레이트 `kfind/core`.

## 한 일
- `SearchOptions.allow`·`deny: Option<Arc<RoaringTreemap>>`(문서 ID 집합). 허용 집합 안이고 제외 집합 밖인 문서만 결과에 낸다. 빈 허용 집합 = 빈 결과.
- 거르기는 한 함수(`scope_sets`)에서: 조각 판정 뒤 범위 밖 문서의 문장을 일치·붙어 있는 일치 비트맵에서 뺀다. `rank`·`search`·`count`·`match_sentences`·`Ranking::sentences`가 모두 같은 결과를 쓴다.
- 질의 이력 캐시는 범위와 무관한 판정만 담는다 → 범위만 바꾼 같은 질의도 캐시를 그대로 쓴다.
- 붙여 쓴 검색어 나누기는 범위와 무관하게 색인 전체 낱말로 한다(범위에 따라 낱말 해석이 바뀌지 않음).
- `kfind_core::RoaringTreemap` 다시 냄(호출 쪽이 roaring 판을 맞추지 않아도 됨).
- 구조체 리터럴로 `SearchOptions`를 만들던 3곳(`tests/common/mod.rs` 도우미, `examples/kc1_bench.rs`, `bench/kb1`)에 `..SearchOptions::default()`만 더함(시험 내용은 그대로).

## 대조(oracle)
- `tools/oracle.py`에 범위 표 `tests/data/oracle_scope_{1,2}.tsv` 추가: 범위 8가지(mod3 허용, mod4 제외, 둘 다, 2^40 이상 허용/제외, 빈 허용, 없는 ID 허용/제외) × 검색어 14 × 나누기 3 = 상태마다 336줄. 기존 대조 표 4개는 바뀌지 않음(`--check` 통과).
- `tests/scope_filter.rs`: 조각 1개·20개, 증분 상태(지운 문장 비트맵 있음, 지운 문서 ID가 허용 집합에 있어도 다시 안 나옴), 질의 이력 캐시 켬, 합친 뒤 — 모두 차이 0.

## 시험
`cargo test`(kfind-core): 49건 통과(기존 46 + 새 3), 실패 0.

## 지연(1회, `examples/kfc1_scope_bench.rs`, release)
합성 10만 문서 120만 문장, 허용 1만 문서, 반복 50회 중앙값(ms). 원자료 `reports/KFC1-scope-bench-raw.json`.

| 검색어 | 거르기 없음 | 허용(흩어짐) | 허용(이어짐) | 허용+제외 |
|---|---|---|---|---|
| 예산 | 7.1 | 16.3 | 12.4 | 17.4 |
| 예산 집행 | 3.1 | 10.7 | 9.2 | 9.3 |
| 주민센터 프로그램 | 51.2 | 73.5 | 85.0 | 87.2 |
| 예산집행 | 4.0 | 13.7 | 13.6 | 15.2 |
| 도로 공사 협의 | 1.9 | 3.6 | 3.0 | 3.4 |
| 공원 | 8.3 | 14.9 | 10.4 | 14.5 |

- 거르기는 판정 뒤에 하므로 범위가 좁아도 판정 비용은 그대로이고, 일치 문서마다 집합 확인·범위 빼기가 더해져 거르기 없음보다 느리다(최대 2~3배, 최악 중앙값 87ms). 2초 한도 안.
- 개선 여지(안 함): 캐시가 꺼져 있을 때 허용 집합을 조각별 문장 비트맵으로 바꿔 판정 전에 좁히기(`eval_seg`의 `restrict`). 실제 문서 측정 뒤 결정.
