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

/// 고급 질의 문법 파싱 결과 (QMS-QUERY).
///
/// - 정확 구문: `"…"` / `“…”` -> `phrases`
/// - 제외어: `-낱말`, `-"…"` -> `excludes`
/// - 확장자 필터: `ext:hwp,pdf` -> `file_types`
/// - 폴더 필터: `path:폴더`, `path:"경로"` -> `folder`
/// - 날짜 필터: `after:날짜` -> `start_date`, `before:날짜` -> `end_date`
/// - 연산자·구문·제외어가 빠진 일반 검색어 -> `clean_text`, `groups`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuerySyntax {
    /// 연산자/구문/제외어가 제거된 순수 검색어 문자열 (그룹별 `; `로 결합)
    pub clean_text: String,
    /// 정확 구문 목록 ("…")
    pub phrases: Vec<String>,
    /// 제외어 목록 (-낱말, -"구문")
    pub excludes: Vec<String>,
    /// 파일 확장자 필터 (ext:hwp,pdf)
    pub file_types: Option<Vec<String>>,
    /// 폴더 필터 (path:...)
    pub folder: Option<String>,
    /// 시작 날짜 (after:...) -> YYYYMMDD i64
    pub start_date: Option<i64>,
    /// 종료 날짜 (before:...) -> YYYYMMDD i64
    pub end_date: Option<i64>,
    /// 와일드카드 질의(토큰 하나, * = 0개 이상, ? = 한 글자).
    pub wildcards: Vec<String>,
    /// 정규식은 안전한 엔진 의존성이 없어 현재 미지원. 보존용.
    pub regex_patterns: Vec<String>,
    /// 세미콜론(;)으로 분리된 각 그룹의 clean_text
    pub groups: Vec<String>,
}

impl QuerySyntax {
    /// 파싱된 문법 옵션을 기존 SearchOptions에 적용한다.
    /// phrases, file_types, folder, start_date, end_date를 반영한다.
    pub fn apply_to_options(&self, opts: &mut crate::index::SearchOptions) {
        opts.wildcards.extend(self.wildcards.iter().cloned());
        if !self.phrases.is_empty() {
            for p in &self.phrases {
                if !opts.phrases.contains(p) {
                    opts.phrases.push(p.clone());
                }
            }
        }
        if let Some(types) = &self.file_types {
            let mut current = opts.file_types.take().unwrap_or_default();
            for t in types {
                if !current.contains(t) {
                    current.push(t.clone());
                }
            }
            opts.file_types = Some(current);
        }
        if let Some(f) = &self.folder {
            opts.folder = Some(f.clone());
        }
        if let Some(s) = self.start_date {
            opts.start_date = Some(s);
        }
        if let Some(e) = self.end_date {
            opts.end_date = Some(e);
        }
    }
}

fn is_quote_open(c: char) -> bool {
    c == '"' || c == '“'
}

fn is_quote_close(c: char) -> bool {
    c == '"' || c == '”'
}

fn unescape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next == '"' || next == '\\' || next == '“' || next == '”' {
                    out.push(next);
                    chars.next();
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

fn strip_case_prefix<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s.is_char_boundary(prefix.len()) && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// 날짜 문자열(YYYY-MM-DD, YYYY/MM/DD, YYYY.MM.DD, YYYYMMDD, YYYY)을 YYYYMMDD i64로 파싱한다.
/// 유효하지 않은 경우 None을 돌려준다(파싱 에러 시 일반 낱말로 취급하기 위함).
pub fn parse_date_term(s: &str, is_after: bool) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let digits_only: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    match digits_only.len() {
        8 => {
            let val: i64 = digits_only.parse().ok()?;
            let y = val / 10000;
            let m = (val / 100) % 100;
            let d = val % 100;
            if (1900..=2100).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d) {
                Some(val)
            } else {
                None
            }
        }
        6 => {
            let val: i64 = digits_only.parse().ok()?;
            let y = val / 100;
            let m = val % 100;
            if (1900..=2100).contains(&y) && (1..=12).contains(&m) {
                if is_after {
                    Some(val * 100 + 1)
                } else {
                    Some(val * 100 + 31)
                }
            } else {
                None
            }
        }
        4 => {
            let y: i64 = digits_only.parse().ok()?;
            if (1900..=2100).contains(&y) {
                if is_after {
                    Some(y * 10000 + 101)
                } else {
                    Some(y * 10000 + 1231)
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

/// 확장자 목록 문자열(`hwp,pdf` 또는 `.hwp,.pdf`)을 소문자 목록으로 파싱한다.
pub fn parse_ext_term(s: &str) -> Option<Vec<String>> {
    let mut exts = Vec::new();
    for part in s.split(',') {
        let clean = part.trim().trim_start_matches('.').to_ascii_lowercase();
        if !clean.is_empty() && clean.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            if !exts.contains(&clean) {
                exts.push(clean);
            }
        }
    }
    if exts.is_empty() {
        None
    } else {
        Some(exts)
    }
}

fn parse_group_tokens(g: &str, syntax: &mut QuerySyntax) -> String {
    let chars: Vec<(usize, char)> = g.char_indices().collect();
    let len = g.len();
    let mut i = 0;
    let mut clean_words = Vec::new();

    while i < chars.len() {
        let (byte_idx, ch) = chars[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }

        // 1. 따옴표 구문 시작 ("...")
        if is_quote_open(ch) {
            let content_start = byte_idx + ch.len_utf8();
            let mut close_idx = None;
            let mut j = i + 1;
            let mut escaped = false;
            while j < chars.len() {
                let (_, c2) = chars[j];
                if escaped {
                    escaped = false;
                } else if c2 == '\\' {
                    escaped = true;
                } else if is_quote_close(c2) {
                    close_idx = Some(j);
                    break;
                }
                j += 1;
            }

            if let Some(c_idx) = close_idx {
                let content_end = chars[c_idx].0;
                let raw_content = &g[content_start..content_end];
                let unescaped = unescape_string(raw_content);
                let trimmed = unescaped.trim();
                if !trimmed.is_empty() {
                    syntax.phrases.push(trimmed.to_string());
                }
                i = c_idx + 1;
                continue;
            } else {
                // 닫는 따옴표 없음: 타이핑 중인 미완성 입력으로 보고 일반 텍스트로 보존
                let rest = g[content_start..].trim();
                if !rest.is_empty() {
                    clean_words.push(rest.to_string());
                }
                break;
            }
        }

        // 2. 접두사 확인
        let sub = &g[byte_idx..];

        // 2-1. 제외 구문: -"..."
        if sub.starts_with("-\"") || sub.starts_with("-“") {
            let quote_ch_len = if sub.starts_with("-\"") { 1 } else { '“'.len_utf8() };
            let content_start = byte_idx + 1 + quote_ch_len;
            let mut j = i + 2;
            let mut close_idx = None;
            let mut escaped = false;
            while j < chars.len() {
                let (_, c2) = chars[j];
                if escaped {
                    escaped = false;
                } else if c2 == '\\' {
                    escaped = true;
                } else if is_quote_close(c2) {
                    close_idx = Some(j);
                    break;
                }
                j += 1;
            }
            if let Some(c_idx) = close_idx {
                let content_end = chars[c_idx].0;
                let raw_content = &g[content_start..content_end];
                let unescaped = unescape_string(raw_content);
                let trimmed = unescaped.trim();
                if !trimmed.is_empty() {
                    syntax.excludes.push(trimmed.to_string());
                }
                i = c_idx + 1;
                continue;
            } else {
                let rest = g[content_start..].trim();
                if !rest.is_empty() {
                    clean_words.push(rest.to_string());
                }
                break;
            }
        }

        // 2-2. 폴더 구문: path:"..."
        let path_quote = if let Some(rest) = strip_case_prefix(sub, "path:") {
            if rest.starts_with('"') || rest.starts_with('“') {
                Some(rest)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(rest) = path_quote {
            let quote_ch_len = if rest.starts_with('"') { 1 } else { '“'.len_utf8() };
            let content_start = byte_idx + 5 + quote_ch_len;
            let mut j = i + 1;
            while j < chars.len() && chars[j].0 < content_start {
                j += 1;
            }
            let mut close_idx = None;
            let mut escaped = false;
            while j < chars.len() {
                let (_, c2) = chars[j];
                if escaped {
                    escaped = false;
                } else if c2 == '\\' {
                    escaped = true;
                } else if is_quote_close(c2) {
                    close_idx = Some(j);
                    break;
                }
                j += 1;
            }
            if let Some(c_idx) = close_idx {
                let content_end = chars[c_idx].0;
                let raw_content = &g[content_start..content_end];
                let unescaped = unescape_string(raw_content);
                let trimmed = unescaped.trim();
                if !trimmed.is_empty() {
                    syntax.folder = Some(trimmed.to_string());
                }
                i = c_idx + 1;
                continue;
            } else {
                let rest = g[content_start..].trim();
                if !rest.is_empty() {
                    syntax.folder = Some(rest.to_string());
                }
                break;
            }
        }

        // 3. 공백 전까지의 일반 토큰 읽기
        let mut j = i;
        let mut token_end = len;
        while j < chars.len() {
            let (pos, c2) = chars[j];
            if c2.is_whitespace() {
                token_end = pos;
                break;
            }
            j += 1;
        }
        let token = &g[byte_idx..token_end];
        i = j;

        // 토큰별 연산자 해석
        // 3-1. 제외어: -낱말
        if token.starts_with('-') && token.len() > 1 {
            let word = &token[1..];
            syntax.excludes.push(word.to_string());
            continue;
        }

        // 3-2. ext:확장자
        if let Some(rest) = strip_case_prefix(token, "ext:") {
            if let Some(exts) = parse_ext_term(rest) {
                let mut list = syntax.file_types.take().unwrap_or_default();
                for e in exts {
                    if !list.contains(&e) {
                        list.push(e);
                    }
                }
                syntax.file_types = Some(list);
                continue;
            }
        }

        // 3-3. path:경로
        if let Some(rest) = strip_case_prefix(token, "path:") {
            let trimmed = rest.trim();
            if !trimmed.is_empty() {
                syntax.folder = Some(trimmed.to_string());
                continue;
            }
        }

        // 3-4. after:날짜
        if let Some(rest) = strip_case_prefix(token, "after:") {
            if let Some(d) = parse_date_term(rest, true) {
                syntax.start_date = Some(d);
                continue;
            }
        }

        // 3-5. before:날짜
        if let Some(rest) = strip_case_prefix(token, "before:") {
            if let Some(d) = parse_date_term(rest, false) {
                syntax.end_date = Some(d);
                continue;
            }
        }

        // 3-6. 연산자에 해당하지 않거나 파싱 실패 시 일반 검색어 낱말로 취급
        if token.chars().any(|c| c == '*' || c == '?') {
            if !token.contains('\\') && !token.chars().any(|c| matches!(c, '[' | ']' | '{' | '}' | '(' | ')' | '|')) {
                syntax.wildcards.push(token.to_string());
                clean_words.push("qmswildcardtoken".to_string());
                continue;
            }
        }
        if let Some(pattern) = token.strip_prefix("re:") {
            // regex 크레이트가 의존성에 없으므로 실행하지 않고, 구문 전체를 일반 낱말로 보존한다.
            syntax.regex_patterns.push(pattern.to_string());
            clean_words.push(token.to_string());
            continue;
        }
        clean_words.push(token.to_string());
    }

        clean_words.join(" ")
}

/// 질의 문자열 전체를 고급 문법으로 파싱한다.
///
/// 기존 세미콜론(`;`) 묶음 검색과 충돌하지 않으며, 파싱 오류는 검색을 막지 않고 낱말로 안전하게 취급한다.
pub fn parse_query_syntax(raw: &str) -> QuerySyntax {
    let mut syntax = QuerySyntax::default();
    let groups = parse_groups(raw);

    if groups.is_empty() {
        return syntax;
    }

    let mut clean_groups = Vec::new();
    for g in groups {
        let clean = parse_group_tokens(g, &mut syntax);
        let trimmed = clean.trim();
        if !trimmed.is_empty() {
            clean_groups.push(trimmed.to_string());
        }
    }

    syntax.clean_text = clean_groups.join("; ");
    syntax.groups = clean_groups;
    syntax
}

#[cfg(test)]
mod wildcard_syntax_tests {
    use super::*;

    #[test]
    fn wildcard_tokens_and_fallback_table() {
        let cases = [
            ("ab*", Some("ab*")), ("*ab", Some("*ab")), ("a?b", Some("a?b")),
            ("한?", Some("한?")), ("??", Some("??")), ("a*b?c", Some("a*b?c")),
            ("*", Some("*")), ("?", Some("?")), ("re:ab+", None),
            (r"a\*b", None), ("a[b]", None), ("a{2}", None), ("a(b)", None),
            ("a|b", None), ("ext:", None), ("after:invalid", None), ("-", None),
            ("\"ab*", None), ("ab*;cd?", Some("ab*")), ("re:", None),
        ];
        for (q, expected) in cases {
            let got = parse_query_syntax(q);
            if let Some(w) = expected { assert!(got.wildcards.iter().any(|x| x == w), "{q}"); }
            else { assert!(got.wildcards.is_empty(), "{q}"); }
        }
    }
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

    struct SyntaxTestCase {
        label: &'static str,
        query: &'static str,
        clean_text: &'static str,
        phrases: &'static [&'static str],
        excludes: &'static [&'static str],
        file_types: Option<&'static [&'static str]>,
        folder: Option<&'static str>,
        start_date: Option<i64>,
        end_date: Option<i64>,
        groups: &'static [&'static str],
    }

    #[test]
    fn table_driven_query_syntax_tests() {
        let cases = [
            // 1. 기본 낱말
            SyntaxTestCase {
                label: "1. 기본 낱말",
                query: "공사 계약",
                clean_text: "공사 계약",
                phrases: &[],
                excludes: &[],
                file_types: None,
                folder: None,
                start_date: None,
                end_date: None,
                groups: &["공사 계약"],
            },
            // 2. 정확 구문 (기본 따옴표)
            SyntaxTestCase {
                label: "2. 정확 구문 (기본 따옴표)",
                query: "\"예산 집행\"",
                clean_text: "",
                phrases: &["예산 집행"],
                excludes: &[],
                file_types: None,
                folder: None,
                start_date: None,
                end_date: None,
                groups: &[],
            },
            // 3. 정확 구문 (유니코드 따옴표)
            SyntaxTestCase {
                label: "3. 정확 구문 (유니코드 따옴표)",
                query: "“공사 계약”",
                clean_text: "",
                phrases: &["공사 계약"],
                excludes: &[],
                file_types: None,
                folder: None,
                start_date: None,
                end_date: None,
                groups: &[],
            },
            // 4. 일반 낱말 + 정확 구문
            SyntaxTestCase {
                label: "4. 일반 낱말 + 정확 구문",
                query: "2026 \"예산 집행\" 계획",
                clean_text: "2026 계획",
                phrases: &["예산 집행"],
                excludes: &[],
                file_types: None,
                folder: None,
                start_date: None,
                end_date: None,
                groups: &["2026 계획"],
            },
            // 5. 제외어 단일
            SyntaxTestCase {
                label: "5. 제외어 단일",
                query: "예산 -보류",
                clean_text: "예산",
                phrases: &[],
                excludes: &["보류"],
                file_types: None,
                folder: None,
                start_date: None,
                end_date: None,
                groups: &["예산"],
            },
            // 6. 제외어 복수
            SyntaxTestCase {
                label: "6. 제외어 복수",
                query: "공사 -보류 -취소 계약",
                clean_text: "공사 계약",
                phrases: &[],
                excludes: &["보류", "취소"],
                file_types: None,
                folder: None,
                start_date: None,
                end_date: None,
                groups: &["공사 계약"],
            },
            // 7. 제외 구문
            SyntaxTestCase {
                label: "7. 제외 구문",
                query: "공사 -\"임시 보류\" 계약",
                clean_text: "공사 계약",
                phrases: &[],
                excludes: &["임시 보류"],
                file_types: None,
                folder: None,
                start_date: None,
                end_date: None,
                groups: &["공사 계약"],
            },
            // 8. 제외 구문 유니코드 따옴표
            SyntaxTestCase {
                label: "8. 제외 구문 유니코드 따옴표",
                query: "공사 -“임시 보류” 계약",
                clean_text: "공사 계약",
                phrases: &[],
                excludes: &["임시 보류"],
                file_types: None,
                folder: None,
                start_date: None,
                end_date: None,
                groups: &["공사 계약"],
            },
            // 9. 확장자 단일
            SyntaxTestCase {
                label: "9. 확장자 단일",
                query: "계약 ext:hwp",
                clean_text: "계약",
                phrases: &[],
                excludes: &[],
                file_types: Some(&["hwp"]),
                folder: None,
                start_date: None,
                end_date: None,
                groups: &["계약"],
            },
            // 10. 확장자 다중
            SyntaxTestCase {
                label: "10. 확장자 다중",
                query: "계약 ext:hwp,pdf",
                clean_text: "계약",
                phrases: &[],
                excludes: &[],
                file_types: Some(&["hwp", "pdf"]),
                folder: None,
                start_date: None,
                end_date: None,
                groups: &["계약"],
            },
            // 11. 확장자 점 접두사 및 대문자
            SyntaxTestCase {
                label: "11. 확장자 점 접두사 및 대문자",
                query: "계약 ext:.HWP,.PDF",
                clean_text: "계약",
                phrases: &[],
                excludes: &[],
                file_types: Some(&["hwp", "pdf"]),
                folder: None,
                start_date: None,
                end_date: None,
                groups: &["계약"],
            },
            // 12. 폴더 단독
            SyntaxTestCase {
                label: "12. 폴더 단독",
                query: "path:기안",
                clean_text: "",
                phrases: &[],
                excludes: &[],
                file_types: None,
                folder: Some("기안"),
                start_date: None,
                end_date: None,
                groups: &[],
            },
            // 13. 폴더 + 낱말
            SyntaxTestCase {
                label: "13. 폴더 + 낱말",
                query: "path:공문서/2026 계약",
                clean_text: "계약",
                phrases: &[],
                excludes: &[],
                file_types: None,
                folder: Some("공문서/2026"),
                start_date: None,
                end_date: None,
                groups: &["계약"],
            },
            // 14. 폴더 따옴표 (공백 포함)
            SyntaxTestCase {
                label: "14. 폴더 따옴표 (공백 포함)",
                query: "path:\"C:/My Documents/2026\" 보고서",
                clean_text: "보고서",
                phrases: &[],
                excludes: &[],
                file_types: None,
                folder: Some("C:/My Documents/2026"),
                start_date: None,
                end_date: None,
                groups: &["보고서"],
            },
            // 15. 폴더 유니코드 따옴표
            SyntaxTestCase {
                label: "15. 폴더 유니코드 따옴표",
                query: "path:“공문서 보관함” 보고서",
                clean_text: "보고서",
                phrases: &[],
                excludes: &[],
                file_types: None,
                folder: Some("공문서 보관함"),
                start_date: None,
                end_date: None,
                groups: &["보고서"],
            },
            // 16. 시작 날짜 (대시 구분)
            SyntaxTestCase {
                label: "16. 시작 날짜 (대시 구분)",
                query: "after:2026-05-01 예산",
                clean_text: "예산",
                phrases: &[],
                excludes: &[],
                file_types: None,
                folder: None,
                start_date: Some(20260501),
                end_date: None,
                groups: &["예산"],
            },
            // 17. 종료 날짜 (대시 구분)
            SyntaxTestCase {
                label: "17. 종료 날짜 (대시 구분)",
                query: "before:2026-12-31 결산",
                clean_text: "결산",
                phrases: &[],
                excludes: &[],
                file_types: None,
                folder: None,
                start_date: None,
                end_date: Some(20261231),
                groups: &["결산"],
            },
            // 18. 날짜 범위 (슬래시 및 점 구분)
            SyntaxTestCase {
                label: "18. 날짜 범위 (슬래시 및 점 구분)",
                query: "after:2026/01/01 before:2026.06.30 예산",
                clean_text: "예산",
                phrases: &[],
                excludes: &[],
                file_types: None,
                folder: None,
                start_date: Some(20260101),
                end_date: Some(20260630),
                groups: &["예산"],
            },
            // 19. 날짜 연속 숫자 8자리
            SyntaxTestCase {
                label: "19. 날짜 연속 숫자 8자리",
                query: "after:20260101 before:20261231 보고",
                clean_text: "보고",
                phrases: &[],
                excludes: &[],
                file_types: None,
                folder: None,
                start_date: Some(20260101),
                end_date: Some(20261231),
                groups: &["보고"],
            },
            // 20. 연도만 지정 (after는 1월 1일, before는 12월 31일)
            SyntaxTestCase {
                label: "20. 연도만 지정",
                query: "after:2026 before:2026 집행",
                clean_text: "집행",
                phrases: &[],
                excludes: &[],
                file_types: None,
                folder: None,
                start_date: Some(20260101),
                end_date: Some(20261231),
                groups: &["집행"],
            },
            // 21. 세미콜론 묶음 검색 기본
            SyntaxTestCase {
                label: "21. 세미콜론 묶음 검색 기본",
                query: "2026; 공사 계약",
                clean_text: "2026; 공사 계약",
                phrases: &[],
                excludes: &[],
                file_types: None,
                folder: None,
                start_date: None,
                end_date: None,
                groups: &["2026", "공사 계약"],
            },
            // 22. 세미콜론 + 연산자 복합
            SyntaxTestCase {
                label: "22. 세미콜론 + 연산자 복합",
                query: "2026; 공사 계약 ext:hwp -보류",
                clean_text: "2026; 공사 계약",
                phrases: &[],
                excludes: &["보류"],
                file_types: Some(&["hwp"]),
                folder: None,
                start_date: None,
                end_date: None,
                groups: &["2026", "공사 계약"],
            },
            // 23. 따옴표 내부 세미콜론 보호
            SyntaxTestCase {
                label: "23. 따옴표 내부 세미콜론 보호",
                query: "\"공사; 계약\"; 2026",
                clean_text: "2026",
                phrases: &["공사; 계약"],
                excludes: &[],
                file_types: None,
                folder: None,
                start_date: None,
                end_date: None,
                groups: &["2026"],
            },
            // 24. 따옴표 이스케이프
            SyntaxTestCase {
                label: "24. 따옴표 이스케이프",
                query: "\"그는 \\\"계약\\\"이라 했다\"",
                clean_text: "",
                phrases: &["그는 \"계약\"이라 했다"],
                excludes: &[],
                file_types: None,
                folder: None,
                start_date: None,
                end_date: None,
                groups: &[],
            },
            // 25. 타이핑 중 미완성 따옴표 (오류 복구)
            SyntaxTestCase {
                label: "25. 타이핑 중 미완성 따옴표 (오류 복구)",
                query: "\"예산",
                clean_text: "예산",
                phrases: &[],
                excludes: &[],
                file_types: None,
                folder: None,
                start_date: None,
                end_date: None,
                groups: &["예산"],
            },
            // 26. 타이핑 중 미완성 제외 따옴표 (오류 복구)
            SyntaxTestCase {
                label: "26. 타이핑 중 미완성 제외 따옴표 (오류 복구)",
                query: "-\"보류",
                clean_text: "보류",
                phrases: &[],
                excludes: &[],
                file_types: None,
                folder: None,
                start_date: None,
                end_date: None,
                groups: &["보류"],
            },
            // 27. 단독 하이픈 (오류 복구)
            SyntaxTestCase {
                label: "27. 단독 하이픈 (오류 복구)",
                query: "공사 - 계약",
                clean_text: "공사 - 계약",
                phrases: &[],
                excludes: &[],
                file_types: None,
                folder: None,
                start_date: None,
                end_date: None,
                groups: &["공사 - 계약"],
            },
            // 28. 잘못된 확장자 / 빈 콜론 (오류 복구)
            SyntaxTestCase {
                label: "28. 잘못된 확장자 / 빈 콜론 (오류 복구)",
                query: "ext: 공사",
                clean_text: "ext: 공사",
                phrases: &[],
                excludes: &[],
                file_types: None,
                folder: None,
                start_date: None,
                end_date: None,
                groups: &["ext: 공사"],
            },
            // 29. 잘못된 날짜 형식 (오류 복구)
            SyntaxTestCase {
                label: "29. 잘못된 날짜 형식 (오류 복구)",
                query: "after:invalid-date 계약",
                clean_text: "after:invalid-date 계약",
                phrases: &[],
                excludes: &[],
                file_types: None,
                folder: None,
                start_date: None,
                end_date: None,
                groups: &["after:invalid-date 계약"],
            },
            // 30. 한글 자모 및 조합 중 입력 보호
            SyntaxTestCase {
                label: "30. 한글 자모 및 조합 중 입력 보호",
                query: "예산 ㅎ ext:hwp",
                clean_text: "예산 ㅎ",
                phrases: &[],
                excludes: &[],
                file_types: Some(&["hwp"]),
                folder: None,
                start_date: None,
                end_date: None,
                groups: &["예산 ㅎ"],
            },
            // 31. 따옴표 내부 연산자 접두사 무시
            SyntaxTestCase {
                label: "31. 따옴표 내부 연산자 접두사 무시",
                query: "\"ext:pdf\"",
                clean_text: "",
                phrases: &["ext:pdf"],
                excludes: &[],
                file_types: None,
                folder: None,
                start_date: None,
                end_date: None,
                groups: &[],
            },
            // 32. 복합 전체 연산자 쿼리
            SyntaxTestCase {
                label: "32. 복합 전체 연산자 쿼리",
                query: "\"핵심 과제\" 2026 -임시 ext:hwp,pdf path:기안 after:2026-01-01 before:2026-12-31",
                clean_text: "2026",
                phrases: &["핵심 과제"],
                excludes: &["임시"],
                file_types: Some(&["hwp", "pdf"]),
                folder: Some("기안"),
                start_date: Some(20260101),
                end_date: Some(20261231),
                groups: &["2026"],
            },
        ];

        for case in cases {
            let res = parse_query_syntax(case.query);
            assert_eq!(res.clean_text, case.clean_text, "Failed on [{}]: clean_text", case.label);
            assert_eq!(res.phrases.as_slice(), case.phrases, "Failed on [{}]: phrases", case.label);
            assert_eq!(res.excludes.as_slice(), case.excludes, "Failed on [{}]: excludes", case.label);
            let actual_file_types = res.file_types.as_ref().map(|v| v.iter().map(|s| s.as_str()).collect::<Vec<_>>());
            let expected_file_types = case.file_types.map(|v| v.to_vec());
            assert_eq!(actual_file_types, expected_file_types, "Failed on [{}]: file_types", case.label);
            assert_eq!(res.folder.as_deref(), case.folder, "Failed on [{}]: folder", case.label);
            assert_eq!(res.start_date, case.start_date, "Failed on [{}]: start_date", case.label);
            assert_eq!(res.end_date, case.end_date, "Failed on [{}]: end_date", case.label);
            assert_eq!(res.groups.as_slice(), case.groups, "Failed on [{}]: groups", case.label);
        }
    }

    #[test]
    fn query_syntax_apply_to_search_options() {
        let syntax = parse_query_syntax("\"주민 센터\" 2026 ext:hwp path:보고서 after:2026-01-01 before:2026-12-31");
        let mut opts = crate::index::SearchOptions::default();
        syntax.apply_to_options(&mut opts);

        assert_eq!(opts.phrases, vec!["주민 센터"]);
        assert_eq!(opts.file_types, Some(vec!["hwp".into()]));
        assert_eq!(opts.folder, Some("보고서".into()));
        assert_eq!(opts.start_date, Some(20260101));
        assert_eq!(opts.end_date, Some(20261231));
    }
}
