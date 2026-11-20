# WinRust 벤치마킹 2단계 — 기법별 적용 점검 (2026-11-20)

카드 WINRUST2. 근거는 ① TODO.md의 WinRust 두 항목(요약 설명)과 ② 우리 저장소 코드를 직접 읽은 결과뿐이다.

## 0. 작성 원칙·한계

- **클린룸**: Lemxy/WinRust에는 LICENSE가 없다. 그 저장소의 소스를 열람·복사·인용하지 않았고, WinRust 쪽 서술은 TODO에 이미 적힌 요약만 썼다. WinRust 쪽 동작·수치는 전부 **미확인(TODO 서술 그대로)**이다. WinRust는 설치·실행하지 않았다.
- 시스템 교체·변경 기능(검색 서비스 끄기, 탐색기 대체, 서비스 변경, 앱 삭제)은 우리 앱에 들이지 않는다.
- 코드는 바꾸지 않았다. 이 PC의 실행 중 앱·서비스는 건드리지 않았고, 설치본은 `ls -l`로 크기만 읽었다.
- 줄 번호는 `feature/WINRUST2`(github/main 기준) 시점이다.

## 1. 항목별 점검 (현황 → 차이 → 개선안 → 측정 필요 여부)

### ① 파일 목록: MFT 일괄 읽기 + USN 저널 + 파이프 질의
- **현황**: `kfind/names/src/ntfs.rs:19-21`에 `FSCTL_ENUM_USN_DATA`·`FSCTL_QUERY_USN_JOURNAL`·`FSCTL_READ_USN_JOURNAL`을 쓴다. `enum_mft`(145행)는 1MiB 버퍼로 MFT를 일괄 열거하고, `read_usn`(174행)은 `wait_secs`로 새 기록을 막아 기다리며 따라간다. 저널이 잘렸으면 `Error::Rescan`(`:204-209`)으로 전체 재색인한다. 질의는 tokio 이름 있는 파이프(`crates/kfind/src/pipe.rs:21,218`)이고, 질의 하나의 패닉은 `pipe.rs:873-879`에서 격리한다.
- **차이**: 방식은 같다(WinRust도 같은 기법이라고 TODO에 적혀 있음). 차이는 수치뿐이다. WinRust는 C: 전체 색인 약 53MB, 전체 검색 45ms를 보고한다(TODO 서술, 미확인). 우리 수치는 이 문서에서 재지 않았다.
- **개선안**: 구조 변경은 필요 없다. 1단계 벤치마크에 같은 조건(검색어 10개·첫/반복 질의·색인 메모리)으로 넣는다.
- **측정 필요: 예**(색인 메모리·갱신 지연·질의 시간. 이 PC에서는 재지 않음).

### ② 화면 그리기: GDI+ 둥근 모서리·안티앨리어싱, DWM, 고DPI
- **현황**: 우리는 GDI+를 쓰지 않는다(kagent·ktm·kvf·dock·ui_theme·klib·crates 전체에서 `gdiplus` 문자열 검색 결과 0건). 순수 GDI(`Win32_Graphics_Gdi`)로 그리고, 모서리·테두리는 DWM에 맡긴다. kagent `frame.rs:79-90`·ktm `frame.rs:112-130`이 `DWMWA_WINDOW_CORNER_PREFERENCE`(33)로 모서리를 두르고 `DwmExtendFrameIntoClientArea`를 쓴다. 테두리는 `DWMWA_BORDER_COLOR`(34)(`kagent/src/frame.rs:101`, `ktm/src/frame.rs:149`)이고, DWM이 안 되면 `own_border`로 직접 그린다(`kagent/src/frame.rs:106`). 고DPI는 세 곳이 `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)`를 쓴다(`kagent/src/main.rs:1103`, `ktm/src/window.rs:4045`, `kvf/src/main.rs:86`). 크기 환산은 `GetDpiForWindow`(`kagent/src/frame.rs:119-121`, ktm은 시험용 강제 DPI 포함 `frame.rs:179-188`)이다.
- **차이**: 창 자체의 둥근 모서리·DPI는 이미 맞다. 다만 창 안의 그림(독 알약·아이콘)은 GDI라서 안티앨리어싱이 없다. 독 아이콘은 벡터 도형(`dock/src/lib.rs:266 icon_shapes`)을 `native.rs:296 draw_icon`으로 GDI에 그린다. WinRust 쪽 그리기 세부는 미확인.
- **개선안**: 독 알약처럼 큰 둥근 도형만 GDI+ 또는 Direct2D 대신 "지름 확대 후 축소(수퍼샘플링)"로 시험한다. 새 의존은 늘리지 않는다. 눈으로 보는 확인이 필요하다.
- **측정 필요: 예**(독 그리기 시간·화면 품질을 4K·150% DPI에서).

### ③ 공용 스타일 파일 하나로 모든 앱 테마 즉시 반영
- **현황**: 공용 크레이트 `ui_theme`(`ui_theme/src/lib.rs` 543줄, `win.rs` 222줄)가 팔레트(`Palette`, `:42`), 선호 해석(`resolve`, `:295`), 시스템 앱 모드·고대비 읽기(`win.rs:25,38`), 전체 알림(`broadcast_changed`, `win.rs:124`)을 맡는다. 사용처는 kagent·kagent/ffi·ktm·kvf의 `Cargo.toml`이다. 선호값의 출처는 kagent `settings.json`의 "theme"(`lib.rs:254` 주석)이다.
- **차이**: WinRust는 `%ProgramData%\WinRust\style.txt` 파일 하나를 공유한다(TODO). 우리는 파일 공유가 아니라 레지스트리 시스템 설정 + 메시지 방송 방식이다. **독(dock)은 `ui_theme`를 쓰지 않는다**(`dock/Cargo.toml`에 의존 없음, `dock/src`에서 `ui_theme` 검색 0건). 독은 자체 `palette_for_mode`(`dock/src/lib.rs:277`)를 쓴다. 따라서 독의 테마는 `ui_theme`와 따로 놀 수 있다.
- **개선안**: 독도 `ui_theme`에 올린다(독 색 계산을 `Palette`로 이관). 스타일 파일 방식은 공공기관 PC에서 이득이 적어 도입하지 않는다.
- **측정 필요: 아니오**(기능 정합성 확인으로 충분).

### ④ 떠 있는 독(자동 숨김·최대화 창 처리)
- **현황**: `dock/src/native.rs:204-236`의 `foreground_changed`가 포그라운드 창이 모니터 전체를 덮으면(`:226-229`) 독을 숨기고(`SW_HIDE`), 아니면 다시 보인다. 위치는 `reposition`(`:95-129`)이 작업 영역·DPI로 계산하고 `HWND_TOPMOST`로 둔다(`WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_TOPMOST`, `:57`). 호버 확대·툴팁은 `lib.rs:108`·`:283`에 있다.
- **차이**: WinRust는 최대화 창 위에서 "미끄러져 숨고, 화면 아래 가장자리에 마우스를 대면 다시 나타난다"(TODO). 우리는 즉시 숨김뿐이고 슬라이드 애니메이션·가장자리 호버 재등장은 없다. 즉 최대화 창(전체화면이 아닌 작업 영역 최대화)에서는 독이 계속 떠 있다. `full` 판정이 모니터 전체 사각형 기준이라 작업표시줄을 남기는 일반 최대화는 숨기지 않는다.
- **개선안**: (a) 최대화 창 판정을 `IsZoomed`·작업 영역 기준으로 확장, (b) 숨김 중 화면 아래 가장자리 1~2px 감지 영역을 두고 호버하면 재등장, (c) 슬라이드는 레이어드 창 위치 보간. 순수 로직(`lib.rs`)에 상태 전이를 두면 시험하기 쉽다.
- **측정 필요: 아니오**(기능·체감 확인. 숨김/재등장 지연만 눈으로 확인).

### ⑤ WIC 사진 해독·보기(몽땅사진첩)
- **현황**: `kagent/src/wic.rs`(300줄)가 WIC를 직접 쓴다. JPEG는 `IWICBitmapSourceTransform`으로 1/2·1/4·1/8 축소 해독(`:1-6` 주석), 그 외는 BGRA 줄 묶음 복사 후 `boxscale`로 축소한다. COM 메서드 표는 `windows-sys`에 없어 직접 적었다(`:8`). 회전(EXIF 방향 ≠ 1)은 None을 주고 탐색기 썸네일로 넘긴다. 크게 보기 화면은 이 PC의 Rust 코드에는 아직 없다(미확인: 몽땅사진첩 Rust 화면 계획만 TODO에 있음).
- **차이**: WinRust의 사진 보기는 약 420줄(TODO). 우리는 썸네일 해독은 이미 WIC이고 "큰 보기 창"만 없다. WebP·HEIC·AVIF 코덱 지원 여부는 미확인.
- **개선안**: 큰 보기 창은 `wic.rs`의 해독을 재사용하고 새 해독 코드를 만들지 않는다. EXIF 회전은 `IWICBitmapFlipRotator`로 직접 처리하면 탐색기 썸네일 경로가 필요 없어진다.
- **측정 필요: 예**(큰 사진 열기 시간·메모리).

### ⑥ 공용 크레이트 분리(언어 문자열·MFT·설정·스타일)
- **현황**: 분리된 것: 스타일 `ui_theme`, 검색 엔진 `kfind/core`·`kfind/names`, 편집기 `editor_core`, 위계 `hierarchy`(klib가 경로 의존, `klib/Cargo.toml`). MFT 읽기는 `kfind/names/src/ntfs.rs`에만 있고 독립 크레이트가 아니라 `kfind-names`의 한 모듈이다. 설정은 각 앱이 `settings.json`을 따로 읽는다(독은 `dock/src/lib.rs:206 parse_hidden_tabs`, `native.rs:420 hidden_tabs`). 언어 문자열 공용 크레이트는 없다(⑩ 참고).
- **차이**: WinRust는 `common/src/mft.rs`로 MFT 읽기를 여러 프로그램이 공유한다(TODO). 우리는 같은 엔진을 몽땅사진첩·문서뷰폴더가 쓰려면 `kfind-names`를 통째로 끌어와야 한다. 또 크레이트마다 `[workspace]`를 따로 선언해(`ktm/Cargo.toml:9`, `dock/Cargo.toml:7`, `kfind/*/Cargo.toml`) 루트 워크스페이스가 없고, 의존성 잠금·프로필이 크레이트별로 흩어져 있다(⑨ 참고).
- **개선안**: (a) `ntfs.rs`를 `kfind-mft` 같은 작은 크레이트로 분리(라이선스가 PolyForm Strict이므로 분리 후에도 같은 조건 유지), (b) 설정 읽기(`settings.json`)를 공용 크레이트로. 지금은 사용처가 둘뿐이라 급하지 않다.
- **측정 필요: 아니오**.

### ⑦ 설치 프로그램: 구성요소 선택·부분 실패 계속·되돌림·설정 유지
- **현황**: `installer/setup.iss`(118줄). 구성요소 선택 없음(`[Components]` 구역 없음, `[Tasks]`는 바탕화면 바로가기 하나, `:33-34`). 파일은 `dist` 전체를 한 번에 복사(`:38`). 설치 마지막에 ktm·kvf를 `--install --respect-disabled`로 부른다(`:44-48`, `runhidden waituntilterminated skipifdoesntexist`). `--respect-disabled`로 사용자가 끈 기능은 업데이트해도 켜지 않는다(설정 유지, `:45` 주석). 제거 때는 사용자 권한 kfind 종료(`:55`), ktm·kvf `--uninstall`(`:57,59`), 서비스 존재 시 관리자 권한 kfind `--uninstall`(`:105-116`)을 부르고, 색인 폴더를 지운다(`:63-67`). 레지스트리 우클릭 메뉴는 `uninsdeletekey`로 제거 때 지운다(`:69-75`). 업그레이드 시 실행 중 프로세스는 `CloseApplications=force`(`:22`)로 닫는다.
- **차이**: WinRust는 구성요소 선택·부분 실패 계속·변경 값 저장 후 되돌림(TODO). 우리는 구성요소 선택이 없고, `[Run]` 항목의 종료 코드를 [Code]에서 확인·기록하지 않는다(`[Code]`는 정리 함수와 `CurUninstallStepChanged`만 있음, `:76-118`). Inno Setup 기본은 `[Run]`의 비정상 종료 코드를 설치 실패로 보지 않으므로 "실패해도 계속"은 사실상 되어 있으나, **실패를 사용자·로그에 알리지 않는다**(Inno 기본 동작 설명이며 실기 확인은 하지 않음: 미확인). "시스템 값 저장 후 되돌림"은 우리가 시스템 값을 바꾸지 않아 해당 없음.
- **개선안**: (a) 구성요소(몽땅검색·몽땅클립보드·가상 폴더·독)를 `[Components]`로 분리 — 공공기관 PC에서 필요한 것만 설치하는 이점, (b) `[Run]` 종료 코드를 `Log()`로 남기는 [Code] 래퍼, (c) 업그레이드 시 `--respect-disabled`가 모든 상주 앱에 적용되는지 점검(kagent·dock은 `--install` 줄이 없음, 확인 필요).
- **측정 필요: 아니오**(설치·제거·업그레이드 시나리오 실기 시험은 필요).

### ⑧ 서비스 스레드 패닉 격리(`panic = "unwind"` 분리 빌드)
- **현황**: 격리 경계가 이미 있다. `kfind/core/src/lib.rs:86-96`의 `catch_unwind` 경계가 패닉을 `Error::Panic`으로 바꾼다(`:70`). 색인 구축은 `crates/kfind/src/names.rs:127-151`, 질의는 `pipe.rs:873-879`에서 격리한다. 뮤텍스 독 오염은 `unwrap_or_else(|e| e.into_inner())`로 풀어 쓴다(`names.rs:138,151,217,261,292`). 프로필 `panic = "unwind"`가 `kfind/core`·`kfind/names`·`kfind/annual`에 적혀 있으나 **이 설정은 의존 크레이트에서는 무시된다**(Cargo는 빌드의 최상위 크레이트 프로필만 쓴다). 실제 `kfind.exe`는 `crates/kfind`(`[workspace]` 선언, `Cargo.toml:7`; CI `build-windows-exe.yml:249`)가 최상위이고, 이 크레이트에는 `[profile.release]`가 없어 기본값(unwind)으로 우연히 맞다. 상주 앱은 다르다: kagent·kvf·klib는 `panic = "abort"`(`kagent/Cargo.toml:57`, `kvf/Cargo.toml:16`, `klib/Cargo.toml:48`), ktm·dock은 `unwind`(`ktm/Cargo.toml:22`, `dock/Cargo.toml:18`).
- **서비스 오류 처리**: `crates/kfind/src/service.rs:14-18`의 `service_main`은 오류를 `eprintln!`으로만 남긴다. 서비스에는 콘솔이 없어 이 메시지는 사라진다. 오류 시 서비스 상태를 STOPPED로 보고하는지, 이벤트 로그나 파일 로그가 있는지는 이 문서에서 `service.rs` 나머지를 끝까지 읽지 않아 **미확인**.
- **차이**: WinRust는 서비스 검색을 `unwind`로 따로 빌드해 스레드 하나의 오류가 프로세스를 죽이지 않게 한다(TODO). 우리 kfind는 방식이 같고 격리 경계도 실제로 있다. 약점은 (a) kfind의 `unwind`가 프로필이 아니라 "기본값에 의존"한다는 점(누군가 `panic = "abort"`를 넣으면 격리가 조용히 사라짐), (b) kagent(클립보드 감시 스레드)·klib(색인)는 `abort`라서 스레드 패닉 하나가 프로세스를 죽인다는 점이다.
- **개선안**: `crates/kfind/Cargo.toml`에 `[profile.release] panic = "unwind"`를 **명시**하고, 격리 동작을 확인하는 시험(패닉 주입 후 다음 질의 성공)을 CI에 둔다. kagent·klib는 `catch_unwind`가 필요한 스레드가 있으면 `unwind`로 바꾸고 크기 증가를 잰다(⑨와 같이).
- **측정 필요: 예**(unwind 전환 시 exe 크기·속도 변화).

### ⑨ 바이너리 줄이기(LTO·codegen-units·strip·panic, 의존성)
상세 표는 2절. 요약: 7개 상주·도구 앱 중 `kfind.exe`는 `[profile.release]` 자체가 없다(기본값: LTO 꺼짐, codegen-units 16, strip 꺼짐). `hierarchy`·`outliner`·`pdf-edit`·`qms_bench`도 일부 또는 전부 없다. `opt-level`은 전부 3이거나 기본이고 `"s"`·`"z"`를 쓰는 곳은 없다. 가장 큰 `klib.exe`(9.36MB)는 `tantivy`·`rusqlite(bundled)`·`regex`·`zip`·`cfb` 등 의존성이 크기를 정한다(`klib/Cargo.toml` 의존성 절). `.cargo/config.toml`이 정적 CRT(`+crt-static`)를 전 크레이트에 적용해 VC 런타임 DLL 의존은 없으나 크기가 늘어난다(의도된 선택, 2026-11-14).
- **개선안**: (a) `crates/kfind`에 프로필 추가(LTO·cg1·strip), (b) `hierarchy` 등 빠진 크레이트에 같은 프로필, (c) klib에서 `opt-level = "s"`와 tantivy·rusqlite 기능 정리를 시험, (d) 루트 워크스페이스 또는 공용 프로필 파일로 일원화(현재 크레이트마다 `[workspace]`라 프로필 복붙).
- **측정 필요: 예**(변경 전후 크기, 속도 회귀 확인).

### ⑩ 다국어 문자열 매크로(`t!`) 방식
- **현황**: Rust 앱에는 언어 문자열 방식이 없다. 사용자에게 보이는 문구를 소스에 한국어 문자열로 직접 쓴다(예: `ktm/src/backend.rs:787` "문서 #{} (경로 정보 없음)", `:864` "문서 표가 손상되었습니다", `:1027`). `t!`·`i18n`·`locale` 계열 매크로·표는 ktm·kagent·kvf·klib·dock·hierarchy·ui_theme에서 찾지 못했다(`macro_rules!`·`lang`·`i18n` 검색, 시험 코드 제외). 설치 프로그램도 `Korean.isl` 하나(`setup.iss:30-31`)다.
- **차이**: WinRust는 `t!` 매크로 + 공용 크레이트 언어 문자열(TODO 서술, 구체 구현 미확인). 우리는 한국어 단일 언어이고 공공기관 대상이라 지금 다국어 수요는 없다. 문구가 흩어져 있어 나중에 영어를 넣거나 문구를 일관되게 고칠 때 비용이 크다.
- **개선안**: 다국어보다 **문구 한곳 모으기**가 실익이다 — `ui_text` 같은 작은 모듈에 상수로 모으고, 나중에 `t!`로 바꿀 수 있게 키 이름만 맞춰 둔다. 영어 수요가 확인되기 전에는 하지 않는다.
- **측정 필요: 아니오**.

## 2. ⑨ 상세 — 크레이트별 release 프로필과 설치본 크기

설치본 경로: `C:/Users/haiju/AppData/Local/Programs/HWP_AutoDocFit/_internal/resources/` (`ls -l`, 수정 시각 모두 11월 19일 14:40, 읽기 전용).

| 실행 파일 | 빌드 크레이트(Cargo.toml) | opt-level | LTO | codegen-units | strip | panic | 설치본 크기(바이트) | MB |
|---|---|---|---|---|---|---|---|---|
| kagent.exe | `kagent/Cargo.toml:53-58` | 3 | true | 1 | true | abort | 2,444,800 | 2.33 |
| ktm.exe | `ktm/Cargo.toml:18-23` | 3 | true | 1 | true | unwind | 2,242,048 | 2.14 |
| kvf.exe | `kvf/Cargo.toml:12-17` | 3 | true | 1 | true | abort | 623,616 | 0.59 |
| kfind.exe | `crates/kfind/Cargo.toml` — **`[profile.release]` 없음**(기본) | 3(기본) | **꺼짐** | **16(기본)** | **꺼짐** | unwind(기본) | 2,942,464 | 2.81 |
| klib.exe | `klib/Cargo.toml:44-49` | 3 | true | 1 | true | abort | 9,363,968 | 8.93 |
| hierarchy.exe | `hierarchy/Cargo.toml:20-22` | 기본(3) | true | 1 | **꺼짐** | unwind(기본) | 2,516,992 | 2.40 |
| dock.exe | `dock/Cargo.toml:14-19` | 3 | true | 1 | true | unwind | 443,904 | 0.42 |
| outliner.exe | `outliner_core/Cargo.toml` — **프로필 없음** | 3(기본) | 꺼짐 | 16 | 꺼짐 | unwind(기본) | 587,264 | 0.56 |
| pdf-edit.exe | `pdf_edit_core/Cargo.toml` — **프로필 없음** | 3(기본) | 꺼짐 | 16 | 꺼짐 | unwind(기본) | 892,416 | 0.85 |
| qms_bench.exe | `qms_bench/Cargo.toml` — **프로필 없음** | 3(기본) | 꺼짐 | 16 | 꺼짐 | unwind(기본) | 916,480 | 0.87 |

참고(설치본 외 크레이트): `bench/kb1`(LTO·cg1·unwind), `kagent/ffi`(프로필 없음, cdylib), `kfind/engine-bench`(LTO·cg1), `kfind/annual`(LTO·cg1·unwind), `editor_core`(LTO·cg1), `scripts/kagent_thumb_bench`(LTO·cg1·abort).

읽는 법과 주의:
- 표의 "기본"은 Cargo 기본 release(opt-level 3, LTO 꺼짐, codegen-units 16, strip 없음, panic unwind)이다. 이 줄들은 Cargo.toml에 프로필이 없다는 사실에서 추론했고, 빌드 스크립트·CI가 환경변수(`CARGO_PROFILE_RELEASE_*`)로 덮어쓰는지는 확인하지 못했다(**미확인**).
- `kfind/names`·`kfind/core`·`kfind/annual`의 `[profile.*]`(`kfind/names/Cargo.toml:42-47`의 `debug = 1` 포함)는 의존 크레이트로 쓰일 때 Cargo가 무시한다. `kfind.exe`의 실제 프로필은 `crates/kfind`가 정한다. `kfind/names`의 `debug = 1`이 최종 exe에 영향을 주지 않는지는 위 이유로 영향이 없을 것으로 보이나 빌드로 확인하지 않았다(**미확인**).
- `hierarchy.exe`는 설치본에서 단독 실행 파일(2.4MB)이고 `klib`는 `hierarchy`를 경로 의존으로 가져간다(`klib/Cargo.toml`).
- 정적 CRT(`.cargo/config.toml`)가 모든 exe에 들어 있어 크기에 일정 부분 포함된다. 얼마나인지는 재지 않았다(**미확인**).
- `strip = true`가 없는 exe도 MSVC에서는 심볼이 PDB로 분리돼 exe가 크게 부풀지는 않는다. 효과는 재 봐야 안다.

## 3. 개선 실행 우선순위 5개 (효과 큰 순)

| 순위 | 개선 | 효과 | 위험 | 작업량 | 근거 |
|---|---|---|---|---|---|
| 1 | **kfind 프로필 정비**: `crates/kfind/Cargo.toml`에 `[profile.release]`(opt-level 3·LTO·cg1·strip·`panic = "unwind"` 명시) 추가. 같은 프로필을 `hierarchy`·`outliner_core`·`pdf_edit_core`·`qms_bench`에도 적용 | 크기·속도 개선과, 서비스 격리 설정이 "우연"에서 "명시"로 바뀜 | 낮음(빌드 시간 증가, 격리 시험으로 보증) | 작음(반나절) | ⑧·⑨, `crates/kfind/Cargo.toml`, 표 |
| 2 | **klib 줄이기 시험**: `opt-level = "s"` 비교, tantivy·rusqlite·regex 기능 정리, 사용 안 하는 의존성 제거 | 가장 큼(8.93MB, 설치본에서 단일 최대) | 중(검색 속도 회귀 가능 → 벤치로 확인 필수) | 중(1~2일) | ⑨, `klib/Cargo.toml` |
| 3 | **상주 앱 오류 격리 점검**: kagent·kvf·klib의 `abort`를 스레드별로 따져 `unwind` + `catch_unwind` 경계(또는 패닉 시 로그 후 자동 재시작) 도입. 서비스 `service_main`의 오류를 파일·이벤트 로그로 남김 | 사용자 체감 안정성(클립보드 감시·색인 스레드 하나가 프로세스를 죽이지 않음) | 중(크기 증가, 패닉 후 상태 불일치 위험) | 중(1~2일) | ⑧, `service.rs:14-18`, `kagent/Cargo.toml:57` |
| 4 | **독 최대화 창·자동 숨김 동작 보강**: 일반 최대화 창 판정, 가장자리 호버 재등장, (선택) 슬라이드. 독을 `ui_theme`에 연결 | 기능·체감(WinRust와 가장 눈에 띄는 차이) | 낮음~중(작업표시줄·다중 모니터 경계 사례) | 중(1~2일) | ③·④, `dock/src/native.rs:204-236` |
| 5 | **설치 프로그램 보강**: `[Run]` 종료 코드 로그, 구성요소 선택 설계(몽땅검색·몽땅클립보드·가상 폴더·독), 업그레이드 시 설정 유지 점검 | 배포 신뢰성(문제 진단) | 낮음(설치 시나리오 실기 시험 필요) | 중(1일 + 시험) | ⑦, `installer/setup.iss:44-48,76-118` |

참고: 1단계 측정표(WinRust 비교 포함)는 WinRust를 가상 머신에서만 설치할 수 있어 이 문서에서는 다루지 않았다. 순위 1·2는 변경 전후 크기를 같은 방법으로 재서 TODO 표를 갱신한다.

## 4. 미확인 항목 정리

1. WinRust 쪽 모든 수치·동작(색인 53MB, 검색 45ms, 독 슬라이드, 사진 보기 420줄, 설치 방식, `t!` 구현) — TODO 서술만 근거, 소스 미열람.
2. 우리 kfind 색인 메모리·갱신 지연·질의 시간 — 재지 않음.
3. `service.rs`의 서비스 상태 보고·로그 여부(`service_main` 이후 전체 흐름) — 끝까지 읽지 않음.
4. 프로필 "기본" 표기 exe(kfind·outliner·pdf-edit·qms_bench·hierarchy 일부)가 환경변수로 덮어쓰이는지 — 빌드 스크립트 미확인.
5. 정적 CRT가 크기에 미치는 비중 — 미측정.
6. 설치 중 `[Run]` 실패 시의 실제 표시·로그 — Inno 기본 동작 설명이며 실기 시험 안 함.
7. WIC가 WebP·HEIC·AVIF를 해독하는지(코덱 의존) — 확인 안 함.
8. 몽땅사진첩 Rust 큰 보기 화면의 현재 구현 여부 — TODO 계획만 확인.
9. kagent·dock의 설치 시 `--install` 처리 여부 — `setup.iss`에는 ktm·kvf 줄만 있고(`:44-48`), 나머지는 앱 내부 설정 경로로 하는지 미확인.
