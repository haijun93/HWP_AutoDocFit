# OSVFIX 결과 (2026-11-09)

## 범위와 조사

- 기준: `github/main` (`eb9e64c5d08617990ac8f4da1eea624cf48a6cf6`), 작업 브랜치 `fix/osv1109`.
- OSV API batch: Python 입력, kagent/klib/editor_core Cargo.lock, flutter_ui/pubspec.lock의 총 293개 패키지 질의. 잠금 파일 질의는 0건.
- 지정 가상환경 `C:/Users/haiju/OneDrive/바탕 화면/HWP_AutoDocFit/mapo-agent-1/.venv`에 `pip-audit 2.10.1` 설치 후 전체 환경 검사.

## 발견 및 수정

| ID | 패키지 | 설치 버전 | 최소 수정 버전 |
|---|---|---:|---:|
| PYSEC-2026-4320 | certifi | 2026.7.22 | 2026.10.18 |
| PYSEC-2026-4327, -4328, -4326, -4339, -4340, -4343, -4334, -4337, -4341, -4330, -4338, -4335, -4336, -4331, -4332, -4329, -4333, -4342; CVE-2026-112895, -112400, -112897, -112395, -112404, -112899, -112900, -112893, -112891, -112390, -112401, -112391, -112894, -112406, -112399, -112892, -112393, -96638, -96640, -96778; GHSA-78g4-xqwv-q3g8 | Pillow | 12.3.0 | 13.0.0 |
| PYSEC-2026-1795, -1796, -2875, -2876, -196, -3721 | pip | 25.2 | 26.2 |
| GHSA-xmj3-9gf7-h52w | setuptools | 84.0.0 | 84.1.0 |

업데이트 후 `pip-audit` 결과: **No known vulnerabilities found**. 저장소의 `constraints.txt`와 `requirements-mcp.txt`는 이미 취약하지 않은 버전으로 고정돼 있어 소스 의존성 변경은 필요하지 않았다.

## 검증

- Python `unittest discover -s tests`: 실행 시작 후 통과 점 출력이 진행됐으나 도구 세션이 종료돼 최종 요약은 수집되지 않음.
- `kagent`: `cargo test --locked --lib`, 16 passed.
- `klib`: 전체 `cargo test --locked`는 기존 Cargo 프로세스의 빌드 잠금 대기 중 완료 결과 미수집. `--lib`은 library target이 없는 binary-only crate라 해당하지 않음.
- `editor_core`: 완료 결과 미수집.
- `flutter analyze`: 분석이 시작됐으나 도구 세션에서 최종 결과 미수집.
- OSV 잠금 파일 결과: 293개 패키지 쿼리 중 취약점 0건.

COM 시험은 수행하지 않음 (이 작업은 의존성 감사/테스트이며 HWP COM을 사용하지 않음).
