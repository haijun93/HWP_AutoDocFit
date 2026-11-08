# XLSHADE: 엑셀 표 '목표 결과' 붙여넣기 차이 수정

- 작업 일자: 2026-11-08
- 작업 브랜치: `feature/xlshade` (기준: `origin/main` - v2.2.39 `edec4d1`)
- 대상 모듈: `docfit_core/excel_paste_target.py`, `docfit_core/excel_table.py`, `tests/test_excel_paste_target.py`, `TODO.md`

---

## 1. 개요 및 배경

`v2.2.39`에서 몽땅클립보드 엑셀 표 '목표 결과' 붙여넣기 연결(`feature/xllink`, `feature/xlpaste`) 후, Windows 실기기(한/글 2020)에서 확인한 결과(`reports/VERIFY1108.final.md` 1번) 다음 3가지 차이점이 발견되었습니다:

1. **셀 음영이 문단 배경으로 들어감**: 엑셀의 `<style>` 클래스(`.xl65` 등) 및 인라인 `background` 색상을 한/글이 칸(셀) 배경이 아닌 글자/문단 배경으로 해석하여 칠함.
2. **숫자가 왼쪽 정렬됨**: 회계 서식의 앞 공백을 제거하면서 엑셀의 오른쪽 정렬 속성이 명시되지 않아 한/글에서 기본값인 왼쪽 정렬로 표시됨.
3. **칸 여백 미반영**: `<style>` 안의 `td` padding 규칙을 한/글 2020 파서가 무시하고 `<table>`의 `cellpadding=0`으로 인해 칸 여백(상하 141, 좌우 510)이 정상 적용되지 않고 0/미지정으로 들어감.

---

## 2. 세부 수정 내역

### (1) 셀 음영 → 칸(셀) 배경(`bgcolor`)으로 변환 (`docfit_core/excel_paste_target.py`)
- 각 셀(`<td>`, `<th>`) 변환 시 클래스(`.xl65` 등)와 인라인 `style`에서 배경색(`background`, `background-color`)을 추출하여 `<td bgcolor="#RRGGBB">` 속성으로 부여.
- `_color` 함수를 개선하여 16진수 색상(#RRGGBB, #RGB) 및 표준 색상명뿐 아니라 `rgb(r, g, b)` 형식도 16진수 색상 코드로 변환 지원.
- 한/글이 문단 배경으로 오인하는 것을 방지하기 위해:
  - 각 셀 인라인 `style`에서 `background*` 및 `mso-pattern` 속성을 제거.
  - `shading="keep"`일 때도 `<style>` 블록 안의 클래스 규칙에서 `background*` 선언을 제거하여 문단 배경 생성을 원천 차단.
- `shading="strip"`(음영 제거) 모드에서는 `bgcolor` 속성을 부여하지 않고, `strip_cell_shading_report`를 통해 모든 음영 속성을 완전히 제거.

### (2) 숫자 오른쪽 정렬 및 엑셀 정렬 보존 (`docfit_core/excel_paste_target.py`)
- 숫자 셀(`kind == "numeric"` 또는 정규식 `_NUMERIC` 매칭):
  - 앞 공백이 제거되더라도 엑셀의 오른쪽 정렬이 보존되도록 `align="right"` 속성 및 인라인 `style='text-align:right'`를 강제 부여.
- 일반 셀:
  - 엑셀 클래스/인라인 스타일의 정렬(`left`, `center`, `right`, `justify`)을 감지하여 태그 `align` 속성과 인라인 `text-align`에 일치시켜 정렬 누락 방지.

### (3) 나머지 차이: 칸 여백 인라인 적용 및 서식 보존 (`docfit_core/excel_paste_target.py`)
- 한/글 2020이 `<style>` 안의 `td` 선택자를 읽지 못하는 문제를 해결하기 위해, 모든 셀(`<td>`, `<th>`)의 인라인 스타일에 직접 정상 칸 여백(상하 1.4pt, 좌우 5.1pt ≈ 141, 510 한/글 단위)을 부여:
  - `padding-top:1.4pt;padding-right:5.1pt;padding-bottom:1.4pt;padding-left:5.1pt;`
- `<table>` 태그에서 `cellpadding=0`을 제거하여 한/글이 칸 여백을 0으로 덮어쓰지 않도록 수정.
- 추가로 세로 정렬(`valign="middle"` / `vertical-align:middle`), 굵기(`font-weight:bold`), 테두리(`border`) 등의 핵심 서식을 각 셀 인라인에 반영하여 한/글 붙여넣기 시 서식 완전성 확보.

---

## 3. 시험 및 검증

### 단위 시험 추가 및 갱신 (`tests/test_excel_paste_target.py`)
- `test_shading_kept_by_default_and_strip_option`: 셀 음영이 `bgcolor="#FFFF00"` / `bgcolor="#C0D5E6"` 속성으로 들어가고, `<style>` 및 인라인에서 문단 배경용 `background`가 제거되었는지 검증. `shading="strip"` 시 `bgcolor` 및 모든 음영이 지워지는지 검증.
- `test_numeric_alignment_preserved_even_when_spaces_removed`: 앞 공백 제거 후에도 `align=right` 및 `text-align:right`로 엑셀 오른쪽 정렬이 보존되는지 검증 (신규 추가).
- `test_cell_shading_inlined_as_bgcolor_and_style_shading_stripped`: 클래스 정의 음영이 `bgcolor`로 전환되고 스타일시트의 `background`는 제거되는지 검증 (신규 추가).
- `test_cell_padding_inlined_to_each_td`: 모든 셀에 상하 1.4pt, 좌우 5.1pt 여백이 인라인 부여되고 `cellpadding` 속성이 제거되는지 검증 (신규 추가).
- `test_rgb_color_and_various_number_formats_alignment`: `rgb(r, g, b)` 배경색의 `bgcolor` 변환 및 통화(`₩ 120,000`), 괄호 음수(`(50,000)`), 백분율(`98.5%`) 숫자의 오른쪽 정렬(`align=right`, `text-align:right`) 및 기타 폰트 서식 유지 검증 (신규 추가).
- `test_no_widths_still_cleans_text`: 인라인 스타일 부여 상태에서도 빈 칸(`<td ...></td>`) 및 숫자 정렬(`align=right`) 정상 처리 검증.

### 시험 실행 결과
- `tests/test_excel_paste_target.py`: 23건 전체 통과 (0.073s)
- `tests/test_excel_html_shading.py`: 19건 전체 통과
- `tests/test_excel_table.py`: 17건 전체 통과
- `tests/test_kagent_app_service.py`: 통과 (8건 중 7건 통과, Windows 전용 파이프 시험 1건 스킵)
- `tests/test_kclip_service.py`: 22건 전체 통과
- 연관 모듈 67건 전체 통과.
- 한/글 COM 실기기 시험은 사용자 지시에 따라 실행하지 않음(총괄 수행 예정).
