# KLIBENT2 결과 — 자료함 색인·이름 바꾸기 XML 처리 보안·정확성

- 브랜치 `feature/KLIBENT2` (기준 `github/main`), 커밋하지 않음 (실행기 위임).
- 발단: KLIBENT1 남은 문제 2(자료함 색인 `content.rs`의 DTD 미거부 및 `xml_text`의 숫자 참조 공백 치환) 및 문제 3(DOCX `parse_xml_tree`의 속성 정의되지 않은 참조 누락/조용한 버림).

## 원인

1. **DOCX `parse_xml_tree` (`klib/src/extract.rs`)**:
   - `for attr in e.attributes().with_checks(false).flatten()`와 `if let Ok(value) = attr.decode_and_unescape_value(reader.decoder())`를 사용하여, 속성 값에 정의되지 않은 엔티티(`&secret;`)나 거부 대상 참조가 있어도 에러를 전파하지 않고 해당 속성을 조용히 버린 채 본문 추출을 성공시켰습니다.
2. **자료함 색인 `content.rs`**:
   - 자체 훑기 함수 `scan`: `<!DOCTYPE ...>`를 만나면 `tag.starts_with('!')` 조건으로 인해 단순 건너뛰기(`continue`)되어 DTD를 거부하지 않았습니다.
   - `unescape`: 모르는 엔티티를 공백(`' '`)으로 치환했고, 숫자 참조는 XML 1.0 유효 범위 검사 없이 수용했습니다.
   - `xml_text`: 표준 엔티티 5개 외에는 숫자 참조를 포함한 모든 엔티티를 공백(`' '`)으로 치환했으며, DTD를 태그로 취급하여 통과시켰습니다.
3. **이름 바꾸기 `rename.rs`**:
   - `read_hwpx_title`에서 쓰이는 `xml_text`: `content.rs`와 동일하게 숫자 참조(`&#xAC00;` 등)를 공백으로 치환하여 제목이 왜곡되거나 비워지는 문제가 있었으며, DTD나 정의되지 않은 참조를 거부하지 않았습니다.

## 고친 것

KLIBENT1의 `resolve_xml_ref` 규칙(XML 1.0 허용 문자 및 표준 5대 엔티티만 수용, DTD·외부/사용자 정의/불법 숫자 참조 거부)으로 세 모듈을 통일했습니다.

| 모듈 | 변경 전 동작 | 변경 후 동작 |
|---|---|---|
| DOCX 속성 (`extract.rs`) | 속성 값의 정의되지 않은 참조 조용히 누락 | `unescape_xml_attr` 적용, 정의되지 않거나 불법 참조 시 즉시 거부 (`corrupt: XML 엔티티 참조 거부`) |
| 자료함 색인 블록 (`content.rs`) | DTD 건너뜀, 모르는 참조 공백 치환 | `scan`에서 `<!DOCTYPE`·`<!ENTITY` 즉시 거부, `unescape`에서 불법/미정의 엔티티 즉시 거부 |
| 자료함 색인 텍스트 (`content.rs`) | `xml_text`가 숫자 참조도 공백 치환 | 공용 `klib::extract::xml_text` 적용: 숫자 참조 정상 디코딩, DTD 및 불법 참조 즉시 거부 |
| 이름 바꾸기 제목 (`rename.rs`) | `xml_text`가 숫자 참조를 공백 치환 | 공용 `klib::extract::xml_text` 적용: HWPX 첫 문단 제목의 숫자 참조 정상 디코딩, DTD/불법 참조 거부 |

## 시험

### 1. 재현 시험

- **DOCX 속성 엔티티 거부 재현 (`klib/tests/test_xml_entities.rs`)**:
  - `docx_attribute_rejects_custom_and_illegal_references`: 속성에 `&secret;`, `&nbsp;`, `&#0;`, `&#x1;`, `&#xD800;`, `&#xFFFE;`, `&#x110000;`, `&#+65;`가 포함된 문서 거부 확인.
  - 수정 전: `extract docx attr custom: 거부해야 함` 단언 실패로 FAILED 재현.
  - 수정 후: 모두 정상 거부되어 PASS.
- **DOCX 속성 정상 디코딩 (`klib/tests/test_xml_entities.rs`)**:
  - `docx_attribute_decodes_standard_and_numeric_references`: 표준 5종 및 정상 숫자 참조 속성 디코딩 및 본문 추출 성공 확인.
- **자료함 색인 단위 시험 (`klib/src/content.rs`)**:
  - `xml_security_and_decoding_in_content`: DTD 거부, 모르는 엔티티 거부, 숫자 참조(`&#xAC00;&#44;&#x20;&#128512;&amp;` → `가, 😀&`) 정상 추출 검증 통과.
- **이름 바꾸기 단위 시험 (`klib/src/rename.rs`)**:
  - `hwpx_title_decodes_numeric_references_and_rejects_dtd_and_illegal`: HWPX 첫 문단 제목의 숫자 참조 정상 디코딩 및 DTD/모르는 엔티티 거부 검증 통과.

### 2. klib 크레이트 전체 cargo test (macOS 기준선 비교)

- 타깃 디렉토리: `CARGO_TARGET_DIR=$HOME/cargo-target-KLIBENT2`
- 명령어: `cargo test --manifest-path klib/Cargo.toml --no-fail-fast`

**수정 전 베이스라인 결과:**
```text
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.41s
test result: FAILED. 75 passed; 45 failed; 1 ignored; 0 measured; 0 filtered out; finished in 5.05s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

**수정 후 결과:**
```text
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.24s
test result: FAILED. 86 passed; 45 failed; 1 ignored; 0 measured; 0 filtered out; finished in 5.42s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

**실패 목록 비교 분석:**
- 맥 환경 베이스라인 45건 실패 목록과 수정 후 45건 실패 목록을 전수 대조(Set Difference)한 결과, 실패 목록의 변동이 0건으로 정확히 일치함(새로운 실패 없음).
- 신규 추가된 단위 시험 및 통합 시험(8건 + 2건 + 11건) 정상 통과.

### 3. 관련 Python 모듈 시험

- `python3 -m unittest tests/test_file_rename_core.py`:
  ```text
  Ran 28 tests in 0.025s
  OK
  ```
- `python3 -m unittest tests/test_timemachine_extractors.py`:
  ```text
  Ran 20 tests in 0.106s
  OK
  ```
- (맥 실행 환경 지침에 따라 Windows COM, 전체 Python 스위트, Flutter 시험은 제외함)
- `tests/fixtures/timemachine_extract/sample.*` 파일은 시험 후 원래 상태로 복구 확인 완료.
