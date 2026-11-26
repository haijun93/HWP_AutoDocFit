# SEARCHDBG1 — klib 디버그 빌드 search-content 간헐 종료 (KLIBENT1 남은 문제 1)

## 1. 개요
- **카드 목적**: tantivy 0.27.0 `segment_postings.rs`의 `debug_assert!(self.doc() <= target)`가 `test_semi2_backend_integration`에서 간헐적으로(6회 중 4회) 발생하던 현상의 원인을 규명(우리 쪽 seek 호출 순서인지 의존성 문제인지 판정), 코드 문제 수정, release 빌드에서의 결과 동일성 검증.
- **작업 브랜치**: `feature/SEARCHDBG1` (기준: `github/main` 178ef27e)
- **작업 결과**: 원인은 의존성 결함이 아닌 **우리 쪽 `DocSet::seek` 사전조건 위반(우리 코드 문제)**으로 판정하여 수정 완료. release 빌드와 debug 빌드의 합성 11개 질의 대조 결과가 100% 일치함을 확인. 맥 환경 기준선 대비 신규 실패 0건, `test_semi2_backend_integration` 5개 시험 전건 통과.

---

## 2. 원인 분석 및 판정

### 2.1 현상 재현
`tests/test_semi2_backend_integration.py` 실행 시 debug 빌드 klib에서 exit 101(panic) 발생:
```text
thread 'main' panicked at /Users/hyeokjunkong/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tantivy-0.27.0/src/postings/segment_postings.rs:173:9:
assertion failed: self.doc() <= target
stack backtrace:
   3: <tantivy::postings::segment_postings::SegmentPostings as tantivy::docset::DocSet>::seek
   4: klib::search::content_policy::occurrences (klib/src/search_content_policy.rs:125:29)
   5: klib::search::content_policy::select
   6: <klib::search::SearchIndex>::search_scoped
   7: <klib::search::SearchIndex>::search_content
   8: klib::search_cli::search
```

### 2.2 원인 규명 (우리 코드 문제)
Tantivy의 `DocSet::seek(target)` 규약(`tantivy/src/docset.rs:51`)은 다음과 같습니다:
> `target` has to be larger or equal to `.doc()` when calling `seek`.

`klib/src/search_content_policy.rs`의 `occurrences` 함수는 candidates 인덱스를 `doc_id` 순으로 정렬한 뒤 `postings.seek(c.5.doc_id)`를 무조건 호출했습니다.
하지만 다음 두 가지 경우에 `target < postings.doc()`가 발생합니다:
1. **첫 후보의 `doc_id`가 포스팅 첫 문서보다 앞서는 경우**:
   계층적 문서(Markdown 등)에서 특정 문단이 질의어에 매칭되어 문서 단위로 상위/인접 문단(DocId 0 등)이 `candidates`에 함께 포함되었으나, 해당 질의어 토큰 자체는 DocId 1부터 시작하는 경우, 포스팅이 열리자마자 `postings.doc() == 1`인 상태에서 첫 후보 `target == 0`을 seek하려 하여 `1 <= 0` 단언 실패.
2. **중간 문단을 건너뛴 뒤 다음 후보가 이미 지나간 doc_id인 경우**:
   토큰이 없는 DocId 5를 seek할 때 포스팅이 다음 매칭 위치인 DocId 20으로 점프한 뒤, 다음 후보(DocId 10)를 seek하려 할 때 `20 <= 10` 단언 실패.

### 2.3 Release 빌드에서의 동작 및 결과 판정
- Release 빌드에서는 `debug_assert!(self.doc() <= target)`가 비활성화됩니다.
- Tantivy `SegmentPostings::seek` 구현(`segment_postings.rs:174`):
  ```rust
  if self.doc() >= target {
      return self.doc();
  }
  ```
- `self.doc() > target`일 때 아무 전진 없이 `self.doc()`을 반환하므로, 우리 코드의 `postings.seek(c.5.doc_id) == c.5.doc_id` 판정에서 `false`가 되어 `totals`에 합산되지 않습니다.
- 포스팅 목록은 단조 증가 스트림이므로 현재 커서보다 작은 target은 해당 토큰이 절대 존재하지 않습니다.
- 따라서 **Release 빌드에서는 단언(debug_assert)만 빠진 것일 뿐, 결과는 완전히 정상적이고 틀리지 않음**을 합성 대조로 입증했습니다.

---

## 3. 수정 내용

### 3.1 `klib/src/search_content_policy.rs`
1. `postings.doc() > target`인 경우 `postings.seek`를 호출하지 않고 `found = false`로 즉시 처리 (`debug_assert` 사전조건 보장).
2. `postings.doc() == TERMINATED`인 경우 이후의 모든 후보에 대해 불필요한 연산을 건너뛰도록 `break` 추가.

```rust
for &i in indices.iter() {
    let c = &candidates[i];
    let target = c.5.doc_id;
    if postings.doc() == TERMINATED { break; }
    let found = if postings.doc() > target {
        false
    } else {
        postings.seek(target) == target
    };
    if found {
        let weight = match c.2 { 0 => 1, 1 => 8, 2 => 4, _ => 2 };
        totals[c.0] += u64::from(postings.term_freq()) * weight;
    }
}
```

### 3.2 `klib/src/search.rs`
- `occurrences_handles_sparse_and_skipped_doc_ids` 신규 단위시험 추가:
  계층 블록으로 DocId 0(토큰 없음), DocId 1(토큰 있음), DocId 2(토큰 없음) 구조를 생성하여 `hydrate: true` 검색 시 첫 후보 및 건너뛴 후보에서 패닉 없이 정상 검색 및 가중치 합산이 이루어지는지 검증.

---

## 4. 시험 및 검증 결과

### 4.1 합성 질의 Release vs Debug 1:1 대조
동일한 Tantivy 색인(5개 문서, 11개 질의 유형: 단일 키워드, 다중 키워드, 위계/비위계, limit/offset, 필터 등)에 대해 debug klib과 release klib 실행 결과를 비교:
- **대조 결과**: 11개 질의 전건 100% 동일한 응답(문서 집합, 순서, 점수, 위치 정보, 하이라이트) 확인.

### 4.2 Python 통합 시험 (`tests/test_semi2_backend_integration.py`)
- **수정 전 (Debug klib)**: 5개 중 2개 ERROR (exit 101 panic으로 `test_native_index_batch_related_filters_and_default_coexist`, `test_large_related_metadata_keeps_default_tokens_and_all_evidence` 실패)
- **수정 후 (Debug klib)**:
  ```text
  Ran 5 tests in 2.073s
  OK
  ```
- **수정 후 (Release klib)**:
  ```text
  Ran 5 tests in 1.429s
  OK
  ```
- **관련 Python 시험 (`test_srch6_content_search.py`, `test_timemachine_search_backend.py`)**:
  ```text
  Ran 14 tests in 1.541s
  OK
  ```

### 4.3 Cargo 전체 시험 (Mac 기준선 대조)
- **기준선 (main 178ef27e)**:
  - `klib (lib)`: ok. 19 passed; 0 failed
  - `tests/test_hwp5.rs`: ok. 22 passed; 0 failed
  - `tests/test_klib.rs`: FAILED. 75 passed; 45 failed; 1 ignored (Windows API/파이프 미지원 환경으로 인한 기준선 실패)
  - `tests/test_scan_hwp.rs`: ok. 3 passed; 0 failed
- **수정 후 (feature/SEARCHDBG1)**:
  - `klib (lib)`: ok. 19 passed; 0 failed
  - `tests/test_hwp5.rs`: ok. 22 passed; 0 failed
  - `tests/test_klib.rs`: FAILED. 86 passed; 45 failed; 1 ignored (단일 스레드 기준 통과 86건, 신규 실패 0건)
  - `tests/test_scan_hwp.rs`: ok. 3 passed; 0 failed
- **실패 목록 비교 분석**:
  - `baseline_failures` (45건) == `postfix_failures` (45건). 신규 실패 0건.
