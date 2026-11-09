# RL1D 단위3: 동일 PC 1회 측정 전후 비교 보고서

## 1. 측정 환경
- **작업 폴더**: `C:/HWP_AUTODOCFIT/v1803/rl1`
- **Git 브랜치**: `feature/rust-launch-rl1`
- **Python**: `C:/Users/haiju/OneDrive/바탕 화면/HWP_AutoDocFit/mapo-agent-1/.venv/Scripts/python.exe`
- **Cargo**: `cargo 1.99.1` (%LOCALAPPDATA%/DocFitTools/mingw64/bin)
- **격리 설정**: `LOCALAPPDATA`, `DOCFIT_LAUNCHER_TEST_ROOT`, `DOCFIT_LAUNCHER_MANIFEST`, `DOCFIT_LAUNCHER_PAYLOAD` 격리 디렉터리 사용 (사용자 실행 중인 앱·K에이전트 격리 보존)

## 2. 측정 결과 비교 (전후 1회 실측)

| 측정 항목 (단위: ms) | 기존 C# 런처 (`launcher/smart_launcher.cs`) | Rust 런처 (수정 전: 6.6초 대기 버그) | Rust 런처 (수정 후: 인자 정상 전달) | 개선 효과 (전후 비교) |
|---|---:|---:|---:|---|
| **cached_ms (캐시 히트 기동)** | 658.71 ms | **6,644.03 ms** | **134.87 ms** | **49.3배 단축** (6.6초 지연 완전 소멸) |
| **child-exit (자식 프로세스 대기)** | 0 ms | **6,634.00 ms** | **75.00 ms** | **98.9% 단축** (자식 정상 종료) |
| **self_test_ms (첫 실행/검증)** | 931.11 ms | **9,231.76 ms** | **425.48 ms** | **21.7배 단축** |
| **second_consecutive_ms (연속 실행)** | 630.42 ms | **7,554.94 ms** | **132.03 ms** | **57.2배 단축** |
| **cold_ms (콜드 추출 후 기동)** | 743.41 ms | **8,614.12 ms** | **277.30 ms** | **31.1배 단축** |
| **자식 크래시 여부** | 없음 | `ArgumentNullException` (WerFault 대기 발생) | **정상 0 종료** | 크래시 완전 해결 |

## 3. 세부 추적 로그 (HWP_LAUNCHER_TRACE)

### 수정 전 (RL1B/RL1C 실측)
```json
"rust": {
  "cached": [
    "HWP_LAUNCHER_TRACE stage=manifest-check elapsed_ms=0 total_ms=0",
    "HWP_LAUNCHER_TRACE stage=cache-path elapsed_ms=0 total_ms=0",
    "HWP_LAUNCHER_TRACE stage=manifest-validation elapsed_ms=0 total_ms=1",
    "HWP_LAUNCHER_TRACE stage=child-start elapsed_ms=18 total_ms=19",
    "HWP_LAUNCHER_TRACE stage=child-exit elapsed_ms=6634 total_ms=6654"
  ]
}
```

### 수정 후 (Unit 2 적용 후 동일 PC 실측)
```json
"rust": {
  "cached": [
    "HWP_LAUNCHER_TRACE stage=manifest-check elapsed_ms=0 total_ms=0",
    "HWP_LAUNCHER_TRACE stage=cache-path elapsed_ms=0 total_ms=0",
    "HWP_LAUNCHER_TRACE stage=manifest-validation elapsed_ms=0 total_ms=1",
    "HWP_LAUNCHER_TRACE stage=child-start elapsed_ms=0 total_ms=1",
    "HWP_LAUNCHER_TRACE stage=child-exit elapsed_ms=75 total_ms=77"
  ]
}
```

## 4. 요약
자식 프로세스 실행 시 `--self-test` 인자가 정상 전달됨에 따라 자식 fixture가 정상적으로 플래그를 인식하고 즉시 응답하게 되었으며, `child-exit` 대기 시간이 6,634ms에서 75ms로 단축되어 총 캐시 기동 시간이 6,644ms에서 134ms로 혁신적으로 단축되었습니다.
이는 기존 C# 런처(658ms) 대비 4.9배 빠른 성능입니다.
