# BENCH4 작업 보고서 (2026-11-19)

## 변경 파일

- `scripts/qms_bench_analyze.py`
- `tests/test_qms_bench_analyze.py`
- `tests/fixtures/qms_bench_analyze/base.json`
- `tests/fixtures/qms_bench_analyze/current.json`
- `tests/fixtures/qms_bench_analyze/everything.json`
- `docs/BENCH_ANALYSIS_2026-11-19.md`
- `REPORT.md`

## 시험

`python -m unittest tests.test_qms_bench_analyze`

```
Ran 22 tests in 0.003s
OK
```

## 남은 격차

현 Rust writer는 실행별 단일 adapter의 분포 요약을 기록하며, 설계 예시의 질의 세트 해시·양쪽 엔진 결과·원시 지연·상위 N 일치율·idle/search/post 자원 구간과 표본은 기록하지 않는다. 분석기는 가능한 스키마를 검증하고 데이터를 조작해 만들지 않는다. 빠진 값 때문에 직접적인 엔진 간 대조, 신뢰도 있는 동일 세트 회귀 확정, 질의당 자원 비용 및 선형 메모리 기울기 산출은 불가하며 보고서에서 해당 제한을 표시한다.
