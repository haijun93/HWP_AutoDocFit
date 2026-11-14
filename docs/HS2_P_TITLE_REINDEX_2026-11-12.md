# HS2-P 제목 표 승격·자동 재색인 (2026-11-12)

작업 브랜치: `feature/hs2-hsa`. 로컬 커밋만 수행. 푸시·태그·GitLab 접근 없음.

## 구현

- HWPX에 명시적인 document_title이 없고 첫 표 앞의 비표 문단이 3개 미만이면,
  hierarchy_path의 기존 제목 칸 선택 기준으로 고른 table_cell_id의 모든 문단을
  document_title(순위 0)로 판정한다. 날짜·담당자 등 다른 칸은 table_cell로 유지한다.
- 추출 중 table_id를 보존하여, 첫 표가 비어 있거나 제목 후보가 없어도 다음 표를
  대신 제목 표로 삼지 않는다. 엔진 경로와 Python 판정 경로에 동일하게 적용한다.
- 제목 칸의 부제·본제를 합쳐 breadcrumb에 표시한다. 기존 document_title_text의
  순위 0 존재 시 보강 중단 동작을 유지하여 제목을 중복 삽입하지 않는다.
- 기존 content_meta.hierarchy_schema_version을 규칙 버전으로 재사용하고 1→2로 올렸다.
  이전 버전도 저장된 문단으로 검색하며, needs_reindex_count에는 계속 포함한다.
- 문단 색인이 없던 버전 0 DB는 기존 전체 본문을 임시 본문 문단으로 복구한다.
  버전은 올리지 않는다. 2글자 키워드도 조각 색인이 없으면 LIKE 후보 경로를 사용한다.
- 서비스 초기화 시 재색인 필요 문서가 있으면 기존 _refresh_content_index_async를 호출한다.
  이 경로는 Python 배경 worker를 사용하며 running/seen/indexed/errors 상태를 그대로 쓴다.
  기존 자동 갱신은 스캔 완료 후에만 호출되어 있었고, 첫 서비스 시작 연결을 추가했다.

## 허용된 기존 시험 수정

`tests/test_timemachine_content_search.py`:

- 711줄: test_table_cell_two_mentions_beats_body_one_at_same_lowest_level의 첫 문서
  best_level 기대값을 표 칸→문서 제목으로 변경. 첫 표의 제목 칸에 해당한다.
- 907줄: test_table_cell_records_keep_table_level_and_order의 records[1] 기대값을
  table_cell→document_title로 변경. 비표 본문 1개 다음 첫 표의 제목 칸에 해당한다.
  records[2]는 별도 표이므로 기존 table_cell 기대를 유지한다.
- test_heading_markers_reuse_existing_classifier는 첫 표 앞에 문단 8개가 있어 수정하지 않았다.
- 그 밖의 기존 시험은 수정하지 않았다.

## 검증

Python: `C:/HWP_AUTODOCFIT/v1803/exfile/.venv/Scripts/python.exe -X utf8 -m unittest ...`
TEMP/TMP는 E:/docfit-tmp로 설정했다.

- 서비스 관련 전체 5개 모듈 + 새 회귀 모듈: **97건, 전부 통과**, 44.847초.
  test_timemachine_service 64, test_timemachine_content_index_lifecycle 6,
  test_timemachine_extensions 5, test_timemachine_related 11,
  test_timemachine_thumbnail_cache 5, test_hs2_p_title_reindex 6.
- 지정 3개 모듈: **93건 실행, failures=4, errors=1**, 65.800초.
  content_search 78, hierarchy_path 13, hs2_m_title 2.
  실패는 아래 기존 시험 3건의 새 요구와 반대되는 기대값이며, 두 subTest 실패가 별도로 집계된다.
  따라서 완료 조건인 전체 통과는 아직 충족하지 못했다.
- 3,000건 위계 검색: 0.3057초(1초 기준 통과).
  2글자 3,000건 키워드 검색: 0.2994초,
  LIKE 후보 0.0078초 / 조각 후보 0.0024초.
  병렬 실행 중 2.2160초 성능 실패가 한 번 있었고, 단독 전체 재실행에서 해소됐다.
- 서비스 시험 중 스캔 종료 후 이미 닫힌 DB에 접근하는 로그가 출력됐으나 시험은 통과했다.
  서비스 close의 스캔 스레드 종료 처리는 이번 변경 범위에서 수정하지 않았다.
- git diff --check 통과. 한/글 COM/UI/배포 EXE 시험은 수행하지 않았다.

### 수정하지 않은 충돌 시험

1. content_search의 test_title_fallback_keeps_hwpx_table_levels_after_rule_revert, 158줄:
   앞에 본문 0개·2개인 첫 제목 표도 table_cell이어야 한다는 기대.
   새 규칙에 따라 document_title이므로 subTest 2개 실패.
2. hs2_m_title의 test_index_persists_cell_groups_without_changing_ranking, 50줄:
   제목 칸 두 문단도 table_cell(8)이어야 한다는 기대.
   새 규칙에 따라 document_title(0) 두 개이므로 실패.
3. content_search의 test_legacy_schema_migrates_without_losing_documents, 645줄:
   재색인 전 검색 결과가 []여야 한다는 기대.
   이번 요구에 따라 이전 결과 1개를 유지하므로 실패.
   이 단언 뒤의 migrated.close()가 실행되지 않아 Windows 임시 폴더 정리에서
   WinError 32 부수 오류 1개도 발생한다. 새 별도 회귀 시험에서는 finally로 닫는다.

이 세 시험은 사용자가 허용한 기존 시험 3건 목록 밖이다.
따라서 기대값 수정 허용 여부를 확인 요청했으며, 답변 없이 수정하지 않았다.
테스트 분기·건너뛰기·편법으로 통과시키지 않았다.

## 실제 HWPX 사본

원본: `E:/docfit-tmp/perf/1112-demo-txt/run01/source(일괄적용).hwpx`
사본·시험 DB: `E:/docfit-tmp/hs2-p-demo-j6u96xtz/`
원본·사본 바이트 일치 확인. SHA-256:
`150de67bc7cb92f3dab13696977a527ccdd380f22e66c6a263f54a9410e98c3a`

- 제목 칸 문단 0: 삼체 문명 존속을 위한 → document_title, cell_id=1.
- 제목 칸 문단 1: 지구 이주·점령 계획(안) 보고 → document_title, cell_id=1.
- 버전 1 상태를 재현한 사본 DB: needs_reindex_count=1,
  “삼체” 검색 문서 1개, best_level=표 칸, total_matches=10.
- index_file로 재색인 후: needs_reindex_count=0,
  “삼체” 검색 문서 1개, best_level=문서 제목, total_matches=10.
- “삼체” 대표 일치가 제목 자체(문단 0)이므로 상위 breadcrumb는 빈 목록/빈 문자열이다.
  제목을 자기 자신의 조상으로 중복 삽입하지 않은 결과다.
- 같은 검색의 키워드 위치 문단 4 breadcrumb:
  `삼체 문명 존속을 위한 지구 이주·점령 계획(안) 보고`
- 같은 검색의 키워드 위치 문단 6 breadcrumb:
  `삼체 문명 존속을 위한 지구 이주·점령 계획(안) 보고 > □ 추진 배경`

## 로컬 커밋

1. `afd07244`: 제목 표 판정·breadcrumb·허용된 시험 기대 변경·제목 회귀 시험.
2. 후속 커밋: 규칙 버전·자동 배경 재색인·이전 검색 유지·재색인 회귀 시험·이 보고서.
