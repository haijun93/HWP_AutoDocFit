//! `docfit_core.style_hierarchy.stored_role` 1:1 이식.
//! 화면 표시용 단계 레이블(2단계, 3단계 등)을 내부 저장용 역할 이름(장, 중제목 등)으로 변환한다.

pub fn stored_role(label: &str) -> String {
    match label {
        "1단계" => "제목",
        "2단계" => "장",
        "3단계" => "중제목",
        "4단계" => "소제목",
        "5단계" => "본문",
        "6단계" => "내용",
        "부연설명" => "부연설명",
        _ => label,
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stored_role() {
        assert_eq!(stored_role("1단계"), "제목");
        assert_eq!(stored_role("2단계"), "장");
        assert_eq!(stored_role("3단계"), "중제목");
        assert_eq!(stored_role("4단계"), "소제목");
        assert_eq!(stored_role("5단계"), "본문");
        assert_eq!(stored_role("6단계"), "내용");
        assert_eq!(stored_role("부연설명"), "부연설명");
        assert_eq!(stored_role("기타"), "기타");
        assert_eq!(stored_role("장"), "장");
        assert_eq!(stored_role("미분류"), "미분류");
        assert_eq!(stored_role(""), "");
    }
}
