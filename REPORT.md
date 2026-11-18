# ANNUAL3 작업 보고서

## 구현

- `kfind/annual/src/output.rs`에 외부 의존성 없는 SVG 문자열 렌더러 3종 추가: 월별 문서 수 막대, ISO 주차×요일 열지도, 주제별 월 타임라인.
- SVG마다 UTF-8 문자열, 한글 글꼴 fallback 목록, 접근성용 `<title>`·`<desc>`를 포함하며 색상 상수는 `chart_colors` 한 곳에 정의.
- XML 특수문자 이스케이프, 빈 통계·0 값 처리, ISO 연도와 주차를 보존.
- `write_svg`는 인접 임시 파일에 쓴 뒤 rename하고, `write_outputs`에서 SVG 3종도 출력.
- 태그 균형, 비례 막대, 빈 데이터, 한글/XML 이스케이프, 2020-W53, 파일 출력 시험 추가.

## 검증

- `git diff --check`: 통과.
- `CARGO_TARGET_DIR="$HOME/cargo-target-ANNUAL3" cargo test --manifest-path kfind/annual/Cargo.toml`: 실행 불가. `errno 0.3.15` 다운로드 중 `static.crates.io` DNS 조회 실패.
- `CARGO_TARGET_DIR="$HOME/cargo-target-ANNUAL3" cargo test --offline --manifest-path kfind/annual/Cargo.toml`: 실행 불가. 로컬 캐시에 `errno v0.3.15`가 없음.
- 따라서 Cargo 테스트 출력의 `test result:` 줄은 생성되지 않음.
