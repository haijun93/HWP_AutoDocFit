//! 사진 태그·검색을 위한 플랫폼 독립 엔진 규칙.

/// 태그 목록을 정리한다. 공백 태그를 버리고 앞뒤 공백을 제거하며 중복을 제거한다.
pub fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut normalized: Vec<String> = tags
        .iter()
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
        .map(str::to_owned)
        .collect();
    normalized.sort();
    normalized.dedup();
    normalized
}

/// 사진 메타데이터에 사진뷰 검색어가 일치하는지 확인한다.
/// 검색어는 파일명·경로·태그에서 부분 문자열로, 촬영일에서 날짜 접두사로 찾는다.
pub fn matches_search(filename: &str, path: &str, date_taken: &str, tags: &[String], query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    filename.to_lowercase().contains(&query)
        || path.to_lowercase().contains(&query)
        || date_taken.to_lowercase().contains(&query)
        || tags.iter().any(|tag| tag.to_lowercase().contains(&query))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_trim_drop_empty_and_deduplicate() {
        let input = vec![" 풍경 ".into(), "".into(), "여행".into(), "풍경".into(), "  ".into()];
        assert_eq!(normalize_tags(&input), vec!["여행", "풍경"]);
    }

    #[test]
    fn search_matches_filename_date_and_tag_case_insensitively() {
        let tags = vec!["현장 점검".to_string()];
        assert!(matches_search("IMG_100.JPG", "D:/photos/IMG_100.JPG", "2026-08-15 12:00:00", &tags, "img_100"));
        assert!(matches_search("IMG_100.JPG", "D:/photos/IMG_100.JPG", "2026-08-15 12:00:00", &tags, "2026-08"));
        assert!(matches_search("IMG_100.JPG", "D:/photos/IMG_100.JPG", "2026-08-15 12:00:00", &tags, "점검"));
        assert!(!matches_search("IMG_100.JPG", "D:/photos/IMG_100.JPG", "2026-08-15 12:00:00", &tags, "휴가"));
    }

    #[test]
    fn empty_search_matches_every_photo() {
        assert!(matches_search("a.jpg", "photos/a.jpg", "", &[], "  "));
    }
}
