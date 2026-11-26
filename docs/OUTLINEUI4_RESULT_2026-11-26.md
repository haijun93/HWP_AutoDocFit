# OUTLINEUI4 결과 — Rust 창 참조·임베드·속성·태그

## 결과

- 시작 때 `feature/OUTLINEUI3`의 커밋 `43c40f93`과 통과 기록(win 174·core 130·관련 Python 33)을 확인하고 fast-forward 병합했다.
- **최종 outliner_win 187개, outliner_core 147개, 관련 Python 33개 통과. 실패·제외 0, 컴파일 경고 0.** 새 시험 30개(core 17·win 13). 기존 시험 파일은 바꾸지 않았다.
- 재개 경위: 앞 실행은 사용 한도로 멈췄고 이어받은 실행은 명령을 돌리지 못했다. 재개 실행에서 미커밋 변경을 직접 빌드·시험하고(수정 전 win 185·core 143 통과) 코드를 검토해 아래 결함을 고친 뒤 다시 시험했다. 이 문서의 출력 줄은 재개 실행의 것이다.
- Rust+Win32/GDI에서 연결 참조·미연결 언급, 블록 참조 인라인 본문, 블록/페이지 임베드, 속성 추가·수정, 정확한 태그 검색을 구현했다. 외부 의존성·XML 파서·WebView2·COM 추가 없음.
- 로컬 커밋만. 푸시·태그·main 병합·배포·GitLab 접근 없음. 설치본·서비스·자동 실행·사용자 설정·기본 실행 선택·Flutter 변경 없음.

## 재개 검토에서 고친 결함

새 시험 6개로 먼저 실패를 재현(core 4·win 2 FAILED)한 뒤 고쳤다.

1. 괄호가 든 페이지 이름(`[[회의(1차)]]`)과 태그가 참조로 인식되지 않아 클릭되지 않았다. 짝 없는 백틱 하나가 뒤의 참조를 모두 가렸다.
2. 여러 줄 코드 울타리 안의 참조가 풀려 표시되고 클릭 대상이 됐다.
3. 일반 페이지 링크에 대상이 없으면 `· 없음`이 붙고 원문 표기가 바뀌었다. 표시 경로에서 `collapsed:: true` 글자가 다시 보였다.
4. 속성 수정이 속성을 맨 뒤로 옮겼고, 본문 줄에 적힌 속성은 중복 줄이 생겼다.
5. 속성 입력 중 페이지를 옮기면 입력이 남아 다른 페이지의 같은 번호 블록에 적용될 수 있었다.
6. 참조 없는 줄도 새 그리기 경로를 탔다.

시험 없이 코드 검토로 함께 고친 것: 페이지 링크 클릭·Shift+클릭을 기존 동작(열기/만들기·카드)으로 되돌림, 데이터 폴더 없는 단일 파일에서 페이지 링크·태그 클릭은 기존처럼 편집 시작, 미연결 언급 재검색을 블록 선택 때마다가 아니라 목록 갱신 때만 수행.

## 구현한 동작

| 항목 | 동작 |
|---|---|
| 연결 참조 | 현재 페이지를 링크/태그/속성 값으로 가리키는 블록과 선택 블록의 역참조를 우측에 표시한다. 기존 페이지 정렬·역링크 클릭 위치를 유지한다. 선택·확정 편집 후 메모리에서 갱신한다. |
| 미연결 언급 | 다른 페이지의 일반 본문에서 현재 페이지 이름을 찾는다. 명시적 링크·태그·코드 구간은 제외한다. 영문·숫자 이름은 단어 경계를 적용하고 한글 이름은 부분 일치를 허용한다. 클릭은 해당 블록 이동만 수행하며 자동 링크 변환은 하지 않는다. |
| `((id))` | UUID 대상 본문을 인라인 표시한다. 읽을 때 ID를 만들지 않는다. 누르면 대상 블록, Shift+클릭은 기존 읽기 전용 우측 카드다. EDIT에서는 원문 문법을 사용하고 Ctrl+클릭으로 따라간다. |
| 임베드 | `{{embed ((id))}}`, `{{embed [[페이지]]}}`, 기존 `(((id)))`를 읽기 전용 본문·하위 계층으로 표시한다. 원문 자식 들여쓰기를 유지하며 표시 줄 수를 클릭·스크롤 계산에도 반영한다. 임베드 안 링크·태그도 같은 GDI 폭으로 판정한다. |
| 속성 | 선택 블록의 전체 `key:: value`를 우측에 표시한다. 같은 이름은 제자리에서 바꾸며 본문 줄에 적힌 속성은 본문 편집으로만 고친다. 속성 행 또는 `＋ 추가`를 누르고 위 입력 줄에서 `이름:: 값`으로 확정한다. Enter는 메모리 변경·실행 취소 한 단계이며, Ctrl+S 또는 기존 명시적 페이지 이동으로 저장한다. Esc는 취소한다. 공백·이모지가 든 값을 보존한다. |
| 태그 | `#태그`, `#[[여러 단어]]`, `tags::`의 쉼표 목록을 지원한다. 본문·우측 태그를 누르면 그래프 전체의 정확히 일치하는 태그 블록을 검색한다. 속성 값의 페이지/블록 참조도 색인한다. 코드 안 참조·태그는 새 사이드바/태그 검색에서 제외한다. |

## 상태·보존·제한

- core `References`는 파일 접근 없는 불변 그래프 스냅숏과 현재 문서 오버레이를 사용한다. 표시 조각과 클릭 표적을 같은 모델로 연결했다. 문서 해시·선택 블록으로 캐시를 무효화하며 키 입력·그리기 때 파일을 다시 읽거나 쓰지 않는다.
- 해결 상태는 `Found / Missing / Ambiguous / Cycle / Limit`다. 누락된 블록 참조·임베드는 `없음`(일반 링크·태그는 원문 그대로), 중복 영구 ID는 `중복 ID/이름`, 순환은 `순환 참조`, 확장 제한은 `확장 한도`로 표시한다. 임시 숫자 ID는 현재 문서에서만 해결한다. 타 페이지의 임시 숫자 ID를 다른 블록으로 연결하지 않는다.
- 조상 경로로 순환을 차단하고 확장 깊이 **8**, 루트 표시당 **64줄·4,096문자** 한도를 둔다. 재귀 인라인 설명은 **160문자**로 줄인다. 임베드는 대상 본문·하위 구조의 미리보기이며 대상 파일을 직접 편집하지 않는다.
- 참조 대상 파일이 스냅숏 뒤 바뀌면 블록 위치 이동을 거부하고 목록을 갱신한다. 다시 눌러 새 위치로 이동한다. 미연결 언급 이동도 같은 원문 변경 검사를 한다. 페이지 링크는 기존 PageStore 열기 동작을 그대로 쓴다.
- 속성 입력은 `PromptKind::Property`와 대상 블록 ID로 상태를 고정한다. 잘못된 입력은 입력 상태를 유지한다. 한글 조합/후보창이 열린 동안에는 속성 입력 상태를 열지 않는다. 속성 값 비우기는 빈 값 저장이며 속성 삭제 전용 UI는 제공하지 않는다.
- 사용자 원본을 사용하지 않았으며 합성 Markdown만 시험했다. 참조 조회·표시·검색 과정에서 파일 생성/내용 변경이 없음을 확인했다. 저장 시험은 합성 파일에서 명시적으로 실행했다.
- 기존 UTF-8·줄바꿈을 보존했다. 기존 Rust 파일은 LF, 새 텍스트 파일은 CRLF다. 본체 `hwp-auto-docfit.py` 변경 없음, CRLF **3,523줄**. 빌드·MinGW·TEMP/TMP 경로는 환경변수로만 지정했고 PC 전용 경로를 저장소 파일에 추가하지 않았다.

## 실패·보완 기록(앞 실행 기록)

초기 컴파일 오류(E0499), 조합 중 속성 입력이 열리는 문제, 기존 shell_window 시험 2개 실패(정렬·첫 역링크 행 위치), 임베드 자식 들여쓰기 누락을 수정했다. 기존 시험은 수정하지 않았다.

## 실창 캡처

재개 수정 뒤 합성 페이지로 3테마 × 2배율 = 6장을 다시 만들었다(IDAT만 재압축, 화소 동일). 밝게 96 DPI·어둡게 192 DPI 2장을 직접 열어 참조 본문·자식 들여쓰기·속성·태그·우측 언급을 확인했다. 나머지 4장은 생성만 확인했다.

| 테마 | 96 DPI | 192 DPI |
|---|---|---|
| 밝게 | [캡처](assets/OUTLINEUI4_2026-11-26/references_light_96.png) | [캡처](assets/OUTLINEUI4_2026-11-26/references_light_192.png) |
| 어둡게 | [캡처](assets/OUTLINEUI4_2026-11-26/references_dark_96.png) | [캡처](assets/OUTLINEUI4_2026-11-26/references_dark_192.png) |
| 고대비 | [캡처](assets/OUTLINEUI4_2026-11-26/references_hc_96.png) | [캡처](assets/OUTLINEUI4_2026-11-26/references_hc_192.png) |

## 미확인·남은 문제

- 실제 키보드 한글 IME·후보창, 다중 모니터 DPI 이동, 장시간 포인터 사용, CI/MSVC, 설치본 연결, 기본 Rust 창 전환은 확인하지 않았다. Win32 시험은 메시지 주입·화면 밖 창이다.
- 전체 Python·Flutter는 PM 릴리스 범위다. `.venv`가 없어 PC의 Python으로 관련 33개만 실행했다.
- 속도·메모리 측정 없음. 화면 갱신마다 문서 직렬화·해시를 하고, 선택 블록이 바뀌면 연결 참조를 그래프 전체에서 다시 찾으며, 페이지를 열 때 전체 페이지를 읽는다. 큰 그래프에서의 지연은 측정이 필요하다.
- 임베드 첫 줄은 대상 첫 줄 요약이라 아래 계층의 첫 줄과 같은 글이 두 번 보인다. 긴 표시 줄은 줄바꿈 없이 잘린다.
- 참조 없는 줄의 코드 안 `[[링크]]`는 기존처럼 강조·클릭된다.
- 우측의 `Shift+클릭으로 카드를 여세요` 안내는 블록이 없는 페이지에서만 보인다.
- 외부 파일 실시간 감시·미연결 언급 일괄 변환·속성 삭제 UI·임베드 안 직접 편집은 범위 밖. 참조/언급·태그 목록은 최대 300건.

## 실제 최종 출력

종료 코드 모두 0. 로컬 MinGW(GNU 도구 모음), 빌드·TEMP 경로는 환경변수로만 지정했다.

`cargo test --manifest-path outliner_win/Cargo.toml --locked -j 2 --no-fail-fast`

```text
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.22s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.10s
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.69s
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.82s
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.64s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.17s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.48s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.07s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cargo test --manifest-path outliner_core/Cargo.toml --locked -j 2 --no-fail-fast`

```text
test result: ok. 84 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.39s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.70s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`python -m unittest tests.test_outliner_service tests.test_bridge_methods tests.test_package_integrity`

```text
Ran 33 tests in 5.254s
OK
```

캡처 압축 도구 출력:

```text
PNG: 6 files; 44938833 -> 221290 bytes; decompressed pixels identical
```
