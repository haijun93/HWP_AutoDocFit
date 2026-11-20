# OUTLINERWIN3 결과 — 아웃라이너 Win32 3단계: 핵심 편집(입력·Enter 분할·Tab·삭제·저장)

작성일: 2026-11-21 · 브랜치 `feature/OUTLINERWIN3` · 설계: [OUTLINER_WIN32_DESIGN_2026-11-20.md](OUTLINER_WIN32_DESIGN_2026-11-20.md) §3·§4 · 앞 카드: [OUTLINERWIN2_RESULT_2026-11-21.md](OUTLINERWIN2_RESULT_2026-11-21.md)

Python·Flutter·`hwp-auto-docfit.py`·엔진(`outliner_core`) 변경 없음. 새 외부 크레이트 없음(`serde_json` 은 엔진이 이미 쓰는 것).

## 1. 변경 파일 (`outliner_win/`)

| 파일 | 내용 |
|---|---|
| `src/edit.rs` (새) | **편집 상태 기계**(Win32 없음). `Editor<H: EditHost>` — 편집 시작·Enter 분할·Tab/Shift+Tab·↑↓ 이동·Backspace 삭제/합치기·Esc 취소·Ctrl+S·창 닫기 직전 확정. `ImePolicy`(조합 중 키 처리 정책 구조체). IME 순서 안전 규칙 5개(파일 머리 주석). |
| `src/docio.rs` (새) | 엔진 호출은 전부 `outliner_core::handle_request` 직접 호출(`request`·`parse_doc`·`edit_doc`), 파일 읽기(`load_file`)·저장(`save_checked`). |
| `src/sha256.rs` (새) | SHA-256 구현(NIST 시험 벡터 4개). 저장 충돌 비교용. |
| `src/window.rs` | 읽기 전용 → 편집 가능. 편집 중 블록 위에만 Win32 EDIT(한 줄 `ES_AUTOHSCROLL`) 겹침, EDIT 서브클래스(키·IME 메시지 → `KeyEvent`), 100 ms 타이머(확정 대기 시간 제한·제목 ●), 닫기 확인, 저장 경고 대화상자. 캡처·시험용 `Harness`(화면 밖 실제 창 + 실제 EDIT). |
| `src/paint.rs` | `draw_ext`(편집 줄 테두리·머리글 안내·제목 ●), `Metrics::edit_rect`(EDIT 위치 계산). 기존 `draw` 는 그대로(래퍼). |
| `src/capture.rs`·`src/main.rs` | `--capture-edit`: 편집 중 PNG 3장 추가. 파일 인자는 `load_file` 로 한 번만 읽어 SHA-256 기억. |
| `tests/edit_machine.rs`(30)·`tests/save_io.rs`(6)·`tests/edit_window.rs`(4) | 새 시험 40개. |

## 2. 동작

- **편집 시작**: 선택 블록에서 Enter·F2·더블클릭. EDIT 는 블록 글자 위치에 정확히 겹치고(TODO·DONE 줄은 체크 칸 오른쪽), 글꼴(맑은 고딕)·색은 목록과 같은 `ui_theme` 팔레트. 편집 글은 블록의 **원문 글**(`collapsed:: true` 같은 속성 글자도 그대로 — 지우지 않기 위해).
- **Enter = 커서 위치 분할**(선택 영역이 있으면 지우고 분할). **엔진에 커서 위치 분할이 없어**(main 의 `split` 은 구분 문자열 기준) 임시로 `set_text(앞)` + `insert(뒤)` 로 구성한다 — 한 트랜잭션(둘 다 성공해야 반영). 엔진에 커서 분할이 들어오면 `edit.rs` 의 `Cmd::Enter` 한 곳만 바꾸면 된다. 분할 뒤 새 블록 편집으로 이동, 캐럿은 맨 앞.
- **Tab / Shift+Tab** = `indent` / `outdent`(글 먼저 반영). 첫 블록·최상위 오류는 머리글 오른쪽에 안내하고 입력은 그대로.
- **↑ / ↓** = 글 반영 후 이웃 줄 편집으로 이동. 접힌 하위는 건너뜀(보이는 줄 기준).
- **Backspace**: 빈 블록이면 `delete`(앞 줄 끝으로 이동, 마지막 한 블록은 안 지움), 캐럿이 맨 앞이면 앞 블록과 `merge`. 엔진 `merge` 는 두 글 사이에 공백 1개를 넣는다(Flutter 는 공백 없음 — **차이**, 설계 4절 표 7 에 예고된 것). 앞 블록과 문서상 바로 붙어 있지 않으면 합치지 않고 안내.
- **Esc** = 마지막 확정 이후 입력만 버리고 편집 종료(문서는 그대로).
- **Ctrl+S** = EDIT 글 확정 → 저장. 제목 줄·머리글 ● = 저장 안 된 변경(확정된 문서 변경 또는 EDIT 에 입력만 한 글).
- **저장**: `handle_request(op='save')`(= `save_atomic`, 임시 파일 + 교체). **읽을 때 파일 바이트 SHA-256 을 기억하고 저장 직전에 파일을 다시 읽어 비교** — 다르거나 파일이 사라졌으면 저장하지 않고 경고(머리글 ⚠ + 대화상자). 엔진에 `expected_digest` 가 main 에 없어 크레이트 안에서 구현. 성공하면 기억한 SHA-256 을 새 내용으로 갱신.
- 닫기: 편집 중 글 확정 → 변경이 있으면 저장 여부 확인(저장 충돌이면 닫지 않음).
- 접힘은 엔진 요청마다 재파싱으로 사라지므로 `Tx` 가 접힘 표시를 블록 순서에 맞춰 이어 붙인다(시험 있음).

## 3. 한글 입력(IME) 안전 규칙 구현

설계 §3 상태 기계(N/E/C/F)를 구현했다. 핵심:

1. 조합 중 판정 = `ImmGetCompositionStringW(GCS_COMPSTR)` 길이 > 0 **또는** `WM_IME_STARTCOMPOSITION`~`ENDCOMPOSITION` 사이(전역 플래그). 후보창 열림(`IMN_OPENCANDIDATE`)이면 Enter·방향키는 IME 소유.
2. 조합 중 **Enter·Tab·↑↓** 는 명령을 바로 실행하지 않고 `ImmNotifyIME(NI_COMPOSITIONSTR, CPS_COMPLETE)` 로 먼저 확정 → 조합 종료 알림(`WM_IME_ENDCOMPOSITION`)이 오면 `PostMessage` 로 자기 창에 보내 **결과 글자 처리 후** 실행(200 ms 안에 알림이 없으면 타이머가 실행). 확정이 동기로 끝나면 즉시 실행 — 한 번에 끝난다.
3. 명령 실행 직전에 EDIT 글을 **항상** 읽어 `set_text` 로 반영한다(변경 없으면 생략).
4. 조합 중 Backspace·Esc·글자는 가로채지 않는다(조합이 끝난 빈 칸에서만 삭제).
5. `VK_PROCESSKEY` 로 처리한 명령 뒤 50 ms 안에 같은 진짜 키가 또 오면 중복으로 버린다(한 물리 키 = 한 명령). 별개 물리 키(연타)는 버리지 않고 큐에 순서대로 쌓는다.
6. 엔진 오류 시 문서도 EDIT 글도 바꾸지 않는다.

`ImePolicy`(구조체)로 키 종류별(`enter`·`tab`·`arrows`) 동작을 `CompleteThenRun`(기본) / `CompleteOnly`(확정만, 명령 버림) / `Ignore` 중에서 바꿀 수 있고, 대기 제한(200 ms)·중복 제거 시간(50 ms)도 같은 구조체에 있다. 탐침 U1·U2 결과가 나오면 값만 바꾸면 된다.

## 4. 캡처 검증 — 실제 PNG 를 열어서 본 것

`outliner_win --capture-dir <폴더> --capture-edit [--dark]` → `<CARGO_TARGET_DIR>/capture3/` (저장소 밖). 실제 창(화면 밖, 보이지 않음)과 **실제 EDIT 컨트롤**을 만들고 EDIT 에 `WM_KEYDOWN(Enter)` 를 보내 분할한 결과다. 문서는 메모리에서만 바뀌고 저장하지 않는다. 캐럿은 `WM_PRINT` 에 안 찍혀서 `EM_POSFROMCHAR` 위치에 선을 따로 그렸다(그림용).

- `outliner_edit_light_96dpi.png`: 보이는 줄 2번 "예산 편성 [[예산 지침]]" 줄 전체가 하얀 편집 상자로 덮이고(파란 테두리, 줄 선택 띠 안), 상자 안에 한글이 선명하게 `예산 편성 [[예산 지침]]` 로 보인다. 캐럿(세로선)이 "편성" 바로 뒤. 글자 시작 x 는 위·아래 줄 글자와 같은 열(≈61 px). 아래 줄들은 그대로.
- `outliner_split_light_96dpi.png`: Enter 후. 위에 일반 줄 `예산 편성`, 그 아래 새 편집 상자에 `[[예산 지침]]`, 캐럿이 맨 앞. 머리글 `블록 15개`(14→15)·제목 앞 ●. 원래 있던 하위 3줄은 두 번째 블록 아래에 그대로(엔진 `insert` 의 동작 — 아래 "알려진 한계").
- `outliner_edit_todo_light/dark_96dpi.png`: TODO 줄 "부서별 요구서 수합": 체크 칸은 그대로 보이고 EDIT 상자가 체크 칸 **오른쪽**에서 시작해 글자가 보인다.
- 다크 3장(`…_dark_96dpi.png`): 어두운 배경, 상자 안은 팔레트 배경색에 밝은 글자, 테두리는 파란 강조색. 읽을 수 있다.
- 처음 시도에서 두 번 틀렸다가 고친 것: ① 보이지 않는 창에서는 `WM_PRINT` 가 EDIT 를 그리지 않아 검은 상자가 나왔다 → 화면 밖(-32000)에 표시 상태로 둠. ② 좁은 EDIT 에 글을 넣어 가로로 끝쪽까지 스크롤돼 "예산 편성"이 가려졌다 → EDIT 크기를 맞춘 뒤 맨 앞으로 되돌리고 캐럿이 보이게 함(실제 창에서도 같은 코드).
- 화소 시험(`edit_window.rs`): 상자 안쪽 배경색, 마젠타(미그림 표시) 0개, 상자 안 비배경 화소 > 150, 아래 줄 글자 유지, 위 테두리 accent 색 — 밝게·어둡게 둘 다.

## 5. 시험 (실제 결과 줄)

```
cargo test  (outliner_win)
 unittests src\lib.rs            test result: ok. 6 passed   (probe_fmt 5 + sha256 1)
 tests\edit_machine.rs           test result: ok. 30 passed
 tests\edit_window.rs            test result: ok. 4 passed
 tests\layout_and_capture.rs     test result: ok. 10 passed  (기존 시험 유지: 기존 15개 = lib 5 + 10)
 tests\save_io.rs                test result: ok. 6 passed
cargo test  (outliner_core)      test result: ok. 58 passed  + test result: ok. 1 passed
```
outliner_win 합계 56개 통과, 실패 0. 새로 만든 시험 41개(상태 기계 30·저장 6·창 4·SHA 1).

`edit_machine.rs` 가 확인하는 것: Enter 분할(UTF-16·서로게이트·선택 영역)·끝에서 Enter·Tab/Shift+Tab·Backspace 삭제/합치기/중간 글자 삭제는 EDIT 에 맡김·Esc 복원·**IME 순서 주입**(조합 중 Enter 동기 확정 / 비동기 확정 대기 / 200 ms 제한 / 6가지 순서 A~F 가 모두 같은 결과 / 별개 물리 Enter 3번 = 블록 3개 / 대기 중 연타가 큐로 / 조합 중 Tab·Shift+Tab·↓ / 조합 중 Backspace·Esc 는 IME 소유 / 후보창 열림 / 정책 `CompleteOnly`·`Ignore`) · 엔진 오류 시 입력 보존 · Ctrl+S 가 입력 글을 먼저 확정 · 접힘 유지.
`save_io.rs`: 엔진 직접 호출 왕복(`handle_request`)·저장 왕복(임시 복사본)·원본 `sample.md` 바이트 불변·**SHA-256 충돌**(다른 곳이 바꾸면 저장 안 하고 남의 변경 보존, 내용이 같아지면 저장)·파일 삭제·저장 대상이 디렉터리인 오류·CRLF 유지·UTF-8 아님 거부·임시 파일 찌꺼기 없음.
스모크: 릴리스 exe 를 임시 복사본으로 실제 창으로 띄워 2초 생존 → 변경 없이 닫힘, 복사본 해시 불변(키 입력 자동화는 안 함).

## 6. 사용자 실기로 확인할 것

1. **실제 한글 조합 중 Enter / Tab / ↑↓** — `안녕` 입력 직후 Enter 에서 `- 안녕` 다음 빈 블록이 정확히 1개 생기는지, 글자 미손실, 끝 자모(`안녕ㅎ`)까지 포함되는지(설계 시나리오 1·2·3·4). 이 카드는 U1·U2(IME 가 실제로 보내는 순서)를 모르는 채 "어느 순서로 와도 같은 결과"가 되게 만들었다. 탐침(`outliner-ime-probe`) 로그가 나오면 `ImePolicy` 를 조정.
2. 조합 중 Backspace 로 자모를 모두 지운 뒤 빈 칸 Backspace(블록 삭제는 그때만), 조합 중 Esc(조합만 취소되고 편집 유지).
3. **구형/신형 Microsoft 한글 IME**("이전 버전 IME 사용" 켬/끔) 두 가지, 후보창(한자) 열린 상태의 Enter·방향키.
4. **고DPI**(125·150·200 %) — EDIT 위치·글꼴 크기·후보창 위치, 모니터 간 이동 시 `WM_DPICHANGED`(코드는 글꼴 재생성 포함, 실기 시험 못 함).
5. **고대비** 테마(EDIT 배경·글자·테두리 색), 다크↔밝게 전환 중 편집.
6. 실제 두 창에서 같은 `.md` 를 열고 한쪽이 저장한 뒤 다른 쪽 Ctrl+S → 경고 대화상자가 나오는지.
7. 긴 글(창 폭보다 긴 블록) 편집 시 가로 스크롤, 마우스로 다른 줄 클릭(조합 중 클릭 포함).

## 7. 미확인·알려진 한계

- U1·U2(IME 키 순서·`CPS_COMPLETE` 즉시 반영)는 여전히 미확인 — 위 1번. 가짜 호스트 시험은 "순서 다양성에 견딤"을 보일 뿐 실제 IME 동작의 증거가 아니다.
- 확정 대기 타임아웃(200 ms)이 지나도 조합이 안 끝나면 EDIT 에 보이는 글로 명령을 실행하므로, 그 경우 마지막 조합 글자가 빠질 수 있다(설계 §3.3 F 상태의 정해진 동작; 실제로 일어나는지 실기 확인 필요).
- 창을 잃는 포커스(Alt+Tab·다른 창 클릭)는 자동 확정하지 않는다 — EDIT 글은 보존되고 다음 명령·저장·닫기 때 확정된다(재진입 위험을 피한 선택). 프로세스가 강제 종료되면 마지막 확정 이후 입력은 잃는다.
- 저장은 "SHA-256 비교 → 교체" 사이에 짧은 틈(검사-사용 시차)이 있다. 엔진의 `expected_digest`(OUTLINERCORE1)가 들어오면 엔진 쪽 비교로 교체. 충돌 시 "내 변경을 다른 이름으로 보존" 은 아직 없음(경고 후 저장 안 함, 사용자가 다시 열어야 함).
- Enter 분할은 현재 블록의 하위 블록을 **새로 생긴 뒤쪽 블록 아래**로 남긴다(엔진 `insert` 가 바로 다음 줄에 넣기 때문 — Flutter Enter 와 같은 성질). 접힌 하위가 있는 블록에서 Enter 시 접힌 채 뒤쪽 블록 쪽에 붙는다.
- Alt+↑/↓(블록 이동)·Ctrl+Enter(접기)·링크 자동완성·TODO 토글·여러 줄 선택은 이번 범위 밖.
- 시스템 고대비 캡처는 못 했다(이전 카드와 같음).
- 시험 중 `Harness` 가 화면 밖 창을 잠깐 띄운다(`WS_EX_TOOLWINDOW|NOACTIVATE`, 작업 표시줄 단추 없음).
