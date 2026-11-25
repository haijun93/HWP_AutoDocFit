# DBLOPEN1 — 사진뷰·문서뷰 더블클릭 → 연결 앱으로 열기 (2026-11-25)

사용자 지시: "사진뷰, 문서뷰에서 파일을 선택하여 더블클릭하면 해당 파일을 연결앱으로 여는 기능 추가"

## 동작

| 화면 | 한 번 클릭 | 더블클릭 | Enter |
|---|---|---|---|
| 사진뷰(Flutter) 묶음 격자·검색 결과·중복 보기 | 크게 보기(약 0.3초 뒤) | 연결 앱으로 열기 | 추가하지 않음 |
| 사진뷰 크게 보기 창의 사진 영역 | — | 지금 보이는 사진 열기 | 추가하지 않음 |
| 문서뷰(Flutter) | 선택 테두리(신규) | 열기(기존) | 추가하지 않음 |
| 네이티브 `docview_win`·`photoview_win` | 선택(기존) | 항목 위에서 열기 | 선택 항목 열기(검색칸 입력 중·제외 폴더 패널 제외) |

- 사진뷰의 한 번 클릭은 두 번째 클릭을 기다리므로 크게 보기가 약 0.3초(Flutter `kDoubleTapTimeout`) 늦게 뜬다. 기다리는 동안 카드 테두리를 강조한다.
- 사진뷰 선택 모드에서는 이전처럼 연속 클릭이 선택 전환이며 파일을 열지 않는다.
- 별표 단추는 판정을 기다리지 않는다.

## 구현

- 백엔드: `docfit_app/open_listed_file.py`(공용). `photo_album` `open` 명령 추가, 문서뷰 `open_file`도 같은 함수를 쓴다. 응답은 `{"ok", "path"}` 또는 `{"ok": false, "code", "message"}`.
  - 코드: `INVALID_PATH`, `FILE_NOT_FOUND`, `UNSUPPORTED_TYPE`(신규), `OPEN_FAILED`.
  - **동작 변경**: 문서뷰 `open`은 전에는 아무 파일이나 열었다. 이제 목록 대상 확장자(HWP·HWPX·XLS·XLSX·PPT·PPTX·PDF)만 연다. 사진뷰는 `PHOTO_EXTENSIONS`만 연다.
- 네이티브: `grid_core/src/open.rs`(검사 + `ShellExecuteW`), `App::click`·`App::enter_target`·`open_error`. 더블클릭은 좌표를 다시 계산하지 않고 첫 클릭이 누른 항목을 연다(첫 클릭이 항목을 화면 안으로 스크롤해도 다른 파일이 열리지 않게). 실패 안내는 상태줄 오른쪽.
- Flutter 사진 카드: `InkWell.onDoubleTap`을 쓰면 한 번 클릭이 타이머로만 지연되어 기존 사진뷰 위젯 시험 6개가 실패했다. 기존 시험은 고치지 않고, 카드에 대기 상태(`AnimationController _clickWait`)를 직접 두었다.

## 시험

| 검사 | 결과 |
|---|---|
| 전체 `python -m unittest discover -s tests`(KLIB_EXE 지정) | `Ran 3706 tests in 995.969s` / `OK (skipped=23)`. 신규 `tests/test_dblopen1.py` 11개 포함 |
| `flutter analyze` | No issues found |
| `flutter test` 전체 | 832 통과, 3 건너뜀. 신규 `test/dblclick_open_test.dart` 8개 포함 |
| `flutter build web --release` | Built build/web |
| cargo test `grid_core` | 13 통과(신규 `open_tests` 6 포함), 1 ignored |
| cargo test `docview_win` | 52 통과(5+11+31+5) |
| cargo test `photoview_win` | 44 통과(5+11+23+5) |
| cargo test `convert_win`(grid_core 의존) | 15 통과 |
| cargo test `rename_win`(grid_core 의존) | 39 통과 |

`real_klib_tests`(각 5개)는 처음에 klib.exe 가 없어 실패했고, 이 작업 폴더의 `klib`를 빌드해 `KLIB_EXE`로 지정한 뒤 통과했다. Python 전체 시험은 `tests/fixtures/timemachine_extract/`의 파일 4개를 다시 써 놓아 실행 뒤 되돌렸다.

## 확인하지 못한 것

- 실제 연결 앱이 열리는지(`os.startfile`·`ShellExecuteW`): 시험은 가짜 opener만 썼다. 실기 확인 필요.
- 네이티브 창의 실제 마우스 더블클릭·Enter: 목록·대상 선택 로직과 열기 검사는 단위 시험했지만, 창 메시지 처리(`window.rs`)를 띄워 확인하지는 않았다(창 조작 스크립트 실행이 승인되지 않음).
- 설치본·exe 빌드에서의 동작, 클라우드 전용 파일 열기(열면 내려받기가 일어날 수 있다).

## 작업 중 본 기존 문제(고치지 않음)

- 사진뷰 묶음 격자의 크게 보기 순번(`globalIndexCounter`)은 항목을 그리는 순서로 센다. 스크롤·다시 그리기 뒤에는 누른 사진과 다른 사진이 크게 보일 수 있다고 코드상 판단한다(재현 시험은 하지 않음). 더블클릭 열기는 순번이 아니라 항목 경로를 쓰므로 영향이 없다.
