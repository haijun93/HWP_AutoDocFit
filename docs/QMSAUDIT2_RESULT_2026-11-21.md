# QMSAUDIT2 결과 — 퀵몽땅검색(ktm) 화면 점검 2차 (2026-11-21)

1차(`docs/QMSAUDIT_RESULT_2026-11-21.md`)의 '남은 문제' 6건을 고쳤다. 캡처 모드(`--capture-dir`)로 전·후를 다시 만들어 직접 열어 확인했다.
캡처 PNG는 저장소에 넣지 않았다(같은 명령으로 재생성). 명령: `ktm.exe --capture-dir <폴더> [--dpi 144] [--size 520x300]`.

## 수정 6건 (전 → 후)
| # | 문제 | 수정 | 전 → 후(장면) |
|---|---|---|---|
| 1 | 바둑판 카드 경로 글자가 이유 배지에 맞닿음 | `window.rs` 경로 오른쪽 끝을 배지에서 8px(배율) 안쪽으로 | 16_grid: 경로가 배지에 붙어 잘림 → 경로와 배지 사이 여백, 말줄임 |
| 2 | 발췌 없는 선택 행의 미리보기 칸이 빔 | 빈 경우 흐린 글자로 '본문 발췌가 없습니다' 표시. 사이드바 열림 때 글자가 사이드바 위에 그려지던 왼쪽 여백도 `side_w` 기준으로 정정 | 16_grid·15_sidebar: 빈 칸 → 안내 문구 |
| 3 | 144dpi 배지 글자가 아래로 치우침 | 이유·확장자 배지 사각형을 글자 높이+4px 기준으로 첫 줄 가운데에 맞춤 | 15_sidebar_144dpi: 글자 아래쪽 쏠림 → 사각형 안 수직 가운데 |
| 4 | 연관 검색어 칩 간격이 좁음 | `layout.rs` `related_chip_gap`: 논리 6px×DPI 배율(96→6, 144→9). 그리기·히트 테스트가 같은 함수를 쓴다 | 04_related_chips_144dpi: 4px → 9px |
| 5 | 최소 크기에서 '파일명 매…' | `pick_fitting_label`: 칸에 안 들어가면 짧은 라벨('이름 7 ▾'·'본문 7 ▾') | min/03_results: '파일명 매…' → '이름 7 ▾' / '본문 6 ▾' |
| 6 | 사이드바 폭 220이 배율을 안 따름 | `sidebar_width_px(open,width,full)`; 호출부는 `scale(h, SIDEBAR_W)`. 창 넓히기(Show·토글·캡처)도 같은 배율 폭 | 15_sidebar_144dpi: 사이드바 220px/창 1270 → 330px/창 1380, 내용 폭 1050(=700×1.5) 유지 |

캡처 위치(이 PC, 참고용): `E:/docfit-tmp/QMSAUDIT2-cap/{before,after}/{l96,l144,min}/` (재생성 가능).
이미 정상인 부분(오류 화면, 모달 배경, 통계/설정, 날짜·형식 칩, 상호작용 14건)은 건드리지 않았고 144dpi 상호작용 PASS 14건은 그대로다.

## 시험
`cargo test --manifest-path ktm/Cargo.toml`:
- `test result: ok. 258 passed; 0 failed; 1 ignored`
- `test result: ok. 3 passed; 0 failed; 0 ignored`
- `test result: ok. 0 passed; 0 failed; 0 ignored`
추가: `sidebar_width_scales_with_dpi_and_keeps_content_width`, `related_chip_gap_scales_with_dpi`, `pick_fitting_label_falls_back_to_short_form`. 갱신: `view_dump` 칩 간격 기대값 60→62(간격 4→6px).
배지 수직 정렬·경로 여백·안내 문구는 화면 코드라 캡처로만 확인했다.

## 남은 문제
- 최소 크기(520x300)에서 제목줄 단축키 안내 말줄임은 의도된 줄임으로 그대로 둠.
- 경로가 매우 좁은 카드에서는 여전히 경로가 몇 글자만 보인다(경로 최소 폭 70px 규칙).
- 1차 문서 §4의 TODO 미구현 항목(스마트 폴더·차트 등)은 범위 밖.

## 사용자 실기 확인 항목
- 실제 125·150% 모니터에서 사이드바 토글 시 창 폭 증가·작업 영역 경계 동작, 혼합 DPI 이동.
- 설정의 UI 배율(`bounded_ui_scale`) 변경 시 사이드바 폭·칩 간격·배지 정렬.
- 사이드바가 열린 상태에서 하단 미리보기 안내 문구 위치, 실제 마우스로 칩 클릭(간격 변경으로 칩 위치가 2px 이동).
- 캡처는 메모리 DC 렌더라 DWM 그림자·테두리와 완전히 같지 않다.
