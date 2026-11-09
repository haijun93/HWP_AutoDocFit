# HIER7: detect_scheme Python/Rust 7건 차이 0 달성 최종 보고서

## 1. 초기 7건 차이 현황 및 원인 분석

도구: `tools/hier_fn_regress.py detect_scheme --dump --no-build`
대상: 29개 코퍼스 문서 (549개 문단) 중 `hierarchy/tests/golden.json`에서 7건 불일치 발생.

| 번호 | 입력 (문서 / 문단 텍스트) | 필드 (JSON Path) | Python 값 | 초기 Rust 값 | 수정 후 Rust 값 | 비고 / 원인 분석 |
|:---:|---|---|:---:|:---:|:---:|---|
| 1 | `hierarchy/tests/golden.json`<br>`"2026. 11. 04. 시행계획 안내"` | `$.family_levels.1111.` | `<missing>` | `3` | `<missing>` | `classify_marker`의 다자리 숫자 개별 치환 버그로 `"1111."` 오분류되던 문제 해결 |
| 2 | `hierarchy/tests/golden.json`<br>`"2026. 11. 04. 시행계획 안내"` | `$.summary.1111.` | `<missing>` | `"-"` | `<missing>` | `"1111."` 제거에 따른 summary 파생 차이 해소 |
| 3 | `hierarchy/tests/golden.json`<br>`"1. 첫째"` | `$.family_levels.1.` | `3` | `5` | `3` | 임의 4자리 필터 제거 및 문두 `"1."`의 level 3 등록 정상화 |
| 4 | `hierarchy/tests/golden.json`<br>`"(1) 괄호 번호"` | `$.family_levels.(1)` | `4` | `6` | `4` | 상위 계열 정상화로 +2 오프셋 해소 |
| 5 | `hierarchy/tests/golden.json`<br>`"1) 괄호 없는 번호"` | `$.family_levels.1)` | `5` | `7` | `5` | 상위 계열 정상화로 +2 오프셋 해소 |
| 6 | `hierarchy/tests/golden.json`<br>`"① 원문자 항목"` | `$.family_levels.①` | `6` | `8` | `6` | 상위 계열 정상화 및 `order` 배열(공문서 7~8단계) 일치화 |
| 7 | `hierarchy/tests/golden.json`<br>`"㉮ 한글 원문자"` | `$.family_levels.㉮` | `7` | `9` | `7` | 상위 계열 정상화 및 `order` 배열(공문서 7~8단계) 일치화 |

---

## 2. 단위별 수정 내역 (Divide & Rule)

1. **단위 1**: 7건 목록 표 및 원인 분석 문서화 (`reports/HIER7.final.md`)
2. **단위 2-A (`classify_marker` 수정)**:
   - 파일: `hierarchy/src/compat/marker_scheme.rs`
   - 내용: `re.sub(r"[0-9]+|[가나다라마바사아자차카타파하]|[A-Za-z]", representative, token)`를 문자 단위 순회 치환 대신 `sub_marker_re` (`regex::Regex`) 일괄 치환으로 변경.
   - 결과: 다자리 숫자(예: 2026, 12, 10)가 `1111.` 등으로 쪼개지지 않고 정상적인 `"1."` 계열로 식별됨.
3. **단위 2-B (`detect_scheme` 수정 및 공문서 규정 1:1 일치화)**:
   - 파일: `hierarchy/src/compat/marker_scheme.rs`
   - 내용:
     - `OFFICIAL` 상수에 `("-", "-")` 추가 (`_OFFICIAL_LEVEL`과 1:1 일치).
     - 임의로 추가되었던 `family == "1." && count >= 4 { continue; }` 필터 제거 (Python 원본에 없음).
     - `order` 배열을 정부 공문서 규격 및 Python `_OFFICIAL_ORDER`와 동일하게 `["1.", "가.", "1)", "가)", "(1)", "(가)", "①", "㉮"]`로 수정.
     - 동일 들여쓰기 rank 비교 pop 로직을 Python `_OFFICIAL_ORDER.get(stack[-1][0], -1)`과 동일하게 처리.
   - 단위 테스트 추가: `scheme_matches_python_golden_differences_zero`

---

## 3. 검증 결과

1. **`detect_scheme` 회귀 검증**:
   - 명령: `python tools/hier_fn_regress.py detect_scheme --dump --no-build`
   - 결과: `detect_scheme documents=29 differences=0` (29개 코퍼스 전체 차이 0)
2. **`analyze_hierarchy` 회귀 검증 (필수 유지)**:
   - 명령: `python tools/hier_fn_regress.py analyze_hierarchy --no-build`
   - 결과: `analyze_hierarchy: cases=22 differences=0`
3. **코퍼스 549문단 전체 회귀 검증**:
   - 명령: `python tools/hier_regress.py --no-build`
   - 결과: `documents=29 paragraphs=549 differences=0 elapsed_seconds=1.704`
4. **Rust 단위/통합 테스트**:
   - 명령: `cargo test --manifest-path hierarchy/Cargo.toml`
   - 결과: 41개 테스트 전체 통과 (lib 38개, bin_cli 3개)

---

## 4. 수정 결과

1. **불일치 0건 달성**:
   - `detect_scheme`: 29개 코퍼스 문서 549문단 전체에서 Python 대비 차이 7건 -> **0건** 완전 해소.
   - `analyze_hierarchy`: 22개 케이스 차이 **0건** 유지.
   - `hier_regress`: 29개 문서 549문단 차이 **0건** 유지.
2. **원인별 조치 완료**:
   - `classify_marker`: `regex::Regex` 기반 일괄 치환(`sub_marker_re`)으로 다자리 연도("2026.")가 "1111."로 오분류되던 문제를 해결하여 Python과 1:1 일치.
   - `detect_scheme`: `OFFICIAL`에 `("-", "-")` 반영, 임의 4자리 숫자 건너뛰기 제거, 공문서 표준 순서(`order`) 일치화 완료.
3. **테스트 통과**:
   - `cargo test --manifest-path hierarchy/Cargo.toml` 41개 테스트 전체 통과 (unit 38, integration 3).

