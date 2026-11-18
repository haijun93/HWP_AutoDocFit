# OUTLINER6 작업 보고

## 변경

- 여러 페이지를 대상으로 `status`, `tag`, `link`, `prop`, `text` 조건을 공백 AND로 처리하는 쿼리 API와 JSON `query`를 추가했다. 페이지 입력 순서 및 문서 블록 순서를 유지하며 블록 경로, 본문, 상태를 반환한다.
- 일지 페이지 이름 `YYYY_MM_DD` 생성·역변환, 주입 날짜 기준 전일/당일/익일 계산, 유효 일지명의 최근순 목록 API와 JSON `journal_name`, `journal_list`를 추가했다.
- 쿼리는 빈 조건·알 수 없는/잘못된 조건·짧은 텍스트 조건·10 KiB 초과 입력을 코드화된 오류로 반환한다. 날짜는 윤년 및 월별 일수를 검증한다.
- 기존 `daily_page`의 ISO 하이픈 계약은 유지했다. 기존 시험 파일은 삭제하거나 약화하지 않았다.

## 검증

실행: `CARGO_TARGET_DIR=/private/tmp/cargo-target-OUTLINER6 cargo test --manifest-path outliner_core/Cargo.toml`

홈 디렉터리 지정 타깃 경로는 샌드박스 권한으로 생성되지 않아 허용된 `/private/tmp` 경로를 사용했다. 새 의존성은 추가하지 않았다.

```text
test result: ok. 58 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.27s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
