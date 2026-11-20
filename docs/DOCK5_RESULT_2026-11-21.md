# DOCK5 결과 — 2026-11-21

## 현황 대조

| 항목 | 시작 전 상태 | 이번 작업 / 판정 |
|---|---|---|
| 아이콘 줄·툴팁·키보드·실행 점 | Rust GDI 경로와 순수 함수가 있음 | 기존 동작 보존. 실제 실행 중 창 추적은 구현되지 않아 점은 현재 프로세스의 클릭 상태만 반영 |
| `hidden_tabs` | `%APPDATA%/HwpAutoDocFit/settings.json`을 Rust에서 직접 읽음 | 유지. 사용자 설정 기본 숨김 조합과 Flutter의 `kDefaultHiddenTabs`가 완전히 같은지는 추가 연결 검증 필요 |
| 메뉴 묶음 순서·구분선 | 0~15 탭 번호 순서, 구분선 없음 | 사이드바 4묶음과 동일한 순서로 정렬하고 묶음 경계선 추가 |
| 설정 전환·시작 실패 폴백 | Flutter `DockModeController`가 설정 저장, 준비 대기, 실패 시 사이드바 복구 | 기존 구현. Python 런타임 시험과 Flutter 컨트롤러 시험 통과 |
| 파이프·탭 라우팅 | Named pipe와 `docfit-open-tab-N` 이벤트 경로, ktm/kvf/kagent 분기 있음 | 기존 경로 확인. 실제 운영 실행 파일 연결/화면 라우팅 실기는 미수행 |
| 재클릭 토글 | 내부 `WindowState`만 바뀌며 매 클릭 `open_tab` 호출 | 앱 창 숨김/복귀까지 이어지는 토글은 미구현 |
| 위치·작업표시줄·모니터·DPI | 주 모니터 rcWork 기반, `WM_DISPLAYCHANGE`·`WM_DPICHANGED`에서 재배치 | 위치 계산 순수 시험 있음. 자동 숨김·상단/좌우 작업표시줄·혼합 DPI 실기 미수행. 현재 자동 가장자리 선택은 측면 작업표시줄에서 세로 Dock을 선택할 수 있음 |
| 다크/밝게/고대비 | 고대비 시스템 팔레트, 고정 일반 팔레트 | 밝게/다크 설정 연동 미구현. 고대비 순수 함수만 시험 |
| 단일 인스턴스 | 이름 있는 뮤텍스 | 코드 확인, 실행 중인 프로세스 실험은 사용자 금지 범위라 미수행 |
| 캡처·자원 | 자원 스크립트 있음, 캡처 경로 없음 | PNG 캡처 모드를 추가하지 못해 캡처/Read 관찰과 자원 실측 미수행. 가짜 화면/상태를 만들어 검증하지 않음 |

## 변경 파일

- `dock/src/lib.rs`: 사이드바의 4개 그룹/순서와 Dock 메뉴 모델 연결, 그룹 ID 제공, 그룹 순서·숨김 탭 시험 추가.
- `dock/src/native.rs`: 메뉴 개수에 따라 Dock 폭을 산정하고 그룹 경계선을 GDI로 표시.
- `TODO.md`: DOCK5 결과와 잔여 항목을 연결.

`hwp-auto-docfit.py`, 설정 브리지, Flutter 소스, `.cargo/config.toml`은 변경하지 않았다.

## 실행한 명령과 결과

- `cargo test --manifest-path dock/Cargo.toml` — 12 passed.
- `python -m unittest discover -s tests -p test_dock_runtime.py` — 4 passed.
- `flutter test test/dock_mode_controller_test.dart test/tab_navigation_shell_test.dart` (`flutter_ui`) — all tests passed (14).
- `cargo fmt --manifest-path dock/Cargo.toml -- --check` — 실패. 기존 두 Rust 소스 전체가 rustfmt 형식과 달라 대규모 무관 변경을 만들 수 있어 일괄 포맷은 적용하지 않음.

모든 Cargo 호출은 `PATH` 맨 앞에 `%LOCALAPPDATA%/DocFitTools/mingw64/bin`, `CARGO_TARGET_DIR`는 임시 환경변수로 지정했다. 전체 시험은 실행하지 않았다.

## 캡처 관찰

캡처 PNG가 생성되지 않아 라이트·다크·150% DPI의 16개 아이콘, 이름, 실행 중 점, 구분선을 이미지로 Read 검증하지 못했다. 따라서 캡처 검증은 미완료다.

## 사용자 실기 / 미확인

- 실제 Windows에서 작업표시줄 자동 숨김·위/좌/우 배치별 Dock 위치와 포커스.
- 100–200% 혼합 DPI 및 다중 모니터 배치. 앱이 있는 모니터로 이동 옵션은 제안만 하기로 한 상태.
- 실제 한/글 앱에서 아이콘 클릭 라우팅, 재클릭 시 창 토글, 실행 중 표시.
- 라이트/다크/고대비 화면 비교.
- Dock 단독 CPU·메모리·핸들 1회 측정.
- Dock의 재클릭 토글은 현재 표시 상태만 전환하는 완결 기능이 아니며, 화면 복귀 이벤트 계약을 추가해야 한다.
