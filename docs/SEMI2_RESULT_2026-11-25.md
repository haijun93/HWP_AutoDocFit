# SEMI2 — 연관 검색어 공통 backend 통합

공통 SQLite·Rust backend와 같은 JSONL 프로세스·v5 색인에 연관 검색을 통합했다. 필수 Rust 571개 통과·2개 제외, 최종 전체 Python 3,695개 OK(22개 제외), 합성 두 경로 48/48 차이 0, 5천 문서 warm 6조건 중앙값 30ms 이하를 확인했다. 추가 파일명 서비스 시험의 잘못된 SID 1실패는 남아 있으며, 기존 시험이나 SID 검증은 변경하지 않았다.

## 기준과 범위

- 기준: `github/main` v2.2.100 (`8b18b99d`), 브랜치 `feature/SEMI2`.
- 병합 대상: `feature/SEMI1` (`3b4b158e`). 병합 커밋 `c3a3533e`의 부모는 위 두 커밋이다.
- 지정 폴더가 이미 같은 github/main 커밋의 깨끗한 worktree여서, 중복 `worktree add` 대신 그 폴더에서 기능 브랜치를 생성했다. 다른 브랜치의 미커밋 작업을 지우거나 옮기지 않았다.
- 한/글 COM·사용자 문서·GitLab·푸시·태그·배포·설치 서비스 교체는 수행하지 않았다. 합성 사본만 사용했다.

## 충돌 해결과 계약 보존

1. `search_backend.py`: main의 추상 `SearchBackend`, SQLite 기본값, Rust 장기 실행 프로세스, 배치 색인, fallback, shadow 비교를 유지했다. `search(..., related_groups=False, filenames=False)` 옵션을 더하고, 옵션이 켜진 경우만 SEMI1 검색으로 분기한다. 기본 응답 변환은 그대로다.
2. `search.rs`: SRCH6의 프로파일·캐시·`ContentScope`와 SEMI1의 모듈 선언을 함께 유지했다. 공용 자료함 `search`/`search_box`, SRCH6 순위 정책과 집계 계산은 바꾸지 않았다.
3. `search_cli.rs`: main의 `search-content` JSONL 루프·프로파일·색인/검색 명령을 유지하고 `related_groups=true` 분기 및 SEMI1의 추출 완료 문단 입력 명령을 합쳤다. 연관 응답은 기존 SRCH 변환을 다시 거치지 않아 `group_matches` 근거가 보존된다.
4. 자동 병합된 `main.rs`의 중복 모듈 선언과 중복 `search-content` 진입점도 제거했다. main의 환경변수 기반 색인 선택을 유지한다.

연관 검색은 Python의 `related_query.py`, Rust의 `kfind/core/src/related_query.rs`를 재사용한다. 새 파서나 문장 매처를 복제하지 않았다. 묶음 안 문장 단위 부분 문자열 AND → 문서 교집합 → 최고 위계 → 묶음별 일치 문단 수 합 → 정규화 경로 순이다. roots·기간 필터와 offset/limit은 교집합 이후 결과에 적용하며, 후보를 상위 N개로 잘라 교집합을 잃지 않는다. 파일명은 확장자의 점까지 이름 전체를 한 단위로 취급한다.

Rust의 `index_file`/`index_files`와 `index_records`/`index_many` 모두 같은 프로세스·같은 v5 색인에서 연관 검색을 제공한다. native 색인은 기존 Rust 추출기·위계 분류기로 연관 문단 자료를 만들며, SRCH의 기본 본문·순위용 필드는 보존한다. 위계 체계와 문단 번호가 서로 다른 두 검색 경로를 억지로 같은 번호로 바꾸지 않았다. 연관 자료는 파일의 첫 색인 문단에서 **STORED 전용 `body` 필드의 두 번째 값**에 저장한다. 첫 값은 기존 본문이고 추가 값은 검색 토큰에 들어가지 않는다. `sheet`/`row`에 시각·크기를 넣던 SEMI1 전용 저장 방식은 새 입력에서 제거했다.

스키마는 v5다. native 추출기 판을 `srch5-semi2-2`로 올려 다음 색인 요청에서 기존 지문이 같아도 연관 자료를 채운다. 연관 자료가 없는 기존 SRCH v5 색인에 연관 검색을 요청하면 재색인 필요 오류를 내며, 잘못된 근거나 빈 결과를 성공으로 반환하지 않는다. 기본 검색은 기존 색인을 읽을 수 있다.

`shadow_compare`는 SQLite 응답을 계속 반환한다. 연관 옵션에서는 집합·순서·`group_matches` 일치 여부를 함께 기록한다. 정상 종료·취소 시 Python의 자식 프로세스 스트림도 닫도록 보완했다.

연관 자료의 추가 추출에 실패해도 기존 일반 색인은 계속 성공한다. 그 파일이 있는 색인에서 연관 검색을 요청할 때만 오류를 전달해 fallback이 가능하게 했다. 일반 색인·검색의 기존 성공 계약을 보존하는 회귀 시험도 추가했다.

## 검증 결과

검증 소스는 `bbbf578c`로 고정했다. Windows x64, Python 3.15.2 가상환경, 로컬 MinGW GNU Rust 환경이다. 빌드 출력은 저장소 밖 카드 전용 `CARGO_TARGET_DIR`에만 두었다. 실제 Python/Rust 통합 시험에는 `KLIB_EXE=<이번 release klib.exe>`와 기존 SEMI1 시험용 같은 경로를 지정했다. release SHA-256: `6F122D935B8AD00C83FCE7ECD850E356136EBE11ACBCDE52E2DE652FE4FC215B`.

### 필수 Rust 전체 시험: 571개 통과·2개 제외·실패 0

`cargo test --manifest-path klib/Cargo.toml --no-fail-fast -- --test-threads=1` — 종료 코드 0

```text
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.62s
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.39s
test result: ok. 132 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 52.33s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.53s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cargo test --manifest-path kfind/core/Cargo.toml` — 종료 코드 0

```text
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.93s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.66s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.47s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.12s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.26s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.24s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.31s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.69s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cargo test --manifest-path kfind/names/Cargo.toml` — 종료 코드 0

```text
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 23.53s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.21s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cargo test --manifest-path ktm/Cargo.toml` — 종료 코드 0

```text
test result: ok. 260 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.60s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

SRCHFIX1의 네 회귀 시험도 모두 통과했다: `content::block_tests::heading_hit_ranks_before_body_only_hit`, `search::tests::fast_columns_handle_missing_pages_and_multiple_segments`, `search::tests::schema_change_keeps_result_set_and_order_for_flat_levels`, `search::tests::search_matches_original_order_scores_and_snippets`. 기존 시험은 수정하지 않았다.

### 실제 release 집중 회귀

```text
Ran 59 tests in 20.867s
OK
```

`test_srch6_content_search`와 새 `test_semi2_backend_integration`을 포함한다. 같은 프로세스의 일반/연관 검색 공존, native/추출 완료 문단 입력, 기간·roots·페이지, 갱신·삭제·재시작·취소, shadow 순서/근거, 큰 근거 JSON, 추가 추출 실패 시 기본 검색 보존을 확인했다.

### 합성 집합·순서·근거

시드 `20261124`, 생성 문서 1,000개 + 제어 문서 5개. 본문/파일명 각각 12개 조합으로, 추출 완료 문단 입력 **24/24**, native `index_files` 입력 **24/24**가 일치했다. 두 실행 모두 누락·추가·순서·`group_matches` 차이 0이다. 합성 대조 결과는 [원시 결과](SEMI2_PARITY_2026-11-25.json)에 기록했다.

| 질의 | 본문 SQL/Rust | 파일명 SQL/Rust |
|---|---:|---:|
| `행정; 정책` | 1000/1000 | 0/0 |
| `행정; "안전 점검 계획"` | 1000/1000 | 0/0 |
| `행정 정책; 안전 지원` | 993/993 | 0/0 |
| `2026; 공사 계약` | 3/3 | 2/2 |
| `; 2026;; "공사 계약";` | 3/3 | 1/1 |
| `R&D; "a;b"` | 1/1 | 0/0 |
| `행정; 없는검색어` | 0/0 | 0/0 |
| `2026; 2026` | 4/4 | 2/2 |
| `synthetic; 000` | 0/0 | 1000/1000 |
| `""; ;` | 0/0 | 0/0 |
| `report; R&D` | 0/0 | 1/1 |
| `i; "a;b"` | 1/1 | 0/0 |

두 입력 경로 모두 위 문서 수와 일치한다. 래퍼가 메타데이터와 같은 이름의 결과 JSON을 덮어쓴 것을 발견해, 보존된 stdout JSON 원문에서 원시 결과를 복원했다. 래퍼의 출력 파일 이름도 분리했다. 수치와 질의를 다시 실행하거나 바꾸지 않았다.

### 5천 문서 warm 재확인

`scripts/semi2_warm_bench.py --klib-exe <release klib.exe> --output <JSON>`을 한 번 실행했다. 기존 SRCH6 생성기·시드·문서 수·배치 250·limit 20을 사용했고, 다른 빌드·시험과 겹치지 않았다. 일반 질의는 warm 5회, 문서 AND는 7회다. 색인 48.630초. 아래 cold는 질의 유형별 첫 요청이며 OS cold-cache가 아니다.

| 조건 | cold ms | warm 중앙값 ms | warm 최대 ms |
|---|---:|---:|---:|
| 짧은 키워드 | 56.843 | 18.912 | 19.569 |
| 긴 일반 AND | 13.282 | 10.126 | 11.950 |
| 일반 AND | 28.223 | 21.152 | 24.118 |
| phrase | 30.283 | 25.753 | 27.286 |
| 문서 AND 2단어 | 40.013 | 18.155 | 36.171 |
| 문서 AND 5단어 | 56.857 | 18.743 | 19.695 |

6조건 모두 기존 SRCH6와 같은 **warm 중앙값 30ms 이하** 기준을 충족했다. 단일 warm 표본 1개는 36.171ms였다. 모든 요청이 30ms 이하라는 뜻은 아니다. 변경 전 소스의 이번 동일 기기 재측정은 하지 않았으므로 속도 향상이나 지연 불변을 주장하지 않는다. 원시 표본은 [warm 결과](SEMI2_WARM_2026-11-25.json)에 있다. 증분 색인·동시 색인 지연·RSS·디스크·5천 전체 집합 일치는 이번에 재측정하지 않았다.

### 추가 파일명 서비스 시험

`cargo test --manifest-path crates/kfind/Cargo.toml` — 종료 코드 101. 필수 네 Rust 묶음과 별도다.

```text
test result: ok. 39 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.04s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

유효 SID로 별도 임시 검사 프로그램을 실행한 결과(종료 코드 0):

```text
valid SID: names_related evidence OK; legacy names_query items unchanged; invalid SID rejected
```

### 전체 Python — 실제 release 실행 파일

`python -m unittest discover -s tests`를 같은 소스로 단독 실행했다. 첫 실행은 런처 통합 시험 1건의 네이티브 종료 코드 `3221225786`으로 실패했다. `tests/test_smart_launcher.py`와 런처 소스는 이번 변경 대상이 아니며, 시험/런처/검색 코드를 수정하지 않고 해당 시험을 단독 재실행한 뒤 전체를 한 번 더 실행했다.

첫 전체 실행(종료 코드 1):

```text
Ran 3695 tests in 927.060s
FAILED (errors=1, skipped=22)
```

실패 시험 단독 재실행(종료 코드 0):

```text
Ran 1 test in 9.781s
OK
```

최종 전체 재실행(종료 코드 0):

```text
Ran 3695 tests in 903.680s
OK (skipped=22)
```

두 전체 실행 동안 소스는 `bbbf578c`와 같았고, 중간 커밋 `5b1d85ea`은 사용자 지정 30분 기록용 진행 문서 한 줄만 더했다. COM 생성 차단 환경에서 차단 요청 **0건**, 수집한 Rust stderr **0바이트**였다. 시험이 변경한 binary fixture 4개는 각 실행 후 원복했다. 기존 ResourceWarning과 종료 후 sandbox SQLite 잠금 정리 `PermissionError`는 남았으며 최종 unittest 결과·종료 코드는 성공이다. 초기 런처 조기 종료의 원인은 확인하지 못했다. 검증 요약은 [원시 요약](SEMI2_VALIDATION_2026-11-25.json)에 기록했다.

## 예비 시험에서 발견한 사항

- 추가 실행한 `cargo test --manifest-path crates/kfind/Cargo.toml`에서 SEMI1이 가져온 `names_related_command_carries_evidence` 1건은 `invalid client SID`로 실패했다. 시험의 `S-1-5-21-semi1`은 현재 main의 숫자 SID 검증을 통과하지 못한다. 기존 시험 수정 금지 규칙에 따라 그 시험은 수정하지 않았고, 서비스의 SID 검증도 완화하지 않았다. 나머지 서비스 단위 39건과 기존 names 파이프 1건은 통과했다. 유효 SID의 별도 확인 결과는 최종 검증에 기록한다.
- Debug 예비 집중 시험에서 대용량 문서의 기본 검색 프로세스가 한 번 응답 전에 종료됐다(20개 중 오류 1). 해당 시험 단독 재실행과 같은 JSONL 요청의 별도 재현은 통과했다. 당시 stderr가 수집되지 않아 종료 원인은 확인하지 못했다. 최종 release 집중·전체 시험 결과와 구분한다.
- 초기 근거 저장 방식을 검토해, JSON을 색인되는 `location` 값에 두던 중간 구현을 저장 전용 값으로 보완했다. 최종 시험은 보완 후 소스를 사용한다.

## 확인하지 않은 범위와 한계

- 실제 사용자 문서·한/글 COM·설치본·설치된 파일명 서비스·실제 ktm 화면·맥 빌드는 확인하지 않았다.
- 연관 검색 자체의 5천 문서 지연 목표는 측정하지 않았다. 이번 30ms 판정은 기존 SRCH6 일반 질의 조건이다. 연관 경로는 부분 문자열·기호·한 글자 질의의 누락을 막기 위해 저장 문단을 전수 확인한다.
- 실제 파일 형식별 추출·위계 근거의 전수 일치는 확인하지 않았다. 검증 코퍼스는 합성 TXT/Markdown이며, 실문서 품질이나 모든 형식의 결과 일치를 뜻하지 않는다.
- 기존 v5 색인에 연관 자료가 없으면 재색인이 필요하다. 기본 backend 승격·자동 옵션 활성화는 하지 않았다.
