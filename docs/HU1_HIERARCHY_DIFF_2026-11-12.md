# HU1 위계 판정 통일 1단계: 세 판정 대조 (2026-11-12)

브랜치 `feature/hierarchy-unify`. 이 단계는 **대조만** 했다(제품 코드 변경 없음).
대조 스크립트: `scripts/hu1_hierarchy_diff.py` (경로는 인자로만 받음, 문서는 읽기만).

## 1. 세 판정의 입력·출력

| 구분 | (A) 서식 분석 Python | (B) Rust 위계 엔진 | (C) 검색 색인 |
|---|---|---|---|
| 위치 | `docfit_core/style_hierarchy.py` `analyze_hierarchy`·`leading_marker` + `marker_scheme.detect_scheme` | `hierarchy/src/compat/*` (C ABI `hierarchy_analyze_json`, exe 표준입출력) ← `docfit_app/hierarchy_bridge.py` | `docfit_core/timemachine/content_search.py` `classify_paragraph_levels` |
| 입력 | text, left, indent(HWPUNIT), font, size_pt, prev_spacing | (A)와 같음(같은 JSON) | ParagraphRecord: text, is_table_cell, style_name, outline_level, table_id, 파일 확장자 |
| 서식 정보 사용 | 들여쓰기 → 글꼴 → 크기 → 기호 | (A)와 같음 | 사용 안 함(엔진 호출 때도 indent 0, font_size 0) |
| 출력 | `role_sequence`(한글 역할명) + styles·variants·document_type·warnings | (A)와 같은 dict | `level`(영문) + `level_rank` |
| 표 칸 | 위계 없음(별도 `표 머리글/본문`) | 없음 | `table_cell`(순위는 body와 같음), 제목 표 → `document_title` |
| 설정 | 항상 사용 | `hierarchy_engine`(기본 False)일 때만 | 항상 Python 경로(엔진 켜면 엔진 결과 변환) |

**순위 대응**(공통 눈금은 공문서 6단계 기준):

| 공통 | (A)/(B) 역할 | 표시 | (C) level | rank | Rust 네이티브 `Role` |
|---|---|---|---|---|---|
| T | 제목 | 1단계 | document_title | 0 | DocumentTitle=0 |
| CH | 장 | 2단계 | chapter | 1 | Chapter=1 |
| SEC | 중제목(Ⅰ·가.) | 3단계 | section(절) | 2 | Section=2 |
| — | (대응 없음) | — | topic(주제·소제목: `###`, `가.`, 제목3, 번호 3순위) | 3 | Topic=3 |
| L4 | 소제목(□) | 4단계 | square | 4 | Square=4 |
| L5 | 본문(ㅇ, 가)) | 5단계 | circle (단 `가)`처럼 기호 분류에 없으면 **body**) | 5 | Circle=5 |
| L6 | 내용(-, 1)) | 6단계 | dash (단 `1)` 등은 **body**) | 6 | Dash=6 |
| NOTE | 부연설명(* ※ •) | 부연설명 | asterisk | 7 | Asterisk=7 |
| BODY | 미분류 | — | body / table_cell | 8 | Body=8, TableCell=9, Ambiguous=10 |

정수 → 이름 변환표가 **네 벌**이고 서로 다르다:
- `document_review.convert_engine_levels_to_review_roles`: 3→소제목, 4·5→본문 (Rust 열거형과 한 칸 어긋남)
- `content_search.convert_engine_levels_to_records`: Rust 열거형과 같음
- `hierarchy_bridge.ENGINE_ROLE_NAMES`(쪽 맞춤): 4→소제목, 5→본문, 6→내용, 7→부연설명
- `tools/hier_regress.py ROLE_LEVEL`: 제목0·장1·중제목2·소제목3·본문4·내용5·부연설명6·미분류7

참고: Rust에는 브리지가 부르는 `compat`(Python 이식본) 외에 네이티브 `analyze_paragraphs`(DP·붙임 영역·진단)와
`classify`가 있으나 **제품 경로에 연결되지 않았다**. `classify`의 `parse_prefix`는 `"2026. 11. 04."`, `"1."·"2."`만,
`"가."·"나."`만 같은 시험 문장 맞춤 분기가 있어 통일 기준으로 쓰면 안 된다.

## 2. 같은 문서로 돌린 결과

명령(이 PC):
`python -X utf8 scripts/hu1_hierarchy_diff.py --dll <hierarchy.dll> --out E:/docfit-tmp/hu1/out <문서…>`
엔진은 `cargo build --release`(대상 폴더 `E:/docfit-tmp/hunify-target`, 3분 45초)로 새로 빌드했다.
문서는 모두 `E:/docfit-tmp/hu1/in`에 사본을 두고 썼다. 문단별 결과: `E:/docfit-tmp/hu1/out/hu1_paragraphs.csv`.

| 문서 | 문단 | 본문 | A≠B | A≠C(공통 눈금) | 다른 내용(A→C, 건수) |
|---|---|---|---|---|---|
| 시연용 txt(원문) | 47 | 47 | 0 | 5 | L4→T 1(`제목:`), L4→BODY 4(`개요:`, `붙임:`과 아래 번호 2줄) |
| 시연용 일괄적용 HWPX(perf run01) | 48 | 43 | 0 | 1 | L5→BODY 1(`붙임: 1. …`) / 표 칸 5: 제목 표 2칸→T |
| 시연용 일괄적용 HWPX(바탕 화면본) | 44 | 39 | 0 | 0 | 표 칸 제목 2칸→T |
| fixtures `timemachine_extract/sample.md` | 4 | 4 | 0 | 1 | L4→T 1(`# 제목`) |
| fixtures `timemachine_extract/sample.hwpx` | 3 | 3 | 0 | 0 | — |
| fixtures `beomgov_markdown.txt` | 16 | 16 | 0 | 13 | SEC→BODY 12(`네모:`·`원:` 등 라벨), SEC→T 1(`제목:`) |
| fixtures `c23_continuation.hwpx` | 4 | 4 | 0 | 4 | L4→CH 2(`• 위치: ①…`), L5→BODY 2(②·③) |
| fixtures `label228_circled_black.hwpx` | 12 | 12 | 0 | 1 | L5→BODY 1(②) |
| fixtures `ai_answer_claude.txt` | 23 | 23 | 0 | 18 | `#`→T, `##`→CH, `###`→SEC, `1. **…**`→CH 3, 표 줄 `|…|`→TOPIC 2 |
| fixtures `ai_answer_gemini.txt` | 19 | 19 | 0 | 9 | 표 줄 `|…|`→CH 4, `**1. …**`→BODY |
| **합계** | **220** | **210** | **0** | **52** | |

- 기존 `tools/hier_regress.py --no-build --dll …`: `documents=29 paragraphs=549 differences=0` (A↔B).
- **엔진을 켰을 때(C′)**: 위 10개 문서 모두 검색 레코드가 **0개**가 된다. 서식 검토는
  `review_document` → `KeyError: 'role'`(시연 HWPX에서 재현). 원인: 엔진(compat)은 `role_sequence`(한글 문자열)만
  내보내는데 `document_review`·`content_search`의 변환 함수는 옛 응답 형식 `levels`(정수)를 읽는다.
  시험(`test_document_review`, `test_hs2_m_chapters`, `test_hs2_p_title_reindex`, `test_hierarchy_page_fit_wire`)은 옛
  `{"levels": […]}` 형식을 직접 넣어서 통과한다. 쪽 맞춤(`engine_role_of_last`)은 `role_sequence`를 먼저 읽어 정상.
  설정 기본값이 False이고 이 PC 설정에도 켜져 있지 않아 현재 사용자 영향은 없다.

## 3. 차이별 판정과 분류

분류: **[서식]** 서식 분석·결과 문서에 영향 / **[검색]** 검색 색인에만 필요 / **[결함]** 고쳐야 할 오류.

| # | 차이 | 맞는 쪽(근거) | 분류 |
|---|---|---|---|
| D1 | A↔B 판정 | 같음(549+210문단 차이 0) | — |
| D2 | 엔진 켬 시 `levels` 형식 불일치(검색 0문단, 검토 KeyError) | 둘 다 틀림. `role_sequence`를 읽어야 함 | [결함] 서식·검색 모두. 엔진 켜기 전 필수 |
| D3 | `marker_scheme.family_levels` 1·2·3을 C가 chapter·section·topic으로 읽음(`• 위치:`→장) | **A**. marker_scheme의 1·2·3은 □·ㅇ·- 키(`_LEVELS`)이지 장·절이 아님 | [검색] 순위 과대평가. 통일 시 A 역할을 그대로 쓰면 해소 |
| D4 | Markdown `#`~`####` → 제목·장·절·주제 | **C**(원문 MD 기준). A는 기호 없는 글로 보고 들여쓰기 0의 역할을 붙임 | [검색]. 서식 결과는 붙여넣기 단계 `ai_answer.parse_ai_answer`가 `#`을 □·ㅇ·-로 바꾼 뒤라 A에 `#`이 오지 않음 |
| D5 | `제목:`·`제목1:` 줄, 첫 표의 제목 칸 → document_title | **C**. 보고서 제목은 1단계 | [검색]. 서식 쪽은 제목 표·`제목_문단` 규칙이 따로 있음(A 본문 판정에 표 칸 없음) |
| D6 | TXT/MD의 Ⅰ.·1.·가.·제N장 연속 번호 → 장·절·주제 | 원칙은 둘 다 일리: 편람 순서(1.→가.→1)→가)…)대로 상대 순위. 단 **`## 2.` 바로 아래 `1. 2. 3.` 목록을 C가 장으로 올림 — 틀림**(앞 줄이 MD 제목이면 `follows_body` 검사를 건너뜀) | [검색] + [결함](검색 순위 과대) |
| D7 | MD 표 줄 `| … |` → C가 기호 `|`(Sm)를 계열로 잡아 장·주제 | 둘 다 틀림. 표 칸/본문이어야 함 | [검색][결함]. A도 원문 txt에선 틀리나 서식 경로는 표로 바뀐 뒤라 영향 없음 |
| D8 | `붙임:` 줄과 아래 번호 줄: A는 들여쓰기로 본문(ㅇ)·소제목, C는 body | **C**. 붙임은 본문 위계 밖(attachment_block이 체계 감지에서 빼는 것과 같은 취지). Rust 네이티브엔 `AttachmentHeading`이 있음 | **[서식]** A의 styles·variants(서식 학습)·구조 경고에 붙임이 ㅇ으로 섞임 → 바꾸면 결과 바뀔 수 있음 |
| D9 | 기호 없는 문단: A는 들여쓰기 ±100 안의 역할(서식 없는 txt면 대개 소제목·중제목), C는 body | HWPX: **A**(이어지는 줄은 앞 항목 서식). 서식 정보 없는 txt/md: **C** | [검색]. 엔진에 "서식 정보 없음"을 알려 body로 두는 방식 필요 |
| D10 | 원문자 ②, `가)`, `1)`: A는 문맥으로 본문·내용, C는 body | **A**(편람 순서상 하위 항목) | [검색] 순위만 낮게 나옴 |
| D11 | `개요:`·범정부 MD 라벨(`네모:` `원:` `바:` `주석1:`) | 둘 다 미흡. 라벨 뜻(네모=□, 원=ㅇ, 바=-, 주석=부연)대로가 맞음 | [검색] 선택 개선(라벨 변환기와 표 공유) |

## 4. 통일 계획(Rust 엔진 하나로)

원칙: 엔진 출력은 **역할 문자열 하나**(role_sequence)로 고정하고, 정수 변환표 네 벌은 없앤다.
"서식 분석 결과 바뀜" 단계는 한/글 실기(시연 문서 일괄 적용 결과 비교)와 **사용자 승인** 후 병합한다.

| 단계 | 내용 | 서식 결과 | 깨질 수 있는 시험 |
|---|---|---|---|
| U1 | 엔진 응답 계약 정리: `document_review`·`content_search` 변환을 `role_sequence` 기준으로(한 함수 `hierarchy_bridge.roles()`), 옛 `levels` 시험 입력을 실제 형식으로 교체. D2 해소 | 안 바뀜(엔진 기본 꺼짐) | `test_document_review` 변환 시험, `test_hs2_m_chapters`, `test_hs2_p_title_reindex`, `test_hierarchy_page_fit_wire`(입력 형식만 교체) |
| U2 | 엔진에 문단 속성 추가: is_table_cell, style_name, outline_level, file_kind(hwpx/txt/md), has_format. compat 경로는 has_format=true일 때 지금과 한 글자도 같게 | 안 바뀜 | hier_regress 차이 0 유지, `cargo test` |
| U3 | 검색 전용 규칙을 엔진 "검색 모드"로 이식: D4(MD 제목), D5(제목 줄·제목 표), D6(연속 번호), D9(서식 없는 글=body), `제N장` 확인. 출력은 공통 역할에 `topic` 추가 | 안 바뀜(검색 모드만) | `test_timemachine_content_search`, `test_hs2_*`, `test_eval_hierarchy_search`, `test_rlev_rust_index_levels`. 색인 위계 버전(`CURRENT_HIERARCHY_SCHEMA_VERSION`) 올려 재색인 |
| U4 | 검색이 D3·D10을 A 역할로 받음(scheme 1·2·3을 장·절로 읽지 않음, 원문자·`가)`·`1)`은 circle·dash) | 안 바뀜 | 검색 순위 시험 일부 기대값 변경 → 보고 필요 |
| U5 | D6 목록 과승격·D7 표 줄 결함 수정 | 안 바뀜 | 검색 시험 기대값 추가 |
| U6 | 서식 분석이 엔진 결과를 기본으로 사용(`hierarchy_engine` 기본 True 또는 설정 제거). compat 결과 = Python 결과라 결과 문서 동일해야 함 | **동일해야 함 → 실기 확인** | 전체 시험, 시연 문서 일괄 적용 결과 HWPX 바이트/문단 비교 |
| U7 | D8 붙임 영역을 위계 밖으로(네이티브 `AttachmentHeading` 규칙 이식) | **바뀜 → 실기 확인·사용자 승인** | `test_style_hierarchy`, `test_style_profile*`, `test_document_review` 구조 경고 |
| U8 | Python `style_hierarchy.analyze_hierarchy`·`classify_paragraph_levels` Python 경로를 엔진 실패 시 대비용으로만 남기거나 제거(사용자 결정) | U6과 같음 | — |

### Python·Rust 차이 0 확인 방법
1. `tools/hier_regress.py --dll <dll>`: 지금 29문서 549문단 차이 0. U2·U6 전후 0 유지가 조건.
2. `scripts/hu1_hierarchy_diff.py`: A≠B 0 유지, 그리고 C′(엔진 켬 검색)의 레코드 수 = 문단 수, C′ = C(U3 이후 목표, 현재 C 기준 52건 차이 중 의도한 변경만 남게).
3. `hierarchy/tests/measure_ndet.py`·`ndet_regress_exe.csv`(비결정성), `cargo test`(golden.json).
4. U3부터는 검색 모드 골든(fixtures md·txt·hwpx 문단별 level)을 `hierarchy/tests`에 추가해 Python 결과와 문단 단위 비교.

## 5. 시험 결과(이번 단계)
- 관련 시험 13개 모듈: `Ran 181 tests in 69.313s — OK (skipped=8)`
  (test_style_hierarchy, test_hierarchy_bridge, test_document_review, test_timemachine_content_search,
  test_hs2_m_chapters, test_hs2_m_title, test_hs2_p_title_reindex, test_hs2_d2_numbered_headings,
  test_hierarchy_page_fit_wire, test_marker_scheme, test_marker_scheme_general, test_rlev_rust_index_levels,
  test_eval_hierarchy_search)
- hier_regress: `documents=29 paragraphs=549 differences=0 elapsed_seconds=0.112`

## 6. 남은 것·확인 못 한 것
- 한/글 COM 실기는 하지 않았다(지시대로 기존 HWPX만 사용). U6·U7은 실기 필요.
- 붙여넣기 변환(`ai_answer.parse_ai_answer`의 MD 제목 → □·ㅇ·- 상대 순위)도 넷째 위계 판정이다. 이번 범위(세 곳) 밖이라
  대조하지 않았고, 통일 범위에 넣을지 결정 필요.
- klib Rust 본문 색인(`index-content`)은 위계 없이 쓰고 Python이 다시 매기는 구조(`test_rlev_rust_index_levels`)라 U3에서 함께 정리해야 한다.
- 범정부 MD 라벨(D11)은 어느 판정에도 없다. 검색에서 필요한지 사용자 판단 필요.
