//! Port of `docfit_core.marker_scheme` for the hierarchy compatibility path.
use fancy_regex::Regex;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;
use super::lead_regex::LeadRegex;

const LEVELS: [&str; 3] = ["□", "ㅇ", "-"];
const OFFICIAL: [(&str, &str); 5] = [("□", "□"), ("ㅁ", "□"), ("ㅇ", "ㅇ"), ("○", "ㅇ"), ("-", "-")];
const NOTES: [&str; 3] = ["*", "**", "※"];
const LEGACY: &str = "◦☞•·∙‧⋅ㆍ";
const HANGUL: &str = "가나다라마바사아자차카타파하";
const CIRCLED: &str = "①②③④⑤⑥⑦⑧⑨⑩⑪⑫⑬⑭⑮⑯⑰⑱⑲⑳㉠㉡㉢㉣㉤㉥㉦㉧㉨㉩㉪㉫㉬㉭㉮㉯㉰㉱㉲㉳㉴㉵㉶㉷㉸㉹㉺㉻";
const BLACK_CIRCLED: &str = "❶❷❸❹❺❻❼❽❾❿";

fn re(pattern: &str) -> Regex { Regex::new(pattern).expect("valid marker scheme regex") }

/// Leading part of a paragraph that marker regexes can ever match (HRUST1 speed fix).
/// fancy_regex tries every start position even for `^` patterns, so a 1,000-character paragraph
/// cost ~0.3 ms per regex. Every marker token starts at the first non-space character, has its
/// optional inner spaces within the first few characters, and never crosses a later space; keeping
/// the first 64 characters, then up to the next whitespace run and one more character, gives the
/// same matches (including `(?=\s|$)` and `\s*[:：]`) as the whole text.
pub fn marker_head(text: &str) -> &str {
    const KEEP: usize = 64;
    let lead = text.len() - text.trim_start_matches(char::is_whitespace).len();
    let body = &text[lead..];
    let Some((start, _)) = body.char_indices().nth(KEEP) else { return text };
    let Some(space) = body[start..].find(char::is_whitespace).map(|i| start + i) else { return text };
    let rest = &body[space..];
    let after_space = space + (rest.len() - rest.trim_start_matches(char::is_whitespace).len());
    let end = body[after_space..].chars().next().map_or(after_space, |c| after_space + c.len_utf8());
    &text[..lead + end]
}
fn label_re() -> &'static Regex { static R: OnceLock<Regex> = OnceLock::new(); R.get_or_init(|| re(r"^\s*(?:제목|개요|요약|부제|중제목)\s*[:：]")) }
fn multi_re() -> &'static LeadRegex { static R: OnceLock<LeadRegex> = OnceLock::new(); R.get_or_init(|| LeadRegex::new(r"^(?:([0-9]+(?:[.．][0-9]+)+)[.．]?(?=\s|$))")) }
fn hyphen_re() -> &'static LeadRegex { static R: OnceLock<LeadRegex> = OnceLock::new(); R.get_or_init(|| LeadRegex::new(r"^(?:([0-9]+(?:-[0-9]+)+)[.．]?(?=\s|$))")) }
fn number_re() -> &'static LeadRegex { static R: OnceLock<LeadRegex> = OnceLock::new(); R.get_or_init(|| LeadRegex::new(r"^(?:(?:[0-9]+[.．)]|\([0-9]+\))(?=\s|$))")) }
fn korean_re() -> &'static LeadRegex { static R: OnceLock<LeadRegex> = OnceLock::new(); R.get_or_init(|| LeadRegex::new(r"^(?:(?:[가나다라마바사아자차카타파하][.．)]|\([가나다라마바사아자차카타파하]\))(?=\s|$))")) }
fn roman_re() -> &'static LeadRegex { static R: OnceLock<LeadRegex> = OnceLock::new(); R.get_or_init(|| LeadRegex::new(r"^(?:(?:[Ⅰ-Ⅿ]+[.．]?|(?:[IVX]|[IVXLCDM]{2,})[.．])(?=\s|$))")) }
fn letter_re() -> &'static LeadRegex { static R: OnceLock<LeadRegex> = OnceLock::new(); R.get_or_init(|| LeadRegex::new(r"^(?:(?:[A-Za-z][.．)]|\([A-Za-z]\))(?=\s|$))")) }

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MarkerScheme {
    pub decimal: bool,
    pub symbols: HashMap<String, String>,
    pub excluded: Vec<String>,
    pub paragraph_count: usize,
    pub family_levels: HashMap<String, usize>,
}

impl MarkerScheme {
    pub fn is_empty(&self) -> bool { !self.decimal && self.symbols.is_empty() && self.family_levels.is_empty() }
    pub fn is_excluded(&self, text: &str) -> bool { self.excluded.iter().any(|x| x == text.trim()) }
    pub fn canonical(&self, text: &str) -> Option<(String, usize)> {
        if text.is_empty() || self.is_excluded(text) || label_re().is_match(marker_head(text)).ok()? { return None; }
        let (family, end) = classify_marker(text)?;
        if family == "-" && text.trim_start().chars().next().map(|c| BLACK_CIRCLED.contains(c)).unwrap_or(false) { return Some(("-".into(), end)); }
        let mut level = self.family_levels.get(&family).copied();
        // Python C6: deeper multi-level numbers use the detected anchor's relative depth.
        let separator = if family.starts_with("1-") { Some('-') } else if family.starts_with("1.") { Some('.') } else { None };
        if let (None, Some(sep)) = (level, separator) {
            let depth_of = |f: &str| f.matches(sep).count() + usize::from(sep == '-');
            let prefix = if sep == '-' { "1-" } else { "1." };
            let depth = depth_of(&family);
            if let Some((anchor_depth, anchor_level)) = self.family_levels.iter()
                .filter(|(f, _)| f.starts_with(prefix)).map(|(f, n)| (depth_of(f), *n)).min() {
                if depth >= anchor_depth { level = Some(anchor_level + depth - anchor_depth); }
            }
        }
        if let Some(level) = level {
            return Some((LEVELS[level.saturating_sub(1).min(2)].into(), end));
        }
        if let Some(symbol) = self.symbols.get(&family) { return Some((symbol.clone(), end)); }
        None
    }
    pub fn style_text<'a>(&self, text: &'a str) -> String {
        if let Some((marker, end)) = self.canonical(text) {
            let start = text.len() - text.trim_start().len();
            format!("{}{}{}", &text[..start], marker, &text[end..])
        } else { text.to_owned() }
    }
    pub fn summary(&self) -> BTreeMap<String, String> {
        self.family_levels.iter().map(|(f, n)| (f.clone(), LEVELS[n.saturating_sub(1).min(2)].into())).collect()
    }
}

pub fn classify_marker(text: &str) -> Option<(String, usize)> {
    let stripped = text.trim_start_matches(char::is_whitespace);
    if stripped.is_empty() { return None; }
    let lead_bytes = text.len() - stripped.len();
    let lead = text[..lead_bytes].chars().count();
    let stripped = marker_head(stripped);
    if let Some(full) = multi_re().find(stripped) {
        static DATE: OnceLock<LeadRegex> = OnceLock::new();
        let date = DATE.get_or_init(|| LeadRegex::new(r"^[0-9]{4}[.．][0-9]{1,2}[.．][0-9]{1,2}[.．]?(?=\s|$)"));
        if date.is_match(stripped) { return None; }
        let trimmed = full.trim_end_matches(['.', '．']);
        let parts: Vec<&str> = trimmed.split(['.', '．']).collect();
        let family = format!("{}.", vec!["1"; parts.len()].join("."));
        return Some((family, lead + full.chars().count()));
    }
    for (rx, sep) in [(hyphen_re(), "-")] {
        if let Some(token) = rx.find(stripped) {
            let depth = token.chars().filter(|c| if sep == "." { *c == '.' || *c == '．' } else { *c == '-' }).count() + 1;
            let mut digit_groups = Vec::new(); let mut digits = 0usize;
            for c in token.chars() { if c.is_ascii_digit() { digits += 1; } else { digit_groups.push(digits); digits = 0; } }
            digit_groups.push(digits);
            let date_like = sep == "." && digit_groups.len() == 3 && digit_groups[0] == 4 && digit_groups[1] >= 1 && digit_groups[1] <= 2 && digit_groups[2] >= 1 && digit_groups[2] <= 2;
            if date_like { return Some(("1.".into(), lead + 5)); }
            return Some((format!("{}.", vec!["1"; depth].join(sep)), lead + token.chars().count()));
        }
    }
    for (rx, rep) in [(number_re(), "1"), (korean_re(), "가"), (roman_re(), "Ⅰ"), (letter_re(), "")] {
        if let Some(m) = rx.find(stripped) {
            let token = m.replace('．', ".");
            let family = if std::ptr::eq(rx, roman_re()) {
                format!("{}{}", if token.chars().next().unwrap().is_ascii() { "I" } else { "Ⅰ" }, if token.ends_with('.') { "." } else { "" })
            } else if rep.is_empty() {
                let c = token.trim_start_matches('(').chars().next().unwrap();
                token.replace(c, if c.is_ascii_uppercase() { "A" } else { "a" })
            } else if std::ptr::eq(rx, number_re()) && token.ends_with(')') && !token.starts_with('(') {
                "1)".to_owned()
            } else {
                { static DIGIT_OR_HANGUL: OnceLock<Regex> = OnceLock::new();
                  DIGIT_OR_HANGUL.get_or_init(|| re(r"[0-9]+|[가나다라마바사아자차카타파하]")).replace_all(&token, rep).into_owned() }
            };
            return Some((family, lead + m.chars().count()));
        }
    }
    let mut chars = stripped.chars(); let first = chars.next()?; let second = chars.next();
    let has_token_boundary = second.map(char::is_whitespace).unwrap_or(true);
    if CIRCLED.contains(first) && has_token_boundary {
        let family = if ('①'..='⑳').contains(&first) { "①" } else if ('㉠'..='㉭').contains(&first) { "㉠" } else { "㉮" };
        return Some((family.into(), lead + 1));
    }
    if BLACK_CIRCLED.contains(first) && has_token_boundary { return Some(("-".into(), lead + 1)); }
    if stripped.starts_with("**") && stripped[2..].chars().next().map(char::is_whitespace).unwrap_or(true) { return Some(("**".into(), lead + 2)); }
    if has_token_boundary && (OFFICIAL.iter().any(|x| x.0 == first.to_string()) || NOTES.contains(&&*first.to_string()) || LEGACY.contains(first) || matches!(unicode_category(first), 'S' | 'P')) {
        if first == '–' || first == '—' || (first == '£') { return None; }
        return Some((first.to_string(), lead + 1));
    }
    None
}

fn unicode_category(c: char) -> char {
    // Python accepts So, Sm and Po, not all punctuation (Pd/Ps/Pe/Sc).
    static CATEGORY: OnceLock<regex::Regex> = OnceLock::new();
    if CATEGORY.get_or_init(|| regex::Regex::new(r"^[\p{So}\p{Sm}\p{Po}]$").unwrap()).is_match(&c.to_string()) { 'P' } else { 'O' }
}

#[cfg(test)]
mod lexical_parity_tests {
    use super::*;
    #[test]
    fn marker_head_keeps_long_paragraph_results() {
        // HRUST1 속도 수정: 긴 문단은 앞부분만 정규식에 넘기되 결과는 전체 문단과 같아야 한다.
        let tail = "가".repeat(80) + " 나다 " + &"라".repeat(500);
        let long_space = " ".repeat(120);
        for head in ["1. ", "2024.10 ", "1.2.3 ", "1-2 ", "가. ", "(3) ", "Ⅱ. ", "b. ", "① ", "□ ", "- ", "** ",
                     "2024.1.24. ", "12345678901234567890123456789012345678901234567890123456789012345678901234567890. ", "제목: ", "", "  ㅇ "] {
            let text = format!("{head}{tail}");
            assert!(marker_head(&text).len() < text.len());
            assert_eq!(classify_marker(&text), classify_marker(&format!("{head}끝")), "{head}");
        }
        let label = format!("제목{long_space}: {tail}");
        assert!(label_re().is_match(marker_head(&label)).unwrap());
        assert_eq!(marker_head("짧은 문단"), "짧은 문단");
        let no_space = "1".repeat(200) + ".";
        assert_eq!(marker_head(&no_space), no_space);
    }
    #[test]
    fn unicode_categories_and_multi_digit_families() {
        assert_eq!(classify_marker("・ 내용").map(|x|x.0), Some("・".into()));
        assert_eq!(classify_marker("－ 내용"), None);
        assert_eq!(classify_marker("- 내용").map(|x|x.0), Some("-".into()));
        assert_eq!(classify_marker("12. 제목").map(|x|x.0), Some("1.".into()));
    }
    // HRUST1 실문서 대조에서 찾은 Python 차이(합성 입력으로 고정).
    #[test]
    fn latin_letter_families_collapse_like_python() {
        assert_eq!(classify_marker("b. 항목").map(|x| x.0), Some("a.".into()));
        assert_eq!(classify_marker("(c) 항목").map(|x| x.0), Some("(a)".into()));
        assert_eq!(classify_marker("D) 항목").map(|x| x.0), Some("A)".into()));
        assert_eq!(classify_marker("2024.10 항목").map(|x| x.0), Some("1.1.".into()));
        assert_eq!(classify_marker("2024.1.24. 항목"), None);
        let s = detect_scheme(&["1. 가".into(), "(a) 나".into(), "(b) 다".into(), "2. 라".into()]);
        assert_eq!(s.family_levels.len(), 2);
        assert_eq!(s.family_levels["(a)"], 2);
    }
    #[test]
    fn year_numbers_label_attachment_and_tabs_follow_python() {
        let t = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // 연도 '2024.'도 Python처럼 1. 계열 후보다.
        let s = detect_scheme(&t(&["2024. 1. 24.", "2025. 3. 1."]));
        assert_eq!(s.family_levels.get("1."), Some(&1));
        // 부제: 라벨은 체계에서 빠지고, 붙임: 줄은 라벨이 아니다.
        assert!(label_re().is_match("부제: 설명").unwrap());
        assert!(!label_re().is_match("붙임: 1. 자료").unwrap());
        // 앞 공백이 있는 붙임 묶음도 제외한다.
        assert_eq!(find_attachment_blocks(&t(&[" 붙임 1. 자료 1부.", "2. 자료 1부.  끝."])), vec![0, 1]);
        assert!(!is_numbered("1.5배 증가"));
        assert!(is_numbered("1.「법」 개정"));
        // 탭은 4칸 배수로 펼친다(공백 2 + 탭 = 4칸, 6칸이 아님).
        let s = detect_scheme(&t(&["1. 가", "  \t가. 나", "    (1) 다"]));
        assert_eq!(s.family_levels["가."], 2);
        assert_eq!(s.family_levels["(1)"], 3);
    }
    #[test]
    fn deeper_multilevel_numbers_use_anchor_depth() {
        let mut s = MarkerScheme::default();
        s.family_levels.insert("1.1.".into(), 1);
        assert_eq!(s.canonical("1.2.3. 하위").map(|x| x.0), Some("ㅇ".into()));
        assert_eq!(s.canonical("3. 상위"), None);
    }
}

pub fn ordinal(family: &str, token: &str) -> Option<Vec<i64>> {
    let token = token.trim().replace('．', ".");
    if family.starts_with('1') || family == "(1)" { static DIGITS: OnceLock<Regex> = OnceLock::new(); return Some(DIGITS.get_or_init(|| re(r"[0-9]+")).find_iter(&token).filter_map(|m| m.ok()?.as_str().parse::<i64>().ok()).collect()); }
    if family.contains('가') { return HANGUL.find(token.trim_matches(['.', '(', ')'])).map(|i| vec![(i / 3) as i64]); }
    if ["①", "㉠", "㉮"].contains(&family) { let base = family.chars().next()? as i64; return token.chars().next().map(|c| vec![c as i64 - base]); }
    if family.starts_with('Ⅰ') { return unicode_roman_value(&token).map(|v| vec![v]); }
    if family.starts_with('I') { return roman_value(token.trim_end_matches('.')).map(|v| vec![v]); }
    let f = family.trim_matches(['.', '(', ')']);
    if f == "A" || f == "a" { return token.trim_matches(['.', '(', ')']).chars().next().map(|c| vec![c as i64]); }
    None
}
fn unicode_roman_value(s: &str) -> Option<i64> {
    let mut normalized = String::new();
    for c in s.trim_end_matches('.').chars() {
        normalized.push_str(match c {
            'Ⅰ'=>"I", 'Ⅱ'=>"II", 'Ⅲ'=>"III", 'Ⅳ'=>"IV", 'Ⅴ'=>"V", 'Ⅵ'=>"VI",
            'Ⅶ'=>"VII", 'Ⅷ'=>"VIII", 'Ⅸ'=>"IX", 'Ⅹ'=>"X", 'Ⅺ'=>"XI", 'Ⅻ'=>"XII",
            'Ⅼ'=>"L", 'Ⅽ'=>"C", 'Ⅾ'=>"D", 'Ⅿ'=>"M",
            other if other.is_ascii() => { normalized.push(other); continue; },
            _ => return None,
        });
    }
    roman_value(&normalized)
}
fn roman_value(s: &str) -> Option<i64> {
    let chars: Vec<_> = s.chars().collect();
    if chars.iter().any(|c| !matches!(c, 'I'|'V'|'X'|'L'|'C'|'D'|'M')) { return None; }
    let value = |c| match c { 'I'=>1,'V'=>5,'X'=>10,'L'=>50,'C'=>100,'D'=>500,'M'=>1000,_=>0 };
    Some((0..chars.len()).map(|i| if i+1<chars.len() && value(chars[i])<value(chars[i+1]) {-value(chars[i])} else {value(chars[i])}).sum())
}

pub fn detect_scheme(texts: &[String]) -> MarkerScheme {
    let excluded_indices = find_attachment_blocks(texts);
    let mut excluded: Vec<String> = excluded_indices.iter().map(|&i| texts[i].trim().to_owned()).filter(|s| !s.is_empty()).collect(); excluded.sort(); excluded.dedup();
    let body: Vec<&String> = texts.iter().enumerate().filter(|(i,t)| !excluded_indices.contains(i) && !t.is_empty() && !t.trim().is_empty()).map(|(_,t)|t).collect();
    let found: Vec<(&String, Option<(String,usize)>)> = body.iter().copied().filter(|t| !label_re().is_match(marker_head(t)).unwrap_or(false)).map(|t| (t, if t.trim_start().chars().next().map(|c| BLACK_CIRCLED.contains(c)).unwrap_or(false) {None} else {classify_marker(t)})).collect();
    let occupied: Vec<String> = found.iter().filter_map(|(_,m)| m.as_ref().and_then(|(f,_)| OFFICIAL.iter().find(|(k,_)| k==f).map(|(_,v)|v.to_string()))).collect();
    let has_box = occupied.iter().any(|x| x == "□");
    let order = ["1.", "가.", "1)", "가)", "(1)", "(가)", "①", "㉮"];
    let mut ordinals: HashMap<String,Vec<Option<Vec<i64>>>> = HashMap::new(); let mut candidates=Vec::new();
    // Python defaultdict preserves each family's first occurrence in the document.
    let mut family_order = Vec::new();
    for (text, marker) in &found { let Some((family,end))=marker else {continue}; let token_end = char_to_byte(text, *end); if family == "-" || OFFICIAL.iter().any(|(f,_)|f==family) || NOTES.contains(&family.as_str()) {continue;} if LEGACY.contains(family.chars().next().unwrap_or('\0')) && !occupied.is_empty(){continue;} if has_box && ["가.","Ⅰ","Ⅰ.","Ⅱ","I."].contains(&family.as_str()){continue;} let ord=ordinal(family,&text[..token_end]); if !ordinals.contains_key(family) { family_order.push(family.clone()); } ordinals.entry(family.clone()).or_default().push(ord); candidates.push(((*text).clone(),family.clone(),*end)); }
    let sequential: Vec<String> = family_order.iter().filter(|f| {
        let v = &ordinals[*f];
        v.windows(2).any(|w| matches!((&w[0],&w[1]), (Some(a),Some(b)) if a.len()==b.len() && a[..a.len()-1]==b[..b.len()-1] && b.last()==a.last().map(|x|x+1).as_ref()))
    }).cloned().collect();
    let mut eligible = sequential.clone();
    for f in &family_order {
        let v = &ordinals[f];
        if v.first().map(Option::is_none).unwrap_or(false) || f.starts_with("1.1") || f.starts_with("1-1") {
            if !eligible.contains(f) { eligible.push(f.clone()); }
        }
    }
    let ranks: Vec<Option<usize>> = family_order.iter().map(|f| order.iter().position(|x| *x == f)).collect();
    let official_chain = ranks.len() > 1 && ranks.iter().all(Option::is_some) && ranks.windows(2).all(|w| w[0] < w[1]);
    let shapes_exist = family_order.iter().any(|f| ordinals[f].first().map(Option::is_none).unwrap_or(false));
    let multi_exist = family_order.iter().any(|f| f.starts_with("1.1") || f.starts_with("1-1"));
    let supported = ordinals.len() > 1 && (!sequential.is_empty() || shapes_exist || multi_exist || official_chain);
    if supported {
        for f in &family_order {
            let v = &ordinals[f];
            let distinct: std::collections::HashSet<_> = v.iter().collect();
            if distinct.len() == 1 && !eligible.contains(f) { eligible.push(f.clone()); }
        }
    }
    let mut levels = HashMap::new();
    let mut levels_order = Vec::new();
    let mut stack: Vec<(String, usize)> = Vec::new();
    let available = LEVELS.iter().position(|x| !occupied.contains(&x.to_string())).map(|i| i + 1).unwrap_or(3);
    for (text, family, _) in candidates {
        if !eligible.contains(&family) { continue; }
        let indent = text.len() - text.trim_start().len();
        // Python str.expandtabs(4): a tab moves to the next multiple of 4.
        let indent = text[..indent].chars().fold(0usize, |col, c| if c == '\t' { col + 4 - col % 4 } else { col + 1 });
        if levels.contains_key(&family) {
            stack.retain(|(f, _)| levels[f] < levels[&family]);
        } else {
            if stack.last().map(|x| indent < x.1).unwrap_or(false) {
                while stack.last().map(|x| indent <= x.1).unwrap_or(false) { stack.pop(); }
            } else if stack.last().map(|x| indent == x.1 && indent > 0).unwrap_or(false) {
                let rank = order.iter().position(|x| *x == family);
                while stack.last().map(|(f, n)| indent == *n && rank.map(|r| r <= order.iter().position(|x| *x == f).unwrap_or(0)).unwrap_or(false)).unwrap_or(false) { stack.pop(); }
            }
            let mut proposed = stack.last().map(|x| levels[&x.0] + 1).unwrap_or(available);
            while proposed < 3 && occupied.contains(&LEVELS[proposed - 1].to_string()) { proposed += 1; }
            levels.insert(family.clone(), proposed);
            levels_order.push(family.clone());
        }
        stack.push((family, indent));
    }
    let symbols = levels_order.iter().filter(|f| f.chars().count() == 1 && ordinal(f, f).is_none()).map(|f| (f.clone(), LEVELS[(levels[f] - 1).min(2)].to_string())).collect();
    let mut scheme = MarkerScheme { decimal: levels.keys().any(|f| f.starts_with("1.1")), symbols, excluded, paragraph_count: 0, family_levels: levels };
    scheme.paragraph_count=body.iter().filter(|t|scheme.canonical(t).is_some()).count(); scheme
}

fn char_to_byte(text: &str, char_count: usize) -> usize {
    text.char_indices().nth(char_count).map(|(index, _)| index).unwrap_or(text.len())
}

pub fn scheme_json(s: &MarkerScheme) -> Value { json!({"decimal":s.decimal,"symbols":s.symbols,"excluded":s.excluded,"paragraph_count":s.paragraph_count,"family_levels":s.family_levels,"summary":s.summary()}) }
fn find_attachment_blocks(texts:&[String])->Vec<usize>{let mut out=Vec::new();let mut i=0;while i<texts.len(){if !start_re().is_match(marker_head(&texts[i])).unwrap_or(false){i+=1;continue;}let mut end=None;for j in i..texts.len().min(i+30){let t=&texts[j];if t.trim().is_empty()||!(t.trim_end().ends_with('.')||is_numbered(t)){break;}if t.trim_end().ends_with("끝."){end=Some(j);break;}}if let Some(e)=end{out.extend(i..=e);i=e+1;}else{i+=1;}}out}
// Python attachment_block._START/_ITEM/_HEADER.
fn start_re() -> &'static Regex { static R: OnceLock<Regex> = OnceLock::new(); R.get_or_init(|| re(r"^\s*붙임")) }
fn is_numbered(t:&str)->bool{
    let t = marker_head(t);
    static ITEM: OnceLock<LeadRegex> = OnceLock::new();
    static HEADER: OnceLock<LeadRegex> = OnceLock::new();
    ITEM.get_or_init(|| LeadRegex::new(r"^[ \t\u{00a0}\u{3000}]*\d+\.(?=\s|$|「)")).is_match(t)
        || HEADER.get_or_init(|| LeadRegex::new(r"^[ \t\u{00a0}\u{3000}]*붙임[ \t\u{00a0}\u{3000}]*[:：]?[ \t\u{00a0}\u{3000}]*\d+\.(?=\s|$|「)")).is_match(t)
}

#[cfg(test)] mod tests { use super::*; #[test] fn scheme_smoke(){let s=detect_scheme(&["1. 첫".into(),"2. 둘".into(),"가. 하위".into(),"나. 다음".into()]);assert!(!s.is_empty());assert!(s.canonical("1. 첫").is_some());} }

#[cfg(test)]
mod order_tests {
    use super::*;

    #[test]
    fn official_chain_follows_first_occurrence() {
        for _ in 0..1000 {
            for families in [vec!["1. first", "가. child"], vec!["1. first", "① child"], vec!["1. first", "㉮ child"]] {
                let texts = families.iter().map(|s| s.to_string()).collect::<Vec<_>>();
                let scheme = detect_scheme(&texts);
                assert_eq!(scheme.family_levels.len(), 2);
                assert_eq!(scheme.family_levels["1."], 1);
                let child = classify_marker(&texts[1]).unwrap().0;
                assert_eq!(scheme.family_levels[&child], 2);
                let reverse = texts.into_iter().rev().collect::<Vec<_>>();
                assert!(detect_scheme(&reverse).family_levels.is_empty());
            }
        }
    }

    #[test]
    fn mixed_numbering_ndet_reproduction_is_deterministic_1000_times() {
        let texts: Vec<String> = vec![
            "1. 추진 배경".into(),
            "가. 세부 계획".into(),
            "ㅇ 사업의 추진 현황을 다음과 같이 보고합니다.".into(),
            "- 세부 내용 및 향후 일정".into(),
        ];
        let first = detect_scheme(&texts);
        assert_eq!(first.family_levels.get("1."), Some(&1));
        assert_eq!(first.family_levels.get("가."), Some(&3));
        for _ in 0..1000 {
            let s = detect_scheme(&texts);
            assert_eq!(s.family_levels, first.family_levels);
            assert_eq!(s.symbols, first.symbols);
            assert_eq!(s.paragraph_count, first.paragraph_count);
        }
    }
}
