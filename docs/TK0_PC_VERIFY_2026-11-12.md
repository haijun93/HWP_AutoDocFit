# TK0 Windows 실측 — 2026-11-12

실제 숨은 Tk 이벤트 루프에서 14종 × 50회(700회)를 실행했다. 측정 묶음의 Tk 왕복은 **703 → 150회(78.66% 감소)**였다. 반복마다 Tk를 기다리던 조회의 중앙 응답 시간이 감소했다. 서비스 최초 생성 때만 왕복하던 두 경로의 중앙값은 오히려 소폭 증가했으므로 모든 호출이 빨라졌다고 판단하지 않는다. 단일 실행 결과이며 통계적 유의성이나 실제 Flutter 화면 체감 개선을 입증하는 자료는 아니다.

## 실행 환경과 범위

- 브랜치: `verify/tk0-pc`. 변경 전: `E:/docfit-tmp/tk0base`, `d7e979dc36228d1caea333fc9bbef87f2185491e`. 변경 후: `E:/docfit-tmp/tk0pc`, `7ba67bcc430f1495cb67f6ea48725e9397254d2a`에 검증 스크립트 추가. 기준 코드 폴더의 추적 파일 변경 없음.
- 먼저 `HANDOVER.md`, `docs/TK0_INVENTORY_2026-11-12.md` 5절, `84713b0a`·`496ba092`의 코드 변경, 두 smoke 스크립트를 확인했다.
- 지정 Python으로 실행, 실제 Tcl/Tk 버전은 각 `result.json`에 기록. `tkinter.Tk().withdraw()`와 `mainloop()`, 운영 `install_scheduler`의 10ms 큐 펌프, 운영 `DesktopWebBridge`, 운영 설정·서비스를 사용했다. Mock, 가짜 root, 즉시 실행 `_tk` 대체를 사용하지 않았다.
- **2-나 대체 경로**: 두 폴더에 `desktop_web/flutter`가 없다. `run_webview`도 소스/EXE 모두 `_resource_path()`와 `DOCFIT_SMOKE_CDP_PORT`를 사용하므로 자산이 있으면 소스에서도 같은 CDP 연결 방식이 가능하다. 다만 `_resource_path()`가 반환할 수 있는 예전 `desktop_web/index.html`은 존재한다. 그 화면을 Flutter 화면 검증으로 대체하지 않았다.
- **전체 `hwp-auto-docfit.py` 앱과 WebView2 창은 실행하지 않았다.** 숨은 Tk 호스트가 상태 조회용 실제 Tk 변수와 AppState를 구성했다. 기존 GUI 생성자는 호출하지 않았으므로 실제 앱 초기화 부하·WebView 직렬화·렌더링·화면 정상 여부를 검증한 결과가 아니다. 스크린샷: **없음(PNG 미생성)**.
- `APPDATA`, `LOCALAPPDATA`, `TEMP`, `TMP`는 실행별 산출물 하위로 격리하고 `DOCFIT_OFFLINE=1`을 설정했다. 기준 폴더에 pyc를 쓰지 않도록 `sys.dont_write_bytecode=True`. 외부 앱 상태 조회를 위해 격리 설정에만 개발자 모드를 켰다. 사용자 설정·문서 변경과 한/글 COM·문서 작업 시작은 수행하지 않았다.
- 변경 전에는 `record_tk`와 내장 왕복 JSON이 없다. 양쪽 공통 `FreezeTracer.begin_tk`를 상속해 횟수만 관찰하고 원래 구현을 그대로 호출했다. `_tk`와 콜백은 변경하지 않았다. 변경 후 내장 JSON 총 **152회**와 관찰 합계 **152회**가 일치한다. 이 중 2회는 별도 바쁨 재현·큐 비우기이며 아래 성능 표에서는 제외했다. 변경 전 총 705회도 같은 2회를 제외했다.

## 전후 수치

왕복 수는 각 50회 전체 합계. 시간은 호출자가 브리지 메서드에 진입해서 반환받을 때까지의 ms이며 최초 import·서비스 생성도 포함한다. 별도 예열 없음. 두 최종 실행은 순차 실행했다.

| 호출 | Tk 전 | Tk 후 | 중앙값 전 ms | 중앙값 후 ms | 최대 전 ms | 최대 후 ms |
|---|---:|---:|---:|---:|---:|---:|
| get_settings | 100 | 0 | 31.5714 | 2.2648 | 46.0333 | 22.1518 |
| get_pending_dialog | 50 | 0 | 15.9361 | 0.0011 | 16.5885 | 0.0104 |
| document_tool(clean) | 50 | 0 | 15.9764 | 0.0750 | 63.1953 | 26.2919 |
| file_rename(list) | 1 | 0 | 0.0048 | 0.0051 | 761.1935 | 31.8409 |
| timemachine(settings_get) | 1 | 0 | 0.0040 | 0.0070 | 402.0722 | 187.6549 |
| kclip_window(get) | 50 | 0 | 15.9584 | 0.2997 | 17.8444 | 2.9907 |
| external_app(status, photo_album) | 1 | 0 | 4.8730 | 3.2112 | 150.7954 | 144.7341 |
| table_sketch(take_drop) | 50 | 0 | 15.9469 | 0.0083 | 322.8559 | 58.8995 |
| table_units(arrows) | 50 | 0 | 39.0219 | 36.3600 | 190.8130 | 144.7480 |
| cancel_zero_click | 100 | 50 | 31.7465 | 15.8300 | 80.0294 | 22.3950 |
| start_zero_click(무효 token) | 100 | 50 | 31.7489 | 15.8452 | 50.7227 | 24.3661 |
| add_folder(미선택) | 50 | 0 | 15.9002 | 0.0026 | 21.8613 | 0.0185 |
| _initial_window_settings | 50 | 0 | 15.9257 | 1.7629 | 21.8977 | 2.8161 |
| get_state | 50 | 50 | 15.9545 | 15.8757 | 24.3569 | 17.2873 |

양쪽 700회 모두 Python 예외 0. `table_sketch(take_drop)`는 그림이 없는 정상 초기 상태의 `ok:false`, `add_folder`는 `FILE_SELECTION_REQUIRED`이며 실제 파일 선택·문서 추가는 없다. 유효한 서비스 조회의 `ok`는 true였다. zero_click 두 명령은 비어 있는 상태/유효하지 않은 token으로 실행해 문서 처리를 유발하지 않았다. `_initial_window_settings`는 시작 시 내부 helper이므로 화면 API 호출과 구분해야 한다.

## 18개 변경 지점 대응과 미실행

분류표의 변경 지점 18개 중 13개를 위 묶음으로 확인했다: cancel_zero_click, start_zero_click, get_settings, get_pending_dialog, document_tool의 설정 폴더 읽기, _file_rename_service, _timemachine_service, kclip_window, external_app의 저장소 읽기, table_sketch의 저장소 읽기, _table_units_command의 비COM 저장소 읽기, _cancel_before_edit(add_folder·get_settings를 통해), _initial_window_settings.

다음 5개는 확인하지 않았다:

- cancel_job의 job id 읽기: 작업 취소 동작은 읽기 전용 요청 묶음에서 제외.
- _start_legacy의 파일 목록 읽기와 hwp_bulk(start)의 running 읽기: 문서 처리 시작 금지.
- _table_units_command의 COM 구간 저장소 읽기(copy/thumbnails): COM 실행 금지.
- run_webview.initialize_webview의 시작 설정 조회: WebView 미실행.

따라서 18곳 전체 실기 확인 완료로 해석하지 않는다.

## 5초 시작 대기 재현

사설 Tk 큐에 Event 설정 후 6초 sleep하는 콜백을 넣고, 다른 스레드에서 `_tk(Event.set, timeout=None)`를 호출했다. 실제 대화상자나 문서 동작은 없다. 블로커가 끝난 뒤 후속 `_tk`로 큐를 비워 취소된 콜백이 뒤늦게 실행되는지도 확인했다.

| 항목 | 변경 전 | 변경 후 |
|---|---|---|
| 호출 결과 | 정상 반환 | TkBusyError |
| 응답 시간 | 6000.627ms | 5008.065ms |
| 대상 콜백 실행 | true | false(큐 비운 뒤에도) |

실제로 받은 예외 문구: **“앱이 다른 작업을 처리하고 있어 요청을 시작하지 못했습니다. 잠시 후 다시 시도해 주세요.”** `desktop_web_ui.py`의 TkBusyError 문자열과 일치한다. `flutter_ui/web/docfit_bridge.js`는 pywebview Promise의 거절을 그대로 넘기며 `bridge_web.dart` 역시 예외를 별도 문자열로 바꾸지 않는다. 화면의 최종 표시 문구는 호출부별 예외 처리에 달려 있으며 **실제 화면에서는 미확인**이다. 이 재현은 공통 `_tk` 시작 제한을 검증한 것이고 13개 대화상자 명령 각각을 실행한 결과가 아니다.

## 실행과 산출물

```powershell
& C:/HWP_AUTODOCFIT/v1803/exfile/.venv/Scripts/python.exe -X utf8 scripts/tk0_verify_pc.py --repo E:/docfit-tmp/tk0base --output E:/docfit-tmp/perf/tk0-pc --label baseline
& C:/HWP_AUTODOCFIT/v1803/exfile/.venv/Scripts/python.exe -X utf8 scripts/tk0_verify_pc.py --repo E:/docfit-tmp/tk0pc --output E:/docfit-tmp/perf/tk0-pc --label updated
```

`--label`에는 새 하위 폴더 이름을 사용한다. 각 실행은 exit code 0, worker_alive=false로 종료했다. 사설 호스트는 bridge._shutdown → tracer 중지/JSON flush → Tk 큐의 quit → worker join → root.destroy 순서로 정리했다.

- `E:/docfit-tmp/perf/tk0-pc/baseline/result.json`: 전 수치·응답 원문·50개 시간 표본·바쁨 재현.
- `E:/docfit-tmp/perf/tk0-pc/updated/result.json`: 후 수치와 같은 구조.
- 각 폴더 `observer-roundtrips.json`: 공통 관찰 집계(내장 계측 파일이 아님).
- `E:/docfit-tmp/perf/tk0-pc/updated/freeze-logs/tk-roundtrips-20261112-080958-038388.json`: 변경 후 내장 오류추적 152회.
- `E:/docfit-tmp/perf/tk0-pc/process-cleanup.json`: CIM으로 스크립트 명령행을 검색, 실행한 launcher/실제 Python 포함 **남은 프로세스 0**. WebView·COM·외부 앱 프로세스는 생성하지 않았다.

초기 탐색 측정은 전체 함수 profiler의 XML 부하와 표 목록 분석 시간을 확인하고 중단했다. `before`, `after`, `before-final` 폴더는 미완료 탐색 산출물이며 위 표에 사용하지 않았다. 해당 실행 프로세스만 명령행으로 특정해 강제 종료했다. 최종 묶음은 무거운 표 목록 대신 같은 비COM 설정 읽기를 거치는 `arrows`를 사용했다. baseline/updated는 정상 종료했다.

관련 단위시험: `python -X utf8 -m unittest tests.test_tk0_direct_reads tests.test_tk0_tracing tests.test_tk0_start_timeout` — **39개, 35.723초, OK**. 기존 시험·앱 기능 코드·본체 CRLF 수정 없음. 전체 unittest discover는 이번 작업에서 실행하지 않았다. 푸시·태그·GitLab 접근 없음. 검증 도구와 보고서만 로컬 커밋한다.
