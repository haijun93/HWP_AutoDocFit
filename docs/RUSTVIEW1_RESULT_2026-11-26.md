# RUSTVIEW1 — 문서뷰·사진뷰 Rust 창을 앱 화면 기능 수준으로 (2026-11-26)

브랜치 `feature/RUSTVIEW1`. 커밋만 했고 푸시하지 않았다. 앱의 기본 화면(`docview_ui`·`photoview_ui` = `flutter`)은 바꾸지 않았고 Flutter 화면도 그대로다. 설치본·서비스·자동 실행 등록·사용자 설정은 바꾸지 않았다.

## 시작 절차

처음에는 선행 시험 실패 기록 때문에 병합 전에 멈췄다(커밋 `62bd78d1`). PM 재개 지시에 따라 `git merge github/main`(이미 최신) → `git merge feature/SIDEMERGE1` 뒤 확인했다.

```text
python -m unittest tests.test_default_library_scan tests.test_library_exclusion_settings
Ran 5 tests in 8.279s
OK (skipped=1)
```

## 비교표 (앱 화면 ↔ Rust 창)

| 기능 | 앱 화면 | 작업 전 Rust 창 | 지금 |
|---|---|---|---|
| 격자·검색·정렬·스캔 취소·제외 폴더·다중 선택·파일 작업 | 있음 | 있음 | 그대로 |
| 다른 화면으로 파일 넘기기(문서뷰 5곳, 사진뷰 2곳) | 있음 | 있음(SIDEMERGE1) | 그대로 |
| 썸네일 크기 저장(두 화면) | 있음 | 읽기만 | **추가** |
| 사진 크게 보기(앞뒤·슬라이드쇼·닫기·연결 앱 열기) | 있음 | 없음 | **추가** |
| 즐겨찾기, 즐겨찾기만 보기 | 있음 | 없음 | **추가** |
| 태그 붙이기·빼기, 태그로 거르기 | 있음 | 없음 | **추가** |
| 중복 사진 찾기 | 있음 | 없음 | **추가** |
| 공문 첨부용 사본 저장 | 있음 | 없음 | **추가** |
| 사진 편집(사본) | 슬라이더·입력칸 | 없음 | **추가(글 입력 한 줄)** |
| 사진대지 HWPX 만들기 | 있음 | 없음 | 미구현 |
| 날짜·폴더·카메라·위치 묶음 보기 | 있음 | 없음 | 미구현 |

문서뷰는 대조 결과 썸네일 크기 저장만 빠져 있었다(화면 코드 대조이며 실제 두 화면을 나란히 조작해 비교하지는 않았다).

## 구현

- **둘째 도구줄**(사진뷰): 즐겨찾기만 · 태그 ▾ · 중복 찾기 / 크게 보기 · 즐겨찾기 · 태그 · 첨부용 사본 · 사본 편집. 오른쪽 메뉴에도 같은 선택 항목. 단축키 Space(크게 보기), Ctrl+D(즐겨찾기), Ctrl+T(태그), Esc(중복 보기 닫기).
- **크게 보기**: 창 전체. ← → ·휠로 이동, Space 슬라이드쇼(3초), Enter 연결 앱, Esc 닫기. 그림은 배경 스레드에서 WIC 로 읽고 EXIF 방향을 편다(긴 변 2560 이하).
- **즐겨찾기·태그**: 앱 화면과 **같은 파일**(`photo_album.db`, `KAGENT_DATA` 또는 앱 데이터 폴더)·같은 표를 쓴다. 표시를 바꾸기 전에는 파일을 만들지 않는다. 이 창에서 이동·이름 변경·복사·휴지통을 하면 앱과 같은 규칙으로 따라간다. `--photo-db` 로 바꿀 수 있고, 합성 목록에는 기본 DB 를 쓰지 않는다.
- **중복 찾기**: 앱과 같은 dHash·거리 6·묶음 순서. 취소 가능. 결과는 묶음 번호를 붙여 걸러 보여 주고 기존 휴지통 삭제를 쓴다.
- **첨부용 사본**: 긴 변 1280, JPEG 85, `<이름>_docfit.jpg`.
- **사본 편집**: `회전 밝기 대비 [왼 위 오른 아래]`. 원본 폴더에 `<이름>_edited.<확장자>`.
- 클라우드에만 있는 사진은 크게 보기·중복·사본에서 읽지 않는다.
- Python·한/글을 부르지 않는다(사진 기능은 모두 Rust).

## 앱 화면과 다른 점

| 항목 | 앱 화면 | Rust 창 |
|---|---|---|
| 첨부용 사본 이름이 이미 있을 때 | 덮어쓴다 | 번호를 붙인다(덮어쓰지 않음) |
| 사본 편집 회전 | 임의 각도 지정 가능한 API(화면은 90° 단위) | 90° 단위 |
| WebP·HEIC 편집 사본 | WebP 저장 | JPEG 저장(Windows 에 WebP 저장 기능이 없음). HEIC 읽기는 Windows 코덱이 있어야 함 |
| 즐겨찾기 | 한 장씩 | 선택 여러 장(하나라도 꺼져 있으면 모두 켬) |
| 크게 보기 열기 | 클릭 | Space·단추·메뉴(더블클릭은 기존대로 연결 앱) |
| 슬라이드쇼 중 태그·편집 | 일시 정지 | 일시 정지(같음) |

## 시험 (실제 출력)

로컬 MinGW, 별도 `CARGO_TARGET_DIR`, release `klib` 를 `KLIB_EXE`·`DOCFIT_KLIB` 로 지정.

| `cargo test` 대상 | 통과 | 실패 | 제외 |
|---|---:|---:|---:|
| grid_core | 42 | 0 | 1(기존 썸네일 측정) |
| docfit_config | 9 | 0 | 0 |
| docview_win | 52 | 0 | 0 |
| photoview_win | 68 | 0 | 1(이번 시간 측정) |
| convert_win | 15 | 0 | 0 |
| rename_win | 39 | 0 | 0 |
| settings_win | 32 | 0 | 0 |
| dock | 41 | 0 | 0 |
| 합계 | **298** | **0** | **2** |

photoview_win 마지막 실행 출력:

```text
test result: ok. 23 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.81s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.92s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.55s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.24s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.82s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.45s
```

관련 Python(두 번에 나눠 실행, Python 3.14):

```text
test_photoview_library_schema, test_photo_album_service, test_photofix1, test_photoview_window_service,
test_docview_window_service, test_thumb_size_setting, test_doc_grid_service, test_doc_grid_bridge,
test_default_library_scan, test_library_exclusion_settings
Ran 73 tests in 20.788s
OK (skipped=2)

test_window_handoff, test_sidebar_launch  (DOCFIT_SIDEBAR_TEST_EXE=이번에 빌드한 sidebar.exe)
Ran 43 tests in 13.124s
OK
```

실제 창(합성 목록·임시 설정·임시 DB, 창 메시지로 조작):

```text
즐겨찾기 저장 1건: DSC00029.jpg
크게 보기에서 넘긴 뒤 즐겨찾기: 2건
종료 코드: 0
설정 파일 변경 없음
photo_album 저장된 설정: {'custom': 1, 'ui_thumb_sizes': {'photo_album': 240.0}}
doc_grid 저장된 설정: {'custom': 1, 'ui_thumb_sizes': {'doc_grid': 240.0}}
Python 앱 서비스가 Rust 창 DB 에서 읽은 즐겨찾기: 2 ['DSC00002.tif', 'DSC00029.jpg']
```

화면 그림(합성)은 1100×720 과 최소 폭 820×480 에서 도구줄이 겹치지 않는 것을 눈으로 확인했다.

## 시간 (release, `.full-test.lock` 없을 때, 합성 4000×3000 JPEG, 각 5회)

```text
view decode 4000x3000 -> 2560: median 190 ms (min 173 / max 228)
export 1280 jpeg: median 208 ms (min 183 / max 216)
dhash: median 166 ms (min 163 / max 178)
```

앱 화면(Python)과의 비교 측정은 하지 않았다. 중복 찾기는 한 장씩 읽으므로 이 값이면 1만 장에 약 28분이다(실제 사진·디스크에서는 다를 수 있음). 병렬화는 하지 않았다.

## 확인하지 않은 것·남은 문제

1. **실제 창에서 고르는 조작**: 태그 메뉴·태그 입력창, 오른쪽 메뉴의 사진 줄, 폴더 고르기, 사본 편집 입력창, 창을 통한 첨부용 사본·편집·중복 결과, 파일 작업 뒤 DB 반영. 각 계산·저장 함수는 단위 시험했다.
2. **실제 사진**으로는 확인하지 않았다(합성만). 중복 묶음이 Python 결과와 같은지 같은 사진으로 대조하지 않았다(축소 방식이 완전히 같지 않아 경계값에서 다를 수 있음). HEIC·WebP 읽기 미확인.
3. **CI(MSVC) 빌드 미확인**: photoview_win 에 SQLite(C 소스)가 새로 들어간다. `klib` 와 같은 구성이라 `crt-static` 이 적용될 것으로 보지만 VC++ 런타임 비의존 검사는 CI 에서 확인해야 한다. 실행 파일 크기도 늘어난다(재지 않음).
4. 미구현: 사진대지 HWPX, 날짜·폴더·카메라·위치 묶음 보기, 편집 미리보기·슬라이더.
5. 사본 편집 원본 해상도 상한 1.2억 화소. 크게 보기는 확대·이동이 없다.
6. 다중 모니터·125%/150% 실제 창, 고대비, 키보드만으로 둘째 도구줄 이동(단축키는 있음)·화면 읽기는 확인하지 않았다.
7. 전체 Python·Flutter 시험은 PM 릴리스 범위라 돌리지 않았다. 설치본·한/글 COM 시험도 하지 않았다.
8. 다이어그램·마인드맵·순서도 WebView2 자식 창은 이 카드(문서뷰·사진뷰)에서 다루지 않았다.
9. 기본 화면 전환은 PM·사용자 확인 후.
