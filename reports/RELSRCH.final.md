# RELSRCH 결과 보고서

- 작업 위치: `C:/HWP_AUTODOCFIT/v1803/relsrch`
- 브랜치: `feature/rename-related-search` (`github/main` 기준)
- 범위: 타임머신 검색 모드의 「연관검색」을 「위계검색」으로 표시

## 변경 내용

- Python 서비스 주석과 시리즈 시험 모듈 설명을 「위계검색」으로 갱신했습니다.
- Flutter 검색 모드 선택/상태 표시와 결과 대화상자 제목을 「위계검색」으로 바꾸고, 화면 시험 문구도 맞췄습니다.
- `related_search` 및 `related_*` 내부 이름/API는 변경하지 않았습니다. 설명 문구 「연관 문서」, 「연관 자료」는 검색 종류를 설명하는 표현이라 유지했습니다. 「위계기반 본문검색」 표기는 이번 대상 파일에서 발견되지 않았습니다.
- `releases/` 및 과거 노트는 수정하지 않았습니다.

## 시험 및 분석

- `python -m unittest tests.test_timemachine_series`: 여러 번 실행했으나 프로세스가 수십 초 이상 계속 실행되고 완료 요약/종료 코드가 나오지 않아 통과 여부를 확인하지 못했습니다.
- `flutter test test/timemachine_screen_test.dart` 및 `flutter analyze`: 패키지 확인 후 진행 표시만 출력된 상태로 30초 이상 정체했습니다. `--no-pub` 재실행도 결과를 내지 못해 통과 여부를 확인하지 못했습니다.
- `git diff --check`: 통과.
- 기준값이나 시험 기대치를 완화하지 않았습니다.

## 커밋

- `cf68e026` Rename timeline related search to hierarchy search (Python 단위)
- `09543838` Show hierarchy search label in timeline (Flutter 단위)

푸시 결과는 브랜치 push 후 아래에 기록합니다.
