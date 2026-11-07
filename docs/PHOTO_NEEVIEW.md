# 사진첩(NeeView 포크 mongdangview) — 포크 빌드 기록

작성 2026-11-07, 갱신 2026-11-08. 근거: TODO.md "개발자 모드 사이드메뉴 '사진첩' 추가"(사용자 승인 2026-11-07).

> 사진첩·아웃라이너·사진관리 세 프로그램의 공용 실행기, 라이선스 준수, 배포본 포함(2단계) 설계는
> **`docs/EXTERNAL_APPS.md`**가 기준이다. 이 문서는 사진첩 포크의 빌드 기록만 남긴다.

## 1. 결정 사항 요약

| 항목 | 결정 |
| --- | --- |
| 구현 방식 | 본 앱에 이식하지 않음. 포크 https://github.com/haijun93/mongdangview(원본 neelabo/NeeView)를 **별도 프로그램**으로 빌드하고, 사이드메뉴는 실행 진입점만 맡는다 |
| 라이선스 | NeeView 본체 MIT(원 저작권 고지 유지). 제3자 부품은 포크의 `THIRDPARTY_LICENSES.md`. **7-Zip(7z.dll)·SevenZipSharp는 LGPL** → 정적 결합하지 않고 별도 DLL 그대로 배포(교체 가능) |
| 노출 범위 | 개발자 모드에서만 사이드메뉴 표시, Python 브리지도 개발자 모드가 아니면 거절 |
| 실행 방식 | 외부 실행(별도 창·별도 프로세스, 앱을 닫아도 사진첩은 남음). 창 내장은 하지 않음 |
| 앱 쪽 위치 | 앱 폴더 `tools\mongdangview\NeeView.exe` 또는 사용자 지정 경로(공용 실행기 `docfit_app/external_apps_service.py`, app_id `photo_album`) |
| 포크 코드 | 변경하지 않음. 빌드 설정(GitHub Actions 워크플로)만 추가 |

## 2. 포크 빌드 (haijun93/mongdangview)

- 워크플로 `.github/workflows/docfit-portable.yml`(master). `windows-latest` + `actions/setup-dotnet`(.NET 10.0.x).
- 원본 `MakePackage/MakePackage.ps1 -Target Zip -continue`를 그대로 실행한다.
  - 스크립트 첫머리 `Read-Host`에는 빈 줄을 표준입력으로 넣는다.
  - 필요한 도구: pandoc 3.6.4(공식 zip, SHA256 고정 확인), NetBeauty2(`nulastudio.nbeauty` 2.1.5.1, dotnet 도구).
  - 결과: 자체 포함 x64 zip(.NET 설치 불필요)과 fd zip(.NET 10 데스크톱 런타임 필요).
- 후처리: zip 안 최상위 폴더 이름을 `NeeView`로 통일한다. 다음 파일을 넣는다.
  - `LICENSE.md`, `THIRDPARTY_LICENSES.md`, `THIRDPARTY_LICENSES.NeeLaboratory.IO.Search.md`
  - `SOURCE.txt`(포크 커밋 URL·원본·빌드 실행 URL·LGPL 안내)
  - 그다음 `SHA256SUMS.txt`를 만든다.
  - 앱 폴더에 풀 때는 zip 안 `NeeView\` 폴더의 내용을 `tools\mongdangview\`에 둔다.
- 산출물: 아티팩트 `neeview-docfit-portable`(30일). 수동 실행에서 `release=true`이면 릴리스 `docfit-v<버전>`으로 게시한다(릴리스 작업만 `contents: write`).
  - 파일 이름: `NeeView-<버전>-docfit-x64.zip`, `NeeView-<버전>-docfit-x64-fd.zip`, `SHA256SUMS.txt`.
  - 버전은 원본 규칙(`NeeViewVersion.xml` 기준 + 커밋 수)을 따른다.
- 외부 액션은 모두 커밋 SHA 고정: checkout v7.0.1, setup-dotnet v6.1.0, upload-artifact v7.0.2, download-artifact v8.0.2.
- 첫 실행 결과(2026-11-07, push 실행 42379881257): 성공(4분 11초), 아티팩트 약 124MB(자체 포함 zip + fd zip).
- 원본에서 따라온 `jekyll-gh-pages.yml`('Deploy Jekyll with GitHub Pages dependencies preinstalled')은 포크에서 실패하므로 2026-11-08 `gh workflow disable`로 껐다(파일은 원본과 같게 둔다).

## 3. 배포본 포함(2단계)

`docs/EXTERNAL_APPS.md` 4장을 따른다(포크 릴리스 zip을 lock 파일의 SHA256으로 확인해 첫 실행 때 `tools\mongdangview\`에 푸는 방식).
