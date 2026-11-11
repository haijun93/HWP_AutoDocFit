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
