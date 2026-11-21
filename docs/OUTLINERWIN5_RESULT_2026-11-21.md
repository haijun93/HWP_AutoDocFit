# OUTLINERWIN5 결과 — 아웃라이너 Win32 5단계: Flutter 대비 차이 점검 도구와 남은 기능

작성일: 2026-11-21 · 브랜치 `feature/OUTLINERWIN5` · 설계: [OUTLINER_WIN32_DESIGN_2026-11-20.md](OUTLINER_WIN32_DESIGN_2026-11-20.md) §5·§6 · 앞 카드: [OUTLINERWIN4_RESULT](OUTLINERWIN4_RESULT_2026-11-21.md) · 엔진: [OUTLINER_ENGINE_API](OUTLINER_ENGINE_API_2026-11-21.md)

`hwp-auto-docfit.py`·Python·Flutter·엔진(`outliner_core`) 변경 없음. 새 외부 크레이트 없음(`Cargo.lock` 불변, `--locked` 빌드). 저장소에 이 PC 전용 경로 없음(산출물은 `CARGO_TARGET_DIR` 환경변수로만).
표기: "코드로 확인" = 해당 줄을 읽었다. 읽지 못했거나 실행하지 못한 것은 6절에 모았다.

## 1. 기능 격차표 (Flutter `outliner_workspace.dart` ↔ `outliner_win`)

판정: **있음**(같은 결과) · **다름**(있으나 동작이 다름 — 의도된 것은 3절 번호) · **없음**. "이번"은 이번 카드에서 구현.

| # | 기능 | Flutter(근거) | outliner_win(근거) | 판정 |
|---|---|---|---|---|
| 1 | 페이지 목록에서 열기 | 왼쪽 목록 `ListTile.onTap`(:`_open`) | 왼쪽 패널 클릭 → `Shell::open_page`(`window.rs click`) | 있음 |
| 2 | 최근 페이지(20개, `outliner_recent.json`) | `_recentPages` | `PageStore` 같은 파일·형식(최대 20) | 있음 |
| 3 | 오늘의 일지 | `오늘의 일지` 목록 항목, 파일명 `YYYY-MM-DD` | `오늘` 단추, 파일명 `YYYY_MM_DD`(+옛 이름 정리) | 다름(사용자 확정) |
| 4 | 새 페이지 | 대화상자 | 위쪽 입력 줄(IME 안전), 중복·금지 문자 오류 | 다름(입력 방식) |
| 5 | 이름 바꾸기 | 파일 이름만 변경(링크 갱신 안 함) | 파일 이동 + 모든 페이지 `[[링크]]` 갱신 | 다름(사용자 확정) |
| 6 | 페이지 삭제 | 확인 후 `unlink`(복구 불가) | 확인 후 `.trash` 이동(탐색기 복구) | 다름(사용자 확정) |
| 7 | 페이지 이름 필터(검색 칸에 쓰면 목록이 줄어듦) | `_pageQuery` 로 `where(contains)` | 이전: 없음(결과 패널만). **이번**: 왼쪽 "전체 페이지 (n/N)" 가 줄어듦 | **이번 구현** |
| 8 | 블록 검색 | Enter, 현재 페이지 블록만 걸러냄, 되돌리기 없음 | 2글자부터 즉시, 모든 페이지, 결과 이동 | 다름(확장) |
| 9 | 역링크 패널 | 현재 페이지 본문만, "블록 N" 번호 | 데이터 폴더 전체, 페이지별 묶음, 클릭 이동 | 다름(확장·사용자 확정) |
| 10 | `[[링크]]` 이동 | 블록 아래 칩 `ActionChip` | 글 속 링크 강조·클릭(편집 중 Ctrl+클릭), 없으면 새로 만듦 | 다름 |
| 11 | `[[` 자동완성 | 블록 목록 아래 후보 6개 `ListTile` | 편집 상자 아래 팝업 8개(↑↓·Enter·Tab·Esc, 조합 중 Enter 는 확정만) | 다름(확장) |
| 12 | 접기·펼치기 | 블록 왼쪽 단추·Ctrl+Enter | 점·삼각형 클릭, ←/→ (Ctrl+Enter 는 TODO 토글) | 다름(키) |
| 13 | 들여쓰기·내어쓰기 단추 | 블록마다 단추 2개 | 이전: 키(Tab·Shift+Tab)만. **이번**: 위쪽 줄 단추 2개(선택·편집 중 블록) | **이번 구현** |
| 14 | TODO 토글 | 블록 오른쪽 ☑ 단추(어느 블록이든) | 체크 칸 클릭(이미 상태 있는 블록만), Ctrl+Enter(아무 블록) | 다름(마우스로 TODO 시작 불가) |
| 15 | `＋ 새 블록`(머리글, 마지막 블록 뒤) | `_edit(_blocks.last,'insert','')`, 블록 0개면 비활성 | 이전: 없음. **이번**: `＋ 새 블록` 단추 | **이번 구현** |
| 16 | Enter | 전체 글 저장 + 아래 빈 블록(커서 무관) | 커서 위치에서 분할 | 다름 D1 |
| 17 | Tab·Shift+Tab | 들여쓰기/내어쓰기, 입력 글은 안 보냄 | 같음 + 입력 글 먼저 확정 | 다름 D3 |
| 18 | Backspace(빈 칸) | 삭제 | 삭제(마지막 한 블록은 거절) | 다름 D6 |
| 19 | Backspace(맨 앞) | 앞 블록에 공백 없이 이어붙임 | 엔진 merge(두 글이 있으면 공백 1칸), 숨은 하위 건너편은 거절 | 다름 D2·D8 |
| 20 | Alt+↑/↓ | 엔진 `up/down` 한 줄 교환 | 하위 포함 같은 단계 형제 건너편으로 | 다름 D5 |
| 21 | 저장 | 편집마다 자동 `save`(SHA-256 충돌 확인) | Ctrl+S + 페이지 이동·이름 바꾸기·삭제·닫기 전 저장(같은 충돌 확인) | 다름(시점) |
| 22 | 엔진 호출 중 키 | `_busy` 면 키를 버림 | 호출이 동기(직접 링크)라 버릴 일이 없음, IME 조합은 큐 | 다름(Flutter 결함 제거) |
| 23 | 접힘 | 화면 계산뿐, 다음 편집에서 풀림 | 세션 동안 유지, `collapsed:: true` 로 시작 | 다름 D4·D7 |
| 24 | 오류 안내 | 머리글 아래 빨간 글자 | 위쪽 안내 줄·저장 경고 대화상자 | 있음 |
| 25 | 한 줄 규칙(탭·줄바꿈) | 그대로 보냄 | 공백으로 바꿈 | 다름 D9 |
| 26 | 클릭으로 편집 시작 | 글 칸 한 번 클릭 | 한 번 클릭 = 선택, 더블클릭·Enter·F2 = 편집 | 다름(남은 격차) |
| 27 | 블록 0개 문서 안내("+를 눌러…") | 있음 | 없음(시작·페이지 열기가 `- ` 한 블록을 보장) | 없음(불필요) |
| 28 | 일지 어제·내일 이동 | 없음 | 없음 | 둘 다 없음 |
| 29 | 드래그 이동·블록 참조 UI(`((id))`)·휴지통 복원 단추 | 없음 | 없음(복원은 API 만) | 둘 다 없음(G1 남은 일) |
| 30 | 다크·밝게 | 앱 테마 | `ui_theme`(밝게·다크·고대비) | 있음 |
| — | 네이티브에만 있는 것 | — | Ctrl+F·Ctrl+N·F2·PageUp/Down·Home/End·단일 인스턴스·휠·고DPI | (확장) |

## 2. 차이 0 확인 도구: `outliner_win/tests/parity.rs`

같은 시작 문서와 같은 입력 순서를 두 경로로 적용해 **결과 Markdown 과 화면에 보이는 줄을 단계마다 전수 대조**한다.

- **(a) Flutter 모델** — `outliner_workspace.dart` 가 키마다 보내는 엔진 요청 순서를 그대로 구현(`_commitAndInsert`=set_text→insert, `_edit`=indent/outdent/fold, `_setTextAndEdit`=set_text→up/down/toggle_status/delete, Backspace 병합=앞 블록 set_text(앞+뒤)→delete, `＋`=insert('')). 요청 모양은 `outliner_service.py` 규칙(`{'op':'edit','document':…,'args':{'action':…,'block_id':…,'text':…}}`)으로 `outliner_core::handle_request` 를 직접 호출한다. Flutter 가 요청마다 `_markdown` 을 다시 파싱하는 성질(블록 번호 재부여·접힘 소실)과 `_visibleBlocks` 도 그대로 흉내 낸다.
- **(b) 네이티브** — `Editor`(가짜 EDIT)에 실제 키 이벤트(`Enter`·`Tab`·`Shift+Tab`·`Backspace`·`Alt+↑/↓`·`Ctrl+Enter`)를 넣는다. 접기는 `View::set_fold`, `＋ 새 블록`은 `Editor::add_block_at_end`.
- **시나리오 54개**: 차이 0 이어야 하는 것 42개(빈 문서·`- ` 한 블록·긴 줄 560자·한글·이모지·결합 문자·CRLF·끝 줄바꿈 없음·제목(보존 줄) 낀 문서·들여쓰기 경계(첫 블록 Tab·최상위 Shift+Tab·깊이 4 트리)·TODO/DONE 순환·접기/펼치기·`＋ 새 블록` 등) + 허용 차이 12개.
- **허용 차이는 결과를 고정**한다: 차이가 사라지거나 다른 모양으로 바뀌면 시험이 실패하고, 등록 안 된 번호·쓰이지 않는 번호도 실패한다(`allowed_differences_are_pinned_and_registered`). 접힘 차이(D4·D7)는 Markdown 이 같고 보이는 줄이 달라 `folding_differences_show_in_visible_rows_only` 가 따로 고정한다.

### 결과: 차이 0 시나리오 42개 모두 일치 — 제품 코드 수정 없음

첫 실행에서 갈라진 것은 **시험 쪽 문제 3건**이었다(제품 차이가 아님): ① 허용 차이 D5 의 Flutter 쪽 기대값을 잘못 적음(엔진 `down` 은 `b` 와 그 하위 `b1` 을 교환), ② "첫 블록 맨 앞 Backspace" 에서 네이티브가 안내 문구를 남기는 것을 "실패"로 세어 비교(문서는 같음 → 비교 기준을 결과로 한정), ③ 접기 두 번 시나리오는 Flutter 의 D4(접힘 소실) 그 자체여서 시나리오를 정정. 제품 동작이 Flutter 와 다른 곳은 모두 아래 허용 차이로 설명된다.

## 3. 허용 차이 등록부 (사유·결과 고정)

| 번호 | 차이 | 사유 | 어느 쪽이 맞나 | 고정 시나리오 |
|---|---|---|---|---|
| D1 | Enter 가 **커서 위치에서 분할**(Flutter 는 항상 전체 저장 + 빈 블록) | 설계 4절 6, 기능 확장 | 네이티브(Flutter 는 커서를 무시) | 가운데 Enter, 맨 앞 Enter |
| D2 | Backspace 병합 **공백 1칸**(두 글이 모두 있을 때; Flutter 는 공백 없이 이음) | 엔진 `merge` 규칙, 설계 4절 7 | 네이티브(`안녕`+`하세요` 가 한 단어로 붙는 것은 의도 불명) | 두 글 병합 |
| D3 | Tab·Shift+Tab 시 **입력 중 글 먼저 확정**(Flutter 는 글을 안 보내 파일에서 빠짐) | 설계 3.4, 글 유실 방지 | 네이티브 | Tab, Shift+Tab |
| D4 | **접힘이 편집 뒤에도 유지**(Flutter 는 다음 요청에서 접힘이 풀림: `_markdown` 재파싱) | Flutter 결함 | 네이티브 | 접기 후 토글 |
| D5 | Alt+↑/↓ 가 **하위 블록 동반**, 같은 단계 형제가 없으면 거절(Flutter 는 한 줄 교환이라 자식이 다른 부모 밑으로 새어 나감) | OUTLINERWIN4 사용자 확정 | 네이티브 | 하위 있는 블록 Alt+↓, 첫 자식 Alt+↑ |
| D6 | 마지막 한 블록 Backspace **거절**(Flutter 는 블록 0개가 되어 새 블록을 못 만듦, 설계 1.4-6) | Flutter 결함 | 네이티브 | 한 블록 Backspace |
| D7 | `collapsed:: true` 속성이 있는 블록은 **접혀서 시작**(Flutter 는 무시) | Logseq 관례 | 의도(확장) | 속성 문서 |
| D8 | 접힌 하위 건너편 Backspace 병합 **거절**(Flutter 는 숨은 마지막 하위에 이어붙임) | Flutter 결함 | 네이티브 | 접힌 하위 건너 병합 |
| D9 | 입력 칸의 탭 문자 → **공백**(Flutter 는 그대로 보내 한 줄 규칙 위반 가능) | 한 줄 규칙 | 네이티브 | 탭 문자 Enter |

Flutter 쪽이 틀린 항목(D4·D5·D6·D8, 그리고 격차표 22 `_busy` 키 버림)은 **Flutter 를 고치지 않고** 이 문서에 기록한다(지시: 카드는 Python·Flutter 최소 변경). 네이티브가 기본이 되면 자연히 사라진다.
엔진 요청 수준 비교라서 **비교 밖**인 것(격차표 3~6·8~11·21·26): 페이지 관리·검색·링크·저장 시점·마우스는 엔진 호출 순서가 아니라 화면 동작이어서 이 하네스가 보지 않는다 — 각각 `pages_store`·`find_links`·`shell_flow`·`shell_window` 시험이 네이티브 쪽만 확인한다. Flutter 쪽 화면 동작과의 대조는 사용자 실기(5절)다.

## 4. 빠진 기능 구현 3개 (격차표 7·13·15) — 캡처 검증

선정 기준: 격차표의 "없음/마우스 경로 없음" 중 코드 변경이 작고 Flutter 와 같은 의미로 정의되는 것.

1. **`＋ 새 블록`** (격차 15): 문서 맨 끝에 마지막 블록과 같은 단계의 빈 블록을 만들고 편집 시작. 편집 중 글은 먼저 확정, 접힌 조상은 자동으로 펼침, 블록이 0개면 안내만. `Editor::add_block_at_end`. 엔진 요청은 Flutter 와 같은 `insert('')` 라서 parity 시나리오 3개가 차이 0 으로 확인한다.
2. **`들여쓰기`·`내어쓰기` 단추** (격차 13): 편집 중이면 편집을 유지한 채, 아니면 선택 블록에 적용(키 Tab·Shift+Tab 과 같은 `Cmd::Indent/Outdent` 경로).
3. **왼쪽 페이지 목록 이름 필터** (격차 7): 위쪽 검색 칸에 한 글자만 써도 "전체 페이지 (n/N)" 가 이름에 그 글자가 든 것(대소문자 무시)으로 줄어든다. 최근 페이지는 Flutter 처럼 그대로. 이름 입력 줄(새 페이지·이름 바꾸기)이 열려 있는 동안은 거르지 않는다. 줄어든 목록의 항목 클릭이 그 페이지를 연다.

위쪽 줄 단추는 4개 → 7개(`오늘`·`새 페이지`·`이름 바꾸기`·`삭제` │ `새 블록`·`들여쓰기`·`내어쓰기`)가 되었고, 검색 칸을 240→190px 로 줄여 기본 창 폭 820 에서 모두 보이게 했다(시험으로 확인: 96·192 DPI 모두 7개가 겹치지 않고 820 폭 안).

### 캡처(직접 열어 본 것)

생성: `outliner_win --capture-dir <폴더> --capture-gap [--dark]` (실제 창·실제 EDIT, 시연 데이터 폴더만 읽고 아무것도 저장 안 함). 경로(저장소 밖, 같은 명령으로 재생성): `C:\Users\haiju\AppData\Local\Temp\ow5cap\outliner_gap_{filter,indent,newblock}_{light,dark}_96dpi.png` (1000×600). 6장 모두 Read 로 열어 확인했다.

- **페이지 이름 필터(`gap_filter`, 밝게·다크)**: 검색 칸에 "예". 왼쪽 "최근 페이지 / 2026_11_21" 아래 "**전체 페이지 (1/6)**" 에 "예산 지침" 한 줄만 보임(원래 6개: 업무 매뉴얼·예산 지침·주간 업무·참고 자료·회의록·2026_11_21). 가운데는 그대로 일지(결과 패널은 2글자부터라 열리지 않음). 위쪽 줄에 7개 단추 전부 보이고 글자 읽힘 양호.
- **들여쓰기 단추(`gap_indent`)**: "회의 자료 인쇄"(앞 형제 "[[예산 지침]] 검토" 가 있는 줄)를 선택하고 단추 → 그 줄이 한 단계 안으로 들어가고 "검토" 줄 앞에 접기 삼각형(▼)이 생기며 안내 점이 ●(저장 안 한 변경)로 바뀜. 다크에서도 같은 결과, 가이드 선·체크 표시·선택 색 읽힘.
- **새 블록 단추(`gap_newblock`)**: 단추 결과 "블록 7개 · 보이는 줄 7", 마지막 "메모" 하위(예산 관련 문의는 [[회의록]] 참고) 바로 아래 같은 단계에 편집 상자(강조 테두리)가 생겨 "단추로 추가한 새 블록" 글이 보임(캡처 코드가 편집 칸에 글을 넣은 것). 밝게·다크 모두 같음.
- 처음 찍은 들여쓰기 캡처는 첫 자식 줄("검토")을 골라 눈에 보이는 변화가 없어(엔진은 들여쓰기 칸 수만 늘리고 화면 깊이는 앞 형제가 있어야 달라짐) 대상을 바꿔 다시 찍었다. 빈 화면·가짜 시안 아님(`●` 변경 표시와 블록 수가 실제 편집 결과).

## 5. 시험 결과 (실제 줄)

```
cargo test --release --locked   (outliner_win)
 unittests src\lib.rs            test result: ok. 6 passed
 unittests ime-probe / main.rs   test result: ok. 0 passed (둘)
 tests\edit_machine.rs           test result: ok. 30 passed
 tests\edit_window.rs            test result: ok. 4 passed
 tests\find_links.rs             test result: ok. 10 passed
 tests\layout_and_capture.rs     test result: ok. 10 passed
 tests\pages_store.rs            test result: ok. 9 passed
 tests\parity.rs                 test result: ok. 5 passed      (새; 시나리오 54개 = 일치 42 + 허용 차이 12)
 tests\save_io.rs                test result: ok. 6 passed
 tests\shell_flow.rs             test result: ok. 31 passed     (+3 새)
 tests\shell_window.rs           test result: ok. 13 passed     (+2 새)
 doc-tests                       test result: ok. 0 passed

cargo test --release --locked   (outliner_core)
 unittests src\lib.rs            test result: ok. 69 passed
 unittests src\main.rs           test result: ok. 0 passed
 tests\cli_smoke.rs              test result: ok. 1 passed
 doc-tests                       test result: ok. 0 passed
```
outliner_win 합계 **124개 통과, 실패 0**(이전 114 + 새 10: parity 5, shell_flow 3, shell_window 2). outliner_core 70개 통과(69+1). 전체 시험·Python·Flutter 시험은 지시대로 돌리지 않았다.

## 6. 사용자 실기로 확인할 것

1. **한글 조합 중 입력**(U1·U2 미확인 그대로): 조합 중 `＋ 새 블록`·`들여쓰기`·`내어쓰기` 단추를 누르면 조합이 끊기지 않는지(단추는 부모 창을 눌러 EDIT 포커스가 잠시 빠지지만 `ui_sync` 가 되돌린다 — 가짜 호스트로는 증명 못 함). 검색 칸에 한글 한 글자("예")만 써도 왼쪽 목록이 따라 줄어드는지(조합 중 글자 포함 여부).
2. 위쪽 줄 단추 7개가 사용자 모니터·DPI 125/150% 에서 잘림 없이 보이는지(820px 미만이면 오른쪽 단추가 잘린다 — 이전 4단추 구성도 616px 이상 필요했다).
3. Flutter 화면과 나란히 놓고 같은 페이지에서 키 순서를 쳐 보기(Enter 커서 분할·Backspace 병합 공백이 의도대로인지: D1·D2).
4. 격차표 26(클릭 한 번으로 편집)·14(마우스로 TODO 시작)가 사용에 거슬리는지 판단 — 거슬리면 다음 카드.

## 7. 미확인·한계

- **(a) 경로는 `outliner_service.py` 를 실제로 실행하지 않는다.** 요청 모양을 같은 규칙으로 만들어 엔진 라이브러리를 직접 부른다(Python 이 하는 일은 `payload`→`args` 변환·`page` 검사뿐이어서 같다고 본다 — `tests/test_outliner_service.py` 가 그 변환을 따로 확인). Flutter 위젯 이벤트 자체(포커스·IME·`_busy` 타이밍)는 모델 밖이다.
- 하네스는 **블록 번호 기준 편집**이라 마우스·검색·페이지 전환 흐름은 비교하지 않는다(3절 끝).
- 엔진에는 이제 커서 분할(`offset`)·`expected_digest` 가 있으나(OUTLINERCORE1 반영, API 문서 확인) 네이티브는 아직 `set_text`+`insert`·크레이트 안 SHA-256 비교를 쓴다. 결과는 같다(D1 외 차이 0 확인). 교체는 별도 카드.
- 새 단추 3개의 IME 실기 없음. 단추 폭은 고정이라 글꼴·DPI 가 크게 다르면 잘릴 수 있다.
- 격차표 26·14·28·29(클릭 편집·마우스 TODO 시작·일지 이동·드래그·블록 참조·휴지통 복원 단추)는 구현하지 않았다.
