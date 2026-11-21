# DOCVIEWWIN2 결과 (2026-11-22)

카드: 문서뷰 Win32 창 앱 연결(설정 `docview_ui`) + 번들 + 제외 폴더·설정 읽기 + 실제 klib 경로 검증.
브랜치 `feature/DOCVIEWWIN2`, 커밋만(푸시 없음). `hwp-auto-docfit.py`·GitLab(`origin`)·이 PC 의 실제 설정(`%APPDATA%\HwpAutoDocFit\settings.json`)·실행 중 앱은 건드리지 않았다(시험·캡처·스모크 모두 임시 폴더·임시 설정 파일).
기본값은 `docview_ui='flutter'` 라서 **사용자 눈에 보이는 변화는 없다**(문서뷰 탭 그대로).

## 연결 표 (`outliner_ui`·`settings_ui` 와 같은 방식)

| 위치 | 변경 |
|---|---|
| `docfit_app/settings_schema.py` | `DEFAULTS['docview_ui'] = 'flutter'` |
| `docfit_app/configuration_service.py` | `EXCLUDED` 에 `docview_ui`(설정 내보내기·설정 창 글 입력칸에서 제외) |
| `docs/settings_defaults.json`(골든) | `docview_ui` 추가 |
| `docfit_config/src/lib.rs` | 설정 창 숨김 목록에 `docview_ui`(내부 키) |
| `docfit_app/docview_window_service.py`(새) | `ui_mode`(읽기·쓰기, 'flutter'/'native' 외 거부)·`open_native`·`open_if_native`(native 일 때만 `docview_win.exe --settings <앱 설정 파일> [--klib <앱이 찾은 klib>]` 실행). 오류 코드 `DOCVIEW_WINDOW_MISSING`·`DOCVIEW_WINDOW_FAILED` |
| `desktop_web_ui.py` | 브리지 메서드 `docview_window(payload)` + `_BrowserApi` 허용 목록 |
| `docfit_app/application_service.py` | `dispatch` 개수표·분기에 `docview_window` |
| `flutter_ui/lib/features/doc_grid/docview_ui_gate.dart`(새) | `DocViewUiGate`: flutter 면 기존 `DocGridScreen` + "새 창(Win32)으로 열기" 단추, native 면 창을 열고 탭에는 안내("문서뷰는 별도 창(Win32)에서 열립니다.")·[창 다시 열기]·[앱 안에서 보기]. 어떤 실패도 기존 화면으로 |
| `flutter_ui/lib/features/workspace/workspace_adapters.dart` | 문서뷰 탭의 지연 어댑터가 `DocViewUiGate` 를 거침(`DocGridAdapter` 자체는 그대로) |
| `HWP_AutoDocFit.spec` | `resources/docview_win.exe` 가 있으면 번들 |
| `.github/workflows/build-windows-exe.yml` | 새 단계 "Build document view window"(`cargo test --release --locked` → `cargo build --release --locked --bin docview_win` → `resources/docview_win.exe` 복사 → 존재 확인) + VC++ 런타임 비의존 검사 목록(`$files`)에 추가. **`klib` 빌드 단계 뒤에 두고 `KLIB_EXE` 를 지정**한다(시험이 실제 klib 를 돌리므로; `outliner_win`·`settings_win` 과 달리 위치가 다른 이유) |
| `installer/setup.iss` | `StopHelper('docview_win.exe')`(설치 중 열린 창 종료) |
| `.gitignore` | `docview_win/target/` |
| `docview_win/Cargo.toml`·`Cargo.lock` | 의존성 `docfit_config`(경로)·`serde_json`·dev `tempfile` 추가. `Cargo.lock` 은 cargo 가 만든 그대로(`--locked` 통과) |
| `grid_core/src/scan.rs` | `find_klib` 가 `KLIB_EXE`(Python `find_klib_executable` 과 같은 이름)도 인식(사진뷰도 같은 규칙 공유) |

시험이 **숫자를 고정하지 않게** 했다: 골든·키 개수 비교는 스키마에서 계산하고, 배포 시험은 목록 끝 괄호(`'settings_win.exe')`)에 의존하던 한 줄을 `'settings_win.exe'` 포함 여부로 완화했다(새 파일이 목록 끝에 붙으면 깨지던 것).

## 창(`docview_win`) 변경

- **설정 읽기**(`settings::load`): `docfit_config::get` 으로 앱 설정을 읽어 `ui_thumb_sizes.doc_grid`(100~240, 기본 190)·`library_excluded_folders`·`docview_ui` 와 저장 때 쓸 revision 을 얻는다. 파일이 없으면 기본값(파일을 만들지 않음), 깨졌으면 기본값 + 상태줄 안내(파일 그대로).
- **제외 폴더를 `klib scan --exclude` 로 전달**(기존 `klib_args`) — 이제 창이 시작할 때 읽은 설정 값이다.
- **제외 폴더 편집 패널**(`panel.rs`, 도구줄 "제외 폴더 (n)" 단추): 목록·선택·[폴더 추가…]·[선택 삭제]·[닫기], 키보드(↑↓·Delete·Insert=추가·Esc=닫기)·휠 스크롤, 저장 결과·오류 안내 줄. 저장은 `docfit_config::set`(목록 검증·원자 저장·**revision 충돌 검사**) 한 길뿐. 정렬·중복 규칙은 Python `library_exclusions` 와 같다(대소문자 무시 정렬, 구분자·끝 `\`·대소문자 무시 중복). 충돌이면 **파일을 한 바이트도 바꾸지 않고** 최신 목록을 다시 불러온 뒤 "다른 곳에서 설정이 바뀌어 저장하지 않았습니다…" 를 보인다. 패널에서 바꾼 뒤 닫으면 새 제외 폴더로 **다시 검색**(도는 중이면 취소하고 끝나면 다시).
- **폴더 선택**: IFileDialog 가 아니라 `SHBrowseForFolderW`(새 대화상자 스타일) — `windows-sys` 에 COM 래퍼가 없어 `settings_win` 과 같은 방식. 사용자 눈에는 폴더 선택 창.
- **단일 인스턴스**: 같은 설정 파일로 이미 떠 있으면 새 프로세스는 곧 끝나고 기존 창을 앞으로(최소화면 복원). 설정 파일이 다르면 별개 창.
- 캡처 모드 확장: `--scan-root <폴더> --klib <exe> --settings <파일> [--panel] [--panel-add <폴더>]` — 합성 모델 대신 **실제 klib 스캔 결과**(제외 폴더는 설정에서)와 패널을 그린다.

### "기본 범위" 에 대해 (카드 지시와 다른 점)
앱에는 기본 검색 범위를 담은 설정 키가 **없다**(Python `current_default_roots` 는 고정 드라이브 + Desktop + OneDrive Desktop 을 환경에서 계산). 그래서 "설정에서 읽기" 대상은 썸네일 크기·제외 폴더 둘이고, 범위는 이전과 같이 **고정 드라이브 전체**(`fixed_drive_roots`)다. Python 은 Desktop·OneDrive Desktop 을 후보로 더 넣지만 모두 고정 드라이브 안이라 같은 파일을 두 번 훑는 중복일 뿐이라 Rust 쪽은 넣지 않았다(미확인: OneDrive 가 다른 드라이브·네트워크에 있는 환경). 범위를 바꾸는 설정 키가 필요하면 별도 카드.

## 실제 klib 경로 검증 (건너뛰지 않는다)

`docview_win/tests/real_klib_tests.rs`(5개). klib 는 `DOCFIT_KLIB` → `KLIB_EXE` → 실행 파일 옆·상위 `klib.exe`/`resources\klib.exe`/`klib\target\…` 순으로 찾고, **없으면 실패**한다(`KLIB_EXE` 없이 돌려 보면 `klib.exe 를 찾지 못했습니다…` 로 5개 모두 실패하는 것을 확인; 시험 시 skip 이 아님). 이 PC 에서는 다른 카드(THUMBRS2)가 빌드해 둔 `klib.exe` 를 스크래치 폴더에 복사해 `KLIB_EXE` 로 지정했다(이번에는 `dlltool.exe` 가 들어 있는 `%LOCALAPPDATA%\DocFitTools\mingw64\bin` 을 PATH 맨 앞에 두고 빌드가 됐다. 이전 카드가 "dlltool 없음"으로 klib 를 못 빌드한 원인은 확인하지 않았다).

| 시험 | 확인 |
|---|---|
| `real_klib_scans_a_temp_tree_with_the_document_extensions_only` | 시험이 만든 임시 트리(한글 이름·하위·깊은 폴더·`.txt`)를 실제 `klib scan` 으로 → 지원 확장자 6개만, `.txt` 제외, 실제 크기(7바이트)·수정시각·절대경로·실제 파일 |
| `real_klib_honors_excluded_folders_passed_with_exclude` | `--exclude <폴더>` 로 그 폴더와 아래 폴더 문서가 빠짐(4개). 대문자·슬래시 형태도 같은 폴더로 적용 |
| `real_klib_nonexistent_root_is_an_empty_successful_scan` | 없는 루트 = 오류 아님(0건, 종료 코드 0) — klib 의 실제 동작 확인 |
| `real_klib_error_exit_code_becomes_a_failed_scan` | 루트가 없는 인자 오류 → klib 종료 코드 1 → `Err("klib 종료 코드 …")`; 없는 실행 파일 → "klib 를 실행하지 못했습니다" |
| `real_klib_scan_can_be_cancelled_quickly_and_the_child_exits` | 첫 고정 드라이브 루트를 읽기 전용으로 훑다 300ms 뒤 취소 → 5초 안에 `Finished{cancelled:true}`, 실패 아님, 스레드 정리 |

## 시험 결과 (`--release --locked`, CI 와 같게. 환경: `CARGO_TARGET_DIR`·PATH 앞에 `%LOCALAPPDATA%/DocFitTools/mingw64/bin`·`KLIB_EXE`)

```
cargo test --release --locked --manifest-path docview_win/Cargo.toml
  src\lib.rs                test result: ok. 0 passed; 0 failed
  src\main.rs               test result: ok. 0 passed; 0 failed
  tests\capture_tests.rs    test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.38s
  tests\exclude_tests.rs    test result: ok. 11 passed; 0 failed; ... finished in 0.26s
  tests\pure_tests.rs       test result: ok. 31 passed; 0 failed; ... finished in 0.35s
  tests\real_klib_tests.rs  test result: ok. 5 passed; 0 failed; ... finished in 0.42s
  Doc-tests docview_win     test result: ok. 0 passed; 0 failed
cargo test --release --locked --manifest-path grid_core/Cargo.toml
  test result: ok. 0 passed / ok. 5 passed / ok. 0 passed  (lib·core_tests·doc-tests)
cargo test --locked --manifest-path docfit_config/Cargo.toml
  test result: ok. 8 passed; 0 failed (+ 0 passed ×2)
python scripts/settings_compare.py --exe <settings-core.exe>
  cases 6, equal True (차이 0)
python -m unittest tests.test_docview_window_service tests.test_settings_window_service tests.test_bridge_methods tests.test_distribution_build tests.test_outliner_service tests.test_theme_mode tests.test_core_app_state tests.test_kclip_window_state tests.test_search_engine_settings
  Ran 79 tests ... OK
python -m unittest tests.test_settings_compare tests.test_settings_list_keys tests.test_abbreviation_service
  Ran 19 tests ... OK (skipped=2)
flutter analyze → No issues found!
flutter test test/docview_ui_gate_test.dart → +7: All tests passed!
flutter test test/docview_ui_gate_test.dart test/doc_grid_screen_test.dart test/workspace_test.dart test/modal_dialogs_test.dart test/tab_open_events_test.dart test/tab_navigation_shell_test.dart test/workspace_adapters_test.dart → +63: All tests passed!
```

- `docview_win` 합계 **52개 통과 / 0 실패**(이전 37 → `pure_tests` 의 건너뛰던 klib 시험 1개를 `real_klib_tests` 5개로 대체, `exclude_tests` 11개 추가).
- `exclude_tests`(11): 설정 읽기(썸네일·제외 폴더·ui)·없는/깨진 파일은 기본값이고 파일을 만들거나 바꾸지 않음·`docview_ui` 기본값이 앱 기본값(docfit_config)에 있고 설정 창에 안 보임·폴더 키 규칙(Python 과 같음)·**추가·삭제가 정렬 목록을 저장하고 모르는 키·다른 설정을 보존**·설정 파일이 없을 때 생성·**앱이 그 사이 쓴 파일은 거부(파일 불변, 최신 목록 재로드, 재시도 시 앱이 쓴 값 보존)**·저장 오류 종류(충돌/검증)·30개 목록에서 키보드 선택이 보이는 범위 유지·**패널 배치가 모든 창 크기(820×480~1920×1080)×DPI(96·120·144·192)에서 창·상자 안, 서로 겹침 없음**·히트 테스트.
- CI 주의 반영: 시험 실행 파일 위치는 `CARGO_BIN_EXE` 기준(기존 `capture_tests` 유지), 파일 비교 시험은 새로 만들지 않아 CRLF 정규화 필요 없음(Python 배포 시험은 `read_text` 로 부분 문자열만 비교). 새 workflow 단계 순서를 `assertLess(klib 단계, docview 단계)` 로 시험이 고정한다.
- 전체 시험은 돌리지 않았다(지시).

## 캡처 (실제 `paint::draw_with_panel`, 실제 `klib scan` 결과, PNG 를 `Read` 로 직접 열어 본 것)

명령: 임시 트리(시험용 한글 문서 12개 + `.txt` 1개, 그중 "제외한 폴더" 아래 2개)를 만들고
`docview_win --capture-dir <폴더> --scan-root <트리> --klib <klib.exe> --settings <임시 설정> [--panel] [--panel-add <폴더>] [--dark] [--dpi 144]`.
PNG 는 용량(2~5MB) 때문에 저장소에 넣지 않았다(재생성 가능).

| 파일 | 직접 확인한 내용 |
|---|---|
| `docview_realscan_light_96dpi` | 도구줄(검색 자리표시 "문서 이름 검색"·"정렬: 이름 ▲"·"10개 문서"·슬라이더(설정의 170 위치)·"제외 폴더 (3)"·"새로고침"), 카드 10장: "11월 정례회의 회의록.hwpx"(선택)·"2026년 본예산 설명자료.pptx"·"2027년 업무계획(안).hwp"·"민원 처리 현황 보고.xls"·"부서별 추진과제 정리.hwpx"·"신청사 개관 보도자료.pdf"·"안전 점검 결과.pdf"·"예산 집행 현황(11월).xlsx"·"주민 설명회 발표자료.ppt"·"추경 편성 검토.xls" — **실제 파일 크기(31.0 KB·3.0 MB·48.0 KB·120.0 KB·1.3 MB·849.6 KB·310.0 KB·2.1 MB·95.0 KB)와 수정일(2026-11-22)**, 배지 색(HWPX/HWP 파랑·PPT 빨강·XLS 초록·PDF 주황). **제외한 폴더의 "내부 임시자료.hwp"·"초안 1차.pdf"·`.txt` 는 없음**. 상태줄 "엔진: klib scan · 문서 10개 · 완료 0:00" + 선택 파일의 실제 경로 |
| `docview_realscan_dark_96dpi` | 같은 10장, 다크 팔레트(배지·선택 테두리·글자 모두 읽힘) |
| `docview_realscan_light_144dpi` | 150% 배율, 같은 10장·같은 배치, 잘림·겹침 없음 |
| `docview_realscan_panel_light_96dpi` | 제외 폴더 패널: 제목 "제외할 폴더", 안내 두 줄("여기에 추가한 폴더(와 그 아래 폴더)는 문서 검색에서 빠집니다." / "windows·program files·appdata·node_modules·.git 등은 항상 제외됩니다."), 목록 3줄(`C:\Users\haiju\AppData\Local\Temp\claude\ceabd…\제외한 폴더`(말줄임)·`C:\Users\공무원\Downloads`·`D:\백업\옛날 자료`), 버튼 [폴더 추가…]·[선택 삭제](선택 없어 비활성)·[닫기] |
| `docview_realscan_panel_dark_96dpi` | 같은 패널 다크 |
| `docview_realscan_panel_light_144dpi` | 같은 패널 150%, 겹침·잘림 없음 |
| `add/docview_realscan_panel_dark_96dpi` | `--panel-add` = [폴더 추가] 와 같은 저장 경로를 실제 실행한 직후: 목록 4줄(새 `C:\Users\공무원\Documents\개인 메모` 가 정렬 위치에 들어가 선택·강조), 아래 "제외 폴더에 추가했습니다: C:\Users\공무원\Documents\개인 메모", [선택 삭제] 활성. 임시 설정 파일에 실제로 저장됨(모르는 키·다른 값 보존 확인) |

캡처에서 알게 된 점: `--panel-add` 캡처의 도구줄은 아직 "제외 폴더 (3)" 이다 — 캡처 모드는 편집 전에 읽은 값을 쓰기 때문이며, **실제 창은 패널을 닫을 때 개수를 갱신하고 다시 검색**한다(코드 경로 `close_panel`, 자동 시험은 모델 쪽만).

## 실제 창 스모크 (임시 설정 파일, 이 PC 의 앱·설정은 건드리지 않음)
- `docview_win --settings <임시> --synthetic 3000` 3초 뒤 `Responding=True`.
- 같은 설정 파일로 두 번째 실행 → 즉시 종료(기존 창으로 위임), 다른 설정 파일로 실행 → 별개 창(종료 안 함).
- `CloseMainWindow` 로 정상 종료.

## 사용자 실기 확인 항목
1. **실제 폴더 선택 대화상자**: 패널 [폴더 추가…]/Insert 로 고른 폴더가 목록에 들어가는지, 취소 시 변화 없는지(모달이라 자동 시험 불가), 추가·삭제 뒤 **닫으면 다시 검색**되고 그 폴더의 문서가 빠지는지/다시 나오는지.
2. 앱에서 문서뷰 탭 → 설정 `docview_ui`='native' 일 때 새 창이 열리고 이미 떠 있으면 앞으로 오는지, "앱 안에서 보기" 로 되돌아오는지(Flutter 시험은 가짜 API 만). **탭에서 바꾸는 UI 는 탭 안 단추("새 창(Win32)으로 열기")뿐**이다 — 설정 서랍에 별도 줄은 두지 않았다(필요하면 `SettingsUiModeTile` 처럼 추가).
3. 앱이 켜진 채 새 창에서 제외 폴더를 바꾼 뒤, 앱(Flutter 문서뷰/사진뷰)이 바뀐 제외 목록을 다시 읽는 시점(앱 쪽 재읽기 연결은 범위 밖 — 저장 충돌 거부만 구현). 두 화면이 같은 키 `library_excluded_folders` 를 공유한다.
4. 설치본: `resources/docview_win.exe` 가 번들·설치되는지, **설치본에서 `klib.exe` 를 같은 `resources` 에서 찾는지**(앱이 `--klib` 로 경로를 넘긴다), 설치 중 창이 열려 있으면 종료되는지(`build-windows-exe.yml` 은 이 PC 에서 실행하지 않았다).
5. 한글 입력(IME)·고대비·마우스 조작 전반(이전 카드 그대로 미확인).

## 미확인·남은 문제
- **썸네일은 여전히 자리표시**(THUMBRS 후속). 캡처의 문서 카드는 모두 문서 모양 자리표시다.
- 슬라이더 값 **저장**은 이번에도 하지 않았다(읽기만). `ui_thumb_sizes.doc_grid` 는 `docfit_config::validate_patch` 의 보이는 키가 아니라 `docfit_config` 에 중첩 숫자 키 저장 규칙을 더해야 한다(설계 §3 2단계 항목 중 남음).
- 더블클릭 열기·15초 무응답 안내·저장본 즉시 표시·service 엔진·IME 입력은 후속 단계(설계 §7 2~3단계).
- 설정 파일이 이 창에서 **처음 저장될 때** `docfit_config::set` 이 마이그레이션 기본값(`label_symbols`·`report_files_rev` 등)을 파일에 함께 쓴다 — 앱·`settings_win` 과 같은 동작이지만 새 파일이면 키가 늘어 보인다.
- 기본 범위 설정 키 없음(위), `docfit_app` 쪽 Python 은 Desktop/OneDrive 후보를 더 넣는 차이(위).
- 제외 폴더 경로 검증은 `docfit_config`(문자열·NUL·중복)까지만 — 존재하지 않는 폴더도 목록에는 넣을 수 있다(폴더 선택 대화상자는 실제 폴더만 돌려준다).
- 새 창이 열려 있는 동안 앱이 같은 파일을 쓰면 패널은 **저장하는 순간에만** 충돌을 안다(열려 있는 목록은 자동 갱신하지 않음).

## 실행 환경·git
이 PC 의 실제 설정 파일은 읽지도 쓰지도 않았다. `.cargo/config.toml` 없음(산출물은 `CARGO_TARGET_DIR` 환경변수로만). GitLab(`origin`) 미접촉, **푸시하지 않음**(로컬 커밋만).
