# SM3 작업 수명 관측

범위: `docs/state-machine-inventory`. 기존 동작·응답 값·화면 문구·기존 시험은 유지한다. COM과 실문서는 사용하지 않는다.
선행 문서: HANDOVER.md와 상태 조사 ① 전체를 확인했다. 이 폴더에 TEAM_OPERATING_RULES.md는 없다. 인접 hsa 폴더의 같은 문서는 1~8절만 있으므로 요청된 0절 원문은 미확인이다.

## 1. 현재 동작 재현

추가 시험: `tests.test_sm3_job_lifecycle.LifecycleSymptomsTests`, 3건 통과.

| 증상 | 결과 | 재현 방법과 한계 |
|---|---|---|
| 중단 요청 소실 | 구성 요소 수준 재현 | 실제 pipeline.py AST에서 유일한 `중단_event.clear()` 문장을 추출해 cancel 직후 실행. Event는 해제되지만 manager의 cancel_requested는 남고 finished는 cancelled가 된다. 전체 pipeline·실제 클릭 타이밍은 실행하지 않았다. |
| 끝난 직후 재시작 거부 | 재현 | 실제 스레드를 Event에서 대기시키고 legacy_event에 finished 전달, Tk 소비 후 상태인 running=False 구성. manager는 done인데 begin은 JOB_RUNNING으로 거부한다. |
| 죽은 작업이 실행 중으로 남음 | 재현 | 실제 스레드를 SystemExit로 종료하고 terminal 사건을 보내지 않는다. running=True일 때 prepare_legacy가 회수하지 않아 manager도 running으로 남는다. pipeline 전체의 예외 경로는 실행하지 않았다. |

기존 증상은 고치지 않고 현재 동작을 고정했다. 스레드 대기는 시간 경과 경쟁 대신 Event와 join으로 통제한다. 실사용 발생 빈도는 확인하지 않았다.
