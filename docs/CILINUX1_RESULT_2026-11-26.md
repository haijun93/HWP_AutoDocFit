# CILINUX1 결과 — Linux core and real Tk regression

- 대상: `feature/CILINUX1`, 시작 커밋 `43e59a14` (`github/main` 기준 작업 사본).
- 범위: CI 실패 2건의 원인 구분, 재현성 수정, Windows 전체 unittest 확인. 푸시·태그·배포는 수행하지 않는다.
- 판정: **부분 완료**. ZIP 해시 결함은 수정했다. 파이프 시험의 OS 모의 격리 수정은 기존 시험 변경 금지 규칙의 예외 허용 응답이 없어 미적용했다. 따라서 이 커밋만으로 Linux CI 실패 2건이 모두 해결됐다고 판정하지 않는다.

## CI 실행 조건

- `.github/workflows/test-linux-gui.yml`: `main` 푸시, PR, 수동 실행에서 동작한다. 기능 브랜치 푸시 자체는 실행 조건이 아니다.
- Ubuntu 최신 runner, Python 3.14, 제한 15분. Xvfb에서 실제 TkDND 창 생성·갱신·종료를 먼저 검증한다.
- 전체 명령: `xvfb-run -a python scripts/linux_stub_test_runner.py --require-gui discover -s tests`.
- 실행기는 Windows 전용 모듈만 대체한다. Tk/TkDND·pywebview·XML은 실제 모듈을 쓰며, GUI 부재로 인한 skip은 실패로 처리한다.
- 워크플로 및 실행 조건, 기존 skip 정책은 변경하지 않는다.

## 원인과 수정

### 합성 fixture manifest 해시 불일치

- 생성기의 실제 이식성 결함이다. `_FixedZip`은 시각·권한·압축 방식은 고정하지만 ZIP 중앙 디렉터리의 `ZipInfo.create_system`을 고정하지 않았다.
- Python 기본값은 Windows `0`, Linux/macOS `3`이다. 커밋된 Office fixture 5개는 모두 `0`이다.
- Windows에서 ZIP 모듈의 호스트 선택만 Linux로 모의했을 때 DOCX 3개·XLSX 1개·PPTX 1개의 SHA-256 불일치를 재현했다. 텍스트 fixture 6개는 일치했다.
- `docfit_core/conversion_fixtures.py`에서 생성 OS 값을 기존 fixture와 같은 `0`으로 고정했다. manifest와 커밋된 fixture는 수정하지 않았다.
- `tests/test_conversion_fixture_portability.py`를 추가했다. Windows·Linux·macOS의 ZIP 호스트 선택을 각각 모의하고 manifest 전체, Office 파일 바이트, 모든 ZIP 항목의 생성 OS 값을 검사한다.
- 수정 전 소스를 메모리에서 실행한 대조에서는 Linux·macOS subtest 2건이 실패했다. 수정 후에는 3개 호스트 조건 모두 통과했다. 이는 실제 Linux 실행과 구분한다.

### 파이프 준비 상태 시험의 None 반환

- 시험 격리 결함이다. 기존 시험의 `patch("docfit_app.rust_scan.os.name", "nt")`는 공유 `os` 모듈을 바꾸므로 `pathlib`와 `library_scope`에도 영향을 준다.
- 코드 추적상 Linux에서 Windows 파이프를 모의하려다 Windows 경로 구현까지 선택한다. 기본 스캔 루트 구성 중 발생한 예외는 `names_query`의 폴백 처리에 의해 `None`이 되고, 이후 시험의 행 순회에서 TypeError가 된다. Linux에서 직접 재현한 결과는 아니다.
- 실제 서비스 상태 전이는 유지해야 한다: 준비된 서비스는 필터링한 행 목록, 구축 중·사용자 모드·연결 실패·잘못된 응답은 `None`, 이후 서비스→klib→Python 순으로 폴백한다.
- 기존 시험 변경은 [팀 운영 규칙 3절](TEAM_OPERATING_RULES.md)의 “기존 시험은 고치지 않는다(고쳐야 하면 되돌리고 보고)” 제한에 해당하여, 검증 조건을 유지한 OS 모의 격리 수정의 예외 허용을 요청했다. 응답이 없어 기존 시험은 변경하지 않았다.
- 적용이 필요한 수정안은 준비 상태 시험과 구축 중·연결 실패 시험의 두 컨텍스트를 아래처럼 바꾸는 것이다. 표준 `os` 모듈과 `pathlib`는 실제 호스트 상태를 유지하고, `rust_scan`이 참조하는 `os`만 모의한다. 기존 입력·assertion·폴백 판정은 그대로 유지한다.

```python
with patch("docfit_app.rust_scan.os", wraps=os) as platform_os:
    platform_os.name = "nt"
    # 기존 시험 본문 유지
```

- 저장소 밖에 준비한 수정안은 임시 모듈로 읽어 지정 Python에서 6개 시험을 실행했다: `Ran 6 tests in 0.059s / OK`. 기존 assertion 행이 동일함도 확인했다. 이 검증은 저장소의 기존 시험 변경이나 Linux 직접 실행을 의미하지 않는다.

## 검증 현황

- 지정된 venv315 Python 3.15.2에서 수정 전 대상 시험: 스캔 캐시 `Ran 6 tests in 0.120s / OK`, 변환 충실도 `Ran 42 tests in 6.775s / OK`. 두 실패는 기본 Windows 조건에서는 재현되지 않았다.
- 신규 ZIP 이식성 회귀 시험: `Ran 1 test in 0.455s / OK`.
- Windows 전체 시험: 다른 전체 unittest 실행 종료 후 COM 잠금을 획득하고, 사용자 지정 venv315 Python 3.15.2로 `-m unittest discover -s tests`를 실행했다. `KLIB_EXE`를 지정했고 임시 폴더와 원시 로그는 작업 트리 밖에 두었다. 종료 코드 **0**, 실제 출력은 다음과 같다.

```text
Ran 3707 tests in 1271.411s

OK (skipped=23)
```

- 전체 시험에는 신규 ZIP 이식성 회귀 시험 1개가 포함된다. 사용자가 제공한 과거 CI 3691개와 이 작업 사본의 3707개는 기준 소스·OS가 다르므로 동일 모집단 수치로 비교하지 않는다.
- `OK` 출력 후 `tests/__init__.py`의 atexit 임시 샌드박스 정리에서 `timeline-content.sqlite` 파일 사용 중에 따른 `PermissionError [WinError 32]`가 출력됐다. unittest 실패·오류로 집계되지는 않았고 프로세스 종료 코드도 0이었다. 이 후처리 문제는 이번 CI 실패 2건의 수정 범위에서 별도로 남긴다.
- 전체 시험이 재생성한 `timemachine_extract` fixture 바이너리 4개(DOCX/HWPX/PPTX/XLSX)는 원복했다. COM 잠금은 시험 종료 후 해제했고 시험 전부터 있던 한/글 프로세스 2개는 유지됐다.
- Linux 직접 실행: 수행하지 못했다. 이 PC에는 사용 가능한 WSL 배포판이 없으며 Linux CI를 새로 실행하거나 통과를 확인하지 않았다.
- 사용자 문서 원본, fixture 바이너리, manifest, 본체 CRLF, XML 파서는 변경하지 않았다. 저장소에 PC 전용 경로를 추가하지 않았다.

## 남은 작업

- 기존 파이프 시험의 OS 모의 컨텍스트 2곳 수정에 대한 예외 허용 후 적용·검증.
- Linux에서 전체 실제 Tk CI 재실행 및 두 실패 해소 확인. 이번 작업에서는 푸시·원격 CI 실행을 하지 않았다.

## PM 판정(2026-11-26)
기존 시험의 OS 모의 범위 수정 2곳을 허용해 적용했다(assertion 불변). tests.test_scan_cache_service: Ran 6 / OK(Windows). Linux 확인은 main 병합 뒤 CI 로 한다.
