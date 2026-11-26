# MERGERUST 통합 결과 (2026-11-27)

`feature/MERGERUST`, 시작 커밋 `f2efee4b`(v2.2.110). 아래 순서로 `git merge --no-ff`를 실행하고 단계별 시험 뒤 커밋했다. 푸시·태그·배포·설치는 하지 않았다.

| 순서 | 병합 원천 | 병합 커밋 |
|---|---|---|
| 1 | `feature/RUSTVIEW2` — SIDEMERGE1·RUSTSIDE1·2 포함 | `8ca4d382` |
| 2 | `feature/RUSTVIEW1` — 문서뷰·사진뷰 Rust 기능 | `9fc431d6` |
| 3 | `feature/OUTLINEUI4` — 참조·임베드·속성·태그 | `1e6a2259` |

## 충돌 해결·추가 보완

| 단계 | 충돌 | 해결 |
|---|---|---|
| 1 | `TODO.md`, `reports/PM-HANDOVER.md` | 양쪽 내용을 이어 붙임. 양쪽 원본의 비어 있지 않은 줄을 각각 대조해 누락 0줄 |
| 1 | `hwp-auto-docfit.py` | 화면 없는 작업 프로세스가 읽는 앞쪽 선언에 `APP_VERSION = "2.2.110"`을 두고 뒤쪽 중복 선언 제거. 선언 1개·CRLF 유지 |
| 2 | `TODO.md` | 양쪽 항목 보존. TODO·PM 인계 모두 양쪽 원본 대비 누락 0줄 |
| 2 | 사진 DB의 의미상 충돌(자동 병합에서는 드러나지 않음) | 기존 `test_photoview_library_schema`가 Rust 스키마의 `missing` 열 누락을 재현. Python v2.2.110과 같은 `missing INTEGER NOT NULL DEFAULT 0` 추가. 이전 DB는 쓰기 트랜잭션에서 열만 추가하며 태그·즐겨찾기·촬영 정보 보존 |
| 3 | `TODO.md`, `reports/PM-HANDOVER.md` | 양쪽 내용 보존, 각 원본 대비 누락 0줄. Rust 코드는 충돌 없이 병합 |

사진 DB 회귀 시험 2개를 `photoview_win/tests/mergerust_library.rs`에 추가했다. 이전 스키마의 메타데이터·표시 보존, 누락 행의 표시 변경 시 `missing`·촬영 정보·다른 목록 상태 보존을 검사한다. 기존 스키마 대조 시험은 수정하지 않았다.

추가 통합 회귀 시험 `tests/test_mergerust.py` 4개: 작업 프로세스와 본체의 단일 버전 선언, CRLF, 기본 실행 선택, 배포 검사 대상, 설치 안내 보존. 기존 Python 시험 파일은 변경하지 않았다. 기존 Rust 시험의 삭제·주석 처리·판정 완화도 없다.

## 기본 동작 변화

- **기본 화면과 기본 실행 방식은 그대로다.** `launch_mode="sidebar"` 또는 `--sidebar`를 고르거나 `sidebar.exe`를 직접 실행해야 사이드바 방식이 켜진다. `--full-app`은 기존 앱을 연다. 문서뷰·사진뷰의 `*_ui` 기본값은 `flutter`로 유지된다. 사용자 설정 파일을 바꿔 활성화하지 않았다.
- 별도 화면 창은 메뉴별 단일 인스턴스·작업 상태 파일을 사용하고, 그 창이 바꾼 설정 키만 저장한다. 파일 전달은 수신 목록 추가이며 자동 작업 시작이 아니다. 사이드바 로그온 시작은 기본 꺼짐이다.
- 기본 앱에도 RUSTSIDE2의 **미저장 편집 닫기 확인**이 추가된다. 변환 중 닫기는 중단 확인을 거친다. 따라서 화면 기본값은 같지만 종료 보호 동작까지 변화가 없다고 주장하지 않는다.
- SIDEMERGE1의 파일 작업과 여러 파일 보내기를 한 메뉴에 합친 동작을 유지한다. RUSTVIEW1의 사진 전용 메뉴도 그 메뉴에 함께 남는다.
- v2.2.110의 문서·사진 목록 DB, 저장 목록 우선 표시, 증분 반영, 촬영 정보 배경 정리, 구버전 색인 서비스 안내·사용자가 고르는 재시작 경로를 유지했다. 관련 Python·Flutter 시험을 다시 통과했다. 이 Python 서비스와 두 기본 화면의 해당 소스는 시작 커밋에서 바뀌지 않았다.
- Rust 문서뷰·사진뷰는 기존처럼 직접 `klib scan`을 사용한다. Python 목록 DB의 즉시 표시 경로를 Rust 창에 새로 이식한 것은 아니다. Rust 사진뷰가 공유하는 `photo_album.db`의 태그·즐겨찾기 스키마는 이번 통합에서 v2.2.110과 맞췄다.

## 설치·CI·패키징

- `sidebar.exe`는 dock 크레이트에서 빌드되어 CI 복사·동결 번들 검사·spec 수집·설치 파일의 재귀 포함·종료 도우미에 들어간다. 설치 스크립트의 색인 서비스 재시작 안내도 보존했다.
- 사이드바 SHA-256 생성·존재 검사와 spec 수집을 추가했다. CI의 VC++ 런타임 의존성 검사에 `sidebar.exe`, `dock.exe`, `rename_win.exe`를 추가했다.
- `native_tasks`는 독립 exe가 아닌 공통 Rust 라이브러리다. 변환·설정·파일명 창에 연결되어 빌드된다. 의존 라이브러리의 자체 시험이 빠지지 않도록 CI에 별도 `cargo test`를 추가했다.
- `outliner_win`, `settings_win`, `docview_win`, `photoview_win`, `convert_win`, `rename_win`, `dock`, `sidebar` 실행 파일의 CI·spec·번들 검사 연결을 확인했다. `tests/test_distribution_build.py`를 반드시 실행했으며, 통합 검사와 함께 별도 실행한 19건도 통과했다.
- Inno Setup은 합성 텍스트 파일로 만든 dist만 묶어 컴파일했다. `[Code]`가 원문과 같음을 확인하고 파일 원천·출력 위치·아이콘 위치·압축만 시험용으로 바꿨다. 컴파일 성공, 설치 프로그램 실행 없음. 기존 `x64` 식별자 폐기 예정 경고는 남는다.
- GNU 도구 체인으로 검증했다. 실제 CI/MSVC·PyInstaller 동결 번들·설치/업데이트/제거 검증으로 확대 해석하지 않는다.

## 시험 범위와 최종 수치

| 단계 | Rust | 관련 Python | Flutter |
|---|---|---|---|
| 1 | 변경 8크레이트 273 통과·기존 제외 1 | 539건 중 538 통과·기존 제외 1. 이후 2·3단계의 완전 합성 재시험이 이 범위를 포함 | 분석 이상 없음, 전체 위젯 897 통과·기존 제외 3, 웹 릴리스 빌드 성공 |
| 2 | 변경 4크레이트 173 통과·기존 제외 2 | 최종 합성 558건 중 557 통과·기존 제외 1 | 분석 이상 없음, 관련 12파일 84 통과 |
| 3 | 변경 2크레이트 334 통과·제외 0 | 최종 합성 61모듈 586건 중 585 통과·기존 제외 1 | 분석 이상 없음, 관련 7파일 41 통과 |

최종 코드 기준 변경된 **11크레이트**의 시험 합계는 **642 통과·기존 제외 2·실패 0**이다. 단계 간 같은 시험의 재실행을 중복 합산하지 않았다.

| 최종 크레이트 | 통과 | 기존 제외 |
|---|---:|---:|
| native_tasks | 5 | 0 |
| convert_win | 18 | 0 |
| settings_win | 32 | 0 |
| rename_win | 39 | 0 |
| dock | 41 | 0 |
| grid_core | 42 | 1 |
| docfit_config | 9 | 0 |
| docview_win | 52 | 0 |
| photoview_win | 70 | 1 |
| outliner_core | 147 | 0 |
| outliner_win | 187 | 0 |

Rust 제외 2건은 기존 썸네일·사진 처리 시간 측정이다. Python 제외 1건은 실제 휴지통 시험의 기존 조건이다. 성능 측정은 하지 않았으며 시험 소요 시간을 성능 개선 근거로 쓰지 않는다. 전체 Python discover는 실행하지 않았다(PM 담당).

## 합성 실창·재현

- RUSTVIEW2: 실제 Rust 창에서 23종 규칙 폼, 드롭·미리보기·이름 변경·이력·되돌리기·미저장 닫기 보호, 합성 IDO 저장·HWPX 읽기 전용 관리, 12개 변환 대상 표시·합성 PDF 병합 순서·입력 바이트 보존 통과. 세 범위의 PASS 줄을 아래에 적었다. [파일명 미리보기](MERGERUST_2026-11-27/rename-preview.png), [PDF 병합 완료](MERGERUST_2026-11-27/convert-complete.png)를 직접 열어 확인했다.
- RUSTVIEW1: 합성 사진 2개와 그 두 경로만 반환하는 시험용 스캐너로 실창 즐겨찾기·크게 보기·다음 사진 이동을 확인했다. Rust가 저장한 즐겨찾기 2개를 Python `PhotoAlbumDb.search()`로 읽음, 사진 해시·격리 설정 원문 보존 통과. [실제 사진뷰](MERGERUST_2026-11-27/photo-grid.png)를 직접 확인했다.
- 썸네일 저장은 별도 실창 시험에서 스캔을 끄고 두 창 모두 240 저장·다른 설정 키 보존을 확인했다. 사용자 설정은 사용하지 않았다.
- OUTLINEUI4: 기존 Win32 참조·속성·태그 시험을 모두 재실행했다. 캡처 시험 1개를 출력 폴더를 지정해 추가 실행하여 3테마×96/192 DPI의 6장을 다시 생성했다. 밝게 96·어둡게 192 두 장을 직접 열어 인라인 참조·임베드 자식·우측 속성·태그를 확인했다. 나머지 4장은 생성 및 기존 화소 검사를 통과했다.

재현은 저장소 루트에서 앱 가상환경 Python을 사용한다. 이 작업 사본에는 가상환경이 없어 공유 개발 가상환경 Python 3.14로 실행했다. 도구 경로는 기록하지 않는다. `CARGO_TARGET_DIR`, `TEMP`, `TMP`는 작업 폴더 밖 시험 전용 폴더로 지정하고 MinGW bin을 PATH 앞에 둔다. `KLIB_EXE`·`DOCFIT_KLIB`는 준비된 개발용 klib, `DOCFIT_SIDEBAR_TEST_EXE`는 이번에 빌드한 sidebar를 지정한다.

```text
python scripts/test_mergerust_synthetic.py --stage 1
python scripts/test_mergerust_synthetic.py --stage 2
python scripts/test_mergerust_synthetic.py --stage 3
python -m unittest tests.test_distribution_build tests.test_mergerust
cargo test --locked --manifest-path <크레이트>/Cargo.toml
flutter analyze --fatal-infos
flutter test --reporter expanded
flutter build web --release
```

합성 실행기는 기존 시험을 그대로 로드하고 공개 HWP 미리보기 입력을 합성 CFB/PNG로, 기본 HWPX 문서 입력을 메모리에서 정의한 합성 서식 패키지로 바꾼다. 앱에 동봉된 IDO·규칙·글꼴 등의 프로그램 자산은 시험 대상이다. 기존 fixture나 시험 기대값은 고치지 않는다. `tests/__init__.py`의 사용자 데이터 격리·`DOCFIT_FORBID_REAL_DATA=1`을 유지했다.

1단계 Flutter 전체 실행 뒤 두 후속 병합은 Flutter 소스를 바꾸지 않았다. 2단계는 목록 DB·서비스 안내·파일 작업·별도 창 12파일, 3단계는 아웃라이너 4파일과 별도 창·목록 DB 3파일을 재시험했다. 웹 빌드도 같은 Flutter 소스에서 성공했다. Wasm 안내·미동봉 Cupertino 글꼴 경고는 남지만 종료 코드는 0이다.

## 실패 시도·시험 도구 보완

- 초기 Python 489건은 Rust 보조 바이너리 미준비로 10건 제외였다. sidebar·설정 CLI를 준비한 뒤 539건·제외 1로 재실행했다. 이 초기 두 실행은 동봉 HWPX 바탕을 일부 사용했으므로 최종 합성 근거는 2단계 558건·3단계 586건이다. 사용자 문서는 사용하지 않았다.
- 2단계 기존 스키마 대조 1건이 먼저 실패했다. 제품 Rust 스키마와 이전 DB 마이그레이션을 고쳐 같은 시험을 통과했다.
- 완전 합성 실행기의 첫 바탕 패키지에 미리보기 항목·쪽 설정·서식 정의가 부족해 558건 중 실패 1·오류 4가 발생했다. 합성 입력 생성기만 보완하고 관련 48건 및 558건·586건을 재실행해 통과했다. 기존 시험 완화 없음.
- RUSTVIEW2 실창 시험 첫 실행은 쓰는 중인 PDF 잠금, 다음은 작성 중인 JSONL 마지막 조각을 읽어 중단됐다. `scripts/smoke_rustview2.py`가 PDF 완성을 기다리고 완성된 로그 줄만 판독하도록 수정한 뒤 세 범위 모두 통과했다. 시험 전용 외부 출력 폴더도 허용하도록 보완했다. 최종 PDF 두 쪽 순서·원본 보존 판정은 그대로다.
- 추가 사진 실창 통합 시도 6회는 주입 도구의 mouse-down 누락·슬라이더 좌표 산술 오류, 목록 준비 전 선택·viewer 스냅숏 시점 문제로 끝까지 통과하지 못했다. 제품 오류로 단정하지 않았다. 즐겨찾기/뷰어/Python DB 왕복은 별도 합성 시험, 두 창의 슬라이더 저장은 `--no-scan` 별도 시험으로 각각 통과했다. 모든 동작을 한 번에 이어 실행한 추가 도구의 완주는 미확인이다.
- 불량 합성 PDF의 pypdf 경고·일부 로깅 출력, 기존 변환·파일명 Win32 시험 보조 함수의 미사용 경고는 남는다. 실패를 숨기기 위해 기존 시험을 변경하지 않았다.

```text
rename: 23 native rule forms + drop/preview/apply/history/undo + dirty close protection PASS
settings: isolated config unchanged + synthetic IDO save + HWPX readonly management/about/runtime settings read PASS
convert: real native 12 targets + ordered PDF merge + sources unchanged PASS
photo: two favorites across grid/viewer + Python DB read + sources unchanged PASS
isolated settings unchanged PASS
thumbnail sizes: photo_album=240, doc_grid=240; unrelated settings preserved PASS
```

## 아직 없는 기능·미확인

- Rust 사진뷰: 사진대지 HWPX, 날짜·폴더·카메라·위치 묶음 보기, 편집 미리보기·슬라이더, 크게 보기의 확대·이동은 미구현. 편집 회전·출력 형식 등의 기존 Flutter와 차이는 RUSTVIEW1 결과 문서 그대로다. 중복 dHash의 Python과 경계값 일치·HEIC/WebP 코덱은 미확인이다.
- 사이드바: 실행 방식 선택 설정 화면은 없으며 인자·설정 파일로 고른다. Flutter 별도 창의 시작 비용·메모리, 모든 창의 설정 변경 감시, 전달 요청 만료 안내, 닫기 보호의 남은 범위는 RUSTSIDE2 결과 문서의 제약이 남는다. SIDEMERGE1 이후 여러 파일 보내기는 구현되어 있어 이전의 1개 제한 설명은 현재 상태가 아니다.
- 아웃라이너: 외부 파일 실시간 감시, 미연결 언급 일괄 변환, 속성 삭제 UI, 임베드 내부 직접 편집은 범위 밖. 표시 확장·목록 개수 한도와 긴 줄 잘림은 OUTLINEUI4 결과 문서대로다.
- 실제 한/글 COM·물리 IME·다중 모니터 이동·화면 읽기·장시간/사용자 규모·실제 서비스 재시작·자동 실행 등록·CI/MSVC·동결 EXE IPC·실제 설치/업데이트/제거는 미확인이다. 설치·자동 실행·서비스를 실제로 바꾸는 선행 결과 문서의 시험은 이번 금지 범위라 재현하지 않았다.
- **설치본·설치된 색인 서비스·실제 자동 실행·사용자 설정 변경 없음.** 창 전달 시험의 레지스트리는 시험 전용 임시 키만 사용했다. COM 시험을 하지 않아 COM 잠금도 사용하지 않았다. 사용자 문서·사진·사용자 DB는 읽거나 수정하지 않았다. 전체 Python은 PM 몫이며, 푸시·GitLab 접속도 하지 않았다.

## 실제 시험 출력 전체

아래는 초기 실패를 포함한 모든 `Ran`·`OK`·`FAILED`·`test result` 줄이다. Rust 문서 시험의 0건 출력도 남겼다. `EXIT`는 실행 종료 코드다. 빌드·실창 도구 중 이 형식의 출력이 없는 실행의 결과는 위에 별도로 적었다.

도구 분리 전의 작은 확인 실행:

```text
Ran 6 tests in 0.441s
OK
```

### s1-native_tasks

```text
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=0
```

### s1-convert_win

```text
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=0
```

### s1-settings_win

```text
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.84s
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.54s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=0
```

### s1-python

```text
Ran 489 tests in 149.299s
OK (skipped=10)
SYNTHETIC_HWP_FIXTURE_READS=1
EXIT=0
```

### s1-rename_win

```text
test result: ok. 39 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.81s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=0
```

### s1-flutter-analyze

```text
No issues found! (ran in 49.5s)
EXIT=0
```

### s1-dock

```text
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.73s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=0
```

### s1-grid_core

```text
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=0
```

### s1-docview_win

```text
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 14.16s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.69s
test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.65s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.25s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=0
```

### s1-photoview_win

```text
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 15.57s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.36s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.87s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.49s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=0
```

### s1-python-final

```text
Ran 539 tests in 234.639s
OK (skipped=1)
SYNTHETIC_HWP_FIXTURE_READS=1
EXIT=0
```

### s1-distribution

```text
Ran 19 tests in 0.153s
OK
EXIT=0
```

### s1-flutter-test

```text
13:11 +897 ~3: All tests passed!
EXIT=0
```

### s2-schema-before

```text
Ran 1 test in 0.132s
FAILED (failures=1)
EXIT=1
```

### s2-docfit_config

```text
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.26s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=0
```

### s2-grid_core

```text
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=0
```

### s2-docview_win

```text
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.68s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.83s
test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.31s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.36s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=0
```

### s2-python

```text
Ran 558 tests in 149.761s
FAILED (failures=1, errors=4, skipped=1)
SYNTHETIC_HWP_FIXTURE_READS=1
EXIT=1
```

### s2-photoview_win

```text
test result: ok. 23 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.96s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.70s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.78s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.85s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.36s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=0
```

### s2-synthetic-input-fix

```text
Ran 48 tests in 7.483s
OK
SYNTHETIC_HWP_FIXTURE_READS=0
EXIT=0
```

### s2-final-python

```text
Ran 558 tests in 114.081s
OK (skipped=1)
SYNTHETIC_HWP_FIXTURE_READS=1
EXIT=0
```

### s2-final-flutter-analyze

```text
No issues found! (ran in 11.3s)
EXIT=0
```

### s2-final-flutter-test

```text
01:19 +84: All tests passed!
EXIT=0
```

### s3-outliner_core

```text
test result: ok. 84 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.33s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.86s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=0
```

### s3-outliner_win

```text
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.86s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.63s
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.36s
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.66s
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.57s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.17s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.43s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.57s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EXIT=0
```

### s3-python

```text
Ran 586 tests in 131.831s
OK (skipped=1)
SYNTHETIC_HWP_FIXTURE_READS=1
EXIT=0
```

### s3-outline-captures

```text
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 12 filtered out; finished in 3.37s
EXIT=0
```

### s3-flutter-analyze

```text
No issues found! (ran in 7.4s)
EXIT=0
```

### s3-flutter-test

```text
00:19 +41: All tests passed!
EXIT=0
```
