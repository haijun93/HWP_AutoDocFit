pub fn normalize_leading_dot(text: &str) -> String {
    let text = text;
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if !c.is_whitespace() { start = i; break; }
        start = i + c.len_utf8();
    }
    let rest = &text[start..];
    if let Some(c) = rest.chars().next() {
        if "•·‧∙⋅ㆍ●".contains(c)
            && rest[c.len_utf8()..].chars().next().map(char::is_whitespace).unwrap_or(true)
        {
            return format!("{}•{}", &text[..start], &rest[c.len_utf8()..]);
        }
    }
    text.to_string()
}
