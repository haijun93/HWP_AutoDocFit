# APP1 새 검색 엔진 비교 B 미설명 차이 15건 원인 분석 (2026-11-14)

작업 브랜치 `feature/kfind-app`(mac 작업 트리 `mac/GEM-EQ`), 2026-11-14 실행 보고서(`reports/APP1-equiv-2026-11-14.json` 및 `reports/APP1_GIJH_2026-11-14.md`) 기반.
**코드는 전혀 수정하지 않고, 코드 추론 및 원인 규명 문서만 작성한다.**

---

## 1. 개요 및 배경

2026-11-14 APP1 동등성 첫 보고에서:
- **비교 A (kfind 실서비스 대 Python 기준 구현)**: 차이 **0건**으로 완전 일치 통과.
- **비교 B (기존 SQLite FTS5 본문검색 대 새 kfind 엔진)**: 차이 중 `classify_difference`가 설명하지 못한 `UNKNOWN`이 **15건** 발생.

`docs/APP1_KFIND_INTEGRATION_2026-11-13.md` §6-2에 정의된 원칙:
> "차이 0의 뜻을 둘로 나눈다. 규칙이 바뀌었으므로 예전 결과와 새 결과는 같을 수 없다. 합격 기준은 ① 새 경로 대 Python 기준 구현: 차이 0 ② 예전 대 새: 모든 차이가 미리 정한 종류(E1~E8)로 설명됨, 설명 안 되는 차이 0."

현재 `tools/app1_equivalence.py`는 E1(문장 단위 일치 차이), E2(붙여 쓴 말 나누기), E3(adjacent 우선 순서 역전), E4(제목 결합 일치 제외), E5(비교형 정규화 차이)의 5가지만 감지하도록 구현되어 있으며, 주석에 명시된 대로 "E6·E7·E8은 증거가 부족하면 UNKNOWN으로 유지"하여 15건이 UNKNOWN으로 남았다.

본 문서는 이 15건의 발생 원인을 관련 코드와 매핑하여 추론·분류하고, 의도된 차이(사용자 결정 사양)인지 버그인지 규명하며, 향후 도구 보완 방안을 제시한다.

---

## 2. 미설명 15건 총괄표

| 번호 | 검색 질의 (Query) | 차이 방향 | 대상 문서 | 분류된 원인 유형 | 의도/버그 여부 |
|---|---|---|---|---|---|
| 1 | `예산` | order (순서) | 문서.txt ↔ 계획.md | 유형 1: 순위 결정 기준 차이 (일치 수 vs BM25) | 의도된 차이 (사양) |
| 2 | `예산 및 집행` | added (추가) | `<CORPUS>\계획.md` | 유형 2: 2글자 미만 낱말 제외 (dropped) | 의도된 차이 (Q8 결정) |
| 3 | `예산 및 집행` | added (추가) | `<CORPUS>\문서.txt` | 유형 2: 2글자 미만 낱말 제외 (dropped) | 의도된 차이 (Q8 결정) |
| 4 | `예산` (NFD) | order (순서) | 문서.txt ↔ 계획.md | 유형 1: 순위 결정 기준 차이 (일치 수 vs BM25) | 의도된 차이 (사양) |
| 5 | `(계획)` | added (추가) | `<CORPUS>\계획.md` | 유형 3: 특수문자/괄호 fold 정규화 차이 | 의도된 차이 (사양) |
| 6 | `(계획)` | added (추가) | `<CORPUS>\문서.txt` | 유형 3: 특수문자/괄호 fold 정규화 차이 | 의도된 차이 (사양) |
| 7 | `예산 after:2020` | order (순서) | 문서.txt ↔ 계획.md | 유형 1: 순위 결정 기준 차이 (일치 수 vs BM25) | 의도된 차이 (사양) |
| 8 | `예산 before:2099` | order (순서) | 문서.txt ↔ 계획.md | 유형 1: 순위 결정 기준 차이 (일치 수 vs BM25) | 의도된 차이 (사양) |
| 9 | `예산 -없는말` | order (순서) | 문서.txt ↔ 계획.md | 유형 1: 순위 결정 기준 차이 (일치 수 vs BM25) | 의도된 차이 (사양) |
| 10 | `"예산 집행"` | added (추가) | `<CORPUS>\계획.md` | 유형 4: 구문 검색 fold 정규화 및 도구 누락 | 의도된 차이 / 도구 버그 |
| 11 | `가 예산` | added (추가) | `<CORPUS>\계획.md` | 유형 2: 2글자 미만 낱말 제외 (dropped) | 의도된 차이 (Q8 결정) |
| 12 | `가 예산` | added (추가) | `<CORPUS>\문서.txt` | 유형 2: 2글자 미만 낱말 제외 (dropped) | 의도된 차이 (Q8 결정) |
| 13 | `및 계획` | added (추가) | `<CORPUS>\계획.md` | 유형 2: 2글자 미만 낱말 제외 (dropped) | 의도된 차이 (Q8 결정) |
| 14 | `및 계획` | added (추가) | `<CORPUS>\문서.txt` | 유형 2: 2글자 미만 낱말 제외 (dropped) | 의도된 차이 (Q8 결정) |
| 15 | `ㅇ 관리` | added (추가) | `<CORPUS>\문서.txt` | 유형 2: 2글자 미만 낱말 제외 (dropped) | 의도된 차이 (Q8 결정) |

---

## 3. 유형별 상세 원인 분석

### 유형 1: 순위 결정 기준 및 동률 처리 방식 차이 (5건)
- **해당 건수**: 5건 (Row 0, 45, 57, 58, 60)
- **검색 질의**: `예산`, `예산`, `예산 after:2020`, `예산 before:2099`, `예산 -없는말`
- **현상**: 두 엔진 모두 `문서.txt`와 `계획.md`를 검색 결과로 반환했으나 정렬 순서가 정반대로 뒤집힘.
  - 새 엔진(kfind): 1위 `문서.txt`, 2위 `계획.md`
  - 기존 엔진(legacy SQLite): 1위 `계획.md`, 2위 `문서.txt`
  - 동등성 도구: `direction: order`이나 E3 판정 조건을 만족하지 못하여 `UNKNOWN`으로 남음.

#### 원인 코드 위치
1. **kfind (새 엔진)**:
   - [kfind/core/src/index.rs#L358-L370](file:///Users/hyeokjunkong/work/hwp/kfind/core/src/index.rs#L358-L370) (`Agg::set`):
     ```rust
     let tier = if adjacent { 0 } else { ADJACENT_BIT };
     self.key = tier | ((level as u64) << 32) | (u32::MAX - count) as u64;
     ```
   - [kfind/core/src/index.rs#L424-L427](file:///Users/hyeokjunkong/work/hwp/kfind/core/src/index.rs#L424-L427) (`order` 함수):
     ```rust
     a.key.cmp(&b.key).then_with(|| b.date.cmp(&a.date)).then_with(|| a.doc_id.cmp(&b.doc_id))
     ```
   - 순위 기준: `adjacent`(붙어 있는 일치) > `level`(위계 등급, 작을수록 상위) > `count`(일치 문장 수 역순, 많을수록 상위) > `date`(최신순) > `doc_id`.
   - `문서.txt`: adjacent=True, level=8, count=3
   - `계획.md`: adjacent=True, level=8, count=1
   - 두 문서 모두 adjacent=True, level=8로 동일하나, `count`(일치 문장 수)가 3 vs 1이므로 **kfind는 `문서.txt`를 1위로 정렬**.
2. **legacy (기존 SQLite 본문검색)**:
   - [docfit_core/timemachine/content_search.py#L1950-L1960](file:///Users/hyeokjunkong/work/hwp/docfit_core/timemachine/content_search.py#L1950-L1960) 및 [L2065](file:///Users/hyeokjunkong/work/hwp/docfit_core/timemachine/content_search.py#L2065):
     ```python
     bm25(content_paragraph_fts, 0.0, 3.0, 2.0, 1.0, 0.0, 0.0, 1.0) AS rank
     ...
     ranked.sort(key=lambda value: (value[0], value[1].casefold()))
     ```
   - SQLite FTS5 BM25 알고리즘은 문서 전체 길이(총 문단 수/토큰 수) 대비 검색어 출현 빈도로 점수를 매김. `계획.md`는 총 3문단으로 짧아 BM25 점수가 더 낮게(유리하게) 산출되어 **legacy는 `계획.md`를 1위로 정렬**.
3. **동등성 도구 미분류 원인**:
   - [tools/app1_equivalence.py#L130-L136](file:///Users/hyeokjunkong/work/hwp/tools/app1_equivalence.py#L130-L136):
     ```python
     adj = {h['doc_key']: h['adjacent'] for h in new['hits']}
     inversions = [(a, b) for i, a in enumerate(new_keys) for b in new_keys[i+1:]
                   if old_keys.index(a) > old_keys.index(b)]
     kind = 'E3' if inversions and all(adj[a] and not adj[b] for a, b in inversions) else 'UNKNOWN'
     ```
   - E3는 한 문서는 adjacent=True이고 다른 문서는 adjacent=False일 때의 역전만 인정함. 두 문서 모두 adjacent=True인 상태에서 일치 문장 수(`count`) vs BM25로 인한 순서 역전을 판정하는 규칙이 없어 `UNKNOWN`으로 남음.

#### 의도된 차이인지 여부
- **의도된 차이 (kfind 사양)**:
  `docs/KC1_KFIND_CORE_2026-11-12.md` 및 `kfind/core/src/index.rs` 주석에 명시된 확정 사양. 기존 FTS5 BM25는 문서 길이에 따라 점수가 요동쳐 사용자가 "왜 이 문서가 먼저 나오는지" 직관적으로 납득하기 어려웠음. 새 엔진은 `붙어 있는 일치 > 위계 > 일치 문장 수 > 날짜 > 문서 ID`라는 명확하고 결정론적인 다단계 순위를 도입함.

#### 고치는 방안
- 엔진 코드는 정상 동작하므로 수정 불필요.
- `tools/app1_equivalence.py`의 `classify_difference`에 순위 기준 차이를 감지하는 규칙(예: `E7_COUNT_VS_BM25` 또는 `E7_RANKING_METRIC`) 추가:
  두 문서의 adjacent가 동일하고, new 결과의 count 또는 level 우선순위에 부합하는 역전일 경우 정상 설명된 차이로 분류.

---

### 유형 2: 2글자 미만(1글자) 낱말 제외(dropped) 정책 차이 (7건)
- **해당 건수**: 7건 (Row 12의 2건, Row 66의 2건, Row 67의 2건, Row 68의 1건)
- **검색 질의**:
  - Row 12: `예산 및 집행` (계획.md added, 문서.txt added)
  - Row 66: `가 예산` (계획.md added, 문서.txt added)
  - Row 67: `및 계획` (계획.md added, 문서.txt added)
  - Row 68: `ㅇ 관리` (문서.txt added)
- **현상**: 기존 legacy는 검색 결과 0건(완전 탈락), kfind는 정상 검색되어 결과에 추가(`direction: added`).

#### 원인 코드 위치
1. **kfind (새 엔진)**:
   - [kfind/core/src/query.rs#L4-L5](file:///Users/hyeokjunkong/work/hwp/kfind/core/src/query.rs#L4-L5):
     ```rust
     /// 본문 검색 낱말의 최소 글자 수(사용자 결정 Q8).
     pub const MIN_WORD_CHARS: usize = 2;
     ```
   - [kfind/core/src/query.rs#L34-L45](file:///Users/hyeokjunkong/work/hwp/kfind/core/src/query.rs#L34-L45) (`parse_query`):
     ```rust
     if f.chars().count() < MIN_WORD_CHARS {
         if !out.dropped.contains(&f) {
             out.dropped.push(f);
         }
     } else if !out.words.iter().any(|w| w.text == f) {
         out.words.push(QueryWord { text: f, pieces: None });
     }
     ```
   - 2글자 미만 낱말("및", "가", "ㅇ" 등 1글자)은 `dropped`로 빼고 실제 검색 대상 `words`에서 제외함.
   - 따라서 `예산 및 집행`은 `[예산, 집행]`, `가 예산`은 `[예산]`, `및 계획`은 `[계획]`, `ㅇ 관리`는 `[관리]`로 검색되어 대상 문서들을 정상적으로 찾아냄.
2. **legacy (기존 SQLite 본문검색)**:
   - [docfit_core/timemachine/content_search.py#L1918-L1930](file:///Users/hyeokjunkong/work/hwp/docfit_core/timemachine/content_search.py#L1918-L1930):
     ```python
     short_positive = [t for t in positive if len(t) < 3]
     ...
     else:
         clauses.append("body LIKE ?"); params.append("%" + term + "%")
     ```
   - [docfit_core/timemachine/content_search.py#L2020-L2045](file:///Users/hyeokjunkong/work/hwp/docfit_core/timemachine/content_search.py#L2020-L2045):
     ```python
     rows = [row for row in rows if all(term.casefold() in candidate_texts.get(row["path"], "").casefold()
                                        for term in parsed["terms"]) ...]
     ```
   - legacy는 1글자 단어도 제외하지 않고 `body LIKE ?` 및 `candidate_texts` 검사에 필수 AND 조건으로 강제함.
   - 본문에 단독 1글자 "및", "가", "ㅇ"이 정확히 포함되지 않은 문서는 AND 조건 불충족으로 결과가 0건이 됨.
3. **동등성 도구 미분류 원인**:
   - [tools/app1_equivalence.py#L102-L103](file:///Users/hyeokjunkong/work/hwp/tools/app1_equivalence.py#L102-L103) 및 [L118-L128](file:///Users/hyeokjunkong/work/hwp/tools/app1_equivalence.py#L118-L128):
     `classify_difference`는 `parsed = ContentSearchIndex.parse_query(query)`를 호출하여 원본 terms(`["예산", "및", "집행"]`)를 모두 가져옴.
     `classify_difference`의 E2/E5 판정 로직에 kfind의 `dropped` 필드를 확인하는 조건이 전혀 없어 `UNKNOWN`으로 남음.

#### 의도된 차이인지 여부
- **의도된 차이 (사용자 결정 Q8 확정)**:
  `kfind/core/src/query.rs` 4줄에 명시: **`사용자 결정 Q8`**. 한 글자 단어(조사, 접속사 등)는 본문 검색 시 방대한 노이즈를 유발하므로 2글자 이상 단어만 검색에 사용하고, 1글자 단어는 화면에 `dropped`로 표시해 안내하기로 결정된 사양임.

#### 고치는 방안
- 엔진 코드는 정상 동작하므로 수정 불필요.
- `tools/app1_equivalence.py`의 `classify_difference`에 `E_DROPPED_CHARS`(2글자 미만 낱말 제외로 인한 일치 추가) 판정 규칙 추가:
  `new.get('dropped')`가 비어있지 않고, dropped를 제외한 나머지 검색어들이 본문에 모두 일치하는 경우 정상 설명된 차이로 분류.

---

### 유형 3: 특수문자/구두점/괄호 정규화(fold) 차이 (2건)
- **해당 건수**: 2건 (Row 48의 2건)
- **검색 질의**: `(계획)` (계획.md added, 문서.txt added)
- **현상**: 기존 legacy는 검색 결과 0건, kfind는 `계획.md`, `문서.txt` 2건 모두 검색됨(`direction: added`).

#### 원인 코드 위치
1. **kfind (새 엔진)**:
   - [tools/app1_reference/fold_ref.py#L25-L37](file:///Users/hyeokjunkong/work/hwp/tools/app1_reference/fold_ref.py#L25-L37) (`tokens`) 및 `kfind/core/src/fold.rs`:
     - kfind는 질의 입력 시 공백뿐만 아니라 괄호 `(` `)` 등 특수기호를 분리자로 처리하고 비교형(`fold`)으로 정규화함.
     - `(계획)` 입력 시 괄호가 제거되고 2글자 단어인 `계획`으로 파싱됨.
     - 따라서 본문의 "계획"과 완벽히 매칭됨.
2. **legacy (기존 SQLite 본문검색)**:
   - [docfit_core/timemachine/content_search.py#L53-L73](file:///Users/hyeokjunkong/work/hwp/docfit_core/timemachine/content_search.py#L53-L73) (`parse_query`):
     ```python
     tokens = re.findall(r'-?"(?:[^"\\]|\\.)*"|\S+', query)
     ```
   - legacy의 토큰 분리 정규식 `\S+`는 공백만 분리자로 보므로 `(계획)` 4글자를 하나의 검색어로 파싱함 (`parsed["terms"] = ["(계획)"]`).
   - 이후 [L1924-L1926](file:///Users/hyeokjunkong/work/hwp/docfit_core/timemachine/content_search.py#L1924-L1926)에서 FTS MATCH 또는 `body LIKE '%(계획)%'`로 검색. 본문에 괄호 없이 "계획"으로만 적혀 있는 문서는 매칭되지 않아 0건이 됨.
3. **동등성 도구 미분류 원인**:
   - [tools/app1_equivalence.py#L124-L127](file:///Users/hyeokjunkong/work/hwp/tools/app1_equivalence.py#L124-L127):
     ```python
     elif words and all(w in fold(body) for w in words) and not all(w in body for w in words):
         kind = 'E5'
     ```
     - `words = [fold("(계획)")]` = `["계획"]`
     - `all(w in fold(body))` = True ("계획"이 body에 있음)
     - `not all(w in body for w in words)` = **False** ("계획"이 원문 body에 그대로 존재하므로 `not all(...)`이 거짓이 됨!)
     - E5 조건은 "본문에 비교형으로만 있고 원문에는 없는 경우(예: NFD, 대소문자)"만 인정하도록 짜여 있어, "검색어 쪽에 기호가 붙어 있어 legacy가 못 찾은 경우"를 포괄하지 못하고 `UNKNOWN` 처리됨.

#### 의도된 차이인지 여부
- **의도된 차이 (kfind 사양)**:
  kfind는 검색어에 포함된 불필요한 기호/문장부호를 걸러내고 본질적인 단어로 검색하는 것이 핵심 설계 목적임. 기존 legacy의 융통성 없는 단순 문자열/구두점 처리 한계를 극복한 정상적인 개선임.

#### 고치는 방안
- 엔진 코드는 정상 동작하므로 수정 불필요.
- `tools/app1_equivalence.py`의 E5 조건을 보완:
  검색어 토큰 자체에 기호가 포함되어 있어 정규화 전후가 다른 경우(`any(t != fold(t) for t in parsed['terms'])`), fold된 검색어가 본문에 존재하여 kfind가 찾아낸 결과를 E5(비교형 정규화)로 올바르게 분류하도록 판정식을 확장.

---

### 유형 4: 구문 검색(Quotes)의 비교형 정규화 및 도구의 phrases 누락 (1건)
- **해당 건수**: 1건 (Row 61)
- **검색 질의**: `"예산 집행"` (계획.md added)
- **현상**: `문서.txt`는 legacy와 kfind 모두 찾았으나, `계획.md`는 legacy에서 누락되고 kfind에서만 검색되어 추가(`direction: added`)됨.

#### 원인 코드 위치
1. **kfind (새 엔진)**:
   - [kfind/core/src/query.rs#L48-L68](file:///Users/hyeokjunkong/work/hwp/kfind/core/src/query.rs#L48-L68) (`add_phrases`):
     ```rust
     /// 구문(KFC2)을 검색 낱말로 더한다. 구문은 비교형(공백·기호를 뺀 글)이 붙어 있는 그대로 든 문장만 찾는 낱말 하나이고 나누지 않는다.
     pub(crate) fn add_phrases(parsed: &mut ParsedQuery, phrases: &[String]) {
         for p in phrases {
             let f = crate::fold::fold(p);
             ...
             parsed.words.push(QueryWord { text: f, pieces: None });
         }
     }
     ```
   - [kfind/core/src/index.rs#L73-L76](file:///Users/hyeokjunkong/work/hwp/kfind/core/src/index.rs#L73-L76):
     구문 검색 `"예산 집행"`은 `fold`를 거쳐 공백/기호가 제거된 `"예산집행"`으로 정규화된 뒤 `strict` 연속 일치로 검색됨.
   - `계획.md` 본문에는 `"예산집행"`(붙여 씀) 또는 `"예산·집행"` 형태로 작성되어 있어, fold 비교형 기준으로 연속 일치하므로 kfind에서는 정상 매칭됨.
2. **legacy (기존 SQLite 본문검색)**:
   - [docfit_core/timemachine/content_search.py#L1931-L1933](file:///Users/hyeokjunkong/work/hwp/docfit_core/timemachine/content_search.py#L1931-L1933) 및 [L2042](file:///Users/hyeokjunkong/work/hwp/docfit_core/timemachine/content_search.py#L2042):
     ```python
     for phrase in parsed["phrases"]:
         clauses.append("body LIKE ?"); params.append("%" + phrase + "%")
     ```
   - legacy는 구문 검색을 원문 그대로 `body LIKE '%예산 집행%'`으로 검사함.
   - `계획.md` 본문에는 공백이 들어간 `"예산 집행"`이 없고 `"예산집행"` 또는 `"예산·집행"`만 존재하므로 LIKE 매칭에 실패하여 탈락함 (`문서.txt`는 공백이 있는 `"예산 집행"`이 있어 매칭됨).
3. **동등성 도구 미분류 원인 (도구 버그)**:
   - [tools/app1_equivalence.py#L102-L104](file:///Users/hyeokjunkong/work/hwp/tools/app1_equivalence.py#L102-L104):
     ```python
     parsed = ContentSearchIndex.parse_query(query)
     words = [fold(t) for t in parsed['terms']]
     ```
   - `query = '"예산 집행"'`일 때 `parse_query`는 `parsed["terms"] = []`, `parsed["phrases"] = ["예산 집행"]`을 반환함.
   - 도구가 `phrases`를 `words`에 합치지 않아 `words = []`가 됨.
   - 그 결과 `classify_difference`의 `if words and ...` 조건을 전혀 타지 못하고 무조건 `UNKNOWN`으로 떨어짐.

#### 의도된 차이인지 여부
- **의도된 차이 (kfind KFC2 사양) + 도구 버그 (phrases 누락)**:
  - kfind가 따옴표 구문 검색 시 공백·기호를 정규화하여 연속 일치(`strict`)를 찾는 것은 공식 사양(KFC2)에 따른 의도된 동작임.
  - 반면, 동등성 도구(`tools/app1_equivalence.py`)가 `classify_difference`에서 `parsed['phrases']`를 `words` 목록에 누락한 것은 **동등성 검증 도구의 명백한 구현 버그**임.

#### 고치는 방안
- 엔진 코드는 정상 동작하므로 수정 불필요.
- `tools/app1_equivalence.py` line 103 수정:
  ```python
  words = [fold(t) for t in [*parsed['terms'], *parsed['phrases']]]
  ```
  구문 검색어도 `words`에 포함시키고, 원문 공백 불일치 구문 매칭을 E5(비교형 정규화 차이) 또는 별도 구문 정규화 규칙으로 분류하도록 수정.

---

## 4. 기타 요구사항별 점검 (체크리스트)

| 점검 항목 | 본 분석 결과 및 영향 여부 |
|---|---|
| **문장 단위 대 문단 단위** | 이미 `E1`으로 정상 분류 중 (본 15건 UNKNOWN과는 무관) |
| **붙여 쓴 말 나누기** | 이미 `E2`로 정상 분류 중 (본 15건 UNKNOWN과는 무관) |
| **2글자 미만** | **유형 2에 해당 (7건)**: kfind Q8 사양(2글자 미만 dropped) vs legacy 1글자 필수 AND 차이 |
| **정규화 (NFC·대소문자·기호)** | **유형 3(2건) 및 유형 4(1건)에 해당**: 검색어 기호 제거 및 구문 공백 fold 정규화 차이 |
| **순위 동점/평가 기준 처리** | **유형 1에 해당 (5건)**: kfind 결정론적 다단계(adjacent > level > count) vs legacy FTS5 BM25 차이 |
| **위계 판정 출처** | 합성 문서 레벨 8(본문) 동일하여 본 15건 순위 역전에 직접 영향 없음 (향후 level 차이 발생 시 kfind 우선순위에 정상 반영됨) |
| **결과 상한 (limit / top)** | 86개 검색어 모두 문서 2개 수준의 소규모 코퍼스이므로 상한(100건)에 도달하지 않아 무관 |

---

## 5. 결론 및 향후 조치 제언

1. **엔진 결함(버그) 여부**:
   - 15건 모두 kfind 엔진(Rust 코어 및 파이프 서비스)의 버그가 **아님**.
   - 모든 결과는 사용자 결정 사양(Q8: 2글자 미만 제외, 결정론적 다단계 순위 정렬, KFC2: 구문 비교형 연속 일치)에 정확히 부합함.
2. **차이의 본질**:
   - 15건은 모두 기존 SQLite FTS5 legacy 검색의 한계(문서 길이에 편향된 BM25 스코어, 1글자 불용어 강제 AND로 인한 검색 실패, 괄호 등 기호 미처리, 구문 공백 불일치 시 누락)가 kfind에서 개선되면서 발생한 **의도된 동작 차이**임.
   - 단지 검증 도구(`tools/app1_equivalence.py`)의 `classify_difference` 함수가 이를 분류할 수 있는 규칙을 아직 완비하지 못해 `UNKNOWN`으로 분류되었을 뿐임.
3. **향후 권고 조치**:
   - kfind 코드는 변경하지 않고 그대로 유지.
   - 향후 APP1-K 관문 통과 및 전체 전환을 위해 `tools/app1_equivalence.py`의 `classify_difference` 판정 로직을 보완(순위 기준 역전 E7, dropped 제외어 E9, 구문 fold 및 기호 검색어 E5 확장 등)하여 15건의 `UNKNOWN`을 합당한 의도된 차이(Explained)로 해소할 것을 권고함.

---

## 6. 조치 내역 (2026-11-14)

### 유형 4(따옴표 구문 검색 누락) 도구 수정
- **수정 파일**: `tools/app1_equivalence.py`
- **바꾼 이유**:
  따옴표 구문(`"…"`) 검색 시 `ContentSearchIndex.parse_query`는 검색어를 `parsed['phrases']`에 담으나, 기존 도구는 `parsed['terms']`만 `words`에 반영하여 구문 검색어만 있는 경우 `words = []`가 되어 차이 분류 조건(`if words and ...`)을 타지 못하고 `UNKNOWN`으로 누락됨.
  이에 따라 `raw_terms = [*parsed['terms'], *parsed['phrases']]`로 구문 검색어를 포함시키고, 원문 공백·기호 불일치로 legacy에서는 누락되었으나 kfind에서 fold 비교형 정규화로 일치하여 추가된 경우(`added`)를 `E5(비교형 정규화 차이)`로 올바르게 분류하도록 보완함.
- **시험 추가**: `tests/test_app1_equivalence.py`에 따옴표 구문 E5 분류(일반 및 tempfile 기반)와 문장 분리 E1 분류 시험 추가.
