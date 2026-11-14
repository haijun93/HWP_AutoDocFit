//! byte_to_utf16·utf16_to_byte가 문자열 모델과 같은 결과를 내는지, 가운데 입력 뒤 질의가 빠른지 확인한다.
use editor_core::doc::Document;
use std::time::Instant;

fn model_b2u(s: &str, pos: usize) -> usize {
    s[..pos].chars().map(char::len_utf16).sum()
}

#[test]
fn utf16_matches_model_with_gap_in_middle() {
    let base = "가나다\nabc 😀 한글\n\n😀😀\r\nlast 줄 é\n끝";
    let mut d = Document::from_text(base);
    let mut model = d.text().to_string();
    let pieces = ["x", "😀", "\n", "한\n글", "😀\n😀"];
    let mut seed = 7u64;
    for step in 0..300 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let bounds: Vec<usize> = model.char_indices().map(|(i, _)| i).chain([model.len()]).collect();
        let pos = bounds[(seed >> 33) as usize % bounds.len()];
        let s = pieces[(seed >> 20) as usize % pieces.len()];
        d.insert(pos, s).unwrap();
        model.insert_str(pos, s);
        // 갭이 가운데 있는 상태에서 질의(질의가 갭을 옮기면 안 되지만 결과는 같아야 함)
        for &b in bounds.iter().step_by(1 + step % 7) {
            let b = b.min(model.len());
            if !model.is_char_boundary(b) {
                continue;
            }
            let u = d.byte_to_utf16(b).unwrap();
            assert_eq!(u, model_b2u(&model, b), "byte_to_utf16 {step}/{b}");
            assert_eq!(d.utf16_to_byte(u).unwrap(), b, "utf16_to_byte {step}/{u}");
        }
        let total: usize = model.chars().map(char::len_utf16).sum();
        assert_eq!(d.utf16_to_byte(total).unwrap(), model.len());
        assert!(d.utf16_to_byte(total + 1).is_err());
    }
    // 대리 쌍 한가운데
    let mut d = Document::from_text("a😀b");
    assert!(d.utf16_to_byte(2).unwrap_err().contains("대리 쌍"));
    assert_eq!(d.utf16_to_byte(3).unwrap(), 5);
    assert_eq!(d.byte_to_utf16(5).unwrap(), 3);
}

#[test]
fn utf16_bench_1m_chars_middle_edit() {
    let line = "가나다라마바사 abc 😀 줄입니다\n";
    let mut text = String::new();
    while text.chars().count() < 1_000_000 {
        text.push_str(line);
    }
    let mut d = Document::from_text(&text);
    let mid = text.char_indices().map(|(i, _)| i).nth(text.chars().count() / 2).unwrap();
    let mid = text[..mid].rfind('\n').unwrap() + 1;
    let t = Instant::now();
    let mut acc = 0;
    for i in 0..200 {
        d.insert(mid + i, "x").unwrap();
        acc += d.byte_to_utf16(mid + i + 1).unwrap();
    }
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("UTF16 BENCH 100만 글자 가운데 입력+byte_to_utf16 200회: {ms:.1} ms (acc {acc})");
    let t = Instant::now();
    let u = d.byte_to_utf16(mid + 100).unwrap();
    assert_eq!(d.utf16_to_byte(u).unwrap(), mid + 100);
    println!("UTF16 BENCH utf16_to_byte 1회: {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
}
