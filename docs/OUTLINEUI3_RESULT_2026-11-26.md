# OUTLINEUI3 결과 — Rust 창 슬래시·자동완성·빠른 검색·명령 팔레트

## 결과

- `feature/OUTLINEUI3`에서 구현·자동 시험 완료. 시작 시 OUTLINEUI2 `e4a5e2d8`의 커밋과 core 130개·win 159개·관련 Python 33개 통과 기록을 확인하고 `git merge feature/OUTLINEUI2`를 실행했다. 병합 커밋은 `9cf919d4`, 충돌 없음. 병합 직후 win 159개 재시험 통과.
- 최종 **outliner_win 전체 174개, outliner_core 전체 130개, 관련 Python 33개 통과**, 실패·제외 0. 새 시험 15개를 추가했고 기존 시험은 수정하지 않았다. 제품 변경은 `outliner_win/src` 8개 파일, 새 시험 파일 1개다.
- 화면·후보·검색·명령 실행은 Rust+Win32/GDI로 구현했다. 외부 의존성·XML 파서·WebView2·COM 사용을 추가하지 않았다. 기존 Markdown 파일 형식을 유지한다.
- 로컬 커밋까지만 수행한다. 푸시·태그·main 병합·배포·GitLab 접근 없음. 설치본·서비스·자동 실행·사용자 설정·기본 실행 선택은 변경하지 않았다. Flutter 앱도 보존한다. 기본 Rust 창 전환은 카드 지시대로 PM·사용자 확인 단계다.

## 동작과 단축키

| 입력/진입 | 구현한 동작 |
|---|---|
| `/` | TODO·DONE, h1~h3, 오늘 날짜, 페이지 참조, 블록 참조 **8종**. 한글 설명과 영문 명령으로 필터. 상태 명령은 core의 실제 TODO/DONE 상태를 변경한다. |
| `[[` | 최근 페이지 우선·접두/포함 후보, 닫는 괄호 중복 방지·UTF-16 캐럿 보존. 일치 후보가 없으면 기존 동작대로 닫는다. 페이지 생성은 빠른 검색의 명시적 생성 행을 이용한다. |
| `((` / `(((` | 현재 문서 및 타 페이지 영구 ID 후보. ID와 페이지/본문 설명을 구분해 표시한다. 현재 문서의 임시 숫자 ID는 선택 시 UUID 형태의 영구 `id::` 속성을 만들며 참조 삽입과 함께 한 단계로 되돌린다. |
| `#` | 그래프의 태그·페이지 이름·현재 문서 태그로 완성. 공백이 있는 이름은 `#[[이름]]`으로 삽입. 완성 직후 같은 트리거에서 재개방하지 않는다. |
| Ctrl+K | **빠른 검색**. 빈 입력은 최근 페이지, 1글자부터 페이지 이름, 2글자부터 블록 본문. 기존 페이지·블록·새 페이지 생성 행을 함께 표시한다. 생성 행을 선택하기 전에는 파일을 만들지 않는다. |
| Ctrl+Shift+P | **명령 팔레트 11종**: 저장·실행 취소·다시 실행·좌/우 패널·오늘 일지·새 페이지·이름 변경·TODO 순환·접기·펼치기. 기존 Editor/Shell 동작으로 실행한다. |
| 후보 ↑↓ / Ctrl+P·Ctrl+N | 이전/다음 후보. Enter 또는 기존 호환 키 Tab으로 완성, Esc로 닫기. 검색/팔레트는 ↑↓·Enter·Esc 및 결과 클릭. |

Logseq 기준은 파일 그래프 판 **0.10.9** 공개 소스다. [자동완성 Enter·이동 키 정의](https://github.com/logseq/logseq/blob/0.10.9/src/main/frontend/modules/shortcut/config.cljs#L145-L157), [검색 mod+k·팔레트 mod+shift+p](https://github.com/logseq/logseq/blob/0.10.9/src/main/frontend/modules/shortcut/config.cljs#L320-L325), [슬래시 트리거와 명령 목록](https://github.com/logseq/logseq/blob/0.10.9/src/main/frontend/commands.cljs)을 확인했다. Windows의 mod를 Ctrl로 매핑했고 Tab 완성은 기존 Rust 창 호환 동작이다. Logseq 앱을 실행한 비교는 하지 않았으며 소스·자산은 복사하지 않았다.

## 상태·IME·보존

- 검색 상태를 `SearchMode::{Legacy, Quick, Commands}`로 명시했다. 기존 일반 검색의 2글자 규칙을 유지한다. 편집 확정에 실패하면 빠른 검색/팔레트로 넘어가지 않는다. 새 페이지·이름 변경은 기존 이름 입력 상태로 연결한다.
- 완성 문맥은 Page/Block/TripleBlock/Tag/Slash로 구분한다. 치환 직전에 트리거·질의·캐럿·선택 범위를 다시 확인한다. 오래된 문맥이나 선택 범위가 바뀌면 치환을 거부한다. 일반 URL·단어 내부 `/`·`#`·열린 백틱 코드에서는 후보를 열지 않는다.
- 한글 조합 중 기존 후보를 고정하고 클릭 치환을 거부한다. 첫 트리거는 확정된 접두를 표시용으로 읽을 수 있으나 조합 글을 문서에 반영하지 않는다. 조합/후보창의 화살표·Esc는 IME에 맡긴다. Enter/Tab은 먼저 조합만 확정하고, 50ms 안 후속 중복 키도 후보 선택으로 사용하지 않는다.
- 검색 EDIT는 별도 조합/후보 상태를 갖는다. 조합 중 검색 결과를 갱신하거나 Enter/결과 클릭으로 페이지를 생성하지 않는다. 조합 종료 후 질의를 갱신하며 종료 직후 중복 Enter를 억제한다.
- 자동 시험은 한글 문자열·이모지·UTF-16 오프셋, 네 종류 트리거의 조합 경계, 실제 Win32 EDIT의 `WM_IME_STARTCOMPOSITION`/`WM_IME_ENDCOMPOSITION`, Enter/클릭 경로를 확인했다. **실제 한국어 IME의 물리 키 입력·후보창 조작은 미확인**이다. 메시지 주입 시험을 실키보드 시험으로 표현하지 않는다.
- 기존 Rust 파일은 UTF-8/LF, 새 텍스트 파일은 UTF-8/CRLF를 사용했다. 기존 TODO·인계 파일의 줄바꿈을 유지했다. 본체 `hwp-auto-docfit.py`는 수정하지 않았으며 CRLF **3,523줄**을 확인했다. PC 전용 빌드 경로는 저장소 설정에 추가하지 않고 `CARGO_TARGET_DIR` 및 로컬 MinGW PATH 환경변수로만 지정했다.

## 실패·보완 기록

1. 영구 ID 속성 삽입 코드의 동시 가변/불변 빌림으로 컴파일 오류 E0502 발생. 줄 수를 먼저 계산해 빌림을 분리했다.
2. 새 시험 15개 중 1개가 트리거 위치를 바꿔 놓고 같은 위치의 억제를 기대했다. 새 시험 입력을 동일 위치로 바로잡았다. 기존 시험 수정 없음.
3. 첫 전체 시험에서 기존 `enter_while_composing_with_popup_open_only_completes_composition` 1개 실패. 조합 중 일괄 후보 억제가 첫 `[[` 문맥까지 막아 Enter가 블록 분할로 전달됐다. 첫 접두는 표시용으로 인식하고 기존 후보만 고정하도록 수정한 뒤 관련 **46개** 및 최종 전체 시험 통과.
4. 육안 확인에서 192 DPI의 본문 EDIT 글자가 보이지 않음을 발견했다. 실제 문자열은 남았지만 내부 서식 사각형이 `(8, 0, -8, 0)`이었다. 초기 여러 줄 EDIT 크기를 DPI별 200×28 기준으로 생성해 글꼴보다 작은 초기 영역을 없앴다. 3테마×2배율 내부 서식 영역 양수 회귀 검사와 실제 캡처로 보완을 확인했다.
5. 독립 캡처 도구의 첫 직접 rustc 링크에서 Windows import library 경로 누락으로 실패했다. Cargo가 사용하는 native library 경로를 명시해 해결했다. 제품 의존성·빌드 설정 변경 없음.

관련 Python은 이 작업 폴더에 `.venv`가 없어 설치된 Python 3.14로 실행했다. 새 환경 설치나 설정 변경 없음. 최종 시험의 TEMP/TMP·로그·캡처 도구는 Git 제외 `outliner_win/target/`에 격리했다. PowerShell이 stderr를 `NativeCommandError`로 감싼 출력이 있으나 최종 시험 프로세스 종료 코드는 모두 **0**이다.

## 실창 캡처

합성 페이지로 실제 화면 밖 Win32 창을 만들고 제품과 같은 GDI/EDIT 경로를 캡처했다. 슬래시·빠른 검색·팔레트 **3상태 × 3테마 × 2배율 = 18장**. PNG의 IDAT만 재압축했고 해제한 화소가 동일함을 검증했다. **103,978,044 → 629,427바이트**. 최종 검토에서 slash_light_96·slash_hc_192·quick_light_96·quick_dark_192·palette_dark_96·palette_hc_192의 **6장**을 직접 열어 확인했다. 나머지 12장은 생성·압축 검증만 했다.

| 상태·테마 | 96 DPI | 192 DPI |
|---|---|---|
| 슬래시·밝게 | [캡처](assets/OUTLINEUI3_2026-11-26/slash_light_96.png) | [캡처](assets/OUTLINEUI3_2026-11-26/slash_light_192.png) |
| 슬래시·어둡게 | [캡처](assets/OUTLINEUI3_2026-11-26/slash_dark_96.png) | [캡처](assets/OUTLINEUI3_2026-11-26/slash_dark_192.png) |
| 슬래시·고대비 | [캡처](assets/OUTLINEUI3_2026-11-26/slash_hc_96.png) | [캡처](assets/OUTLINEUI3_2026-11-26/slash_hc_192.png) |
| 빠른 검색·밝게 | [캡처](assets/OUTLINEUI3_2026-11-26/quick_light_96.png) | [캡처](assets/OUTLINEUI3_2026-11-26/quick_light_192.png) |
| 빠른 검색·어둡게 | [캡처](assets/OUTLINEUI3_2026-11-26/quick_dark_96.png) | [캡처](assets/OUTLINEUI3_2026-11-26/quick_dark_192.png) |
| 빠른 검색·고대비 | [캡처](assets/OUTLINEUI3_2026-11-26/quick_hc_96.png) | [캡처](assets/OUTLINEUI3_2026-11-26/quick_hc_192.png) |
| 팔레트·밝게 | [캡처](assets/OUTLINEUI3_2026-11-26/palette_light_96.png) | [캡처](assets/OUTLINEUI3_2026-11-26/palette_light_192.png) |
| 팔레트·어둡게 | [캡처](assets/OUTLINEUI3_2026-11-26/palette_dark_96.png) | [캡처](assets/OUTLINEUI3_2026-11-26/palette_dark_192.png) |
| 팔레트·고대비 | [캡처](assets/OUTLINEUI3_2026-11-26/palette_hc_96.png) | [캡처](assets/OUTLINEUI3_2026-11-26/palette_hc_192.png) |

## 미확인·남은 범위

- 실제 IME/후보창·다중 모니터 DPI 이동·실키보드/포인터 장시간 사용, PM의 전체 Python·Flutter·CI/MSVC·설치본 연결·기본 Rust 창 전환은 별도 확인이다.
- 타 페이지 자동완성은 이미 영구 `id::`가 있는 블록만 대상으로 하며 후보 조회 과정에서 다른 파일에 ID를 만들지 않는다. 타 페이지 중복 ID는 후보에서 제외한다. 그래프 후보는 페이지 전환·목록 갱신 시 읽는 스냅숏으로 외부 편집의 실시간 감시는 이번 범위 밖이다.
- 공백 태그는 기존 엔진에서 페이지 링크로도 인식되는 `#[[이름]]` 표기로 보존한다. core의 태그 검색 의미를 확장하지 않았다. Logseq 전체 명령·플러그인·명령 사용자 지정은 제공하지 않는다.
- 속도·메모리 측정을 하지 않았으며 성능 개선이나 목표 달성을 주장하지 않는다. 사용자 문서는 사용하지 않았다. COM 시험도 하지 않았다.

## 실제 최종 출력

`cargo test --manifest-path outliner_win/Cargo.toml --locked -j 2` (exit 0)

```text
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.83s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.59s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.02s
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.34s
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.56s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.18s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.38s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.37s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cargo test --manifest-path outliner_core/Cargo.toml --locked -j 2` (exit 0)

```text
test result: ok. 84 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.24s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.52s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`python -m unittest tests.test_outliner_service tests.test_bridge_methods tests.test_package_integrity` (exit 0)

```text
Ran 33 tests in 3.206s
OK
```

캡처 도구 실제 출력(exit 0):

```text
Win32 captures: 18 (3 states x 3 themes x 2 DPI)
PNG: 18 files; 103978044 -> 629427 bytes; decompressed pixels identical
UTF-8 / existing line endings preserved; hwp-auto-docfit.py CRLF=3523
```
