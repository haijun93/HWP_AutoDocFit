//! Port of `docfit_core.marker_scheme` for the hierarchy compatibility path.
use fancy_regex::Regex;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

const LEVELS: [&str; 3] = ["□", "ㅇ", "-"];
const OFFICIAL: [(&str, &str); 4] = [("□", "□"), ("ㅁ", "□"), ("ㅇ", "ㅇ"), ("○", "ㅇ")];
const NOTES: [&str; 3] = ["*", "**", "※"];
const LEGACY: &str = "◦☞•·∙‧⋅ㆍ";
const HANGUL: &str = "가나다라마바사아자차카타파하";
const CIRCLED: &str = "①②③④⑤⑥⑦⑧⑨⑩⑪⑫⑬⑭⑮⑯⑰⑱⑲⑳㉠㉡㉢㉣㉤㉥㉦㉧㉨㉩㉪㉫㉬㉭㉮㉯㉰㉱㉲㉳㉴㉵㉶㉷㉸㉹㉺㉻";
const BLACK_CIRCLED: &str = "❶❷❸❹❺❻❼❽❾❿";

fn re(pattern: &str) -> Regex { Regex::new(pattern).expect("valid marker scheme regex") }
fn label_re() -> &'static Regex { static R: OnceLock<Regex> = OnceLock::new(); R.get_or_init(|| re(r"^\s*(?:제목|개요|요약|붙임|중제목)\s*[:：]")) }
fn multi_re() -> &'static Regex { static R: OnceLock<Regex> = OnceLock::new(); R.get_or_init(|| re(r"([0-9]+(?:[.．][0-9]+)+)[.．]?(?=\s|$)")) }
fn hyphen_re() -> &'static Regex { static R: OnceLock<Regex> = OnceLock::new(); R.get_or_init(|| re(r"([0-9]+(?:-[0-9]+)+)[.．]?(?=\s|$)")) }
fn number_re() -> &'static Regex { static R: OnceLock<Regex> = OnceLock::new(); R.get_or_init(|| re(r"(?:[0-9]+[.．)]|\([0-9]+\))(?=\s|$)")) }
fn korean_re() -> &'static Regex { static R: OnceLock<Regex> = OnceLock::new(); R.get_or_init(|| re(r"(?:[가나다라마바사아자차카타파하][.．)]|\([가나다라마바사아자차카타파하]\))(?=\s|$)")) }
fn roman_re() -> &'static Regex { static R: OnceLock<Regex> = OnceLock::new(); R.get_or_init(|| re(r"(?:[Ⅰ-Ⅿ]+[.．]?|(?:[IVX]|[IVXLCDM]{2,})[.．])(?=\s|$)")) }
fn letter_re() -> &'static Regex { static R: OnceLock<Regex> = OnceLock::new(); R.get_or_init(|| re(r"(?:[A-Za-z][.．)]|\([A-Za-z]\))(?=\s|$)")) }

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
        if text.is_empty() || self.is_excluded(text) || label_re().is_match(text).ok()? { return None; }
        let (family, end) = classify_marker(text)?;
        if family == "-" && text.trim_start().chars().next().map(|c| BLACK_CIRCLED.contains(c)).unwrap_or(false) { return Some(("-".into(), end)); }
        if let Some(&level) = self.family_levels.get(&family) {
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
    if let Some(m) = multi_re().find(stripped).ok().flatten().filter(|m| m.start() == 0) {
        let date = re(r"^[0-9]{4}[.．][0-9]{1,2}[.．][0-9]{1,2}[.．]?(?=\s|$)");
        if date.is_match(stripped).ok()? { return None; }
        let full = m.as_str();
        let trimmed = full.trim_end_matches(['.', '．']);
        let parts: Vec<&str> = trimmed.split(['.', '．']).collect();
        let depth = if parts.first().map(|p| p.len() == 4).unwrap_or(false) { 1 } else { parts.len() };
        let family = if parts.first().map(|p| p.len() == 4).unwrap_or(false) { "1.".to_owned() } else { format!("{}.", vec!["1"; depth].join(".")) };
        return Some((family, lead + full.chars().count()));
    }
    for (rx, sep) in [(hyphen_re(), "-")] {
        if let Some(m) = rx.find(stripped).ok().flatten().filter(|m| m.start() == 0) {
            let token = m.as_str();
            let depth = token.chars().filter(|c| if sep == "." { *c == '.' || *c == '．' } else { *c == '-' }).count() + 1;
            let mut digit_groups = Vec::new(); let mut digits = 0usize;
            for c in token.chars() { if c.is_ascii_digit() { digits += 1; } else { digit_groups.push(digits); digits = 0; } }
            digit_groups.push(digits);
            let date_like = sep == "." && digit_groups.len() == 3 && digit_groups[0] == 4 && digit_groups[1] >= 1 && digit_groups[1] <= 2 && digit_groups[2] >= 1 && digit_groups[2] <= 2;
            if date_like { return Some(("1.".into(), lead + 5)); }
            return Some((format!("{}.", vec!["1"; depth].join(sep)), lead + stripped[..m.end()].chars().count()));
        }
    }
    for (rx, rep) in [(number_re(), "1"), (korean_re(), "가"), (roman_re(), "Ⅰ"), (letter_re(), "A")] {
        if let Some(m) = rx.find(stripped).ok().flatten().filter(|m| m.start() == 0) {
            let token = m.as_str().replace('．', ".");
            let family = if std::ptr::eq(rx, roman_re()) {
                format!("{}{}", if token.chars().next().unwrap().is_ascii() { "I" } else { "Ⅰ" }, if token.ends_with('.') { "." } else { "" })
            } else if rep.is_empty() {
                let c = token.trim_start_matches('(').chars().next().unwrap();
                token.replace(c, if c.is_ascii_uppercase() { "A" } else { "a" })
            } else if std::ptr::eq(rx, number_re()) && token.ends_with(')') && !token.starts_with('(') {
                "1)".to_owned()
            } else {
                let mut out = token.clone();
                for c in token.chars() { if c.is_ascii_digit() || HANGUL.contains(c) { out = out.replace(c, rep); } }
                out
            };
            return Some((family, lead + stripped[..m.end()].chars().count()));
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
    let n = c as u32;
    if matches!(n, 0x0021..=0x002F | 0x003A..=0x0040 | 0x005B..=0x0060 | 0x007B..=0x007E | 0x00A1..=0x00BF | 0x2000..=0x206F | 0x2E00..=0x2E7F | 0x3001..=0x303F | 0xFE10..=0xFE1F | 0xFE30..=0xFE6F | 0xFF01..=0xFF0F | 0xFF1A..=0xFF20 | 0xFF3B..=0xFF40 | 0xFF5B..=0xFF65) { 'P' }
    else if matches!(n, 0x0024..=0x002B | 0x003C..=0x003E | 0x005E..=0x005E | 0x0060..=0x0060 | 0x007C..=0x007E | 0x00A2..=0x00A9 | 0x20A0..=0x20CF | 0x2100..=0x214F | 0x2190..=0x2BFF | 0x1F000..=0x1FAFF) { 'S' }
    else { 'O' }
}

pub fn ordinal(family: &str, token: &str) -> Option<Vec<i64>> {
    let token = token.trim().replace('．', ".");
    if family.starts_with('1') || family == "(1)" { return Some(re(r"[0-9]+").find_iter(&token).filter_map(|m| m.ok()?.as_str().parse::<i64>().ok()).collect()); }
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
    let found: Vec<(&String, Option<(String,usize)>)> = body.iter().copied().filter(|t| !label_re().is_match(t).unwrap_or(false)).map(|t| (t, if t.trim_start().chars().next().map(|c| BLACK_CIRCLED.contains(c)).unwrap_or(false) {None} else {classify_marker(t)})).collect();
    let occupied: Vec<String> = found.iter().filter_map(|(_,m)| m.as_ref().and_then(|(f,_)| OFFICIAL.iter().find(|(k,_)| k==f).map(|(_,v)|v.to_string()))).collect();
    let has_box = occupied.iter().any(|x| x == "□");
    let order = ["1.", "가.", "1)", "가)", "(1)", "(가)", "Ⅰ", "Ⅱ"];
    let mut ordinals: HashMap<String,Vec<Option<Vec<i64>>>> = HashMap::new(); let mut candidates=Vec::new();
    for (text, marker) in &found { let Some((family,end))=marker else {continue}; if OFFICIAL.iter().any(|(f,_)|f==family) || NOTES.contains(&family.as_str()) {continue;} if LEGACY.contains(family.chars().next().unwrap_or('\0')) && !occupied.is_empty(){continue;} if has_box && ["가.","Ⅰ","Ⅱ","I."].contains(&family.as_str()){continue;} let ord=ordinal(family,&text[..*end]); ordinals.entry(family.clone()).or_default().push(ord); candidates.push(((*text).clone(),family.clone(),*end)); }
    let sequential: Vec<String> = ordinals.iter().filter(|(_,v)| v.windows(2).any(|w| matches!((&w[0],&w[1]),(Some(a),Some(b)) if a.len()==b.len() && a[..a.len()-1]==b[..b.len()-1] && b.last()==a.last().map(|x|x+1).as_ref()))).map(|(f,_)|f.clone()).collect();
    let mut eligible=sequential.clone(); for (f,v) in &ordinals { if v.first().map(Option::is_none).unwrap_or(false) || f.starts_with("1.1") || f.starts_with("1-1") { if !eligible.contains(f){eligible.push(f.clone());} } }
    let ranks:Vec<Option<usize>>=ordinals.keys().map(|f|order.iter().position(|x|*x==f)).collect(); let official_chain=ranks.len()>1&&ranks.iter().all(Option::is_some)&&ranks.windows(2).all(|w|w[0]<w[1]);
    let supported=ordinals.len()>1&&(!sequential.is_empty()||ordinals.values().any(|v|v.first().map(Option::is_none).unwrap_or(false))||ordinals.keys().any(|f|f.starts_with("1.1")||f.starts_with("1-1"))||official_chain);
    if supported { for (f,v) in &ordinals { let distinct:std::collections::HashSet<_>=v.iter().collect(); if distinct.len()==1&&!eligible.contains(f){eligible.push(f.clone());} } }
    let mut levels=HashMap::new(); let mut stack:Vec<(String,usize)>=Vec::new(); let available=LEVELS.iter().position(|x|!occupied.contains(&x.to_string())).map(|i|i+1).unwrap_or(3);
    for (text,family,_) in candidates { if !eligible.contains(&family){continue;} let indent=text.len()-text.trim_start().len(); let indent=text[..indent].chars().map(|c|if c=='\t'{4}else{1}).sum::<usize>();
        if levels.contains_key(&family) { stack.retain(|(f,_)| levels[f] < levels[&family]); }
        else { if stack.last().map(|x|indent<x.1).unwrap_or(false) {while stack.last().map(|x|indent<=x.1).unwrap_or(false){stack.pop();}} else if stack.last().map(|x|indent==x.1&&indent>0).unwrap_or(false) {let rank=order.iter().position(|x|*x==family); while stack.last().map(|(f,n)|indent==*n&&rank.map(|r|r<=order.iter().position(|x|*x==f).unwrap_or(0)).unwrap_or(false)).unwrap_or(false){stack.pop();}}
            let mut proposed=stack.last().map(|x|levels[&x.0]+1).unwrap_or(available); while proposed<3&&occupied.contains(&LEVELS[proposed-1].to_string()){proposed+=1;} levels.insert(family.clone(),proposed); }
        stack.push((family,indent));
    }
    let symbols=levels.iter().filter(|(f,_)|f.chars().count()==1&&ordinal(f,f).is_none()).map(|(f,n)|(f.clone(),LEVELS[(n-1).min(2)].to_string())).collect();
    let mut scheme=MarkerScheme{decimal:levels.keys().any(|f|f.starts_with("1.1")),symbols,excluded,paragraph_count:0,family_levels:levels};
    scheme.paragraph_count=body.iter().filter(|t|scheme.canonical(t).is_some()).count(); scheme
}

pub fn scheme_json(s: &MarkerScheme) -> Value { json!({"decimal":s.decimal,"symbols":s.symbols,"excluded":s.excluded,"paragraph_count":s.paragraph_count,"family_levels":s.family_levels,"summary":s.summary()}) }
fn find_attachment_blocks(texts:&[String])->Vec<usize>{let mut out=Vec::new();let mut i=0;while i<texts.len(){if !texts[i].starts_with("붙임"){i+=1;continue;}let mut end=None;for j in i..texts.len().min(i+30){let t=&texts[j];if t.trim().is_empty()||!(t.trim_end().ends_with('.')||is_numbered(t)){break;}if t.trim_end().ends_with("끝."){end=Some(j);break;}}if let Some(e)=end{out.extend(i..=e);i=e+1;}else{i+=1;}}out}
fn is_numbered(t:&str)->bool{re(r"^[ \t\u{00a0}\u{3000}]*\d+\.(?=\s|$|[^0-9])").is_match(t).unwrap_or(false)||re(r"^\s*붙임\s*[:：]?\s*\d+\.").is_match(t).unwrap_or(false)}

#[cfg(test)] mod tests { use super::*; #[test] fn scheme_smoke(){let s=detect_scheme(&["1. 첫".into(),"2. 둘".into(),"가. 하위".into(),"나. 다음".into()]);assert!(!s.is_empty());assert!(s.canonical("1. 첫").is_some());} }
