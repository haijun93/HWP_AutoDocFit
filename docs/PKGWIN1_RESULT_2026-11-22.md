# PKGWIN1 결과 — 설치본에 Win32 창 프로그램이 빠진 원인 규명 (2026-11-22)

## 현상
설치 폴더 `_internal\resources` 에 dock·hierarchy·kagent·kfind·klib·ktm·kvf·outliner·pdf-edit·qms_bench 는 있으나
outliner_win·settings_win·docview_win·photoview_win·convert_win·rename_win·mdm-ctx 가 없음. 사용자는 v2.2.91 사용.

## 가설별 판정

| 가설 | 판정 | 근거 |
|---|---|---|
| (나) CI 꾸러미에 안 들어감 | **사실상 배제(Portable 기준)** | CI 로그(ci291.log): 7종 모두 빌드·`Copy-Item`(3832·4095·7577·7753·7907·7978행 등) 후 "17 Rust binaries" 점검(8105행). v2.2.91 태그의 spec(37~63행)에 7종이 모두 `binaries` 로 들어 있음. 받아 둔 `v2291art/HWP_AutoDocFit_v2.2.91_Portable.exe` 의 바이트에서 7종 파일 이름이 zip 항목 구조(3회=로컬 헤더·중앙 디렉터리 등)로 확인됨. 같은 `dist\HWP_AutoDocFit` 에서 Setup 을 만든다. |
| (다) setup.iss [Files] 가 일부를 뺌 | **배제(코드상)** | `Source: "..\dist\HWP_AutoDocFit\*"; Flags: ignoreversion recursesubdirs createallsubdirs` 한 줄뿐, 제외 규칙 없음. ignoreversion 은 버전 비교로 건너뛰지 않게 하는 옵션. 단, **Setup.exe 안의 파일 목록은 압축돼 있어 직접 열어 보지 못함(미확인)**. |
| (가) 설치가 중간에 끊겨 일부만 교체 | **가장 유력하나 확정 못 함** | ① `install.log` 에 v2.2.91 은 "설치 준비 시작" 한 줄뿐(18:01:59) — 시작만 있고 끝 기록이 없음(당시 설치 프로그램에는 완료 기록이 없어 이것만으로는 성패 판별 불가). ② 11-22 23:09 설치 시도 로그는 앱 실행 중 `DeleteFile 코드 5` → "Abort 기본값" → 롤백(조용히 종료)을 보여 줌(RestartManager "Permission Denied", HWP_AutoDocFit.exe·색인 서비스가 파일 사용 중). ③ 설치본 mtime: `HWP_AutoDocFit.exe` 와 `_internal\resources` 의 dock·hierarchy·kagent 등은 07:18(v2.2.91 CI 빌드 시각) = 일부는 새 빌드. kfind·klib·ktm·kvf·outliner·pdf-edit·qms_bench 는 11-20 13:25 — CI 의 Rust 캐시로 이전 빌드 산출물이 그대로 쓰였을 가능성이 있어 **그 자체로는 부분 설치의 증거가 아님**. ④ 하지만 새로 만든 7종만 없는 것은 "CI 에서 빠짐" 이 아니면 "설치 후 사라짐/설치 미완료"로 설명해야 함. |
| (라) Setup 이 아닌 Portable/다른 경로 | **미확인** | 설치 폴더에 `unins000.exe/dat`(11-20)가 있어 Setup 설치 경로는 맞음. 단 18:01 설치가 Setup 인지는 확인 못 함. |
| (마, 추가) 백신(Defender)이 서명 없는 새 Rust exe 7종을 격리 | **미확인, 개연성 있음** | 새로 추가된 서명 없는 소형 GUI exe 만 없다는 점이 부합. 이 PC 에서는 보호 기록 조회가 권한 때문에 막혀 확인하지 못함. |

## 사용자가 확인할 방법(읽기 전용)
1. 설치 폴더 파일 목록:
   `dir "%LOCALAPPDATA%\Programs\HWP_AutoDocFit\_internal\resources\*_win.exe" /s` 와 `dir "%LOCALAPPDATA%\Programs\HWP_AutoDocFit\_internal\resources\mdm-ctx.exe"`
2. 같은 버전 Portable 에 7종이 있는지: Portable 을 실행해 위 목록이 풀린 폴더(`%LOCALAPPDATA%` 아래 캐시)를 확인.
3. Defender 격리 이력: Windows 보안 → 바이러스 및 위협 방지 → 보호 기록. `outliner_win.exe` 등이 보이면 (마)가 원인. 복원 후 예외 등록.
4. 설치 로그: 설치 프로그램을 `/LOG=C:\임시\setup.log` 로 실행해 `Dest filename: ...\outliner_win.exe` 항목이 있는지 확인(이 PC 에서 이미 한 방법).
5. 앱을 모두 닫고(트레이 포함) 같은 Setup 을 다시 실행했을 때 7종이 생기면 (가) 확정, 안 생기면 (마) 또는 (다).

## 재발 방지(이번에 구현)
- **앱 안내**: 새 `docfit_app/package_integrity.py` — 6개 창 서비스(OUTLINER/SETTINGS/DOCVIEW/PHOTOVIEW/CONVERSION/RENAME_WINDOW_MISSING)의 메시지가 "…(파일)을 찾을 수 없습니다. 설치가 완료되지 않았을 수 있습니다. 앱을 모두 닫고 … 설치 프로그램을 다시 실행해 주세요." 로 바뀜(오류 코드 불변).
- **시작 점검**: 배포 앱(frozen)만, 시작 5초 뒤 한 번 6종 존재를 점검해 누락 시 로그에 경고(UI·동작 불변).
- **설치 프로그램(setup.iss)**: [Files]·[Run]·`CloseApplications=force` 등 기존 설치 경로는 그대로 두고 [Code] 만 추가.
  - `CurStepChanged`: 파일 설치 시작·끝을 `install.log` 에 기록, 끝난 직후 7종 존재 확인(없으면 log + 대화형일 때 안내 창).
  - `DeinitializeSetup`: 시작했으나 끝나지 않고 종료(롤백)되면 `install.log` 에 기록, 대화형일 때 "설치가 완료되지 않았습니다 … 앱을 모두 닫고 다시 실행" 안내.
  - 조용한 설치(/VERYSILENT)에서는 창을 띄우지 않고 로그만 남김.
- **CloseApplications 동작 확인**: `CloseApplications=force` 는 Restart Manager 로 사용 중 프로그램을 닫으려 하나, 이 PC 로그처럼 `Permission Denied` 면 닫지 못하고 `DeleteFile 코드 5` 로 중단·롤백한다. 이 부분(권한)은 설치 프로그램만으로 바꿀 수 없어 안내·기록으로 보완했다. 파일 교체 방식 변경은 "설치 동작 불변" 원칙에 따라 하지 않음.

## 시험
- `python -m unittest` 관련 7개 모듈(package_integrity·outliner_service·settings/docview/photoview/conversion/rename 창 서비스): **Ran 51 tests, OK**.
- `ISCC`(Inno 6.7.1) 임시 더미 dist 로 컴파일: **Successful compile** (경고: x64 식별자 deprecated — 기존과 동일).
- 실제 설치·실행 시험은 하지 않음(이 PC 설치 금지 지시). 새 [Code]의 대화상자 동작은 컴파일만 확인했고 실행 확인은 못 했다.

## 남은 문제
- 원인 확정 못 함: (가)·(마) 중 어느 쪽인지는 위 사용자 확인 1~5 가 필요.
- v2.2.91 Setup.exe 의 내부 파일 목록은 직접 확인하지 못함(Portable 로 간접 확인).
- CI 의 frozen 번들 점검이 kvf·pdf-edit·outliner·qms_bench 만 하므로, Win32 창 7종은 PM 이 추가한 "꾸러미 포함 확인"(7ffb8602)이 유일한 방어선.
