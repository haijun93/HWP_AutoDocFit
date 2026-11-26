# ROLLBACK1 — 원문·서식 원복과 필수 수정 실패 전파

작성: 2026-11-26 · 브랜치: `feature/ROLLBACK1` · 로컬 커밋만, 푸시 없음.

## 결과와 범위

F02·F07·F06·F03 네 결함을 수정했다. 관련 Python 30모듈 **279시험 OK**, 신규 회귀시험은 3모듈 27개다. 기존 시험 파일은 수정하지 않았다. 전체 Python 시험은 PM 릴리스 검증으로 남겼다. 실제 한/글 새 인스턴스의 합성 문서에서는 표 삽입·HWPX 저장, 삭제 후 삽입 예외의 원문/서식 복원, 장평 80·160 복원을 확인했다. 사용자 문서 사용 0건.

시작 시 `git merge feature/SAVEGUARD1`을 수행했다. 선행 커밋 `45bfa465`를 병합한 커밋은 `97158da3`이며, 병합 직후 `tests.test_output_naming` 5개를 직접 실행해 통과를 확인했다. 선행 브랜치에는 별도 SAVEGUARD1 결과 문서가 없어 커밋·코드와 이 시험을 근거로 삼았다. `feature/REVIEWALL1:docs/REVIEWALL1_RESULT_2026-11-26.md`는 `git show`로 읽었다.

## 결함별 변경과 재현

| 지적 | 검토 보고서의 근거 | 먼저 확인한 실패 | 수정 결과 |
|---|---|---|---|
| F07 | `spacing.py:962` | delta=0 복원에도 80→85, 160→150. 2시험 중 1실패 | 새 후보에만 85~150 제한. 복원은 보관한 7언어 원값 그대로 적용 |
| F06 | `spacing.py:1077~1088` | 탐색 중단/예외 후 장평 -5 잔존, 자간 부분 적용 예외 후 -4 잔존. 8시험 중 4실패 | 성공 전까지 시도 상태를 보관하고 finally에서 장평·자간 모두 복원. 한쪽 복원 실패에도 다른 쪽 시도 |
| F02 | `formatter_text.py:967,1009,1029,1063~1076` | 삭제 뒤 생성 False/예외, 빈 칸, 이동 False, 조용한 생성 실패, 중간 취소 때 원문 소실. 복구 검증/문서 실패도 누락 | 삭제 전 보관 → 교체 → 저장된 표 개수/칸 내용 확인 → 확정. 실패·취소 시 재열기와 원문/서식 비교 검증. 실패 문서는 결과 저장 차단 |
| F03 | `formatter_apply.py:1063~1069`, `pipeline.py:1067` | Execute False/예외·필드 설정 예외가 정상 반환. 내부 예외를 잡으면 단계 성공 및 SaveAs 도달. 7시험 중 6실패 | 필수 수정 오류를 예외와 문서별 실패 상태로 기록. 단계 종료·선행 서식 뒤·최종 저장 직전·문서 반환 시 확인 |

현재 변경 위치는 `spacing.py:954,1034`, `formatter_text.py:950,1036`, `formatter_apply.py:974`, `pipeline.py:72,282,299,541,1091` 및 신규 `box_table_rollback.py`, `edit_failures.py`다.

F02 최초 실행은 7시험, subTest 포함 11실패였다. 이 중 성공 대조군 1실패는 짧은 합성 문서가 기존 50% 글자수 저장 방어에 걸린 시험 자료 문제였다. 앞뒤 합성 문장을 늘려 고쳤으며 저장 방어와 기존 시험은 바꾸지 않았다. 나머지는 원문 소실·실패 전파/복구 검증 누락 재현이었다.

추가 경계도 먼저 실패시켰다. 자간·장평 **복원 자체의 오류를 상위가 잡으면 SaveAs에 도달**하는 1시험을 확인한 뒤, 이 오류도 필수 수정 실패 상태로 기록했다. 이후 결과 저장에 도달하지 않는다.

편집 스크립트의 매칭 수 사전 검증이 두 번 중단됐고, 해당 파일 쓰기 전에 멈췄다. 수정 후 재실행했으며 최종 줄바꿈·diff를 점검했다.

## 복구와 실패 상태의 계약

- 박스 표 대상이 있을 때만 교체마다 현재 편집 문서를 임시 HWPX로 보관한다. 새 표의 개수와 칸 내용을 저장 HWPX로 확인하므로 `Execute=True`인데 실제 삽입이 없거나 칸 글이 빠지는 경우도 실패한다. `TableCreate=False`, 칸 삽입 False, 필수 칸 이동 False는 즉시 실패다.
- 보관 실패 시 삭제를 시작하지 않는다. 삭제 후 실패·취소 시 보관본을 다시 열고 별도의 HWPX로 저장해 검증한다. 비교 대상은 `Contents/header.xml`, 모든 `Contents/section*.xml`의 요소/속성/텍스트, `BinData/*` 바이트다. 재조판되는 `linesegarray`만 제외한다. ZIP 컨테이너 전체 바이트 일치를 요구하는 검사는 아니다.
- 복원 확인 실패는 문서 예외로 전파한다. 복원에 성공해도 변환 실패를 성공으로 표시하지 않고 해당 문서의 최종 결과를 저장하지 않는다. 취소는 복원을 마친 뒤 False로 상위에 전달한다. 복원용 임시 저장과 최종 결과 저장을 구분한다.
- `_필수수정_실패`는 현재 실행 컨텍스트에 첫 실패를 보관한다. 문서 시작 때만 초기화한다. 호출부가 예외를 잡아 True/None을 반환해도 단계는 성공 종료하지 않으며 최종 SaveAs를 차단한다. 다음 문서는 이전 실패 상태를 초기화한다.
- 글꼴 TTF/HFT 재시도, 정상 성공 반환, 새 장평 후보 제한은 유지했다. 이동/선택/Cancel의 허용 False는 변경하지 않았다. 일반 Run 수정 명령 전부의 반환 계약을 이번에 재설계한 것은 아니다.

## Python 시험

작업 폴더에 `.venv`가 없어 동일 PC 설치 프로젝트의 Python 3.14 가상환경 실행 파일을 사용했다. 재실행 시 아래 `DOCFIT_TEST_PYTHON`에 사용할 `.venv/Scripts/python.exe`를 지정한다. 테스트 임시 자료는 작업 폴더 밖의 임시 디렉터리에서 생성했다.

선행 병합 확인:

```text
Ran 5 tests in 0.074s
OK
```

F07 수정 뒤 관련 시험:

```text
Ran 10 tests in 1.195s
OK
```

F06 수정 뒤 관련 시험:

```text
Ran 16 tests in 0.791s
OK
```

F02 수정 뒤 관련 시험:

```text
Ran 50 tests in 4.603s
OK
```

F03 수정 뒤 관련 시험:

```text
Ran 141 tests in 15.618s
OK
```

복원 오류의 저장 차단 추가 뒤:

```text
Ran 37 tests in 1.414s
OK
```

최종 30모듈 실행 명령:

```powershell
$env:PYTHONIOENCODING='utf-8'
& $env:DOCFIT_TEST_PYTHON -m unittest tests.test_output_naming tests.test_rollback_spacing tests.test_format_preserve_failure tests.test_word_split_direction tests.test_format_save_and_pagefit tests.test_step_search tests.test_spacing_contract tests.test_spacing_indent_loop tests.test_spacing_memo tests.test_spacing_memo_boundary tests.test_spacing_reset_xml tests.test_rollback_box_table tests.test_box_table_conversion tests.test_box_table_residue tests.test_text_table tests.test_rollback_required_edit tests.test_char_shape_values tests.test_pipeline_review tests.test_pipeline_order_fixes tests.test_batch_format tests.test_style_unify tests.test_style_unify_auto tests.test_style_unify_elements tests.test_style_unify_hierarchy tests.test_table_unify tests.test_reference_sample tests.test_stage_timing tests.test_stage_selection tests.test_stage_registry tests.test_table_bulk_format
```

```text
Ran 279 tests in 21.261s
OK
```

기존 가짜 페이지수 시험의 진단 로그와 줄검사 기억 로그가 함께 출력됐지만 실패/오류/제외는 0건이다. 전체 시험을 실행한 수치가 아니다.

## 실제 한/글 합성 확인

시작 시 COM 잠금이 있어 실행하지 않았다. 관련 Python 시험을 마친 뒤 COM·전체시험 잠금이 모두 없음을 확인했고, COM 잠금을 배타 생성한 뒤 전체시험 잠금을 다시 확인했다. `DispatchEx`만 사용해 새 인스턴스를 만들었으며 각 시도 종료 시 그 객체만 `Clear(1)`·`Quit()`했다. 사용자 한/글 창을 탐색·종료하지 않았다. COM과 Python 시험은 동시에 실행하지 않았다.

최종 확인 3개:

1. 합성 5문단 중 박스 표를 실제 1행 2열 한/글 표로 변환하고 HWPX 저장·표 내용 검증 통과.
2. 같은 합성 문서에서 실제 삭제 뒤 삽입 예외를 주입. 보관본 재열기, 재저장, 원문/서식 비교 통과. 삽입 예외도 상위에 전달됨.
3. 합성 두 글자 구간에 7언어 장평 `(80,160,90,100,110,120,130)` 복원. 실제 COM GetDefault와 저장 HWPX의 ratio가 모두 일치.

외부 임시 스크립트로 COM 세션 3회를 실행했다. 앞 2회는 1·2 확인은 통과했으나 3번의 검증 도구가 첫 문단의 구역 설정/단 정의가 차지하는 16위치를 무시하고 `(0,0,0)`을 글자 시작으로 가정해 0을 읽었다. 실제 삽입 뒤 GetPos로 글자 위치를 계산하도록 스크립트만 수정한 최종 실행은 3개 모두 통과했다. 이 도구 오류를 제품 수정으로 계산하지 않았다. 합성 산출물은 저장소 밖 `rollback1_com_*` 임시 폴더에 두었으며 최종 폴더의 `result.json`은 passed/3이다.

## 커밋과 남은 일

- `d6c01ae1`: F07 원래 장평 복원.
- `f0ec1a59`: F06 취소/예외/부분 적용의 자간·장평 복원.
- `31222581`: F02 HWPX 보관·삽입 확인·검증 복원·실패 전파.
- `d1f281d1`: F03 필수 글자 모양 오류의 단계/문서 전파와 저장 차단.
- `d3f66ca3`: 복원 오류가 호출부에서 잡혀도 저장 차단.

**E07 쪽 간격 원복은 미수정**이다. 검토 보고서의 `page_fit.py:2340~2341` 취소 조기 반환과 `:1809` 복원 예외 처리 등은 별도 카드로 남긴다. F04 저장 후 취소/검수 상태, F05 무결성 성공 계약, 그 밖 검토 지적도 이번 범위 밖이다.

전체 Python·Flutter·CI·설치본·사용자 문서·복잡한 다구역/중첩 표 실기 검증은 미실행이다. 실제 COM 중간 취소와 실제 COM 복구 불능은 실행하지 않았으며 상태형 가짜 hwp의 시험으로만 확인했다. 표마다 보관/확인 저장이 추가되므로 속도 개선을 주장하지 않는다. 원래 줄바꿈과 UTF-8을 보존했고 `hwp-auto-docfit.py`는 수정하지 않았다. 푸시·태그·릴리스·main 병합·GitLab 접근은 수행하지 않았다.
