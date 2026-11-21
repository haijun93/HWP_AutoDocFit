# LAUNCH1 결과 — 런칭 경로 분석 (2026-11-21)

## 요약

- 404는 **미확인**이다. 현재 CI 보고서는 콘솔에 `Failed to load resource: the server responded with a status of 404`라는 문구만 기록하고 요청 URL을 담지 않는다. `failed_requests`는 브라우저 요청 실패를 기록하지만 HTTP 404는 네트워크 실패가 아니며 이 필드로 특정되지 않는다.
- 빌드 HTML이 직접 참조하는 `docfit_bridge.js`, `flutter_bootstrap.js`와 Flutter 기본 진입 자원 `main.dart.js`, `flutter_service_worker.js`는 빌드 산출물에 존재한다. Flutter의 `assets/…`, CanvasKit 파일도 산출물에 존재한다. 대조 명령은 아래와 같다.
- 자동 favicon 요청은 후보이나 확정 근거가 없어 수정하지 않았다. `scripts/check_flutter_web_references.py`는 정적 참조와 대표 런타임 기본 경로를 확인하며, 브라우저가 자동 요청하는 favicon 등은 런타임 URL 추적이 필요하다고 명시한다.

## 웹 자원 대조

`flutter_ui/web/index.html`은 `docfit_bridge.js`, `flutter_bootstrap.js`를 상대 경로로 참조한다. `desktop_web_ui.py:4006-4012`의 `_resource_path()`는 `desktop_web/index.html`을 선택하고, `desktop_web_ui.py:4262-4270`은 파일 경로로 창을 만들며 `desktop_web_ui.py:4396`은 pywebview HTTP 서버를 켠다. Flutter 산출물은 별도 `flutter_ui/build/web` 경로에서 만든다.

정적 대조 명령:

```powershell
python scripts/check_flutter_web_references.py flutter_ui/build/web
```

검사 결과(2026-11-21): 모든 HTML 참조와 Flutter 진입 자원이 있음. `favicon.ico`는 없음(자동 요청 후보). 이 결과만으로 관측된 404 URL을 확정하지 않는다.

미확인 후보: 브라우저의 암묵적 `favicon.ico`; 런타임에서 조합되는 Flutter asset URL; HTTP 서버의 상대 URL 매핑. `webview-content-smoke.json`에 요청 URL을 포함한 응답 상태 기록이 없어 셋 중 실제 원인은 미확인이다.

## 시작 타임라인과 CI 측정치

| 단계 | 코드 경로 | 의미 및 대응 측정 |
|---|---|---|
| 스마트 런처 진입 | `launcher/smart_launcher.cs:Main` | 매니페스트 확인, 캐시 경로 계산, 필요 시 페이로드 추출 후 `app_main.exe` 실행. 별도 self-test: 818 ms 콜드, 767 ms 캐시 시작(목표 1,500 ms). |
| Python 모듈 import | `hwp-auto-docfit.py:224-284` 및 모듈 상단 import | UI 진입 전에 광범위한 표준/앱 모듈 import. 별도 START2 실측 중앙값 894.7 ms/249개 모듈(`docs/RUSTALL1_PLAN_2026-11-13.md:68`); 2.2.86 CI 결과와 같은 프로세스 구간 측정은 아님. |
| 앱 준비·창 호출 | `hwp-auto-docfit.py:3402-3490` | 런타임 배포·kfind 확인 작업을 시작하고 Tk 이벤트 루프를 백그라운드 스레드로 띄운 뒤 `run_webview` 호출. |
| WebView2 창 생성 | `desktop_web_ui.py:4225-4270` | `_resource_path()`가 가리키는 HTML을 URL로 창 생성. 콜드 앱 UI 창 준비 4,604 ms, 웜 첫/두 번째 1,336/1,317 ms. 측정 기준은 버전 창을 찾은 시점(`scripts/smoke_windows_exe.py`); 첫 Flutter 프레임과 다름. |
| 웹 초기화·부가 서비스 | `desktop_web_ui.py:4337-4396` | loaded 이벤트에서 스플래시 닫기; 콜백에서 설정·서비스를 초기화하고 WebView 루프 시작. |
| Flutter 첫 프레임 | `flutter_bootstrap.js` → `main.dart.js`, 앱에서 `window.docfitFirstFrame` 표시 | 별도 웹 콘텐츠 smoke 첫 프레임 3,264 ms. 창 준비 후 추가된 절대시간으로 단순 합산할 수 없음(측정 시작점 차이). |

## 스플래시 판정 기준

`scripts/smoke_windows_exe.py:43-48`은 창 제목이 `실행 준비`인 런처 스플래시를 5ms 주기로 처음 발견한 시간을 `first_run_splash_ms`로 기록한다. `scripts/smoke_smart_launcher.py:59-72`는 이 smoke를 캐시가 없는 조건에서 실행하고, `:99-107`에서 `first_run_splash_ms <= target_splash_ms`를 `cold_ui_splash_within_target`로 판정한다. GitHub workflow는 `--target-splash-ms 100`을 전달한다.

따라서 100ms는 스플래시가 보이는 지속시간이 아니라 **프로세스 시작 후 스플래시 창이 관찰될 때까지의 지연 목표**다. 관측값 100.01ms는 100ms를 0.01ms 초과해 false다. 5ms polling 간격과 OS 스케줄링/창 열거 오차가 있는 측정에서 100ms 경계는 지나치게 엄격하고, 화면 캡처로 실제 표시 프레임 시각을 확인하지 않는다. 목표 조정이나 계측 변경은 이번 카드에서 하지 않았다. `--require-target`이 꺼져 workflow에서는 기록되지만 이 값 자체로 빌드가 실패하지 않는다; 함수가 해당 플래그가 있을 때만 목표 미달로 종료한다.

## 지연 로드 후보 (문서 제안만)

| 후보 | 잠재 효과 | 위험·검토점 | 근거 |
|---|---|---|---|
| kordoc 런타임 배포를 첫 사용 시까지 연기 | 앱 시작 때 ZIP 검증/추출 스레드 경쟁·디스크 작업 감소 가능 | 첫 DOCX/변환 사용이 늦어지거나 배포 실패가 최초 작업과 겹침 | `hwp-auto-docfit.py:3428` |
| kfind 프로세스 확인을 검색/색인 화면 진입 때 시작 | 무관한 첫 화면에서 별도 exe 확인·시작 비용 제거 가능 | 사용자가 시작 직후 검색/색인을 열면 첫 요청이 늦어짐; 중복 시작/복구 상태 관리 필요 | `hwp-auto-docfit.py:3432` |
| kagent·kvf·ktm 동기화/업그레이드 초기화를 첫 관련 기능 사용 때 시작 | WebView UI 로드 직후의 서비스 설정·프로세스/네트워크 작업 경쟁 감소 가능 | 클립보드·가상폴더·몽땅검색 기능이 늦게 준비되며 설정 변경과 동시 진입 경합 가능 | `desktop_web_ui.py:4353-4381` |

각 항목은 시작 시 꼭 필요한 상태인지와 사용자 첫 상호작용 영향을 별도 계측한 뒤 구현을 제안한다. 코드는 변경하지 않았다.

## 변경 및 검증

- 코드 수정 없음. `hwp-auto-docfit.py` 및 PC 설치 앱/실행 프로세스를 건드리지 않았다.
- `flutter analyze`: 성공, `No issues found! (ran in 19.9s)`.
- `flutter build web --release`: 성공, `√ Built build\web` (93.8s).
- 관련 Python 시험은 실행하지 않음. 404 자원이 특정되지 않아 수정 대상이 없었다.
