# THUMBSHELL1 결과 (2026-11-22)

## 구현

- `grid_core::shell_thumb`는 Windows 전용이다. 매 요청을 격리 STA 스레드에서 처리하고, 해당 스레드에서 `CoInitializeEx(COINIT_APARTMENTTHREADED)`와 `CoUninitialize`를 짝지어 호출한다. 셸 COM 호출을 15초 기다리며, 작업자 패닉·COM 오류·잘못된 크기·픽셀 읽기 오류를 `Result`로 반환한다.
- 셸 확장 프로그램이 COM 호출 안에서 멈추면 호출자에게 `Timeout`을 반환하지만 해당 스레드를 안전하게 강제 종료할 수 없다. 이런 멈춤이 누적되지 않도록 호출부의 동시 작업 수 상한이 필요하다.
- 경로는 `canonicalize()`로 절대 경로 및 존재 여부를 확인한다. Windows가 반환할 수 있는 `\\?\` / `\\?\UNC\` 접두를 일반 경로로 바꾸고 `/`를 `\`로 정규화한 뒤 UTF-16 널 종료 문자열로 전달한다.
- `SHCreateItemFromParsingName`에는 `pbc = NULL`을 전달한다. 반환된 `IShellItem`의 타입 지정 `cast::<IShellItemImageFactory>()`는 Windows 생성 래퍼가 인터페이스 IID 포인터를 전달하므로 수동 IID/vtable 정의가 없다. `GetImage`는 `SIIGBF_RESIZETOFIT`을 사용한다. HBITMAP은 `GetDIBits`로 top-down BGRA로 복사하고 RAII guard에서 해제한다.
- 이전 `E_INVALIDARG (0x80070057)`은 `canonicalize()` 결과에 포함될 수 있는 확장 경로 접두가 셸 항목 생성과 맞지 않는 문제로 확인됐다. 정규화 후 `%WINDIR%\System32\notepad.exe` 단순 probe가 성공했고, 네 합성 확장자도 항목 생성 및 이미지 반환에 성공했다. 이 수정으로 경로와 호출 문제를 분리했다.
- 호출 실패는 작업명과 HRESULT를 보존한다. `GetImage` 오류의 HRESULT만으로 미리보기 처리기 부재, 클라우드 전용 파일, 접근 거부를 항상 구별할 수 있다고 가정하지 않는다. 이 세 오류 상황은 별도 입력을 만들어 검증하지 않았다.

## 의존성 결정

`windows` 0.62.2를 유지한다. Shell COM 인터페이스와 GDI 핸들을 안전한 생성 래퍼·타입 변환으로 사용하므로 `windows-sys`에서 vtable, IID, 참조 카운트, ABI, 핸들 해제를 직접 맞추는 방식보다 실수 가능성이 낮다. `windows-sys`는 이미 다른 Windows 기능에 쓰이고 있지만, COM 인터페이스를 직접 선언하는 절감분보다 유지·호출 위험이 크다고 판단했다.

선택한 `windows` 기능은 `Win32_Foundation`, `Win32_Graphics_Gdi`, `Win32_System_Com`, `Win32_UI_Shell` 네 개다. Cargo lockfile에 windows-rs 계열을 포함해 15개 패키지가 추가됐다. 해당 의존성은 MIT OR Apache-2.0 선택지를 제공한다. 이번 수정 뒤 release 명령의 증분 컴파일은 10.26초였다. 이전 시도 기록의 초기 의존성 컴파일은 30초 이상이었고, 정확한 clean build 시간은 재측정하지 않았다. 앱 전체의 의존성 도입 전후 바이너리 크기 비교는 없어 크기 증가는 미확인이다. 기능을 제한해 필요하지 않은 Win32 모듈은 생성 대상에서 제외한다.

## 합성 입력 및 측정

시스템 임시 폴더 아래 카드 시험용으로 만든 합성 파일만 사용했다. 1600×1200 JPEG, 1200×900 PNG, 최소 PDF, 일반 TXT 및 4000×3000 JPEG 20개다. 사용자 사진·문서는 열거나 복사하지 않았다. 시간은 이 PC에서 한 번 측정했다.

| 입력 | 반환 결과 | 첫 호출 | 같은 항목 재호출(셸 캐시 warm) | 출력 |
|---|---|---:|---:|---|
| 합성 JPG | 성공, 사진 미리보기 | 1.6747초 | 0.1426초 | 256×192 |
| 합성 PNG | 성공, 그림 미리보기 | 1.3878초 | 0.4596초 | 256×192 |
| 최소 합성 PDF | 성공, 일반 PDF 아이콘 | 0.7909초 | 0.1060초 | 256×256 |
| 합성 TXT | 성공, 일반 텍스트 아이콘 | 1.1139초 | 0.2174초 | 256×256 |
| `C:\Windows\System32\notepad.exe` probe | 성공 | 1.1263초 | 측정 안 함 | 128×128 |

시험한 JPG·PNG·PDF·TXT 모두 HBITMAP은 반환했다. 다만 PDF/TXT는 문서 내용 미리보기가 아니라 Windows 셸의 일반 파일 아이콘이었다. 실패한 확장자는 이번 입력 집합에서 0개이며, 미리보기 처리기가 전혀 없는 확장자는 별도 시험하지 않아 확장자별 실패 목록은 미확인이다.

| 4000×3000 합성 JPEG 20장 | 성공 수 | 총 시간 | 이전 THUMBRS2 기준과 비교 |
|---|---:|---:|---|
| Windows 셸 API, 요청 256px | 20/20 | 5.4170초 | Pillow 3.646초보다 1.48배 느림, Rust image 10.822초의 약 절반 |

이 수치는 셸 API/탐색기 캐시가 개입한 단일 실행이며 THUMBRS2와 동일한 디코드·리사이즈 경로라고 볼 수 없다. 따라서 해당 비교는 참고값이고, 성능 우위를 일반화하지 않는다.

## 요청 크기별 PNG 육안 확인

요청 크기별 결과를 PNG로 저장하고 `view_image`로 파일을 각각 직접 열어 확인했다. 입력은 합성 JPG다.

| 요청 크기 | 실제 크기 | 육안 관찰 |
|---:|---:|---|
| 64 | 64×48 | 4:3 비율과 도형은 유지된다. 작은 글자와 가는 선은 식별하기 어렵다. |
| 128 | 128×96 | 도형 윤곽은 뚜렷하고 작은 표식은 읽기 어렵다. |
| 256 | 256×192 | 선·도형이 선명하고 표식 일부가 보인다. |
| 512 | 512×384 | 가장 세밀하게 보이며 원본의 가는 선과 작은 글자가 더 잘 유지된다. |

이미지 파일: `quality-64.png`, `quality-128.png`, `quality-256.png`, `quality-512.png` (시험 fixture 폴더). PNG는 카드가 생성한 합성 포스터의 축소 결과다.

## 캐시·동시성 설계 메모

- 격자에서는 현재 보이는 칸만 요청 큐에 넣고, 큐 순서는 화면 읽기 순서로 둔다. 스크롤로 가시 항목이 바뀌면 이전 대기 항목은 폐기하고 새 가시 목록을 앞세운다. 이미 시작된 COM 호출은 중단할 수 없으므로 세대 ID를 확인해 늦게 도착한 결과를 버린다.
- 셸 API 호출은 STA worker에서 수행한다. 동시 STA 호출을 최대 4개로 제한하고, 초과 요청은 큐에서 대기시킨다. 15초 timeout 후에도 실제 호출 스레드가 살아 있을 수 있으므로 같은 worker를 무한히 새로 만들지 않도록 제한을 유지한다.
- 성공 결과는 키(정규화 경로, 요청 크기, 파일 변경 시각/크기)로 메모리 LRU에 보관한다. 초기 바이트 상한은 64 MiB로 두고, BGRA 바이트 수 기준으로 오래된 항목부터 제거한다. 한 항목이 상한보다 크면 캐시에 넣지 않는다.
- `grid_core::thumb::ByteLru`와 `ThumbQueue`는 순수 Rust 자료구조로 구현돼 기본 시험에서 검증된다. worker 풀, 취소 세대, 화면 연결, 캐시 키/상한 적용은 아직 구현하지 않았다.
- 격자/문서뷰/사진뷰에 연결하지 않았다.

## 시험 및 재현

Windows 셸 실기 시험은 처리기가 없는 CI 서버 에디션에서 결과가 달라질 수 있어 `#[ignore]` 처리했다. 시험 전 시스템 임시 폴더에 합성 fixture를 만들고 아래 환경변수에 그 폴더와 임시 cargo target 경로를 지정한다. 저장소 `.cargo/config.toml`은 생성하거나 수정하지 않았다. Cargo 실행 시에만 PATH 앞에 `%LOCALAPPDATA%/DocFitTools/mingw64/bin`을 넣는다.

```powershell
$env:PATH = "$env:LOCALAPPDATA\DocFitTools\mingw64\bin;$env:PATH"
$env:CARGO_TARGET_DIR = '<temporary-cargo-target-dir>'
$env:SHELL_THUMB_FIXTURES = '<temporary-fixture-dir>'
cargo test --manifest-path grid_core\Cargo.toml --release --locked
cargo test --manifest-path grid_core\Cargo.toml --release --locked --test shell_thumb_tests -- --ignored --nocapture
```

모든 시험 결과 줄:

```text
기본 시험:
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

ignored Windows 셸 시험(별도 실행):
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.62s
```

실기 시험에서는 Windows notepad probe와 네 합성 확장자, 20개 대형 JPEG가 성공했다. 별도 실행 명령은 출력 PNG를 fixture 폴더에 저장한다. 사용자 파일을 대상으로 실행하지 않는다.

## 미확인 사항

- 처리기가 없는 확장자에서 `GetImage`가 실패하는 HRESULT와 시스템 간 차이
- 클라우드 전용 placeholder 및 접근 거부가 반환하는 HRESULT/오류 구분
- worker 풀의 실제 동시성 제한, 진행 중 결과 폐기, 64 MiB LRU 연결
- 셸 API와 Pillow의 동일 입력·동일 크기 조건 직접 비교, dependency 도입 전후 앱 바이너리 크기와 clean build 시간
