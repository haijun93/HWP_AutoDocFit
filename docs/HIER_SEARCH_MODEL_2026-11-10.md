# 위계검색 노드 모델·호환 경계

기준: `2b7ec241`. 원본을 읽거나 수정하지 않고 기존 분석 결과를 정규화한다.
입력은 `document_structure.nodes`/`nodes` 또는 추출·색인된 `paragraphs`다.
Rust `feature/klib-index-rust`의 `klib/src/index_content.rs`에서 저장하는
`body, level, paragraph_index, location_label, section` 이름을 그대로 받는다.
별도 브랜치를 병합하거나 그 색인의 위계를 다시 판정하지 않는다.

## 모델

`document → chapter → section → item → paragraph → sentence` 논리 트리다.
없는 장·절은 가짜 제목으로 채우지 않는다. 기존 square/circle/dash는 paragraph
노드 사이의 세부 위계로 보존한다. 문장 offset은 입력 text의 Python 문자 인덱스이며,
구두점 기반 근사이므로 약어·번호에서 분할이 부정확할 수 있다.

페이지는 여러 논리 노드와 교차하므로 문서 아래 별도 page 노드를 두고 `page_ids`로
연결한다. 표도 별도 table 노드를 생성하고 `table_id`로 연결한다. 같은 section의
같은 table_order만 합친다. 표 ID가 없는 table_cell은 위치 미확인 진단을 남긴다.
페이지 번호가 없으면 null이다. `location_label`의 명시적 `N쪽`만 복원한다.
sheet/row/section/paragraph/표 번호 및 원래 라벨을 position에 보존한다.

기존 정수·문자열 ID와 parent_id를 보존하며 합성 노드는 충돌하지 않는 문자열 ID를
쓴다. 중복 ID, 없는 부모, 순환 참조는 ValueError로 거절한다. 입력 딕셔너리는
deep copy로 보존한다. 원문 텍스트·기호·탭·내어쓰기를 바꾸지 않는다.

## 판정 우선순위

1. 기존 nodes의 role/level/parent_id 또는 Rust 색인의 문자열 level이 최우선이다.
2. 새 추정은 기호 + 제목 스타일/개요 수준 또는 인접 문단의 서로 다른 들여쓰기를
   요구한다. 단독 기호는 body다. 표 칸 기호는 새 전역 제목으로 승격하지 않는다.
3. 차이는 `diagnostics`의 existing_hierarchy_preferred에 기존 role/level,
   추정 role과 근거를 남긴다. 기존 엔진 결과를 수정하지 않는다.

문자열 level은 document_title/chapter/section/topic/square/circle/dash/asterisk/body/
table_cell을 지원한다. paragraphs의 숫자 level은 Rust Block 형식(0 본문,
1 장, 2 절, 3 항, 4 square, 5 circle, 6 dash)이다. hierarchy CLI의 Role 열거값은
이 형식과 다르므로 호출 측에서 role 이름으로 변환해야 한다. 기존 nodes의 숫자
level은 원래 문서 구조의 깊이로 유지한다.

## 검색 호환

검색 records는 원래 source 노드만 포함한다. 합성 문장·페이지·표를 중복 후보로
색인하지 않아 기존 입력에서 토큰 수·문서 빈도·점수의 불필요한 변화를 막는다.
기존 `path`, `breadcrumb`은 그대로 두고 문서명이 포함된 경로는 별도 필드로 더한다.
기존 루트 records.parent_id=None도 유지한다. 전체 그래프는 index.nodes에 있다.
등록/변경 시 build_index를 호출하고 검색은 생성된 index만 사용한다. 등록 측의
변경 감지나 SQLite 트랜잭션은 기존 라이브러리의 책임이다.

포맷 파서는 새로 만들지 않는다. HWP/HWPX/PDF/DOCX/XLSX를 포함해 기존 추출기가
내놓는 문단을 공통 모델로 받는다. 실제 바이너리 포맷별 구조 복원·OCR 품질은
이 모델의 합성 회귀 시험만으로 검증됐다고 주장하지 않는다.
