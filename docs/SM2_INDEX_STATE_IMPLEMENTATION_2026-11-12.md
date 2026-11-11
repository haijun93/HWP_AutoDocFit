# SM2 본문 색인 상태 통합 구현 (2026-11-12)

작업: `E:/docfit-tmp/hsa`, `feature/hs2-hsa`. 사용자 지정 3단계를 구현했다. 기존 명령 이름·응답 필드·화면 문구를 유지했고 상태 필드를 추가했다. 원격 접근·푸시·태그·사용자 문서 수정은 하지 않았다.

## 변경

1. 종료 시 새 자동 갱신/스캔을 막고 진행 중인 스캔과 본문 색인 스레드가 끝난 뒤 DB를 닫는다. 종료 후 자동 갱신의 DB 접근 금지와 DB 종료 전 스레드 대기를 회귀시험 2개로 고정했다. 커밋 `5d061082`.
2. `TimemachineService.content_status()`를 시작·취소·상태 명령의 공통 응답 함수로 사용한다. `phase`는 idle/indexing/reindexing/cancel_requested/cancelled/finished/failed이다. 기존 색인이 있는 수동 작업과 자동 갱신은 reindexing이며, 작업 예외와 스레드 시작 실패는 failed와 `failure_reason`을 남긴다. 파일별 추출 오류는 기존 errors 카운트와 처리 방식을 유지한다. 저장 상태는 `ContentSearchIndex.status_summary()`가 제공한다. 기존 needs_reindex 필드 외에 has_index/indexed_document_count/persistent_error_count를 추가했다. 작업 오류와 영속 오류는 별개이며 상태 조회에 파일 stat 순회를 추가하지 않았다. 커밋 `50493ff6`.
3. Flutter의 `_applyContentStatus()`가 모든 상태 응답을 해석한다. 진입·검색·본문 모드 전환에서 상태를 읽고 자동 작업도 기존 진행 표시와 1초 폴링에 연결한다. cancel_requested도 진행 상태를 유지하고, 통신 실패는 완료로 간주하지 않고 재조회한다. 검색 결과의 누락 필드가 재색인 안내를 지우지 않게 상태 응답에서 안내를 읽는다. 위젯 시험 5개를 추가했다. 이 보고서를 포함한 화면 커밋으로 기록한다.

## 실제 실행 결과

Python 명령은 사용자 지정 `C:/HWP_AUTODOCFIT/v1803/exfile/.venv/Scripts/python.exe -X utf8 -m unittest tests.test_timemachine_service tests.test_timemachine_content_search tests.test_hs2_p_title_reindex`를 사용했다.

| 실행 | 시험 수 | 결과 |
|---|---:|---|
| 변경 전 Python 기준선 | 148 | 182.127초, 실패/오류 0. 단 닫힌 DB 자동 갱신 오류 로그 2회 |
| 종료 후 갱신 회귀시험, 수정 전 | 1 | 의도한 실패 1: closed DB accessed |
| 1단계 Python | 150 | 238.522초, 실패/오류 0, 닫힌 DB 오류 로그 없음 |
| 1단계 종료 관련 집중 시험 | 4 | 1.575초, 실패/오류 0 |
| 2단계 첫 Python 실행 | 155 | 159.995초, 오류 1: 자동 Thread.start 실패 복구 누락. 구현 보완 후 재실행 |
| 2단계 최종 Python | 155 | 156.648초, 실패/오류 0, 닫힌 DB 오류 로그 없음 |
| 변경 전 Flutter 화면 시험 | 48 | 모두 통과 |
| 변경 후 flutter analyze --no-pub | - | 22.1초, No issues found, 지적 0건 |
| 변경 후 flutter test test/timemachine_screen_test.dart --reporter expanded --no-pub | 53 | 모두 통과 |

최종 Python 구성은 서비스 71개, 본문검색 78개, 제목 재색인 6개다. Python 7개·Flutter 5개를 추가했고 기존 시험 본문은 변경하지 않았다. 시험 임시 폴더는 기존처럼 tempfile 기본 위치를 사용하며 PC 고정 경로를 시험에 넣지 않았다. 실행 시간은 성능 비교 자료로 사용하지 않는다. 기존 Flutter 손상 JPEG 대체 표시 시험의 JPEG 진단 출력은 동일하게 남는다.

## 한계와 인계

- 저장소 운영 규칙 파일에는 현재 0절이 없다. 사용자 메시지에 명시된 대원칙을 적용했다.
- 지정 Python 3모듈과 Flutter 정적 분석·화면 위젯 시험을 실행했다. Python 전체 discover, Flutter 전체 시험, 실제 앱/EXE, 한/글 COM 실기 시험은 실행하지 않았다.
- 종료는 진행 중인 Python 추출/스캔의 종료를 기다린다. 외부 작업이 영구 정지하는 상황의 강제 종료는 이번 범위에 포함하지 않았다.
- phase finished는 작업 루프의 정상 종료를 뜻하며 파일별 errors가 0이라는 보장은 아니다. needs_reindex_count는 기존 스키마/문단 정보 보수 의미를 유지하며 디스크 전체 최신성 보장은 아니다.
- 해결하지 못한 지정 완료 조건은 없다. 실기·전체 회귀 검증은 위 범위 밖 미확인이다. main 병합과 게시도 수행하지 않았다.
