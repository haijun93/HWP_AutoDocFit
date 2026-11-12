# IS1 실시간·다중 단어 검색 — 조사 (2026-11-12)

- 사용자 지시(2026-11-12 요지): 타임라인·파일명 검색을 Search Everything 같은 기술로 벤치마킹해 Rust 기반 최고 속도로 만든다. 한 글자 칠 때마다 즉시 결과(실시간 검색). 여러 단어 키워드는 띄어쓰기와 상관없이 그 단어들이 함께 있는 문장을 중심으로 찾는다. 타임라인·본문검색에도 여러 단어 검색. 본문검색도 실시간이 되게 한다.
- 이 문서는 조사만 한다. 코드는 고치지 않았다. 계획은 `docs/IS1_PLAN_2026-11-12.md`.
- 표기: **확인**은 출처를 직접 읽은 것, **확인 못 함**은 출처에서 찾지 못한 것, **미확인**은 이 조사에서 읽지 않은 코드다.

## (가) Everything이 빠른 이유 — 공개 자료 기준

읽은 출처(2026-11-12 열람): voidtools FAQ `https://www.voidtools.com/faq/`, 색인 안내 `https://www.voidtools.com/support/everything/indexes/`, 검색 안내 `https://www.voidtools.com/support/everything/searching/`, 검색 문법 `https://www.voidtools.com/support/everything/search_syntax/`. `/support/everything/content/`는 404(없음).

| 항목 | 공개 자료 내용 | 판정 |
|---|---|---|
| 이름만 색인 | "only indexes file and folder names and generally takes a few seconds to build its database" (FAQ) | 확인 |
| 대상 파일 체계 | 고정 NTFS 볼륨 기본 색인, 고정 ReFS 자동 색인. FAT·exFAT·네트워크 드라이브는 "folder indexing"으로 추가(색인 안내) | 확인 |
| NTFS MFT 직접 읽기 | 위 4개 쪽에 MFT라는 말이 없다. "low level read access to NTFS volumes"만 있다(FAQ) | **확인 못 함**(MFT라는 낱말은 공개 쪽에서 못 찾음. 볼륨 저수준 읽기는 확인) |
| USN 변경 기록으로 실시간 갱신 | "automatically keep your NTFS indexes up to date with the NTFS USN Journal", "Changes will not be missed when Everything is not running as the system maintains the NTFS USN Journal" (FAQ) | 확인 |
| 비 NTFS 폴더의 변경 감시 방식 | "folder monitors"라는 말만 있다. ReadDirectoryChanges 사용 여부·재검사 주기는 없음 | 확인 못 함 |
| 관리자 권한 | "NTFS indexing requires the Everything service or running Everything as administrator", "requires administrative privileges for low level read access to NTFS volumes" (FAQ) | 확인: **NTFS 색인에는 관리자 권한 또는 서비스 설치(설치 시 관리자)가 필요** |
| 메모리 | Windows 11 새 설치(약 25만 파일) 약 35MB RAM·디스크 14MB 미만, 100만 파일 약 100MB RAM·45MB 디스크, 색인 시간 25만 약 5초·100만 약 1분 (FAQ). DB는 실행 중 메모리에 있고 종료 때만 디스크 저장(색인 안내) | 확인 |
| 입력마다 즉시 필터 | "Type a partial word to instantly find files", 설정에 "search as you type"(검색 안내) | 확인 |
| 검색 알고리즘(입력마다 전체 목록 훑기인지 역색인인지) | 공개 쪽에 없음 | 확인 못 함 |
| 여러 단어 | 공백 = AND("AND is the default boolean operator", 예: `abc 123`), `|` = OR, `!` = NOT, OR가 AND보다 먼저 계산 | 확인. **단어 순서가 결과에 영향을 주는지는 확인 못 함** |
| 정규식 | PCRE, `regex:` 수식어. 메뉴에서 정규식을 켜면 일반 문법이 꺼진다(검색 안내) | 확인 |
| 본문 검색 | `content:` 함수로 가능하나 "File content is not indexed, searching content is slow", "content searches can be extremely slow". Word·PDF는 IFilter로 글자를 뽑는다 | 확인. **Everything의 실시간성은 이름 검색에만 해당하며 본문은 느리다고 스스로 밝힌다** |

우리에게 주는 뜻:
1. Everything의 속도는 "이름만, 전부 메모리, 변경은 OS 기록(USN)으로 받기" 세 가지에서 나온다(앞 둘은 확인, 알고리즘 세부는 확인 못 함).
2. 그 구조의 첫 단계(볼륨 저수준 읽기·USN)는 관리자 권한이나 서비스 설치가 필요하다. 구청 PC에서 사용자에게 관리자 권한이 있는지는 **사용자 확인 필요**(계획 Q1).
3. 우리 대상은 드라이브 전체가 아니라 사용자가 고른 조사 폴더 몇 개다(`timemachine_service.py`의 `roots`). 볼륨 전체 MFT를 읽을 이유가 약하고, 폴더 열거 + `ReadDirectoryChangesW`(관리자 불필요, KAgent가 이미 사용)로 같은 효과를 낼 수 있다.
4. 본문 실시간 검색은 Everything에 선례가 없다. 본문 쪽은 Everything이 아니라 우리 FTS 색인 구조를 고쳐야 한다.

## (나) 지금 우리 검색의 동작과 실측 지연

### 동작(코드 위치)

| 기능 | 화면 → 서비스 | 실제 처리 | 다중 단어 |
|---|---|---|---|
| 입력 | `flutter_ui/lib/features/timemachine/timemachine_screen.dart:1626` 검색창 `onChanged` → **300ms 디바운스** 뒤 본문 모드면 `_contentSearch`, 아니면 `_query` 갱신 후 `_filtersChanged()`(타임라인)·표 보기면 `_runFileSearch` | 검색마다 먼저 `_readContentStatus()`를 기다린다(`:315`, `:487`) — 왕복 1회 추가 | — |
| 앞 요청 버리기 | 화면: 세대 번호 `_contentSearchRevision`(`:314,342`)·`_fileSearchRevision`(`:486,505`)로 늦게 온 결과를 버린다 | **백엔드는 앞 요청을 취소하지 않는다**(끝까지 계산) | — |
| 요청 직렬화 | `flutter_ui/lib/core/ipc/native_client.dart:80` 루프백 소켓 → `docfit_app/ipc_server.py:67-68` | 모든 명령이 `with self.server.app.lock:` 하나를 잡고 실행된다. 본문검색 하나가 5초 걸리면 다음 글자의 검색·상태 조회가 모두 뒤에서 기다린다. (`app`이 어떤 객체로 연결되는지 끝까지는 미확인, 잠금 자체는 확인) | — |
| 파일명 검색 | `timemachine_service.py:675-677` `file_search` → `content_search.py:2064-2082` `search_files` | 주석은 "in-memory cache"지만 **매번 `content_meta` 전체를 SQL로 읽어** Python에서 `casefold` 부분 일치. 대상이 **본문 색인된 문서만**(타임라인 `files` 표가 아님). 화면이 보낸 `kinds`·`roots`는 서비스에서 쓰지 않음(설정의 roots 사용) | 공백으로 나눈 낱말 모두 포함(AND), 순서 무관 |
| 타임라인 검색 | `timemachine_service.py:719-734` `range` → `docfit_core/timemachine/store.py:163-193` | `lower(name) LIKE '%질의 전체%'` 하나. 현재 기간 `[start,end)` 안에서만, 200개씩 | **없음**: "예산 집행"은 공백까지 그대로 한 덩어리로 찾는다 → 이름이 `예산_집행계획.hwp`면 못 찾음 |
| 본문검색(일반) | `timemachine_service.py:649-655` `content_search` → `content_search.py:1742` `keyword_search` | FTS5 `trigram` 문단 표(`:730-759`) MATCH(3글자 이상) 또는 `LIKE`(3글자 미만, `:1764-1772`). 2글자는 `content_short_grams` 조각 표로 후보만 좁힘(`:761-769`, `:1786-1800`). 그 뒤 **후보 문서의 모든 문단을 Python으로 읽어 합친다**(`:1873-1881`). 순위 목록은 120초 보관(`:74`, `:1773-1779`) | 한 행(문단) 안에 모든 낱말(AND, `:1764-1772,1819-1824`) — 사실상 **문단 단위 동시 출현**. `"구문"`, `-제외`, `ext:`, `path:`, `after:`/`before:`, `~N` 근접(`parse_query` `:963-985`) |
| 위계검색 | (관련 문서 화면 등) `content_search.py:1277` `search` | 낱말별 후보 문서 교집합(`:1286-1316`) → SQL 집계(`:1397-1459`) → 직접 일치 없는 문서는 조상 제목까지 보고 일치(`:1537-1570`) | **문서 단위 AND**. 같은 문단에 모두 있으면 `direct_match`로 위 |
| Rust 본문 색인기 | `timemachine_service.py:95-115` `_run_klib_index` → `klib/src/index_content.rs:52-53` | Python과 같은 FTS5 `trigram` 표에 쓴다. **검색은 Rust가 하지 않는다**(Python SQLite) | — |
| Rust 폴더 열거 | `klib/src/tm_scan.rs:14-30` | `read_dir` 재귀 열거(일회성). 메모리 상주 색인 없음 | — |
| 변경 감시 | `kagent/src/watch.rs:1-2,98`, `kagent/src/main.rs:289-324` | KAgent(상주)가 타임라인 폴더를 `ReadDirectoryChangesW`로 감시, 알림을 400ms 모아 증분 스캔 예약. 폴링 없음 | — |
| 참고: klib의 다른 검색 | `klib/src/search.rs:396` | 자료실·노트북용 tantivy 색인(한국어 2글자 조각 `KoBigram`)이 이미 있다. 타임머신 본문검색에는 쓰지 않는다 | — |

### 실측(이번 조사, 1회 실행 기준)

측정 스크립트: `E:/docfit-tmp/is1_bench/bench.py`(작업 폴더 밖, 커밋 안 함). 합성 자료: 본문 3,000문서 × 40문단(낱말 24개로만 만든 문장 → 모든 낱말이 "흔한 말"인 최악 조건), 타임라인 `files` 10만 행. 같은 질의를 반복해 중앙값(ms), 괄호는 결과 수. 첫 실행과 둘째 실행 값이 다르다(파일명 18~30ms 등). 조각 표(`content_short_grams`)는 비워 두었으므로 2글자 본문은 `LIKE` 경로다.

| 경로 | 1글자 | 2글자 | 2단어 | 3단어 | 비고 |
|---|---|---|---|---|---|
| 파일명 `search_files`(3,000행) | 18.5 (238) | 19.7 (238) | 21.1 (8) | — | 없는 말 11.0 |
| 타임라인 `Store.range`(10만 행) | 144.0 (200) | 143.8 (200) | 118.2 (183) | — | 2단어는 이름에 공백이 있는 합성 자료라 걸린 것. 실제 이름(`_`·붙여쓰기)에서는 0건이 될 수 있음 |
| 본문 `keyword_search` 첫 질의(순위 보관 비움) | — | **5,013.7** (100) | **6,344.5** (100) | **5,823.4** (100) | 3글자 "예산을" 2,049.9. 드문 말 0.1 |
| 본문 `keyword_search` 같은 질의 반복(순위 보관 적중) | — | 90.2 | 145.2 | 124.3 | 실사용에서 한 글자씩 칠 때는 질의가 매번 달라 적중하지 않는다 |
| 위계검색 `search` | — | 1,364.8 | 2,381.5 | 3,485.0 | 3글자 434.9 |
| 참고: Python 리스트 10만 이름 2낱말 부분 일치 | — | — | 6.9 (264) | — | 메모리에 두기만 해도 SQL 전체 읽기보다 20배 빠름 |

해석:
- 본문검색 첫 질의 2~6초는 TODO.md의 "후보가 전체 문서인 본문검색이 5초대"와 같은 현상이다. 원인은 후보 문서의 모든 문단을 Python으로 읽는 구조(`content_search.py:1873-1881`)로 보인다(프로파일은 이번에 안 함 → 계획 S0에서 측정).
- 기존 측정: 위계검색 중앙값 118 → 133ms(300문서 × 40문단, `docs/HS2_J_LATENCY_2026-11-12.md`). 이번 3,000문서에서는 0.4~3.5초로 문서 수에 따라 크게 늘었다.
- 파일명·타임라인의 계산 자체는 수십~150ms라 "Everything급"이 아닌 주원인은 계산보다 ① 300ms 디바운스 ② 검색 전 상태 조회 왕복 ③ IPC 전역 잠금 뒤 대기 ④ 매번 SQL 전체 읽기다.
- 측정하지 않은 것: 실제 사용자 폴더(파일 수·이름 분포), 화면 그리기 시간, IPC 왕복 시간, `klib tm_scan` 열거 속도, 색인 열 때 0.8초(TODO의 `purge_excluded` 추정).

## (다) "띄어쓰기와 상관없이 여러 단어가 함께 있는 문장"의 정의 후보

사용자 말을 두 가지로 나눠 읽을 수 있다. 둘 다 필요하다고 보고 각각 후보를 둔다.

**(다-1) 낱말 하나 안의 띄어쓰기 무시** — "예산집행"을 치면 "예산 집행"도, "예산 집행"을 치면 "예산집행"도 찾는가.
- W0 지금: 공백은 낱말 구분자. "예산집행"은 붙어 있는 곳만 찾는다.
- W1 공백 제거 비교: 질의 낱말과 본문 양쪽의 공백을 지우고 부분 일치. 본문 쪽은 색인에 "공백 없는 본문" 열이 따로 필요(저장량 증가, 계획에서 측정).
- W2 질의만 쪼개기: 붙여 친 낱말은 그대로 두고, 띄어 친 여러 낱말은 (다-2)의 규칙으로 처리. 색인 변경 없음.

**(다-2) 여러 낱말이 "함께 있다"의 범위**

| 후보 | 범위 | 장점 | 단점 |
|---|---|---|---|
| R1 문장 | 마침표·물음표·느낌표·줄바꿈, `다.`로 끝나는 곳에서 자른 문장 | 사용자 말("문장")과 가장 가깝다 | 문장 경계가 색인에 없다. 개조식 공문서는 마침표 없이 끝나는 줄이 많다. 표 칸은 문장이 아님 |
| R2 문단 | 지금 색인 단위(문단·표 칸 한 행) | 지금 `keyword_search`가 이미 이렇게 한다. 색인 변경 없음 | 긴 문단이면 멀리 떨어진 낱말도 걸림 |
| R3 글자 창 | 낱말들이 N글자(예: 50) 안에 모두 있음 | 문장 경계 없이도 "가까이 있음"을 표현 | N을 정해야 함. 지금 `~N`은 낱말 수 창(`content_search.py:109-113`) |
| R4 문서 | 문서 어딘가에 모두 | 재현율 최고 | "문장 중심"이 아님. 지금 위계검색 방식 |

- 순서: 순서 무관(O1, Everything의 공백 AND와 같음)을 기본으로, 순서까지 맞으면 순위를 올리는 방식(O2)이 자연스럽다. 순서를 강제하려면 지금도 `"구문"`이 있다.
- 한국어 조사: 지금 방식(부분 문자열 일치, trigram)은 "예산을·예산의·예산에서"를 이미 "예산"으로 찾는다. 형태소 분석기는 오프라인 원칙·배포 부담 때문에 쓰지 않는 것이 기존 결정(`docs/TM_SEARCH_DESIGN.md` 3번)과 맞다. 부작용: "예산"이 "추경예산"·"예산서"에도 걸린다(지금도 같음).
- 권고(계획에 반영, 최종은 사용자 결정 Q3): **순위 계층** — 같은 문장(R1)에 모두 > 같은 문단(R2)에 모두 > 같은 문서(R4)에 모두. 걸러내지 않고 순위로 나눈다. 문장 경계는 색인을 바꾸지 않고 일치한 문단만 검색할 때 잘라 본다(일치 문단 수만큼만 계산). 단, 이미 사용자가 확정한 위계 정렬 규칙(`docs/TM_SEARCH_DESIGN.md` 30-37줄: 가장 높은 위계 > 언급 횟수 > 날짜 > bm25)과 어떤 순서로 합칠지는 사용자 결정이 필요하다(Q4).

## (라) 정규식 검색을 사용자에게 줄지와 위험

- Everything은 `regex:`로 PCRE를 준다(확인). PCRE·Python `re`는 되돌아가기(backtracking) 방식이라 `(a+)+$` 같은 식에서 입력 길이에 지수로 느려질 수 있다(일반 지식, 이 조사에서 측정 안 함).
- 위험: ① 본문 수천 문서 × 수십 문단에 걸면 느린 식 하나로 수십 초 이상 멈춤 ② 지금 IPC 전역 잠금(`ipc_server.py:67`) 때문에 앱 전체가 함께 멈춤 ③ 색인(trigram)으로 후보를 좁힐 수 없어 전체 훑기 ④ 공무원 사용자 대부분에게 문법이 낯섦.
- 완화 선택지: Rust `regex` 크레이트는 되돌아가기 없는 방식이라 입력 길이에 선형 시간을 보장한다고 공식 문서가 밝힌다(**이번에 출처를 열어 확인하지 못함** — 계획 S7 착수 전 확인). 정규식은 파일명(메모리 색인)에만 먼저 주고, 본문은 다른 낱말로 후보를 좁힌 뒤 그 안에서만, 시간 상한(예: 2초)을 두는 방식이 안전하다.
- 권고: 1차에는 정규식을 **주지 않는다**. "띄어쓰기 무관 여러 단어"는 정규식 없이 (다)의 규칙으로 해결한다. 정규식은 `re:` 접두를 쓴 파일명 검색부터 별도 단계(계획 S7)로, 사용자 결정(Q6) 뒤에.

## (마) 확정된 결정과 확인 필요

| 구분 | 내용 | 근거 |
|---|---|---|
| 확정 | 오프라인(폐쇄망) — 외부 서비스·모델 없음 | `docs/TM_SEARCH_DESIGN.md` 3줄 |
| 확정 | 한국어는 형태소 분석 없이 부분 일치(trigram + 2글자 보조) | 같은 문서 8줄 |
| 확정 | 본문검색 결과에 파일명·폴더·스니펫·키워드별 횟수 | 같은 문서 20줄 |
| 확정 | 위계 정렬 규칙(가장 높은 위계 > 언급 횟수 > 최근 날짜 > bm25) | 같은 문서 30-37줄 |
| 확정 | 목표: 본문 1초 이내(수천 문서), 파일명 100ms 이내 | 같은 문서 12줄(2026-11-03) |
| 확정 | Rust 전환은 측정상 큰 몫·계산량 원인일 때만, 한/글 COM은 Python | `docs/TEAM_OPERATING_RULES.md` 61줄 |
| 확정(사용자 지시 2026-11-12) | 실시간(한 글자마다), 다중 단어(띄어쓰기 무관), 타임라인·파일명·본문 모두, Rust 기반 최고 속도 | 이번 지시 |
| 확인 필요 | 관리자 권한 없이 동작해야 하는가(MFT·USN 사용 가능 여부) | 계획 Q1 |
| 확인 필요 | 대상: 조사 폴더만인가, 드라이브 전체인가 | 계획 Q2 |
| 확인 필요 | "함께 있는 문장"의 범위·순위 계층, 위계 정렬과의 순서 | 계획 Q3·Q4 |
| 확인 필요 | 붙여 친 낱말의 띄어쓰기 무시(W1) 여부 | 계획 Q5 |
| 확인 필요 | 정규식 제공 여부 | 계획 Q6 |
| 확인 못 함 | Everything의 MFT 사용(공개 쪽에 낱말 없음), 검색 알고리즘, 비 NTFS 감시 방식, 단어 순서 영향 | (가) 표 |
| 미측정 | 실제 사용자 폴더 규모, 화면 그리기·IPC 왕복 시간, 색인 열기 0.8초 | (나) |
