# 탐색기 오른쪽 버튼 메뉴 '몽땅파일변환' 1단계 결과 (CTXMENU2, 2026-11-21)

설계: `docs/CONTEXT_MENU_DESIGN_2026-11-21.md`. 이번 단계는 이름·구조 교체, Rust 전달 프로그램 `mdm-ctx.exe`, 설치·설정 연결, 시험까지다. 푸시는 하지 않았다.

## 1. 결정과 사유
| 항목 | 결정 | 사유 |
|---|---|---|
| 표시 이름 | 상위 항목 **몽땅파일변환**(`MUIVerb`) | 사용자 결정(TODO 결정 기록) |
| 레지스트리 키 이름 | `HwpAutoDocFit` → **`MongttangFileConvert`** | 설계 §4-2의 권장 이름. 이름이 앱 옛 이름과 같으면 옛 키와 새 키를 구분할 수 없어 마이그레이션·제거 시험이 어렵다. 옛 키 이름은 `LEGACY_MENU`로 남겨 정리 대상으로만 쓴다 |
| 마이그레이션 | 앱 시작 `refresh_registered()`: 옛 키 또는 낡은 새 키가 있으면(= 설정이 켜진 상태) 새 키로 교체, 아무 키도 없으면 아무것도 쓰지 않음. `register()`는 옛 키를 먼저 지움. `unregister()`는 새·옛 키 모두 지움 | 설계 D3 |
| 하위 항목 | 지금 기능 그대로(확장자별 6개 키). '한글편집 후처리로 열기' 라벨만 **'앱으로 열기'**로 | 요청 문구 |
| 설치 프로그램 | `[Registry]`에 새 키 6개 `uninsdeletekey` 추가(옛 6개 유지), `PrepareToInstall`에 `StopHelper('mdm-ctx.exe')` | 제거 때 흔적 정리 |
| 설치 중 선택(D1) | **하지 않음**(사용자 D1 미결정). 등록은 설정 `shell_menu`(기본 False)를 켤 때만 | 설계 D1 |
| 번들 | `HWP_AutoDocFit.spec`: `resources/mdm-ctx.exe`가 있으면 `resources`에 추가(`datas`가 `resources` 폴더 전체를 이미 담지만 klib와 같은 방식으로 명시). CI(`build-windows-exe.yml`)에 빌드·시험·복사 단계와 crt-static 확인 목록 추가. `.gitignore`에 산출물 추가 | `kvf`·`klib` 선례 |

## 2. `mdm-ctx.exe` 동작 (`mdm-ctx/`)
메뉴 명령: `"<mdm-ctx.exe>" --app "<앱 exe>" convert "%1" --to <형식> --notify`
1. 각 프로세스가 `%LOCALAPPDATA%\HWP_AutoDocFit\ctx_spool\<형식>\`에 경로를 `.tmp`→`.req`로 원자적으로 남긴다.
2. 형식별 이름 있는 뮤텍스(`Local\HwpAutoDocFit.ctx.<형식>`)를 잡은 한 프로세스만 대표가 되고 나머지는 즉시 종료한다.
3. 대표는 새 요청이 **1.5초** 조용해질 때까지(상한 30초·200개 — `shell_batch.py`와 같은 값) 모은 뒤 존재·일반 파일·심볼릭 링크 아님·지원 확장자(20개)를 거르고, 중복 제거·자연 정렬해 **앱 `convert <경로…> --to <형식> --notify`를 한 번 실행**하고 끝난다. 명령줄이 30,000자를 넘으면 `pdfm` 외에는 묶음을 나눠 실행하고, `pdfm`은 메시지 상자로 안내한다.
4. 대표가 끝나는 순간 도착한 요청을 놓치지 않게, 뮤텍스를 놓은 뒤 남은 요청이 있으면 다시 대표를 시도한다. 120초 넘은 낡은 요청(대표 비정상 종료 잔여)은 지운다.
5. 로그는 `ctx.log` 한 줄씩. 의존성은 `windows-sys`만.

앱 파이프 전달(`convert_open`)·`--convert` 화면 열기는 **이번 단계 범위 밖**이다. 설계 §3-3·3-2③의 제안 그대로 CTXMENU3에서 `mdm-ctx`의 마지막 단계(앱 실행)를 '파이프 `convert_open` 전송, 없으면 열기 요청 큐 + 앱 시작'으로 바꾸면 되고, 지금 구조에서 바뀌는 곳은 `main.rs::launch` 한 함수다.

`mdm-ctx.exe`가 없거나(개발 환경: 앱 실행 명령이 `python 스크립트` 두 조각) 앱 실행 명령이 한 조각이 아니면 **기존 직접 호출(`--collect`, `shell_batch.py`)**로 등록된다. 이 때문에 `shell_batch.py`·`cli.py convert --collect`는 그대로 남아 있다.

## 3. 시험 결과 (실제 출력)
| 명령 | 결과 |
|---|---|
| `cargo test`(mdm-ctx, 단위) | `test result: ok. 5 passed; 0 failed` |
| `cargo test`(mdm-ctx, 통합 `tests/flow.rs`) | `test result: ok. 5 passed; 0 failed; ... finished in 3.42s` — 한글·공백 경로 1건, 30개 프로세스 동시 → 대표 1회 전달·자연 정렬(문서1·문서2·…·문서10), 중복·없는 파일·`.exe` 제외, 유효 파일 없으면 앱 실행 안 함, 형식(`pdf`/`hwpx`)별 모음 분리 |
| `python -m unittest tests.test_mdm_ctx tests.test_shell_menu tests.test_shell_batch tests.test_cli` | `Ran 56 tests in 8.261s` / `OK` |
| `flutter analyze` | `No issues found! (ran in 25.9s)` |
| `flutter build web --release` | `√ Built build\web` |

`tests/test_mdm_ctx.py`: Rust 확장자 상수 20개 = `CONVERSION_INPUTS`, 변환 명령이 ctx 경유(`--collect` 없음)·열기는 앱 직접, 확장자별 하위 메뉴 라벨, 멱등, 옛 키 마이그레이션(켜짐/꺼짐), 해제 때 새·옛 키 제거·타 앱 키 보존, 낡은 경로·ctx 변화 시 재기록, **실제 레지스트리 임시 키**(`HwpAutoDocFitTest_<난수>`·`HwpAutoDocFitTestOld_<난수>`) 등록→옛 키 정리→멱등→해제 후 0개, 시험 전후 실제 `HwpAutoDocFit`·`MongttangFileConvert` 키 존재 여부 불변(읽기만).

## 4. 크기·시작 시간 (이 PC, release `opt-level="z"`)
- **크기: 381,440 B (약 373 KiB)** — 목표 ≤ 400 KB 달성. `VCRUNTIME140`·`MSVCP140`·`api-ms-win-crt-` 문자열 없음(crt-static 적용).
- **시작 시간(프로세스 생성~종료, 30회, 모음 대기 0ms로 설정)**: 유효 파일 1개(앱 실행 포함) 중앙값 **139.2 ms**(최소 82.8, p90 211.3), 전달할 파일 없음 중앙값 **122.3 ms**(최소 82.0). 같은 방식으로 잰 `whoami.exe` 기준선이 중앙값 **112.5 ms**(최소 73.1)이라 프로세스 생성 비용이 대부분이다. **목표 ≤100 ms는 중앙값으로는 미달**(최소값만 도달) — 이 PC 환경(백신 검사로 보이는 프로세스 생성 지연) 영향이며 코드 시간은 기준선과 비슷하다. 대표는 모음 1.5초를 기다린 뒤 끝나지만 추종자(두 번째 이후 프로세스)는 이 기준으로 바로 끝난다.
- 기존 방식(파일마다 Python 앱 본체 기동, 호스트 시작 import 894.7 ms 기록)과 달리 메뉴가 띄우는 프로세스는 이 작은 exe뿐이고 앱 `convert`는 모은 뒤 **한 번**만 뜬다.

## 5. 미확인·한계
- **실제 탐색기 표시는 미실행(사용자 실기)**. 확인 방법: 앱 설정에서 '탐색기 우클릭 메뉴'를 켠 뒤 `.hwp`·`.pdf` 등을 우클릭 → Windows 11은 **'추가 옵션 표시'**(Shift+F10) 안의 '몽땅파일변환' → 하위 메뉴. (U1·U2·U6 그대로 미확인)
- 앱 `convert` 한 번 실행에서도 변환 자체(한/글 COM 등)는 이번에 시험하지 않았다(변환은 기존 기능 그대로, COM 시험 잠금 미사용).
- 경로 전달은 아직 `convert` 명령줄 인자이므로 매우 긴 경로 다수는 나뉘어 실행된다(`pdfm` 제외). 파이프 전달로 해소 예정(CTXMENU3).
- 옛 키가 `HKCU`에 남아 있는 PC는 앱을 한 번 실행해야 새 키로 바뀐다(설정이 켜진 경우). 앱을 안 켜고 제거하면 `[Registry]` 행이 옛·새 키를 모두 지운다.
- `shell_menu.py`의 마이그레이션·`ctx_exe_for`는 Python 로직이다(등록 쓰기를 Rust로 옮기는 설계 §4-2 `--register`는 이번 범위 밖 — 최소 변경으로 기존 등록 코드를 확장). 새로 늘어난 Python은 레지스트리 키 이름 교체와 옛 키 삭제뿐이고 모음·전달 로직은 모두 Rust다.
- GitLab(`origin`)은 건드리지 않았고 푸시도 하지 않았다.
