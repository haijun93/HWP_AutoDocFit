pub fn canonical_marker(marker: &str) -> String {
    if "•·‧∙⋅ㆍ●".contains(marker) && marker.chars().count() == 1 { return "•".into(); }
    if marker == "□" || marker == "ㅁ" { return "□".into(); }
    marker.into()
}
