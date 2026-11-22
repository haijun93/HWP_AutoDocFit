# PDFWIN3 결과 — Windows.Data.Pdf 쪽 썸네일 프로토타입

작성일: 2026-11-22 · 브랜치: `feature/PDFWIN3` · 범위: 썸네일 공급 프로토타입(앱 연결 제외)

## 구현

- 독립 크레이트 `pdf_thumb`에 `PdfDocument::LoadFromFileAsync` → `GetPage` → `RenderWithOptionsToStreamAsync`를 구현했다. PNG 스트림은 Windows Imaging API로 디코딩해 BGRA 픽셀을 반환한다.
- 4개의 지속 MTA worker가 각자 가장 최근 PDF 문서를 유지한다. `set_visible`은 요청 큐를 보이는 쪽 순서로 교체하고, 결과는 비동기 `drain`으로 회수한다.
- 썸네일 메모리 캐시는 LRU 방식이며 64 MiB로 제한했다. PDF 로드, 암호, 손상, 범위 초과, WinRT 초기화 등 오류는 `NumberCard { page_number }`로 반환한다.
- 픽셀 검증에서 확인한 두 가지 문제(WinRT 경로의 `\\?\` 접두어와 RGBA/BGRA 해석)를 수정했다. 세 장의 저장 PNG를 직접 열어 파랑/빨강 채널과 페이지 형태를 확인했다.

## 합성 PDF 시험과 실측

직접 생성한 벡터 합성 PDF 10·50·200쪽을 같은 PC에서 각각 한 번 측정했다. 측정은 새 4-worker 서비스에 가시 쪽 요청을 넣은 시점부터 1쪽 결과 수신까지(first), 이어서 전 쪽을 회수할 때까지(total)다. 비교 경로는 `pypdfium2`에서 문서를 한 번 열고 각 쪽을 최대 변 240px로 렌더하는 현재 방식이다. 측정 전후의 시스템/프로세스 캐시는 통제하지 않은 단회 결과이며, 첫 10쪽 측정에는 WinRT 런타임의 최초 초기화 비용이 포함된다.

| 쪽 수 | Windows.Data.Pdf 첫 쪽 | Windows.Data.Pdf 전체 | pypdfium2 첫 쪽 | pypdfium2 전체 |
|---:|---:|---:|---:|---:|
| 10 | 630.0 ms | 678.2 ms | 48.8 ms | 54.4 ms |
| 50 | 36.3 ms | 338.3 ms | 0.9 ms | 28.2 ms |
| 200 | 45.7 ms | 1,178.7 ms | 1.0 ms | 107.4 ms |

시험은 정상 렌더 10/10, 50/50, 200/200쪽을 확인했고, 각 페이지 크기와 픽셀 버퍼 길이 및 비어 있지 않은 픽셀을 검증했다. 정상 입력의 쪽 수 다음 인덱스는 폴백 카드가 된다. AES-128 사용자 암호 PDF와 손상 PDF도 1쪽 번호 카드 폴백을 확인했다.

PNG 출력물 `page-1.png`, `page-2.png`, `page-3.png`는 합성 입력에서 만들었다. 세 파일을 이미지로 열어 내용과 색을 눈으로 검사했다. 산출물은 저장소 외부에 두었다.

## 실행 파일 크기

동일한 Rust release 설정에서 `render_page`를 호출하는 최소 크기 probe 실행 파일은 **337,920 B**였다. 빈 `main`만 있는 최소 Rust 실행 파일은 **256,512 B**로, 이 호스트 예제에서의 증가분은 **81,408 B (79.5 KiB)**다. 이는 썸네일 API를 연결한 앱 전체의 크기 증가량이 아니라, 같은 툴체인·설정의 최소 probe 간 차이다. 외부 PDFium DLL은 추가하지 않는다.

## 검증 결과

명령: `cargo test --manifest-path pdf_thumb/Cargo.toml --release --locked -- --nocapture`

```text
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.20s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

위 release 시험은 Windows Rust 1.99.1 GNU 툴체인에서 수행했다. 독립 crate의 release 테스트 전체가 통과했다.

## 채택 판단

**현재는 앱 기본 공급자로 채택하지 않는 편이 낫다.** API 연동과 번호 카드 폴백은 실증됐고 추가 PDFium 바이너리도 필요 없지만, 단회 비교에서 Windows.Data.Pdf는 200쪽 전체 렌더가 1,178.7 ms로 pypdfium2의 107.4 ms보다 약 11배 오래 걸렸다. 첫 쪽도 10쪽 입력의 런타임 최초 초기화 구간에서 630 ms였다. 캐시되지 않은 많은 쪽을 빠르게 보여야 하는 편집 격자에는 성능 근거가 부족하다. 실제 앱 적용 전에는 WinRT 초기화 비용을 사용자 첫 요청에서 분리할 수 있는지와 더 다양한 문서의 품질·지연을 반복 측정해야 한다.

## 범위와 한계

- 이 작업은 `pdf_thumb` 프로토타입, 시험, 측정에 한정하며 편집 UI와 연결하지 않았다.
- 이전 단계의 PDFWIN2 35종 합성 픽스처 세트는 현재 checkout에서 찾지 못했다. 따라서 이 단계의 페이지 수/픽셀 통합 시험용 PDF를 새로 합성했고, 암호 fixture는 저장소의 기존 PDF 편집 fixture를 재사용했다.
- 실측은 단회이므로 성능 결론은 채택 판단용 초기 근거이며 통계적 비교는 아니다.
