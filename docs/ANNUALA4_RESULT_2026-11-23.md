# ANNUALA4 결과 — 업무 연보 HWPX 보고서 생성기

작성일: 2026-11-23
브랜치: `feature/ANNUALA4` (`github/main` 기준)

## 구현

- `kfind/annual/src/hwpx.rs`에 Rust HWPX 생성 모듈을 추가했다. `AnnualInput`과 `Report`로부터 `인수인계서.hwpx`, `실적보고서.hwpx`를 직접 조립하며 한/글 COM이나 사용자 문서를 쓰지 않는다.
- HWPX ZIP에 `mimetype`(첫 항목·무압축), `version.xml`, `META-INF/container.xml`, `META-INF/manifest.xml`, `Contents/header.xml`, `Contents/section0.xml`, `Contents/content.hpf`를 넣는다. 제목/본문 글자 모양과 문단 모양, 표 테두리·머리 행 음영은 `header.xml`에 정의하고 본문은 ID를 참조한다.
- 제목은 한컴돋움 16pt, 본문은 휴먼명조 11pt, 표 머리 음영은 `#E7EEF8`, 표 폭은 170mm(48,189 HWP 단위)다. 월별 표와 상위 주제 표는 한/글 네이티브 표로 생성하며 열 너비 비율을 표 목적에 따라 배분한다. 이미지 개체는 포함하지 않는다.
- 인수인계서에는 주제, 탐지된 정기 업무, 버전 줄 문서 이력 및 최근 문서 최대 3건을 넣는다. 입력 모델에 담당자 이름·인사 변동이 없어 해당 칸에는 정보가 없다고 표시한다.
- 실적보고서에는 1~12월 문서 수 표, 상위 주제 표, 분석 기간·건수 및 상위 주제 요약 문장을 넣는다. 입력이 없을 때도 두 문서는 머리글 표를 포함한 유효한 구조로 만든다.
- `write_outputs`는 두 HWPX를 출력 폴더 최상위에 만들고, 기존 Markdown·CSV·SVG·JSON은 파일명을 유지해 `원자료` 아래에 저장한다. XLSX는 이 카드에서 생성하지 않았다(계획서의 ANNUAL_A5_XLSX_BUILDER 범위).
- `scripts/validate_annual_hwpx.py`를 추가해 기존 `docfit_core.hwpx` Python 검증·검사기를 호출한다. 필수 ZIP/XML, 표 수·행/열 수, 월별 13행, 170mm 너비 합계, 이미지 부재를 확인한다.

## 의존성 및 라이선스

새 외부 크레이트를 추가하지 않고 저장소에서 이미 사용하는 `zip` 크레이트를 직접 참조했다. 현재 잠금된 버전은 `zip 9.8.1`, 크레이트 메타데이터의 라이선스는 MIT다. `kfind-annual` 자체는 기존 PolyForm Strict License 1.0.0을 유지한다.

## 시험 결과

실행 명령: `cargo test --manifest-path kfind/annual/Cargo.toml --release --locked -j 2`
환경: Cargo PATH 맨 앞에 `%LOCALAPPDATA%/DocFitTools/mingw64/bin`, `CARGO_TARGET_DIR` 환경 변수 지정.

```text
test result: ok. 53 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Rust 테스트가 합성 보고서와 빈 입력으로 총 4개 HWPX를 만들고 Python 검증기를 호출했다. 네 파일 모두 ZIP/XML 구조 검증을 통과했고, 인수인계서는 표 1개(3열), 실적보고서는 표 2개(각각 2열·3열)로 확인됐다. 월별 표는 머리글 포함 13행이며 각 표의 열 너비 합계는 48,189 HWP 단위다. 텍스트 추출 결과에 경고나 이미지가 없었다.

작업 중 출력 위치 변경에 따라 기존 테스트 3개가 한 차례 실패했으나, `원자료` 경로를 기대하도록 수정한 뒤 위 전체 시험에서 모두 통과했다.

## 미확인 및 후속

- 한/글 2020에서 실제로 열고 조판·인쇄되는지는 확인하지 않았다. 이번 작업에서 COM 시험은 실행하지 않았다. 실기 확인은 후속 `ANNUAL_A4_WIN_VERIFY`에서 해야 한다.
- 담당자 변천 데이터는 `AnnualInput`/`Report` 입력에 없다. 현재 문서에는 이 한계를 표시하고 문서 버전 줄을 이력으로 제공한다. 담당자 이력 필드가 생기면 연결해야 한다.
- XLSX, 주간 열지도 및 타임라인 HWPX 표는 이번 카드 범위에서 구현하지 않았다.
- 전체 크레이트 `cargo fmt --check`는 실행 시 기존 `output.rs`·`stats.rs` 등의 포맷 차이도 표시해 통과하지 않았다. 새 `hwpx.rs`는 rustfmt를 적용했다.
