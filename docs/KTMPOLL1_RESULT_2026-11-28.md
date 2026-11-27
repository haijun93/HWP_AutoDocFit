# KTMPOLL1 — KTM 상태 반복 실행 조사·수정 (2026-11-28)

## 결과와 범위

- `feature/KTMPOLL1`에서 Python 제품 코드 2개, 신규 시험 1개, 결과 문서와 TODO를 변경했다. 기존 시험·Flutter·Rust·버전 번호는 변경하지 않았다.
- 설정 상태를 브리지 수명 동안 5초 캐시한다. 같은 시각의 설정 조회 120회는 가짜 runner 실행 **120→1회**, 동시 조회 40회는 **40→1회**로 줄었다.
- 가상 시계의 0~119초에 1초마다 설정을 조회하면 수정 후 `--status`는 **24회**다. 수정 전은 조회마다 실행하는 경로이며, 120회 실행을 실패 시험으로 확인했다. 같은 120회 조회 기준 80% 감소다. 실제 시간·CPU·설치본 프로세스 수를 측정한 결과는 아니다.
- KTM 설정 저장 직후에는 캐시의 옛 상태 대신 **켜는 중/끄는 중**을 반환한다. 백그라운드 명령 완료 후 실제 상태를 즉시 다시 읽어 캐시를 교체한다. 실패도 오류로 교체한다. TTL을 기다릴 필요가 없다.
- 관련 Python **Ran 50 tests / OK**, 제외·실패·오류 0. 신규 시험 10개다.
- 설치본·설치 서비스·자동 실행 등록·사용자 설정·사용자 문서·실행 중인 앱은 조작하지 않았다. 로컬 커밋만 하며 푸시하지 않는다.

## 호출 경로

주기적 원인은 메인 새로고침이나 파일 전달이 아니라 **몽땅검색 화면의 개발자 모드 확인**이다.

```text
TimemachineScreen.initState
  → _pollDeveloperMode
  → Timer(1초) → await api('get_settings') → 완료 후 다시 Timer(1초)
  → DesktopWebBridge.get_settings
  → _settings_snapshot → _attach_ktm_status
  → [수정 전] 매번 새 KtmService().status()
  → runner([ktm.exe, --status])
```

근거:

- `flutter_ui/lib/features/timemachine/timemachine_screen.dart`: `_pollDeveloperMode`는 첫 화면 생성 때 시작하고 `dispose` 때 취소한다. 요청 완료 뒤 다음 1초 타이머를 잡으므로 실제 간격은 **1초 + 요청 처리 시간**이다. `_load`도 초기 설정 조회를 한 번 한다.
- `flutter_ui/lib/features/workspace/workspace_adapters.dart`: 몽땅검색 화면을 작업 공간에 전달한다.
- `flutter_ui/lib/features/workspace/tab_navigation_shell.dart`: 일반 앱은 모든 탭을 `IndexedStack` 자식으로 유지한다. `TickerMode`는 Dart `Timer`를 취소하지 않으므로 아웃라이너를 보는 중에도 몽땅검색 타이머가 실행될 수 있다. 전용 창은 문서편집과 선택 화면만 생성하므로 화면 구성에 따라 달라진다.
- `flutter_ui/lib/main.dart`: 1초 `refresh`는 `get_state`, 700ms 수신 타이머는 `handoff(action: take)`를 부른다. 두 백엔드 경로에서 `_settings_snapshot`이나 KTM 상태 조회로 이어지는 호출은 확인되지 않았다.
- 설정 창 자체는 열 때 `get_settings`를 호출한다. 설정 저장 성공 시 창을 닫으며, 저장 직후 다시 열어도 최신 캐시 또는 적용 중 상태를 읽는다.

사용자 관찰의 약 1.07초 간격과 코드의 구조는 부합한다. 다만 당시 실행 추적을 새로 수집하지 않았으므로 96회 모두의 호출자와 이후 8초간 2회로 줄어든 이유까지 확정하지 않는다.

## 캐시와 설정 적용 상태

`docfit_app/ktm_service.py`의 `KtmStatusCache`를 `DesktopWebBridge`마다 하나 둔다. 기본 `KtmService.status()`의 직접 조회 계약과 앱 시작 시 `sync`는 유지한다.

| 상태 | 다음 동작 |
|---|---|
| 미조회 | 첫 요청에서 조회하고 결과 보관 |
| 유효 | 5초 동안 성공·오류 결과를 재사용 |
| 만료 | 다음 요청에서 한 번 조회; 자체 주기 타이머 없음 |
| 적용 중 | 저장 성공 직후 세대를 올리고 켜는 중/끄는 중 반환; 별도 조회 생략 |
| 적용 완료 | 수명 명령 성공 후 `--status` 결과를 즉시 보관 |
| 적용 실패 | 명령 오류 또는 예외를 보관하고 다음 만료 때 재조회 |

- 조회·수명 명령은 잠금으로 직렬화하고 동시 요청은 캐시를 다시 확인한다.
- 적용 중에는 오래된 조회 응답이나 이전 설정 작업 결과가 최신 상태를 덮지 못한다. 빠른 설정 변경도 마지막 선택으로 끝난다.
- 반환값은 복사해 호출자의 수정이 캐시를 오염시키지 않게 했다.
- 다른 프로세스나 외부에서 바꾼 상태는 다음 만료 후 조회에서 반영된다(기본 5초 + 요청/명령 처리 시간). 브리지 사이의 캐시는 공유하지 않는다.
- 설정 저장은 기존 검증·revision 확인이 성공한 뒤 수명 명령을 시작한다. 설치·자동 실행·켜기·끄기 명령의 의미는 변경하지 않았다.

## 다른 주기 경로 점검

아래는 Flutter 타이머, 브리지, 관련 Python 서비스, 사이드바 타이머에서 확인한 **11종 경로**다. 저장소의 모든 Rust 실행 파일과 모든 작업 루프를 전수 검증한 목록은 아니다. KTM 이외 제품 코드는 변경하지 않았다.

| 경로 | 주기·조건 | 프로세스 실행 여부와 조치 |
|---|---|---|
| 몽땅검색 개발자 모드 | 응답 후 1초 | 매번 KTM 상태 실행하던 경로를 이번에 캐시 |
| 메인 상태·작업 이벤트 | 1초 / 작업 중 350ms | 상태·이벤트 읽기. 파일변환 모드에서는 아래 준비 상태 경로도 실행 |
| 파일변환 준비 상태 | 메인 조회 중 30초 캐시 만료 | `desktop_web_ui._conversion_prerequisites` → `kordoc_bridge.engine_status` → 조건부 `node --version`. 내장 런타임/꾸러미가 없고 로컬 kordoc 또는 허용된 npx 경로일 때 실행할 수 있음. 기존 30초 캐시 유지 |
| 파일 수신 | 전용 수신 창 700ms | `handoff.take`는 수신함 파일 읽기. 빈 수신함은 프로세스 실행 0회. 실제 보내기/수신 작업의 창·엔진 실행은 별도 동작 |
| 몽땅검색 스캔 상태 | 스캔 중 응답 후 1초 | `_status`는 DB·에이전트 실행 여부/상태 파일을 읽음. KTM 실행 없음 |
| 몽땅검색 내용 색인 상태 | 색인 중 응답 후 1초 | `content_status` → 검색 백엔드. SQLite는 DB 조회, RustBackend는 살아 있는 `klib search-content` 재사용. 프로세스가 없거나 종료됐으면 요청 시 재실행할 수 있음 |
| 자료함 스캔 상태 | 스캔 중 기본 400ms | `box_scan_status` → LibraryBridge 파이프. 연결 실패 시 요청마다 `klib` 시작을 한 번 시도할 수 있음. 실패 재시도·복구 의미가 있어 변경하지 않음 |
| 글 편집 keepalive | 문서당 기본 30초 | `KlibEditorCore`·`LargeTextSession`의 `edit_info`는 LibraryBridge를 사용. 정상 연결은 재사용, 파이프 실패 시 위 재실행 가능 |
| 검색 벤치마크 상태 | 실행 중 300ms | `QmsBenchService`의 메모리 상태/이벤트 읽기. 실제 벤치마크 엔진 실행과 분리됨 |
| 검색 복구·상주 확인 | 복구 작업 충돌 시 60초, 최대 30회 / 업데이트 기본 6시간 | 검색 복구는 kfind 파이프·필요한 색인 작업, 상주는 업데이트 조회. `resident.sync_search`의 KTM 점검은 시작 때 한 번이며 주기 실행 아님 |
| 인스턴스 수신·사이드바 | Tk 500ms / 사이드바 150ms | 수신함·창 상태·설정/업데이트 알림 파일 읽기. 비어 있는 상태의 정기 KTM 실행 없음. 실제 요청이 있으면 작업/창을 실행할 수 있음 |

이 밖에 문서뷰·사진뷰 경과 시간, 슬라이드쇼, 진행 화면 타이머는 화면 상태를 갱신한다. 디바운스 타이머는 입력·저장 요청에 따라 동작하며 상시 상태 조회와 구분했다. 조건부 복구 경로를 억제하면 작업 상태/복구 동작이 달라질 수 있어 이번 수정에 포함하지 않았다.

## 시험

처음 추가한 시험 3개를 수정 전 실행:

```text
Ran 3 tests
FAILED (failures=2)
```

- 설정 조회 120회 → runner 120회라 실패.
- 동시 조회 40회 → runner 40회라 실패.
- 상태 필드 없는 스냅샷·빈 수신함 120회 → runner 0회로 통과.

최종 시험은 공유 개발 가상환경의 Python 3.14로 저장소 루트에서 실행했다. 임시 폴더는 작업 폴더 밖의 합성 시험 영역으로 지정했다.

```text
python -m unittest tests.test_ktm_status_polling tests.test_ktm_service tests.test_configuration_service tests.test_bridge_methods tests.test_tk0_direct_reads.DirectStateReadTests tests.test_tk0_direct_reads.ZeroClickInvalidationTests tests.test_native_tasks.NativeTasksTests.test_runtime_settings_apply_service_only_for_changed_search_and_keep_other_keys tests.test_native_tasks.NativeTasksTests.test_runtime_service_failure_does_not_save_desired_setting

Ran 50 tests in 1.227s
OK
```

신규 10개는 반복/동시 조회, 2분 가상 시계, 필드 없음·빈 수신함, 실제 임시 설정 저장 뒤 적용 중/완료, 오류 캐시·만료, 적용 실패, 반환값 변경, 오래된 응답 경합, 빠른 설정 변경을 검사한다. 실제 KTM 실행은 모두 가짜 runner로 대체했다. 버전 번호 고정과 기존 시험 수정은 없다. `git diff --check`도 통과했다.

## 미확인·후속

- 실제 설치본/EXE에서 프로세스 수 재측정, 화면별 체감 속도, 장시간 동작은 미확인이다. 이번 작업에서 설치본이나 실행 중 앱을 조작하지 않았다.
- 전체 Python 시험과 COM 시험은 실행하지 않았다. Flutter 소스를 바꾸지 않아 analyze·위젯 시험·웹 빌드는 실행하지 않았다.
- 일반 앱에서 숨은 탭의 개발자 모드 타이머 자체는 유지한다. 이벤트 기반 갱신이나 별도 경량 API로 옮기는 것은 추가 범위다.
- LibraryBridge의 연결 실패 재실행 정책은 후속 조사 대상이며, 이번에 억제하지 않았다.
