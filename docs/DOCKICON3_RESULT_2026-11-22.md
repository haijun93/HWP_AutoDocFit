# DOCKICON3 결과 — 하단 독 뒤 검은 사각형 제거 (2026-11-22)

## 원인
`WS_EX_LAYERED` 창에 `SetLayeredWindowAttributes(LWA_ALPHA, 235)`만 걸어 창 전체에 단일 투명도만 적용했다. 알약 밖 네 모서리는 메모리 비트맵의 초기색(검정)이 그대로 노출되었다.

## 선택한 방식: 1순위 UpdateLayeredWindow + per-pixel alpha (premultiplied BGRA DIB)
- `dock/src/native.rs`: `render_layered()`가 ① 검정으로 초기화된 32비트 DIB에 기존대로 `draw_dock`을 그리고(알약 가장자리 AA 픽셀은 검정과 섞여 이미 coverage 만큼 premultiplied), ② 같은 알약(`draw_pill`, 기하 공유)을 흰색으로 그린 마스크 DIB에서 coverage를 읽어 알파로 쓴다. 알약 밖은 B·G·R·A 모두 0(완전 투명), 색은 알파를 넘지 않게 자른다. GDI 글리프·글자는 알약 안쪽이라 알파 255.
- 창 전체 불투명도 235는 `BLENDFUNCTION.SourceConstantAlpha`(=`LAYER_OPACITY`)로 유지(기존과 동일한 시각 농도). `SetLayeredWindowAttributes` 호출은 제거.
- per-pixel alpha 창은 WM_PAINT/InvalidateRect 로 갱신되지 않으므로 `InvalidateRect` 호출을 모두 `refresh()`(렌더 + UpdateLayeredWindow)로 교체, `reposition()` 끝에서도 `refresh()` 호출.
- 순수 로직(`apply_pill_mask`, `unpremultiply_bgra_to_rgba`, `composite_over_bgra`, `encode_png`)은 `dock/src/lib.rs`에 두어 단위 시험.
- 캡처: `dock_<테마>_<dpi>.png`는 마젠타 위에 실제 불투명도로 합성한 RGB(모서리 확인용), `..._alpha.png`는 알파 포함 RGBA.

## 폴백
GDI+ 시작/그래픽스 생성 실패 시 `draw_pill`이 GDI `RoundRect`로 그리고, 마스크도 같은 GDI 경로로 만든다. 즉 가장자리는 계단져도 per-pixel alpha 로 모서리는 투명이다(카드가 말한 SetWindowRgn 대신 같은 마스크 경로를 재사용 — 코드 단순·고대비/폴백 동일 동작). 이 폴백 경로를 실제로 강제 실행한 시험은 하지 않았다(코드 확인만).

## 자동 숨김·DPI·테마
- 자동 숨김(DOCK2): 숨김/표시는 `ShowWindow` 만 바꾸고, 표시 전 `reposition()`이 `refresh()`를 호출하므로 모서리 유지(코드 확인).
- DPI 변경·디스플레이/테마 변경: `WM_DPICHANGED`/`WM_DISPLAYCHANGE`/`WM_SETTINGCHANGE` → `reposition()` → `refresh()`가 새 크기·DPI·팔레트로 다시 렌더(코드 확인).
- 고대비: 팔레트만 달라지고 같은 마스크 경로.

## 시험 결과 (`cargo test --release --locked`, dock)
- lib: `test result: ok. 18 passed; 0 failed`
- bin(native): `test result: ok. 7 passed; 0 failed` — 밝은/어두운 × 96/120/144 DPI + 고대비에서 네 모서리·(1,1)·(3,3) 픽셀이 [0,0,0,0], 중앙·상단 변 안쪽 알파 255, premultiplied 위반 0, 투명 픽셀 비율 10% 미만.
- doc-tests: `0 passed; 0 failed`
- 기존 아이콘 시험(DOCKICON1)·AA 알약(DOCKICON2) 시험 모두 통과(위 18개에 포함).

## 캡처 확인
헤드리스로 밝음/어두움 × 96/120/144 + 고대비 7장(+알파 PNG 7장) 생성. **직접 열어 본 것은 밝음 100%와 어두움 150% 두 장**: 네 모서리가 마젠타(=투명)이고 알약이 둥글며 검은 사각형 없음, 아이콘·라벨 정상. 나머지는 단위 시험의 알파 수치로만 확인. 대표 2장: `docs/img/dockicon3_light_96.png`, `docs/img/dockicon3_dark_144.png`(마젠타는 투명 영역 표시용).

## 못 한 것
실제 창(UpdateLayeredWindow 경로, 일반 모드 실행)은 지시에 따라 띄우지 않아 **실창에서의 확인은 못 했다**. 캡처와 같은 `render_layered` 를 쓰지만 `UpdateLayeredWindow` 호출 자체는 컴파일만 확인. 자동 숨김·DPI 전환의 실동작도 코드 확인뿐.
