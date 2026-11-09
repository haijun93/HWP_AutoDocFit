pub fn stored_role(label: &str) -> String {
    match label { "1단계" => "제목", "2단계" => "장", "3단계" => "중제목", "4단계" => "소제목", "5단계" => "본문", "6단계" => "내용", "부연설명" => "부연설명", _ => label }.into()
}
