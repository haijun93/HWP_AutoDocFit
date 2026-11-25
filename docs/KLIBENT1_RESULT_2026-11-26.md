# KLIBENT1 결과 — klib 본문 추출의 XML 엔티티 처리

- 브랜치 `feature/KLIBENT1`(기준 `github/main`), 커밋만 하고 푸시하지 않음.
- 발단: KORDOCD1 의 extract/HWPX 비교에서 klib 4/5. `&amp;` 가 든 정상 입력 1건 거부(해당 결과 문서는 `feature/KORDOCD1` 브랜치에 있음).

## 원인

quick-xml 0.41 은 `&amp;` 같은 표준 참조도 `GeneralRef` 이벤트로 넘기는데, `klib/src/extract.rs` 가 이 이벤트를 모두 거부했다.

- HWPX 본문(`extract_section`)뿐 아니라 DOCX(`parse_xml_tree`)도 같은 이유로 거부했다.
- 색인 경로의 `klib/src/legacy_records.rs` 는 숫자 참조만 풀고 `&amp;` 는 글자 그대로 남겼다. 색인 추출(`extract_for_index`)은 이 글로 본문을 덮어쓰므로 앞의 거부만 고치면 `&amp;` 가 그대로 색인된다. DTD 도 거부하지 않았다(`legacy-records` 명령으로 직접 호출 가능).

## 고친 것

공용 함수 `resolve_xml_ref` 를 세 파서가 함께 쓴다.

| 입력 | 처리 |
|---|---|
| `&amp; &lt; &gt; &quot; &apos;` | 해당 글자로 풂(한 번만, `&amp;lt;` → `&lt;`) |
| 숫자 참조(10진·16진) | XML 1.0 허용 문자만 풂 |
| 그 밖의 숫자(0, 제어 문자, 대리 영역, U+FFFE, 범위 밖, 부호 붙은 수) | 거부 |
| 사용자 정의·HTML 이름 참조(`&nbsp;` 등) | 거부 |
| DTD(`DOCTYPE`), 내부·외부 엔티티 선언 | 거부 |

동작 변화: `legacy-records` 와 HWPX 색인 추출은 이제 DTD·정의되지 않은 참조가 있으면(`Contents/header.xml` 포함) 실패한다. 전에는 글자 그대로 남기고 지나갔다.

## 시험

합성 문서만 썼다. 새 시험 `klib/tests/test_xml_entities.rs` 는 실행 파일의 `extract`·`legacy-records` 명령을 부른다. 거부 사례 12종은 HWPX 두 경로와 DOCX 에서 확인한다.

수정 전 재현:

```text
test result: FAILED. 0 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out
```

수정 후 klib 크레이트 전체 `cargo test --no-fail-fast`(gnu 툴체인, 디버그):

```text
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.51s
test result: ok. 132 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 26.49s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.18s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.52s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

전체 `python -m unittest discover -s tests`(Python 3.15 venv, `KLIB_EXE` 미지정):

```text
Ran 3748 tests in 826.475s
OK (skipped=47)
```

전체 시험은 `tests/fixtures/timemachine_extract/sample.{docx,hwpx,pptx,xlsx}` 를 다시 써 놓는다. 커밋 전에 되돌렸다.

추가 확인 — 새로 빌드한 klib 를 `KLIB_EXE` 로 지정한 연동 시험 6개 모듈:

- release 빌드: `Ran 33 tests in 36.656s` / `OK`. `test_semi2_backend_integration` 8회 반복도 모두 OK.
- 디버그 빌드: `Ran 33 tests in 39.572s` / `FAILED (errors=1)`. 아래 남은 문제 1.

## 남은 문제

1. 디버그 빌드의 `search-content` 가 `test_semi2_backend_integration` 에서 간헐적으로 죽는다(6회 반복 중 4회 실패). stderr 는 tantivy 0.27.0 `segment_postings.rs:173` 의 `debug_assert!(self.doc() <= target)` 실패다. 입력이 md·txt 라 이번에 고친 코드를 지나지 않으므로 기존 문제로 판단하지만, 수정 전 바이너리로 재현해 보지는 않았다. release 에서는 이 단언이 빠지므로 통과가 검색 결과의 정확성까지 보장하는지는 확인하지 못했다.
2. 자료함 색인의 `content.rs` 는 자체 훑기를 쓴다. DTD 를 거부하지 않고(엔티티를 확장하지도 않음) 모르는 참조는 공백으로 바꾼다. `content.rs`·`rename.rs` 의 `xml_text` 는 숫자 참조도 공백으로 바꾼다. 이번 범위에서 고치지 않았다.
3. DOCX 파서(`parse_xml_tree`)는 속성 값의 정의되지 않은 참조를 오류 없이 그 속성만 버린다(기존 동작). HWPX 본문 파서는 속성을 읽지 않는다.
4. KORDOCD1 의 HWPX 5건 비교(품질·시간)는 다시 돌리지 않았다. 설치본의 klib 에는 다음 빌드·릴리스 때 반영된다.
