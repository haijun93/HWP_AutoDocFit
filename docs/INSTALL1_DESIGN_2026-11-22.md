# INSTALL1 — 설치 프로그램 보강 설계 (2026-11-22)

근거: `docs/WINRUST_TECHNIQUE_REVIEW_2026-11-20.md` ⑦·우선순위 5. 대상: `installer/setup.iss`.
이 PC에는 설치·실행하지 않았고 Inno Setup 컴파일(ISCC)만 했다.

## 1. 이번에 구현한 것 — 설치 기록(위험 낮음)
- 기록 파일: `%LOCALAPPDATA%\HwpAutoDocFit\logs\install.log`(덧붙임, 줄마다 시각·버전).
- 기록 함수 `InstallLog`는 예외를 삼켜 **기록 실패가 설치를 막지 않는다**.
- 남기는 내용: 설치 준비 시작, `taskkill` 정리 실패(종료 코드 0·128 외 또는 실행 실패), ktm·kvf `[Run]`의 "대상 파일 없음(건너뜀)"/"시작"/"끝", 제거 때 `kfind --uninstall` 결과.
- **한계(중요)**: Inno의 `[Run]`은 종료 코드를 `[Code]`에 주지 않는다. 그래서 ktm·kvf의 종료 코드 자체는 이 기록에 없다. 종료 코드는 `Setup.exe /LOG="경로"`로 만든 Inno 자체 로그("Process exit code")로 확인한다. 종료 코드를 남기려면 아래 3절 2안(`[Run]`을 `Exec`로 바꿈)이 필요하며 동작이 바뀌므로 이번에는 하지 않았다.
- 기존 동작 불변: `[Run]` 파라미터·Flags(`--install --respect-disabled`, `runhidden waituntilterminated skipifdoesntexist`)는 그대로, `BeforeInstall`/`AfterInstall`만 추가.

## 2. 구성요소 선택 도입안(미구현)
| 구성요소 | 내용 | 기본 |
|---|---|---|
| core(고정) | 앱 본체, 한/글 후처리, 설정·뷰어 창 | 필수 |
| search | 몽땅검색(ktm), 색인(kfind) 및 서비스 연동 | 선택(켬) |
| vfolder | 퀵가상폴더(kvf) | 선택(켬) |
| clip | 몽땅클립보드(kagent)·자료함(klib) | 선택(켬) |
| ctxmenu | 탐색기 우클릭 메뉴 | 앱 설정에서 켜는 현재 방식 유지 |

- 구현 방식: `[Components]` 추가 후 `[Run]`에 `Components: search` 등 지정. 파일은 `dist` 단일 꾸러미(`[Files]` 1줄)라 **파일 단위 제외는 어렵다** → 1단계는 "설치 후 자동 시작·등록(`--install`)만 구성요소로 제어"하고 파일은 모두 복사한다. 파일까지 빼려면 빌드 산출물 폴더를 구성요소별로 나눠야 한다(PyInstaller spec 변경, 큰 작업).
- 업그레이드: 이전 선택을 레지스트리(`Uninstallable` 키 `Inno Setup: Selected Components`)에서 Inno가 기억한다. 사용자가 설정에서 끈 기능은 `--respect-disabled`가 계속 보호.

## 3. 영향 범위·위험·되돌리기
| 안 | 영향 | 위험 | 되돌리기 |
|---|---|---|---|
| 1안: 이번 기록 추가 | `setup.iss` [Code]·[Run] 2줄 | 낮음(예외 삼킴, 동작 불변). 로그에 경로·사용자명이 남는 정도 | 커밋 되돌리기. 로그 파일은 지워도 무해 |
| 2안: `[Run]`→`Exec` 래퍼로 종료 코드 기록 | ktm·kvf 호출 방식 변경(숨김·대기·`skipifdoesntexist` 재구현) | 중간(조용한 설치 `/SILENT`·권한·대기 동작 회귀 가능) | 1안 상태로 복귀 |
| 3안: `[Components]` | 설치 마법사에 선택 화면 추가, 사일런트 설치는 `/COMPONENTS=` 필요 | 중간~높음(기존 배포 스크립트·자동 업데이트가 `/SILENT`로 호출 → 기본값 전체 선택이면 호환) | 구성요소 구역 삭제. 이미 설치된 PC는 영향 없음(전체 선택 기본) |

권장 순서: 1안(완료) → 실기 시험 후 `/LOG`로 종료 코드 수집 → 필요하면 2안 → 3안.

## 4. 실기 시험 필요(미수행)
- 설치·업그레이드·제거 각각에서 `install.log` 생성 확인, ktm·kvf 종료 코드 `/LOG` 확인.
- 읽기 전용·권한 없는 `%LOCALAPPDATA%`에서 기록 실패가 설치를 막지 않는지.
- kagent·dock은 `[Run]` 줄이 없다(앱 내부 설정 경로로 추정, 미확인).
