# APP1 앱 검색을 kfind-core로 바꾸는 연결부 설계 (2026-11-13)

작업 폴더 `E:/docfit-tmp/isearch`, 브랜치 `feature/instant-search`(기준 `f2355aca`, KB4 위). **문서만 썼고 코드는 고치지 않았다.**

읽은 것: `docs/TEAM_OPERATING_RULES.md`, 카드 `IS1.md`·`KB2~KB4.md`·`T4.md`, 보고 `KB2~KB4.final.md`, `docs/IS1_PLAN_2026-11-12.md`, `docs/KC1_KFIND_CORE_2026-11-12.md`, `E:/docfit-tmp/tmp/docs/TM1_PLAN_2026-11-12.md`(§0·§1·§7·§9·§11), `kfind/core/src`(lib·index·query·writer), `docfit_core/timemachine/content_search.py`, `docfit_app/timemachine_service.py`, `docfit_app/library_bridge.py`, `docfit_app/application_service.py`, `klib/src/search.rs`·`pipe.rs`(search·item_search·nb_search 부분), `feature/instant-search-ui` 브랜치의 `flutter_ui/lib/core/instant_search.dart`·`library_document_search.dart`·ISUI2 커밋 `840df579`, `feature/vfolder-popup` 브랜치의 `kvf/src/vf_data.rs`(함수 목록만).

경로 표기: 카드의 `crates/kfind-core`는 이 작업 폴더에서 **`kfind/core`** 이다. `flutter_ui/lib/core/instant_search.dart`는 이 브랜치에 없고 `feature/instant-search-ui`(ISUI1~3, `9f6193c4`)에만 있다. kvf 코드는 `feature/vfolder-popup`에만 있다.

읽지 않은 것(미확인으로 표시): klib `classify_levels.rs`·`index_content.rs` 본문, `docfit_core/library/store.py`, kfind 서비스 크레이트(T4 진행 중, 커밋 없음 — `E:/docfit-tmp/t4` 로그 확인), MFT 파일명 색인(IS1 S1·S2) 구현 여부, Flutter 타임머신 화면 전체.

---

## 0. 요약

- 지금 앱의 본문·위계 검색은 **엔진이 셋**이다: 타임머신 = Python SQLite FTS5(trigram), 가상 폴더·업무자료함 = klib Tantivy(파이프). 가상 폴더 화면의 본문·위계 메뉴(ISUI2)는 타임머신 명령을 빌려 쓰고 Dart에서 경로로 거른다. kvf는 이름 거르기만 한다.
- 세 화면 모두 **Python을 지난다**(Flutter → Python IPC → SQLite 또는 `library_bridge` → klib 파이프). 그래서 교체 지점은 Python 함수 3곳(`content_search`·`related_search(query)`·`library_bridge.request`의 `search`/`item_search`/`nb_search`)으로 모인다.
- 호출: 앱 Python → `\\.\pipe\HwpAutoDocFit.Index`(kfind 서비스, T4) `query`/`count`/`cancel`/`status`. TM1 Q17 확정대로 앱은 매핑을 직접 읽지 않는다.
- 위계: **Python 규칙(`classify_paragraph_levels`, 위계 규칙 버전 4)이 정한 문단 위계를 색인할 때 kfind에 넣는다.** 앱이 문단(위계 포함)을 보내고 서비스가 문장으로 나눠 쓴다. 문서마다 "위계 출처·규칙 판"을 기록해 낡은 것만 다시 보낸다(지금 klib→Python `relevel_file` 방식과 같은 구조).
- 전환: 설정 `search_engine` = `legacy`(기본) → `shadow`(둘 다 돌려 비교, 화면은 legacy) → `kfind`(화면은 kfind, 예전 색인은 계속 유지) → 한 버전 뒤 `legacy` 폐지. 되돌리기는 설정 한 값.
- **"차이 0"의 뜻을 둘로 나눈다.** 규칙이 바뀌었으므로(문단→문장 일치 Q3, 붙여 쓴 검색어 넓게, 붙어 있는 일치 우선, 제목 결합 일치 없앰) 예전 결과와 새 결과는 같을 수 없다. 합격 기준은 ① **새 경로 대 Python 기준 구현: 차이 0** ② **예전 대 새: 모든 차이가 미리 정한 종류(E1~E8)로 설명됨, 설명 안 되는 차이 0**.
- kfind-core에 **없는 것**: 문서 거르기(조사 폴더·`ext:`·`path:`·날짜·가상 폴더·출처), 제외어, 구문, 동의어, 원문(미리보기 글). 앞의 셋은 엔진 카드(KFC1~KFC4)로 넣고, 원문은 1차에 기존 SQLite 문단 표에서 읽는다.

---

## 1. 현재 검색 경로 지도

### 1-1. 화면 → 명령 → 함수 → 엔진

| 화면·메뉴 | Flutter 호출 | Python 진입 | 실제 검색 함수 | 엔진·저장소 | 일치 단위 / 순위 |
|---|---|---|---|---|---|
| 타임머신 본문검색 | `timemachine` `content_search` (`timemachine_api.dart:254`) | `TimemachineService._command` `timemachine_service.py:649` | `ContentSearchIndex.keyword_search` `content_search.py:1775` | SQLite FTS5 trigram(`content_paragraph_fts`) + 2글자 표(`content_short_grams`), `timeline-content.sqlite` | 문서 안 AND(문단 넘나듦), BM25, 문단 위치 최대 3개 |
| 타임머신 위계검색(검색어) | `related_search` {query, sort} | ISUI2 `_related_query_search` (`840df579`, **이 브랜치엔 없음**) | `ContentSearchIndex.search` `content_search.py:1310` | 같은 SQLite | 문서 안 AND, 위계 > 언급 수 > 날짜 > bm25 > 경로(`_sort_hierarchy_results` :159), **제목 결합 일치**(:1570-1603) 뒤로 |
| 타임머신 연관 문서(기준 문서) | `related_search` {path} | `timemachine_service.py:678` | `related_candidates` :2126 | SQLite | **교체 범위 아님**(TM1 T9 몫) |
| 타임머신 파일명 | `file_search` (`timemachine_api.dart:290`) | `timemachine_service.py:675` | `search_files` :2097 | SQLite `content_meta` 전체 훑기 | 이름 부분 문자열 AND, 이름순 |
| 가상 폴더 파일·본문(상자) | `library` → `item_search`/`search` {box_id} | `library_bridge.request` (`ALLOWED_COMMANDS` :17) → klib 파이프 | klib `pipe.rs:1372`·`:1047` → `SearchIndex::search_box` `search.rs:458` | Tantivy(2글자·자모·초성 필드, 동의어 확장 `search.rs:479`) | 문단 단위, 문서별 묶기, `hit_pages`, `box_ids` 거르기 |
| 가상 폴더 본문·위계 메뉴(ISUI2) | `timemachine` `content_search`/`related_search` | 타임머신과 같음 | 타임머신과 같음 | SQLite | 결과를 Dart(`library_document_search.dart`)가 상자 자료 경로로 거름 — **조사 폴더 밖 자료·메모는 못 찾음**(IS1 결정의 "합치기" 대상) |
| 업무자료함(노트북) 본문검색·출처 비교 | `nb_search`·compare (`notebook_api.dart:311`) | `library_bridge.request` → klib | `pipe.rs:749` → `SearchIndex::search_notebook` `search.rs:348` | Tantivy(같은 색인, `notebook_id`·`source_id` 필드) | 문단 단위, 출처 거르기 |
| kvf(가상 폴더 팝업) | — | — | `vf_data::filter` (`feature/vfolder-popup`) | `library.db` 읽기 | 이름만 거름. 본문검색 없음 |
| Python `library` 서비스 | `library` `search` | `LibraryService.search` `library_service.py:219` | `docfit_core/library/store.py:165` | `workspace/library.db` | 미확인(화면에서 쓰는지 확인 못 함). 이 설계의 교체 대상 아님 |

### 1-2. 색인 쓰기 경로

| 대상 | 쓰는 쪽 | 위계 출처 | 상태 표지 |
|---|---|---|---|
| 타임머신 본문 | `_content_index_worker` `timemachine_service.py:185`: txt·md·hwpx·docx·hwp는 klib `index-content`(Rust)가 SQLite에 본문 행을 쓰고 → `stale_hierarchy_paths` → `relevel_file`(:1243)이 Python 규칙으로 위계를 다시 매김. 나머지는 `index_file`(:1148, Python) | **Python 규칙만 기준**(`content_search.py:1229` 주석). `CURRENT_HIERARCHY_SCHEMA_VERSION = 4`(:72) | `content_meta.hierarchy_schema_version`, `content_errors`, 작업 `phase`(SM2: idle/indexing/reindexing/cancel_requested/cancelled/finished/failed) |
| 가상 폴더·업무자료함 | klib 파이프 `index_file` 등 → Tantivy | klib `classify_levels.rs`(Rust, **Python과 같은지 미확인**) | Tantivy 스키마 버전, `rebuild` 플래그(`search.rs:83`) |

### 1-3. 실시간 검색 화면 상태(ISUI1, `instant_search.dart`)

`InstantSearchPhase`: empty → typing(150ms 지연) → searching → results | error. 동시에 1건만 보내고 그 사이 입력은 마지막 1건만 남긴다(요청 번호로 늦은 응답 무시). 이유: `ipc_server.py`가 디스패치 전체를 전역 잠금으로 감싸 **서버 쪽 취소가 불가능**하다(파일 머리 주석). → 이 설계에서 kfind 질의는 시간 한도(2초)·취소 플래그가 있으므로, Python이 **새 요청이 오면 앞 kfind 질의를 `cancel`로 끊는 것**을 넣는다(§2-4).

---

## 2. 호출 방식: 앱 Python → kfind 파이프 (TM1 Q17 확정)

### 2-1. 프로세스 그림

```
Flutter ─IPC(전역 잠금)─▶ Python 앱
                           ├─ timemachine_service ─┐
                           ├─ library_bridge ──────┼─▶ kfind_client.py ─파이프─▶ kfind.exe (HwpAutoDocFit.Index 서비스 또는 사용자 권한 대체 모드)
                           │                       │                               └─ kfind-core Index/Writer (쓰기는 서비스 하나)
                           └─ ContentSearchIndex(SQLite) — legacy·shadow 모드와 미리보기 원문
```

- 파이프 이름·보안(원격 거부, 요청 64KB 상한, 클라이언트 SID 확인)은 T4 카드 그대로.
- 대체 모드(서비스 설치가 막힌 PC, TM1 Q15): 같은 `kfind.exe`가 사용자 권한으로 같은 파이프를 연다. 앱 쪽은 차이를 모른다(`status`의 `mode`로만 구분).

### 2-2. 파이프 명령(앱이 쓰는 것)

T4가 정하는 `query`·`cancel`·`status`에 **앱 연결에 필요한 명령을 더한다**(T4 다음 카드 KFS1). 요청·응답은 UTF-8 JSON 한 덩어리(64KB 상한). 모든 응답에 `gen`(답한 색인 세대)과 `ms`.

| 명령 | 요청 | 응답 | 엔진 함수 |
|---|---|---|---|
| `query` | `{id, q, scope, top, refs, split?, filters?}` | `{id, gen, words, dropped, total_docs, total_sentences, adjacent_docs, hits:[{doc_key, path, adjacent, level, count, date, sentences:[{ord, para, start, len, level, adjacent}]}]}` | `Index::rank` + 보일 문서만 `Ranking::sentences` |
| `count` | `{id, q, scope, filters?}` | `{id, gen, words, dropped, total_docs, total_sentences}` | `Index::count` |
| `cancel` | `{id}` | `{ok}` | `SearchOptions.cancel` 플래그 |
| `status` | `{}` | `{svc_state, mode, gen, docs, sentences, fold_rule, format, level_rule_counts:{py4, rs, none}, queue, last_error}` | 서비스 |
| `ingest` | `{doc_key, path, date, level_src, level_rule, scopes, paras:[{p, lv, t}], part?, parts?}` | `{ok, queued}` | 서비스가 문장 나누기 → `Writer::add_document` |
| `remove` | `{doc_keys:[…]}` | `{ok, removed}` | `Writer::delete_document` |
| `stale` | `{scope, level_rule}` | `{doc_keys:[…], more}` | 서비스 문서 표 조회 |

- `scope`: `tm`(조사 폴더), `box:<id>`, `nb:<id>`(+`sources:[…]`). 서비스의 문서 표(doc_key ↔ doc_id ↔ path·size·mtime·ext·reference_date·scopes·level_src·level_rule)로 허용 문서 집합을 만들어 KFC1 `allow`로 넘긴다.
- `filters`: `ext`·`path`·`after`·`before`·`exclude`·`phrases`(지금 `parse_query` `content_search.py:996`의 결과 그대로). 해석은 Python이 하고 서비스는 값만 받는다(문법이 한 곳).
- `ingest`가 64KB를 넘으면 `part`/`parts`로 나눠 보내고 서비스가 클라이언트별로 모아(문서 하나 64MB 상한, TM1 견고성 1과 같은 수) 마지막 조각에서 넣는다. (Q-A7)
- `path`를 `query` 응답에 바로 넣는다(TM1 §1-5의 `path_of` 왕복을 앱 결과마다 하지 않으려고).

### 2-3. `docfit_app/kfind_client.py`(새) — 함수 단위

| 함수 | 하는 일 |
|---|---|
| `KfindClient.__init__(pipe_name, *, opener=None, clock=None)` | 시험용 가짜 파이프 주입 |
| `connect()` | 파이프 열기 + `status`로 판 확인(`fold_rule`·`format`·프로토콜 판). 실패 이유를 연결 상태(§3-2)로 |
| `query(q, scope, *, top, refs, filters, request_id)` | 한 번 왕복. 시간 한도 넘으면 `KfindTimeout` |
| `count(q, scope, *, filters, request_id)` | 개수만 |
| `cancel(request_id)` | 앞 질의 끊기 |
| `ingest(doc)` / `remove(keys)` / `stale(scope, level_rule)` | 색인 쓰기 요청 |
| `state()` | 연결 상태 값(§3-2)과 마지막 오류 — `content_status()`에 그대로 실음 |

### 2-4. 입력마다 앞 질의 끊기

ISUI1이 밝힌 대로 Flutter→Python은 요청을 차례로 처리한다. 그래서 Python 쪽은 **질의 전용 스레드 1개 + "가장 최근 요청 1건" 슬롯**을 두고, 새 검색 요청이 들어오면 진행 중인 kfind 질의에 `cancel`을 보낸다. 늦은 응답은 요청 번호로 버린다(Flutter와 같은 규칙). 전역 잠금을 쥔 채 파이프 응답을 기다리지 않게 하는 것은 `ipc_server.py`를 고쳐야 할 수 있어 **카드 APP1-E에서 먼저 측정**한다(미확인).

---

## 3. 기존 SQLite 색인과의 병행·전환·되돌리기

### 3-1. 기능 플래그

- 위치: 앱 설정(`settings_schema.DEFAULTS`)에 `search_engine` = `"legacy" | "shadow" | "kfind"`, 기본 `"legacy"`. 세 화면(타임머신·가상 폴더·업무자료함)을 **한 값으로 함께** 바꾼다(IS1 결정: 동시 교체). 개발자 모드에서만 화면별 덮어쓰기(`search_engine_override`)를 둔다(시험·문제 분리용).
- 기존 `timemachine.json`의 `rust_content_index`(klib 색인 사용 여부, `timemachine_service.py:219`)와는 별개다. 그 값은 SQLite에 쓰는 방법만 정한다.

| 모드 | 화면에 나오는 것 | SQLite·Tantivy 색인 | kfind 색인 | 비교 |
|---|---|---|---|---|
| `legacy` | 예전 결과 | 지금과 같이 유지 | 만들지 않음(서비스가 없거나 꺼짐) | 없음 |
| `shadow` | 예전 결과 | 유지 | 앱이 `ingest`로 채움 | 질의마다 두 결과를 비교해 개발자 로그에 차이 종류만 기록(§6-3) |
| `kfind` | kfind 결과 | **계속 유지**(되돌리기용, 한 버전) | 앱이 채움 | 없음(원하면 개발자 모드에서 shadow 비교 계속) |
| (폐지 후) | kfind | FTS5·Tantivy 본문 색인 지움, 문단 원문 표만 남김(Q-A6) | 유지 | — |

### 3-2. 상태 기계 ① kfind 연결 상태(앱 쪽, `KfindClient.state()`)

| 상태 | 뜻 | 들어오는 전이 |
|---|---|---|
| `off` | 플래그가 `legacy` | 시작, 설정 변경 |
| `connecting` | 파이프 여는 중 | off → (플래그 shadow/kfind) |
| `ready(gen)` | `status` 확인 끝, 판 일치 | connecting → 성공 |
| `unavailable(reason)` | `not_installed`·`stopped`·`denied`·`version_mismatch`·`timeout` | connecting → 실패, ready → 파이프 끊김 |
| `rebuilding` | 서비스가 판 차이(`FormatVersion`/`RuleVersion`)로 다시 만드는 중 | ready → `status.svc_state=rebuilding` |

- 불가능한 전이: `off`에서 바로 `ready`(판 확인 없이 질의 금지), `unavailable`에서 질의 성공으로 넘어가기(반드시 `connecting`을 거침).
- `kfind` 모드에서 상태가 `ready`가 아니면 **그 질의는 legacy로 답하고** 응답에 `engine:"legacy", engine_reason:"…"`를 붙인다(화면 상태 줄: "새 검색 엔진 준비 중 — 예전 방식으로 찾음"). 조용히 바꾸지 않는다.
- 다시 연결: `unavailable`은 5초 뒤 한 번, 이후 30초마다(질의가 있을 때만). 타이머 상주 없음.

### 3-3. 상태 기계 ② 문서 색인 상태(문서 하나, 서비스 문서 표 + 앱 SQLite)

```
not_indexed ─(앱 index_file/relevel 성공)─▶ extracted(py4) ─ingest─▶ queued ─commit─▶ visible(gen g)
     ▲                                                                                │
     └──────────────(파일 지워짐·조사 폴더 밖·제외 이름: remove)◀──────────────────────┤
visible ─(mtime·size 바뀜 | level_rule < 현재 | fold_rule 바뀜)─▶ stale ─▶ extracted …
extracted ─(추출 실패)─▶ failed(content_errors와 같은 이유)
서비스 자체 추출(T5 뒤): not_indexed ─▶ visible(level_src=rs) ─(앱이 켜지면 relevel)─▶ visible(level_src=py4)
```

- 표지 한 곳: 앱 SQLite `content_meta`에 열 2개를 더한다 — `kfind_level_rule`(마지막으로 kfind에 넣은 위계 규칙 판), `kfind_gen`(그때 받은 세대). 서비스 문서 표가 원본이고 이 두 열은 앱이 빨리 고르기 위한 사본이다. 어긋나면 서비스 `stale`이 이긴다.
- 불가능한 전이: `failed` 문서가 `visible`로 남는 것(실패하면 `remove`를 먼저 보냄 — 지금 `index_file` 실패 경로 `content_search.py:1216-1226`이 행을 지우는 것과 같은 규칙), `level_src=rs` 문서가 `py4` 문서를 덮는 것(같은 mtime·size면 서비스가 거절).

### 3-4. 상태 기계 ③ 코퍼스 상태(`status` 응답, 화면·에이전트 표시)

| 상태 | 조건 | 화면 |
|---|---|---|
| `empty` | 문서 0 | "새 검색 색인 없음" |
| `seeding` | 앱이 SQLite에서 처음 채우는 중(§3-6) | 진행 수/전체 |
| `catching_up` | `stale` 수 > 0 | "n건 갱신 중" |
| `synced(gen)` | stale 0, 대기열 0 | 문서 수·세대 |
| `rebuilding` | 형식·비교형 규칙 판이 바뀜 | "새 규칙으로 다시 만드는 중" |
| `corrupt` | `Error::Corrupt` | 자동으로 `rebuilding`, 그동안 legacy로 답 |

### 3-5. 상태 기계 ④ 전환 상태(설정 값 + 판정 기록)

```
legacy ──(사용자 켬, 서비스 설치)──▶ shadow ──(관문 통과 §6-4 + 사용자 승인)──▶ kfind ──(한 버전 + 사용자 승인)──▶ legacy_retired
  ▲                                   │                                       │
  └────────(되돌리기: 설정 한 값)──────┴───────────────────────────────────────┘
legacy_retired ─▶ legacy : 불가능(예전 색인이 없음). 필요하면 SQLite 재색인을 명시 작업으로(수 분~수십 분, 측정 필요)
```

- 관문 판정 결과는 `reports/APP1-gate-<날짜>.json`(차이 수·종류별 수·지연)에 남기고, 설정 화면은 마지막 판정만 보여 준다.
- `kfind` 모드에서도 SQLite 쓰기는 계속하므로 되돌리기 즉시 같은 결과가 나온다(되돌리기 시험 §6-5).

### 3-6. 처음 채우기(seeding): 다시 추출하지 않는다

SQLite 문단 표에는 이미 Python 규칙 위계(`level_rank`)가 들어 있다(`hierarchy_schema_version = 4`인 문서). 그래서 첫 채우기는 **문서를 다시 열지 않고** `content_paragraph_map` + `content_paragraph_fts`에서 `(path, paragraph_index, level_rank, body)`를 읽어 `ingest`로 보낸다(함수 `ContentSearchIndex.paragraphs_for_kfind(path)`, 카드 APP1-C). 판이 낮은 문서는 지금처럼 `relevel_file` 뒤에 보낸다. 가상 폴더·업무자료함 자료 중 조사 폴더 밖 파일·메모는 SQLite에 없으므로 Python `extract_paragraphs`로 새로 추출한다(§4-3).

---

## 4. 위계 판정: Python 규칙 그대로, 색인할 때 넣기

### 4-1. 값 대응

kfind-core `Sentence { para: u32, level: u8, text }`, `level`은 작을수록 높음. Python `LEVEL_ORDER`(`content_search.py:23`)의 순번이 이미 같은 뜻이다.

| Python level | level_rank | kfind `level` |
|---|---|---|
| document_title | 0 | 0 |
| chapter·section·topic·square·circle·dash·asterisk | 1~7 | 1~7 |
| body, table_cell | 8 | 8 |

- 표 칸은 순위가 본문과 같고(:29) 표시 이름만 다르다 → kfind에는 8로 넣고, 표시용 `table_cell` 여부는 미리보기에서 SQLite `level` 열로 읽는다(지금 `search()`가 :1635에서 하는 것과 같음).
- 날짜: `_document_date(ctime, mtime)`(:153) = 둘 중 큰 값, 없으면 0. kfind `date`는 i64 → **밀리초 정수**로 넣는다(초로 자르면 같은 초 문서끼리 순서가 바뀔 수 있음). 0은 kfind에서도 가장 뒤라 "날짜 없는 문서 뒤로"(:166)와 같다.

### 4-2. 문장 나누기는 서비스 한 곳에서

kfind-core는 문장 나누기를 하지 않는다(`writer.rs:21`, KC1 §9 "S5 `sentence.rs`"). 아직 `sentence.rs`는 **없다**(이 브랜치 확인). 권고(Q-A1): 앱은 **문단(위계 포함)** 을 보내고 서비스의 Rust `sentence.rs`가 나눈다. 문단 위계는 그 문단의 모든 문장에 똑같이 붙는다. Python에는 시험 전용 기준 구현 `kfind/core/tools/sentence_ref.py`를 두고 비교형(`fold_ref.py`)처럼 같은 대조 표로 맞춘다. 이렇게 하면 나누기 규칙이 하나이고, `ktm.exe`·klib 작업 프로세스(T5)와도 같은 규칙을 쓴다.

### 4-3. 넣는 함수(앱 쪽)

| 함수(새, 파일) | 입력 → 출력 |
|---|---|
| `kfind_records.doc_from_records(path, records, stat, ref_date, scopes)` (`docfit_core/timemachine/kfind_records.py`) | `ParagraphRecord` 목록 → `ingest` 요청 dict(위 표 대응, `level_src="py"`, `level_rule=CURRENT_HIERARCHY_SCHEMA_VERSION`) |
| `kfind_records.doc_key(path=None, memo=None)` | 파일은 `os.path.normcase(os.path.abspath(path))`, 메모는 `memo:<box>:<item>`, 업무자료함 메모는 `nbmemo:<nb>:<id>` |
| `ContentSearchIndex.paragraphs_for_kfind(path)` (`content_search.py`, 새 메서드) | SQLite 문단 표 → `ParagraphRecord` 목록(seeding용) |
| `TimemachineService._kfind_sync(path, result)` | `index_file`·`relevel_file`·`remove_file` 결과를 받아 `ingest`/`remove`를 대기열에 넣음(플래그 `legacy`면 아무것도 안 함) |
| `LibraryKfindSync.sync_box(box_id)` (`docfit_app/library_kfind_sync.py`) | 상자 자료 목록(klib `item_search` 빈 질의) → 조사 폴더 안 파일은 `scopes`에 `box:<id>`만 더함, 밖 파일·메모는 Python 추출 후 `ingest` |
| `NotebookKfindSync.sync_notebook(nb_id)` | 출처(파일·메모) → 같은 방식, `scopes`에 `nb:<id>:src:<sid>` |

- 위계 규칙이 바뀌면(`CURRENT_HIERARCHY_SCHEMA_VERSION` 올림) `stale(scope, level_rule)`가 낡은 문서를 돌려주고 seeding과 같은 경로로 다시 보낸다 — 지금 `needs_reindex_count`(:1026)와 같은 뜻을 kfind에도 둔다.
- 앱이 꺼져 있는 동안 서비스가 스스로 추출한 문서(T5 뒤)는 `level_src="rs"`로 표시된다. 그동안 위계 순서는 klib `classify_levels.rs` 판정을 따른다(Python과 같은지 미확인, Q-A2). 앱이 켜지면 `stale`로 `rs` 문서를 골라 Python 위계로 바꾼다.

---

## 5. 결과 형식 대응과 개수만(count) 사용처

### 5-1. 응답 필드 대응(화면이 바뀌지 않게 같은 이름으로)

어댑터 모듈 `docfit_core/timemachine/kfind_results.py`(새). 미리보기 글·위계 경로는 kfind에 원문이 없으므로 **SQLite 문단 표에서** 읽는다(Q-A6). 강조 위치는 kfind 응답의 `words`(나눈 조각 포함)로 문장 안에서 Python이 찾는다.

| 예전 필드 | 예전 출처 | 새 값 |
|---|---|---|
| `path`, `name`, `extension`, `mtime`, `folder_path`, `folder_name` | `content_meta` | 응답 `path` + `content_meta`(조사 폴더 밖 자료는 서비스 문서 표) |
| `snippet`, `highlights` | 고른 문단 앞 70자~뒤 180자 | **일치 문장**(`para`·`start`·`len`)을 중심으로 같은 창 크기, 강조는 `words`/조각 |
| `locations[]`(최대 3) | BM25 순 문단 | `sentences[]` 앞 3개(`refs=3`), `position.label`은 SQLite `location_label` |
| `breadcrumb`, `breadcrumb_text`, `sections` | `hierarchy_path.breadcrumb`(SQLite 위계) | 같은 함수, 고른 문단 = 첫 일치 문장의 `para` |
| `best_level` | 일치 문단 최소 `level_rank` | `hit.level` → `LEVEL_LABELS` |
| `total_matches`, `match_counts`, `score` | 낱말별 출현 횟수 합 | `hit.count`(**일치 문장 수**). `match_counts`는 낱말마다 같은 값(문장마다 모든 낱말이 있으므로). 뜻이 바뀜 → Q-A5 |
| `relevance_rank` | 순번 | 순번 |
| `context_hits` | 제목 결합 일치 수 | 0(제목 결합 일치 없음, Q-A3) |
| klib `hit_pages`, `page` | Tantivy `page` 열 | SQLite `location_label`의 쪽 번호(`keyword_search`가 :2083에서 하는 해석 그대로) |
| klib `box_ids` | Tantivy | 서비스 문서 표 `scopes` |
| `nb_search` `source_id`·`ordinal`·`display_name`·`location`·`body` | Tantivy 문단 | 서비스 문서 표(`nb:…:src:<sid>`) + 문장 `para` + 원문 |
| 새 필드 | — | `adjacent`(붙어 있는 일치), 응답 머리 `dropped`(짧아서 뺀 낱말), `engine`, `gen` |

정렬 선택(`sort`): `hierarchy_relevance` = kfind 기본 순위. `mentions` = `hit.count` 내림차순(상위 N이 아닌 전체 순위 `top=0` 뒤 Python 정렬 — 0.35~0.76ms, KB2). `relevance`(BM25) = 1차에는 kfind 기본 순위로 대신(Q-A9).

### 5-2. 개수만(`count`, 0.07~0.17ms) 사용처

| 곳 | 언제 | 보여 줄 것 |
|---|---|---|
| 실시간 검색 상태 줄(ISUI1 `typing`·`searching` 단계) | 150ms 지연 전, 글자마다 | "본문 n문서 · m문장" — 상위 N은 지연 뒤 `query` |
| 검색 메뉴 탭 배지(파일명·본문·위계) | 탭 전환 전, 검색어 바뀔 때 | 탭마다 문서 수 |
| 가상 폴더 상자 목록 | 검색어가 있을 때 | 상자마다 일치 문서 수(`scope=box:<id>`) |
| 업무자료함 출처 목록 | 출처 비교 | 출처마다 일치 문장 수 |
| 짧은 낱말 안내 | 항상 | `dropped` → "‘및’은 너무 짧아 뺐습니다"(IS1 Q13) |
| Markdown 내보내기 확인 | 내보내기 전 | "n건을 내보냅니다" |

`count`는 KFC1(거르기) 전에는 범위·거르개를 반영하지 못한다 → KFC1 뒤에 화면에 붙인다.

---

## 6. 시험 계획

### 6-1. 단위 시험(코드 카드마다)

- Python: `kfind_records`(위계·날짜·키 대응, 표 칸, 빈 문단), `KfindClient`(가짜 파이프로 연결 상태 전이 전부와 불가능한 전이 거절), `kfind_results`(필드 대응, 강조 위치, 조각 강조), 플래그 분기(`legacy`에서 kfind를 한 번도 부르지 않음).
- Rust: KFC 카드마다 `cargo test`(kfind-core 현재 46건, KB4 보고 — 이번에 다시 돌리지 않음) + `oracle.py` 대조 표 확장.
- 기존 Python 시험 수 불변(아래 기준선).

### 6-2. 결과 동등성 비교 도구 `tools/app1_equivalence.py`(새)

입력: 문서 폴더(사용자 문서 **사본**, `E:/docfit-tmp/app1-corpus`), 검색어 목록, 범위(`tm`/`box`/`nb`). 출력: `reports/APP1-equiv-<날짜>.json`·`.md`.

1. 사본 폴더로 예전 경로 색인(SQLite + klib Tantivy)을 만든다.
2. 같은 문단(SQLite에서 읽은 Python 위계)을 kfind 서비스(사용자 권한 모드, 시험용 파이프 이름)에 `ingest`한다.
3. **기준 구현**: SQLite 문단 → `sentence_ref.py` → `oracle.py` 규칙(비교형·나누기·문장 일치·순위) + Python 거르기(§2-2 `filters`)로 기대 결과를 낸다.
4. 비교 A(합격선 **차이 0**): kfind 경로 대 기준 구현 — 검색어마다 일치 문서 수·문장 수·상위 100 문서 순서·문서별 일치 문장 `(para, ord)`·`dropped`.
5. 비교 B(합격선 **설명 안 되는 차이 0**): 예전 대 새. 차이마다 아래 종류를 자동으로 붙인다.

| 종류 | 설명 | 판정 방법 |
|---|---|---|
| E1 | 문단 안 다른 문장·다른 문단에 낱말이 흩어져 있어 빠짐(Q3) | 기준 구현의 문장 일치가 없고 문서 AND는 있음 |
| E2 | 붙여 쓴 검색어를 나눠 새로 찾음(넓게) | 결과 `words[].pieces` 있음 |
| E3 | 붙어 있는 일치 우선으로 순서 바뀜 | `adjacent` 다름 |
| E4 | 제목 결합 일치가 빠짐(Q-A3) | 예전 `_context_match` |
| E5 | 비교형 차이(NFC·공백·구분자 제거) | `fold` 전후 비교 |
| E6 | 동의어·자모·초성 일치가 빠짐(klib) | Tantivy 동의어 표·자모 필드로만 일치 |
| E7 | 같은 순위 값의 동률 정렬 차이(경로순 대 문서 ID) | 순위 열쇠 같음 |
| E8 | 언급 수 뜻 변경(출현 횟수 → 문장 수)으로 `mentions` 정렬 차이 | 정렬 `mentions`에서만 |

검색어 목록(최소 80개): KB1 8개, `tools/kidx_index_regress.py` 10개, TM1 실기 40개, 거르개 조합 12개(`ext:`·`path:`·`after:`·`before:`·`-제외`·`"구문"`), 짧은 낱말·1글자 섞임 5개, 영문·숫자 5개. 같은 입력으로 두 번 돌려 결과가 같아야 도구 합격.

### 6-3. 앱 안 shadow 비교(개발자 모드)

`shadow` 모드에서 질의마다 두 결과를 위 E 종류로 분류해 `%LOCALAPPDATA%\HwpAutoDocFit\logs\search-shadow.jsonl`에 **종류별 수와 검색어 해시만** 쓴다(검색어 원문은 개발자 모드 설정을 따로 켰을 때만 — TM1 Q9와 같은 원칙, Q-A8). 1MB 순환.

### 6-4. 전환 관문(shadow → kfind)

- 비교 A 차이 0, 비교 B 설명 안 되는 차이 0(사본 말뭉치, 위 80개).
- shadow 실사용 비교에서 설명 안 되는 차이 0.
- 지연 1회 측정(측정 중 다른 시험 금지): Flutter 요청 → 응답, 본문 상위 20 p95가 예전 경로보다 짧을 것, 개수만 p95 ≤ 20ms(파이프·IPC 포함 목표, 근거 미측정 — APP1-E에서 먼저 잼).
- 코퍼스 상태 `synced`, 되돌리기 시험 통과.
- 사용자 승인(전환은 사용자 영향이 있는 선택, 운영 규칙 7절).

### 6-5. 되돌리기·고장 시험

- `kfind` → `legacy` 설정 변경 직후 같은 검색어 결과가 legacy 단독과 같음.
- 서비스 강제 종료 중 질의 → legacy로 답하고 `engine_reason` 표시, 앱 멈춤 없음(2초 한도).
- 판 불일치(가짜 `status`로 `format` 다르게) → `rebuilding`, 그동안 legacy.
- 손상 색인(`Error::Corrupt`) → 자동 다시 만들기, 그동안 legacy.

### 6-6. 기준선(이번에 직접 돌린 시험)

검색 관련 Python 시험 5개 모듈 210건 통과(§9). APP1 카드마다 이 수 + 새 시험 수로 보고한다.

---

## 7. 카드 단위 작업 분할(함수 단위, 의존 순서)

파일 범위가 겹치지 않는 카드만 병렬. 모든 카드: 로컬 커밋만, 기존 시험 불변, 임시 파일은 `E:/docfit-tmp`, GitLab 금지.

| 카드 | 내용(함수) | 파일 | 완료 조건 | 의존 | 워커 권고 |
|---|---|---|---|---|---|
| KFC1 | `SearchOptions.allow: Option<Arc<RoaringTreemap>>`(문서 ID 허용 집합)와 `deny`; `rank`·`count`·`match_sentences`가 따름 | `kfind/core/src/index.rs`, 새 시험 | oracle 대조 표에 범위 거르기 열 추가, 차이 0. 허용 집합 1만 문서에서 상위 20 지연 기록 | — | Claude |
| KFC2 | 낱말별 나누기 끄기(구문 `"…"` = 나누지 않은 붙어 있는 일치), `Index::doc_set(word)`(제외어용 문서 집합) | `query.rs`·`index.rs` | 대조 표 차이 0, 구문·제외어 시험 | KFC1 | Claude |
| KFC3 | `SentenceRef`에 `start`·`len`(문단 안 글자 위치) 노출 | `index.rs`·`segment.rs` 읽기만 | 대조 표 차이 0, 형식 판 그대로 | — | Codex 상위 |
| KFC4 | 동의어 확장(낱말마다 대안 목록, 대안 중 하나면 그 낱말 일치) — Q-A4 결정 뒤 | `query.rs`·`index.rs` | 대조 표 차이 0 | Q-A4 | Claude |
| SEN1 | 문장 나누기 `sentence.rs` + Python 기준 `sentence_ref.py` + 대조 표 | `kfind/core/src/sentence.rs`, `tools/sentence_ref.py` | 대조 표 200문단 차이 0 | Q-A1 | Claude |
| KFS1 | 서비스 명령 `ingest`·`remove`·`stale`·`count`, 문서 표(doc_key·scopes·level_src·level_rule), `query` 응답에 path·문장 위치 | `kfind/src/…`(T4 크레이트) | 사용자 권한 모드 실제 파이프 왕복 시험, 64KB 분할 ingest, 불가능한 전이 거절(rs가 py 덮기) | T4, KFC1, SEN1 | Claude |
| APP1-A | `kfind_records.doc_from_records`·`doc_key` | `docfit_core/timemachine/kfind_records.py`(새), `tests/test_kfind_records.py` | 단위 시험, 위계 9단계·표 칸·날짜 밀리초 | — | Codex 상위 |
| APP1-B | `KfindClient`(연결 상태 기계 포함) + 가짜 파이프 서버 | `docfit_app/kfind_client.py`(새), `tests/test_kfind_client.py` | 상태 전이 표 전부 시험 | 프로토콜 확정(KFS1 설계) | Codex 상위 |
| APP1-C | `ContentSearchIndex.paragraphs_for_kfind`, `content_meta` 열 2개(`kfind_level_rule`·`kfind_gen`) 이전 | `content_search.py` | 기존 `test_timemachine_content_search` 수 불변 + 새 시험 | — | Codex 상위 |
| APP1-D | `kfind_results.to_keyword_rows`·`to_hierarchy_rows`·`to_library_rows`·`to_notebook_rows` | `docfit_core/timemachine/kfind_results.py`(새) | 필드 대응 표(§5-1) 전부 시험, 강조 위치 | APP1-C | Codex 상위 |
| APP1-E | 측정 먼저: Python IPC 전역 잠금 아래 파이프 왕복·취소 지연(가짜 서버) → 필요하면 질의 스레드·최근 1건 슬롯 | `docfit_app/kfind_client.py`, (필요 시) `ipc_server.py` | 측정 보고(1회), 취소가 다음 질의를 막지 않음 | APP1-B | Claude |
| APP1-F | 타임머신 분기: `_search_engine()`, `content_search`·`related_search(query)` 분기, `_kfind_sync` 훅, seeding 작업(작업 상태 `phase`에 `kfind_seeding` 추가) | `timemachine_service.py` | `legacy`에서 기존 시험 수 불변, `kfind`·`shadow` 새 시험 | APP1-A~E, KFS1 | Claude |
| APP1-G | 가상 폴더·업무자료함 분기: `library_bridge.request`에서 `search`/`item_search`(질의 있음)/`nb_search`를 플래그에 따라 kfind로, `LibraryKfindSync`·`NotebookKfindSync` | `library_bridge.py`, `docfit_app/library_kfind_sync.py`(새) | `test_library_bridge`·`test_notebook_service` 수 불변 + 새 시험, 조사 폴더 밖 자료·메모가 본문검색에 나옴 | APP1-F | Claude |
| APP1-H | 가상 폴더 화면의 Dart 경로 거르기(ISUI2)를 서버 범위(`scope=box`)로 대체, `count` 배지·상태 줄 | `flutter_ui/lib/features/library/library_document_search.dart`, `instant_search.dart` 소비처 | 위젯 시험, ISUI 시험 수 불변 | APP1-G, ISUI 병합 | Claude(화면) |
| APP1-I | 설정 `search_engine` + 설정 화면 + `content_status()`에 `engine` 묶음(연결·코퍼스·전환 상태) | `settings_schema.py`, 설정 화면(미확인 위치), `timemachine_service.content_status` | 시험, 상태 문자열 표 | APP1-F | Codex 기본 |
| APP1-J | 동등성 도구 `tools/app1_equivalence.py` + 첫 보고 | `tools/`, `reports/APP1-equiv-*` | 같은 입력 2회 같은 결과, 비교 A 차이 0 | KFS1, APP1-D | Codex 상위 |
| APP1-K | 관문 판정·되돌리기 시험(§6-4·6-5) | 시험·보고서 | 관문 표 전부 숫자 | APP1-G~J | 총괄 |

병렬 가능 묶음: {KFC1, KFC3, SEN1, APP1-A, APP1-C} → {KFC2, APP1-B, APP1-D} → {KFS1, APP1-E} → APP1-F → {APP1-G, APP1-I, APP1-J} → APP1-H → APP1-K.

파일명 검색(`file_search`)을 kfind 이름 색인으로 바꾸는 것은 IS1 S1·S2(MFT 이름 색인)가 이 저장소에 아직 없어(미확인 — `kfind/`에 `core`·`engine-bench`만 있음) **이 묶음에서 뺀다**. 이름 색인이 생기면 별도 카드(APP2)로 `search_files` 분기만 더한다.

---

## 8. 결정이 필요한 것

번호 충돌 주의: IS1 계획의 Q17(위계검색 제목 결합 일치)과 TM1의 Q17(앱 파일명 검색 = 파이프, **확정**)은 다른 질문이다. 카드의 "Q17 확정"은 TM1 쪽으로 읽었다.

| Q | 질문 | 권고(답이 없으면 이쪽, 되돌리기 쉬움) | 대가 |
|---|---|---|---|
| Q-A1 | 문장 나누기를 어디서 하나? | 서비스 Rust `sentence.rs` 한 곳 + Python 기준 구현(시험 전용) | Python 기준과 Rust를 대조 표로 계속 맞춰야 함 |
| Q-A2 | 앱이 꺼져 있을 때 서비스가 스스로 색인한 문서(T5 뒤)의 위계 | klib `classify_levels.rs` 판정을 `rs`로 표시해 넣고 앱이 켜지면 Python 위계로 교체 | 그 사이 위계 순서가 Python과 다를 수 있음(차이 크기 미확인). 대안: 모두 본문(8)으로 넣기 — 순서가 더 크게 틀림 |
| Q-A3 | (IS1 계획 Q17) 제목에 "예산", 아래 문장에 "집행"인 제목 결합 일치를 없애도 되나? | 없앰(Q3 문장 규칙, 속도) | 위계검색 결과 일부 빠짐(E4로 수를 보고) |
| Q-A4 | klib에 있던 동의어(동주민센터↔행정복지센터 등)·자모·초성 본문 일치를 유지하나? | 동의어는 유지(KFC4), 자모·초성은 본문에서 빼고 파일명 거르기에만 | 동의어 유지 시 질의가 낱말 수만큼 느려짐(미측정). 초성 본문검색 사용자 영향 미확인 |
| Q-A5 | "언급 횟수"를 "일치 문장 수"로 바꿔 표시해도 되나? | 예, 화면 글자를 "n문장"으로 | `mentions` 정렬 순서가 바뀜(E8) |
| Q-A6 | 미리보기 원문·위계 경로를 어디서 읽나? | 1차: SQLite 문단 표(병행 기간에 어차피 유지). 예전 경로 폐지 때 FTS5·short_grams만 지우고 문단 표는 남김 | 디스크 일부 유지. 대안(서비스가 원문 저장)은 kfind 색인 커짐 + 형식 판 올림 |
| Q-A7 | 64KB 넘는 문서를 어떻게 보내나? | 파이프에서 `part`로 나눠 보냄 | 서비스가 클라이언트별 조각을 모아야 함. 대안(공유 폴더에 파일로 두고 경로만)은 권한·정리 문제 |
| Q-A8 | shadow 비교 로그에 검색어 원문을 남기나? | 기본은 해시만, 개발자 모드에서 따로 켤 때만 원문 | 차이 원인 조사가 느려짐 |
| Q-A9 | 본문검색의 `relevance`(BM25) 정렬을 유지하나? | 1차에는 kfind 기본 순위로 대신하고 정렬 메뉴에서 "관련도"를 "기본"으로 이름 변경 | BM25를 원하던 사용자 순서 변화 |
| Q-A10 | 전환 순서: shadow 기간을 얼마나? | 관문(§6-4) 통과 뒤 사용자 승인 1회. 기간은 정하지 않음 | — |
| Q-A11 | 가상 폴더의 조사 폴더 밖 원본 파일까지 색인하나(ISUI 결정 "합치기")? | 예, 상자에 넣은 파일·메모만. 상자에서 빼면 `remove` | kfind 색인 크기 증가(상자 자료 수에 비례) |
| Q-A12 | 질의 이력 캐시(KB4, 기본 꺼짐)를 앱에서 켜나? | 끈 채로 시작, 실제 문서 사본으로 재고 결정(KB4 남은 문제) | — |
| Q-A13 | 날짜 기준: 타임머신 max(ctime, mtime), klib mtime이 다르다. 하나로? | max(ctime, mtime)로 통일 | 가상 폴더·업무자료함 동률 순서 변화(E7) |

---

## 9. 이번 작업의 시험·확인

- 코드 수정 없음. 이 문서 1개만 더함.
- 기준선(변경 전 상태, 이 브랜치 `f2355aca`, 2026-11-13 직접 실행): **210건 통과, 실패 0, 75.4초.** 이후 APP1 카드는 이 수가 줄지 않아야 한다.
  `C:/HWP_AUTODOCFIT/v1803/exfile/.venv/Scripts/python.exe -X utf8 -m unittest tests.test_timemachine_content_search tests.test_timemachine_service tests.test_timemachine_content_index_lifecycle tests.test_library_bridge tests.test_notebook_service`
  (같은 실행의 시험 안 출력: 2글자 3,000건 전체검색 0.5194초, LIKE 후보 0.0180초, 그램 후보 0.0036초 — 1회, 참고값)
- `cargo test`(kfind-core)는 돌리지 않음(코드 변경 없음, KB4 보고 46건 통과는 워커 보고 숫자).

## 10. 남은 문제·확인 못 한 것

- T4(kfind 서비스) 프로토콜이 아직 확정 전이다(커밋 없음). §2-2 명령 형식은 이 설계의 제안이고 T4·KFS1에서 맞춰야 한다.
- klib `classify_levels.rs`가 Python 위계 규칙과 같은지 확인하지 않았다(Q-A2의 대가 크기).
- Python IPC 전역 잠금 아래 파이프 왕복·취소 지연을 재지 않았다(APP1-E에서 먼저).
- 가상 폴더 설정 화면 파일 위치, Python `library` 서비스(`store.py`)를 화면이 쓰는지 확인하지 않았다.
- 실제 한국어 문서에서 kfind 크기·속도(KB2~KB4 남은 문제)는 여전히 미측정이다. 관문 측정(§6-4)이 첫 실측이 된다.
