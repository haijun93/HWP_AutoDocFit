# RL1E 단위 3: 실제 GUI 실행 경로(self-test 없음) 전후 비교 보고서

## 1. 측정 개요
- **환경**: Windows 11, 동일 PC, 격리된 APPDATA / LOCALAPPDATA / TEST_ROOT
- **경로**: `--self-test` 인자 없이 런처 단독 실행 (실제 사용자 GUI 실행 경로)
- **자식 앱**: 실제 GUI 창(`HWP_AutoDocFit v{version}`)을 생성·표시하고 `DOCFIT_LAUNCHER_HANDOFF_EVENT` 신호 인계 수행
- **측정 도구**: `scripts/bench_real_launch.py`, `tests/fixtures/real_gui_child.cs`

---

## 2. C# vs Rust(수정 전) vs Rust(수정 후) 실측 비교표

| 측정 항목 (단위: ms) | 기존 C# 런처 | Rust 런처 (수정 전) | Rust 런처 (수정 후) | 개선 효과 및 C# 대비 |
|---|---:|---:|---:|---|
| **웜(캐시 히트) 앱 창 표시 (`window_ready_ms`)** | 411.01 ms | **미표시 (null / 실패)** | **11.20 ms** | **창 미표시 결함 해결, C# 대비 36.7배 빠름** |
| **웜 런처 인계 종료 (`launcher_exit_ms`)** | 411.01 ms | 60.16 ms (미실행 조기종료) | **73.39 ms** | **핸드오프 정상 인계 완료 (ExitCode 0), C# 대비 5.6배 빠름** |
| **콜드(첫 추출) 창/스플래시 표시 (`window_ready_ms`)** | 750.91 ms | 192.24 ms | **219.02 ms** | **C# 대비 3.4배 빠름 (스플래시 즉시 표시)** |
| **콜드 런처 인계 종료 (`launcher_exit_ms`)** | 762.09 ms | 244.81 ms (미실행 조기종료) | **532.49 ms** | **압축 해제 + 자식 인계 정상 완료, C# 대비 1.4배 빠름** |
| **실제 GUI 창 표시 및 핸드오프 성공 여부 (`ok`)** | **성공 (true)** | **실패 (false)** | **성공 (true)** | **실제 사용 경로 완전 정상화** |

---

## 3. 수정 구간 및 기술적 개선 내역

1. **인스턴스 뮤텍스 가드 조건 정상화 (`launcher_rs/src/main.rs:350`)**:
   - `DOCFIT_LAUNCHER_TEST_ROOT` 및 파일 인자 시 `instance = None`으로 강제하던 잘못된 가드를 제거.
   - `self_test`일 때만 뮤텍스 획득을 생략하고, 일반 GUI 기동 시에는 C#과 동일하게 격리 접미사(`_Test`)가 반영된 단일 인스턴스 뮤텍스를 획득하도록 정상화.

2. **핸드오프 이벤트 사전 생성 및 레이스 컨디션 제거 (`launcher_rs/src/main.rs:379`)**:
   - `command.spawn()` 이전에 `CreateEventW`로 `Local\HWP_AutoDocFit_Handoff_<PID>` 이벤트를 생성하여, 자식 프로세스가 시작 즉시 `OpenEventW`를 호출해도 레이스 컨디션 없이 이벤트를 열 수 있도록 보장.

3. **커널 이벤트 블로킹 대기로 전환 (`launcher_rs/src/main.rs:310`)**:
   - 기존의 비효율적인 `WaitForSingleObject(event, 0)` + `thread::sleep(100)` 폴링 루프를 제거.
   - `WaitForSingleObject(event, 100)` 커널 블로킹 대기로 전환하여, 자식 프로세스가 `SetEvent`를 호출하는 순간 0ms(커널 즉시 웨이크업)로 감지하고 뮤텍스를 즉시 인계(Release)하여 런처가 73.39ms 만에 정상 종료되도록 최적화.
