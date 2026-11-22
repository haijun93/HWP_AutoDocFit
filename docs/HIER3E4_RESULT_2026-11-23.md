# HIER3E4 결과 보고서

작성일: 2026-11-23

## 변경 내용

- `extract_paragraphs()`가 HWP5 본문에서 받은 klib 문단 레코드를 기존 `classify_paragraph_levels()`로 전달하는 경로를 확인했다. HWP는 HWPX/DOCX와 달리 별도 위계 추출 선행 기록을 만들지 않아 같은 문단이 두 번 합쳐지지 않는다.
- 텍스트 기반 `제N장`, 연속 번호(Ⅰ./1./가.), `□`, `ㅇ`, `-`, `*` 분류가 HWP에서도 실행되도록 Python 분류기의 HWP 확장자 분기를 추가했다. 장 제목 단독 표기도 HWPX와 같이 판정한다.
- Rust `hierarchy::search_levels`에 HWP/HWPX 텍스트 위계 처리를 적용하고 양쪽 입력의 결과가 같은지 단위 시험을 추가했다.
- HWP klib 레코드의 `is_table_cell`을 기존 `table_cell` 레벨로 유지한다. 글상자 구역(`location_kind=textbox`)은 기존 `글상자` 섹션 위치로 보존하고 텍스트 기호 위계 판정을 받는다.
- 합성 klib JSON 입력으로 HWP 추출 → 분류 → 색인 → 검색을 검증하는 Python 시험을 추가했다. 문단 14개가 14개 레코드로 남고, 표 셀은 1회 기록되며, 고유 검색어 언급 수도 1회다. 동일 텍스트의 HWPX 합성 입력과 레벨 배열도 일치한다.
- 사용자 문서, 한/글 COM, Windows 전용 API, 새 의존 크레이트는 사용하지 않았다.

## 색인 스키마

`CURRENT_HIERARCHY_SCHEMA_VERSION`은 5로 유지했다. 기존 HWP 색인의 위계 재판정 필요성을 기록하며, 자동 재판정 전환은 HIER3D-5 범위다.

## 변경 전 기준선

작업 트리는 `feature/HIER3E4`이며 `github/main`의 `d7beb88b`에서 시작했다. 요청한 결과 문서 중 `docs/HIER3E1_RESULT_2026-11-23.md`는 작업 트리에 없어 읽지 못했다. `docs/HIER3E2_RESULT_2026-11-23.md`는 읽었다.

`.venv\Scripts\python.exe`가 없어서 시스템 Python 3.14.0과 설치된 defusedxml 0.7.1로 기준선을 실행했다. 17개 모듈, 319건 모두 통과했다.

## Python 시험 결과

변경 후 같은 17개 모듈에서 320건 모두 통과했다. 모듈별 `Ran/OK` 결과:

```text
Ran 7 tests in 0.009s — OK (test_timemachine_basedate)
Ran 8 tests in 2.386s — OK (test_timemachine_content_index_lifecycle)
Ran 86 tests in 29.625s — OK (test_timemachine_content_search)
Ran 15 tests in 3.462s — OK (test_timemachine_core)
Ran 5 tests in 0.551s — OK (test_timemachine_extensions)
Ran 20 tests in 0.779s — OK (test_timemachine_extractors)
Ran 14 tests in 3.401s — OK (test_timemachine_idle_indexing)
Ran 5 tests in 0.584s — OK (test_timemachine_ieeee_exclusion)
Ran 11 tests in 1.630s — OK (test_timemachine_kfind)
Ran 11 tests in 1.274s — OK (test_timemachine_related)
Ran 6 tests in 0.516s — OK (test_timemachine_related_order)
Ran 6 tests in 0.334s — OK (test_timemachine_series)
Ran 79 tests in 14.282s — OK (test_timemachine_service)
Ran 5 tests in 0.310s — OK (test_timemachine_thumbnail_cache)
Ran 30 tests in 2.585s — OK (test_timemachine_thumbs)
Ran 7 tests in 0.102s — OK (test_hu2_search_hierarchy)
Ran 5 tests in 1.788s — OK (test_eval_hierarchy_search)
```

처음 추가한 회귀 시험은 검색 검증에서 아직 색인을 만들지 않은 상태를 검사하도록 작성되어 한 차례 실패했다. 색인 동작을 시험 범위에 포함한 뒤 통과했으며, 최종 Python 결과는 위와 같다.

## Rust 시험 결과

`PATH` 앞에 `%LOCALAPPDATA%\DocFitTools\mingw64\bin`을 추가하고 `CARGO_TARGET_DIR=E:\docfit-tmp\cargo-target-HIER3E4` 환경 변수 및 `cargo test --manifest-path hierarchy/Cargo.toml -j 2`로 실행했다.

```text
test result: ok. 62 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.34s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.72s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 마무리

`git diff --check` 통과. 시험 중 변경된 추적 바이너리 fixture 4개는 원래 내용으로 복구했다. 색인 스키마 버전 변경, 푸시, 태그, 릴리스는 하지 않는다.
