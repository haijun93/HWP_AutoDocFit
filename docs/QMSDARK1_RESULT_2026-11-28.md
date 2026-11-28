# QMSDARK1 — 퀵몽땅검색 다크 테마 적용 (2026-11-28)

## 원인 (캡처 모드로 재현)
- 창은 시작할 때 옛 값 `theme`(Ctrl+L, 기본 "dark")만 읽었다. 설정 창의 '테마'는 별개 값 `ui_theme`에 저장돼 시작에도, 누른 즉시에도 반영되지 않았다.
- `{"theme":"light","ui_theme":"dark"}`에서 흰 창을 재현했다(`captures/QMSDARK1/before_ui-theme-dark_old-light_96dpi.png`, 재현 PC의 Windows 앱 모드는 어두움).
- 앱 본체의 `theme_mode`는 ktm에 전달되지 않았다.
- Windows 테마 감지(`ui_theme::refresh`) 자체는 정상이었다. 실행 중 테마 알림은 받지 않았다.
- 별도 결함: Ctrl+L이 메모리의 설정을 갱신하지 않아, 다음 검색어 저장 때 이전 값으로 덮일 수 있었다(코드 확인).

## 바뀐 규칙
앱이 인자를 넘기지 않는다(ktm은 독립 상주·자동 실행). ktm이 앱 설정 파일 `%APPDATA%\HwpAutoDocFit\settings.json`의 `theme_mode`를 **읽기만** 한다.

| 순서 | 값 | 결과 |
|---|---|---|
| 0 | Windows 고대비 켜짐 | 시스템 색 |
| 1 | 퀵검색 `ui_theme` = light·dark | 그 값 |
| 2 | 앱 `theme_mode` = light·dark | 그 값 |
| 3 | 그 밖(system·파일 없음) | Windows 앱 모드 |

- 설정 창의 '시스템' 표시는 실제 동작에 맞춰 **'앱 설정 따름'**으로 바꿨다.
- 옛 `theme`: `ui_theme` 키가 없는 파일의 "light"만 옮긴다. 키가 있는 파일에서는 무시한다. 필드는 남겨 두되 결정에는 쓰지 않는다.
- `--theme` 인자는 고정값으로 남겼다. 사용자가 창에서 테마를 바꾸면 풀린다.
- `--app-settings <경로>`를 추가했다. 캡처 모드·`KTM_TEST_ROOT`에서는 이 인자가 없으면 자료 폴더의 `app-settings.json`만 읽는다.

## 상태와 전이
상태는 `ui_theme::CURRENT` 한 곳이고, ktm의 전이 입구는 `apply_current_theme` 하나다.
시작, 창 열기(`Effect::Show`), 트레이 메뉴 열기, 설정 창 테마 클릭, Ctrl+L, 테마 알림(`WM_SETTINGCHANGE`·`WM_SYSCOLORCHANGE`·`WM_THEMECHANGED`·`DocFitThemeChanged`)에서 부른다.
앱에서 테마만 바꾸면 알림이 없으므로 **퀵검색을 다시 열 때** 반영된다.

## 색
- 검색어 강조의 고정 노랑·검정 글자를 테마별 색(`ktm::theme::term_mark`)으로 바꿨다. 밝게는 이전과 같은 색이다.
- 고대비에서 묶음 강조색이 파스텔이던 것을 시스템 선택 색으로 바꿨다(`ui_theme`).
- 메뉴는 기존 `ui_theme::refresh`의 메뉴 모드를 따른다.

## 시험
Rust `stable-x86_64-pc-windows-gnu` + 휴대용 MinGW, `CARGO_TARGET_DIR`은 저장소 밖.

| 대상 | 수정 전 | 수정 후 |
|---|---|---|
| `cargo test` ktm | 299 통과, 0 실패, 1 제외 | 306 통과, 0 실패, 1 제외 |
| `cargo test` ui_theme | 13 통과 | 13 통과(고대비 묶음색 단언 추가) |
| Python 11개 모듈 | 실행 안 함 | Ran 103, OK(skipped=1) |

- 새 `ktm/tests/theme_capture.rs` 2개는 수정 전 둘 다 실패했다(흰 바탕, 고정 노랑 943화소). 실제 `ktm.exe`를 캡처 모드로 실행해 화소를 본다. 고대비 PC에서는 건너뛴다.
- Python 모듈: ktm_service, ktm_status_polling, unified_search_host, sidebar_launch, native_tasks, distribution_build, theme_mode, data_path_guard, settings_window_service, search_engine_settings, app_settings_writer. Python 코드는 바꾸지 않았고 전체 시험은 돌리지 않았다.
- 기존 시험은 지우거나 완화하지 않았다.

## 캡처 (`docs/captures/QMSDARK1/`)
라이트·다크 × 96·144 DPI(150%)로 26장면씩 실행해 보고서 FAIL 0. 앱 설정 파일은 실제 `SettingsService`로 만든 합성 파일을 썼다.
96 DPI는 라이트·다크 모아보기로 열어 확인했고, 150%는 다크의 설정·미리보기와 라이트의 미리보기를 열어 확인했다. 저장소에는 대표 장면(미리보기·설정·옵션·최근 검색·결과 내 검색)과 수정 전 1장을 남겼다.

## 확인하지 않은 것·남은 일
- 설치본·실행 중인 앱은 건드리지 않았다. 핫키로 열기, 트레이·내보내기·형식·기간 메뉴의 어두운 모양, 실제 Windows 모드 전환, 고대비 화면은 **실기 확인이 필요하다**.
- 150% 캡처는 DPI 강제값이며 실제 150% 모니터가 아니다. 150%의 나머지 장면 이미지는 눈으로 확인하지 않았다(배치 자동 검사만 통과).
- 폴더 선택 창(`SHBrowseForFolderW`)과 시작 실패 `MessageBox`는 Windows 기본 모양 그대로다.
- kagent는 자체 `theme` 설정을 쓰고 앱 `theme_mode`를 따르지 않는다. kvf·kagent와의 규칙 통일은 하지 않았다.

## 사용자가 알아야 할 동작 변화
- 아무것도 고르지 않은 사용자는 이전에는 항상 어두운 창이었지만, 이제 앱·Windows 설정이 밝으면 밝게 뜬다.
- 최근 버전에서 Ctrl+L로만 고른 "light"/"dark"는 이어지지 않는다(설정 창에는 '시스템'으로 보이던 상태). 다시 Ctrl+L이나 설정 창에서 고르면 된다.
