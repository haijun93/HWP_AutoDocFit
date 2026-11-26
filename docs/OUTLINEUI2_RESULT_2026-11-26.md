# OUTLINEUI2 결과 — Rust 창 불릿·접기·줌·경로·드래그

## 결과와 범위

- `feature/OUTLINEUI2`에서 구현·자동 시험 완료. 시작 커밋 `a502f109`에서 지시대로 `git merge feature/OUTLINEUI1`을 실행하여 `bfe01e2d`까지 fast-forward했다. OUTLINEUI1 커밋·통과 기록을 확인했고, 병합 직후 core **122개**, win **146개** 전체 시험도 통과했다.
- 최종 변경 크레이트 전체: **outliner_core 130개, outliner_win 159개 통과**, 실패·제외 0. 관련 Python **33개 통과**. 새 시험은 core 8개·win 13개다. 기존 시험은 수정하지 않았다.
- 제품 소스는 두 크레이트의 9개 파일(기존 8개·새 `bullet.rs` 1개), 새 시험 파일은 2개다. 설계는 `OUTLINEUI2_DESIGN_2026-11-26.md`, 상태는 `TODO.md`와 `reports/PM-HANDOVER.md`에 기록했다.
- Win32·GDI·기존 Rust 엔진만 사용했다. 새 외부 의존성·WebView2·COM·XML 파서는 없다. Markdown 저장 형식은 그대로다. 합성 Markdown만 사용했다.
- 설치본·서비스·자동 실행 등록·사용자 설정·Flutter·기본 실행 전환은 변경하지 않았다. 기능 브랜치 로컬 커밋까지만 수행한다. 푸시·태그·main 병합·배포 없음. GitLab 접근 없음.

## Logseq 동작 확인과 구현 선택

기준은 파일 그래프 판 Logseq **0.10.9**의 공개 소스다. 실제 Logseq 앱을 실행한 비교는 하지 않았다. 동작만 확인했으며 AGPL 코드·CSS·아이콘 등 자산은 복사하지 않았다. 기존 Rust 모델과 GDI 도형으로 구현했다.

| 동작 | 근거 | 반영 |
|---|---|---|
| 불릿 클릭·Shift+클릭 | [block.cljs L1560–1589](https://github.com/logseq/logseq/blob/0.10.9/src/main/frontend/components/block.cljs#L1560-L1589) | 클릭은 블록 줌, Shift+클릭은 기존 우측 카드 유지 |
| 접기와 불릿 분리 | [block.cljs L1625–1668](https://github.com/logseq/logseq/blob/0.10.9/src/main/frontend/components/block.cljs#L1625-L1668) | 왼쪽 삼각형은 접기, 오른쪽 점은 줌·드래그. 접힌 불릿에는 원형 테두리 |
| 상위 경로 | [page.cljs L463–475](https://github.com/logseq/logseq/blob/0.10.9/src/main/frontend/components/page.cljs#L463-L475) | 페이지·조상 경로, 상위 이동 ↑, 직전 줌 뒤로 ←. 좁은 창은 경로 앞부분 축약 |
| 드롭 위치·트랜잭션 | [block.cljs L2451–2496](https://github.com/logseq/logseq/blob/0.10.9/src/main/frontend/components/block.cljs#L2451-L2496), [dnd.cljs L9–51](https://github.com/logseq/logseq/blob/0.10.9/src/main/frontend/handler/dnd.cljs#L9-L51) | 앞·하위 전체 뒤·마지막 자식 위치를 표시하고 놓을 때 한 번 이동 |
| 불릿 오른쪽 메뉴 | [Logseq 공식 블록 참조 문서](https://github.com/logseq/docs/blob/master/pages/The%20basics%20of%20block%20references.md) | 불릿 메뉴 진입을 확인. 이번 메뉴는 지원하는 줌·우측 열기·접기/펼치기·하위 포함 복사/삭제·실행 취소 6종 |

드롭 판정의 수치는 이 창의 DPI·행 높이에 맞췄다. 대상 행 위쪽 1/4은 앞, 아래쪽 1/4은 하위 전체 뒤, 가운데에서 본문 쪽은 마지막 자식이다. 기존 Ctrl+[ 뒤로 키와 상단 뒤로 단추는 줌 이력이 있으면 그 이력을 먼저 되돌린다. 줌 앞으로·다른 페이지로 드래그·여러 선택 루트 동시 드래그·Alt+드래그 참조 생성·UUID 생성 메뉴는 이번 범위 밖이다.

## 상태와 보존 규칙

| 상태 | 전이와 실패 처리 |
|---|---|
| 전체 / 블록 줌 | 문서 전체를 유지하고 해당 하위만 표시, 깊이는 화면 안에서 다시 계산. 줌·경로·접기는 저장 파일을 바꾸지 않음 |
| 줌 방문 이력 | 이전 루트·선택·스크롤을 메모리에 보관. ←·Ctrl+[·상단 뒤로로 복귀. 외부 블록 검색/점프는 필요하면 줌 해제 |
| Idle / Armed / Dragging | 불릿 누름에서 Armed, Windows 드래그 문턱 초과 시 Dragging, 놓을 때만 이동. Esc·WM_CANCELMODE·캡처 상실·창 밖 놓기는 문서와 이력 유지 |
| 드롭 대상 | 앞·뒤·자식의 선과 안내 문구. 자기 자신·하위에는 드롭 불가. 본문 위/아래 가장자리에서는 타이머로 자동 스크롤 |
| 편집·IME | 실제 포인터 드래그 진입 전 기존 입력을 확정한다. 비동기 조합 확정이 끝나지 않으면 진입하지 않고 안내한다. Editor의 직접 드롭 호출은 입력 확정과 이동을 한 단계로 기록 |
| 이동·실행 취소 | 하위 깊이 차이·TODO/DONE·숨긴 속성·블록 ID·접힘을 동반. 대상 자식 들여쓰기 폭을 따르고 새 자식이면 기존 엔진의 1칸 규칙 사용. 한 번 이동은 Ctrl+Z 한 단계 |
| 실패·재파싱 | 자기 하위·없는 대상·보존 Markdown 문법 경계는 원문 불변으로 거부. 실행 취소/다시 실행은 Markdown 재파싱으로 ID가 재배정되므로 줌·줌 이력을 비워 다른 블록을 잘못 표시하지 않음 |

- 이동 후 같은 ID로 재편집할 수 있도록 Editor 내부 확정은 기존 core 함수를 직접 호출한다. JSON 왕복으로 ID를 다시 매기던 경로를 화면 내부에서 제거했다. Python/CLI 공개 요청 형식은 변경하지 않았다.
- 드래그 중 문서를 바꾸지 않으며 놓기 성공만 이동 이력에 추가한다. 같은 자리 드롭은 문서를 편집 상태로 만들지 않는다. 사본의 저장 충돌·기존 키 규칙·페이지 탐색·우측 카드 회귀 시험도 통과했다.
- 원래 각 파일의 UTF-8·줄바꿈을 유지했다(기존 수정 Rust 파일은 LF). 본체 `hwp-auto-docfit.py`는 수정하지 않았으며 작업 사본의 CRLF 3,523줄을 확인했다. 새 텍스트 파일은 CRLF다. 빌드 경로는 프로세스의 `CARGO_TARGET_DIR`·로컬 MinGW PATH로만 지정했고 PC 전용 경로를 저장소에 추가하지 않았다.

## 시험과 실패 기록

중간 전체 검증에서 기존 `edit_rect_overlaps_the_block_row_exactly` **1개 실패**: 불릿 폭을 본문 위치에 더하여 EDIT 왼쪽이 기대 56px 대신 70px가 됐다. 본문·EDIT·TODO 좌표를 원래대로 유지하고 기존 왼쪽 여백 안에서 삼각형과 불릿을 분리했다. 기존 시험 변경 없이 해당 5개와 새 화면 시험 11개가 통과했으며, 추가 경로·비동기 IME 시험을 포함한 최종 전체 검증도 통과했다.

관련 Python은 작업 폴더에 `.venv`가 없어 설치된 Python 3.14로 실행했다. 새 환경 설치나 사용자 설정 변경은 없다. 최종 Rust·Python 시험 임시 자료와 로그는 Git 제외 `outliner_win/target/` 안에 두었다. PowerShell이 Cargo/Python의 stderr 출력을 `NativeCommandError`로 감싸는 줄이 있으나 최종 프로세스 종료 코드는 모두 **0**이다.

### 실제 최종 출력

`cargo test --manifest-path outliner_core/Cargo.toml --locked -j 2` (exit 0)

```text
test result: ok. 84 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.46s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.57s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cargo test --manifest-path outliner_win/Cargo.toml --locked -j 2` (exit 0)

```text
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.14s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.79s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.06s
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.86s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.18s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.39s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.26s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`python -m unittest tests.test_outliner_service tests.test_bridge_methods tests.test_package_integrity` (exit 0)

```text
Ran 33 tests in 3.766s

OK
```

## 실창 캡처와 미확인 범위

합성 문서로 실제 화면 밖 Win32 창을 만들고 같은 GDI 그리기·창 메시지 경로를 사용했다. 3상태(줌·드래그·접힘) × 3테마 × 2배율, **18장**이다. PNG는 IDAT 압축만 변경하여 해제한 화소 데이터가 같음을 확인했다. 총 54,775,404바이트를 141,353바이트로 줄였다. 직접 연 파일은 zoom_light_96·zoom_dark_192·drag_light_96·drag_dark_192·fold_hc_96·fold_hc_192의 **6장**이며 나머지 12장은 생성·자동 검증만 했다.

| 상태·테마 | 96 DPI | 192 DPI |
|---|---|---|
| 줌·밝게 | [실창 캡처](assets/OUTLINEUI2_2026-11-26/zoom_light_96.png) | [실창 캡처](assets/OUTLINEUI2_2026-11-26/zoom_light_192.png) |
| 줌·어둡게 | [실창 캡처](assets/OUTLINEUI2_2026-11-26/zoom_dark_96.png) | [실창 캡처](assets/OUTLINEUI2_2026-11-26/zoom_dark_192.png) |
| 줌·고대비 | [실창 캡처](assets/OUTLINEUI2_2026-11-26/zoom_hc_96.png) | [실창 캡처](assets/OUTLINEUI2_2026-11-26/zoom_hc_192.png) |
| 드래그·밝게 | [실창 캡처](assets/OUTLINEUI2_2026-11-26/drag_light_96.png) | [실창 캡처](assets/OUTLINEUI2_2026-11-26/drag_light_192.png) |
| 드래그·어둡게 | [실창 캡처](assets/OUTLINEUI2_2026-11-26/drag_dark_96.png) | [실창 캡처](assets/OUTLINEUI2_2026-11-26/drag_dark_192.png) |
| 드래그·고대비 | [실창 캡처](assets/OUTLINEUI2_2026-11-26/drag_hc_96.png) | [실창 캡처](assets/OUTLINEUI2_2026-11-26/drag_hc_192.png) |
| 접힘·밝게 | [실창 캡처](assets/OUTLINEUI2_2026-11-26/fold_light_96.png) | [실창 캡처](assets/OUTLINEUI2_2026-11-26/fold_light_192.png) |
| 접힘·어둡게 | [실창 캡처](assets/OUTLINEUI2_2026-11-26/fold_dark_96.png) | [실창 캡처](assets/OUTLINEUI2_2026-11-26/fold_dark_192.png) |
| 접힘·고대비 | [실창 캡처](assets/OUTLINEUI2_2026-11-26/fold_hc_96.png) | [실창 캡처](assets/OUTLINEUI2_2026-11-26/fold_hc_192.png) |

- 실제 키보드·포인터·한글 IME 후보창·모니터 간 DPI 이동·오른쪽 팝업 메뉴의 수동 조작은 미확인이다. 오른쪽 메뉴의 동작 명령은 Win32 Harness로 확인했지만 모달 메뉴를 사람이 선택하는 시험은 하지 않았다. 캡처는 화면 밖 합성 창이며 설치본 실사용 확인을 대신하지 않는다.
- 줌과 뒤로 이력은 현재 페이지의 실행 중 메모리에만 있다. 실행 취소/다시 실행 후 전체 보기로 돌아간다. 페이지를 바꾸고 다시 돌아올 때 이전 줌 범위를 복원하지 않는다. 깊은 경로는 앞부분을 축약하며 ↑로 조상을 차례로 열 수 있다.
- 속도·메모리 측정을 하지 않았고 성능 개선이나 목표 충족을 주장하지 않는다. 전체 Python·Flutter·CI/MSVC·설치본 연결·기본 Rust 창 전환은 PM/사용자 확인 단계에 남겼다. 기존 Flutter 앱과 설치 파일은 보존했다.
- 푸시 금지 지시를 유지하며 이 카드에서는 로컬 커밋만 남긴다.
