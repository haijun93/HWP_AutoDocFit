# PHOTOFIX1 — 사진뷰 설정 저장소 연결과 제외 폴더 반영 (2026-11-26)

FILEOPS1 에서 코드로만 확인했던 문제를 재현 시험으로 확인하고 고쳤다. `feature/CTXMENU1`(f385ce29)을 병합한 뒤 작업했다.

## 재현(수정 전, 임시 설정·합성 파일)

| 경로 | 결과 |
|---|---|
| 데스크톱 브리지 `photo_album` | `thumb_size_set`·`excluded_folder_add` 실패(`settings 저장소가 연결되지 않았습니다`) |
| `application_service` | 저장은 되지만, 제외 폴더로 옮긴 사진이 `list_by_date` 에 남음 |
| 데스크톱 | 제외하기 전에 DB 에 들어간 사진이 제외 뒤에도 목록·저장된 스캔 목록에 남음 |

## 원인

1. `desktop_web_ui.py` `photo_album` 이 `PhotoAlbumService()` 를 설정 저장소 없이 만들었다.
2. 사진뷰 목록은 DB 를 읽는데 읽을 때 제외 폴더를 거르지 않았다. 설정 저장소를 연결한 경로에서도 같았다.

## 수정

- 데스크톱 브리지: `doc_grid` 와 같이 `settings_store=self._settings_store()` 를 넘긴다.
- `photo_album_service.py`: `_without_excluded` 로 날짜 목록·검색·태그 수·중복 찾기·묶음 보기(폴더·카메라·위치)·저장하는 스캔 목록에서 제외 폴더 사진을 감춘다.
- DB 의 태그·즐겨찾기는 지우지 않는다. 제외를 풀면 다시 보인다.
- `application_service` 의 생성 코드는 이미 `settings_store=self.store` 라 바꾸지 않았다.

## 동작에서 알아 둘 점

- 제외 폴더의 사진은 그 폴더를 지정해 조회해도 보이지 않는다.
- 제외 폴더만 있는 태그는 태그 목록에 나오지 않는다.

## 시험

| 검사 | 결과 |
|---|---|
| 신규 `tests/test_photofix1.py` | 4개. 처음 3개는 수정 전 실패 확인. 저장 목록 시험은 해당 수정 한 줄을 뺀 상태에서 실패 확인 |
| 관련 시험(사진뷰·파일 작업·제외 설정·썸네일 크기·브리지 등) | `Ran 81 tests in 16.297s` / `OK (skipped=2)` |
| 전체 `python -m unittest discover -s tests`(venv315, KLIB_EXE 지정) | `Ran 3740 tests in 1040.568s` / `OK (skipped=23)` |
| Flutter·`cargo test` | 실행하지 않음 — 이 카드에서 Flutter·Rust 소스를 바꾸지 않음(`klib` 는 Python 시험용으로 빌드만) |

저장→재생성 시험은 데스크톱 브리지와 `application_service` 모두에서 새 객체를 만들어 썸네일 크기·제외 폴더 추가·삭제가 유지되는지 본다. 설정·DB·썸네일은 임시 폴더만 썼다.

전체 시험을 돌리면 `tests/fixtures/timemachine_extract/sample.{docx,hwpx,pptx,xlsx}` 가 수정 상태가 되어, 커밋 전에 되돌렸다.

## 확인하지 못한 것

- 실제 앱(설치본·exe, WebView)에서의 제외 폴더 창·썸네일 크기 슬라이더.
- 실제 Windows 검색 서비스(`engine=service`)·`klib` 스캔 결과에서의 제외 적용. 신규 시험은 Python 스캔 엔진을 썼다.
- 사진이 아주 많고 제외 폴더가 여러 개일 때 목록 거르기에 더 걸리는 시간(재지 않음).
