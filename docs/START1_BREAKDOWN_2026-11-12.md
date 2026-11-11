# START1 시작 속도 분해 측정 (2026-11-12)

## 범위와 방법

이번 카드는 측정 도구 작성과 1회 측정만 수행한다. 애플리케이션 코드는 수정하지 않았다. 측정은 아래 Python으로 `hwp-auto-docfit.py`를 소스 모듈로 import했으며, exe나 `main()`을 실행하지 않아 UI는 생성되지 않았다.

```text
C:/HWP_AUTODOCFIT/v1803/exfile/.venv/Scripts/python.exe
```

`scripts/startup_breakdown.py`가 별도 프로세스의 `-X importtime` stderr를 파싱해 누적 시간이 높은 모듈 30개를 저장한다. 인터프리터 시작부터 모듈 import 종료까지의 외부 wall clock도 잰다. JSON 결과는 `docs/startup-breakdown.json`에 있다.

## 기존 시작 속도 측정 방식

| 위치 | 기존 측정 내용 | 한계/해석 |
|---|---|---|
| `scripts/smoke_smart_launcher.py` | EXE의 cold/warm self-test 경과 시간, warm UI의 `window_ready_ms`, cold 스플래시 표시 시간, 캐시 marker 보존 | launcher 및 EXE 동작 측정. 소스 단계별 시간은 분리하지 않음 |
| `.github/workflows/build-windows-exe.yml` 시작 속도 단계 | `smoke_smart_launcher.py --report dist/smart-launcher-startup.json --target-cache-ms 1500 --target-splash-ms 100` 실행. workflow 주석에 목표는 기록만 하고 빌드를 막지 않는다고 명시 | EXE 전용 smoke 결과를 artifact/release-assets에 보관 |
| `release-assets/smart-launcher-startup.json` | 버전/payload hash, self-test/cached start, cold/warm UI 준비 시간, 스플래시 및 캐시 목표 충족 여부 | 현재 저장된 표본에서 self-test 825.91ms, cache 855.35ms, cold window 6631.44ms, splash 115.01ms, warm UI 1573.11/1522.52ms |
| `tools/startup_import_total.py` | `-X importtime`으로 소스 모듈 import 누적 self 시간 합계와 subprocess wall clock. import worker가 `hwp-auto-docfit.py`를 실행하지 않고 import | 시작 경로 import 총합/비교 도구. 개별 단계나 상위 모듈 30개를 보고하지 않음 |
| `scripts/` launcher 관련 | `build_smart_launcher.py`, `smoke_smart_launcher.py` | 빌드와 EXE launcher 검증이며 소스 초기화 세분 측정은 없음 |

### 이번 실행에서 확인한 분해 가능 범위

`--self-test`는 `hwp-auto-docfit.py`가 `__main__`일 때 패키징 검사를 하고 일반 GUI 초기화 전에 종료하는 frozen smoke 경로다. 일반 소스 시작의 설정 파일 읽기나 웹 화면 준비는 통과하지 않는다. 일반 경로에서 `main()`은 Tk/WebView 창과 이벤트 루프를 만들고 COM/UI 초기화를 시작하며, 이를 실행하지 않고 설정 읽기 및 “웹 화면 창 생성 직전”까지 측정하는 기존 무화면 옵션은 찾지 못했다. 따라서 본 회차는 ① 인터프리터 시작~모듈 진입/import 종료와 ② import 누적 상위 30개까지만 기록한다. ③ 설정 읽기와 ④ 웹 창 생성 직전은 미측정이다.

또한 소스 import 방식은 `__name__ == "__main__"`가 아니므로 진입점의 `main()` 호출 자체를 포함하지 않는다. `interpreter_to_entry_ms`는 인터프리터 기동, 측정용 `-c` bootstrap, 대상 소스 import를 포함한 wall clock 근사치다. importtime 활성화와 editable 개발 환경의 import finder가 있어 일반 사용자 실행 시간과 직접 등치할 수 없다.

## 1회 측정 결과

| 구간 | 측정값 | 비고 |
|---|---:|---|
| ① 인터프리터 기동~소스 모듈 import 종료 | 1449.19 ms | `-X importtime`, import-only subprocess wall clock |
| ② importtime 기록 항목 수 | 294개 | 전체 기록 중 누적 시간 상위 30개를 아래 JSON에 포함 |
| ③ 설정 파일 읽기 | 측정 안 함 | import-only 무화면 경로에서 설정 파일을 읽지 않음 |
| ④ 웹 화면 창 생성 직전까지 | 측정 안 함 | 일반 `main()` 진입 시 UI 생성 경로라 안전한 무화면 경로 확인 불가 |

### Import 누적 시간 상위 10개

| 순위 | 모듈 | 누적 시간 | 시작 시점 필수 여부 후보 |
|---:|---|---:|---|
| 1 | `site` | 85.476 ms | Python 환경 초기화에 해당. 앱 관점에서 제거 후보로 보기 어려움 |
| 2 | `http.client` | 84.495 ms | 추측: 첫 화면 전 네트워크 기능이 필수인지 근거를 확인하지 못해 지연 가능성 후보 |
| 3 | `__editable___govflow_python_worker_0_1_0_finder` | 66.552 ms | 개발 venv editable 설치 finder로 보임. 추측이며 배포 환경 대표성이 낮음 |
| 4 | `traceback` | 62.383 ms | 추측: 오류 보고에 쓰일 수 있으나 첫 화면 전에 꼭 필요한지는 불명 |
| 5 | `docfit_engine.runtime` | 50.142 ms | 엔진 함수 연결에 쓰이는 것으로 보이며 시작 필수 여부는 기능 영향 검토 필요 |
| 6 | `tkinter` | 47.593 ms | 창 생성에 필요하므로 시작 경로에서 제외하기 어려움 |
| 7 | `_colorize` | 46.413 ms | 출처/필요성 미확인. 추측성 후보 |
| 8 | `docfit_core.batch_format` | 46.402 ms | 문서 처리 엔진으로 보임. 추측: 첫 창 표시 이전에 필수인지 검토 후보 |
| 9 | `win32com.client` | 46.367 ms | COM 통합에 사용됨. 첫 화면 이전 필수인지는 확인 필요 |
| 10 | `docfit_core.korean_proofread` | 45.172 ms | 교정 기능으로 보임. 추측: 시작 즉시 필요하지 않을 가능성이 있어 후보 |

후보 표시는 import명만 보고 한 추측이며 최적화 제안이나 안전성 판정이 아니다. 특히 30위 안에는 Tk/COM/엔진 관련 모듈이 섞여 있어 실제 사용 흐름과 의존 관계 확인 없이는 지연 로딩 적합성을 결론내릴 수 없다.

## 구현 및 시험

- 추가: `scripts/startup_breakdown.py`, importtime 파서의 순수 함수 시험 `tests/test_startup_breakdown.py`.
- 결과: `docs/startup-breakdown.json`.
- 지정 Python으로 `-X utf8 -m unittest tests.test_startup_breakdown`: 2 tests, OK.
- 1회 측정: `interpreter_to_entry_ms=1449.19`, import 기록 294개.
- 응용 프로그램 코드, 기존 시험 기대값, 원본 사용자 문서는 수정하지 않았다.

## 확인하지 못한 점

- 한글 COM을 사용하지 않았으며 잠금 파일도 필요하지 않아 COM 시험은 실행하지 않았다.
- 설정 파일 읽기 및 웹 창 생성 직전 구간은 무화면 계측 경로가 없어 미측정이다.
- 결과는 지정 PC의 개발 가상환경에서 1회 수집한 표본이다. 반복 측정이나 exe 결과와의 직접 비교는 하지 않았다.
