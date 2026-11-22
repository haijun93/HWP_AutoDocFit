# RSXML1 결과 — HWPX 직접 처리기 Rust 이식 프로토타입 (2026-11-22)

범위: `표날짜칸_hwpx_처리`(표 날짜 칸 한 줄), `두글자칸_hwpx_처리`(표 두 글자 칸)를 새 크레이트 `hwpx_fmt`로 이식하고, 파이썬 처리기와 결과 XML 차이가 0인지 시험했다.
한/글 COM·사용자 문서는 쓰지 않았다(합성 HWPX와 `tests/fixtures`의 기존 HWPX만 사용). 이 카드는 프로토타입이며 앱에 연결하지 않았다.

## 결론

- 파이썬 대 Rust 차이 **0건**: 91개 문서 × 처리기 2종 = 182쌍 모두 통과(실패 0). 이 중 74쌍에서 실제로 칸이 바뀌었고 24쌍은 둘 다 입력을 거부했다(악성 ZIP 10건·잘못된 XML 2건 × 2처리기).
- 한 건도 숨긴 차이는 없다. **개발 중 하네스가 실제 차이 3종을 잡았고 고쳤다**(아래 "하네스가 잡은 차이").
- 속도(표 500개, 1회): 표 날짜 칸 파이썬 2.516 s / Rust 0.222 s (11.3배), 두 글자 칸 파이썬 2.995 s / Rust 0.273 s (11.0배). 단, 측정 중 PC에서 한/글 시간 측정이 돌고 있어 잡음이 있다. Rust 값은 프로세스 기동·조사·처리·저장 전체, 파이썬 값은 모듈 적재를 뺀 함수 호출(조사+처리)이다.

## 구성

| 경로 | 내용 |
|---|---|
| `hwpx_fmt/` | 새 크레이트(라이브러리 + CLI). `[profile.release]` 는 PROFILE1 과 같다(opt-level 3, lto, codegen-units 1, strip, panic unwind). 자체 `[workspace]`, `Cargo.lock` 포함 |
| `hwpx_fmt/src/zipio.rs` | HWPX 안전 읽기·쓰기 |
| `hwpx_fmt/src/dom.rs` | 최소 XML 트리(ElementTree 와 같은 text/tail 모델) |
| `hwpx_fmt/src/rules.rs` | 보호표·서식표·날짜칸 판별 규칙 |
| `hwpx_fmt/src/lib.rs` | 두 처리기와 조사·처리 API |
| `hwpx_fmt/tests/unit.rs` | Rust 시험 13건 |
| `scripts/rsxml_parity.py` | 차이 0 하네스(합성 생성기·정규화·비교·속도 측정) |
| `tests/test_rsxml_parity.py` | 하네스 단위 시험 + 전체 차이 0 시험(바이너리 없으면 건너뜀) |

CLI:
```
hwpx_fmt <date|twochar> <입력.hwpx> <출력.hwpx> [--selections 파일.json]   # 처리, 표준출력 {"count":N}
hwpx_fmt survey <date|twochar> <입력.hwpx>                                  # 조사 모드: {"Contents/section0.xml":[0,3], ...}
```
조사 모드 JSON 은 파이썬 처리기를 `selections=None`으로 부른 결과와 같은 모양이다(`date`: 대상 표 번호, `twochar`: 0..N 번호). `--selections`를 생략하면 먼저 조사한 결과로 처리한다(하네스에서 파이썬이 하는 일과 같음).
입력과 출력이 같은 파일이면 거부한다(원본 보존).

## 시험 결과

| 시험 | 결과 |
|---|---|
| `cargo test --release --locked -j 2` (hwpx_fmt) | `test result: ok. 0 passed` (lib), `ok. 0 passed` (bin), **`ok. 13 passed; 0 failed`** (tests/unit.rs), `ok. 0 passed` (doc) |
| `python scripts/rsxml_parity.py` | 문서 91 · 쌍 182 · 통과 182 · 실패 0 · 통과 문서 91 · 실패 문서 0 · 실제 변경 쌍 74 · 둘 다 거부 24 · 엄격(공백 포함) 비교 잡음 0 |
| `python -m unittest tests.test_rsxml_parity tests.test_table_date_single_line tests.test_designed_tables tests.test_table_limits_cleanup` | `Ran 27 tests in 37.765s` / `OK` |

문서 91건 = 합성 63건(손으로 만든 경우 51 + 무작위 12, 시드 고정; 51건 안에 잘못된 XML 2건 `invalid_control_char_rejected`·`invalid_char_reference_rejected` 포함) + `tests/fixtures`의 HWPX 18건 + 악성 ZIP 10건. 손으로 만든 경우는 두 글자 한글 칸·숫자/영문 섞인 칸·이미 빈칸 있는 칸·앞뒤 공백 칸·글 요소 분리·글 요소 안 자식·보호 표(장식 막대·조직도·흐름표·단위 캡션)·서식 표(제목 1·3, 중제목 로마자/숫자, 붙임, 개요 상자)·중첩 표(이중 대상 포함)·날짜 칸 형식 여러 가지·이미 한 줄·subList 없음·표 없음·본문 비어 있음·여러 구역(section10 순서 포함)·대상이 한 구역에만 있음·대상 전혀 없음(바이트 복사)·바이너리/디렉터리 항목·엔티티/CDATA/속성 이스케이프·주석/처리 지시·들여쓴 공백·XML 선언 변형·UTF-8 BOM·이상한 접두사·꼬리 글·유니코드 공백 등이다.

### 하네스가 잡은 차이(고친 것)
1. **XML 에 허용되지 않는 글자(U+001C 등)**: 파이썬(expat)은 `ParseError`로 거부, Rust(quick-xml)는 받아들였다. → 파서에서 XML 1.0 금지 글자와 금지 문자 참조(`&#1;`)를 거부하도록 고침.
2. **같은 이름이 두 번 든 ZIP 항목**: 파이썬은 `중복 압축 항목`으로 거부, Rust 의 zip 크레이트는 중복을 조용히 하나로 합쳐 통과시켰다.
3. **심볼릭 링크 항목**: 같은 이유로 Rust 가 놓침.
   → 2·3 은 zip 크레이트가 아니라 ZIP 중앙 디렉터리를 직접 읽어(`zipio.rs central_directory`) 파이썬 `_check_archive_limits`와 같은 목록으로 검사하도록 고침.
수정 후 전체 재실행에서 실패 0.

## 정규화 규칙(비교기)

`scripts/rsxml_parity.py`의 `norm_tree`/`compare_outputs`. 압축 항목은 이름·순서·`mimetype` 압축 방식이 같아야 하고, `Contents/*.xml`은 파이썬 defusedxml 로 두 출력을 다시 읽어 트리 단위로 비교한다. 그 밖 항목은 바이트 단위로 비교한다.

| 무시하는 것 | 이유 |
|---|---|
| 네임스페이스 접두사(`hp:`/`ns0:`/`b:`) | 확장 이름 `{uri}local`로 비교 |
| 속성 순서 | 속성은 정렬한 쌍으로 비교(값은 그대로) |
| `t` 요소 밖의 공백뿐인 글/꼬리 | 직렬화 도구가 들여쓰기·줄바꿈을 달리 낼 수 있음. `t` 안 글은 공백 하나까지 그대로 비교 |
| XML 선언 모양(`'`/`"`), `<x/>`/`<x />`, 주석·처리 지시 | 읽는 순간 사라지거나 같은 트리 |
| 압축 바이트(deflate 구현 차이) | 항목 내용만 비교(압축 전 바이트) |

비교하는 것: 요소 이름·속성 이름과 값·글·꼬리·자식 순서·압축 항목 이름과 순서·바이너리 항목 바이트·조사 결과(대상 목록)·바꾼 개수·대상 없을 때 출력이 입력과 바이트 동일한지·입력 원본 해시 불변.
엄격 모드(공백뿐인 글도 비교)에서도 차이가 없다(잡음 0쌍). 하네스 비교기 자체 시험(접두사·속성 순서는 무시, 글/속성 차이는 잡음)도 `test_rsxml_parity.py`에 있다.

## 규칙 근거 위치(파이썬 원본 → Rust)

| 이식한 규칙 | 파이썬 근거 | Rust |
|---|---|---|
| 표 날짜 칸 한 줄(칸 순회·건너뛰기) | `docfit_engine/formatter_tables.py:48` `표_날짜칸_한줄`, `:68` `표날짜칸_hwpx_처리` | `lib.rs date_cells/survey_date/apply_date` |
| 두 글자 칸 | `formatter_tables.py:106` `두글자칸_hwpx_처리` | `lib.rs twochar_targets/apply_twochar` |
| 보호표(사용자가 꾸민 표) 집합 | `docfit_core/table_style.py:160` `protected_tables`, `:154` `is_user_designed_table`, `:96` 장식 막대, `:120` 조직도, `:138` 흐름표, `:148` 단위 캡션 (`hwp-auto-docfit.py:317`에서 `보호표_목록`으로 연결) | `rules.rs protected_tables/is_user_designed_table` |
| 보호 표 경계에서 멈추는 칸 순회 | `table_style.py:174` `iter_unprotected_cells` | `rules.rs unprotected_cells` |
| 서식표 종류 판별 | `docfit_engine/formatter_titles.py:552` `서식표_종류판별` | `rules.rs is_format_table` |
| 제목 표 유형 1·2·3 | `formatter_titles.py:466` `제목_유형판별`, 담당자 정규식 `hwp-auto-docfit.py:2638`, 날짜 칸 정규식은 같은 함수 안 | `rules.rs title_kind` |
| 중제목 표 | `formatter_titles.py:76` `중제목_유형판별`, `:49` `중제목_글칸들`, 번호 정규식 `hwp-auto-docfit.py:844-845` | `rules.rs mid_kind/mid_cells` |
| 붙임 표 | `formatter_titles.py:13` `붙임_유형판별` | `rules.rs attach_kind` |
| 개요 상자 | `formatter_titles.py:383` `개요_표인가` | `rules.rs overview_table` |
| 날짜 칸 판정 | `formatter_titles.py:647` `날짜칸인가`, 날짜 정규식 `hwp-auto-docfit.py:948` | `rules.rs is_date_cell` |
| 요소 도우미 | `formatter_titles.py:391-409` `제목_xml이름·제목_자식·제목_셀들·제목_문단들·제목_문자열`, `table_style.py:58-90` `_position·_size·_text` | `rules.rs impl Doc` |
| ZIP 안전 검사 | `docfit_core/hwpx/__init__.py:22` `HwpxLimits`, `:68` `_safe_member_name`, `:78` `_check_archive_limits`, `:132` `validate_hwpx`, `:195` `open_hwpx_safely` | `zipio.rs read_hwpx_safely` |
| 안전 저장 | `docfit_engine/formatter_utils.py:302` `_hwpx_안전_저장` | `zipio.rs write_hwpx_safely` |

ZIP 안전 기준(같은 값): 항목 10,000개·전체 512 MiB·파일 128 MiB·압축률 1,000배 초과 거부, 절대 경로·`..`·드라이브 문자·심볼릭 링크·중복 이름 거부, `Contents/header.xml`·`Contents/section*.xml` 필수, CRC 검사(Rust 는 읽는 동안 검증). 쓰기: `원본경로.tmp`에 쓴 뒤 교체, `mimetype`은 무압축·나머지는 deflate, 교체 실패(PermissionDenied) 시 100 ms 간격 20회 재시도, 실패하면 임시 파일을 지운다. 입력 파일은 열기만 한다(원본 해시 불변을 모든 쌍에서 확인).

## 파이썬과 일부러 똑같이 둔 동작(버그처럼 보여도)
- `두글자칸_hwpx_처리`는 `selections` 목록 내용을 보지 않고 "전부 비었는지"만 본 뒤 모든 구역의 모든 대상을 처리한다. Rust 도 같다.
- 중첩 표 안 한 칸의 글만 있는 바깥 칸은 같은 글 요소가 바깥·안쪽 두 번 대상이 되어 빈칸이 두 개(`구  분`)가 된다. Rust 도 같다(`nested_duplicate_target` 사례, 통과). 앱 규칙상 의도인지는 파이썬 쪽 판단 사항이며 이 카드에서 고치지 않았다.
- 대상이 없으면 출력은 입력의 바이트 복사다. 대상이 있으면 모든 구역 XML 을 다시 직렬화한다.
- 파이썬의 "표 내용 보존 검사(RuntimeError)"는 이식하지 않았다. 날짜 처리는 속성만 바꾸므로 Rust 구조상 글이 바뀔 수 없다.

## 남은 오탐 가능성·한계
1. **`제목_담당자_글` 설정 미반영**: 앱이 사용자 설정으로 담당자 칸 글을 지정하면(`formatter_titles.py:1239 _설정_담당자_글인가`) 제목 표 판별(따라서 두 글자 칸의 제외 대상)이 달라진다. 하네스와 CLI 는 기본값(빈 문자열)만 다룬다. 설정이 켜진 문서에서는 Rust 가 서식 표를 일반 표로 보고 칸에 빈칸을 넣을 수 있다.
2. **XML 파서 차이**: 입력은 UTF-8(BOM 허용)만 받는다. 다른 인코딩 선언 문서는 거부한다(파이썬 expat 은 받음). DOCTYPE 은 전부 거부한다(defusedxml 은 엔티티 선언만 거부). 실제 한/글 HWPX 는 UTF-8·DOCTYPE 없음이 일반이다. 위반 시 Rust 는 처리하지 않고 실패(출력 파일 없음)하므로 조용히 틀린 결과를 내지는 않는다.
3. **`int()` 의미**: 파이썬 `int("٣")`(아랍 숫자)·`int("1_0")`은 받지만 Rust `py_int`는 ASCII 숫자만 받는다(`rowCnt`/`colCnt`/`rowAddr` 등). 한/글이 그런 값을 쓰지 않으므로 실무 위험은 낮으나 하네스 사례에는 없다. 그 경우 Rust 는 실패(안전 쪽)한다.
4. **정규식 의미**: 파이썬 `\s`(= `str.isspace`, U+001C~001F 포함)를 Rust 에서 `[\s\x1c-\x1f]`로 맞췄다. `\d`(유니코드 숫자)와 `$`(문자열 끝 직전 개행 허용)는 파이썬과 같은 의미를 쓰는 곳에서만 쓰였고, `$`는 앞에서 `strip()`된 글에만 적용되므로 같다. 그래도 합성 사례 밖의 희귀 문자(예: 다른 정규화 형태의 한글 자모)는 시험하지 않았다.
5. **표본의 한계**: 합성 63건과 기존 fixture 18건에는 실제 기관 문서(대형·복잡 표)가 없다. 사용자 문서 금지 때문이다. fixture 18건은 날짜 처리 대상 0, 두 글자 처리 대상이 있는 것 3건뿐이라 규칙 검증력은 합성 사례가 맡는다. 실제 문서 대량 대조는 후속 카드가 필요하다.
6. **무작위 사례의 도달 범위**: 무작위 12건 중 날짜·두 글자가 실제로 바뀐 것은 일부다. 판별 함수들의 분기(제목 유형 2의 날짜/담당자 조합 등)는 손으로 만든 사례가 일부만 덮는다. 서식표 판별 분기를 더 촘촘히 덮는 사례 보강 여지가 있다.
7. 속도는 1회 측정이며 부하 중 측정이다. Rust 의 11배는 대략의 규모로만 읽을 것(파이썬 쪽 비용 원인은 프로파일하지 않았다).
8. 개발 편의로 `hwpx_fmt`는 zip 9.x·quick-xml 0.37·regex 1.x·serde_json(preserve_order)에 의존한다(`Cargo.lock` 고정). 오프라인 환경 고려 시 `cargo vendor` 필요 여부는 확인하지 않았다.

## 이식 난이도 실측 평가
- 이식한 코드: Rust 1,047줄(dom 199·zipio 191·rules 395·lib 221·main 41, 시험 제외). 파이썬 원본은 처리기 본체 약 60줄과 도우미 여러 개다. 처리기 두 개 자체보다 **도우미 이식이 대부분**이었다(보호표 4판정·서식표 4종 판별·날짜 칸). 도우미는 한/글 COM 과 무관한 순수 XML 규칙이라 기계적으로 옮겨진다.
- 쉬웠던 것: 요소 순회(문서 순서 배열 + 하위 구간 `[i,end)`로 `iter()`가 상수 비용), 텍스트 규칙, 정규식.
- 어려웠던 것: (1) 파이썬 표준 라이브러리의 **암묵적 동작 재현** — `str.strip()/\s`의 공백 집합, `int()`의 파싱, ElementTree 의 text/tail 모델·주석 버리기·병합, expat 의 금지 글자 거부, zipfile 의 중복 항목 처리. 이 다섯 곳에서 실제로 차이가 났거나 날 뻔했다. (2) 파이썬 처리기의 quirk(`selections` 내용 무시·이중 대상)까지 같게 둬야 차이 0이 된다. 이식 전에 하네스를 먼저 두는 편이 안전하다는 실측 근거다.
- 시간 감각(이 카드 기준): 규칙 읽기와 이식이 가장 길었고, 하네스가 규칙 이식보다 큰 비중(스크립트 548줄)이었다. 처리기를 더 옮길수록 하네스 비용은 재사용되므로 후속 처리기당 비용은 규칙 이식량에 비례한다.
- 앱 연결 시 남는 일: 설정 값 전달(담당자 글), 로그 문구(파이썬 `로그()`는 Rust 가 호출하지 않음), 오류 경로(Rust 실패 시 파이썬 폴백 여부), 한/글 COM 단계 앞뒤 파일 교환 규칙, 설치본(MSVC) 빌드·서명.

## 재현
```
set PATH=%LOCALAPPDATA%\DocFitTools\mingw64\bin;%PATH%
set CARGO_TARGET_DIR=<저장소 밖 폴더>
cd hwpx_fmt && cargo test --release --locked -j 2 && cargo build --release --locked -j 2
set HWPX_FMT_BIN=<CARGO_TARGET_DIR>\release\hwpx_fmt.exe
python scripts/rsxml_parity.py --bench --json <결과.json>
python -m unittest tests.test_rsxml_parity
```
