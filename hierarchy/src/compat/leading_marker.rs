use regex::Regex;
use std::sync::OnceLock;
use super::canonical_marker::canonical_marker;
fn marker_patterns() -> &'static [( &'static str, &'static str)] { &[
    ("장", r"(?:제\s?\d{1,2}\s?[장편부]|\d{1,2}\s?장|[Cc]hapter\s?\d{1,2})(?=\s|$|[.．:：])"),
    ("중제목", r"(?:[Ⅰ-Ⅿ]+|[가-하])(?=\s|[.．])"),
    ("소제목", r"(?:[□ㅁ■]|\d{1,2}[.．]?)(?=\s|$)"),
    ("본문", r"(?:[ㅇ○◦☞]|[가-하])(?=\s|$)"),
    ("내용", r"(?:-|\d+\))(?=\s|$)"),
    ("부연설명", r"(?:\*\*?|※|[•·‧∙⋅ㆍ●]|\([가-하0-9]+\)[.．]?)(?=\s|$)"),
] }
pub fn leading_marker(text: &str) -> (String, String) {
    static CIRCLED: OnceLock<Regex> = OnceLock::new();
    static ROMAN: OnceLock<Regex> = OnceLock::new();
    let stripped = text.trim_start();
    let circled = CIRCLED.get_or_init(|| Regex::new(r"^[①-⑳㉠-㉻❶-❿➊-➓](?=\s|$)").unwrap());
    if let Some(m) = ROMAN.get_or_init(|| Regex::new(r"^(?:[IVX]|[IVXLCDM]{2,})[.．](?=\s|$)").unwrap()).find(stripped) { return (m.as_str().into(), "중제목".into()); }
    if let Some(c) = stripped.chars().next() {
        if "❶❷❸❹❺❻❼❽❾❿".contains(c) && stripped[c.len_utf8()..].chars().next().map(char::is_whitespace).unwrap_or(true) { return ("-".into(), "내용".into()); }
    }
    if let Some(m) = circled.find(stripped) { return (m.as_str().into(), String::new()); }
    for (role, pattern) in marker_patterns() {
        if let Some(m) = Regex::new(&format!("^{pattern}")).unwrap().find(stripped) { return (canonical_marker(m.as_str()), (*role).into()); }
    }
    (String::new(), String::new())
}
