//! 질의 해석: 낱말 나누기, 2글자 미만 제외, 붙여 쓴 검색어 나누기.
use crate::fold::tokens;

/// 본문 검색 낱말의 최소 글자 수(사용자 결정 Q8).
pub const MIN_WORD_CHARS: usize = 2;

/// 붙여 쓴 검색어(4글자 이상)를 낱말로 나누는 방법.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitMode {
    /// 나누지 않는다(붙어 있는 곳만 찾음).
    Off,
    /// 색인에 있는 낱말(어절)로 가장 길게 나눈다. 끝까지 나뉠 때만 적용. 기본값.
    DictLongest,
    /// 앞에서부터 2글자씩 나눈다(홀수면 마지막 조각이 3글자).
    Bigram,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryWord {
    /// 비교형 낱말.
    pub text: String,
    /// 나눈 조각(2개 이상). 없으면 붙어 있는 일치만.
    pub pieces: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedQuery {
    pub words: Vec<QueryWord>,
    /// 2글자 미만이라 뺀 낱말(화면 안내용).
    pub dropped: Vec<String>,
}

/// 공백으로 낱말을 나누고 비교형으로 바꾼다. 2글자 미만은 빼고 같은 낱말은 한 번만. 조각 나누기는 색인이 한다.
pub fn parse_query(q: &str) -> ParsedQuery {
    let mut out = ParsedQuery::default();
    for f in tokens(q) {
        if f.chars().count() < MIN_WORD_CHARS {
            if !out.dropped.contains(&f) {
                out.dropped.push(f);
            }
        } else if !out.words.iter().any(|w| w.text == f) {
            out.words.push(QueryWord { text: f, pieces: None });
        }
    }
    out
}

/// 세미콜론(`;`)으로 질의를 묶음으로 나눈다.
///
/// - 따옴표(`"…"`) 안의 `;`는 글자로 취급하며 묶음을 나누지 않는다.
/// - 각 묶음은 앞뒤 공백을 자르고, 빈 묶음은 무시한다.
/// - `;`가 없으면 전체를 하나의 묶음으로 돌려준다(비어 있으면 빈 목록).
pub fn parse_groups(q: &str) -> Vec<&str> {
    let mut groups = Vec::new();
    let mut in_quote = false;
    let mut start = 0;

    for (i, c) in q.char_indices() {
        match c {
            '"' | '“' | '”' => {
                in_quote = !in_quote;
            }
            ';' if !in_quote => {
                let piece = q[start..i].trim();
                if !piece.is_empty() {
                    groups.push(piece);
                }
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }

    let tail = q[start..].trim();
    if !tail.is_empty() {
        groups.push(tail);
    }

    groups
}

/// 구문(KFC2)을 검색 낱말로 더한다. 구문은 비교형(공백·기호를 뺀 글)이 붙어 있는 그대로 든 문장만 찾는 낱말 하나이고
/// 나누지 않는다. 같은 낱말이 이미 있으면 그 낱말의 나누기를 끈다. 비교형이 2글자 미만이면 뺀 낱말에 더하고,
/// 비교형이 빈 구문은 무시한다. 구문 문법(`"…"`)은 호출하는 쪽(앱)이 해석한다.
pub(crate) fn add_phrases(parsed: &mut ParsedQuery, phrases: &[String]) {
    for p in phrases {
        let f = crate::fold::fold(p);
        if f.is_empty() {
            continue;
        }
        if f.chars().count() < MIN_WORD_CHARS {
            if !parsed.dropped.contains(&f) {
                parsed.dropped.push(f);
            }
            continue;
        }
        match parsed.words.iter_mut().find(|w| w.text == f) {
            Some(w) => w.pieces = None,
            None => parsed.words.push(QueryWord { text: f, pieces: None }),
        }
    }
}

/// 줄기 꼬리(조사·어미) 목록 원문. 형식은 파일 머리말 참고.
const STEM_SUFFIXES_TXT: &str = include_str!("stem_suffixes.txt");

/// 붙여 쓴 검색어 나누기의 줄기 꼬리 목록: 조각 후보 p가 색인 어절로 없어도 p + 꼬리가 색인 어절이면 낱말로 본다.
pub fn stem_suffixes() -> &'static [&'static str] {
    static LIST: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    LIST.get_or_init(|| STEM_SUFFIXES_TXT.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect())
}

/// 색인 낱말 방식의 낱말 판정: 어절 그대로이거나, 어절에서 조사·어미 꼬리를 뗀 줄기.
pub(crate) fn is_word_or_stem(p: &str, mut is_word: impl FnMut(&str) -> bool) -> bool {
    if is_word(p) {
        return true;
    }
    let mut buf = String::with_capacity(p.len() + 16);
    stem_suffixes().iter().any(|s| {
        buf.clear();
        buf.push_str(p);
        buf.push_str(s);
        is_word(&buf)
    })
}

pub(crate) fn char_bounds(w: &str) -> Vec<usize> {
    w.char_indices().map(|(i, _)| i).chain([w.len()]).collect()
}

/// 2글자씩 나누기. 4글자 미만은 나누지 않는다.
pub(crate) fn split_bigram(w: &str) -> Option<Vec<String>> {
    let b = char_bounds(w);
    let n = b.len() - 1;
    if n < 2 * MIN_WORD_CHARS {
        return None;
    }
    let mut out = Vec::with_capacity(n / 2);
    let mut i = 0;
    while i < n {
        let end = if n - i == 3 { n } else { i + 2 };
        out.push(w[b[i]..b[end]].to_string());
        i = end;
    }
    Some(out)
}

struct DictSplit<'a, F: FnMut(&str) -> bool> {
    w: &'a str,
    b: Vec<usize>,
    n: usize,
    /// dead[i]: i번째 글자부터는 끝까지 나눌 수 없음이 확인됨.
    dead: Vec<bool>,
    cuts: Vec<usize>,
    is_word: F,
}

impl<F: FnMut(&str) -> bool> DictSplit<'_, F> {
    fn go(&mut self, i: usize) -> bool {
        if i == self.n {
            return true;
        }
        if self.dead[i] || self.n - i < MIN_WORD_CHARS {
            return false;
        }
        // 첫 조각은 낱말 전체가 될 수 없다(그러면 나눈 것이 아니다).
        let mut end = if i == 0 { self.n - MIN_WORD_CHARS } else { self.n };
        while end >= i + MIN_WORD_CHARS {
            // 남는 꼬리가 1글자면 나눌 수 없다.
            if (end == self.n || self.n - end >= MIN_WORD_CHARS) && (self.is_word)(&self.w[self.b[i]..self.b[end]]) {
                self.cuts.push(end);
                if self.go(end) {
                    return true;
                }
                self.cuts.pop();
            }
            end -= 1;
        }
        self.dead[i] = true;
        false
    }
}

/// 색인 낱말로 가장 길게 나누기: 앞에서부터 가장 긴 색인 낱말을 고르되, 남은 부분이 끝까지 나뉘지 않으면
/// 더 짧은 낱말로 물러난다. 조각은 2글자 이상이고 낱말 전체보다 짧아야 한다. 나뉘지 않으면 None.
pub(crate) fn split_dict(w: &str, is_word: impl FnMut(&str) -> bool) -> Option<Vec<String>> {
    let b = char_bounds(w);
    let n = b.len() - 1;
    if n < 2 * MIN_WORD_CHARS {
        return None;
    }
    let mut s = DictSplit { w, n, dead: vec![false; n + 1], cuts: Vec::new(), is_word, b };
    if !s.go(0) {
        return None;
    }
    let mut out = Vec::with_capacity(s.cuts.len());
    let mut start = 0;
    for &end in &s.cuts {
        out.push(w[s.b[start]..s.b[end]].to_string());
        start = end;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_drops_short_and_dupes() {
        let p = parse_query("예산 및 집행 예산");
        assert_eq!(p.words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), ["예산", "집행"]);
        assert_eq!(p.dropped, ["및"]);
        assert!(parse_query("ㅇ").words.is_empty());
    }

    #[test]
    fn parse_groups_splits_semicolon_and_ignores_empty() {
        assert_eq!(parse_groups("2026; 공사 계약"), ["2026", "공사 계약"]);
        assert_eq!(parse_groups("2026;; 공사 계약; "), ["2026", "공사 계약"]);
        assert_eq!(parse_groups("; ; "), Vec::<&str>::new());
        assert_eq!(parse_groups("2026"), ["2026"]);
        assert_eq!(parse_groups(""), Vec::<&str>::new());
    }

    #[test]
    fn parse_groups_preserves_semicolon_inside_quotes() {
        assert_eq!(parse_groups("2026; \"공사; 계약\""), ["2026", "\"공사; 계약\""]);
        assert_eq!(parse_groups("\"2026; 01\"; 공사; \"계약; 서식\""), ["\"2026; 01\"", "공사", "\"계약; 서식\""]);
    }

    #[test]
    fn phrases_are_unsplit_words() {
        let mut p = parse_query("예산집행 계획");
        p.words[0].pieces = Some(vec!["예산".into(), "집행".into()]);
        add_phrases(&mut p, &["예산 집행".into(), "주민·센터".into(), "및".into(), "…".into(), "계획".into(), "주민센터".into()]);
        let words: Vec<(&str, bool)> = p.words.iter().map(|w| (w.text.as_str(), w.pieces.is_some())).collect();
        // 같은 낱말은 나누기만 끄고, 새 구문은 나누지 않는 낱말로 한 번만, 기호만 있는 구문은 무시.
        assert_eq!(words, [("예산집행", false), ("계획", false), ("주민센터", false)]);
        assert_eq!(p.dropped, ["및"]);
    }

    #[test]
    fn bigram_split() {
        assert_eq!(split_bigram("예산집행").unwrap(), ["예산", "집행"]);
        assert_eq!(split_bigram("예산을집행").unwrap(), ["예산", "을집행"]);
        assert_eq!(split_bigram("예산집행계획").unwrap(), ["예산", "집행", "계획"]);
        assert!(split_bigram("예산을").is_none());
    }

    #[test]
    fn dict_split_longest_with_backtrack() {
        let dict = ["예산", "예산집", "집행", "행계획", "계획", "주민센터", "주민", "센터", "프로그램"];
        let is = |s: &str| dict.contains(&s);
        assert_eq!(split_dict("예산집행", is).unwrap(), ["예산", "집행"]);
        // 가장 긴 "예산집"을 먼저 고르고 "행계획"으로 끝난다.
        assert_eq!(split_dict("예산집행계획", is).unwrap(), ["예산집", "행계획"]);
        assert_eq!(split_dict("주민센터프로그램", is).unwrap(), ["주민센터", "프로그램"]);
        // 낱말 전체는 조각이 될 수 없다 → 더 짧게.
        assert_eq!(split_dict("주민센터", is).unwrap(), ["주민", "센터"]);
        assert!(split_dict("프로그램", is).is_none());
        assert!(split_dict("예산집", is).is_none());
        assert!(split_dict("예산을집행", is).is_none());
        // 물러나기: "예산집" 뒤 "행"이 남아 실패하면 "예산"으로.
        let dict2 = ["예산", "예산집", "집행"];
        assert_eq!(split_dict("예산집행", |s| dict2.contains(&s)).unwrap(), ["예산", "집행"]);
    }

    /// KB3: 조각 낱말이 조사·어미가 붙은 꼴로만 색인에 있어도 줄기로 알아본다.
    #[test]
    fn dict_split_accepts_stems_of_inflected_words() {
        let dict = ["예산을", "집행한다", "계획의", "주민센터"];
        let is = |s: &str| is_word_or_stem(s, |x| dict.contains(&x));
        // 어절 그대로만 보면 나뉘지 않는다(KC1 남은 문제).
        assert!(split_dict("예산집행", |s| dict.contains(&s)).is_none());
        assert_eq!(split_dict("예산집행", is).unwrap(), ["예산", "집행"]);
        assert_eq!(split_dict("예산집행계획", is).unwrap(), ["예산", "집행", "계획"]);
        // 꼬리 목록에 없는 덧붙임("센터" → "주민센터"의 앞 "주민"은 줄기가 아니다)은 받지 않는다.
        assert!(split_dict("주민예산", is).is_none());
        assert!(stem_suffixes().contains(&"을") && stem_suffixes().contains(&"한다"));
        assert!(stem_suffixes().iter().all(|s| !s.is_empty() && !s.starts_with('#') && crate::fold::fold(s) == *s));
    }
}
