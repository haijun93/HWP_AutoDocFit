//! `docfit_core.style_hierarchy.display_role` 1:1 이식.
//! 내부 역할 이름(장, 중제목 등)을 화면 표시용 단계 레이블(2단계, 3단계 등)로 변환한다.

pub fn display_role(role: &str) -> String {
    match role {
        "제목" => "1단계",
        "장" => "2단계",
        "중제목" => "3단계",
        "소제목" => "4단계",
        "본문" => "5단계",
        "내용" => "6단계",
        "부연설명" => "부연설명",
        _ => role,
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_role() {
        assert_eq!(display_role("제목"), "1단계");
        assert_eq!(display_role("장"), "2단계");
        assert_eq!(display_role("중제목"), "3단계");
        assert_eq!(display_role("소제목"), "4단계");
        assert_eq!(display_role("본문"), "5단계");
        assert_eq!(display_role("내용"), "6단계");
        assert_eq!(display_role("부연설명"), "부연설명");
        assert_eq!(display_role("기타"), "기타");
        assert_eq!(display_role("1단계"), "1단계");
        assert_eq!(display_role("미분류"), "미분류");
        assert_eq!(display_role(""), "");
    }
}
