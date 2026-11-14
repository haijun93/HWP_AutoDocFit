# T1111-KHOT 단축키 등록 결과

- 시작 및 WM_SETTINGS 처리에서 RegisterHotKey 실패 또는 파싱 불가 조합을 NIF_INFO 경고로 알린다.
- 같은 설정 문자열의 연속 실패는 억제하고, 문자열 변경 또는 성공 이후 실패는 다시 알린다.
- 트레이 아이콘을 추가한 다음 시작 등록을 수행하므로 시작 실패도 알림을 보낼 수 있다.
- 실패 중 툴팁은 일시정지 여부와 관계없이 `단축키 사용 불가`를 표시한다.
- `settings_get`과 `settings_set`은 기존 `ok`, `settings`를 유지하고 최상위 `hotkey_ok`를 추가한다.
- `settings_set`은 모든 잠금을 해제한 뒤 SendMessageW(WM_SETTINGS)로 창 스레드 처리를 기다린다. 응답의 hotkey_ok는 해당 등록이 완료된 값이며 추가 status 조회가 필요 없다.
- 새로운 타이머·폴링·상주 스레드는 없다. Windows 알림 설정에 따라 풍선 표시가 억제될 수 있다.

## 검증 명령

PowerShell에서 PATH 앞에 `$env:LOCALAPPDATA/DocFitTools/mingw64/bin`, `$env:USERPROFILE/.cargo/bin`을 추가하고 TEMP/TMP를 `E:/docfit-tmp`로 지정한다. 빌드와 시험은 하나씩 실행한다.

```powershell
cd kagent
cargo test --offline
cargo build --release --offline
python tests/khot_live.py target/release/kagent.exe
```

기존 util·lifecycle 시험은 프로세스 공용 KAGENT_TEST_ROOT를 각각 변경한다. 기본 병렬 실행에서 경합할 수 있으며, 직렬 확인은 `cargo test --offline -- --test-threads=1`로 한다.

격리 시험은 `E:/docfit-tmp/khot-root`만 사용하며 기존 status.json이 있으면 중단한다. 실제 인스턴스는 종료·설정 변경하지 않는다. 클립보드를 쓰거나 키 입력을 보내지 않는다. 시험 인스턴스는 기록을 일시정지하고 스크린샷·타임머신 감시를 끈다. Ctrl+Shift+V 충돌, V·Ctrl+Space 파싱 실패, Ctrl+Alt+Shift+F24 성공, 재충돌을 각각 settings_set·settings_get·status로 검증하고 quit으로 종료한다. Ctrl+Alt+Shift+F24도 다른 프로그램이 사용 중이면 해당 성공 검증은 실패한다.

## 2026-11-11 검증 결과

- `cargo test --offline`: 라이브러리 16 통과, 본체 107 통과·1 실패·1 제외. 기존 lifecycle 시험이 KAGENT_TEST_ROOT 환경변수 경합으로 실패했다.
- `cargo test --offline -- --test-threads=1`: 라이브러리 16, 본체 108, xl_regress 7로 총 131 통과·1 제외·실패 0. 새 단위 시험 2개 포함.
- `cargo build --release --offline`: 성공. 기존 unused/dead_code 경고 유지.
- `tests/khot_live.py`: 18개 검사 통과. 충돌·잘못된 조합에서 세 응답의 hotkey_ok=false, Ctrl+Alt+Shift+F24에서 true, 다시 Ctrl+Shift+V에서 false를 즉시 확인했다. 전용 파이프 quit 응답 ok=true, 종료 코드 0.
- 실제 인스턴스 PID 14428은 시험 전후 그대로 실행 중. 클립보드 쓰기·키 입력·실제 인스턴스 설정 변경 요청 없음.
- 첫 격리 실행에서 기존 status.json 동시 쓰기로 추정되는 여분의 닫는 괄호를 관찰했다. 전용 파이프로 종료 후, 스크립트는 util::instance_suffix와 같은 해시 및 현재 SID로 파이프를 직접 계산하도록 변경했다. 상태 파일 쓰기는 이번 범위에서 변경하지 않았다.
- 풍선 알림의 실제 화면 표시 여부는 육안 검증하지 않았다. 알림 판정·문구는 단위 시험, 실제 등록 결과는 파이프로 확인했다.
