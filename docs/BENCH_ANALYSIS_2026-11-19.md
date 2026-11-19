# BENCH4 분석기 구현 및 스키마 확인 (2026-11-19)

## 추가 파일

- `scripts/qms_bench_analyze.py`: 표준 라이브러리만 사용하는 스키마 v1 입력 검증, 집계, 회귀 관찰 및 Markdown 보고서 CLI.
- `tests/test_qms_bench_analyze.py`: 고정 JSON fixture를 이용한 독립 시험 22개.
- `tests/fixtures/qms_bench_analyze/{base,current,everything}.json`: 손계산 가능한 최소 입력 데이터.

실행: `python scripts/qms_bench_analyze.py 결과.json ... [--out report.md] [--compare 이전.json]`. 정상 완료는 0, 필드 경고가 있는 정상 보고서는 1, 파일/JSON/스키마 오류는 2를 반환한다. 원문이 없는 입력은 종류·길이·해시로만 표시한다.

## 크레이트 스키마 확인 결과

`qms_bench/src/lib.rs`의 `BenchRun`은 최상위 `schema_version`, `run`, `queries`, `resources`, `summary`를 갖고 `validate_result`는 버전 1과 해당 필드 존재만 검증한다. `csv_summary`의 실제 컬럼은 `run_id, query_id, kind, query_length, query_hash, comparability, p50_ms, p90_ms, p95_ms, p99_ms, min_ms, max_ms, mean_ms, stddev_ms, result_count, timeouts, errors`이다.

실제 JSON 질의 레코드는 `digest`, `statistics`, `result_count`, `timeouts`, `errors`를 저장한다. 실행 메타는 adapter, query_set_version, warmups, repeats, top_n 등이며 결과 JSON에 기계 정보·색인 메타데이터·query_set_hash는 없다. 이와 달리 설계 §6 예시의 `engines`, `latency_ms`, 상위 N 일치율, 원시 관측값 및 프로세스 자원 구간 구조는 현 writer JSON에 기록되지 않는다. 분석기는 이들 미래/설계 필드도 조건부 처리하고 부재 시 표본 부족/측정 불가로 취급한다.

현재 결과에는 어댑터당 JSON 하나가 생성되므로 서로 다른 파일을 직접 짝지을 때도 `query_set_version`은 실제 세트 해시가 아니다. 따라서 보고서에서 동일 세트임을 확정할 수 없다고 명시한다. 유휴 상주 메모리·질의별 리소스 비용·선형 메모리 추세는 입력에 구간/표본이 없으면 계산 불가로 표시한다. 이는 writer 변경 없이 정직하게 산출할 수 없는 범위다.

## 시험 결과

명령: `python -m unittest tests.test_qms_bench_analyze`

```
Ran 22 tests in 0.003s
OK
```

## 남은 격차

Rust writer가 설계 §6의 실행 메타(기계/색인/세트 해시), 두 엔진 질의별 지연 배열·정확도 비교, 자원 대상 및 idle/search_peak/post_search/raw sample을 기록하지 않는다. 이 입력이 제공되지 않는 분석 항목은 보고서에 계산 불가로 남는다. 이번 카드 범위는 Python 분석기와 관련 시험으로 한정했다.
