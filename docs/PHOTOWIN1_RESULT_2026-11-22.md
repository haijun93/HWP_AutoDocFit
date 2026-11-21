# PHOTOWIN1 결과 (2026-11-22)

카드: 사진뷰 Rust+Win32 창 — 설계 + 1단계(읽기 전용 격자 표시). 설계 문서: `docs/PHOTOVIEW_WIN32_DESIGN_2026-11-22.md`.
브랜치 `feature/PHOTOWIN1`, 커밋만(푸시 없음). Python·Flutter·`hwp-auto-docfit.py`·설정 기본값·CI 워크플로 변경 없음. 앱 연결(`photoview_ui` 게이트)은 다음 카드.

## 한 일

1. **공용 크레이트 `grid_core/` 신설**(PM 지침: 문서뷰 코드 최대 재사용) — 문서뷰에서 JSON 파서·PNG 인코더·격자 배치/가상화/스크롤바/키 이동/도구줄(`GridSpec::DOC`·`PHOTO`)·목록 모델(`Doc`·`Collection`·klib 줄 파서)·표시 글자·스캔 상태·화면 상태 `App`·스캔 작업(자식 프로세스 읽기·배치·취소·종료 코드)·GDI 도우미·메모리 DC 캡처를 옮겼다. `docview_win` 은 같은 경로로 재노출해 **기존 시험 37개를 하나도 고치지 않고 통과**한다.
2. **새 크레이트 `photoview_win/`**(의존성 `grid_core`·`ui_theme`·`windows-sys` 셋뿐): 사진 격자(간격 10·가로세로비 0.8·여백 16, Flutter 사진뷰 규칙), 확장자 색 배지·해/산 자리표시 썸네일·파일 이름·크기·수정일 카드, 가상화·스크롤·선택·키보드 이동, 썸네일 크기 슬라이더(100~240, 기본 160, `ui_thumb_sizes.photo_album` **읽기만**), 검색(이름·확장자 낱말)·정렬(기본 수정일 최신순), 스캔 진행 띠·가운데 안내·취소(Esc·단추, 자식 klib 종료), 새로고침 F5, 상태줄(엔진·사진 수·경과), 캡처 모드.
3. **데이터 경로**: Rust 가 `klib scan --ext jpg,jpeg,png,webp,bmp,tif,tiff,heic --json-lines` 를 직접 호출(Python 경유 없음). 줄 형식은 `klib/src/scan_cli.rs::emit` 확인(문서와 동일).
4. **설계 문서**: 기능 전수표 25행, 창 구조, 데이터 경로, 썸네일(캐시 키 = 기존 `ThumbnailCache.key` 재사용, 공급 방식 4종 비교, **Windows 셸 썸네일 API 우선 시험 권고**), 설정, 위험, 단계 계획, 공용화·중복 목록.

## 시험 결과 (모두 `cargo test`, 디버그 빌드, 이 PC)

| 크레이트 | 실행 파일 | 결과 |
|---|---|---|
| `photoview_win` | tests\capture_tests.rs | `test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.78s` |
| `photoview_win` | tests\pure_tests.rs | `test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.82s` |
| `photoview_win` | src\lib.rs / src\main.rs / Doc-tests | 각 `ok. 0 passed; 0 failed` |
| `grid_core` | tests\core_tests.rs | `test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `grid_core` | src\lib.rs / Doc-tests | 각 `ok. 0 passed; 0 failed` |
| `docview_win`(이동 뒤 회귀 확인) | tests\capture_tests.rs | `ok. 5 passed; 0 failed; finished in 4.19s` |
| `docview_win` | tests\pure_tests.rs | `ok. 32 passed; 0 failed; finished in 0.32s` |
| `docview_win` | src\lib.rs / src\main.rs / Doc-tests | 각 `ok. 0 passed; 0 failed` |
| `ui_theme`(선례 확인) | src\lib.rs | `ok. 13 passed; 0 failed` |

- 합계: **새로 만든 시험 34개(photoview 29 + grid_core 5) 통과, 문서뷰 37개 통과, 실패 0**. 전체 저장소 시험은 카드 지시대로 돌리지 않았다.
- `pure_tests`(photoview 24): 사진 격자 규칙(7열·간격 10·비율 0.8, 문서뷰 규칙과 다름)·썸네일 크기별 열 수·150% 배율·**5만 장 가상화**(보이는 칸만 겹침, 앞뒤 줄은 범위 밖, 끝 사진 포함)·히트 테스트·스크롤 보이기·스크롤바 왕복·키보드 이동(방향·Home·End·PageUp/Down·마지막 줄)·슬라이더 범위·도구줄 칸 겹침 없음(폭 820/1100/1920)·크기 변경 때 선택 칸 유지·NaN 은 사진 기본 160·합성 목록 유일/결정적/최신 먼저·검색/정렬·표시 글자·스캔 띠 규칙·스캔 중 선택 유지·**klib 사진 줄 파서(정상·이스케이프·이모지 서로게이트·거절 10종)·klib 인자가 사진 확장자**·스캔 스트리밍/오류 줄/드문 줄 진행/**무한 스캔 취소 1초 내·취소 뒤 더 읽지 않음**·**가짜 klib 배치 파일 자식 프로세스(정상·종료 코드 3 실패·실행 파일 없음·취소 시 자식 종료)**·**설정 읽기**(`photo_album` 키·범위 맞춤·문서뷰 값 무시·형식 오류·BOM·CRLF·읽기 전용·제외 폴더 정규화·`photoview_ui`).
- `capture_tests`(5): 8개 장면 × (라이트 96 / 다크 144dpi) 가 **빈 화면이 아님**(다른 화소 ≥2%)·한 화면에 그린 칸 ≤40(가상화)·라이트≠다크·선택/스캔 띠가 그림을 바꿈·실행 파일(`CARGO_BIN_EXE_photoview_win`)이 PNG 를 쓰고 헤더·크기(1350×900)·바이트 수 확인·`--capture-set` 14장.
- 시험 중 잡은 것: 슬라이더 픽셀 왕복에서 160 → 161(정수 픽셀 반올림). 시험의 가정 오류였고 ±2 허용으로 고침. 캡처를 보고 잡은 것: ① 좁은 칸에서 수정일이 "2026-06-…" 로 잘림 → 칸이 좁으면 `26-06-07` 로 줄임 ② 실패 안내 제목이 보라색(HEIC 색을 잘못 씀) → 빨강으로 수정.
- 실제 창 연기 시험: `photoview_win --synthetic 20000` 을 띄워 4초 뒤 `Responding=True`, `CloseMainWindow` 로 정상 종료(`HasExited=True`). 제가 띄운 프로세스만 다뤘고 이 PC 의 실행 중 앱·서비스는 건드리지 않았다.

## 캡처 확인 (PNG 를 Read 로 직접 열어 확인한 것)

`photoview_win --capture-dir <폴더> --capture-set` 결과(1100×720, 150% 는 1650×1080). 모두 합성 모델 데이터이며 상태줄 엔진 칸에도 "합성 모델"로 표시된다.

| 파일 | 직접 확인한 내용 |
|---|---|
| `photoview_list_light_96dpi.png` | 7열 격자, 첫 카드 "DSC00195.jpg"(2.2 MB), "DSC00145.heic"(HEIC 보라 배지), "IMG_20260628_0…"(WEBP 주황), "행사 스케치 0017…"(TIF 분홍), "DSC00259.jpeg"; 선택 칸 "IMG_20260607_0…"(695.3 KB, 파랑 테두리); 도구줄 "파일 이름 검색"·"정렬: 수정일 ▼"·"사진 400장"·슬라이더·"제외 폴더 (2)"·"새로고침"; 상태줄 "엔진: 합성 모델 · 사진 400장 · 완료 0:07"·선택 경로 `D:\사진\공사 준공\IMG_20260607_0144.tif` |
| `photoview_list_light_144dpi.png` | 150% 배율, 7열, 같은 구성, 날짜 `26-07-06` 형식 |
| `photoview_list_dark_96dpi.png`·`photoview_scanning_dark_96dpi.png` | 다크 팔레트. 스캔 띠 "사진 검색 중 · 훑은 사진 1,240장 · 0:12 · 현재 폴더: D:\사진\2026 현장 점검", 버튼 "스캔 취소", 선택 "DSC00543.jpg", 상태줄 "사진 1,240장 · 검색 중 0:12" |
| `photoview_search_light_96dpi.png` | 검색어 "점검", "사진 22 / 400장", 결과 "안전 점검 00167.h…"·"안전 점검 00050.tif"·"안전 점검 00230.png(선택)"·"현장 점검 00033.…", 상태줄 "(검색 22장)" |
| `photoview_virtual_light_96dpi.png`·`…_dark_96dpi.png`·`…_144dpi.png` | **5만 장 가상화**: 스크롤바 손잡이가 아래쪽, "준공 검사 00375.jpeg" 선택(31,337번째), "사진 50,000장", 상태줄 "엔진: 합성 모델 · 사진 50,000장 · 완료 1:34", **실제 그린 칸 35개**(`photoview_virtual_*` 로그) |
| `photoview_scanning_empty_light_96dpi.png` | 항목 없을 때 가운데 "사진 검색 중 · 훑은 사진 215장 · 0:02 / 현재 폴더: C:\Users\공무원\Pictures", "스캔 취소" |
| `photoview_empty_light_96dpi.png` | "사진이 없습니다." / "폴더에 JPG, PNG, HEIC 등 사진 파일이 없습니다." |
| `photoview_failed_light_96dpi.png` | 빨간 "사진 목록을 불러오지 못했습니다." + "klib 실행 파일을 찾지 못했습니다(DOCFIT_KLIB 또는 --klib 로 지정)." |
| `photoview_idle_light_96dpi.png` | "모든 고정 드라이브의 사진을 확인합니다." / "지원 형식: JPG, JPEG, PNG, WEBP, BMP, TIF, TIFF, HEIC" |

PNG 는 용량(장당 2~5MB, 무압축) 때문에 저장소에 넣지 않았다. `photoview_win --capture-dir <폴더> --capture-set` 으로 다시 만든다(옵션 `--dark --dpi N --size WxH --scene 이름`).

## 솔직한 한계 (거짓 완료 금지)

- **썸네일은 구현되지 않았다.** 모든 카드는 해·산 모양 자리표시다. 셸 썸네일 API(`IShellItemImageFactory`)를 비롯한 공급 방식 비교는 설계 문서 §4 에만 있고 **측정하지 않았다**(Pillow 대비 속도·화질 수치 없음). 권고만 한다.
- **캡처의 사진 목록은 합성 모델**이다(실제 사진 파일을 읽지 않음). 이 PC 에는 `klib.exe` 가 없어(DOCVIEWWIN1 과 같은 사정) 실제 `klib scan` 결과로 격자를 그린 캡처는 없다. `real_klib_scan_if_available` 시험은 `klib.exe` 가 없으면 건너뛰며 이 PC 에서는 건너뛰었다. 대신 klib 줄 형식과 같은 줄을 내는 가짜 klib 배치 파일로 자식 프로세스 경로를 시험했다.
- 실제 창의 마우스·키보드 조작은 사람이 눌러 보지 않았다(기동·응답·종료만 확인). 조작 논리는 순수 함수 시험으로 검증.
- **이번 단계에 없는 사진뷰 기능**: 연·월 묶음 격자, 즐겨찾기, 태그(검색 포함), 폴더별·카메라별·위치별 보기, 중복 검사 결과 보기, 사본 편집, 사진대지 HWPX, 슬라이드쇼·뷰어, 다중 선택·일괄 내보내기. 모두 설계 문서 단계 계획(2~4단계)에 넣었다.
- 검색 칸 한글 조합 중 표시(IME) 없음, 슬라이더·제외 폴더 읽기 전용, 더블클릭 열기·저장본 즉시 표시·service 엔진 없음(문서뷰와 같음).
- 공용화는 **일부만** 끝났다: `window.rs`(약 540줄)·`paint.rs` 의 도구줄/상태줄/빈 화면·`main.rs`·`capture.rs`·`settings.rs`·`synth.rs`·시험 일부는 복사로 남겼다. 목록과 이유·후속은 설계 문서 §8.
- Flutter 사진뷰에 없던 표시(검색·정렬·수정일·확장자 배지)를 문서뷰와 맞춰 넣었다. 사용자 확인 후 유지 여부를 정해야 한다.
- `docview_win/Cargo.lock` 이 `grid_core` 추가로 바뀌었다. CI 등록 시 `--locked` 가 통과하는지 다음 카드에서 확인해야 한다.

## 다음 카드를 위한 메모

- CI 등록: `grid_core`·`photoview_win`·`docview_win` 의 `cargo test --release --locked --manifest-path …`, 번들 복사 `photoview_win/target/release/photoview_win.exe → resources/photoview_win.exe`, 파일 목록 추가. `grid_core` 변경은 문서뷰·사진뷰 시험 모두 돌려야 한다.
- 2단계 우선순위 제안: 제외 폴더·슬라이더 저장 → IME → 즐겨찾기·태그(DB 소유자 결정) → (3단계) 셸 썸네일 API 측정 → 연월 묶음.
- 시험이 만드는 임시 폴더는 `std::env::temp_dir()` 아래 `photoview_win_*_<pid>` 이며 끝나면 지운다.
