# DOCK6 결과 — 2026-11-21

## 변경

- `dock/src/native.rs`, `dock/src/lib.rs`, `dock/Cargo.toml`, `dock/Cargo.lock`: 실제 Dock GDI paint와 공유하는 DIB 오프스크린 렌더링을 추가했다. `dock --capture-dir <폴더> [--dark] [--dpi N] [--hc]`는 창·뮤텍스·파이프 없이 PNG 한 장을 생성한다. 캡처는 16개 메뉴 전체를 표시하고 라벨·그룹 구분선·실행 점을 그린다. `ui_theme`의 시스템 테마 감지와 `%APPDATA%/HwpAutoDocFit/settings.json`의 `theme_mode`(system/light/dark)를 사용하며, 고대비는 시스템 색상 팔레트를 사용한다.
- `dock/src/lib.rs`, `dock/src/native.rs`: Dock 메뉴 위치와 아이콘 DPI 계산을 유지하고 클릭 상태 전이를 Open/Close로 명시했다. `docfit-close-tab-N` 이벤트를 `docfit-open-tab-N`의 짝 요청으로 추가했다.
- `flutter_ui/lib/features/workspace/tab_open_events.dart`, `flutter_ui/lib/main.dart`: 활성 탭과 번호가 일치하는 닫기 이벤트만 탭 선택을 비운다. 다른 탭의 닫기 요청은 무시한다.
- `flutter_ui/test/tab_open_events_test.dart`: 닫기 이벤트 상태 시험을 추가했다.
- `docs/DOCK_DESIGN_2026-11-18.md`: 재클릭과 작업표시줄 위치 규칙을 기록했다.

`hwp-auto-docfit.py`와 `.cargo/config.toml`은 변경하지 않았다. 캡처 PNG는 저장소 밖 임시 캡처 폴더에 보관했다.

닫기 신호는 기존 Rust 코드의 `OpenEventW`/`SetEvent` 패턴을 따라 이름 있는 `docfit-close-tab-N` 이벤트로 구현했다. Flutter 시험은 DOM 이벤트 리스너의 상태 전이를 검증한다. 저장소에는 Windows 이벤트를 DOM 이벤트로 전달하는 브리지가 없고 설계 §5는 파이프 JSON을 제안으로만 기술하므로, Dock 클릭에서 실제 Flutter 창까지의 전달은 미완료/미확인이다. 이 결과만으로 실사용 토글 완료로 판정하지 않는다.

## 캡처와 직접 관찰

Read 도구로 아래 PNG를 직접 열어 확인했다.

- `dock_light_96.png`: 라이트 테마. 16개 아이콘, 이름, 네 그룹의 세 경계선, 문서편집 아이콘의 파란 포커스 링과 아래 실행 점이 보인다.
- `dock_dark_96.png`: 다크 테마에서 같은 요소가 보이며 배경·글자·구분선 대비가 유지된다.
- `dock_light_144.png`: 150% 배율(144 DPI)에서 같은 요소가 보인다. 아이콘과 글자 기준선이 정렬되고 이름이 잘리지 않는다.
- `dock_hc_96.png`: 시스템 팔레트 경로로 생성했다. 이 PC는 실제 Windows 고대비 모드가 아니므로 이 캡처의 색은 고대비 실기 결과를 대신하지 않는다.

화면에 보인 메뉴 이름은 `문서편집`, `표템플릿`, `붙임사진 편집`, `표그림판`, `다이어그램`, `마인드맵`, `몽땅검색`, `문서뷰`, `사진뷰`, `자료함`, `업무자료함/노트북`, `파일명 일괄변경`, `파일변환`, `PDF 편집`, `아웃라이너`, `검색 벤치마크`다. 아이콘 16개가 모두 한 번씩 보였고 구분선은 문서 작업/그리기/찾기·자료/변환·도구의 4묶음을 나눴다. 관찰은 포인터 확대가 없는 정지 캡처 기준이다.

캡처 명령은 실제 실행 파일의 인자와 동일하게 `--capture-dir`, `--dark`, `--dpi`, `--hc`를 처리했다. 라이트·다크·144 DPI 캡처를 각각 생성했다.

## 배치 규칙과 순수 함수 시험 표

Dock은 전경 창이 있는 모니터의 작업 영역을 기준으로 한다. 아래 작업표시줄이면 아래쪽 중앙, 위쪽이면 작업 영역 위 안쪽, 좌/우면 해당 가장자리 안쪽에 두고 세로 중앙 정렬한다. 자동 숨김은 `rcWork == rcMonitor`일 때 아래 배치를 기본값으로 쓴다.

| 조건 | 입력(모니터/작업 영역, 96 DPI) | 결과 사각형 (left, top, right, bottom) |
|---|---|---|
| 아래 작업표시줄 | (0,0,1920,1080) / (0,0,1920,1040) | (760,966,1160,1022) |
| 위 작업표시줄 | (0,0,1920,1080) / (0,40,1920,1080) | (760,58,1160,114) |
| 왼쪽 작업표시줄 | (0,0,1920,1080) / (48,0,1920,1080) | (66,340,122,740) |
| 오른쪽 작업표시줄 | (0,0,1920,1080) / (0,0,1872,1080) | (1798,340,1854,740) |
| 자동 숨김 / 아래 기본 | (0,0,1920,1080) / 동일 | (760,1006,1160,1062) |
| 보조 모니터, 음수 좌표 | (-1920,0,0,1080) / (-1920,0,0,1040) | (-1160,966,-760,1022) |

## 실행한 명령과 결과

Cargo 호출 전 `PATH` 맨 앞에 `%LOCALAPPDATA%/DocFitTools/mingw64/bin`을 두고 `CARGO_TARGET_DIR`를 임시 환경변수로 지정했다.

- `cargo test --manifest-path dock/Cargo.toml`
  - `test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s`
  - lib 외 main/doc 테스트: 각각 `0 passed; 0 failed`.
- `cargo run --manifest-path dock/Cargo.toml -- --capture-dir $env:TEMP\DOCK6-captures --dpi 96`
  - `dock_light_96.png`
- `cargo run --manifest-path dock/Cargo.toml -- --capture-dir $env:TEMP\DOCK6-captures --dark --dpi 96`
  - `dock_dark_96.png`
- `cargo run --manifest-path dock/Cargo.toml -- --capture-dir $env:TEMP\DOCK6-captures --dpi 144`
  - `dock_light_144.png`
- `cargo run --manifest-path dock/Cargo.toml -- --capture-dir $env:TEMP\DOCK6-captures --hc --dpi 96`
  - `dock_hc_96.png`
- `flutter test test/tab_open_events_test.dart`
  - `00:00 +2: All tests passed!`
- `flutter analyze`
  - `No issues found! (ran in 11.5s)`
- `flutter build web --release`
  - `√ Built build\web`

전체 Python/Flutter 시험과 한/글 COM 시험은 실행하지 않았다.

## 사용자 실기 및 미확인

- 실제 Windows 작업표시줄을 아래·위·좌·우로 옮긴 상태와 자동 숨김에서 Dock 위치 및 포커스 동작 확인.
- 서로 다른 DPI를 쓰는 다중 모니터에서 전경 창 이동에 따라 Dock이 올바른 모니터로 이동하는지 확인.
- 실제 고대비 모드에서 시스템 팔레트와 구분선 가시성 확인.
- 실제 앱에서 테마 설정을 바꾸고 Dock이 곧바로 다시 그려지는지 확인. 현재 설정 파일은 paint 시 읽으며, 앱의 설정 변경이 Dock에 즉시 repaint를 알리는 연결은 별도 실기 확인이 필요하다.
- `docfit-close-tab-N` Win32 이벤트가 Flutter web의 `window` 이벤트까지 전달되는 IPC 브리지 구현 및 실제 앱 통합. 기존 `docfit-open-tab-N` 전달 경로도 같은 이유로 실기 확인이 필요하다.
- Dock이 Flutter 호스트를 여닫는 이벤트와 별개인 ktm/kvf/kagent 단독 창의 재클릭 닫기 계약은 아직 없다.
- 사용자가 금지한 현재 실행 앱·실제 작업 환경은 실행하거나 변경하지 않았다.
