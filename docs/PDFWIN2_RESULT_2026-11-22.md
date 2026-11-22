# PDFWIN2 결과 — `_classify` Rust 이식 + 인메모리 다중 편집 파이프라인 (2026-11-22)

설계 문서 `docs/PDF_EDIT_WIN32_DESIGN_2026-11-22.md` 7.1절 Phase 2 구현. 기존 Python·Flutter 경로와 `pdf-edit` CLI 동작은 바꾸지 않았다(공개 함수 시그니처·결과 동일, 내부만 함수 분리).

## 1. 변경 내용

| 파일 | 내용 |
|---|---|
| `pdf_edit_core/src/pdf_classify.rs` (신규) | Python `PdfEditService._classify` 이식. 판정 순서·오류 코드·한국어 메시지 동일. `classify_path` / `classify_bytes` / `check_size` |
| `pdf_edit_core/src/pipeline.rs` (신규) | `EditSession`(열기·`apply`/`apply_all`·`page_info`·`save`·`to_bytes`·`merge`)와 `EditOp`(Rotate/Delete/Reorder). 문서 하나를 메모리에 두고 연달아 적용, 저장은 최종 1회 |
| `pdf_edit_core/src/lib.rs` | `load`→`validate_doc`, `delete_doc`/`rotate_doc`/`page_info_of` 분리(공개 API 동일). `rotate_doc`을 검증→적용 2단계로 바꿔 실패 시 문서 불변 |
| `pdf_edit_core/Cargo.toml`·`Cargo.lock` | 직접 의존성 `md-5 0.10`, `sha2 0.10`, `aes 0.8` 추가 (열기 암호 검증용, §3 참조) |
| `pdf_edit_core/tests/classify_parity.rs`, `tests/fixtures/classify/` | Python 대조 시험 + 합성 PDF 35종과 Python 판정 `expected.json` |
| `pdf_edit_core/tests/pipeline.rs` | 파이프라인 단위 시험 7개 |
| `tools/gen_classify_fixtures.py` | 합성 PDF·기대값 생성기(pypdf 6.21.4 기준) |
| `tests/test_pdf_classify_fixtures.py` | `expected.json`이 현재 Python `_classify`와 일치하는지 확인(기대값 노후 방지) |

## 2. 시험 결과 (실제 출력)

`pdf_edit_core`에서 `cargo test --release --locked`:

```
test result: ok. 3 passed; 0 failed   (lib 단위)
test result: ok. 0 passed; 0 failed   (bin pdf-edit)
test result: ok. 2 passed; 0 failed   (classify_parity)
test result: ok. 7 passed; 0 failed   (pipeline)
test result: ok. 6 passed; 0 failed   (safety, 기존)
test result: ok. 0 passed; 0 failed   (doc-tests)
```
합계 18개 통과, 실패 0. 기존 safety 6개·lib 3개는 수정 없이 통과.

Python: `python -m unittest tests.test_pdf_edit_service tests.test_pdf_classify_fixtures` → `Ran 11 tests ... OK` (기존 10 + 신규 1).

### 2.1 Python 대비 차이 0 대조 (36건)
합성 PDF 35종 + 없는 파일 1건의 Python 판정(OK 11, DAMAGED 14, ENCRYPTED 5, RESTRICTED 4, SIGNED 1, UNAVAILABLE 1)과 Rust 판정이 **36건 모두 일치**(차이 0). 범주: 정상(단일·다중·회전·메타데이터·내용 스트림·위젯 주석·증분 갱신·끝 잡데이터 100바이트), 손상(빈 파일·텍스트·헤더만·앞 잡데이터·%%EOF 없음·절반 절단·startxref 오염·xref 오염·endobj 누락·/Kids 훼손·/Length 거짓·CRLF 변환·끝 잡데이터 5000바이트·쪽 0개), 암호화(RC4-40/RC4-128/AES-128/AES-256 각각 열기 암호 없음→RESTRICTED, 있음→ENCRYPTED, `/Encrypt`만 있고 `/P` 없는 원시 바이트), 서명 필드.

### 2.2 대조 중 발견한 차이와 처리 (숨김 없이)
1. **AES-128/256, 열기 암호 없음 + 권한 제한 PDF**: 처음에는 Rust가 `PDF_ENCRYPTED`(Python은 `PDF_RESTRICTED`)로 2건 불일치. 원인: lopdf 0.34는 V1~2/R2~3(RC4)의 빈 암호만 검증. → R2~R6 사용자 암호(빈 암호) 검증을 직접 구현(RC4+MD5, R5 SHA-256, R6 알고리즘 2.B). 수정 후 일치.
2. **trailer에 `/Root` 없는 PDF**: pypdf는 `/Catalog` 객체를 찾아 복구해 OK, 처음 Rust는 `PDF_DAMAGED`. → 같은 복구 동작 구현. 일치. **단, 이 파일은 분류를 통과해도 기존 편집 엔진(`lib.rs load`)에서 실패할 가능성이 있다**(Python 경로와 동일한 기존 동작, 이번 카드에서 바꾸지 않음).

## 3. 알려진 한계 · 남은 문제
- **시험 범위는 합성 PDF 36건**이다. 실제 스캐너·워드 출력물, 객체 스트림/xref 스트림 PDF, 서명 사전(`/ByteRange`) 실문서, 거대 파일(1GB 경계는 `check_size` 단위 시험만)은 대조하지 않았다. "모든 PDF에서 차이 0"을 보장하지 않는다.
- 판정 불가 암호화 사전(필드 누락·미지원 리비전)은 Rust가 `PDF_ENCRYPTED`로 처리한다. pypdf는 일부 경우 예외 → `PDF_DAMAGED`가 될 수 있으나 이 조합은 시험하지 않았다.
- 서명 판정은 Python과 같이 쪽 `/Annots`의 `/FT /Sig`만 본다. 반면 편집 엔진 `load()`는 모든 사전의 `/Sig`를 보므로 엔진이 더 엄격하다(기존 동작).
- **기존 CLI `pdf-edit merge`의 결함 확인**: 합칠 때 쪽 `Parent`를 새 루트로 바꾸면서 상속 속성(Rotate 등)을 복사하지 않아 사라진다(`cli_merge_loses_inherited_geometry_documented_difference` 시험이 현행 동작을 기록). 인메모리 `EditSession::merge`는 상속 속성을 복사해 보존하고 안 쓰는 객체를 정리한다. 쪽 순서는 CLI와 동일(시험 확인). **두 방식 결과는 상속 속성에서 다르다** — CLI 쪽은 이번 카드에서 건드리지 않았고, 교체는 PDFWIN6 몫.
- `EditSession::merge` 로직은 `main.rs`의 합치기와 중복(CLI 불변 유지를 위해 의도적). PDFWIN6에서 CLI를 라이브러리 호출로 통일해야 한다.
- 인메모리 저장(`lopdf::save_to`)은 저장마다 xref 스트림 객체 번호가 늘어나 같은 문서도 바이트가 달라질 수 있다(내용은 동일). 대조는 쪽 순서·크기·회전으로 했다.
- `apply_all`은 중간 실패 시 앞 작업을 유지하고 실패 위치를 돌려준다(전체 롤백 아님). 단일 `apply`는 실패 시 불변.
- Python `PdfEditService`는 아직 Rust 분류기를 부르지 않는다(기본 동작 불변). 연결은 후속 카드.
- `Cargo.lock` 갱신 시 오프라인 레지스트리 때문에 `bumpalo 3.20.4→3.20.3`, `cc 1.11.2→1.11.1`이 함께 내려갔다(`--locked` 빌드·시험 통과). 온라인 환경에서 필요하면 재갱신 가능.
- 취소 플래그·진행률(설계 5.3)은 UI 연결 단계(PDFWIN5)에서 다룬다. 이번에는 구현하지 않았다.

## 4. 재현
```
python tools/gen_classify_fixtures.py        # 합성 PDF·expected.json 재생성(암호화 시료는 매번 달라질 수 있음)
cd pdf_edit_core && cargo test --release --locked
python -m unittest tests.test_pdf_edit_service tests.test_pdf_classify_fixtures
```
cargo는 PATH 맨 앞에 mingw64/bin, 산출물 경로는 `CARGO_TARGET_DIR` 환경변수로만 지정.
