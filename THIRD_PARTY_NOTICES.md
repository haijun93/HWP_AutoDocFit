# Third-party notices

## kordoc

The optional advanced document tools invoke `kordoc` version 4.x through npm.

- Project: https://gitlab.aigov.go.kr/chrisryugj/kordoc
- Copyright (c) 2026 chrisryugj
- License: MIT

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

Optional kordoc features can load additional packages and models. Their own
license and notice files remain available in the installed kordoc package.

## Other components

| Component | Use | License |
|---|---|---|
| Hancom HWP automation security module sample (`MapoHwpAutoDocFitSecurity.dll`) | Approves HWP COM automation; Hancom sample binary renamed for this project | Hancom sample distribution terms — **재배포·이름 변경·재허락(서브라이선스) 허용 여부는 아직 확인하지 못했다(2026-11-09 점검)**. 확인 전에는 구동 안내서의 "재배포가 제한되는 구성요소"에 명시한다 |
| pywin32 | HWP COM bridge | PSF |
| tkinterdnd2 | Drag and drop | MIT |
| Pillow | Images | MIT-CMU (HPND) |
| defusedxml | Safe XML parsing | PSF |
| pypdf 6.21.4 | PDF compression (file format: PDF 압축) | BSD-3-Clause |
| cryptography 50.0.2 | AES encryption when re-securing compressed PDFs | Apache-2.0 OR BSD-3-Clause |
| pywebview | Web UI window | BSD-3-Clause |
| Flutter | flutter_ui | BSD-3-Clause |
| Noto Sans KR | flutter_ui font (see `flutter_ui/assets/OFL.txt`) | SIL Open Font License 1.1 |
| Noto Sans Symbols 2 Regular | Flutter arrow glyph fallback (`flutter_ui/assets/fonts/NotoSansSymbols2-Regular.ttf`) | SIL Open Font License 1.1 (`flutter_ui/assets/fonts/OFL.txt`) |
| STIX Two Math | Flutter fallback for ⇑ and ⇓ (`flutter_ui/assets/fonts/STIX2Math.otf`) | SIL Open Font License 1.1 (`flutter_ui/assets/fonts/STIX-OFL.txt`) |
| VS Code Codicons (microsoft/vscode-codicons) | flutter_ui icons (`flutter_ui/assets/fonts/codicon/`) | Icons: CC-BY-4.0 (`LICENSE`), Code: MIT (`LICENSE-CODE`) |
| PyInstaller | Executable build | GPL-2.0 with bootloader exception |
| Python / Tcl/Tk runtime | Bundled in the executable | PSF / Tcl/Tk License |

The HWP application itself (HWPFrame.HwpObject) is not bundled; the user's installed copy is used.

### 앱 실행 파일에 함께 들어가거나 앱이 실행하는 추가 구성요소 (2026-11-09 점검 보강)

위 표에 빠져 있던 구성요소를 의존성 파일·빌드 스크립트에서 확인해 보탰다. **근거** 칸이 "파일"이면 저장소 파일(잠금 파일·매니페스트·빌드 스크립트)에서 이름·버전·라이선스 표기를 직접 확인한 것이고, "일반 지식"이면 라이선스 표기를 해당 프로젝트의 일반적인 배포 조건으로 적은 것이라 **배포 전에 각 패키지의 라이선스 파일로 다시 확인**해야 한다.

| 구성요소 | 용도 | 라이선스 | 근거 |
|---|---|---|---|
| Pandoc 3.12.1 | DOCX·EPUB·HTML·MD 변환(별도 프로세스로 실행) | GPL-2.0-or-later | 버전·SHA-256: `scripts/build_kordoc_runtime.ps1`, 동봉 안내: `docs/OFFLINE_DEPLOYMENT.md`. 라이선스는 일반 지식 |
| Node.js 22.x | kordoc 실행 런타임(내장) | MIT (동봉 구성요소별 라이선스는 Node.js 배포물의 `LICENSE` 참고) | `docs/OFFLINE_DEPLOYMENT.md`, 라이선스는 일반 지식 |
| kordoc 4.24.9 | 문서 구조 분석 | MIT | 파일: `kordoc_pack/package.json`, 위 kordoc 절 |
| pdfjs-dist 4.10.38 | PDF 읽기 | Apache-2.0 | 버전: `kordoc_pack/package.json`, 라이선스는 일반 지식 |
| @hyzyla/pdfium 2.1.13 | PDF 렌더링(글자 인식 꾸러미) | MIT | 파일: `kordoc_ocr_pack/package-lock.json` |
| onnxruntime-node 1.30.0 | 글자 인식 추론(Windows x64 CPU 파일만 보관) | MIT | 파일: `kordoc_ocr_pack/package-lock.json` |
| sharp 0.35.5와 libvips Windows 바이너리(`@img/sharp-win32-x64`) | 이미지 처리 | Apache-2.0 AND **LGPL-3.0-or-later** | 파일: `kordoc_ocr_pack/package-lock.json` |
| PP-OCRv5 모델(PaddlePaddle: `PP-OCRv5_mobile_det`, `korean_PP-OCRv5_mobile_rec`) | 글자 인식 | Apache-2.0 (모델 배포처 안내 기준) | 고정 주소·SHA-256: `scripts/download_ocr_models.py`, 라이선스는 일반 지식 |
| pypdfium2 | PDF 렌더링 | Apache-2.0 OR BSD-3-Clause (동봉 PDFium은 BSD-3-Clause) | 의존성: `requirements.txt`, 라이선스는 일반 지식 |
| xlrd 2.0.1 | 엑셀(.xls) 읽기 | BSD-3-Clause | 의존성: `requirements.txt`, 라이선스는 일반 지식 |
| windows-sys 0.59, png 0.17 | 몽땅클립보드 에이전트(`kagent`, Rust) | MIT OR Apache-2.0 | 의존성: `kagent/Cargo.toml`, 라이선스는 일반 지식 |
| rusqlite 0.33(SQLite 포함), tantivy 0.27 | 자료함·검색 엔진(`klib`, Rust) | MIT (SQLite는 퍼블릭 도메인) | 의존성: `klib/Cargo.toml`, 라이선스는 일반 지식 |
| file_selector, web, webview_flutter_windows 1.2.0 | Flutter 화면 | BSD-3-Clause | 의존성: `flutter_ui/pubspec.yaml`, 라이선스는 일반 지식 |
| Material Icons 글꼴(Flutter 포함) | 화면 아이콘 | Apache-2.0 | 일반 지식 |
| VS Code Codicons (microsoft/vscode-codicons) | 화면 아이콘 (`flutter_ui/assets/fonts/codicon/`) | CC-BY-4.0(아이콘) / MIT(코드) | 파일: `flutter_ui/assets/fonts/codicon/LICENSE` 및 `LICENSE-CODE` |

#### 소스 코드 제공 (GPL·LGPL 구성요소)

- **Pandoc(GPL-2.0-or-later)**: 위 버전의 대응 소스는 Pandoc 프로젝트(https://github.com/jgm/pandoc, 태그 3.12.1)에서 받을 수 있다. 이 앱은 Pandoc을 **수정하지 않고 별도 실행 파일로 호출**하며, 빌드 스크립트(`scripts/build_kordoc_runtime.ps1`)가 Pandoc의 `COPYING.rtf`와 `COPYRIGHT.txt`를 함께 포함한다. 원 저장소에 접근할 수 없는 환경에서는 이 저장소의 이슈(https://gitlab.aigov.go.kr/haijun93/hwp_autodocfit/-/issues 또는 https://github.com/haijun93/hwp-auto-docfit/issues)로 대응 소스를 요청하면 제공한다.
- **libvips(`sharp`의 Windows 바이너리, LGPL-3.0-or-later)**: 수정하지 않았고 별도 DLL로 들어 있어 사용자가 같은 버전의 다른 빌드로 바꿀 수 있다. 대응 소스는 sharp-libvips 프로젝트(https://github.com/lovell/sharp-libvips)에서 받을 수 있고, 위와 같은 이슈로도 요청할 수 있다.
- **PyInstaller**: 실행 파일 부트로더 예외 조항이 적용된다(위 표).

#### 아직 확인하지 못한 항목 (배포 전 점검 필요)

- `kordoc_pack/package-lock.json`의 전이 의존성 180개는 잠금 파일에 라이선스 표기가 없어 이번에 판단하지 못했다. 설치 후 `npm ls`나 라이선스 점검 도구로 확인해야 한다.
- Rust 전이 크레이트(`Cargo.lock`)와 Flutter 전이 패키지의 라이선스, Flutter 엔진·Skia 등 동봉 구성요소의 고지.
- `tkinterdnd2`가 동봉하는 tkdnd 확장과 `webview_flutter_windows`가 쓰는 WebView2 로더의 라이선스 표기.
- 위 "일반 지식" 근거 항목 전체의 라이선스 문구.
- 한컴 보안 모듈 DLL의 재배포 조건(위 표 참고).

## 개발자 모드 연결 프로그램 (앱에 포함하지 않음)

개발자 모드 사이드메뉴의 아래 프로그램은 이 앱과 별도로 실행되는 독립 프로그램이며, 앱 설치 파일에 들어 있지 않다. 메뉴를 처음 누를 때 공식 배포처(또는 포크 저장소 릴리스)에서 받아 사용자 폴더에 설치한다. 각 프로그램의 라이선스 전문은 설치된 프로그램 폴더에 함께 있다.

| 메뉴 | 프로그램(포크) | 원본 | 라이선스 | 소스 |
|---|---|---|---|---|
| 사진첩 | mongdangview | NeeView (neelabo/NeeView) | MIT (7-Zip·SevenZipSharp는 LGPL) | https://github.com/haijun93/mongdangview |
| 아웃라이너 | mongdangoutliner | Logseq (logseq/logseq) | AGPL-3.0 | https://github.com/haijun93/mongdangoutliner |
| 사진관리 | mondangpics | digiKam (KDE/digikam) | GPL-2.0 | https://github.com/haijun93/mondangpics |
| PDF편집 | mongdangpdf | PdfCraft (storytold/pdfcraft) | Apache-2.0 OR MIT (원본 저장소의 라이선스 파일 참고. ArtCraft 이름·로고는 상표로 이 라이선스 대상이 아니며 사용하지 않는다) | https://github.com/haijun93/mongdangpdf |
