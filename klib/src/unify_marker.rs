//! Compatibility port of `docfit_core.style_unify.unify_marker`.

/// Return (group symbol, role, actual symbol), matching the Python helper.
pub fn unify_marker(text: &str) -> (String, String, String) {
    let stripped = text.trim_start_matches(|c| matches!(c, ' ' | '\t' | '\u{00a0}' | '\u{3000}'));
    let (mut marker, mut role) = hierarchy::compat::leading_marker::leading_marker(stripped);
    if !marker.is_empty() && matches!(role.as_str(), "소제목" | "중제목") {
        if let Some(tail) = stripped.strip_prefix(&marker) {
            let period = tail.chars().next();
            if marker.len() == 1 && marker.chars().all(|c| c.is_ascii_digit()) && period.is_some_and(|c| matches!(c, '.' | '．')) {
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
            let chars: Vec<char> = stripped.chars().collect();
            if chars.len() >= 3 && chars[0].is_ascii_digit() && chars[1].is_ascii_digit()
                && matches!(chars[2], '.' | '．') && (chars.len() == 3 || (!chars[3].is_ascii_digit() && !chars[3].is_whitespace() && !matches!(chars[3], '.' | '．' | ',' | '，')))
            {
                marker = chars[..3].iter().collect();
                role = "소제목".to_string();
            }
        }
    }
    if marker.is_empty() { return (String::new(), String::new(), String::new()); }
    let group = marker_class(&marker);
    (group, if role.is_empty() { "원문자".into() } else { role }, marker)
}

fn marker_class(marker: &str) -> String {
    if marker.chars().count() == 1 && marker.chars().next().is_some_and(|c| ('\u{F02B1}'..='\u{F02C4}').contains(&c)) {
        return "\u{F02B1}".into();
    }
    let chars: Vec<char> = marker.chars().collect();
    let numeric = chars.iter().all(|c| c.is_ascii_digit())
        || (chars.last().is_some_and(|c| matches!(c, '.' | '．')) && chars[..chars.len()-1].iter().all(|c| c.is_ascii_digit()))
        || (chars.len() >= 2 && chars[0] == '(' && chars.last() == Some(&')') && chars[1..chars.len()-1].iter().all(|c| c.is_ascii_digit()));
    if numeric && chars.iter().any(|c| c.is_ascii_digit()) {
        if chars.iter().any(|c| *c == ')') { return "1)".into(); }
        if chars.first() == Some(&'(') { return "(1)".into(); }
        return "1.".into();
    }
    let hangul = |c: char| ('가'..='하').contains(&c);
    if chars.first().is_some_and(|c| hangul(*c)) {
        if chars.contains(&')') { return "가)".into(); }
        return "가.".into();
    }
    if chars.first().is_some_and(|c| ('Ⅰ'..='Ⅹ').contains(c)) { return "Ⅰ".into(); }
    if chars.first().is_some_and(|c| ('①'..='⑳').contains(c)) { return "①".into(); }
    if chars.first().is_some_and(|c| ('㉠'..='㉻').contains(c)) { return "㉠".into(); }
    if chars.first().is_some_and(|c| ('❶'..='❿').contains(c)) { return "❶".into(); }
    if chars.first().is_some_and(|c| ('➊'..='➓').contains(c)) { return "➊".into(); }
    marker.to_string()
}
