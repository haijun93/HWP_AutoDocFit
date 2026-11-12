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
}
