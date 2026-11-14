# HS2-F 위계 기반 검색 사례 조사 보고서

조사일: 2026-11-12  
범위: 공개 공식 문서·저장소에서 확인되는 구조와 현재 구현의 설계 적합성. 성능 비교는 하지 않았다.

## 1. 사례별 비교

| 사례 | 방식 | 라이선스 | 오프라인 가능 | 우리에게 맞는 점 | 안 맞는 점 |
|---|---|---|---|---|---|
| PageIndex | 문서를 계층 트리로 구성하고 LLM 추론으로 트리를 탐색. 공식 저장소는 벡터 DB·청킹 없는 검색이라고 설명한다. | MIT | **부분 가능**: 공식 README의 현재 로컬 모드는 로컬 인덱싱을 지원하지만, 인덱스/검색 모델 호출은 LLM 제공자·키에 의존. 폐쇄망 보장은 미확인. | 문서 구조를 중심으로 찾고 출처 경로를 설명하는 설계 참고. | 외부 모델 의존이 기본 전제이며 모델을 완전히 로컬에서 쓸 수 있는지 공식 안내에서 미확인. 오프라인 기본 경로에는 부적합. |
| Docling (`HierarchicalChunker`, `HybridChunker`) | Docling 문서 구조를 바탕으로 요소를 청킹하고 제목·캡션 등 문맥 메타데이터를 붙인다. Hybrid는 토크나이저 기준으로 큰 조각을 분할하고 같은 제목의 작은 연속 조각을 합친다. | MIT | **가능하나 조건부**: 로컬 패키지 실행 가능. 첫 모델 다운로드 필요 여부와 모델별 오프라인 배포 조건은 이 조사에서 공식 문서로 특정하지 못함. | 구조/제목 보존, 크기 기반 분할·형제 조각 병합은 현 검색에 적용 가능한 설계 패턴. | 문서 변환 모델 및 토크나이저 운영·배포 복잡도가 늘 수 있음. 현재 시스템에 필요한 범위를 넘을 수 있음. |
| LlamaIndex (`HierarchicalNodeParser`, `AutoMergingRetriever`) | 부모·자식 노드 계층을 만들고 검색된 자식 노드 중 같은 부모 아래 충분한 비율이 회수되면 부모 노드로 병합해 반환하는 패턴. | MIT (llama-index-core) | **코어는 로컬 사용 가능**; 실제 임베딩/LLM/스토어는 선택한 통합에 따라 달라짐. | 여러 형제 문단이 동시에 검색될 때 부모 절을 함께 보여주는 UX 아이디어. | 벡터 검색·노드 그래프·병합 임계값 관리가 추가된다. PM 제시의 기본 2048/512/128 토큰 설정은 현재 공식 문서에서 확인하지 못함. |
| Azure AI Search Document Layout skill | 레이아웃 모델이 제목 구조를 Markdown 형태로 인식하고, Text Split이 각 Markdown 구역 안에서 크기를 제한해 청킹한다. | 관리형 서비스(소스 라이선스 해당 없음) | **아니오**: Azure 서비스 의존. 클라우드 처리·과금. | 제목 경계를 우선하고 구역 안에서 크기를 제한하는 분리 방식 참고. | 클라우드 서비스·과금 및 문서 외부 처리 의존. Microsoft는 새 skillset에 Content Understanding skill 사용을 안내한다. |
| Google Document AI Layout Parser | 레이아웃 파싱·청킹 설정에 `includeAncestorHeadings`를 두어 조상 제목(최대 두 단계)을 조각에 포함할 수 있다. | 관리형 서비스(소스 라이선스 해당 없음) | **아니오**: Google Cloud Document AI 서비스 호출 필요. | 검색 조각에 상위 제목을 함께 넣는 간단하고 직접적인 문맥 보강 패턴. | 클라우드 의존·과금. 로컬 검색 구현에 서비스 자체를 도입할 이유는 낮음. |

## 2. 우리 구현과 대응

현재 검색 인덱스는 문단 단위로 위계 순위(`level_rank`)를 저장한다. [hierarchy_path.py](../docfit_core/timemachine/hierarchy_path.py)는 순위가 더 높은(숫자가 낮은) 앞선 문단을 조상으로 찾아 문단의 상위 경로를 만든다. 제목 경로는 검색 결과의 `breadcrumb` 및 `breadcrumb_text`로 제공되며, 결과 정렬은 위계 우선 정렬 모드에서 최상위 일치 위계, 전체 일치 수, 날짜, BM25 순서로 적용된다([content_search.py](../docfit_core/timemachine/content_search.py)).

이는 문서의 구조를 이미 추출해 검색 결과에 설명 가능한 경로를 연결한다는 점에서 Docling의 구조 메타데이터, Google의 조상 제목 포함과 닮았다. 다만 현재 확인한 구현은 검색 결과에 경로를 추가하는 것이며, 경로를 색인 본문에 합쳐 검색 recall을 높이거나 형제 문단을 부모 절로 합쳐 반환하는 동작은 확인되지 않았다. LlamaIndex식 자동 병합과는 별도 단계다.

## 3. 도입 권고 순위 (설계 적합성)

1. **조상 제목을 검색용 텍스트에 포함하는 방안 검토** — Google식 문맥 보강. 기존 breadcrumb 계산 결과를 색인 또는 검색 시 텍스트에 반영하면, 제목을 직접 포함하지 않는 본문 문단도 장·절 용어로 검색될 가능성이 있다. 색인 저장량과 점수 편향은 확인이 필요하다.
2. **형제 문단 다수가 함께 걸릴 때 부모 절을 묶어 보여 주는 방안 검토** — LlamaIndex식 응집 결과. 원문 문단 결과와 부모 절 문맥을 보존하고, 묶음 기준과 중복 제거를 정해야 한다.
3. **구조 우선 후 구역 내 크기 제한** — Docling/Azure식 패턴은 긴 문단·표 등 검색 조각이 커질 때 선택적으로 참고한다. 현재 문단 단위 구조가 요구를 충족하는지 먼저 확인한다.
4. **벡터 검색·LLM 기반 트리 추론은 기본 경로에서 제외** — 폐쇄망·오프라인 원칙에 맞지 않을 가능성이 있고, 현재의 규칙 기반 위계·BM25 검색보다 운영 요소가 추가된다. 완전 로컬 모델 경로가 별도 요구로 생기면 재검토한다.

## 4. 미확인 항목

- PageIndex의 로컬 모드가 모델 제공자 없이 완전 폐쇄망에서 인덱싱·검색 가능한지, 로컬 모델 지원 범위.
- Docling 첫 실행 시 모델 다운로드가 필수인지, 이미 받은 모델을 통한 완전 오프라인 재실행 절차.
- LlamaIndex 최신 공식 API에서 기본 계층 토큰 크기·중첩 설정이 2048/512/128인지. 이 수치는 고정 기본값으로 인용하지 않았다.
- AutoMergingRetriever의 최신 API에서 병합 임계값과 반환 노드 원문 범위의 구체 기본값.
- 우리 검색에서 조상 제목 텍스트 색인 시 재현율·정밀도 및 순위 편향이 어떻게 바뀌는지.
- 부모 절 병합 표시가 기존 검색 UI의 중복·정렬·페이지 위치 표시에 미치는 영향.
- Docling 및 각 클라우드 서비스의 모델 다운로드·처리 요금은 실제 구성·지역·사용량에 따라 달라 구체액 미확인.

## 출처

- PageIndex 공식 저장소 README 및 라이선스: https://github.com/VectifyAI/PageIndex
- Docling 공식 청킹 문서: https://docling-project.github.io/docling/concepts/chunking/ ; 저장소 라이선스: https://github.com/docling-project/docling/blob/main/packages/docling/pyproject.toml
- LlamaIndex 공식 코어 저장소 라이선스: https://github.com/run-llama/llama_index/blob/main/llama-index-core/pyproject.toml
- Azure AI Search 공식 Document Layout 청킹 문서: https://learn.microsoft.com/en-us/azure/search/search-how-to-semantic-chunking ; 새 skill 안내: https://learn.microsoft.com/en-us/azure/search/multimodal-search-overview
- Google Document AI 공식 ProcessOptions: https://docs.cloud.google.com/document-ai/docs/reference/rest/v1/ProcessOptions ; Layout Parser quickstart: https://cloud.google.com/document-ai/docs/layout-parse-quickstart
- 본 저장소 코드: `docfit_core/timemachine/hierarchy_path.py`, `docfit_core/timemachine/content_search.py` (조사일 현재 브랜치).
