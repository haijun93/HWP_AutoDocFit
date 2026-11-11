# HS2-D2 번호 제목 판정 조사

Python 경로만 사용(Rust 브리지 반환값 None). 합성 TXT 3종을 SQLite에 실제 색인한 변경 전 결과.

| 문서 | 문단 | 실제 level |
|---|---|---|
| consistent | 제1장 시설 관리 | chapter |
| consistent | Ⅰ. 추진 배경 | chapter |
| consistent | 1. 추진 계획 | section |
| consistent | 가. 세부 기준 | topic |
| consistent | 나. 추가 기준 | topic |
| consistent | 2. 실행 계획 | section |
| consistent | Ⅱ. 운영 배경 | chapter |
| consistent | 제2장 운영 관리 | chapter |
| isolated | 제1장 시설 관리 | chapter |
| isolated | Ⅰ. 추진 배경 | section |
| isolated | 1. 추진 계획 | body |
| isolated | 가. 세부 기준 | body |
| body_list | 시설 관리에 관한 설명입니다. | body |
| body_list | 제1장 시설 관리 | chapter |
| body_list | Ⅰ. 추진 배경 | chapter |
| body_list | 다음 사항을 확인합니다. | body |
| body_list | 1. 추진 계획 | section |
| body_list | 2. 실행 계획 | section |
| body_list | 가. 세부 기준 | topic |
| body_list | 이상입니다. | body |

고립된 장·로마자 및 본문 목록이 제목으로 오인된다. 일관된 체계에서도 장과 로마자가 같은 chapter여서 첫 등장 순서와 다르다. 4계열은 chapter/section/topic/topic으로 표현한다(제목 단계가 3개이므로 최하위는 topic 공유).

## 장 계열 시도 및 복원

- 변경 전 필수 두 모듈: 80개, 실패/오류 0.
- 제N장 연속성 조건을 적용한 뒤 본문검색 전체 73개 + 신규 3개 = 76개 실행, 75개 통과/1개 실패.
- 실패: `ContentSearchTests.test_hwpx_chapter_and_section_markers_map_to_distinct_levels`.
  HWPX의 고립된 `제1장 운영 지침`을 기존 시험은 chapter로 요구하지만 새 조건은 body로 판정했다.
- 사용자 지시에 따라 장 계열 수정과 해당 신규 시험을 모두 되돌렸다. 기존 시험은 수정하지 않았다.
- 남은 세 계열 수정은 TXT/MD에 한정한다. 장 계열의 고립 번호 오인은 미해결로 남긴다.

## 반영한 변경

- Python 규칙의 TXT/MD 경로에서 로마자(유니코드/영문), 숫자, 가나다 마침표 계열을 확인한다.
- 동일 계열에서 이웃한 두 번호가 1씩 증가한 이력이 있어야 인정한다. 이후 반복·재시작 번호는 기존 번호 체계 시험과 같이 허용한다.
- 확인된 계열의 첫 등장 순서로 chapter/section/topic을 배정한다. 4번째 계열부터 topic을 공유한다.
- 표 칸은 증거에서 제외하고 명시적인 스타일·Markdown 제목을 우선한다. 번호 행이 60자를 넘으면 body다.
- 본문 목록의 보수적 판별: 계열의 첫 행 바로 앞이 일반 본문(번호 후보나 Markdown 제목이 아님)이면 그 계열을 body로 둔다.
- Rust, 공유 marker_scheme, 기존 시험은 수정하지 않았다. HWPX/DOCX의 기존 동작도 유지한다.

## 단계별 검증 및 커밋

실행 Python: `C:/HWP_AUTODOCFIT/v1803/exfile/.venv/Scripts/python.exe -X utf8 -m unittest`.

| 단계 | 실행 모듈 | 시험 수 | 통과 | 실패/오류 | 커밋 |
|---|---|---:|---:|---:|---|
| 변경 전 | tests.test_timemachine_content_search + tests.test_hierarchy_path | 80 | 80 | 0/0 | 7b4c1a6b (조사) |
| 장 시도 | tests.test_timemachine_content_search + 신규 장 시험 | 76 | 75 | 1/0 | 424ee17c (복원 기록) |
| 로마자 | tests.test_timemachine_content_search + tests.test_hs2_d2_numbered_headings | 74 | 74 | 0/0 | b8af7cde |
| 숫자 | 위와 같음 | 76 | 76 | 0/0 | 1d73c4da |
| 가나다 | 위와 같음 | 79 | 79 | 0/0 | b1dfadff |
| 최종 | tests.test_timemachine_content_search + tests.test_hierarchy_path + tests.test_hs2_d2_numbered_headings | 87 | 87 | 0/0 | 본 보고서와 색인 회귀 시험 커밋 |

최종 실행 시간 63.439초. 기존 80개 + 신규 7개, 제외 0개. `git diff --check` 통과.

## 남은 문제와 확인 범위

- 제N장 수정은 회귀 실패로 복원했다. 따라서 고립된 장이 body여야 한다는 요구는 달성하지 못했다.
- TXT/MD는 목록과 제목을 완벽하게 구분할 서식 정보가 없다. 위 보수적 규칙 때문에 일반 본문 바로 뒤에서 처음 등장한 실제 제목도 body가 될 수 있다. 실제 사용자 문서로 확인하지 않았다.
- 기존 색인의 자동 재색인/스키마 변경은 포함하지 않았다. 결과 확인은 새 합성 문서를 신규 색인한 경우다.
- 합성 파일·DB는 E:/docfit-tmp 안에서 생성·정리했다. 사용자 문서, 한/글 COM, Rust 엔진은 실행/변경하지 않았다.
- feature/hs2-hsa에 로컬 커밋만 작성. 푸시·태그·원격 API 호출 없음.

## 변경 후 실제 재색인 결과

| 문서 | 문단 | 실제 level |
|---|---|---|
| consistent | 제1장 시설 관리 | chapter |
| consistent | Ⅰ. 추진 배경 | section |
| consistent | 1. 추진 계획 | topic |
| consistent | 가. 세부 기준 | topic |
| consistent | 나. 추가 기준 | topic |
| consistent | 2. 실행 계획 | topic |
| consistent | Ⅱ. 운영 배경 | section |
| consistent | 제2장 운영 관리 | chapter |
| isolated | 제1장 시설 관리 | chapter |
| isolated | Ⅰ. 추진 배경 | body |
| isolated | 1. 추진 계획 | body |
| isolated | 가. 세부 기준 | body |
| body_list | 시설 관리에 관한 설명입니다. | body |
| body_list | 제1장 시설 관리 | chapter |
| body_list | Ⅰ. 추진 배경 | body |
| body_list | 다음 사항을 확인합니다. | body |
| body_list | 1. 추진 계획 | body |
| body_list | 2. 실행 계획 | body |
| body_list | 가. 세부 기준 | body |
| body_list | 이상입니다. | body |
