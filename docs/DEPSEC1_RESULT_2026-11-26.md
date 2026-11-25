# DEPSEC1 결과 — 의존성 취약점 해소 (CILINUX2 목록)

- 대상: `feature/DEPSEC1` (`feature/CILINUX2` 를 빨리 감기 병합한 뒤 작업). 커밋까지만 하며 푸시·태그·병합·배포는 하지 않았다.
- 판정: CILINUX2 목록 중 **알려진 취약점 11건은 모두 해소**했다. **유지보수 중단 경고는 남아 있어 OSV 감사 작업은 계속 실패 상태**다(점검 범위 축소·advisory 무시 없음).

## 변경

| 항목 | 이전 → 이후 | 해소한 보고 |
|---|---|---|
| `pdf_edit_core` lopdf | 0.34.0 → 0.42.0 | RUSTSEC-2026-0187 |
| `hwpx_fmt` quick-xml | 0.37.5 → 0.41.0 | RUSTSEC-2026-0194·0195 |
| `requirements.txt` | `bottle==0.13.4` 명시(잠금 파일과 동일) | PYSEC-2014-77·2021-129·2022-227 |
| `bench/kb1` url | 2.5.8 → 2.5.9 (form_urlencoded 1.2.2 → 1.2.3 동반) | RUSTSEC-2026-0592 |

## 버전만 올리면 생기는 동작 변화와 처리

### lopdf 0.42 (사용자 PDF 처리)

컴파일은 수정 없이 되지만 읽기 동작이 달라졌다.

- **빈 암호로 열리는 암호화 PDF(열기 암호 없이 권한 제한만 있는 것)를 읽으면서 복호화하고 `/Encrypt` 를 지운다.** 그대로 두면 편집 대상으로 통과하고 저장 사본에서 보호가 벗겨진다. RC4 40/128·AES 128/256 합성 파일 4종에서 실제로 재현했다. `was_encrypted()` 까지 검사해 거부를 복원했고, 분류기의 `PDF_RESTRICTED` 판정은 복호화 상태의 권한값으로 유지했다.
- **`%PDF-` 앞에 군더더기 바이트가 있어도 읽는다.** 예전처럼 맨 앞 헤더를 요구하는 검사를 편집·바이트 열기·합치기 입력에 추가했다.
- CLI 합치기(`merge`)는 `open_for_merge` 로 같은 헤더·암호 검사를 쓴다.
- 회귀 시험 2개 추가(`tests/safety.rs`): 암호화 8종·앞 군더더기 1종이 모든 편집 경로에서 거부되는지, 20만 단계 중첩 객체에서 프로세스가 죽지 않는지.
- 거부 파일의 CLI 오류 문자열은 일부 달라진다(예: `Pdf(Trailer)` → `Pdf(Parse(InvalidTrailer))`, 헤더 오류는 `Invalid("Invalid PDF header")`). 거부 여부는 같다.

**0.47 을 선택하지 않은 이유:** 0.47.0 은 `ttf-parser` 대신 `skrifa` 를 써 아래 새 경고가 없어지지만, 시험해 보니 잘린 파일·잘못된 xref 위치 등 합성 손상 PDF 6종(`bad_stream_length`, `crlf_whole_file`, `garbled_startxref`, `missing_endobj`, `shifted_offsets`, `truncated_half`)을 정상(OK)으로 받아들여 분류 정합성 시험이 실패했다.

### quick-xml 0.41

- 0.38 부터 텍스트의 `&…;` 가 별도 `GeneralRef` 이벤트로 온다. 컴파일 오류만 고치면 해당 글자가 유실된다. `dom.rs` 에서 문자 참조·기본 엔티티 5종을 이어 붙이고, 정의되지 않은 엔티티·허용되지 않는 문자·루트 밖 참조는 거부한다.
- 0.41 의 속성값 API 는 속성 안의 실제 줄바꿈·탭을 공백으로 바꾼다. 서식 보존을 위해 이전처럼 참조만 푼다.
- `루트 밖 참조` 거부만 이전보다 엄격하다(이전에는 `&#32;` 처럼 공백으로 풀리면 통과).
- 시험 2개 추가(`tests/unit.rs`). **같은 시험 파일을 이전 quick-xml 0.37 소스에 넣어도 통과**해 파서 동작이 같음을 확인했다.

## 시험 결과 (Windows, rustc 1.99.1 gnu + 휴대용 MinGW)

`cargo test --release --locked`

| 크레이트 | 변경 전 | 변경 후 (`test result` 줄 순서) |
|---|---|---|
| `pdf_edit_core` | 3·0·2·7·6·0 통과 | `ok. 3 passed` / `ok. 0` / `ok. 2` / `ok. 7` / `ok. 8` / `ok. 0` — 실패 0 |
| `hwpx_fmt` | 0·0·16·13·16·5·13·0 통과 | `ok. 0` / `ok. 0` / `ok. 16` / `ok. 13` / `ok. 16` / `ok. 5` / `ok. 15` / `ok. 0` — 실패 0 |
| `bench/kb1` (`cargo test --locked`, dev) | 측정 안 함 | `ok. 0 passed; 0 failed` (시험 없는 크레이트, 컴파일 확인) |

### 보존 회귀 (합성 파일만 사용, 원본 미수정)

- **PDF 신·구 대조:** 입력 13개(상속 자원·글꼴·주석·책갈피·메타데이터·객체 스트림 포함)에 대해 count·info·rotate·reorder·delete·split·merge **100건 모두 동일**(결과 파일 83개의 객체 그래프·메타데이터·쪽 렌더 픽셀·본문). 분류 합성 파일 전체의 `info` 종료 코드 성공/실패도 이전과 같다.
- **중첩 공격 재현:** 중첩 10,000·200,000 단계에서 이전 실행 파일은 스택 오버플로(0xC00000FD)로 종료, 새 실행 파일은 정상 처리.
- **HWPX 파이썬 대조(`scripts/rsxml_parity.py`):** 이전·새 실행 파일 모두 1,099문서·2,751쌍 통과, 실패 0, 미지원 검사 55건 실패 0.
- **HWPX 신·구 대조:** 위 하네스 출력 6,066개의 ZIP 항목 내용 차이 0. 본문에 참조 문자를 주입한 367문서(텍스트 노드 3,013곳)를 처리기 19종에 돌린 6,973회 결과(종료 코드·출력 JSON·ZIP 항목) 모두 동일.

### 전체 Python 시험

`python -m unittest discover -s tests` (Python 3.15.2, `KLIB_EXE` 는 같은 klib 소스의 기존 release 빌드, `HWPX_FMT_BIN` 은 새 빌드, PDF 시험은 새 `pdf-edit.exe`). 종료 코드 0.

```text
Ran 3751 tests in 1509.236s

OK (skipped=22)
```

- `OK` 뒤 atexit 샌드박스 정리의 `PermissionError` 출력은 CILINUX2 에도 기록된 기존 현상이며 실패·오류에 포함되지 않는다.
- 시험이 재생성한 `tests/fixtures/timemachine_extract` 바이너리 4개는 되돌렸다.
- 한/글 COM 잠금은 다른 카드(KORDOCD1) 이름으로 05:35 부터 남아 있어 **획득하지 않고 그대로 두었다.** 해당 카드 기록은 Codex 사용 한도 오류로 끝났고 관련 시험 프로세스는 없었다. unittest 에서 실제 한/글 COM 을 띄우는 호출은 검색되지 않았고, 시험 전후 `Hwp.exe` 프로세스 목록이 같았다.
- 제3자 구성요소 목록 스크립트: 변경 전후 모두 구성요소 564개·확인 필요 5개.

## OSV 재점검

OSV-Scanner v2.7.0(Windows), 워크플로와 같은 `scan source --recursive` + Python 잠금 파일 2개. 결과: **5개 항목, 모두 수정 버전이 없는 유지보수 중단 경고**(CILINUX2 의 8항목·11건 중 취약점은 0건).

| 패키지 | 보고 ID | 위치 | 조치안(이번에는 적용하지 않음) |
|---|---|---|---|
| chrono 0.4.45 | RUSTSEC-2026-0543 | `pdf_edit_core`, `bench/kb1` | PDF: lopdf 를 `default-features = false, features = ["rayon"]` 로 두면 빠질 것으로 보인다(날짜 기능 영향 확인 필요). 벤치: seekstorm 갱신·대체 |
| **ttf-parser 0.25.1 (이번에 새로 유입)** | RUSTSEC-2026-0192 | `pdf_edit_core` (lopdf 0.42 필수 의존성) | lopdf 의 `skrifa` 판(0.47.0 에서 확인)으로 갱신. 먼저 손상 PDF 관대 처리 차이를 strict 읽기 옵션 등으로 막고 분류 정합성 6건을 통과시켜야 한다 |
| number_prefix 0.4.0 | RUSTSEC-2025-0119 | `bench/kb1` | indicatif 상위 의존성 갱신 |
| paste 1.0.15 | RUSTSEC-2024-0436 | `bench/kb1` | tokenizers 상위 의존성 갱신 |

## 남은 문제·미확인

- 위 경고 5건 때문에 `dependency-audit` 작업은 계속 실패한다.
- CI(원격·MSVC 빌드, Linux OSV)는 푸시하지 않아 확인하지 못했다. 저장소에 커밋된 번들 실행 파일은 갱신하지 않았으므로 **배포본에 반영되려면 CI 빌드가 필요**하다.
- `pdf-edit.exe` 크기가 724,992 → 1,081,344 바이트로 늘었다(lopdf 하위 의존성 증가). `hwpx_fmt.exe` 는 3,316,736 → 3,326,464.
- 기존 문제(이번에 바꾸지 않음): CLI `merge` 는 전자서명 PDF 를 거부하지 않는다(이전 실행 파일도 동일). 앱 경로는 입력마다 파이썬 분류기를 먼저 거치므로 거부한다.
- 기존 문제(이번에 바꾸지 않음): `hwpx_fmt` 는 속성 안의 실제 줄바꿈·탭을 그대로 두어 파이썬 ElementTree 와 다를 수 있다.
