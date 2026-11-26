# SVCCHECK1 결과 — 색인 서비스 호환성·목록 조회

2026-11-26 승인 카드, 2026-11-27 재개·검증. 브랜치 `feature/SVCCHECK1`.

## 판정

- 구현·관련 시험·합성 측정 완료. 로컬 커밋만 수행하며 푸시하지 않는다.
- 변경 Rust 크레이트 전체: kfind 51건, kfind-names 33건 통과. 별도 합성 측정 시험 1건 통과.
- 관련 Python 93건 실행, 실패·오류 0(기존 조건부 제외 3). Flutter 분석 이상 없음, 관련 위젯 25건 통과. 설치 스크립트 합성 컴파일 종료 코드 0.
- 합성 400,000항목에서 같은 8,000건을 읽는 시간: **169.503ms → 38.300ms**. 양쪽 모두 2쪽이며 경로·이름·순서가 일치한다.
- 설치된 서비스는 중지·재시작·교체하지 않았다. 실제 사용자 문서·사진을 이용한 시험·측정은 하지 않았다.

## 선행 병합

`feature/SCANSPEED1` 커밋 `1523ea96`은 현재 HEAD의 조상이다. 해당 커밋의 결과 문서에 시험 통과가 기록되어 있음을 확인했다. 선행 전체 시험은 재실행하지 않았다.

시작 시 실행한 `git merge feature/SCANSPEED1`의 실제 출력:

```text
Already up to date.
```

## 구현과 상태 전이

- `names_status`에 프로토콜 버전 2, 기능·명령 목록을 추가했다. 기능 광고가 없는 이전 서비스는 작은 `names_list` 요청으로 지원 여부를 확인한다.
- 호환 상태는 한곳에서 `unknown → ok / outdated / unavailable`로 관리한다. 지원하지 않는 명령과 일시 장애를 구분한다. 외부 오류 원문은 화면·로그에 남기지 않는다.
- `names_list`가 없으면 이전 버전 안내와 함께 기존 파일 검색으로 폴백한다. 목록 명령은 되지만 `metadata_v2` 기능이 없는 이전 서비스도 안내를 표시하며, 사용 가능한 이전 목록 응답은 계속 받는다. 기능 광고 없는 서비스에 버전 번호를 임의로 붙이지 않는다.
- 문서뷰·사진뷰에 이전 버전 안내, 재시작 확인 대화상자, Windows 서비스에서 직접 재시작하는 방법을 표시한다. 브리지는 `confirmed=true` 없이는 재시작하지 않는다.
- 재시작 도우미도 `--help`에 `--restart`가 있는지 먼저 확인한다. 옛 실행 파일이 모르는 인자에 도움말+종료 코드 0을 반환해도 성공으로 오인하지 않는다.
- `kfind --restart`는 한 번의 UAC 승인 안에서 중지 완료를 기다린 뒤 시작하고 Running 상태를 확인한다. 중지 실패·시간 초과 뒤에는 시작하지 않는다. 성공 뒤 호환 상태를 unknown으로 돌리고 기능을 재확인한다. 실제 데이터 금지 표시는 기능 확인·재시작에도 적용한다.
- 설치 스크립트는 INSTALL3의 잠긴 실행 파일 이름 변경·복원 절차를 유지한다. 대화형 설치에서 사용자가 동의하면 새 실행 파일의 `--restart`를 호출하고 실제 프로세스 종료 코드까지 확인한다. 무인 설치는 권한 상승·재시작을 자동 실행하지 않으며 앱에서 사용자가 선택한다. 재시작 후 옛 실행 파일을 정리하되 여전히 잠겨 있으면 보존한다.

## 목록·메타데이터

- 기존 구현도 offset/next(원시 색인 번호)로 이어 읽었다. 쪽마다 처음부터 다시 훑는 구조는 아니었다. 이 커서를 유지하고 확장자 ID로 먼저 선별하여 비대상 파일의 문자열 처리 비용을 줄였다. 마지막 확장자 원문 검사도 남겨 정규화 충돌로 다른 확장자가 섞이지 않게 했다.
- 서버에 이미 있는 예약 폴더 제외를 유지하고, 클라이언트의 루트×예약 폴더 중복 제외 목록을 없앴다.
- 기존 path/name에 ext/size/mtime/mtime_ns/cloud를 추가했다. FILETIME을 Unix 초·나노초로 변환하고 알려지지 않은 크기·시각은 0으로 반환한다. 이전 요청 형식과 메타데이터 없는 이전 응답도 허용한다.
- 기존 색인은 REPARSE만 보존해 cloud 전용 여부를 정확히 구분할 수 없었다. kfind-names의 기존 u8 표지에서 비어 있던 비트 64에 OFFLINE·RECALL 분류를 보존한다. Python의 기존 cloud_only 기준과 같으며 일반 로컬 REPARSE 파일은 cloud=false다. 항목당 메모리를 늘리지 않고 USN 변경·compact·세그먼트 왕복으로 분류를 보존한다.
- 사진은 변경 없는 DB 행의 촬영일·회전을 보존한다. 신규·변경 로컬 사진만 EXIF를 읽으며 cloud 전용 행은 읽지 않는다.
- 변경 파일의 CRLF 속성을 지정해 작업 사본의 UTF-8·CRLF와 Git 저장 형식(LF)을 함께 유지했다. 본체 스크립트는 수정하지 않았다.

## 실제 검증 출력

시작 때 PM 전체 시험 잠금이 있어 빌드·시험·측정을 보류했다. 잠금 해제를 확인한 뒤 실행했으며 잠금 파일을 변경하지 않았다. 지정된 CARGO_TARGET_DIR을 사용했다. 시스템 영구 설정은 변경하지 않았다.

### Rust 전체 — kfind

`cargo +stable-x86_64-pc-windows-gnu test --manifest-path crates/kfind/Cargo.toml`

```text
test result: ok. 46 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 3.11s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test names_list_cursor_metadata_and_capabilities_over_isolated_pipe ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.39s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.48s
CARGO_FINAL_EXIT=0
```

측정 1건은 전체 시험 중에는 ignored로 분리하고 아래에서 단독 실행했다. 실제 파이프 시험은 고유한 이름의 사용자 모드 파이프와 합성 파일만 사용했다. 설치된 서비스에 접속하지 않는다.

### Rust 전체 — kfind-names

`cargo +stable-x86_64-pc-windows-gnu test --manifest-path kfind/names/Cargo.toml`

```text
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test cloud_flags_survive_updates_compaction_and_segment_roundtrip ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 26.59s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
NAMES_TEST_EXIT=0
```

### 관련 Python

앱 기준 가상환경 Python 3.14.0 사용(작업 사본에 .venv가 없어 공유 개발 저장소의 .venv 인터프리터를 사용하고 현재 작업 사본에서 실행했다).

`python -m unittest tests.test_svccheck1 tests.test_scan_cache_service tests.test_kfind_process tests.test_scan_stream tests.test_default_library_scan tests.test_doc_grid_service tests.test_doc_grid_bridge tests.test_photo_album_service`

```text
Ran 93 tests in 30.693s
OK (skipped=3)
PYTHON_FINAL_EXIT=0
```

제외 3건은 klib 실행 파일 또는 기존 시험 자료가 없는 조건이다. 새 Python 시험은 제외 없이 통과했다. 서비스 판별·메타데이터·커서·실제 데이터 금지·확인 없는 재시작 거절·옛 도우미의 거짓 성공 방지·EXIF 보존을 확인했다.

### Flutter

`flutter analyze`

```text
No issues found! (ran in 32.5s)
ANALYZE_EXIT=0
```

`flutter test test/service_version_notice_test.dart test/doc_grid_screen_test.dart test/photo_album_screen_test.dart test/library_scan_stream_test.dart --reporter expanded`

```text
02:18 +25: All tests passed!
FLUTTER_TEST_EXIT=0
```

### 설치 스크립트

[Code]를 그대로 둔 합성 꾸러미를 Inno Setup으로 컴파일했다. 파일 원천·출력 위치·압축·아이콘 상대 경로만 시험용으로 바꾸었다. 설치 프로그램은 실행하지 않았다.

```text
INSTALLER_COMPILE_EXIT=0
```

### 빌드 환경 실패 기록

처음 기본 GNU 도구 체인에서는 PATH에 dlltool이 없었다. MSVC 시도는 Git의 동명 link 도구를 찾아 실패했다. 이미 설치된 MSYS2 mingw64 도구를 현재 명령의 PATH에 추가해 해결했다. 새 도구 설치나 영구 환경 설정 변경은 하지 않았다.

```text
error: error calling dlltool 'dlltool.exe': program not found
CARGO_EXIT=101
error: linking with `link.exe` failed: exit code: 1
CARGO_MSVC_EXIT=101
```

## 합성 전후 측정 — 각 1회

`cargo test --release --no-run --lib`로 먼저 측정 실행 파일을 만들었다. 모든 빌드·시험 종료 및 PM 잠금 부재를 확인한 뒤 release 시험 실행 파일에서 `names::tests::benchmark_names_list_before_after --exact --ignored --nocapture --test-threads=1`을 한 번 실행했다.

```text
RELEASE_FINAL_BUILD_EXIT=0
SVCCHECK1 synthetic=400000 matches=8000 before_pages=2 after_pages=2 before_ms=169.503 after_ms=38.300 identical_paths=true
test names::tests::benchmark_names_list_before_after ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 46 filtered out; finished in 0.46s
BENCHMARK_EXIT=0
```

같은 메모리 색인에서 수정 전 구현과 최종 구현을 순서대로 각각 한 번 읽었다. 색인 생성·파이프·화면·실제 파일 읽기는 이 시간에서 제외한다. 원래 경로·이름·순서·개수를 대조하며 새 메타데이터 검증은 기능 시험으로 분리했다. 결과에는 파일명·경로를 출력하지 않는다. 단일 측정이므로 캐시 영향이 있을 수 있으며 실제 설치본 화면 시간으로 일반화하지 않는다.

## 확인하지 않은 범위·PM 후속

- 설치된 서비스의 실제 중지·재시작·교체 및 실제 UAC 실행은 금지 범위라 하지 않았다.
- 실제 설치·업데이트·롤백은 실행하지 않았다. 설치 스크립트 문법과 Rust SCM 순서·실패 처리는 합성 컴파일 및 가짜 SCM으로 검증했다.
- 무인 업데이트 뒤에는 사용자 확인 재시작이 필요하다. 새 서비스가 준비되는 동안 기존 파일 검색으로 목록을 표시할 수 있다.
- 사용자 문서·사진을 이용한 속도 측정 및 실제 WebView 조작은 하지 않았다.
- Python·Flutter 전체 시험과 릴리스 판정은 PM 담당이다. 이번에는 관련 모듈·위젯만 실행했다.
