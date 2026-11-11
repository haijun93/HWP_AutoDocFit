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
