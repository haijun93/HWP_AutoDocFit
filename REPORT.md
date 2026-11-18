# OUTLINER5 작업 보고서

## 1. 작업 개요
- **작업명**: OUTLINER5 — 아웃라이너 엔진: 블록 참조 해석·페이지 이름 변경 시 링크 갱신
- **작업 대상**: `outliner_core` (Rust, 플랫폼 독립 크레이트)
- **주요 요구사항**:
  1. `((블록ID))` 참조 해석 (`id:: UUID` 속성을 가진 블록의 본문 조회 및 역참조 목록 반환, 없는 ID는 오류 대신 빈 결과 반환)
  2. 페이지 이름 변경 시 다른 페이지의 `[[옛이름]]` 링크를 `[[새이름]]`으로 일괄 갱신하는 연산 (원본 파일은 임시 파일 후 교체, 미리보기 모드로 바뀔 파일 목록만 반환하는 옵션)
  3. 둘의 JSON CLI 명령 및 시험 (한글 페이지 이름 지원, 별칭·중첩 링크·코드 블록 안 링크는 건드리지 않음, 기존 시험 삭제·약화 금지, 새 외부 의존성 금지)

---

## 2. 구현 상세

### 2.1 블록 참조 해석 (`outliner_core::resolve_block_ref`, `BlockRefResolution`)
- `outliner_core/src/index.rs`:
  - `BlockRefResolution` 구조체 정의 (`found`, `text`, `content`, `block_id`, `uuid`, `backlinks`).
  - `resolve_block_ref(document, uuid)` 함수 구현:
    - 문서 내 블록의 `properties`에서 `id` 속성값이 `uuid`와 일치하는 대상 블록 탐색.
    - `backlinks(document, uuid)`를 통해 해당 UUID를 `((UUID))` 형태로 역참조하는 블록 ID 목록 조회.
    - 일치하는 블록이 존재하면 본문 텍스트와 역참조 목록을 반환.
    - 존재하지 않는 ID인 경우, 오류를 발생시키지 않고 `found: false`, 빈 본문(`""`), 빈 역참조 목록(`[]`)의 빈 결과를 반환.

### 2.2 페이지 이름 변경 및 링크 일괄 갱신 (`outliner_core::rename`)
- `outliner_core/src/rename.rs`:
  - `update_line_links(line, old_name, new_name)`:
    - **한글 페이지 이름 지원**: Unicode scalar/UTF-8 문자 경계를 안전하게 처리.
    - **별칭 링크 보존**: `[[옛이름|별칭]]` 등 파이프(`|`)가 포함된 링크 및 `[별칭]([[옛이름]])` 마크다운 형태는 건드리지 않음.
    - **중첩 링크 보존**: `[[[[옛이름]]]]`, `[[[옛이름]]]`, `[[...[[옛이름]]...]]` 등 중첩 링크는 건드리지 않음.
    - **인라인 코드 보존**: 백틱(`` `...` ``)으로 둘러싸인 인라인 코드 내의 `[[옛이름]]`은 건드리지 않음.
  - `update_page_links_text(content, old_name, new_name)`:
    - 펜스 코드 블록(``` 또는 ~~~) 내부 줄은 건드리지 않고 그대로 보존.
    - CRLF(`\r\n`), LF(`\n`), 후행 개행 유무 등 원래 줄바꿈을 바이트 단위로 정확히 보존.
  - `rename_page_in_files(paths, old_name, new_name, preview)`:
    - 대상 파일들에서 `[[옛이름]]` 링크를 `[[새이름]]`으로 갱신.
    - `preview: true` (미리보기 모드): 파일을 디스크에 쓰지 않고 변경될 파일 목록(`affected_files`)과 개수(`count`)만 반환.
    - `preview: false` (실행 모드): `save_atomic`을 호출하여 같은 디렉터리의 임시 파일에 쓰고 원자적으로 원본 파일을 교체.
  - `collect_markdown_files(path)`: 단일 파일 및 디렉터리 내의 `.md` 파일을 재귀 탐색 수집.

### 2.3 JSON CLI 인터페이스 연동 (`outliner_core::api`)
- `outliner_core/src/api.rs`:
  - `"resolve_block_ref"` (별칭: `"block_ref"`):
    - `document` 또는 `args.path`에서 문서를 읽고 대상 `uuid` 해석 수행.
    - 성공 시 `{"found": bool, "text": String, "block_id": Option<u64>, "uuid": String, "backlinks": [u64]}` 반환.
  - `"rename_page"` (별칭: `"update_links"`):
    - `old_name`, `new_name`, `preview` (또는 `dry_run`), `files`/`path`/`directory` 또는 단일 `document` 문자열 처리.
    - 단일 문서 문자열 및 다중 파일 일괄 원자적 교체 및 미리보기 지원.

### 2.4 의존성 및 기존 시험 보존
- 새로운 외부 크레이트 추가 없음 (`Cargo.toml` 그대로 유지: `serde`, `serde_json`).
- 기존 45개 유닛 테스트 및 1개 CLI 통합 테스트를 하나도 삭제하거나 약화시키지 않고 100% 보존.
- 신규 유닛 테스트 8개 및 CLI smoke 테스트 케이스 추가 (총 54개 테스트 통과).

---

## 3. 시험 결과 (`cargo test`)

실행 명령:
```bash
CARGO_TARGET_DIR=$HOME/cargo-target-OUTLINER5 cargo test --manifest-path outliner_core/Cargo.toml
```

모든 `test result` 줄:
- `test result: ok. 53 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s`
- `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s`
- `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.29s`
- `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s`
