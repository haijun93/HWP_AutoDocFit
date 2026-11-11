# SM3 작업 수명 관측

범위: `docs/state-machine-inventory`. 기존 동작·응답 값·화면 문구·기존 시험은 유지한다. COM과 실문서는 사용하지 않는다.
선행 문서: HANDOVER.md와 상태 조사 ① 전체를 확인했다. 이 폴더에 TEAM_OPERATING_RULES.md는 없다. 처음 확인한 hsa 사본은 1~8절뿐이었다. 구현 뒤 주변 사본들을 추가 검색해 mainfix/docs/TEAM_OPERATING_RULES.md의 0절 원문을 확인했다. 요청된 선행 읽기 순서는 충족하지 못했다. 기존 동작·시험·결과 문서 유지, UX·DX·AX 모두 개선 원칙과 구현이 일치함을 재검토했다. 다른 작업 폴더는 읽기만 했다.

## 1. 현재 동작 재현

추가 시험: `tests.test_sm3_job_lifecycle.LifecycleSymptomsTests`, 3건 통과.

| 증상 | 결과 | 재현 방법과 한계 |
|---|---|---|
| 중단 요청 소실 | 구성 요소 수준 재현 | 실제 pipeline.py AST에서 유일한 `중단_event.clear()` 문장을 추출해 cancel 직후 실행. Event는 해제되지만 manager의 cancel_requested는 남고 finished는 cancelled가 된다. 전체 pipeline·실제 클릭 타이밍은 실행하지 않았다. |
| 끝난 직후 재시작 거부 | 재현 | 실제 스레드를 Event에서 대기시키고 legacy_event에 finished 전달, Tk 소비 후 상태인 running=False 구성. manager는 done인데 begin은 JOB_RUNNING으로 거부한다. |
| 죽은 작업이 실행 중으로 남음 | 재현 | 실제 스레드를 SystemExit로 종료하고 terminal 사건을 보내지 않는다. running=True일 때 prepare_legacy가 회수하지 않아 manager도 running으로 남는다. pipeline 전체의 예외 경로는 실행하지 않았다. |

기존 증상은 고치지 않고 현재 동작을 고정했다. 스레드 대기는 시간 경과 경쟁 대신 Event와 join으로 통제한다. 실사용 발생 빈도는 확인하지 않았다.

## 2. 추가 필드

desktop/browser의 기존 `get_state` 응답 최상위에 `job_phase`만 추가했다. 기존 Tk snapshot 콜백 안에서 계산하며 Tk 왕복을 추가하지 않는다. `JobManager.status`, native ApplicationService의 응답은 변경하지 않았다.
선택 대상은 GUI의 `_job_id`이며 없으면 최근 문서 작업이다. preview 등 LIGHT_KINDS는 제외한다. 기존 `job` 필드는 종전대로 최근 작업을 반환하므로 가벼운 작업이 뒤에 있으면 서로 대상이 다를 수 있다.

판정은 아래 순서이며 최초 일치 값을 반환한다. 저장 상태가 아니라 순간 관측 값이다.

| 값 | 조건 |
|---|---|
| closing | closing 플래그가 켜지고 GUI가 실행 중이거나 스레드 생존 |
| finishing | 스레드 생존 + GUI 대기 또는 manager 종료 상태 |
| dead | 스레드 객체는 있으나 죽었고 GUI 또는 manager가 실행 중 |
| failed | manager 실패 종료 |
| finished | manager done 또는 cancelled 종료 |
| stop_requested | manager cancel_requested 또는 연결된 stop Event 설정 |
| starting | manager queued |
| running | GUI 또는 manager 실행 중 |
| idle | 위 조건 모두 아님 |

증상별 새 값: 중단 소실 직후 `stop_requested`(finished 사건 뒤에는 `finished`, 기존 job.state는 cancelled), 완료 후 스레드 정리 대기 `finishing`, 종료 통지 없는 사망 `dead`. 실패 문서를 포함한 done도 finished다. 이 필드만으로 성공/취소를 판정하지 않고 기존 job.state와 failures를 함께 읽는다. 서식 분석 플래그는 작업 수명과 별개이므로 이 값으로 시작 가능 여부를 대신 판단하지 않는다.

추가 시험은 현재 동작 3건과 관측 5건이다. 불변 snapshot·사건 목록·Event, light 작업 제외, 기존 get_state의 job/running 유지, Tk 호출 1회도 확인한다.

## 3. 후속 수정 제안 (미구현)

| 제안 | 사용자 영향 | 깨질 수 있는 시험 | 되돌리기 |
|---|---|---|---|
| pipeline의 두 번째 stop clear 제거, 시작 접수 전 초기화 한 곳으로 통일 | 시작 직후 누른 중단이 유지됨. 직전 작업 Event가 새 작업에 섞이지 않도록 경계 확인 필요 | 새 현재 동작 중단 소실 시험, test_jobs의 협력 취소, 실제 시작 직후 중단 시험 필요 | 초기화 한 곳 변경을 별도 커밋으로 되돌림 |
| terminal 사건과 실제 worker 종료를 분리하고 can_start가 정리 대기를 반영 | 완료 직후 시작 버튼을 누를 수 있는 시점이 늦어지며 JOB_RUNNING 거부 감소 | 현재 재시작 거부 시험, test_legacy_adapter, test_conversion_job_runs_cleanup_before_terminal, 화면 can_start 시험 | 별도 커밋으로 사건/버튼 정책 함께 복원 |
| worker 종료를 finally에서 통지하고 Tk 회수에서 실행 플래그·스레드 불일치 처리 | 비정상 종료를 계속 실행 중으로 표시하지 않고 실패 안내 | 현재 dead 시험, fatal_error/정상 종료 단일 통지, test_force_stop, 종료 감시 시험 | 회수 정책을 별도 커밋으로 복원. 살아 있는 COM 작업을 죽었다고 판단하지 않는 검증 필요 |

세 제안은 상태 변경·사용자 동작 변경이므로 이번에는 구현하지 않았다.

## 4. 검증 기록

- Python: 사용자 지정 v1803/exfile 가상환경, `-X utf8 -m unittest` 사용.
- 필드 추가 전 현재 동작 시험 3건 통과. 필드 추가 후 jobs·desktop_browser_api·현재 동작 54건 통과.
- desktop/web/jobs 이름을 가진 6개 기존 모듈 + SM3 신규 모듈: 90건 전부 통과(0 실패·0 오류·0 제외).
- 확장 범위: `test_*.py` 중 `desktop_web_ui` 또는 `docfit_app.jobs`를 직접 import하는 모듈 48개. 첫 실행은 528건 중 import 오류 2건·제외 2건. 기존 시험의 최상위 import(`test_fidelity`, `test_desktop_browser_api`) 때문에 `PYTHONPATH=tests`가 필요했으며 시험은 수정하지 않았다.
- 재실행 환경은 `PYTHONPATH=<작업 폴더>/tests`, `TMP`와 `TEMP`는 사용자 지정 임시 폴더. 신규 시험에는 PC 고유 경로와 임시 폴더 위치를 하드코딩하지 않았다.
- 확장 재실행 최종 결과: 48개 모듈, 548건 실행, 546건 통과·2건 조건부 제외, 실패 0·오류 0(74.798초). import 오류가 해소되어 첫 실행보다 실제 시험 20건이 추가로 로드됐다. SSL/SQLite ResourceWarning과 외부 조회 HTTP 403 경고가 있었으나 시험 실패는 없었다.
- COM 실행·실문서·실제 클릭 경쟁·Flutter 실화면은 미확인. 원본 문서, legacy.py, hwp-auto-docfit.py, 기존 시험은 변경하지 않았다. 푸시·태그·GitLab 접근 없음.

단계별 로컬 커밋: `21204431` 재현, `eae4e372` 읽기 전용 필드, `66a769cc` 새 필드 시험. 마지막 문서 커밋은 git log에서 확인한다.
