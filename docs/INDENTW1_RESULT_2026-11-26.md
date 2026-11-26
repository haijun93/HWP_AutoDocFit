# INDENTW1 결과 — 공문 글꼴별 글자 폭 표와 폭 계산 함수

- 브랜치 `feature/INDENTW1` (기준 `github/main`), 로컬 커밋만(푸시 안 함).
- 사용자 결정(2026-11-26): 공문 글꼴은 기본안대로, 한/글 방식(`ParagraphShapeIndentAtCaret`) 대비 0.5pt 이내.
- 기존 내어쓰기 함수(`xml_hanging_indent`, `continuation.text_width`, formatter, `hwpx_fmt`)는 바꾸지 않았다. 새 모듈은 아직 어디에도 연결되지 않았다.

## 만든 것

| 파일 | 내용 |
|---|---|
| `scripts/make_font_width_table.py` | 설치된 TTF/OTF/TTC 의 cmap·hmtx 를 표준 라이브러리로 읽어 표 생성 |
| `resources/font_widths.json` | 생성된 표(약 45KB, 폭 수치만. 글꼴 파일 없음) |
| `docfit_core/font_widths.py` | 언어 칸별 글꼴을 골라 폭 계산 |
| `tests/test_font_widths.py` | 단위 시험 19개 |

### 표

- TTF: 휴먼명조, 함초롬바탕, 함초롬돋움, HY헤드라인M, 맑은 고딕. 함초롬 2종과 맑은 고딕은 굵은 글꼴 폭도 담았다.
- 담은 구역: 영문·숫자·문장부호, 라틴 보충, 그리스, 기호·화살표·원문자·도형, CJK 기호(「」)·호환 자모, 한글 음절 전체, 전각 형. 한자는 담지 않았다.
- HFT(`needs_measurement`, INDENTW2 가 채움): 휴먼명조(HFT), 한양신명조, 신명 세명조·신명조·신신명조·중명조·태명조·견명조·신문명조·순명조·세고딕·중고딕·태고딕·견고딕·신문고딕·세나루·디나루·신그래픽·태그래픽·궁서, 신명조 약자·간자, #신명조, #태신명조. 이름은 한/글 `HFTInfo.dat` 에서 확인했다. 휴먼명조(HFT)·한양신명조를 목록에 넣은 것은 작업자 판단이다.
- INDENTW2 입력 형식: 항목을 `status: "measured"`, `units_per_em`, `ranges: [[첫 코드, 끝 코드, 폭]]` 로 바꾼다. 재생성해도 `measured` 항목은 유지된다.

### 계산

`폭 = Σ (크기×상대크기) × (글자폭/units_per_em × 장평 + 자간)`, 언어 칸(한글·영문·한자·일어·기타·기호·사용자)별 값 사용. HWPX 는 `fontfaces_from_header` + `style_from_char_pr`, 계산은 `measure_text` / `measure_runs(…, offset)`.

- 빈칸: `useFontSpace="0"` 이면 **0.5em 고정**, `1` 이면 글꼴의 빈칸 폭. 빈칸은 영문 칸의 장평·자간을 쓴다.
- 표 없는 글꼴·미측정 HFT 글자가 하나라도 있으면 `None`(모름).
- 글꼴에 없는 글자: East Asian Width W·F·A 는 1em, 그 밖은 0.5em 으로 어림하고 `WidthResult.substituted` 에 기록(`exact == False`). 0.5pt 보증이 필요하면 `exact` 만 써야 한다.
- 탭·제어 문자는 `None`.

기본안 글꼴에서 빠진 것을 확인한 주요 기호: 휴먼명조 `◦ – • ∙ ▪`, HY헤드라인M `◦ • ∙ ▪`, 맑은 고딕 `◦ ▪`. `•` 는 앱의 기본 항목기호 중 하나다.

## 대조

### 글꼴 파일

표의 폭을 PIL(`getlength`) 값과 전 글자 대조: 5종 보통·굵게 모두 1단위 초과 차이 0건.

PM 실측값(글꼴 빈칸 기준)은 `use_font_space=True` 로 그대로 재현된다. 한/글 기본(`useFontSpace=0`)에서는 값이 다르다.

| 문자열 | 휴먼명조 (글꼴 빈칸 / 고정) | 함초롬바탕 (글꼴 빈칸 / 고정) |
|---|---|---|
| `□ ` | 1.50 / 1.50 | 1.27 / 1.47 |
| `  - ` | 2.00 / 2.00 | 1.45 / 2.05 |
| `   · ` | 3.00 / 3.00 | 1.52 / 2.32 |

### 한/글이 저장해 둔 내어쓰기 값

`resources/서식예시` 의 HWPX 에 저장된 `intent` 와 비교했다(읽기만, **COM 실측 아님**. 언제 어떤 방식으로 저장된 값인지는 알 수 없다).

| 표본 | 저장 | 계산 | 차이 |
|---|---|---|---|
| `  - ` 휴먼명조 14pt | 2800 | 2800 | 0.00pt |
| `◈ ` HY헤드라인M 13pt | 1948 | 1950 | 0.02pt |
| ` ㅇ ` 한컴돋움 15pt (*) | 3000 (2996) | 3000 | 0.00~0.04pt |
| `   * 항세기: ` 한컴돋움 13pt (*) | 8336 | 8350 | 0.14pt |
| `   ** 난세기: ` (*) | 9092 | 9108 | 0.16pt |
| `   ※ ` (*) | 3892 | 3900 | 0.08pt |

(*) 한컴돋움은 기본안 밖이라 대조용 임시 표를 메모리에만 만들었다. 산식 확인용이며 저장소 표에는 없다.
`' ㅇ '` 은 빈칸이 글꼴 빈칸이면 2499 가 되어 저장값 3000 과 맞지 않는다. 이것이 0.5em 고정으로 판단한 근거다.

맞지 않은 표본:

- `☞ ` 함초롬바탕 15pt: 저장 2427, 계산 2268 (1.59pt). **원인 미확인.**
- `◈ ` HY헤드라인M 13pt 2건: 저장 1648, 계산 1950·1872 (3.02·2.24pt). 1648 은 11pt 크기에 해당해 크기 변경 뒤 남은 값으로 추정(미확인).
- 괄호 라벨 뒤를 기준으로 저장된 것으로 보이는 `ㅇ` 문단들과 `#` 1건은 현재 기준 위치 규칙과 달라 비교 대상에서 뺐다.

## 시험

- `python -m unittest tests.test_font_widths`: 19개 통과.
- 관련 기존 모듈 18개(xml_hanging_indent, continuation, hanging_paren_label, hangul_fonts, format_fidelity_marker_indent, indent_justify_layout, supplement_indent, spacing_indent_loop, attachment_align·block·left_trim, marker_bold_consistency·bracket·prefix·scheme·scheme_general·spacing, symbol_star_font): 148개 통과.
- 전체 시험·한/글 COM 시험·EXE 빌드는 하지 않았다.

## 남은 문제

1. **0.5pt 기준은 아직 검증되지 않았다.** 한/글 COM 대조가 필요하다.
2. 미확인 가정: 자간을 장평 적용 전 글자 크기의 %로 계산(기존 코드와 동일, 한/글이 장평 적용 폭 기준이면 장평≠100·자간≠0 에서 달라짐), 고정 빈칸에 장평·자간 적용, `·` 를 기호 칸으로 분류, 굵게일 때 굵은 글꼴 폭 사용, 글자별 반올림 없이 합계만 반올림, 커닝 미적용.
3. 앱 기본 서식이 쓰는 **한컴돋움·HY견고딕은 기본안 글꼴에 없어 '모름'** 이다. 표에 추가할지 사용자 결정 필요.
4. 신명 계열 HFT 는 폭이 없다(INDENTW2).
5. HWPX 의 `hp:fwSpace`·`hp:nbSpace`·`hp:tab` 요소는 호출 쪽에서 처리해야 한다(함수는 문자열만 받는다). 고정폭 빈칸의 폭은 확인하지 않았다.
