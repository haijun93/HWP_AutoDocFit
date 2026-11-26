# CTXEXT1 — 문서뷰 입력 형식 연결 (2026-11-26)

상태: 구현·관련 검증 완료. 기능 브랜치 `feature/CTXEXT1`, 로컬 커밋만 수행하며 푸시·태그·배포하지 않는다. 기준 커밋 `a502f109`.

## 범위와 판정

카드 대상은 XLS·PPT·PPTX·HML 4형식이다. 기본 엔진 kordoc 4.24.9와 기존 후처리 경로를 유지한다.

| 입력 | 문서편집 메뉴 | 파일 변환 메뉴 | 실제 확인 |
|---|---|---|---|
| XLS | 활성 | 활성 | 합성 OLE/BIFF8 → HWPX, 표 안의 글·숫자 보존 |
| PPTX | 활성 | 활성 | 합성 슬라이드 → HWPX, 제목·한국어 본문 보존 |
| HML | 활성 | 활성 | 합성 HWPML → HWPX, 한국어 본문·숫자 보존 |
| PPT | 비활성 + `.ppt` 미지원 이유 | 비활성 + `.ppt` 미지원 이유 | 기존 변환 입력 정책에서 거부, 결과 미생성 |

PPT는 kordoc 입력 지원 목록에 없으므로 열지 않았다. PPT 시험은 합성 바이트 파일에 대한 확장자 거부 확인이며 실제 PowerPoint 바이너리 파싱 시험이 아니다. 지원 3형식도 복잡한 실문서의 조판·레이아웃 충실도를 검증한 것은 아니다.

## 변경과 상태 전이

- `native_file_dialog_service.DOCUMENTS/ADDABLE`: XLS·PPTX·HML을 파일 선택·경로 추가·폴더 추가의 공용 허용 목록에 추가했다. 다른 목적(서식 예시 등)의 제한은 그대로다.
- `file_handoff.dart`: 문서편집·파일 변환 메뉴에 같은 3형식을 허용했다. PPT가 섞인 선택은 묶음 전체 비활성과 이유 표시를 유지한다. 대문자 확장자도 동일하다.
- `doc_grid_service.DOC_EXTENSIONS`: HML이 실제 문서뷰에 나타나도록 목록 검색·Rust 스캔 요청·캐시 키·파일 작업·열기의 공용 확장자에 추가했다. 이전 문서뷰는 HML을 나열하지 않았다.
- `kordoc_bridge.parse_document`: HML과 PPTX의 XML/관계 파일을 외부 엔진에 넘기기 전 `defusedxml`로 검사한다. `forbid_dtd`, `forbid_entities`, `forbid_external`을 모두 켠다. HML 50MiB 상한과 기존 Office ZIP 경로·중복·압축 해제 한도를 적용한다.
- 기존 문서편집 사전 변환 함수가 3형식을 이미 kordoc HWPX 변환으로 보냈으므로 엔진·후처리 알고리즘을 바꾸지 않았다.

| 상태 | 조건/다음 상태 |
|---|---|
| 선택 | 카드의 3개 지원 형식 → 전달 가능; PPT 포함 → 이유가 있는 전달 차단 |
| 전달 가능 | 기존 `add_paths`의 경로·재분석 지점·파일 수 검사를 통과해야 목록 등록 |
| 목록 등록 | 수신 목록에 등록된 것이 확인되면 대상 화면으로 이동; 자동 작업 시작 없음 |
| 변환 요청 | HML·PPTX 안전 검사 → kordoc 파싱·생성 → 기존 HWPX 검사·새 경로 게시 |
| 실패 | XML 거부/미지원 입력/기존 출력 충돌 → 결과 게시 차단, 원본 보존 |

Rust 크레이트 변경은 없다. Python은 기존 경로 정책과 외부 엔진 호출 앞의 보안 검사를 수정했으며, 새 변환 엔진을 구현하지 않았다. 기존 시험 파일은 수정하지 않고 신규 시험 모듈을 추가했다.

## 실제 엔진 시험

`DOCFIT_CTXEXT1_ENGINE_TEST=1`, 오프라인 모드와 설치된 고정 kordoc 4.24.9로 `python -m unittest -v tests.test_ctxext1_engine`을 실행했다. 임시 파일은 저장소 밖 작업용 임시 디렉터리에만 만들었다. 사용자 문서·사진·한/글 COM은 사용하지 않았다.

```text
Ran 4 tests in 7.923s
OK
CTXEXT1 .hml: HWPX 42057 bytes; 본문 ['CTXEXT1 HML', '합성 본문', '42'] 보존; 원본 SHA256 동일; 덮어쓰기 거부
CTXEXT1 .ppt: 입력 형식 거부; 결과 미생성; 원본 동일(실제 PPT 파싱 미실행)
CTXEXT1 .pptx: HWPX 51213 bytes; 본문 ['CTXEXT1 PPTX', '합성 슬라이드 본문'] 보존; 원본 SHA256 동일; 덮어쓰기 거부
CTXEXT1 .xls: HWPX 54855 bytes; 본문 ['CTXEXT1 XLS', 'Synthetic', '42'] 보존; 원본 SHA256 동일; 덮어쓰기 거부
```

XLS 표본은 표준 라이브러리로 생성한 OLE 복합 파일의 BIFF8 Workbook이다. 확장자만 바꾼 XLSX가 아니다. HWPX ZIP/XML 유효성, 기대 문자열, 원본 SHA256, 기존 출력 덮어쓰기 거부를 함께 확인했다.

## 관련 검사

PM의 가속 지시를 적용하여 전체 Python discovery와 전체 Flutter test는 실행하지 않았다. 변경 Rust 크레이트가 없어 cargo test·Rust 빌드는 실행하지 않았다. Python은 지정된 3.15 가상환경에서 실행했다.

첫 관련 Python 실행은 신규 시험의 `_install` 인자 누락으로 75개 중 오류 1개였다. 신규 시험만 수정하고 재실행했다.

```text
Ran 75 tests in 2.352s
OK
```

위 실행 모듈: `tests.test_ctxext1`, `tests.test_v2_file_dialog_service`, `tests.test_image_to_pdf_input`, `tests.test_file_conversion`, `tests.test_kordoc_bridge`, `tests.test_kordoc_contract`.

HML 문서뷰 검색 시험 추가 후 위 6개 모듈과 `tests.test_doc_grid_service`, `tests.test_doc_grid_bridge`, `tests.test_bridge_methods`까지 최종 실행했다.

```text
Ran 116 tests in 5.541s
OK (skipped=1)
```

116개 중 115개 통과, 기존 klib/fixture 실물 미리보기 시험 1개 환경 조건 제외다. COM은 실행하지 않았다.

Flutter 정적 분석:

```text
No issues found! (ran in 23.1s)
```

관련 위젯 시험은 `test/ctxext1_test.dart`, `test/context_menu_test.dart`, `test/conversion_workspace_test.dart`, `test/conversion_tab_native_test.dart`만 지정했다. 최초 실행은 37개 통과·신규 시험 1개 실패였다. 혼합 XLS/PPT 선택에서 `.ppt`만 적힌 이유는 문서편집·변환 2곳이고 업무자료함은 `.xls, .ppt`를 표시하는데, 신규 시험이 3곳으로 잘못 기대했다. 신규 기대값만 2곳으로 수정한 뒤 38개 전부 통과했다. 기존 시험은 바꾸지 않았다.

```text
01:28 +38: All tests passed!
```

신규 위젯 시험 8개는 지원 3형식 각각의 문서편집/변환 대상 목록 연결 6개, PPT 혼합 비활성·이유 1개, 형식 정책 1개다. 기존 파일의 UTF-8/LF를 유지했고, `hwp-auto-docfit.py`는 변경하지 않아 작업 트리의 CRLF를 보존했다. `git diff --check` 통과. 저장소에 이 PC 전용 경로를 추가하지 않았다.

Flutter 웹/Windows 빌드는 실행하지 않았다. 릴리스 전 웹 빌드는 PM 통합본에서 별도 확인이 필요하다.

## 실패 시도와 보호 조건

- 시작 당시 전체 시험 잠금이 있어 구현·조사만 했다. 잠금이 해제된 뒤 Python·실제 엔진 시험을 실행했다. 잠금은 직접 삭제하지 않았다.
- 최초 XLS 합성 표본(BIFF2 단독 스트림)은 실제 엔진이 `지원하지 않는 파일 형식입니다`로 거부했다. 일반 OLE/BIFF8 표본으로 바꿔 성공했다. BIFF2 지원은 주장하지 않는다.
- 이후 `bench_library_scan.py` 실행을 확인하여 Flutter 검사를 보류했고, 프로세스 종료를 확인한 뒤 검사에 착수했다.

## 남은 확인과 기존 제한

- 실제 WebView/설치본의 오른쪽 클릭, 한/글에서 결과 열기·후처리 최종 저장·PDF/그림 출력은 확인하지 않았다.
- CTXMENU1의 Win32 별도 변환 창 전달 제한, 공용 파일 목록 혼합에 따른 출력 제한은 그대로다.
- 이 카드는 XLS·PPT·PPTX·HML 4형식에 한정했다. XLSX와 HTML 등 다른 형식의 기존 목록/메뉴 제한은 확장하지 않았다.
- 네이티브 Flutter 실행기의 별도 `ApplicationService.add_paths` 제한은 이 카드에서 바꾸지 않았다. 운영 WebView의 `DesktopWebBridge.add_paths`를 신규 Python 시험으로 확인했다.
- GitLab 접근·원격 푸시·태그·게시·설치본 교체는 하지 않았다.
