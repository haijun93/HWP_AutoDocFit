# 문서뷰 Rust+Win32 창 설계 (DOCVIEWWIN1, 2026-11-22)

근거: `docs/DEV_PLAN_2026-11-28.md` G4, `docs/RUST_MIGRATION_INVENTORY_2026-11-19.md` 8번 행, `docs/SCAN_PROFILE_2026-11-20.md`, `docs/SCANRUST3_RESULT_2026-11-21.md`, `flutter_ui/lib/features/doc_grid/doc_grid_screen.dart`, `docfit_app/{doc_grid_service,rust_scan,thumb_size_setting,library_exclusions}.py`, 선례 `outliner_win/`·`settings_win/`·`ui_theme/`.
이번 카드는 **설계 + 1단계(읽기 전용 격자 표시)** 까지다. 앱 연결(설정 키 `docview_ui` 게이트·Flutter 숨김)은 다음 카드.

## 1. 기능 전수표 (현재 Flutter → Win32 대응)

| # | 현재 기능(근거) | Win32 대응 | 1단계 |
|---|---|---|---|
| 1 | 기본 범위 = 모든 고정 드라이브(`current_default_roots`, 안내문 "모든 고정 드라이브의 문서를 확인합니다.") | `GetLogicalDrives`+`GetDriveTypeW==DRIVE_FIXED` 루트를 `klib scan --roots` 에 전달(`sys::fixed_drive_roots`). 처음 안내 화면 문구 동일 | ✅ |
| 2 | 지원 확장자 7종 HWP·HWPX·XLS·XLSX·PPT·PPTX·PDF | `scan::DOC_EXTENSIONS`(klib `--ext` 로 전달) | ✅ |
| 3 | 제외 폴더 관리(대화상자: 목록·삭제·폴더 추가, 설정 키 `library_excluded_folders`) | 도구줄 "제외 폴더 (n)" 단추. 1단계는 **읽기**: 설정에서 목록을 읽어 `--exclude` 로 전달하고 단추를 누르면 목록만 보여 줌. 추가·삭제·저장은 `docfit_config` 경유로 2단계 | 읽기만 |
| 4 | 예약 제외(windows·program files·appdata·node_modules·.git 등) | klib 가 내장(`scan_cli.rs::EXCLUDED_DIRS`). Rust 쪽에서 따로 넘기지 않음 | ✅(klib) |
| 5 | 썸네일 격자 `SliverGridDelegateWithMaxCrossAxisExtent`(최대 칸=썸네일 크기, 간격 14, 가로세로비 0.74, 바깥 여백 16) | `layout::Metrics` 가 같은 식으로 열 수·칸 크기 계산 | ✅ |
| 6 | 카드: 썸네일(없으면 문서 아이콘), 확장자 배지(색: HWP 파랑·XLS 초록·PPT 빨강·PDF 주황·기타 슬레이트), 이름, 크기(`_formatBytes`) | 같은 색표·같은 크기 표기. 수정일(`YYYY-MM-DD`)·클라우드 표시 추가(카드 요구). **썸네일은 자리표시**(문서 모양 + 확장자 색 바탕) | ✅(자리표시) |
| 7 | 상단 폴더명 + "N개 문서" 알약 | 도구줄 "N개 문서"(검색 중엔 "m / N개 문서") | ✅ |
| 8 | 썸네일 크기 슬라이더(`ThumbSizeSlider`, 100~240, 기본 190, 놓을 때 저장 `ui_thumb_sizes.doc_grid`) | 도구줄 슬라이더, 같은 범위·기본값·키 규칙으로 **읽기**(`settings::thumb_size`). 끌면 즉시 반영(선택 칸이 계속 보이게 스크롤 보정). **저장은 2단계** | 읽기만 |
| 9 | 스캔 진행 표시("문서 검색 중 · 훑은 항목 N개 · 경과", "현재 폴더: …") | 항목이 있으면 격자 위 띠, 없으면 가운데 큰 글. 훑은 수·경과·현재 폴더 | ✅ |
| 10 | 15초 무응답 안내문 | 2단계(마지막 진행 시각 기록 필요) | ❌ |
| 11 | 스캔 취소(가운데 단추) | 도구줄 "스캔 취소"(스캔 중 새로고침 자리) + Esc. 자식 `klib` 프로세스를 죽여 즉시 끝냄 | ✅ |
| 12 | 새로고침(`_loadDefault`) | 도구줄 "새로고침"·F5 | ✅ |
| 13 | 마지막 목록 저장본 즉시 표시(`state=cached`, SCANRUST3) | 2단계(저장본은 현재 Python 이 씀 → Rust 이전 후) | ❌ |
| 14 | 엔진 선택 service→klib→python 과 `engine` 필드 | 상태줄에 "엔진: …" 표시. 1단계 엔진은 `klib scan` 하나. service(`names_query`) 는 3단계, Python 폴백은 두지 않음(§3) | klib만 |
| 15 | 더블클릭 → 시스템 연결 프로그램으로 열기 | 2단계(`ShellExecuteW`) | ❌ |
| 16 | 보이는 칸만 썸네일 요청 | 가상화 격자가 "보이는 칸 목록"을 계산해 THUMBRS 요청에 그대로 사용(§4) | 구조만 |
| 17 | 검색 | (카드 요구; 현 Flutter 문서뷰엔 없음) 이름·확장자 낱말 모두 포함, 대소문자 무시. Ctrl+F | ✅ |
| 18 | 정렬 | (카드 요구; 현 Flutter 엔 없음) 이름·수정일·크기·종류, 방향 토글 | ✅ |
| 19 | 오류 표시(빨간 글) | 가운데 안내 + 상태줄 "실패" | ✅ |
| 20 | 빈 상태 "지원하는 문서가 없습니다." | 같은 문구 | ✅ |
| 21 | 라이트·다크·고대비 | `ui_theme::Palette` | ✅ |
| 22 | 고DPI | 모든 치수 `scale(v, dpi)`, PerMonitorV2 | ✅ |

검색·정렬(17·18)은 현 Flutter 문서뷰에 **없어서** "차이 0" 대상이 아니라 새 기능이다. 사용자 확인 때 유지 여부를 물어야 한다.

## 2. 창 구조

```
┌ 도구줄(52) ──────────────────────────────────────────────────────────┐
│ [검색 칸] [정렬: 이름 ▲] (N개 문서)        크기 ──●── [제외 폴더(n)] [새로고침|스캔 취소] │
├ 스캔 띠(30, 스캔 중이고 항목이 있을 때만) ─────────────────────────────┤
│ 문서 검색 중 · 훑은 항목 1,240개 · 0:12 · 현재 폴더: D:\업무\예산        │
├ 격자(가상화, 오른쪽 12px 스크롤바) ──────────────────────────────────┤
│ ┌썸네일┐ ┌썸네일┐ …   (카드: 자리표시 썸네일+배지 / 이름 2줄 / 크기 · 날짜) │
├ 상태줄(28) ──────────────────────────────────────────────────────────┤
│ 엔진: klib scan · 문서 12,345개 (검색 31개) · 완료 1:34      선택 경로… │
└───────────────────────────────────────────────────────────────────────┘
```

- **그리기**: 실제 창과 캡처 모드가 같은 `paint::draw(hdc, w, h, &App, &Palette, dpi)`. 더블버퍼(메모리 DC → BitBlt). 색은 `ui_theme::Palette` + 확장자 색표(Flutter 와 동일 값)만.
- **가상화(수만 항목)**: `Metrics::visible_range(scroll, n)` 이 화면에 걸리는 행만 계산해 그 칸만 그린다. 50,000개에서 한 번에 24칸(시험 `every_scene_renders…` 가 ≤40칸을 확인). 스크롤 위치는 픽셀 정수, 내용 높이 = `2*pad + rows*card_h + (rows-1)*gap`. 항목은 `Collection` 의 정렬·검색된 **인덱스 목록(u32)** 으로만 참조해 복사하지 않는다. 글 정렬 키는 한 번만 만들어 비교마다 할당하지 않는다.
- **입력**: 마우스 클릭=선택, 휠=스크롤, 스크롤바 손잡이 끌기·막대 클릭=쪽 이동, 슬라이더 끌기, 정렬 단추(클릭=기준 순환, 우클릭·Ctrl/Shift+클릭=방향 반전). 키보드: 방향키·Home·End·PageUp/Down(선택 칸이 항상 보이게 스크롤), Ctrl+F(검색), Esc(검색 지우기/스캔 취소), F5(새로고침). 검색 칸은 직접 그린 입력(WM_CHAR). **한글 조합 중 표시(IME)는 미구현**(완성 글자만 들어감) — 아웃라이너 IME 작업이 끝나면 같은 방식을 가져온다.
- **스레드**: UI 스레드는 그리기·입력만. 스캔은 별도 스레드가 `klib` 자식 프로세스의 stdout 을 줄 단위로 읽어 500줄 또는 80ms 마다 `mpsc` 로 보내고 `PostMessageW(WM_APP+1)` 로 UI 를 깨운다. UI 는 0.5초 타이머로 경과를 갱신.
- **크기 규칙**: 기본 창 1100×720, 최소 820×480(도구줄이 겹치지 않는 최소 폭을 시험이 보증).

## 3. 데이터 경로 (Python 경유 없음)

```
docview_win ──(자식 프로세스) klib scan --roots <고정 드라이브…> --ext hwp,… --json-lines [--exclude <사용자 제외…>]
              └ stdout JSON 줄 → scan::parse_line → Doc → Collection(검색·정렬 인덱스)
[3단계] docview_win ──(named pipe) names_status / names_list → 서비스 색인 목록(SCANRUST3 의 service 경로)
```

- **klib 를 라이브러리로 묶지 않고 실행 파일을 호출**한다. 근거: klib 의존성(tantivy·rusqlite bundled 등)을 끌어오면 빌드가 무겁고, 이미 Python 어댑터(`rust_scan.py`)가 같은 JSON 줄 계약을 쓰며 실행 파일은 릴리스 번들(`resources/klib.exe`)에 항상 있다. 카드가 허용한 "필요 시 klib" 중 필요가 없었다. 의존성은 `windows-sys`·`ui_theme` 둘뿐(외부 JSON 크레이트도 쓰지 않고 `json.rs` 에 읽기 전용 파서를 둠).
- **JSON 줄 계약**(`klib/src/scan_cli.rs::emit`): `{"path","name","ext","size","mtime","mtime_ns","cloud"}`. 파서는 Python `rust_scan.scan` 과 같은 기준으로 `path·name·ext·size·mtime·cloud` 필수·형식 검사, 잘못된 줄이면 **스캔 실패**(Python 은 이때 Python 순회로 폴백했지만, 문서뷰 Win32 는 Python 폴백을 두지 않는다 — Python 제거가 목표).
- **klib 찾기**: `DOCFIT_KLIB` → 실행 파일 옆·상위의 `klib.exe`·`resources\klib.exe`·`klib\target\{release,debug}`. 없으면 가운데 안내 + 상태줄 "실패".
- **취소**: 스레드에 취소 깃발을 세우고 자식 프로세스를 `kill`. 파이프가 닫혀 읽기가 즉시 끝나고 `Finished{cancelled:true}`. 취소 결과는 저장하지 않는다(SCANRUST3 규칙과 같음). 취소 뒤 도착한 줄은 버린다.
- **엔진 선택 순서(SCANRUST3)**: 최종 목표는 service(`names_status state=ready mode=service` → `names_list`) → `klib scan` 이다. 1단계는 klib 만. service 경로는 Rust 가 named pipe(`\\.\pipe\…`)를 직접 열어 같은 요청을 보내는 것으로, 3단계(이 PC 의 서비스를 건드리지 않는 가짜 pipe 서버 시험 선행).
- **메타 보충**: SCANRUST3 결정대로 목록 전체를 stat 하지 않는다. service 경로의 size·mtime sentinel 은 "화면에 나타난 카드만" 보충(3단계).
- **설정 읽기**(`settings.rs`, 쓰기 없음): `%APPDATA%\HwpAutoDocFit\settings.json`(`docfit_config::settings_path` 와 같은 순서). `ui_thumb_sizes.doc_grid`(100~240, NaN·형식 오류는 190), `library_excluded_folders`(빈 값 제거·대소문자·구분자 무시 중복 제거), `docview_ui`('native' 만 인식, 그 외 'flutter'). 저장은 `docfit_config` 의 원자 저장을 쓰도록 2단계에서 연결(여기서 직접 쓰지 않는 이유: 설정 소유자는 `docfit_config` 하나).
- **저장본(scan-cache) 즉시 표시**: 현재 Python(`library_scan_cache`)이 JSON 으로 씀. Rust 가 같은 파일을 읽을지, 형식을 옮길지는 2단계에서 결정(두 구현이 동시에 쓰지 않게 소유자 하나).

## 4. 썸네일 (이번 단계는 자리표시 + 캐시 키 규칙만)

- **1단계**: 칸마다 문서 모양 + 확장자 색 바탕 자리표시(실제 썸네일 아님). 캡처·시험 PNG 에도 자리표시로 보이며, "썸네일이 나온다"고 보고하지 않는다.
- **캐시 키 규칙(THUMBRS 후속이 그대로 쓰도록 확정)**: 키 = `sha256( 소문자·역슬래시 통일 경로 ∥ 0x00 ∥ mtime_ns ∥ 0x00 ∥ size ∥ 0x00 ∥ 썸네일 한 변 px ∥ 0x00 ∥ 렌더러 버전 )` 의 16진 앞 32자. 이유: 파일이 바뀌면(mtime_ns·size) 자동 무효화, 크기별(슬라이더 100~240 × dpi → **8px 단위로 올림**해 키 폭증을 막음) 캐시, 렌더러 버전이 오르면 전체 무효화. 저장 위치는 THUMBRS 가 정하는 기존 `ThumbnailCache` 와 같은 폴더(Python·Rust 가 같은 키를 쓰도록 THUMBRS2 의 키 정의를 **정본**으로 하고 여기 규칙은 합의 후보). *주의*: 이 키 규칙은 이 카드에서 코드로 구현하지 않았다(문서만).
- **요청 흐름(후속)**: 화면의 `visible_range` 를 가운데 칸 우선 순서로 큐에 넣고, 스크롤로 벗어난 요청은 취소. 결과는 UI 스레드로 보내 해당 칸만 다시 그림. 생성은 `klib preview`(이미 있는 CLI) 또는 THUMBRS 엔진.

## 5. 설정·앱 연결 (다음 카드)

- `docview_ui` = `'flutter'`(기본) | `'native'`. `outliner_ui`·`settings_ui` 와 같은 방식: 'native' 일 때 앱의 문서뷰 진입이 `docview_win.exe` 를 띄우고 Flutter 화면은 숨김. 되돌리기 스위치 유지. 이번 카드는 키 **읽기 함수와 해석 규칙**(`settings::docview_ui`, 시험 포함)만 있고 앱·Flutter·Python·`docs/settings_defaults.json` 은 변경하지 않았다.
- 릴리스 번들: `.github/workflows/build-windows-exe.yml` 에 `docview_win` 시험·빌드·`resources/docview_win.exe` 복사·파일 목록 추가(다음 카드). 시험은 `cargo test --release --locked`, 캡처 시험은 `CARGO_BIN_EXE_docview_win` 기준, 파일 비교 시험이 생기면 CRLF 정규화(CI 체크아웃 CRLF).

## 6. 위험

| 위험 | 대응 |
|---|---|
| 수만 항목 정렬·검색 지연(스캔 중 배치마다 재정렬) | 배치는 ≥80ms 간격, 글 키 1회 생성. 50,000건 합성 캡처·시험에서 문제 없음. 20만 건 이상은 미측정 → 정렬을 스캔 완료 뒤로 미루는 옵션 준비 |
| 자식 프로세스 취소 후 좀비/잔여 | `kill` 후 파이프 EOF 로 읽기 스레드 종료. 시험(가짜 klib 배치 파일)이 취소 후 5초 안 종료를 확인. 앱 종료(WM_DESTROY)에서도 취소 |
| klib 가 없는 환경 | 명확한 실패 안내(가운데 글·상태줄). 번들에는 항상 `klib.exe` 가 있음 |
| 한글 입력(IME 조합 표시) | 1단계는 완성 글자만. 검색 칸을 실제 EDIT 컨트롤로 바꾸거나 `outliner_win` IME 방식을 도입(2단계) |
| 스캔 결과 전부 메모리 상주(수십만 × 수백 바이트) | `Doc` 는 문자열 3개+숫자. 항목당 ≈300B 로 추정(미측정) → 수십만이어도 수십 MB. service 경로(이름만)에서는 더 작음 |
| Flutter 와의 차이 | 검색·정렬·수정일 표시는 Flutter 에 없던 기능. 카드 배치·색·크기 표기는 동일 규칙. 사용자 실기 확인 전 기본 전환 금지 |
| 썸네일 없는 상태를 완성으로 오인 | 자리표시를 문서 모양으로 명시, 결과 보고서에 "썸네일 미구현" 기재 |
| 시간대 | 수정일은 `GetTimeZoneInformation` 오프셋으로 지역 날짜 표시(순수 함수 시험은 오프셋을 인자로 받음) |

## 7. 단계 계획

| 단계 | 내용 | 상태 |
|---|---|---|
| 1 (이 카드) | 새 크레이트 `docview_win`: 격자·가상화·선택·키보드·슬라이더(읽기)·검색·정렬·스캔 진행·취소·캡처 모드·시험 | 구현 완료, 실기 대기 |
| 2 | 제외 폴더 관리(추가·삭제·저장) — **DOCVIEWWIN2 에서 구현**; 남음: 슬라이더 값 저장(`docfit_config`), 더블클릭 열기, 15초 무응답 안내, IME 입력, 저장본 즉시 표시, 오른쪽 버튼 메뉴(CTXMENU3 연동 검토) | 일부 완료 |
| 3 | 엔진 service 경로(named pipe `names_status`/`names_list`)·메타 보충·엔진 표시 | 예정 |
| 4 | 실제 썸네일(THUMBRS 연동: 캐시 키 §4·보이는 칸 요청·취소) | THUMBRS2 이후 |
| 5 | 앱 연결: `docview_ui` 설정·릴리스 번들·CI·Flutter 게이트(기본 flutter → 실기 확인 후 native 전환 → Flutter 화면·Python 제거) | 연결·번들 구현(DOCVIEWWIN2, `docs/DOCVIEWWIN2_RESULT_2026-11-22.md`), native 기본 전환은 실기 확인 후 |
| 6 | 사진뷰 격자 재사용(`docview_win` 격자·가상화 부품을 공용 크레이트로 분리 검토) | G4 후속 |
