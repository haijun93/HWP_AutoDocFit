//! 문장 나누기 대조용: 표준 입력 줄마다 16진 코드포인트 문단("C608 C0B0", 빈 글은 "-")을 받아
//! 줄마다 "시작:길이 …"(코드포인트, 문장 없으면 "-")를 쓴다. tools/sentence_ref.py --fuzz/--sqlite가 쓴다.
use std::io::{BufRead, Write};

fn main() {
    let stdin = std::io::stdin();
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for line in stdin.lock().lines() {
        let line = line.expect("stdin");
        let line = line.trim_end();
        let text: String = if line == "-" {
            String::new()
        } else {
            line.split(' ').map(|h| char::from_u32(u32::from_str_radix(h, 16).expect("hex")).expect("char")).collect()
        };
        let sp = kfind_core::split_sentences(&text);
        let s = if sp.is_empty() { "-".to_string() } else { sp.iter().map(|s| format!("{}:{}", s.start, s.len)).collect::<Vec<_>>().join(" ") };
        writeln!(out, "{s}").expect("stdout");
    }
}
