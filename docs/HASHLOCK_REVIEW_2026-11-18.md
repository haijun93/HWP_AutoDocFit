# Python 의존성 해시 잠금 검토 (2026-11-18)

## 범위와 조사 기준

이 문서는 저장소의 `requirements*.txt`, `constraints.txt`, PyInstaller 설치·빌드, GitHub Actions의 Python 설치 경로를 grep으로 확인한 결과다. 파일을 실행하거나 패키지 인덱스와 대조하지 않았다. 버전·해시의 게시물 일치, 현재 설치 환경, 릴리스 워크플로의 실제 실행 상태는 미확인이다.

## 기존 보안 계획

관련 계획과 중복 작성하지 않고 상태를 보완한다.

- [최고 수준 보안 검토](보안성_검토_최고수준_2026-10-12.md) §S5는 당시 `requirements.txt`의 버전·해시 미고정과 `pip install --require-hashes`(pip-compile)를 계획으로 기록했다.
- [알파 보안 개선 검토](보안성_검토_알파_개선_2026-10-12.md) §S5도 CI 해시 잠금과 SBOM/attestation을 미적용 항목으로 기록했다.

2026-11-18에 확인한 현재 트리는 Windows EXE CI에 Python 해시 잠금을 이미 도입했으므로, 두 문서의 해당 상태 설명은 과거 시점 기록이다. 이 문서는 현행 적용 범위와 남은 경로만 다룬다.

## 현재 고정 수준

| 파일·경로 | 버전 고정 | 해시 고정 | 관찰 |
|---|---|---|---|
| `requirements.txt` | 직접 의존성은 `==`로 고정 | 없음 | 앱 런타임 의존성 9개. 전이 의존성은 선언하지 않음. |
| `requirements-lock.txt` | 있음 | 있음 | PyInstaller 및 전이 의존성을 포함한 27개 항목에 `--hash=sha256:`가 있다. Windows 조건부 항목 포함. |
| `constraints.txt` | 있음 | 없음 | 위 잠금 패키지 버전과 대부분 대응하고 `pip==26.2`도 제한한다. constraints는 설치 목록 자체가 아니며 해시 검증을 제공하지 않는다. |
| `requirements-mcp.txt` | 파일 내 항목은 `==` 고정 | 없음 | 30개가량의 MCP 도구 의존성. CI 설치 경로는 확인되지 않음. |
| Windows EXE CI (`build-windows-exe.yml`) | 잠금 파일을 따름 | `--require-hashes` | `.venv`에 `requirements-lock.txt`를 설치한 뒤 PyInstaller를 실행한다. |
| Native prototype CI (`build-native-prototype.yml`) | 직접 의존성과 constraints 버전 사용 | 없음 | `-c constraints.txt -r requirements.txt pyinstaller`; 해시 검증 옵션이 없다. |
| Linux GUI CI (`test-linux-gui.yml`) | 일부 직접 핀 | 없음 | 명령행에서 패키지를 개별 설치하며 버전은 별도 Windows 잠금과 다르다. |
| `build.bat` 로컬 패키지 설치 | `requirements.txt` 직접 핀 | 없음 | `-r requirements.txt pyinstaller`; 전이 패키지는 resolver가 결정한다. |

버전 숫자는 조사 시점 파일의 값이며, 공개 저장소의 최신 버전이라는 뜻은 아니다. `requirements-lock.txt`의 모든 허용 wheel이 해당 배포본의 정식 산출물인지, 잠금 파일이 어떤 도구·명령·환경으로 만들어졌는지는 파일만으로 확인할 수 없다. 생성 절차와 재현 검증은 미확인이다.

## `--require-hashes`의 영향

- pip 해시 모드는 설치 그래프의 모든 요구 패키지에 정확한 핀과 허용 해시가 있어야 한다. `requirements.txt`만 전달하거나 해시 없는 추가 패키지(`pyinstaller` 등)를 같은 설치 명령에 더하면 이 모드를 그대로 적용할 수 없다. `requirements-lock.txt`처럼 직접·전이 의존성 전체를 명시하고 모든 항목에 호환 가능한 해시를 기록해야 한다.
- PyInstaller는 Python 배포 의존성이다. 현재 Windows CI는 PyInstaller와 hook 패키지를 해시 잠금에 포함해 해당 설치 단계에서 검증한다. 로컬 `build.bat`와 native prototype CI는 여전히 해시 잠금 밖이다. 잠금 파일 갱신 시 지원 Python/Windows 아키텍처에 필요한 wheel 해시 누락이나 조건부 마커 오류가 있으면 설치가 실패할 수 있다. 현재 대상별 wheel 커버리지는 미확인이다.
- Flutter 의존성은 Python/pip 해시 잠금과 별도다. `build_flutter.ps1`은 `flutter pub get --enforce-lockfile`을 사용하고 `flutter_ui/pubspec.lock`을 사용하지만, Flutter SDK 설치·다운로드와 Pub 패키지 무결성까지 pip의 `--require-hashes`가 검증하지 않는다. native prototype 워크플로의 Python 설치 옵션도 Flutter 단계의 잠금을 대신하지 않는다.
- Rust 의존성도 별도다. 주 Windows workflow의 다수 Cargo 빌드·시험은 `--locked`와 `Cargo.lock`을 사용한다. 다만 같은 workflow에서 `hierarchy`의 `cargo test`는 `--locked` 없이 실행한다. Cargo `--locked`는 잠금 해시가 아니라 lockfile 변경 방지이며, crates 다운로드 콘텐츠의 별도 체크섬 검증과 구별해야 한다. Rust 공급망 보증 전체는 이 검토에서 검증하지 않았다.
- 따라서 pip 해시 잠금은 Python wheel 설치 경로를 좁혀 재현성과 다운로드 무결성 검사를 높이지만, 전체 빌드 도구 체인(Flutter SDK, Dart Pub, Node/npm, Rust toolchain/crates, GitHub Actions)의 출처·무결성을 한 번에 보장하지 않는다.

## 후보 방식

1. **pip-tools `pip-compile --generate-hashes`**: 기존 `requirements.txt`를 입력 파일로 유지하고, 플랫폼별/공통 출력 잠금 파일을 생성하는 방식이다. pip `--require-hashes`와 바로 맞물리고 현재 CI와의 변경 폭이 작다. 생성 도구 자체의 버전 고정, 입력·출력 파일 구분, 여러 Python/OS 대상에서의 해시와 marker 처리가 필요하다. 프로젝트의 적합 버전·설치 가능성은 미확인이다.
2. **uv lock / `uv export --locked --no-emit-workspace` 등**: 선언 파일과 `uv.lock`으로 resolver 결과를 관리하고 CI에서 lock을 고정해 설치하는 방식이다. 하나의 lock 표현으로 여러 환경을 다루기 편할 수 있지만, 현재 pip 설치 흐름 및 pip 호환 해시 requirements 출력과 이중 관리가 될 수 있다. 현재 저장소에 uv 설정은 확인되지 않았다. 도구 도입·운영 적합성은 미확인이다.
3. **현행 해시 requirements 유지 및 범위 확대**: 기존 `requirements-lock.txt`를 기준으로 로컬/CI 설치 명령을 통일하고 미잠금 보조 requirements를 별도 검토한다. 초기 변경이 작지만 잠금 생성·검토 절차와 플랫폼별 해시 검증을 명문화해야 한다.

## 단계별 도입안

1. **잠금 생성 경로를 명시한다.** 검토 대상으로 pip-tools를 우선 삼아 입력 requirements와 생성 잠금 파일을 분리하고, 도구 버전·Python 버전·플랫폼·정확한 갱신 명령을 문서화한다. 기존 잠금 파일을 재생성하기 전에는 차이를 검토하고 실제 설치 가능성을 확인한다.
2. **검증 범위를 분리해 확인한다.** Windows 배포 대상(Python 3.14, Windows runner)에서 잠금 파일 전체의 해시와 조건부 패키지를 확인한다. Linux GUI 경로는 의도적으로 별도 버전 집합인지 확인한 뒤 독립 잠금 파일 또는 기존 잠금의 재사용 여부를 정한다. MCP 의존성의 설치 위치·목적도 확인한다.
3. **기존 CI 해시 설치를 기준으로 삼는다.** `build-windows-exe.yml`의 성공 경로와 `requirements-lock.txt`를 기준선으로 기록한다. PyInstaller의 직접·전이 의존성이 모두 잠금에 들어가는지 검토한다.
4. **미잠금 설치 경로를 한 번에 하나씩 전환한다.** 로컬 `build.bat`, native prototype, Linux GUI CI를 각 목적에 맞는 해시 잠금 설치로 옮기되, 서로 다른 OS·Python 패키지 버전을 하나의 파일에 억지로 합치지 않는다. 각 전환은 실제 해당 환경에서 패키지 설치와 후속 빌드가 가능한지 검증한 뒤 진행한다.
5. **별도 생태계 잠금은 각각 검토한다.** Flutter/Pub, npm, Rust/Cargo의 기존 lockfile·`--locked` 또는 `--enforce-lockfile` 적용 경로를 독립적으로 감사한다. 누락된 명령과 SDK/action 고정은 별도 공급망 작업으로 추적한다.
6. **갱신·감사 운영을 정한다.** 주기적 취약점 감사는 현재 `dependency-audit.yml`에서 OSV Scanner로 `requirements.txt`와 Python 잠금 파일을 입력 삼는다. 해시 생성·검토 절차와 lock 변경 시 감사가 수행되는 경로를 확인하고, SBOM/attestation은 기존 보안 계획 §S5와 연결해 별도 범위로 결정한다.

## 위험과 미확인 사항

- 기존 보안 검토에서 제안한 전체 CI 해시 잠금은 아직 완료되지 않았다. 현재 `--require-hashes`는 주 Windows EXE 설치 단계에만 확인됐다.
- `build.bat`, native prototype, Linux GUI, MCP 별도 설치 경로가 각각 어떤 개발·배포 시나리오에서 실제 사용되는지 미확인이다.
- 잠금 파일 생성 명령·생성 도구 버전·재현성·다중 플랫폼 wheel 해시 범위는 미확인이다. 파일 내용만으로 `requirements-lock.txt`가 올바른 전체 해시 세트인지 단정하지 않는다.
- 해시 잠금은 허용된 아티팩트가 악성/취약하지 않음을 보장하지 않는다. 취약점 감사, 출처 검증, 빌드 환경 및 액션 고정, 서명·attestation은 별도 통제가 필요하다.
- Python 해시 모드의 적용만으로 Flutter·Rust·Node 또는 SDK/action 공급망이 잠기지 않는다. 각 도구가 실제 설치 시 사용하는 lockfile과 실행 옵션을 따로 확인해야 한다.

## 조사 명령 범위

`rg` 실행 파일이 이 PowerShell 환경에서 제공되지 않아 PowerShell `Select-String`으로 `requirements*.txt`, `constraints.txt`, workflow YAML, 빌드 스크립트 및 기존 보안 검토 문서를 검색했다. 이 문서 작성 자체 외에 설치·빌드·시험은 실행하지 않았다.
