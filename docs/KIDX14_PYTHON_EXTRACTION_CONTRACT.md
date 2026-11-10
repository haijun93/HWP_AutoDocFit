# KIDX14 Python OOXML 추출 기준

기준 구현: `docfit_core/timemachine/extractors_ooxml.py`의 `extract_xlsx`, `extract_pptx`. 아래 규칙과 전체 `ExtractionRecord` 필드의 순서·값을 Python 결과의 기준으로 삼는다.

| 형식 | 순회 순서 | 레코드 단위 | 추출 텍스트 | 위치·섹션 | 빈 문서 |
|---|---|---|---|---|---|
| XLSX | 워크북 시트 선언 순서, 각 시트 XML의 행 순서 | 비어 있지 않은 행 하나 | 셀을 XML 순서로 순회해 비어 있지 않은 값을 공백으로 연결. 공유 문자열은 `sharedStrings.xml`의 `<si>` 안 `<t>`를 연결하고 양끝 공백 제거. 인라인 문자열도 `<is>` 안 `<t>`를 연결하고 양끝 공백 제거. 그 밖의 셀은 `<v>` 캐시값을 양끝 공백 제거해 사용(수식은 계산하지 않고 캐시값만). | `location_kind=sheet_row`, `location_label="{시트명} {행번호}행"`, `section=시트명`; 행 번호는 숫자 `r` 속성, 없거나 숫자가 아니면 직전 행 번호+1. sequence는 출력 순서 1부터. | 시트 없음은 `empty: XLSX 워크시트를 찾을 수 없음`; 텍스트 레코드 없음은 `empty: 추출 가능한 XLSX 데이터가 없음`. |
| PPTX | `ppt/slides/slideN.xml`의 N 오름차순, 각 XML 요소의 문서 순서 | 비어 있지 않은 `<p>` 하나 | 해당 문단 요소의 모든 descendant text를 이어 붙이고 양끝 공백 제거. | `location_kind=paragraph`, `location_label="슬라이드 N 문단 M"`, `section="슬라이드 N"`; M은 해당 슬라이드에서 출력한 문단 순서 1부터, sequence는 전체 출력 순서 1부터. | 슬라이드 XML 없음 또는 텍스트 레코드 없음은 `empty` 오류. |

두 형식 모두 Python `_open_zip_safely`의 ZIP 보안 검사와 `defusedxml.ElementTree` 파싱을 기준으로 한다. hierarchy 부여는 추출 결과 바깥의 공용 색인 단계에서 수행한다.
