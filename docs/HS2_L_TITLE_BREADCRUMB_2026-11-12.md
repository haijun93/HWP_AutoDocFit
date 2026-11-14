# HS2-L 문서 앞 표 제목의 경로 표시

- 브랜치: `feature/hs2-hsa`. 로컬 커밋만 수행한다.
- `document_title_text(rows)`는 rank 0 문단이 없는 문서에서 첫 비표 문단 전의 표 칸만 조사한다. 공백을 제거한 글이 4~60자인 첫 칸을 선택한다.
- `breadcrumb`에만 표시용 document_title 항목을 추가한다. 제목 칸 자체를 선택하면 추가하지 않는다. 입력 행과 위계는 수정하지 않는다.
- `content_search.py`의 두 경로 조회 모두 이미 `level`을 포함하므로 검색 코드는 변경하지 않았다.
- 기존 시험은 수정하지 않고 순수 함수 시험 6개를 추가했다. 제목 표, 제목 없음, 중간 표 제외, 기존 제목 우선, 자기 자신 제외, 길이 경계와 첫 적합 칸 및 입력 불변을 확인한다.

## 실제 HWPX 확인

`E:/docfit-tmp/perf/1112-demo-txt/run01/*.hwpx`는 1개다. 작업 폴더 안 임시 디렉터리로 복사해 새 SQLite 색인을 만들었다. 원본·사본 SHA-256 일치, 색인 성공 1개/실패 0개. COM은 사용하지 않았다.

`source(일괄적용).hwpx`의 `keyword_search("삼체 행성")` 결과는 1건이다.

| 문단 번호 | 실제 breadcrumb |
|---:|---|
| 4 | `["삼체 문명 존속을 위한"]` |
| 6 | `["삼체 문명 존속을 위한", "□ 추진 배경"]` |

키워드 검색 점수는 -0.10200000000000001, relevance_rank는 1이다. `search("삼체 행성")`도 1건이며 score 12, best_level `표 칸`, relevance_rank 1이다. 위계 검색은 제목 칸 자신을 선택하므로 breadcrumb는 빈 목록이다.

같은 색인에 HEAD의 기존 breadcrumb 함수를 적용한 결과와 비교했다. 두 검색 방식 모두 breadcrumb/breadcrumb_text/context_hits를 제외한 모든 결과 필드(위치·점수·순위·best_level 포함)가 동일하다. 저장된 앞쪽 표 칸 0~4의 level은 table_cell, level_rank는 8로 유지된다.

## 시험과 제한

지정 Python의 `-X utf8 -m unittest tests.test_timemachine_content_search tests.test_hierarchy_path`: 첫 실행 91개, 전부 통과, 실패·오류·제외 0개, 98.546초. 최종 실행 결과는 아래에 기록한다.

최종 두 모듈: 91개 전부 통과, 실패·오류·제외 0개, 77.822초. 이후 시험의 제목 문자열을 정리한 `tests.test_hierarchy_path` 단독 확인도 13개 전부 통과, 0.182초다.

`git diff --check` 통과. 전체 discovery와 한/글 COM 시험은 이번 범위에서 실행하지 않았다. 제목이 두 문단으로 나뉜 실제 파일은 요청 규칙에 따라 첫 적합 문단만 경로에 표시한다. 재색인은 이 표시 변경에 필요하지 않다.

푸시·태그·GitLab 접근 없음. 원본 문서 수정 없음.

임시 사본·SQLite·실측 JSON은 작업 폴더의 `hs2-l-uo91vjss/`에 미추적 상태로 남겼다. 정리 삭제 명령이 도구 정책에 의해 차단되어 삭제하지 않았다. 커밋에는 포함하지 않는다.
