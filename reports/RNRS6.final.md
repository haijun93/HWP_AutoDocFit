# RNRS6 — 이름 바꾸기 Rust 미리보기 속도 판정

- 기준 커밋: `99388c6cd5316c61208f2b17a4882c4baf529373` (`feature/rename-rust`)
- 실행 PC: Windows, Python 3.14.0 x64; 요청 지정 Python 환경
- Rust: `cargo build --release --manifest-path klib/Cargo.toml` 성공 (klib release)
- 방법: 파일 시스템을 변경하지 않는 메모리 `Entry` 1,000개/5,000개 생성. Python `build_plan`과 klib `--rename-preview-json @request.json`을 각각 7회 실행해 중앙값 기록. Rust 측정에는 JSON 임시 요청 파일 준비, 새 프로세스 1회 호출, stdout 수신 및 JSON 파싱이 포함된다. Python은 이미 준비된 Entry/rules에 대한 서비스 미리보기 계획 생성 시간이다. 파일 목록 수집 및 UI IPC 시간은 제외.
- 데이터 이름: `IMG_000000 보고서(최종).JPG` 형식, 20개 폴더에 분산, mtime/ctime 고정 기준 시각에 항목별 초 증가. 결과 이름은 세 조합에서 Python과 Rust가 일치했다.

| 파일 수 | 규칙 3개 조합 | Python 중앙값 (ms) | klib 프로세스 포함 (ms) | 판정 |
|---:|---|---:|---:|---|
| 1,000 | replace + add + ext_case | 56.49 | 52.70 | Rust 1.07배 빠름 |
| 1,000 | digits_pad + cleanup + ext_case | 44.62 | 293.65 | Python 6.58배 빠름 |
| 1,000 | date + number + ext_case | 62.37 | 41.57 | Rust 1.50배 빠름 |
| 5,000 | replace + add + ext_case | 290.00 | 155.77 | Rust 1.86배 빠름 |
| 5,000 | digits_pad + cleanup + ext_case | 228.34 | 832.28 | Python 3.64배 빠름 |
| 5,000 | date + number + ext_case | 304.28 | 140.88 | Rust 2.16배 빠름 |

## 판정

세 조합 중 `digits_pad + cleanup + ext_case`가 두 파일 크기 모두 Python보다 느리므로 RNRS6 연결 게이트를 통과하지 못했다. 앱에는 `rust_rename_preview` 설정이나 Rust 호출/fallback을 추가하지 않았다. 따라서 가짜 klib 실패 대체 시험과 앱 회귀시험은 연결 조건이 성립하지 않아 실행하지 않았다.

`klib/src/rename.rs`는 `cleanup`의 조합 비용이 큰 것으로 관찰되지만, 본 측정은 총 미리보기 시간을 비교한 것이며 개별 병목 프로파일링은 수행하지 않았다. 이 보고서는 해당 커밋의 측정 결과만 기록한다.
