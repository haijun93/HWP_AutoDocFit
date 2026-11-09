# RL1E 단위 1: self-test 없는 실제 사용 경로 C# vs Rust 런처 1회 측정 보고서

## 1. 측정 개요
- **환경**: Windows 11, 동일 PC, 격리된 APPDATA / LOCALAPPDATA / TEST_ROOT
- **경로**: `--self-test` 인자 없이 런처 단독 실행 (실제 사용자 GUI 실행 경로)
- **자식 앱**: 실제 GUI 창(`HWP_AutoDocFit v{version}`)을 생성·표시하고 `DOCFIT_LAUNCHER_HANDOFF_EVENT` 신호 인계 수행
- **측정 도구**: `scripts/bench_real_launch.py`, `tests/fixtures/real_gui_child.cs`

---

## 2. 실측 결과 비교표 (1회 측정)

| 항목 (단위: ms) | C# 런처 (`smart_launcher.cs`) | Rust 런처 (`launcher_rs/src/main.rs`) | Rust 상태 및 병목 분석 |
|---|---:|---:|---|
| **콜드 앱 창 표시 (`window_ready_ms`)** | **605.59 ms** | **192.24 ms\*** | \*Rust는 자식 앱 창이 아닌 런처 스플래시 창만 감지됨 |
| **콜드 런처 종료 (`launcher_exit_ms`)** | **617.81 ms** | **244.81 ms** | 자식 프로세스 미실행 상태에서 조기 리턴 (ExitCode 0) |
| **웜(캐시 히트) 앱 창 표시 (`window_ready_ms`)** | **357.25 ms** | **미표시 (null / 실패)** | **★ 자식 프로세스 미실행으로 앱 창 표시 실패** |
| **웜 런처 종료 (`launcher_exit_ms`)** | **357.25 ms** | **60.16 ms** | 자식 미실행 후 조기 종료 |
| **웜 실행 성공 여부 (`ok`)** | **성공 (true)** | **실패 (false)** | 자식 앱 창 및 핸드오프 인계 미도달 |

---

## 3. Rust 런처 실제 경로 미실행/지연 원인 규명

### 3.1 원인: 잘못된 인스턴스 뮤텍스 가드 조건 (`main.rs:350`)
```rust
// 현재 코드 (launcher_rs/src/main.rs:350)
let self_test = args.first().map(|a| a == "--self-test").unwrap_or(false);
#[cfg(windows)]
let instance = if self_test || env::var_os("DOCFIT_LAUNCHER_TEST_ROOT").is_some() || args.first().map(|a| !a.starts_with('-')).unwrap_or(false) {
    None
} else { acquire_single_instance()? };
```
그리고 372행:
```rust
#[cfg(windows)]
if instance.is_none() && !self_test {
    // A second ordinary GUI launch focuses the app via its existing instance path.
    return Ok(0);
}
```

1. **테스트 격리 환경 차단**:
   - `DOCFIT_LAUNCHER_TEST_ROOT` 환경변수가 있을 때 `acquire_single_instance()`는 이미 내부에서 `_Test` 접미사를 붙여 격리된 테스트 뮤텍스를 사용하도록 설계되어 있음.
   - 그러나 350행에서 `TEST_ROOT`가 존재하면 무조건 `instance = None`으로 강제함.
   - 그 결과 372행에서 `instance.is_none() && !self_test`에 걸려 **자식 앱 프로세스를 시작조차 하지 않고 0으로 즉시 종료**되어 앱 창이 화면에 뜨지 않음.

2. **일반 파일 인자 실행 차단**:
   - 사용자가 문서를 열기 위해 `launcher.exe "doc.hwp"`와 같이 실행하면 `args.first()`가 `-`로 시작하지 않음.
   - 이 경우에도 `instance = None`이 되어 앱이 실행되지 않고 꺼지는 치명적 결함 존재.

3. **핸드오프 이벤트 레이스 컨디션 및 폴링 지연**:
   - 자식 프로세스 시작(`spawn()`) 이후에 `CreateEventW`를 생성하여 자식의 `OpenEventW`와 레이스 발생.
   - `WaitForSingleObject(event, 0)` 후 `thread::sleep(100)`을 수행하여 커널 이벤트를 즉시 수신하지 못하고 최대 100~500ms 지연 발생.
