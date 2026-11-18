# QMSSEMI3 검증 보고

## 변경 사항

- `kfind/core/tests/groups_oracle.rs`에 색인 없이 합성 문서 원문을 직접 훑는 고정 시드 오라클 시험을 추가했다. 48개 문서(192문장)와 40개 질의를 사용하며, 1~3개 묶음, 두 글자 낱말, 조사 부착, 공백·문장부호 무시, 제외 문서, 묶음 순서 교환, 빈 묶음, 중복 묶음을 다룬다.
- 시험에서 `search_groups`의 문서 집합은 오라클과 일치했다. 그러나 `count_groups.total_docs`는 40개 질의 중 29개에서 `search_groups.total_docs`와 달랐다.
- 재현 질의 `공사; 계약`: 오라클 및 `search_groups` 기대 문서 `[1, 14, 20, 26, 39, 45]`(6건), `count_groups` 결과 23건.
- 구현은 수정하지 않았다. 상세 재현 및 전체 불일치 질의는 [QMSSEMI3 findings](docs/QMSSEMI3_FINDINGS_2026-11-18.md)에 기록했다.

## 시험 결과

실행 명령: `CARGO_TARGET_DIR=/private/tmp/cargo-target-QMSSEMI3 cargo test --manifest-path kfind/core/Cargo.toml --no-fail-fast`

`$HOME/cargo-target-QMSSEMI3` 사용은 샌드박스에서 쓰기 권한 오류로 거부되어, 같은 조건의 작업 폴더 외부인 `/private/tmp`를 사용했다. 전체 시험 실행은 오라클이 찾아낸 `count_groups` 불일치 때문에 실패했다.

```text
test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.68s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.86s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.69s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.94s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.86s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.78s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.55s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.89s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
