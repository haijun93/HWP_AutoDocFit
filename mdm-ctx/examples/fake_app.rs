//! 시험용 가짜 앱: 받은 인자를 한 줄씩 MDM_FAKE_OUT 파일에 덧붙인다(호출 하나 = --END-- 줄로 끝남).
use std::io::Write;
fn main() {
    let out = std::env::var("MDM_FAKE_OUT").expect("MDM_FAKE_OUT");
    let mut text = String::new();
    for a in std::env::args().skip(1) { text.push_str(&a); text.push('\n'); }
    text.push_str("--END--\n");
    std::fs::OpenOptions::new().create(true).append(true).open(out).unwrap().write_all(text.as_bytes()).unwrap();
}
