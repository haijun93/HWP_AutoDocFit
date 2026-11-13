# KFC2 구문(나누기 끄기)·제외어 문서 집합 보고 (2026-11-13)

브랜치 `feature/instant-search`, 크레이트 `kfind/core`. KFC1(`d73a6eea`) 위.

## 한 일
- `SearchOptions.phrases: Vec<String>` — 구문. 비교형(공백·기호 없앰)을 **나누지 않는** 낱말 하나로 검색 낱말에 더한다(AND, 붙어 있는 그대로만 일치). 같은 낱말이 검색어에 있으면 그 낱말의 나누기만 끈다. 비교형 2글자 미만은 `dropped`에, 빈 비교형은 무시. 구문 문법(`"…"`)은 앱(Python `parse_query`)이 해석하고 값만 넘긴다(설계 §2-2).
- `query::add_phrases`(해석 한 곳) + `Index::resolve_with(query, opts)`(검색이 쓰는 해석을 그대로 보여 줌). `search`·`rank`·`count`·`match_sentences`가 같은 해석을 쓴다.
- `Index::doc_set(word) -> RoaringTreemap` — 제외어용: 비교형 낱말이 붙어 있는 그대로 든 문장이 있는 살아 있는 문서 ID. 결과를 `deny`에 합쳐 쓴다(KFC1). 2글자 미만은 `Error::Invalid`.
- 질의 이력 캐시: 구문 낱말은 나누지 않은 낱말과 같은 열쇠라 판정이 같아 그대로 맞다.

## 대조(oracle)
- `tools/oracle.py`: `resolve(…, phrases)`, `doc_set`, 구문·제외어 줄 17가지 × 나누기 3 = 51줄을 범위 표에 더함(상태마다 387줄). 구문만 있는 질의, 검색어 낱말과 같은 구문, 짧은 구문, 기호만 있는 구문, 영문 구문, 구문 2개, 제외어(나눠야만 맞는 낱말 포함)·없는 제외어·검색어와 같은 제외어, 허용·제외 규칙과 함께.
- 기존 대조 표 4개 그대로(`--check` 통과).

## 시험
`cargo test`(kfind-core): 51건 통과(KFC1 뒤 49 + query 단위 1 + `doc_set` 1), 실패 0. 범위 표는 조각 1·20개, 증분(지운 문장), 캐시 켬, 합친 뒤 모두 차이 0.

## 남은 것
- 구문이 문장 경계를 넘는 경우는 찾지 않는다(문장 단위 규칙 그대로).
- 제외어는 나누지 않은 붙어 있는 일치만 본다(앞으로 넓히려면 사용자 결정 필요).
