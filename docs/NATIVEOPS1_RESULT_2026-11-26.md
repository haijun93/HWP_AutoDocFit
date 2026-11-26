# NATIVEOPS1 — 네이티브 문서뷰·사진뷰 파일 작업 (2026-11-26)

## 조사·구현 계획

- 기준: DBLOPEN1·FILEOPS1·CTXMENU1 결과 문서. 네이티브 창의 공용 모델은 `grid_core::app::App`, 창 메시지와 그리기는 두 크레이트에 각각 있다.
- 공용 선택 모델은 경로 집합·Shift 기준 경로·현재 초점으로 표현한다. 화면 순서로 범위를 계산하며 검색·정렬·재검색 시 선택을 푼다. 스캔 항목 추가는 경로로 선택을 보존한다.
- 파일 작업 상태: `Idle → Preparing → Running → Complete`. 준비 중에는 확인·입력 대화상자를 띄우고, 실행 중에는 배경 스레드에서 파일별 결과를 보낸다. 준비·실행 중 중복 실행과 재검색을 막는다. 실행 중 종료는 작업 완료까지 거절한다.
- 휴지통 COM 진행 콜백에서 재활용 표시가 없으면 중단한다. 영구 삭제 대체 경로는 없다. 복사·이동은 배타 생성/이동으로 충돌 이름을 붙이고 이름 변경은 확장자를 고정한다.
- 공용 메뉴·대화상자·단축키 판정을 `grid_core`에 두고 두 창에서 연결한다. 앱으로 파일을 넘기는 통로는 없으므로 앱 기능 연결 메뉴는 만들지 않는다.
- 결과물: 이 문서와 `TODO.md`. 선택·합성 파일 작업 단위 시험, 공용 및 의존 크레이트 5개 시험, Python 전체 시험을 기록한다. Flutter 변경 계획 없음.

## 검증

다른 워커의 `bench_library_scan.py` 종료까지 구현·조사만 진행했다. 종료 후 공유 전체 시험 잠금을 잡고 빌드·시험을 순차 실행했으며, Python 전체 시험에는 공용 COM 잠금도 잡았다. 두 잠금 모두 실행 후 해제했다.

요청한 Python 3.15 실행기·로컬 MinGW와 별도 `CARGO_TARGET_DIR`를 사용했다. 이 작업 폴더의 `klib`를 release로 빌드하여 `KLIB_EXE`·`DOCFIT_KLIB`로 지정했다. 빌드 경로는 환경변수에만 설정했으며 저장소 설정에는 추가하지 않았다.

| 시험 | 통과 | 실패 | 제외 |
|---|---:|---:|---:|
| `cargo test --manifest-path grid_core/Cargo.toml` | 36 | 0 | 1(기존 썸네일 측정) |
| `cargo test --manifest-path docview_win/Cargo.toml` | 52 | 0 | 0 |
| `cargo test --manifest-path photoview_win/Cargo.toml` | 44 | 0 | 0 |
| `cargo test --manifest-path convert_win/Cargo.toml` | 15 | 0 | 0 |
| `cargo test --manifest-path rename_win/Cargo.toml` | 39 | 0 | 0 |
| Rust 합계 | **186** | **0** | **1** |

모든 Cargo 매니페스트를 조사하여 `grid_core`의 의존 크레이트 4개와 추가 간접 의존 크레이트가 없음을 확인했다. 위 5개 크레이트는 필터 없이 전체 시험을 실행했다. 새 시험 23개(선택·목록 반영·메뉴/클립보드 자료 10개, 합성 파일 작업 12개, 휴지통 진행 콜백 1개)는 모두 통과했다.

신규 파일 작업 시험은 임시 폴더의 합성 파일만 사용했다. 휴지통은 같은 임시 폴더 안의 가짜 휴지통으로 이동하거나 거절하는 구현이며, 실제 휴지통·사용자 클립보드는 호출하지 않았다. 충돌 파일·폴더 보존, 확장자·예약 이름 검사, 클라우드 복사 거절, 파일별 부분 실패, 다른 드라이브 이동의 복사/원본 제거 실패와 사본 되돌리기, 배경 작업의 비동기 반환을 검증했다. 기존 두 창의 `real_klib_tests`에는 합성 트리 스캔 및 고정 드라이브의 파일 메타데이터를 300ms 읽고 취소하는 시험이 포함되어 있다. 문서 본문을 열거나 원본을 수정하는 시험은 추가하지 않았다.

전체 `python -m unittest discover -s tests` 종료 코드 0:

```text
Ran 3774 tests in 982.822s
OK (skipped=29)
```

Python 시험이 재생성한 `tests/fixtures/timemachine_extract/`의 합성 fixture 4개는 시험 전 상태로 되돌렸다. Python·Flutter 코드는 변경하지 않았으며 Flutter 검사 대상 변경은 없다.

아래는 **모든 `test result:` 줄 25개**의 원문이다.

### `grid_core`

```text
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### `docview_win`

```text
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.43s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.58s
test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.23s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.92s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### `photoview_win`

```text
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.58s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.49s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.82s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.42s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### `convert_win`

```text
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### `rename_win`

```text
test result: ok. 39 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.46s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 구현 동작

- 클릭은 하나 선택, Ctrl은 추가·해제, Shift는 기준부터 화면 순서 범위, Ctrl+Shift는 범위 추가다. Ctrl+A는 현재 검색 결과 전체, Esc는 선택을 푼다. 선택이 없을 때 Esc의 기존 스캔 취소도 유지한다.
- 오른쪽 클릭은 선택 안이면 전체 선택을 유지하고 밖이면 그 파일만 선택한다. 메뉴 키·Shift+F10도 같은 메뉴를 연다. 검색 입력·제외 폴더 패널에서는 파일 작업 단축키를 처리하지 않는다.
- 열기·이름 변경은 하나 선택, 다른 파일 작업은 1~5000개 선택에서 활성화한다(FILEOPS1 작업 상한과 동일). Shift+Delete도 휴지통 확인 경로다. 확인창 첫 초점은 취소다.
- 복사·이동은 `이름 (2).ext`부터 충돌을 피하고 배타 생성/이동한다. Rust 기본 `rename`은 대체를 허용하므로 Windows에서는 대체 플래그 없는 `MoveFileExW`를 쓴다. 이름 변경은 원래 확장자를 유지한다.
- 클라우드 표시와 실제 파일 속성의 OFFLINE·RECALL 표시를 다시 검사한다. 클립보드·폴더 복사와 다른 드라이브 이동은 클라우드 전용 파일을 거절한다. 클립보드는 CF_HDROP와 Preferred DropEffect=복사 형식이다.
- 파일 작업은 별도 스레드, 상태 갱신은 타이머다. 완료 후 실패 전체를 스크롤 가능한 파일별 결과창으로 표시하며 성공한 파일만 목록과 썸네일 캐시에 반영한다. 제외 폴더로 이동한 파일은 목록에서 뺀다. 늦은 스캔 결과로 옛 경로가 되살아나지 않게 기록한다.
- 작업 중 새 파일 작업·재검색·창 종료를 막고 완료 뒤 다시 허용한다. 파일을 옮기다 프로세스가 종료되는 것을 방지한다. 강제 프로세스 종료까지 막지는 않는다.
- 휴지통은 고정 드라이브만 시도하며 `PreDeleteItem`에서 재활용 표시가 없으면 E_ABORT로 중단한다. 근거: [Microsoft PreDeleteItem 문서](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperationprogresssink-predeleteitem).

## 앱 기능 연결 설계(미구현)

네이티브 두 실행기는 `klib scan` 자식 프로세스로 목록을 받으며 Flutter의 `FileInbox`·탭 이동이나 Python `add_paths`로 보내는 통로가 없다. 문서편집·파일 변환·사진 편집 메뉴는 만들지 않았다.

후속 구현에는 앱이 만든 인증된 로컬 IPC 연결이 필요하다. 네이티브 창은 기능 이름과 선택 경로 목록을 보내고, 앱은 CTXMENU1과 같은 형식·개수·권한 검사 후 기존 추가 명령에 전달한다. 앱 수신 확인 전에는 성공 표시나 화면 이동을 하지 않으며, 파일을 자동 편집하지 않는다. 두 창이 앱 없이 실행되거나 연결이 끊기면 해당 연결 메뉴를 숨겨야 한다.

네이티브 사진뷰는 사진 DB를 사용하지 않고 스캔 목록만 표시한다. 이번 작업도 Flutter 사진 DB의 태그·즐겨찾기 경로 이관 및 앱의 저장 목록 갱신은 수행하지 않는다. 이 정보까지 함께 옮기려면 위 IPC를 통해 기존 FILEOPS1 서비스에서 작업하도록 통합해야 한다.

## 확인하지 않은 범위

- 실제 창을 띄워 마우스·단축키·메뉴·확인/이름 입력/결과창을 조작하는 실기 시험. 선택·메뉴 활성 조건·단축키 판정과 GDI 그리기는 자동 시험했지만 창 메시지 전체 흐름을 직접 조작하지 않았다.
- 실제 휴지통 삭제, 휴지통 바로 삭제 설정·용량 초과 상태, 실제 클립보드와 탐색기 붙여넣기, 실제 폴더 선택창. 휴지통 거절과 클립보드 자료 구조는 가짜/단위 시험으로 확인했다.
- 실제 다른 드라이브 이동·클라우드 전용 파일·긴 경로 및 설치본에서의 동작. 다른 드라이브 오류 분기는 가짜 전송 경계와 합성 파일로 시험했다.
- 앱 기능 연결과 사진 DB·저장된 목록 이관은 위 설계만 기록했다.

## 변경·보호 확인

- 공용 구현: `grid_core/src/selection.rs`, `file_ops.rs`, `file_ops_windows.rs`, `native_ops.rs` 및 `app.rs`.
- 창 연결·선택 강조·개수/진행 표시: 두 창의 `window.rs`·`paint.rs`. 새 직접 의존성 `windows-core`는 기존 간접 의존성과 같은 버전이며 COM 진행 콜백 구현에 필요하다. 5개 잠금 파일의 변경도 이 의존성 항목 1줄씩뿐이다.
- 사용자 문서·사진에 새 파일 작업 시험을 하지 않았다. XML 파싱 코드를 추가하거나 변경하지 않았다. 본체 `hwp-auto-docfit.py`와 CRLF는 그대로 보존했다. 기존 파일의 UTF-8·줄바꿈 형식을 유지했다.
- `feature/NATIVEOPS1` 로컬 커밋만 수행한다. 푸시·태그·릴리스·설치 및 GitLab 접속은 수행하지 않는다.
