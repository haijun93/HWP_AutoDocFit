# HRUST1 위계 판정 원본을 Rust로 (2026-11-14)

사용자 승인(2026-11-13): 검색 위계 판정의 원본을 Python에서 Rust(`hierarchy` 크레이트)로 바꾼다.
Python 규칙(`content_search.classify_paragraph_levels`, 규칙 판 5)은 같은 판을 유지하는 **대조 기준**으로만 남긴다.
브랜치 `feature/hierarchy-rust-primary`(github/main `b214b433` 기준). main 병합·GitLab 접근 없음.

## 1. 바뀐 것

| 구분 | 전 | 후 |
|---|---|---|
| 앱의 검색 위계 판정 | Python 규칙(엔진은 `hierarchy_engine=True`일 때만) | **Rust DLL**(`hierarchy_search_levels_json`, 문서 한 개의 문단 전체를 한 번에, DLL은 처음 한 번 적재 후 상주) |
| 되돌리기 | – | 설정 `rust_search_hierarchy=False` → Python 규칙(설정 파일에서만, 화면 설정 목록 제외) |
| DLL 없음·오류·길이/이름 불일치 | – | 자동으로 Python 규칙(문단 누락 없음) |
| 서식 역할(`analyze_levels`, 쪽 맞춤) | `hierarchy_engine=False` 기본 | **변경 없음** |
| 비교 도구·시험의 Python 기준 | `analyze_levels`를 None으로 바꿔치기 | `classify_paragraph_levels(..., backend="python")` 명시 |

판정 순서(`classify_paragraph_levels`): ① `hierarchy_engine`이 켜졌으면 그 응답 ② Rust 검색 위계(기본) ③ Python 규칙.
기존에 `analyze_levels`를 바꿔치기하던 결과 고정 시험은 그대로 둔다(Rust DLL이 없는 시험 환경에서는 ③, 있으면 ②가 같은 값을 낸다).

상태(암묵적 상태 드러내기): 검색 위계 경로는 `hierarchy_bridge.search_backend(settings)`가 `"rust"`/`"python"`으로 돌려준다.
`rust` 상태에서 DLL 실패는 문서 단위로 `python` 판정으로 내려가며, 상태 값 자체는 바뀌지 않는다.

## 2. 실제 문서 대조

- 입력: 이 PC 몽땅검색 본문 색인 DB의 **사본**(원본은 읽기만, 사본은 작업 후 삭제)에 있는 문서 342개 전부.
  유형별 txt 203·hwpx 105·pdf 23·xlsx 11. 뺀 문서 0, 추출 실패 0.
- 두 가지 입력으로 각각 대조: **file**(원본 파일을 읽기만 해서 색인과 같은 방식으로 다시 추출, 스타일·outline·표 정보 포함), **db**(사본 DB에 저장된 문단 본문과 표 칸 정보).
- 비교: Python 규칙(`backend="python"`) ↔ Rust DLL `search_levels` ↔ Rust 응답을 쓰는 앱 판정 경로(`engine_path`), 문단별.
- 도구: `tools/hrust1_real_diff.py`(요약에는 건수·유형만, 본문이 담긴 상세 파일은 작업 폴더 밖).

| 회차 | file 차이 문단(문서) | db 차이 문단(문서) | 바꾼 것 |
|---|---:|---:|---|
| 수정 전 | 80 (14) | 83 (16) | – |
| 1차 수정 | 5 (1) | 5 (1) | 아래 ①~⑦ |
| 2차 수정 | **0 (0)** | **0 (0)** | 아래 ⑧ |

문단 수는 두 입력 모두 32,662개. DLL과 앱 판정 경로의 차이 수는 매 회차 같았다.

수정 전 차이 유형(file 기준, 문단 수): 원→바 24, 네모→바 36, 네모→원 10, 주제→본문 5, 표 칸↔문서 제목 2, 본문↔절 2, 원→네모 1.

### 고친 Rust 규칙(모두 Python 쪽에 이미 있던 규칙에 Rust를 맞춤, Python 규칙 판 변경 없음)

| # | 위치 | 차이 | 영향 유형 |
|---|---|---|---|
| ① | `compat/marker_scheme.rs` `classify_marker` | 라틴 문자 번호(`b.`·`(c)`)를 Python처럼 `a.`·`(a)` 한 계열로 묶지 않음(대표 문자 인자 오류) | 영문 목록 문서의 원·네모→바 |
| ② | 같은 파일 `classify_marker` | 첫 숫자가 4자리인 다단 번호(`2024.10`)를 버림 — Python에는 없는 규칙 | 체계 감지 |
| ③ | 같은 파일 `detect_scheme` | 4자리 숫자 `1.` 후보(연도 줄)를 체계 감지에서 뺌 — Python에는 없는 규칙 | 계열 단계 어긋남 |
| ④ | 같은 파일 라벨 정규식 | `붙임:`을 라벨로, `부제:`는 라벨 아님 — Python은 반대 | 체계 감지 |
| ⑤ | 같은 파일 붙임 묶음 | 앞 공백 있는 `붙임`, 번호 뒤 조건(`\s`·끝·「)이 Python `attachment_block`과 다름 | 체계 감지 |
| ⑥ | 같은 파일 들여쓰기 | 탭을 4칸 고정으로 셈 — Python `expandtabs(4)`는 다음 4의 배수 | 계열 단계 |
| ⑦ | 같은 파일 `canonical` | 감지된 다단 번호보다 깊은 번호의 상대 깊이 조회(Python C6) 누락 | 기호 위계 |
| ⑦′ | `search_levels.rs` 제목 칸 | 글자 판정이 `char::is_alphabetic`(로마 숫자 Ⅰ 등 Nl 포함) — Python `isalpha`(L* 범주)로 맞춤 | 표 칸↔문서 제목 |
| ⑧ | `search_unify_marker.rs` | 띄어쓰기 없는 번호(`1.본문`)를 두 자리만 인정 — Python `_TIGHT_NUMBER`(1~2자리, 뒤 글자 필수) | 주제→본문 |

각 항목은 합성 입력 회귀시험으로 고정했다(`marker_scheme.rs` 3개, `search_levels.rs` 1개, `search_unify_marker.rs` 1개, ABI 1개).
문서 내용·경로·개인정보는 이 문서와 시험에 옮기지 않았다.

### 속도 수정(판정 결과 변화 없음, 수정 후 실문서 대조 다시 0)

첫 측정에서 Rust 경로 판정 시간이 Python보다 길었다(300문서 판정 합계 Rust 15.5초 ↔ Python 2.5초). 원인과 수정:

| # | 원인 | 수정 |
|---|---|---|
| ⑨ | `classify_marker`·`ordinal`이 호출마다 정규식 3개를 새로 컴파일 | `OnceLock`으로 한 번만 |
| ⑩ | 번호 정규식이 문단 전체를 훑은 뒤 시작 위치만 확인(fancy_regex는 `^`가 있어도 모든 위치를 시도) — 평균 1,000자 문단에서 문단당 수 ms | `^` 고정 + `marker_head`(앞 64자, 이어지는 공백 묶음, 한 글자까지만 정규식에 넘김) |
| ⑪ | `search_levels::classify`의 번호 계열 관찰에서 문단마다 `classify_marker`를 계열 8개만큼 반복 호출 | 문단당 1회 계산 후 재사용 |
| ⑫ | 끝 미리보기 `(?=…)` 때문에 기호 정규식이 fancy_regex 역추적 엔진에서 실행 | `compat/lead_regex.rs`: `X(?=L)` → `X(?P<la>L)`로 바꿔 `regex` 크레이트에서 실행, 표지 끝 = `la` 시작(우선순위 규칙이 같아 결과 동일, fancy_regex와 직접 비교하는 시험 포함) |

문서 한 개 기준(Rust 내부 시간): 긴 문단 PDF(문단 359개) 655ms → 48ms(Python 약 53ms), xlsx(문단 4,308개) 642ms → 99ms(Python 약 400~550ms).
실문서 대조 도구 전체 시간도 588초 → 72.5초로 줄었다(Rust 판정을 문단 3만여 개에 세 경로로 부름).

| 회차 | file 차이 문단(문서) | db 차이 문단(문서) | klib 차이 |
|---|---:|---:|---:|
| 속도 수정 후(⑨~⑫) | **0 (0)** | **0 (0)** | 0 |

## 3. 측정(색인 300문서, 위계 판정 포함 전체 시간)

`tools/hrust1_measure.py`: 사본 DB의 문서 300개를 경로마다 새 임시 색인에 넣는다(원본 파일 읽기만). 한 번 추출만 돌려 OS 파일 캐시를 맞춘 뒤 Python → Rust 순서로 잰다.

| 회차 | 경로 | 전체 시간 | 위계 판정 시간 | 판정 문단 | 오류 |
|---|---|---:|---:|---:|---:|
| 속도 수정 전 | Python | 719.8초 | 2.46초 | 33,014 | 0 |
| | Rust | 650.1초 | 15.48초 | 33,014 | 0 |
| 속도 수정 후 | Python | 728.8초 | **4.12초** | 33,014 | 0 |
| | Rust | 876.4초 | **3.06초** | 33,014 | 0 |

- 두 경로의 색인 결과(문단 26,990개의 위계)는 매 회차 완전히 같았다(`same_levels: true`).
- 전체 시간의 99% 이상은 본문 추출(파일 읽기·hwpx/pdf/xlsx 해석)이다. 같은 조건인데도 회차마다 ±150초 흔들려(위 표의 650~876초), 위계 판정 경로 차이(1초 안팎)는 전체 시간으로는 구별되지 않는다. 판정만 보면 수정 후 Rust가 Python보다 약 25% 빠르다(Python 쪽 JSON 직렬화·DLL 호출 포함).

## 4. 시험 결과

- `cargo test --release`(hierarchy): 단위 56개 + 통합 8개 모두 통과.
- `cargo test --release`(klib): 175개 통과(무시 2, 기존 그대로).
- Python 전체(`python -m unittest discover -s tests`, 속도 수정 전 회차): 3,006개 중 오류 1(`test_native_ipc` 두 번째 백엔드 10초 시간 초과) — 같은 시험을 단독으로 다시 돌리면 통과(전체 실행 부하 때문, 이번 변경과 무관). 그 뒤 Python 코드는 바뀌지 않았다.
- 속도 수정 후 새 DLL을 `hierarchy/target/release`에 두고 위계 관련 Python 시험 14개 모듈(216개) 다시 실행: 모두 통과(건너뜀 7).
- 실문서 대조: 위 2절, 문서 342개·문단 32,662개, Python ↔ DLL ↔ 앱 경로 ↔ klib 차이 0.

## 5. 남은 일

- klib의 다른 단계(`classify-levels` 1~6, `index-content`의 자체 판정)는 이번 범위 밖이다. 검색 공용 단계(7)는 같은 함수를 쓴다.
- HWP(바이너리)·PDF 위계 추출은 여전히 본문만이다(TODO 그대로).
- 배포 exe는 CI(`build-windows-exe.yml`)가 `hierarchy.dll`을 `resources/`에 넣는다. 이전 DLL이면 전체 분석 ABI로 대신하고, DLL이 없으면 Python 규칙이다. 소스 실행은 `hierarchy/target/release/hierarchy.dll`이 있어야 Rust 경로가 켜진다.
