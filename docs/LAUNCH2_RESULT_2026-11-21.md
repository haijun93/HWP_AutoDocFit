# LAUNCH2 결과 — 시작 부가 작업 지연 (2026-11-21)

## 선택한 항목 (2건)
| 항목 | 이전 위치 | 이후 | 왜 안전한가 |
|---|---|---|---|
| kordoc 런타임 배포 확인 | `hwp-auto-docfit.py` `main()` 에서 `run_webview` 전에 스레드 시작 | 웹 창 `loaded` 이벤트 뒤 스레드 | 이미 백그라운드 스레드·실패 무시. 첫 화면과 무관. 순서만 변경(`desktop_web_ui.py` `_on_loaded`). |
| kfind 프로세스 확인(`ensure_kfind_running`) | 같은 곳 | 같은 방식으로 미룸 | 이미 백그라운드·예외 무시. 첫 화면에 필요 없음. 검색 화면은 자체적으로 서비스 상태를 확인. |

Tk 전용/웹뷰 미설치/웹 시작 실패 경로에서는 기존처럼 즉시(또는 폴백 직전) 실행한다. 실행은 최초 1회만 보장(`DeferredStartup.trigger`).

## 보류
- kagent 앱 서비스 파이프·`upgrade_if_stale`·kvf 동기화·ktm 동기화(`desktop_web_ui.py initialize_webview`): 클립보드·가상폴더·몽땅검색이 시작 직후 사용자에게 보이는 기능이고 설정 경합 가능성이 있어 이번에는 그대로 둔다. 이미 창 생성 후 스레드에서 실행됨.
- `refresh_registered`(셸 메뉴 갱신), 단일 인스턴스 확인: 시작 순서에 의존하므로 변경하지 않음.
- 업데이트 확인: 시작 경로에서 찾지 못함(별도 서비스 호출 시점 미확인).

## 측정 장치
`docfit_app/startup_deferral.py`: `mark(name)`·`summary()`. 기록 지점 `main_enter`, `webview_loaded`, `deferred_start`(프로세스 시작 기준 ms). `docfit_infra.startup` 로거에 `startup timeline: ...` 한 줄 요약(경로·개인정보 없음). 
주의: 모듈 import 시점이 기준이라 인터프리터 시작~import 구간은 제외된다.

## 변경 파일
`hwp-auto-docfit.py`(CRLF 유지, 줄 수 3522 = CRLF 3522), `desktop_web_ui.py`, `docfit_app/startup_deferral.py`(신규), `tests/test_startup_deferral.py`(신규), `tests/test_kfind_process.py`(배선 검사 갱신), 이 문서.

## 시험
- `python -m unittest tests.test_startup_deferral tests.test_kfind_process`: 22건 OK.
- 관련 파일 묶음(`desktop_web_ui`·kfind 언급 60여 파일): 602건, 실패 1건(kfind 배선 — 위에서 갱신해 통과), 오류 2건은 `-m unittest 모듈명` 호출 방식의 import 경로 문제로, `discover` 로 다시 실행해 13건·9건 OK.
- Dart 변경 없음 → `flutter analyze`·웹 빌드는 실행하지 않음.

## 예상 효과 (추정, 측정 전)
시작 초반 Python/WebView2 생성과 경쟁하던 ZIP 검증·외부 exe 확인이 빠져 콜드 창 준비가 수십~수백 ms 줄 수 있다. 크기는 미측정이며, 다음 릴리스 CI 로그의 `startup timeline` 으로 확인해야 한다.

## 미확인
- 실제 앱 실행 효과(설치된 앱은 실행하지 않음).
- `loaded` 는 HTML 로드 시점이며 Flutter 첫 프레임보다 앞선다(첫 프레임 뒤로 더 미루는 것은 JS 신호 연동 필요 — 후속 과제).
- 설치 직후 첫 DOCX 변환이 `loaded` 이전에 요청될 때의 배포 경합(기존에도 별도 잠금 사용, 미실측).
