# SM5 파일 변환 결과 상태

## 재현

- 같은 결과 형식 파일(`.hwpx` → HWPX)을 임시 테스트 파일로 `run_conversion`에 넣었다. 가짜 COM factory를 예외 발생 설정해 COM 미실행을 확인했다. 반환 결과는 `skipped`였지만 사건은 `document_error`, 종료는 `file_conversion_finished(0, 1)`이었다.
- 텍스트 입력의 DOCX 변환기를 `RuntimeError('fake converter failure')`로 대체했다. 반환 결과는 실패, 결과 경로는 없었지만 종료 사건은 `file_conversion_finished(0, 1)`이었다.
- 기존 가짜 변환 시험에서 재현했고 한/글 및 kordoc은 실행하지 않았다.

## 변경

- `file_conversion_finished` data에 기존 성공·실패 수를 유지하면서 `outcome`, `skipped`, 파일별 `results`(source/status/reason/output)를 추가했다.
- 변환 작업 item 사건에 파일별 `converted/skipped/failed`, 사유와 결과 경로를 담았다. 기존 `item_done`/`item_failed` 이벤트 타입은 유지한다.
- Flutter 진행 화면은 변환 완료 요약의 성공 수가 0이면 건너뜀·실패 수를 표시하고, 건너뜀 파일을 실패 파일과 다른 상태·문구로 나타낸다.

## 검증

- Python 변환 관련 시험: `tests.test_cli tests.test_jobs tests.test_conversion_stop tests.test_progress_guide` — 56개 통과.
- `git diff --check` 통과.
- `flutter analyze` 통과(issues 없음).
- `flutter test test/live_progress_test.dart`는 테스트 로딩 뒤 출력이 멈추고 도구 세션이 30초에 반환되어 완료/통과 수를 확인하지 못했다. 재시도에서도 같은 현상.

## 제한

- 저장소 안내의 `docs/TEAM_OPERATING_RULES.md`는 존재하지 않았다. `docs/TEAM_ORGANIZATION_AND_WORKFLOW.md`는 있었지만 0절이 없고 다른 문서라 대체해서 읽지 않았다.
- Flutter 위젯 시험 결과를 확인하지 못해 커밋 전 상태다.
