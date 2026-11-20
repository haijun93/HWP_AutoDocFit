# OUTLINERWIN2 결과 — 아웃라이너 Rust+Win32 2단계 첫 카드

작성일: 2026-11-21 · 브랜치 `feature/OUTLINERWIN2` · 설계: [OUTLINER_WIN32_DESIGN_2026-11-20.md](OUTLINER_WIN32_DESIGN_2026-11-20.md) (권장안 A, 7.1절 W2-1·W2-2 일부)

## 1. 만든 것

새 크레이트 `outliner_win/` (독립 작업 공간, ktm·kagent 와 같은 방식).

| 실행 파일 | 역할 |
|---|---|
| `outliner-ime-probe` | **한글 IME 키 순서 탐침.** 입력칸 2개(A·B)와 로그 창이 있는 작은 창. 입력칸이 받는 `WM_KEYDOWN`(VK_PROCESSKEY 포함)·`WM_IME_*`·`WM_CHAR`·포커스·클릭 메시지를 타임스탬프와 함께 로그 파일에 기록한다. 키는 모두 EDIT 기본 처리로 넘기므로 실제 편집 때와 같은 순서가 남는다. |
| `outliner_win` | 읽기 전용 블록 목록 창(편집 없음) + `--capture-dir` 캡처 모드. |

의존성: `outliner_core`(경로), `ui_theme`(경로, 외부 크레이트 없음), `windows-sys 0.59`(기존 버전). **새 외부 크레이트 0개.** `ui_theme` 은 카드 지시 문구("windows-sys, outliner_core 만")보다 하나 더 들어갔다 — 다크·밝게·고대비 색 표를 새로 만들지 않고 저장소 공용 부품을 쓰라는 설계 문서 2.4절을 따른 것이다(되돌리려면 색 표 복제가 필요). 이견이 있으면 알려 주세요.

소스: `src/view.rs`(순수 모델·스크롤 계산), `src/paint.rs`(GDI 그리기), `src/capture.rs`+`src/png.rs`(캡처), `src/window.rs`(창), `src/probe_fmt.rs`(탐침 로그 서식), `src/bin/outliner-ime-probe.rs`(탐침).

## 2. 읽기 전용 블록 목록 창

- 실행: `outliner_win [문서.md]` (인자 없으면 저장소 안 `outliner_win/tests/sample.md`).
- ↑↓·PgUp·PgDn·Home·End 로 블록 선택, ←/→ 접기·펼치기(하위가 있는 블록만), 마우스 휠·스크롤 막대·클릭 선택. 편집 없음.
- 들여쓰기 = "조상 개수"(엔진 공백 폭 1칸·2칸·탭이 섞여도 단계가 맞는다). 접힌 블록은 ▸, 펼친 부모는 ▾, 말단은 점, TODO/DONE 은 체크 칸.
- 접힘은 파일에 저장되지 않으므로(엔진 `parse` 는 항상 펼침) Logseq 관례 `collapsed:: true` 가 있는 블록을 접힌 상태로 시작시키고 그 글자는 화면에서 뺀다.
- 테마: 시스템 설정 따라 밝게·어둡게·고대비(`ui_theme`), DPI: 모니터별 V2 + `WM_DPICHANGED`.

### 캡처 모드

```
outliner_win --capture-dir <폴더> [--dark | --hc] [--dpi N] [--file 문서.md]
```
창을 만들지 않고 같은 `draw` 를 메모리 DC에 그려 `outliner_<light|dark|highcontrast>_<dpi>dpi.png` 를 쓴다(720×520 ×dpi/96). `--dark` 없으면 밝게. `--hc` 는 현재 시스템 색으로 고대비 팔레트를 만든다(시스템 고대비가 꺼져 있으면 일반 시스템 색이라 실제 고대비 모습은 아니다 — 아래 미확인).

## 3. IME 탐침 사용 방법

### 실행

```
outliner-ime-probe.exe
```
(빌드: `cargo build --release` 후 `<CARGO_TARGET_DIR>\release\outliner-ime-probe.exe`. 이 문서의 경로는 이 PC 전용 값을 적지 않는다.)

창 위쪽에 로그 파일 경로가 표시된다: `%TEMP%\outliner-ime-probe\ime-probe-<날짜>-<시각>.log`. 아래 로그 창에는 최근 400줄이 실시간으로 보인다. **끝나면 이 로그 파일을 보내 주세요.**

- **F5** = 시나리오 시작 표시줄(`=== 시나리오 N 시작`)을 로그에 남긴다. 각 시나리오를 시작하기 전, 조합이 끝난 상태에서 한 번 누른다.
- **F6** = 지금 조합을 `ImmNotifyIME(CPS_COMPLETE)` 로 강제 확정하고, 호출 직후 입력칸 글과 메시지 펌프 한 바퀴 뒤 글을 남긴다(설계 U2: 확정이 글에 즉시 반영되는지).
- 한글 키보드(IME)를 한글 모드로 두고 시험한다. 가능하면 Microsoft 한글 IME 의 "이전 버전 IME 사용" 켬/끔 두 설정에서 각각 한 번씩 한다(U2: 구형/신형 차이).

### 로그 읽는 법

`[시각] +프로그램 시작 후 ms 출처(EDIT_A/EDIT_B/LOG/MAIN) 메시지 상세`. 주요 항목:
`WM_KEYDOWN vk=VK_PROCESSKEY real=VK_RETURN`(IME가 가로챈 키와 실제 키), `compLen`(키 도착 시 조합 중 글자 수), `text`(그 시점 입력칸 글), `WM_IME_COMPOSITION flags=GCS_COMPSTR|GCS_RESULTSTR comp=… result=…`, `└ … 처리 후 text=…`(기본 처리 직후 글), `WM_CHAR`.

### 시나리오 10개 (설계 문서 3.6절)

각 시나리오 앞에서 F5 를 누르고 입력칸 A 에서 시작한다. 별도 표시가 없으면 `안녕` 은 한글로 직접 타자한다.

| # | 하는 일 | 로그에서 볼 것(확정하려는 것) |
|---|---|---|
| 1 | `안녕` 입력 직후(밑줄 조합 중) **Enter** | Enter 가 `VK_PROCESSKEY`(real=VK_RETURN)로만 오는가, 조합 확정 뒤 진짜 `VK_RETURN` 이 한 번 더 오는가, `WM_CHAR` 결과 글자가 키보다 먼저인가 (U1) |
| 2 | `안녕ㅎ`(마지막이 미완성 자모) 상태에서 **Enter** | `ㅎ` 이 `result=` 에 포함되어 확정되는가 |
| 3 | 조합 중 **Tab**, 그리고 다시 조합 중 **Shift+Tab** | Tab 의 vk/real 과 확정 순서, Tab 이 입력칸에 `\t` `WM_CHAR` 로 들어가는가 |
| 4 | 조합 중 **↓** 한 번 후 **↑** 한 번 | 방향키가 조합 확정에만 쓰이는지, 두 번째 키부터 일반 키로 오는지 (U1) |
| 5 | 조합 중 **Alt+↑** | `WM_SYSKEYDOWN` 으로 오는지, 조합이 확정·취소되는지 |
| 6 | 조합 중 **입력칸 B 를 마우스로 클릭**, 그리고 다시 조합 중 **로그 창 클릭** | `WM_KILLFOCUS` 전후 `WM_IME_ENDCOMPOSITION`·`result=` 순서, A 의 글에 확정 글자가 남는가 |
| 7 | `가` 입력 후 **Backspace** 를 자모가 다 지워질 때까지 반복, 빈 칸에서 한 번 더 | 조합 중 Backspace 가 `VK_PROCESSKEY` 인지, 조합이 끝난 뒤의 Backspace 만 일반 `VK_BACK` 으로 오는지 |
| 8 | 조합 중 **Alt+Tab**(다른 창으로 갔다 돌아오기), 조합 중 **창 닫기(Alt+F4)** | `WM_ACTIVATE`·`WM_KILLFOCUS`·`WM_CLOSE` 와 조합 종료 순서, 닫힐 때 `compLen` |
| 9 | `가나다` 를 빠르게 치면서 **Enter 3 회 연타** | 연타한 Enter 가 몇 번의 `WM_KEYDOWN`/`WM_CHAR` 로 나뉘어 오는가(`prev=`·`repeat=` 로 중복 구분 가능한지), 빠진 키는 없는가 |
| 10 | `한자` 입력 후 **한자 키**(후보창) → 후보창에서 **Enter**·**↑↓** → 선택 확정. 그리고 **F6**(조합 중 강제 확정) 한 번 | 후보창 열림/닫힘 `IMN_OPENCANDIDATE`·`IMN_CLOSECANDIDATE`, 후보창이 열린 동안의 Enter·방향키 도착 형태, F6 의 `ImmNotifyIME ret`·`right_after_text` (U2) |

추가(선택): 영문↔한글 전환 키(한/영), 다크 모드·150~200% DPI 에서 후보창 위치(U11)는 육안으로 확인해 알려 주세요.

## 4. 캡처 결과 (실제 PNG 를 열어 확인)

`<CARGO_TARGET_DIR>/capture/` 에 생성(저장소 밖). 샘플 문서 14개 블록 중 보이는 줄 12(접힌 블록의 하위 2개 숨김).

- `outliner_light_96dpi.png`: 머리글 "샘플 문서 / 블록 14개 · 보이는 줄 12", 12줄의 한글 블록 글자가 모두 읽힌다. `1분기`·`2분기`(깊이 0) 아래 자식이 한 단계씩 들여써져 있고 `예산 편성` 줄이 파란 선택 띠, `TODO` 두 줄은 빈 체크 칸, `DONE` 줄은 체크된 칸에 회색 글, 긴 문장 줄은 말줄임표(…)로 잘림, `접힌 블록 (하위 2개 숨김)` 은 ▸ 이고 하위 2줄은 보이지 않는다, 마지막 줄은 빈 블록(점만). 밝은 배경.
- `outliner_dark_96dpi.png`: 같은 내용, 어두운 배경·밝은 글자·진한 파란 선택 띠.
- `outliner_highcontrast_96dpi.png`: 시스템 색 팔레트(굵은 선택 띠, 세로 안내선이 선명). 시스템 고대비가 꺼진 상태에서 만든 것이다.
- `outliner_light_192dpi.png`: 1440×1040, 치수가 2배로 커진 같은 화면.

## 5. 시험 결과

`cargo test`(해당 크레이트만): 단위 5개 + 통합 10개, **15개 통과, 0개 실패**. 내용: 블록 N개→줄 N개(0·1·5·100·1000), 스크롤 한계, 선택 이동·화면 유지, 접힘 숨김·펼침 복원, 혼합 들여쓰기 깊이, 파싱 블록 14 대 화면 줄 12(숨김 2개 이름 확인), 빈 문서, **메모리 DC 화소 검사**(밝게·어둡게에서 비배경 화소 5,000 이상, 줄별 글자 화소 40 이상, 깊이가 깊을수록 오른쪽 시작, 빈 아래 영역은 배경색뿐, 밝게≠어둡게), 192 DPI, PNG 구조, `--capture-dir` 실행 파일이 실제 PNG 를 쓰는지.

## 6. 미확인 항목

- **U1·U2(IME 순서·`ImmNotifyIME` 동기 여부)**: 도구만 만들었다. 실제 한글 입력 로그는 사용자 실기가 필요하다. 이 카드에서 얻은 것은 탐침이 실제 IME 메시지(`WM_IME_SETCONTEXT`·`IMN_*`)를 기록한다는 것뿐이다.
- 탐침의 `IMN_0xA`·`IMN_0xB` 이름은 상수표에 없어 숫자로 남긴다(로그 해석에는 지장 없음).
- 시스템 고대비를 실제로 켠 상태의 그림·선택 줄 글자색은 확인하지 못했다(캡처 PNG 는 고대비 꺼짐 상태의 시스템 색).
- 실제 창의 마우스·키보드 조작은 자동 시험하지 않았다(창이 뜨고 IME 메시지가 기록되는 것까지만 확인). 150%·200% 모니터 이동(`WM_DPICHANGED`)은 미확인.
- 스크린 리더 접근성(U5), 수천 블록 성능(U4)은 이 카드 범위 밖.
