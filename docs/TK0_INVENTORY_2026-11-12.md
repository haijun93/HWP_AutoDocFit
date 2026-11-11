# TK 제거 0단계 — `_tk()` 호출 조사표 (2026-11-12)

> 근거 문서: `docs/TK_REMOVAL_REVIEW_2026-11-09.md` 4절 "0 즉시 완화". 이 문서는 **0단계만** 다룬다.
> 조사 기준 커밋 `ca79fb63`(브랜치 `feature/tk-removal-step0` 시작점)의 `desktop_web_ui.py`. 줄 번호는 그 커밋 기준이다.
> 코드 읽기와 시험(가짜 GUI)으로 판단했다. **Windows 실기(WebView2 + 숨은 Tk)와 한/글 COM은 실행하지 않았다.**

## 1. 집계

`ast`로 센 `_tk(` 호출은 **96곳**(메서드 정의·주석 제외. 검토 문서의 91곳에서 그 뒤 5곳 늘었다).

| 분류 | 뜻 | 곳 |
|---|---|---|
| ① | Tk 위젯·변수가 필요 없다(설정 읽기·상태 조회·순수 계산·브리지 자체 상태) | 31 |
| ② | Tk 위젯·Tk 변수·Tk 창(대화상자 포함)을 실제로 쓴다 | 56 |
| ③ | 판단 불가(Tk 스레드의 암묵적 직렬화에 기대는지 코드만으로 확정할 수 없음) | 9 |

①의 처리(0단계):

| 처리 | 곳 | 내용 |
|---|---|---|
| **직접 처리로 변경** | 18 | 읽기 전용(또는 값이 항상 같은 무효화)이고 잠금·원자성 근거가 있는 것 |
| 보류(쓰기) | 9 | Tk는 안 쓰지만 상태를 바꾼다 → 0단계 범위 밖(1단계 "서비스 분리"에서 잠금과 함께) |
| 보류(기존 시험 고정) | 1 | `flow_editor`의 설정 저장소 읽기: 기존 시험이 Tk 왕복 횟수 2를 고정한다 |
| 보류(무거운 서비스 읽기) | 3 | 읽기지만 파일 분석을 포함해 1단계에서 전용 스레드로 옮길 대상 |

변경 뒤 `_tk(` 호출 수: 96 → **78**. `_cancel_before_edit`가 감싸는 편집 메서드 28개는 호출마다 Tk 왕복이 2번 → 1번(이미 Tk를 안 쓰는 경로는 1번 → 0번)이 된다.

## 2. 스레드 안전성 근거(직접 처리로 바꾼 것)

| 대상 | 근거 |
|---|---|
| 설정 저장소 `gui._준말_서비스().settings`(`SettingsService`) | `read`/`update`가 경로별 `RLock`(`settings_service._PATH_LOCKS`)과 프로세스 잠금으로 직렬화되고, 쓰기는 임시 파일 + `os.replace`라 읽는 쪽이 반쯤 쓴 파일을 보지 않는다. Tk 객체를 갖지 않는다. 이미 `_tk_fallback_enabled`·`set_sidebar_collapsed`·`_developer_mode_enabled`가 Tk 밖에서 같은 객체를 쓴다 |
| `ConfigurationService.snapshot` | 상태가 없고 위 `read()` 한 번의 결과만 가공한다. `_Flutter설정_반영`은 설정 파일을 다시 쓰지 않으므로(코드 주석 "Do not rewrite all settings") 저장과 겹쳐도 저장 전·후 어느 한쪽의 일관된 값과 그 revision을 본다 |
| `zero_click` 무효화(`_cancel_pending_drop`) | `AppState.zero_click`에 항상 같은 값 `{}`를 넣는 속성 대입 한 번이다(다른 값을 넣는 코드가 없다). 순서와 무관하게 결과가 같다 |
| `gui.running`, `gui._job_id` | 단일 속성 읽기(원자적). 읽은 뒤의 실제 변경은 그대로 Tk 스레드에서 다시 검사한다(`작업중단`·`begin_conversion`이 `running`을 재확인) |
| `list(gui.files)` | 파이썬 리스트 한 번 복사(인터프리터 잠금 안에서 원자적). 목록을 바꾸는 쪽은 Tk 스레드뿐이고, 시작 자체는 뒤이은 Tk 호출이 `running`·파일 순서를 다시 검사한다 |
| `gui._flutter_dialog` | 사전을 다 만든 뒤 속성에 넣는다(`legacy.py` `_알림`). 읽는 쪽은 참조를 한 번 잡아 고정된 키 목록만 복사하므로 순회 중 크기 변경 오류가 없다. `_close_request`는 원래도 `_close_lock`으로 읽는다 |

## 3. 호출별 분류표

조치: **변경** = Tk 왕복 제거, **대기** = 3절(시작 대기 제한) 적용, 유지 = 그대로.

| 줄 | 함수 | 분류 | 근거 | 조치 |
|---|---|---|---|---|
| 205 | `cancel_zero_click` | ① | `_cancel_pending_drop`만 실행(상수 대입) | **변경** |
| 210 | `start_zero_click` | ① | 위와 같음 | **변경** |
| 245 | `quick_register_and_activate_profile` | ② | `_서식_분석_시작`(Tk 화면 갱신·`after`) | 유지 |
| 460 | `check_update` | ③ | `_업데이트_서비스()`가 `hasattr`로 처음 만들 때 Tk 스레드와 겹치면 서비스가 둘 생길 수 있음 | 유지 |
| 474 | `start_update` | ③ | 위와 같음 + `running` 검사와 시작을 한 번에 직렬화 | 유지 |
| 477 | `open_update_page` | ③ | 460과 같음 | 유지 |
| 582 | `get_state` | ② | `selected_mode`·`status_var`·범위·옵션 등 Tk 변수 `.get()` 다수 (호출 빈도 최고, 3단계 대상) | 유지 |
| 820 | `add_paths` | ② | `status_var.set`, `파일추가`(목록 위젯) | 유지 |
| 835 | `remove_file` | ② | `file_list` 위젯 선택·삭제 | 유지 |
| 839 | `clear_files` | ② | `목록지우기`(위젯) | 유지 |
| 840 | `clear_files` | ② | `_update_document_hints`가 `selected_mode`·`status_var` 설정 | 유지 |
| 873 | `set_mode` | ② | `selected_mode.set`, `_모드_선택됨` | 유지 |
| 883 | `set_conversion_target` | ① 쓰기 | 브리지 값 `conversion_target`만 바꾸지만 `running` 검사와 묶인 쓰기 | 유지(보류) |
| 913 | `start_single_file_conversion` | ② | 작업 시작(`status_var`·단추·로그 위젯) | 유지 |
| 1009 | `set_include_spacing` | ② | Tk 변수 `.set` | 유지 |
| 1030 | `set_reset_spacing` | ② | Tk 변수 `.set` | 유지 |
| 1053 | `set_table_spacing` | ② | Tk 변수 `.set` | 유지 |
| 1075 | `set_exclude_tables` | ② | Tk 변수 `.set` | 유지 |
| 1125 | `set_card_option` | ② | Tk 변수 `.set` | 유지 |
| 1139 | `set_range` | ② | 범위 Tk 변수 `.set` | 유지 |
| 1182 | `cancel_job` | ① | `gui._job_id` 속성 읽기 | **변경** |
| 1183 | `cancel_job` | ② | `작업중단`(단추·`status_var`·로그) | 유지 |
| 1216 | `_start_legacy` | ① | `list(gui.files)` 복사 | **변경** |
| 1295 | `_start_legacy` | ② | 변환 시작(Tk 변수·위젯) | 유지 |
| 1333 | `_start_legacy` | ② | `작업시작`(Tk 변수·위젯) | 유지 |
| 1342 | `stop` | ② | `작업중단` | 유지 |
| 1362 | `get_settings` | ① | 설정 스냅숏 + 레지스트리 읽기 + `running` 읽기 | **변경** |
| 1383 | `save_settings` | ② | `_Flutter설정_반영`이 Tk 변수 `.set` | 유지 |
| 1424 | `open_settings` | ② | Tk 설정 창(폴백), 무기한 대기 | 대기 |
| 1430 | `text_input` | ② | Tk 창(폴백), 무기한 대기 | 대기 |
| 1436 | `open_stages` | ② | Tk 창(폴백), 무기한 대기 | 대기 |
| 1444 | `open_log` | ② | Tk 로그 창 | 유지 |
| 1453 | `get_log` | ② | `log_text` 위젯 읽기 | 유지 |
| 1463 | `clear_log` | ② | `log_text` 위젯 지우기 | 유지 |
| 1476 | `get_pending_dialog` | ① | `_flutter_dialog` 사전 읽기(작업 중 1초마다 조회) | **변경** |
| 1548 | `resolve_dialog`(종료 승인) | ② | `gui.종료`·`root.winfo_exists`, 무기한 대기 | 유지(종료 경로) |
| 1566 | `resolve_dialog` | ② | `pending["done"]`이 `tk.BooleanVar` | 유지 |
| 1580 | `flow_editor` | ① | 설정 저장소 읽기. 단 `test_clipboard_hwp_conversion`이 `copy_result`의 Tk 왕복 횟수 2를 고정 | 유지(보류) |
| 1584 | `flow_editor`(`copy_result`) | ③ | 클립보드를 창 소유 스레드에서 쓰도록 한 의도("on owner thread" 시험) | 유지 |
| 1635 | `flow_editor`(`export_hwpx`) | ① 읽기·무거움 | `TableTemplateService.snapshot`(서비스 읽기) → 1단계 | 유지(보류) |
| 1684 | `document_tool` | ① | 설정 폴더 경로 읽기 | **변경** |
| 1686 | `document_tool`(`add_text`) | ② | `파일추가`(목록 위젯) | 유지 |
| 1707 | `stage_config` | ② | `selected_mode.get`, `_세부작업_옵션_맞춤` | 유지 |
| 1786 | `format_profile` | ② | `_프로파일_목록갱신`·`_프로파일_선택`(콤보 위젯). `get`만은 읽기지만 한 콜백에 섞여 있음 | 유지 |
| 1917 | `table_template` | ① 쓰기·무거움 | Tk는 안 쓰지만 서비스 전체가 쓰기·HWPX 분석 → 1단계 | 유지(보류) |
| 1961 | `hwp_bulk`(결과표 열기) | ② | 실패 시 `status_var.set`·Tk 알림창 | 유지 |
| 1967 | `hwp_bulk`(`start`) | ① | `gui.running` 속성 읽기 | **변경** |
| 2011 | `_file_rename_service` | ① | 설정 저장소 읽기 | **변경** |
| 2020 | `_timemachine_service` | ① | 설정 폴더 경로 읽기 | **변경** |
| 2050 | `_timemachine_send_to_feature` | ② | `selected_mode`·`status_var` | 유지 |
| 2088 | `kclip_window` | ① | 설정 저장소 읽기 | **변경** |
| 2102 | `external_app` | ① | 설정 저장소 읽기 | **변경** |
| 2158 | `new_document` | ③ | `gui._서식예시_경로`가 Tk를 쓰는지 확정 못 함, `blank`는 한/글 COM 생성 | 유지 |
| 2193 | `sidebar_mode` | ① 쓰기 | Win32 창 이동 + 설정 저장, 컨트롤러 생성 | 유지(보류) |
| 2213 | `sidebar_mode_switch_side` | ① 쓰기 | 위와 같음 | 유지(보류) |
| 2221 | `sidebar_mode_drag` | ① 쓰기 | Win32 창 이동(끄는 동안 호출 잦음). 컨트롤러에 잠금이 없어 직렬화가 필요 | 유지(보류) |
| 2234 | `always_on_top` | ① 쓰기 | 설정 저장 + 창 상태 | 유지(보류) |
| 2291 | `table_sketch` | ① | 설정 폴더 경로 읽기 | **변경** |
| 2635 | `_table_units_command` | ① | 설정 저장소 읽기 | **변경** |
| 2644 | `_table_units_command` | ① | 설정 저장소 읽기(한/글 COM 구간 안에서 Tk를 기다리던 곳) | **변경** |
| 2653 | `get_abbreviations` | ① 읽기·무거움 | 준말 HWPX 문서를 매번 읽음. 같은 문서를 쓰는 저장 명령과의 직렬화가 필요 → 1단계 | 유지(보류) |
| 2656 | `save_abbreviation` | ① 쓰기 | 서비스 쓰기 | 유지(보류) |
| 2660 | `delete_abbreviation` | ① 쓰기 | 서비스 쓰기 | 유지(보류) |
| 2663 | `set_abbreviation_defaults` | ① 쓰기 | 서비스 쓰기 | 유지(보류) |
| 2667 | `abbreviation_document` | ① 읽기·무거움 | 문서 만들기·열기·다시 읽기(쓰기 포함) → 1단계 | 유지(보류) |
| 2671 | `open_results` | ② | `_결과파일_열기`(Tk 알림창), 무기한 대기 | 대기 |
| 2689 | `open_result` | ② | `status_var.set`·Tk 알림창, 무기한 대기 | 대기 |
| 2714 | `open_source_file` | ② | 위와 같음(검토 문서는 "gui 미사용"으로 봤으나 현재 코드는 `status_var`·알림창 사용) | 대기 |
| 2719 | `show_source_file` | ② | 위와 같음 | 대기 |
| 2729 | `open_result_source` | ② | 위와 같음 | 대기 |
| 2743 | `show_result` | ② | 위와 같음 | 대기 |
| 2751 | `open_diff`(폴백) | ② | Tk 비교 창, 무기한 대기 | 대기 |
| 2799 | `set_stage` | ② | `selected_mode.get`, `_요약갱신` | 유지 |
| 2815 | `reset_stages` | ② | 위와 같음 | 유지 |
| 2824 | `set_stage_default` | ② | `selected_mode.get` | 유지 |
| 2844 | `set_profile` | ② | 콤보 위젯 | 유지 |
| 2877 | `_apply_drop_target` | ③ | `_서식예시_자동감지`가 Tk를 쓰는지 확정 못 함 | 유지 |
| 2917 | `add_format_paths` | ② | `status_var.set`, `_서식_분석_시작` | 유지 |
| 3004 | `write_format_example` | ② | Tk 창 + 한/글, 무기한 대기 | 대기 |
| 3015 | `rename_profile` | ③ | `_서식_이름_저장`이 목록 위젯을 갱신하는지 확정 못 함 | 유지 |
| 3019 | `delete_profile` | ③ | 위와 같음 | 유지 |
| 3023 | `edit_profile` | ② | Tk 창, 무기한 대기 | 대기 |
| 3028 | `show_format_example` | ② | 작업 시작(Tk 변수·위젯) | 유지 |
| 3046 | `format_gallery` | ③ | `_썸네일_만들기_시작`이 `after`를 쓰는지 확정 못 함 | 유지 |
| 3052 | `open_format_samples` | ② | 실패 시 Tk 알림창 | 유지 |
| 3066 | `set_option` | ② | Tk 변수 `.set` | 유지 |
| 3083 | `next_job` | ② | 목록 위젯·Tk 변수 | 유지 |
| 3093 | `run_tool` | ② | Tk 창(폴백), 무기한 대기 | 대기 |
| 3130 | `request_close` | ② | `root.after`·`gui.종료` | 유지 |
| 3150 | `request_close` | ② | `gui.종료`·`root.winfo_exists`, 무기한 대기 | 유지(종료 경로) |
| 3163 | `_cancel_before_edit` | ① | `_cancel_pending_drop`만 실행. 편집 메서드 28개에 붙어 호출마다 왕복 1번 추가 | **변경** |
| 3242 | `_initial_window_settings` | ① | 설정 스냅숏 읽기(시작 때 1번) | **변경** |
| 3539 | `_bind_file_drop` | ② | `status_var.set` | 유지 |
| 3708 | `run_webview` | ② | `winfo_screenwidth` | 유지 |
| 3712 | `run_webview` | ② | `winfo_screenwidth` | 유지 |
| 3802 | `run_webview`(`initialize_webview`) | ① | 설정 스냅숏 읽기(시작 때 1번, 5초 제한) | **변경** |

## 4. 무기한 대기(`timeout=None`) 15곳

`_tk`는 요청을 큐에 넣고 결과를 기다린다. 대기는 두 구간이다: **(가) 콜백이 시작되기까지**(Tk 스레드가 다른 일로 바쁜 시간) **(나) 콜백이 끝나기까지**(Tk 창을 사용자가 닫을 때까지 등).

- (나)는 줄이지 않는다. 이미 시작된 쓰기·창을 도중에 버리면 의미가 바뀐다.
- (가)만 제한한다(`_TK_START_TIMEOUT` 5초 — 검토 문서의 "예: 5초"). 5초 안에 시작하지 못하면 **요청을 취소해 나중에도 실행되지 않게** 하고 `TkBusyError`("앱이 다른 작업을 처리하고 있어 요청을 시작하지 못했습니다…")를 낸다. 실행되거나, 실행되지 않고 분명한 오류가 나거나 둘 중 하나라 쓰기의 의미는 같다.
- 적용 13곳: 표의 "대기". 제외 2곳: 종료 경로(1548·3150). 종료는 자체 감시(`_close_unanswered`·`_force_close`)가 있고, 닫기 이벤트에서 오류를 내면 창이 닫히지 않을 수 있어 그대로 둔다.
- 기본 120초 대기(나머지 호출)는 이번에 바꾸지 않았다. 작업 지시가 "무한정 기다리는 곳"으로 한정했고, 1초 주기 상태 조회가 5초 뒤 오류로 바뀌면 화면 쪽 오류 처리를 함께 봐야 한다 → 계측 결과를 본 뒤 정한다.

## 5. 오류추적 계측

오류추적(`freeze_tracing`)이 켜져 있을 때만 `_tk` 왕복마다 **호출 이름(브리지 메서드:콜백)·큐 대기 시간·실행 시간·전체 시간·결과**를 `FreezeTracer.record_tk`로 남긴다.

- 멈춤 스냅숏(`freeze-*.json`)에 `recent_tk_roundtrips`(최근 50건)와 `tk_roundtrip_stats`(이름별 횟수·합계·최대)를 넣는다.
- 오류추적을 끄거나 앱을 끝낼 때 기록이 있으면 `tk-roundtrips-*.json`(이름별 통계, 전체 시간 큰 순)으로 한 번 저장한다(최근 20개 유지). 멈춤이 없어도 어떤 호출이 Tk 왕복을 많이 쓰는지 볼 수 있다.
- 예전에는 호출 이름이 콜백 함수 이름(`<lambda>`·`update`·`execute`)이라 어느 화면 명령인지 알 수 없었다.
- 함께 고친 것: 시간 초과·종료로 **실행되지 않은** 요청이 `tk_pending`에 영원히 남아 그 뒤로 계속 "멈춤"으로 판정되던 문제.

## 6. 남은 일(1단계 이후로 넘김)

- 보류한 ① 13곳(쓰기 9·무거운 읽기 3·시험 고정 1)과 ③ 9곳: 서비스별 잠금·전용 스레드와 함께 옮긴다.
- `get_state`(②, 호출 빈도 최고): Tk 변수를 읽으므로 상태 객체 이전(3단계) 전에는 Tk 왕복이 남는다.
- Windows 실기에서 오류추적을 켜고 `tk-roundtrips-*.json`으로 실제 지연 지점을 확인한다.

## 7. 적용 결과(2026-11-12)

| 항목 | 값 |
|---|---|
| `_tk(` 호출 | 96 → **78**(18곳 제거: 무효화 3, 상태 읽기 5, 설정 저장소 읽기 8, 시작 설정 읽기 2) |
| 무기한 대기 15곳 | 13곳 시작 대기 5초 제한, 2곳(종료 경로) 그대로 |
| 시험 | desktop·web·settings·freeze 관련 52개 모듈 **540개 통과**(건너뜀 2, 실패 0). 변경 전 같은 범위 501개 통과 + 새 시험 39개(`test_tk0_direct_reads` 19, `test_tk0_tracing` 11, `test_tk0_start_timeout` 9) |
| 기존 시험 | 수정 없음(결과 변화 없음) |

확인하지 못한 것:
- Windows 실기(WebView2 + 숨은 Tk 펌프)에서의 체감 지연·프리징 변화. 가짜 GUI·실제 Tk 시험만 돌렸다.
- `TkBusyError`가 났을 때 Flutter 화면이 보여 주는 문구(브리지 예외가 화면에 어떻게 표시되는지).
- 한/글 COM 경로(`_table_units_command`의 `copy`·`thumbnails`)는 실행하지 않았다(설정 저장소 읽기만 바꿨고 COM 호출 순서는 그대로).
