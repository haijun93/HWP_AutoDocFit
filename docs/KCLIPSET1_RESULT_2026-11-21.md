# KCLIPSET1 결과 — 몽땅클립보드 Win32 설정 창 1단계

## 변경 파일

- `kagent/src/settings_window.rs`: kagent 설정 창의 화면 그리기·컨트롤, 설정 읽기/쓰기, 로그인 자동 실행, 동의·켜기, 15초 제한, 라이트/다크 캡처.
- `kagent/src/main.rs`: `--capture-settings <폴더>`를 캡처 진입점에 연결(기존 `--settings`, 빠른 목록 ⚙ 메뉴 연결은 앞선 작업에 이미 포함).
- `docs/KCLIPSET1_RESULT_2026-11-21.md`: 이번 구현 범위·캡처·시험·미확인 기록.

`flutter_ui/lib/features/kclip/kclip_dialog.dart` 및 `docfit_app/kclip_service.py`는 수정하지 않았다. 앱의 [몽땅클립보드 설정] 버튼 연결은 후속 제안으로 남겼다.

## 구현 범위와 차이

설정 창은 kagent의 `Settings` 저장 모델과 `ui_theme::Palette`를 사용한다. 보관 개수·항목 최대/전체 최대 용량·보관 기간, 스티커·항상 위·즉시 붙여넣기·엑셀 목표 붙여넣기·그림 미리보기·화면캡처 감시를 읽고 저장한다. 암호화는 kagent 저장소가 DPAPI로 항상 적용하므로 “항상 켜짐”으로 표시한다. 단축키는 현재 값, 캡처 종류는 글·그림·파일 모두 허용으로 표시한다.

제외 앱과 폴더별 캡처 제외는 현재 설정 모델에 값은 있지만 이 창에서는 표시 전용이다. 단축키 변경과 목록 편집도 표시 전용으로 남겼다. Windows 로그인 자동 실행은 체크박스로 읽고 변경한다. 동의 안내·동의 확인·켜기 성공 시 닫기 흐름을 제공하고, 자식 `--enable --consent` 명령은 최대 15초 기다린 후 시간 초과/실패 안내를 표시한다. 전체 설정 창 진입은 `kagent --settings`, 빠른 목록 ⚙ 메뉴의 설정 항목에서 가능하다.

설계 문서 ④의 모두 지우기 확인 및 켜진 뒤 일시정지·끄기 관리는 기존 빠른 목록 설정 메뉴에 남아 있다. ⑤의 `ui_kit` 신규 공용 컨트롤은 추가하지 않았고 기존 `ui_theme` 팔레트와 Win32 기본 컨트롤을 사용한다. 앱 버튼 연결·Flutter/Python 설정층 제거는 하지 않았다.

## 캡처와 화면 관찰

캡처 모드는 창·트레이·단일 인스턴스를 띄우지 않고 메모리 DIB를 만든 뒤, 실제 창의 `WM_PAINT`와 공유하는 `paint_surface`를 호출해 PNG를 만든다. 캡처 입력은 `KAGENT_TEST_ROOT` 임시 자료 폴더를 사용했다.

- 라이트: `E:\kclipset1-captures-final\kclip-settings-light.png`
- 다크: `E:\kclipset1-captures-final\kclip-settings-dark.png`

두 파일을 이미지 뷰어(Read 도구)로 각각 직접 열어 확인했다. 두 화면 모두 제목·소개, 보관 개수/항목 크기/전체 용량/기간 콤보, 단축키·암호화 표시, 6개 동작 체크, 캡처 3종, 제외 앱·폴더 표시, 자동 실행, 동의, 저장/켜기/닫기 버튼이 보인다. 겹침이나 잘림은 보이지 않았고, 긴 제외 앱 목록은 말줄임으로 처리됐다. 다크 화면은 콤보·체크·버튼 배경과 글자 대비를 보완한 뒤 다시 캡처했다.

## 실행한 명령과 결과

```text
cargo test --manifest-path kagent/Cargo.toml --bin kagent settings_window --quiet
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 154 filtered out

cargo test --manifest-path kagent/Cargo.toml --bin kagent --quiet
running 158 tests
test result: ok. 156 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out

python -m unittest tests.test_kagent_settings_menu tests.test_kclip_service
Ran 44 tests in 0.696s
OK

cargo build --manifest-path kagent/Cargo.toml --bin kagent --quiet
성공 (기존 Rust 경고만 출력)

kagent.exe --capture-settings E:/kclipset1-captures-final
kagent.exe --capture-settings E:/kclipset1-captures-final --dark
PNG 2개 생성: kclip-settings-light.png 126,167 bytes, kclip-settings-dark.png 128,974 bytes
```

Cargo 명령에는 `%LOCALAPPDATA%/DocFitTools/mingw64/bin`을 PATH 앞에 두고 `CARGO_TARGET_DIR=E:/cargo-target/KCLIPSET1` 환경변수만 사용했다. 캡처 실행은 별도 `KAGENT_TEST_ROOT=E:/kclipset1-settings-temp`에서 수행했다. 실제 몽땅클립보드 프로세스를 종료·재시작·설치하지 않았다. `hwp-auto-docfit.py`와 `.cargo/config.toml`은 변경하지 않았다.

## 사용자 화면 확인 및 미확인

- 사용자가 실제 kagent 창을 띄워 Win32 기본 컨트롤의 고DPI·고대비 표현, Tab/Space/Enter/Esc 동작, 설정 저장 후 빠른 목록 반영을 확인해야 한다. PNG는 공용 `paint_surface`의 화면 시안이며 Windows의 실제 자식 컨트롤·창 테두리를 캡처한 실기 화면은 아니다.
- 한글 IME 입력칸은 만들지 않았다. 현재 편집 가능한 값은 콤보·체크뿐이고, 단축키/제외 앱/폴더 입력 편집은 표시 전용이다.
- 로그인 자동 실행 레지스트리 변경, 실제 켜기 성공 후 닫기, 실제 제한 시간 초과·실패 안내는 사용자 앱에 영향을 주지 않도록 실기 실행하지 않았다.
- 저장·설정 왕복, 범위 밖 잘못된 값 거부, 15초 제한 상수, DPI별 레이아웃 계산은 위 kagent 시험에서 확인했다. Python 시험도 지정된 kagent 설정 메뉴·kclip 서비스 파일만 실행했다.
