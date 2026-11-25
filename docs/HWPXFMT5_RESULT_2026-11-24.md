# HWPXFMT5 — 구조 서식 Rust 이식 및 Python 차이 대조

작업일: 2026-11-25. 기준: `github/main` 로컬 참조 `b868dd22`, 브랜치 `feature/HWPXFMT5`.
푸시·태그·원격 변경 없음. 합성 자료와 저장소 fixture의 외부 사본만 사용한다.

## 범위 확정

`stage_registry.py`·`pipeline.py`의 선행 목록은 **19개 실행 슬롯, 고유 처리기 18종**이다.
기본 표 서식과 그 정렬 재확인은 같은 함수를 두 번 호출한다.
RSXML1~4는 고유 13종(14슬롯)을 구현했으며, 남은 다섯 종류는 다음과 같다.

| 처리기 | Python 원본 | 이번 범위 |
|---|---|---|
| 보고서 머리 | `formatter_analysis.py`의 `보고서머리_hwpx_처리` | 미이식. 명시적 미지원으로 Python 폴백 |
| 제목·개요 | `formatter_titles.py`의 `제목_hwpx_처리` | 유형 1~3·직후 개요·복제·가운데 정렬·괄호/부제 크기·가로 폭·빈 줄 정리. 보수적 입력 제한 |
| 중제목 | `중제목_hwpx_처리` | 기존 표 유형 1·2 및 `fit`, 번호 굵게, 글자 크기·장평·자간·여백 기반 칸 폭. 일반 문단→표는 미확인 |
| 붙임 | `붙임_hwpx_처리` | 두 유형, 내장/사용자 예시와 공유 ID 병합 |
| 한 칸 상자 | `상자서식_hwpx_처리` | 사용자 예시, 제목 직후 제외·문단 4개 이상 보호·본문 문단 선택 |

다섯 종류 전체 이식 완료로 보고하지 않는다. 실행 가능한 네 종류에도 아래 제한이 있다.

## 구현과 옵션

- `hwpx_fmt/src/structure.rs`: 글꼴 중복 재사용, 테두리→탭→문자→문단 순서의 ID 추가, 기존 정의 보존, `itemCnt`/`fontCnt`, 서로 다른 XML 접두사의 URI 보존, 문자와 따옴표 run 복제.
- `hwpx_fmt/templates.json`: 기존 Python 내장 자료에서 defusedxml로 추출한 예시. 대상 문서의 판정·변환은 Rust에서 수행한다.
- `scripts/hwpxfmt5_parity.py`: Python/Rust 선택 목록·변경 수·정규화 바이트·ZIP 항목/바이너리·입력 해시 대조. 양쪽 거부는 통과에서 분리한다.
- `docfit_core/hwpx_fmt_backend.py`: 기본은 원래 Python 함수. `DOCFIT_HWPXFMT5=1`, `HWPX_FMT_BIN=<실행 파일>`로만 켠다. 실행 중 RunContext의 설정을 전달한다. 단계 상태는 `rust_survey`, `rust_applied`, `python_fallback` 로그로 기록한다. Python 조사 후에는 적용도 Python으로 고정한다.
- 기존 선행 실행기가 최신 산출물을 단계마다 다시 조사하는 순서는 유지한다. Rust 오류/미지원이면 같은 입력·같은 선택으로 Python을 실행한다.

## 미이식·미확인

- **보고서 머리**: `report_header.py`의 보고서 경계, 날짜·기관·담당자 값 추출과 예시 틀 재배치, 원본 run 이동과 섹션 컨트롤 보존까지 함께 이식해야 한다. 단순 표 서식 복제로 대체할 수 없어 이번 범위에서는 미이식으로 남긴다. CLI 이름은 미지원 이유를 반환하는 식별자이며 구현 완료를 뜻하지 않는다.
- **제목 유형 4·5 및 다른 표가 섞인 문서**: 제목다운 글/기관 정보/대표 글자 크기 판정 미이식. 본문 또는 쪽 나눔이 제목과 개요 사이에 있으면 거부한다. Python이 원래 맡는다.
- **일반 로마자·`로1` 문단→중제목 표**: 새 표 id·zOrder, 원래 run/컨트롤 이동·삭제의 동등성 미확인. 해당 후보가 있으면 Rust가 문서 전체를 거부해 Python으로 넘긴다.
- 중제목 사전 검사는 보수적이어서 ASCII `I`/`V`/`X` 또는 `로`로 시작하는 일반 문장도 Python으로 넘길 수 있다. 이 경우의 적용률 최적화는 하지 않았다.
- 설정된 담당자 글 및 활성 기호 체계의 제목/중제목 경로는 미지원이다. 잘못된 예시 프로필도 Rust 적용을 중단한다.
- 드문 Unicode 숫자·잘못된 수치 속성·위 제한 밖의 사용자 프로필 조합 전체는 확인하지 못했다. 이 카드의 차이 0은 아래 명시한 시험 범위의 결과다.

## 최종 하네스·속도

Python 3.15.2, Unicode 17.0.0. 기존 로컬 MinGW를 사용했고 빌드 산출물 위치는 `CARGO_TARGET_DIR` 환경변수로만 지정했다.
정규식 캐시 변경 후에도 **234쌍 중 일치 233·차이 0·양쪽 거부 1**, 실제 변경 84쌍, 미지원 검사 8건 통과다. 양쪽 거부는 일치 수에서 제외한다.
세부 사례와 측정값은 [HWPXFMT5_PARITY_2026-11-24.json](HWPXFMT5_PARITY_2026-11-24.json)에 보관했다.
비교기는 ZIP 항목 순서·바이너리 바이트·조사 목록·변경 수·원본 해시와 XML 정규화 UTF-8 바이트를 대조한다. XML 접두사·속성 순서와 `t` 밖 들여쓰기만 정규화하며, 글 안 공백·속성 값·자식 순서는 보존한다.

각 입력·처리기당 1회, 다른 빌드·시험 없이 순차 측정했다. Python은 모듈 적재 제외 조사+적용, Rust는 CLI 프로세스 기동을 포함한 조사+적용이다. 이전의 느린 제목 결과는 아래 개발 기록에 그대로 남겼다.

| 입력 | 처리기 | Python 초 | Rust 초 | 배수 | 변경 수 | 출력 동일 |
|---|---|---:|---:|---:|---:|---|
| 최소 합성 400표 | 붙임 | 0.353 | 0.175 | 2.02 | 400 | 예 |
| 최소 합성 400표 | 상자 | 0.197 | 0.162 | 1.22 | 400 | 예 |
| 최소 합성 400표 | 중제목 | 2.176 | 0.197 | 11.06 | 400 | 예 |
| 최소 합성 400표 | 제목 | 0.603 | 0.239 | 2.52 | 400 | 예 |
| 내장 표 구조 400표 | 붙임 | 0.651 | 0.272 | 2.39 | 400 | 예 |
| 내장 표 구조 400표 | 상자 | 0.428 | 0.221 | 1.93 | 400 | 예 |
| 내장 표 구조 400표 | 중제목 | 0.762 | 0.296 | 2.57 | 400 | 예 |
| 내장 표 구조 400표 | 제목 | 1.040 | 0.431 | 2.42 | 400 | 예 |

최소 합성 표에는 표 크기 속성이 없어 일부 폭 계산을 건너뛴다. 실제 내장 표 구조는 `sz`·칸 크기·안팎 여백·쪽 폭을 포함한다. 최소 합성 중제목의 11배를 대표 성능으로 주장하지 않는다. 보고서 머리 속도는 미측정이다.

최종 관련 Cargo 시험(`cargo test --release --locked -j 2 --manifest-path hwpx_fmt/Cargo.toml`) 결과:

```text
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s  (rsxml2)
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s  (rsxml3)
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s  (rsxml4)
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s   (structure)
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.23s  (unit)
```

합계 62건, 실패 0. lib/bin/doc-test는 각 0건이며 모두 성공했다. 연결·비교기 별도 시험은 `Ran 9 tests in 0.300s` / `OK`다.

## ALLQA COM 전후 대조

공통 COM 잠금을 하나 소유한 상태에서 다른 빌드·시험 없이 OFF 6문서 → ON 6문서를 순차 실행했다. 각 실행은 20분 제한 안에서 종료했다. 원본 및 처리 사본 해시는 6문서 모두 불변이다.
상세 비교는 수정하지 않은 `scripts/allqa_compare.py`로 생성한 [HWPXFMT5_ALLQA_2026-11-24.md](HWPXFMT5_ALLQA_2026-11-24.md)에 보관했다.

| 문서 | OFF 초 | ON 초 | 문단 수 전→후 | 텍스트/원시 서식/정규화 서식 | 판정 |
|---|---:|---:|---:|---|---|
| txt_short | 31.757 | 24.803 | 4→4 | 모두 일치 | 결과 동일 |
| txt_long | 282.767 | 265.096 | 38→38 | 모두 일치 | 결과 동일 |
| hwp_reference | 63.143 | 54.513 | 16→16 | 모두 일치 | 결과 동일 |
| hwpx_table | 3.665 | 3.259 | — | 결과 없음 | 확인 못 함 |
| hwpx_continuation | 22.472 | 19.842 | 4→4 | 모두 일치 | 결과 동일 |
| hwpx_shapes | 14.026 | 12.917 | 7→7 | 모두 일치 | 결과 동일 |

**6문서 중 결과 5쌍의 차이 0, 1문서는 확인 못 했다. 전체 QA 통과로 보고하지 않는다.** `hwpx_table`은 OFF/ON 모두 한/글 `Open`에서 실패해 서식 처리에 진입하지 못했다. 수정 없이 유지했다.
기존 자동 검수의 원문 텍스트 보존(공백 무시)은 `hwp_reference`·`txt_long`·`txt_short`에서 OFF/ON 모두 FAIL이며, 이번 카드에서 그 원인을 해소하거나 기준을 완화하지 않았다.

이 세트의 ON 로그에는 Rust 조사 11회·Python 폴백 4회·**Rust 실제 적용 0회**가 기록됐다. 서식 프로필이 없는 상자·보고서 머리는 원래 실행되지 않는다. 따라서 전체 시간 감소를 이식 효과로 주장하지 않으며, ALLQA만으로 네 처리기의 실제 변경 후 COM 호환성이 검증됐다고도 하지 않는다.

이를 보완해 `scripts/hwpxfmt5_com_probe.py`로 내장 합성 표의 실제 변경 산출물을 별도 검사했다. 저장소 리소스 패키지는 읽기만 하고 외부 사본에 내장 header/section을 조합했다. Python/Rust 출력 각각을 독립 한/글로 열어 쪽 수와 추출 텍스트 해시를 대조했다.

| 처리기 | 실제 변경 수(Python/Rust) | 정규화 출력 | COM 쪽 수 | COM 텍스트 |
|---|---:|---|---:|---|
| 붙임 | 1/1 | 동일 | 1/1 | 동일 |
| 상자 | 1/1 | 동일 | 1/1 | 동일 |
| 중제목 | 1/1 | 동일 | 1/1 | 동일 |
| 제목 | 1/1 | 동일 | 1/1 | 동일 |

8개 산출물 모두 열기 성공, 입력 및 패키지 리소스 원본 해시 불변. [HWPXFMT5_COM_2026-11-24.json](HWPXFMT5_COM_2026-11-24.json)에 결과를 보관했다. 최초 제목 합성 자료 준비에서 defusedxml에 없는 `Element` 생성 API를 사용해 실패했고, 파싱된 요소의 타입으로 생성하도록 고친 뒤 제목만 재시험해 통과했다. 사용자 문서는 쓰지 않았다. 이는 열기·쪽 수·텍스트 검사이며 PDF/화면 전수 대조는 하지 않았다.

## 개발 중 확인

초기 대조 186쌍 중 185쌍 일치. `attp_table.hwpx`의 붙임 처리 1쌍은 헤더에 `fontfaces`가 없어 양쪽 모두 거부했다. 통과에 넣지 않는다.
처음 하네스 실행은 누락된 `re` import 때문에 중단되어 수정했다. 실행 중 EXE를 Cargo가 교체하려다 Windows 접근 거부가 발생했으며, 이후 빌드와 하네스를 순차 실행한다.

첫 확대 하네스: 234쌍 중 정규화 바이트 일치 233, 차이 0, 양쪽 거부 1, 실제 변경 84쌍. 미지원 조사/적용 검사 8건 통과. 기존 포함 Cargo 62건 통과(16+13+16+4+13). 연결·비교기 Python 9건 통과.

첫 버전의 단독 측정(400표, 각 1회, Python 조사+적용 / Rust CLI 기동 포함 조사+적용):

| 처리기 | Python 초 | Rust 초 | 배수 | 변경 수 | 동일 |
|---|---:|---:|---:|---:|---|
| 붙임 | 0.394 | 0.178 | 2.21 | 400 | 예 |
| 상자 | 0.249 | 0.173 | 1.44 | 400 | 예 |
| 중제목 | 2.828 | 0.233 | 12.11 | 400 | 예 |
| 제목 | 0.612 | 1.046 | 0.59 | 400 | 예 |

제목은 첫 버전에서 느려졌다. 표마다 날짜 정규식을 다시 컴파일하던 부분을 캐시로 고쳤고, 변경 후 결과는 위 최종 하네스·속도 절에 기록했다. 첫 결과를 숨기거나 가장 빠른 회차를 고르지 않았다.

## 검증 명령과 후속 검사

```powershell
$env:CARGO_TARGET_DIR = "<저장소 밖 빌드 폴더>"
$env:PATH = "$env:LOCALAPPDATA/DocFitTools/mingw64/bin;$env:PATH"
cargo test --release --locked -j 2 --manifest-path hwpx_fmt/Cargo.toml
python scripts/hwpxfmt5_parity.py --bin "<실행 파일>" --out "<외부 폴더>" --bench
python scripts/hwpxfmt5_com_probe.py --bin "<실행 파일>" --out "<외부 폴더>"
$env:HWPX_FMT_BIN = "<실행 파일>"
python -m unittest tests.test_hwpxfmt5_parity tests.test_hwpx_fmt_backend
python -m unittest discover -s tests
```

실제 명령은 지정된 Python 3.15.2 가상환경으로 실행했다. `HWPX_FMT_BIN`을 지정한 신규 연결·native 차이 대조 시험은 `Ran 10 tests in 49.655s` / `OK`(skip 0)다. 기존 RSXML 전체 하네스와 마지막 전체 Python 시험은 후속 기록에 실제 결과를 적는다.

## 기존 처리기 회귀 대조

수정하지 않은 `scripts/rsxml_parity.py`를 별도로 단독 실행했다. 문서 1,099건·2,751쌍 모두 하네스 통과, 실패 0, 실제 변경 828쌍, 엄격 공백 비교 잡음 0, 미지원 검사 55건 모두 통과, 종료 코드 0(324.24초)이다. 기존 하네스의 통과에는 양쪽 거부 224쌍이 포함된다. 새 처리기의 양쪽 거부 1쌍은 위와 같이 통과에서 분리했다.

신규 native 검사와 기존 native 하네스는 실행 파일 경로를 명시해 각각 완료했다. 마지막 전체 unittest는 별도 기본 환경에서 실행한다(해당 프로세스에는 `HWPX_FMT_BIN`·`CARGO_TARGET_DIR` 미지정). 전체 시험의 조건부 native skip과 실제 별도 native 검증 결과를 구분해 기록한다.
