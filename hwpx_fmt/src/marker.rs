//! 항목기호·보호 구간 규칙(파이썬 `marker_scheme.classify_marker`, `xml_hanging_indent.marker_end_offset`,
//! `spacing.문장부호_마커_끝위치`·`자간보호_글자수`, `formatter_indent._문단_본문시작_오프셋` 등).
//! 활성 항목기호 체계가 없고 내어쓰기 규칙이 비어 있을 때의 동작만 옮겼다(그 밖은 config 에서 거부).
//! 오프셋은 모두 파이썬 str 인덱스와 같은 **문자(코드 포인트) 수**다.
use crate::rules::{py_strip, py_ws};
use regex::Regex;
use std::sync::LazyLock;

macro_rules! re {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pat).unwrap());
    };
}

// 파이썬 `(?=\s|$)` 를 먹는 그룹 `la` 로 흉내 낸다(끝 위치 = la 의 시작). [\s\x1c-\x1f] = 파이썬 \s
re!(RE_DATE_LEAD, r"^[0-9]{4}[.．][0-9]{1,2}[.．][0-9]{1,2}[.．]?(?:[\s\x1c-\x1f]|\z)");
re!(RE_MULTI, r"^([0-9]+(?:[.．][0-9]+)+)[.．]?(?P<la>[\s\x1c-\x1f]|\z)");
re!(RE_HYPHEN, r"^([0-9]+(?:-[0-9]+)+)[.．]?(?P<la>[\s\x1c-\x1f]|\z)");
re!(RE_NUMBER, r"^(?:[0-9]+[.．)]|\([0-9]+\))(?P<la>[\s\x1c-\x1f]|\z)");
re!(RE_KOREAN, r"^(?:[가나다라마바사아자차카타파하][.．)]|\([가나다라마바사아자차카타파하]\))(?P<la>[\s\x1c-\x1f]|\z)");
re!(RE_ROMAN, r"^(?:[Ⅰ-Ⅿ]+[.．]?|(?:[IVX]|[IVXLCDM]{2,})[.．])(?P<la>[\s\x1c-\x1f]|\z)");
re!(RE_LETTER, r"^(?:[A-Za-z][.．)]|\([A-Za-z]\))(?P<la>[\s\x1c-\x1f]|\z)");
re!(RE_STARS, r"^[*＊]+");
re!(RE_CAT_SOMPO, r"^[\p{So}\p{Sm}\p{Po}]$");
re!(RE_CAT_PS, r"^[\p{P}\p{S}]$");
re!(RE_CAT_BRACKET, r"^[\p{Ps}\p{Pe}\p{Pi}\p{Pf}]$");
re!(RE_PAREN_LABEL_ONLY, r"^[\s\x1c-\x1f]*[（(][^\r\n]+[)）][\s\x1c-\x1f]*[:：]?[\s\x1c-\x1f]*$");
re!(RE_PAREN_LABEL_LEAD, r"^[（(][^()（）\r\n]+[)）][ \t]*[:：][ \t]*(?P<la>\S)");
re!(RE_COLON_LABEL, r"^([^\n\r:：]{1,30}?)[ \t]*[:：][ \t]+(?P<la>\S)");
re!(RE_LABEL_REJECT, r"(?i)[/\\]|https?\z|\A\p{Nd}+\z");
re!(RE_YEAR, r"^['’‘]?[0-9]{2,4}(?:년|[./-][0-9]{1,2})*\.?$");
// 항목_패턴(hwp-auto-docfit.py) = ITEM_PATTERNS(xml_hanging_indent.py)
re!(RE_ITEM0, r"^[\s\x1c-\x1f]*\d+(?:-\d+)+[\s\x1c-\x1f]*[.)]");
re!(RE_ITEM1, r"^[\s\x1c-\x1f]*\d+\.");
re!(RE_ITEM2, r"^[\s\x1c-\x1f]*\d+\)");
re!(RE_ITEM3, r"^[\s\x1c-\x1f]*[가-힣]\.");
re!(RE_ITEM4, r"^[\s\x1c-\x1f]*[가-힣]\)");
re!(RE_ITEM5, r"^[\s\x1c-\x1f]*\([가-힣]\)");
re!(RE_ITEM6, r"^[\s\x1c-\x1f]*\(\d+\)");
re!(RE_ITEM7, r"^[\s\x1c-\x1f]*\[[가-힣]\]");

fn item_patterns() -> [&'static Regex; 8] {
    [&RE_ITEM0, &RE_ITEM1, &RE_ITEM2, &RE_ITEM3, &RE_ITEM4, &RE_ITEM5, &RE_ITEM6, &RE_ITEM7]
}

const OFFICIAL_SYMBOLS: &str = "※ㅁ□■○●◎◇◆△▲▽▼▷▶◁◀▪▫ㆍ·‧•∙⋅‣⁃ㅇ①②③④⑤⑥⑦⑧⑨⑩❶❷❸❹❺❻❼❽❾❿➊➋➌➍➎➏➐➑➒➓㉠㉡㉢㉣㉤㉥㉦㉧㉨㉩㉪→←↑↓⇒⇐―–—-‒−";
const CIRCLED: &str = "①②③④⑤⑥⑦⑧⑨⑩⑪⑫⑬⑭⑮⑯⑰⑱⑲⑳㉠㉡㉢㉣㉤㉥㉦㉧㉨㉩㉪㉫㉬㉭㉮㉯㉰㉱㉲㉳㉴㉵㉶㉷㉸㉹㉺㉻";
const BLACK_CIRCLED: &str = "❶❷❸❹❺❻❼❽❾❿";
const OFFICIAL_LEVEL: &str = "□ㅁㅇ○-";
const NOTES: &str = "*※";
const LEGACY: &str = "◦☞•·∙‧⋅ㆍ";
const OPENING_BRACKET_QUOTE: &str = "([{\"'“‘「『《〈（〔【";

fn clen(s: &str) -> usize {
    s.chars().count()
}
/// 바이트 위치 → 문자 수
fn cpos(s: &str, byte: usize) -> usize {
    s[..byte].chars().count()
}
fn lstrip(s: &str) -> &str {
    s.trim_start_matches(py_ws)
}
fn cat(c: char, re: &Regex) -> bool {
    let mut b = [0u8; 4];
    re.is_match(c.encode_utf8(&mut b))
}
fn second_is_space_or_end(s: &str) -> bool {
    let mut it = s.chars();
    it.next();
    match it.next() {
        None => true,
        Some(c) => py_ws(c),
    }
}

/// marker_scheme.classify_marker 의 "원문 기호 끝"(문자 수). 분류가 없으면 None.
pub fn classify_marker(text: &str) -> Option<usize> {
    let stripped = lstrip(text);
    if stripped.is_empty() {
        return None;
    }
    if RE_DATE_LEAD.is_match(stripped) {
        return None;
    }
    let lead = clen(text) - clen(stripped);
    for re in [&*RE_MULTI, &*RE_HYPHEN, &*RE_NUMBER, &*RE_KOREAN, &*RE_ROMAN, &*RE_LETTER] {
        if let Some(c) = re.captures(stripped) {
            let end = c.name("la").unwrap().start();
            return Some(lead + cpos(stripped, end));
        }
    }
    let first = stripped.chars().next().unwrap();
    if CIRCLED.contains(first) && second_is_space_or_end(stripped) {
        return Some(lead + 1);
    }
    if BLACK_CIRCLED.contains(first) && second_is_space_or_end(stripped) {
        return Some(lead + 1);
    }
    if stripped.starts_with("**") && stripped.chars().nth(2).map_or(true, py_ws) {
        return Some(lead + 2);
    }
    if second_is_space_or_end(stripped)
        && (OFFICIAL_LEVEL.contains(first) || NOTES.contains(first) || LEGACY.contains(first) || cat(first, &RE_CAT_SOMPO))
    {
        return Some(lead + 1);
    }
    None
}

/// xml_hanging_indent.marker_end_offset
pub fn marker_end_offset(text: &str) -> Option<usize> {
    if text.is_empty() {
        return None;
    }
    if let Some(e) = classify_marker(text) {
        return Some(e);
    }
    let stripped = lstrip(text);
    if stripped.is_empty() {
        return None;
    }
    let lead = clen(text) - clen(stripped);
    if let Some(m) = RE_STARS.find(stripped) {
        return Some(lead + cpos(stripped, m.end()));
    }
    for p in item_patterns() {
        if let Some(m) = p.find(stripped) {
            return Some(lead + cpos(stripped, m.end()));
        }
    }
    let first = stripped.chars().next().unwrap();
    if OFFICIAL_SYMBOLS.contains(first) {
        return Some(lead + 1);
    }
    if OPENING_BRACKET_QUOTE.contains(first) {
        return None;
    }
    if cat(first, &RE_CAT_PS) {
        return Some(lead + 1);
    }
    None
}

/// spacing.문장부호_마커_끝위치(활성 항목기호 체계 없음)
pub fn punct_marker_end(text: &str) -> Option<usize> {
    if text.is_empty() {
        return None;
    }
    if let Some(e) = classify_marker(text) {
        return Some(e);
    }
    let s = lstrip(text);
    if s.is_empty() {
        return None;
    }
    let lead = clen(text) - clen(s);
    if let Some(m) = RE_STARS.find(s) {
        return Some(lead + cpos(s, m.end()));
    }
    for p in item_patterns() {
        if let Some(m) = p.find(s) {
            return Some(lead + cpos(s, m.end()));
        }
    }
    let first = s.chars().next().unwrap();
    if OFFICIAL_SYMBOLS.contains(first) {
        return Some(lead + 1);
    }
    if cat(first, &RE_CAT_BRACKET) {
        return None;
    }
    if cat(first, &RE_CAT_PS) {
        return Some(lead + 1);
    }
    None
}

fn skip_ws(t: &[char], mut idx: usize) -> usize {
    while idx < t.len() && matches!(t[idx], ' ' | '\t' | '\u{a0}' | '\u{3000}') {
        idx += 1;
    }
    idx
}
fn rstrip_crlf(s: &str) -> &str {
    s.trim_end_matches(['\r', '\n'])
}
fn sub(t: &[char], from: usize) -> String {
    t[from.min(t.len())..].iter().collect()
}

/// formatter_indent._문단_기호뒤_오프셋(활성 항목기호 체계 없음)
fn after_marker(text: &str) -> Option<usize> {
    if text.is_empty() {
        return None;
    }
    let text = rstrip_crlf(text);
    let t: Vec<char> = text.chars().collect();
    let start = skip_ws(&t, 0);
    if start >= t.len() {
        return None;
    }
    let tail = sub(&t, start);
    let first = t[start];
    if !(OFFICIAL_SYMBOLS.contains(first) || "*＊☞ㅁ".contains(first) || item_patterns().iter().any(|p| p.is_match(&tail))) {
        return None;
    }
    let marker_end = marker_end_offset(text).unwrap_or(start + 1);
    let idx = skip_ws(&t, marker_end);
    if idx >= t.len() || t[idx] == '\r' || t[idx] == '\n' {
        return None;
    }
    Some(idx)
}

/// formatter_indent._문단_본문시작_오프셋(활성 항목기호 체계 없음). 자간 보호에서는 괄호라벨_뒤=true.
fn body_start(text: &str, paren_label_after: bool) -> Option<usize> {
    if text.is_empty() {
        return None;
    }
    let text = rstrip_crlf(text);
    let t: Vec<char> = text.chars().collect();
    let start = skip_ws(&t, 0);
    if start >= t.len() {
        return None;
    }
    let tail = sub(&t, start);
    let first = t[start];
    let structural = OFFICIAL_SYMBOLS.contains(first) || "*＊☞ㅁ".contains(first) || item_patterns().iter().any(|p| p.is_match(&tail));
    if !structural {
        return RE_PAREN_LABEL_LEAD.captures(&tail).map(|c| start + cpos(&tail, c.name("la").unwrap().start()));
    }
    let idx = after_marker(text)?;
    let pair_close = |c: char| match c {
        '(' => Some(')'),
        '（' => Some('）'),
        _ => None,
    };
    if pair_close(t[idx]).is_some() {
        let mut stack: Vec<char> = vec![];
        let mut close = None;
        for j in idx..t.len() {
            let ch = t[j];
            if ch == '\r' || ch == '\n' {
                break;
            }
            if let Some(c) = pair_close(ch) {
                stack.push(c);
            } else if ch == ')' || ch == '）' {
                if stack.pop() != Some(ch) {
                    break;
                }
                if stack.is_empty() {
                    close = Some(j);
                    break;
                }
            }
        }
        if let Some(close) = close {
            let label_raw: String = t[idx + 1..close].iter().collect();
            let label = py_strip(&label_raw);
            let mut after = skip_ws(&t, close + 1);
            let mut separated = after > close + 1;
            if after < t.len() && (t[after] == ':' || t[after] == '：') {
                after = skip_ws(&t, after + 1);
                separated = true;
            }
            let year = RE_YEAR.is_match(label);
            let quote_boundary = after < t.len() && "「『‘“'\"".contains(t[after]);
            let colon_label = t.get(skip_ws(&t, close + 1)).map_or(false, |&c| c == ':' || c == '：');
            if !label.is_empty()
                && !year
                && (separated || quote_boundary)
                && after < t.len()
                && t[after] != '\r'
                && t[after] != '\n'
                && (colon_label || paren_label_after)
            {
                return Some(after);
            }
        }
        return Some(idx);
    }
    let rest = sub(&t, idx);
    if let Some(c) = RE_COLON_LABEL.captures(&rest) {
        let end = c.name("la").unwrap().start();
        let g1 = c.get(1).unwrap().as_str();
        let squeezed: usize = g1.chars().filter(|&ch| !py_ws(ch)).count();
        if squeezed <= 12 && !RE_LABEL_REJECT.is_match(py_strip(g1)) {
            return Some(idx + cpos(&rest, end));
        }
    }
    Some(idx)
}

/// formatter_indent._여는낫표_건너뛰기
fn skip_open_quote(text: &str, offset: Option<usize>) -> Option<usize> {
    let offset = offset?;
    let body: Vec<char> = rstrip_crlf(text).chars().collect();
    if offset + 1 < body.len() && (body[offset] == '「' || body[offset] == '『') && !py_ws(body[offset + 1]) {
        return Some(offset + 1);
    }
    Some(offset)
}

/// spacing.자간보호_글자수(활성 항목기호 체계 없음·내어쓰기 규칙 없음). 호출부의 `or 0` 은 `unwrap_or(0)`.
pub fn spacing_protect(text: &str) -> usize {
    let mut offset = skip_open_quote(text, body_start(text, true));
    if let Some(me) = punct_marker_end(text) {
        let t: Vec<char> = text.chars().collect();
        let rest = sub(&t, me);
        let full = clen(rstrip_crlf(text));
        if RE_PAREN_LABEL_ONLY.is_match(&rest) {
            offset = Some(full);
        }
        if py_strip(&rest).is_empty() {
            offset = Some(full);
        }
    }
    offset.unwrap_or(0)
}
