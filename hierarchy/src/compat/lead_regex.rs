//! Start-anchored marker regexes with one trailing lookahead, run on the fast `regex` crate (HRUST1).
//!
//! The Python marker patterns end with a zero-width check of the next character, e.g. `(?=\s|$)`.
//! `fancy_regex` needs its backtracking VM for that and tries every start position even for `^`
//! patterns, which made search hierarchy slower than Python on long documents. A trailing lookahead
//! `X(?=L)` is rewritten to `X(?P<la>L)`: with leftmost-first (Perl priority) semantics both forms pick
//! the same branch of `X`, and the matched marker ends where the `la` group starts.

pub struct LeadRegex(regex::Regex);

impl LeadRegex {
    /// `pattern` must start with `^` and end with one lookahead `(?=L)` (optionally followed only by
    /// group-closing `)`), where `L` contains no parentheses. Panics otherwise (patterns are constants).
    pub fn new(pattern: &str) -> Self {
        assert!(pattern.starts_with('^'), "lead regex must be anchored: {pattern}");
        let open = pattern.rfind("(?=").expect("lead regex needs a trailing lookahead");
        let close = open + pattern[open..].find(')').expect("closed lookahead");
        let look = &pattern[open + 3..close];
        let rest = &pattern[close + 1..];
        assert!(!look.contains('(') && rest.chars().all(|c| c == ')'), "unsupported lead regex: {pattern}");
        let rewritten = format!("{}(?P<la>{}){}", &pattern[..open], look, rest);
        LeadRegex(regex::Regex::new(&rewritten).expect("valid lead regex"))
    }

    /// The marker matched at the start of `text` (lookahead excluded), like `fancy_regex::find`.
    pub fn find<'t>(&self, text: &'t str) -> Option<&'t str> {
        let captures = self.0.captures(text)?;
        Some(&text[..captures.name("la")?.start()])
    }

    pub fn is_match(&self, text: &str) -> bool { self.find(text).is_some() }
}

#[cfg(test)]
mod tests {
    use super::LeadRegex;

    /// Same result as the original fancy_regex pattern on tricky inputs.
    #[test]
    fn matches_fancy_regex_lookahead() {
        let patterns = [
            r"^(?:([0-9]+(?:[.．][0-9]+)+)[.．]?(?=\s|$))",
            r"^(?:(?:[0-9]+[.．)]|\([0-9]+\))(?=\s|$))",
            r"^(?:(?:[Ⅰ-Ⅿ]+[.．]?|(?:[IVX]|[IVXLCDM]{2,})[.．])(?=\s|$))",
            r"^(?:(?:\*\*?|※|[•·‧∙⋅ㆍ●]|\([가-하0-9]+\)[.．]?)(?=\s|$))",
            r"^(?:(?:[□ㅁ■]|\d{1,2}[.．]?)(?=\s|$))",
            r"^(?:(?:[Ⅰ-Ⅿ]+|[가-하])(?=\s|[.．]))",
            r"^\d{1,2}[.．](?=[^\d\s.．,，])",
            r"^[ \t\u{00a0}\u{3000}]*\d+\.(?=\s|$|「)",
        ];
        let texts = ["1.2 a", "1.2.", "1.2.3. x", "12. x", "12.x", "1.본문", "(3) x", "3) x", "Ⅱ. x", "Ⅱ x", "Ⅱx",
                     "IV. x", "I. x", "** x", "*x", "(가). x", "(가) x", "□ x", "12 x", "123 x", "가. x", "가나",
                     "가 x", "  3. 「", "3.「x", "1.,", "", " ", "ㅁ", "1"];
        for pattern in patterns {
            let fancy = fancy_regex::Regex::new(pattern).unwrap();
            let fast = LeadRegex::new(pattern);
            for text in texts {
                let expected = fancy.find(text).unwrap().filter(|m| m.start() == 0).map(|m| m.as_str());
                assert_eq!(fast.find(text), expected, "{pattern} {text:?}");
            }
        }
    }
}
