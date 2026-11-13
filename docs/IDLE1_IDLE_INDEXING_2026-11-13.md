# IDLE1 검색 색인을 쉬는 시간에 자동 진행 (2026-11-13)

사용자 요청(2026-11-13): 검색 색인도 쉬는 시간에 자동으로 진행. 브랜치 `feature/idle-indexing`.

## 1. 무엇을 바꿨나

| 파일 | 내용 |
|---|---|
| `docfit_app/idle_gate.py`(새) | 유휴 판정 상태 기계 `decide()`, Windows 신호 측정 `WindowsIdleProbe`, 상태 보관 `IdleGate`, 스레드 배경 우선순위 `begin_background()`/`end_background()` |
| `docfit_app/idle_scheduler.py`(새) | 작업 인터페이스 `IdleJob`, 실행 손잡이 `IdleContext`(checkpoint·progress·promote), 스케줄러 `IdleIndexScheduler`, 앱 공용 `shared_scheduler()` |
| `docfit_app/timemachine_service.py` | 자동 재색인 요청을 `_schedule_content_refresh()` 한 곳으로, 쉬는 시간 작업 `_ContentIdleJob`, 설정 2개, 상태 필드 |
| `docfit_app/library_idle_refresh.py`(새) | 자료함(가상 폴더) 자료의 원본이 바뀐 것만 klib `index_file`로 다시 색인하는 `LibraryItemRefreshJob` |
| `desktop_web_ui.py`, `docfit_app/application_service.py` | 타임머신 서비스에 공용 스케줄러를 넘기고 자료함 작업을 한 번 등록 |
| `flutter_ui/lib/features/timemachine/idle_indexing_status.dart`(새), `timemachine_screen.dart` | 상태 줄 "쉬는 시간 색인 중 n/m"·"잠시 멈춤 · 이유 n/m", 설정 대화상자에 "쉬는 시간에 색인"·"쉬는 시간 기준" |

## 2. 상태 기계

### 2-1. 사용자 유휴(`IdleGate`, 앱에 하나)

| 상태 | 조건(위에서부터 먼저) | 이유 값 |
|---|---|---|
| `paused` | 설정 "쉬는 시간에 색인" 꺼짐 | `disabled` |
| `held` | 마지막 입력 시각을 모름(다른 OS·API 실패) | `input_unknown` |
| `active` | 마지막 키보드·마우스 입력이 기준 초보다 최근 | `input` |
| `held` | 배터리 전원 / 전체 화면 앱·D3D 전체 화면·발표 모드 | `battery` / `fullscreen` |
| `held` | 이 앱을 뺀 CPU > 25%(쉬는 중이면 60%) / 디스크 > 8MB/s(쉬는 중이면 32MB/s) | `cpu` / `disk` |
| `idle` | 위 어느 것도 아님 | `idle` |

- 신호: `GetLastInputInfo`, `GetSystemTimes`−`GetProcessTimes`(자기 몫 제외), `GetSystemPowerStatus`, `SHQueryUserNotificationState`(2·3·4). 디스크는 Win32 API(`IOCTL_DISK_PERFORMANCE`·`GetProcessIoCounters`, NBIDX1에서 psutil 대신 — 배포판에 psutil이 들어간다는 보장이 없어서). Win32 측정이 실패할 때만 psutil(있으면). 측정은 1초에 한 번까지.
- 불가능한 전이: 설정 꺼짐·입력 있음 상태에서 다른 조건 확인 없이 `idle`로. 부하 기준은 들어갈 때·나갈 때를 달리 둬 색인 자신 때문에 들락거리지 않는다. 입력은 즉시 `active`.

### 2-2. 작업(`IdleIndexScheduler`의 `jobs[이름].state`)

```
none ─request/주기─▶ waiting ─(gate=idle)─▶ running ⇄ yielded ─▶ done | failed | cancelled ─(every)─▶ waiting
```
- 작업은 스케줄러 스레드 하나에서 차례로. 쉬는 시간이 아니면 시작하지 않는다.
- 작업은 문서 하나 시작 전마다 `ctx.checkpoint()`: 쉬는 시간이 아니면 그 자리에서 기다리고(진행 중 문서는 끝낸 뒤) 다시 쉬는 시간이 오면 이어서. 앱 종료·취소면 False.
- `ctx.promote()`: 사용자가 같은 색인을 직접 시작하면 기다림·낮은 우선순위를 풀고 끝까지.

### 2-3. 타임머신 본문 작업 상태(`content_status()` 추가 필드)

| 필드 | 값 |
|---|---|
| `trigger` | `manual`(본문 색인 단추) · `auto`(쉬는 시간 색인 꺼짐 → 예전처럼 곧바로) · `idle`(쉬는 시간) · `null` |
| `total`·`processed` | 목록을 다 훑은 뒤 처리할 문서 수·처리한 수(쉬는 시간 상태 줄 n/m) |
| `yielded` | 쉬는 시간 모드에서 사용자에게 양보 중 |
| `idle` | 스케줄러 상태(`state`·`reason`·`reason_text`·`enabled`·`idle_seconds`·`running_job`·`jobs`) |

기존 `phase`(idle/indexing/reindexing/cancel_requested/cancelled/finished/failed) 값은 그대로다. 양보 중에도 `reindexing`이다.

## 3. 동작

- 시작할 때 `needs_reindex_count > 0`, 타임라인 훑기 끝: 예전에는 곧바로 자동 재색인 → 이제 스케줄러에 요청(쉬는 시간 색인이 꺼져 있으면 예전과 같음).
- 쉬는 시간마다 30분 간격으로 변경분 다시 훑기(타임머신 본문·자료함 자료). 바뀌지 않은 문서는 `index_file`이 크기·시각 비교로 바로 건너뜀.
- 낮은 우선순위: 작업 스레드에 `THREAD_MODE_BACKGROUND_BEGIN`(CPU·디스크 I/O·메모리 우선순위 낮음). 쉬는 시간 자동 재색인은 예전부터 Python 색인기만 써서 klib 자식 프로세스를 띄우지 않는다. 자식 프로세스를 쓸 일이 생기면 `idle_gate.LOW_PRIORITY_CREATIONFLAGS`(IDLE_PRIORITY_CLASS)로 띄운다.
- 설정(`timemachine.json`): `idle_indexing`(기본 true), `idle_seconds`(기본 120, 15~3600). 잘못 저장된 값은 그 값만 기본으로(조사 폴더 설정을 잃지 않음).
- 자료함 자료: 처음 한 번은 기준만 잡는다(`library-idle.json`, 설정 폴더). klib를 이번 실행에서 쓴 적이 없으면 아무것도 하지 않는다.

## 4. 새 엔진(kfind 서비스) 연결 지점 — `E:/docfit-tmp/isearch/docs/APP1_KFIND_INTEGRATION_2026-11-13.md`

isearch 파일은 고치지 않았다. APP1 카드가 쉬는 시간 스케줄러를 쓰도록 연결할 곳:

| APP1 지점 | 이 브랜치에서 쓸 것 | 방법 |
|---|---|---|
| §3-6 처음 채우기(seeding), §7 APP1-F의 `phase` `kfind_seeding` | `IdleJob` 하나(`KfindSeedJob`, 이름 `kfind-seed`) | `pending()` = 연결 상태 `ready` + 코퍼스 `empty`/`seeding`. `run(ctx)`에서 문서마다 `ctx.checkpoint()` → `paragraphs_for_kfind(path)` → `KfindClient.ingest`, `ctx.progress(n, m)` |
| §4-3 `TimemachineService._kfind_sync`(색인 결과를 ingest 대기열로) | 대기열을 비우는 `IdleJob`(`kfind-sync`) | 사용자가 연 문서(수동 색인)는 곧바로 보내고, 자동 재색인에서 나온 것은 대기열 → 쉬는 시간에 보냄. `scheduler.request("kfind-sync")` |
| §3-3 `stale(scope, level_rule)`(위계 규칙 판이 바뀐 문서) | `kfind-seed`와 같은 작업, 주기 `every=1800` | `stale`가 돌려준 문서만 다시 보냄 |
| §4-3 `LibraryKfindSync.sync_box`·`NotebookKfindSync.sync_notebook` | `IdleJob`(`kfind-library`) | 상자·업무자료함 출처를 쉬는 시간에, 상자에서 뺀 것은 `remove` |
| §2-1 대체 모드 `kfind.exe`(사용자 권한) | `LOW_PRIORITY_CREATIONFLAGS` | 앱이 띄울 때 낮은 우선순위로. 서비스 모드에서는 서비스가 스스로 `PROCESS_MODE_BACKGROUND_BEGIN`을 걸거나, 앱이 `status`의 `queue`를 보고 쉬는 시간에만 `ingest`를 보내는 것으로 같은 효과 |
| §3-4 코퍼스 상태 화면 | `content_status()["idle"]["jobs"]["kfind-seed"]` | 상태 줄은 `idle_indexing_status.dart`를 같은 필드(trigger·processed·total·yielded)로 재사용 |

규칙: kfind 작업도 **스케줄러를 새로 만들지 않고** `shared_scheduler()`에 `register(job, every=…)`만 한다(유휴 판정이 한 곳). `IdleJob`은 `name`·`pending()`·`run(ctx)` 셋만 갖추면 된다(`typing.Protocol`, 상속 불필요).

## 5. 시험

- `tests/test_idle_gate.py`(15), `tests/test_idle_scheduler.py`(12), `tests/test_timemachine_idle_indexing.py`(8), `tests/test_library_idle_refresh.py`(7), `tests/test_idle_wiring.py`(1), `flutter_ui/test/idle_indexing_status_test.dart`(5).
- 시험은 가짜 신호(`Probe`)로 돌고, 이 PC 경로를 쓰지 않는다.

## 6. 남은 문제

- ~~업무자료함(노트북) 출처는 변경분 색인을 붙이지 못했다.~~ → NBIDX1(`docs/NBIDX1_NOTEBOOK_SEPARATE_INDEX_2026-11-13.md`): 별도 색인 + klib `nb_source_check`·`nb_source_refresh`(같은 id로 문단 교체) + 쉬는 시간 작업 `notebook-sources`.
- 한 파일이 여러 자료함에 들어 있으면 자료함 변경분 색인을 건너뛴다(파이프 `index_file`이 자료함 id 하나만 받음, `skipped_multi_box`). klib에 `box_ids`를 받는 명령이 생기면 풀 것.
- 실제 앱에서 사용자 입력·전체 화면·배터리 전이를 손으로 확인하지 않았다(시험은 가짜 신호). 부하 기준 숫자(25%/60%, 8/32MB/s)는 측정 근거 없이 정한 값이다.
- 상태 줄은 타임머신 화면에만 붙였다. 앱 전체 상태 줄·자료함 화면에는 아직 없다(`content_status()["idle"]["jobs"]`로 읽을 수 있음).
