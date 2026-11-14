# SM-ALL — 앱 전체의 암묵적 상태 기계 목록

- 기준: 2026-11-12, 브랜치 `docs/state-machine-inventory`(시작 커밋 `7ba67bcc`). **코드 변경 없음, 조사 문서 1개.**
- 선행 확인: `docs/TK0_INVENTORY_2026-11-12.md`는 읽었다. `docs/TEAM_OPERATING_RULES.md`는 **이 작업 폴더에 없고**, 다른 작업 폴더(hsa)의 같은 파일에는 1~8절만 있어 **0절(대원칙) 원문은 미확인**이다. 지시문에 인용된 첫 문장("Find all of the implicit state machines in my app")만 기준으로 삼았다.
- 색인·재색인(타임머신 본문 색인)은 `STATE_MACHINES_INDEX_2026-11-12.md`(hsa 폴더)에서 조사됐으므로 다시 조사하지 않았다. 형식은 그 문서를 따른다.
- 약어: L=`docfit_ui/legacy.py`, W=`desktop_web_ui.py`, J=`docfit_app/jobs.py`, P=`docfit_engine/pipeline.py`, C=`docfit_engine/com_driver.py`, H=`docfit_core/hwp_process.py`, R=`docfit_app/conversion_runner.py`, U=`docfit_app/update_service.py`, KC=`docfit_app/kagent_client.py`, KS=`docfit_app/kclip_service.py`, KM=`kagent/src/main.rs`, LB=`docfit_app/library_bridge.py`, LS=`docfit_app/library_scan.py`, KL=`klib/src/`, F=`flutter_ui/lib/main.dart`, N=`flutter_ui/lib/core/ipc/native_client.dart`. 줄 번호는 위 커밋의 실제 파일 기준이다.
- 상태 이름은 조사자가 붙였다. "코드상 가능"은 코드 읽기로만 판단한 것이고 **실제 발생 여부·빈도는 모두 미확인**이다(실기·한/글 COM·Flutter 실행 없음).
- 전체 결론: 명시적 상태 값이 있는 곳은 3곳뿐이다 — 작업 관리자 `state`(J:11), 업데이트 `step`(U:157), klib 스캔 `state`(KL/scan.rs:33). 나머지는 bool·Event·스레드 생존·파일 존재·Tk 문구의 조합이다. 그리고 **같은 사실을 두 곳이 따로 들고 있는 곳**(작업: `gui.running` ↔ J의 `state`)에서 어긋남이 생긴다.

## ① 문서 처리 작업의 수명

**(a) 상태와 판정 조건**

| 상태 | 지금 알아내는 조건 |
|---|---|
| 대기 | `gui.running=False`(L:271) 그리고 최근 작업이 종료 상태(J:11 `done/failed/cancelled`)이거나 없음 |
| 서식 분석 중(시작 불가) | `_서식분석중`(L:3126·3183). 작업 상태와 별개의 bool, 시작 시 거부(L:4658) |
| 접수(queued) | J:84. 기존 작업은 곧바로 started(L:4697), 변환은 스레드 안에서 started(J:93) |
| 실행 중 | `gui.running=True`(L:4698, W:1002·1332) **그리고** J `state='running'`(J:147) — 출처 2개 |
| 중단 요청 중 | `중단_event` set(L:4881) + J `cancel_requested`(J:203). `gui.running`은 그대로 True, 전용 필드 없음(문구 `status_var`만, L:4883) |
| 중단 유예 초과 | 10초 뒤 한/글 강제 종료. 타이머가 2개(L:4889→4894, J:206→211)이며 서로 모른다 |
| 중단됨·재개 가능 | `_재개_대기중/_재개_시작_인덱스/_재개_모드`(L:5593~5595). 목록·모드가 바뀌면 해제(L:2045·4691) |
| 완료 / 문서 오류 포함 완료 | `finished` 사건(P:855→L:5606), 문서별 실패는 `_실패목록`. J는 둘 다 `done` |
| 치명 오류 | `fatal_error`(P:860→L:5673), J `failed` |
| 종료 처리 중 | `closing=True`+`running=True`(L:5726), 10초 뒤 한/글 정리·15초 뒤 강제 종료(L:5745~5754) |
| 고아 작업 | 재시작 때 `ORPHANED_JOB`(J:53), 종료 알림 없이 끝남 `JOB_INTERRUPTED`(J:267~269) |

**(b) 전이**: 시작=`작업시작`(L:4654)·`_start_legacy`(W:1265)·단일 변환(W:979). 중단=`작업중단`(L:4875), 화면은 `cancel_job`(W:1244)·`stop`(W:1404). 종료 사건은 작업 스레드가 큐에 넣은 튜플을 Tk의 `queue_처리`가 읽어 `running=False`로 바꾸고, `legacy_event`(J:273)가 같은 튜플을 J의 상태로 옮긴다.

**(c) 일어나면 안 되는데 표현되는 조합**
1. **화면은 대기, 작업 관리자는 바쁨.** 종료 튜플(P:855)은 COM 정리(P:861~876)보다 먼저 나가고, 변환은 정리 뒤에야 종료 보고(R:38)를 한다. 그 사이 `can_start`는 `gui.running`만 본다(W:641). 바로 다시 시작하면 `JOB_RUNNING`(J:79)이 난다.
2. **중단 요청이 사라짐(코드상 가능).** `작업시작`이 `중단_event.clear()`(L:4703 부근)를 한 뒤 작업 스레드가 **다시** clear한다(P:663). 그 사이에 누른 중단은 지워지지만 J의 `cancel_requested`는 남아, 화면 문구는 "완료"인데 J는 `done`을 `cancelled`로 바꿔 적는다(J:149~150). Flutter는 J를 보고 취소로 표시한다(F:697~703).
3. **스레드는 죽었는데 `running=True`.** 작업 스레드는 `Exception`만 잡는다(P:858). 그 밖의 예외로 끝나면 종료 튜플이 없고, 회수 함수는 `running=False`일 때만 동작한다(J:267).
4. 종료 확인으로 중단할 때는 `중단_event`만 세우고 J에는 알리지 않는다(L:5726~5727). `closing`은 `get_state` 응답에서 찾지 못했다(W:568~600 확인 범위).

**(d)** UX: 중단을 눌렀는데 "완료"로 끝나거나, 끝난 직후 다시 시작이 거부된다. DX: "작업 중인가"의 답이 `gui.running`·J `state`·스레드 생존 3가지다. AX: `job.state`와 `running`이 다를 때 어느 쪽을 믿을지 규칙이 없다.

**(e) 가장 작은 변경**: `get_state` 응답에 **읽기 전용 `job_phase` 한 필드**를 덧붙인다(기존 값에서 계산: `idle/running/cancel_requested/closing/cleanup`). `cleanup`은 "`running=False`인데 작업 스레드 생존"이다. 기존 필드·동작은 그대로. 깨질 수 있는 시험: `test_legacy_adapter`, `test_browser_exposes_job_api`(tests/test_jobs.py), 상태 응답의 키 목록을 고정한 시험(이름 미확인).

## ② 한/글 COM 연결

**(a) 상태와 판정 조건**

| 상태 | 조건 |
|---|---|
| 연결 없음 | 전역 `hwp is None`(C:504·532) |
| 생성됨·모듈 미등록 | `hwp` 대입(C:271) 뒤 `RegisterModule` 전(C:275). 실패하면 예외(C:277)인데 `hwp`는 이미 살아 있다 |
| 연결 완료 | `한글_시작` 반환(C:288). 표시 여부·문서 열림은 별도 변수 없음 |
| 비교 보기 임베드 | `작업_hwp_hwnd`·`원본_hwp_hwnd`가 0이 아님(C:466·453) |
| 원본 창 실패→비교 보기 꺼짐 | 3회 실패 후 `비교보기_사용=False`(C:442~445) — 설정 값이 실행 중에 조용히 바뀐다 |
| 분리(창 보존) | `작업창_분리`: `hwp=None`이지만 한/글 프로세스는 남는다(C:500~505) |
| 닫힘 | `작업창_닫기`: `Quit` 후 None(C:522~533). `Quit` 실패는 무시 |
| 앱 소유 PID | `H._owned` 집합(H:7·31). `DispatchEx` 경로에서만 등록(C:226~236) |

**(b) 전이**: 작업 시작 시 `한글_시작`(P:749), 종료 시 `자동닫기_설정`에 따라 닫기/분리(P:866~871). 강제 종료는 `kill_owned`(H:46)를 중단 유예(L:4894, J:211), 앱 종료(L:5749), 강제 닫기(W:1594)가 부른다.

**(c) 문제 조합**
1. **보존한 한/글 창이 '앱 소유'로 남는다.** 분리해도 `_owned`에서 빼는 코드가 없다(`H.forget`을 부르는 곳을 찾지 못함). 나중 작업의 중단 유예·강제 종료 때 `kill_owned`가 **사용자가 보고 있는 이전 결과 창까지** 닫을 수 있다(코드상 가능, 실기 미확인). 저장 안 한 수정이 있으면 손실 위험.
2. **소유가 아닌 한/글.** `DispatchEx`가 실패해 `Dispatch`로 만든 경우(C:221)와 비교 보기 원본 창(C:433)은 등록되지 않아 강제 종료가 닿지 않는다. "응답 없는 한/글을 닫았어요" 문구(L:4896)는 죽인 것이 있을 때만 나온다.
3. `문서 열림` 상태는 변수로 없다. 실패 시 `Clear(1)`로 되돌리는 식(R:161·179)이라 "문서가 열려 있나"를 물을 방법이 없다.

**(d)** UX: 이전 결과 창이 갑자기 닫힐 수 있다. DX: 전역 `hwp` 하나로 생성·등록·표시를 구분 못 한다. AX: 한/글이 떠 있는지·누가 소유하는지 조회할 API가 없다.

**(e) 가장 작은 변경**: `H`에 **읽기 전용 `snapshot()`**(소유 PID 목록과 지금 살아 있는지)을 추가하고 오류추적·상태 응답에 싣는다. 소유 해제 규칙은 동작 변경이므로 별도 승인. 깨질 수 있는 시험: `test_only_owned_live_hwp_is_killed`, `test_dispatch_ex_registers_difference`(tests/test_force_stop.py).

## ③ 앱 시작·종료

**(a) 상태와 판정 조건**

| 상태 | 조건 |
|---|---|
| 스플래시 | `pyi_splash.is_alive()`(W:3871~3875). 페이지 loaded 또는 웹뷰 초기화 때 닫음(W:3879·3882) |
| 웹 창 있음·Tk 미준비 | `gui`가 `PendingGui`(W:81). 브리지 호출은 최대 30초 대기(W:87) |
| Tk 준비됨 | `bridge.gui = real`(W:3835) |
| Tk 실패 | `holder["오류"]`(W:89) → `tk_stopped.set()`(W:3839) → 웹 창 닫힘(W:3867). 사용자 안내는 호출한 쪽 몫(`hwp-auto-docfit.py`, **미확인**) |
| 화면 첫 상태 전 | Flutter `state.isEmpty`(F:485) |
| 닫기 보호 꺼짐/켜짐 | `_close_protocol`(W:1558) |
| 닫기 요청 대기(5초) | `_close_request`+`_close_timer`(W:3177~3195) |
| 닫기 확인 중(무기한) | `acknowledge_close`가 타이머를 끔(W:1566~1573). Flutter `appClosing=true`(F:2428) |
| 작업 중 종료 확인 | `gui.종료`의 대화상자(L:5718~5725) → ①의 종료 처리 중 |
| 강제 종료 | 닫기 두 번(W:3184~3187), 5초 무응답(W:1575~1584), `closing`에서 다시 닫기(L:5712~5716) |
| Tk 종료됨 | `tk_stopped` set(W:152), `user_closing`으로 웹 창을 누가 닫을지 구분(W:117·3862) |

**(b) 전이**: 창 닫기 이벤트→`request_close`, Flutter의 `resolve_dialog`(W:1604), 타이머, `root.destroy`를 감싼 `_shutdown`(W:3843).

**(c) 문제 조합**
1. **확인 응답 뒤에는 감시가 없다.** `acknowledge_close` 뒤 Flutter가 `resolve_dialog`를 끝내 보내지 못하면(저장 대화상자 대기 등) 요청이 남는다. 다시 닫기를 누르면 **강제 종료**로 간다(W:3184) — 저장 확인 중이던 내용이 버려질 수 있다.
2. Tk 미준비 30초 동안 화면은 "빈 상태"와 "연결 실패"를 구분하지 못한다(F:485·708). 시작 단계 값이 없다.
3. 웹뷰 초기화가 `gui.running`을 읽으며(W:3887~3888) Tk 준비를 기다릴 수 있다(예외는 삼킴).
4. 종료 관련 bool이 5개다: `closing`·`user_closing`·`_closed`·`_close_protocol`·`appClosing`.

**(d)** UX: 시작이 늦을 때 이유가 안 보이고, 닫기를 두 번 누르면 확인 없이 꺼진다. DX: 종료 경로 3개(정상·무응답·두 번 닫기)를 bool 조합으로 추적해야 한다. AX: "지금 종료 중인가·시작 중인가"를 물을 필드가 없다.

**(e) 가장 작은 변경**: 브리지에 **읽기 전용 `lifecycle` 필드**(`starting/ready/close_requested/close_confirming/closing/stopped`)를 기존 값에서 계산해 `get_state`·오류추적에 싣는다. 깨질 수 있는 시험: `test_polling_without_handling_close_does_not_disable_timeout`, `test_stale_acknowledgement_does_not_disable_timeout`(tests/test_desktop_close_watchdog.py).

## ④ 업데이트

**(a) 상태**: `step` 문자열이 명시돼 있다(U:157) — `idle → checking → available | latest | failed`, `available → downloading → installing`, 실패 `failed`, UAC 취소 `cancelled`(U:366). 바쁨 판정 `BUSY`(U:28)와 `_operation` 잠금(U:153·282~289)이 겹쳐 있다. 부가 상태: 주기 확인 시작 여부 `_periodic_started`, 이미 알린 버전 `_announced`(U:386~399), 내려받은 파일 캐시(검증 통과 시 재사용, U:306).

**(b) 전이**: 수동 확인(W:523), 6시간 주기 확인(U:400), `start_update`(U:334). 검증은 크기·EXE 머리·SHA-256(U:291~300), 설치형은 PowerShell 실행 뒤 `exit_app`(U:352~357, L:845~849), 포터블은 `portable_install`(L:843, 내부 **미확인**).

**(c) 문제 조합**
1. **`installing`에서 나오는 길이 재시작뿐이다.** 설치 실행 뒤 `exit_app`이 성공하면 앱이 꺼지지만, 포터블 설치가 True를 돌려주고 앱이 안 꺼지는 경우 `step`은 `installing`에 머문다. 그러면 확인·업데이트가 계속 거부된다(U:286, W:531). 포터블 경로의 재시작 보장은 미확인.
2. **알림이 조용히 사라진다.** 주기 확인이 새 버전을 찾으면 먼저 `_announced`에 넣고(U:398) 안내를 부르는데, 안내는 작업 중·종료 중이면 그냥 돌아간다(L:867). 같은 버전은 앱을 다시 켤 때까지 다시 알리지 않는다.
3. 주기 확인 실패도 `step='failed'`와 문구를 남긴다(U:259·262). 사용자가 누르지 않은 확인의 실패가 화면에 어떻게 보이는지는 미확인.
4. 내려받기 끝난 뒤 작업 중이면 `available`로 되돌린다(U:346~348) — 정상 설계. 다만 `installing` 설정 뒤에는 작업 시작을 막는 검사가 없다(`작업시작`은 `step`을 보지 않는다).

**(d)** UX: 작업 중에 나온 새 버전 안내를 놓친다. DX: `BUSY` 검사와 잠금이 이중이라 어느 쪽이 거부했는지 응답으로 구분되지 않는다(둘 다 False). AX: `step`은 읽기 쉽지만 "자동 확인이었는지 수동이었는지"가 없다.

**(e) 가장 작은 변경**: `state()`에 **`trigger`(`manual/periodic`)와 `announced` 여부**를 덧붙인다(기존 키 유지). 깨질 수 있는 시험: `test_busy_and_document_job_refuse_start`, `test_failed_refresh_retains_previous_release_and_notes`(tests/test_update_service.py).

## ⑤ 파일 변환 작업

**(a) 상태**: 전용 상태 값 없음. `run_conversion` 안의 지역 변수가 단계다 — 준비(COM 필요 판단·생성, R:112~126; `app`·`com_initialized`), 파일 순회(R:127), 합치기(pdfm, R:183~201), 종료 사건(R:202~206), 정리(R:209~219). 바깥에서는 ①의 `running`·J `state`로만 보인다. 별도로 **HWP 일괄 변환**은 자체 `phase`(`scan/convert`, W:2024)를 가진 다른 기계다.

**(b) 전이**: 시작은 W:979·W:1310 부근, 중단은 공용 `중단_event` 또는 J 취소(R:15~16). 종료 보고는 정리가 끝난 뒤로 미뤄진다(R:24~25·38).

**(c) 문제 조합**
1. **`done`인데 성공 0.** PDF 합치기에서 일부 실패하면 합본을 만들지 않고 오류를 '기록'으로만 내보낸 뒤(R:187~190, R:86) `finished`를 낸다(R:206) → J는 `done`.
2. **건너뜀이 실패로 센다.** 같은 형식 파일은 `document_error`로 보고하고 실패 수에 더한다(R:136~141). 결과 목록에는 `skipped`(R:80)인데 진행 사건은 `item_failed`(J:289)다 — 두 표현이 다르다.
3. ①-(c)1의 "화면은 대기, 관리자는 바쁨" 구간이 변환에서 가장 길다(한/글 `Quit`까지 기다림).
4. 일괄 변환은 시작 때 `gui.running`을 본다(W:2048). 반대 방향은 막지 않는 것으로 보인다: `_can_start`(W:663~664)와 `작업시작`(L:4654~4664)은 `_bulk_state`를 보지 않고, `get_state`는 값만 싣는다(W:659~660). 일괄 변환 내부(`docfit_core/hwp_bulk.py`)의 자체 보호는 **미확인**.

**(d)** UX: "완료"인데 결과 파일이 없다. DX: 실패/건너뜀/중단이 사건 이름만으로 구분되지 않는다. AX: `done`을 성공으로 읽으면 틀린다(결과의 success 수를 따로 봐야 함).

**(e) 가장 작은 변경**: 종료 사건 data에 **`outcome`(`ok/partial/nothing_saved`)과 `skipped` 수**를 덧붙인다(기존 `success/failed` 유지). 깨질 수 있는 시험: `test_conversion_job_runs_cleanup_before_terminal`(tests/test_jobs.py), `test_stop_during_last_file_still_reports_stopped`(tests/test_conversion_stop.py).

## ⑥ K에이전트 연결·몽땅클립보드

**(a) 상태와 판정 조건**

| 상태 | 조건 |
|---|---|
| 꺼짐 | `status.json` 없음(KC:43). 정상 종료 때 지운다(KM:983) |
| 켜짐 | 파일 있음 + `pid` 생존(KC:50~52) |
| 죽었는데 파일 남음 | 파일 있음 + `pid` 죽음. 비정상 종료 시 |
| 일시정지 | `paused`(KM:444), 시한부는 앱 쪽에서 `pause_until`을 덧씌움(KS:86~88·102~104) |
| 단축키 실패 | `hotkey_ok=false`(KM:448) |
| 낡은 버전 | 디스크 파일 다름 또는 실행 버전≠번들 버전(KS:192~193, `kagent/src/lifecycle.rs:2`) |
| 교체 중 | 끄기→교체→다시 켜기(KS:196~237), 실패 이유 `still_running`·`version_unavailable` |
| 앱 서비스 파이프 | 앱이 여는 역방향 파이프. 시작 실패는 기록만(W:3896~3900) |

**(b) 전이**: 에이전트가 변경마다 `write_status`(KM:429), 앱은 시작 때 `upgrade_if_stale`(W:3902~3912), 화면 명령 `enable/disable/status`(KS:125·239·82).

**(c) 문제 조합**
1. **'실행 중' 판정이 3가지다.** 앱은 pid 생존(KC:52), 에이전트의 한 경로는 pid 생존(KM:1307), 다른 경로는 **파일 존재만**(KM:1366) 본다. 죽은 파일이 남으면 답이 갈린다.
2. `request()`는 pid를 확인하지 않고 파일의 파이프 이름으로 바로 연결한다(KC:75~83). 죽은 파일이면 2초 뒤 "연결하지 못했습니다"만 나온다 — '꺼짐'과 '응답 없음'이 같은 문구.
3. 일시정지 판정 한 곳은 JSON을 풀지 않고 문자열 포함 검사다(KM:1389).
4. 앱 서비스 파이프가 안 열려도 화면·에이전트는 모른다. `hotkey_ok=false`를 화면이 보여 주는지는 **미확인**(Flutter kclip 화면 미열람).

**(d)** UX: 켜져 있다고 나오는데 붙여넣기가 안 된다. DX: 상태 판정이 Python·Rust에 중복 구현. AX: `status.json` 하나로 판단하면 죽은 에이전트를 살아 있다고 본다.

**(e) 가장 작은 변경**: `KS.status()` 응답에 **`link` 한 필드**(`off/running/stale_file/unreachable`)를 덧붙인다(파일·pid에서 계산, 기존 `running` 유지). 깨질 수 있는 시험: `test_status_exposes_store_and_optional_ffi_paths`, `test_current_agent_is_left_alone`(tests/test_kclip_service.py).

## ⑦ 자료함·klib 엔진

**(a) 상태와 판정 조건**

| 상태 | 조건 |
|---|---|
| klib 사용 불가 | 실행 파일 없음 또는 Windows 아님(LB:168~176) |
| klib 꺼짐(유휴 종료) | 프로세스 없음. 60초 무요청이면 스스로 종료(KL/main.rs:208~227) |
| klib 켜짐 | 파이프 연결 성공. **앱은 연결을 들고 있지 않고** 요청마다 새로 연다(LB:130) |
| 시작 중 | 연결 실패 → 한 번 실행 → 재시도(LB:150~160). 전용 표시 없음 |
| 스캔 | klib 메모리의 `state`(`idle/scanning/done/cancelled/error`)와 `phase`(`enumerate/reconcile`)(KL/scan.rs:33·93·192) |
| 스캔 담당 | 앱의 `_active`(`klib` 또는 Python 대체, LS:99) + Python 쪽 상태 사전(LS:116~133) |
| 색인 다시 만들기 | `reindex::is_running()`(KL/main.rs:221). 내부 상태는 **미확인** |

**(b) 전이**: 화면 명령 `box_scan`·`box_scan_status`·`box_scan_cancel`(LS:52~66), klib 감시 스레드의 유휴 종료, 연결 실패 시 Python 대체(LS:98~107).

**(c) 문제 조합**
1. **끝난 스캔 결과가 `idle`로 바뀐다.** 스캔 상태는 klib 프로세스 메모리에만 있다. 끝난 뒤 60초 조용하면 klib이 꺼지고, 다음 상태 조회는 새 프로세스를 띄워 `idle`을 받는다. 스캔 중 klib이 죽어도 같다 — 화면은 "스캔이 없었던 것"으로 본다(코드상 가능).
2. **조용한 대체.** klib에 닿지 못하면 알림 없이 Python 스캔으로 넘어간다(LS:98~107). 대체 스캔은 별도 DB(`scan_fallback.db`, LS:24)를 쓴다. 두 결과가 화면에서 합쳐지는지는 미확인.
3. `_active`가 `klib`인 채로 klib이 사라지면 상태 조회는 `error`를 준다(LS:147~152). '엔진 꺼짐'과 '스캔 실패'가 같은 값이다.

**(d)** UX: 스캔이 끝났는지 안 했는지 헷갈린다. DX: 상태가 프로세스 2개(앱·klib)와 구현 2개(Rust·Python)에 나뉜다. AX: `idle`만 보고 "아직 스캔 안 함"으로 판단해 다시 돌릴 수 있다.

**(e) 가장 작은 변경**: `LS.status()`가 **마지막으로 본 종료 상태를 앱 쪽에 기억**해 `last` 필드로 덧붙인다(klib 응답은 그대로 전달). 깨질 수 있는 시험: `test_uses_klib_when_available_and_polls_status_and_cancel`, `test_klib_error_is_returned_without_fallback`(tests/test_library_scan.py).

## ⑧ Flutter 화면의 큰 상태

**(a) 상태와 판정 조건**

| 상태 | 조건 |
|---|---|
| 탭(문서 선택/작업 방식/진행 과정/결과 확인) | `tab` 0~3(F:399·452) |
| 화면 잠금 | `locked = appClosing ‖ busy ‖ running ‖ state.isEmpty ‖ error != null`(F:484~485) — 이유 5개가 bool 하나 |
| 명령 처리 중 | `busy`(F:776~812). 이때 새 명령은 **말없이 버려진다**(F:773) |
| 조회 중 | `polling`(F:664). `busy`면 대화상자만 확인(F:654~662) |
| 연결 실패 | `error` 문구. `get_state` 실패 1회로 설정(F:708), 다음 성공으로 해제(F:704) |
| 작업 추적 | `activeJobId`·`jobCancelled`·`state['live']`(F:403~404). 1초 상태 조회(F:574)와 350ms 사건 조회(F:576) 두 길로 갱신 |
| 백엔드 대화상자 열림 | `backendDialogOpen`(F:449) |
| 종료 준비 | `appClosing`(F:450·2428) |
| 네이티브 백엔드 연결 | 프로세스 시작→준비 15초→소켓 5초→버전 확인(N:77~88), 호출 20초 제한(N:154), 끊김 `disconnected()` |

**(b) 전이**: 탭 자동 이동은 3곳 — 작업 끝(F:626), 파일 끌어 놓기(F:644), 시작 성공(F:834). 다시 연결은 네이티브일 때만 `restart_backend`(F:1197, `bridge_native.dart:7~11`), 웹뷰에서는 다시 조회뿐.

**(c) 문제 조합**
1. **한 번의 조회 실패로 전체 잠금.** `error`가 `locked`에 들어 있어 1초짜리 지연(TK0 문서의 `TkBusyError` 포함)에도 화면 전체가 잠겼다 풀린다.
2. **`live.running`과 `state.running`이 다를 수 있다.** 사건 길은 `live.running=false`로 만들고(F:755) 상태 길은 다음 1초 조회까지 `running=true`다. ①-(c)2의 "완료인데 취소"도 여기서 그대로 보인다.
3. 사건 조회 오류는 삼킨다(F:764). 사건이 끊겨도 표시가 없다.
4. `restart_backend` 뒤 백엔드가 새 프로세스면 진행 중 작업은 사라진다. 화면이 이를 어떻게 알리는지는 **미확인**(`docfit_app/application_service.py` 미열람).

**(d)** UX: 버튼이 왜 회색인지 알 수 없고, 누른 것이 무시된다. DX: 잠금 원인을 화면에서 재현하려면 5개 변수를 다 봐야 한다. AX: 화면 조작 에이전트가 "기다려야 하나, 다시 연결해야 하나"를 구분 못 한다.

**(e) 가장 작은 변경**: `locked` 옆에 **`lockReason` 게터 하나**(`closing/busy/running/loading/offline/none`)를 추가하고 표시는 그대로 둔다(잠금 조건 불변). 깨질 수 있는 시험: Flutter 시험 이름은 **미확인**(`flutter_ui/test`에서 잠금·재연결 시험을 찾지 못함).

## 전체 순위표

| 추천 순서 | 영역 | 사용자 영향(멈춤·오해·데이터 손실) | 고치기 쉬움 |
|---|---|---|---|
| 1 | ① 작업 수명 | **오해·멈춤**: 중단이 사라짐, 끝난 직후 재시작 거부, 죽은 작업이 '실행 중' | 쉬움(읽기 전용 필드 1개) |
| 2 | ② 한/글 COM | **데이터 손실 위험**: 보존한 결과 창이 나중에 강제 종료될 수 있음(실기 확인 필요) | 드러내기는 쉬움, 고치기는 보통(소유 규칙 결정 필요) |
| 3 | ③ 시작·종료 | **데이터 손실 위험·멈춤**: 닫기 두 번이면 확인 없이 강제 종료, 시작 지연 원인 안 보임 | 보통(종료 시험이 촘촘함) |
| 4 | ⑧ Flutter 화면 | **오해**: 잠금 이유 불명, 명령 무시 — 모든 영역의 증상이 여기로 모임 | 쉬움(게터 1개) |
| 5 | ⑤ 파일 변환 | **오해**: '완료'인데 결과 없음, 건너뜀이 실패로 표시 | 쉬움(사건 data 추가) |
| 6 | ⑦ 자료함·klib | **오해**: 끝난 스캔이 `idle`로 보임, 조용한 대체 | 보통(프로세스 2개) |
| 7 | ⑥ K에이전트 | **오해**: 죽은 에이전트를 켜짐으로 표시 | 쉬움(필드 1개) |
| 8 | ④ 업데이트 | **오해**: 안내 누락. 이미 `step`이 명시돼 가장 건강함 | 쉬움 |

- 순서 근거: 1·2는 서로 묶여 있다(①의 중단 유예가 ②의 `kill_owned`를 부른다). ④는 이미 명시적 상태가 있어 뒤로 뺐다.
- 공통 처방: 영역마다 **"기존 값에서 계산한 읽기 전용 상태 이름 1개"를 먼저 드러내고**, 동작을 바꾸는 수정(소유 해제·중단 clear 제거·잠금 조건 변경)은 그 이름을 오류추적에서 본 뒤 따로 승인받는다.

## 검증과 한계

- 코드·시험·사용자 문서는 고치지 않았다. 이 문서 1개만 추가했다.
- **읽지 않은 것(미확인)**: `hwp-auto-docfit.py`(시작 진입점·시작 실패 안내), `docfit_app/application_service.py`(네이티브 백엔드), `L._자동업데이트_설치`(포터블 교체·재시작), klib `reindex.rs`·`index_content.rs` 내부, kagent `pipe.rs`·`popup.rs`, Flutter의 kclip·library·progress 화면, P의 `문서_처리` 내부 단계.
- 줄 번호는 조사 시점 파일 기준이며, TK0 문서의 줄 번호(더 이전 커밋 기준)와 다르다.
- "깨질 수 있는 시험"은 이름만 확인했다. 각 시험이 무엇을 고정하는지는 읽지 않았다.
