# DOCVIEWWIN1 결과 (2026-11-22)

카드: 문서뷰 Rust+Win32 창 — 설계 + 1단계(읽기 전용 격자 표시). 설계 문서: `docs/DOCVIEW_WIN32_DESIGN_2026-11-22.md`.
브랜치 `feature/DOCVIEWWIN1`, 커밋만(푸시 없음). Python·Flutter·`hwp-auto-docfit.py`·설정 기본값·CI 워크플로 변경 없음.

## 한 일

- 새 크레이트 `docview_win/`(의존성 `windows-sys`·`ui_theme` 둘뿐, 독립 `[workspace]`):
  - `json.rs` 읽기 전용 JSON 파서 · `scan.rs` `klib scan --json-lines` 줄 파서 + 비동기 `ScanJob`(자식 프로세스·진행·취소) · `settings.rs` 설정 읽기(`ui_thumb_sizes.doc_grid`·`library_excluded_folders`·`docview_ui`) · `model.rs` 문서·검색·정렬·표시 글자·상태 · `layout.rs` 격자·가상화·키보드 이동·스크롤바·도구줄 배치 · `synth.rs` 합성 모델(캡처·시험용) · `paint.rs` GDI 그리기 · `capture.rs`/`png.rs` 캡처 · `window.rs` 실제 창.
  - 화면: 도구줄(검색·정렬·개수·썸네일 크기 슬라이더·제외 폴더·새로고침/스캔 취소) / 스캔 띠 / 가상화 격자(카드: 자리표시 썸네일·확장자 배지·이름·크기·수정일·클라우드) / 상태줄(엔진·항목 수·경과·선택 경로).
  - 데이터 경로: Rust 가 `klib scan` 실행 파일을 직접 호출(Python 경유 없음). klib 를 라이브러리로 묶지 않은 이유는 설계 §3.
- 캡처 모드: `docview_win --capture-dir <폴더> [--dark] [--dpi N] [--size WxH] [--scene 이름]`, `--capture-set` 은 14장 일괄. 실제 `paint::draw` 를 메모리 DC 에 그려 PNG(`DOCVIEW_WIN_OUT` 파일에 결과 줄).

## 시험 결과 (`cargo test --locked --manifest-path docview_win/Cargo.toml`, 3회 연속 같은 결과)

```
Running unittests src\lib.rs     test result: ok. 0 passed; 0 failed
Running unittests src\main.rs    test result: ok. 0 passed; 0 failed
Running tests\capture_tests.rs   test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.21s
Running tests\pure_tests.rs      test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.32s
Doc-tests docview_win            test result: ok. 0 passed; 0 failed
```

- 합계 **37개 통과 / 0 실패**. 선례 `ui_theme` 시험도 `13 passed; 0 failed`(변경 없음 확인용).
- `pure_tests`(32): 격자 열 수 규칙(Flutter `maxCrossAxisExtent` 와 같은 식)·썸네일 크기와 열 수·dpi 배율·**5만 항목 가상화**(보이는 칸만 겹침, 범위 밖 칸은 안 겹침, 끝 항목 포함)·빈/짧은 목록·히트 테스트·스크롤 한계/보이기·스크롤바 손잡이 왕복·키보드 이동(방향·Home·End·PageUp/Down)·선택 스크롤·슬라이더 크기 변경 시 선택 칸 유지·슬라이더 100~240 매핑과 도구줄 칸 겹침 없음(폭 820~1920)·검색/정렬·스캔 중 선택 유지·표시 글자(크기·날짜·자리 쉼표)·**klib 줄 파서**(정상·이스케이프·서로게이트·거절 10종)·합성 줄 왕복·klib 인자(Python 어댑터와 동일)·**스캔 스트리밍/실패/읽기 오류/무한 스캔 취소(1초 내 종료·이후 더 읽지 않음)**·**가짜 klib 배치 파일로 자식 프로세스 경로(정상·종료 코드 3 실패·실행 파일 없음·취소 시 자식 종료)**·스캔 상태 기계·**설정 읽기**(범위 맞춤·형식 오류·BOM·CRLF·읽기 전용·제외 폴더 정규화·`docview_ui`)·JSON 파서.
- `capture_tests`(5): 8개 장면 × (라이트 96 / 다크 144dpi) 가 **빈 화면이 아님**(다른 화소 비율 ≥2%)·한 화면에 그린 칸 ≤40(가상화)·라이트≠다크·선택/스캔 띠가 그림을 바꿈·썸네일을 줄이면 더 많은 칸·실행 파일(`CARGO_BIN_EXE_docview_win`)이 PNG 를 쓰고 헤더·크기(1350×900)·바이트 수 확인·`--capture-set` 14장.
- 실제 창 연기 시험: 합성 스캔 2만 건으로 창을 띄워 4초 뒤 응답 상태(Responding=True), `CloseMainWindow` 로 정상 종료. 이 PC 의 앱·서비스는 건드리지 않음.
- 시험 중 잡은 결함 2건: ① 도구줄이 최소 폭 760 에서 겹침 → 최소 폭 820 으로(시험이 보증). ② **스캔 줄이 드물게 오면(80ms 넘게 다음 줄이 없으면) 진행 갱신이 멈추는 결함** → 읽기 스레드와 배치 스레드를 분리해 80ms 마다 비우도록 수정(실제 klib 가 한 디스크 구간에서 문서를 드물게 찾을 때 훑은 수가 멈춰 보이는 문제였음).

## 캡처 확인 (PNG 를 Read 로 직접 열어 확인한 것)

`docview_win --capture-dir <폴더> --capture-set` 결과(1100×720, 150% 는 1650×1080):

| 파일 | 직접 확인한 내용 |
|---|---|
| `docview_list_light_96dpi.png` | "2022년 감사 대응 자료 …" 카드 6열, PDF/HWP/PPTX/XLS 배지 색, 크기·날짜("13.4 MB · 2022-11-09"), 선택 칸(10번째) 강조, 도구줄(검색 자리표시 "문서 이름 검색"·"정렬: 이름 ▲"·"400개 문서"·슬라이더·"제외 폴더 (2)"·"새로고침"), 상태줄 "엔진: 합성 모델 · 문서 400개 · 완료 0:07" |
| `docview_list_dark_96dpi.png`·`docview_scanning_dark_96dpi.png` | 다크 팔레트 적용(스캔 띠: "문서 검색 중 · 훑은 항목 1,240개 · 0:12 · 현재 폴더: D:\업무\예산\2026년 본예산", 버튼 "스캔 취소") |
| `docview_list_light_144dpi.png` | 150% 배율 |
| `docview_search_light_96dpi.png` | 검색어 "예산", 정렬 "수정일 ▼", "31 / 400개 문서", 결과 카드 "2022년 예산 편성 검토 00141.hwp …", 상태줄 "(검색 31개)" |
| `docview_virtual_light_96dpi.png`·`…_dark_96dpi.png`·`…_144dpi.png` | **5만 항목 가상화**: 스크롤 위치 깊숙이(오른쪽 스크롤바 손잡이가 아래쪽), "2025년 공사 계약 변경 28125.pptx" 선택, "50,000개 문서", 상태줄 "엔진: 합성 모델 · 문서 50,000개 · 완료 1:34", 실제 그린 칸 24개 |
| `docview_scanning_empty_light_96dpi.png` | 항목이 없을 때 가운데 "문서 검색 중 · 훑은 항목 215개 · 0:02 / 현재 폴더: C:\Users\공무원\Documents" |
| `docview_empty_light_96dpi.png` | "지원하는 문서가 없습니다." / "폴더에 HWP, HWPX, XLS, XLSX, PPT, PPTX, PDF 파일이 없습니다." |
| `docview_failed_light_96dpi.png` | 빨간 "문서 목록을 불러오지 못했습니다." + klib 실행 파일 없음 안내 |
| `docview_idle_light_96dpi.png` | 처음 안내 "모든 고정 드라이브의 문서를 확인합니다." |

PNG 는 용량(장당 2~5MB, 무압축) 때문에 저장소에 넣지 않았다. 위 명령으로 다시 만든다.

## 솔직한 한계 (거짓 완료 금지)

- **썸네일은 구현되지 않았다.** 모든 카드는 자리표시(문서 모양 + 확장자 색 바탕)다. 캐시 키 규칙은 설계 문서에만 있고 코드로 구현하지 않았다(THUMBRS 후속).
- **캡처의 문서 목록은 합성 모델**(`synth.rs`)이다. 상태줄 엔진 칸에도 "합성 모델"로 표시된다. 실제 `klib scan` 결과로 격자를 그린 캡처는 만들지 못했다: 이 PC 에는 `klib.exe` 가 없고, 이 PC 의 MinGW 도구함에 `dlltool` 이 없어 klib 를 새로 빌드하지 못했다(`getrandom` 단계에서 "dlltool.exe program not found"). 따라서 `real_klib_scan_if_available` 시험은 `klib.exe` 가 없으면 **건너뛴다**(이 PC 에서는 건너뜀). 실제 klib 와의 연결은 klib 줄 형식(소스 `scan_cli.rs::emit` 확인)과 같은 줄을 내는 **가짜 klib 배치 파일**로 자식 프로세스 경로(출력 파싱·종료 코드·취소)를 시험했다. CI 에서 klib 를 만든 환경이나 `DOCFIT_KLIB` 지정 시 같은 시험이 실제 klib 로 돈다.
- 실제 창의 마우스·키보드 조작은 사람이 눌러 보지 않았다(창 기동·응답·종료만 확인). 조작 논리(클릭 히트, 키 이동, 슬라이더, 스크롤바)는 순수 함수 시험으로 검증.
- 검색 칸은 한글 조합 중 표시(IME)가 없다(완성된 글자만).
- 슬라이더·제외 폴더는 **읽기 전용**(저장 없음). 더블클릭 열기·15초 무응답 안내·저장본 즉시 표시·service 엔진은 후속 단계.
- Flutter 문서뷰에 없는 기능(검색·정렬·수정일)을 카드 요구에 따라 넣었다. 유지 여부는 사용자 확인 필요.
- 앱 연결(`docview_ui` 게이트·번들·CI 워크플로 등록)은 범위 밖이라 `.github/workflows/build-windows-exe.yml` 에 아직 없다.

## 다음 카드를 위한 메모

- CI 등록 시 `cargo test --release --locked --manifest-path docview_win/Cargo.toml`, 번들 복사 `docview_win/target/release/docview_win.exe → resources/docview_win.exe`, 파일 목록(`$files`) 추가.
- `docview_win` 은 `klib.exe` 를 같은 폴더·`resources` 에서 찾는다(`scan::find_klib`). 번들 위치가 다르면 `DOCFIT_KLIB` 또는 `--klib` 로 지정.
- 시험이 만드는 임시 폴더는 `std::env::temp_dir()` 아래 `docview_win_*_<pid>` 이며 끝나면 지운다. 저장소 추적 파일은 건드리지 않는다(`tests/fixtures` 변경 없음).
