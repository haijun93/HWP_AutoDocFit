# 탐색기 오른쪽 버튼 메뉴 '몽땅파일변환' 1단계 — 조사·설계 (CTXMENU1, 2026-11-21)

범위: 조사·설계 문서만(코드·레지스트리·시스템 설정 변경 없음). 근거는 `파일:줄` 또는 공식 문서 이름이고, 확인하지 못한 것은 **미확인**(§9에 번호로 모음)으로 적었다. 이번 세션에서 웹 재확인은 하지 않았다 — Windows 동작 설명은 '알려진 사실(Microsoft Learn 문서 기준)'로 표시하고 이 PC 실측이 아님을 밝힌다.
이 PC 읽기 전용 조회: Windows 11 DisplayVersion 26H2(`HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion`), `HKCU\Software\Classes\SystemFileAssociations\.hwp\shell`·`HKCU\Software\Classes\*\shell`·`HKCU\Software\Classes\Directory\shell` 아래 하위 키 0개(등록 없음).

## 0. 권장안 (한 줄)
**관리자 권한이 필요 없는 `HKCU\Software\Classes` 레지스트리 하위 메뉴(Win11은 '추가 옵션 표시' 안) + 작은 Rust 전달 실행 파일 `mdm-ctx.exe`(선택 경로를 모아 실행 중인 앱의 파이프로 보내거나 앱을 시작)로 1차를 만들고, `IExplorerCommand`+패키지 신원(Win11 새 메뉴)은 공공기관 PC 적합성이 낮아 2차 선택 과제로 보류한다.**

## 1. 먼저 알아야 할 현황: 우클릭 메뉴가 이미 있다
TODO 항목은 '신설'로 적혀 있으나 코드에 **'한글편집 후처리' 우클릭 메뉴가 이미 구현**되어 있다. 새 설계는 이를 대체·정리하는 문제와 함께 다뤄야 한다.

| 사실 | 근거 |
|---|---|
| HKCU `Software\Classes\SystemFileAssociations\<ext>\shell\HwpAutoDocFit`에 등록. 확장자 6개 `.hwp .hwpx .pdf .png .jpg .jpeg` | `docfit_app/shell_menu.py:6-8` |
| 상위 키 `SubCommands=''` + 하위 `shell\NN_<대상>` 키의 `MUIVerb`·`command`, `MultiSelectModel=Player` | `shell_menu.py:39-45`, `:72-81` |
| 항목: PDF로 변환·HWPX로 변환·PDF 하나로 합치기·PDF 압축·그림(PNG)으로 변환·앱으로 열기(확장자별로 다름) | `shell_menu.py:46-71` |
| 명령은 `"<launcher 또는 python+스크립트>" convert "%1" --to <형식> [--collect pdfm] --notify` — 앱 본체(파이썬)를 파일마다 새로 띄움 | `shell_menu.py:11-17`, `:76-80` |
| 여러 파일은 '모음 폴더'에 요청 파일을 남기고, 잠금(`O_EXCL`)을 잡은 첫 프로세스가 1.5초 조용할 때까지 모아 일괄 처리 | `docfit_app/shell_batch.py:39-56`, `:87-144`, `:224-241`, 기본값 `quiet_seconds=1.5` |
| 등록·해제는 설정 키 `shell_menu`(기본 False)를 켤 때만, 현재 사용자(HKCU)만 | `docfit_app/settings_schema.py:3`, `desktop_web_ui.py:1486-1491`, 안내문 `flutter_ui/lib/features/settings/setting_guides.dart:850-854`(Win11은 '더 많은 옵션 표시' 안내) |
| 앱 시작 때 등록된 경로가 낡았으면 갱신 | `shell_menu.py:129-133`, 호출 `hwp-auto-docfit.py:3412-3416` |
| 설치 프로그램은 위 6개 키를 `dontcreatekey uninsdeletekey`로 제거만 한다(등록은 안 함) | `installer/setup.iss:69-74` |
| 시험은 가짜 `winreg` 주입 방식 | `tests/test_shell_menu.py:4,51,84-96` |
| Rust 이전 목록에는 '셸 메뉴·배치 → Rust CLI, 규모 S'로 이미 올라 있음 | `docs/RUST_MIGRATION_INVENTORY_2026-11-19.md:126` |

**기존 구현의 한계(새 설계의 동기)**: ① 클릭마다 앱 본체(파이썬 런처·번들 import, 호스트 시작 import 894.7 ms 기록은 `RUST_MIGRATION_INVENTORY_2026-11-19.md:8`) 프로세스가 파일 수만큼 뜬다 ② 파일변환 **화면**을 열지 않고 창 없이 변환해 메시지 상자로 알린다(`shell_menu.py:76-80`, `docfit_app/cli.py:183-196`) ③ 확장자가 6개뿐이라 DOCX·XLSX·PPTX 등 파일변환 입력 형식이 빠졌다(§5).

## 2. 등록 방식 비교 (Windows 10/11)

표의 '알려진 사실'은 Microsoft Learn의 *Creating Shortcut Menu Handlers / Verbs and File Associations / Choosing a Static or Dynamic Shortcut Menu Method*, *Create packaged context menu handlers(Windows 11 shell extensions, IExplorerCommand)* 문서 기준이다(재확인 안 함). 이 PC 실측이 아니면 '실측 없음'.

| 항목 | (a) 레지스트리 정적 verb<br>`HKCU\Software\Classes\*\shell\<verb>`<br>`…\SystemFileAssociations\.<ext>\shell\<verb>`<br>`…\Directory\shell\<verb>` | (b) `IExplorerCommand` COM + 패키지 신원<br>(MSIX 또는 희소 패키지, Win11 새 메뉴) |
|---|---|---|
| Win11 노출 위치 | 알려진 사실: 옛 메뉴 = '추가 옵션 표시'(Shift+F10) 안. Win10은 첫 단계. **이 PC(26H2) 실측 없음 → 미확인 U1** | 알려진 사실: Win11 새(1단계) 메뉴에 표시. 패키지 신원이 있어야 새 메뉴에 올라감. Win10에서는 해당 없음(옛 방식으로 따로 등록) |
| 관리자 권한 | 불필요(HKCU 쓰기). 기존 구현이 이미 HKCU만 씀(`shell_menu.py:88`) | 알려진 사실: 희소 패키지 등록은 사용자 단위로 가능하나 **서명 인증서를 신뢰시키는 단계가 일반적으로 관리자 권한을 요구**하고, 개발자 모드·사이드로드 정책에 따라 막힐 수 있음. 현장 정책은 **미확인 U3** |
| 공공기관 PC(설치 제한) 적합성 | 높음 — 파일 복사 + 사용자 레지스트리 키뿐. 단 그룹 정책이 HKCU Classes 쓰기를 막는지는 **미확인 U7** | 낮음 — 서명 인증서·패키지 정책·AppLocker/WDAC 영향. TODO가 정한 '관리자 권한·패키지 설치 제한' 환경 원칙과 충돌 |
| 제거·되돌림 | 키 삭제로 끝(흔적 없음). 기존 `unregister()`(`shell_menu.py:93-113`)·Inno `uninsdeletekey`(`setup.iss:69-74`)가 선례 | 패키지 제거 + 인증서 정리 필요. 인증서를 시스템에 남길 위험 |
| 확장자 필터 | 키 단위(`SystemFileAssociations\.ext`) 또는 `*` + `AppliesTo`(AQS 질의). 기존은 확장자별 키 방식(`shell_menu.py:38-39`) | 매니페스트의 `ItemType`(확장자·`*`·`Directory`)과 `IExplorerCommand::GetState`에서 코드로 결정 |
| 여러 파일 선택 | `MultiSelectModel`: `Single`/`Player`/`Document` 3종이 알려져 있음. 선택 수 상한(15로 알려짐)·각 모델의 프로세스 수 **미확인 U2**. 설계는 최악(파일마다 프로세스)을 가정하고 §3 모으기로 흡수 | `IShellItemArray`로 선택 전체를 한 번에 전달 — 프로세스 모으기 불필요(알려진 사실) |
| 하위 메뉴 | 가능 — `SubCommands=''` + 중첩 `shell` 키(기존 구현이 사용, `shell_menu.py:41-44`, `:72-74`) 또는 `ExtendedSubCommandsKey`. Win11 '추가 옵션' 안에서의 표시는 U1 | 가능 — `IExplorerCommand::EnumSubCommands`(알려진 사실) |
| 성능(탐색기 지연) | 정적 키는 탐색기가 읽기만 함. 코드가 탐색기 안에 로드되지 않음(알려진 사실). 수치 실측 없음 | 알려진 사실: 확장 DLL/서버가 우클릭 때마다 호출됨(DLL이면 탐색기 안에서 실행 → 느리거나 비정상 종료 시 탐색기 영향). Rust로 만들 때의 크기·지연 **미확인 U4** |
| 코드 형태 | 실행 파일 1개(`mdm-ctx.exe`) — 앱 원칙(작은 Rust exe)과 부합 | COM DLL(또는 로컬 서버) + 패키지 + 서명 — 규모·유지비 큼 |

(c) 참고: 보내기(SendTo) 바로가기는 레지스트리를 안 쓰는 대안으로 이미 `--rename`에 쓰인다(`docfit_app/file_rename_service.py:536-561`, 전달 `docfit_app/instance_files.py:43-57`). 그러나 '보내기' 하위 메뉴(Win11은 추가 옵션 안)이고 파일 형식·하위 항목 선택이 안 돼 이번 목적에는 맞지 않는다.

**결론**: (a)를 채택. Win11에서 '추가 옵션 표시' 안에만 보이는 한계는 설정 안내문에 남기고(`setting_guides.dart:850-854` 문구 재사용), (b)는 (a)가 현장에서 검증된 뒤 '관리자·패키지가 허용되는 환경 전용 옵션'으로 따로 검토한다.

## 3. 우리 쪽 연결 지점과 `mdm-ctx.exe` 설계

### 3-1. 앱의 파일변환이 지금 시작되는 방식
- 화면: 탭 번호 `kTabFileConversion = 10`(`flutter_ui/lib/features/workspace/tab_navigation_shell.dart:16`), 사이드바 항목 '파일변환'(`flutter_ui/lib/features/workspace/sidebar_menu_settings.dart:129`), 화면 `ConversionWorkspace`(`flutter_ui/lib/app/conversion_tab.dart:25`).
- 목록 추가: 파일을 놓으면 놓기 대상이 `conversion`일 때 `receive_drop`이 확장자 필터 후 `add_paths`로 넣는다(`desktop_web_ui.py:3406-3416`). `add_paths`는 폴더면 `gui.폴더추가`, 파일이면 `gui.파일추가`를 부른다(`desktop_web_ui.py:876-916`). 이 목록은 문서 탭과 같은 `gui.files`다(`:3414`→`:913`).
- 결과 형식: `set_conversion_target(target)`이 `CONVERSION_OUTPUTS` 안에서만 허용(`desktop_web_ui.py:981-990`; `docfit_core/file_conversion.py:414`). 형식별 사용 가능 여부는 `_conversion_targets`가 계산(`desktop_web_ui.py:753-845`).
- 실행: `start_single_file_conversion`/`_start_single_conversion_snapshot` → `docfit_app/conversion_runner.start_conversion_job`(`desktop_web_ui.py:1001-1066`).
- **앱 밖에서 파일변환으로 보내는 선례가 이미 하나 있다**: 몽땅검색 '보내기'가 `_timemachine_send_to_feature("convert", paths)`로 `selected_mode='file_format'`을 켜고 `add_paths`를 부른다(`desktop_web_ui.py:2299-2320`). 단 `len(paths) > 1`이면 '한 번에 한 파일만'으로 거절한다(`:2304-2305`) — 여러 파일 우클릭에는 그대로 못 쓴다.
- 지금 우클릭 변환(`cli.py convert`)은 화면 없이 `run_conversion`을 직접 부른다(`docfit_app/cli.py:183-196`).

### 3-2. 앱으로 경로를 넘기는 기존 통로 3가지 (모두 같은 사용자 안에서만)
| 통로 | 방식 | 근거 | 이번 용도 평가 |
|---|---|---|---|
| ① 두 번째 실행 → 열기 요청 큐 | 앱이 이미 떠 있으면(Named Mutex `Global\HWP_AutoDocFit_SingleInstance_Mutex`) 인자를 `%LOCALAPPDATA%\HWP_AutoDocFit\open_requests\*.json`에 원자적으로 쓰고 종료, 앱이 0.5초마다 읽음 | `hwp-auto-docfit.py:3305-3366`, `docfit_app/instance_files.py:12-27`, `:60-80` | 느림(앱 본체가 먼저 시작돼야 함) — **앱이 꺼져 있을 때의 시작 경로**로만 사용 |
| ② `--rename` 인자 → `_rename_startup_paths` → `get_state`가 `rename_open`을 돌려주면 화면이 탭을 연다 | `docfit_app/instance_files.py:43-57`, `desktop_web_ui.py:580`, `:654`, `:2655-2675`, `flutter_ui/lib/main.dart:785`(`onRenameOpen`) | **`--convert` 인자의 본보기**: 같은 구조로 `convert_open`·`--convert [--to <형식>]`을 추가 |
| ③ 앱 서비스 파이프(앱=서버, kagent=클라이언트) | `\\.\pipe\HwpAutoDocFit.appsvc.<SID>`, 한 줄 JSON 요청·응답, 현재 사용자만(`PIPE_REJECT_REMOTE_CLIENTS`), 요청 `cmd`: `ping`·`open_photo_edit`·`xl_target` | 서버 `docfit_app/kagent_app_service.py:23`, `:96-99`, `:182-189`; 명령 분기 `:43-70`; 앱이 시작 때 기동·콜백 등록 `desktop_web_ui.py:4299-4302`; Rust 클라이언트 선례 `kagent/src/xl_target.rs:115` | **실행 중 앱으로 보내는 주 통로**: `cmd:"convert_open"`(`paths`, `target`)을 추가하고 `set_photo_edit_handler`와 같은 방식의 콜백을 등록 |

### 3-3. `mdm-ctx.exe` 동작(가칭, 창 없는 Rust exe)
1. 클릭 한 번이 만든 프로세스(1~N개)가 각자 `argv`의 경로(`"%1"` 하나, 또는 `%*`가 되는 모델이면 여러 개)를 **스풀 폴더**(`%LOCALAPPDATA%\HWP_AutoDocFit\ctx_spool\`)에 `.tmp`→`.req`로 원자적 기록(`shell_batch.write_request`·`instance_files.forward`와 같은 규약, `shell_batch.py:39-56`).
2. `Local\HwpAutoDocFit.ctx.<SID>` 이름 있는 뮤텍스를 잡은 **한 프로세스만 대표**가 된다(kvf의 `CreateMutexW`+`ERROR_ALREADY_EXISTS` 패턴: `kvf/src/main.rs:89-95`). 나머지는 스풀에 쓰고 바로 종료(기존 추종자 방식 `shell_batch.py:222-226`과 같음).
3. 대표는 새 요청이 일정 시간(설계값 0.3초, 상한 10초·500개; 기존 파이썬은 1.5초·30초·200개 `shell_batch.py:224-226`) 조용해질 때까지 모은 뒤 **중복 제거·자연 정렬·확장자 검사**(`shell_batch.py:18-21`, `:158-206`).
4. 앱 서비스 파이프에 연결해 `{"cmd":"convert_open","paths":[…],"target":"<형식>"}`를 한 줄로 보낸다. 파이프가 **없으면**(앱 꺼짐) 경로를 열기 요청 큐 형식(`instance_files.py:19-24`에 `convert` 필드 추가)으로 쓰고 `HWP_AutoDocFit.exe --convert --to <형식>`을 시작한다(명령줄 길이 32,767자 한계를 피하려 경로는 인자가 아니라 큐 파일에 담는다).
5. 앱은 `gui.selected_mode='file_format'`·`conversion_target`를 정하고 `add_paths`로 목록에 넣은 뒤 `get_state`에 `convert_open:true`를 실어 화면이 파일변환 탭(10번)을 열게 한다(②와 같은 구조).
6. 앱이 변환을 **자동으로 시작하지는 않는다** — 사용자가 화면에서 확인 후 시작(원본 보호·실수 방지). 자동 시작 여부는 결정 항목 D2.
7. 대표는 파이프 응답(또는 앱 시작 요청)을 받은 뒤 스풀을 지우고 종료. 로그는 파일 한 줄(`ctx.log`)만.

**경로 규약**: UTF-16 그대로 다루고(한글·공백 경로), 파이프 JSON은 UTF-8(앱 서비스 규약 `kagent_app_service.py:80-90`와 동일). 심볼릭 링크·존재하지 않는 경로·지원 밖 확장자는 대표가 거른다(앱도 한 번 더 거른다: `desktop_web_ui.py:900-912`).

### 3-4. 책임·크기·시작 시간 목표
- 책임: ①경로 수집·모으기 ②파이프 전송 또는 앱 시작 ③`--register/--unregister/--status/--refresh`(레지스트리 쓰기, §4). 변환·화면·파일 열기는 하지 않는다. 의존성은 `windows-sys`만(kvf `Cargo.toml`과 동일한 `opt-level=3, lto, codegen-units=1, panic=abort, strip`), 콘솔 창이 깜빡이지 않도록 `windows_subsystem="windows"`(`kvf/src/main.rs:1`).
- 크기 목표 **≤ 400 KB**, 클릭→경로 전달 완료(앱 실행 중) **≤ 100 ms 목표**. 기존 Rust exe 크기: kvf 0.59 MB, ktm 2.20 MB, kagent 2.44 MB(`RUST_MIGRATION_INVENTORY_2026-11-19.md:64-72`). 두 수치는 **목표일 뿐 실측 없음 → 미확인 U5**(이 저장소에는 빌드된 kvf·ktm·kagent exe도 없어 비교 측정을 못 했다).

## 4. 설치·제거·갱신 설계

### 4-1. 지금의 설치 프로그램 구조 (`installer/setup.iss` 읽은 결과)
- `[Setup]`: `PrivilegesRequired=lowest`, 설치 폴더 `{localappdata}\Programs\HWP_AutoDocFit`(`:9-11`) — 이미 관리자 권한이 필요 없다.
- `[Tasks]`: 바탕 화면 바로 가기 1개(`unchecked`)(`:34-35`) — **설치 중 선택 항목의 선례**.
- `[Run]`: `ktm.exe --install --respect-disabled`, `kvf.exe --install --respect-disabled`(`:44-51`) — 상주 프로그램 설치형 호출, 사용자가 설정에서 끈 경우(`*.disabled` 표식)는 켜지 않음(`kvf/src/install.rs:231-262`).
- `[UninstallRun]`: `ktm.exe --uninstall`, `kvf.exe --uninstall`(`:57-59`). `[UninstallDelete]`: `{app}`·색인(`:61-66`).
- `[Registry]`: 우클릭 키 6개를 `dontcreatekey uninsdeletekey`로 제거만(`:69-74`).
- `[Code]`: `PrepareToInstall`에서 상주 도우미 종료(`StopHelper` `:81`, 호출 `:92-100`), `CurUninstallStepChanged`에서 관리자 서비스 해제(`:105-118`).

### 4-2. 새 설계
| 항목 | 설계 | 선례 |
|---|---|---|
| 실행 파일 위치 | `{app}\_internal\resources\mdm-ctx.exe`(꾸러미에 포함, kvf·ktm와 같은 위치). 레지스트리 명령은 이 절대 경로를 가리킨다. 이 exe는 상주하지 않아 갱신 때 파일 잠김 위험이 작다 — 그래도 `PrepareToInstall`(`setup.iss:92`부터)의 `StopHelper('mdm-ctx.exe')`를 한 줄 추가 | `setup.iss:44-51`, `:81-100` |
| 레지스트리 형태 | `SystemFileAssociations\.<ext>\shell\MongttangFileConvert`(상위 `MUIVerb=몽땅파일변환`, `Icon`, `SubCommands=''`, `MultiSelectModel=Player`) + 하위 `shell\NN_<대상>\command` + `Directory\shell\MongttangFileConvert`(폴더). `*\shell`+`AppliesTo`는 쓰지 않음(질의 평가 비용, 확장자별 키가 선례) | `shell_menu.py:35-82` |
| 켜기/끄기 | `mdm-ctx.exe --register|--unregister|--status|--refresh`가 한 줄 JSON을 출력(kvf의 `lifecycle_command` 규약: `kvf/src/main.rs:70-80`). 앱 설정 키는 기존 `shell_menu`(탐색기 우클릭 메뉴)를 **재사용**해 값만 mdm-ctx 호출로 바꾼다(`desktop_web_ui.py:1448-1450`, `:1486-1491`). 레지스트리 쓰기는 사용자가 설정에서 켠 때만 — 앱 시작 때 `--refresh`는 **이미 등록된 경우에만** 경로를 고친다(`shell_menu.py:129-133` 동작 유지) | 위 |
| 설치 중 선택 | `[Tasks]`에 `ctxmenu`(탐색기 오른쪽 버튼 메뉴 '몽땅파일변환' 등록) **기본 해제(unchecked)**, 체크했을 때만 `[Run]`에서 `mdm-ctx.exe --register`(조용한 설치에서는 `Check: WizardSilent` 아님 → 체크 상태 그대로). 체크 = 사용자가 직접 켠 것으로 본다(결정 D1) | `setup.iss:34-35` |
| 제거 | `[UninstallRun]`에서 `mdm-ctx.exe --unregister`를 **파일 삭제 전에** 실행 + 안전망으로 `[Registry]`에 새 키 이름의 `dontcreatekey uninsdeletekey` 행을 확장자 수만큼 추가. 기존 `HwpAutoDocFit` 키 정리 행(`setup.iss:69-74`)은 유지(예전 버전 흔적 제거) | `setup.iss:53-59`, `:69-74` |
| 업데이트 때 경로 갱신 | 설치 위치가 같으면 경로가 안 바뀐다. 바뀔 때를 위해 ① `[Run]`에 `--refresh`(등록돼 있을 때만 갱신) ② 앱 시작 때 `--refresh`(기존 `refresh_registered` 자리). 등록 값 형식이 달라졌으면(예: 하위 항목 추가) `--refresh`가 키를 다시 쓴다 | `hwp-auto-docfit.py:3412-3416` |
| 기존 '한글편집 후처리' 메뉴 | **결정 D3**: 권장 = 새 메뉴가 대체. 같은 설정 키를 쓰고, `--register`가 옛 `HwpAutoDocFit` 키를 먼저 지운다(우클릭 메뉴에 항목이 둘 보이는 혼란 방지). 옛 항목 중 앱 열기·PDF 합치기는 새 하위 메뉴로 흡수 | `shell_menu.py:46-71` |

## 5. 지원 확장자·폴더·하위 메뉴

### 5-1. 파일변환 입력 형식(코드 확인)
`CONVERSION_INPUTS = SUPPORTED_INPUT_EXTENSIONS | TEXT_INPUTS | IMAGE_INPUT_EXTENSIONS | HTML_INPUTS`(`docfit_core/file_conversion.py:411-413`):
- 문서·그림(변환 엔진 입력, 12개): `.hwp .hwpx .hml .pdf .xls .xlsx .docx .pptx .png .jpg .jpeg .webp`(`docfit_core/kordoc_bridge.py:37-40`)
- 그림 추가분: `.bmp .tif .tiff .gif`(`file_conversion.py:14`에 8개 그림 형식 전체)
- 글자·웹: `.txt .md .html .htm`(`file_conversion.py:411-412`)
- 합계 **20개**(이 저장소에서 `len(CONVERSION_INPUTS)` 실행 결과 20): `.bmp .docx .gif .hml .htm .html .hwp .hwpx .jpeg .jpg .md .pdf .png .pptx .tif .tiff .txt .webp .xls .xlsx`. 구현 카드에서 이 수를 시험으로 고정한다.
- 출력(`CONVERSION_OUTPUTS`): `hwpx pdf pdfz pdfm pdfi docx epub html jpg png txt md`(`file_conversion.py:414`).
- 주의: 레거시 목록 `지원_확장자`(`hwp-auto-docfit.py:3111-3112`)에는 `.doc`이 있으나 `CONVERSION_INPUTS`에는 없다(`.doc`은 `add_paths`만 통과, `desktop_web_ui.py:907-909`) — 메뉴 대상은 `CONVERSION_INPUTS` 기준으로 하고 `.doc`은 제외한다.
- 우클릭 메뉴 확장자 목록은 **코드에 복제하지 않고** `mdm-ctx.exe`가 갖는 상수 + 시험(`CONVERSION_INPUTS`와 같은 집합인지 대조)으로 고정한다(단일 출처 어긋남 방지).

### 5-2. 폴더 선택 때 동작
- 앱의 `폴더추가`는 **바로 아래 파일만**(비재귀), 지원 확장자만 넣는다(`docfit_ui/legacy.py:4328-4342`). 하위 폴더는 들어가지 않는다.
- 놓기 경로의 `conversion` 필터는 확장자로만 걸러 **폴더를 버린다**(`desktop_web_ui.py:3406-3410`, 폴더의 `suffix`가 비어 있음) — 그러므로 우클릭 전달은 `receive_drop`이 아니라 `add_paths`(폴더 처리 포함)를 직접 호출한다.
- 설계: `Directory\shell`에 같은 메뉴를 달고, 폴더는 `mdm-ctx`가 **펼치지 않고 그대로 전달**, 앱의 `폴더추가` 동작(바로 아래 파일만)을 따른다. 하위 폴더까지 넣을지는 결정 D4(권장: 1단계는 현행 유지). 폴더 안 파일 수가 많을 때의 상한은 `add_paths`의 1,000개 제한(`desktop_web_ui.py:879`)을 따른다.

### 5-3. 하위 메뉴 항목 후보와 기존 기능 대응
| 하위 항목 | `--to` 값 | 기존 파일변환 기능 | 보이는 조건 |
|---|---|---|---|
| PDF로 | `pdf` | 형식 'PDF' (`desktop_web_ui.py:819` `{"format":"pdf"}`; HWP·HWPX는 한/글 PDF 저장, 그림은 Pillow, 그 밖은 변환 엔진) | 입력 전부 `CONVERSION_INPUTS` |
| PDF 하나로 합치기 | `pdfm` | 'PDF 하나로 합치기' — 2개 이상일 때만 enabled(`desktop_web_ui.py:821-826`의 `pdfm`) | 파일 2개 이상(여러 선택) |
| PDF 압축 | `pdfz` | 'PDF 압축' — PDF에만(`file_conversion.py:80-84`, `desktop_web_ui.py` 형식 목록) | `.pdf` |
| 이미지 PDF | `pdfi` | '이미지 PDF'(쪽을 그림으로 렌더링한 PDF, `file_conversion.py:261`·`:85-86`) | 입력 전부 `CONVERSION_INPUTS` |
| HWPX로 | `hwpx` | 형식 'HWPX'(HWP는 한/글 저장, 그 밖은 변환 엔진) | `.hwp` 등(PDF→HWPX 등은 앱이 enabled로 판단) |
| 파일변환 열기… | (미지정) | 화면만 열고 형식은 사용자가 고름 | 모든 대상 |

- 하위 메뉴는 **조건부 표시를 레지스트리로 못 한다**(정적 verb는 확장자 단위만) — 확장자 키마다 해당 항목만 둔다(예: PDF 압축은 `.pdf` 키에만). `pdfm`처럼 파일 수에 따른 조건은 앱이 enabled를 판단하고 안 되면 안내 문구를 보인다(`desktop_web_ui.py:707-715`의 `_can_start`와 같은 판단 재사용).
- 최종 항목 수·순서는 결정 D2와 함께 구현 카드에서 확정한다. 이번 단계는 후보 6개까지.

## 6. 시험 계획
1. **등록·해제 단위 시험(레지스트리 쓰기는 임시 키)**: `mdm-ctx.exe --root <HKCU 하위 임시 키> …`(kvf의 `KVF_TEST_ROOT` 환경 분기 선례: `kvf/src/install.rs:36`, `:57`)로 `HKCU\Software\HwpAutoDocFit.CtxTest.<pid>` 아래에만 쓰고 시험 끝에 키 전체 삭제. 확인: 키 구조(확장자 수·하위 항목·`command` 값 따옴표), `--register` 두 번 = 같은 결과(멱등), `--unregister` 뒤 키 0개, `--status` 값, 옛 `HwpAutoDocFit` 키 정리, `--refresh`가 미등록이면 아무것도 안 씀. **실제 `HKCU\Software\Classes`는 시험에서 건드리지 않는다.** 파이썬 쪽 `shell_menu` 대체 변경은 기존 가짜 `winreg` 시험(`tests/test_shell_menu.py`)을 유지한다.
2. **경로 전달 시험**: 가짜 파이프 서버(`kagent/src/xl_target.rs:312-363`의 시험 파이프 이름 규약 선례)로 `mdm-ctx`가 보낸 JSON을 검증 — 한글·공백·`#`·`&`·긴 경로(250자 근처)·유니코드 정규화, 존재하지 않는 경로·심볼릭 링크·지원 밖 확장자 제외, 중복 제거·자연 정렬.
3. **다중 선택 시험**: 프로세스 N개(1·15·16·50·200)를 동시에 띄워 대표 1개만 전송하는지, 전송 목록이 N개와 일치하는지, 앱 꺼짐/시작 중/실행 중 3상태, 대표가 강제 종료됐을 때 남은 뮤텍스·스풀 복구(`shell_batch.py:98-117`의 낡은 잠금 처리 선례). 대량 선택 안정성은 미확인 U6.
4. **앱 쪽 시험**: `convert_open` 명령 처리·`--convert` 인자 분리·`get_state.convert_open`(`tests/` 기존 `test_kagent_app_service.py`·`test_instance_files` 계열 방식).
5. **실제 탐색기 확인은 사용자 실기**: Win10·Win11(`추가 옵션 표시`)에서 단일·다중·폴더·한글 경로 우클릭, 메뉴 표시 위치·하위 메뉴·아이콘, 설정 켜기/끄기·제거 후 흔적(`reg query`), 탐색기 응답 지연 체감. 실기 절차서는 구현 단계 마지막 카드에서 작성.
6. 한/글 COM 시험이 필요한 변환 자체는 이번 기능의 범위 밖(기존 파일변환 시험 유지). COM 시험이 필요하면 `C:\HWP_AUTODOCFIT\.com-test.lock` 규정을 따른다.

## 7. 단계 계획·카드 수 추정
| 단계 | 내용 | 카드 |
|---|---|---|
| CTXMENU2 | `mdm-ctx` 크레이트: 경로 수집·스풀·대표 선출·파이프 전송, 등록 로직(`--register/--unregister/--status/--refresh`, `--root` 시험 키), Rust 단위 시험 | 2(수집·전송 / 등록) |
| CTXMENU3 | 앱 쪽 수신: `convert_open`(앱 서비스 파이프)·`--convert` 인자·열기 요청 큐 `convert` 필드·`get_state.convert_open`·Flutter 탭 전환(`main.dart:785` 옆)·시험 | 2(파이썬 / Dart) |
| CTXMENU4 | 설치·설정: `setup.iss` 작업·[Run]·[UninstallRun]·[Registry], 설정 `shell_menu` 연결 교체, 옛 메뉴 정리, 안내문 | 1 |
| CTXMENU5 | 통합 시험(앱 꺼짐/시작 중/실행 중), 사용자 실기 절차서, 결과 문서 | 1 |
합계 **6장 안팎**(CTXMENU2~5). 사용자 결정 D1~D4는 CTXMENU2 시작 전에 받는다.

## 8. 위험 3가지와 대응
1. **Win11에서 '추가 옵션 표시' 안에만 보인다**(사용자가 기대한 새 메뉴 1단계가 아님; U1). 대응: 설정 안내문에 위치를 명시, 설정 화면에서 '지금 메뉴 열어 보기' 안내 없음(시스템 설정 변경 금지), (b) 방식은 2차 과제로 분리.
2. **다중 선택 수집 경합·앱 시작 중 경쟁**(선택 수 상한·프로세스 수 미확인 U2·U6; 앱 시작 중 두 번째 대표가 또 앱을 띄우는 경우). 대응: 스풀+뮤텍스 대표 방식, 앱 시작은 열기 요청 큐+기존 단일 인스턴스 뮤텍스가 중복 실행을 막음(`hwp-auto-docfit.py:3305-3366`), 대표 비정상 종료용 낡은 잠금 복구, 시험 §6-3.
3. **갱신·제거 때 레지스트리 흔적·낡은 경로, 그리고 기존 '한글편집 후처리' 메뉴와의 중복**(현장 HKCU 쓰기 정책 U7, 백신이 작은 exe의 레지스트리 쓰기를 의심할 가능성은 이번에 조사하지 않음). 대응: `--unregister`+`uninsdeletekey` 이중 제거, 앱 시작 때 `--refresh`, 옛 키 정리, 서명·해시 파일(kvf 같은 `.sha256` 사이드카 `kvf/src/install.rs:8-12`) 검토.

## 9. 미확인 항목 (9개)
| 번호 | 내용 | 확인 방법 |
|---|---|---|
| U1 | Win11(26H2)에서 정적 verb·하위 메뉴·아이콘이 '추가 옵션 표시' 안에 어떻게 보이는지(이 PC에서 레지스트리 쓰기 금지라 실측 안 함) | 사용자 실기(§6-5) |
| U2 | `MultiSelectModel`(`Single/Player/Document`)별 프로세스 수와 선택 수 상한(15로 알려졌으나 재확인 안 함) | 임시 키로 실기 또는 Microsoft Learn 재확인 |
| U3 | 희소 패키지·`IExplorerCommand` 등록에 대한 공공기관 정책(인증서 신뢰·개발자 모드·AppLocker/WDAC) | 대상 기관 IT 정책 문의 |
| U4 | Rust `IExplorerCommand` DLL의 크기·탐색기 지연 수치 | 2차 과제에서 프로토타입 측정 |
| U5 | `mdm-ctx.exe` 실제 크기(목표 ≤400 KB)·시작 시간(목표 ≤100 ms) | CTXMENU2 빌드 후 측정 |
| U6 | 대량 선택(수십~200개)에서 프로세스 폭주·수집 안정성·탐색기 체감 | CTXMENU5 시험 |
| U7 | 대상 공공기관 PC에서 `HKCU\Software\Classes` 쓰기 허용 여부, 기존 `shell_menu`의 현장 사용 실적(저장소에는 근거 문서가 없음) | 사용자 확인 |
| U8 | 파일변환 탭 진입 시 기존 `gui.files`(문서 탭과 공유로 보임)의 처리 — 비울지 합칠지 | CTXMENU3에서 `gui.files` 흐름 확인(결정 D5) |
| U9 | Flutter `onRenameOpen`류 콜백이 탭 전환과 변환 형식 선택을 함께 할 수 있는지(`main.dart:785` 호출부만 확인, 구현부는 읽지 않음) | CTXMENU3에서 확인 |

## 10. 사용자 결정이 필요한 항목
- **D1** 설치 중 선택 항목(기본 해제)을 '사용자가 켠 것'으로 인정할지. 인정하지 않으면 등록은 설정에서만.
- **D2** 화면을 연 뒤 변환을 **자동 시작**할지(권장: 시작하지 않음 — 사용자가 확인 후 시작).
- **D3** 기존 '한글편집 후처리' 메뉴를 새 메뉴가 대체할지(권장: 대체, 같은 설정 키).
- **D4** 폴더는 바로 아래 파일만(권장, 현행) / 하위 폴더 포함.
- **D5** 파일변환 탭에 이미 파일이 있을 때 새 파일을 합칠지 교체할지(U8 확인 후).
