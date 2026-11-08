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
| Hancom HWP automation security module sample (`MapoHwpAutoDocFitSecurity.dll`) | Approves HWP COM automation; Hancom sample binary renamed for this project | Hancom sample distribution terms |
| pywin32 | HWP COM bridge | PSF |
| tkinterdnd2 | Drag and drop | MIT |
| Pillow | Images | MIT-CMU (HPND) |
| defusedxml | Safe XML parsing | PSF |
| pypdf 6.21.4 | PDF compression (file format: PDF 압축) | BSD-3-Clause |
| cryptography 50.0.2 | AES encryption when re-securing compressed PDFs | Apache-2.0 OR BSD-3-Clause |
| pywebview | Web UI window | BSD-3-Clause |
| Flutter | flutter_ui | BSD-3-Clause |
| Noto Sans KR | flutter_ui font (see `flutter_ui/assets/OFL.txt`) | SIL Open Font License 1.1 |
| PyInstaller | Executable build | GPL-2.0 with bootloader exception |
| Python / Tcl/Tk runtime | Bundled in the executable | PSF / Tcl/Tk License |

The HWP application itself (HWPFrame.HwpObject) is not bundled; the user's installed copy is used.

## 개발자 모드 연결 프로그램 (앱에 포함하지 않음)

개발자 모드 사이드메뉴의 아래 프로그램은 이 앱과 별도로 실행되는 독립 프로그램이며, 앱 설치 파일에 들어 있지 않다. 메뉴를 처음 누를 때 공식 배포처(또는 포크 저장소 릴리스)에서 받아 사용자 폴더에 설치한다. 각 프로그램의 라이선스 전문은 설치된 프로그램 폴더에 함께 있다.

| 메뉴 | 프로그램(포크) | 원본 | 라이선스 | 소스 |
|---|---|---|---|---|
| 사진첩 | mongdangview | NeeView (neelabo/NeeView) | MIT (7-Zip·SevenZipSharp는 LGPL) | https://github.com/haijun93/mongdangview |
| 아웃라이너 | mongdangoutliner | Logseq (logseq/logseq) | AGPL-3.0 | https://github.com/haijun93/mongdangoutliner |
| 사진관리 | mondangpics | digiKam (KDE/digikam) | GPL-2.0 | https://github.com/haijun93/mondangpics |
