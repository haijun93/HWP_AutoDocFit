# KIDX13: Python `.hwp` 본문 색인 추출 경로

기준 커밋 `c62dc1e9`의 Python 색인 경로는 `ContentSearchIndex.index_file` → `extract_paragraphs` → `extract_document` → `extract_hwp` 순서다. `.hwp`는 HWPX/DOCX 위계 레코드 주입 없이 kordoc 변환 결과를 색인한다.

| 단계 | 코드 | 동작 |
|---|---|---|
| 형식 선택 | `docfit_core/timemachine/content_search.py:SUPPORTED` | `.hwp`를 색인 지원 형식으로 허용한다. |
| 위계 보조 레코드 | `content_search.py:extract_paragraphs` | `hwpx`, `docx`만 `extract_paragraphs_legacy`를 호출한다. `.hwp`에는 전달하지 않는다. |
| 변환 | `docfit_core/timemachine/extractors.py:extract_document` → `extractors_hwp.py:extract_hwp` | kordoc 번들 런타임으로 HWP5를 Markdown으로 변환한다. COM 호출과 임의 네트워크 설치는 하지 않는다. |
| 위치 분할 | `extractors_hwp.py:extract_hwp` | CRLF/CR을 LF로 정규화한다. 결과가 비면 `empty`. 페이지 마커(`form feed`, `<!-- page... -->`)가 있으면 줄 단위 `page` 레코드, 없으면 빈 줄 경계로 분할한 `paragraph` 레코드를 만든다. 공백만 있는 항목은 제외한다. |
| 레코드 위계 | `content_search.py:extract_paragraphs` | `.hwp` 변환기는 기본 메타데이터를 제공하지 않으므로 본문은 공통 위계 분류기에 전달되고, 위치 라벨은 변환기가 만든 페이지/문단 순서로 보존된다. |
| 색인 | `ContentSearchIndex.index_file` | 레코드 텍스트를 문단 FTS와 전체 본문 FTS에 저장하고, 날짜/검색 보조 인덱스를 갱신한다. |

비교 기준은 변환 출력 레코드의 텍스트, 순서, 위치 종류/라벨, 분류 위계다. kordoc 런타임 부재·시간 초과·암호화·손상·빈 본문은 각 정규화 오류로 반환한다. 기준값을 완화하지 않는다.
