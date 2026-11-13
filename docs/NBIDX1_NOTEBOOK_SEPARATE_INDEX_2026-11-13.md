# NBIDX1 업무자료함 색인 분리 (2026-11-13)

사용자 결정(2026-11-13): "업무자료함은 별도의 색인으로 분리한다." 브랜치 `feature/notebook-separate-index`.
원칙: COM 외 Rust 우선 — 색인·옮기기·다시 색인은 klib(Rust) 안에서 한 덩어리로, Python은 호출만.

## 1. 무엇이 바뀌었나

| 전 | 후 |
|---|---|
| 업무자료함 표 6개(`notebook`·`nb_source`·`nb_paragraph`·`nb_note`·`nb_citation`·`nb_note_tag`)가 자료함 DB `library\library.db` 안 | 별도 DB `library\notebook\notebook.db`. klib 연결에 `nb`로 붙여(ATTACH) 기존 nb_* SQL은 표 이름 그대로 별도 파일을 읽고 씀 |
| 업무자료함 문단이 가상 폴더·자료함 공용 Tantivy 색인(`library\tantivy`, `ext=nb`)에 섞임 | 별도 색인 `library\notebook\tantivy`(같은 스키마·토크나이저, writer 1스레드). 공용 색인의 `ext=nb` 문단은 옮긴 뒤 한 번 지움 |
| 출처 내용이 바뀌면 지우고 다시 넣어야 함(출처 번호가 바뀌어 메모·인용 연결이 끊김) | `nb_source_refresh`: 같은 출처 번호로 문단만 교체, 인용 위치를 새 문단에 다시 맞춤 |
| 쉬는 시간 색인(IDLE1)에 업무자료함 없음(IDLE1 남은 문제 1) | 쉬는 시간 작업 `notebook-sources`(30분 간격): `nb_source_check` → 바뀐 출처만 `nb_source_refresh` |
| 공용 색인 다시 만들기(`reindex::rebuild`)가 업무자료함 출처도 넣음 | 자료함 항목만. 업무자료함 색인은 별도 저장소가 열 때 DB로 확인·재구성 |

klib 파일: `nb_store.rs`(새: 별도 저장소 열기·옮기기 상태 기계·색인 재구성), `nb_split_check.rs`(새: 비교 도구), `notebook.rs`(출처 상태 기계·다시 색인·원본 변경 검사), `db.rs`(`Database`가 별도 저장소를 가짐), `pipe.rs`(nb_* 명령을 별도 색인으로, 새 명령 4개), `search.rs`(writer 스레드 수, 문서 수·비우기·`ext=nb` 지우기), `reindex.rs`.
Python: `notebook_service.py`(`refresh_source`), `notebook_idle_refresh.py`(새), `library_bridge.py`(허용 명령), `desktop_web_ui.py`(작업 등록), `idle_gate.py`(디스크 부하 Win32). Flutter: 출처 상태 문구·다시 색인 단추.

## 2. 상태 기계

### 2-1. 옮기기(`nb_store::MigrationState`, 기록 `library\notebook\migration.json`, 명령 `nb_status`)

```
not_started ─옛 자료 없음─▶ not_needed
not_started | failed ─▶ backing_up ─▶ copying ─▶ indexing ─▶ verifying ─▶ switching ─▶ done
                        (어느 단계든 오류) ──────────────────────────────────────────▶ failed
```

| 단계 | 하는 일 | 실패하면 |
|---|---|---|
| backing_up | `VACUUM INTO`로 자료함 DB 전체를 `notebook\backup\library-before-notebook-split-<ms>.db`에 백업 | 만든 파일만 지움 |
| copying | `notebook.db.partial`에 표 6개를 번호 그대로 복사(외래키 끄고 있는 그대로, 옛 DB에 없는 열은 기본값), AUTOINCREMENT 다음 번호도 복사, 표마다 외래키 위반 수가 옛 DB와 같은지 | `.partial` 지움 |
| indexing | `tantivy.partial`에 새 DB 문단을 넣음 | `.partial` 지움 |
| verifying | 표마다 행 수 같음, 색인 문단 수 = DB 문단 수, `integrity_check` = ok | `.partial` 지움 |
| switching | 색인 폴더 → DB 파일 순으로 이름 바꿈. klib 연결에 붙인 뒤 자료함 DB의 옛 표를 한 트랜잭션으로 `nbidx_legacy_<ms>_<표>`로 이름만 바꿔 둠(지우지 않음) | 새 파일 지움, 자료함 DB 그대로 |
| done | `migration.json` `state=done`, 다음 요청 때 공용 색인의 `ext=nb` 문단 지움(`nb_meta.shared_index_cleaned`) | — |

- 실패해도 자료함 DB 파일은 한 바이트도 바뀌지 않는다(시험 `failed_move_leaves_library_untouched_and_retry_succeeds`가 파일 내용을 견줌). 다음 klib 실행이나 `nb_migrate`가 다시 시도한다(`attempts` 증가).
- 이름 바꾸기 직후 멈추면(새 DB의 `nb_meta.migration_state=switching`, 자료함 DB에 옛 표가 남음) 다음에 열 때 옛 표 떼어 두기를 마저 한다.
- 두 klib 프로세스가 동시에 옮기지 않도록 `notebook\migration.lock`(공유 금지로 연 파일, 프로세스가 죽으면 OS가 품).
- 옮기지 못한 동안 업무자료함 명령은 `code=NB_UNAVAILABLE`로 거절(가상 폴더는 정상). `nb_status`·`nb_migrate`만 받는다.
- 불가능한 전이: 단계 건너뛰기, done·not_needed에서 다시 옮기기, failed → done.

### 2-2. 출처 색인 상태(`notebook::SourceStatus`, `nb_source.status`)

```
pending ─추가 완료─▶ ready ─원본 바뀜(nb_source_check)─▶ stale ─다시 색인 시작─▶ refreshing ─완료─▶ ready
                       │  ▲                              │                         └─실패─▶ failed ─다시 색인─▶ refreshing
                       └──┴─원본 없어짐─▶ missing ─원본 다시 보임─▶ stale | ready(크기·시각 같음)
```
- 불가능한 전이: pending→stale(아직 색인 전), refreshing→stale·missing(다시 색인 중에는 검사가 덮지 않음), ready→failed(실패는 다시 색인에서만).
- 쉬는 시간 작업이 다시 색인하는 대상: stale·failed. missing은 원본이 다시 보일 때까지 문단을 그대로 둔다.
- 기준값: `nb_source.original_size`·`original_mtime`(ms). 둘 다 0인 옛 출처는 처음 검사 때 기준만 잡고 다시 색인하지 않는다.
- 원본 경로: 자료함 항목과 연결된 출처(`item_id`)는 항목의 지금 사본 경로, 아니면 `origin`.

## 3. 명령(klib 파이프, Python `NOTEBOOK_COMMANDS`·`ALLOWED_COMMANDS`)

| 명령 | 요청 | 응답 |
|---|---|---|
| `nb_status` | — | `ready`, `error`, `migration`(위 기록), `db_path`, `index_path`, `index_paragraphs`, `sources`(상태별 수) |
| `nb_migrate` | — | 옮기기·열기를 다시 시도한 뒤 `nb_status`와 같음 |
| `nb_source_check` | `notebook_id`(없으면 전체) | `checked`, `counts`(상태별), `refresh`(stale·failed 목록), `sources`(전체) — 행마다 `source_id`·`notebook_id`·`status`·`path`·`kind`·`needs_paragraphs` |
| `nb_source_refresh` | `notebook_id`, `source_id`, (Python 추출이면) `paragraphs`, `part`(0부터), `last`(기본 true) | 끝나면 `paragraph_count`·`citations`·`kept`·`relinked`·`unresolved`·`status=ready`. 중간 묶음은 `staged`·`status=refreshing`. hwp·pdf를 문단 없이 부르면 `code=NEEDS_PARAGRAPHS`+`path`, 원본이 없으면 `code=MISSING` |

- 다시 색인은 준비 표(`nb_refresh`·`nb_refresh_paragraph`)에 모은 뒤 마지막 묶음에서 한 트랜잭션으로 문단을 바꾼다. 그 전까지 검색은 옛 문단으로 된다. 실패하면 옛 문단을 그대로 두고 `failed`(이유).
- 인용 다시 맞추기: 같은 번호 문단에 인용 글이 있으면 그대로(`kept`), 없으면 인용 글이 든 가장 가까운 문단으로(`relinked`), 어디에도 없으면 번호를 그대로 둔다(`unresolved`, 노트에 인용 글 사본은 남음). 공백 차이는 무시.
- DB는 바꿨는데 색인 쓰기가 실패하면 `nb_meta.index_dirty`를 남기고, 다음에 열 때 DB로 색인을 다시 채운다(색인 문단 수가 DB와 달라도 다시 채움).
- 화면: 출처 목록에 상태 문구(원본이 바뀜 · 다시 색인 대기 / 다시 색인 중 / 원본 파일 없음 / 다시 색인 실패: 이유)와 "다시 색인" 단추(`nb-source-refresh-<id>`).

## 4. 쉬는 시간 연결(IDLE1)

- 작업 `notebook-sources`(`docfit_app/notebook_idle_refresh.py`, 30분 간격)를 klib 브리지를 처음 만들 때 공용 스케줄러에 한 번 등록(`DesktopWebBridge._register_notebook_idle_job`). 자료함 작업 등록(`_idle_scheduler`)은 그대로.
- `run(ctx)`: `nb_source_check` → 대상마다 `ctx.checkpoint()` → `NotebookService.refresh_source`(화면과 같은 서비스). 결과 `last_result`: `checked`·`changed`·`refreshed`·`errors`·`missing`·`relinked`·`unresolved`·`unavailable`.
- 상태 파일이 따로 없다. 기준과 상태는 업무자료함 DB에 있어, 멈추거나 실패한 출처는 다음 쉬는 시간에 다시 한다.
- kfind(APP1) 연결 지점: `NotebookKfindSync.sync_notebook`(APP1 §4-3, `scopes nb:<id>:src:<sid>`)은 출처 번호가 바뀌지 않으므로 다시 색인 뒤에도 같은 문서 키로 `ingest`하면 된다. `notebook-sources` 작업이 `refresh_source` 성공 때 kfind 대기열에 넣는 것이 가장 가까운 자리다(isearch 파일은 고치지 않음).

## 5. 디스크 부하 판정(psutil)

- 조사: `psutil`은 `constraints.txt`에만(7.2.2) 있고 `requirements.txt`에는 없다. `build.bat`은 `pip install -r requirements.txt pyinstaller`(constraints 미적용)라 새로 만든 빌드 환경에는 psutil이 깔리지 않고, PyInstaller는 깔린 것만 묶는다. 지금 개발 .venv와 `C:\HWP_AUTODOCFIT\venv315`에는 깔려 있어 이 PC 빌드에는 들어갈 수 있지만 보장되지 않는다.
- 결정: requirements에 넣지 않고 Win32 API로 바꿨다(의존성 없이 항상 판정). 전체 디스크 = `\\.\PhysicalDriveN`(0~31)을 접근 권한 0으로 열어 `IOCTL_DISK_PERFORMANCE`의 읽기+쓰기 바이트 합(관리자 권한 불필요, psutil과 같은 방법), 이 앱 몫 = `GetProcessIoCounters`. psutil은 시험에서 넘겨 주거나 Win32 측정이 실패할 때만 쓴다.
- 확인(이 PC): Win32 516,084,396,032 바이트 / psutil 516,084,391,424 바이트(두 측정 사이 차이), 자기 프로세스 1,463,856 = 1,463,856.

## 6. 비교 도구와 결과 — `klib nb-split-compare <자료함 폴더 사본>`

사본 폴더에서만 돌린다(그 사본을 옮긴다). 옮기기 전(옛 표 + 공용 색인)과 옮긴 뒤(별도 DB + 별도 색인)를 같은 질의로 견준다. 질의: 노트북마다 문단 낱말(2글자 이상) 최대 300개 + 이웃 낱말 쌍 최대 100개 + 출처 이름. 검색은 `nb_search`와 같은 전처리(`query_terms`).

사용자 실제 자료 사본(`%LOCALAPPDATA%\HwpAutoDocFit\library`의 library.db·tantivy를 G:/docfit-tmp로 복사, 노트북 3·출처 4·문단 94·노트 2·인용 3·태그 1) — 디버그 빌드, 새 사본으로 4회 돌려 모두 같은 값:

| 항목 | 결과 |
|---|---|
| 옮기기 | done, 시도 1회, 표 6개 행 수 전후 같음, 색인 문단 94 |
| 표 전체 행 비교(107행, 옛 표의 열 그대로) | **차이 0** |
| 개수 제한 없이 찾은 문단 집합(404 질의, 4,170건) | **차이 0** |
| 상위 20건(노트북 전체)·출처별 5건 검색, 2,020 질의 | 35건 다름: 경계 문단이 바뀐 것 13, 순서만 다른 것 22(그중 같은 점수끼리의 순서만 3) |
| 점수 최대 차이 | 2.19 |
| 옮기기 시간(디버그 빌드, 단계별 ms) | 1.9~2.4초(백업 123·복사 900·색인 996·확인 66·이름 바꾸기 143). 다른 세션 빌드가 겹친 1회는 15초 |

- 순위가 달라지는 까닭: BM25 점수는 색인 전체의 통계(문서 수·낱말이 든 문서 수·평균 길이)로 계산한다. 옛 공용 색인에는 가상 폴더 문서가 섞여 있어, 같은 노트북 안의 순위가 가상 폴더에 무엇이 들었는지에 따라 움직였다. 분리 뒤에는 업무자료함 자료만으로 정해진다(가상 폴더를 바꿔도 업무자료함 순위가 그대로). 찾는 문단은 하나도 잃지 않았다(제한 없는 집합 차이 0).
- 처음 측정에서는 실행마다 102건·32건으로 달랐다: 새 색인을 여러 스레드로 쓰면 같은 점수 문단의 순서가 실행마다 바뀌었다. 업무자료함 색인 writer를 1스레드로 바꿔(넣은 순서 = 출처·문단 순) 결정적으로 만들었다.
- **"상위 N건 순서까지 차이 0"은 이루지 못했다(사용자 결정 필요, §7-1).**

## 7. 남은 문제

1. **검색 순위 차이 0 미달**: 상위 N건 2,020 질의 중 35건. 선택지 — (가) 지금대로: 업무자료함 통계만으로 순위(가상 폴더와 무관, 결정적). (나) 옮길 때 공용 색인의 가상 폴더 몫 통계(전체 문서 수·필드 토큰 수·업무자료함 낱말별 문서 수)를 `nb_meta`에 얼려 두고 tantivy `search_with_statistics_provider`로 더해 옛 순위를 그대로 재현(옮긴 순간 차이 0 기대, 이후 가상 폴더 변화는 반영 안 함). (나)는 만들지 않았다.
2. 다시 색인 뒤 같은 점수끼리의 순서: 다시 색인한 출처의 문단이 색인 끝에 붙어, 같은 점수일 때 다른 출처보다 뒤로 간다.
3. failed 출처는 원본이 그대로여도 쉬는 시간마다(30분) 다시 시도한다(영구히 못 읽는 파일이면 헛수고 반복).
4. 옛 표는 자료함 DB 안에 `nbidx_legacy_<ms>_*`로 남는다(백업과 별개로 두 벌). 지우는 시점은 정하지 않았다. 옛 버전 앱으로 되돌리면 업무자료함이 빈 것으로 보인다(자료는 notebook.db·옛 표·백업에 있음).
5. 실제 앱 화면(klib.exe 릴리스 빌드)에서 손으로 옮기기·다시 색인을 확인하지 않았다. 시험은 klib 단위·통합 시험과 Python 가짜 브리지, Flutter 가짜 API로 했다.
6. 한 klib 연결이 두 DB에 걸친 트랜잭션(자료함 항목 이동 + 출처 경로 갱신)은 WAL에서 DB별로만 원자적이다. 출처 경로가 어긋나도 다시 색인은 연결된 항목의 지금 경로를 먼저 쓴다.
