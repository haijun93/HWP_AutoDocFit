# IS-S0 검색 병목 프로파일 (2026-11-12)

작업: `feature/instant-search`. 검색 구현과 기존 시험은 수정하지 않았다. 측정 도구는 설계 에이전트의 `bench.py`를 `scripts/benchmark_is1.py`로 옮겨 확장했다. 기준 원자료는 [JSON](IS_S0_PROFILE_RAW_2026-11-12.json)에 모든 호출 표본·함수별 시간·실제 SQL·EXPLAIN QUERY PLAN을 보관했다.

## 조건과 측정 상태

- Windows 11 10.0.26300, Python 3.14.0 64비트, SQLite 3.50.4. 합성 3,000문서 × 40문단 = 120,000문단, 타임라인 100,000행. seed=7, 낱말 24개와 조사로 만든 문장 12어절. 모든 문단은 body/level_rank=8이다.
- 조사 스크립트와 동일하게 short_grams는 비어 있다. 2글자는 LIKE 대체 경로다. 실제 운영 색인에서 조각 표가 완비된 경우의 측정으로 일반화하면 안 된다. 본문 자료·파일명 분포는 원본 스크립트와 동일하다. 타임라인 경로는 합성 임시 폴더 아래 일련번호로 바꾸어 난수 소비 순서가 달라졌으므로 결과 수는 기존 183에서 186으로 달라졌다. 검색 동작 변경이 아니다.
- 상태: 합성 DB 작성 → 채널·질의마다 새 프로세스 → DB 열기 → 첫 질의 1회 → 같은 질의 반복 3회 → 연결 닫기 → 새 연결에서 진단 1회. 4채널 × 7질의 = 28조합, 일반 측정 112호출과 진단 28호출. DB 생성 64.27초.
- 디스크 캐시를 비우지 않았다. 첫 질의는 **새 프로세스 첫 질의**이며 OS 캐시가 차가운 상태라는 뜻이 아니다. 같은 DB를 순서대로 사용한다. 본문 순위 캐시는 새 객체에서 비어 시작하고 반복에서는 적중한다. 생성자 시간은 별도로 잰다. 임시 DB는 작업 폴더 안에서 생성하고 자동 삭제했다(작업 폴더 밖 수정 금지 준수).
- 시간 표는 계측하지 않은 perf_counter 값이다. 진단 호출만 cProfile과 SQL 프록시를 함께 썼다. SQL 시간은 execute + fetchall/fetchone/iteration 소비 시간이며 EXPLAIN 시간은 제외한다. 함수 시간에는 프록시·EXPLAIN 오버헤드가 들어간다. 진단 합계를 일반 측정 시간에서 빼거나 비율을 곧바로 적용하지 않는다. 함수 누적 시간은 서로 중첩된다.
- 반복 3개 p95는 nearest-rank, 즉 최댓값이다. 안정적인 화면 p95나 부하 분포 추정으로 사용할 수 없다. 모든 조합에서 첫/반복 결과의 정렬된 JSON SHA-256이 일치했다(28/28).

재실행(저장소 루트, Python 경로는 실행 환경에 맞춰 지정):

```powershell
python -X utf8 scripts/benchmark_is1.py --docs 3000 --paragraphs 40 --timeline 100000 --reps 3 --temp-dir . --output docs/IS_S0_PROFILE_RAW_2026-11-12.json
python -X utf8 -m unittest tests.test_benchmark_is1 tests.test_timemachine_content_search tests.test_timemachine_core
```

`--docs`, `--paragraphs`, `--timeline`, `--reps`, `--temp-dir`로 규모와 반복을 조절한다. `--rust-bench <exe>`는 같은 도구에서 `<exe> --bench`의 표준 출력을 JSON에 보관하는 진입점이다. 현재 Rust 검색 엔진이 없어 실행·성능 검증하지 않았다. 1글자 본문은 현행 동작의 기준선만 기록하며 Q8 이후 허용 규칙을 구현한 것이 아니다. 붙여쓰기 질의도 현행 0건을 그대로 기록한다.

## 병목 상위 5개와 근거

1. **본문 후보 문단을 Python에서 다시 검사·순위 계산** — `keyword_search`의 2낱말 진단 누적 7,187.5ms, 자체 1,516.6ms. str.casefold 자체 1,306.8ms, sum 누적 1,210.9ms, `_min_position_span` 누적 203.9ms. `scored_rows`의 횟수 계산 뒤 `paragraph_rows_by_path`·`candidate_texts`를 다시 만들고 유효 문단·근접도를 검사한다. SQL만 빨라져도 이 반복 계산은 남는다.
2. **위계 전체 문단 SQL 집계와 정렬** — `search('예산 집행')`의 단일 집계 SQL 2,104.9ms(진단 총 2,877.7ms). `SCAN ... VIRTUAL TABLE INDEX 0:`, `USE TEMP B-TREE FOR GROUP BY`, `... ORDER BY`. `lower`/`instr`/`replace`/`length`로 낱말별 언급 횟수를 집계한다. 3낱말에서 더 늘어난다. 조상 계산 `_breadcrumb_rows`는 36.0ms로 작다. “질의 시 위계 경로 계산이 주원인”이라는 기존 추정은 이 합성 자료에서는 지지되지 않는다.
3. **본문 후보 원문 두 번 전수 읽기** — `_select_paragraphs_for_paths` 2낱말 누적 1,296.5ms. 점수용 `SELECT path,name,level_rank,body,0.0 AS rank ...` 660.6ms와 검증용 `SELECT path,paragraph_index,body,level_rank ...` 524.1ms. 둘 다 `SCAN ... VIRTUAL TABLE INDEX 64:`와 임시 경로 표 IN 인덱스를 사용한다. 후보가 거의 전 문서여서 임시 표가 원문 읽는 양을 크게 줄이지 못한다.
4. **짧은 낱말 후보 조회의 반복 LIKE** — 본문 2낱말 `SELECT path FROM content_paragraph_fts WHERE body LIKE ?` 2회 합계 623.6ms, 위계 `SELECT DISTINCT path ... LIKE ?` 2회 442.0ms. `VIRTUAL TABLE INDEX 0:L5`, 위계는 DISTINCT용 임시 B-tree도 사용. 2글자는 trigram으로 선택도를 얻지 못하며 조각 표도 비어 있다. 반면 3글자 ‘예산을’은 본문 첫 1,734.2ms, 위계 첫 433.1ms로 줄었다.
5. **타임라인 root 후보 전체에 이름 LIKE 적용 후 정렬** — `Store.range('예산')` 진단 총 159.0ms 중 SQL execute+소비 157.8ms. `SEARCH files USING INDEX files_root (root=?)`와 `USE TEMP B-TREE FOR ORDER BY`. 시간/이름/확장자 필터와 LIMIT 200이 있어도 해당 root 10만 행의 이름 조건과 정렬 비용이 남는다. 필터 생성·Python 결과 변환은 각각 1ms 미만이다.

상위 5개는 서로 다른 경로의 개선 후보를 정리한 것이며 중첩 시간을 더한 순위가 아니다. 생성자는 본문 DB를 여는 데 대체로 388.9~610.8ms가 추가된다. 전체 첫 프로세스는 DB 열기 2,575.5ms였다. 생성자 내부 함수·SQL은 이번 진단 범위 밖이므로 그 시간을 purge_excluded 하나에 귀속하지 않는다. JOIN이 언제나 주원인이라는 근거도 없다. 3글자 위계 경로의 CTE/JOIN 계획은 원자료에 별도로 남겼다.

## 기준선 재현과 목표 지연

- 본문 2글자 첫 4,488.6ms는 조사 5,013.7ms 대비 -10.5%, 2낱말 첫 5,389.8ms는 조사 6,344.5ms 대비 -15.0%, 3낱말 첫 4,554.5ms는 조사 5,823.4ms 대비 -21.8%로 모두 ±30% 안이다.
- 타임라인 10만 행 ‘예산’ 반복 중앙값 140.3ms는 조사 143.8ms 대비 -2.4%로 ±30% 안이다. 기준선 재현 조건을 만족한다.
- 위계 2/3낱말 반복 중앙값 2,470.3/3,205.9ms로 조사 2,381.5/3,485.0ms와 비슷하다. 순위 캐시 적중에 따른 본문 반복 81~86ms를 실시간 입력의 새 질의 성능으로 해석하면 안 된다.
- 계획 §3 목표(미달/미측정 구분): 본문 top 50ms·complete 150ms, 위계 3,000문서 150ms, 타임라인 5ms에는 현재 기준선이 못 미친다. 화면 목표 100/250/40ms, 100만·200만 파일명 10/20ms, 취소 10ms, 프로세스 재시작 300ms는 이번 측정 대상이 아니다. 본문 3만 문서·화면·IPC·COM·실제 사용자 자료도 확인하지 않았다.

## 실패한 초기 점검과 제한

초기 300문서 점검에서 PowerShell→Python 파이프의 한글 리터럴이 `?`로 바뀌어 측정이 무효였고, Store의 연결 속성을 db로 가정하여 실패했다. 질의 리터럴을 Unicode escape로 바꾸고 `_db`를 사용하도록 고쳤다. 이 무효 실행은 기준선 표에 섞지 않았다. 실제 기준 규모 측정은 위 수정 후 1회 실행했으며 측정 중 시험·빌드를 실행하지 않았다. SQL 프록시는 검색 구현에 삽입하지 않고 진단 객체의 연결만 감쌌다.

실제 사용자 문서와 한/글 COM은 사용하지 않았다. 최적화·스키마 변경·네트워크·푸시·태그는 하지 않았다. 실제 위계 제목이 다양한 문서에서는 조상 계산 비용이 달라질 수 있다. 진단은 같은 프로세스에서 새 연결을 연 첫 질의이므로 새 프로세스·차가운 디스크의 함수 분해와 동일하지 않다.

## 전체 지연 표

단위 ms. DB 열기와 질의는 별도. 결과 수는 기본 limit에 의해 잘린 수이며 전체 일치 문서 수가 아니다.

| channel | query | open | first | repeat median | repeat p95 (n=3) | count |
|---|---|---:|---:|---:|---:|---:|
| search_files | 예 | 2575.5 | 14.4 | 11.3 | 11.3 | 238 |
| search_files | 예산 | 521.8 | 30.4 | 16.4 | 23.6 | 238 |
| search_files | 예산을 | 543.6 | 12.9 | 12.6 | 21.8 | 0 |
| search_files | 예산 집행 | 471.8 | 12.7 | 17.1 | 25.6 | 8 |
| search_files | 예산 집행 계획 | 406.5 | 13.5 | 13.0 | 15.4 | 0 |
| search_files | 예산집행 | 407.4 | 9.9 | 8.9 | 10.1 | 0 |
| search_files | 가나다라 | 440.7 | 15.6 | 9.0 | 9.7 | 0 |
| keyword_search | 예 | 408.5 | 3627.0 | 74.0 | 88.9 | 100 |
| keyword_search | 예산 | 469.9 | 4488.6 | 81.0 | 84.7 | 100 |
| keyword_search | 예산을 | 427.6 | 1734.2 | 126.9 | 128.5 | 100 |
| keyword_search | 예산 집행 | 605.3 | 5389.8 | 84.1 | 118.9 | 100 |
| keyword_search | 예산 집행 계획 | 610.8 | 4554.5 | 85.8 | 93.6 | 100 |
| keyword_search | 예산집행 | 413.6 | 1.4 | 0.1 | 0.2 | 0 |
| keyword_search | 가나다라 | 399.5 | 1.0 | 0.1 | 0.1 | 0 |
| search | 예 | 394.1 | 1153.7 | 1056.8 | 1303.6 | 100 |
| search | 예산 | 506.1 | 1261.4 | 1648.1 | 2062.9 | 100 |
| search | 예산을 | 388.9 | 433.1 | 370.0 | 379.5 | 100 |
| search | 예산 집행 | 422.8 | 2541.0 | 2470.3 | 2683.8 | 100 |
| search | 예산 집행 계획 | 554.1 | 3224.6 | 3205.9 | 3620.5 | 100 |
| search | 예산집행 | 396.0 | 0.8 | 0.1 | 0.1 | 0 |
| search | 가나다라 | 459.2 | 1.3 | 0.2 | 0.2 | 0 |
| timeline | 예 | 3.1 | 142.6 | 142.0 | 204.9 | 200 |
| timeline | 예산 | 1.8 | 170.9 | 140.3 | 155.6 | 200 |
| timeline | 예산을 | 2.4 | 133.9 | 136.4 | 139.6 | 0 |
| timeline | 예산 집행 | 1.8 | 114.2 | 114.4 | 139.4 | 186 |
| timeline | 예산 집행 계획 | 2.5 | 154.5 | 117.5 | 119.4 | 0 |
| timeline | 예산집행 | 1.8 | 115.3 | 127.8 | 133.8 | 0 |
| timeline | 가나다라 | 2.1 | 117.8 | 118.0 | 140.3 | 0 |

## cProfile / SQL diagnostic detail


### keyword_search / 예산

| function | line | calls | self ms | cumulative ms |
|---|---:|---:|---:|---:|
| `query` | 128 | 1 | 38.6 | 7136.8 |
| `keyword_search` | 1742 | 1 | 1934.1 | 7098.2 |
| `_call` | 50 | 47995 | 86.9 | 1749.0 |
| `_select_paragraphs_for_paths` | 1689 | 4 | 0.2 | 1437.1 |
| `fetchall` | 60 | 5 | 0.0 | 1332.9 |
| `<method 'fetchall' of 'sqlite3.Cursor' objects>` | 0 | 5 | 1311.0 | 1311.0 |
| `<method 'casefold' of 'str' objects>` | 0 | 2142300 | 1031.4 | 1031.4 |
| `<built-in method builtins.sum>` | 0 | 240000 | 195.9 | 679.7 |
| `__next__` | 66 | 47977 | 32.8 | 448.9 |
| `_min_position_span` | 79 | 47975 | 227.6 | 375.5 |
| `<genexpr>` | 1836 | 240000 | 161.4 | 363.3 |
| `<built-in method builtins.next>` | 0 | 48277 | 327.8 | 328.5 |

SQL (1 calls, 823.3 ms):

```sql
SELECT path,name,level_rank,body,0.0 AS rank FROM content_paragraph_fts WHERE path IN (SELECT path FROM temp.keyword_path_filter) ORDER BY rowid
```

```text
[3, 0, 199, 'SCAN content_paragraph_fts VIRTUAL TABLE INDEX 64:']
[8, 0, 0, 'USING INDEX sqlite_autoindex_keyword_path_filter_1 FOR IN-OPERATOR']
```

SQL (1 calls, 487.7 ms):

```sql
SELECT path,paragraph_index,body,level_rank FROM content_paragraph_fts WHERE path IN (SELECT path FROM temp.keyword_path_filter) ORDER BY rowid
```

```text
[3, 0, 199, 'SCAN content_paragraph_fts VIRTUAL TABLE INDEX 64:']
[8, 0, 0, 'USING INDEX sqlite_autoindex_keyword_path_filter_1 FOR IN-OPERATOR']
```

SQL (1 calls, 372.0 ms):

```sql
SELECT path FROM content_paragraph_fts WHERE body LIKE ?
```

```text
[2, 0, 132, 'SCAN content_paragraph_fts VIRTUAL TABLE INDEX 0:L5']
```

### keyword_search / 예산을

| function | line | calls | self ms | cumulative ms |
|---|---:|---:|---:|---:|
| `query` | 128 | 1 | 33.3 | 2526.8 |
| `keyword_search` | 1742 | 1 | 464.4 | 2493.4 |
| `_call` | 50 | 14 | 8.4 | 552.2 |
| `fetchall` | 60 | 5 | 0.0 | 552.1 |
| `<method 'fetchall' of 'sqlite3.Cursor' objects>` | 0 | 5 | 543.7 | 543.7 |
| `_select_paragraphs_for_paths` | 1689 | 2 | 0.1 | 529.3 |
| `<method 'casefold' of 'str' objects>` | 0 | 575771 | 392.3 | 392.3 |
| `<genexpr>` | 1907 | 105801 | 98.9 | 234.5 |
| `search` | 174 | 2975 | 3.8 | 225.3 |
| `<genexpr>` | 1957 | 105801 | 86.0 | 221.0 |
| `<method 'search' of 're.Pattern' objects>` | 0 | 2975 | 215.2 | 215.2 |
| `execute` | 78 | 22 | 0.5 | 124.2 |

SQL (1 calls, 458.0 ms):

```sql
SELECT path,paragraph_index,body,level_rank FROM content_paragraph_fts WHERE path IN (SELECT path FROM temp.keyword_path_filter) ORDER BY rowid
```

```text
[3, 0, 199, 'SCAN content_paragraph_fts VIRTUAL TABLE INDEX 64:']
[8, 0, 0, 'USING INDEX sqlite_autoindex_keyword_path_filter_1 FOR IN-OPERATOR']
```

SQL (1 calls, 92.7 ms):

```sql
SELECT path, name, level_rank, body,
                               bm25(content_paragraph_fts, 0.0, 3.0, 2.0, 1.0, 0.0, 0.0, 1.0) AS rank
                        FROM content_paragraph_fts WHERE content_paragraph_fts MATCH ?
                        ORDER BY rank ASC
```

```text
[3, 0, 132, 'SCAN content_paragraph_fts VIRTUAL TABLE INDEX 0:M8']
[17, 0, 0, 'USE TEMP B-TREE FOR ORDER BY']
```

SQL (1 calls, 61.9 ms):

```sql
SELECT path, paragraph_index, body, location_label, section, bm25(content_paragraph_fts, 0.0, 3.0, 2.0, 1.0, 0.0, 0.0, 1.0) AS rank FROM content_paragraph_fts WHERE content_paragraph_fts MATCH ? AND path IN (SELECT path FROM temp.keyword_path_filter)
```

```text
[2, 0, 132, 'SCAN content_paragraph_fts VIRTUAL TABLE INDEX 0:M8']
[8, 0, 0, 'USING INDEX sqlite_autoindex_keyword_path_filter_1 FOR IN-OPERATOR']
```

### keyword_search / 예산 집행

| function | line | calls | self ms | cumulative ms |
|---|---:|---:|---:|---:|
| `query` | 128 | 1 | 38.3 | 7225.9 |
| `keyword_search` | 1742 | 1 | 1516.6 | 7187.5 |
| `_call` | 50 | 95956 | 134.4 | 1903.5 |
| `<method 'casefold' of 'str' objects>` | 0 | 2205299 | 1306.8 | 1306.8 |
| `_select_paragraphs_for_paths` | 1689 | 4 | 0.2 | 1296.5 |
| `<built-in method builtins.sum>` | 0 | 240000 | 267.9 | 1210.9 |
| `fetchall` | 60 | 5 | 0.0 | 1204.5 |
| `<method 'fetchall' of 'sqlite3.Cursor' objects>` | 0 | 5 | 1181.7 | 1181.7 |
| `__next__` | 66 | 95938 | 55.2 | 754.1 |
| `<genexpr>` | 1836 | 360000 | 311.0 | 715.7 |
| `<built-in method builtins.next>` | 0 | 96237 | 546.2 | 547.6 |
| `<genexpr>` | 1907 | 186210 | 174.5 | 406.3 |

SQL (1 calls, 660.6 ms):

```sql
SELECT path,name,level_rank,body,0.0 AS rank FROM content_paragraph_fts WHERE path IN (SELECT path FROM temp.keyword_path_filter) ORDER BY rowid
```

```text
[3, 0, 199, 'SCAN content_paragraph_fts VIRTUAL TABLE INDEX 64:']
[8, 0, 0, 'USING INDEX sqlite_autoindex_keyword_path_filter_1 FOR IN-OPERATOR']
```

SQL (2 calls, 623.6 ms):

```sql
SELECT path FROM content_paragraph_fts WHERE body LIKE ?
```

```text
[2, 0, 132, 'SCAN content_paragraph_fts VIRTUAL TABLE INDEX 0:L5']
```

SQL (1 calls, 524.1 ms):

```sql
SELECT path,paragraph_index,body,level_rank FROM content_paragraph_fts WHERE path IN (SELECT path FROM temp.keyword_path_filter) ORDER BY rowid
```

```text
[3, 0, 199, 'SCAN content_paragraph_fts VIRTUAL TABLE INDEX 64:']
[8, 0, 0, 'USING INDEX sqlite_autoindex_keyword_path_filter_1 FOR IN-OPERATOR']
```

### search / 예산

| function | line | calls | self ms | cumulative ms |
|---|---:|---:|---:|---:|
| `query` | 128 | 1 | 6.2 | 1688.8 |
| `search` | 1277 | 1 | 57.3 | 1682.6 |
| `execute` | 78 | 18 | 0.4 | 847.6 |
| `<method 'execute' of 'sqlite3.Connection' objects>` | 0 | 27 | 847.0 | 847.0 |
| `_call` | 50 | 3012 | 6.1 | 602.8 |
| `fetchall` | 60 | 4 | 0.0 | 387.3 |
| `<method 'fetchall' of 'sqlite3.Cursor' objects>` | 0 | 4 | 385.8 | 385.8 |
| `__next__` | 66 | 3002 | 2.4 | 217.8 |
| `<built-in method builtins.next>` | 0 | 3102 | 209.5 | 209.6 |
| `_select_paragraphs_for_paths` | 1689 | 2 | 0.1 | 64.2 |
| `_breadcrumb_rows` | 1726 | 1 | 5.9 | 47.0 |
| `<method 'sort' of 'list' objects>` | 0 | 2 | 4.6 | 31.0 |

SQL (1 calls, 1162.7 ms):

```sql
SELECT path, COUNT(*) AS matched_paragraphs,
                           MIN(CASE WHEN (instr(body_folded, lower(?)) > 0) THEN COALESCE(level_rank, 8) END) AS best_level_rank,
                           0.0 AS best_bm25,
                           MIN(CASE WHEN (instr(body_folded, lower(?)) > 0) THEN CAST(paragraph_index AS INTEGER) END) AS chosen_paragraph_index,
                           SUM(CASE WHEN instr(body_folded, lower(?)) > 0 THEN (length(body_folded) - length(replace(body_folded, lower(?), ''))) / max(length(lower(?)), 1) ELSE 0 END) AS match_count_0, MAX(CASE WHEN instr(body_folded, lower('예산')) > 0 THEN 1 ELSE 0 END) AS direct_match
                    FROM (SELECT content_paragraph_fts.*, lower(body) AS body_folded
                          FROM content_paragraph_fts)
                    WHERE (instr(body_folded, lower(?)) > 0)
                      AND path IN (SELECT path FROM temp.hierarchy_candidate_paths)
                    GROUP BY path
                    ORDER BY SUM(CASE WHEN (instr(body_folded, lower(?)) > 0) THEN 1 ELSE 0 END) DESC
                    LIMIT 100000
```

```text
[8, 0, 199, 'SCAN content_paragraph_fts VIRTUAL TABLE INDEX 0:']
[21, 0, 0, 'USING INDEX sqlite_autoindex_hierarchy_candidate_paths_1 FOR IN-OPERATOR']
[27, 0, 0, 'USE TEMP B-TREE FOR GROUP BY']
[164, 0, 0, 'USE TEMP B-TREE FOR ORDER BY']
```

SQL (1 calls, 212.6 ms):

```sql
SELECT DISTINCT path FROM content_paragraph_fts WHERE body LIKE ?
```

```text
[3, 0, 132, 'SCAN content_paragraph_fts VIRTUAL TABLE INDEX 0:L5']
[17, 0, 0, 'USE TEMP B-TREE FOR DISTINCT']
```

SQL (1 calls, 34.0 ms):

```sql
SELECT path,paragraph_index,body,level,level_rank FROM content_paragraph_fts WHERE rowid IN (SELECT para_rowid FROM content_paragraph_map WHERE path IN (SELECT path FROM temp.keyword_path_filter)) ORDER BY rowid
```

```text
[3, 0, 33, 'SCAN content_paragraph_fts VIRTUAL TABLE INDEX 64:=']
[7, 0, 0, 'LIST SUBQUERY 2']
[10, 7, 102, 'SEARCH content_paragraph_map USING PRIMARY KEY (path=?)']
[12, 7, 0, 'USING INDEX sqlite_autoindex_keyword_path_filter_1 FOR IN-OPERATOR']
[23, 7, 0, 'CREATE BLOOM FILTER']
[47, 0, 0, 'USE TEMP B-TREE FOR ORDER BY']
```

### search / 예산을

| function | line | calls | self ms | cumulative ms |
|---|---:|---:|---:|---:|
| `query` | 128 | 1 | 3.3 | 506.9 |
| `search` | 1277 | 1 | 58.2 | 503.6 |
| `execute` | 78 | 13 | 0.3 | 175.0 |
| `<method 'execute' of 'sqlite3.Connection' objects>` | 0 | 21 | 174.6 | 174.6 |
| `_call` | 50 | 2622 | 5.3 | 102.3 |
| `__next__` | 66 | 2616 | 2.0 | 75.3 |
| `<built-in method builtins.next>` | 0 | 2716 | 68.4 | 68.6 |
| `_breadcrumb_rows` | 1726 | 1 | 6.2 | 37.0 |
| `_select_paragraphs_for_paths` | 1689 | 1 | 0.0 | 29.4 |
| `fetchall` | 60 | 3 | 0.0 | 29.0 |
| `<method 'fetchall' of 'sqlite3.Cursor' objects>` | 0 | 3 | 27.5 | 27.5 |
| `<method 'sort' of 'list' objects>` | 0 | 2 | 2.9 | 26.4 |

SQL (1 calls, 164.5 ms):

```sql
WITH scored AS MATERIALIZED (
                            SELECT rowid AS paragraph_rowid, path, paragraph_index, level, level_rank, body,
                                   lower(body) AS body_folded, bm25(content_paragraph_fts) AS bm25_rank
                            FROM content_paragraph_fts
                            WHERE content_paragraph_fts MATCH ?
                              AND path IN (SELECT path FROM temp.hierarchy_candidate_paths)
                        ), matched AS (
                            SELECT * FROM scored WHERE (instr(body_folded, lower(?)) > 0)
                        ), best AS (
                            SELECT path, MIN(COALESCE(level_rank, 8)) AS best_level_rank,
                                   MIN(bm25_rank) AS best_bm25,
                                   MIN(CAST(paragraph_index AS INTEGER)) AS first_match_index
                            FROM matched GROUP BY path
                        ), chosen AS (
                            -- MIN 하나만 있는 집계에서는 SQLite가 그 최솟값 행의 다른 열을 돌려준다.
                            SELECT m.path, MIN(CAST(m.paragraph_index AS INTEGER)) AS chosen_paragraph_index,
                                   m.paragraph_rowid AS chosen_rowid
                            FROM matched m JOIN best b ON b.path=m.path AND b.best_level_rank=COALESCE(m.level_rank, 8)
                            GROUP BY m.path
                        ), counts AS (
                            SELECT path, COUNT(*) AS matched_paragraphs, SUM(CASE WHEN instr(body_folded, lower(?)) > 0 THEN (length(body_folded) - length(replace(body_folded, lower(?), ''))) / max(length(lower(?)), 1) ELSE 0 END) AS match_count_0, MAX(CASE WHEN instr(body_folded, lower('예산을')) > 0 THEN 1 ELSE 0 END) AS direct_match
                            FROM matched GROUP BY path
                        )
                        SELECT b.path, c.matched_paragraphs, b.best_level_rank, b.best_bm25,
                               ch.chosen_paragraph_index, ch.chosen_rowid, c.direct_match, c.match_count_0
                        FROM best b JOIN counts c ON c.path=b.path JOIN chosen ch ON ch.path=b.path
                        ORDER BY c.matched_paragraphs DESC LIMIT 100000
```

```text
[3, 0, 0, 'MATERIALIZE best']
[6, 3, 0, 'MATERIALIZE scored']
[9, 6, 132, 'SCAN content_paragraph_fts VIRTUAL TABLE INDEX 0:M8']
[15, 6, 0, 'USING INDEX sqlite_autoindex_hierarchy_candidate_paths_1 FOR IN-OPERATOR']
[43, 3, 62, 'SCAN scored']
[51, 3, 0, 'USE TEMP B-TREE FOR GROUP BY']
[108, 0, 0, 'MATERIALIZE counts']
[116, 108, 216, 'SCAN scored']
[124, 108, 0, 'USE TEMP B-TREE FOR GROUP BY']
[205, 0, 0, 'MATERIALIZE chosen']
[215, 205, 216, 'SCAN scored']
[225, 205, 0, 'BLOOM FILTER ON b (path=? AND best_level_rank=?)']
[235, 205, 53, 'SEARCH b USING AUTOMATIC COVERING INDEX (path=? AND best_level_rank=?)']
[245, 205, 0, 'USE TEMP B-TREE FOR GROUP BY']
[294, 0, 82, 'SCAN b']
[300, 0, 0, 'BLOOM FILTER ON c (path=?)']
[312, 0, 47, 'SEARCH c USING AUTOMATIC COVERING INDEX (path=?)']
[322, 0, 0, 'BLOOM FILTER ON ch (path=?)']
[333, 0, 47, 'SEARCH ch USING AUTOMATIC COVERING INDEX (path=?)']
[357, 0, 0, 'USE TEMP B-TREE FOR ORDER BY']
```

SQL (1 calls, 69.8 ms):

```sql
SELECT DISTINCT path FROM content_paragraph_fts WHERE content_paragraph_fts MATCH ?
```

```text
[3, 0, 132, 'SCAN content_paragraph_fts VIRTUAL TABLE INDEX 0:M8']
[14, 0, 0, 'USE TEMP B-TREE FOR DISTINCT']
```

SQL (1 calls, 26.0 ms):

```sql
SELECT path,paragraph_index,body,level,level_rank FROM content_paragraph_fts WHERE rowid IN (SELECT para_rowid FROM content_paragraph_map WHERE path IN (SELECT path FROM temp.keyword_path_filter)) ORDER BY rowid
```

```text
[3, 0, 33, 'SCAN content_paragraph_fts VIRTUAL TABLE INDEX 64:=']
[7, 0, 0, 'LIST SUBQUERY 2']
[10, 7, 102, 'SEARCH content_paragraph_map USING PRIMARY KEY (path=?)']
[12, 7, 0, 'USING INDEX sqlite_autoindex_keyword_path_filter_1 FOR IN-OPERATOR']
[23, 7, 0, 'CREATE BLOOM FILTER']
[47, 0, 0, 'USE TEMP B-TREE FOR ORDER BY']
```

### search / 예산 집행

| function | line | calls | self ms | cumulative ms |
|---|---:|---:|---:|---:|
| `query` | 128 | 1 | 3.4 | 2877.7 |
| `search` | 1277 | 1 | 65.4 | 2874.3 |
| `execute` | 78 | 24 | 0.4 | 1328.4 |
| `<method 'execute' of 'sqlite3.Connection' objects>` | 0 | 34 | 1327.7 | 1327.7 |
| `_call` | 50 | 6016 | 11.1 | 1295.4 |
| `fetchall` | 60 | 5 | 0.0 | 847.5 |
| `<method 'fetchall' of 'sqlite3.Cursor' objects>` | 0 | 5 | 846.0 | 846.0 |
| `__next__` | 66 | 6003 | 5.0 | 452.8 |
| `<built-in method builtins.next>` | 0 | 6103 | 435.2 | 435.3 |
| `_select_paragraphs_for_paths` | 1689 | 3 | 0.1 | 57.9 |
| `_breadcrumb_rows` | 1726 | 1 | 7.6 | 36.0 |
| `<method 'sort' of 'list' objects>` | 0 | 2 | 4.5 | 30.6 |

SQL (1 calls, 2104.9 ms):

```sql
SELECT path, COUNT(*) AS matched_paragraphs,
                           MIN(CASE WHEN (instr(body_folded, lower(?)) > 0 OR instr(body_folded, lower(?)) > 0) THEN COALESCE(level_rank, 8) END) AS best_level_rank,
                           0.0 AS best_bm25,
                           MIN(CASE WHEN (instr(body_folded, lower(?)) > 0 OR instr(body_folded, lower(?)) > 0) THEN CAST(paragraph_index AS INTEGER) END) AS chosen_paragraph_index,
                           SUM(CASE WHEN instr(body_folded, lower(?)) > 0 THEN (length(body_folded) - length(replace(body_folded, lower(?), ''))) / max(length(lower(?)), 1) ELSE 0 END) AS match_count_0, SUM(CASE WHEN instr(body_folded, lower(?)) > 0 THEN (length(body_folded) - length(replace(body_folded, lower(?), ''))) / max(length(lower(?)), 1) ELSE 0 END) AS match_count_1, MAX(CASE WHEN instr(body_folded, lower('예산')) > 0 AND instr(body_folded, lower('집행')) > 0 THEN 1 ELSE 0 END) AS direct_match
                    FROM (SELECT content_paragraph_fts.*, lower(body) AS body_folded
                          FROM content_paragraph_fts)
                    WHERE (instr(body_folded, lower(?)) > 0 OR instr(body_folded, lower(?)) > 0)
                      AND path IN (SELECT path FROM temp.hierarchy_candidate_paths)
                    GROUP BY path
                    ORDER BY SUM(CASE WHEN (instr(body_folded, lower(?)) > 0 OR instr(body_folded, lower(?)) > 0) THEN 1 ELSE 0 END) DESC
                    LIMIT 100000
```

```text
[8, 0, 199, 'SCAN content_paragraph_fts VIRTUAL TABLE INDEX 0:']
[29, 0, 0, 'USING INDEX sqlite_autoindex_hierarchy_candidate_paths_1 FOR IN-OPERATOR']
[35, 0, 0, 'USE TEMP B-TREE FOR GROUP BY']
[237, 0, 0, 'USE TEMP B-TREE FOR ORDER BY']
```

SQL (2 calls, 442.0 ms):

```sql
SELECT DISTINCT path FROM content_paragraph_fts WHERE body LIKE ?
```

```text
[3, 0, 132, 'SCAN content_paragraph_fts VIRTUAL TABLE INDEX 0:L5']
[17, 0, 0, 'USE TEMP B-TREE FOR DISTINCT']
```

SQL (1 calls, 23.8 ms):

```sql
SELECT path, paragraph_index, body, level FROM content_paragraph_fts WHERE rowid IN (SELECT para_rowid FROM content_paragraph_map WHERE path IN (SELECT path FROM temp.keyword_path_filter)) ORDER BY rowid
```

```text
[3, 0, 33, 'SCAN content_paragraph_fts VIRTUAL TABLE INDEX 64:=']
[7, 0, 0, 'LIST SUBQUERY 2']
[10, 7, 102, 'SEARCH content_paragraph_map USING PRIMARY KEY (path=?)']
[12, 7, 0, 'USING INDEX sqlite_autoindex_keyword_path_filter_1 FOR IN-OPERATOR']
[23, 7, 0, 'CREATE BLOOM FILTER']
[46, 0, 0, 'USE TEMP B-TREE FOR ORDER BY']
```

### timeline / 예산

| function | line | calls | self ms | cumulative ms |
|---|---:|---:|---:|---:|
| `query` | 128 | 1 | 0.0 | 159.0 |
| `range` | 181 | 1 | 0.6 | 158.9 |
| `execute` | 78 | 1 | 0.1 | 157.6 |
| `<method 'execute' of 'sqlite3.Connection' objects>` | 0 | 2 | 157.5 | 157.5 |
| `fetchall` | 60 | 1 | 0.0 | 0.6 |
| `_call` | 50 | 1 | 0.0 | 0.6 |
| `<method 'fetchall' of 'sqlite3.Cursor' objects>` | 0 | 1 | 0.5 | 0.5 |
| `_filters` | 162 | 1 | 0.0 | 0.1 |
| `<method 'extend' of 'list' objects>` | 0 | 2 | 0.0 | 0.1 |
| `<method 'split' of 'str' objects>` | 0 | 201 | 0.1 | 0.1 |
| `<genexpr>` | 170 | 2 | 0.0 | 0.1 |
| `<method 'disable' of '_lsprof.Profiler' objects>` | 0 | 1 | 0.1 | 0.1 |

SQL (1 calls, 157.8 ms):

```sql
SELECT id, path, name, kind, size, mtime, ctime, work_time, flags, cloud FROM files WHERE work_time >= ? AND work_time < ? AND hidden = 0 AND (CASE WHEN name LIKE '%#ieeee%' THEN NOT tm_excluded(name) ELSE 1 END) AND root IN (?) AND lower(name) LIKE ? ESCAPE '\' AND (lower(name) LIKE ?) ORDER BY work_time DESC, id DESC LIMIT ? OFFSET ?
```

```text
[8, 0, 61, 'SEARCH files USING INDEX files_root (root=?)']
[61, 0, 0, 'USE TEMP B-TREE FOR ORDER BY']
```

### timeline / 예산을

| function | line | calls | self ms | cumulative ms |
|---|---:|---:|---:|---:|
| `query` | 128 | 1 | 0.0 | 127.5 |
| `range` | 181 | 1 | 0.1 | 127.5 |
| `execute` | 78 | 1 | 0.1 | 127.4 |
| `<method 'execute' of 'sqlite3.Connection' objects>` | 0 | 2 | 127.3 | 127.3 |
| `_filters` | 162 | 1 | 0.0 | 0.1 |
| `<method 'disable' of '_lsprof.Profiler' objects>` | 0 | 1 | 0.1 | 0.1 |
| `<method 'extend' of 'list' objects>` | 0 | 2 | 0.0 | 0.0 |
| `<genexpr>` | 170 | 2 | 0.0 | 0.0 |
| `path_key` | 38 | 1 | 0.0 | 0.0 |
| `fetchall` | 60 | 1 | 0.0 | 0.0 |
| `normcase` | 50 | 1 | 0.0 | 0.0 |
| `sql_key` | 42 | 1 | 0.0 | 0.0 |

SQL (1 calls, 127.2 ms):

```sql
SELECT id, path, name, kind, size, mtime, ctime, work_time, flags, cloud FROM files WHERE work_time >= ? AND work_time < ? AND hidden = 0 AND (CASE WHEN name LIKE '%#ieeee%' THEN NOT tm_excluded(name) ELSE 1 END) AND root IN (?) AND lower(name) LIKE ? ESCAPE '\' AND (lower(name) LIKE ?) ORDER BY work_time DESC, id DESC LIMIT ? OFFSET ?
```

```text
[8, 0, 61, 'SEARCH files USING INDEX files_root (root=?)']
[61, 0, 0, 'USE TEMP B-TREE FOR ORDER BY']
```

### timeline / 예산 집행

| function | line | calls | self ms | cumulative ms |
|---|---:|---:|---:|---:|
| `query` | 128 | 1 | 0.0 | 116.5 |
| `range` | 181 | 1 | 0.6 | 116.5 |
| `execute` | 78 | 1 | 0.1 | 115.3 |
| `<method 'execute' of 'sqlite3.Connection' objects>` | 0 | 2 | 115.2 | 115.2 |
| `fetchall` | 60 | 1 | 0.0 | 0.6 |
| `_call` | 50 | 1 | 0.0 | 0.5 |
| `<method 'fetchall' of 'sqlite3.Cursor' objects>` | 0 | 1 | 0.5 | 0.5 |
| `_filters` | 162 | 1 | 0.0 | 0.1 |
| `<method 'disable' of '_lsprof.Profiler' objects>` | 0 | 1 | 0.1 | 0.1 |
| `<method 'split' of 'str' objects>` | 0 | 187 | 0.0 | 0.0 |
| `<method 'extend' of 'list' objects>` | 0 | 2 | 0.0 | 0.0 |
| `<genexpr>` | 170 | 2 | 0.0 | 0.0 |

SQL (1 calls, 115.6 ms):

```sql
SELECT id, path, name, kind, size, mtime, ctime, work_time, flags, cloud FROM files WHERE work_time >= ? AND work_time < ? AND hidden = 0 AND (CASE WHEN name LIKE '%#ieeee%' THEN NOT tm_excluded(name) ELSE 1 END) AND root IN (?) AND lower(name) LIKE ? ESCAPE '\' AND (lower(name) LIKE ?) ORDER BY work_time DESC, id DESC LIMIT ? OFFSET ?
```

```text
[8, 0, 61, 'SEARCH files USING INDEX files_root (root=?)']
[61, 0, 0, 'USE TEMP B-TREE FOR ORDER BY']
```

## 시험 결과와 완료 판정

- `tests.test_benchmark_is1` + 기존 `tests.test_timemachine_content_search` + `tests.test_timemachine_core`: **98건, 70.929초, 실패 0·오류 0·skip 0**. 기존 시험 파일은 수정하지 않았다. 첫 실행 중 unclosed database ResourceWarning 1회가 있었고, 새 시험의 SQLite 연결을 명시적으로 닫도록 수정했다.
- 수정 후 새 시험 5건 재실행 통과. 시험은 중앙값·nearest-rank p95·빈 표본 거부·seed 재현·SQL 정규화·bound parameter 결과 보존·인덱스 실행 계획·트랜잭션 위임을 확인한다. 이 PC의 절대 경로는 시험 코드에 없다.
- CLI 소규모 연동: 3문서 × 2문단·타임라인 20행, 28조합 모두 완료, 첫/반복 결과 일치 28/28. 전체 CLI와 SQL 프록시 경로를 확인하는 점검이며 성능 기준선으로 사용하지 않았다.
- 기준 규모 측정 완료, 조사 수치 ±30% 재현, 상위 병목 5개·함수 시간·실제 SQL·EQP·첫/반복 표 완료. S0 구현 결과이며 운영 규칙에 따른 총괄의 독립 판정은 별도다. 전체 저장소 시험·COM·화면·Rust 실측은 실행하지 않았다. 성능 구현을 바꾸지 않아 기존 전체 시험의 결과가 유지된다는 새 주장은 하지 않는다.
