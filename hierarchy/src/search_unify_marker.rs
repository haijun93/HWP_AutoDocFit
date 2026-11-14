//! Compatibility port of `docfit_core.style_unify.unify_marker`.

/// Return (group symbol, role, actual symbol), matching the Python helper.
pub fn unify_marker(text: &str) -> (String, String, String) {
    let stripped = text.trim_start_matches(|c| matches!(c, ' ' | '\t' | '\u{00a0}' | '\u{3000}'));
    let (mut marker, mut role) = crate::compat::leading_marker::leading_marker(stripped);
    if !marker.is_empty() && matches!(role.as_str(), "소제목" | "중제목") {
        if let Some(tail) = stripped.strip_prefix(&marker) {
            let period = tail.chars().next();
            if marker.len() == 1
                && marker.chars().all(|c| c.is_ascii_digit())
                && period.is_some_and(|c| matches!(c, '.' | '．'))
            {
                let after = &tail[period.unwrap().len_utf8()..];
                if let Some(next) = after.chars().next() {
                    if !next.is_whitespace() && !matches!(next, '.' | '．' | ',' | '，') {
                        marker.push(period.unwrap());
                    }
                }
            }
        }
    }
    if marker.is_empty() {
        if let Some(first) = stripped.chars().next() {
            if ('\u{F02B1}'..='\u{F02C4}').contains(&first) {
                marker = first.to_string();
                role = "중제목".to_string();
            } else if first == '・' {
                marker = "•".to_string();
                role = "부연설명".to_string();
            }
        }
        if marker.is_empty() {
            // Python style_unify._TIGHT_NUMBER: one or two digits, a period, then a non-space glyph.
            static TIGHT: std::sync::OnceLock<crate::compat::lead_regex::LeadRegex> = std::sync::OnceLock::new();
            let tight = TIGHT.get_or_init(|| {
                crate::compat::lead_regex::LeadRegex::new(r"^\d{1,2}[.．](?=[^\d\s.．,，])")
            });
            if let Some(m) = tight.find(crate::compat::marker_scheme::marker_head(stripped)) {
                marker = m.to_string();
                role = "소제목".to_string();
            }
        }
    }
    if marker.is_empty() {
        return (String::new(), String::new(), String::new());
    }
    let group = marker_class(&marker);
    (
        group,
        if role.is_empty() {
            "원문자".into()
        } else {
            role
        },
        marker,
    )
}

fn marker_class(marker: &str) -> String {
    if marker.chars().count() == 1
        && marker
            .chars()
            .next()
            .is_some_and(|c| ('\u{F02B1}'..='\u{F02C4}').contains(&c))
    {
        return "\u{F02B1}".into();
    }
    let chars: Vec<char> = marker.chars().collect();
    let numeric = chars.iter().all(|c| c.is_ascii_digit())
        || (chars.last().is_some_and(|c| matches!(c, '.' | '．'))
            && chars[..chars.len() - 1].iter().all(|c| c.is_ascii_digit()))
        || (chars.len() >= 2
            && chars[0] == '('
            && chars.last() == Some(&')')
            && chars[1..chars.len() - 1].iter().all(|c| c.is_ascii_digit()));
    if numeric && chars.iter().any(|c| c.is_ascii_digit()) {
        if chars.iter().any(|c| *c == ')') {
            return "1)".into();
        }
        if chars.first() == Some(&'(') {
            return "(1)".into();
        }
        return "1.".into();
    }
    let hangul = |c: char| ('가'..='하').contains(&c);
    if chars.first().is_some_and(|c| hangul(*c)) {
        if chars.contains(&')') {
            return "가)".into();
        }
        return "가.".into();
    }
    if chars.first().is_some_and(|c| ('Ⅰ'..='Ⅹ').contains(c)) {
        return "Ⅰ".into();
    }
    if chars.first().is_some_and(|c| ('①'..='⑳').contains(c)) {
        return "①".into();
    }
    if chars.first().is_some_and(|c| ('㉠'..='㉻').contains(c)) {
        return "㉠".into();
    }
    if chars.first().is_some_and(|c| ('❶'..='❿').contains(c)) {
        return "❶".into();
    }
    if chars.first().is_some_and(|c| ('➊'..='➓').contains(c)) {
        return "➊".into();
    }
    marker.to_string()
}

#[cfg(test)]
mod tight_number_tests {
    use super::unify_marker;
    // HRUST1 실문서 대조: 띄어쓰기 없는 한 자리 번호도 Python처럼 소제목 후보다.
    #[test]
    fn tight_numbers_match_python_style_unify() {
        let marker = |t: &str| unify_marker(t).2;
        assert_eq!(unify_marker("1.본문"), ("1.".into(), "소제목".into(), "1.".into()));
        assert_eq!(marker("12.본문"), "12.");
        assert_eq!(marker("3．항목"), "3．");
        assert_eq!(marker("123.본문"), "");
        assert_eq!(marker("1.5배"), "");
        assert_eq!(marker("1.,가"), "");
    }
}
