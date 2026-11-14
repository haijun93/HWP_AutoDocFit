//! 검색어 강조 범위와 일치 문장 발췌. UTF-8 바이트 경계로 반환하므로 한글·이모지도 자르지 않는다.

/// 검색어(공백으로 나눈 낱말)의 강조 범위.
pub fn ranges(text: &str, query: &str) -> Vec<(usize, usize)> {
    let words: Vec<&str> = query.split_whitespace().collect();
    ranges_terms(text, &words)
}

/// 낱말 목록(검색어 낱말 + 엔진이 나눈 조각)의 강조 범위. 겹치거나 붙은 범위는 합친다.
pub fn ranges_terms<S: AsRef<str>>(text: &str, terms: &[S]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    // Korean syllables keep their byte boundaries; ASCII case is insensitive.
    let lower = text.to_ascii_lowercase();
    for word in terms.iter().map(AsRef::as_ref).filter(|w| !w.is_empty()).take(32) {
        for (start, matched) in lower.match_indices(&word.to_ascii_lowercase()).take(256) {
            ranges.push((start, start + matched.len()));
        }
    }
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (a, b) in ranges {
        if let Some(last) = merged.last_mut().filter(|r| a <= r.1) {
            last.1 = last.1.max(b);
        } else {
            merged.push((a, b));
        }
    }
    merged
}

/// 검색어를 ';' 기준으로 쪼갠 묶음별 낱말 목록을 구한다.
pub fn parse_groups(query: &str) -> Vec<Vec<String>> {
    query
        .split(';')
        .map(|grp| {
            grp.split_whitespace()
                .filter(|w| !w.is_empty())
                .map(String::from)
                .collect()
        })
        .filter(|v: &Vec<String>| !v.is_empty())
        .collect()
}

/// ';'로 구분된 묶음별 강조 구간: `(start_byte, end_byte, group_index)`.
/// 겹치지 않는 순서 있는 구간들로 정규화된다.
pub fn ranges_by_groups<S: AsRef<str>>(
    text: &str,
    groups: &[Vec<S>],
) -> Vec<(usize, usize, usize)> {
    let mut raw_ranges = Vec::new();
    let lower = text.to_ascii_lowercase();

    for (group_idx, terms) in groups.iter().enumerate() {
        for word in terms.iter().map(AsRef::as_ref).filter(|w| !w.is_empty()).take(32) {
            for (start, matched) in lower.match_indices(&word.to_ascii_lowercase()).take(256) {
                raw_ranges.push((start, start + matched.len(), group_idx));
            }
        }
    }

    raw_ranges.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| (b.1 - b.0).cmp(&(a.1 - a.0)))
            .then_with(|| a.2.cmp(&b.2))
    });

    let mut merged: Vec<(usize, usize, usize)> = Vec::new();
    for (start, end, grp) in raw_ranges {
        if let Some(last) = merged.last_mut() {
            if start < last.1 {
                if last.2 == grp {
                    last.1 = last.1.max(end);
                } else if end > last.1 {
                    let from = last.1;
                    merged.push((from, end, grp));
                }
            } else {
                merged.push((start, end, grp));
            }
        } else {
            merged.push((start, end, grp));
        }
    }
    merged
}

/// 문단 글에서 첫 일치 앞 `before`글자부터 최대 `max`글자를 한 줄로 낸다(줄바꿈·연속 공백은 공백 하나로).
/// 앞이나 뒤를 잘랐으면 "…"를 붙인다. 일치가 없으면 문단 앞부분.
pub fn excerpt<S: AsRef<str>>(text: &str, terms: &[S], before: usize, max: usize) -> String {
    let mut flat = String::with_capacity(text.len().min(4096));
    let mut space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            space = !flat.is_empty();
        } else {
            if space {
                flat.push(' ');
                space = false;
            }
            flat.push(c);
        }
    }
    let first = ranges_terms(&flat, terms).first().map_or(0, |r| r.0);
    let chars_before = flat[..first].chars().count();
    let start_char = chars_before.saturating_sub(before);
    let start = flat.char_indices().nth(start_char).map_or(flat.len(), |(i, _)| i);
    let rest = &flat[start..];
    let end = rest.char_indices().nth(max).map_or(rest.len(), |(i, _)| i);
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.push_str(&rest[..end]);
    if end < rest.len() {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn korean_multiterm_and_overlaps() {
        assert_eq!(
            ranges("예산집행 예산", "예산 집행 예산집행"),
            vec![(0, 12), (13, 19)]
        );
    }
    #[test]
    fn ascii_case_and_emoji_boundaries() {
        assert_eq!(ranges("🙂Budget🙂", "budget"), vec![(4, 10)]);
    }
    #[test]
    fn empty_query() {
        assert!(ranges("예산", " ").is_empty());
    }
    #[test]
    fn pieces_highlight_spaced_text() {
        // "예산집행"을 엔진이 "예산"+"집행"으로 나눴을 때 띄어 쓴 문장도 강조.
        assert_eq!(ranges_terms("예산 집행", &["예산집행", "예산", "집행"]), vec![(0, 6), (7, 13)]);
    }
    #[test]
    fn groups_highlight_ranges() {
        let text = "2026년 공사 계약 집행 예산 문서";
        let groups = vec![
            vec!["2026".to_string()],
            vec!["공사".to_string(), "계약".to_string()],
            vec!["예산".to_string()],
        ];
        let ranges = ranges_by_groups(text, &groups);
        // "2026"은 group 0
        assert_eq!(ranges[0], (0, 4, 0));
        // "공사"는 group 1
        assert_eq!(&text[ranges[1].0..ranges[1].1], "공사");
        assert_eq!(ranges[1].2, 1);
        // "계약"은 group 1
        assert_eq!(&text[ranges[2].0..ranges[2].1], "계약");
        assert_eq!(ranges[2].2, 1);
        // "예산"은 group 2
        assert_eq!(&text[ranges[3].0..ranges[3].1], "예산");
        assert_eq!(ranges[3].2, 2);
    }
}
