# RL1F 단위 1: 실제 v2.2.48 앱 본체 기준 런처 기동 실측 비교 (C# vs Rust)

## 1. 개요 및 측정 목적

- **배경**: 이전 RL1E 측정은 간이 가짜 자식(`tests/fixtures/real_gui_child.cs`)을 대상으로 한 측정이었으므로, 대용량 실제 패키징 파일 및 pywebview 초기화가 수반되는 실제 앱 기준의 기동 성능을 반영하지 못함.
- **목적**: GitHub 릴리스 `v2.2.48 Portable` 안의 실제 앱(`HWP_AutoDocFit` 본체, 220MB 압축/온디렉터리 pywebview 앱)을 자식 프로세스로 하여, C# 런처와 Rust 런처가 '실제 앱 창 제목 「한글편집 후처리」가 보일 때까지'의 시간을 격리된 APPDATA 환경에서 각 1회 측정하고 비교 표를 작성함.
- **안전 규칙 준수**: 현재 실행 중인 다른 앱 및 K에이전트에 절대 간섭하지 않고, 실행된 자식 프로세스의 창 제목과 PID만을 추적하여 측정 종료 후 확실하게 종료 처리함.

---

## 2. 측정 환경 및 방법

- **OS / 플랫폼**: Windows 11 (64-bit)
- **자식 앱 대상**: GitHub 릴리스 `v2.2.48` 온디렉터리 번들 (`HWP_AutoDocFit.exe` / `app_main.exe` + `_internal`, 총 220MB 페이로드)
- **격리 APPDATA**:
  - `LOCALAPPDATA = <isolated>/local`
  - `APPDATA = <isolated>/roaming`
  - `DOCFIT_LAUNCHER_TEST_ROOT = <isolated>/cache`
  - `WEBVIEW2_USER_DATA_FOLDER = <isolated>/wv2`
- **측정 도구**: `scripts/bench_real_app_launch.py`
- **측정 항목**:
  1. **Cold Launch (콜드 기동)**: 캐시가 전혀 없는 상태에서 런처 실행 → 220MB 페이로드 압축 해제 → 자식 프로세스 기동 → 창 표시
  2. **Warm Launch (웜 기동)**: 캐시가 유효한 상태에서 런처 실행 → 유효성 검증 → 자식 프로세스 즉시 기동 및 뮤텍스 인계 → 창 표시
  3. **창 제목 「한글편집 후처리」 첫 표시 시간 (`first_window_ms`)**: 스플래시 창 포함 사용자가 첫 화면 피드백을 받는 시간
  4. **메인 앱 창 표시 시간 (`main_window_ms`)**: 실제 GUI 메인 화면(`한글편집 후처리 · v2.2.48`)이 뜨는 시간
  5. **런처 프로세스 종료 시간 (`launcher_exit_ms`)**: 뮤텍스 핸드오프 완료 후 런처가 정상 종료되는 시간

---

## 3. 단위 1 실측 결과 비교표

| 측정 항목 (단위: ms) | C# 런처 (`v2.2.48 Portable`) | Rust 런처 (`HWP_AutoDocFit_Rust.exe`) | 차이 및 분석 |
|---|---:|---:|---|
| **[Cold] 첫 창 「한글편집 후처리」 표시 (`first_window_ms`)** | **114.74 ms** | **15,924.23 ms** | **C# 대비 15.8초 지연 (Rust 스플래시 창 제목 불일치 결함 발견)** |
| ↳ 표시된 첫 창 제목 | `한글편집 후처리 (HWP_AutoDocFit) - 실행 준비` | `한글편집 후처리 · v2.2.48` | Rust는 스플래시 제목이 달라 첫 창으로 감지되지 않음 |
| **[Cold] 메인 창 표시 (`main_window_ms`)** | 12,551.42 ms | 15,924.23 ms | 220MB 압축 해제 및 파이썬 초기화 시간 포함 |
| **[Cold] 런처 프로세스 종료 (`launcher_exit_ms`)** | 8,857.58 ms | 11,947.93 ms | 핸드오프 신호 수신 후 런처 정상 종료 (ExitCode 0) |
| **[Warm] 메인 창 표시 (`main_window_ms`)** | **6,062.76 ms** | **6,141.60 ms** | **양측 대등 (약 6.0~6.1초, 실제 pywebview 로드 시간)** |
| ↳ 표시된 창 제목 | `한글편집 후처리 · v2.2.48` | `한글편집 후처리 · v2.2.48` | 양측 동일하게 메인 GUI 창 정상 표시 확인 |
| **[Warm] 런처 프로세스 종료 (`launcher_exit_ms`)** | **2,090.02 ms** | **1,988.76 ms** | **Rust 런처가 101.26 ms 더 빠름** |
| **프로세스 정상 종료 및 격리 여부** | 정상 종료 (Exit 0) | 정상 종료 (Exit 0) | PID 기반 안전 정리 완료, K에이전트 미간섭 |

---

## 4. 원인 분석 (단위 2 개선 대상 도출)

1. **Rust 런처 콜드 첫 창 표시 지연 (114ms vs 15.9초)**:
   - C# 런처(`launcher/smart_launcher.cs:16, 635`)는 콜드 런칭 시 즉시 `한글편집 후처리 (HWP_AutoDocFit) - 실행 준비`라는 제목의 네이티브 스플래시 창을 생성하여 114ms 만에 사용자에게 피드백을 표시함.
   - 반면 Rust 런처(`launcher_rs/src/main.rs:219`)는 스플래시 창 제목이 `HWP_AutoDocFit v{version} 준비 중`으로 하드코딩되어 있어, 창 제목 「한글편집 후처리」 규격에 부합하지 않고 첫 창 표시 감지에서 누락되어 15.9초 동안 사용자에게 올바른 제목의 준비 창을 노출하지 못함.
2. **단위 2 조치 계획**:
   - `launcher_rs/src/main.rs`의 네이티브 스플래시 창 제목을 C# 런처 규격인 `한글편집 후처리 (HWP_AutoDocFit) - 실행 준비`로 단 1개만 수정하여, 콜드 기동 시 즉각적인 창 표시를 보장함.
