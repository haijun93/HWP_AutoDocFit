//! Opt-in related queries: literal terms within a paragraph, document intersection.
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelatedGroup {
    pub query: String,
    pub terms: Vec<String>,
}

pub fn parse_related_query(input: &str) -> Result<Vec<RelatedGroup>, String> {
    if input.chars().count() > 200 {
        return Err("검색어는 200자 이내로 입력해 주세요.".into());
    }
    let mut groups = Vec::new();
    let mut terms = Vec::new();
    let mut token = String::new();
    let mut label = String::new();
    let mut quote = None;
    let mut escaped = false;
    fn flush(token: &mut String, terms: &mut Vec<String>) {
        let word = std::mem::take(token);
        if !word.is_empty() && !terms.contains(&word) {
            terms.push(word);
        }
    }
    fn group(
        groups: &mut Vec<RelatedGroup>,
        terms: &mut Vec<String>,
        token: &mut String,
        label: &mut String,
    ) {
        flush(token, terms);
        if !terms.is_empty() {
            groups.push(RelatedGroup {
                query: label.trim().into(),
                terms: std::mem::take(terms),
            });
        }
        label.clear();
    }
    for c in input.nfc() {
        if escaped {
            if c != '\\' && Some(c) != quote {
                token.push('\\');
            }
            token.push(c);
            label.push(c);
            escaped = false;
        } else if quote.is_some() && c == '\\' {
            label.push(c);
            escaped = true;
        } else if quote == Some(c) {
            label.push(c);
            quote = None;
        } else if quote.is_none() && (c == '"' || c == '“') {
            label.push(c);
            quote = Some(if c == '“' { '”' } else { '"' });
        } else if quote.is_none() && c == ';' {
            group(&mut groups, &mut terms, &mut token, &mut label);
        } else {
            label.push(c);
            if quote.is_none() && c.is_whitespace() {
                flush(&mut token, &mut terms);
            } else {
                token.push(c);
            }
        }
    }
    if quote.is_some() || escaped {
        return Err("검색어의 따옴표를 닫아 주세요.".into());
    }
    group(&mut groups, &mut terms, &mut token, &mut label);
    Ok(groups)
}

pub fn related_normalized(text: &str) -> String {
    text.nfc().collect::<String>().to_lowercase()
}

pub fn related_matches(text: &str, group: &RelatedGroup) -> bool {
    text.split(|c: char| matches!(c, '.' | '!' | '?' | '。' | '！' | '？' | '\r' | '\n'))
        .any(|sentence| {
            let folded = related_normalized(sentence);
            group
                .terms
                .iter()
                .all(|t| folded.contains(&related_normalized(t)))
        })
}

/// File names are one search unit, including their extension punctuation.
pub fn related_matches_single(text: &str, group: &RelatedGroup) -> bool {
    let folded = related_normalized(text);
    group
        .terms
        .iter()
        .all(|t| folded.contains(&related_normalized(t)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grammar() {
        let g = parse_related_query(" ; 2026;; 공사  계약 ; \"a;b\" R&D; \"\";").unwrap();
        assert_eq!(g.len(), 3);
        assert_eq!(g[1].terms, ["공사", "계약"]);
        assert_eq!(g[2].terms, ["a;b", "R&D"]);
        assert_eq!(
            parse_related_query("“공사; 계약”").unwrap()[0].terms,
            ["공사; 계약"]
        );
        assert_eq!(
            parse_related_query(r#""a\";b"; c"#).unwrap()[0].terms,
            ["a\";b"]
        );
        assert!(parse_related_query("\"미완료").is_err());
        assert!(parse_related_query("; ;").unwrap().is_empty());
    }
    #[test]
    fn literal_and() {
        let group = &parse_related_query("공사 계약").unwrap()[0];
        assert!(related_matches("계약 검토 후 공사를 추진", group));
        assert!(!related_matches("공사", group));
        assert!(!related_matches("공사 일정. 계약 검토", group));
        assert!(related_matches_single("공사 계약.pdf", group));
        assert!(!related_matches(
            "RD",
            &parse_related_query("R&D").unwrap()[0]
        ));
    }
}
