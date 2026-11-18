# ANNUALUI 업무 연보 격차 구현 (2026-11-18)

## 조사 및 대조

근거: `TODO.md` 업무 연보 항목, `docs/ANNUAL_A1_2026-11-14.md`, `kfind/annual/src/{lib,stats,output}.rs`.

| 항목 | 상태 | 확인 근거 |
|---|---|---|
| 파일명·제목·중소제목 기반 주제 묶기 | 구현 | A1 `analyze`, `cluster_topics` |
| 월·주 통계 및 주간 열지도 | 분석 구조 구현, 파일 출력 누락 | `Stats.topic_by_month`, `topic_by_iso_week`, `week_day_heatmap`; 기존 출력은 월별 CSV만 |
| 타임라인 및 버전 줄 | 버전 데이터 구현, 타임라인 렌더링 미구현 | `Report.version_lines`; A2 계획만 확인 |
| 인수인계·실적 보고서 HWPX | 미구현 | A3/A4 계획만 확인, HWPX 출력 없음 |
| 엑셀 통계 결과 | 부분 구현 | 월별·주기 업무·폴더 이력 Excel 호환 CSV, 주차·요일 데이터 파일 누락 |

## 선택한 범위

가장 작은 시험 가능한 격차로 기존 `Report.stats` 데이터에서 주제×ISO주 및 ISO주×요일 열지도를 Excel 호환 UTF-8 BOM CSV로 내보내는 파일 2종을 추가했다. HWPX 템플릿 자동 채우기와 타임라인 렌더링은 범위에서 제외했다. ISO 연도는 `AnnualInput.iso_week_years` 선택 입력에서 제공하며, 기존 입력은 읽히고 연도 칸을 비워 둔다.

## 변경 및 검증

변경 파일:

- `kfind/annual/src/output.rs`: 주차별 주제·요일 열지도 CSV 생성 및 BOM 파일 출력, 연도 매핑 선택 필드, 직렬화 결과·CSV 헤더/행·파일 생성 시험 추가.
- `docs/ANNUALUI_GAP_2026-11-18.md`: 구현 전 대조표 및 결과 기록.

`cargo test --manifest-path kfind/annual/Cargo.toml` (PATH 앞에 `C:/Users/haiju/AppData/Local/DocFitTools/mingw64/bin`, `CARGO_TARGET_DIR=E:/cargo-target/annualui`):

```text
test result: ok. 40 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`git diff --check`: 통과.

## 남은 격차

- 주차 통계 구조는 ISO 연도별이 아닌 주 번호별 합산이므로 여러 해를 한 분석 기간에 넣으면 같은 주차가 합쳐진다. CSV 연도 매핑은 이를 분리하지 못한다. 통계 모델 개선이 필요하다.
- 타임라인 시각화와 HWPX 인수인계·실적 보고서 출력은 구현되지 않았다.
- 실 Excel 앱 열기와 한/글 COM 출력은 요청 범위 밖이라 시험하지 않았다.
