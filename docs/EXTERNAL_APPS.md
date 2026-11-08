# 개발자 모드 외부 프로그램 — 사진첩·아웃라이너·사진관리

작성 2026-11-08. 근거: 사용자 요청(2026-11-07) "개발자 모드 사이드메뉴에 별도 프로그램 3개 연결", TODO.md 사진첩 항목.

## 1. 대상과 결정 사항

| 메뉴(아이콘) | app_id | 포크(haijun93) | 원본 | 라이선스 | 실행 파일 |
| --- | --- | --- | --- | --- | --- |
| 사진첩 (`photo_library_outlined`) | `photo_album` | [mongdangview](https://github.com/haijun93/mongdangview) | [neelabo/NeeView](https://github.com/neelabo/NeeView) | MIT. 동봉 부품 7-Zip(7z.dll)·SevenZipSharp는 LGPL | `NeeView.exe` |
| 아웃라이너 (`account_tree_outlined`) | `outliner` | [mongdangoutliner](https://github.com/haijun93/mongdangoutliner) | [logseq/logseq](https://github.com/logseq/logseq) | AGPL-3.0 | `Logseq.exe` |
| 사진관리 (`photo_camera_back_outlined`) | `photo_manager` | [mondangpics](https://github.com/haijun93/mondangpics) | [KDE/digikam](https://invent.kde.org/graphics/digikam) | GPL-2.0(이후 판 포함) | `digikam.exe` |

- **이식하지 않는다.** 세 프로그램은 본 앱과 **별도 프로세스**로 실행되는 독립 프로그램이다. 본 앱은 실행 파일을 찾아 띄우기만 한다(링크·코드 결합·내장 창 없음).
- **개발자 모드 전용.** 사이드메뉴 항목은 개발자 모드에서만 보이고, Python 브리지(`DesktopWebBridge.external_app`)와 네이티브 백엔드(`ApplicationService.dispatch('external_app', …)`)도 개발자 모드가 아니면 `DEVELOPER_MODE_REQUIRED`로 거절한다.
- 포크 저장소 이름이 바뀌었다(옛 이름 haijun93/NeeView → **haijun93/mongdangview**). 코드·문서의 주소는 새 이름만 쓴다.

## 2. 공용 실행기 (1단계 구현)

### 2.1 Python — `docfit_app/external_apps_service.py`

- 등록부 `APPS`(위 표)가 유일한 기준이다. 새 프로그램을 붙일 때는 여기와 Flutter `kExternalApps`에 한 줄씩 추가한다.
- 명령: `external_app(action, {"app": app_id, ...})`
  - `list`: 세 프로그램의 이름·라이선스·포크/공식 URL·설치 여부.
  - `status`: 실행 파일 위치·출처(`bundled`/`custom`)·지정 경로·설치 안내.
  - `launch`: `subprocess.Popen([실행 파일], cwd=그 폴더, shell=False, DETACHED_PROCESS|CREATE_NEW_PROCESS_GROUP)`. 인자는 실행 파일 하나뿐인 **목록**이고, `default_popen`은 문자열 명령을 거절한다.
  - `choose`: 네이티브 파일 선택 창(용도 `external_app_exe`, `.exe`만)으로 고른 경로를 검증한 뒤 설정 `external_app_paths[app_id]`에 저장한다. 화면(웹뷰)에서 경로 문자열은 받지 않는다.
  - `clear`: 그 프로그램의 지정 경로만 지운다.
  - `open_download`: `{"target": "fork"|"official"}` — 등록부의 고정 URL만 연다(화면이 보낸 URL은 무시).
- 실행 파일 찾는 순서
  1. 앱 폴더의 `tools\<포크 이름>\<실행 파일>` — 예: `tools\mongdangview\NeeView.exe`, `tools\mongdangoutliner\Logseq.exe`, `tools\mondangpics\digikam.exe`. 앱 폴더 후보는 배포 exe 폴더 → PyInstaller 추출 폴더 → 소스 저장소 루트.
  2. 없으면 사용자가 지정한 경로(설정 `external_app_paths`).
  3. 둘 다 없으면 `APP_MISSING` + 설치 안내(포크 저장소·원본 공식 배포처 링크).
- 경로 검증 `validate_exe(app, path)`
  - 로컬 절대 경로만. UNC·장치 경로·대체 데이터 스트림(`:`) 거절.
  - 파일 이름은 **그 프로그램의 실행 파일 이름만**(대소문자 무시). 사진관리 칸에 `NeeView.exe`를 지정하는 식의 엇갈림도 거절 — 임의 프로그램을 띄우는 통로가 되지 않게.
  - 경로 구성요소에 심볼릭 링크·정션이 있으면 거절. 실제 파일이어야 한다.
- 시험: `tests/test_external_apps_service.py`.

### 2.2 Flutter — `flutter_ui/lib/features/external_apps/external_app_launcher.dart`

- `kExternalApps`(id·이름·아이콘)와 공용 진입점 `openExternalApp(context, api, app)`.
- 사이드메뉴(`WorkspaceTabBar`)는 노트북 아래에 세 항목을 `_TabPill`로 그린다(키 `external-app-<id>`). 다른 메뉴와 같은 위젯이라 펼침·접힘·사이드메뉴 모드에서 아이콘 세로줄이 맞는다(시험으로 확인).
- 누르면 실행만 요청하고 실행창(오른쪽 화면)은 바꾸지 않는다. 마우스 올림 자동 호출도 하지 않는다.
- 실행 파일이 없으면 '<이름> 준비' 창: 설치 안내 + [포크 저장소] [공식 배포처] [실행 파일 지정] [닫기]. 지정에 성공하면 바로 실행한다.
- 시험: `flutter_ui/test/external_apps_sidebar_test.dart`.

## 3. 라이선스 준수

### 3.1 공통 원칙

- **별도 프로그램이므로 결합 저작물이 아니다.** 본 앱은 세 프로그램을 링크하거나 코드를 섞지 않고, 운영체제의 프로세스 실행(인자 목록)으로만 띄운다. 서로 주고받는 것은 "실행" 한 가지이고 내부 자료구조를 공유하지 않는다. GPL FAQ의 "aggregate(단순 집합)"에 해당하므로 AGPL·GPL 조건이 본 앱 코드에 번지지 않는다.
  - 이 판단은 지금 연결 방식(별도 실행)에 한해 유효하다. 창 내장·라이브러리 링크·내부 API 결합으로 바꾸면 다시 검토해야 한다.
- 원 저작권·라이선스 고지는 그대로 둔다(포크에서도 지우거나 바꾸지 않는다).
- 배포본(2단계)에 함께 넣을 때는 각 프로그램의 **라이선스 전문과 대응 소스 위치**를 같은 폴더에 동봉하고, 본 저장소 `THIRD_PARTY_NOTICES.md`와 앱 '정보' 화면 고지에 항목을 추가한다.

### 3.2 프로그램별

| 프로그램 | 수정할 때 | 배포할 때 동봉 |
| --- | --- | --- |
| mongdangview (MIT) | 소스 공개 의무는 없지만 포크에 공개해 둔다. LGPL 부품(7z.dll, SevenZipSharp)은 수정하면 그 부분 소스를 공개 | `LICENSE.md`, `THIRDPARTY_LICENSES.md`, `SOURCE.txt`(포크 커밋 URL·LGPL 안내). LGPL 부품은 교체 가능한 별도 DLL로 둔다 |
| 아웃라이너 (AGPL-3.0) | **수정하면 반드시 포크(haijun93/mongdangoutliner)에 소스 공개.** AGPL 제13조: 네트워크로 사용자에게 서비스하는 경우에도 소스 제공 의무 — Logseq 동기화 서버를 우리가 운영할 계획은 없다 | `LICENSE.md`(AGPL 전문), 대응 소스 위치(수정 없으면 원본 태그, 수정하면 포크 커밋 URL), 원 저작권 고지 |
| 사진관리 (GPL-2.0) | **수정하면 반드시 포크(haijun93/mondangpics)에 소스 공개.** 바이너리를 배포하는 한 대응 소스(빌드 스크립트 포함)를 3년간 제공하거나 함께 제공 | `COPYING`(GPL-2.0 전문), 동봉 라이브러리 고지(digiKam 배포본에 포함된 것 그대로), 대응 소스 위치(원본 태그 또는 포크 커밋 URL) |

- 현재(1단계) 포크 코드 변경: mongdangview는 빌드 워크플로만 추가(프로그램 코드 변경 없음). mongdangoutliner·mondangpics는 변경 없음.
- 원본 공식 바이너리를 **수정 없이** 재배포할 때도 GPL/AGPL은 대응 소스 제공 의무가 있다.
  - "원본 배포처의 소스 안내를 그대로 전달"(GPL-2.0 제3조 c)은 비상업적 배포에만 허용되므로 기대지 않는다.
  - 대신 바이너리와 **같은 곳(GitHub)에서 소스를 받을 수 있게** 포크에 같은 판의 태그를 유지하고, 그 URL을 `SOURCE.txt`에 적어 동봉한다(GPL-2.0 제3조 끝 문단, AGPL-3.0 제6조 d).

## 4. 2단계 설계 — 배포본에 포함

### 4.1 공통 구조

1. 본 저장소에 고정 파일 `resources/external_apps.lock.json`을 둔다. 프로그램마다 다음을 적는다.
   - 받는 곳(URL), 파일 이름, **SHA256**, 판(버전·태그), 대응 소스 위치(커밋/태그 URL), 압축 안 실행 파일 상대 경로.
   - 판을 올리는 일은 사람이 이 파일을 갱신하는 커밋으로만 한다(자동 최신판 추적 금지).
2. 본 저장소 `build-windows-exe.yml`에 단계 추가(kagent 빌드 다음).
   - lock 파일의 URL에서 내려받고 SHA256을 대조한다. 다르면 빌드 실패.
   - 압축 해제 경로 탈출(`..`, 절대 경로, 링크) 검사.
   - 실행 파일·라이선스 파일·`SOURCE.txt`가 있는지 확인(빠지면 실패).
3. 배치: **첫 실행 설치(권고)** — 압축 파일을 exe에 넣고, 사용자가 처음 메뉴를 누를 때 `C:\HWP_AUTODOCFIT\tools\<포크 이름>\`(권한이 없으면 `%LOCALAPPDATA%\HwpAutoDocFit\tools\<포크 이름>\`)에 해시 확인 후 푼다. 공용 실행기의 앱 폴더 후보에 이 두 경로를 추가한다.
   - 장점: 앱 시작 속도에 영향 없음, 쓰지 않는 사용자 PC에는 풀지 않음.
   - 단점: 세 프로그램을 모두 넣으면 배포 exe가 수백 MB가 된다(아래 크기). **권고: 사진첩만 동봉하고, 아웃라이너·사진관리는 '내려받아 설치' 단추(앱이 lock 파일의 URL에서 받아 해시 확인 후 푼다)로 시작.**
4. 보안 검사: 내려받은 바이너리는 Gitleaks·Semgrep·OSV 대상에서 제외하고 lock 파일만 검사한다. 세 프로그램 exe는 우리 서명 정책(본 앱 exe만 서명) 밖이므로 서명하지 않고 해시 대조로 무결성을 확인한다.
5. 시험: lock 파일 형식 시험, 해시 불일치·경로 탈출 거절 시험(작은 가짜 압축), Windows 실기에서 메뉴 → 창 뜨는지 확인.

### 4.2 프로그램별 받는 방식

| 프로그램 | 방식 | 근거·크기(2026-11-08 확인) |
| --- | --- | --- |
| 사진첩 | **포크 Actions 휴대용 zip.** 포크 `docfit-portable.yml`을 수동 실행(`release=true`) → 릴리스 `docfit-v<버전>`의 자체 포함 x64 zip과 `SHA256SUMS.txt`. zip 안 `SOURCE.txt`의 커밋 SHA도 lock 파일과 대조 | 2026-11-07 push 실행(42379881257) 성공, 아티팩트(자체 포함+fd zip 2개) 합계 약 124MB. 자세한 빌드 기록: `docs/PHOTO_NEEVIEW.md` |
| 아웃라이너 | **원본 공식 Windows 배포본을 해시 확인 후 받기.** logseq/logseq GitHub 릴리스의 `Logseq-win-x64-<판>.zip`(휴대용 zip). GitHub가 자산마다 `sha256:` digest를 주므로 lock 파일에 그 값을 고정 | 최신 2.0.2: zip 약 196MB(sha256 3332383c…57db), NSIS 설치본 약 153MB. Electron+ClojureScript 빌드라 포크에서 자체 빌드할 이유가 없다(수정 전까지) |
| 사진관리 | **원본 공식 Windows 배포본을 해시 확인 후 받기.** download.kde.org `stable/digikam/<판>/digiKam-<판>-Qt6-Win64.tar.xz`(설치 없이 푸는 묶음) 또는 `.exe`(설치본). 미러의 `.sha256`과 KDE 서명 `.sig`(GPG)를 모두 확인 | 최신 9.1.0 확인. digiKam Windows 빌드는 KDE Craft(MXE/Craft 툴체인, 수십 개 KDE·Qt 의존성)라 GitHub Actions에서 무겁고 오래 걸려(캐시 없이 수 시간) 포크 자체 빌드는 하지 않는다. 수정이 필요해지면 그때 KDE Craft 빌드를 별도 검토 |

- 아웃라이너·사진관리를 **수정하게 되면** 공식 배포본을 쓸 수 없으므로 포크에서 빌드해야 하고, 그 소스를 포크에 공개해야 한다(3.2).
- digiKam `.exe`는 NSIS 설치 프로그램이라 앱이 대신 실행하면 관리자 권한·설치 경로 문제가 생긴다. 앱이 직접 받는다면 `.tar.xz`를 `tools\mondangpics\`에 푸는 쪽을 권고(압축 안 실행 파일 상대 경로는 실물로 확인 필요).

### 4.3 남은 결정(사용자 확인 필요)

- 배포본에 무엇을 동봉할지(권고: 사진첩만, 나머지는 '내려받아 설치').
- 개발자 모드 해제 시 이미 실행 중인 외부 프로그램: 현재는 그대로 둔다(별도 프로세스이므로 종료하지 않음).
- 앱 연동 범위: 예) 자료함·타임머신 폴더를 사진첩/사진관리로 열기(실행 인자에 검증된 폴더 1개, 인자 목록 방식).
- 지정 경로를 바꾸거나 지우는 화면: 지금은 실행 파일이 없을 때 뜨는 준비 창에서만 지정할 수 있다(`clear` 명령은 있음). 설정 화면에 '외부 프로그램' 칸을 둘지 결정.
- 정식 공개 전환 기준(개발자 모드 졸업 조건).

## 5. 자동 내려받기·설치 코어 (feature/extdl, 2026-11-08)

사이드메뉴를 처음 눌렀을 때 실행 파일이 없으면 받아 설치하는 **Python 코어와 서비스 명령**이다(화면은 다음 카드). 코드: `docfit_app/external_apps_download.py`, 연결: `ExternalAppsService`. 시험: `tests/test_external_apps_download.py`(인터넷·한/글 COM 없이 주입식 opener).

### 5.1 서비스 명령
`external_app('install'|'cancel'|'progress'|'status', {"app": app_id 또는 포크 폴더 이름})`
- `install`: 실행 파일이 이미 있으면(앱 폴더 tools → 사용자 지정 → 내려받은 설치본) **건너뛴다**(`skipped: true`, 네트워크 접근 없음). 없으면 백그라운드 스레드로 시작하고 현재 상태를 돌려준다. 이미 돌고 있으면 `already_running`.
- `progress`: `install.phase`(starting·resolving·downloading·verifying·extracting·installing·installer_launched·done·error·cancelled), `downloaded`/`total`/`percent`, 실패 시 `code`/`message`.
- `cancel`: 받기·풀기·무인 설치 대기 중 취소(임시 파일은 지운다).
- `status`에 `downloadable`, `install`(진행 상태)이 추가되고 `source`에 `downloaded`가 생겼다. 찾는 순서: 앱 폴더 tools → 사용자 지정 경로 → 내려받은 설치본.

### 5.2 출처 목록(`SOURCES`, 데이터) — 앞에서부터 시도
| 앱 | 1순위 | 2순위 | 설치 방식 |
| --- | --- | --- | --- |
| 사진첩 | GitHub `haijun93/mongdangview` 최신 릴리스의 `.zip`(이름에 fd·framework 없는 것) | GitHub `neelabo/NeeView` 최신의 `NeeView*.zip` | 휴대용 zip 풀기 |
| 아웃라이너 | GitHub `logseq/logseq` 최신의 `Logseq-win-x64-*.zip` | 같은 릴리스의 `Logseq-win-x64-*.exe`(NSIS) | zip 풀기 / `설치exe /S /D=경로`(무인) |
| 사진관리 | download.kde.org `stable/digikam/` 목록에서 최신 판 폴더(최근 3개까지) → `digiKam-<판>-Qt6-Win64.exe` + 공개 `.sha256` | — | **설치 프로그램을 띄워 사용자가 진행**(`/D=경로`로 폴더만 미리 지정) |

- 사진관리를 무인(`/S`)으로 하지 않는 이유: digiKam Windows 설치본의 무인 옵션 문서를 확인하지 못했다. 확인되기 전까지는 검증한 설치 파일을 `tools\_installers\mondangpics\`에 두고 띄우기만 한다. `/D=`가 무시되면 사용자가 고른 폴더에 깔리고, 그 경우 앱은 "사용자 지정 경로(`choose`)"로 찾는다.
- 아웃라이너의 NSIS 무인 설치도 실기 확인 전이다(zip이 있으면 zip이 우선이라 거의 쓰이지 않는다).

### 5.3 받기 규칙
- **HTTPS만**, 사용자 정보(`user:pw@`)·443 외 포트 거절.
- **호스트 허용 목록**: `github.com`, `objects.githubusercontent.com`, `download.kde.org` + (요청에 없었지만 필요해 더한 것) `api.github.com`(릴리스 조회·`digest`), `release-assets.githubusercontent.com`(현행 GitHub 자산 리디렉션 도착지). **리디렉션은 단계마다 검사**하고 응답의 최종 주소도 다시 검사한다. 허용 목록 밖으로 가면 `HOST_BLOCKED`로 중단한다.
  - 한계: download.kde.org는 보통 미러로 리디렉션한다. 미러 호스트가 허용 목록에 없으면 실패하므로, 실기에서 막히면 사용자 지정 안내(`open_download`) 또는 목록 보강이 필요하다. 미러가 바뀌어도 아래 SHA-256 확인은 유지된다.
- 크기 상한: 출처마다(`Source.max_bytes`: 사진첩 400MB·아웃라이너 zip 800MB/exe 600MB·사진관리 1.5GB). `Content-Length`로 먼저, 받는 동안 누적으로 다시 확인. API/목록 응답은 5MB.
- 진행률 콜백, 취소 플래그(청크마다 확인), **이어받기 없음**: `<파일>.part`에 받고 끝까지 받아 길이가 맞을 때만 이름을 바꾼다. 실패·취소 시 `.part` 삭제.

### 5.4 무결성 — 공개 해시 대조, 없으면 TOFU
1. 출처가 공개한 SHA-256이 있으면(GitHub API 자산의 `digest: "sha256:…"`, KDE `<파일>.sha256`) 받은 파일의 계산값과 대조. 다르면 `HASH_MISMATCH`, **아무것도 설치하지 않는다**.
2. 공개 해시가 없으면(예: 오래된 릴리스) 계산값을 `tools\tofu.json`에 `{app_id: {파일이름: {sha256, first_seen}}}`으로 **처음 받은 값**을 기록하고, 같은 파일 이름을 다시 받을 때 값이 다르면 `HASH_MISMATCH`. 설치 기록 `.installed.json`에 `trust`: `published`/`tofu`와 해시·판을 남긴다.
3. **TOFU의 한계**: 처음 받은 파일이 이미 변조됐다면 알아채지 못한다. 새 판은 파일 이름이 달라 비교 대상이 없으므로(첫 기록만 한다) "같은 이름의 파일이 나중에 바뀐 경우"만 잡는다. 공개 해시가 있는 경우도 해시와 파일을 **같은 서버**(GitHub/KDE)에서 받으므로 그 서버·계정이 뚫리면 막지 못한다. KDE GPG 서명(`.sig`) 확인은 이번 범위 밖이다(§4.2의 `.sig` 대조는 미구현).
4. 서명되지 않은 외부 exe를 실행하는 것이므로 개발자 모드 전용 제한은 그대로 둔다.

### 5.5 설치 위치·풀기
- `%LOCALAPPDATA%\HWP_AutoDocFit\tools\<포크 폴더 이름>\` — 사용자 권한만 쓴다(관리자 권한 불필요).
- zip: `open_zip_safely`(항목 수·크기·압축률 상한, 중복·위험 이름 거절; 한도는 항목 5만·합계 4GB·파일 2GB)로 연 뒤 한 항목씩 풀며 **경로 탈출(`..`·절대·드라이브 문자)·심볼릭 링크 항목 거절**, 풀린 총량 재확인.
- 임시 폴더 `.stage-*`에 받고 풀어 실행 파일 존재를 확인한 **뒤에** 기존 폴더와 교체(교체 실패 시 되돌림). 실패하면 기존 설치는 그대로이고 임시 폴더는 지운다.
- 실행 파일 위치는 압축 안 최상위 폴더 유무와 무관하게 3단계 깊이까지 이름으로 찾아 `.installed.json`의 `exe_rel`에 기록한다.

### 5.6 남은 일
화면(준비 창의 [자동 설치] 단추·진행률 막대·취소), 실기(Windows) 확인 — 각 출처의 실제 자산 이름 패턴·KDE 미러 리디렉션·NSIS 옵션, KDE `.sig` 검증, 설치본 삭제/갱신 명령.
