# 공공 GitLab Runner 삭제 조치 대응

**기준일: 2026-11-09**

공공 GitLab 공지에 따라 등록 Runner가 2026년 11월 9일 15시부터 일괄 삭제될 예정이다. 저장소의 기존 `.gitlab-ci.yml`은 세 작업 모두 `security-deploy` 태그 Runner를 요구하므로, 조치 후에는 작업을 수행할 Runner를 찾지 못하고 파이프라인이 대기하거나 실패할 수 있다.

## 영향 범위

| GitLab 작업 | 기능 | 조치 후 상태 |
|---|---|---|
| `validate-source` | MR·기본 브랜치·수동 실행 시 Python 소스 검사 | Runner가 없어 실행 불가 |
| `sync-releases` | GitLab Release 메타데이터 및 자산 링크 등록 | 자동 동기화 중단 |
| `create-pages` | `website/`를 GitLab Pages로 배포 | 자동 배포 중단 |

## 즉시 조치

- `.gitlab-ci.yml`의 `workflow: rules: - when: never`로 GitLab 파이프라인 생성을 잠정 차단했다. Runner가 없는 동안 대기 파이프라인을 만들지 않는다.
- GitHub Actions의 Windows 빌드·전체 회귀시험(`build-windows-exe.yml`), 태그 릴리스(`github-release.yml`), Linux GUI 시험(`test-linux-gui.yml`)은 별도 GitHub 호스팅 Runner를 이용하므로 이 공지의 직접 영향 대상이 아니다. 각 워크플로의 브랜치·이벤트 조건에 포함된 경우에만 실행된다.
- 이미 배포된 설치 파일과 `downloads` 브랜치 파일은 Runner 삭제만으로 삭제되지 않는다. 다만 접근성·해시·다운로드는 별도 확인이 필요하며, GitLab 릴리스 페이지 및 Pages 자동 갱신은 보장되지 않는다.
- `scripts/publish_gitlab_downloads.py`는 사용자가 인증된 개발 PC에서 직접 실행하는 수동 게시 경로다. GitLab 접속 및 권한이 정상일 때만 사용할 수 있으며 Runner를 대체하는 자동화로 보지 않는다.

## 재개 조건

GitLab Runner가 공식적으로 다시 제공되거나 대체 실행 환경과 인증 방식이 승인되면, Runner 태그와 프로젝트 권한을 확인한 뒤 파이프라인을 재설계한다. `workflow: rules` 차단은 해당 조치가 검증된 후에만 해제한다. GitLab Release 동기화에 별도 토큰을 쓰게 되면 보호 변수로 보관하고 로그에 출력하지 않는다. GitLab Pages를 계속 운영할지, 별도 호스팅으로 옮길지는 실제 사용 여부와 접근 정책을 확인해 결정한다.

이 환경에서는 GitLab 원격과 Runner 관리 권한이 확인되지 않아 실시간 파이프라인 상태, Runner 삭제 완료 여부, GitLab 다운로드 링크 접근을 점검하지 않았다.
