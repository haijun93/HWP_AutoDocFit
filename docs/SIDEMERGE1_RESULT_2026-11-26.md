# SIDEMERGE1 — RUSTSIDE2(사이드바·별도 창)를 최신 main 에 병합 (2026-11-26)

브랜치 `feature/SIDEMERGE1`(`github/main` 기준)에 `git merge --no-ff feature/RUSTSIDE2`. 커밋만 했고 푸시하지 않았다.

## 충돌 해결

| 파일 | 해결 |
|---|---|
| `docview_win`·`photoview_win` 의 `src/window.rs` | NATIVEOPS1 의 `file_action`·`context_menu`·`poll_file_ops` 를 남기고, RUSTSIDE2 의 `send_menu` 와 두 번째 `WM_CONTEXTMENU` 분기(도달 불가)를 없앴다. 보내기는 아래의 합친 메뉴로 옮겼다 |
| `hwp-auto-docfit.py` | RUSTSIDE2 가 위로 옮긴 `APP_VERSION` 선언 하나만 두고 main 의 `2.2.106` 을 적었다. CRLF 보존 |

단일 인스턴스, 변환 창의 파일 받기, 파일 작업 중 `WM_CLOSE` 거절(NATIVEOPS1)은 충돌 없이 그대로다. 전체 종료·설치의 닫기 요청도 이 거절을 따른다.

## 병합하면서 바꾼 동작

1. **오른쪽 메뉴 하나**: 위에 파일 작업(열기·복사·폴더로 복사·이동·이름 변경·휴지통), 구분선 아래에 '다른 창으로 보내기'. 메뉴 키·Shift+F10 도 같다. 입력 중인 검색칸·제외 폴더 패널·파일 작업 중에는 NATIVEOPS1 규칙대로 열리지 않는다.
2. **선택한 파일 전부를 보낸다**(RUSTSIDE2 는 1개). 대상의 형식·개수 한도에 하나라도 걸리면 그 줄은 흐리게 두고 이유를 적는다. 경로가 명령줄에 안 들어갈 만큼 길면 임시 목록 파일(`--list`)로 넘기고, 보내기 프로세스가 끝나면 지운다.
3. **형식 표 맞춤**: main 의 CTXEXT1 이 Flutter 표에만 더한 `xls`·`pptx`·`hml`(문서편집·파일 변환)을 `grid_core/src/handoff.rs`·`docfit_app/handoff.py` 에도 넣었다. git 충돌로는 드러나지 않는 어긋남이다.

RUSTSIDE2 결과 문서의 "Rust 문서뷰·사진뷰는 한 번에 1개만 보낸다"와 NATIVEOPS1 결과 문서의 "앱 기능 연결 설계(미구현)" 중 파일 전달 부분은 이 병합으로 바뀌었다.

## 시험

로컬 MinGW, 별도 `CARGO_TARGET_DIR`, release `klib` 를 `DOCFIT_KLIB`·`KLIB_EXE` 로 지정했다.

| 대상 | 통과 | 실패 | 제외 |
|---|---:|---:|---:|
| `cargo test` grid_core | 42 | 0 | 1(기존 썸네일 측정) |
| `cargo test` docview_win | 52 | 0 | 0 |
| `cargo test` photoview_win | 44 | 0 | 0 |
| `cargo test` convert_win | 15 | 0 | 0 |
| `cargo test` rename_win | 39 | 0 | 0 |
| `cargo test` dock | 41 | 0 | 0 |
| Rust 합계 | **233** | **0** | **1** |
| 관련 python(32개 모듈, Python 3.15) | 355 | 0 | 6 |
| `flutter analyze --fatal-infos` | 문제 0 | | |
| `flutter test` 전체 | 881 | 0 | 3 |

- 새 Rust 시험 3개: 메뉴 명령 번호 구분(문서편집 번호 0 포함), 여러 파일의 형식·개수 검사, 긴 선택의 목록 파일 왕복.
- python 의 창끼리 넘기기 시험은 이번에 빌드한 `sidebar.exe` 를 `DOCFIT_SIDEBAR_TEST_EXE` 로 지정했다.
- 첫 Rust 실행은 도구의 시간 제한으로 `rename_win` 에서 중단돼, `rename_win`·`dock` 을 다시 돌렸다.

## 확인하지 않은 것

- 전체 python discover 는 돌리지 않았다(카드 범위는 관련 시험). RUSTSIDE2 에 기록된 기존 실패 4건(`test_default_library_scan` 등)도 다시 확인하지 않았다.
- 실제 창에서 합친 오른쪽 메뉴를 열어 고르는 조작, 여러 파일을 실제로 보내 받는 창에 올라오는 것. `sidebar --send` 로 가는 인자까지 단위 시험했다.
- 설치본·설치 프로그램·웹 빌드·한/글 COM 은 하지 않았다.
- 클라우드 전용 파일은 파일 복사와 달리 보내기에서 따로 막지 않는다(RUSTSIDE2 와 같음).
- 긴 목록을 보낸 직후 보낸 창을 닫으면 임시 폴더에 목록 파일(경로만)이 남을 수 있다.
- RUSTSIDE2 의 나머지 남은 문제는 `docs/RUSTSIDE2_RESULT_2026-11-26.md` 그대로다.
