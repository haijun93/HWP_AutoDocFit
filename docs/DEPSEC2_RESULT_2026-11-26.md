# DEPSEC2 결과 — 남은 유지보수 중단 경고 줄이기 (DEPSEC1 조치안)

- 대상: `feature/DEPSEC2`
- 작업 성격: DEPSEC1 조치안에 따른 유지보수 중단 경고 해소 및 상위 의존성 갱신 조사.
- 커밋·푸시·태그는 하지 않았다(실행기 커밋 예정, 푸시 금지).
- 저장소 파일에 PC 전용 경로를 포함하지 않았다.
- 주의: 본 시험은 macOS 환경에서 수행되었으며, **이 맥 결과만으로 통과라 판정하지 않는다(PM이 Windows 환경에서 다시 시험 필요).**

---

## 1. 변경 요약

| 항목 | 변경 전 | 변경 후 | 해소/조사 결과 |
|---|---|---|---|
| `pdf_edit_core` lopdf | `0.42` (기본 features 포함) | `0.42` (`default-features = false`, `features = ["rayon"]`) | `chrono 0.4.45` (RUSTSEC-2026-0543) 완전 제외 및 해소 |
| `bench/kb1` chrono | `0.4.45` | 현행 유지 (조사 완료) | seekstorm 5.2.1 갱신 시 `jiff`로 전환되어 해소 가능 (API 변경 호환성 검토 필요) |
| `bench/kb1` number_prefix | `0.4.0` | 현행 유지 (조사 완료) | upstream `model2vec-rs` 최신(0.5.1)의 tokenizers/hf-hub 의존 제약으로 상위 갱신만으로 즉시 제거 불가 |
| `bench/kb1` paste | `1.0.15` | 현행 유지 (조사 완료) | upstream `model2vec-rs` 최신(0.5.1)의 tokenizers/hf-hub 의존 제약으로 상위 갱신만으로 즉시 제거 불가 |
| `pdf_edit_core` ttf-parser | `0.25.1` | 현행 유지 (카드 지시) | 손상 PDF 관대 처리 차이 및 분류 정합성 검토 선행 필요로 이번 작업 대상 제외 |

---

## 2. pdf_edit_core lopdf 기능 조정 및 날짜 동작 검증

### 2.1 lopdf feature 변경
- `pdf_edit_core/Cargo.toml` 의 `lopdf = "0.42"` 를 `lopdf = { version = "0.42", default-features = false, features = ["rayon"] }` 로 변경.
- lopdf 0.42의 기본 features(`["chrono", "jiff", "rayon", "time"]`) 중 미사용 날짜 라이브러리(`chrono`, `jiff`, `time`)를 제외하고 병렬 처리에 필요한 `rayon`만 유지.
- `cargo tree` 검사 결과 `chrono`, `jiff`, `time`, `iana-time-zone` 크레이트가 의존성 트리에서 완전히 제외됨을 확인(`package ID specification chrono did not match any packages`).

### 2.2 날짜 관련 동작 분석 및 불변성 확인
1. **lopdf 소스 분석 (`src/datetime.rs`)**:
   - `chrono` feature는 `DateTime<Local>`, `DateTime<Utc>`와 lopdf의 `Object::string_literal` 및 `lopdf::DateTime` 사이의 `From`/`TryFrom` 변환 구현에만 사용된다.
   - lopdf의 핵심 파서 및 메타데이터 처리(`src/reader.rs`의 `ModDate`, `CreationDate` 등)는 `lopdf::DateTime(String)` 구조체를 통한 문자열 레벨 처리로 동작하므로 `chrono` feature 유무와 무관하게 정상 유지된다.
2. **`pdf_edit_core` 소스 분석**:
   - `pdf_edit_core` 소스 코드 전체에서 `chrono` 또는 lopdf의 날짜 변환 함수를 호출하는 곳이 전혀 없음(`grep` 검색 결과 0건).
3. **시험 검증**:
   - PDF 분류 정합성(`tests/classify_parity.rs`), 다단계 편집 파이프라인(`tests/pipeline.rs`), 보안 및 안전성 검증(`tests/safety.rs`) 전체가 수정 전 기준선과 100% 동일하게 통과함을 확인했다.

---

## 3. bench/kb1 의존성 조사 (number_prefix·paste·chrono)

`bench/kb1` 크레이트는 KB1 비교 측정 전용(kfind-core 대 Tantivy 대 SeekStorm) 벤치마크 크레이트이며 본 제품 배포 의존성이 아니다(`bench/kb1/Cargo.toml: description = "KB1 비교 측정 전용: ... 본 제품 의존성이 아니다."`).

### 3.1 의존성 유입 구조 (cargo tree 분석)
```text
number_prefix v0.4.0 (RUSTSEC-2025-0119)
└── indicatif v0.17.11
    ├── hf-hub v0.4.3
    │   └── model2vec-rs v0.4.0
    │       └── seekstorm v4.0.1
    │           └── kb1-bench v0.1.0 (bench/kb1)
    └── tokenizers v0.21.4
        └── model2vec-rs v0.4.0 (*)

paste v1.0.15 (RUSTSEC-2024-0436)
└── tokenizers v0.21.4
    └── model2vec-rs v0.4.0
        └── seekstorm v4.0.1
            └── kb1-bench v0.1.0 (bench/kb1)

chrono v0.4.45 (RUSTSEC-2026-0543)
└── seekstorm v4.0.1
    └── kb1-bench v0.1.0 (bench/kb1)
```

### 3.2 항목별 상위 의존성 갱신 조사 결과 및 조치안

1. **chrono 0.4.45 (`seekstorm 4.0.1` 직접 의존)**:
   - **조사 결과**: `seekstorm` 최신 버전(5.2.1)의 `Cargo.toml`을 확인한 결과, `chrono`가 완전히 제거되고 `jiff = "1.0.2"`로 대체되어 있다.
   - **조치안**: `seekstorm = "5.2.1"` 로 버전을 상향하면 chrono 경고는 완전히 제거된다. 단, seekstorm 4.x → 5.x는 메이저 버전 변경으로 쿼리/인덱스 API 변경 사항이 있어 `bench/kb1/src/main.rs`의 벤치마크 코드 적응 작업 및 재검증이 수반되어야 한다.

2. **number_prefix 0.4.0 & paste 1.0.15 (`seekstorm` → `model2vec-rs` 경유)**:
   - **조사 결과**:
     * `indicatif`: 0.18.6 최신 버전에서 `number_prefix` 의존성이 제거되었음.
     * `tokenizers`: 1.0.0-rc.5 최신 버전에서 `paste` 의존성이 제거되었음.
     * 그러나 `seekstorm` 최신(5.2.1)이 사용하는 `model2vec-rs`(0.5.0, 최신 0.5.1)의 `Cargo.toml`이 `tokenizers = "0.21"` 및 `hf-hub = "0.4"`(indicatif 0.17 사용)로 버전이 고정되어 있다.
     * `model2vec-rs`는 seekstorm의 필수(non-optional) 의존성이다.
   - **조치안**:
     * 현재 시점에서는 `seekstorm` 최신(5.2.1)으로 갱신하더라도 `model2vec-rs 0.5.1`을 거쳐 `tokenizers 0.21.4`(paste 1.0 유입) 및 `indicatif 0.17.11`(number_prefix 0.4 유입)이 강제 설치되므로, **상위 의존성 단순 갱신만으로는 완전히 제거할 수 없다.**
     * 완전한 해소를 위해서는 upstream `model2vec-rs`가 `tokenizers 1.x`와 `indicatif 0.18+` 지원 버전으로 릴리스되고, 이를 `seekstorm`이 채택해야 한다.
     * `bench/kb1`은 본 제품 바이너리가 아닌 벤치마크 크레이트이므로, OSV 점검 워크플로에서 제품 크레이트와 벤치마크 크레이트의 감사 대상을 분리하는 방안을 권장한다(advisory 무시나 점검 범위 축소가 아닌 타깃 크레이트 명시).

---

## 4. 시험 실행 결과

### 4.1 pdf_edit_core 기준선(수정 전) vs 수정 후 비교

- 실행 명령: `CARGO_TARGET_DIR=$HOME/cargo-target-DEPSEC2 cargo test --manifest-path pdf_edit_core/Cargo.toml`
- 대상: macOS 환경 (rustc 1.85.0 / aarch64-apple-darwin)

| 시험 대상 (test runner) | 수정 전 기준선 | 수정 후 (chrono 제거) | 결과 |
|---|---|---|---|
| `unittests src/lib.rs` | `ok. 3 passed; 0 failed` | `ok. 3 passed; 0 failed` | 동일 (통과) |
| `unittests src/main.rs` | `ok. 0 passed; 0 failed` | `ok. 0 passed; 0 failed` | 동일 (통과) |
| `tests/classify_parity.rs` | `ok. 2 passed; 0 failed` | `ok. 2 passed; 0 failed` | 동일 (통과) |
| `tests/pipeline.rs` | `ok. 7 passed; 0 failed` | `ok. 7 passed; 0 failed` | 동일 (통과) |
| `tests/safety.rs` | `ok. 8 passed; 0 failed` | `ok. 8 passed; 0 failed` | 동일 (통과) |
| `Doc-tests pdf_edit_core` | `ok. 0 passed; 0 failed` | `ok. 0 passed; 0 failed` | 동일 (통과) |
| **전체 합계** | **20 passed, 0 failed** | **20 passed, 0 failed** | **새 실패 0건** |

#### 수정 후 실제 실행 출력 전문:
```text
   Compiling lopdf v0.42.0
   Compiling pdf_edit_core v0.1.0 (/Users/hyeokjunkong/work/docfit/jobs/DEPSEC2/pdf_edit_core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 5.03s
     Running unittests src/lib.rs (/Users/hyeokjunkong/cargo-target-DEPSEC2/debug/deps/pdf_edit_core-74cd307d9ba3997b)

running 3 tests
test tests::delete_and_split_reject_empty_or_invalid_ranges ... ok
test tests::page_info_reports_dimensions_and_effective_rotation ... ok
test tests::page_operations_preserve_requested_order_and_count ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s

     Running unittests src/main.rs (/Users/hyeokjunkong/cargo-target-DEPSEC2/debug/deps/pdf_edit-b9ae07e05bef2bf7)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/classify_parity.rs (/Users/hyeokjunkong/cargo-target-DEPSEC2/debug/deps/classify_parity-ce44bc83a3b726d5)

running 2 tests
test size_limit_is_one_gib ... ok
test rust_classify_matches_python_verdicts_for_every_fixture ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.07s

     Running tests/pipeline.rs (/Users/hyeokjunkong/cargo-target-DEPSEC2/debug/deps/pipeline-6c1a70e0f6f78320)

running 7 tests
test failed_operation_leaves_document_unchanged ... ok
test from_bytes_applies_same_safety_rules ... ok
test design_scenario_four_rotate_delete_reorder ... ok
test only_final_file_is_written_and_source_is_untouched ... ok
test in_memory_chain_equals_temp_file_chaining ... ok
test cli_merge_loses_inherited_geometry_documented_difference ... ok
test merge_matches_cli_merge_page_order_and_keeps_inherited_attributes ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.52s

     Running tests/safety.rs (/Users/hyeokjunkong/cargo-target-DEPSEC2/debug/deps/safety-70602cd10cd26597)

running 8 tests
test output_never_overwrites_source_or_existing_file ... ok
test invalid_delete_and_split_have_no_partial_output ... ok
test deeply_nested_objects_do_not_overflow_the_stack ... ok
test rotation_uses_inherited_angle_and_handles_large_angles ... ok
pdf-edit: Invalid("Encrypted PDFs are not supported")
test signed_and_encrypted_documents_are_refused_before_writing ... ok
test nested_page_tree_reorder_keeps_inherited_geometry_and_order ... ok
test valid_delete_and_split_keep_expected_pages ... ok
pdf-edit: Invalid("Encrypted PDFs are not supported")
pdf-edit: Invalid("Encrypted PDFs are not supported")
pdf-edit: Invalid("Encrypted PDFs are not supported")
pdf-edit: Invalid("Encrypted PDFs are not supported")
pdf-edit: Invalid("Encrypted PDFs are not supported")
pdf-edit: Invalid("Encrypted PDFs are not supported")
pdf-edit: Invalid("Encrypted PDFs are not supported")
pdf-edit: Invalid("Invalid PDF header")
test protected_or_prefixed_pdfs_are_rejected_by_every_edit_path ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.83s

   Doc-tests pdf_edit_core

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### 4.2 관련 Python 모듈 시험
- 실행 명령: `python3 -m unittest discover -s tests -p "*pdf*"`
- 결과: `Ran 41 tests in 15.830s`, 40개 통과, 1개 실패.
  * 실패(`test_finds_installed_resources_before_development_engine`)는 macOS 임시 디렉터리 경로 별칭(`/var/folders/...` 대 `/private/var/folders/...`) 비교 차이로, 이전 작업(PDFUI2)에서도 확인된 기존 환경 현상이며 이번 변경과 무관함.

---

## 5. 남은 문제 및 권고사항

1. **Windows 환경 재검증 필요**:
   - macOS 환경에서 `pdf_edit_core`의 컴파일 및 20개 테스트가 전부 통과했으나, 카드의 지침에 따라 PM이 Windows 환경에서 MinGW / MSVC 및 Python 전체 스위트를 재시험하여 최종 승인해야 한다.
2. **ttf-parser 유지보수 중단 경고**:
   - `lopdf 0.42`의 필수 의존성으로 유입되는 `ttf-parser 0.25.1`은 손상 PDF 관대 처리 차이 문제로 인해 이번 카드에서 건드리지 않았다.
3. **bench/kb1 처리 방향**:
   - `bench/kb1`의 chrono는 seekstorm 5.2.1 갱신으로 해소 가능하나, `number_prefix`와 `paste`는 upstream `model2vec-rs`의 차기 릴리스가 필요하다.
   - 제품 배포와 무관한 벤치마크 디렉터리이므로 OSV Scanner 타깃 구성 시 크레이트 격리를 검토할 수 있다.
