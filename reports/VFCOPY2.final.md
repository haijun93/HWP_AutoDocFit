# VFCOPY2 완료 보고

- 브랜치: `feature/library-area-copies`
- 기준 커밋: `da02fce8`
- 작업 커밋: `96d29abfa1611a7c99be7a2fcd44a75489828cdf`
- 원격: `github/feature/library-area-copies` 확인 결과 같은 해시. `git push github feature/library-area-copies` 결과 `Everything up-to-date`.
- 변경: `docs/VFCOPY_AREA_COPIES_DESIGN.md`, `klib/tests/test_klib.rs`. 기존 항목 지연 이전, 관리 복사본 이동·삭제, 원본 보호, 충돌 및 안전하지 않은 내보내기 회귀 시험을 문서화/보강.

## 시험 결과

- `python -m unittest discover -s tests`: 실패. `Ran 2633 tests in 624.378s; FAILED (failures=1, errors=33, skipped=7)`. 실패 상세 중 확인된 차단 요인은 현재 Python 환경에 `cryptography` 모듈이 없어 `test_review_regressions`의 동적 import gate가 실패한 것. 다른 ERROR는 `test_file_format_mode`, PDF, GUI/업데이트 등 여러 시험에서 발생했으며 이 작업 변경 파일과 관련된 오류인지는 확인되지 않음.
- `flutter test flutter_ui/test/` (Flutter 프로젝트 폴더에서 `flutter test test/`로 실행): 실패. 506개 성공, 1개 suite 로딩 실패(`timemachine_excluded_name_test.dart`: `Connection closed before test suite loaded`), 이후 8개 항목이 미완료 처리됨.
- `cargo test --manifest-path klib/Cargo.toml`: 통과. 바이너리 시험 50개 및 통합 시험 76개 통과.
- `git diff da02fce8..96d29abf --check`: 통과.

Python 전체 시험은 기준 커밋 이후 이 시험 세트로 재실행된 비교 결과가 없어, 보고서에서는 시험 실패를 숨기지 않고 기록함.
