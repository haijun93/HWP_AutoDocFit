# HWP·PDF 위계 추출 격차 (2026-11-18)

## 조사 범위

TODO의 “HWP·PDF 위계 추출(지금은 본문만), hwpx 본문 안 제N장 판정”을 기준으로
`docfit_core/pdf_extract.py`, `docfit_core/style_hierarchy.py`,
`docfit_core/timemachine/{extractors.py,extractors_hwpx.py,extractors_hwp.py,extractors_pdf.py,content_search.py}`와
`hierarchy/src`의 검색·호환 계층을 확인했다. Rust 계층은 현재 레코드의 위계 변환과 검색용 분석을 제공하지만,
PDF 페이지 텍스트를 문단·줄로 나누는 역할은 하지 않는다.

## 구현 여부

| 항목 | 상태 | 근거 및 남은 일 |
|---|---|---|
| HWPX 본문 `제N장` 판정 | 구현됨 | HWPX 본문 문단을 추출한 뒤 `classify_paragraph_levels`가 `제1장` 제목을 chapter로 판정한다. 기존 합성 HWPX 시험 `test_hwpx_chapter_and_section_markers_map_to_distinct_levels`가 이를 고정한다. |
| HWP5 본문 텍스트 위계 | 일부 구현 | Rust `klib` 우선 추출 및 kordoc 대체 경로가 텍스트·일부 outline/style 정보를 제공한다. 검색 분류기는 추출된 문단에 공통 위계 분류를 적용한다. 실제 HWP 파일/COM에 의존하지 않는 추출 위계의 폭넓은 실문서 검증은 별도 과제다. |
| PDF 텍스트 위계 | 기존 미구현, 이번에 부분 구현 | PDF 추출기가 한 페이지 텍스트 전체를 한 레코드로 반환해 번호 체계의 연속성과 순서를 볼 수 없었다. 줄 단위 레코드와 기존 번호 판정기를 연결해 연속 번호 체계의 장·절·주제 단계를 추정한다. 글꼴·좌표·목차·페이지 레이아웃을 이용한 PDF 구조 복원은 미구현이다. |
| Rust 위계 엔진의 PDF 레이아웃 처리 | 미구현 | hierarchy 엔진은 전달받은 레코드·스타일/들여쓰기 정보를 분류한다. PDF 좌표나 글꼴 메타데이터를 추출해 엔진 입력으로 만드는 경로는 없다. |

## 이번 변경

- PDFium의 텍스트를 줄 단위 레코드로 반환하되 각 줄의 실제 페이지 위치와 문서 순서를 보존한다.
- PDF 입력도 기존 연속 번호 판정(`Ⅰ./Ⅱ.` → 장, `1./2.` → 절, `가./나.` → 주제)에 전달한다.
- PDF 레이아웃 기반 추정, 스캔 PDF OCR 위계, 실 HWP 문서 표본 검증은 남은 격차다.

## 검증

합성 PDFium 입력으로 줄 분리·쪽 위치를 확인하고, 합성 PDF 레코드로 번호 체계의 장·절·주제 판정을 확인한다. HWPX `제1장` 및 절 표시는 기존 합성 입력 시험으로 확인한다.
