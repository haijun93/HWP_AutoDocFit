# HS2-A 위계 분석 정확도 조사 (2026-11-12)

## 변경 내용

- Markdown과 TXT의 `#`, `##`, `###`를 각각 문서 제목/장, 절, 주제로 분류한다. 최상위 `#`가 하나뿐이면 문서 제목으로 본다. 여러 개면 첫 번째만 문서 제목으로 두고 나머지는 장으로 본다. 파일 안의 첫 명시 제목을 문서 제목으로 취급한다는 일반적인 문서 구조 관례를 적용했으며, 별도 제목이 없는 여러 `#` 문서에서도 첫 장을 문서 제목으로 보존한다.
- DOCX의 `styles.xml`에 선언된 Heading 1~3/제목 1~3 이름을 문단 스타일 ID와 연결한다. Word 기본 스타일 ID(`Heading1` 등)도 처리한다. XML은 `defusedxml`로 읽는다.
- 번호 계열은 `detect_scheme`에서 확인된 계열만 승격한다. 추가로 `(1)`, `①`, `㉠`, `㉮`는 번호 계열 2개 이상이 위계 순서와 들여쓰기 증가를 함께 만족할 때만 장·절·항 체계에 합류시킨다. 고립된 번호는 본문을 유지한다.
- 기존 HWPX/DOCX 스타일 및 outline 우선, 표 칸 최하위, 일반 기호 분류, 기존 검색·breadcrumb 구조는 유지했다.

## 형식별 실제 추출 결과

| 형식/입력 | 실제 추출 레코드 관찰 | 위계 결과 | 조치/한계 |
|---|---|---|---|
| MD 합성 `# 시설물 정비`, `## 관광안내판 철거`, `### 대상 위치` | 줄 단위 레코드 3건 | 문서 제목, 절, 주제 | 수정 및 시험 완료 |
| TXT 합성 동일 표기 | 줄 단위 레코드 3건 | 문서 제목, 절, 주제 | 수정 및 시험 완료 |
| DOCX 저장소 fixture `sample.docx` | 본문 2문단 + 머리글 1문단, 전부 `style_name=None`, `outline_level=None` | 전부 본문 | 실제 fixture에는 Heading 스타일이 없어 본문 유지 |
| DOCX 합성 Heading 1~3 | `styles.xml` 이름과 문단 `pStyle` ID 연결됨 | 장, 절, 주제 | 수정 및 시험 완료 |
| PDF 저장소 fixture `sample.pdf` | 1쪽 텍스트 레코드 1건, `style_name=None`, `outline_level=None` | 본문 | PDF 추출기는 페이지 텍스트만 제공. 실제 제목 메타데이터/서식 구조는 현재 미제공 |
| HWPX 합성 위계 fixture | outline/style 없는 번호 문단 8건 | Ⅰ·Ⅱ=장, 1·2=절, 가·나 및 (1)·(2)=주제 | 번호 계열 순서 근거로 처리. 테스트에서 확인 |
| HWP 바이너리 | 이 작업에서 유효한 HWP fixture를 찾지 못함 | 미확인 | kordoc은 HWP를 Markdown으로 변환하지만 HWP 변환 결과의 제목 의미 정보를 보존하는지는 실제 파일로 확인하지 못함 |

DOCX/PDF fixture는 저장소 외부 `C:/HWP_AUTODOCFIT/v1803/alpha216/tests/fixtures/timemachine_extract/`에서 읽기 전용으로 관찰했다. 사용자 문서나 원본 파일은 수정하지 않았다. 다른 형식의 추출기가 heading style/outline 정보를 제공하지 않으면 위계는 본문으로 남는다. XLSX/PPTX/EML도 현재 추출 계약에서 일반 문단 heading 의미를 제공하지 않는 범위다.

## Rust 엔진과 차이

Rust 분석은 `hierarchy_engine` 설정이 켜져 있을 때만 `analyze_levels`를 통해 사용되므로 기본 동작의 기준은 Python이다. Rust는 문단 역할·문서 위계 family를 동적 계획법으로 공동 추정하고, 들여쓰기·글자 크기·굵기·번호 간격·영역 전환(붙임/참고)을 근거로 confidence와 구조 진단도 만든다. Python은 여기서 보강한 Markdown 헤더 문법과 DOCX `styles.xml`/기본 스타일 ID를 직접 해석하고, 제한된 번호 계열 매핑 규칙을 사용한다. Rust의 본문 크기·강조 기반 무기호 제목 판별 및 confidence/진단은 Python에 없다. 이번 작업에서는 Rust를 수정하지 않았다.

## 검증

- 새 핵심 시험 7건: 통과.
- `tests/test_hierarchy_path.py`: 6건 통과.
- `tests.test_timemachine_content_search` 전체 실행은 환경에서 완료 메시지를 받지 못했다. verbose 실행에서 40여 건은 통과하고 검색 조합 테스트 구간에서 장시간 반환하지 않았다. 마지막 완료된 시험 번호를 확정할 수 없어 전체 통과 숫자는 보고하지 않는다.
- `git diff --check`: 통과.
