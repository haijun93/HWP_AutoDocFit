# HS2-N 부모 절 묶음

- `hierarchy_path.group_by_parent` 순수 함수 추가. 실제 위계의 가장 가까운 조상 제목으로 묶고 첫 일치 문단 순서로 반환한다. 구성 문단도 문서 순서이며 중복 번호는 한 번만 세고 존재하지 않는 번호는 무시한다.
- 부모 없는 문단은 `parent_paragraph_index=None`, 제목·경로는 빈 문자열인 하나의 묶음으로 반환한다. 표시용 표 제목은 실제 부모가 아니다. 경로는 부모 제목까지 포함하며 기존 제목 축약 규칙을 사용한다.
- `keyword_search`의 표시 대상 문서에만 `sections`를 추가한다. 기존 `_breadcrumb_rows` 일괄 조회를 재사용하며 추가 조회는 없다. 기존 `locations`는 최대 3개를 유지하고 `sections`는 문서의 전체 유효 일치 문단을 묶는다.
- 기존 필드·정렬 계산과 Flutter는 변경하지 않았다. 기존 시험도 수정하지 않았다.

## 검증

지정 Python에 `-X utf8 -m unittest tests.test_timemachine_content_search tests.test_hierarchy_path tests.test_parent_sections` 실행. TEMP/TMP는 `E:/docfit-tmp`로 지정했다.

- 총 94개 통과, 실패 0, 오류 0, 제외 0, 66.908초.
- 기존 검색 78개(합성 색인 10개 질의 결과 고정 시험 포함), 기존 위계 경로 13개, 신규 3개.
- 신규 시험: 중첩·부모 없음·문서 순서·중복/누락 번호·입력 보존·표 제목 처리, 전체 5개 일치 문단과 기존 3개 위치 유지, 페이지 표시 대상만 계산, 위계 조회 1회 재사용, 기존 필드 값 보존.
- `git diff --check` 통과.

## 범위와 남은 확인

이번 범위의 미해결 문제 없음. 실제 사용자 문서·한/글 COM·Flutter 화면 시험은 실행하지 않았다. 합성 문서만 사용했다. 로컬 커밋만 수행하며 푸시·태그·원격 API 작업은 하지 않는다.
