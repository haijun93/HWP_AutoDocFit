# SETTINGSRS1 결과 — 설정 저장소 Rust 공용 라이브러리 1단계

작성: 2026-11-21 · 브랜치 `feature/SETTINGSRS1`

## 변경 내용

- `docfit_config/`: 설정 경로, JSON 읽기, 97개 기본값, Python 마이그레이션, 설정 잎 단위 검증, revision 확인, 프로세스 잠금, 임시 파일 저장 및 교체를 제공하는 Rust 라이브러리와 `settings-core` CLI.
- `docs/settings_defaults.json`: `settings_schema.DEFAULTS`의 골든 복사본. 비교 도구가 원본 Python 기본값과 키 개수 97개를 확인한다.
- `scripts/settings_compare.py`: 격리 임시 파일에서 Python 설정 서비스와 Rust `get|set|migrate`/검증 결과 비교.
- `tests/test_settings_compare.py`: Windows 전용 비교 및 Python/Rust 잠금 상호 배제 시험.
- `tests/test_configuration_service.py`는 변경하지 않았고 지정 시험만 실행했다.
- Python 애플리케이션 연결, Flutter, 창/UI는 수정하지 않았다. `hwp-auto-docfit.py` 및 실제 사용자 설정 파일도 수정하지 않았다.

의존성은 JSON 처리와 키 순서 보존을 위한 `serde_json`, SHA-256 revision 일치를 위한 `sha2`, Windows 파일 잠금·교체 API를 위한 `windows-sys`, Unix 시험 잠금용 `libc`, Rust 임시 디렉터리 시험용 `tempfile`이다.

## 차이 비교

5개 입력군(빈 파일, 부분 설정, 구버전, 이상값, 알 수 없는 키) 모두 `get`, `migrate`, 저장 후 JSON 및 검증 성공/오류 코드 비교에서 동일했다. 각 케이스마다 검증 입력 9종도 대조했다.

Python `msvcrt.locking`과 Rust `LockFileEx`는 같은 `<settings.json>.lock` 파일의 첫 1바이트에서 상호 배제됨을 Windows 시험으로 확인했다. 기본값 전수표는 Python DEFAULTS와 골든 JSON이 동일하고 최상위 97개임을 확인했다.

## 실행 명령과 결과

```text
cargo test --manifest-path docfit_config/Cargo.toml
test result: ok. 5 passed; 0 failed

python scripts/settings_compare.py --exe E:/cargo-target/SETTINGSRS1/debug/settings-core.exe
COMPARE_CASES 5 EQUAL True

python -m unittest tests.test_settings_compare tests.test_configuration_service
Ran 7 tests in 2.432s
OK

git diff --check
(출력 없음, 종료 코드 0)
```

Cargo 대상 디렉터리는 `E:/cargo-target/SETTINGSRS1`, PATH에는 `%LOCALAPPDATA%/DocFitTools/mingw64/bin`을 추가했다. 첫 Cargo 호출은 지정 PATH 없이 실행해 `dlltool.exe` 오류가 났고, 환경을 바로잡은 뒤 위 결과로 재실행했다.

## 미확인 및 차기 연결 지점

- 이 시험이 확인한 잠금 호환은 단일 Windows PC의 같은 디렉터리 로컬 파일 기준이다. 네트워크 공유·다중 사용자 프로필의 동작은 시험하지 않았다.
- Windows 강제 종료·전원 차단 직후의 파일 시스템 복구 동작은 시험하지 않았다. 저장 경로는 임시 파일 동기화 후 `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)`를 사용한다.
- Rust `f64` 숫자 파싱은 Python `Decimal`보다 극단적으로 큰 소수의 정밀도가 낮을 수 있다. 일반 범위·정수·경계·NaN/Infinity 및 길이 제한은 비교했지만, Decimal 정밀도 경계의 전수 비교는 미완료다.
- 설계 부록 B의 미확인 앱 키 전수(B-8), APPDATA/LOCALAPPDATA 로밍 정책(B-9), 다른 설정 파일은 이 카드 범위 밖이다.
- 연결 지점 제안: 다음 카드에서 Python `SettingsService` 저장 구현을 `docfit_config`의 `path` 의존 및 FFI/CLI 계약으로 대체한다. 기존 서식 엔진은 설계 부록 C대로 읽기 전용 설정 로더로 유지한다. `ConfigurationService.save`의 revision·JOB_RUNNING 경계는 호출 계층이 전달하고, Rust 저장소는 검증·마이그레이션·잠금·원자 저장의 정본이 된다. 이 카드에서는 실제 연결을 하지 않았다.
- 이번 비교에서 Python `save`는 마이그레이션된 키를 현재 설정에 `update`하므로 명시된 구형 별칭 키가 원본에 남을 수 있다. 비교기는 이 기존 Python 결과를 기준으로 하며 Rust도 저장 결과를 동일하게 유지한다. 마이그레이션 단독 출력에서는 별칭이 제거된다.

## 커밋

로컬 커밋으로만 기록한다. 푸시는 수행하지 않는다.
