# KORDOCB2 — 변환 품질 비교 harness 결과 (KORDOC2D 카드 B)

- 작성일: 2026-11-23, 브랜치 `feature/KORDOCB2`
- 범위: 기준 경로(kordoc)와 후보 경로(대체 변환)의 출력을 비교해 구조 지표를 내는 harness, 합성 fixture 생성기, 단위 시험. 한/글 COM·외부 다운로드·설치·새 의존 패키지 없음(표준 라이브러리 + 저장소가 이미 쓰는 `defusedxml`).

## 1. 만든 것

| 파일 | 역할 |
|---|---|
| `docfit_core/conversion_fidelity.py` | 출력(HWPX/마크다운/텍스트)에서 구조 추출, 지표 계산, 합격 판정, 골든 JSON 입출력, markdown 보고서 |
| `docfit_core/conversion_fixtures.py` | 합성 문서 생성기(MD·HTML·DOCX·XLSX·PPTX를 `zipfile`로 직접 조립), manifest 작성, 시험용 최소 HWPX 대역 생성 |
| `scripts/compare_conversion_fidelity.py` | CLI: `generate` / `run-reference` / `extract` / `compare` |
| `tests/fixtures/conversion_fidelity/` | 생성된 합성 fixture 11개 + `manifest.json`(SHA-256·기대 구조). 사용자 문서 아님 |
| `tests/test_conversion_fidelity.py` | 단위 시험 42개 |

합성 fixture: `doc_basic`(제목 3단·중첩 글머리/번호 목록·표 3×3·그림), `doc_tables`(표 2개), `doc_text_only`(구조 없음)를 MD·HTML·DOCX 3형식으로, `sheet_basic`(XLSX 시트 2개), `slides_basic`(PPTX 슬라이드 2장: 제목·목록·표·그림). 생성은 결정적이라(ZIP 항목 시각 고정) 같은 코드면 manifest 해시도 같고, 시험이 커밋된 manifest와 생성 결과의 일치를 확인한다.

## 2. 지표 정의

구조(Structure)는 `headings[[수준,글]]`, `lists[[깊이,번호여부,글]]`, `tables[행[셀]]`, `images`, `text`로 정규화한다. 비교는 **같은 fixture·같은 출력 형식끼리**(HWPX↔HWPX, MD↔MD, 텍스트↔텍스트)만 한다. 공백·줄바꿈은 모두 제거한 뒤 비교.

| 지표 | 정의 | 합격 |
|---|---|---|
| 제목 | 기준 제목 중 (수준, 글)이 후보에 있는 비율(중복은 개수 대응) | 100% |
| 목록 텍스트 | 기준 목록 항목 글이 후보에 있는 비율 | 100% |
| 목록 깊이·유형 | (깊이, 번호/글머리, 글)이 모두 일치하는 비율 | 100% |
| 표 개수 | 같으면 100%, 다르면 min/max | 100% |
| 표 행×열 | 같은 순번의 표끼리 행 수·최대 열 수가 같은 표의 비율 | 100% |
| 표 셀 텍스트 | 같은 좌표(행,열)의 셀 글이 같은 셀의 비율 | 100% |
| 그림 수 | 같으면 100%, 다르면 min/max | 100% |
| 재현율 | 기준 글자 중 후보에 순서대로 대응되는 비율(`difflib` 일치 블록 합 ÷ 기준 길이) | ≥ 99.5% |
| 정밀도 | 같은 일치 글자 ÷ 후보 길이 | ≥ 99.0% |

- 합격선은 설계 4.3의 **잠정안**이며 사용자 승인 전이다(`--structure/--recall/--precision`으로 바꿔 볼 수 있다). 평균으로 상쇄하지 않고 지표별·fixture별로 판정하며, 하나라도 불합격이면 그 행이 불합격이다.
- `n/a`: 기준에 해당 요소가 없거나(예: 표 없는 문서), 출력이 텍스트 형식이라 구조를 담지 못하는 경우. 판정에서 제외한다.
- 후보 결과가 아예 없으면 `누락`으로 불합격 처리한다.
- 글자 30,000자 초과는 `SequenceMatcher`가 느려 순서 무시 문자 빈도 비교로 대체한다(결과 `text_method`에 기록).
- HWPX 추출 규칙: `Contents/section*.xml`의 `p/t`·`tbl/tr/tc`·`pic`, `header.xml`의 스타일 이름(`개요 N`·`제목 N`·`Heading N`)과 `paraPr/heading`(OUTLINE/BULLET/NUMBER) 기준으로 제목·목록 판별, 속성이 없으면 본문 앞 기호(`- ` `• ` `1. ` `가. ` `① ` 등)로 목록 판별. 그림 요소가 없으면 `BinData/` 파일 수로 대체.

## 3. 사용법

```
# 합성 fixture 다시 만들기 (시험이 커밋된 manifest와 일치를 확인함)
python scripts/compare_conversion_fidelity.py generate [--out DIR]

# 비교: 기준·후보는 각각 골든 JSON 또는 출력 폴더(<fixture id>.hwpx|md|txt). 기준은 'expected'(합성 정답)도 가능
python scripts/compare_conversion_fidelity.py compare --reference ref/golden.json --candidate cand_dir --report report.md [--json rows.json]
python scripts/compare_conversion_fidelity.py compare --reference expected --candidate cand_dir
# 종료 코드: 전부 합격 0, 불합격 있음 1

# 출력 폴더를 골든 JSON으로 고정(맥 등 kordoc 없는 환경에서 재사용)
python scripts/compare_conversion_fidelity.py extract OUT_DIR --golden golden.json --engine kordoc --version 4.24.9
```

보고서 예(시험용 대역 출력에서 `doc_tables` HWPX의 표 마지막 행을 일부러 뺀 경우; 실제 kordoc 결과 아님):

| 입력 형식 | 출력 | 합격/전체 | 불합격 fixture |
|---|---|---|---|
| md | hwpx | 2/3 | doc_tables__md |
| md | markdown | 3/3 | - |

해당 행의 표 행×열 0.00%❌, 표 셀 68.75%❌, 재현율 74.32%❌.

## 4. kordoc 없이 도는 모드(골든)

기준 경로 결과를 `golden.json`(`{"engine","version","results":{fixture id:{출력형식:구조}}}`)으로 고정해 두면 kordoc·Node 없이 비교기만 돈다. 시험은 전부 이 방식(합성 출력 → 골든 → 비교)이라 맥에서도 돈다.

## 5. PC에서 kordoc 기준 결과를 만드는 절차 (PM 실행용)

1. kordoc 런타임 준비: 앱 폴더 옆에 `HWP_AutoDocFit_kordoc_runtime.zip`이 있거나 설치된 상태여야 한다(`kordoc_bridge.engine_status()['available']`가 True).
2. `.venv\Scripts\python.exe scripts\compare_conversion_fidelity.py generate` (이미 커밋돼 있으면 생략 가능)
3. `.venv\Scripts\python.exe scripts\compare_conversion_fidelity.py run-reference --out C:\temp\kordoc_ref`
   - docx/xlsx/pptx: `parse_document`로 `.md`, `convert_to_hwpx`로 `.hwpx` 생성. md: `generate_hwpx`로 `.hwpx`. html: kordoc 입력 지원 목록에 없어 건너뜀(stderr에 사유).
   - 끝나면 `C:\temp\kordoc_ref\golden.json`이 같이 저장된다. 엔진을 쓸 수 없으면 종료 코드 2.
4. 정답 대비 kordoc 점검: `compare --reference expected --candidate C:\temp\kordoc_ref --report C:\temp\kordoc_vs_expected.md`
5. 후보 엔진 결과를 같은 이름 규칙으로 `C:\temp\cand`에 만든 뒤: `compare --reference C:\temp\kordoc_ref\golden.json --candidate C:\temp\cand --report C:\temp\kordoc_vs_cand.md`
6. 설계 4.3의 "입력별 반복 3회 동일" 조건은 3번을 세 폴더에 반복해 `golden.json`끼리 비교하면 된다(자동 반복 기능은 아직 없음).

## 6. 시험 결과

```
$ python -m unittest tests.test_conversion_fidelity
..........................................
Ran 42 tests in 3.871s
OK
```
(이웃 시험 `test_kordoc_bridge`·`test_kordoc_contract`·`test_file_conversion`도 49개 OK.)

## 7. 한계·못 한 것

- **실제 kordoc 기준 결과는 만들지 못했다.** 이 PC에서 `engine_status()`가 `available: False`(런타임 ZIP이 앱 폴더 옆이 아니라 `C:\HWP_AUTODOCFIT\`에 있고 풀려 있지 않음)였고, 사용자 영역에 145MB를 풀어 넣는 것은 이 카드 범위가 아니라고 판단해 `run-reference`는 종료 코드 2로 멈추는 것까지만 확인했다. 따라서 **HWPX 추출 규칙(스타일 이름·paraPr·기호)이 kordoc이 만든 실제 HWPX에서 제목·목록을 제대로 잡는지는 미검증**이다. 시험의 HWPX는 이 저장소가 만든 최소 대역이라 같은 가정을 공유한다. 첫 실제 실행에서 제목·목록이 `n/a`나 0%로 나오면 추출 규칙부터 의심할 것.
- 합성 DOCX/XLSX/PPTX는 규격에 맞춰 조립했지만 Word/Excel/PowerPoint에서 열어 본 적이 없다(python-docx 등 검증 도구도 없음). 구조(ZIP·XML 파싱, 필수 파트)만 시험했다. PPTX는 theme·master·layout 최소본이라 특히 주의.
- fixture는 합성 11개뿐이다. 병합 셀, 표 안 줄바꿈, 페이지 구분, 링크, XLSX 수식·날짜, 스캔 PDF 쌍은 설계 4.1에 있으나 이번에는 넣지 않았다. 병합 셀 span 비교 지표도 없다.
- 그림은 개수만 비교한다(바이트 해시·캡션 연결 미구현). 표는 순번 대응이라 표 순서가 바뀌면 불합격으로 보인다. 중첩 표는 안쪽을 바깥 셀 텍스트로 합친다.
- 마크다운 파서는 시험에 필요한 문법(ATX 제목, 목록, 파이프 표, 이미지, 코드 블록)만 처리한다. 4칸 들여쓰기 외의 목록 깊이 규칙·HTML 표는 다루지 않는다.
- HWPX 구조 유효성·열림 검증(설계 4.2 "산출 유효성")과 렌더링 비교는 포함하지 않았다(기존 `validate_hwpx`와 Windows 단계 몫).
- 합격선 수치는 잠정안 그대로이며 실문서로 현실성 검증을 하지 않았다.
