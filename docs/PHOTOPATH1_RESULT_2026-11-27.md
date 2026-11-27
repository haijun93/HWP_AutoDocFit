# PHOTOPATH1 결과 — 사진 DB 경로 별칭 조회 (2026-11-27)

## 판정

합성 자료에서 불일치를 확인하여 조회를 보완했다. Python 단건 API는 이미 `Path.resolve()`를 사용하므로 정션을 전혀 풀지 않는다는 최초 가설은 단건 API에는 해당하지 않았다. 그러나 묶음 저장/목록 저장은 입력 경로를 그대로 저장했고, Rust 쓰기는 부모만 풀고 파일명은 보존한 뒤 SQLite의 정확한 문자열로 행을 찾았다. Rust 표시 열쇠는 실제 경로를 풀지 않았다.

- 수정 전 `PhotoStore` 재현: 정션 경로의 기존 행 1개에 같은 파일의 실제 경로를 갱신하면 2행. `Ran 1 test in 3.136s / FAILED (failures=1)`.
- Rust 수정 전 왕복: 4개 시험에서 실패 10건. 정션 행 표시 실패, 파일명 대소문자 입력으로 별도 행 생성, 기존 별칭 행 재사용 실패 등을 확인했다. 이 실행에는 Python 단건 조회 보완이 먼저 들어갔으며 Rust는 원래 코드였다. `Ran 4 tests in 12.792s / FAILED (failures=10)`.
- DB 재사용 수정 후에도 목록 비교 캐시가 별칭을 새 사진으로 보아 두 항목을 표시하는 재현을 추가했다. `Ran 1 test in 3.645s / FAILED (failures=1)`. 원천 입력을 보관 중인 같은 사진에 연결하여 이 경우도 수정했다.
- Python→Rust, Rust→Python 순서와 기존 원시 별칭 행을 각각 시험한다. 제품 버전 번호에 대한 고정 기대값은 없다.

## 수정 범위

- Rust `library.rs`: 기존 문자열 열쇠를 유지하고, 별칭용 실제 경로 열쇠를 추가. 기존 경로를 정확히 찾지 못하면 같은 파일의 기존 `photos.path`를 조회하여 태그 추가/제거·즐겨찾기 쓰기에 사용한다. 저장 형식·스키마·원래 행의 경로를 바꾸지 않는다.
- Rust `marks()`: 같은 파일의 표시를 메모리에서 합쳐 즐겨찾기는 OR, 태그는 중복 없는 목록으로 읽는다. DB에는 쓰지 않는다. `extras.rs`와 태그 개수 계산도 별칭 조회를 사용하며, 보통 경로는 기존 문자열 조회를 먼저 한다.
- Python `photo_paths.py`: 정확한 경로 우선, 실패하면 실제 경로를 비교. 묶음 작업의 별칭 색인은 한 번만 만든다. `PhotoAlbumDb`의 단건/묶음 갱신·조회·표시 편집이 이 도우미로 기존 행을 재사용한다.
- Python `PhotoStore`·`PhotoViewSync`: 목록 갱신과 표시 새로 읽기에서 기존 별칭 행을 재사용한다. 같은 파일의 촬영일·회전 정보도 기존 보관 값에서 찾는다. 원천 입력도 보관 중인 같은 사진에 연결하여 목록과 그 표시가 두 벌로 갈라지지 않게 한다.

경로 정리를 위한 `DELETE`, 경로 `UPDATE`, 태그 이동, 별칭 DB 마이그레이션은 추가하지 않았다. 태그 제거·즐겨찾기 변경은 시험이 명시적으로 요청한 편집이다. 기존 파일 이동/이름 변경 처리와 기존 `PhotoStore.load()`의 대소문자 중복 정리는 수정하지 않았다. 해당 기존 시험도 그대로 실행한다.

## 합성 시험 범위

| 조건 | 확인 내용 |
|---|---|
| 실제 경로·정션·대소문자·SUBST·확장 경로 접두사 | Python 선저장/Rust 선저장, 서로 태그 읽기/쓰기, 즐겨찾기 변경, 원래 행 수/경로 유지 |
| 각 별칭으로 저장된 기존 행 | 양쪽에서 실제 경로로 찾아 태그 추가, 경로를 옮기지 않음 |
| 누락 파일 | 부모는 접근 가능한 상태에서 기존 행/태그를 조회, DB 파일 바이트 불변 |
| 목록 묶음/단건 갱신 | 새 중복 행 없음, 같은 파일의 촬영일·회전 보존, 생성기 입력 지원 |
| 기존 중복 행(Rust) | 읽는 동안 두 행·세 태그·DB 파일 바이트 그대로, 표시만 합침 |
| 이름만 같은 다른 폴더 | 별개의 파일로 유지 |
| 정션 TEMP/TMP | 동일한 Rust 전체·관련 Python 시험을 다시 실행 |
| 8.3 | 시험 볼륨에서 `GetShortPathNameW`가 긴 경로를 그대로 반환. 실제 짧은 별칭 왕복은 미확인. 볼륨 설정을 변경하지 않음 |

`tests/test_photopath1.py` 9개와 Rust `tests/path_aliases.rs` 3개를 추가했다. 기존 시험은 수정·삭제·완화하지 않았다.

## 실행과 결과

작업 사본에는 가상환경이 없어 공유 개발 가상환경 Python으로 실행했다. 산출물·임시 폴더는 저장소 밖의 합성 시험 폴더에만 둔다. MinGW 경로는 실행 환경에서만 지정하며, 저장소의 Cargo 설정에는 절대 경로를 넣지 않는다.

```text
python scripts/test_photopath1_synthetic.py --artifacts <합성 시험 폴더> --klib <개발용 klib.exe>
python scripts/test_photopath1_synthetic.py --artifacts <같은 합성 시험 폴더> --klib <개발용 klib.exe> --junction
```

실행기는 `LOCALAPPDATA`·`APPDATA`·`USERPROFILE`·`HOME`·`KAGENT_DATA`를 합성 폴더로 격리한다. `DOCFIT_FORBID_REAL_DATA=1`을 사용하고 실제 휴지통 시험 옵션은 넘기지 않는다. 앱 설치본·설치 색인 서비스·자동 실행·사용자 설정을 변경하지 않았다. COM은 사용하지 않았다.

최종 Python 시험에서는 사진대지 시험의 기본 HWPX 입력도 기존 합성 템플릿 생성기로 공급했다. 기존 시험의 검증 조건은 그대로 유지했다. 초기 직접 모듈 시험은 프로그램에 동봉된 기본 템플릿을 사용했으며 사용자 문서는 사용하지 않았다.

Rust 기존 취소 시험의 고정 드라이브 인수는 실행기에서 4,096개 폴더·65,536개 합성 파일의 트리로 바꿔 **실제 klib**에 전달한다. 나머지 합성 루트는 그대로 전달하며, 샌드박스 밖 스캔은 거부한다. stdout·종료 코드·취소 판정·300ms 이전 미완료 검사는 바꾸지 않았다. Job Object가 취소 시 실제 klib도 종료한다. 첫 실행에서는 제한 도구가 기존 ‘없는 루트’ 시험을 거부하여 실패했다. 해당 입력도 합성 폴더 아래의 없는 경로로 전달하도록 도구를 수정했다. 제품/기존 시험 실패를 숨기거나 제외한 것이 아니다.

일반 TEMP: `photoview_win` 전체 73개 통과·기존 제외 1개. 관련 Python 8모듈 93개 중 92개 통과·기존 실제 휴지통 시험 1개 제외. Rust 제외는 기존 사진 처리 시간 측정이다.

일반 TEMP 원문(`src/main.rs`·doc-test는 각 0개):

```text
src/lib.rs
test result: ok. 23 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 5.13s
capture_tests
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 15.29s
exclude_tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.24s
extras_capture_tests
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.39s
mergerust_library
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
path_aliases
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
pure_tests
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.84s
real_klib_tests
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.54s
Ran 93 tests in 38.000s
OK (skipped=1)
```

Python 대상: `test_photopath1`, `test_photo_library`, `test_photo_album_service`, `test_photofix1`, `test_photoview_library_schema`, `test_photoview_window_service`, `test_library_db`, `test_fileops1`.

정션 TEMP도 같은 범위 통과. 시작 시 `TEMP_IS_JUNCTION=True`를 확인했고 종료 후 정션 자체만 제거했다.

```text
TEMP_IS_JUNCTION=True
src/lib.rs
test result: ok. 23 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 2.99s
capture_tests
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.84s
exclude_tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.75s
extras_capture_tests
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.26s
mergerust_library
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
path_aliases
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.17s
pure_tests
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.88s
real_klib_tests
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.84s
Ran 93 tests in 59.232s
OK (skipped=1)
```

## 한계와 후속 설계

- 실제 8.3 별칭 생성이 가능한 합성 볼륨, UNC 공유, 큰 사용자 DB, MSVC/CI, 실제 설치본과 UI 수동 동작은 미확인이다. 전체 Python discover는 실행하지 않고 관련 8모듈을 실행했다. 성능 개선을 측정하거나 주장하지 않는다.
- 별칭이 끊겨 대상 자체를 확인할 수 없는 경우에는 같은 파일임을 새로 입증할 수 없다. 행은 보존한다. 이 경우까지 영구 대응하려면 접근 가능할 때의 별칭→실제 경로/파일 식별 정보를 별도 보관하고 읽기에서 참조하는 설계가 필요하다. 스키마 변경과 기존 DB 마이그레이션은 수행하지 않았다.
- 이미 여러 행이 있고 서로 다른 즐겨찾기/태그를 가진 경우 Rust 읽기는 표시를 합치되 행은 그대로 둔다. 쓰기는 정확한 기존 경로를 우선하고, 없으면 경로순 첫 일치 행을 재사용한다. 기존 중복 행의 영구 병합이나 표시 편집을 모든 중복 행에 전파하는 정책은 이번 범위에 넣지 않았다.
- 정확한 경로가 없을 때의 조회는 기존 경로를 실제 경로와 비교한다. Python 묶음은 색인을 재사용하지만 별칭이 많은 대규모 DB의 비용은 별도 검증이 필요하다.

기능 브랜치에서 로컬 커밋만 남긴다. 푸시·태그·릴리스·GitLab 접근은 하지 않는다.
