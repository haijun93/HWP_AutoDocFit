# INDENTFIX 작업 결과 보고서

## 1. 개요 및 배경

- **이슈**: `reports/VERIFY1108.final.md 3번`에서 보고서 서식 적용 시 항목기호/번호 항목 간 내어쓰기 차이가 발생함.
  - 괄호형 문단(예: `   ❶ (이동도서관) 차량...`): 번호+빈칸 뒤 첫 글자 `(` 기준으로 계산되어 `intent = -4200` HWPUNIT 부여.
  - 콜론형 문단(예: `   ❶ 모든 주민 대상: 안내 내용...`): 콜론 라벨 뒤(`_COLON_LABEL_RE`)로 오프셋이 건너뛰어져 `intent = -15076` HWPUNIT 부여.
  - 동일한 위계의 번호 항목임에도 라벨 표기 형태(괄호 vs 콜론)에 따라 내어쓰기 폭이 크게 달라져 문서 균형이 어긋남.
- **수정 목표**: 불변 규칙인 **「붙임·항목기호 내어쓰기: 번호+빈칸 뒤 본문 첫 글자 기준」**을 일괄 적용하여 괄호형과 콜론형의 내어쓰기를 동일한 기준(-4200)으로 일치시킴.

## 2. 변경 사항

1. **`docfit_core/xml_hanging_indent.py`**:
   - `hanging_indent_offset(text, *, rule="after_marker")` 기본 동작을 **번호+빈칸 뒤 본문 첫 글자 기준**으로 통일.
   - 콜론 라벨이 오더라도 콜론 뒤로 건너뛰지 않고 번호/기호 뒤 공백 다음 첫 글자(`idx`)를 내어쓰기 오프셋으로 확정 (괄호형·콜론형 내어쓰기 일치).
   - 단, `inline_item` 복합 문장(예: `• 위치: ① 와우산로...`)은 기존과 동일하게 인라인 기호 시작 위치 기준 유지.
   - 기호 바로 뒤에 구분자 콜론이 붙은 경우(`①: 본문`)는 콜론 다음 공백 뒤 본문 첫 글자 기준 처리.
   - 본문 첫 글자가 여는 낫표(「, 『)인 경우 낫표 안쪽으로 건너뜀 유지.
   - 레거시 호환을 위해 명시적으로 `rule="colon_label"`을 호출한 경우에만 콜론 뒤 위치를 계산하도록 보존.
   - `hanging_indent_pair`의 기본 `rule`도 `"after_marker"`로 변경.

2. **`docfit_engine/formatter_indent.py`**:
   - `문단_내어쓰기_기준_오프셋`에서 COM Shift+Tab 기준 위치를 계산할 때, 기존의 콜론 뒤 본문 탐색(`_문단_본문시작_오프셋`) 대신 `_문단_기호뒤_오프셋`을 기반으로 하여 "번호+빈칸 뒤 본문 첫 글자" 기준으로 통일.

3. **단위 테스트 수정 및 추가**:
   - `tests/test_indentfix.py`: INDENTFIX 전용 신규 단위 테스트 추가 (6개 검증 케이스).
     - 괄호형과 콜론형 내어쓰기 `offset` 및 `(left, intent)` 일치 검증 (`(4200, -4200)`).
     - `-`, `①`, `1.`, `가.`, `*` 등 다양한 기호 계층에서 콜론 라벨 시 번호+빈칸 뒤 본문 첫 글자 정렬 검증.
     - 낫표(`「`, `『`) 건너뛰기 및 `inline_item` 복합 문장 동작 검증.
     - XML 패키지 일괄 서식 적용(`apply_xml_hanging_indent`) 시 `paraPr`의 `(4200, -4200)` 동일 생성 검증.
   - `tests/test_label228.py`: `test_hanging_offset_same_as_dash`에서 콜론형 기대 오프셋을 `len('   ❶ ')` = 5로 갱신.
   - `tests/test_xml_hanging_indent.py`: `test_todo_96_colon_label_hanging_indent` 및 별표 오프셋 기대값을 INDENTFIX 규칙에 맞게 갱신하고, 레거시 `rule="colon_label"` 동작도 검증하도록 보강.

## 3. 검증 결과

- **단위 시험**:
  ```bash
  python3 scripts/linux_stub_test_runner.py tests.test_indentfix tests.test_xml_hanging_indent tests.test_label228 tests.test_label229 tests.test_continuation tests.test_marker_scheme tests.test_marker_prefix tests.test_format_elements tests.test_release_format_audit
  ```
  결과: **84 tests OK (전체 통과)**
- **COM 시험**: 사용자 지시에 따라 **COM 시험은 수행하지 않음**.

## 4. 커밋 및 브랜치

- 기준 브랜치: `github/main`
- 작업 브랜치: `feature/indentfix`
