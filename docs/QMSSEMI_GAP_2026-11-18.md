# QMSSEMI 몽땅검색 연관 검색어 격차 (2026-11-18)

기준 TODO: `몽땅검색 연관 검색어 ;` — `2026; 공사 계약`을 묶음마다 문장 단위 검색하고 문서 단위로 교집합, 순위는 최고 위계 우선·묶음별 일치 수 합. `&`는 사용하지 않음.

## 구현/미구현 대조

| 영역 | 상태 | 근거 |
|---|---|---|
| 질의 나누기 | 구현 | `kfind/core/src/query.rs`의 `parse_groups`: 따옴표 밖 `;` 분리, 빈 묶음 무시, 인용 구간 보존. 관련 단위 시험 존재. |
| 문장 단위 검색 및 문서 교집합 | 구현 | `kfind/core/src/index.rs`의 `search_groups`/`rank_groups`: 묶음별 ranking 결과의 `doc_id`를 교차시키고 묶음별 검색 결과가 있어야 반환. |
| 순위 | 구현 | 묶음별 최고 위계(`best_level`) 오름차순, 일치 문장 합(`sum_count`) 내림차순, 날짜·문서 ID 후순위. |
| 연관 문장 응답 | 구현 | `search_groups`가 묶음별 최고 문장을 선택해 `DocHit.sentences`에 채움. |
| legacy 동일 의미 | 부분 구현/미확인 | kfind core와 ktm 본문 검색은 `search_groups` 사용. `crates/kfind` 서비스 파이프는 단일 질의 요청만 받으며 연관 그룹 필드를 별도로 갖지 않음. |
| ktm 검색 묶음 칩 데이터 | 미구현 → 이번 카드 구현 | `Answer`에는 강조 `terms`만 있고 묶음 구분은 전달하지 않음. 검색 문자열에서 묶음 목록을 보존하도록 확장. |
| 실제 칩 표시 | 미구현 | `ktm/src/window.rs` 검색창/결과 화면에 묶음 칩 렌더링 없음. |
| 묶음별 결과 강조 | 미구현 | 현재 발췌 강조는 모든 검색어를 한 목록으로 적용하며 결과 문장과 질의 묶음의 대응 정보가 UI에 없음. |
| 파일명 연관 검색 및 칩 | 미구현 | 파일명(names) 검색은 기존 단일 질의 경로이며 그룹 교집합 동작 없음. |

## 이번 변경 범위

ktm 검색 응답에 묶음 문자열 목록을 추가해 UI에서 칩으로 나타낼 기반을 제공하고, 목록을 고정하는 단위 시험을 추가한다. 나머지 격차는 후속 카드로 남긴다.

## 변경 파일

- `docs/QMSSEMI_GAP_2026-11-18.md`
- `ktm/src/backend.rs`
- `ktm/src/window.rs`
- `ktm/src/view_dump.rs`

## 시험 결과

PATH 앞에 `C:\Users\haiju\AppData\Local\DocFitTools\mingw64\bin`, `CARGO_TARGET_DIR=E:\cargo-target\qmssemi`를 설정해 세 manifest 전체 `cargo test`를 실행했다. 모든 결과 줄:

```text
kfind/core:
test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.87s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.98s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.27s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.39s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.29s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.48s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.07s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.45s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.04s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 17.08s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.74s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.25s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.23s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

ktm:
test result: ok. 224 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.14s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

crates/kfind:
test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.59s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.51s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 남은 격차

실제 ktm 칩 렌더링(이번에는 Answer 응답 분해 시 그룹 목록은 소비하지 않음), 묶음별 문장 강조, 파일명(names) 경로의 문서 교집합 및 legacy 서비스 파이프 확장. 작업 트리에서 diff 공백 검사는 통과했다.
