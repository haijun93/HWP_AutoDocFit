# SM4 작업 수명 수정 (2026-11-12)

범위: E:/docfit-tmp/sm, docs/state-machine-inventory. 로컬 커밋만 수행한다.
HANDOVER.md, SM3 보고서, 이 폴더에 없는 운영 규칙은 mainfix/docs/TEAM_OPERATING_RULES.md 0절 사본을 읽었다. 다른 작업 폴더는 수정하지 않았다.

## 상태 전이와 구현

- 종료 알림 없이 실제로 종료한 worker: running → failed(JOB_INTERRUPTED, 작업 종료 알림 없이 실행이 끝났습니다.). Tk 큐가 비었을 때 회수하고 running을 해제하며 버튼과 오류 상태/로그를 갱신한다.
- terminal 사건 뒤 실제 스레드 종료: 남아 있는 GUI running을 해제한다. 스레드가 살아 있으면 JOB_RUNNING 거부를 유지한다. attach 후 start 전(ident 없음)도 회수하지 않는다.
- 큐가 비었다는 최초 확인과 스레드 종료 사이에 정상 종료 알림이 들어올 수 있으므로, 실제 종료를 확인한 뒤 pending 콜백으로 큐를 재확인한다. 대기 중인 정상 종료 사건을 실패로 덮어쓰지 않는다.
- stop Event 초기화는 JobManager.begin의 실행 가능 검사 뒤, 새 작업 등록 전에 한 번만 한다. UI와 pipeline의 뒤늦은 clear를 제거했다. 거부된 시작은 기존 Event를 지우지 않는다. 저장/문서 단계의 협조적 중단 확인 위치는 변경하지 않았다.
- job_phase 값 목록은 변경하지 않았다. dead는 큐 회수 전 순간 관측 값으로 남으며 회수 후 failed가 된다. 정상 종료 회수 후 finished, 살아 있는 정리 중 finishing을 시험한다.

## 수정 전 재현 → 수정 후 기대

| 시험 | 수정 전 | 수정 후 |
|---|---|---|
| LifecycleSymptomsTests.test_dead_worker_is_failed_and_unlocks_gui | running=True여서 실패 | failed, 오류 사유, running=False, phase=failed |
| LifecycleSymptomsTests.test_finished_dead_worker_allows_restart_despite_running_flag | running=True여서 실패 | running=False, phase=finished, 새 begin 성공 |
| LifecycleSymptomsTests.test_stop_survives_pipeline_initialization | pipeline clear 1개가 남아 실패 | clear 0개, Event 유지, finished 수신 시 cancelled |

SM3 시험 이름 변경: test_current_dead_worker_remains_running → test_dead_worker_is_failed_and_unlocks_gui (running 유지 → 실패 회수); test_current_stop_is_cleared_but_manager_reports_cancelled → test_stop_survives_pipeline_initialization (Event 해제 → 유지).
SM3의 test_current_finished_worker_still_blocks_restart는 스레드가 실제로 살아 있는 재현이므로 기대와 이름을 유지했다. 실제 종료 뒤 정리 플래그가 남는 경우는 새 시험으로 추가했다.
SM3 관측 시험의 기대는 유지했다. lost_stop 보조 함수는 초기화 clear 부재를 검사하며, 읽기 전용 관측 시험에 한해 Event를 명시적으로 해제해 기존의 소실된 Event 입력을 구성한다.
그 밖의 기존 시험 파일은 수정하지 않았다.

추가 경계 시험: test_new_job_clears_previous_stop_but_rejected_start_preserves_stop, test_unstarted_worker_is_not_reaped, test_live_worker_is_not_reaped_and_dead_failure_notifies_once, test_pending_terminal_is_consumed_before_recovery.

## 검증

Python은 사용자 지정 v1803/exfile 가상환경, -X utf8 -m unittest 계열 사용. TEMP/TMP는 지정 임시 폴더, PYTHONPATH는 작업 폴더/tests. 시험 코드에 PC 고유 경로를 추가하지 않았다.
단계 1: 재현 1건 실패 확인 → SM3/jobs 25건 통과 → c031aabc.
단계 2: 재현 1건 실패 확인 → SM3/jobs 26건 통과 → 653ea689.
단계 3: 재현 1건 실패 확인 → SM3/jobs 27건 통과 → 3ddf9306.
확장 대상: 파일 이름에 desktop/web/jobs/pipeline/sm3가 있거나 desktop_web_ui, docfit_app.jobs, docfit_engine.pipeline를 참조하는 test_*.py 55개 모듈.
첫 확장 실행: 603건, 통과 601건, 제외 2건, 실패/오류 0 (75.143초).
최종 확장 실행: 55개 모듈, 606건 실행, 통과 604건·조건부 제외 2건, 실패 0·오류 0 (62.131초). 기존 시험 실패 없음. 정상 종료 큐 경쟁 보완과 경계 시험 3건 추가 뒤 재실행했다.

## 릴리스 노트용 문장

1. 작업이 비정상 종료되면 실패 사유를 표시하고 실행 중 화면의 잠금을 해제합니다.
2. 작업 스레드가 종료된 뒤에는 남은 실행 플래그를 정리해 다시 작업을 시작할 수 있습니다.
3. 시작 직후와 단계 사이에 요청한 중단을 유지하고 다음 안전 확인 지점에서 반영합니다.

## 남은 확인과 제한

COM 실행, 실문서 저장, 실제 Tk/Flutter 화면 클릭 타이밍은 시험하지 않았다. 협조적 확인 지점까지의 대기와 저장 중 중단 보류는 기존 정책을 유지한다. 살아 있는 COM 스레드를 강제로 종료하는 코드를 추가하지 않았다.
복구는 Tk 큐 소비 뒤 polling에서 수행하므로 dead가 한 snapshot 동안 보일 수 있고 UI 이벤트 루프 자체가 멈춘 상황은 이 수정으로 복구하지 않는다. 비정상 종료 사유는 종료 알림 누락(JOB_INTERRUPTED)이며, 원래 예외 객체를 수집하는 수정은 하지 않았다.
기존 실패/정상 종료 사건의 상태는 유지하고 terminal 중복 보고를 막는 기존 JobManager.report 규칙을 그대로 사용한다.
_tk 구조, hwp-auto-docfit.py, 사용자 문서 원본은 변경하지 않았다. 푸시/태그/GitLab 접근 없음.
