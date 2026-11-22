//! RSXML3 공용: 항목기호 계층(style_hierarchy.leading_marker, style_unify.unify_marker/marker_class),
//! 글자 폭 추정(connector_tabs.char_em), 파이썬 반올림. 활성 항목기호 체계(scheme)가 없을 때의 동작만 옮겼다.
//! 오프셋은 모두 파이썬 str 인덱스와 같은 문자(코드 포인트) 수다.
use crate::rules::{py_strip, py_ws};
use regex::Regex;
use std::sync::LazyLock;

macro_rules! re {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pat).unwrap());
    };
}

// 파이썬 lookahead (?=...) 는 먹는 그룹 `la` 로 흉내 낸다. [\s\x1c-\x1f] = 파이썬 \s, \z = $(끝; 앞 줄바꿈은 \s 가 먹는다)
re!(RE_ROMAN_UP, r"^(?:[IVX]|[IVXLCDM]{2,})[.．](?P<la>[\s\x1c-\x1f]|\z)");
re!(RE_CIRCLED, r"^[①-⑳㉠-㉻❶-❿➊-➓](?P<la>[\s\x1c-\x1f]|\z)");
re!(RE_M_CHAPTER, r"^(?:제[\s\x1c-\x1f]?\d{1,2}[\s\x1c-\x1f]?[장편부]|\d{1,2}[\s\x1c-\x1f]?장|[Cc]hapter[\s\x1c-\x1f]?\d{1,2})(?P<la>[\s\x1c-\x1f]|\z|[.．:：])");
re!(RE_M_MID, r"^(?:[Ⅰ-Ⅿ]+|[가-하])(?P<la>[\s\x1c-\x1f]|[.．])");
re!(RE_M_SUB, r"^(?:[□ㅁ■]|\d{1,2}[.．]?)(?P<la>[\s\x1c-\x1f]|\z)");
re!(RE_M_BODY, r"^(?:[ㅇ○◦☞]|[가-하]\))(?P<la>[\s\x1c-\x1f]|\z)");
re!(RE_M_CONTENT, r"^(?:-|\d+\))(?P<la>[\s\x1c-\x1f]|\z)");
re!(RE_M_NOTE, r"^(?:\*\*?|※|[•·‧∙⋅ㆍ●]|\([가-하0-9]+\)[.．]?)(?P<la>[\s\x1c-\x1f]|\z)");
re!(RE_TIGHT, r"^(\d{1,2}[.．])(?P<la>[^\d\s\x1c-\x1f.．,，])");

fn lstrip(s: &str) -> &str {
    s.trim_start_matches(py_ws)
}

fn canonical_marker(m: &str) -> String {
    let mut it = m.chars();
    match (it.next(), it.next()) {
        (Some(c), None) if "•·‧∙⋅ㆍ●".contains(c) => "•".into(),
        (Some(c), None) if c == '□' || c == 'ㅁ' => "□".into(),
        _ => m.to_string(),
    }
}

fn head_of<'a>(re: &Regex, s: &'a str) -> Option<&'a str> {
    let c = re.captures(s)?;
    Some(&s[..c.name("la").unwrap().start()])
}

/// style_hierarchy.leading_marker(text) (scheme 없음) → (기호, 역할). 없으면 ("", "").
pub fn leading_marker(text: &str) -> (String, &'static str) {
    let stripped = lstrip(text);
    if let Some(m) = head_of(&RE_ROMAN_UP, stripped) {
        return (m.to_string(), "중제목");
    }
    let mut chars = stripped.chars();
    if let Some(f) = chars.next() {
        if "❶❷❸❹❺❻❼❽❾❿".contains(f) && chars.next().map_or(true, py_ws) {
            return ("-".into(), "내용");
        }
    }
    if let Some(m) = head_of(&RE_CIRCLED, stripped) {
        return (m.to_string(), "");
    }
    for (role, re) in [("장", &*RE_M_CHAPTER), ("중제목", &*RE_M_MID), ("소제목", &*RE_M_SUB), ("본문", &*RE_M_BODY), ("내용", &*RE_M_CONTENT), ("부연설명", &*RE_M_NOTE)] {
        if let Some(m) = head_of(re, stripped) {
            return (canonical_marker(m), role);
        }
    }
    (String::new(), "")
}

static CLASS_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (r"\d{1,2}[.．]?", "1."),
        (r"\d+\)", "1)"),
        (r"\(\d+\)[.．]?", "(1)"),
        (r"[가-하][.．]?", "가."),
        (r"[가-하]\)", "가)"),
        (r"\([가-하]\)[.．]?", "(가)"),
        (r"[ⅠⅡⅢⅣⅤⅥⅦⅧⅨⅩ]+[.．]?", "Ⅰ"),
        (r"[①-⑳]", "①"),
        (r"[㉠-㉻]", "㉠"),
        (r"[❶-❿]", "❶"),
        (r"[➊-➓]", "➊"),
    ]
    .into_iter()
    .map(|(p, n)| (Regex::new(&format!(r"\A(?:{p})\z")).unwrap(), n))
    .collect()
});

fn is_pua_circled(c: char) -> bool {
    ('\u{F02B1}'..='\u{F02C4}').contains(&c)
}

/// style_unify.marker_class
pub fn marker_class(marker: &str) -> String {
    if marker.is_empty() {
        return String::new();
    }
    let mut it = marker.chars();
    if let (Some(c), None) = (it.next(), it.next()) {
        if is_pua_circled(c) {
            return "\u{F02B1}".into();
        }
    }
    for (re, name) in CLASS_PATTERNS.iter() {
        if re.is_match(marker) {
            return (*name).to_string();
        }
    }
    marker.to_string()
}

/// style_unify.unify_marker → (그룹 기호, 역할, 실제 기호). 항목기호가 없으면 ("", "", "").
pub fn unify_marker(text: &str) -> (String, String, String) {
    let stripped = text.trim_start_matches([' ', '\t', '\u{a0}', '\u{3000}']);
    let (mut marker, role) = leading_marker(stripped);
    let mut role_s: String = role.to_string();
    if marker.is_empty() {
        let first = stripped.chars().next();
        if first.map_or(false, is_pua_circled) {
            marker = first.unwrap().to_string();
            role_s = "중제목".into();
        } else if first == Some('・') {
            marker = "•".into();
            role_s = "부연설명".into();
        } else if let Some(c) = RE_TIGHT.captures(stripped) {
            marker = c.get(1).unwrap().as_str().to_string();
            role_s = "소제목".into();
        }
    }
    if marker.is_empty() {
        return (String::new(), String::new(), String::new());
    }
    let r = if role_s.is_empty() { "원문자".to_string() } else { role_s };
    (marker_class(&marker), r, marker)
}

pub fn unify_group(text: &str) -> String {
    unify_marker(&py_strip(text).to_string()).0
}

/// connector_tabs.char_em
pub fn char_em(ch: char) -> f64 {
    let o = ch as u32;
    if ch == ' ' {
        return 0.5;
    }
    if ch == '\u{3000}'
        || (0xAC00..=0xD7A3).contains(&o)
        || (0x1100..=0x11FF).contains(&o)
        || (0x3130..=0x318F).contains(&o)
        || (0x4E00..=0x9FFF).contains(&o)
        || (0xFF01..=0xFF60).contains(&o)
        || (0x2460..=0x24FF).contains(&o)
        || (0x25A0..=0x25FF).contains(&o)
        || (0x2160..=0x217F).contains(&o)
        || (0x3000..=0x303F).contains(&o)
    {
        return 1.0;
    }
    if ".,:;·'`|!()[]{}".contains(ch) {
        return 0.33;
    }
    0.55
}

/// 파이썬 round(x)(짝수 쪽 반올림) → 정수
pub fn py_round(x: f64) -> i64 {
    x.round_ties_even() as i64
}
