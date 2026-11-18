# 퀵몽땅검색 단축키 창 UI 개편 격차

작성일: 2026-11-18 · 브랜치: `feature/QMSWIN`

## 조사 범위

먼저 `TODO.md`의 **몽땅검색 단축키 창 UI 전면 개편** 항목, `docs/MG2_KTM_2026-11-14.md`, `docs/QMS_DOCUFINDER_PARITY.md`, 팀 운영 규칙과 `ktm/src/layout.rs`, `window.rs`, `tm_state.rs`, `view_dump.rs`를 확인했다. 기존 기능 목록 및 계측은 저장소 코드·문서 확인이며, 이번 작업에서 설치본이나 UI를 실행하지 않았다.

## 구현 여부

| 요구 | 현재 코드 판정 | 근거 / 격차 |
|---|---|---|
| 큰 검색칸 | 구현됨 | `window.rs`가 별도 검색 EDIT 컨트롤과 `SEARCH_BOX_H`를 배치한다. 검색칸 크기 개편의 구체 치수는 TODO에 정해져 있지 않다. |
| 거르기 줄 | 구현됨 | `window.rs`에 검색 범위 줄(기간·형식·폴더)과 옵션 컨트롤이 있으며 `FILTER_H` 레이아웃 계산을 쓴다. |
| 2줄 결과 | 부분 구현 | 결과 화가는 이름, 메타, 발췌 3개 줄 영역을 사용한다. 2줄 요구와 현재 표현이 다르다. 이번 작은 변경은 별도 2줄 카드 높이 계산 및 단위 시험만 추가했으며 화가·창 배치에는 연결하지 않았다. |
| 키보드 우선 | 구현됨 | 검색 중 위/아래·페이지 이동·탭·Enter·Escape와 숫자 선택 처리가 있다. 단축키 흐름의 종단 간 검증은 미실행이다. |
| 입력→첫 그리기 16ms | 미확인 | `input_to_paint_ms` 로그와 MG2 이전 측정은 있으나, TODO의 16ms 목표로 반복 측정한 현재 빌드 결과는 없다. MG2 기록의 입력→그리기 측정은 9.6~14.6ms(두 번째 질의), 즉시 첫 입력은 377~586ms 사례가 적혀 있어 목표 달성 증거로 볼 수 없다. |
| 쉴 때 5MB 안 | 부분 확인 | MG2 측정은 개인 작업 집합 2.34~2.61MB였으나 현재 변경분 기준 실기 재측정은 하지 않았다. |

## 이번 변경

- `ktm/src/layout.rs`: DPI 백분율을 입력받는 순수 `two_line_row_height` 계산과 배율 경계 단위 시험 추가. 기존 `ROW_H`, 행 밀도 및 화면 그리기 동작은 그대로 둔다.
- 첫 시험에서는 새 계산과 기존 70px 행 높이 계약이 충돌하는 것을 확인했다. 기존 계약을 보존하도록 전역 상수 수정은 되돌렸고, 두 줄 카드용 46 논리 픽셀 기준 계산을 별도로 남겼다.

## 시험

명령: `cargo test --manifest-path ktm/Cargo.toml` (MinGW 경로를 PATH 앞에 추가, 임시 `CARGO_TARGET_DIR` 사용). 실행 중 앱을 띄우거나 종료하지 않았다.

최초 시도(전역 행 높이 변경 포함):

```text
test result: FAILED. 222 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.54s
```

실패는 신규 시험의 잘못된 기준값과 기존 `density_heights_differ`의 행 높이 계약이었다. 전역 상수를 복구하고 기준 계산을 조정한 뒤 같은 전체 명령을 재실행했다. 최종 결과의 모든 `test result` 줄:

```text
test result: ok. 224 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.36s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 남은 격차

- 새 두 줄 높이 계산을 실제 결과 화가와 목록 스크롤/선택 레이아웃에 연결하고, 첫 번째 줄에 파일명·경로 등 어떤 내용을 둘지 정한 뒤 view dump를 확장해야 한다.
- 입력→첫 그리기 16ms와 현재 빌드의 유휴 개인 메모리 목표는 앱 실기 계측이 필요하다. 이번 요청의 앱 실행 금지 조건에 따라 미측정이다.
- 큰 검색칸의 목표 크기와 필터 줄의 최종 배치 사양은 TODO에 수치로 명시되지 않았다.
