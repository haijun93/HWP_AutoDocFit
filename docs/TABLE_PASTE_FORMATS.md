# 한/글 표 클립보드 복사 포맷 조사 결과 (Windows PC 실측)

- **조사 일시**: 2026-11-08
- **실행 환경**: Windows 11, 한/글 2020 COM (`HwpFrame.HwpObject`) 격리 프로세스
- **조사 도구**: `scripts/inspect_hwp_table_clipboard.py`
- **COM 잠금**: `C:\HWP_AUTODOCFIT\.com-test.lock` (원자적 파일/디렉터리 생성 및 해제)

---

## 1. 개요 및 시험 절차

한/글 2020에서 표가 포함된 문서를 열고 표를 복사(Copy)했을 때, Windows 클립보드에 등록되는 모든 데이터 포맷과 각 포맷의 구조·특성을 분석하였다.
사용자 작업 환경 보호를 위해 기존 실행 중인 사용자 한/글 프로세스를 건드리지 않고, 별도의 격리된 `DispatchEx` 인스턴스에서 임시 HWPX(2×2 표, 배경색 `#E2EFDA`, 이중선/굵은선 테두리, 텍스트 정렬, 굵게 서식 포함)를 로드하여 `SelectAll` → `Copy`를 수행하였다.

---

## 2. 감지된 클립보드 포맷 목록 (총 11개)

| 순번 | 포맷 ID | 포맷 이름 (Format Name) | 크기 (Bytes) | 주요 용도 및 특성 |
|:---:|:---:|:---|:---:|:---|
| 1 | 49161 | `DataObject` | 4 | OLE 클립보드 데이터 전송 핸들 |
| 2 | 49837 | `Hwp_Native_Info` | 8 | 한/글 고유 포맷 메타정보 |
| 3 | 49663 | `Hwp Native` | 14,336 | 한/글 고유 바이너리/복합 문서 스트림 (한/글 간 완벽 붙여넣기용) |
| 4 | 49921 | `DOCX Format` | 4,588 | Word 등 워드프로세서 호환용 OpenXML 압축 스트림 |
| 5 | 49281 | `Rich Text Format` | 4,754 | RTF 서식 (글꼴 테이블, 셀 테두리 및 텍스트) |
| 6 | 49458 | **`HTML Format`** | 19,767 | **웹/타 앱 호환용 HTML 표 조각 (CF_HTML 표준)** |
| 7 | 13 | `CF_UNICODETEXT` | 31 | 유니코드 평문 (열은 탭/공백, 행은 줄바꿈 구분) |
| 8 | 1 | `CF_TEXT` | 41 | ANSI 평문 |
| 9 | 49171 | `Ole Private Data` | 344 | OLE 내부 관리 데이터 |
| 10 | 16 | `CF_LOCALE` | 4 | 로캘 식별자 (한국어: 0x0412) |
| 11 | 7 | `CF_OEMTEXT` | 41 | OEM 인코딩 텍스트 |

---

## 3. 포맷별 세부 분석 및 표그림판 활용 가능성

### 3.1. `HTML Format` (CF_HTML) — **표그림판 최우선 활용 포맷**
- **헤더 규격**:
  ```http
  Version:1.0
  StartHTML:0000000105
  EndHTML:0000019767
  StartFragment:0000001096
  EndFragment:0000019735
  ```
- **본문 내용**:
  `<!--StartFragment-->`와 `<!--EndFragment-->` 사이에 완전한 `<table>` 엘리먼트가 제공됨.
- **포함되는 표 속성**:
  1. **셀 크기**: `<td>`의 인라인 style 속성에 `width:56.69pt; height:28.35pt;` (pt 단위)로 정밀하게 명시됨.
  2. **셀 배경색**: `background:#e2efda;` 형식으로 16진수 색상 코드 제공.
  3. **셀 테두리**:
     - `border-top:solid #000000 1.70pt;`
     - `border-bottom:double #000000 1.98pt;`
     - `border-left:solid #000000 0.28pt;`
     - `border-right:solid #000000 0.28pt;`
     - 선 종류(`solid`, `double`, `dash`, `none`), 선 색상(`#000000`), 두께(`pt`) 완벽 보존.
  4. **셀 병합**: `colspan="N"`, `rowspan="M"` 속성 보존.
  5. **텍스트 및 정렬**:
     - 셀 내부 `<p>`에 `text-align:center`, `left`, `right`, `justify` 명시.
     - `<b>`, `<strong>` 또는 `font-weight:bold` 서식 보존.
- **장점**:
  - COM 프로세스 기동이나 디스크 파일 I/O 없이 클립보드 메모리에서 순수 파이썬(표준 라이브러리 `html.parser` 또는 `defusedxml`)으로 즉시 파싱 가능.
  - 가볍고 빠르며 다른 프로그램(Excel, Word, 브라우저 등)에서 복사된 표와도 구조적으로 호환성이 뛰어남.

### 3.2. `Hwp Native`
- **구조**: Compound File Binary(OLE Storage) 포맷 (`\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1` 시그니처).
- **특징**: 한/글 내부 문서 엔진의 전체 데이터 모델을 담고 있어 한/글 간 붙여넣기 시 원본 100% 복원 가능.
- **한계**: 바이너리 파싱이 매우 복잡하거나 한/글 COM 기동을 통한 변환이 필요하여 속도 및 자원 비용이 큼.

### 3.3. `Rich Text Format` (RTF)
- **구조**: `{\rtf1\ansi\kis94... \trowd \cellx...}`.
- **특징**: 셀 너비와 텍스트는 보존되나, 복잡한 병합 표나 CSS 스타일 파싱 대비 파서 복잡도가 높고 실익이 적음.

### 3.4. `CF_UNICODETEXT`
- **구조**: 탭(`\t`)과 줄바꿈(`\r\n`)으로 구분된 단순 텍스트.
- **한계**: 선 모양, 테두리 두께, 배경색, 병합 구조 정보가 소실됨.

---

## 4. 표그림판 변환 전략 및 결론

1. **최우선 읽기 포맷**: `HTML Format`
   - 한/글이 제공하는 `HTML Format`은 표그림판에 필요한 모든 정보(셀 크기, 병합, 4변 테두리 굵기/모양/색, 셀 배경색, 텍스트 정렬/굵게)를 완벽하게 유지하고 있음.
   - 따라서 `docfit_core`의 순수 파이썬 함수(`html.parser` 기반)를 통해 클립보드의 `HTML Format`을 표그림판 `sketch` (`grid`, `lines`, `texts`, `fills`)로 즉각 변환하는 것이 가장 안전하고 빠르며 효과적임.
2. **UI 연동**:
   - 표그림판의 **'한/글 표 붙여넣기' (Ctrl+V)** 동작 시, `paste_hwp` 호출 결과를 기존 '한/글 문서에서 표 가져오기'와 동일하게 **겹쳐 보기(overlay preview) → 적용/취소 → 되돌리기(Ctrl+Z)** 흐름으로 연동하여 사용자의 기존 그림 덮어쓰기 사고를 방지하고 작업 안전성을 확보함.
