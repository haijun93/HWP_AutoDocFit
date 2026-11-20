# KCLIPSET1 결과 — 몽땅클립보드 Win32 설정 창 1단계

## 변경 파일

- `kagent/src/settings_window.rs`: 네이티브 설정 창, kagent 설정 저장소 읽기/쓰기, 켜기 제한 시간과 안내, 캡처 시안, 단위 시험.
- `kagent/src/main.rs`: `--settings`, `--capture-dir <폴더>`, `--dark` 진입점과 설정 창 메시지 처리.
- `kagent/src/settings_menu.rs`, `kagent/src/popup.rs`: 빠른 목록 설정 메뉴에서 새 창 열기.

Flutter의 `kclip_dialog.dart`와 앱의 설정 단추는 건드리지 않았다. 앱 단추 연결은 후속 제안으로 남긴다.

## 현재 연결 범위와 차이

보관 개수·항목 최대 크기·전체 최대 크기·보관 기간, 자동 붙여넣기·스티커·항상 위·엑셀 목표 붙여넣기·그림 미리보기·화면캡처 감시 값은 기존 `Settings` 저장소에서 읽고 쓴다. 저장은 기존 `Settings::save`를 이용한다. 켜기는 `--enable --consent` 자식 명령을 최대 15초 기다리며 시간 초과 또는 실패 메시지를 표시하고, 성공이면 창을 닫는다. 빠른 목록 설정 메뉴 진입도 추가했다.

요청한 전체 설정 항목을 다 채우지는 못했다. 현 저장 형식에서 암호화는 항상 적용되어 토글이 없고, 캡처 동작은 화면캡처 폴더 감시와 폴더 제외만 있다. 제외 앱·폴더별 제외는 현재 읽어 표시만 하며 편집하지 못한다. 단축키는 표시만 하고 변경할 수 없다. 자동 실행 체크, 동의 후 자동 실행 선택, 일시정지·모두 지우기·끄기 UI도 이 버전에 완성되지 않았다. 캡처 3종 결정은 저장소에서 근거 문서를 찾지 못해 새 설정 키를 만들지 않았다. 따라서 이 결과는 요청한 1단계 전체 완료가 아니라 연결 기반의 부분 구현이다.

설정 창 UI는 표준 Win32 체크·콤보·단추를 사용한다. 현재는 다크/밝은 팔레트 배경과 정적 GDI 캡처 시안만 확인했다. 실제 HWND를 캡처한 결과가 아니므로 IME 한글 편집, Tab/Space/Enter/Esc, 고DPI·고대비에서의 실제 컨트롤 동작은 미확인이다.

## 캡처 파일과 관찰

- 라이트: `E:\docfit-tmp\kclipset1-captures\kclip-settings-light.png`
- 다크: `E:\docfit-tmp\kclipset1-captures\kclip-settings-dark.png`

두 PNG를 직접 열어 확인했다. 글자 잘림·겹침은 보이지 않았고 문구 간 간격은 확보되어 있다. 다만 실제 컨트롤 모양·체크 상태·창 테두리는 시안에 포함되지 않아 이 캡처를 최종 UI 검수로 볼 수 없다.

## 실행한 명령과 결과

```text
cargo test --manifest-path kagent/Cargo.toml --bin kagent settings_window -- --nocapture
running 4 tests
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 154 filtered out

cargo test --manifest-path kagent/Cargo.toml --bin kagent --quiet
running 158 tests
test result: ok. 156 passed; 0 failed; 2 ignored; 0 measured; finished in 16.40s

cargo build --manifest-path kagent/Cargo.toml --bin kagent
Finished `dev` profile [optimized + debuginfo] target(s) in 9.09s

python -m unittest tests.test_kagent_settings_menu tests.test_kclip_service
Ran 44 tests in 0.928s
OK

& 'E:/cargo-target/KCLIPSET1/debug/kagent.exe' --capture-dir $cap
& 'E:/cargo-target/KCLIPSET1/debug/kagent.exe' --capture-dir $cap --dark
PNG 2? ??: ??? 58,402 bytes, ?? 63,825 bytes
```

GNU 링크 도구가 필요해 Cargo 실행 쉘의 PATH 앞에 `%LOCALAPPDATA%/DocFitTools/mingw64/bin`을 두었고 `CARGO_TARGET_DIR` 환경변수만 사용했다. 캡처는 kagent를 설치·재시작하지 않고 `KAGENT_TEST_ROOT` 임시 데이터와 `--capture-dir`로 생성했다. `hwp-auto-docfit.py`와 `.cargo/config.toml`은 변경하지 않았다.

## 사용자 화면 확인 및 미확인

- 실제 `--settings` 창의 렌더링과 빠른 목록 메뉴 진입을 사용자가 확인해야 한다.
- 실제 켜기 성공 후 창 닫힘, 제한 시간 초과·실패 안내, 실제 agent 설정 반영은 실기 확인하지 않았다.
- 설정 항목 전체 구현, 제외 앱·폴더 편집, 단축키 편집, 자동 실행, 동의 후 자동 실행, 캡처 3종, 고DPI·고대비, 키보드 접근성, IME 입력은 미완료/미확인이다.
