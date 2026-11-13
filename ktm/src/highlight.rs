//! 검색어 강조 범위. UTF-8 바이트 경계로 반환하므로 한글·이모지도 자르지 않는다.
pub fn ranges(text: &str, query: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    for word in query.split_whitespace().filter(|w| !w.is_empty()).take(32) {
        // Korean syllables keep their byte boundaries; ASCII case is insensitive.
        let lower = text.to_ascii_lowercase();
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
}
