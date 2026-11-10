# HIERX EML 디코딩

- 메일 전체를 UTF-8/CP949로 추측하지 않고 MIME 본문의 바이트를 보존한다.
- 전송 인코딩을 먼저 해제하고 각 파트의 charset으로 디코딩한다.
- quoted-printable의 LF 및 CRLF soft break를 모두 처리한다.
- Python email의 기본 동작처럼 잘못된 문자 바이트는 대체 문자로 바꾸고 알 수 없는 charset은 decode_error로 반환한다.
- 최상위 단일 파트에도 Content-Transfer-Encoding을 적용한다.
- 앱의 Rust 색인 분배에 eml을 포함한다. 실패 시 기존 Python 대체 처리를 사용한다.

기존 작업에서 수정된 tests/fixtures/eml의 세 파일은 덮어쓰지 않았다.
검증: debug 추출 4개 파일 차이 0, 색인 19개 문단의 문서·위계·본문·검색 차이 0.
