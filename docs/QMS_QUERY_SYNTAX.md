# 퀵몽땅검색(ktm / kfind) 질의 문법 명세 (QMS_QUERY_SYNTAX)

작성일: 2026-11-15 · 카드: QMS-QUERY · 대상: `kfind/core`, `ktm`

이 문서는 퀵몽땅검색(`ktm`) 및 색인 코어(`kfind/core`)에서 사용하는 고급 질의 문법의 파싱 규칙, 연산자 우선순위, 따옴표/이스케이프 처리, 오류 복구, 그리고 기존 `SearchOptions`와의 매핑 방식을 정의한다.

---

## 1. 개요 및 설계 원칙

1. **검색 중단 없는 복구(Fault-Tolerant & Non-blocking)**:
   사용자가 입력창에 글자를 타이핑하는 도중(실시간 검색, 매 키 입력마다 실행) 미완성 문법(예: `ext:`, `after:202`, `-"`, `-`)이 들어와도 파싱 에러로 검색을 멈추지 않는다. 유효하지 않은 문법 요소는 일반 검색 낱말로 안전하게 폴백한다.
2. **한글 입력 조합 보호**:
   한글 조합 중인 자모(예: `ㅎ`, `하`, `한`) 및 조합 완료 글자를 왜곡하거나 누락하지 않고 원문 그대로 유지한다.
3. **초고속 단일 패스(Zero-Allocation & Microsecond Latency)**:
   단일 패스(O(N)) 스캐너로 동작하여 파싱 비용을 수 마이크로초(µs) 이하로 제한하고, 입력→첫 그림 16ms 한도를 엄격히 지킨다.
4. **기존 기능과의 완전한 호환**:
   세미콜론(`;`) 묶음 검색, 2글자 미만 필터링, 붙여 쓴 한국어 분할(`SplitMode`) 등 기존 검색 체계와 충돌 없이 직교(orthogonal)하게 동작한다.

---

## 2. 지원 연산자 및 문법

| 문법 | 이름 | 설명 | 예시 | 연결되는 `SearchOptions` |
|---|---|---|---|---|
| `"…"` / `“…”` | 정확 구문 (Phrase) | 따옴표로 감싼 문장/어절을 분할 없이 연속 일치로 검색 | `"예산 집행"`, `"공사 계약"` | `phrases: Vec<String>` |
| `-낱말` / `-"…"` | 제외어 (Exclusion) | 해당 낱말이나 구문이 포함된 문서를 결과에서 완전히 제외 | `-보류`, `-temp`, `-"임시 보류"` | `deny: Option<Arc<RoaringTreemap>>` (via `Index::doc_set`) |
| `ext:확장자[,...]` | 파일 형식 (Extension) | 특정 파일 확장자(쉼표로 다중 지정 가능)로 한정 | `ext:hwp`, `ext:hwp,pdf`, `ext:.docx` | `file_types: Option<Vec<String>>` |
| `path:경로` | 폴더 범위 (Folder) | 특정 폴더 경로(공백 포함 시 따옴표 가능)로 한정 | `path:기안`, `path:"C:\공문서\2026"` | `folder: Option<String>` |
| `after:날짜` | 시작 날짜 (After) | 해당 날짜 이후(포함) 수정/작성된 문서 | `after:2026-01-01`, `after:20260101`, `after:2026` | `start_date: Option<i64>` (YYYYMMDD) |
| `before:날짜` | 종료 날짜 (Before) | 해당 날짜 이전(포함) 수정/작성된 문서 | `before:2026-12-31`, `before:20261231`, `before:2026` | `end_date: Option<i64>` (YYYYMMDD) |
| `*`, `?` | 와일드카드 낱말 | 토큰 하나에서 `*`는 0개 이상 글자, `?`는 한 글자와 일치. 고정 글자 2개 이상이 있을 때만 비트맵으로 후보를 좁힌 뒤 후보 문장 원문(비교형)에서 대조 | `계약*`, `*보고`, `공?사` | `QuerySyntax.wildcards` |
| `re:패턴` | 정규식 | 현재 미지원. `regex` 크레이트가 기존 의존성에 없고 신규 의존성 추가 없이 안전한 실행을 보장할 방법이 없어 일반 검색 낱말로 보존 | `re:계약.*보고` | 미지원 사유 기록 |
| `;` | 묶음 검색 (Group) | 세미콜론으로 구분된 각 질의 그룹의 문서 교집합 | `2026; 공사 ext:hwp` | `search_groups` 경로 유지 |

---

## 3. 연산자 우선순위 (Operator Precedence)

파서는 다음 순서대로 질의 문자열을 해석한다:

```
[1단계: 세미콜론(;) 분리] (따옴표 밖의 세미콜론 기준 묶음 분리)
        ↓
[2단계: 토큰화 및 따옴표/이스케이프 처리] (따옴표 안은 리터럴 보호)
        ↓
[3단계: 접두사 연산자 해석] (ext:, path:, after:, before:, -)
        ↓
[4단계: 잔여 일반 검색어 추출] (나머지 낱말들은 검색어 문자열로 결합)
```

1. **1단계: 세미콜론(`;`) 묶음 분리 (Groups)**
   - 최상위 연산자. 단, 따옴표(`"…"`) 내부의 `;`는 묶음 분리자로 동작하지 않고 리터럴 문자로 보호된다.
   - 분리된 각 묶음은 독립적인 질의 표현식을 구성하며, 전체 검색 시 각 묶음 결과의 교집합을 구한다.
   - 스코프 한정 연산자(`ext:`, `path:`, `after:`, `before:`)는 질의 전체에 공통으로 수집되어 `SearchOptions`의 단일 범위로 적용된다.

2. **2단계: 따옴표 및 이스케이프 (Quotes & Escaping)**
   - 따옴표(`"`, `“`, `”`) 안의 내용은 단일 리터럴 구문으로 취급된다.
   - 따옴표 내부에서는 연산자 접두사(`ext:`, `path:`, `after:`, `before:`, `-`)가 동작하지 않는다.
     - 예: `"ext:pdf"` → 확장자 필터가 아니라 `ext:pdf`라는 문자열 구문 검색.
   - 백슬래시(`\`)를 통한 따옴표 이스케이프(`\"`)를 지원한다.

3. **3단계: 접두사 연산자 식별 및 유효성 검증**
   - 토큰의 시작 접두사가 `-`, `ext:`, `path:`, `after:`, `before:` 인지 검사.
   - 접두사 뒤의 값이 유효한지 검증:
     - `ext:`: 비어있지 않은 확장자 목록(쉼표 구분). 앞점(`.`)은 자동 제거.
     - `path:`: 비어있지 않은 경로 문자열(따옴표 포함 가능).
     - `after:` / `before:`: 유효한 날짜 포맷(`YYYY-MM-DD`, `YYYY/MM/DD`, `YYYY.MM.DD`, `YYYYMMDD`, `YYYY`).
     - `-`: 접두사 뒤에 공백 없이 낱말이나 따옴표 구문이 붙어 있는 형태.
   - 검증을 통과하면 해당 필드(`phrases`, `excludes`, `file_types`, `folder`, `after`, `before`)에 저장된다.

4. **4단계: 오류 복구 및 일반 검색어 (Fallback to Text)**
   - 접두사 뒤의 값이 비어있거나 유효하지 않은 경우(예: `ext:`, `after:invalid`, 단독 `-`), 에러를 내지 않고 해당 토큰 전체를 일반 검색어로 환원한다.
   - 연산자로 추출되지 않은 모든 일반 토큰은 공백으로 연결된 검색어(`clean_text`)로 결합된다.

---

## 4. 세부 처리 규칙

### 4.6 와일드카드 및 정규식
- `*`는 0개 이상 비교형 글자, `?`는 비교형 글자 하나와 일치하며 공백으로 구분된 토큰 하나에 적용한다.
- 고정 연속 글자가 2개 이상이면 그 글자쌍 posting 비트맵을 교집합해 후보 문장을 만들고, 그 후보의 비교형 원문만 대조한다. 고정 글자가 없으면 전체 문장 순회를 하지 않으므로 와일드카드가 결과를 내지 않는다.
- 후보 대조는 최대 50,000문장 및 기존 질의 시간 한도를 공유한다. 한도 초과 시 `SearchResult.partial`이 참이다.
- 정규식은 패턴 200자 제한만으로 백트래킹 폭발을 막지 못한다. 현재 기존 의존성에 선형 실행을 보장하는 정규식 크레이트가 없어 미지원이며, `re:` 토큰은 일반 낱말로 보존한다.

### 4.1 따옴표 및 닫히지 않은 따옴표 (Unclosed Quotes)
- 따옴표 쌍(`"구문"`): 정상적인 정확 구문으로 추출.
- 닫히지 않은 따옴표(`"미완성`): 사용자가 타이핑 중인 상태를 고려하여, 따옴표 뒤의 문자열을 버리지 않고 일반 검색어 토큰으로 취급하여 실시간 검색이 끊김 없이 동작하도록 한다.

### 4.2 제외어 (`-낱말`, `-"구문"`)
- `-` 뒤에 공백이 있으면(예: ` - ` 또는 `단어 - 단어`) 제외어로 인식하지 않고 일반 기호로 처리한다.
- 제외어 낱말의 길이가 2글자 미만(비교형 기준)인 경우, 색인 엔진의 `MIN_WORD_CHARS`(2글자) 제약에 의해 검색이 불가능하므로 오류를 내지 않고 안전하게 무시한다.
- 제외어가 지정되면 검색 실행 전 `Index::doc_set(단어)`를 호출하여 해당 단어를 포함한 문서를 조회하고, `SearchOptions::deny`에 `RoaringTreemap` 합집합(union)으로 연결한다.

### 4.3 날짜 정규화 (`after:`, `before:`)
- 지원 입력 형식:
  - `YYYY-MM-DD`, `YYYY/MM/DD`, `YYYY.MM.DD` → `YYYYMMDD` (예: 20261115)
  - `YYYYMMDD` → `YYYYMMDD` (예: 20261115)
  - `YYYY` → 연도 단독 입력 시 `after:2026`은 `20260101`, `before:2026`은 `20261231`로 정규화.
- 정규화된 값은 `i64` 정수로 변환되어 `SearchOptions.start_date` 및 `SearchOptions.end_date`에 바인딩된다.
- 유효 범위를 벗어난 날짜(예: 연도 < 1900 또는 > 2100, 월 > 12 등)는 날짜 연산자로 처리하지 않고 일반 검색어로 유지한다.

### 4.4 파일 확장자 (`ext:`)
- `ext:hwp,pdf,docx` 처럼 쉼표로 연결된 복수 확장자를 분리하여 소문자로 정규화한다.
- 확장자 앞의 점(`.`)은 자동으로 제거한다 (`ext:.hwp,.pdf` → `["hwp", "pdf"]`).

### 4.5 폴더 경로 (`path:`)
- `path:폴더` 또는 `path:"C:\Program Files\Docs"` 형태를 지원한다.
- 경로 구분자(`/`, `\`)는 유지하며 시스템 경로 검색에 맞게 정규화된다.

---

## 5. `SearchOptions` 연결 구조

파싱 결과체인 `QuerySyntax`는 다음과 같이 정의되며, `SearchOptions`로 즉시 변환되거나 병합된다:

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuerySyntax {
    /// 연산자/구문/제외어가 제거된 순수 검색어 (그룹별 또는 전체)
    pub clean_text: String,
    /// 정확 구문 목록 ("…")
    pub phrases: Vec<String>,
    /// 제외어 목록 (-낱말, -"구문")
    pub excludes: Vec<String>,
    /// 파일 확장자 필터 (ext:hwp,pdf)
    pub file_types: Option<Vec<String>>,
    /// 폴더 필터 (path:...)
    pub folder: Option<String>,
    /// 시작 날짜 (after:...) -> YYYYMMDD i64
    pub start_date: Option<i64>,
    /// 종료 날짜 (before:...) -> YYYYMMDD i64
    pub end_date: Option<i64>,
    /// 세미콜론(;)으로 분리된 각 그룹의 clean_text (그룹 검색용)
    pub groups: Vec<String>,
}
```

### SearchOptions 필드 매핑
- `clean_text` / `groups` → 검색 함수 질의 인자(`query`)
- `phrases` → `SearchOptions::phrases` (`Vec<String>`)
- `excludes` → `Index::doc_set()`을 거쳐 `SearchOptions::deny` (`Option<Arc<RoaringTreemap>>`)
- `file_types` → `SearchOptions::file_types` (`Option<Vec<String>>`)
- `folder` → `SearchOptions::folder` (`Option<String>`)
- `start_date` → `SearchOptions::start_date` (`Option<i64>`)
- `end_date` → `SearchOptions::end_date` (`Option<i64>`)

기존 `SearchOptions`에 이미 설정되어 있던 `folder`, `start_date`, `end_date`, `file_types`, `phrases`, `deny`와 질의 문법에서 파싱된 옵션이 함께 주어질 경우, 문법에서 명시한 옵션이 우선 적용되거나 결합된다.
