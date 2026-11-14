//! 파일명 정규화 모듈 (순수 Rust 구현).
//!
//! 확장자, 날짜(2026-11-14, 20261114, 26.11.14), 버전(v2, _2, (1), 최종, 수정본, 사본, copy),
//! 괄호 안 숫자 제거 및 구분자·공백 정리를 수행한다.

/// 파일 전체 경로 또는 파일명에서 확장자를 추출한다 (점 제외, 소문자).
pub fn extract_extension(path_or_name: &str) -> String {
    let name = path_or_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path_or_name);
    if let Some(pos) = name.rfind('.') {
        if pos > 0 && pos + 1 < name.len() {
            let ext = &name[pos + 1..];
            if ext.len() <= 10 && ext.chars().all(|c| c.is_ascii_alphanumeric()) && ext.chars().any(|c| c.is_ascii_alphabetic()) {
                return ext.to_ascii_lowercase();
            }
        }
    }
    String::new()
}

/// 파일명에서 확장자를 뗀 순수 파일명을 반환한다.
pub fn strip_extension(path_or_name: &str) -> &str {
    let name = path_or_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path_or_name);
    if let Some(pos) = name.rfind('.') {
        if pos > 0 && pos + 1 < name.len() {
            let ext = &name[pos + 1..];
            if ext.len() <= 10 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
                return &name[..pos];
            }
        }
    }
    name
}

/// 문자열이 4자리 연도 기반 날짜인지 검사한다 (예: 2026-11-14, 2026.11.14, 2026_11_14, 20261114).
fn is_date_pattern_4digit(s: &str) -> bool {
    // 2026-11-14 or 2026.11.14 or 2026_11_14
    if s.len() == 10 {
        let b = s.as_bytes();
        let sep1 = b[4];
        let sep2 = b[7];
        if (sep1 == b'-' || sep1 == b'.' || sep1 == b'_') && (sep2 == sep1) {
            let y_digits = s[0..4].chars().all(|c| c.is_ascii_digit());
            let m_digits = s[5..7].chars().all(|c| c.is_ascii_digit());
            let d_digits = s[8..10].chars().all(|c| c.is_ascii_digit());
            if y_digits && m_digits && d_digits {
                return true;
            }
        }
    }
    // 20261114 (8자리 연속 숫자)
    if s.len() == 8 && s.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(y) = s[0..4].parse::<i32>() {
            if (1970..=2100).contains(&y) {
                if let (Ok(m), Ok(d)) = (s[4..6].parse::<u32>(), s[6..8].parse::<u32>()) {
                    if (1..=12).contains(&m) && (1..=31).contains(&d) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// 문자열이 2자리 연도 기반 날짜인지 검사한다 (예: 26.11.14, 26-11-14, 26_11_14).
fn is_date_pattern_2digit(s: &str) -> bool {
    if s.len() == 8 {
        let b = s.as_bytes();
        let sep1 = b[2];
        let sep2 = b[5];
        if (sep1 == b'.' || sep1 == b'-' || sep1 == b'_') && (sep2 == sep1) {
            let y_digits = s[0..2].chars().all(|c| c.is_ascii_digit());
            let m_digits = s[3..5].chars().all(|c| c.is_ascii_digit());
            let d_digits = s[6..8].chars().all(|c| c.is_ascii_digit());
            if y_digits && m_digits && d_digits {
                return true;
            }
        }
    }
    false
}

/// 토큰이 버전 표기인지 검사한다.
/// v2, V1, ver3, ver.2, v.1, v1.0, _2, -2, (1), [1], 최종, 수정본, 수정, 사본, copy 등
fn is_version_or_modifier(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    let trimmed = lower
        .trim_matches(|c: char| c == '(' || c == ')' || c == '[' || c == ']' || c == '_' || c == '-');

    if trimmed.is_empty() {
        return false;
    }

    // 키워드
    match trimmed {
        "최종" | "수정본" | "수정" | "사본" | "copy" | "초안" | "배포용" | "제출용" => return true,
        _ => {}
    }

    // 괄호 안 숫자 또는 단독 숫자: (1), [2], _2, -1 등
    if trimmed.chars().all(|c| c.is_ascii_digit()) {
        // 원래 토큰에 괄호나 언더스코어, 하이픈이 감싸져 있었거나 숫자인 경우
        let wrapped = token.starts_with('(') || token.starts_with('[') || token.starts_with('_') || token.starts_with('-');
        if wrapped || token.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
    }

    // v2, ver3, v1.0 등
    if let Some(rest) = trimmed.strip_prefix("ver.") {
        if rest.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return true;
        }
    }
    if let Some(rest) = trimmed.strip_prefix("ver") {
        if rest.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return true;
        }
    }
    if let Some(rest) = trimmed.strip_prefix("v.") {
        if rest.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return true;
        }
    }
    if let Some(rest) = trimmed.strip_prefix('v') {
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return true;
        }
    }

    false
}

/// 파일명 정규화: 확장자·날짜·버전·괄호 안 숫자 제거, 구분자 및 공백 정리.
pub fn normalize_name(path_or_name: &str) -> String {
    let base = strip_extension(path_or_name);

    // 1단계: 날짜 패턴을 먼저 찾아 공백으로 치환
    // (긴 날짜 패턴이 구분자로 인해 쪼개지기 전에 통째로 제거)
    let mut cleaned = String::with_capacity(base.len());
    let chars: Vec<char> = base.chars().collect();
    let n = chars.len();
    let mut i = 0;

    while i < n {
        // 10글자 (2026-11-14 등)
        if i + 10 <= n {
            let slice: String = chars[i..i + 10].iter().collect();
            if is_date_pattern_4digit(&slice) {
                cleaned.push(' ');
                i += 10;
                continue;
            }
        }
        // 8글자 (20261114 또는 26.11.14)
        if i + 8 <= n {
            let slice: String = chars[i..i + 8].iter().collect();
            if is_date_pattern_4digit(&slice) || is_date_pattern_2digit(&slice) {
                cleaned.push(' ');
                i += 8;
                continue;
            }
        }
        cleaned.push(chars[i]);
        i += 1;
    }

    // 2단계: 구분자(언더스코어, 쉼표, 마침표 등)를 기준으로 단어 분리
    // 단, 대괄호 [] 및 소괄호 ()는 내부 확인을 위해 특별 처리
    let mut tokens: Vec<String> = Vec::new();
    let mut cur = String::new();

    for c in cleaned.chars() {
        if c == '_' || c == '-' || c == '.' || c == ',' || c == '/' || c == '\\' || c == '|' {
            if !cur.is_empty() {
                tokens.push(cur.clone());
                cur.clear();
            }
            // 구분자 자체도 버전 번호 prefix (_2, -1) 처리를 위해 유지하거나 표시 가능
        } else if c.is_whitespace() {
            if !cur.is_empty() {
                tokens.push(cur.clone());
                cur.clear();
            }
        } else if c == '(' || c == '[' {
            if !cur.is_empty() {
                tokens.push(cur.clone());
                cur.clear();
            }
            cur.push(c);
        } else if c == ')' || c == ']' {
            cur.push(c);
            tokens.push(cur.clone());
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }

    // 3단계: 각 토큰에 대해 날짜, 버전, 괄호 안 숫자, 수정본/사본 등 제거
    let mut kept: Vec<String> = Vec::new();
    for token in tokens {
        let t = token.trim();
        if t.is_empty() {
            continue;
        }

        // 날짜 여부 확인
        if is_date_pattern_4digit(t) || is_date_pattern_2digit(t) {
            continue;
        }

        // 괄호 감싸진 것: (1), [수정본], (최종) 등
        if (t.starts_with('(') && t.ends_with(')')) || (t.starts_with('[') && t.ends_with(']')) {
            let inner = &t[1..t.len() - 1].trim();
            if inner.is_empty() || inner.chars().all(|c| c.is_ascii_digit()) || is_version_or_modifier(inner) {
                continue;
            }
            // 대괄호로 둘러싸인 태그 (예: [기획], [서식]) 등은 내부 단어로 유지
            kept.push(inner.to_string());
            continue;
        }

        // 버전 및 수정자 확인
        if is_version_or_modifier(t) {
            continue;
        }

        kept.push(t.to_string());
    }

    // 4단계: 단어들을 단일 공백으로 결합 및 잔여 기호 정리
    let result = kept.join(" ");
    let final_clean = result
        .trim_matches(|c: char| c.is_whitespace() || c == '_' || c == '-' || c == '.')
        .to_string();

    // 혹시 모든 단어가 제거되어 비어버린 경우 원래 base 반환
    if final_clean.is_empty() {
        base.trim().to_string()
    } else {
        final_clean
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_name_10_cases() {
        // 요구사항: 정규화 사례 10개 시험
        let cases = [
            // 1. 날짜(YYYY-MM-DD) + 버전(v2) + 확장자(.hwpx)
            ("2026-11-14 업무 계획서 v2.hwpx", "업무 계획서"),
            // 2. 날짜(YYYYMMDD) + 최종 + 언더스코어 구분자 + 확장자(.hwp)
            ("업무보고서_20261114_최종.hwp", "업무보고서"),
            // 3. 괄호 태그[기획] + 날짜(YY.MM.DD) + 괄호 숫자(1) + 수정본
            ("[기획] 26.11.14_예산안(1)_수정본.docx", "기획 예산안"),
            // 4. 언더스코어 숫자(_2) 버전 표기
            ("주간업무보고_2.pdf", "주간업무보고"),
            // 5. 괄호 안 copy 키워드
            ("회의록 (copy).hwpx", "회의록"),
            // 6. 언더스코어 구분자 + v1.0 + 사본
            ("사업_추진_실적_v1.0_사본.xlsx", "사업 추진 실적"),
            // 7. 날짜(YYYY_MM_DD) + ver2
            ("2026_11_14_청사관리계획서_ver2.hwp", "청사관리계획서"),
            // 8. 괄호 안 숫자(2) + [수정본]
            ("민원응대_매뉴얼(2)_[수정본].hwpx", "민원응대 매뉴얼"),
            // 9. 날짜(YY-MM-DD) + (최종)
            ("26-11-14 감사결과_보고서(최종).pdf", "감사결과 보고서"),
            // 10. 공백 언더스코어 숫자(_3) + (사본)
            ("정보화사업 심의자료 _3 (사본).hwp", "정보화사업 심의자료"),
        ];

        for (input, expected) in cases {
            let normalized = normalize_name(input);
            assert_eq!(
                normalized, expected,
                "Case failed for input: '{}'. Got: '{}', Expected: '{}'",
                input, normalized, expected
            );
        }
    }

    #[test]
    fn test_extract_extension() {
        assert_eq!(extract_extension("doc.hwp"), "hwp");
        assert_eq!(extract_extension("path/to/file.HWPX"), "hwpx");
        assert_eq!(extract_extension("no_ext"), "");
        assert_eq!(extract_extension("2026.11.14"), "");
    }
}
