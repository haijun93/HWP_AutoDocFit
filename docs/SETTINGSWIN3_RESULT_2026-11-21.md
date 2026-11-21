# SETTINGSWIN3 결과 — 앱 설정 창 3단계(복잡한 항목 컨트롤 + 앱 연결 `settings_ui`)

작성: 2026-11-21 · 브랜치 `feature/SETTINGSWIN3` · 근거: `docs/SETTINGSWIN2_RESULT_2026-11-21.md`, `docs/SETTINGS_WIN32_DESIGN_2026-11-20.md` ③⑤⑧, `outliner_ui` 연결 방식(OUTLINERWIN4)

## 변경 내용

### 1. 미구현 11개 키의 컨트롤(전부 구현) — `settings_win`
새 `controls.rs`(순수 보조 로직 + Win32 글꼴 열거·폴더 대화상자)와 `schema.rs`·`model.rs`·`layout.rs`·`paint.rs`·`window.rs` 확장.

| 컨트롤 | 대상 키 | 동작 · 검증 |
|---|---|---|
| **색 견본**(`Control::Color`) | `color`(표시 색) | 견본 9개 한 줄(빨강·주황·초록·파랑·하늘·보라·자홍·갈색·회색 = `hwp-auto-docfit.py` 의 `COLOR_OPTIONS`와 같은 값). 클릭·←→ 순환, 선택 색은 고리 + "선택: 초록". 목록에 없는 저장 값은 "목록에 없는 값: …"으로 보이고 그대로 둔다 |
| **글꼴 입력 + 고르기**(`Control::Font`) | `symbol_fonts.{□,ㅇ,-,※,*,•}.font` 6 · `table_fonts.{header,body}.font` 2 | 입력칸(표준 EDIT, 한글 IME) + ▾ 단추/F4 → **글꼴 고르기 패널**: 이 PC 에 설치된 글꼴을 `EnumFontFamiliesExW` 로 열거(이름순·중복 없음), 각 줄을 그 글꼴로 그려 미리 보임, 필터 입력·↑↓·PgUp/PgDn·Enter·휠·클릭·Esc, 현재 값에 ✓. 검증: 빈 이름·64자 초과·제어 문자는 오류, **목록에 없는 이름은 저장을 막지 않고 알림만**("한/글 전용 글꼴일 수 있음") |
| **폴더**(`Control::Folder`) | `hwp_font_folder`, `paste_add_folder` | 입력칸 + [찾아보기…]/F4 → 폴더 선택 대화상자(현재 값에서 시작). 검증: 260자 초과·예약 문자(`< > : " \| ? *`, 드라이브 글자의 `:`와 UNC `\\`는 허용)는 오류, **없는 폴더는 알림만**("지금은 이 폴더가 없습니다"), 비우면 자동 |
| **목록**(`Control::List{max}`) | 앱 스키마에 해당 키 없음(아래 '정직한 한계') | 줄에는 "3개: a, b, c" + [편집…] → **목록 편집 패널**: 입력 + [추가]/Enter, 선택 후 [삭제]/Delete, [▲ 위로]·[▼ 아래로]/Ctrl+↑↓, 앞뒤 공백 제거·빈 값·1000자·NUL·대소문자 무시 중복·최대 개수 검사, 오류는 패널 위쪽 칩 |
| **단축키**(`Control::Hotkey`) | 앱 스키마에 해당 키 없음 | 칸을 누르면 "키를 누르세요…" 대기 → Ctrl/Alt/Shift/Win + 문자·숫자·F1~F24·Space 를 누르면 `Ctrl+Alt+K` 형태로 입력(조합 키 없이/Enter 등은 이유 표시, Esc 취소, Tab 은 대기 종료), [지우기] |
| 여러 줄 글 | 해당 키 없음 | **구현하지 않음**(필요한 키가 없다) |

- **소유자가 둘 이상인 키 2개**(`mongttang_search`·`always_on_top`)는 설계 ⑤의 규칙대로 **읽기 전용 유지**, 설명을 구체화: "저장하면 상주 프로그램도 켜고 꺼야 해서 앱 설정 서랍에서만 바꿉니다"(Python `save_settings` 가 ktm 을 켜고 끔) / "켜는 즉시 창을 바꾸는 전용 명령이 필요해 앱 설정 서랍에서만 바꿉니다"(`always_on_top` 브리지 명령이 창을 바로 바꿈, 개발자 모드 조건).
- **내부 키**는 계속 숨김. 다만 `docfit_config::visible_schema()`의 제외 목록이 Python `EXCLUDED` 와 어긋나 있어(`kclip_settings_mode`·`outliner_ui` 가 설정 창에 글 입력칸으로 보였음) 두 키와 새 `settings_ui`를 제외 목록에 더했다 → 내부(화면 밖) 키 14 → **17개**. 트리 부제는 개수를 적지 않는 "편집하지 않는 내부 키"로 바꿨다.
- 단일 인스턴스: 같은 설정 파일을 여는 창이 이미 있으면 그 창을 앞으로 가져오고 새 프로세스는 끝난다(뮤텍스 이름에 설정 파일 경로 해시).

### 2. 앱 연결(`settings_ui`: 'flutter' 기본 | 'native')
| 위치 | 변경 |
|---|---|
| `docfit_app/settings_schema.py` | `DEFAULTS['settings_ui'] = 'flutter'` |
| `docfit_app/configuration_service.py` | `EXCLUDED` 에 `settings_ui` |
| `docs/settings_defaults.json`(골든) | `settings_ui` 추가(이전 카드가 남긴 97↔99 어긋남은 `687c39bf` 에서 이미 해결, 이번에 100개) |
| `docfit_config/src/lib.rs` | 제외 목록에 `kclip_settings_mode`·`outliner_ui`·`settings_ui` |
| `docfit_app/settings_window_service.py`(새) | `ui_mode`(읽기·쓰기, 'flutter'/'native' 외 거부)·`open_native`·`open_if_native`(native 일 때만 `settings_win.exe --settings <앱 설정 파일>` 실행, 기본에서는 아무 일 없음). 오류 코드 `SETTINGS_WINDOW_MISSING`·`SETTINGS_WINDOW_FAILED` |
| `desktop_web_ui.py` | 브리지 메서드 `settings_window(payload)` + `_BrowserApi` 허용 목록 |
| `docfit_app/application_service.py` | `dispatch` 개수표·분기에 `settings_window` |
| `flutter_ui/lib/features/settings/settings_ui_gate.dart`(새) | `openNativeSettingsIfEnabled(api)`(어떤 실패도 예외 없이 기존 화면으로) + 설정 서랍의 "설정 창 방식" 줄(앱 안 ↔ 새 창 전환: 창이 실제로 열릴 때만 native 로 저장) |
| `flutter_ui/lib/main.dart` | `openSettings()`(상단 톱니·도구 모음 "설정"이 부름)가 문지기를 거침. native 이고 창이 열리면 서랍을 열지 않음, 창을 못 열면 안내 후 서랍. **톱니 오른쪽 클릭은 항상 앱 안 서랍**(새 창 방식에서 돌아오는 길). 서랍에 위 "설정 창 방식" 줄 |
| `HWP_AutoDocFit.spec`·`.github/workflows/build-windows-exe.yml`·`installer/setup.iss`·`.gitignore` | `outliner_win` 과 같은 방식으로 `settings_win.exe` 시험(`cargo test --release --locked`)→빌드→`resources/` 복사, 번들, 설치 때 `StopHelper`, VC++ 런타임 비의존 검사 목록 |

기본값이 'flutter'이므로 **사용자 눈에 보이는 변화는 없다**(톱니·서랍 동작 그대로. 서랍에 "설정 창 방식" 줄이 하나 늘었다 — 눈에 보이는 유일한 차이이며 원치 않으면 지우면 된다). 사이드메뉴 12번(코드상 열리지 않음, 설계 B-3)·Dock(설정 항목 없음)·단축키 창(⚙ 메뉴, 별도 프로세스)은 앱 안에서 설정을 여는 곳이 아니어서 건드리지 않았다. `Cargo.lock` 은 cargo 가 만든 그대로(`--locked` 통과, 손 편집 없음).

### 3. 저장 충돌
`docfit_config::set` 의 revision 검사를 그대로 쓴다(새 창이 읽은 시점의 revision 을 저장 때 보냄). 앱(Python)이 그 사이 같은 파일을 쓰면 **저장을 거부**하고 "다른 곳에서 설정이 바뀌었습니다. [되돌리기]로 다시 불러온 뒤 적용해 주세요."를 아래 줄에 표시, 파일은 한 바이트도 바꾸지 않고, 내 편집은 화면에 남으며, [되돌리기] 후 다시 저장하면 앱이 쓴 값과 모르는 키가 보존된다(시험 `save_is_refused_when_the_app_wrote_the_file_meanwhile`, 여러 번 번갈아 쓰는 `concurrent_writers_never_lose_either_save`).

## 구현/미구현 키 목록(키 수는 소유자 스키마에서 계산 — 시험도 숫자를 고정하지 않는다)

항목 117 = 화면 노출 100 + 내부 17(트리 묶음별 개수 합 34+3+14+5+12+10+7+2+11+2+17 = 117).
- **편집 가능 98**: 체크 62 · 숫자 21 · 짧은 글 3 · 선택 1(`search_engine`) · **색 1 · 글꼴 8 · 폴더 2(이번에 구현한 11개)**.
- **미구현 0**(이전 11 → 0).
- **소유자 둘 이상 2**(읽기 전용): `mongttang_search`·`always_on_top`.
- **내부(화면 밖) 17**(읽기 전용): 이전 14(`menu_display_mode`·`rust_*` 3·`*_rev` 5·`abbreviations`·`abbreviation_source`·`active_format_profile`·`ui_tk_fallback`·`hierarchy_engine`·`theme_mode`) + `kclip_settings_mode`·`outliner_ui`·`settings_ui`.
- 컨트롤은 구현됐지만 **앱 스키마에 대응 키가 없는 것**: 목록·단축키(아래).

### 정직한 한계 — 목록·단축키
카드가 말한 목록·단축키 컨트롤은 구현·시험했지만, **앱 `settings.json` 스키마에는 그런 키가 없다**(설계 ⑤-1 C 의 `hotkey`·목록 키들은 kagent·ktm·kvf 각자의 파일 소유). 그래서 실제 설정 창에는 아직 나타나지 않고, 캡처·시험에서만 `--demo-controls` 로 덧붙인 시연 항목 두 개("시연: 제외할 프로그램 목록", "시연: 단축키 입력", 화면에 '설정 키 아님'이라 적음)로 같은 그리기·동작 코드를 보인다. `docfit_config` 의 `validate_patch` 는 목록 종류를 모르므로, 앱 스키마에 목록 키가 생기면 `docfit_config` 에 "list" 종류 검증을 더해야 한다.

### 폴더 대화상자 — IFileDialog 아님
카드는 IFileDialog 를 말했지만 `windows-sys` 에 COM 인터페이스 래퍼가 없어 kagent `settings_window.rs` 와 같은 `SHBrowseForFolderW`(새 대화상자 스타일·초기 폴더 지정)를 썼다. 사용자 눈에는 폴더 선택 창으로 같다.

## 캡처(실제 `paint::draw`, `--capture-controls`)
명령: `settings_win --capture-dir <보고서 폴더>/SETTINGSWIN3_capture --capture-controls --settings <없는 임시 파일>`(설정은 읽기만, 데모 변경은 모델 API 로 메모리에서만). 150% = `--dpi 144`. PNG 19장은 재생성 가능하므로 저장소에 넣지 않았다. 아래 표는 `Read` 로 **직접 열어 본 것**만 적는다. 연 파일: `color_light_96`·`color_dark_96`·`color_light_144`·`folder_light_96`·`folder_light_144`·`folderwarn_light_96`·`fontrows_light_96`·`fontpick_light_96`·`fontpick_light_144`·`listhotkey_light_96`·`listedit_dark_96`·`listedit_light_144`·`hotkeywait_light_96`(수정 뒤 최종본). **열지 않은 파일**(같은 코드·구도의 변형, 확인했다고 주장하지 않음): `folder_dark_96`·`folderwarn_dark_96`·`fontpick_dark_96`(실제 창 다크 스모크에서 같은 패널을 봄)·`listhotkey_dark_96`·`listedit_light_96`. 일부는 수정 전 빌드로 열었고(글꼴·목록·폴더), 수정은 표시 글·테두리색·내부 키 숨김뿐이라 구도가 같지만 최종 빌드 PNG 로 다시 열지는 않았다.

| 파일 | 본 것 |
|---|---|
| `settings_color_light_96dpi` · `settings_color_dark_96dpi` · `settings_color_light_144dpi` | 「표시 색」 줄: 견본 9개(빨강…회색)가 한 줄, 초록에 고리, 오른쪽 아래 "선택: 초록", 변경 ●, "기본값: 파랑". 다크에서 견본·고리·글자 모두 읽힘. 겹침·잘림 없음 |
| `settings_folder_light_96dpi` · `…_light_144dpi` | 「한글 글꼴 폴더」: 입력칸 `C:₩Windows₩Fonts` + [찾아보기…] 단추, 변경 ●, 포커스 테두리, "기본값: (비어 있음)". 150%에서 같은 비율로 커짐. 아래 「항상 위에 표시」는 읽기 전용 칸 + '소유자 둘 이상' 칩(목록 끝에서 잘려 보이는 것은 스크롤) |
| `settings_folderwarn_light_96dpi` · `…_dark_96dpi` | 결과·기록 묶음 「붙여넣기 문서 저장 폴더」: 없는 폴더 입력값 `C:₩없는폴더₩붙여넣기 문…`(말줄임) + "※ 지금은 이 폴더가 없습니다. 경로를 확인해 주세요."(오류 칩이 아니라 알림 글) |
| `settings_fontrows_light_96dpi` | 항목기호 글꼴 묶음: 「항목기호 글꼴 · □ · 글꼴」 입력칸 + ▾ 단추, 아래 크기 숫자 칸, 「ㅇ」 글꼴을 '맑은 고딕'으로 바꾼 변경 ●, 「- · 글꼴」 줄의 "※ 이 PC에 설치된 글꼴 목록에 없습니다(한/글 전용 글꼴일 수 있음)." |
| `settings_fontpick_light_96dpi` · `_dark_96dpi` · `_light_144dpi` | 글꼴 고르기 패널: 제목 "글꼴 고르기 · 항목기호 글꼴 · ㅇ · 글꼴", 입력칸 "글꼴 이름 찾기" + "387개", 목록 각 줄이 그 글꼴로 그려짐, 선택 줄 "✓ 맑은 고딕"(하이라이트), 아래 "↑↓ 고르기 · Enter 선택 · Esc 닫기" + [닫기]. 3장 모두 패널이 목록 영역 안, 겹침·잘림 없음 |
| `settings_listhotkey_light_96dpi` · `_dark_96dpi` | 프로그램·네트워크 묶음 아래쪽: 「시연: 제외할 프로그램 목록」 줄 "3개: notepad.exe, c…"(말줄임) + [편집…], 변경 ● |
| `settings_listedit_light_96dpi` · `_dark_96dpi` · `_light_144dpi` | 목록 편집 패널: 제목 "목록 편집 · 시연: 제외할 프로그램 목록", 입력칸 "새 값 입력 후 Enter" + [추가], 목록 1 notepad.exe / 2 calc.exe(선택) / 3 mspaint.exe, [삭제] [▲ 위로] [▼ 아래로] [닫기]. 3장 모두 겹침·잘림 없음 |
| `settings_hotkeywait_light_96dpi` | 「시연: 제외할 프로그램 목록」 아래 「시연: 단축키 입력」: 입력 대기 칸 "키를 누르세요…"(포커스 테두리) + [지우기], 아래 "Ctrl·Alt·Shift·Win 과 함께 키를 누르세요. Esc 로 취소". 대기 칸 글자가 처음엔 길어 잘렸고(→ 문구 단축) 대기 테두리가 오류처럼 빨갛게 보였다(→ 포커스 색으로) — 캡처로 찾아 고침 |

캡처로 찾아 고친 것: 위 둘 + 프로그램 묶음에 `kclip_settings_mode`·`outliner_ui` 가 글 입력칸으로 보이던 것(내부 키로 숨김) + 트리 부제 "14개" 고정 문구.

## 실제 창 스모크(자동 클릭·PrintWindow, 임시 설정 파일, 시스템 DPI 120·다크)
- 항목기호 글꼴 묶음 → 첫 줄 ▾ 클릭 → 패널이 실제 창에 뜸(EDIT 가 입력칸에 겹침) → EDIT 에 "고딕" 입력 → "19개"로 걸러짐 → ↓ 로 HY중고딕 선택 → Enter → 값 반영 → [저장] → 상태 줄 "저장했습니다. (1건)", 임시 파일에 `symbol_fonts."□".font` 저장. 영문 "Malgun"은 0건(`EnumFontFamilies` 가 이 PC 에서 한글 이름을 돌려줌).
- 검색 "표시 색" → 첫 줄 색 견본 7번째 클릭 → [저장] → 임시 파일 `"color": "magenta"`.
- 같은 설정 파일로 두 번째 실행 → 즉시 끝나고 기존 창을 앞으로(`ok 이미 열려 있는 설정 창을…`), 다른 설정 파일은 별개 창.
- 확인하지 못함: 실제 창에서 폴더 대화상자(모달)·목록 편집·단축키 입력(실제 창에는 해당 키 없음, 시연 항목은 캡처·모델 시험으로만).

## 시험 결과(실행한 명령과 결과 줄)

환경: `CARGO_TARGET_DIR`(환경변수만, 저장소에 `.cargo/config.toml` 없음), PATH 앞에 `%LOCALAPPDATA%/DocFitTools/mingw64/bin`.

```text
cargo test --locked --manifest-path settings_win/Cargo.toml
  tests\control_tests.rs: test result: ok. 12 passed; 0 failed
  tests\model_tests.rs:   test result: ok. 15 passed; 0 failed
cargo test --locked --manifest-path docfit_config/Cargo.toml
  test result: ok. 5 passed; 0 failed
python -m unittest tests.test_settings_window_service tests.test_bridge_methods tests.test_distribution_build tests.test_outliner_service tests.test_theme_mode tests.test_core_app_state tests.test_kclip_window_state tests.test_search_engine_settings
  Ran 68 tests ... OK
python scripts/settings_compare.py --exe <settings-core.exe>
  cases 5, equal True   (골든 97↔99 어긋남이던 defaults-golden 도 통과)
flutter analyze            → No issues found!
flutter test test/settings_ui_gate_test.dart test/settings_dialog_test.dart test/outliner_ui_gate_test.dart → +18: All tests passed!
flutter test test/workspace_test.dart test/modal_dialogs_test.dart test/theme_mode_setting_test.dart test/settings_ui_gate_test.dart → +47: All tests passed!
flutter build web --release → exit 0 (Built build\web)
```
- `settings_win` 시험 27개: 모델·검증·검색·저장 왕복·revision 충돌·배치(모든 DPI·크기에서 새 컨트롤 모양·견본·겹침 패널이 영역 안)·**항목 수는 스키마에서 계산**(`전체 설정 키 N개` 같은 고정 숫자 없음)·11개 키 저장 왕복·색 순환·단축키 규칙·목록 연산·폴더/글꼴 규칙·저장 충돌.
- 기존 `workspace_test.dart` 의 기록 비교 두 곳에 `settings_window`(새 읽기 명령)를 제외하는 한 줄씩을 더했다(시험 의도는 그대로: Tk 명령·쓰기 호출 검사). `test_distribution_build.py` 의 outliner 시험이 목록 끝 괄호까지 비교하던 것을 `'outliner_win.exe'` 포함 여부로 완화했다.
- 전체 시험은 돌리지 않았다(지시).

## 사용자 실기 확인 항목
1. **실제 폴더 선택 대화상자**: [찾아보기…]/F4 로 폴더를 고르면 입력칸에 들어가는지, 현재 값에서 시작하는지, 취소 시 값 불변인지(모달이라 자동 시험 불가).
2. **키 입력**: 단축키 칸은 실제 창에 없다. 앱 스키마에 키가 생기면 Win 키 조합(셸이 가로챔)·Alt 조합(`WM_SYSKEYDOWN`)이 실제로 들어오는지.
3. **한글 입력**: 글꼴 입력칸·글꼴 고르기 필터·목록 입력칸·폴더 입력칸에서 한글 조합 중 글자·Enter·Esc(스모크는 `WM_SETTEXT` 로 "고딕"을 넣어 필터링까지만 확인).
4. **고대비**: 새 컨트롤(견본 테두리·패널·칩)이 실제 고대비 테마에서 읽히는지(이 PC 는 고대비를 켜지 않아 캡처로는 확인 못 함).
5. 앱에서 `settings_ui`='native' 로 바꾼 뒤: 톱니·도구 모음 "설정"이 새 창을 여는지, 창이 이미 열려 있으면 앞으로 오는지, **톱니 오른쪽 클릭이 서랍을 여는지**, 서랍의 "설정 창 방식" 줄로 되돌리면 이전 동작인지(Flutter 시험은 가짜 API 로만).
6. 설치본: `resources/settings_win.exe` 가 번들·설치되는지(`build-windows-exe.yml` 은 실행하지 않음), 설치 중 창이 열려 있으면 종료되는지.
7. 앱이 켜진 채 새 창에서 저장한 뒤 앱이 바뀐 설정을 다시 읽는 시점(앱 쪽 재읽기 연결은 이번 범위 밖 — 저장 충돌 거부만 구현).

## 미확인·남은 문제
- 목록·단축키는 컨트롤만 있고 대응 키가 없다(위). 여러 줄 글은 구현하지 않았다.
- 글꼴 목록은 Windows 설치 글꼴만이다. 설계의 `hangul_fonts`(한/글 폴더 + Windows) 열거는 하지 않았다 — 한/글 전용 글꼴(한컴돋움 등)은 이 PC 목록에 없으면 알림만 뜨고 직접 입력은 된다.
- 폴더·글꼴 검증은 `docfit_config::validate_patch`(문자열 길이·NUL만)보다 엄격한 설정 창 쪽 규칙이다. Python 설정 서비스는 이 규칙을 모르므로 Python 이 같은 키를 다르게 허용할 수 있다.
- 작업 중 잠금(`JOB_RUNNING` 신호), 접근성(UIA), 항목 안내문 미리보기, 모두 기본값 단추는 이전 단계 그대로 미구현.
- 저장소 밖(작업 드라이브 루트)에 `tmp_fw.txt` 라는 `flutter build` 로그 파일이 하나 남았다(내 경로 실수; 삭제 명령이 권한 정책에 막혀 지우지 못함). 저장소에는 영향 없다.

## 실행 환경·git
이 PC 의 실제 설정 파일(`%APPDATA%\HwpAutoDocFit\settings.json`)은 읽지도 쓰지도 않았다(시험·캡처·스모크 모두 임시 폴더). `hwp-auto-docfit.py` 는 읽기만 했고 고치지 않았다. GitLab(`origin`)은 건드리지 않았고 **푸시하지 않았다**(로컬 커밋만).
