# EXFILE 결과

## 변경 파일

- `release-assets/보고서 예시 파일(한번에 적용).txt`: 사용자 지정 시연용 가상 보고서로 내용 교체. 파일 이름·경로를 유지했다. 기존과 같은 UTF-8(BOM 없음)·LF이며 5,537바이트, LF 94개, 논리 줄 95개, 비어 있지 않은 줄 46개다. 입력의 CRLF만 LF로 바꿨으며 나머지 내용은 바이트 대조로 일치한다.
- `tests/test_abbreviations.py`: 배포 TXT 전체를 실제 앱 읽기 함수로 읽는 회귀 시험 1개 추가. 기존 시험의 기대값이나 합격 기준은 변경하지 않았다.
- `docs/USER_GUIDE.md`: 새 예시의 6개 본문 항목과 붙임 목록 안내, 제목·개요 표 및 부연설명 확인 방법 반영. 새 파일에 없는 ‘침공계획 개요’ 박스 그림 표 설명을 수정했다. 기존 이미지·영상·자간·쪽 수 실측은 교체 전 예시의 기록임을 명시했다.
- `reports/EXFILE.plan.md`: 앱 입력/내장 서식 예시/배포 원본/배포 사본 경로와 검증 계획 기록.
- `reports/EXFILE.final.md`: 본 보고서.

배포 워크플로와 스크립트의 복사 원본이 교체 파일을 가리키는 것을 확인했다. GitHub의 `report-example-all-in-one.txt` 사본과 ZIP의 한국어 파일명, GitLab downloads의 한국어 파일명은 그대로다. `resources/서식예시`의 서식 원본과 별도 내장 `서식예시_글`은 이번 TXT 교체 대상에 해당하지 않는다.

## 원본 보존

입력 경로: `C:/Users/haiju/OneDrive/바탕 화면/시연용예시보고서.txt`.

- 사용자 원본: 5,631바이트, UTF-8(BOM 없음), CRLF 94개.
- 사용자 원본 SHA-256: `678d7ef3e1caf42624da8de3d9a37797797e94c2b766d2b5a763e76a6d0fa5a2`.
- 배포 TXT SHA-256: `747c0301b7a17c583895f87bcb568e0f6f9dc25899bf9f66f7ddb2310b34157b`.
- 사용자 원본은 읽기만 했으며 수정·이동하지 않았다. 작업 전후 해시 동일성과 CRLF 정규화 후 배포 TXT와의 완전 일치를 확인했다.

## 검증

| 검사 | 결과 |
|---|---|
| 준말·서식 예시·라벨 표 관련 시험 | 44개 통과, 7.575초 |
| XML 일괄 서식·항목기호 계층·내어쓰기 시험 | 70개 통과, 74.734초 |
| `.venv/Scripts/python.exe -m unittest discover -s tests` | 2,706개 실행, 통과 2,699개·조건부 제외 7개·실패 0·오류 0, 1,226.201초 |
| `flutter analyze` (`flutter_ui`) | 통과, No issues found, 27.4초 |
| `git diff --check` | 통과 |

추가 회귀 시험은 □ 6개·ㅇ 18개·- 12개·* 3개·** 2개·※ 1개와 붙임 두 번째 목록 번호를 정확한 개수로 판별한다. 제목·개요 준말 2개만 변환하며, 생성된 제목 2×2 표와 개요 1×1 표의 칸 구성·텍스트·27pt/15pt 글자 크기를 검사한다. 제목의 쉼표 앞 부제 분리와 앱 규칙에 따른 제목·개요 주변 빈 줄 3개 삭제를 검증하고, 나머지 본문·주석·붙임·빈 줄 전체를 원문과 일대일 대조한다.

기본 Python에는 pypdf·cryptography·xlrd·pywebview가 없어 첫 전체 실행에서 환경 의존 오류가 발생했다. 그 실행은 중단하고, 작업 워크트리의 `.venv`에 `requirements.txt` 의존성을 설치한 뒤 전체 시험을 처음부터 재실행하여 통과했다. 최종 결과는 `OK (skipped=7)`이며 남은 환경 의존 오류는 없다. 설치 환경·시험 로그(`reports/EXFILE.unittest-venv.log` 등)는 무시되는 로컬 파일이며 커밋에 포함하지 않는다. ResourceWarning과 예외 처리 시험이 의도적으로 출력한 오류 로그는 시험 실패와 구분했다.

실제 한/글 COM의 전체 ‘한 번에 적용’ 실행·조판 실측은 공용 `C:/HWP_AUTODOCFIT/.com-test.lock`이 다른 작업에 의해 잠긴 상태여서 실행하지 않았다. 이번 결과는 관련 분석·XML 변환 시험의 결과이며 실제 쪽 수·자간 실측을 뜻하지 않는다.

## 반영

- 기준: `github/main`의 `b883eb25`, 전용 worktree `C:/HWP_AUTODOCFIT/v1803/exfile`.
- 브랜치: `fix/example-report`.
- 반영 명령: 최종 시험 통과 후 커밋 및 `git push github fix/example-report`. 릴리스나 기존 다운로드 자산 게시 작업은 하지 않았다.
