//! 3단계(큰 글 보기·인코딩 선택 저장) 시험. 성능: `cargo test --release --test bigview -- --nocapture`
use editor_core::api::Engine;
use editor_core::doc::Document;
use editor_core::fileio::{Encoding, LineEnding};
use std::time::Instant;

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("editor_core_big_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn line_range_and_char_conversions_match_model() {
    let text = "가나다\nabc\n\n😀x한\n끝";
    let mut d = Document::from_text(text);
    let r = d.line_range(1, 3);
    assert_eq!(r.total_lines, 5);
    assert_eq!(r.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["abc", "", "😀x한"]);
    assert_eq!(r.lines[0].line, 1);
    assert_eq!(r.lines[0].start, "가나다\n".len());
    assert_eq!(r.lines[0].start_char, 4);
    assert_eq!(r.lines[2].start_char, 9);
    // 범위 밖은 잘려 나온다
    assert_eq!(d.line_range(4, 100).lines.len(), 1);
    assert!(d.line_range(99, 5).lines.is_empty());
    assert_eq!(d.line_range(0, 0).lines.len(), 0);
    // 모든 글자 위치에서 왕복 + 모델 대조
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (ci, &(byte, _)) in chars.iter().enumerate().chain([(chars.len(), &(text.len(), ' '))]) {
        assert_eq!(d.byte_to_char(byte).unwrap(), ci);
        assert_eq!(d.char_to_byte(ci).unwrap(), byte);
        let (l, c) = d.char_to_line_col(ci).unwrap();
        assert_eq!((l, c), d.line_col(byte).unwrap());
        assert_eq!(d.line_col_to_char(l, c), ci);
    }
    assert!(d.char_to_byte(chars.len() + 1).is_err());
    assert_eq!(d.char_count(), chars.len());
    // 편집 뒤에도 캐시가 맞다 (앞·가운데·끝, 줄바꿈 삽입/삭제)
    d.insert(0, "맨앞\n").unwrap();
    assert_eq!(d.char_to_line_col(3).unwrap(), (2, 1));
    let p = d.char_to_byte(8).unwrap();
    d.delete(p, p + 4).unwrap();
    let n = d.char_count();
    let end = d.len();
    d.insert(end, "\n꼬리").unwrap();
    let model = d.text().to_string();
    for (ci, (byte, _)) in model.char_indices().enumerate() {
        assert_eq!(d.byte_to_char(byte).unwrap(), ci);
        assert_eq!(d.char_to_byte(ci).unwrap(), byte);
    }
    assert_eq!(d.char_count(), n + 3);
}

#[test]
fn random_edits_keep_char_cache_consistent() {
    let mut seed = 7u64;
    let mut rnd = move |m: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize % m.max(1)
    };
    let mut d = Document::from_text("");
    for _ in 0..3000 {
        let n = d.char_count();
        let a = d.char_to_byte(rnd(n + 1)).unwrap();
        match rnd(4) {
            0 => { let b = d.char_to_byte((d.byte_to_char(a).unwrap() + rnd(5)).min(n)).unwrap(); d.delete(a, b).unwrap(); }
            _ => { d.insert(a, ["x", "\n", "한글\n", "😀"][rnd(4)]).unwrap(); }
        }
        let n = d.char_count();
        let ci = rnd(n + 1);
        let b = d.char_to_byte(ci).unwrap();
        assert_eq!(d.text()[..b].chars().count(), ci);
        assert_eq!(d.byte_to_char(b).unwrap(), ci);
    }
}

#[test]
fn save_as_chooses_encoding_and_line_ending() {
    let dir = tmp("save");
    let mut d = Document::from_text("가a\nb");
    for (enc, le, expect) in [
        (Encoding::Utf8, LineEnding::Lf, b"\xea\xb0\x80a\nb".to_vec()),
        (Encoding::Utf8Bom, LineEnding::Crlf, b"\xef\xbb\xbf\xea\xb0\x80a\r\nb".to_vec()),
        (Encoding::Utf16Le, LineEnding::Crlf, vec![0xFF, 0xFE, 0x00, 0xAC, b'a', 0, 13, 0, 10, 0, b'b', 0]),
        (Encoding::Cp949, LineEnding::Lf, b"\xb0\xa1a\nb".to_vec()),
    ] {
        let p = dir.join("m.txt");
        d.save_as(&p, enc, le).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), expect, "{}", enc.name());
        assert_eq!((d.encoding, d.line_ending, d.is_dirty()), (enc, le, false));
        let back = Document::open(&p).unwrap();
        assert_eq!(back.encoding, if enc == Encoding::Cp949 { Encoding::Cp949 } else { enc });
        assert_eq!(back.line_ending, le);
    }
}

#[test]
fn unencodable_list_and_failed_save_changes_nothing() {
    let dir = tmp("bad");
    let mut d = Document::from_text("ok\n한😀x\n😀‽😀");
    let r = d.unencodable(Encoding::Cp949, 2);
    assert_eq!(r.total, 4); // 😀 3개 + ‽(U+203D) 1개
    let all = d.unencodable(Encoding::Cp949, 100);
    let em = all.first.iter().filter(|u| u.ch == '😀').count();
    assert_eq!(em, 3);
    assert_eq!(all.distinct, vec![('😀', 3), ('‽', 1)]);
    assert_eq!(all.first[0], editor_core::doc::Unencodable { ch: '😀', byte: "ok\n한".len(), char_index: 4, line: 2, col: 2 });
    assert_eq!(r.first.len(), 2); // limit 적용, total은 전체
    assert!(d.unencodable(Encoding::Utf16Le, 10).first.is_empty());
    // 실패하면 파일·설정 그대로
    let p = dir.join("a.txt");
    std::fs::write(&p, b"old").unwrap();
    let e = d.save_as(&p, Encoding::Cp949, LineEnding::Lf).unwrap_err();
    assert!(e.contains("😀") && e.contains("2줄 2열"), "{e}");
    assert_eq!(std::fs::read(&p).unwrap(), b"old");
    assert_eq!(d.encoding, Encoding::Utf8);
    assert!(d.is_dirty() || d.path.is_none());
}

#[test]
fn json_api_big_view_and_save() {
    let dir = tmp("json");
    let mut e = Engine::new();
    let r = e.handle(r#"{"cmd":"edit_open","text":"가\nb😀\nc"}"#);
    assert!(r.contains(r#""doc":1"#), "{r}");
    let r = e.handle(r#"{"cmd":"edit_line_range","doc":1,"start_line":1,"count":5}"#);
    assert!(r.contains(r#""total_lines":3"#) && r.contains(r#""start_char":2"#) && r.contains("b😀"), "{r}");
    let r = e.handle(r#"{"cmd":"edit_char_to_line_col","doc":1,"char":4}"#);
    assert!(r.contains(r#""line":2"#) && r.contains(r#""col":3"#), "{r}");
    let r = e.handle(r#"{"cmd":"edit_line_col_to_char","doc":1,"line":3,"col":1}"#);
    assert!(r.contains(r#""char":5"#), "{r}");
    let r = e.handle(r#"{"cmd":"edit_unencodable","doc":1,"encoding":"CP949"}"#);
    assert!(r.contains(r#""ok_to_save":false"#) && r.contains("U+1F600") && r.contains(r#""total":1"#), "{r}");
    let p = dir.join("x.txt");
    let bad = format!(r#"{{"cmd":"edit_save","doc":1,"path":{:?},"encoding":"CP949"}}"#, p.to_str().unwrap());
    let r = e.handle(&bad);
    assert!(r.contains(r#""ok":false"#) && !p.exists(), "{r}");
    let good = format!(r#"{{"cmd":"edit_save","doc":1,"path":{:?},"encoding":"UTF-16 LE","line_ending":"LF"}}"#, p.to_str().unwrap());
    let r = e.handle(&good);
    assert!(r.contains(r#""encoding":"UTF-16 LE""#) && r.contains(r#""line_ending":"LF""#), "{r}");
    assert_eq!(&std::fs::read(&p).unwrap()[..2], &[0xFF, 0xFE]);
}

const N: usize = 1_000_000;

#[test]
fn perf_million_lines_range() {
    let text: String = (0..N).map(|i| format!("{i}번째 줄 가나다 abc\n")).collect();
    let t = Instant::now();
    let mut d = Document::from_text(&text);
    println!("ROW|100만 줄 열기(내부 LF 변환·색인)|{:.1} ms", t.elapsed().as_secs_f64() * 1e3);
    assert_eq!(d.line_count(), N + 1);
    let med = |mut v: Vec<f64>| { v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[v.len() / 2] };
    // 줄 범위 조회 50줄, 시작 줄을 흩뿌려 100회
    let mut v = vec![];
    for k in 0..100 {
        let s = (k * 9973 * 7) % (N - 60);
        let t = Instant::now();
        let r = d.line_range(s, 50);
        v.push(t.elapsed().as_secs_f64() * 1e6);
        assert_eq!(r.lines.len(), 50);
        assert!(r.lines[0].text.starts_with(&format!("{s}번째")));
    }
    println!("ROW|줄 범위 50줄 조회(첫 호출은 글자 캐시 채움 포함, 중앙값)|{:.1} µs|최대 {:.1} µs", med(v.clone()), v.iter().cloned().fold(0.0, f64::max));
    // 변환(캐시가 채워진 뒤)
    let mut v = vec![];
    for k in 0..1000 {
        let ci = (k * 7919 * 13) % d.char_count();
        let t = Instant::now();
        let (l, c) = d.char_to_line_col(ci).unwrap();
        let back = d.line_col_to_char(l, c);
        v.push(t.elapsed().as_secs_f64() * 1e6);
        assert_eq!(back, ci);
    }
    println!("ROW|글자→(줄,열)→글자 왕복(중앙값)|{:.2} µs|최대 {:.1} µs", med(v.clone()), v.iter().cloned().fold(0.0, f64::max));
    // 가운데 편집 직후 첫 질의(캐시 뒤쪽 재계산) / 끝 근처 편집 직후
    let mid = d.offset_of(N / 2, 1);
    d.insert(mid, "X").unwrap();
    let t = Instant::now();
    let _ = d.char_to_line_col(d.char_count() - 1).unwrap();
    println!("ROW|가운데(50만 줄) 편집 직후 끝 글자 질의(뒤쪽 캐시 재계산)|{:.1} ms", t.elapsed().as_secs_f64() * 1e3);
    let end = d.len();
    d.insert(end, "Y").unwrap();
    let t = Instant::now();
    let _ = d.char_to_line_col(d.char_count() - 1).unwrap();
    println!("ROW|끝에 입력 직후 끝 글자 질의|{:.1} µs", t.elapsed().as_secs_f64() * 1e6);
    // 저장 (100만 줄)
    let t = Instant::now();
    let b = d.to_bytes_with(Encoding::Cp949, LineEnding::Crlf).unwrap();
    println!("ROW|CP949+CRLF 저장 바이트 생성(변환불가 점검 포함)|{:.1} ms|{} 바이트", t.elapsed().as_secs_f64() * 1e3, b.len());
    let t = Instant::now();
    let b = d.to_bytes_with(Encoding::Utf16Le, LineEnding::Lf).unwrap();
    println!("ROW|UTF-16 LE+LF 저장 바이트 생성|{:.1} ms|{} 바이트", t.elapsed().as_secs_f64() * 1e3, b.len());
}
