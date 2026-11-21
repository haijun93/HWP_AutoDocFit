# SETTINGSWIN2 결과 — 앱 설정 창 Rust+Win32 2단계(`settings_win` 창 뼈대: 읽기·표시·저장)

작성: 2026-11-21 · 브랜치 `feature/SETTINGSWIN2` · 근거: `docs/SETTINGS_WIN32_DESIGN_2026-11-20.md`(③ 창 구조·⑤ 공용 부품·부록 A), `docs/SETTINGSRS1_RESULT_2026-11-21.md`

## 변경 내용

- **새 크레이트 `settings_win/`**(의존성 `docfit_config`·`ui_theme`·`windows-sys` 만. `serde_json` 은 `docfit_config` 공개 API(`Value`) 때문에 같은 버전으로 선언, 시험용 `tempfile`):
  - `schema.rs` 키→컨트롤 매핑. 키 이름·종류·기본값은 `docfit_config::visible_schema()`·`defaults()` 에서 가져오고, 화면 이름은 `labels.rs`(= `configuration_service.LABELS` 83개), 안내문·묶음은 `guides.rs`(= Flutter `setting_guides.dart` 안내문 72개)에서 옮겼다.
  - `model.rs` 읽기·변경 추적(●)·검증·검색·저장. 설정 파일은 **`docfit_config` 로만** 읽고(`read_json`·`merged`·`revision`) 쓴다(`set` — 잠금·원자 교체·revision 충돌 검사). Python 경유 없음.
  - `layout.rs` 배치 계산(순수), `paint.rs` GDI 그리기(색은 `ui_theme::Palette` 만), `window.rs` Win32 창·메시지, `capture.rs`·`png.rs` 캡처 모드, `main.rs`.
  - 창 구조: 좌측 묶음 트리 11개(Flutter 분류 10개 + 화면 밖 키 묶음) / 우측 항목 목록 / 위 검색 상자 / 아래 `[되돌리기][저장]`·상태 줄. 컨트롤은 직접 그린 체크·콤보(클릭·Space·←→ 로 순환)·숫자(입력칸 + − + 단추)·글 입력칸이며, 입력칸을 편집할 때만 표준 EDIT 를 위에 겹친다(한글 IME 는 EDIT 가 처리). 검색 상자는 표준 EDIT.
  - 적용 방식: 설계 ③ 의 권장 C 중 이번 단계는 **전부 [저장]으로 일괄 적용**(독립 키 즉시 적용은 다음 단계 — 아래 미구현).
  - 키보드: Tab/Shift+Tab(검색→트리→목록→되돌리기→저장), 트리 ↑↓·Home/End, 목록 ↑↓·PgUp/PgDn·Home/End·Space/Enter(체크·콤보 바꾸기, 입력칸이면 편집)·←→(숫자 ∓1·콤보 순환)·Delete(기본값으로), Ctrl+F 검색, Ctrl+S 저장, Esc(검색어 지움 → 닫기, 변경이 있으면 확인 상자).
- **`docfit_config` 두 곳(소유자 크레이트 보정)**:
  1. `visible_schema()` 를 `pub` 으로 공개(키→컨트롤 매핑이 소유자 스키마를 그대로 쓰도록).
  2. **스키마 결함 수정**: `std_parspace_*`(7키)가 `number` 가 아니라 `text` 로 분류되어 숫자 검증을 받지 않았다. Python `fields()` 규칙(`root.startswith('std_parspace_')` → number)·설계 부록 A("0~1000 숫자 문자열")와 달랐다. `numeric` 판정에 `root.starts_with("std_parspace_")` 를 더했다.
- Python·Flutter·`hwp-auto-docfit.py` 변경 없음. 앱의 설정 진입 교체·Python 설정 서비스 연결은 하지 않았다.

## 캡처(실제 `paint::draw` 를 메모리 DC 에 그려 PNG)

명령: `settings_win --capture-dir <폴더> --capture-set --settings <임시 설정 파일>`(설정은 읽기만, 데모 변경은 모델 API 로 메모리에서만). 개별 지정: `--dark|--hc --dpi N --group <묶음 id·이름> --search <글> --demo-edits`. 150% = `--dpi 144`.
재생성 가능하므로 PNG 는 저장소에 넣지 않았다(각 2~4.5MB, 무압축). 이 PC 에서는 `C:\HWP_AUTODOCFIT\v1803\reports\SETTINGSWIN2_capture\` 에 둔다.

`Read` 도구로 열어 확인한 관찰:

| 파일 | 본 것 |
|---|---|
| `settings_group_light_96dpi.png` | 트리 묶음 11개 이름·부제·항목 수(보고서 모양 34·문단 간격 3·문장·줄 정리 14·표 5·항목기호 글꼴 12·작업 카드 기본값 10·결과·기록 7·일반 2·프로그램·네트워크 11·개발자 도구 2·내부 값 14), 선택 묶음 강조+●, 항목 「어절 줄 나눔 방지」「장평 줄이기」(포커스 테두리)「문장부호 위치 정리」「문장부호 이동 한도」「문장부호 묶음 유지」「조정한 글자 색 표시」 의 체크(켬/끔)·'기본값: …'. 「문장부호 이동 한도」는 7.5 입력 → 빨간 입력칸 + 빨간 칩 "오류: 소수점 없는 정수를 입력해 주세요."·변경 점 ●, 아래 "● 변경 3건 · 입력 오류 1건", [저장] 비활성. 겹침·잘림 없음.
| `settings_group_dark_96dpi.png` | 위와 같은 구성이 어두운 팔레트로 보임(글자·체크·칩 모두 읽힘). |
| `settings_group_light_144dpi.png` | 150%: 글자·컨트롤·칩이 같은 비율로 커지고 겹침·잘림 없음(창 1500×1020). |
| `settings_search_light_96dpi.png` | 검색 "간격": 결과 「최소 줄 간격 (%)」「최대 줄 간격 (%)」(숫자 입력칸 + − +)·「표 칸 최소 줄 간격 (%)」(글 입력칸)·「기본 줄 간격」「문단 위 간격」「항목 사이 빈 줄 삭제」(체크), 각 줄에 소속 묶음 이름, 트리에 묶음별 결과 수(보고서 모양 7 등, 0이면 흐림). |
| `settings_search_dark_144dpi.png` | 검색 결과 + 어두운 + 150%. |
| `settings_owners_light_96dpi.png` | 프로그램·네트워크: 「한글 글꼴 폴더」 — 읽기 전용 줄 + **'미구현'** 칩 + "폴더 선택 대화상자가 필요", 「항상 위에 표시」 — **'소유자 둘 이상'** 칩 + "소유자 3(설계 5-B)…". |
| `settings_font_light_96dpi.png` | 항목기호 글꼴: 「항목기호 글꼴 · □ · 글꼴」 읽기 전용 + '미구현', 「… · 크기 (pt)」 숫자 입력칸 + − +. |
| `settings_internal_light_96dpi.png` | 내부 값 14개(`menu_display_mode`·`rust_rename_preview` …) 읽기 전용 + '내부' 칩. |
| `settings_group_highcontrast_96dpi.png` | 고대비 팔레트(이 PC 는 고대비를 켜지 않아 시스템 기본 색으로 채워진 값). 선택 줄이 `selected_text` 색으로 보임. |

캡처로 찾아 고친 것: 오류 칩 높이가 좁아 글자가 붙음 → 줄 높이 98 로 조정, 검색 중 트리의 ● 가 결과 수와 섞여 혼란 → 검색 중에는 표시 안 함, 소유자 설명이 한 줄에 잘림 → 문구 단축, 최소 창 높이에서 트리 묶음이 잘림(시험이 발견) → 트리 줄 높이를 창 높이에 맞춰 줄이고 최소 높이를 600 으로.

## 실제 창 스모크(자동 클릭, 임시 설정 파일)

`settings_win --settings <임시 파일>` 을 실제로 띄워(시스템 DPI 120) `PostMessage` 로 클릭: 트리 "문장·줄 정리" → 「어절 줄 나눔 방지」 체크 해제 → [저장]. 결과: 임시 파일에 `"prevent_word_split": false` 가 저장되고 기존 `custom` 키 보존, 창에 "저장했습니다. (1건)", 검색 상자에 문자 입력 시 결과가 걸러짐(`pt` → 묶음별 6·7·2건), `CloseMainWindow` 로 정상 종료. 처음엔 창 크기가 시스템 DPI 로만 계산되어 다른 DPI 모니터에서 작게 떴고, 창 DPI 로 다시 맞추도록 고쳤다.
참고: 첫 저장 때 `docfit_config::set` 이 마이그레이션 필드(`card_options_rev`·`*_rev` 등)도 함께 기록한다(소유자 동작, Python 저장과 동일 계열).

## 시험 결과

```text
cargo test --manifest-path settings_win/Cargo.toml
running 14 tests ... test result: ok. 14 passed; 0 failed

cargo test --manifest-path docfit_config/Cargo.toml
test result: ok. 5 passed; 0 failed
```

`settings_win/tests/model_tests.rs` 14개: 키→컨트롤 매핑(114항목 = 편집 87·미구현 11·소유자 둘 이상 2·내부 14, 최상위 97키 전부 존재, 종류 일치) · 묶음·이름 · 읽기만(파일 미생성) · 검증 거부(9개 잘못된 값 모두 이유 표시 + `docfit_config::validate_patch` 도 거부, 정상값 통과, 4096자·NUL) · 오류 있으면 저장 차단 · 최소≤최대 줄 간격 교차 검증 · 저장 왕복(임시 폴더, 숫자는 문자열 저장·중첩 키·모르는 키 보존·`.tmp` 미잔류) · 변경 되돌림 · ± 범위 · revision 충돌(두 모델, 충돌 시 파일 불변·편집 보존·되돌리기 후 재저장) · 손상 파일은 오류 · 검색 필터(1글자는 안 찾음, Enter 확정, 이름·설명 일치, 묶음 가로질러) · 레이아웃(DPI 96·120·144·192 × 3 크기에서 영역·줄 안 요소 겹침·밖으로 나감 없음, 히트 테스트, 스크롤).

## 키 목록(부록 A 97개 최상위 → 화면 잎 100 + 화면 밖 14 = 114 항목)

- **구현(편집 가능) 87**: 체크 62 · 숫자 21(`punctuation_threshold`·`retry_body`·`retry_table`·`linespacing_min/max`·`std_title_subtitle_pt`·`std_parspace_*` 7·글꼴 크기 `symbol_fonts.*.size` 6 + `table_fonts.*.size` 2) · 짧은 글 3(`title_owner_text`·`table_linespacing_min`·`table_spacing_min`) · 선택 1(`search_engine`). 라벨 기호 4개(`label_symbols.*`)는 체크 4개로 구현에 포함.
- **미구현 11**(읽기 전용 줄 + '미구현' 칩): `color`(색 선택 콤보) · `hwp_font_folder`·`paste_add_folder`(폴더 선택 대화상자) · `symbol_fonts.{□,ㅇ,-,※,*,•}.font` 6 · `table_fonts.{header,body}.font` 2(글꼴 목록 콤보).
- **소유자 둘 이상 2**(읽기 전용, 임의로 쓰지 않음): `mongttang_search`(⑤-D 앱 키 + 몽땅검색 autostart) · `always_on_top`(⑤-B 앱 창·빠른 목록·옛 스티커 창). 설계 ⑤-1 의 나머지(A `theme_mode` 는 아래 내부 키에 포함, C 단축키·E·F·H 는 앱 `settings.json` 키가 아님, G `hidden_tabs`·`kclip_window` 는 스키마 밖 키라 이번 목록에 없음).
- **내부(화면 밖) 14**(읽기 전용, '내부' 칩): `menu_display_mode`·`rust_rename_preview`·`rust_timemachine_scan`·`rust_search_hierarchy`·`report_files_rev`·`label_symbols_rev`·`abbreviations`·`abbreviation_source`·`std_parspace_rev`·`std_supplement_rev`·`active_format_profile`·`ui_tk_fallback`·`hierarchy_engine`·`theme_mode`(소유자 4 — ⑤-A, 전용 명령으로만).
- 이번 범위 밖 기능: 항목 안내문 미리보기(Flutter `preview:`), 모두 기본값 버튼(항목별 Delete 기본값만), 독립 키 즉시 적용, 작업 중 잠금(`JOB_RUNNING`, 신호 수단 미확인 — 설계 B-10), 목록·단축키 항목, 접근성(UIA).

## 연결 지점 제안(이번엔 연결하지 않음)

- Flutter 설정 서랍·전체 설정에서 `settings_win.exe`(필요 시 `--settings <파일>`)를 띄우고, 창이 닫힐 때 앱이 설정을 다시 읽는다. 창은 저장 때 revision 충돌을 스스로 검사하므로 Flutter 쪽 `revision` 전달이 필요 없다.
- Python `SettingsService` 저장 경로를 `docfit_config` 로 대체하는 일은 SETTINGSRS1 의 연결 제안 그대로.
- 작업 중 잠금: 앱이 설정 폴더에 신호 파일을 두고 창이 서식 키만 읽기 전용으로 바꾸는 방식(설계 ③-2)을 다음 단계에서 정한다.

## 사용자 실기 확인 항목

1. 한글 IME 로 숫자·글 입력칸·검색 상자에 입력(조합 중 글자·Enter·Esc 동작). 자동 시험은 영문 문자 입력만 확인했다.
2. 실제 Windows 고대비 테마에서 색(고대비는 팔레트 값만 확인, 실제 켠 상태 미확인).
3. 다중 모니터·DPI 이동 시(`WM_DPICHANGED`) 글꼴·배치.
4. 저장 후 실행 중인 Python/Flutter 앱이 바뀐 설정을 읽는 시점(연결은 다음 단계).
5. 스크린 리더 노출(직접 그린 컨트롤은 UIA 제공자 없음).

## 미확인·남은 문제

- `scripts/settings_compare.py` 는 이번 브랜치에서 **이미 실패하는 상태**: Python `DEFAULTS` 가 99개(`kclip_settings_mode`·`outliner_ui` 추가)인데 골든 `docs/settings_defaults.json` 은 97개다. 이번 변경과 무관한 차이이며 차이 키는 이 2개뿐임을 확인했다(`python_top_level 99 vs rust_golden_top_level 97`). 골든 갱신은 별도 카드가 필요하다. 이 두 키는 `docfit_config` 스키마에 없으므로 설정 창에 나타나지 않는다.
- `std_parspace_*` 숫자 검증 수정(위)은 `docfit_config` 단위 시험 5개와 `settings_win` 시험으로만 확인했고, Python 비교기는 위 이유로 전체 통과를 확인하지 못했다.
- UTF-8 BOM 이 붙은 `settings.json` 은 `docfit_config::read_json` 이 읽기 오류로 처리한다(스모크 중 발견, Python 저장은 BOM 을 쓰지 않아 현재 문제는 아님).
- 콤보는 드롭다운 목록이 아니라 순환 방식(선택지가 2개뿐인 `search_engine` 만 해당). 선택지가 많은 콤보(글꼴·색)는 미구현 키와 함께 다음 단계.
- 검색은 소문자 부분 일치만(Flutter 와 같은 `contains` 규칙, 초성 검색 없음).

## 실행 환경

`CARGO_TARGET_DIR=E:/cargo-target/SETTINGSWIN2`(환경변수만, 저장소에 `.cargo/config.toml` 없음), PATH 앞에 `%LOCALAPPDATA%/DocFitTools/mingw64/bin`. 이 PC 의 실제 설정 파일은 읽지도 쓰지도 않았다(시험·캡처·스모크 모두 임시 폴더). 푸시하지 않았고 로컬 커밋만 남긴다.
