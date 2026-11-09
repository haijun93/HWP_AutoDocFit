pub fn normalize_leading_dot(text: &str) -> String {
    let start = text.len() - text.trim_start_matches(char::is_whitespace).len();
    let rest = &text[start..];
    if let Some(c) = rest.chars().next() {
        if "•·‧∙⋅ㆍ●".contains(c) && rest[c.len_utf8()..].chars().next().map(char::is_whitespace).unwrap_or(true) {
            return format!("{}•{}", &text[..start], &rest[c.len_utf8()..]);
        }
    }
    text.into()
}
