# OUTLINEUI1 결과 — Rust 아웃라이너 3단 구성·상단 막대

## 범위와 기준

- 브랜치: `feature/OUTLINEUI1`. 시작점 `a502f109`에서 `git merge feature/OUTLINEKEY1` 실행, `8019ea39`까지 fast-forward. 병합 직후 Rust 전체 시험으로 선행 변경을 재확인했다.
- 제품 변경은 `outliner_win`의 `layout.rs`, `shell.rs`, `paint_shell.rs`, `window.rs` 4개. `outliner_core`는 병합된 편집 규칙을 사용하며 이번 카드에서 추가 수정하지 않았다.
- 합격 기준: 220px·가변·280px 배치와 DPI 대응, 독립 사이드바 토글, 페이지 방문 뒤로/앞으로, 왼쪽 4항목, Shift+클릭 카드 스택, 기존 편집·검색·역링크·저장 유지. 변경 Rust 크레이트 전체와 관련 Python 시험 통과.
- 진행 순서: 선행 병합·기준선 → Logseq 공개 동작 대조 → 상태·배치 → Win32 입력·그리기 → 합성 시험·실제 컨트롤 캡처 → 기록·로컬 커밋.
- 설치본·서비스·자동 실행·사용자 설정을 변경하지 않았다. Flutter 화면과 설치 전환 기본값을 수정하지 않았다. 새 WebView2·COM 호출 없음. 사용자 문서·외부 XML 입력 없음.
- 빌드는 카드 지정 `CARGO_TARGET_DIR`와 로컬 MinGW를 프로세스 환경변수로 지정했다. 저장소에 PC 전용 경로를 추가하지 않았다. 임시 합성 자료·시험 로그는 작업 폴더의 Git 제외 `scratch/` 아래에 두었다.

## Logseq 확인 근거와 초안 교정

기준은 파일 그래프 판 **Logseq 0.10.9** 공개 소스다. 동작만 참고해 기존 Rust·GDI·`ui_theme`로 구현했고, Logseq 코드·CSS·아이콘·문구 자산을 가져오지 않았다. 실제 Logseq 앱을 실행해 대조하지는 않았다.

| 항목 | 공개 근거 | 이번 반영 |
|---|---|---|
| 좌/우 토글 | [shortcut/config.cljs L418–422](https://github.com/logseq/logseq/blob/0.10.9/src/main/frontend/modules/shortcut/config.cljs#L418-L422) | 편집 밖에서 `t` 다음 `l` / `r`. 초안의 Ctrl+B·Ctrl+G를 토글로 연결하지 않음. Ctrl+B는 해당 소스에서 굵게 명령 |
| 뒤로/앞으로 | [shortcut/config.cljs L340–344](https://github.com/logseq/logseq/blob/0.10.9/src/main/frontend/modules/shortcut/config.cljs#L340-L344), [header.cljs L142–154](https://github.com/logseq/logseq/blob/0.10.9/src/main/frontend/components/header.cljs#L142-L154) | `Ctrl+[` / `Ctrl+]`와 상단 화살표. 초안 Alt+←/→ 대신 확인된 기본 키 적용 |
| 검색·즐겨찾기 | [shortcut/config.cljs L320–325](https://github.com/logseq/logseq/blob/0.10.9/src/main/frontend/modules/shortcut/config.cljs#L320-L325), [L432–433](https://github.com/logseq/logseq/blob/0.10.9/src/main/frontend/modules/shortcut/config.cljs#L432-L433) | `Ctrl+K` 검색 진입, `Ctrl+Shift+F` 즐겨찾기 토글 |
| Shift+클릭 | [block.cljs L475–485](https://github.com/logseq/logseq/blob/0.10.9/src/main/frontend/components/block.cljs#L475-L485), [L1569–1581](https://github.com/logseq/logseq/blob/0.10.9/src/main/frontend/components/block.cljs#L1569-L1581) | 페이지 링크·불릿을 우측 카드로 열기. 왼쪽 목록·검색 결과·역링크에도 같은 진입 제공 |
| 오른쪽 페이지/블록 카드 | [right_sidebar.cljs L33–45](https://github.com/logseq/logseq/blob/0.10.9/src/main/frontend/components/right_sidebar.cljs#L33-L45) | UI1 범위의 카드 컨테이너·읽기 미리보기·닫기·중앙으로 열기. 동시 편집은 후속 범위 |
| 상단 토글·검색 | [header.cljs L52–59](https://github.com/logseq/logseq/blob/0.10.9/src/main/frontend/components/header.cljs#L52-L59), [L216–224](https://github.com/logseq/logseq/blob/0.10.9/src/main/frontend/components/header.cljs#L216-L224) | 기존 검색 EDIT와 기존 작업 단추 7개를 함께 유지 |

참조한 초안은 `feature/OUTLINEUI0:docs/OUTLINEUI0_DESIGN_2026-11-26.md`다. 초안의 0ms·기동/메모리 수치는 실측 근거로 사용하지 않았다. OUTLINEKEY1의 **빈 블록 Enter는 항상 내어쓰기**라는 최신 사용자 예외는 그대로 유지했다. 줌·드래그·슬래시 명령·중앙 검색 모달·UUID/폴더 이관은 후속 카드 범위다.

## 구현한 상태와 동작

| 상태 | 전이 | 실패·불가능한 전이 |
|---|---|---|
| `left_open` / `right_open` | 단추·편집 밖 `t l` / `t r`로 각각 토글 | 좁은 창의 자동 숨김은 토글 의도를 덮어쓰지 않음 |
| `section` | 일지·모든 페이지·즐겨찾기·최근 선택, 목록 스크롤 처음으로 | 일지 진입 시 오늘 열기 실패는 기존 문서 유지·메시지 표시 |
| `visits` / `visit_at` | 페이지 방문 기록, 뒤로/앞으로 시 선택 블록 복원 | 저장 충돌·대상 없음이면 현재 페이지/방문 위치 유지, 없는 방문 파일 재생성 금지 |
| `favorites` | 현재 페이지 별 단추·Ctrl+Shift+F, 이름 변경/되돌리기 동기화 | 실행 중 메모리만 사용. 설정·Markdown에 추가 저장하지 않음 |
| `cards` | Shift+클릭으로 맨 위 추가, 같은 대상 중복 제거, × 닫기, 클릭해 가운데로 열기 | 없는 페이지 생성 금지. 블록 카드의 원문이 바뀌면 순번 오지정을 막고 다시 열기 안내 |
| `toggle_prefix` | 편집 밖 `t`에서 대기, 1.5초 안 `l/r`로 토글, 다른 키/시간 경과로 해제 | EDIT·검색 입력·IME 조합을 토글 연속키로 해석하지 않음 |

- 기존 `BUTTONS` 7개는 유지하고 탐색 단추 5개를 추가했다. 폭이 좁으면 작업 단추 줄을 감싸 배치하며 가운데 폭은 음수가 되지 않는다.
- 왼쪽 기본 목록의 기존 최근·전체 순서를 유지한다. 위 4개 탐색 항목으로 일지·즐겨찾기·최근을 따로 볼 수 있다.
- 카드는 열었을 때의 미리보기다. 다른 페이지를 읽기만 하며, 원문 편집·저장은 기존 중앙 편집기가 맡는다. 블록 카드에는 문서 다이제스트를 보관해 원문 변경 뒤 엉뚱한 순번 블록으로 이동하지 않게 했다.
- 역링크는 카드 아래에 남겨 두며 기존 클릭 점프를 유지한다. 검색 결과가 열렸을 때 EDIT가 결과 위를 덮지 않게 숨기고, 이름 입력 중 숨겨진 단추는 클릭되지 않게 했다.
- 페이지 이동 때 검색 입력과 모델을 함께 비운다. 기존 저장 충돌 검사·실행 취소·블록 편집 규칙·한 파일 모드는 유지한다.

## 검증 기록

선행 병합 기준선: `outliner_core` 122개, `outliner_win` 137개 통과, 실패 0. 기존 시험은 변경하지 않았다.

중간 실행에서 기존 `toolbar_has_seven_buttons_inside_the_window_and_hit_testing_matches`가 `96dpi 단추 0 가 창 안에 있다` 조건으로 1개 실패했다. 기존 작업 단추의 위치 조건을 유지하도록 검색/탐색/작업 줄 배치를 수정했고, 기존 셸 시험 14개가 다시 통과했다. 기준을 낮추거나 시험을 수정하지 않았다.

최종 시험 출력은 아래에 기록한다.

최종 합계: `outliner_core` **122개**, `outliner_win` **146개**(기존 137 + 새 9) 통과·실패 0, 관련 Python **33개** 통과. 저장소 작업 폴더에는 `.venv`가 없어 이미 설치된 시스템 Python 3.14로 관련 시험을 실행했다. 환경 설치·설정 변경 없음. Cargo stderr가 PowerShell에서 `NativeCommandError` 형식으로 표시됐지만 두 Cargo 명령의 실제 종료 코드는 모두 0이다.

`cargo test --manifest-path outliner_core/Cargo.toml --locked -j 2` (exit 0)

```text
test result: ok. 84 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.53s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cargo test --manifest-path outliner_win/Cargo.toml --locked -j 2` (exit 0)

```text
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.23s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.96s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.04s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.65s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.46s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.35s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`python -m unittest tests.test_outliner_service tests.test_bridge_methods tests.test_package_integrity` (exit 0)

```text
Ran 33 tests in 7.534s

OK
```

## 화면 확인과 남은 범위

- 실제 화면 밖 Win32 창·실제 EDIT를 그린 합성 캡처다. HTML 시안이나 그림 생성 모델을 사용하지 않았다. 밝게·어둡게·고대비, 96/192 DPI 6장을 생성했다.
- 캡처 파일은 PNG 압축만 무손실로 다시 했으며 화소 데이터가 같은지 확인했다. 아래 파일을 이미지 도구로 직접 열어 3단 영역·툴바·글자·카드/역링크를 확인했다.

| 테마 | 96 DPI | 192 DPI |
|---|---|---|
| 밝게 | [실창 캡처](assets/OUTLINEUI1_2026-11-26/outlineui1_light_96.png) | [실창 캡처](assets/OUTLINEUI1_2026-11-26/outlineui1_light_192.png) |
| 어둡게 | [실창 캡처](assets/OUTLINEUI1_2026-11-26/outlineui1_dark_96.png) | [실창 캡처](assets/OUTLINEUI1_2026-11-26/outlineui1_dark_192.png) |
| 고대비 | [실창 캡처](assets/OUTLINEUI1_2026-11-26/outlineui1_hc_96.png) | [실창 캡처](assets/OUTLINEUI1_2026-11-26/outlineui1_hc_192.png) |

- 새 `outline_ui1.rs` 시험은 배치/히트테스트, 4종 목록, 방문 위치 복원·앞으로 분기, 저장 충돌/삭제된 방문, 카드 중복/닫기/활성화, 바뀐 블록 카드 거절, 이름 변경/되돌리기, 실창 입력과 캡처를 확인한다. 배치는 96/144/192 DPI와 7종 폭을 대조한다.
- 즐겨찾기·카드·방문 이력은 실행 중에만 유지된다. 카드 내 직접 편집·접기·드래그 재정렬, 여러 일지 연속 화면은 구현 범위에 포함하지 않았다.
- 실제 키보드·한글 IME·다중 모니터에서의 사용자 조작, 설치본 연결·기본 Rust 전환은 미확인이다. 설치본과 번들 실행 파일은 갱신하지 않았다.
- 전체 Python·Flutter 검증은 요청대로 PM 릴리스 단계에 남겼다. 속도·메모리 측정은 하지 않았으며 성능 개선 수치를 주장하지 않는다.
- 결과는 기능 브랜치 로컬 커밋까지다. 푸시·main 병합·태그·배포는 수행하지 않았다.
