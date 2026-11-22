# KORDOC2D — kordoc 의존 줄이기 2·3단계 설계

- 작성일: 2026-11-23
- 기준 요청: `TODO.md` 42행(2026-11-15 사용자 결정)
- 조사 기준: 저장소 스냅숏 `5ed0de6b`; 조사 당시 체크아웃은 `feature/KORDOC2D`였음. 사용자가 지정한 github/main 기준과 동일한지는 미확인.
- 범위: 설계·인벤토리만. 코드·설치기·런타임 번들 변경, 한/글 COM 실행, 외부 다운로드·설치 및 시험은 하지 않았다.
- 표기: `미확인`은 코드만으로 확인할 수 없는 실사용 빈도·용량·법률 판단·품질을 뜻한다.

## 1. 요약 및 현재 진행 위치

TODO의 2단계는 DOCX(Pandoc 경유), MD, MCP, XLS/XLSX, PPTX의 입력 내용을 Markdown 중간 표현과 보고서 스타일 HWPX로 옮기는 작업이다. 현재 변환 브리지는 kordoc의 파싱·생성 명령을 호출한다. 단, HWP5 본문 추출 및 미리보기는 Rust `klib` 우선으로 전환되어 있고 kordoc은 실패 대체 경로다. 타임머신도 같은 구조다.

3단계는 PDF 고급 레이아웃 변환과 OCR만 kordoc 묶음에 남기고 기본 설치에서 Node/npm 런타임과 OCR 구성요소를 제거하는 목표다. 디지털 PDF의 기본 글 추출은 이미 설치 의존성 `pypdfium2`를 쓰는 방향이 제시되어 있다. 현재 `HWP_AutoDocFit.spec`에는 런타임 ZIP을 넣는 항목이 있으며, 실제 기본 설치에서 제외하려면 spec·빌드·설치기·준비 상태·업데이트 경로를 함께 변경해야 한다.

## 2. kordoc 사용 인벤토리

아래 호출 횟수는 **코드 경로의 종류**다. 실제 사용자 호출 빈도나 문서별 호출량 계측은 없어 모두 미확인이다.

| 지점(파일:줄) | 호출·기능 | 입력 → 출력 | 화면/사용처 및 호출 횟수 |
|---|---|---|---|
| `docfit_core/kordoc_bridge.py:266-289` | `run_kordoc`: Node+CLI subprocess 공통 경계, 환경 변수로 모델·네트워크 제한 | CLI 인자/문서 → CLI 결과 | 모든 아래 경로의 공통; 실사용 횟수 미확인 |
| `docfit_core/kordoc_bridge.py:390-412` | `parse_document`; HWP5는 `klib extract` 성공 시 Rust 우선, 그 외 kordoc parse | HWP/HWPX/HML/PDF/XLS/XLSX/DOCX/PPTX/이미지 → Markdown/JSON/chunks 등 | 파일변환, 타임머신 HWP 실패 대체, 텍스트 내보내기 등; 호출량 미확인 |
| `docfit_core/kordoc_bridge.py:415-445` | `convert_to_hwpx`: 파싱 후 Markdown을 HWPX로 생성 | 지원 입력 → 보고서 프리셋 HWPX | 파일변환 UI·공용 파일 변환; 변환 작업당 보통 파싱+생성 2개 CLI 호출(분기별 실제 횟수 미확인) |
| `docfit_core/kordoc_bridge.py:617-629` | `generate_hwpx`: Markdown → HWPX | MD/AI 답변 → HWPX | `docfit_core/mcp_service.py:18,47-104`의 MCP `create_hwpx_from_ai_answer`; 데스크톱 기타 변환 경로; 호출량 미확인 |
| `docfit_core/file_conversion.py:107-160` | Pandoc 대상 형식 변환; kordoc parse로 원본을 Markdown화하는 분기 포함 | DOCX/MD/XLS/XLSX/PPTX 등 → Pandoc 지원 출력(중간 단계에 따라 HWPX 포함) | 파일변환 화면. Pandoc 존재 확인은 `desktop_web_ui.py:767-774`; 호출량 미확인 |
| `docfit_core/file_conversion.py:240-258,423-445` | PDF/HWPX 입력 변환 라우팅 | 비-HWPX 문서 → kordoc HWPX → 한/글 PDF; 지원 문서 → HWPX | 파일변환 UI. HWPX→PDF는 `convert_hwpx_to_pdf`에서 한/글 저장 사용, kordoc 불필요. 입력 파일/출력 목표별 실행 조건이 다름 |
| `docfit_core/file_conversion.py:493-545` | 이미지 및 스캔 PDF OCR 파싱/생성 | 이미지/PDF → OCR 텍스트 → HWPX | 파일변환 이미지/OCR 경로; OCR 모델 사용 여부는 설정·문서 종류에 따름 |
| `docfit_core/timemachine/extractors_hwp.py:78-117` | 타임머신 HWP5 추출: `klib extract` 우선, 실패 시 `parse_document` | HWP5 → 추출 문단/검색 레코드 | 타임머신 색인; 문서별 Rust 실패 건만 kordoc 호출. 건수 미확인 |
| `docfit_core/kordoc_bridge.py:340-388,498-532` | HWP5 Rust 파싱 및 미리보기 우선 경로 | HWP5 → 텍스트 또는 PNG | `klib` 성공 시 kordoc 미호출, 실패 시 대체. UI 진입별 횟수 미확인 |
| `desktop_web_ui.py:767-832` | 변환 사전 준비 상태: 엔진, OCR, Pandoc 및 입력 확장자를 함께 판정 | 선택 파일의 확장자/환경 → 준비 상태·안내 문자열 | 파일변환 UI 상태. `718행 부근`이라는 요청 지점은 현재 checkout에서 관련 계산이 767행부터 시작함 |
| `docfit_core/kordoc_bridge.py:467-494,551-614` | Markdown/IR/RAG 내보내기, 표·양식 분석, 비교·채우기·수정·lint | 지원 문서/MD → 분석 자료·수정 문서 | 앱 공개 API에서 고급 기능 9개를 제외한 기록은 `docfit_core/__init__.py:14-16`; 각 도구가 제품 화면에서 실제 연결되어 있는지는 미확인. MCP에 직접 노출됐다는 근거는 미확인 |
| `docfit_core/kordoc_runtime.py:1-13,96-155,192-250` | 런타임/ZIP 설치, Pandoc·OCR 모델 탐색 | ZIP·파일 시스템 → Node/kordoc/Pandoc 경로와 준비 상태 | 첫 사용/상태 조회 경로. `ensure_runtime`의 압축 해제는 필요 때 발생. 횟수 미확인 |

### 2.1 기본 설치 번들 구성과 확인 가능한 크기

| 구성 | 근거 및 현재 위치 | 확인된 크기 |
|---|---|---|
| Node.js | `scripts/build_kordoc_runtime.ps1:10-11,42-46`; stage `node/node.exe`, `LICENSE` | 실제 파일·압축 번들 크기 미확인 |
| kordoc와 npm 트리 | `kordoc_pack/package.json`의 kordoc `4.24.9`, `pdfjs-dist`; `scripts/build_kordoc_runtime.ps1:53-61`; stage `app/node_modules` | TODO와 `docs/KORDOC_STEP3.md`는 npm 의존성 229개라고 기록. 현재 lockfile 기준 재검산 및 실제 디스크/압축 용량 미확인 |
| OCR 의존성 | `kordoc_ocr_pack/package.json`: `@hyzyla/pdfium`, `onnxruntime-node`, `sharp`; 빌드 스크립트 63-82행에서 app node_modules에 합침 | 패키지·바이너리별 실제 크기 미확인. onnxruntime은 Windows x64만 남김 |
| OCR 모델 | `models/ppocr`에 `det.onnx`, `rec_korean.onnx`, `rec_korean.yml`; 모델을 별도로 내려받고 라이선스 동봉 | 모델 파일별 실제 크기 미확인 |
| Pandoc | 빌드 스크립트 13-14, 32-52행; stage `pandoc/pandoc.exe`, COPYING/COPYRIGHT; runtime manifest에 버전과 GPL-2.0-or-later 표기 | 실제 파일/압축 크기 미확인 |
| 런타임 ZIP | `release-assets/HWP_AutoDocFit_kordoc_runtime.zip` 기본 출력(`build_kordoc_runtime.ps1:7`); `HWP_AutoDocFit.spec:8`에서 `resources`에 포함 | 저장소 조사만으로 현재 산출 ZIP의 존재·바이트 크기·EXE 증가량 확인 안 됨. 스크립트가 빌드 시 MB를 출력하도록 설계됨(109-110행) |
| 설치/실행 배치 | `kordoc_runtime.py:28-29, 120-155, 192-246`; `installer`의 런타임 설치는 spec의 내장 리소스에 의존 | 현재 사용자 기본 설치 실제 크기 미확인 |
| libvips | TODO는 LGPL libvips 제거 대상이라 명시. 현재 OCR manifest의 `sharp` 사용은 확인되지만, 산출 ZIP에 libvips가 포함되는지와 동적/정적 배포 형태는 미확인 | 미확인 |

### 2.2 다른 제품면

- MCP: `docfit_core/mcp_service.py:18`에서 `generate_hwpx`를 가져오고 47행 이후 AI 답변 Markdown을 HWPX로 만듦. 즉 MCP HWPX 생성은 2단계 Markdown 생성기의 직접 소비자로 설계한다.
- 몽땅검색 색인: 이번 탐색에서 kordoc 전용 직접 호출은 확인되지 않음. 앱 전체 파일 색인에서 다른 포맷을 kordoc으로 처리한다는 근거는 미확인. 타임머신 HWP5만 Rust 우선/kordoc 대체가 직접 확인됨.
- 타임머신: HWP5 본문 추출만 `extractors_hwp.py`에서 직접 사용 확인. PDF/DOCX/XLSX 타임머신 추출이 kordoc을 호출하는지는 미확인.
- 파일변환: `file_conversion.py`가 직접 사용. 선택 형식과 경로에 따라 파싱·생성·Pandoc·한/글 PDF 저장을 조합.

## 3. 기능별 대체 경로 표

표의 위험 평가는 문서 구조 변환에서 예상되는 검증 위험이며, 실제 결과 품질은 같은 문서 묶음으로 확인해야 한다.

| 입력 | 현재 kordoc 경로 | 대체 후보 | 주요 품질 위험 | 외부 의존/라이선스 확인 |
|---|---|---|---|---|
| DOCX | `parse_document` → Markdown, 필요 시 `generate_hwpx`; 또는 `file_conversion`에서 Pandoc | Python `python-docx` 기반 추출 + 내부 Markdown→HWPX; Pandoc 변환은 선택 유지; Rust klib는 DOCX 지원 미확인 | 제목/스타일 위계, 번호 목록, 병합 셀, 표 안 그림·각주·머리말 손실 가능 | Pandoc은 현재 빌드 스크립트에서 GPL-2.0-or-later로 표기됨(`runtime.json` 설정). 제품에 번들할 법률상 가능 여부·의무 충족은 미확인. python-docx 라이선스/번들 검토 미확인 |
| MD | kordoc `generate_hwpx`; Pandoc 경유 형식 변환 일부 | 자체 Markdown→보고서 HWPX 생성기(주 경로), Pandoc 후보 | Markdown 확장 문법, 표/중첩 목록, 이미지 경로, 링크, 코드블록 보존 | Pandoc 라이선스 위와 같음. 생성기 자체는 내부 구현 예정 |
| MCP 응답 Markdown | `mcp_service.create_hwpx_from_ai_answer` → kordoc 생성 | 자체 Markdown→HWPX 생성기 공유 | 비정형 AI 표·목록·이미지 참조, 긴 응답·특수문자 처리 | MCP 전송 라이브러리와 생성기 배포 영향 미확인 |
| XLS/XLSX | kordoc parse → Markdown → HWPX/Pandoc | `openpyxl`/`python-calamine` 등 시트·셀 추출 후 Markdown 생성; 자체 생성기 | 수식 계산값, 병합셀, 숨김 행/열, 여러 시트, 숫자/날짜 형식, 표 너비 | 라이브러리 선택 및 라이선스/크기 미확인. openpyxl은 현재 requirements에 없음을 확인 |
| XLS(구형) | kordoc parse | `xlrd` 또는 변환 라이브러리 후보; Rust klib 지원 미확인 | 서식·수식·차트와 구형 BIFF 호환성 | 후보 선택·라이선스·보안 업데이트 미확인 |
| PPTX | kordoc parse → Markdown/HWPX | `python-pptx`로 슬라이드·텍스트·표·그림 추출 후 자체 생성기; Pandoc 후보 | 위치/레이아웃, 도형 안 텍스트 순서, SmartArt, 차트, 그룹 도형 손실 | python-pptx의 라이선스/번들 조건 조사 필요(미확인) |
| HTML | kordoc/Pandoc 파싱 경로 여부는 입력 지원 목록에 없음; Pandoc 지원 포맷 경유 가능 | 표준 라이브러리/HTML parser → 정제된 Markdown; 자체 생성기 | CSS 레이아웃, 의미 없는 태그, 중첩 표, 외부 이미지·스크립트 | 실제 현재 UI 지원 형식은 미확인. HTML 파서 후보/라이선스 미확인 |
| PDF(디지털) | kordoc parse; PDF→PDF 조판 시 HWPX 중간 변환 가능 | `pypdfium2` 기본 텍스트 추출; 필요 시 기존 PDF 텍스트 라이브러리 조사 | 다단 편집 순서, 표/그림/머리글, 읽기 순서·페이지 경계 손실. 기본 추출은 레이아웃 복원을 보장하지 않음 | `requirements.txt`에 `pypdfium2==5.14.0` 확인; 현재 번들링 조건·바이너리 크기 미확인 |
| PDF(스캔)/이미지 | OCR 엔진·한국어 모델 + kordoc 경로 | 고급 꾸러미 내 OCR; 기본은 스캔 판정과 안내, 이미지 PDF 페이지 보존 변환 | OCR 누락/오인식, 표/열·읽기 순서 저하 | OCR 런타임과 모델 라이선스 검토 필요. PaddleOCR 라이선스 파일 복사(빌드 82행)는 확인, 전체 의무 충족 미확인 |
| HWP5 | `parse_document`; `klib extract` 우선(텍스트 출력 조건 한정), 실패 시 kordoc. 미리보기 `klib preview` 우선 | Rust `klib extract`/preview 확대; HWPX/XML 파서 후보 | klib 비교 도구의 문자 재현율 목표 99.5%, 정밀도 98%는 문서에 기록됐으나 실문서 통과·표 구조 동등성 미확인. 암호/배포용 문서 fallback 필요 | Rust 내부 구성. `klib/examples/hwp5_vs_kordoc.rs`는 실제 코드 대조 도구이며 그 기준을 DOCX 등으로 일반화하지 않음 |
| HWPX | 현재 변환 브리지 입력으로 kordoc 지원; HWPX→PDF는 한/글 COM 직접 저장 | 내부 ZIP/XML 처리(`docfit_core.hwpx` 등), 한/글 네이티브 경로; Rust parser 범위는 미확인 | XML 요소·개체/수식/차트/필드/스타일 보존. 직접 생성/수정과 재조판 경로 구분 | 내부 코드. 한/글 자동화는 COM 실행이 필요한 Windows 단계에서만 검증 |
| HML | kordoc 지원 입력 | 내부 XML parser 또는 HWPX 공통 생성 파이프라인 | HML 변종·태그·인코딩 편차 | 지원 범위·라이선스 미확인 |
| PNG/JPG/JPEG/WebP | kordoc + OCR 모델로 글자 추출 후 HWPX; 이미지→PDF 직접 경로도 있음 | 이미지→PDF 보존 + 선택 OCR; OCR 후 내부 생성기 | 이미지 그대로 보존과 인식 텍스트 문서화는 별도 기능. 표/도형 추론 위험 | pypdfium2·Pillow 현재 요구사항 확인. OCR 패키지 라이선스·실제 배포물 크기 미확인 |

`Pandoc`은 현재 `scripts/build_kordoc_runtime.ps1:93`이 `GPL-2.0-or-later`로 manifest를 기록하며 실행 파일과 COPYING/COPYRIGHT를 번들에 복사한다. 이는 저장소가 수행하는 표기 사실이다. 앱과 함께 배포할 수 있는지에 대한 법률 결론은 이 설계의 범위 밖이며 **미확인**이다. 2단계 목표는 Pandoc을 계속 기본 포함한다고 가정하지 말고, 라이선스 검토 및 자체 생성기 출력 품질을 선행 조건으로 둔다.

## 4. 품질 기준과 비교 방법

### 4.1 고정 문서 묶음

1. `tests/fixtures`에서 외부 라이선스·개인정보 문제가 없는 문서를 형식별로 선별한다. 현재 fixtures 전체 형식/수량 목록은 미확인. 원본 fixture는 복사본으로만 실행한다.
2. fixtures의 빈칸은 합성 문서로 채운다: 3단 제목, 번호/글머리/중첩 목록, 일반·병합 셀 표, 표 안 줄바꿈, 인라인/블록 그림 및 캡션, 페이지 구분, 한국어·영문·숫자, 링크. XLSX는 수식·날짜·복수 시트, PPTX는 복수 슬라이드·도형 텍스트, PDF는 디지털/스캔 쌍을 둔다.
3. 입력별로 kordoc 현재 결과와 후보 결과를 각각 별도 임시 폴더에 생성한다. 원본 SHA-256, 엔진·버전, 실행 인자, 경고를 결과 manifest에 기록한다. 매번 동일한 입력 바이트와 프리셋을 사용한다.
4. HWPX 산출물은 ZIP/XML 구조 검증과 텍스트 추출로 기계 비교한다. 화면 렌더링 비교는 승인된 Windows 검증 단계에서 수행하며 이번 설계 단계에서 COM을 호출하지 않는다.

### 4.2 지표 정의

| 요소 | 지표(정의) | 산출 방법 |
|---|---|---|
| 제목 | 제목 레벨별 보존율 = 대체 결과에서 대응 텍스트와 레벨이 보존된 기준 제목 수 / 기준 제목 수 | 원본 fixture의 기대 구조 manifest와 출력 HWPX 스타일/개요 XML을 대응 |
| 목록 | 항목 텍스트 보존율 및 유형·깊이 일치율 | 번호/글머리 목록의 정규화 텍스트와 계층을 기대 manifest와 비교 |
| 표 | 표 개수 일치율, 행·열 수 일치율, 셀 텍스트 보존율 | 표 셀을 좌표별 정규화 비교. 병합 셀은 span을 별도 비교 |
| 그림 | 그림 개수·바이트 해시 보존율, 캡션 연결률 | HWPX BinData 수와 fixture 원본 그림 해시, 캡션 문단 관계 비교 |
| 본문 | 핵심 텍스트 재현율/정밀도 | 공백·줄바꿈을 정규화한 기대 텍스트에 대해 문자/토큰 일치. 페이지 번호 등 무시 규칙은 manifest에 명시 |
| 산출 유효성 | HWPX ZIP/XML 유효성, 열기 가능한지 여부, 빈 결과/누락 경고 | 기존 HWPX 검증기 및 검사 스크립트; 실제 한/글 열기 검증은 Windows 승인자 수행 |

### 4.3 자동 판정기 설계 및 초기 합격선

- 신규 도구 제안: `scripts/compare_conversion_fidelity.py`와 `tests/fixtures/conversion_fidelity/manifest.json`. 구현 카드에서 작성한다. 현재 해당 도구는 존재하지 않는다.
- 판정기는 원본·두 엔진 결과 디렉터리를 받아 HWPX 텍스트/스타일/표/그림 지표를 JSON 및 Markdown으로 저장한다. 픽셀 렌더 비교는 별도 Windows 단계로 분리한다.
- **초기 합격선 제안(사용자 승인 전 잠정)**: HWPX 구조 검증 100%, 기대 제목·목록 항목·표 셀 텍스트·그림 수 보존 100%, 표 행/열·목록 계층 일치 100%, 핵심 텍스트 재현율 99.5% 이상, 정밀도 99.0% 이상. 입력별 반복 3회 결과가 동일해야 한다. 페이지 조판/레이아웃 픽셀 동등성은 합격 조건으로 삼지 않고 수동 검토와 별도 기록으로 관리한다.
- 위 수치는 TODO의 HWP5 klib 문자 기준과 다른 형식에 대한 신규 제안이다. 문서 묶음으로 현실성 검증 전에는 정책으로 확정하지 않는다. 어떤 정보 손실이 허용되는지 사용자 결정 필요.
- 모든 필수 요소는 필수 fixture별로 개별 통과해야 하며 평균 점수로 표·그림 누락을 상쇄하지 않는다. 스캔 PDF는 OCR 정답 텍스트로 별도 집계하며 일반 문서와 합산하지 않는다.

## 5. ‘고급 변환 엔진’ 분리 설계

### 5.1 경계

| 기본 설치(core) | 추가 꾸러미(advanced) |
|---|---|
| 앱·한/글 기능 및 기존 Rust `klib` 기능 | PDF 고급 레이아웃 파싱 및 kordoc CLI |
| 디지털 PDF 기본 텍스트 추출용 `pypdfium2` (현재 requirements에 있음) | 이미지/스캔 PDF OCR 실행 의존성 및 한국어 모델 |
| 2단계 자체 Markdown→HWPX 생성기 | 고급 PDF 변환이 요구하는 별도 Node/npm 트리(정확한 필요 패키지 수는 분리 후 재산정) |
| HWP/HWPX 경로 및 이미지의 원본 보존형 PDF 변환 | 필요 라이선스 고지·버전/해시 manifest |

제품 사용성 목표는 기본 설치에서 Node.js, npm 229개, onnxruntime, sharp/libvips 관련 부담을 제거하는 것이다. 현재 OCR package는 kordoc과 동일 `app/node_modules` 트리에 병합되므로 기본/추가 경계는 아직 구현되지 않았다. Pandoc은 GPL-2.0-or-later 표기가 있으므로 기본 번들에 두지 않는 것을 초기 설계로 하고, 라이선스 검토 후 별도 결정을 기록한다. 추가 꾸러미의 최종 크기·압축 방식·배포 위치·자동 업데이트 방식은 미확인.

### 5.2 꾸러미 없을 때 동작과 안내

- 디지털 PDF: `pypdfium2`로 페이지별 텍스트를 추출해 기본 Markdown/HWPX 변환을 제공한다. 제목/목록/표/그림 레이아웃 보존이 보장되지 않음을 결과 경고로 표시한다. 추출 텍스트가 비어 있으면 스캔 PDF 가능성을 알린다.
- 스캔 PDF/이미지 OCR: 원본 이미지/PDF 보존 변환은 지원 가능한 범위에서 제공하고, 글자 인식은 비활성화한다. “이 파일은 글자가 그림으로 된 문서라 기본 글 추출 결과가 없을 수 있습니다. 글자 인식과 고급 PDF 변환을 사용하려면 ‘고급 변환 엔진’ 추가 꾸러미를 설치하세요.” 문구를 제안한다.
- 고급 PDF 레이아웃 변환: 선택 시 실패 메시지 대신 추가 꾸러미 설치 동작/다운로드 페이지 안내를 제공한다. 네트워크 자동 다운로드는 하지 않는다. 폐쇄망에서는 설치 파일을 별도 전달받는 방법을 안내한다.
- 꾸러미가 설치되어도 OCR 모델이 빠졌거나 손상되면 고급 PDF와 OCR 준비 상태를 각각 구분해 안내한다. `engine_status`, `ocr_available`를 단일 true/false로 묶지 않는다.

### 5.3 설치기 구성요소 선택과 연결

`docs/INSTALL1_DESIGN_2026-11-22.md`는 구성요소 `core` 고정, `search`, `vfolder`, `clip` 선택안과 설치된 실행/등록만 선택 제어하는 1단계 설계를 설명한다. dist 단일 묶음이라 파일별 제외는 어렵다고 기록한다. kordoc 추가 꾸러미는 별도 ZIP/별도 다운로드 경로로 경계를 두어야 하므로 설치기 선택 컴포넌트와는 별도 층위다.

권장안: core 설치 화면에는 “고급 변환 엔진 포함” 선택을 추가하되 기본값은 끔으로 제안한다. 선택 시 installer가 별도 산출물의 존재·해시를 검증하고 함께 설치할 수 있게 한다. Inno Setup이 외부 ZIP을 선택 구성요소로 처리하는 구체 구성과 자동 업데이트의 기존 `/SILENT` 호환성은 미확인이다. 설치기 설계 카드에서 단일 dist를 나눌지 별도 ZIP 설치 작업으로 둘지 결정한다. 기존 자동 업데이트에 번들 경로가 고정되어 있는지도 구현 전 조사한다.

### 5.4 변환 준비 상태 수정 지점

- `desktop_web_ui.py:767-832`: 현재 공통 `kordoc`, `ocr`, `pandoc` 준비 상태를 구하고, PDF/HWPX별 필요성을 계산한다. 입력별 capability 기반(`basic_text`, `hwpx_writer`, `pdf_advanced`, `ocr`, `hancom_pdf`) 결과로 바꾸는 카드가 필요하다.
- `desktop_web_ui.py:795-828`: kordoc 미설치이면 전체 변환을 막는 현재 조건을 기본 엔진 가능 여부와 고급 기능 가능 여부로 나눈다. PDF 기본 텍스트 추출 가능 상태와 고급 변환/ OCR 필요 상태를 분리한다.
- `desktop_web_ui.py:832` 이후 병합·다중 입력 준비 판정에도 동일 capability 모델을 적용한다. 단일 문서 기준만 고쳐 다중 입력 배치에서 잘못 “준비됨”으로 표시하는 회귀를 막는다.
- `docfit_core/kordoc_bridge.py:192-245` 및 `docfit_core/kordoc_runtime.py`의 런타임 탐색은 선택 엔진 상태 조회로 축소하고 기본 경로가 런타임 설치를 시도하지 않도록 경계를 정한다.
- `HWP_AutoDocFit.spec:8`의 런타임 ZIP 추가, `scripts/build_kordoc_runtime.ps1:1-110` 산출, installer와 릴리스 자산 경로에서 기본 번들 참조를 제거하고 별도 산출/설치 절차를 연결한다.

## 6. 단계별 카드 계획

규모: XS(≤1일), S(2~3일), M(4~7일), L(8일 이상) 개발자 추정. Windows 실기 검증은 별도 워커/카드로 둔다. 모두 문서 기준 계획이며 일정 미확인.

| 카드 | 내용 | 완료 조건 | 의존 | 규모 | 워커 종류 |
|---|---|---|---|---|---|
| KORDOC2D-A 기준선·fixture 인벤토리 | github/main 커밋 확인, 실제 변환 UI 호출 맵, fixture 형식·수량·민감도 목록, 현재 번들 ZIP 크기/설치 EXE 차이 계측 | 보고서에 파일:줄, 입력별 실행 횟수 계측 방법, 산출 크기 실측값 기록 | 없음 | S | 저장소 조사 + Windows 빌드 아티팩트 분석 |
| KORDOC2D-B 품질 비교 harness | 합성 문서 및 fixtures manifest, 두 경로 실행/비교기 작성 | 구조 지표와 실패 샘플 자동 리포트, Linux/macOS에서 가능한 ZIP/XML 비교 완료 | A | M | Python 구현 + fixture 생성 |
| KORDOC2D-C Markdown→HWPX 생성기 | 내부 writer 설계·구현(제목/목록/표/그림 우선), MCP·MD 연결 | B의 모든 필수 fixture 합격, HWPX 검증 통과, 원본 스타일/경고 기록 | B | L | Python/HWPX 구현; Windows 검증자 |
| KORDOC2D-D 문서 입력 어댑터 이전 | DOCX·XLS/XLSX·PPTX 파싱을 Python/적합 대체로 옮기고 HTML 여부 확정 | 형식별 fixtures에서 kordoc 기준 품질선 통과; 라이선스·의존성 목록 승인 | C, B | L | Python 형식 전문가 + 의존성 검토 |
| KORDOC2D-E 변환 라우팅·MCP 마이그레이션 | `file_conversion.py`, MCP 경로를 어댑터+공통 writer로 전환 | kordoc 미설치 환경에서도 목표 범위 변환·MCP 생성 완료, 오류/취소 안내 검증 | C, D | M | Python 통합 |
| KORDOC2D-F advanced 런타임 분할 | 런타임 빌드·PyInstaller spec·installer·설치 안내/업데이트 경로 분리 | 기본 EXE/설치 용량 실측 감소, 추가 ZIP 단독 설치·업데이트·제거 검증, 고지 완비 | A, E, 라이선스 검토 | L | Windows 패키징/installer |
| KORDOC2D-G capability 준비 상태 UI | 엔진 상태를 basic/advanced/OCR/PDF/COM capability로 나눔 | 입력 단일·혼합 선택별 상태와 행동이 표준 시나리오에 일치 | F | M | Python UI + 제품 검증 |
| KORDOC2D-H 실문서 회귀/출시 판정 | 합성 및 승인 실문서 묶음에서 kordoc 대비 대체 경로 평가 | 형식별 합격선 통과, 예외/미지원 형식 표시, 크기·성능·실패율 기록 | C-G | M | Windows 문서 QA, COM 사용 시 지정 lock 필수 |

진행 순서 권고: A → B → C → D → E → F/G → H. KORDOC3B에서 pypdfium2 경로의 표 유형 결과가 96.97%로 99% 기준 미달하여 연결하지 않았다는 `docs/TODO_AUDIT_2026-11-21.md:29` 기록이 있다. 따라서 PDF 텍스트 추출을 “동등 변환”으로 간주하지 말고 구조 보존 보장 범위를 UI에서 나눈다.

## 7. 주요 위험 3가지와 대응

1. **문서 구조 손실을 평균 점수로 감춤**: 제목·표·그림 등 핵심 구조별 독립 합격선을 적용하고, 하나라도 누락되면 해당 형식은 전환 보류. 원본 보존과 경고를 유지한다.
2. **기본/추가 경계 변경으로 설치·업데이트 불일치**: spec, Inno 구성요소, 기존 `/SILENT` 업데이트 및 선택 유지 동작을 통합 시나리오로 시험한다. 번들 산출 hash·manifest를 고정하고 제거·복구 경로를 만든다.
3. **추가 의존 라이선스·크기·공급망**: Pandoc GPL 표기 및 OCR(onnxruntime/sharp/libvips 포함 여부)·새 Python 라이브러리의 정확한 라이선스와 배포 산출물을 확인하기 전 기본 번들에 넣지 않는다. 실측 용량, SBOM/고지, lockfile·해시를 카드 완료 조건으로 둔다.

## 8. 사용자 결정이 필요한 사항

- 잠정 합격선(구조 100%, 텍스트 재현율 99.5%/정밀도 99%)을 채택할지, 형식별 예외를 허용할지.
- Pandoc GPL-2.0-or-later 표기와 실제 배포 조건을 법률/라이선스 검토할 담당·완료 조건. 검토 전 Pandoc 기본 번들 지속 여부는 확정 불가.
- 고급 변환 엔진 선택 기본값(기본 설치에 포함 여부), 별도 ZIP 제공 위치, 설치기 통합 대 수동 설치 안내 중 제품 방침.
- 저품질/텍스트만 추출된 PDF 결과를 자동 생성할지, 사용자 확인 후 생성할지.
- 품질 문서 묶음에 넣을 실문서 제공·보관 정책(기밀/개인정보 제거 및 승인).
- kordoc의 앱 미연결 고급 기능(표 분석·폼·patch/lint/IR/RAG)을 실제로 폐기할지 보존할지. 현재 공개 API에서 제외됐다는 사실 외 제품 사용 여부는 미확인.

## 9. 조사하지 못한 점

- 사용자 PC/현재 release artifact에서 ZIP, Node, npm 각 패키지, onnxruntime, 모델, sharp/libvips, Pandoc의 실제 파일 크기와 설치 용량 기여도.
- npm 229개 수치의 현재 lockfile 재검산 및 OCR 의존성이 이 수치에 포함되는 기준.
- 실제 사용자 화면 호출 빈도, 파일 형식별 사용량, UI에서 MCP 기능을 호출하는 제품 흐름.
- 현재 `tests/fixtures` 문서의 형식별 수량 및 원본 비교 허용 범위.
- Pandoc GPL 배포 적법성에 대한 법률 결론, 모든 npm/Python 의존의 현재 라이선스 의무.
- 고급 꾸러미 배포 채널·버전 호환·설치기 자동 업데이트 처리·무결성 키 관리.
- klib 대 kordoc 99.5%/98% 목표의 최신 Windows 실문서 결과 및 본문 색인 재생성 필요 여부.
- 현 checkout이 github/main과 같은 커밋인지. 현재 `feature/KORDOC2D`, HEAD `5ed0de6b`로 확인됨.

## 근거 파일

- `TODO.md:42-43`
- `docfit_core/kordoc_bridge.py:266-629`
- `docfit_core/kordoc_runtime.py:1-250`
- `docfit_core/file_conversion.py:96-160,240-258,423-545`
- `docfit_core/timemachine/extractors_hwp.py:1-117`
- `docfit_core/mcp_service.py:18,47-104`
- `desktop_web_ui.py:767-832`
- `scripts/build_kordoc_runtime.ps1:1-110`, `kordoc_pack/package.json`, `kordoc_ocr_pack/package.json`
- `HWP_AutoDocFit.spec:8`, `docs/INSTALL1_DESIGN_2026-11-22.md`, `docs/KORDOC_STEP3.md`
- `docs/KORDOC3B_2026-11-18.md`, `docs/TODO_AUDIT_2026-11-21.md`, `klib/examples/hwp5_vs_kordoc.rs`
