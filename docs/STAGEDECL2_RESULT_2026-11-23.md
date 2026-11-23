# STAGEDECL2 결과 — 실제 선행목록 shadow 점검

작성일: 2026-11-23

## 연결 및 동작

`docfit_engine/pipeline.py`에서 XML 선행 서식 `선행목록` 구성이 끝난 직후 shadow 점검을 연결했다. `DOCFIT_STAGE_PLAN_CHECK=1`일 때만 planner를 호출하며, 작업 모드·옵션·세부 작업 선택·쪽 범위 및 현재 런타임의 프로필 플래그를 입력으로 전달한다. 실제 목록에서는 켜진 항목의 표시명과 순서를 추출해 선언표 출력과 `compare_stage_names` 순수 함수로 대조한다.

차이는 `[DOCFIT_STAGE_PLAN_CHECK]` 접두사의 로그 한 줄에 모드, 쪽 범위, 차이 항목을 남긴다. 비교 중 예외도 같은 접두사로 기록하고 삼킨다. planner 결과는 실행에 사용하지 않으므로 점검 켜기/끄기가 처리 결과를 바꾸지 않는다. 기본 상태는 꺼짐이며, 끄려면 환경변수 `DOCFIT_STAGE_PLAN_CHECK`를 제거하거나 `1` 이외 값으로 설정한다. 개발자 환경에서도 명시적으로 `1`을 설정해야 활성화된다.

## 차이 조사

- 선언표의 51개 항목과 기존 2,048개 계획 입력 비교: 차이 0건( STAGEDECL1 결과 기준).
- 이번 변경에서 발견한 런타임 선행목록 차이: **아직 확인된 차이 없음**.
- mock pipeline 경로 훅: 쪽 범위를 지정해 문서 전체 XML 단계를 건너뛴 입력으로 점검을 켰으며 경고 0건. 이 입력은 빈 선행목록 비교만 확인한다.
- 다양한 프로필/옵션 조합을 가진 Windows 한/글 실제 엔진 실행은 수행하지 않았고, COM 시험도 요청에 따라 수행하지 않았다. 따라서 런타임 차이가 전혀 없다고 결론 내리지 않는다.

## 시험

각 파일을 별도 실행했다.

```text
test_stage_registry.py: Ran 4 tests ... OK
test_pipeline_review.py: Ran 26 tests ... OK
test_pipeline_order_fixes.py: Ran 9 tests ... OK
test_external_pipeline_routing.py: Ran 2 tests ... OK
```

`test_stage_registry`에는 동일·순서 변경·누락·추가 비교와 쪽 범위에서 문서 전역 XML 단계가 빠지는 검사를 추가했다. `test_pipeline_review`에는 mock pipeline에서 환경변수 훅을 켜고 경고가 없는지 확인하는 시험을 추가했다. 전체 시험과 COM 시험은 실행하지 않았다. 저장소 `.venv`가 없어 시스템 `python` 3.14로 실행했다.

## 다음 단계 제안

선언표 기반 실행기로 옮기기 전에 실제 Windows 시험 데이터에서 표준서식·머리 서식·표 프로필·제목/붙임 옵션을 달리한 shadow 로그를 충분히 축적해야 한다. 발견된 모든 차이를 입력 조건과 함께 판정하고, 누락된 런타임 조건을 선언표와 planner에 반영한 뒤, 별도 변경에서 실행 목록 생성부를 planner 기반으로 바꾸는 것이 필요하다. XML 단계 외 COM 단계의 상태 의존성, 중간 부수효과와 실패/취소 경계도 실행 계약으로 명시해야 한다.
