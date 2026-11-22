# CONVWIN7 결과 (2026-11-22)

## 구현

- `convert_win`의 PDF 하나로 합치기에서 파일 행을 클릭해 선택하고 `Alt+↑/↓`로 순서를 바꿀 수 있다.
- 순서대로 `convert <file1> <file2> ... --to pdfm --notify` 한 번을 실행한다. 순수 함수 `merge_engine_args`가 경로 순서를 그대로 인자에 기록한다.
- CLI 순서 보존 근거: `docfit_app.cli.expand_inputs`는 `inputs`를 입력 순회 순서대로 확장하고, `docfit_app.conversion_runner.run_conversion`은 `files`를 순회하며 `합칠_부분`에 append한다. 마지막으로 `docfit_core.pdf_merge.merge_to_pdf`가 전달된 `parts`를 순회해 페이지를 추가한다. 그러므로 CLI부터 PDF 페이지까지 파일 입력 순서가 보존된다.
- PDF 압축 설정 표시 요청은 보류했다. `docs/settings_defaults.json` 및 `docfit_config`에 PDF 압축 설정 키가 없다. 저장소에서 확인한 `pdf_compress_target_mb`/`pdf_compress_method` 값은 `desktop_web_ui.py`의 UI 상태 기본값일 뿐 `docfit_config` 저장 키가 아니다. 없는 설정을 새로 만들거나 임의의 현재 값으로 표시하지 않았다.

## 검증

- `cargo test --release --locked`: `test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`.
- Python 전체 시험: 체크아웃에 `.venv\Scripts\python.exe`가 없어 Python 3.14 시스템 인터프리터로 실행했다. 긴 시험 도중 진행 출력이 멈춰 종료했으며, `Ran`/`OK` 결과를 얻지 못했다.
- 관련 Python 시험(시스템 Python 3.14): `test_cli.py` Ran 27 / OK, `test_pdf_merge.py` Ran 9 / OK, `test_file_conversion.py` Ran 22 / OK.
- `flutter analyze`: `No issues found! (ran in 28.7s)`.
- `cargo run --release --locked -- --capture E:\docfit-tmp\convwin7-capture.png`: 성공. 생성된 PNG를 직접 열어 기본 창 화면과 파일 목록을 확인했다.
