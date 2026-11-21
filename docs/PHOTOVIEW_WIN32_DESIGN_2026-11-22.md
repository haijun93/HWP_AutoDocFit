# 사진뷰 Rust+Win32 창 설계 (PHOTOWIN1, 2026-11-22)

근거: `docs/DEV_PLAN_2026-11-28.md` G4, `docs/RUST_MIGRATION_INVENTORY_2026-11-19.md` 9번 행(사진뷰), `docs/SCAN_PROFILE_2026-11-20.md`, `docs/SCANRUST3_RESULT_2026-11-21.md`(엔진 선택 service→klib→python·저장본·`engine` 필드), `docs/THUMBRS2_RESULT_2026-11-22.md`, `docs/DOCVIEW_WIN32_DESIGN_2026-11-22.md`(문서뷰 창, 같은 구조), `flutter_ui/lib/features/photo_album/photo_album_screen.dart`(3,143줄), `docfit_app/{photo_album_service,thumb_size_setting,library_exclusions}.py`, `docfit_core/timemachine/thumbs.py`(`ThumbnailCache`).
이번 카드는 **설계 + 1단계(격자 표시·검색·스캔·크기 슬라이더, 읽기 전용)** 까지다. 앱 연결(설정 키 `photoview_ui` 게이트·Flutter 숨김)은 다음 카드.

## 1. 기능 전수표 (현재 Flutter 사진뷰 → Win32 대응)

현재 사진뷰는 Flutter 화면(3,143줄)이 `photo_album` 서비스(Python 2,116줄, SQLite `photo_album.db` + EXIF + Pillow)를 호출한다.

| # | 현재 기능(근거) | Win32 대응 | 단계 |
|---|---|---|---|
| 1 | 기본 범위 = 모든 고정 드라이브(`scan_default_scope`, `_default_roots`) | 문서뷰와 같이 `GetLogicalDrives`+`DRIVE_FIXED` 루트를 `klib scan --roots` 로 전달 | 1 ✅ |
| 2 | 지원 확장자 8종 JPG·JPEG·PNG·WEBP·BMP·TIF·TIFF·HEIC(`PHOTO_EXTENSIONS`) | `scan::PHOTO_EXTENSIONS` → `klib scan --ext` | 1 ✅ |
| 3 | 제외 폴더 관리(`excluded_folders_get/add/remove`, 문서뷰와 **같은 키** `library_excluded_folders`) | 도구줄 "제외 폴더 (n)": 1단계는 읽기만(목록 표시), 추가·삭제·저장은 `docfit_config` 경유 | 1 읽기 / 2 |
| 4 | 썸네일 격자(`SliverGridDelegateWithMaxCrossAxisExtent` 최대 칸=썸네일 크기, **간격 10·가로세로비 0.8**, 가로 여백 16) | `grid_core::layout::GridSpec::PHOTO`(간격 10·여백 16·비 0.8). 문서뷰 `GridSpec::DOC`(14·16·0.74)와 같은 계산기 | 1 ✅ |
| 5 | 카드: 썸네일 + 즐겨찾기 별 + 다중 선택 체크 | 1단계는 자리표시(해·산 그림)+확장자 배지+파일 이름+크기·수정일. 별·체크는 2단계 | 1 일부 |
| 6 | 썸네일 크기 슬라이더(`ThumbSizeSlider`, 100~240, **기본 160**, 키 `ui_thumb_sizes.photo_album`) | 같은 범위·기본값·키로 **읽기**(`settings::thumb_size`). 끌면 즉시 반영(선택 칸 계속 보이게). 저장은 2단계 | 1 읽기 |
| 7 | 스캔 진행 표시("문서 검색 중…" 대응: 훑은 수·현재 폴더·경과)·취소(`_cancelScan`) | 항목이 있으면 격자 위 띠, 없으면 가운데 큰 글, "스캔 취소"·Esc, 자식 `klib` 프로세스 종료 | 1 ✅ |
| 8 | 새로고침(`_refreshCurrentFolder`) | 새로고침 단추·F5 | 1 ✅ |
| 9 | 검색(파일명·태그, 서버 `search`) | 1단계: 파일 이름·확장자 낱말 모두 포함(대소문자 무시, Ctrl+F). **태그 검색은 2단계**(태그 DB 필요) | 1 일부 |
| 10 | 정렬(Flutter 는 보기 모드별 고정: 날짜 내림차순) | 이름·수정일·크기·종류, 방향 토글. 기본은 **수정일 최신순**. 연월 묶음은 3단계 | 1 ✅ |
| 11 | **연·월 묶음 격자**(`list_by_date`: EXIF 촬영일 우선, 없으면 수정일; 헤더 "2026년 11월 (N장)") | 1단계는 평면 격자. 묶음 헤더가 있는 가상화 격자(행 높이가 다른 목록)는 3단계 — 가상화 계산기를 "행 종류 목록"으로 확장 | 3 |
| 12 | 보기 모드 4종: 날짜별·폴더별·카메라별·위치별 | 날짜별은 3단계, 폴더별은 같은 묶음 구조로 쉽게 확장, 카메라별·위치별은 EXIF(Make·Model·GPS) 필요 → 4단계 | 3~4 |
| 13 | 즐겨찾기(`toggle_favorite`)·즐겨찾기 필터 | `photo_album.db`(SQLite) 읽기·쓰기 필요. 소유자 하나 원칙 → Rust 가 DB 를 직접 열지 서비스 호출로 갈지 2단계 결정(§3) | 2 |
| 14 | 태그 추가·삭제·태그 칩 필터(`add_tag/remove_tag/list_tags`) | 13 과 같음 | 2 |
| 15 | 다중 선택·전체 선택·일괄 내보내기(`batch_export`) | 선택 집합(Ctrl/Shift+클릭)·내보내기는 3단계 | 3 |
| 16 | 사진 크게 보기(뷰어 모달)·이전/다음·**슬라이드쇼**(`_PhotoViewerModal`) | 별도 뷰어 창(디코드 필요) — 썸네일 공급과 같은 디코더 결정 뒤 4단계 | 4 |
| 17 | 사본 편집(`edit_copy`: 회전·자르기·밝기 등 Pillow) | 디코드·인코드 필요. Pillow 제거 목표와 묶어 4단계(원본은 절대 수정 안 하고 사본 저장 규칙 유지) | 4 |
| 18 | **사진대지 HWPX** 만들기(`build_photo_sheet_hwpx`, 선택 사진 → HWPX) | HWPX XML 조립은 Python 서비스 기능. Rust 이전은 별도 카드(HWPX 쓰기 공용 부품 후) — 그전엔 창이 서비스 호출 | 4 |
| 19 | **중복·유사 사진 찾기**(`find_duplicates`, dHash) 결과 보기 | DUPRS1(dHash Rust)가 먼저. 창은 결과 그룹 목록 보기만 | 4 |
| 20 | 위치별 묶음(`place_groups`, GPS → 지명) | EXIF GPS 파서 + 지명 사전 필요 → 4단계 | 4 |
| 21 | 마지막 목록 저장본 즉시 표시(`state=cached`, SCANRUST3) | 저장본 소유자 하나 원칙 → 2단계 결정 | 2 |
| 22 | 엔진 선택 service→klib→python, `engine` 필드 | 상태줄 "엔진: …". 1단계는 klib 하나. service(named pipe `names_*`)는 3단계 | 1 klib만 |
| 23 | 클라우드 전용 파일은 내용을 읽지 않음(`CLOUD_ONLY`) | 카드에 "클라우드" 표시(klib `cloud` 필드), 썸네일 요청에서 제외 | 1 표시 |
| 24 | 오류·빈 상태 | "사진 목록을 불러오지 못했습니다." 빨간 글·"사진이 없습니다." | 1 ✅ |
| 25 | 라이트·다크·고대비·고DPI | `ui_theme::Palette`, PerMonitorV2, 모든 치수 `scale(v, dpi)` | 1 ✅ |

1단계 합계: 격자 표시·가상화·선택·키보드·크기 슬라이더(읽기)·검색·정렬·스캔 진행·취소·캡처. **연월 묶음·즐겨찾기·태그·중복·사본 편집·사진대지·슬라이드쇼·위치별은 모두 다음 단계**이며 이번에 코드로 만들지 않았다(§7).

## 2. 창 구조

문서뷰(`docview_win`)와 같은 5구역이고 칸 규칙·글만 다르다.

```
┌ 도구줄(52) ──────────────────────────────────────────────────────────┐
│ [파일 이름 검색] [정렬: 수정일 ▼] (사진 N장)      크기 ──●── [제외 폴더(n)] [새로고침|스캔 취소] │
├ 스캔 띠(30, 스캔 중이고 항목이 있을 때만) ─────────────────────────────┤
│ 사진 검색 중 · 훑은 사진 1,240장 · 0:12 · 현재 폴더: D:\사진\2026 현장 점검 │
├ 격자(가상화, 오른쪽 12px 스크롤바) ──────────────────────────────────┤
│ ┌썸네일(자리표시)┐ …  (카드: 사진 자리표시+배지 / 파일 이름 1줄 / 크기 · 수정일) │
├ 상태줄(28) ──────────────────────────────────────────────────────────┤
│ 엔진: klib scan · 사진 12,345장 (검색 31장) · 완료 1:34      선택 경로… │
└───────────────────────────────────────────────────────────────────────┘
```

- **그리기**: 실제 창과 캡처 모드가 같은 `paint::draw(hdc, w, h, &App, &Palette, dpi)`. 더블버퍼. 색은 `ui_theme::Palette` + 사진 확장자 색표(JPG 파랑·PNG 초록·HEIC 보라·WEBP 주황·TIF 분홍·기타 슬레이트).
- **가상화(수만 항목)**: `Metrics::visible_range(scroll, n)` 이 화면에 걸리는 행만 계산. 50,000장에서 한 번에 35칸(캡처 확인). 항목은 `Collection` 의 정렬·검색된 인덱스 목록(`u32`)으로만 참조한다. 칸이 작을수록(썸네일 100) 한 화면 칸 수가 늘지만 수십 개 수준이다.
- **입력**: 클릭=선택, 휠, 스크롤바 끌기, 슬라이더 끌기, 정렬 단추(클릭=기준 순환, 우클릭·Ctrl/Shift+클릭=방향 반전), 방향키·Home·End·PageUp/Down(선택 칸이 항상 보임), Ctrl+F, Esc(검색 지우기/스캔 취소), F5. 검색 칸 한글 조합 표시(IME)는 문서뷰와 같이 미구현(2단계).
- **스레드**: UI 스레드는 그리기·입력만. 스캔은 별도 스레드가 `klib` 자식 프로세스 stdout 을 읽어 500줄 또는 80ms 마다 `mpsc` 로 보내고 `PostMessageW(WM_APP+1)` 로 깨운다. (3단계 썸네일 공급자는 같은 방식의 별도 스레드 풀.)
- **크기 규칙**: 기본 1100×720, 최소 820×480.

## 3. 데이터 경로 (Python 경유 없음)

```
photoview_win ──(자식 프로세스) klib scan --roots <고정 드라이브…> --ext jpg,jpeg,png,webp,bmp,tif,tiff,heic --json-lines [--exclude …]
                └ stdout JSON 줄 → grid_core::items::parse_scan_line → Doc → Collection(검색·정렬 인덱스)
[3단계] photoview_win ──(named pipe) names_status / names_list → 서비스 색인 목록(SCANRUST3 의 service 경로)
```

- 문서뷰와 **같은 줄 형식**(`klib/src/scan_cli.rs::emit` = `{"path","name","ext","size","mtime","mtime_ns","cloud"}`)이라 파서·스캔 작업·취소를 `grid_core` 로 공용화했다. klib 를 라이브러리로 묶지 않고 실행 파일을 호출하는 이유도 문서뷰 설계 §3 과 같다(tantivy·rusqlite 를 끌어와 빌드가 무거워지는 것을 피함, 의존성 `windows-sys`·`ui_theme`·`grid_core` 뿐). 이 단계의 `klib.exe` 는 사진 확장자를 `--ext` 로 받으면 된다(klib 변경 없음).
- **취소**: 스레드에 깃발을 세우고 자식을 `kill`. 취소 결과는 저장하지 않는다(SCANRUST3 와 같음).
- **엔진 순서(SCANRUST3)**: 최종 service → klib. 1단계는 klib 만, Python 폴백 없음.
- **촬영일·EXIF**: 현재 Python 은 `read_exif_metadata` 로 촬영일을 읽어 연월 묶음에 쓴다. klib scan 줄에는 EXIF 가 없다. 연월 묶음(3단계)은 두 길 중 하나: ① 수정일로 묶고 화면에 나온 사진만 EXIF 보충(문서뷰 메타 보충과 같은 방식) ② `klib` 에 `exif` 하위 명령 추가. **①을 권장**(수만 장 전체 EXIF 읽기는 비쌈).
- **DB 소유(즐겨찾기·태그, 2단계 결정 사항)**: 현재 `photo_album.db`(SQLite)의 소유자는 Python `PhotoAlbumDb`. Rust 가 같은 파일을 동시에 쓰면 충돌하므로 ① Python 서비스가 살아 있는 동안은 서비스 호출 ② Python 제거 시점에 Rust 가 단독 소유(rusqlite bundled — klib 가 이미 쓰는 의존성)로 전환. 소유자는 항상 하나.
- **설정 읽기**(`settings.rs`, 쓰기 없음): `%APPDATA%\HwpAutoDocFit\settings.json`. `ui_thumb_sizes.photo_album`(100~240, NaN·형식 오류는 **160**), `library_excluded_folders`(문서뷰와 공유), `photoview_ui`('native' 만 인식, 그 외 'flutter').

## 4. 썸네일 (이번 단계는 자리표시 + 캐시 키 규칙만 / 공급 방식 비교)

**1단계**: 모든 칸이 자리표시(해·산 그림 + 확장자 색 바탕)다. 캡처 PNG 에서도 자리표시로 보이며 "썸네일이 나온다"고 보고하지 않는다.

**캐시 키 규칙**: 이미 Python 이 쓰는 `ThumbnailCache.key(path, mtime_ns, size) = sha256("{path_key}|{mtime_ns}|{size}")`(파일 `<키>.jpg`, JPEG 품질 70, 총량 200 MB·항목 16 MB 상한, `data_dir()/thumbs`)를 **정본으로 재사용**한다 — 이번에 만든 새 규칙은 없다. 한 사진 파일에 한 장(최대 변 160 기준)만 캐시하고 슬라이더 크기별로 따로 만들지 않는다(화면에서는 칸 크기에 맞춰 축소 그리기). 이 키를 Rust 가 만들려면 SHA-256 구현이 필요하다(의존성 추가 또는 자체 구현 — 3단계에서 결정, 이번엔 코드 없음).

### 공급 방식 후보 비교

| 후보 | 속도(근거) | 화질 | 의존성 | Rust+Win32 원칙 | 위험 |
|---|---|---|---|---|---|
| A. 기존 Python 서비스의 결과 파일 캐시를 읽기만(키 규칙 동일) | 캐시 적중은 파일 읽기 한 번(디코드 없음) | Pillow 결과 그대로(차이 0) | 없음(SHA-256 만) | Python 이 **생성**하므로 Python 제거와 충돌. 캐시에 없는 사진은 못 만듦 | 서비스가 꺼져 있으면 새 사진 썸네일 없음. 전환 기간 한정 |
| B. Rust `klib thumb`(THUMBRS2) | **Pillow 보다 약 3배 느림**(실측) | 평균 오차 16.8/255, 크기 일치 19/19 | `image` 18개 의존성 | 맞음 | 채택하지 않음(THUMBRS2 결론). 재평가는 C 와 묶어서 |
| C. DCT 축소 디코드가 되는 JPEG 디코더 재평가(예: libjpeg-turbo 계열·`zune-jpeg` 스케일 디코드 가능 여부, `jpeg-decoder` 의 scale) | JPEG 는 1/2·1/4·1/8 축소 디코드로 풀 디코드 대비 수 배 빠름(**미측정**; 이론상 THUMBRS2 의 3배 열세를 뒤집을 수 있음) | 축소 디코드는 Lanczos 와 픽셀이 다름 → 오차 기준을 사용자와 합의 필요 | 크레이트 1~2개 추가 | 맞음 | PNG·HEIC·WebP 는 별도 경로 필요, 측정 전엔 결론 불가 |
| D. **Windows 셸 썸네일 API `IShellItemImageFactory::GetImage`** | **미측정**. 셸이 자체 썸네일 캐시(`thumbcache_*.db`)와 코덱(WIC)을 써서 탐색기와 같은 속도·적중. 디코드를 직접 하지 않아 CPU 부담이 작을 가능성이 큼 | WIC 가 EXIF 회전을 반영하고 HEIC·WebP 는 **사용자 PC 에 코덱(HEIF Image Extensions 등)이 있으면** 지원 | **없음**(Win32 COM: `SHCreateItemFromParsingName`+`IShellItemImageFactory`; `windows-sys` 에는 COM 인터페이스 정의가 없어 vtable 을 직접 선언하거나 `windows` 크레이트 필요) | **가장 잘 맞음** | ① 셸 캐시 크기·품질이 탐색기 설정에 의존 ② 클라우드 전용(OneDrive 온디맨드) 파일은 셸이 **내려받기를 유발**할 수 있어 `cloud` 필드가 참이면 호출 금지 필수 ③ 로그온 세션·COM 단일 스레드 공간(STA) 스레드 필요 ④ 폐쇄망 PC 의 HEIC 코덱 유무 ⑤ 결과가 HBITMAP(32비트 알파) → 그려서 쓰면 JPEG 파일 캐시는 불필요 |

**권고**: **D(셸 썸네일 API)를 먼저 시험한다**(3단계 첫 작업). 측정 항목: 사진 1,000장(JPEG 12 MP·PNG·HEIC 섞음)에서 ① 첫 호출·재호출 시간 ② Pillow(`get_thumbnail`)와 같은 입력 시간 비교 ③ 클라우드 전용 파일 호출 시 내려받기 발생 여부(금지 규칙 시험) ④ STA 스레드 2~3개 병렬의 처리량 ⑤ 화질 오차(Pillow 대비 평균 절대 오차 — THUMBRS2 기준 16.8/255 와 비교). D 가 기준을 넘으면 B·C 를 접고 Python 서비스의 썸네일 생성을 제거한다. D 가 코덱 문제 등으로 불가하면 **A 를 전환 기간에 병행**하고 C 를 측정한다. 이 비교는 **설계 근거일 뿐 이번 카드에서 측정하지 않았다**(수치는 THUMBRS2 실측만 근거).

**요청 흐름(후속)**: 화면의 `visible_range` 를 가운데 칸 우선 순서로 큐에 넣고, 스크롤로 벗어난 요청은 취소, 같은 사진 중복 요청 합침, 동시 생성 2~3개(현재 `MAX_SLOTS=2` 와 같은 한도). 결과는 UI 스레드로 보내 해당 칸만 다시 그린다. 클라우드 전용 사진은 요청하지 않고 자리표시를 유지한다.

## 5. 설정·앱 연결 (다음 카드)

- `photoview_ui` = `'flutter'`(기본) | `'native'`. `docview_ui`·`outliner_ui`·`settings_ui` 와 같은 방식. 이번 카드는 키 **읽기 함수와 해석 규칙**(`settings::photoview_ui`, 시험 포함)만 있고 앱·Flutter·Python·`docs/settings_defaults.json` 은 변경하지 않았다.
- 릴리스 번들: `.github/workflows/build-windows-exe.yml` 에 `grid_core`·`photoview_win` 시험·빌드·`resources/photoview_win.exe` 복사·파일 목록 추가(다음 카드). 시험은 `cargo test --release --locked`, 캡처 시험은 `CARGO_BIN_EXE_photoview_win` 기준, 파일 비교 시험이 생기면 CRLF 정규화.

## 6. 위험

| 위험 | 대응 |
|---|---|
| 수만 장 정렬·검색 지연 | 문서뷰와 같은 구조(글 키 1회 생성·인덱스 목록). 50,000장 합성 캡처·시험 통과. 20만 장 이상은 미측정 |
| 썸네일이 없는 상태를 완성으로 오인 | 자리표시를 사진 모양으로 명시, 결과 보고서에 "썸네일 미구현" 기재 |
| 연월 묶음에서 가상화 계산이 복잡해짐(행 높이 다름) | 3단계에서 "행 종류(헤더/사진 줄) 목록 + 누적 높이" 로 확장하고 시험을 먼저 쓴다 |
| 즐겨찾기·태그 DB 동시 쓰기 | 소유자 하나(§3) |
| 셸 썸네일의 클라우드 내려받기 유발 | `cloud` 참이면 호출 금지(시험으로 고정) |
| HEIC 코덱 없는 PC | 자리표시 유지 + 안내 한 줄. HEIC 는 `HEIF Image Extensions` 필요 여부를 사용자 확인 목록에 추가 |
| 문서뷰 공용 부품 변경이 문서뷰를 깨뜨림 | 문서뷰 시험(37개)을 `grid_core` 이동 뒤에도 그대로 통과시킴(결과 보고서) |
| Flutter 와의 차이 | 검색·정렬·수정일·확장자 배지는 Flutter 에 없던 표시. 사용자 실기 확인 전 기본 전환 금지 |

## 7. 단계 계획

| 단계 | 내용 | 상태 |
|---|---|---|
| 1 (이 카드) | `grid_core` 공용 크레이트 + `photoview_win`: 격자·가상화·선택·키보드·슬라이더(읽기)·검색·정렬·스캔 진행·취소·캡처 모드·시험 | 구현 완료, 실기 대기 |
| 2 | 제외 폴더 관리·슬라이더 저장(`docfit_config`), 더블클릭 열기, IME 입력, 저장본 즉시 표시, **즐겨찾기·태그(DB 소유자 결정)**, 오른쪽 버튼 메뉴 | 예정 |
| 3 | **셸 썸네일 API 측정(§4 D)** → 실제 썸네일 공급, 연월 묶음 격자(수정일 → 보이는 사진 EXIF 보충), 다중 선택·일괄 내보내기, 엔진 service 경로 | 예정 |
| 4 | 사진 뷰어·슬라이드쇼, 사본 편집, 사진대지 HWPX, 중복 검사 결과 보기(DUPRS1 후), 카메라별·위치별 묶음 | 예정 |
| 5 | 앱 연결: `photoview_ui`·릴리스 번들·CI·Flutter 게이트(기본 flutter → 실기 확인 후 native → Flutter 화면·Python 제거) | 다음 카드 |

## 8. 문서뷰와의 공용화 결과 (`grid_core`)

문서뷰(`docview_win`)에서 아래를 새 공용 크레이트 `grid_core` 로 옮겼고, 문서뷰는 같은 경로로 재노출해 **기존 시험 37개를 고치지 않고 통과**한다.

| 옮긴 것 | `grid_core` 모듈 | 문서뷰 쪽 |
|---|---|---|
| JSON 읽기 파서 | `json` | `docview_win::json` = 재노출 |
| PNG 인코더 | `png` | `docview_win::png` = 재노출 |
| 격자 배치·가상화·스크롤바·키보드 이동·도구줄 | `layout`(`GridSpec::DOC`/`PHOTO` 로 간격·비율만 구분) | `docview_win::layout` = 재노출, `Metrics::new` = 문서뷰 규칙 |
| 문서 항목·검색·정렬(`Doc`·`Collection`·`SortKey`)·klib 줄 파서 | `items` | `model` 이 재노출(이름 `Doc` 유지) |
| 표시 글자·색 섞기 | `fmt` | 재노출 |
| 스캔 상태·키 이동 | `state` | 재노출 |
| 화면 상태 `App` | `app`(`with_spec`) | `model::App` = 재노출(`App::new` = 문서뷰 규칙) |
| 스캔 작업(자식 프로세스 읽기·배치·취소·종료 코드)·klib 찾기·인자 | `scan`(`ScanSpec<T>`) | `scan::ScanJob` 은 공용 `ScanJob<Doc>` 를 감싼 얇은 형 |
| GDI 도우미(칸·글·선·단추·글꼴)·메모리 DC 캡처·시간대·고정 드라이브 | `gdi` | `paint`·`capture`·`sys` 가 사용/재노출 |
| 썸네일 슬라이더 범위·고정 | `thumb` | `settings` 가 재노출 |

**복사(중복)로 남긴 것**(옮기면 문서뷰 시험·구조를 크게 흔들어 이번엔 복사하고 여기 기록):

| 중복 | 문서뷰 | 사진뷰 | 이유·후속 |
|---|---|---|---|
| `window.rs`(Win32 창·메시지·입력·스캔 연결, 약 540줄) | `docview_win/src/window.rs` | `photoview_win/src/window.rs` | 거의 같다. 다른 곳: 창 이름·`new_app`·`GridSpec::PHOTO`·합성 목록. 공용 `grid_window` 로 뺄 후보(2단계에서 IME 입력 도입 때 함께) |
| `paint.rs`의 도구줄·스캔 띠·빈 화면·상태줄 | `docview_win/src/paint.rs` | `photoview_win/src/paint.rs` | 글만 다름("문서"↔"사진"). 카드 그리기(`draw_card`)만 진짜 다름. 글 표를 인자로 받는 공용 `draw_chrome` 으로 뺄 후보 |
| `main.rs`·`capture.rs` 인자 처리·장면 | `docview_win/src/{main,capture}.rs` | 동일 구조 | 파일 이름 접두(`docview_`/`photoview_`)·환경변수(`DOCVIEW_WIN_OUT`/`PHOTOVIEW_WIN_OUT`)만 다름 |
| `settings.rs`·`synth.rs`(합성 목록·장면) | 문서뷰 키·문서 이름 | 사진뷰 키·사진 이름 | 키·기본값이 다름. `settings_path`·제외 폴더 읽기·`to_json_line` 은 같아 `grid_core::settings` 로 뺄 후보 |
| 시험 일부(스캔 작업·취소·가짜 klib 배치 파일·설정 읽기) | `docview_win/tests/pure_tests.rs` | `photoview_win/tests/pure_tests.rs` | 같은 논리를 사진 규칙으로 시험. 공용 시험 모음으로 뺄 후보 |
