/// Canonicalize list item marker, collapsing number variations,
/// matching style_inventory._marker_class.
///
/// Python reference:
/// def _marker_class(marker: str) -> str:
///     if not marker:
///         return "(없음)"
///     if re.match(r"[①-⑳]", marker):
///         return "①"
///     if re.match(r"[㉠-㉻]", marker):
///         return "㉠"
///     marker = re.sub(r"\d+", "1", marker)
///     return re.sub(r"^[가-하]", "가", marker)
pub fn marker_class(marker: &str) -> String {
    if marker.is_empty() {
        return "(없음)".to_string();
    }

    let first = marker.chars().next().unwrap();
    if ('①'..='⑳').contains(&first) {
        return "①".to_string();
    }
    if ('㉠'..='㉻').contains(&first) {
        return "㉠".to_string();
    }

    let mut collapsed = String::with_capacity(marker.len());
    let mut in_digits = false;
    for c in marker.chars() {
        if c.is_ascii_digit() {
            if !in_digits {
                collapsed.push('1');
                in_digits = true;
            }
        } else {
            in_digits = false;
            collapsed.push(c);
        }
    }

    let mut chars = collapsed.chars();
    if let Some(first_char) = chars.next() {
        if ('가'..='하').contains(&first_char) {
            let mut out = String::with_capacity(collapsed.len());
            out.push('가');
            out.extend(chars);
            return out;
        }
    }

    collapsed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_empty_to_none() {
        assert_eq!(marker_class(""), "(없음)");
    }

    #[test]
    fn collapses_circled_numbers() {
        assert_eq!(marker_class("①"), "①");
        assert_eq!(marker_class("②"), "①");
        assert_eq!(marker_class("⑳"), "①");
        assert_eq!(marker_class("①-1"), "①");
    }

    #[test]
    fn collapses_circled_hangul() {
        assert_eq!(marker_class("㉠"), "㉠");
        assert_eq!(marker_class("㉡"), "㉠");
        assert_eq!(marker_class("㉻"), "㉠");
    }

    #[test]
    fn collapses_digits_and_hangul() {
        assert_eq!(marker_class("1."), "1.");
        assert_eq!(marker_class("12."), "1.");
        assert_eq!(marker_class("1.2"), "1.1");
        assert_eq!(marker_class("(1)"), "(1)");
        assert_eq!(marker_class("(15)"), "(1)");
        assert_eq!(marker_class("가."), "가.");
        assert_eq!(marker_class("나."), "가.");
        assert_eq!(marker_class("하."), "가.");
        assert_eq!(marker_class("가) "), "가) ");
        assert_eq!(marker_class("나) "), "가) ");
        assert_eq!(marker_class("(가)"), "(가)");
        assert_eq!(marker_class("• "), "• ");
    }
}
