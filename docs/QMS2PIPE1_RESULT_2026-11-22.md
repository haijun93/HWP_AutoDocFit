# QMS2PIPE1 결과 — 파일명 검색 `;` 다중 묶음 교집합 (2026-11-22)

## 변경
- `kfind/names/src/query.rs`
  - `split_groups`/`parse_groups`: `;` 로 묶음 분리. 따옴표 안의 `;`, `ext:hwp;pdf` 처럼 `ext:` 낱말 안에서 바로 뒤에 글자가 이어지는 `;` 는 가르지 않음. 빈 묶음은 버림, 전부 비면 빈 질의 1개. `;` 가 없으면 `vec![parse(text)]` 와 동일.
  - `Searcher::search_groups`: 묶음 1개면 기존 `search` 를 그대로 호출(차이 0, 앞 결과 좁히기 캐시도 유지). 2개 이상이면 희소 순서(1차 낱말 길이 > 확장자 > 경로 조건 > 낱말 수)로 정렬해 가장 희소한 묶음을 먼저 훑고, 나머지는 그 후보만 확인하는 단락 평가. 후보가 0이 되면 남은 묶음은 평가하지 않음. 후보 확인은 `verify_full`(경로·제외 폴더 조건까지 묶음별로 확인; 기존 `verify` 는 같은 경로 조건 전제라 쓸 수 없음).
- `crates/kfind/src/names.rs`: `names_query` 가 `parse_groups`/`search_groups` 사용. 묶음 최대 8개(`MAX_GROUPS`), 초과 시 `too many groups` 오류.
- **파이프 스키마는 변경 없음**: 신규 필드를 만들지 않고 기존 `text` 의 `;` 를 해석. 구버전 요청 JSON 은 그대로 동작하고(`deny_unknown_fields` 영향 없음), 새 클라이언트가 구버전 서비스에 `;` 질의를 보내면 예전처럼 `;` 가 무시된 AND 로 동작(점진 열화).
- 동작 변화(의도): 예전 `"예산;공사"` 는 한 낱말 `예산공사` 로 접혔으나 이제 두 묶음 교집합.
- 설계 문서의 `groups` 필드(§4.3)는 이 카드에서 만들지 않음(내용 검색 파이프 `query` 쪽은 다음 카드 몫).

## 시험 (`cargo test --release --locked`, 모든 `test result` 줄)
kfind/names:
```
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s   (lib)
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s    (groups, 신규)
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 31.11s   (oracle)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.61s    (result_order)
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s    (sort_meta)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s    (doc-tests)
```
crates/kfind:
```
test result: ok. 39 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.26s   (lib)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s    (main)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s    (names_pipe, `;` 파이프 시험 추가)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.43s    (user_mode)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s    (doc-tests)
```
신규 시험 `kfind/names/tests/groups.rs`: 1묶음=기존 `search` 와 total·top 동일(정렬 3종, 여러 질의), 2~3묶음을 묶음별 개별 검색의 교집합 기준과 대조, 묶음별 `path:`/`-제외`/`folder:`, 빈 묶음, 한글·영문 혼합, `ext:hwp;pdf` 보존, USN 덧붙임 항목 포함, 구버전 요청(`;` 없는 text) 동일성. `query.rs` 단위 시험으로 분리·파싱 규칙.

## 성능 (1묶음 전후)
`qms_bench` 는 실제 Everything/HTTP 대상 도구라 이 카드의 합성 비교에 맞지 않아 쓰지 않고, 새 예제 `kfind/names/examples/groups_bench.rs`(결정적 합성 색인 2,002,001 항목, 15회 중앙값, 이 PC 일회 측정이라 ±1~2ms 잡음)로 비교. 1묶음은 `Searcher::search`(서비스가 쓰던 경로) 대 `search_groups`:

| 질의 | 이전 ms | 이후 ms |
|---|---|---|
| 보고서 | 16.35 / 15.50 | 16.52 / 16.06 |
| 예산 집행 | 9.06 / 10.87 | 9.11 / 9.75 |
| report ext:pdf | 10.00 / 9.30 | 10.47 / 10.92 |
| 자료 -draft | 15.89 / 19.11 | 15.93 / 16.93 |

(두 번 실행 값.) 1묶음은 같은 코드 경로를 호출하므로 차이는 잡음 범위. 다묶음: `보고서; 예산` 17.9~19.0 ms(total 28,532), `report; plan; ext:pdf` 17.3~20.0 ms(6,977), `공사 계약; 자료; ext:hwp` 9.0~10.9 ms(0).

## 남은 문제
- 희소 순서는 통계가 아니라 휴리스틱(낱말 길이 등). 실제 빈도 추정은 안 함.
- 여러 묶음일 때는 입력 연장 시 앞 결과 좁히기 캐시를 쓰지 않음(매번 전체 훑기; 위 수치가 그 비용).
- 내용 검색 파이프 `query` 의 `groups` 필드, ktm UI/백엔드 연동, 묶음별 강조는 이 카드 범위 밖.
- 실행 중인 색인 서비스·kfind 프로세스는 건드리지 않았고(임시 색인·임시 폴더 시험만), 실서비스에 대한 실측은 하지 않음.
