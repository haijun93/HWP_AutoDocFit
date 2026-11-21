# KAGENTAUDIT 결과 — 몽땅클립보드 화면 캡처 점검 (2026-11-21)

## 범위와 방법

- `kagent/src/popup.rs`, `frame.rs`, `settings_window.rs`, `main.rs`, KTm 선례를 읽고 캡처 가능 여부를 확인했다.
- 설정 화면은 기존 `--capture-settings` 메모리 DIB 경로를 사용했다. 임시 `KAGENT_TEST_ROOT`에서 기본 설정만 읽게 한 뒤 라이트·다크 × 96·144 DPI 4장을 별도 캡처해 Read로 열었다.
- 팝업은 `--capture-popup <폴더> [--dark] [--dpi N] [--size WxH] [--scenario 이름|all]`로 캡처했다. 실제 창은 숨김으로 생성해 DPI/font metric만 얻고, 화면은 실제 `paint_surface`를 메모리 DIB에 그렸다. 창을 보여 주거나 앱 초기화·서비스·클립보드 기록·사용자 저장소를 열지 않았다.
- 합성 항목 15상태 × 라이트/다크 × 96/144 DPI 및 라이트 420×122 최소 크기 = **75장**. 144 DPI는 150% 배율에 가까운 값이다. 산출물은 저장소 밖 임시 폴더에 두었다.
- 캡처를 Read로 직접 열어 15개 상태, 두 테마, 96/144 DPI, 최소 크기와 설정 화면을 점검했다.
- 실행 형식: `kagent --capture-popup <임시 폴더> --scenario all --dpi 96|144 --size 560x420`, 다크는 `--dark` 추가, 최소 크기는 `--size 420x122`(`MIN_W`·`MIN_H`). 설정 화면은 임시 `KAGENT_TEST_ROOT`와 `KAGENT_CAPTURE_DPI=96|144`를 지정하고 `kagent --capture-settings <임시 폴더>`로 캡처했다. 모든 결과는 작업 폴더 밖에 생성했다.

## 점검 표

| # | 항목(상태) | 조합 | 문제 | 심각도 | 처리 |
|---|---|---|---|---|---|
| 1 | 캡처 렌더러 — 다크 테마 | 다크·96/144 DPI | 캡처가 현재 전역 팔레트를 읽어 다크 지정값을 반영하지 않아, 다크 화면이 라이트처럼 저장됨. 앱 화면이 아니라 캡처 경로의 문제 | 중간 | 수정 |
| — | 빈 목록·합성 글/그림/파일·긴 텍스트·한글/영문·선택 행·메모·검색 입력·검색 결과 없음·오류 안내 | 라이트·다크·96/144 DPI | 글자 겹침/잘림, 색 대비 저하, 정렬 이상 없음. 긴 항목은 의도한 말줄임 표시 | — | 유지 |
| — | 문서·그림·파일·폴더·웹·화면캡처·키워드검색 탭 | 라이트·다크·96/144 DPI | 탭 선택 표시와 행의 글자/항목 유형 정렬 정상 | — | 유지 |
| — | 팝업 최소 크기 420×122 | 라이트·96 DPI | `MIN_W`·`MIN_H`와 일치. 두 행까지 잘림 없이 표시 | — | 유지 |
| — | 몽땅클립보드 설정 화면 | 라이트·다크·96/144 DPI | 설정 제목·항목·목록·하단 버튼의 겹침/잘림 없음 | — | 유지 |
| — | 삭제 확인 팝업 | 해당 없음 | 현 팝업/설정 화면에는 별도 삭제 확인 화면이 없다. 즉시 삭제 동작을 캡처 상태로 꾸미지 않음 | — | 대상 아님 |

발견 1건(중간 1), 수정 1건. 정상으로 확인한 화면은 바꾸지 않았다.

## 수정과 전·후 관찰

1. `popup.rs`: WM_PAINT의 버퍼 처리와 공통 렌더링을 분리했다. 창 그리기와 캡처가 상태·오류 문구·팔레트를 인자로 받는 `paint_surface`를 함께 사용한다. 캡처용 합성 데이터로 실제 저장 기록을 읽지 않고 상태를 만든다.
2. `frame.rs`: 캡처 프로세스에서만 DPI 배율을 고정하는 재설정 함수를 추가했다. 기본 앱 경로에서는 기존 창 DPI를 계속 사용한다.
3. `main.rs`: 인스턴스 mutex·DB·트레이 경로보다 앞에서 `--capture-popup`을 처리한다. 캡처 인자에 출력 폴더·테마·DPI·크기·상태 선택을 받는다.
4. 레이아웃 단위 시험 `capture_size_scales_logical_minimum_and_150_percent_dimensions`를 추가했다.

전: 메모리 캡처에서 다크 지정이 무시되어 다크 이미지의 배경이 흰색이었다 (`popup_mixed_dark_144dpi_560x420.png`).
후: 팔레트를 렌더 함수 인자로 전달한 뒤 같은 상태/배율의 배경이 어두운 색으로 저장되고, 선택 행·탭 대비도 정상이다 (`popup_mixed_dark_144dpi_560x420.png`).
함수 분리 전후 라이트 화면의 배치·문구는 변하지 않았다 (`popup_mixed_light_96dpi_560x420.png`, `popup_search_empty_light_96dpi_560x420.png`, `popup_selected_light_144dpi_560x420.png`). 파일은 저장소 외부 임시 캡처 폴더에만 있다.

## 시험

PATH 맨 앞에 `%LOCALAPPDATA%/DocFitTools/mingw64/bin`, `CARGO_TARGET_DIR`은 저장소 밖 임시 경로로 지정했다. 전체 시험은 실행하지 않았다.

```text
cargo test --manifest-path kagent/Cargo.toml
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 160 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 16.28s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

cargo test --manifest-path ui_theme/Cargo.toml
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

기존 kagent 159개 통과 시험은 유지됐고 캡처 치수 단위 시험 1개가 추가됐다.

## 미확인 및 사용자 실기 항목

- 삭제 확인 화면은 실제로 존재하지 않아 확인하지 않았다.
- 숨김 창 + 메모리 DIB는 실제 표시 창의 DWM 테두리·그림자, 다중 모니터 가장자리, 실제 IME 조합 중 동작을 검증하지 않는다.
- 실제 PC의 100/125/150% 및 혼합 DPI 이동, 키보드/마우스 상호작용은 사용자 실기 확인 대상이다.
- 설정 화면은 별도 캡처 경로이며 `--size` 상태 조합은 제공하지 않는다. 기존에 검증된 고정 크기 설정 화면을 그대로 사용했다.
