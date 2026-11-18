# ANNUAL4 작업 보고서: 업무 연보 인수인계·실적 보고서 데이터 구조(JSON) 출력

- **작업 일시**: 2026-11-19
- **작업 브랜치**: `feature/ANNUAL4`
- **대상 크레이트**: `kfind/annual` (Rust, 플랫폼 독립)

---

## 1. 구현 내용 요약

`docs/ANNUALUI_GAP_2026-11-18.md`에 명시된 남은 격차 중 '인수인계·실적 보고서 출력'의 첫 단계로, Windows·한/글 환경의 HWPX 생성 모듈이 그대로 읽어 표, 차트, 타임라인을 채울 수 있는 **보고서 데이터 JSON** 구조 및 출력 기능을 구현했습니다.

1. **보고서 스키마 문서 작성 (`docs/ANNUAL_REPORT_SCHEMA_2026-11-18.md`)**:
   - 최상위 객체 및 세부 객체 구조 정의 (한글 필드 설명 표 포함)
   - 포함 항목:
     - `period`: 전체 분석 기간 (시작일 `start`, 종료일 `end`)
     - `total_docs`: 분석 기간 내 총 문서 수
     - `monthly_docs`: 1월~12월 월별 작업 문서 수 배열
     - `top_topics`: 상위 N개 주제 (주제명, 문서 수, 비중, 대표 문서 최대 3건 경로, 활동 기간)
     - `week_day_heatmap`: ISO 연도 및 주차별 요일(월~일) 작업량 열지도 요약 및 합계
     - `timeline`: 주요 업무 타임라인 (주제별 시작 월, 종료 월, 월별 문서 수 상세 내역)
   - 완전한 JSON 예시 제공

2. **JSON 이스케이프 및 직렬화 엔진 (`kfind/annual/src/output.rs`)**:
   - `serde` derive나 `serde_json` 매크로 직렬화 대신 경량 문자열 이스케이프 함수 `json_escape_str`을 구현하여 자체 직렬화 (`kc1_bench.rs`/`klib` 패턴 준수, 새 의존성 0건).
   - 한글 문자는 UTF-8 그대로 보존되며, 따옴표(`"`), 역슬래시(`\`), 줄바꿈(`\n`, `\r`), 탭(`\t`), 0x20 미만의 ASCII 제어 문자는 RFC 8259에 맞춰 안전하게 이스케이프.
   - 2칸 들여쓰기(pretty-print)를 적용하여 사람이 읽기 쉽고 디버깅에 용이한 JSON 구조 생성.

3. **원자적 파일 교체 출력 함수 (`write_report_json`)**:
   - 동일 디렉토리에 `.tmp` 임시 파일을 생성하여 JSON을 온전히 기록한 후 `fs::rename`으로 원자적 교체 수행.
   - 기존 `write_outputs` 파이프라인에 `보고서_데이터.json` 출력을 통합.

4. **입력 구조 확장 및 라이브러리 인터페이스 (`kfind/annual/src/lib.rs`)**:
   - `AnnualInput`에 `pub docs: Vec<Doc>` 선택 필드를 추가(역호환성 100% 유지)하여 대표 문서 경로 및 상세 기간 추출 지원.
   - `render_report_json`, `write_report_json`, `json_escape_str`을 `kfind_annual` 루트에서 re-export.

5. **단위 테스트 추가 (`kfind/annual/src/output.rs`)**:
   - `test_report_json_empty_input`: 빈 입력(`AnnualInput::default()`)에 대해 유효한 JSON 스키마 및 기본값 출력 검증.
   - `test_report_json_korean_and_escaping`: 한글 문자 보존 및 따옴표/역슬래시/개행/탭 이스케이프 후 JSON 파서 복원 검증.
   - `test_report_json_iso_year_boundary`: 2025-W52와 2026-W01 등 연도 경계 주차가 분리되어 정확히 직렬화되는지 검증.
   - `test_write_report_json_atomic_replacement`: 임시 파일 기반 원자적 교체 및 `write_outputs`를 통한 `보고서_데이터.json` 생성 검증.

---

## 2. 테스트 결과

실행 명령: `CARGO_TARGET_DIR=$HOME/cargo-target-ANNUAL4 cargo test --manifest-path kfind/annual/Cargo.toml`

```text
test result: ok. 49 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

기존 45개 테스트가 모두 유지·통과되었으며, 신규 추가된 4개 테스트를 포함하여 총 49개 테스트가 모두 성공했습니다.
