# AGENTS.md (Codex 등 AI 코딩 에이전트용)

이 저장소에서 작업하기 전에 **`HANDOVER.md`를 먼저 읽으세요.** 개발 환경, 코드 구조, 작업 규칙, 배포 절차, 알려진 함정이 모두 정리되어 있습니다.

꼭 지킬 것:
- Windows + 한/글 2020 COM 환경의 Python(`.venv\Scripts\python.exe`) 앱입니다. 테스트: `python -m unittest discover -s tests`
- **★ 2026-11-11 사용자 지시: GitLab(`origin`) 저장소는 별도 명령 때까지 절대 건드리지 않는다(푸시·태그·삭제·릴리스·downloads·API 모두 금지). 푸시는 `github`에만. 이 PC·맥의 git 설정과 pre-push 훅이 GitLab 푸시를 막고 있으며 되돌리거나 우회하지 말 것.**
- 알파·감마 버전은 폐지(사용자 지시 2026-11-12, `alpha` 브랜치 삭제). 새 기능은 기능 브랜치(`feature/…`)에서 만들어 시험 통과 후 `main`에 병합. 푸시는 `github`에만(GitLab 미러링은 위 지시로 중단)
- XML 파싱은 `defusedxml`만(GitLab 보안 검사). `hwp-auto-docfit.py`의 CRLF 줄바꿈 보존
- 사용자 문서 원본은 수정하지 말고 사본으로 시험. 작업이 끝나면 요청이 없어도 자동으로 커밋하고 지정 브랜치에 푸시(사용자 지시, 2026-10-07)
- 사용자와는 한국어로 소통
- 한/글 COM 시험은 `C:\HWP_AUTODOCFIT\.com-test.lock` 잠금 필수
