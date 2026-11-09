# HIER7: detect_scheme Python/Rust 7건 차이 분석 및 일치화

## 1. 기존 비교 도구 측정 결과 (7건 목록)

도구: `tools/hier_fn_regress.py detect_scheme --dump --no-build`
대상: 29개 코퍼스 문서 (총 549문단) 중 `hierarchy/tests/golden.json` 1개 문서에서 7건 발생.

| 번호 | 입력 (문서 / 문단 텍스트) | 필드 (JSON Path) | Python 값 | Rust 값 | 비고 / 원인 분석 |
|:---:|---|---|:---:|:---:|---|
| 1 | `hierarchy/tests/golden.json`<br>`"2026. 11. 04. 시행계획 안내"` | `$.family_levels.1111.` | `<missing>` | `3` | `classify_marker`의 다자리 숫자 개별 치환 버그로 `"1111."` 오분류 |
| 2 | `hierarchy/tests/golden.json`<br>`"2026. 11. 04. 시행계획 안내"` | `$.summary.1111.` | `<missing>` | `"-"` | `"1111."` 계열 등록에 따른 summary 자동 파생 |
| 3 | `hierarchy/tests/golden.json`<br>`"1. 첫째"` | `$.family_levels.1.` | `3` | `5` | Python은 문두 연도(2026.)에서 `"1."`이 level 3에 등록되나 Rust는 `"1111."`(3), `"■"`(4)에 밀려 5 할당 |
| 4 | `hierarchy/tests/golden.json`<br>`"(1) 괄호 번호"` | `$.family_levels.(1)` | `4` | `6` | 상위 계열 레벨 오프셋(+2) 전파 |
| 5 | `hierarchy/tests/golden.json`<br>`"1) 괄호 없는 번호"` | `$.family_levels.1)` | `5` | `7` | 상위 계열 레벨 오프셋(+2) 전파 |
| 6 | `hierarchy/tests/golden.json`<br>`"① 원문자 항목"` | `$.family_levels.①` | `6` | `8` | 상위 계열 레벨 오프셋(+2) 전파 |
| 7 | `hierarchy/tests/golden.json`<br>`"㉮ 한글 원문자"` | `$.family_levels.㉮` | `7` | `9` | 상위 계열 레벨 오프셋(+2) 전파 |

---

## 2. 원인 분석 및 해결 계획 (Divide & Rule)

1. **원인 1 (`classify_marker` 치환 로직 불일치)**:
   - Python: `re.sub(r"[0-9]+|[가나다라마바사아자차카타파하]|[A-Za-z]", representative, token)`
   - Rust: `for c in token.chars() { if c.is_ascii_digit() || HANGUL.contains(c) { out = out.replace(c, rep); } }`
   - 문제: 다자리 숫자(예: 2026, 12, 10)가 `1111.`, `11.` 등으로 잘못 치환됨. `regex::Regex` 치환으로 Python과 1:1 일치화.
2. **원인 2 (`detect_scheme` 임의 필터 및 공식 규정 불일치 제거)**:
   - Rust에 임의로 추가된 `family == "1." && count >= 4 { continue; }` 필터 제거 (Python 원본에 없음).
   - `OFFICIAL` 정의에 `("-", "-")` 추가 (`_OFFICIAL_LEVEL`과 일치).
   - `order` 배열을 공문서 규정 및 Python `_OFFICIAL_ORDER`와 동일하게 `["1.", "가.", "1)", "가)", "(1)", "(가)", "①", "㉮"]`로 정렬.
3. **검증**:
   - `cargo test --manifest-path hierarchy/Cargo.toml`
   - `python tools/hier_fn_regress.py detect_scheme --no-build` (차이 0)
   - `python tools/hier_fn_regress.py analyze_hierarchy --no-build` (차이 0)
   - `python tools/hier_regress.py --no-build` (549문단 차이 0)
