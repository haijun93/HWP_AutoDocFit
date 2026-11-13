# UI1 몽땅클립보드 관리창 Rust 네이티브 — Flutter와 1:1 기능 표 (2026-11-14)

브랜치 `feature/rust-ui1`. 관리창은 상주 프로그램 kagent 안의 Win32 창(`kagent/src/manage.rs` 그리기·입력, `manage_state.rs` 순수 상태 기계).
한 릴리스 동안 Flutter 화면(`flutter_ui/lib/features/kclip/kclip_dialog.dart`)과 함께 두고 앱 설정 `ui.kclip`(native·flutter, 기본 native)으로 고른다.

## 1. 여는 길

| 길 | Flutter(기존) | Rust(UI1) |
|---|---|---|
| 앱 왼쪽 '몽땅클립보드' 단추 | 앱 안 스티커 창 | `openKClipManager` → `kclip_window ui_get` → native면 `kclip manage_show`(파이프) → kagent `WM_MANAGE` |
| 앱 안 Ctrl+Shift+V | 단축키 화면(메모 칸) | 같은 길, `mode=hotkey` → 메모 칸에 포커스 |
| 트레이 메뉴 | 없음 | '관리창 열기' 추가 |
| 빠른 목록 ⚙ 설정 메뉴 | 없음 | '관리창 열기…' 추가(`settings_menu::Command::OpenManager`) |
| 에이전트가 꺼져 있을 때 | 켜기(동의) 화면 | `manage_show`가 `KCLIP_OFFLINE` → Flutter 화면으로 대체(켜기·동의는 Flutter가 계속 맡음) |
| 테마 | 앱 `theme_mode` | `manage_show`가 `theme`을 함께 보내 kagent 설정 `theme_mode`에 저장. 고대비는 OS 우선(`look.rs`) |

## 2. 기능 대응

| # | 기능 | Flutter | Rust 관리창 | 시험 |
|---|---|---|---|---|
| 1 | 목록(고정 먼저, 최근 순) | `list` | `manage_state::order`, 작업 스레드 `snapshot` | `open_loads_and_orders_pinned_first` |
| 2 | 찾기(입력 지연 후 검색, Esc 지움) | TextField + debounce | `LineEdit`(IME 조합 포함) + `TIMER_DEBOUNCE` | `search_is_debounced_and_escape_clears_it`, `keyboard_and_ime_drive_the_same_state_machine` |
| 3 | 분류 탭 7개(전체·그림·웹·파일·문서·캡처 등) | `kclip_category.dart` | 같은 `category.rs` 규칙, ←→로 이동 | `category_filters_visible_rows_and_resets_selection` |
| 4 | 마지막 분류 기억 | `docfit.kclip.category` | 창 상태 파일(`category`) | `window_buttons_toggle_and_open_resets_but_keeps_category_and_topmost` |
| 5 | 클립보드에 올리기(다시 복사) | `restore` | `Job::Restore`, 같은 완료 글 | `done_messages_match_flutter_and_memo_is_cleared_only_on_success` |
| 6 | 고정·고정 풀기 | `pin` | `Job::Pin`(행 📌 단추·메뉴) | 같음 |
| 7 | 항목 지우기 | `delete` | `Job::Delete`(행 메뉴·Delete 키) | `table_checks_are_asked_once_and_dropped_for_deleted_items` |
| 8 | 모두 지우기(고정 남김, 확인) | `clear` keep_pinned | `Job::Clear` + 확인(고정 개수 안내) | `clear_all_needs_confirmation_and_counts_pinned` |
| 9 | 미리보기(글·그림·파일, 실패 안내) | `kclip_preview.dart` | `Detail` 상태 + `thumb.rs`, '그림을 불러오지 못했습니다.' | `detail_flow_ignores_late_results_and_recopy_restores` |
| 10 | 웹 링크 열기 | `open_web` | `Job::OpenWeb`(http/https만) | `web_and_table_predicates_match_flutter` |
| 11 | 표 항목 메뉴: 음영 없이 복사 | `is_table`·`copy_no_shade` | `Job::CopyNoShade`(앱 서비스 위임), 표 항목만 메뉴 단추 | 같음 + UIA '표 항목 메뉴' |
| 12 | 엑셀 표를 한/글 표로 | `excel_table` | `Job::ExcelTable`(앱 서비스 위임) | `busy_rejects_new_jobs_until_done_then_reloads` |
| 13 | 메모 추가 | `add_text`(단축키 진입만) | `Job::AddMemo`, Enter(성공했을 때만 칸 비움) | `done_messages_match_flutter_…` |
| 14 | 끌어 놓기(파일·폴더·그림·웹 링크) | 앱 드롭 대상 `kclip` | `drop_target.rs`(OLE IDropTarget, HDROP·URL·단일 http 글) → `Job::AddFiles` | `drop_target::*` 3개 |
| 15 | 일시정지(30분·1시간·다시 켤 때까지)·다시 기록 | `pause`·`resume` | `Job::Pause`·`Resume`, 상태 줄 메뉴 | `paused_status_text_and_hotkey_text` |
| 16 | 몽땅클립보드 끄기 | `disable` | `Job::Quit` | 상태 기계 |
| 17 | 설정: 보관 개수·전체 최대·항목 최대·보관 기간 | `settings_set` 드롭다운 | 설정 화면 콤보 4개(같은 키·단위) | `settings_rows_map_to_the_same_settings_set_keys` |
| 18 | 설정: 스티커·항상 위·고르면 바로 붙여넣기·엑셀 목표 결과 | 체크 | 체크 4개 | 같음 |
| 19 | Windows 로그인 시 자동 실행 | `autostart` | `Job::Autostart` | 같음 |
| 20 | 화면캡처 폴더 자동 인식(전체·폴더별) | `capture_dirs` | 체크(전체) + 폴더별 체크, 없으면 '찾은 화면캡처 폴더가 없습니다.' | 같음 |
| 21 | 상태 줄(개수·단축키·일시정지 남은 시간) | status | `status_text`·`hotkey_text` | `paused_status_text_and_hotkey_text` |
| 22 | 창: 항상 위·접기·최대화·닫기, 위치·크기 저장 | `kclip_window` | 머리글 단추 4 + 창 상태 파일 | `window_buttons_…`, UIA '머리글 단추 4' |
| 23 | 작업 중 다른 작업 막기·늦게 온 결과 무시 | 일부 | `busy`·`seq` | `busy_rejects_…`, `stale_or_overlapping_loads_never_overwrite_newer_state` |
| 24 | 키보드(↑↓·←→·Tab·Enter·Esc·글자 치면 찾기로) | 일부 | 전부 | `keyboard_and_ime_drive_the_same_state_machine` |
| 25 | 다크·고대비 | 앱 테마 | `look.rs`(라이트·다크·고대비, OS 변경 감지) | 세 테마 해시 |
| 26 | 화면 읽기(UIA) | Flutter Semantics | ui-core `a11y`, 빈 이름 0 | `accessibility_tree_has_no_unnamed_elements_in_any_state` |

### 의도한 차이
- Flutter는 사이드바 진입에서 메모·끌어놓기를, 단축키 진입에서 설정을 숨긴다. 네이티브 창은 하나로 합쳐 두 진입 모두 목록·메모·끌어놓기·설정(별도 '설정' 화면)을 쓰고, 단축키 진입은 메모 칸에 포커스만 준다.
- 켜기(동의)는 에이전트가 꺼져 있을 때만 필요하므로 Flutter 화면에 남긴다(네이티브 창은 에이전트 안에 있어 꺼진 상태로는 열 수 없다).

## 3. 빠른 목록(popup) — 다크·고대비와 스크롤 막대
- 색을 `look::clip_palette()`로 바꿔 라이트·다크·고대비 연결, OS 변경(`WM_SETTINGCHANGE`·`WM_SYSCOLORCHANGE`·`WM_THEMECHANGED`) 때 빠른 목록·관리창 둘 다 다시 그림.
- 항목이 보이는 줄보다 많으면 오른쪽에 8px 스크롤 막대(손잡이 끌기·위아래 빈 곳 한 쪽씩, UIA ScrollBar '목록 스크롤').
- UI0 기준 그리기 해시(`golden_render_matches_before_ui0`, `--ignored`): 10개 중 9개 그대로. `collapsed`(440×122, 6항목)만 스크롤 막대가 새로 생겨 달라졌고, 막대를 끄면 UI0 값(0xdd002344b5fa7c88)과 같음을 확인한 뒤 기준값을 0x70585bae23d19908로 갱신.

## 4. 세 테마 그리기 해시(이 PC 96 DPI·ClearType)

| 화면 | 크기 | 라이트 | 다크 | 고대비 |
|---|---|---|---|---|
| manage list | 600×720 | 30108d52b8eb3b7c | 95c942ff8d7bb067 | 9c76bab7c351573d |
| manage hover_scrolled | 600×720 | 32acfbfd1bd544f7 | bb01e5b7252aec55 | 751df21c4665b216 |
| manage search_memo | 600×720 | ca1edd2bd3dd7dc4 | 0627708c674f8a68 | 2420121a51c79153 |
| manage narrow_category_empty | 480×560 | e76842f13b5ad94d | 9285da97388532d3 | d1a9aa6942739e34 |
| manage settings | 600×720 | 322b8dad257a56f6 | 423b131fabfd73ab | de3ddeb2df9e5783 |
| manage busy_message_paused | 600×720 | 6a34874cbfd07dcc | ac220ed7e21b0554 | df3977a63dbae6e3 |
| manage detail_image_failed | 600×720 | 03527b9ea0563145 | 397d0572b15437ef | 129a0bc63cf71d95 |
| manage collapsed | 600×34 | 3db1dc713f3fdb42 | 0be8d74513c24143 | 33200d070db0d8a7 |
| popup few | — | 3f187fc7746aa493 | 9506f4fc70d64e12 | 8d55742d83190098 |
| popup scroll | — | 5245ae698d387272 | 47c8c6a7e9d619fe | 5235ba68621ffcb2 |

같은 상태를 두 번 그리면 같은 해시, 상태마다 다른 해시, 라이트≠다크(`renders_are_deterministic_and_three_themes_differ`).

## 5. 측정 1회(릴리스, 2026-11-14 이 PC)

| 항목 | UI0(1df6677f) | UI1 |
|---|---|---|
| kagent.exe 크기 | 2,473,472 B | 2,629,632 B (+156,160, +6.3%) |
| 쉴 때 개인 메모리(창 숨김, 격리 폴더, 3초 뒤, 2회) | 2,312·2,256 KB | 2,308·2,272 KB |
| 관리창 연 뒤 개인 메모리(2회) | — | 3,368·3,360 KB |
| 관리창 표시(show→목록 받아 첫 그리기, 200항목, 30회) | — | 첫 504.2 ms · 중앙 71.2 ms · p90 88.4 ms |
| 빠른 목록 표시(30회) | 첫 55.2 · 중앙 37.7 · p90 55.2 ms | 첫 60.4 · 중앙 21.1 · p90 25.7 ms |

- 측정 도구: `manage::tests::measure_show_ms`·`popup::tests::measure_show_ms`(`cargo test --release -- --ignored`), 메모리는 `KAGENT_TEST_ROOT` 격리 실행 exe의 `PrivateMemorySize64`, 관리창은 파이프 `manage_show`로 열었다.
- 관리창 첫 표시 504ms는 창 클래스 등록·글꼴 생성·DB 첫 읽기(작업 스레드)를 포함한 값. 두 번째부터 70ms대.

## 6. 시험
- kagent `cargo test --bin kagent -- --test-threads=2`, ui-core `cargo test`, Python `tests/test_kclip_service.py`·`test_kclip_window_state.py` 등, Flutter `kclip_dialog_test.dart`(ui.kclip 선택 3개 추가)·`docfit_app_integrated_shell_test.dart`, `flutter analyze`.
