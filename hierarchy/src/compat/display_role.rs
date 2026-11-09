pub fn display_role(role: &str) -> String {
    match role { "제목" => "1단계", "장" => "2단계", "중제목" => "3단계", "소제목" => "4단계", "본문" => "5단계", "내용" => "6단계", "부연설명" => "부연설명", _ => role }.into()
}
