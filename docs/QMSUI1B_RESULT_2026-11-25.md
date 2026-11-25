# QMSUI1B — 퀵몽땅검색 UI 정리

작업일: 2026-11-26(KST). 브랜치: `feature/QMSUI1B`. 기준: `github/main`의 `43e59a1417dcaa786bcfcc53c908e44538720bcc`. 로컬 커밋만 수행하며 푸시·설치·게시·main 병합은 하지 않는다.

## 기준 화면에서 확인한 문제

- TODO의 기간·요약 Y좌표 중복은 기준 코드에 이미 수정되어 있었다. 기본 문구에서는 네 배율 모두 그 중복을 재현하지 못했다.
- 긴 한글 연관어 네 묶음은 두 줄로 그려지는데, `quick_search_layout`은 관련어 높이를 한 줄로만 확보했다. 둘째 줄 칩이 옵션 첫 줄을 덮는 현상을 100·125·150·200%에서 재현했다.
- 기간·형식·폴더는 고정 폭으로 한 줄에 이어 붙어 있었다. 좁은 창에서는 폴더 후보를 표시할 공간이 부족했다. 문구 길이·글꼴 폭을 줄 높이와 함께 계산하는 경로가 없었다.
- 기존 `feature/QMSUI1`은 읽기만 했다. 그 브랜치의 변경은 주로 후속 검색 기능을 제거하는 차이여서 가져오지 않았다.

## 변경

- `layout.rs`에 글꼴 높이·문자열 폭을 인자로 받는 순수 배치 계산을 추가했다. 실제 창은 `GetTextMetricsW`·`GetTextExtentPoint32W`로 선택된 맑은 고딕 글꼴을 측정한다.
- 연관어 줄 감김 높이 → 옵션(접힘/펼침) → 기간 → 형식 → 선택적 폴더 → 요약 → 선택적 자동완성 → 결과 목록 순서로 쌓는다. 표시 상태가 실제 높이로 바뀌는 지점을 한곳에 모았다.
- 기간·형식·폴더·관련어·옵션 팝업의 그리기와 클릭이 같은 사각형을 사용한다. 긴 한글 문구는 `DT_END_ELLIPSIS`로 표시한다. 요약 문구·두 접기 영역·복사·CSV의 가로 영역을 분리했다.
- 폴더 후보가 많아도 한 줄만 사용하며, 긴 선택 폴더를 남은 폭에서 말줄임한다. 미리보기는 결과 한 줄과 함께 들어갈 때만 확보해 요약 줄을 덮지 않게 했다. 상태줄 공간도 먼저 제외한다.
- 기존 검색·정렬·필터 이벤트, 설정 기본값, 질의 API는 유지했다. 기존 고정 치수 레이아웃 API와 덤프 API는 호환용으로 유지하며, 캡처 모드는 실제 측정 좌표를 별도 텍스트로 기록한다. 새 상주 타이머·캐시·의존성은 추가하지 않았다.

## 전·후 캡처

기존 `ktm --capture-dir`의 실제 GDI `draw`와 자식 EDIT의 `WM_PRINT`를 사용했다. 화면 이미지 재구성 목업이 아니다. 기준 소스의 격리 사본에는 같은 긴 문구 장면만 추가했고, 레이아웃·그리기 코드는 바꾸지 않았다. 합성 자료만 사용했다.

각 전·후: 25장면 × 4배율 × 2창크기(논리 700×510, 520×400) 실행. 저장소에는 결과·연관어·옵션·결과 내·한영 긴 이름·긴 한글의 6장면씩, 총 96 PNG를 무손실 압축해 보존했다. 다른 19장면씩은 임시 산출물이며 저장소에 넣지 않았다. 수정 후 저장 장면의 실제 좌표는 PNG 옆 `.txt`에 있다.

| 배율 | 긴 한글 기준 화면(520×400) | 수정 화면(520×400) |
|---|---|---|
| 100% / 96dpi | [전](QMSUI1B_CAPTURES/before/96-520x400/25_long_labels_96dpi.png) | [후](QMSUI1B_CAPTURES/after/96-520x400/25_long_labels_96dpi.png) |
| 125% / 120dpi | [전](QMSUI1B_CAPTURES/before/120-520x400/25_long_labels_120dpi.png) | [후](QMSUI1B_CAPTURES/after/120-520x400/25_long_labels_120dpi.png) |
| 150% / 144dpi | [전](QMSUI1B_CAPTURES/before/144-520x400/25_long_labels_144dpi.png) | [후](QMSUI1B_CAPTURES/after/144-520x400/25_long_labels_144dpi.png) |
| 200% / 192dpi | [전](QMSUI1B_CAPTURES/before/192-520x400/25_long_labels_192dpi.png) | [후](QMSUI1B_CAPTURES/after/192-520x400/25_long_labels_192dpi.png) |

긴 한글 장면의 둘째 칩 줄이 옵션 줄을 덮던 Y구간은 다음과 같다(물리 픽셀, 끝 좌표 제외). 두 창폭 모두 재현했다. 수정 후에는 칩 네 줄 아래에서 옵션이 시작한다.

| 배율 | 기준 겹침 Y구간 | 수정 후 관련어 띠 | 수정 후 요약 띠 |
|---|---|---|---|
| 100% | 104..130 | 76..188 | 330..362 |
| 125% | 130..163 | 95..235 | 412..452 |
| 150% | 156..195 | 114..286 | 501..549 |
| 200% | 208..260 | 152..376 | 660..724 |

전·후 긴 문구 PNG는 네 배율 모두 이미지 보기로 직접 확인했다. 수정 후 200장면에서 띠 순서·칩 사각형 교차 없음 검사 200건이 통과했다. 실제 창 프로시저의 기간 5개·형식 5개·폴더·요약 접기·옵션 팝업 등 상호작용은 21건 × 8조합 = 168건 통과했다. 배율·창폭별 `checks.txt`에 그대로 남겼다.

재현 명령(빌드·출력 위치는 환경변수로 지정):

```powershell
cargo build --release --manifest-path ktm/Cargo.toml
& "$env:CARGO_TARGET_DIR/release/ktm.exe" --capture-dir $captureDir --dpi 120 --size 520x400 --scenario all
cargo test --manifest-path ktm/Cargo.toml
python -m unittest discover -s tests
```

## 시험

변경 크레이트는 `ktm` 하나다. 추가 단위 시험은 배율 4종 × 창폭 3종 × 문구 3종 × 옵션 2상태 × 자동완성 2상태의 144조합과 긴 칩·작은 높이·미리보기·폴더 1,000후보 경계를 확인한다. 기존 시험은 변경하지 않았다.

최종 `cargo test --manifest-path ktm/Cargo.toml` 종료 코드 0. 출력된 모든 결과 줄:

```text
test result: ok. 260 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.61s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

합계 268개 통과, 기존 수동 1만 건 성능 시험 1개 ignored. 최초 컴파일에서 새 좌표 변수의 선언 범위 오류를 고쳤고, 캡처에서 발견한 미리보기 덮임도 실제 원인과 회귀 시험을 추가해 해결했다. 시험 기준을 낮추거나 기존 시험을 삭제하지 않았다.

전체 Python 시험의 `Ran`·`OK` 줄: 실행 후 기록 예정. `KLIB_EXE`는 기존 로컬 release 바이너리를 지정한다(SHA-256 `2D2DE5ACE234DF6DC67FD327E9E28EE74ACD755003857B69B6DBFBE665184A9B`).

Flutter 파일은 변경하지 않아 Flutter 시험 대상이 아니다. XML 파싱 코드와 한/글 문서 처리 코드는 변경하지 않았다.

## 첫 그림·유휴 자원

다른 빌드·시험이 없는 구간에서 합성 색인으로 전·후 각각 1회만 측정한다. 첫 그림은 기존 `--semi1-probe`의 EN_CHANGE → EndPaint이며 DWM 화면 표시 시각과 구분한다. 숨긴 상주 프로세스의 개인 작업 집합·개인 바이트·CPU를 각각 60초 관찰한다. 측정과 전체 Python 시험은 공용 COM 잠금 아래 순차 실행한다.

측정 결과: 실행 후 기록 예정.

## 확인하지 못한 범위

- 사용자 화면에서의 확인, 물리 모니터의 실제 DPI 전환·DWM 합성 화면은 확인 못 함. PNG는 DPI 강제값으로 같은 GDI 렌더러를 실행한 결과다.
- 사용자 실제 색인·원본 문서의 검색 결과와 사용자 IME 입력 타이밍은 확인 못 함. 합성 문서·가짜 캡처 행만 사용했다.
- 극단적으로 작은 높이에서는 상태줄 위에서 제어 영역을 자르고 결과·미리보기 공간이 없을 수 있다. 화면 밖 칩은 클릭 영역을 노출하지 않는다.
