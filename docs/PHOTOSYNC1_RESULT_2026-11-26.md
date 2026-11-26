# PHOTOSYNC1 — 사진뷰: 기존 DB 즉시 표시 + 새·바뀐 사진만 촬영 정보 읽기 (2026-11-26)

기준: `github/main` + `feature/DOCDB1`(병합 `f7c16b3a`).

## 바꾼 것

- 새 DB 를 만들지 않고 기존 `photo_album.db` 를 그대로 쓴다(태그·즐겨찾기 보존). `photos` 에 `missing` 열 하나만 더한다.
- `docfit_app/photo_library.py`(새 파일)
  - `PhotoStore`·`PhotoViewSync`: DOCDB1 의 `library_db.ViewSync` 로 비교하되 저장 자리와 화면 행만 사진용.
  - `ExifOrganizer`: 촬영 정보를 비교와 **따로** 배경에서 읽는다. 200장 또는 2초마다 저장한다.
- `library_db.ViewSync`: 행 모양(`_row`)·감춤(`_hidden`) 확장점만 추가(문서뷰 동작 동일).
- `scan_default`: 곧바로 돌아오고, `scan_progress` 의 `since` 로 저장 목록(최신 먼저, 묶음 최대 4,000) → 추가·변경·삭제를 보낸다.
- 새 명령 `exif_status`(진행 n/m + 채워진 행)·`exif_cancel`. 화면에 "촬영 정보 정리 중 · n/m장" 띠와 "정리 중지".
- 화면은 받은 목록을 그대로 쓴다(끝난 뒤 전체 목록을 다시 받지 않는다). 받은 것이 없거나 중간에 보기를 바꾼 때만 다시 부른다.
- 이름·태그 검색은 DB 만 조회한다(파일 stat·폴더 훑기 없음, 시험으로 고정). 태그 목록도 태그 달린 행만 읽는다.
- 폴더 지정 스캔(`scan`)이 바뀌지 않은 사진의 EXIF 를 다시 읽던 것(klib 경로)도 고쳤다.
- 이전 저장 목록 `scan-cache/photos.json` 은 더 쓰지 않는다(파일은 지우지 않는다).

## 처리 규칙

| 상황 | 동작 |
|---|---|
| 새 파일, 크기·수정 시각이 바뀐 파일 | 촬영일을 비워 두고(= 아직 안 읽음) 배경 정리에서 읽는다 |
| 바뀌지 않은 파일 | 다시 읽지 않는다(이전 버전 DB 의 값 그대로) |
| 정리 중지·앱 종료 | 저장한 데까지 남고, 다음에 열 때 비교 후 남은 것부터 |
| 디스크에서 사라짐, 태그·즐겨찾기 없음 | 행 삭제 |
| 디스크에서 사라짐, 태그·즐겨찾기 있음 | 행은 남기고 숨김(`missing`), 다시 나타나면 그대로 보임 |
| 원천이 비었음·취소·실패 | 삭제하지 않음 |
| 클라우드 전용 | 목록에는 보이고 내용은 읽지 않음(읽기 직전에도 속성 확인) |
| 대소문자만 다른 중복 행(이전 버전 DB) | 하나로 합치고 태그·즐겨찾기를 모음 |
| 제외 폴더·빠진 드라이브 | 화면에서만 숨김 |
| DB 오류 | DB 파일을 지우지 않음(문서 목록 DB 와 다름) |

## 측정 (합성 JPEG 5만 장, 전 = `f7c16b3a`)

`scripts/bench_library_scan.py --kind photo --count 50000 --runs first cached changed`(원천 klib, 후는 `--no-search-probe`). 화면과 같은 순서로 서비스를 부르고 브리지 전달은 JSON 변환·해석으로 대신했다. 설치본·실제 사진·색인 서비스 시간이 아니다. 단위 초, 각 1회.

| 상황 | 항목 | 전 | 후 |
|---|---|---|---|
| 첫 열기(빈 DB) | 첫 묶음 | 0.14 | 0.13 |
| | 전체 표시 | 4.6 | 3.1 |
| | 비교 완료 | 139.6 | 5.0 |
| | 촬영 정보까지 전부 | 141.9 | 122.0 |
| 다시 열기 | 첫 묶음 | 1.45 | 0.98 |
| | 전체 표시(최신 목록) | 9.2 | 1.46 |
| | 완료 | 9.2 | 3.7 |
| 1% 변경(수정 200·추가 150·삭제 150) | 첫 묶음 | 1.04 | 0.75 |
| | 변경분이 화면에 모두 반영 | 8.6 | 3.7 |
| | 촬영 정보까지 전부 | 8.6 | 5.3 |
| 검색 | 이름(100건) | 0.125 | 0.132 |
| | 전체(약 5만 건) | 0.75 | 0.73 |
| | 태그(0건) | 0.080 | 0.078 |

- 전의 "다시 열기" 첫 묶음 1.45초는 옛 JSON 저장 목록이고, 최신 목록은 9.2초에 왔다.
- DOCDB1 에서 문서뷰의 배경 비교 완료가 느려졌던 현상은 사진뷰의 이 측정에서는 없었다(비교 완료가 세 상황 모두 전보다 빠르다).
- 정리 중 1초마다 이름 검색을 넣은 다른 회차: 검색 중앙값 0.17초·최대 0.34초, 첫 열기 전부 완료 150.0초(검색 부하 포함이라 위 표와 비교하지 않는다), 비교 완료 7.6초.
- 브리지 전달량(다시 열기): 34.5MB → 16.7MB.
- `photo_album.db` 크기: 22.8MB → 25.9~26.0MB. 전에는 여기에 JSON 17.4MB 가 더 있었다.
- 합성 사진은 수백 바이트라 실제 사진의 첫 정리 시간(PM 실측 28분)은 이 값으로 추정하지 않는다.

## 시험

- 추가: `tests/test_photo_library.py` 8개(이전 DB 즉시 표시·재읽기 없음, 중복 행 병합, 새·바뀐 것만 읽기, 사라진 태그 사진 보존, 클라우드, 검색 stat 금지, 정리 중 검색·중지·이어하기, 비교 중 삭제).
- 바꾼 기존 시험: JSON 저장 목록 확인 1건(`test_fileops1`)은 DB 기준으로, 몇몇 시험은 임시 폴더를 지우기 전에 배경 정리를 기다리게 함, `library_scan_stream_test.dart` 사진뷰 시험은 새 동작으로.
- `test_library_db.test_old_saved_list_is_imported_once` 는 **시작 상태(병합 `f7c16b3a`)에서도 실패**했다. DOCDB1 시험이 확장자 목록을 직접 적었는데 main 쪽에서 `.hml` 이 늘어난 병합 어긋남이라, `DOC_EXTENSIONS` 를 쓰도록 시험만 고쳤다.
- 파이썬(venv315, `DOCFIT_FORBID_REAL_DATA=1`, `KLIB_EXE`=기존 릴리스 빌드): `test_photo_library`·`test_library_db`·`test_scan_stream`·`test_photo_album_service`·`test_photofix1`·`test_fileops1`·`test_dblopen1`·`test_default_library_scan`·`test_library_exclusion_settings`·`test_thumb_size_setting`·`test_doc_grid_service`·`test_doc_grid_bridge`·`test_scan_cache_service`
  - `Ran 133 tests in 21.942s` / `OK (skipped=1)`
- `flutter analyze`: `No issues found! (ran in 12.4s)`
- `flutter test`(`library_scan_stream`·`photo_album_screen`·`photoview_ui_gate`·`file_ops`·`context_menu`·`dblclick_open`·`workspace_adapters`): `01:03 +63: All tests passed!`
- Rust 변경이 없어 `cargo test`·빌드는 하지 않았다. 전체 python·flutter 시험은 카드 지시대로 돌리지 않았다.
- 실제 사용자 DB·사진은 열지 않았다. 설치된 색인 서비스는 건드리지 않았다.

## 남은 문제

- 설치본·실제 사진·실제 색인 서비스·17만 장 규모는 확인하지 않았다. 서비스 원천은 PM 실측으로 사진 목록에 12~15초라, 새 파일·삭제 반영은 그만큼 늦다.
- 저장 목록 전체를 화면에 넘긴다(5만 장 약 17MB). 17만 장의 전체 표시 시간은 재지 않았다.
- 날짜별 보기 전환·새로고침의 `list_by_date` 는 여전히 전체를 한 번에 주고 폴더를 훑는다. 폴더·카메라·위치 보기(`place_groups`)와 중복 찾기는 파일마다 읽는다(이번에 안 바꿈).
- 촬영 정보 정리는 비교가 끝까지 간 때만 시작한다. 비교를 취소하면 다음에 열 때 한다.
- 비교 도중(수 초~수십 초) 앱이 꺼지면 그 회차의 비교 결과는 남지 않는다.
- 정리 중 바로 그 순간 읽는 파일은 이동·삭제가 한 번 실패할 수 있다(Windows 파일 잠금).
- 이번부터 앱 밖에서 지운 사진은 태그·즐겨찾기가 없으면 DB 에서 행이 지워진다(전에는 남기고 목록에서만 감춤).
- 색인에서 빠졌지만 파일이 있는 항목은 남는다(DOCDB1 과 같음).
