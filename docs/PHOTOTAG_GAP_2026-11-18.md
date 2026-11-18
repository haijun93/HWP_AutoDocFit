# 사진뷰 태그·검색 엔진 GAP (2026-11-18)

## 범위

`TODO.md`의 사진뷰 1단계 중 즐겨찾기·태그와 파일명·날짜·태그 검색을 확인했다. 사진뷰 명세는 Rust `klib` 엔진에 `photo_*` 명령과 태그 DB를 두도록 하지만, 현재 사진 라이브러리 서비스는 Python `docfit_app/photo_album_service.py`에 있다. UI는 이 작업에서 변경하지 않는다.

| 항목 | 구현 상태 | 근거 |
|---|---|---|
| 사진 태그 저장·조회·삭제 | 구현됨 (Python) | `PhotoAlbumDb.add_tag/remove_tag/get_tags/set_tags`, SQLite `tags` 테이블 |
| 즐겨찾기 | 구현됨 (Python) | `PhotoAlbumDb.toggle_favorite` |
| 파일명·경로 검색 | 구현됨 (Python) | `PhotoAlbumDb.search`의 `query` 조건 |
| 날짜 범위 검색 | 구현됨 (Python) | `PhotoAlbumDb.search`의 `start_date/end_date` 조건 |
| 태그 검색 | 구현됨 (Python) | `PhotoAlbumDb.search`의 정확 태그 조건 |
| 사진뷰 Rust 엔진 API 및 `photo_*` 명령 | 미구현 | `klib/src`에 사진 엔진 모듈·명령 없음 |
| Rust 태그 저장소·검색 API | 미구현 | `klib/Cargo.toml`은 `rusqlite`를 사용하지만 사진 스키마/API 없음 |

## 이번 구현

격차 중 작고 독립적으로 검증할 수 있는 Rust 태그 정규화·검색 필터 API를 `klib`에 추가한다. 이는 사진 DB/IPC 명령 구현의 일부 기반이며, DB 영속화와 명령 연결은 남겨 둔다.

## 변경 파일 및 시험

- `klib/src/photo.rs`: 태그 정규화 및 파일명·촬영일·태그 메타데이터 검색 필터.
- `klib/src/lib.rs`: 사진 모듈 공개.
- `klib/tests/photo.rs`: 태그 정규화와 검색 조건 시험.
- Rust 시험 결과: 실행 후 모든 `test result` 줄을 여기에 기록.

## 남은 격차

- Rust 사진 DB 스키마 및 태그 영속화 CRUD.
- Rust `photo_*` IPC 명령 및 Python 브리지 연동.
- 폴더 검색 범위·즐겨찾기 조건 등을 포함한 완전한 사진 목록 API.
