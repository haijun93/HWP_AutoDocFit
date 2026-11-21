# PHOTOWIN2 결과 (2026-11-22)

카드: 사진뷰 Win32 창 앱 연결(설정 `photoview_ui`) + 번들 + 제외 폴더·설정 읽기 + 실제 klib 경로 검증. DOCVIEWWIN2(`docs/DOCVIEWWIN2_RESULT_2026-11-22.md`)와 같은 방식.
브랜치 `feature/PHOTOWIN2`(DOCVIEWWIN2 커밋 위에 쌓음), 커밋만(푸시 없음). `hwp-auto-docfit.py`·GitLab(`origin`)·이 PC 의 실제 설정(`%APPDATA%\HwpAutoDocFit\settings.json`)·실행 중 앱은 건드리지 않았다(시험·캡처·스모크 모두 임시 폴더·임시 설정 파일).
기본값은 `photoview_ui='flutter'` 라서 **사용자 눈에 보이는 변화는 없다**(사진뷰 탭 그대로).

## 연결 표 (`docview_ui` 와 같은 구조)

| 위치 | 변경 |
|---|---|
| `docfit_app/settings_schema.py` | `DEFAULTS['photoview_ui'] = 'flutter'` |
| `docfit_app/configuration_service.py` | `EXCLUDED` 에 `photoview_ui` |
| `docs/settings_defaults.json`(골든) | `photoview_ui` 추가 |
| `docfit_config/src/lib.rs` | 설정 창 숨김 목록에 `photoview_ui` |
| `docfit_app/photoview_window_service.py`(새) | `ui_mode`·`open_native`·`open_if_native`(native 일 때만 `photoview_win.exe --settings <앱 설정> [--klib <앱이 찾은 klib>]`). 오류 코드 `PHOTOVIEW_WINDOW_MISSING`·`PHOTOVIEW_WINDOW_FAILED` |
| `desktop_web_ui.py`·`docfit_app/application_service.py` | 브리지 메서드 `photoview_window(payload)` + `_BrowserApi` 허용 목록 + `dispatch` 개수표·분기 |
| `flutter_ui/lib/features/photo_album/photoview_ui_gate.dart`(새) | `PhotoViewUiGate`: flutter 면 기존 `PhotoAlbumScreen`(키 `tab-photo-album-adapter` 유지) + "새 창(Win32)으로 열기" 단추, native 면 창을 열고 안내·[창 다시 열기]·[앱 안에서 보기] |
| `flutter_ui/lib/features/workspace/workspace_adapters.dart` | 사진뷰 탭의 지연 어댑터가 `PhotoViewUiGate` 를 거침 |
| `HWP_AutoDocFit.spec` | `resources/photoview_win.exe` 가 있으면 번들 |
| `.github/workflows/build-windows-exe.yml` | 새 단계 "Build photo view window"(`cargo test --release --locked` → `cargo build --release --locked --bin photoview_win` → `resources/photoview_win.exe` 복사 → 존재 확인). **klib 빌드 단계 뒤**에 두고 `KLIB_EXE` 지정(시험이 실제 klib 를 돌린다). 런타임 비의존 검사 목록 `$files` 에 추가 |
| `installer/setup.iss` | `StopHelper('photoview_win.exe')` |
| `photoview_win/Cargo.toml`·`Cargo.lock` | `docfit_config`(경로)·`serde_json`·dev `tempfile`, windows-sys 기능 추가. `Cargo.lock` 은 cargo 가 만든 그대로(`--locked` 통과) |

`.gitignore` 에는 `photoview_win/target/` 이 이미 있었다. 시험이 숫자를 고정하지 않는다(골든·키 개수는 스키마에서 계산).

## 창(`photoview_win`) 변경 — 문서뷰와 같은 코드(`panel.rs`·`folder.rs` 이식)

- **설정 읽기**(`settings::load`, `docfit_config::get`): `ui_thumb_sizes.photo_album`(100~240, 없으면 160)·`library_excluded_folders`·`photoview_ui`·revision. 파일이 없으면 기본값(파일을 만들지 않음), 깨졌으면 기본값 + 상태줄 안내. 제외 폴더를 `klib scan --exclude` 로 전달(시작 때 읽은 값).
- **제외 폴더 편집 패널**(도구줄 "제외 폴더 (n)"): 추가(`SHBrowseForFolderW`)·선택 삭제·닫기, 키(↑↓·Delete·Insert·Esc)·휠, 저장은 `docfit_config::set` 한 길(목록 검증·원자 저장·revision 충돌 검사, 충돌이면 파일 불변 + 최신 목록 재로드). 닫으면 바뀐 제외 폴더로 다시 검색. 안내문은 "사진 검색에서 빠집니다".
- **단일 인스턴스**: 같은 설정 파일로 이미 떠 있으면 새 프로세스는 기존 창을 앞으로 하고 종료(뮤텍스 `…PhotoViewWin.Singleton.<설정 경로 해시>`, 창 클래스 `PhotoViewWinMain`).
- 캡처 모드 확장: `--scan-root <폴더> --klib <exe> --settings <파일> [--panel] [--panel-add <폴더>]`(실제 klib 스캔 결과·패널).
- 사진뷰 쪽 차이: 썸네일 슬라이더 값은 `ui_thumb_sizes.photo_album`, 자리표시 카드는 PHOTO 격자 규칙(PHOTOWIN1) 그대로. **썸네일은 이 카드 범위 밖**(THUMBSHELL1 결과로 후속).
- 검색·정렬은 PHOTOWIN1 에서 이미 구현돼 있다(앱 설정 키가 아니라 창 안 상태). 이 카드에서 바꾸지 않았다.

## 실제 klib 경로 검증 (건너뛰지 않는다)

`photoview_win/tests/real_klib_tests.rs`(5개). klib 는 `DOCFIT_KLIB` → `KLIB_EXE` → 옆·상위 `klib.exe`/`resources`/`klib\target` 순(`grid_core::scan::find_klib`), 없으면 **실패**. 이 PC 에서는 THUMBRS2 가 빌드해 둔 release `klib.exe` 를 스크래치 폴더에 복사해 `KLIB_EXE` 로 지정했다.

| 시험 | 확인 |
|---|---|
| `real_klib_scans_a_temp_tree_with_the_photo_extensions_only` | 임시 트리(한글 이름·하위·깊은 폴더·`.txt`·`.hwp`) → 사진 확장자 6개(jpg·PNG·webp·heic·jpeg·bmp)만, `.txt`·`.hwp` 제외, 실제 크기(`.PNG`=7바이트)·수정시각·절대경로·실제 파일 |
| `real_klib_honors_excluded_folders_passed_with_exclude` | `--exclude` 로 그 폴더와 아래 폴더 사진이 빠짐(4개). 대문자·슬래시 형태도 같은 폴더 |
| `real_klib_nonexistent_root_is_an_empty_successful_scan` | 없는 루트 = 오류 아님(0건) |
| `real_klib_error_exit_code_becomes_a_failed_scan` | 루트 없는 인자 → 종료 코드 오류 `Err`, 없는 실행 파일 → "실행하지 못했습니다" |
| `real_klib_scan_can_be_cancelled_quickly_and_the_child_exits` | 고정 드라이브 루트 읽기 전용 스캔을 300ms 뒤 취소 → 5초 안 `Finished{cancelled:true}` |

## 시험 결과 (`--release --locked`, CI 와 같게. `CARGO_TARGET_DIR`·PATH 앞에 mingw64 bin·`KLIB_EXE`)

```
cargo test --release --locked --manifest-path photoview_win/Cargo.toml
  src\lib.rs / src\main.rs / Doc-tests   test result: ok. 0 passed; 0 failed
  tests\capture_tests.rs     test result: ok. 5 passed; 0 failed; ... finished in 3.14s
  tests\exclude_tests.rs     test result: ok. 11 passed; 0 failed; ... finished in 0.71s
  tests\pure_tests.rs        test result: ok. 23 passed; 0 failed; ... finished in 1.45s
  tests\real_klib_tests.rs   test result: ok. 5 passed; 0 failed; ... finished in 1.39s
cargo test --release --locked --manifest-path grid_core/Cargo.toml    → ok. 5 passed (core_tests)
cargo test --locked --manifest-path docfit_config/Cargo.toml          → ok. 8 passed
python -m unittest tests.test_photoview_window_service tests.test_docview_window_service tests.test_settings_window_service tests.test_bridge_methods tests.test_distribution_build tests.test_outliner_service tests.test_theme_mode tests.test_core_app_state tests.test_kclip_window_state tests.test_search_engine_settings tests.test_settings_compare tests.test_settings_list_keys tests.test_abbreviation_service
  Ran 109 tests ... OK (skipped=2)
flutter analyze → No issues found!
flutter test test/photoview_ui_gate_test.dart test/docview_ui_gate_test.dart test/photo_album_screen_test.dart test/workspace_test.dart test/tab_open_events_test.dart test/tab_navigation_shell_test.dart test/workspace_adapters_test.dart → +73: All tests passed!
```

- `photoview_win` 합계 **44개 통과 / 0 실패**(PHOTOWIN1 의 29 → `pure_tests` 의 건너뛰던 klib 시험 1개 제거, `real_klib_tests` 5개·`exclude_tests` 11개 추가).
- `exclude_tests`(11): 설정 읽기(썸네일 `photo_album`·제외 폴더·ui)·없는/깨진 파일은 기본값이고 파일 불변·`photoview_ui` 가 앱 기본값에 있고 설정 창에 안 보임·폴더 키 규칙·추가/삭제 저장과 모르는 키 보존·revision 충돌 거부(파일 불변)·패널 배치(창 크기 820×480~1920×1080 × DPI 96~192 에서 겹침 없음) 등.
- CI 주의: 실행 파일 위치는 `CARGO_BIN_EXE` 기준(`capture_tests` 기존), 파일 비교 시험은 새로 만들지 않아 CRLF 정규화 불필요. workflow 순서는 `assertLess(klib 단계, photoview 단계)` 로 시험이 고정.
- 전체 시험은 돌리지 않았다(지시). `scripts/settings_compare.py --exe`(settings-core.exe 필요)는 이번에 직접 돌리지 않았고 `tests.test_settings_compare`(스키마 쪽)만 통과.

## 캡처 (실제 `paint::draw_with_panel`, 실제 `klib scan` 결과, PNG 를 `Read` 로 직접 열어 본 것)

임시 트리(사진 확장자 9개 + 제외 폴더 안 2개 + `.txt` + `.hwp`)에 `photoview_win --capture-dir <폴더> --scan-root <트리> --klib <klib.exe> --settings <임시 설정> [--panel] [--panel-add <폴더>] [--dark] [--dpi 144]`. PNG 는 용량 때문에 저장소에 넣지 않았다(재생성 가능).

| 파일 | 직접 확인한 내용 |
|---|---|
| `photoview_realscan_light_96dpi` | 도구줄(검색 자리표시 "파일 이름 검색"·"정렬: 수정일 ▼"·"사진 9장"·슬라이더·"제외 폴더 (3)"·"새로고침"), 카드 9장: 로고 시안.bmp(선택)·스마트폰 사진.heic·스캔본 001.tif·마포 야경.webp·주민설명회 현장.jpeg·도로 보수 후.png·도로 보수 전.png·신청사 전경.JPG·공원 준공식.jpg — **실제 파일 크기(1.4 MB·2.7 MB·8.6 MB·390.6 KB·2.1 MB·888.7 KB·800.8 KB·4.9 MB·3.2 MB)와 수정일(2026-11-22)**, 확장자 배지·색. **제외 폴더의 사진 2장·`.txt`·`.hwp` 는 없음**. 상태줄 "엔진: klib scan · 사진 9장 · 완료 0:00" + 선택 파일 실제 경로 |
| `photoview_realscan_dark_96dpi` | 같은 9장, 다크 팔레트(글자·배지·선택 테두리 읽힘) |
| `photoview_realscan_light_144dpi` | 생성만 했고 이 파일 자체는 열어 보지 않았다(150% 는 아래 패널 150% 로 확인) |
| `photoview_realscan_panel_light_96dpi` | 제외 폴더 패널: 제목 "제외할 폴더", 안내("여기에 추가한 폴더(와 그 아래 폴더)는 사진 검색에서 빠집니다." / "windows·program files·appdata·node_modules·.git 등은 항상 제외됩니다."), 목록 3줄(임시 폴더 경로 말줄임·`C:\Users\공무원\Pictures\개인`·`D:\백업\옛날 사진`), [폴더 추가…]·[선택 삭제](비활성)·[닫기] |
| `photoview_realscan_panel_light_144dpi` | 150% 배율 패널: 같은 3줄 목록·버튼, 뒤 카드(BMP·HEIC·PNG), 글자 잘림·겹침 없음 |
| `photoview_realscan_panel_dark_96dpi` | 생성만 했고 열어 보지 않았다(다크 패널은 아래 `add` 캡처로 확인) |
| `add/photoview_realscan_panel_dark_96dpi` | `--panel-add` = [폴더 추가] 와 같은 저장 경로를 실제 실행한 직후: 목록 4줄(새 `C:\Users\공무원\Pictures\여행 사진` 이 정렬 위치에 들어가 선택·강조), "제외 폴더에 추가했습니다: …", [선택 삭제] 활성 |

캡처에서 알게 된 점: `--panel-add` 캡처의 도구줄은 편집 전에 읽은 "제외 폴더 (3)" 그대로다(캡처 모드 한계; 실제 창은 닫을 때 갱신·재검색). 슬라이더 위치는 설정의 170 반영.

## 실제 창 스모크 (임시 설정 파일)
`photoview_win --settings <임시> --synthetic 3000` 3초 뒤 `Responding=True`; 같은 설정 파일로 두 번째 실행 → 즉시 종료(기존 창으로 위임); 다른 설정 파일로 실행 → 별개 창; `CloseMainWindow` 로 둘 다 정상 종료; 읽기만으로 설정 파일이 생기지 않음.

## 사용자 실기 확인 항목
1. 실제 폴더 선택 대화상자(모달이라 자동 시험 불가): 추가·삭제 뒤 **닫으면 다시 검색**되고 그 폴더의 사진이 빠지는지/다시 나오는지.
2. 앱에서 사진뷰 탭 + 설정 `photoview_ui`='native' 일 때 새 창이 열리고 이미 떠 있으면 앞으로 오는지, "앱 안에서 보기" 로 되돌아오는지(Flutter 시험은 가짜 API 만). 탭에서 바꾸는 UI 는 탭 안 단추("새 창(Win32)으로 열기")뿐(설정 서랍 줄 없음).
3. 앱이 켜진 채 새 창에서 제외 폴더를 바꾼 뒤 앱(Flutter 사진뷰)이 바뀐 목록을 다시 읽는 시점(범위 밖 — 저장 충돌 거부만 구현).
4. 설치본: `resources/photoview_win.exe` 번들·설치, 설치본에서 `klib.exe` 를 같은 `resources` 에서 찾는지(앱이 `--klib` 로 경로를 넘김), 설치 중 창 종료. `build-windows-exe.yml` 은 이 PC 에서 실행하지 않았다.
5. IME 입력·고대비·마우스 조작 전반(이전 카드 그대로 미확인).

## 미확인·남은 문제
- **썸네일은 여전히 자리표시**(THUMBSHELL1 결과로 후속 카드).
- 슬라이더 값 **저장**은 하지 않았다(읽기만). `ui_thumb_sizes.photo_album` 저장은 `docfit_config` 에 중첩 숫자 키 저장 규칙이 더 필요.
- 사진 범위(기본 검색 루트)는 앱에 설정 키가 없어 고정 드라이브 전체 그대로(Python 의 Desktop/OneDrive 후보와 같은 파일 중복 차이는 DOCVIEWWIN2 결과와 동일).
- 더블클릭 열기·미리보기·15초 무응답 안내·service 엔진·IME 입력은 후속 단계.
- 제외 폴더 경로 검증은 `docfit_config`(문자열·NUL·중복)까지만 — 존재하지 않는 폴더도 목록에 들어갈 수 있다.
- 설정 파일이 이 창에서 처음 저장될 때 `docfit_config::set` 이 마이그레이션 기본값을 함께 쓴다(앱·설정 창과 같은 동작).
- 이 PC 에서 `dlltool.exe` 는 PATH 에 mingw64 bin(`%LOCALAPPDATA%\DocFitTools\mingw64\bin`)을 앞에 두어야 빌드된다(bash 에서는 `C:` 표기를 `/c/...` 로).

## 실행 환경·git
`.cargo/config.toml` 없음(산출물은 `CARGO_TARGET_DIR` 환경변수로만). GitLab(`origin`) 미접촉, **푸시하지 않음**(로컬 커밋만).
