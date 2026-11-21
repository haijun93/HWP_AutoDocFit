# THUMBSHELL3 결과 (2026-11-22)

THUMBSHELL2 결과의 미완료 항목을 이어서 처리했다. 지정된 커밋 `0d8e210f`, `dc35a17a`, `c74a99fa`를 순서대로 가져왔다. 마지막 커밋의 photoview lockfile은 현재 `github/main`의 의존성과 충돌해, 기존 main lockfile을 보존하고 offline Cargo 해석으로 필요한 Windows 의존성만 추가했다.

## 구현

- `grid_core::shell_thumb::ThumbnailService`를 추가했다. 1~8개의 persistent STA worker가 보이는 항목을 순서대로 요청하고, 뷰포트가 바뀌면 대기 중인 이전 요청을 지운다. 호출이 끝나면 UI 메시지로 결과를 전달한다.
- `docview_win`, `photoview_win` 창은 화면에 보이는 경로를 서비스에 공급한다. 완료된 BGRA 이미지는 64 MiB 캐시에 저장하고 GDI로 카드에 그린다. 캐시가 없거나 Shell 처리기가 실패하면 기존 카드 모양을 표시한다.
- 두 앱의 `--capture-dir --scan-root` headless 캡처는 실제 스캔 결과 중 보이는 항목을 Shell API로 가져와 PNG에 그린다.
- 벤치마크 시험은 실제 `ThumbnailService`의 1·2·4·8 STA worker를 실행하도록 바꿨다.

## 다중 스레드 측정 및 채택 판단

입력은 합성 JPEG 20장, 각 4000×3000 픽셀이다. 각 worker 수 실측 전에 별도 경로로 파일을 복사해 Explorer 썸네일 캐시가 같은 경로를 재사용하지 않게 했다. 환경은 Windows, Rust 1.99.1, Pillow 13.0.0이다.

| 방법 | 20장 시간 | 성공 | Pillow 대비 |
|---|---:|---:|---:|
| Pillow LANCZOS 축소 | 0.8597초 | 20/20 | 기준 |
| Shell STA 1 worker | 1.1017초 | 20/20 | 0.78배 |
| Shell STA 2 workers | 0.6326초 | 20/20 | 1.36배 |
| Shell STA 4 workers | 0.5843초 | 20/20 | 1.47배 |
| Shell STA 8 workers | 0.5748초 | 20/20 | 1.50배 |

4 worker를 앱 기본값으로 채택했다. 같은 실행의 Pillow보다 32% 빠르고, 8 worker보다 1.6% 느린 데 그쳐 추가 worker 4개의 비용을 줄인다. THUMBSHELL2 문서의 Pillow 3.6초는 당시 측정치로 보존한다. 이번 Pillow 재측정은 0.8597초였고, 별도 PC·시험 시점 측정치가 달라 직접 비교를 대체하지 않는다.

Shell 캐시 적중 반복 호출은 JPG 0.0207초, PNG 0.0176초, PDF 0.0243초, TXT 0.0261초였다. 첫 호출은 각각 0.0859초, 0.0560초, 0.2023초, 0.0662초였다. 현재 PC에는 네 형식의 Shell 썸네일 처리기가 있었다.

## 실제 headless 캡처

`klib`로 만든 합성 테스트 입력을 사용했고 사용자 문서·사진은 열지 않았다. 캡처는 저장소 밖 임시 폴더에 두었다.

- `docview_realscan_light_96dpi.png`: 실제 PDF 스캔 1개, PNG 2,509,018바이트. Shell PDF 미리보기가 카드에 표시되는 것을 열어 확인했다.
- `photoview_realscan_light_96dpi.png`: 실제 사진 스캔 52개, 화면 내 28개 카드, PNG 2,509,018바이트. JPG/PNG/PDF 등 Shell 결과가 카드에 표시되는 것을 열어 확인했다.

## 빌드 시간과 실행 파일 크기

`github/main`을 별도 worktree에 checkout하여 각 앱의 release 실행 파일을 직접 빌드했다. 기준과 최종 빌드는 각자 비운 `CARGO_TARGET_DIR`에서 순서대로 실행했다. 두 번째 앱 빌드는 앞 앱이 만든 공통 의존성을 재사용한다.

| 앱 | github/main 시간 / 크기 | THUMBSHELL3 시간 / 크기 | 크기 증가 |
|---|---:|---:|---:|
| docview_win | 27.581초 / 2,121,522 B | 76.416초 / 2,766,968 B | 645,446 B |
| photoview_win | 4.768초 / 2,126,518 B | 7.759초 / 2,764,701 B | 638,183 B |
| 합계 | 32.349초 / 4,248,040 B | 84.175초 / 5,531,669 B | 1,283,629 B |

첫 앱이 새 Windows Shell 의존성과 pool 코드를 함께 컴파일해 빌드 시간 증가분이 docview 쪽에 집중됐다. 두 앱의 실제 이미지 공급 및 캐시는 수치가 개선됐지만 실행 파일은 각각 약 0.61 MiB 커졌다. 사용자는 Pillow로 이미지를 읽고 축소하지 않아도 되고, Explorer가 지원하는 PDF 등도 미리보기 할 수 있다. 4 worker 선택은 위 표의 Pillow 비교와 4 대 8 worker 측정에 근거한다.

## 검증

`KLIB_EXE`를 이번 release `klib.exe`로 지정해 시험했다. 세 크레이트의 `cargo test --release --locked` 최종 `test result` 줄은 다음과 같다.

```text
grid_core
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

docview_win
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.68s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.45s
test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.29s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.36s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

photoview_win
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.16s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.43s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.83s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.43s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

그리드 핵심 시험의 1개 ignored 항목은 기존대로 대화형 Shell에서 실행하는 별도 benchmark이며, 이 작업에서는 `--ignored --nocapture`로 직접 실행해 위 측정 결과를 얻었다. `real_klib_tests`는 docview 5/5, photoview 5/5 통과했다.
