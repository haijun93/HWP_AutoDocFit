# SM6 — 데이터 손실 위험 2건의 확인 (2026-11-12)

- 기준: 브랜치 `docs/state-machine-risks`(시작 커밋 `e84cd904`). 선행 문서 `docs/STATE_MACHINES_ALL_2026-11-12.md` ②·③절, `docs/TEAM_OPERATING_RULES.md` 0절.
- **SM6 시점에는 코드 변경 없음**(이 문서와 현재 동작 고정 시험 6건). **SM7(같은 날)에서 (가)·(나)를 고쳤다 → 9절.** 1~3절의 줄 번호·"현재 동작"은 SM6 시점 기준이다.
- 한/글 COM·실제 프로세스 종료는 실행하지 않았다. 판정은 코드 읽기 + 가짜 COM 생성기·가짜 `tasklist/taskkill` 시험이다.
- 약어(선행 문서와 같음): L=`docfit_ui/legacy.py`, W=`desktop_web_ui.py`, J=`docfit_app/jobs.py`, P=`docfit_engine/pipeline.py`, C=`docfit_engine/com_driver.py`, H=`docfit_core/hwp_process.py`, F=`flutter_ui/lib/main.dart`, B=`docfit_core/hwp_bulk.py`, U=`docfit_app/update_service.py`.

## 1. 판정 요약

| 위험 | 판정 | 사용자가 잃는 것 |
|---|---|---|
| (가) 보존한 결과 창이 나중에 강제 종료됨 | **실제로 일어난다**(코드·시험으로 확정, 실기 미확인) | 결과 창에서 **작업 뒤에 사용자가 고친 저장 안 된 내용**과 열린 창. 결과 문서 파일 자체는 남는다 |
| (나) 닫기 두 번 → 확인 없이 강제 종료 | **실제로 일어난다**(설계상 의도된 탈출구이나, 확인 대화상자가 떠 있는 동안에도 동작) | 확인 대화상자가 지키려던 **Flutter 편집기의 저장 안 된 편집**, 진행 중 작업(확인 없이 중단), (가)와 겹치면 보존한 결과 창의 미저장 편집 |

## 2. (가) 보존한 결과 창의 강제 종료

**사용자 행동 순서**
1. 설정에서 "작업 끝나면 한/글 닫기"(자동닫기)를 끈다.
2. 작업 1 실행 → 결과 한/글 창이 남는다. 사용자가 그 창에서 문서를 고친다(아직 저장 안 함).
3. 같은 앱 실행 중에 작업 2를 시작한다.
4. 작업 2를 "중단"했는데 10초 안에 멈추지 않는다 — 또는 작업 2 중에 앱을 닫아 10초가 지난다 — 또는 (나)의 강제 종료.
5. 작업 1의 결과 창까지 강제 종료(`taskkill /F /T`)된다.

**코드 경로와 근거**
- 등록: `DispatchEx` 생성 전후 `tasklist` 차이를 소유로 등록(C:225~236). 작업 1의 PID가 `H._owned`에 들어간다.
- 보존: 자동닫기 끔이면 작업 끝에 `작업창_분리`(P:868~870)만 부른다. `작업창_분리`(C:500~505)는 `hwp=None`만 하고 **`H.forget`(H:41)을 부르지 않는다.** 저장소 전체에서 `forget` 호출은 시험(tests/test_force_stop.py:16)뿐이다.
- 정리: `kill_owned`(H:46~64)는 등록된 PID 중 **아직 살아 있는 Hwp.exe 전부**를 죽인다. 작업 구분이 없다.
- 부르는 곳: 중단 유예 10초(L:4887·4889~4896), 작업 중 종료 10초(L:5749~5751)와 15초 강제 종료(L:5752·5759~5763), 작업 관리자 중단 시한(J:213~224), 브리지 강제 종료(W:1586~1594), CLI 시한(docfit_app/cli.py:124·221).
- 막는 코드: **작업이 없을 때** 정상 종료는 `kill_owned`를 부르지 않고 창을 분리만 한다(L:5734~5735). 따라서 위험은 "보존 창이 있는 상태에서 다음 작업이 멈추지 않을 때"로 한정된다. 앱을 끝냈다 다시 켜면 `_owned`(프로세스 메모리)가 비므로 이전 실행의 창은 안전하다.
- 같은 결함의 변형: `작업창_닫기`(C:522~533)도 `forget`을 안 하지만, `Quit`가 성공하면 다음 `kill_owned`가 죽은 PID를 목록에서 지운다(H:53~55). `Quit` 실패로 창이 남으면 (가)와 같아진다. 끝난 PID 번호를 Windows가 사용자의 다른 Hwp.exe에 다시 주면 그 창도 대상이 된다(드묾, 코드상 가능).

**잃는 것(정확히)**
- 결과 문서 파일: **남는다.** 결과는 작업 중 `SaveAs`로 이미 디스크에 저장된다(P:146·476, docfit_engine/formatter_text.py:705·719). 강제 종료는 파일을 지우지 않는다.
- **작업 뒤 사용자가 결과 창에서 고친 내용 중 저장하지 않은 부분**: 잃는다(`/F`라 한/글의 "저장할까요?"가 뜨지 않음).
- 사용자가 그 창에서 저장(Ctrl+S) 중에 죽으면 파일이 깨질 수 있다 — **코드만으로는 판단 불가**(한/글 저장 방식에 달림, 4절 실기).
- 한/글 자동 복구 파일로 되살릴 수 있는지: 판단 불가(4절).

**고정한 시험**(`PreservedResultWindowTests`)
- `test_detach_currently_keeps_preserved_window_owned`: 분리 뒤에도 PID 101이 소유로 남음.
- `test_stop_grace_of_next_job_currently_kills_preserved_result_window`: 작업 1 창(101) 보존 → 사용자 한/글(555) → 작업 2(202) → 중단 유예 → 101·202가 죽고 555는 산다.
- `test_app_close_deadline_currently_kills_preserved_result_window`: 작업 중 종료 10초 경과 시 같은 결과.
- `test_preserved_window_without_new_job_is_not_killed_by_idle_close`: 막는 경로(작업 없을 때 정상 종료) 확인.

## 3. (나) 닫기 두 번 → 확인 없이 강제 종료

**사용자 행동 순서**
1. 닫기 보호가 켜진 Flutter 화면에서 창 닫기(X)를 누른다 → `request_close`가 요청을 만들고 5초 타이머를 건다(W:3189~3194).
2. Flutter가 요청을 받아 `acknowledge_close`를 보내고(F:2431, 타이머 꺼짐 W:1572) 편집기의 "편집 내용을 버릴까요?" 같은 확인을 띄운다(F:2432 → 예: features/formats/profile_editor.dart:116, diagrams/diagram_editor_screen_native.dart:47).
3. 사용자가 답하기 전에(또는 반응이 늦어 보여서) X를 한 번 더 누른다.
4. 남아 있던 요청을 보고 곧바로 `force=True`(W:3184~3187) → `_force_close`(W:1586~1601): `_shutdown`, `kill_owned`, `root.destroy`.

**근거**: 2번째 `request_close`는 요청이 "확인 중(acknowledged)"인지 "아직 전달 전"인지 구분하지 않는다(W:3184). `gui.종료`(L:5713~5735)의 "작업 중단하고 종료할까요?"는 거치지 않는다(`resolve_dialog`→`finish_close`(W:1615~1619) 경로가 아님). 막는 코드: 사용자가 "취소"로 먼저 답하면 요청이 지워져(W:1610) 다음 닫기는 새 확인이 된다. 두 번 누르기 자체는 기존 시험 `test_second_native_close_releases_requests_and_cleans_up_without_tk_wait`가 의도로 고정한 동작이다.

**잃는 것(정확히)**
- Flutter 편집기(서식 프로필·설정·흐름도/그림 편집기)의 **저장 안 된 편집**: 잃는다. 확인 대화상자가 바로 그것을 지키려고 떠 있던 상태다.
- 진행 중 문서 작업: 확인 없이 끊긴다. 작업 원본은 사본으로 처리하므로 원본은 안전, **작업 중이던 결과 파일은 미완성/없음**(저장 시점에 따라 다름 — 판단 불가).
- (가)와 겹치면: 보존한 결과 창의 미저장 편집도 잃는다(시험에서 101 종료 확인).

**고정한 시험**(`DoubleCloseTests`)
- `test_second_close_during_acknowledged_confirmation_currently_forces_exit`: 확인 응답 뒤 2번째 닫기 → `gui.종료` 미호출, `root.destroy` 1회, 보존 창 101 강제 종료.
- `test_close_after_user_cancel_asks_again`: 막는 경로(취소 뒤 닫기는 다시 확인) 확인.

## 4. 고치는 방법(제안만, 구현 안 함)

### (가)
| 선택지 | UX | DX | AX | 깨질 수 있는 시험 | 되돌리기 |
|---|---|---|---|---|---|
| A. `작업창_분리`·`원본_뷰어_분리`에서 그 창의 PID를 `forget`(창 핸들 → `win32process.GetWindowThreadProcessId`로 PID) | 보존 창이 절대 강제 종료되지 않음. 대신 그 창이 멈춰도 앱이 못 닫음(사용자가 직접 닫음) | 규칙 1줄: "사용자에게 넘긴 창은 앱 소유 아님" | `snapshot()`과 함께 쓰면 소유 목록이 사실과 맞음 | SM6 시험 3건(기대값 반전, 의도된 변경), `test_run_context`의 분리 대역은 영향 없음 | 쉬움(함수 2곳) |
| B. 소유를 "작업 ID별"로 등록하고 `kill_owned(job_id)`가 그 작업 PID만 죽임 | A와 같고, 여러 작업이 겹쳐도 정확 | 호출부 7곳 인자 변경 — 큼 | 로그에 작업별 PID가 남아 추적 쉬움 | `test_force_stop`·`test_jobs`·`test_cli`·`test_desktop_close_watchdog`의 `kill_owned` 호출 형태 단언 | 중간 |
| C. 강제 종료 전에 보존 창 PID만 따로 묻기("이전 결과 창도 닫을까요?") | 선택권은 생기나 응답 없는 상황에 대화상자가 늘어남 | 상태 하나 더(확인 대기) | 판단 지점 증가 | 종료 시한 시험(`test_close_deadlines`) | 중간 |

권고: **A**(가장 작고, 세 경험 모두 올리며 기존 시험을 바꾸지 않음). 창 핸들이 0인 경우(비교 보기 꺼짐)는 `hwp`가 살아 있을 때 PID를 얻는 방법이 필요 — 실기 확인 필요(5절 ③).

### (나)
| 선택지 | UX | DX | AX | 깨질 수 있는 시험 | 되돌리기 |
|---|---|---|---|---|---|
| A. 확인 중(acknowledged) 요청이 있으면 2번째 닫기는 강제 종료 대신 Flutter에 "확인 창에 답해 주세요"를 다시 띄움(요청에 `acknowledged` 표시) | 미저장 편집 보호. 진짜로 멈췄을 때 탈출은 작업관리자뿐 | `_close_request`에 상태 1개 추가 | 선행 문서의 `lifecycle=close_confirming`과 맞음 | SM6 `test_second_close_during_acknowledged…`(의도된 반전). 기존 `test_second_native_close…`는 확인 전 경로라 유지 | 쉬움 |
| B. A + 확인 중 2번째 닫기에 OS 수준 "강제로 끌까요?" 질문(L:5715 문구 재사용) | 탈출구 유지 + 실수 방지 | Tk/네이티브 질문이 웹 창 스레드에서 떠야 함 — 구현 까다로움 | 같음 | 위와 같음 | 중간 |
| C. 확인 중에는 30초 같은 긴 시한 후에만 2번째 닫기를 강제로 인정 | 실수 방지 대부분, 탈출구 유지 | 타이머 하나 더 | 시한 값이 상태에 드러나야 함 | 위와 같음 | 쉬움 |

권고: **B가 이상적, 우선 C**(작고 탈출구를 잃지 않음).

## 5. 총괄용 실기 확인 절차 — SM6(수정 전) 재현용. 수정 뒤 확인은 9.4절을 따른다
(Windows + 한/글 2020, `C:\HWP_AUTODOCFIT\.com-test.lock` 잡고 진행)

① (가) 재현
1. 설정에서 자동닫기를 끈다. 사본 문서 A로 작업 1 실행 → 끝나면 결과 한/글 창이 남는지 본다.
2. 그 창에 "SM6-테스트" 글자를 입력하고 **저장하지 않는다.**
3. 사용자가 직접 연 한/글(앱 밖에서 문서 B 열기)을 하나 띄워 둔다.
4. 큰 사본 문서로 작업 2를 시작하고 곧 "중단"을 누른다. 10초 안에 멈추면 재현 실패(정상) — 여러 번 시도하거나 작업 2 도중 창 닫기→"작업 중단하고 종료" 후 10초 기다린다.
- 볼 것: 상태줄 "한/글이 응답하지 않아 앱이 띄운 한/글을 강제로 종료했어요."와 결과 창(A) 사라짐 여부, B 창 생존.
- 합격 기준(현재 동작 확인): A 창이 사라지면 (가) **실기 확정**. B는 반드시 살아 있어야 한다(죽으면 별도 중대 결함).
- 추가 확인: 한/글을 다시 켰을 때 "SM6-테스트" 자동 복구 제안이 뜨는지(손실 범위 판정용).

② (나) 재현
1. 서식 프로필 편집기를 열고 값을 하나 바꾼다(저장 안 함).
2. 창 X → "편집 내용을 버릴까요?"가 뜨면 답하지 말고 X를 한 번 더 누른다.
- 합격 기준(현재 동작 확인): 앱이 즉시 꺼지고 프로필 변경이 남지 않으면 (나) **실기 확정**. 같은 시험을 작업 실행 중에 하면 "작업 중단하고 종료할까요?" 없이 꺼지는지 본다.

③ 고치기 전 사전 확인(4절 (가)-A용)
- 비교 보기를 끈 상태에서도 결과 창의 PID를 얻을 수 있는지: 작업 끝 직전 `hwp.XHwpWindows.Item(0)`의 창 핸들을 얻을 수 있는지 로그로 확인.

## 6. 추가 조사(데이터 손실·멈춤 관련만)

| 대상 | 행동 순서 | 판정 | 근거 |
|---|---|---|---|
| B 일괄 HWPX 변환: 원본 보호 | 폴더 선택→변환 | **일어나지 않는다** | 원본은 `Clear(1)`로 저장 없이 닫음(B:309), 결과는 임시 파일→`os.link`(B:292·295, 기존 파일 있으면 실패), 기존 HWPX는 건너뜀(B:258~259) |
| B 일괄 변환: 중단이 안 들음 | 변환 중 "중단" | **일어난다(멈춤)** | `stop`은 파일 사이에서만 봄(B:367). `Open`/`SaveAs`가 대화상자에 막히면(B:270·292) 끝나지 않는다. 이 한/글은 앱 소유로 등록되지만(W:2002 → C:225~236) 일괄 변환 중단 경로에는 시한·`kill_owned` 호출이 없다(앱 강제 종료 때만 정리). 실제로 막히는지는 실기 필요 |
| B 일괄 변환: 시작 실패 | 한/글 없음 | **일어나지 않는다** | 예외 시 `phase="error"`(W:2075~2077) |
| U 포터블 교체 | 업데이트 클릭 | **일어나지 않는다(작업 중)** | 내려받은 뒤 `running()`이면 설치 안 함(U:346~348) |
| U/L 업데이트 후 종료 | 편집기에 미저장 편집이 있는 채 업데이트 | **코드만으로는 판단 불가** | 설치 뒤 `root.destroy`를 바로 부름(L:947~948, legacy.py:846~848) — Flutter `CloseGuards`(F:2432)를 거치지 않는다. 업데이트 단추가 편집기와 동시에 눌릴 수 있는 화면 구성인지 실기 확인 필요. 교체 스크립트(.old 되돌리기)는 PowerShell 본문을 읽지 않아 미확인 |
| `docfit_app/application_service.py` | — | 해당 없음 | 상태 계산(`running`은 J 상태에서 파생, 105줄)뿐, 파일 삭제·교체·대기 없음 |

## 7. 시험 실행 결과

`python -X utf8 -m unittest tests.test_sm6_data_loss_risks tests.test_force_stop tests.test_desktop_close_watchdog tests.test_jobs` → **Ran 46 tests, OK**(새 시험 6건 포함). 기존 시험은 고치지 않았다.

## 8. 남은 것

- 실기 미확인: (가)·(나) 실제 재현, 한/글 자동 복구 범위, 저장 중 강제 종료 시 파일 손상, 일괄 변환 대화상자 막힘.
- 시험은 **현재 동작(위험 있음)을 고정**한다. 4절 수정 시 `currently_` 시험 3건의 기대값을 반전해야 하며 이는 의도된 변경이다. → SM7에서 반전함(9.3절).

## 9. SM7 수정 (2026-11-12, 사용자 승인)

### 9.1 (가) 넘겨준 결과 창 보호 — 4절 (가)-A 변형
- 상태(PID 하나): `owned`(앱 소유, 강제 정리 대상) → 작업창_분리(사용자에게 넘김) → **목록에서 빠짐**. `owned` → `kill_owned`/죽은 PID 발견 → 목록에서 빠짐. 넘긴 PID가 다시 `owned`가 되는 전이는 없다.
- 구현: 창 핸들(hwnd)은 비교 보기를 끄면 0이라 쓰지 않고(5절 ③ 실기 확인이 필요 없어짐), **생성 시점의 PID를 작업 COM 객체에 묶는다.**
  - `docfit_core/hwp_process.py`: `register_owned`가 이 스레드의 마지막 등록을 기억 → `take_registered()`로 꺼냄. `bind_owner(app, pids)`/`unbind_owner(app)`/`hand_over(app)`(묶인 PID를 소유에서 뺌).
  - `docfit_engine/com_driver.py` `한글_시작`: 생성 전 `take_registered()`로 비우고, 생성 직후 `bind_owner(hwp, take_registered())`. 같은 스레드의 다른 생성 등록이 섞이지 않는다(시험 있음).
  - `작업창_분리`: `hand_over(hwp)` → 넘긴 창의 PID가 소유에서 빠짐. `작업창_닫기`(자동닫기 켬): `unbind_owner`만 — PID는 소유로 남아 `Quit` 실패로 남은 창은 계속 정리 대상.
- 바뀌지 않은 것: 아직 넘기지 않은 작업 중 한/글은 그대로 정리 대상(중단 유예 10초·종료 10/15초·작업 관리자·브리지·CLI 시한 모두). `원본_뷰어`(비교 보기 왼쪽)는 `Dispatch`로 만들어 원래 소유 등록이 없다 → 변화 없음.
- 넘긴 창을 앱이 다시 쓰는 경로: 없음. 분리 뒤 `hwp=None`·`작업_hwp_hwnd=None`이라 다음 작업은 새 한/글을 만들고, `비교보기_임베드_재확인`은 `None` 핸들을 건너뛴다. 작업 시작 시의 `작업창_분리`(P:738~739)는 `hwp=None`(격리된 작업 상태)이라 아무것도 풀지 않는다.
- 남는 한계: 넘긴 창이 멈춰도 앱은 더 이상 닫지 않는다(사용자가 직접 닫음). 앱이 이전 실행에서 넘긴 창은 원래 안전했다.

### 9.2 (나) 닫기 연타 보호 — 4절 (나)-C
- 닫기 요청 상태: 없음 → `delivering`(1번째 닫기, 5초 감시) → `confirming`(`acknowledge_close`, Flutter가 확인 창을 띄움, `confirming_since` 기록) → 없음(`resolve_dialog` 답 / 강제 종료).
- `request_close`: `confirming`이고 **30초(`CLOSE_CONFIRM_FORCE_AFTER`) 전**이면 강제 종료하지 않고 `False`(닫기 막음)를 돌려주며 확인 창 요청을 그대로 둔다(`repeat_closes` 수를 셈, 로그 1줄). 30초가 지난 뒤의 닫기, 또는 `delivering`(전달 전) 중의 2번째 닫기는 **기존처럼 즉시 강제 종료**.
- 30초 근거(`desktop_web_ui.py` 상수 주석): 확인 창은 Flutter가 살아 있음을 증명한 뒤에만 뜬다. 문구를 읽는 중의 연타를 보호하면서 확인 응답 뒤 화면이 멈춘 경우 탈출도 허용하는 절충값이며, 사용자 응답 시간의 실측값은 아니다. 승인 뒤 기존 종료 시한(10초 한/글 정리·15초 강제 종료)보다 길다. 중복 `acknowledge_close`는 시작 시각을 갱신하지 않는다.
- 그대로인 경로: 화면 무응답(5초 안에 `acknowledge` 없음)의 자동 강제 종료(`_close_unanswered`), `scripts/smoke_webview_content.py --unresponsive`(JS 정지 → 1번 닫기 → 5초 감시)와 기존 시험 `test_second_native_close_releases_requests_and_cleans_up_without_tk_wait`(전달 전 2번째 닫기).

### 9.3 시험(`tests/test_sm6_data_loss_risks.py`, 6건 → 13건) 전후 기대
| 시험 | SM6 기대 | SM7 기대 |
|---|---|---|
| `test_detach_releases_preserved_window_from_ownership`(구 `…currently_keeps…`) | 분리 뒤 `{101}` | 분리 뒤 `set()` |
| `test_stop_grace_of_next_job_spares_preserved_result_window`(구 `…currently_kills…`) | 죽음 `[101, 202]` | `[202]`, 101·555 생존 |
| `test_app_close_deadline_spares_preserved_result_window`(구 `…currently_kills…`) | `[101, 202]` | `[202]`, 101 생존 |
| `test_second_close_during_acknowledged_confirmation_does_not_force_exit`(구 `…currently_forces_exit`) | 2번째 닫기 `True`·destroy·101 종료 | 29.9초 안 2회 모두 `False`, destroy·종료 없음, 이후 취소 답 유효 |
| 새: `…auto_close_keeps_window_owned_when_quit_fails`, `…working_window_not_yet_handed_over_stays_owned`, `…stale_registration…`, `…after_confirmation_deadline_still_forces_exit`(30초), `…before_delivery_still_forces_exit` | — | 기존 정리·탈출 경로 유지 |
| `test_preserved_window_without_new_job…`, `test_close_after_user_cancel_asks_again` | 그대로 | 그대로 |

시험의 `_create`는 `한글_COM_인스턴스_생성` 직접 호출 대신 실제 작업 경로인 `한글_시작`을 가짜 COM으로 부른다.

### 9.4 총괄 실기 확인 절차(수정 뒤, Windows + 한/글 2020, `C:\HWP_AUTODOCFIT\.com-test.lock` 잡고)
① (가) 보존 창 보호
1. 설정에서 "작업 끝나면 한/글 닫기"를 끈다. 사본 문서 A로 작업 1 → 결과 한/글 창이 남는지 본다.
2. 그 창에 "SM7-테스트"를 입력하고 저장하지 않는다. 앱 밖에서 한/글로 문서 B를 하나 연다.
3. 큰 사본 문서로 작업 2를 시작하고 곧 "중단"(10초 안에 멈추면 작업 2 도중 창 닫기 → "작업 중단하고 종료" 후 15초 기다림).
- 볼 것: 상태줄/로그 "앱이 띄운 한/글 강제 종료: [...]"에 나온 PID 수, 결과 창 A·문서 B 생존.
- 합격: **A 창이 "SM7-테스트"를 가진 채 살아 있고** B도 살아 있다. 작업 2의 한/글만 닫힌다(10초 안에 멈췄으면 강제 종료 자체가 없으므로 A·B 생존만 확인).
- 추가: 자동닫기를 **켠** 상태로 같은 순서를 하면 작업 1 창은 작업 끝에 닫혀야 한다(기존 동작).
- 비교 보기 켬/끔을 각각 확인한다. 켬에서는 작업 1 종료 후 넘긴 결과 창이 앱으로 다시 붙지 않는지, 작업 2를 시작하면 새 작업 창과 원본 비교 창이 정상 표시되는지 본다. 합격: 작업 1 창의 미저장 글자가 유지되고 작업 2 비교 보기가 정상 동작한다.

② (나) 닫기 연타
1. 서식 프로필 편집기에서 값을 하나 바꾼다(저장 안 함). 창 X → "편집 내용을 버릴까요?"가 뜨면 답하지 말고 X를 2~3번 더 누른다(30초 안).
- 합격: 앱이 꺼지지 않고 확인 창이 그대로 있다. "취소"를 누르면 편집 내용이 남아 있다. 다시 X → 확인 창이 새로 뜬다.
2. 같은 확인 창을 띄운 채 **30초 넘게** 기다린 뒤 X를 누른다.
- 합격: 앱이 바로 꺼진다(멈춤 탈출구). 앱이 띄운 작업 중 한/글이 있으면 함께 닫히고, ①에서 넘긴 결과 창은 남는다.
3. 회귀: 화면 무응답 강제 종료는 CI의 `smoke_webview_content.py --unresponsive`가 확인(창이 12초 안에 닫혀야 함).

### 9.5 남은 것
- 실기 미확인: 9.4 ①②. 특히 `DispatchEx` 전후 `tasklist` 차이가 작업 한/글 PID를 정확히 잡는지(다른 한/글이 같은 순간 뜨면 섞일 수 있음 — 원래 등록 방식의 한계, 이번에 바뀌지 않음).
- (나)-B(확인 중 2번째 닫기에 "강제로 끌까요?" 질문)와 Flutter 쪽 "확인 창에 답해 주세요" 안내는 하지 않았다. 30초 전 연타는 아무 반응 없이 무시된다(확인 창은 이미 화면에 있음).
- 30초 뒤 닫기는 확인 없이 강제 종료된다 — 사용자가 확인 창을 30초 넘게 보고 있다가 X를 누르면 미저장 편집을 잃는다(의도된 탈출구).
- 6절 일괄 변환 중단 멈춤, 업데이트 후 종료는 이번 범위 밖.

### 9.6 이어받기 검증 결과 (Codex, 2026-11-12)
- (가) 커밋 `a9261405` 검토 완료: 생성 직후 PID 묶음, 분리 시 소유 해제, 미분리 작업 및 Quit 실패 창의 정리 유지 확인. 비교 보기 재임베드는 분리한 작업 창 핸들을 건너뛰고 원본 창만 다시 붙이는 가짜 COM 시험 추가.
- (나) 수정 전 `HEAD`의 `request_close` 메서드를 메모리에서 복원하여 확인 응답 직후 두 번째 닫기로 `True`, `root.destroy` 1회, 가짜 PID 101 종료를 재현했다. 디스크의 소스나 실제 한/글 프로세스는 바꾸지 않았다.
- 최종 14개 모듈 **152건 통과**, 추가 호출부 회귀 3개 모듈 **37건 통과**, 작업 생명주기·IPC·일괄 변환 5개 모듈 **40건 통과**: 합계 **22개 모듈 229건, 실패 0, 제외 0**. SM7 시험은 13건 포함. 기존 시험 파일 중 수정은 허용된 `tests/test_sm6_data_loss_risks.py`뿐이다.
- 152건 명령: `python -X utf8 -m unittest tests.test_compare_versions_pc tests.test_desktop_browser_api tests.test_desktop_close_watchdog tests.test_desktop_file_drop tests.test_desktop_request_lifecycle tests.test_force_stop tests.test_hwp_compat tests.test_jobs tests.test_pdf_compress tests.test_run_context tests.test_sm6_data_loss_risks tests.test_v2_canvas_commands tests.test_visual_compare tests.test_webview_rendering_guard`
- 37건 명령: `python -X utf8 -m unittest tests.test_cli tests.test_conversion_stop tests.test_engine_extraction`
- 40건 명령: `python -X utf8 -m unittest tests.test_sm3_job_lifecycle tests.test_ipc_job_file_rename tests.test_ipc_job_library_index tests.test_hwp_bulk tests.test_hwp_bulk_bridge`
- 실행 중 subprocess 출력 읽기 스레드에서 `UnicodeDecodeError` 1회가 출력되었다. unittest 결과는 OK이지만 이 출력 디코딩 문제는 미해결로 남긴다. WebView 재시도 상한 경고 2회는 해당 시험에서 출력되었다.
- 실제 COM 실행, EXE 빌드 및 `smoke_webview_content.py --unresponsive` 실기 실행은 하지 않았다. 기존 5초 미응답 감시·전달 전 재닫기 시험은 통과했고 `.github`/smoke 스크립트는 수정하지 않았다. 실제 UI/COM의 합격은 총괄이 9.4절로 확인해야 한다.
- 인수 시부터 있던 `tests/fixtures/timemachine_extract/sample.*` 변경 7개는 수정·커밋하지 않는다.

### 9.7 릴리스 노트용 문장 (초안)
1. 사용자에게 넘긴 결과 한/글 창은 이후 작업의 강제 정리에서 제외하여 저장하지 않은 편집 내용을 보호합니다.
2. 닫기 확인 중 30초 이내의 닫기 연타를 막고, 30초 경과 후 다시 닫으면 강제 종료할 수 있도록 했습니다.
