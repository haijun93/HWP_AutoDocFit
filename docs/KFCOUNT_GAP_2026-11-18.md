# KFCOUNT — kfind-core 격차 및 개수 전용 질의 (2026-11-18)

## 조사 결과

| TODO/BENCH2 항목 | 현 구현 확인 | 상태 |
|---|---|---|
| 색인 크기 | 기존 kfind-core 20.16 MB, Tantivy 8.24 MB. `docs/KFIND_SIZE_2026-11-18.md`의 합성 변경은 디스크 크기 차이 0%. | 남음 |
| 색인 시간 | BENCH2: kfind-core 8.60 s, Tantivy 5.46 s. 11/18 합성 표본에서는 0.546 s → 0.215 s 개선이나 실제 말뭉치 비교는 아님. | 남음 |
| 색인 중 메모리 | BENCH2 최대 RSS 79.1 MB, Tantivy 70.4 MB. 11/18 합성 표본의 peak working set 17.7 → 16.8 MiB. 실제 corpus·저사양 조건에서 개선 여부는 미확인. | 남음 |
| 개수만 질의 | `Index::count`는 문장 수·문서 수만 계산하는 기존 API. 단, `count_groups`는 `rank_groups_inner`를 통해 문서 hit와 순위 구조를 전부 물질화함. | 단일 질의 구현됨, 그룹 질의 약점 확인 및 수정 |

참조: TODO.md의 “kfind-core 남은 약점”, `F:/docfit-tmp/bench2/BENCH2-report.md` §3.2·§5, `docs/KC1_KFIND_CORE_2026-11-12.md`, `docs/APP1_KFIND_INTEGRATION_2026-11-13.md` §5-2, `docs/KFIND_SIZE_2026-11-18.md`.

## 변경

- `kfind/core/src/index.rs`: `count_groups`가 각 검색 그룹의 문장 비트맵으로 문서 교집합을 구하고, 공통 문서 안의 문장 합집합 수를 직접 집계한다. per-document hit 수집, 전체 순위 구성, 미리보기 생성은 하지 않는다. 기존 그룹 질의의 문서 교집합 및 공통 문서 내 문장 합집합 의미를 유지한다.
- `kfind/core/tests/count_only.rs`: 합성 색인에서 그룹 개수와 `search_groups` 결과의 문장 수·문서 수·세대·낱말 해석을 비교한다. 없는 낱말·짧은 낱말 사례도 포함한다.

## 검증

사용자 지정 PATH 앞부분과 `CARGO_TARGET_DIR=E:/cargo-target/kfcount`로 `cargo test --manifest-path kfind/core/Cargo.toml` 전체 실행. 최종 실행에서 나온 모든 `test result` 줄:

```text
test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.77s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.46s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.34s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.55s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.93s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.88s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.19s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.48s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.27s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 남은 격차

- 색인 바이트 크기, 시간, peak 메모리를 Tantivy와 실제 BENCH2 조건에서 개선했는지는 남아 있다. 이번 변경은 그룹 count의 불필요한 결과 물질화만 줄였고 성능 수치는 측정하지 않는다.
- 단일 질의 `count`는 기존에 전용 경로였으므로 이번에 알고리즘을 변경하지 않았다.
- 그룹 질의 count의 지연·할당량 개선 정도는 별도 측정 전까지 수치로 주장하지 않는다.
