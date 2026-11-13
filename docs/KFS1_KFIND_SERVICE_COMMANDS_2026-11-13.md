# KFS1 kfind 서비스 명령 확장 기록 (2026-11-13)

작업 폴더 `E:/docfit-tmp/isearch`, 브랜치 `feature/instant-search`(시작 `e3e20540`, KFC1·KFC2·SEN1·T4 병합 뒤). 설계 근거: `docs/APP1_KFIND_INTEGRATION_2026-11-13.md` §2-2·§3-3·§7 KFS1 행, 결정 Q-A1~Q-A13(모두 권고안 확정).

## 1. 한 일(커밋 단위)

| 커밋 | 내용 |
|---|---|
| KFS1-1 `879facaa` | kfind-core `index_sentences`·`is_indexable`: 비교형이 빈 문장(기호만)은 색인에서 뺌. 나누기 규칙 판 1과 대조 표는 그대로. Python 기준 `sentence_ref.index_spans` |
| KFS1-2 `d83f641f` | 서비스 문서 표 `crates/kfind/src/doctable.rs` |
| KFS1-3 `b4d6276e` | `ingest` 조각 모으기·문단→색인 문장 `crates/kfind/src/ingest.rs` |
| KFS1-4 `05328989` | 파이프 명령 `ingest`·`remove`·`stale`·`count`, `query` 확장, `status` 확장 |
| KFS1-5 `2c158889` | 실행 파일 `--user` 실제 파이프 왕복 시험 |
| KFS1-6 | rustfmt(T4 크레이트만, 코어는 원래 rustfmt를 쓰지 않음)와 이 기록 |

## 2. 파이프 계약(T4 형식 그대로: u32 LE 길이 + UTF-8 JSON, 한 연결에 요청 1개, 요청 64KB 상한)

| 명령 | 요청 | 결과(`{"ok":true,"result":…}`) |
|---|---|---|
| `query` | `id, text, top(1..1000, 기본 20), scope?, sources?, refs(0..100, 기본 3), split?(dict·bigram·off), filters?` | `id, generation, total_docs, total_sentences, adjacent_docs, words[{text,pieces}], dropped, ms, hits[{doc_id, doc_key, path, count, level, adjacent, date, sentences[{ord, para, level, adjacent, start, len}]}]` |
| `count` | `id, text, scope?, sources?, split?, filters?` | `id, generation, total_docs, total_sentences, words, dropped, ms` |
| `ingest` | `doc_key, path, size, mtime, date(밀리초), ref_date?, scopes[], level_src(py·rs), level_rule, paras[{p, lv(0..8), t}], part?, parts?` | 조각 대기: `{doc_key, indexed:false, received, parts}` / 넣음: `{doc_key, indexed:true, doc_id, sentences, generation}` |
| `remove` | `doc_keys[≤1000]` | `{removed, missing, generation?}` |
| `stale` | `level_rule, scope?, after?, limit(1..10000, 기본 1000)` | `{doc_keys[], more}` — 열쇠 순서, 다음 쪽은 `after`=마지막 열쇠 |
| `status` | `{}` | 기존 `state, version, sid, index_available` + `mode(service·user), generation, docs, sentences, format, fold_rule, sentence_rule, level_rule_counts{py4, rs…}, uploads, last_error` |
| `cancel` | `id` | 그대로(`query`·`count` 모두 취소됨) |

- `filters`: `ext[]`(앞 점 무시)·`path[]`(모두 포함, 대소문자 무시)·`after`·`before`(기준 날짜 글 비교, 날짜 없는 문서 제외)·`exclude[]`(엔진 `doc_set` → `deny`)·`phrases[]`(KFC2 구문). Python `content_search.parse_query`/`:1384~1392`와 같은 뜻. 알 수 없는 거르개 필드는 거절.
- `scope`: 문서 범위 `d`가 `scope`와 같거나 `scope + ":"`로 시작하면 포함(`nb:5` ⊃ `nb:5:src:7`, `nb:50`은 아님). `sources`가 있으면 `scope:src:<s>`들만. 범위나 표 거르개가 있으면 문서 표로 KFC1 `allow`를 만든다. 둘 다 없으면 거르지 않는다(T4 기존 동작).
- 문장 위치 `start·len`: 코어 형식(판 3)에 원문 위치가 없어서, 서비스가 문장을 나눌 때 문서 표에 `ord → (문단, 시작, 길이)`를 적어 두고 응답에 붙인다. 코드포인트 단위(Python 문자열 색인과 같음). 코어 `SentenceRef`와 형식 판은 그대로다(KFC3을 코어에서 하면 형식 판을 올려야 함). 문서 표에 없는 문서(표 밖에서 넣은 색인)는 `doc_key·path·start·len`이 null.
- 파이프의 `query`는 `Request::Find(QueryArgs)`로 읽는다. T4의 `Request::Query { id, text, top }`는 내부·시험용 간단 형으로 남겼다(기존 시험 그대로).

## 3. 상태와 전이(드러낸 암묵적 상태)

**문서 표(사용자 SID마다 `<색인 폴더>/<SID>/doctable.jsonl`)**: 줄마다 `put`·`del`·`next`. 순서는 색인 커밋 → 표 기록. 사이에서 꺼지면 색인에만 있는 문서가 남지만 범위 질의는 표로 거르므로 보이지 않고, 같은 ID를 다시 받으면 덮인다. 끊긴 마지막 줄은 버리고 다시 쓴다. 중간 줄이 깨지면 오류. 기록 줄이 살아 있는 문서의 2배 + 1,000을 넘으면 임시 파일 → rename으로 줄인다. 지운 문서의 ID는 다시 쓰지 않는다.

**위계 출처 전이(Q-A2)**: `py`는 무엇이든 덮는다. `rs`는 `rs`를 덮고, 크기나 수정 시각이 다른(파일이 바뀐) `py`를 덮는다. 같은 크기·수정 시각의 `py`를 `rs`가 덮으려 하면 `rejected: …`로 거절하고 색인·표를 바꾸지 않는다. 첫 조각에서도 확인한다.

**조각 모으기(Q-A7, SID·doc_key마다)**: 없음 → part 0 → 모으는 중(next) → 마지막 조각 → 넣기 → 없음. 거절하고 버리는 경우: part 0 없는 조각, 순서가 틀린 조각, 머리(경로·크기·수정 시각·날짜·범위·위계 출처·판)나 `parts`가 다른 조각, 64MB 초과, 사용자당 8건·전체 64건 초과, 60초 동안 다음 조각 없음. part 0이 다시 오면 처음부터. 이어진 항목의 `p`가 같으면 한 문단의 이어진 글로 합친다(64KB 넘는 문단). 이때 `lv`도 같아야 한다.

**쓰기 쪽(Writer)**: 처음 쓰는 요청에서 연다. 질의만 하면 열지 않는다(다른 쓰기 쪽이 잡고 있어도 질의는 된다). 쓰기마다 커밋하고, 조각이 8개를 넘으면 그 자리에서 합친다. 쓰기에 실패하면 Writer를 버리고 다음 쓰기에서 다시 연다(`last_error`에 기록). 형식·비교형 판이 다르면 색인과 문서 표를 비우고 새로 만든다(`last_error: rebuilt index: …`). 앱은 `status.docs`가 0이 된 것을 보고 다시 채운다.

**잠금 순서**: 질의는 색인 → 문서 표 순서로 잠근다. 쓰기는 문서 표만 잠근다. 반대 순서는 없다.

## 4. 시험(직접 실행, `CARGO_TARGET_DIR=F:/cargo-target/isearch`, GNU 도구 체인)

- 병합 직후 기준선: kfind-core **56건**, kfind **18건**(단위 17 + 통합 1) 통과.
- 이번 뒤: kfind-core **58건**(+ `symbol_only_sentences_are_not_indexed`, `index_sentences_match_python_counts`: 대조 표 200문단에서 문장 478 → 색인 475, Python `index_spans`와 같은 수). kfind **33건**(단위 31 = 기존 17 + 문서 표 5 + 조각 4 + 핸들러 5, 통합 2 = 기존 1 + 새 1). 실패 0.
- 실제 파이프 왕복(`executable_user_mode_ingest_roundtrip`): 빈 색인 폴더로 띄운 `kfind.exe --user`에 약 120KB 문서(문단 300개)를 넣는다. 나누지 않은 요청은 연결이 닫히고, 4조각으로 나누면 마지막 조각에서 색인된다. 이어서 범위 질의의 path·문단·start·len, count, stale, rs→py 덮기 거절, remove 뒤 결과, status를 확인한다. 4회 연속 통과(0.28~0.42초).
- 기존 시험은 고치지 않았다(`git diff e3e20540 -- crates/kfind/tests/user_mode.rs`에서 지운 줄 0).
- 하지 않은 것: Python 전체 시험(Python 코드 변경 없음, `sentence_ref.py`에 함수 하나만 더함), 실제 SCM 서비스 모드, 속도 측정.

## 5. 남은 문제

1. **문장 위치를 메모리에 둔다**: 문서 표 전체(문장마다 u32 3개)를 사용자마다 메모리에 올린다. 문장 500만 개면 약 60MB(추정, 미측정). 실제 문서 사본으로 크기를 재고, 크면 표를 파일 매핑이나 SQLite로 바꾼다.
2. **쓰기마다 커밋**: `ingest` 1건 = 커밋 1번(+ 필요하면 합치기). seeding 때 수천 건이면 느릴 수 있다(미측정). 묶음 커밋(`queued`)은 APP1-E 측정 뒤에 결정한다.
3. **범위만 바꾸는 명령 없음**: APP1 §4-3 `LibraryKfindSync`의 "조사 폴더 안 파일에 `box:<id>`만 더함"은 지금은 같은 문서를 범위를 바꿔 다시 `ingest`해야 한다. `scopes {doc_key, add, remove}` 명령이 필요하다(카드 범위 밖).
4. `query`의 `top=0`(전체 순위, `mentions` 정렬용)은 T4 제한(1..1000)을 그대로 두었다. APP1-D에서 필요하면 열어야 한다.
5. 색인에만 있고 표에 없는 문서(커밋과 표 기록 사이에 꺼진 경우)를 지우는 정리 작업이 없다. 코어 Writer에 문서 ID 목록을 내는 API가 없다. 범위 질의에는 보이지 않는다.
6. 형식 판이 바뀌어 다시 만든 사실은 `status.last_error`로만 알린다. APP1 §3-4 코퍼스 상태(`rebuilding` 등)를 `status`에 명시적 값으로 두는 것은 APP1-I와 함께 정한다.
7. `path` 거르개의 대소문자 무시는 Rust `to_lowercase`이고 Python은 `casefold`다(ß 같은 일부 글자가 다름).
