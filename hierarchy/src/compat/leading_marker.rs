use std::sync::OnceLock;

use super::{canonical_marker::canonical_marker, lead_regex::LeadRegex as Regex, marker_scheme::{marker_head, MarkerScheme}};

// Keep these expressions byte-for-byte equivalent to docfit_core/style_hierarchy.py.
const MARKERS: &[(&str, &str)] = &[
    ("장", r"(?:제\s?\d{1,2}\s?[장편부]|\d{1,2}\s?장|[Cc]hapter\s?\d{1,2})(?=\s|$|[.．:：])"),
    ("중제목", r"(?:[Ⅰ-Ⅿ]+|[가-하])(?=\s|[.．])"),
    ("소제목", r"(?:[□ㅁ■]|\d{1,2}[.．]?)(?=\s|$)"),
    ("본문", r"(?:[ㅇ○◦☞]|[가-하]\))(?=\s|$)"),
    ("내용", r"(?:-|\d+\))(?=\s|$)"),
    ("부연설명", r"(?:\*\*?|※|[•·‧∙⋅ㆍ●]|\([가-하0-9]+\)[.．]?)(?=\s|$)"),
];

static CIRCLED_MARKER: OnceLock<Regex> = OnceLock::new();
static ROMAN_MARKER: OnceLock<Regex> = OnceLock::new();
static MARKER_REGEXES: OnceLock<Vec<Regex>> = OnceLock::new();

fn is_match_at_start(regex: &Regex, text: &str) -> Option<String> {
    regex.find(marker_head(text)).map(str::to_owned)
}

pub fn leading_marker(text: &str) -> (String, String) { leading_marker_with_scheme(text, None) }

pub fn leading_marker_with_scheme(text: &str, scheme: Option<&MarkerScheme>) -> (String, String) {
    // Python str.lstrip() uses the Unicode White_Space set, as does char::is_whitespace.
    let stripped = text.trim_start_matches(char::is_whitespace);

    if let Some((marker, _end)) = scheme.and_then(|s| s.canonical(text)) {
        let marker_role = if marker == "□" { "소제목" } else if marker == "ㅇ" { "본문" } else { "내용" };
        return (marker, marker_role.into());
    }

    let roman = ROMAN_MARKER.get_or_init(|| {
        Regex::new(r"^(?:(?:[IVX]|[IVXLCDM]{2,})[.．](?=\s|$))")
    });
    if let Some(marker) = is_match_at_start(roman, stripped) {
        return (marker, "중제목".into());
    }

    if let Some(c) = stripped.chars().next() {
        if "❶❷❸❹❺❻❼❽❾❿".contains(c)
            && stripped[c.len_utf8()..]
                .chars()
                .next()
                .map(char::is_whitespace)
                .unwrap_or(true)
        {
            return ("-".into(), "내용".into());
        }
    }

    let circled = CIRCLED_MARKER.get_or_init(|| {
        Regex::new(r"^(?:[①-⑳㉠-㉻❶-❿➊-➓](?=\s|$))")
    });
    if let Some(marker) = is_match_at_start(circled, stripped) {
        return (marker, String::new());
    }

    let regexes = MARKER_REGEXES.get_or_init(|| {
        MARKERS
            .iter()
            .map(|(_, pattern)| Regex::new(&format!("^(?:{pattern})")))
            .collect()
    });
    for ((role, _), regex) in MARKERS.iter().zip(regexes) {
        if let Some(marker) = is_match_at_start(regex, stripped) {
            return (canonical_marker(&marker), (*role).into());
        }
    }
    (String::new(), String::new())
}
