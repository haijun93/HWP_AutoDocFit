# TM1 조사: 타임머신을 단축키로 부르는 독립 앱 (2026-11-12)

계획은 `docs/TM1_PLAN_2026-11-12.md`. 기준 커밋 `954fd16f`, 브랜치 `feature/timemachine-popup`. 이 문서는 조사만 담는다(코드 변경 없음).
표기: **확인** = 이번에 직접 읽은 것, **추정** = 계산·다른 문서 수치에서 낸 값, **미확인** = 읽지 않았거나 실기로만 알 수 있는 것.

## 1. 무엇을·왜

사용자 지시(2026-11-12): 타임머신도 몽땅클립보드처럼 **단축키로 부르는 독립 앱**으로. UI는 몽땅클립보드와 같은 방식(경량·최소·램 상주). Search Everything·Docufinder보다 **검색 속도와 기능이 우수**하고, 앱 본체 없이 **스스로 자동으로 동작**할 것.

지금 타임머신은 앱 본체(Python + Flutter) 안 화면이다. 찾으려면 앱을 켜고 화면을 바꿔야 하며, 검색 한 번이 Flutter → Python 루프백 → (SQLite·Python 계산) 왕복을 거친다. 본문 첫 질의는 3,000문서 2낱말 5.0~6.3초다(`IS_S0_PROFILE`, IS1 계획 §3 인용, 확인). 독립 앱은 이 왕복을 없애고, 문서를 쓰다가 단축키 한 번으로 "지난주에 만든 그 문서"·"예산 집행이 들어간 문장"을 찾게 하는 것이 목적이다.

## 2. 이미 확정된 결정(이 과제에 그대로 적용)

| 출처 | 결정 | TM1에 주는 뜻 |
|---|---|---|
| 운영 규칙 0절 | 암묵적 상태 기계를 드러내고 UX·DX·AX 셋 다 올리며 아무것도 깨지 않는다 | 창·프로세스·색인 상태를 명시적 값과 상태 파일로 |
| 운영 규칙 7절 | 큰 기능은 조사·계획 먼저, 방향이 정해진 과제는 승인 대기 없이 진행, 사용자 영향 선택만 Q | 이 두 문서 |
| VF1B(사용자) | 가상 폴더 창은 **별도 실행 파일 `kvf.exe`** | 같은 방식의 선례. 공용 파일 `#[path]` 방식(P2), 단일 실행, 자동 실행 Run 값, `--install`·`--enable`·`--upgrade-if-stale`·`--disable` |
| IS1 Q1 | 관리자 권한 사용 가능, MFT 직접 읽기 + USN | 드라이브 전체 파일명은 **SYSTEM 서비스 `kfind`만** 관리자(IS1 계획 §2.2 권고 B) |
| IS1 Q2 | 파일명 범위 = 로컬 NTFS 드라이브 전체 | 타임머신 창의 파일명 찾기도 드라이브 전체 |
| IS1 Q3·Q4 | 여러 낱말은 **같은 문장 안**에서만 일치, 그 안에서 위계 순 | 본문·위계검색 규칙 그대로 |
| IS1 Q5 + 추가 결정 | 붙여쓰기·띄어쓰기 모두. "예산집행"도 같은 문장에 "예산"·"집행"이 있으면 찾고 붙어 있는 일치를 앞에 | 같은 규칙 |
| IS1 Q6·Q8 | 정규식 1차 제외, 본문은 2글자부터(1글자는 파일명만) | 같은 규칙 |
| IS1 Q7 | 파일명 색인·본문 실시간 계산은 Rust | 창 앱도 Rust(Python 없음) |
| IS 엔진 결정 | 본문 엔진 **B**(문장 비트맵 roaring + 조각 사전 fst), 공용 `kfind-core`를 klib·kfind·kvf가 같이 씀 | 타임머신 창도 `kfind-core` 사용자. 엔진을 새로 만들지 않는다 |
| IS 라이선스 | 허용 MIT·Apache·Unicode-3.0, 색인·검색 양쪽 NFC, 규칙 버전으로 자동 재색인 | 같은 규칙 |
| IS-QCACHE | 검색 기록 활용: 낱말별 일치 문장 비트맵 캐시, 바뀐 파일만 다시 판정 | 창 앱의 질의 계층에도 적용(엔진이 이미 목표 안이면 비트맵 캐시만) |
| IS 동시 교체 | 타임머신·가상 폴더·업무자료함 검색을 새 엔진으로 동시에 교체, 예전 경로는 설정으로 한 버전 남김 | 앱 안 타임머신 화면도 같은 엔진·같은 색인을 읽게 된다 → 창 앱과 결과가 같아야 한다 |
| ROBUST-REQ | ① 추출·색인은 별도 작업 프로세스(RSS·시간·건수 상한) ② watchdog과 작업 시간 한도 ③ `panic="unwind"` + `catch_unwind` | 각 단계 완료 조건 |
| IS-DOCUFINDER-TARGET | Docufinder보다 빠른 속도·많은 기능, 같은 자료로 둘 다 잰다(Docufinder 실행은 사용자 승인 후) | 비교 측정 방법(계획 (라)) |
| 사용자 지시(2026-11-12, `de605ac2`) | '추출 실패 N건' 표시 삭제(글 없는 파일을 오류로 오해) | 창 앱에도 실패 건수를 사용자 화면에 띄우지 않는다(상태 파일에만) |

## 3. 선행 사례

### 3.1 몽땅클립보드 창(kagent, 확인: `kagent/src/popup.rs:1-22` 머리 주석)
- 단축키 → 마우스 옆에 **메모리 목록**을 바로 그림(DB 읽기 없음). 숫자 1~9·↑↓·Enter, Esc·바깥 클릭은 닫기.
- 상단 메뉴바(제목 + 화면캡처·항상 위·최소화·최대화/복원·닫기, 46px 캡션 단추, `WM_MOUSEACTIVATE → MA_NOACTIVATE`로 포커스 유지), 분류 메뉴바(←→·Tab), 글자를 치면 키워드검색으로 전환, 한글 조합 중 글자 포함 찾기.
- 최소화 = 숨기되 분류·찾기·선택 유지. 틀은 `WM_NCCALCSIZE`로 없애고 DWM 테두리 한 겹, 가장자리 6px 크기 조절(`frame_hit`).
- 자원(VF1 계획 V0 실측, 확인): 이 PC 설치본 개인 메모리 4.70MB(약 4시간 40분 켜진 상태), 유휴 60초 CPU 0ms. `docs/KAGENT_DESIGN.md`의 1.8~1.95MB는 다른 조건의 값(VF1 계획 인용).
- 암묵적 상태: `minimized`·`sticky_popup`·`popup_collapsed`·`restore_rect` 플래그와 `IsWindowVisible`을 섞어 읽음(VF1 계획 (다) 인용). TM1은 이 방식을 따르지 않고 kvf처럼 상태 값 + 순수 전이 함수로 간다.

### 3.2 가상 폴더 창(kvf, 확인: `E:/docfit-tmp/vfp/docs/VF1_PLAN_2026-11-12.md`)
- 별도 exe, `windows-sys`만, 공용 파일 7개(`sqlite.rs`·`json.rs`·`util.rs`·`lifecycle.rs`·`frame.rs`·`hotkey.rs`·`open_rules.rs`)를 kagent에서 `#[path]`로 컴파일(P2). 뮤텍스·창 클래스·Run 값·자료 폴더·상태 파일 모두 따로.
- 상태 두 층(`Proc` 수명, `View` 창). `step` 순수 함수 한 곳에서만 상태 대입. 늦은 결과는 `seq`로 버림.
- DB는 `winsqlite3.dll` **읽기 전용**으로 부를 때만 열고 닫음(C1: WAL DB 읽기 전용 열기 통과, 단 `-shm`·빈 `-wal`이 생김).
- 실측(V8, 확인): GNU release `kvf.exe` 479,744바이트, 숨김 상주 개인 메모리 **1.548MB**, 유휴 60초 CPU 0ms. 시험 141/141.
- 남은 일: 실기 R1~R17 대부분 미확인, 견고성(`panic="abort"` → unwind 전환)은 후속 조건. `main` 미병합(이 체크아웃에 `kvf/` 없음, 확인).
- V11(다음 범위): klib.exe를 kvf가 찾을 수 있는 곳에 설치 — **TM1도 같은 문제**를 가진다(5절 C5).

### 3.3 실시간 검색(IS1, 확인: `E:/docfit-tmp/isearch/docs/IS1_PLAN_2026-11-12.md`, `IS_S1_ENGINE_COMPARE`, `DFB2_COMPARE`)
- `kfind.exe` = Windows 서비스(SYSTEM), MFT 열거 + USN 막힘 읽기, 파이프 명령 `query`·`cancel`·`status`·`path_of` 읽기 전용, `PIPE_REJECT_REMOTE_CLIENTS`, 설치 때 UAC 1번, Program Files로 복사 후 등록. 메모리 상한 100만 항목 80MB / 200만 160MB(추정). 크기·날짜는 1차에 저장 안 함(Q11).
- 본문 엔진 B 실측(백엔드, 합성 자료): 3,000문서(12만 문장) "예산 집행" 상위20 p95 **0.67ms**, 3만 문서(120만 문장) 전체 p95 최악 **12.65ms**("예산"). 글자 하나 입력 단계별 최악 p95 28.91ms(3만). 디스크 3k 27.1MB / 30k 269.4MB. 질의 중 개인 메모리 2.4MB / 13.8MB, 작업 집합 최대 30.6MB(mmap 포함). 색인 열기 1.8 / 6.5ms.
- B의 대가: 증분 갱신·삭제·병합·충돌 안전을 직접 만들어야 함(시제품은 한 번에 새로 만들기만). 합성 자료라 위계·날짜 순위는 사실상 시험 안 됨.
- 화면 상태 기계(IS1 §2.7): `IndexAvailability`(driveIndexOff/Installing/Building/Ready/Rebuilding/Unavailable, sentenceIndexBuilding/Ready)와 통로별 `SearchSession`(idle/tooShort/inFlight/partial/complete/failed). TM1 창이 그대로 가져다 쓴다.
- IS1 단계는 S0·S1(엔진 비교)까지 진행, kfind 서비스(S2·S3)·문장 색인 증분(S5)·질의(S6)는 아직(확인: `feature/instant-search` 최근 커밋 `51ae7da1`).

### 3.4 Everything · Docufinder (확인: IS-BENCH, IS-DOCUFINDER-TARGET, DFB1, DFB2 보고서)
| | Everything | Docufinder v3.8.14 |
|---|---|---|
| 방식 | MFT 직접 + USN, 관리자(서비스) | Rust+Tauri, SQLite FTS5 + Lindera 형태소, 선택적 임베딩, 파일명 메모리 캐시, notify 감시 |
| 본문 | 없음(파일명 위주) | 있음(HWP/HWPX·PDF·Office·OCR) |
| 공개·실측 수치 | 100만 항목 약 100MB, 첫 열거 100만 약 1분(IS1 조사 인용) | DFB1: 3,000 txt/md 색인 149~161초, 본체 피크 482MiB(+WebView 968MiB), 화면 관찰 상한 0.67~6.43초, 앱 자체 표시 0.07~0.21초 |
| 라이선스 | 폐쇄 소스, 설계만 참고 | BUSL 1.1, 코드 가져오기 금지 |
- DFB2 주의(확인): Docufinder 수치는 **화면 관찰 상한**, 우리 수치는 **엔진 내부**라 직접 비교 금지. 공정 비교는 같은 측정기로 양쪽 화면을 재야 한다 → 계획 (라)의 측정 방법.
- Everything과의 공정 비교는 아직 한 번도 하지 않았다(미측정).

## 4. 지금 타임머신 기능(확인한 범위)

### 4.1 구성과 데이터

| 부분 | 위치 | 내용 |
|---|---|---|
| 앱 측 서비스 | `docfit_app/timemachine_service.py`(930줄) | 명령 분기 `_command`(`:599-790`). 설정은 앱 설정 폴더의 `timemachine.json`(`:38`, 폴더는 `desktop_web_ui.py:2116` `_settings_store()._path.parent` — 실제 경로 미확인) |
| 타임라인 저장소 | `docfit_core/timemachine/store.py` | SQLite `timeline.db`(kagent 자료 폴더, `timemachine_service.py:39`). 표 `files(id,key,path,root,name,kind,size,mtime,ctime,work_time,flags,cloud,hidden,seen)`, 색인 `work_time`·`root`(`store.py:10-32`). 이름 질의는 `LIKE`(`_filters`, `:163`) |
| 타임라인 쓰는 쪽 | kagent `kagent/src/timeline.rs`(474줄, 같은 스키마, "쓰기는 에이전트 한 곳에서만" 머리 주석) + `watch.rs`(266줄, `ReadDirectoryChangesW` + 완료 포트, 400ms 모아 넘김, 넘치면 그 폴더 다시 훑기). 에이전트가 안 맡으면 앱이 `scan_roots`로 직접 훑음(`timemachine_service.py:487-568`) | **쓰는 쪽이 둘**(kagent 또는 앱) — `_sync_agent`가 `settings_get`으로 비교해 정함. 둘 다 꺼져 있으면 타임라인은 갱신되지 않는다 |
| 본문 색인 | `docfit_core/timemachine/content_search.py`(2,200줄) `ContentSearchIndex` | SQLite `timeline-content.sqlite`(`timemachine_service.py:41`). 쓰기는 klib.exe `index-content`(Rust 추출, `klib/src/main.rs:119`, `extract.rs` 705줄) 또는 Python 대체 |
| 본문검색 | `keyword_search`(`content_search.py:1742`) | 연산자 `parse_query`(`:963`): `ext:`·`path:`·`after:`/`before:`·`-제외`·`"구문"`·`~N` |
| 위계검색 | `search`(`:1277`), 위계 정렬 `_sort_hierarchy_results`(`:159`), 경로 `hierarchy_path.py` | |
| 파일명 찾기 | `search_files`(`:2064`) — 본문 색인된 문서만 | |
| 연관 문서 | `related.py`(154줄): 낱말·부서·사업·분기 추정 + `series.py` 정기 문서 계열 + 사용자의 '관련 없음'·'관련 있음' 표(`store.py` `related_exclusions`·`related_links`) | Python 규칙 |
| 썸네일 | `thumbs.py`(427줄): 형식별 160px JPEG, 디스크 캐시, 클라우드 전용 파일은 열지 않음, 시간 한도 5초·동시 2개(`timemachine_service.py:25-27`) | |
| 보내기 | `send_to_feature`(`rename_batch`·`convert`·`document_edit`) | 앱 기능 대기열로 |
| 화면 | `flutter_ui/lib/features/timemachine/timemachine_screen.dart`(1,882줄), `timemachine_api.dart`, `related_documents_dialog.dart`, `timemachine_thumbnails.dart` | 달력·시간대 밀도·파일명 표 보기·본문/위계·미리보기 안 찾기·연관 문서 대화상자 |

### 4.2 화면의 암묵적 상태(확인: `timemachine_screen.dart:50-100`)
`_contentMode`·`_relatedMode`·`_fileTableView`·`_fileSearchActive`·`_fileSearchSupported`·`_contentIndexRunning`·`_contentNeedsReindex`·`_contentReindexDismissed`·`_loading`·`_more`·`_error`와 revision 번호 다섯 개(`_previewRevision`·`_contentSearchRevision`·`_fileSearchRevision`·`_filterRevision`·`_calendarRevision`), `_requestSerial`, 타이머 셋(`_debounce`·`_contentPoll`·`_developerModePoll` 1초 주기). "지금 무슨 화면·무슨 검색 중인지"가 불 조합으로만 드러난다. 개발자 모드 확인이 1초 주기 폴링이다(유휴 CPU 0 원칙과 다름 — 앱 화면 일이라 TM1 범위 밖, 기록만).

### 4.3 서비스의 암묵적 상태(확인)
- 타임라인 쓰는 쪽: `agent`/`app` 판정이 `_watch_mode`(`:467`)와 `_sync_agent`(`:487`) 두 곳.
- 훑기: `_scan_state["running"]` + `_rescan` 플래그(`:550-553`, `:538`).
- 본문 색인: `_content_state`의 `running`·`phase`(`idle`·`reindexing`·`failed` 등) + `_content_thread`·`_content_process`·`_content_cancel`(`:56-64`, `:570-594`).
- 이 셋은 앱 안 상태라 창 앱이 볼 수 없다. 창 앱이 "색인이 최신인지"를 알려면 **디스크에 상태가 있어야** 한다(계획 (가)의 색인 상태 파일).

## 5. 확인 필요(실기·다음 단계에서)

| 번호 | 확인할 것 | 왜 | 언제 |
|---|---|---|---|
| C1 | kagent가 `timeline.db`에 쓰는 동안 다른 프로세스의 읽기 전용 열기 | kvf C1(`library.db`)은 통과했지만 `timeline.db`는 저널 방식이 미확인(kagent `Timeline::open`의 `PRAGMA` 미확인) | T0 |
| C2 | `timemachine.json`의 실제 경로·형식, 앱이 꺼져 있을 때 창 앱이 조사 폴더를 알 방법 | 창 앱의 조사 폴더·확장자 거르기 | T0 |
| C3 | 단축키 후보가 비어 있는지(`Ctrl+Alt+Space`·`Win+Alt+T`·`Ctrl+Shift+Space`) | 전역 단축키 충돌 | T0 |
| C4 | kfind 서비스(IS1 S2·S3)와 `kfind-core` 증분 색인(S5)의 완성 시점·API | TM1의 파일명·본문 단계가 여기에 걸림 | IS1 진행 따라 |
| C5 | klib.exe 크기·시작 시간, 앱 꾸러미 밖(사용자 자료 폴더)에 설치해도 되는지 | 앱이 꺼져 있을 때 창 앱이 본문 색인을 갱신하려면 추출기가 필요(kvf V11과 같은 문제) | T0 |
| C6 | 한/글 문서(HWP) 추출이 klib Rust 경로만으로 되는지, COM이 필요한 형식이 있는지 | 창 앱은 COM을 쓰지 않는다. COM이 필요한 문서는 앱이 켜졌을 때만 색인 | T0 |
| C7 | 현재 앱 안 타임머신의 실제 화면 지연(글자→결과) 기준선 | 개선 폭 보고 | T0 |
| C8 | Everything 설치·측정 허락, Docufinder 재실행 허락(BUSL 개발·시험 범위) | 비교 측정 | T11 전 |
| C9 | 구청 PC 보안 프로그램이 서명 없는 새 상주 exe·전역 단축키·서비스를 막는지 | kagent·kvf와 같은 위험 | 실기 |

## 6. 읽지 않은 것(미확인)
- `kagent/src/timeline.rs`·`watch.rs`의 머리 주석·`use` 줄만 읽었다(`crate::sqlite`·`crate::util`만 씀 — `#[path]` 공용 가능성 있음). 본문은 미확인.
- `popup.rs`의 그리기·입력 본문, `klib/src/extract.rs`·`pipe.rs`·`search.rs` 본문, `content_search.py`의 `search`·`keyword_search` 본문, `related.py`·`series.py` 본문, Flutter 화면 대부분.
- IS1 S2 이후 kfind 서비스 코드(아직 없음), `kfind-core` API(아직 없음).
