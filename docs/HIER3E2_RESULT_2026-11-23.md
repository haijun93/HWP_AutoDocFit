# HIER3E2 결과 보고서

작성일: 2026-11-23

## 원인

HIER3E1은 HWPX 표 셀 문단을 보존하려고 위치 추출기에서 표 안 `<p>`를 건너뛰던 처리를 제거했다. 합성 문서의 표는 빈 바깥 `<p>`가 `<tbl>`을 감싸고 그 안에 실제 셀 문단 `<p>`가 있는 구조다. 계층 추출기는 빈 바깥 문단을 제외하지만 위치 추출기는 그 래퍼의 `itertext()` fallback으로 셀 텍스트를 얻어 계층 레코드를 먼저 소비했다. 이후 실제 셀 문단도 별도 추출되어 같은 문단이 2개 레코드가 되고, 검색 출현 수도 2배가 됐다.

## 수정 사항

- HWPX 텍스트 fallback에서 중첩 `<p>`·`<tbl>`을 건너뛰어 바깥 표 래퍼가 셀 문단 텍스트를 가져오지 않도록 했다. 실제 텍스트가 있는 문단만 계층 레코드를 소비한다.
- 표 안 실제 문단은 유지하면서 중복 기록을 막았다. 행·열 메타데이터 전달 및 1x1·1행 표의 장 제목 위계 판정도 적용했다. 일반 데이터 표는 `table_cell`로 남는다.
- 기존 `제N장` 시작 문단 판정 회귀도 보완했다. HWPX의 단독 장 제목은 장으로 인식하고, 본문 중간 인용은 장으로 인식하지 않도록 Python/Rust 규칙과 회귀 시험을 맞췄다. 접두 `[붙임 N]`, `【별표】`, `(별첨)` 처리도 유지했다.
- 검색 일치 수를 확인하는 새 합성 시험을 추가했다: 표 문단 1개 → 추출 레코드 1개 → 검색 일치 수 1개.
- 의존성 추가 및 사용자 문서 사용은 없다. `CURRENT_HIERARCHY_SCHEMA_VERSION`은 기존 지시대로 올리지 않았다.

## Python 시험

`TMPDIR=/private/tmp python3 -m unittest discover -s tests -p 'test_timemachine_*.py'` 결과는 307건 중 305건 통과, `test_timemachine_core.py`에서 2건 실패다. 개별 모듈 결과:

```text
Ran 7 tests in 0.002s
OK
Ran 8 tests in 0.669s
OK
Ran 85 tests in 10.262s
OK
Ran 15 tests in 6.152s
FAILED (failures=2)
Ran 5 tests in 0.026s
OK
Ran 20 tests in 0.253s
OK
Ran 14 tests in 0.465s
OK
Ran 5 tests in 0.046s
OK
Ran 11 tests in 0.111s
OK
Ran 11 tests in 0.075s
OK
Ran 6 tests in 0.052s
OK
Ran 6 tests in 0.036s
OK
Ran 79 tests in 1.137s
OK
Ran 5 tests in 0.016s
OK
Ran 30 tests in 0.527s
OK
```

위 순서는 `test_timemachine_basedate.py`, `test_timemachine_content_index_lifecycle.py`, `test_timemachine_content_search.py`, `test_timemachine_core.py`, `test_timemachine_extensions.py`, `test_timemachine_extractors.py`, `test_timemachine_idle_indexing.py`, `test_timemachine_ieeee_exclusion.py`, `test_timemachine_kfind.py`, `test_timemachine_related.py`, `test_timemachine_related_order.py`, `test_timemachine_series.py`, `test_timemachine_service.py`, `test_timemachine_thumbnail_cache.py`, `test_timemachine_thumbs.py`다.

```text
Ran 7 tests in 1.014s
OK
Ran 5 tests in 0.029s
OK
```

위 두 결과는 각각 `test_hu2_search_hierarchy.py`, `test_eval_hierarchy_search.py`다. 요청된 기존 3개 실패 시험과 새 회귀 시험, 합성 1x1 표 추출 시험도 별도로 실행해 5건 모두 통과했다.

전체 307건 실행에서는 다음 두 시험이 통과하지 못했다.

- `test_initial_scan_excludes_and_queries`: macOS에서 `os.utime`으로 수정 시각을 설정해도 생성/변경 시각 의미가 Windows와 다르므로 시험의 `copied` 플래그 전제가 맞지 않는다.
- `test_query_speed_month_range`: macOS/Python 3.14에서 100,000행 검색이 6.31초여서 시험의 0.5초 상한을 넘었다.

두 시험 기대값이나 코드는 변경하지 않았다. `/var`와 `/private/var` 임시 경로 별칭 때문에 처음 전체 실행에서 추가로 나타났던 색인 경로 실패는 `TMPDIR=/private/tmp`로 다시 실행해 해소했다.

## Rust 시험

`CARGO_TARGET_DIR=/private/tmp/cargo-target-HIER3E2 cargo test --manifest-path hierarchy/Cargo.toml` 통과:

```text
test result: ok. 61 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.47s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.40s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`hierarchy/src/search_levels.rs`를 수정했으므로 PM은 Windows PC에서 Cargo 시험을 다시 실행해야 한다.

## 작업 제한 및 상태

현재 작업 브랜치는 `feature/HIER3E2`다. 이 작업 폴더에는 GitHub 원격이 아니라 로컬 bundle을 가리키는 `origin`만 등록되어 있어 GitHub `main` 최신 여부는 확인하지 않았다. GitHub/GitLab 원격 작업은 하지 않았다. 실행 환경 지시에 따라 커밋과 푸시는 하지 않았다. `git diff --check`는 통과했다. 시험 생성 과정에서 변경된 추적 fixture는 원래 내용으로 되돌렸다.
