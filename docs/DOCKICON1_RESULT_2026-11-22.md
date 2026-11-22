# DOCKICON1 결과 — 독 아이콘을 사이드바와 같은 Material Icons 글리프로 (2026-11-22)

## 구현
- `tools/make_dock_icon_font.py`: Flutter SDK `MaterialIcons-Regular.otf`(Apache-2.0, 1,645,184 B)에서 15개 코드포인트만 남긴 서브셋을 `dock/assets/MaterialIconsDock.otf`(3,672 B)로 생성(fonttools 사용). 라이선스 고지: `dock/assets/MaterialIcons-LICENSE.txt`.
- `dock/src/native.rs`: `include_bytes!`로 dock.exe에 포함, 시작 시 `AddFontMemResourceEx`(프로세스 전용 등록). `draw_glyph`가 face "Material Icons"를 `ANTIALIASED_QUALITY`로 icon_size_px(DPI 배율) 크기에 그림. 색은 palette.foreground, 포커스 링·실행 점은 기존대로 accent. 글꼴 등록/그리기 실패 시 기존 벡터 아이콘(`icon_shapes`)으로 폴백(벡터 코드 유지).
- `dock/src/lib.rs`: `ICON_CODEPOINTS`·`icon_glyph(tab)` + 단위시험.

## 탭별 대응표
독 탭 번호(`TAB_NAMES`) → 사이드바(`tab_navigation_shell.dart` `_icons`) 기준. 독 0~11·14·15는 사이드바 인덱스와 동일.

| 독 탭 | 메뉴 | Material 아이콘 | 코드포인트 |
|---|---|---|---|
| 0 | 문서편집 | description_outlined | 0xefae |
| 1 | 표템플릿 | table_chart_outlined | 0xf41d |
| 2 | 표그림판 | draw_outlined | 0xf05ed |
| 3 | 다이어그램 | account_tree_outlined | 0xee36 |
| 4 | 마인드맵 | hub_outlined | 0xf0616 |
| 5 | 붙임사진 편집 | brush_outlined | 0xef02 |
| 6 | 파일명 일괄변경 | drive_file_rename_outline | 0xe20b |
| 7 | 몽땅검색 | history_toggle_off | 0xe316 |
| 8 | 문서뷰 | grid_view_outlined | 0xf0d7 |
| 9 | 사진뷰 | photo_library_outlined | 0xf29d |
| 10 | 파일변환 | transform | 0xe678 |
| 11 | PDF 편집 | picture_as_pdf_outlined | 0xf2a4 |
| 12 | 자료함(=가상 폴더, kvf) | dashboard_customize_outlined (사이드바 '가상 폴더' 항목) | 0xefa0 |
| 13 | 업무자료함/노트북 | menu_book_outlined (사이드바 '업무자료함' 항목) | 0xf1c2 |
| 14 | 아웃라이너 | account_tree_outlined (사이드바 14, 3번과 같은 아이콘) | 0xee36 |
| 15 | 검색 벤치마크 | speed_outlined | 0xf3c3 |

(사이드바 12=설정, 13=사용 방법은 독에 없는 메뉴라 대응하지 않음. help_outline=0xe30b, settings_outlined=0xf36e.)

## 미확정·주의
- **13번**: 독 라벨은 "업무자료함/노트북"인데 `route()`는 `Route::Clipboard`(kagent 실행)로 연결됨. 라벨 기준으로 업무자료함 아이콘(menu_book_outlined)을 썼으나, 실제 의도가 클립보드라면 사이드바에 클립보드 메뉴·아이콘이 없어 **대응 미확정**. 확인 필요.
- 14번(아웃라이너)은 사이드바에서도 3번(다이어그램)과 같은 아이콘이라 독에서도 동일(의도적 일치).

## 시험 결과
- `cargo test --release --locked`(dock): `test result: ok. 15 passed; 0 failed` (lib), 나머지 2줄 0 passed. 새 시험 `icon_glyphs_match_sidebar_material_icons`가 0..15 전부 글리프 존재, 사이드바 코드포인트 일치(12·13 제외 별도 단언), 중복 없음을 고정.
- 헤드리스 `--capture-dir`로 밝음/어두움 × 100/125/150%(dpi 96/120/144) 6장 생성·확인: 두부 글자·빈 캡처 없음. 대표 3장은 `docs/img/`에 저장.
- 비교: Flutter 위젯 테스트(SDK 글꼴을 FontLoader로 로드)로 사이드바 아이콘 16개를 렌더해 독 캡처와 나란히 비교 → `docs/img/DOCKICON1_compare.png`(위 Flutter, 아래 독). 모양 동일. (일회용 테스트라 저장소에는 미포함.)
- 고대비(--hc)·실제 창 표시(WM_PAINT)·실행 중 독 교체는 하지 않음(요구 범위 밖/지시상 금지). 150% 미만 소형 크기에서의 획 선명도는 육안 확인만 함.
