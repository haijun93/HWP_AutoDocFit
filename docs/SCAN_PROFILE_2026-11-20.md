# 문서뷰·사진뷰 스캔 프로파일과 Rust 1단계 결과

작성: 2026-11-21 (카드 기준일 2026-11-20)

## 범위와 측정 방식

- TODO 2026-11-20 PM 분석에 적힌 서비스의 항목별 `os.scandir` 순회, `is_excluded_path`, 파일 stat, 문서 썸네일 캐시 조회 및 사진 EXIF/임시 DB upsert를 확인했다.
- 비교 도구는 실제 `DocGridService.list_documents(recursive=True)`와 `PhotoAlbumService.scan_folder(recursive=True)`를 호출한다. 썸네일·SQLite는 `E:\docfit-tmp`의 매 실행 임시 폴더로 격리한다. 앱 설정·캐시·사용자 파일에는 쓰지 않았다.
- Python 기준 행은 실제 서비스 결과이며 mtime 나노초는 결과 경로의 `st_mtime_ns`에서 읽었다. Rust는 `klib scan --roots ... --ext ... --exclude ... --json-lines` 출력이다. 경로 집합, size, mtime_ns, cloud를 전수 비교했다. mtime 초 단위 float는 양쪽 공통 나노초로부터 산출하되 이진 부동소수점 표현 차이 때문에 대조 키로 쓰지 않는다.
- Python 시간은 서비스 호출 벽시계 시간(cProfile 계측 오버헤드 포함), Rust 시간은 프로세스 시작부터 JSON 출력 완료까지다. 각 1회만 측정했다.
- 대표 트리로 저장소 `E:\docfit-tmp\SCANRUST`를 읽기 전용 사용했다. 확장자는 사용자가 지정한 문서 7종과 사진 8종, 제외 집합은 Python `_EXCLUDED_NAMES`와 타임머신 사진 제외 목록의 기본 개발 폴더를 합쳐 양쪽에 전달했다. 실제 스캔된 항목은 문서 29개, 사진 262개로 “수만 이상” 조건에 미달한다. 대규모 성능 결과로 일반화하지 않는다.

## Python 기준 프로파일 상위 항목

누적 비율은 cProfile 총 서비스 벽시계 시간 대비이며, 호출 계층 때문에 함수 행의 비율은 서로 겹칠 수 있다.

| 뷰 | 항목 | 전체 시간 | 초당 항목 | 상위 함수 (누적 비율) |
|---|---:|---:|---:|---|
| 문서 | 29 | 2.372 s | 12.2 | `is_excluded_path` 85.44%, `_canonical` 53.40%, `normcase` 23.99%; `scan_dir` 전체 99.98% |
| 사진 | 262 | 11.278 s | 23.2 | DB `upsert_photo` 57.67%, SQLite 종료/commit 33.66%, EXIF 읽기 17.22%, `is_excluded_path` 16.86%; `scan_dir` 전체 99.65% |

walk/cached-stat 요약: 두 서비스 모두 Python `scan_dir` 177회, 파일 stat/캐시 처리 후보는 문서 29회, 사진 262회다. 문서 캐시 `ThumbnailCache.get`은 29회 호출됐다(문서 cProfile 누적시간 0.36ms, 0.02% 미만). 사진은 스캔 시 기존 SQLite 행 비교, EXIF 추출 및 DB upsert를 실행하며 썸네일 캐시는 이 단계에 없다. 사진 스캔 시간은 순수 걷기 비교가 아니라 서비스의 메타데이터 추출·DB 쓰기까지 포함한다.

요청된 기능별 독립 비율(calls 및 누적 포함):

| 기준 함수 | 문서 | 사진 |
|---|---:|---:|
| 걷기 `scan_dir` | 99.98% (177회) | 99.65% (177회) |
| 제외 `is_excluded_path` | 85.44% (2,406회) | 16.86% (2,406회) |
| stat | 서비스 내부 호출, 하위 C 함수로 별도 집계되지 않음 | 서비스 내부 호출, EXIF 추가 파일 open 포함 |
| 썸네일 캐시 조회 | `get` 29회, 0.02% 미만 | 스캔 함수에서는 조회하지 않음 |
| 정렬 | `list_documents`에서 수행, 개별 누적 항목 0.01% 미만 | 사진 인덱스 DB upsert가 주요 비용이며 이 단계 정렬 없음 |

## Python 대 Rust 1회 비교

| 뷰/확장자 | Python 개수·시간·항목/초 | Rust 개수·시간·항목/초 | 전수 차이 |
|---|---:|---:|---:|
| 문서 (`hwp,hwpx,xls,xlsx,ppt,pptx,pdf`) | 29 · 2.372 s · 12.2/s | 29 · 0.132 s · 219.2/s | 0 |
| 사진 (`jpg,jpeg,png,webp,bmp,tif,tiff,heic`) | 262 · 11.278 s · 23.2/s | 262 · 0.153 s · 1714.4/s | 0 |

같은 문서의 JSON 원본: `E:\docfit-tmp\scan-docs-profile.json`, `E:\docfit-tmp\scan-photos-profile.json` (임시 측정 산출물, 저장소에는 추가하지 않음).

## Rust CLI 선택 근거와 구현

`klib`에는 이미 Windows `windows-sys` 의존성, 독립 하위 명령 분기, `KLIB_EXE` 탐색/실행 선례가 있다. `kfind`는 서비스 설치와 사용자 모드 이름 색인 전용 CLI이므로 독립 스캔 명령은 기존 `klib`의 구조를 확장하는 편이 변경 범위와 신규 의존성이 작다. `klib scan`은 `FindFirstFileExW(FindExInfoBasic, FIND_FIRST_EX_LARGE_FETCH)`를 사용하며 코어 수 기준 표준 스레드 워커, 채널 결과 전달, 파이프 stdout 단절 시 원자 취소 플래그를 사용한다. 출력은 UTF-8 JSON Lines로 path/name/ext/size/mtime/mtime_ns/cloud를 제공한다.

초기 Rust 규칙은 Python `library_scope`와 다르게 개발 폴더 일부를 추가 제외했다. 2단계에서 Rust 기본 예약 폴더를 `library_scope._EXCLUDED_NAMES`와 맞췄고, 비교 시 추가 사용자 제외 경로는 양쪽에 동일하게 전달한다. 사진 서비스는 경로 제외와 Python hidden 폴더/이름 필터를 함께 적용한다.

비교 결과 수정 이력: 최초 파이프 실행에서 stdin EOF를 취소로 오해해 0건을 반환했고, stdin 감지를 제거해 stdout 단절 감지로 바꿨다. 이후 파일 3개 경로/size/cloud는 일치했지만 float mtime 표현에서 최대 sub-microsecond 차이가 나타났다. 비교는 정밀한 정수 `mtime_ns`를 사용하고 `mtime`은 표시 값으로 유지해 최종 차이 0을 얻었다.

## 2단계 연결 지점 제안

- 문서: `DocGridService.list_documents`에서 재귀 순회 블록을 `klib scan --roots <folder> --ext ... --exclude ... --json-lines` 자식 프로세스 호출로 대체한다. JSON 결과를 기존 `stem`, 썸네일 키, `has_thumb` 보강 단계에 전달하고 기존 이름 casefold 정렬을 보존한다.
- 사진: `PhotoAlbumService.scan_folder`에서 순회 결과를 받아 기존 DB 비교/EXIF 추출/upsert 흐름에 넣는다. 증분 DB 상태와 cloud 안전 처리는 Python에서 유지한다.
- 이 카드는 서비스 연결, 색인 서비스 사용, 증분 갱신/UI 변경을 하지 않았다.

## 시험·명령 결과

실행한 명령(환경 변수는 카드 지정 경로):

```powershell
$env:CARGO_TARGET_DIR='E:/cargo-target/SCANRUST'
$env:PATH="$env:LOCALAPPDATA/DocFitTools/mingw64/bin;$env:PATH"
cargo test --manifest-path klib/Cargo.toml --lib scan_cli:: -- --nocapture
# test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 15 filtered out

$env:PYTHONIOENCODING='utf-8'
$env:SCANRUST_KLIB='E:/cargo-target/SCANRUST/release/klib.exe'
python -m unittest tests.test_scan_compare -v
# Ran 1 test in 1.863s
# OK

python scripts/scan_compare.py --root . --binary E:/cargo-target/SCANRUST/release/klib.exe --mode docs --profile-out E:/docfit-tmp/scan-docs-profile.json
# docs: python=29 2.372s (12.2/s), rust=29 0.132s (219.2/s), differences=0

python scripts/scan_compare.py --root . --binary E:/cargo-target/SCANRUST/release/klib.exe --mode photos --profile-out E:/docfit-tmp/scan-photos-profile.json
# photos: python=262 11.278s (23.2/s), rust=262 0.153s (1714.4/s), differences=0
```

## 미확인

- 항목 수 만 단위 이상의 독립 대표 트리에서 같은 측정은 수행하지 못했다. 현재 저장소에서 선택한 291개 결과 항목은 기준 규모에 미달한다.
- cloud-only 속성은 임시 트리/저장소에 실제 placeholder 파일이 없어 실물 파일 대조를 하지 못했다. 구현은 `OFFLINE`, `RECALL_ON_OPEN`, `RECALL_ON_DATA_ACCESS` 속성 비트를 검사한다.
- 심볼릭 링크/정션 및 stdout 파이프 단절 취소는 자동 실행 시험을 아직 두지 않았다. 표준입력 EOF는 취소 신호로 사용하지 않는다. 디렉터리 reparse point는 추적하지 않는다.
- CLI 원본 출력은 순회 순서지만 두 서비스 연결 시 Python과 같은 name casefold 정렬을 적용한다.

## 2단계 결과와 대규모 실측 (2026-11-21)

- `DocGridService.list_documents(recursive=True)`와 `PhotoAlbumService.scan_folder(recursive=True)`의 순회 입력을 `klib scan --json-lines`로 연결했다. `KLIB_EXE` 환경변수와 기존 리소스/저장소 실행 파일 탐색을 공용으로 사용한다. 바이너리 부재, 시작/종료 오류, 잘못된 JSON은 Python 스캔으로 자동 폴백하며 `DOCFIT_SCAN_ENGINE=python`은 Python을 강제한다. 취소 시 자식 프로세스를 종료한다. Rust 출력 항목 수와 파일 상위 폴더를 `on_progress`에 전달한다.
- 썸네일 캐시 조회는 Rust 경로의 전체 목록 스캔에서 하지 않는다. `flutter_ui/lib` Dart 파일에서 `has_thumb` 참조는 발견되지 않았고 문서뷰는 `thumbnail(path)` 응답을 요청한다. 서비스 API 호환과 기존 시험을 위해 키는 남겨 Rust 재귀 스캔 결과에서 `false`를 준다. 비재귀·Python 강제/폴백 경로는 기존 `has_thumb` 동작을 유지한다.
- Rust 기본 예약 폴더를 `library_scope._EXCLUDED_NAMES`와 일치시켰다(Windows, Program Files 계열, ProgramData, Recycle/System Volume Information, AppData, node_modules, VCS, venv). 이전 Rust 전용 `Temp`, `Packages`, `target`, `build` 등의 추가 제외는 제거했다. 숨김 이름과 `#ieeee` 규칙은 양쪽에서 동일하게 적용된다. 사용자 제외 경로는 CLI 인자로 전달된다.
- 큰 트리: `C:\Users\haiju\OneDrive\바탕 화면` (사전 파일 열거 73,978개, 읽기 전용). `scripts/scan_compare.py --mode both` 한 번으로 문서·사진 확장자 합집합을 비교했다. 결과 행 1,485개, 지원하지 않는 확장자/숨김·예약·사용자 제외 항목은 비교 대상에서 빠진다. 전수 경로·크기·mtime_ns·cloud 차이 0.

| 측정 범위 | Python 총 시간 / 항목/초 | Rust 총 시간 / 항목/초 | 결과 행·차이 |
|---|---:|---:|---:|
| OneDrive 바탕 화면 (73,978 파일 전체; docs+photos) | 5.190 s / 286.1/s | 0.193 s / 7,703.1/s | 1,485 / 0 |

이 측정에서는 Rust가 약 26.9배 빠르다. Python 프로파일은 경로 제외 판정 82.39%(4.276초), `_canonical` 51.49%(2.672초, 중첩 누적)였다. Rust의 워커 수는 `available_parallelism()`에 따라 CPU 코어 수를 사용한다. 이번 결과로 스레드 부족이나 디스크 병목 징후는 보이지 않는다. 결과 JSON은 `E:\docfit-tmp\scan-scanrust2-large.json`에 저장했다.

### 2단계 확인 명령

```powershell
cargo test --manifest-path klib/Cargo.toml --lib
# test result: ok. 19 passed; 0 failed; 0 ignored

$env:KLIB_EXE='E:/cargo-target/SCANRUST2/release/klib.exe'
python -m unittest tests.test_doc_grid_service tests.test_photo_album_service tests.test_default_library_scan tests.test_scan_compare -v
# Ran 36 tests in 24.368s
# OK

python scripts/scan_compare.py --root 'C:/Users/haiju/OneDrive/바탕 화면' --binary E:/cargo-target/SCANRUST2/release/klib.exe --mode both --profile-out E:/docfit-tmp/scan-scanrust2-large.json
# both: python=1485 5.190s (286.1/s), rust=1485 0.193s (7703.1/s), differences=0
```

HWP_AutoDocFit.spec는 `resources/klib.exe`가 존재할 때 `binaries`에 선택적으로 추가한다. 현재 설치 폴더의 `installer/setup.iss`에는 klib 포함 항목이 없고, spec의 `klib.exe`도 필수 검사하지 않는다. 따라서 빌드 환경에 `resources/klib.exe`가 없으면 설치본에 포함되지 않을 수 있다. 사용자 지시대로 spec/installer는 변경하지 않았다.

시험은 `tests/fixtures`를 건드리지 않았다. 확인하지 않은 항목: 기본 제외 경로 밖의 다른 드라이브 전수 대조, 실제 OneDrive cloud placeholder 속성별 경로, 설치된 EXE 내부 `resources/klib.exe` 실물 포함 여부. spec 조건부 번들 규칙은 확인했으며 새 설치본은 만들지 않았다.
