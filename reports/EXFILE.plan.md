# EXFILE 계획

- 작업 기준: github/main b883eb25, 전용 worktree `C:/HWP_AUTODOCFIT/v1803/exfile`, 브랜치 `fix/example-report`.
- 사용자 입력 `C:/Users/haiju/OneDrive/바탕 화면/시연용예시보고서.txt`는 읽기만 한다. SHA-256과 원래 바이트를 기록하고 종료 때 동일성을 확인한다.

## 원본과 사용 경로

| 구분 | 경로와 동작 |
|---|---|
| 배포 TXT 원본(교체 대상) | `release-assets/보고서 예시 파일(한번에 적용).txt`. 기존 UTF-8, BOM 없음, LF 66개. 사용자 입력은 UTF-8, BOM 없음, CRLF 94개이므로 저장소 원본의 LF 규칙에 맞춰 내용만 교체한다. |
| 앱의 파일 입력 | `docfit_engine/formatter_utils.py`의 `텍스트파일_읽기`가 사용자가 추가한 경로를 UTF-8-sig/UTF-8/CP949 순으로 읽는다. 배포 TXT를 자동으로 찾아 읽는 코드나 resources 사본은 없다. |
| 앱의 서식 예시 확인 | `hwp-auto-docfit.py`의 `서식예시_글` → `docfit_ui/legacy.py`의 `_서식예시_시작` → 임시 TXT/HWPX 변환·서식 적용. 배포 TXT와 별개인 내장 예시이며 이번 교체 대상은 아니다. `resources/서식예시/*.hwpx`는 제목·개요 등 서식 원본으로 보존한다. |
| GitHub 배포 사본 | `.github/workflows/github-release.yml:55`에서 위 TXT를 `report-example-all-in-one.txt`로 복사하고 한국어 표시 이름을 달아 릴리스에 첨부한다. 기존 ZIP 경로에서는 한국어 파일명 그대로 ZIP에 넣는다. |
| GitLab 배포 사본 | `scripts/publish_gitlab_downloads.py`의 `EXAMPLE`이 위 TXT를 가리키고 임시 다운로드 브랜치 루트에 한국어 이름 그대로 `shutil.copyfile`한다. |
| 안내/시험 | `docs/USER_GUIDE.md`의 현재 실습 설명을 새 내용에 맞춘다. 기존 이미지·영상·성능 기록은 이전 예시로 측정한 증거이므로 그 사실을 명시하고 보존한다. `tests/test_format_example.py`는 별도 내장 예시 검증이고, 배포 TXT 직접 참조 시험은 검색 결과 없다. 배포 TXT를 실제로 읽어 제목·개요 표 변환, 본문·주석·붙임 보존을 검증하는 회귀 시험을 추가한다. |

## 검증 및 완료

1. 파일 이름·배포 경로를 유지하고 사용자 입력 내용을 LF로만 정규화한다. 내용·해시·인코딩을 확인한다.
2. 새 예시에는 기존 ‘침공계획 개요’ 박스 그림 표가 없으며, 제목·개요 준말과 □/ㅇ/-/*/**/※, 2개 붙임 목록이 있다. 없는 일반 표를 추가하지 않는다.
3. 관련 시험과 `python -m unittest discover -s tests`, `flutter analyze`를 실행하고 수치 및 환경 의존 문제를 별도 기록한다. COM 실측은 공용 `.com-test.lock`을 획득할 수 있을 때만 진행한다.
4. `reports/EXFILE.final.md` 작성 후 커밋하고 `github fix/example-report`에 푸시한다. 릴리스/기존 다운로드 자산 게시 작업은 하지 않는다.
