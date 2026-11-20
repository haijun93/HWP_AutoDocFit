# 파일명 색인 서비스 상태 조사 (2026-11-20)

작업 범위: `feature/IDXSVC`에서 코드와 설치 스크립트를 읽고, 이 PC의 서비스·프로세스·경로를 조회했다. 실행 중인 앱과 서비스를 변경하지 않았다. 데이터 폴더는 파일명·크기만 확인했으며 내용은 읽지 않았다.

## 결론

- 관리자 Windows 서비스 `HwpAutoDocFit.Index`는 이 PC에 등록되어 있지 않다. `Get-Service`와 `Win32_Service` 조회 결과 일치 항목이 없고, `C:\ProgramData\HwpAutoDocFit`도 없다.
- 다만 사용자 모드 `kfind.exe`는 실행 중이며 사용자별 색인 디렉터리도 있다. 따라서 “서비스 없음”이 “kfind 전체가 꺼짐”을 뜻하지는 않는다.
- 퀵몽땅검색(ktm)의 파일명 탭은 서비스 파이프에 직접 질의하고, 파이프가 없으면 오류를 반환한다. 반면 문서 본문 검색 색인과 Python 앱의 사용자 모드 kfind 실행은 별도 경로다.

## 코드에서 확인한 시작·동작 경로

### 관리자 서비스가 언제 시작되는가

`installer/setup.iss`의 `[Run]`은 `ktm.exe --install --respect-disabled`, `kvf.exe --install --respect-disabled`, 메인 앱 실행만 호출한다. 여기에는 `kfind.exe --install`이나 서비스 설치가 없다. `[Code]`의 `PrepareToInstall`은 업데이트 전에 현재 계정의 kfind 프로세스를 종료하도록 되어 있지만, 이번 PC 조사에서는 이를 실행하지 않았다. 제거 시에만 서비스 조회 후 `kfind.exe --uninstall`을 관리자 권한으로 호출하는 코드가 있다.

`docfit_app/kfind_process.py`에는 설정 경로에서 호출할 `install_service()`가 있다. 이 함수는 UAC로 `kfind.exe --install`을 실행하고, 성공하면 `--start`도 실행한다. CLI에서 서비스 설치·시작을 제공하는 구현은 `crates/kfind/src/main.rs` 및 `install.rs`에 있다. 서비스 등록은 AutoStart와 지연 자동 시작으로 설정되고, 서비스 계정은 명시하지 않는다(Windows 기본 LocalSystem). 즉, **서비스는 앱 설치만으로 자동 등록되지 않고 설정에서 별도로 설치하도록 만들어진 구조**다.

설치 스크립트가 아닌 앱의 자동 시작 경로도 있다. `docfit_app/kfind_process.py`의 `ensure_kfind_running()`은 검색 엔진이 `legacy`가 아니고 서비스 파이프가 없으면 `kfind.exe --user`를 사용자 권한으로 시작한다. 저장된 대체 방식이 `task`이면 예약 작업을 우선 기대하지만 현재 파이프가 없을 경우에도 이번 실행은 사용자 모드로 시작한다. kfind 실행 파일이 없거나 시작에 실패해도 앱 시작 자체는 막지 않는다.

### 서비스 모드와 사용자 모드

| 구분 | 서비스 모드 | 사용자 모드 |
|---|---|---|
| 목적 | 고정 드라이브 전체 파일명 색인 | 요청에 지정된 등록 폴더만 색인 |
| 수집 | NTFS 볼륨은 MFT 열거와 USN 변경 추적을 시도. 그 외 또는 MFT 열기 실패 시 디렉터리 순회 | `roots`로 받은 경로를 디렉터리 순회 |
| 실행 권한 | Windows 서비스의 기본 LocalSystem 계정. `--install`과 `--start`는 UAC 관리자 권한 요청 | 현재 로그인 사용자의 일반 권한. 서비스 등록 불필요 |
| 저장 위치 | `%ProgramData%\HwpAutoDocFit\index` | `%LOCALAPPDATA%\HwpAutoDocFit\index` 아래 SID별 디렉터리 |
| 범위 제한 | 결과에서 다른 사용자 프로필, `$Recycle.Bin`, `System Volume Information` 제외 | 해당 사용자에게 접근 가능한 등록 경로. 서비스 모드의 프로필 제외 필터 없음 |

사용자 모드는 “등록 폴더만” 대상으로 하지만, 이 PC에서 현재 등록된 `roots` 값이나 목록을 확인하지 않았다. 앱 부팅 코드에는 사용자 프로세스 자동 시작이 있으나 그것만으로 ktm의 전체 드라이브 파일명 검색용 서비스 파이프가 생기는 것은 아니다.

### ktm이 서비스가 없을 때 하는 동작

`ktm/src/backend.rs`의 파일명 검색은 `names_query`를 `\\.\pipe\HwpAutoDocFit.Index`로 보낸다. 서비스 파이프가 없거나 이름 색인 상태가 `empty`이면 결과를 빈 목록으로 정상 처리하는 대신 오류를 만든다. `tm_state.rs`와 `window.rs`도 검색 실패를 오류 상태 및 “검색을 완료하지 못했습니다” 안내로 표현하도록 되어 있다. 서비스 부재를 감지해 사용자에게 서비스 설치를 안내하거나 서비스 설치를 대신 시작하는 ktm 동작은 확인되지 않았다.

TODO의 “색인 서비스 T4는 아직 미완성”은 **서비스 실행 파일과 SCM 수명 관리, 이름 색인 명령이 전혀 없다는 뜻은 아니다.** `crates/kfind`, `docs/T4_KFIND_SERVICE_2026-11-13.md`에는 `--install/--start/--stop/--uninstall/--service`, SYSTEM 모드 및 파일명 색인 프로토콜이 구현·합성 시험되었다고 기록되어 있다. 미완성으로 적힌 부분은 T4 후속 범위(서비스 SID 게시기·NTFS ACL 게시, MFT/USN 검증, 작업 프로세스, 업데이트/되돌리기 등)와 실제 SCM·다른 사용자·조직 정책 환경의 VM 검증이다. 단, TODO 문구는 11월 20일 오류 조사 방향에 남은 표현이며 최신 상태로 갱신되었는지는 확인되지 않았다.

## 이 PC에서 읽기 전용으로 확인한 상태

조회 시점은 이 조사 세션(2026-11-20)이다.

| 항목 | 확인 결과 |
|---|---|
| HwpAutoDocFit/kfind Windows 서비스 | `Get-Service` 및 `Get-CimInstance Win32_Service`에서 일치 항목 없음 |
| `kfind.exe` 프로세스 | 실행 중, PID 12960 |
| 설치된 `kfind.exe` | `C:\Users\haiju\AppData\Local\Programs\HWP_AutoDocFit\_internal\resources\kfind.exe`, 2,942,464바이트 |
| `%ProgramData%\HwpAutoDocFit` | 없음 |
| `%LOCALAPPDATA%\HwpAutoDocFit` | 있음. `index`와 기타 앱 데이터 하위 폴더 존재 |
| 사용자별 색인 디렉터리 | `index\S-1-5-21-2477108563-215478986-3631227696-1001` 존재. 파일명 목록 조회 결과 파일 없음 |

디렉터리 파일 목록은 요청대로 파일 내용 없이 확인했다. 존재 여부만으로 색인의 유효성, 등록된 폴더, ktm 서비스 파이프 상태, 색인 상태(`empty/building/ready/failed`)를 확정할 수 없다. 사용자가 처음 보고한 파이프 부재와 이 조사 시점의 kfind 프로세스 실행은 시점 차이가 있으므로, 파이프 부재가 계속되는지는 이번 PC 조회에서 별도 named-pipe 검사로 재확인하지 않았다.

## 사용자가 지금 할 수 있는 조치

1. **전체 드라이브 파일명 검색이 필요하면** 앱 설정에서 kfind 색인 서비스 설치 항목을 찾아 설치·시작한다. 코드에는 `docfit_app/kfind_process.py`의 `install_service()`로 연결되는 설정 경로가 있으나, 현 Flutter 설정 화면에서 사용자가 누를 정확한 화면 이름·버튼을 찾지 못했다. UI가 보이지 않으면 관리자 권한 PowerShell에서 설치된 실행 파일을 직접 실행할 수 있다:

   ```powershell
   & "$env:LOCALAPPDATA\Programs\HWP_AutoDocFit\_internal\resources\kfind.exe" --install
   & "$env:LOCALAPPDATA\Programs\HWP_AutoDocFit\_internal\resources\kfind.exe" --start
   ```

   각 명령은 UAC 승인이 필요하다. **이 조사에서는 실행하지 않았다.** 서비스는 지연 자동 시작으로 등록되지만 현재 서비스가 미등록이므로 이 두 단계가 필요하다. 서비스가 실행되어도 파일명 색인 상태가 `empty`이면 첫 이름 색인 구축이 필요할 수 있다. ktm 화면에는 구축 명령/진행 상태 안내가 확인되지 않아 그 절차는 미확인이다.
2. **등록 폴더만 색인하는 사용자 모드**는 앱이 `ensure_kfind_running()`에서 자동으로 띄운다. 현재 사용자 모드 프로세스가 실행 중이므로 이를 “서비스를 켠 상태”와 혼동하지 않아야 한다. 전체 드라이브를 검색하려는 목적이라면 서비스 설치 경로를 사용한다.

## 코드로 개선할 후보와 대략 작업량

| 후보 | 제안 | 작업량(추정) |
|---|---|---|
| ktm의 서비스 부재 안내 | 파이프 부재와 서비스 미등록/중지, 서비스는 있으나 색인 empty/building/failed 상태를 나눠 상태줄·0건 화면에 안내. 오류를 빈 결과로 보이지 않게 유지 | 작음~중간: Rust 상태 조회·오류 분류 및 화면 문구, 단위/통합 검증 |
| 사용자 설정의 서비스 관리 | 실제 Flutter 또는 ktm 설정 진입점에서 “전체 파일명 색인 켜기” 동작을 노출하고 `install_service()` 연결, UAC 취소·정책 차단·성공·대체 모드를 표시 | 중간: 브리지/API 연결 여부 조사 후 버튼·상태 UI 포함 |
| 최초/재색인 흐름 | 서비스 시작 뒤 `names_status`를 확인하고 empty이면 구축을 안내하거나 명시적으로 시작하며 진행 상태와 실패 이유 표시 | 중간~큼: 프로토콜·관리 UI·중단/재시도 상태 설계 |
| TODO 조사 방향 정정 | 서비스 미구현 전제 대신, 서비스 미등록 상태·ktm 직접 파이프 질의·오류 표시 문제를 조사 기준으로 기록 | 작음: TODO 한 항목 갱신(이번 요청은 문서 1개만 허용해 수행하지 않음) |

## 미확인 항목

아래 8개는 이 조사에서 확인하지 못했다.

1. 사용자에게 노출되는 설정 화면의 정확한 경로와 서비스 설치 버튼명, 현재 UI에서 Python `install_service()`를 실제 호출하는지.
2. 조사 순간 `\\.\pipe\HwpAutoDocFit.Index`가 생성되어 있었는지. 이 조사에서는 named-pipe를 읽기 전용으로 조회하지 않았다.
3. 사용자 모드 kfind 프로세스가 어느 모드 인수로 시작됐는지, 프로세스가 ktm 요청을 처리하는지.
4. 사용자 색인 디렉터리에 파일이 없는 것이 정상(파일명 색인은 별도 저장/미구축)인지, 실제 검색 가능한 본문 인덱스가 있는지. 파일 내용은 열지 않았다.
5. 현재 ktm 검색 설정·등록 `roots`와 “합의각서” 검색 당시의 실제 설정/필터.
6. 관리자 서비스에서 `names_build`가 수행되었는지, 볼륨·항목 수·`last_error` 및 이름 색인 준비 상태.
7. 사용자의 “파이프 없음” 관찰 시점 이후 kfind가 다시 시작된 경위와 서비스 설치가 이 PC에서 과거에 시도되었는지.
8. `T4` 후속 구현과 실제 서비스/정책 VM 검증이 현재 브랜치 이후 다른 변경에서 완료되었는지.

