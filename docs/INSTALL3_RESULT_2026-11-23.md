# INSTALL3 결과 — 실행 중인 색인 서비스의 kfind.exe 때문에 업그레이드가 롤백되는 문제 (2026-11-23)

## 원인
관리자 색인 서비스(HwpAutoDocFit.Index)가 `{app}\_internal\resources\kfind.exe --service` 를 직접 실행한다. 앱을 모두 닫아도 이 파일은 잠겨 있어
Inno 가 `DeleteFile` 코드 5(또는 32)로 실패하고, `/VERYSILENT` 에서는 Abort 기본값이라 전체가 롤백된다(`docs/DECISIONS_2026-11-23.md` 5절).
실행 중인 exe 는 삭제는 안 되지만 이름 변경은 된다.

## 구현(`installer/setup.iss` [Code] 만 추가, [Files]·[Run] 불변)
- `PrepareToInstall`(StopHelper 뒤): `CleanStaleOld` → `RenameLockedResources`.
  - `{app}\_internal\resources` 의 `*.exe`·`*.dll`·`*.pyd` 중 **쓰기 공유로 열 수 없는(잠긴) 파일만** `<이름>.old-<yyyymmddhhnnss>` 로 이름 변경. 잠기지 않은 파일은 건드리지 않음(잠금 판별은 읽기·쓰기·독점으로 열어 보기만 하고 내용은 바꾸지 않음).
  - 앞선 설치가 남긴 `.old-*`: 원래 이름 파일이 있으면 삭제 시도(잠겨 있으면 그대로), 없으면 원래 이름으로 되돌림.
- `CurStepChanged(ssPostInstall)`(설치 성공): `FinishRenamed(True)` — 새 꾸러미에 없어서 원래 이름이 비어 있는 파일은 되돌리고, 나머지 `.old-*` 는 삭제 시도(실행 중이라 실패하면 남기고 로그). kfind.exe 를 바꿨으면 "색인 서비스가 실행 중이어서 새 색인 프로그램은 서비스를 다시 시작해야 적용됩니다" 를 install.log 에 기록, 조용한 설치가 아닐 때만 안내 창.
- `DeinitializeSetup`(시작했으나 끝나지 않음 = 중단·롤백): `FinishRenamed(False)` — 원래 이름 파일이 없을 때만 되돌림(있으면 "복원 생략" 기록).
- 앱·ktm·kvf 등 실행 중 문제는 기존 `StopHelper` 그대로.

## 시험(`scripts/install3_locked_exe_test.py`, 재현: `--iscc <ISCC.exe> --work <빈 임시 폴더>`)
더미 dist(`PING.EXE` 복사 + 표지 바이트를 kfind.exe 등으로 이름 붙임)로 ISCC 컴파일(Inno 6, 4벌 모두 `Successful compile`),
설치는 모두 임시 폴더 `/VERYSILENT /DIR=` 로만. 이 PC 설치·서비스·앱은 건드리지 않음. 시험용 사본 setup.iss 만 AppId·AppName·install.log 위치·taskkill 대상 이름을 바꿈(제거 정보·바로 가기·로그·실제 프로세스 보호).

| 시나리오 | 결과 |
|---|---|
| A 1차 설치 | 종료 코드 0, 파일 5개 |
| B kfind.exe·legacy.exe(새 꾸러미에 없음)를 실제 실행해 잠근 채 2차 설치 | **종료 코드 0**; 새 kfind.exe 가 원래 이름(해시 d351…→da66…); 옛 파일은 `kfind.exe.old-20261123204927`; legacy.exe 는 원래 이름으로 되돌아옴; **두 프로세스 계속 동작**; 잠기지 않은 klib.exe·tool.dll 은 .old 없이 교체; install.log 에 서비스 재시작 안내 |
| C kfind.exe 실행(잠금)+data.bin 독점 잠금으로 일부러 중단 | 종료 코드 5(롤백); kfind.exe 가 원래 이름·원래 해시(da66…)로 **복원**; 프로세스 계속 동작; data.bin 내용 불변 |
| D 아무것도 안 잠긴 일반 업그레이드 | 종료 코드 0; `.old-*` 없음(앞선 잔여 정리); 새 파일로 교체 |

합계 **16/16 통과**. 설치 로그 발췌(install.log, cp949):
```
잠긴 파일 이름 변경: kfind.exe -> kfind.exe.old-20261123204927
옛 파일이 실행 중이라 남겨 둠(다음 설치 때 정리): kfind.exe.old-20261123204927
원래 이름으로 복원: legacy.exe
색인 서비스가 실행 중이어서 새 색인 프로그램은 서비스를 다시 시작해야 적용됩니다
설치가 끝나지 않고 중단됨(변경은 되돌려짐)      ← 시나리오 C
원래 이름으로 복원: kfind.exe
```
Python: `python -m unittest tests.test_distribution_build` → Ran 15 tests, OK.

## 한계·미시험
- 서비스는 계속 옛 kfind.exe 이미지로 돌므로 **새 색인 프로그램은 관리자가 서비스를 다시 시작해야 적용**된다. 비관리자(PrivilegesRequired=lowest) 설치 프로그램은 서비스를 멈출 수 없다. 서비스 재시작 뒤 다음 설치 때 `.old-*` 가 정리된다.
- 실제 서비스(SYSTEM 계정)로 시험하지 않았다: 잠금은 사용자 프로세스로 흉내 냈고, 시험용 사본에서는 StopHelper 의 taskkill 을 무력화했다(실제 설치 프로그램은 같은 계정의 kfind 사용자 모드를 종료한다).
- 비조용 설치의 MsgBox(서비스 안내)는 컴파일만 확인, 실행 확인은 못 함.
- 수정 전 설치 프로그램으로 B 를 재현하는 대조 시험은 이번에 하지 않음(PM 로그 inst293/inst295 로 갈음).
- 대상은 `_internal\resources` 의 exe/dll/pyd 만. `_internal` 최상위 DLL 이 잠긴 경우는 기존 StopHelper·Restart Manager 경로 그대로.
- 롤백 중 Inno 가 새로 복사한 파일을 지운 뒤에야 복원하므로, 롤백 직후~`DeinitializeSetup` 사이 수 초간 원래 이름 파일이 없다(서비스 프로세스는 이미 메모리에 있어 계속 동작하나, 그 순간 서비스를 새로 시작하면 실패할 수 있음).
