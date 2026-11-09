//! 표준입력 JSON → 표준출력 JSON 위계 분석기. 외부 크레이트 없이 최소 JSON만 다룬다.
use hierarchy::{analyze_paragraphs, ParagraphKind, Role, SchemeParagraph, StructureDiagnostic};
use std::io::{Read, Write};

struct P { text: String, indent: i32, font_size: f64 }

struct Parser<'a> { b: &'a [u8], i: usize }
impl<'a> Parser<'a> {
    fn ws(&mut self) { while self.i < self.b.len() && (self.b[self.i] as char).is_ascii_whitespace() { self.i += 1; } }
    fn eat(&mut self, c: u8) -> Result<(), String> { self.ws(); if self.b.get(self.i) == Some(&c) { self.i += 1; Ok(()) } else { Err(format!("'{}' expected at {}", c as char, self.i)) } }
    fn peek(&mut self) -> Option<u8> { self.ws(); self.b.get(self.i).copied() }
    fn string(&mut self) -> Result<String, String> {
        self.eat(b'"')?;
        let mut out: Vec<u16> = Vec::new();
        let mut buf = String::new();
        loop {
            let c = *self.b.get(self.i).ok_or("unterminated string")?;
            match c {
                b'"' => { self.i += 1; break; }
                b'\\' => {
                    let e = *self.b.get(self.i + 1).ok_or("bad escape")?; self.i += 2;
                    let ch = match e { b'n' => '\n', b't' => '\t', b'r' => '\r', b'b' => '\u{8}', b'f' => '\u{c}', b'/' => '/', b'\\' => '\\', b'"' => '"',
                        b'u' => {
                            let h = std::str::from_utf8(self.b.get(self.i..self.i + 4).ok_or("bad \\u")?).map_err(|e| e.to_string())?;
                            self.i += 4;
                            out.push(u16::from_str_radix(h, 16).map_err(|e| e.to_string())?);
                            continue;
                        }
                        _ => return Err("bad escape".into()) };
                    flush(&mut out, &mut buf); buf.push(ch);
                }
                _ => {
                    flush(&mut out, &mut buf);
                    let start = self.i; self.i += 1;
                    while self.i < self.b.len() && self.b[self.i] & 0xC0 == 0x80 { self.i += 1; }
                    buf.push_str(std::str::from_utf8(&self.b[start..self.i]).map_err(|e| e.to_string())?);
                }
            }
        }
        flush(&mut out, &mut buf);
        Ok(buf)
    }
    fn number(&mut self) -> Result<f64, String> {
        self.ws(); let s = self.i;
        while self.i < self.b.len() && matches!(self.b[self.i], b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E') { self.i += 1; }
        std::str::from_utf8(&self.b[s..self.i]).unwrap().parse::<f64>().map_err(|e| e.to_string())
    }
    fn skip(&mut self) -> Result<(), String> {
        match self.peek() {
            Some(b'"') => { self.string()?; }
            Some(b'{') => { self.i += 1; if self.peek() == Some(b'}') { self.i += 1; return Ok(()); }
                loop { self.string()?; self.eat(b':')?; self.skip()?; if self.peek() == Some(b',') { self.i += 1; } else { break; } } self.eat(b'}')?; }
            Some(b'[') => { self.i += 1; if self.peek() == Some(b']') { self.i += 1; return Ok(()); }
                loop { self.skip()?; if self.peek() == Some(b',') { self.i += 1; } else { break; } } self.eat(b']')?; }
            Some(b't') => self.i += 4, Some(b'f') => self.i += 5, Some(b'n') => self.i += 4,
            _ => { self.number()?; }
        }
        Ok(())
    }
    fn paragraph(&mut self) -> Result<P, String> {
        let mut p = P { text: String::new(), indent: 0, font_size: 0.0 };
        self.eat(b'{')?;
        if self.peek() == Some(b'}') { self.i += 1; return Ok(p); }
        loop {
            let k = self.string()?; self.eat(b':')?;
            match k.as_str() {
                "text" => p.text = self.string()?,
                "indent" => p.indent = self.number()? as i32,
                "font_size" => p.font_size = self.number()?,
                _ => self.skip()?,
            }
            if self.peek() == Some(b',') { self.i += 1; } else { break; }
        }
        self.eat(b'}')?; Ok(p)
    }
    fn root(&mut self) -> Result<Vec<P>, String> {
        let mut v = Vec::new();
        self.eat(b'{')?;
        if self.peek() != Some(b'}') {
            loop {
                let k = self.string()?; self.eat(b':')?;
                if k == "paragraphs" {
                    self.eat(b'[')?;
                    if self.peek() == Some(b']') { self.i += 1; } else {
                        loop { v.push(self.paragraph()?); if self.peek() == Some(b',') { self.i += 1; } else { break; } }
                        self.eat(b']')?;
                    }
                } else { self.skip()?; }
                if self.peek() == Some(b',') { self.i += 1; } else { break; }
            }
        }
        self.eat(b'}')?; Ok(v)
    }
}
fn flush(units: &mut Vec<u16>, buf: &mut String) {
    if !units.is_empty() { buf.push_str(&String::from_utf16_lossy(units)); units.clear(); }
}
fn esc(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() { match c { '"' => o.push_str("\\\""), '\\' => o.push_str("\\\\"), '\n' => o.push_str("\\n"), c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)), c => o.push(c) } }
    o.push('"'); o
}
fn diag(d: &StructureDiagnostic) -> String {
    match d {
        StructureDiagnostic::NumberGap { paragraph, expected, found } => format!("number_gap:{}:{}:{}", paragraph, expected, found),
        StructureDiagnostic::LevelGap { paragraph, from, to } => format!("level_gap:{}:{}:{}", paragraph, from, to),
        StructureDiagnostic::OrphanLevel { paragraph, level } => format!("orphan_level:{}:{}", paragraph, level),
        StructureDiagnostic::OrderViolation { paragraph, expected_rank, found_rank } => format!("order_violation:{}:{}:{}", paragraph, expected_rank, found_rank),
        StructureDiagnostic::SameLevelDifferentFamily { rank } => format!("same_level_different_family:{}", rank),
    }
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::DocumentTitle => "제목", Role::Chapter => "장", Role::Section => "중제목",
        Role::Topic | Role::Square => "소제목", Role::Circle => "본문", Role::Dash => "내용",
        Role::Asterisk => "부연설명", Role::Body | Role::TableCell | Role::Ambiguous => "미분류",
    }
}

fn classification(role: Role, kind: ParagraphKind) -> &'static str {
    match kind {
        ParagraphKind::AttachmentHeading => "붙임제목",
        ParagraphKind::ReferenceHeading => "참고제목",
        ParagraphKind::Annotation => "부연설명",
        ParagraphKind::Heading => "제목",
        ParagraphKind::Body if role == Role::Body => "미분류",
        _ => role_name(role),
    }
}

fn run() -> Result<String, String> {
    let mut raw = Vec::new();
    std::io::stdin().read_to_end(&mut raw).map_err(|e| e.to_string())?;
    let paras = Parser { b: &raw, i: 0 }.root()?;
    let input: Vec<SchemeParagraph> = paras.iter().enumerate().map(|(i, p)| SchemeParagraph {
        text: &p.text, position: i, left_indent: p.indent,
        size_hundredth_pt: (p.font_size * 100.0).round().max(0.0) as u32, bold: false, in_attachment: false,
    }).collect();
    let out = analyze_paragraphs(&input);
    let levels: Vec<String> = out.iter().map(|a| (a.role as u8).to_string()).collect();
    let markers: Vec<String> = paras.iter().map(|p| hierarchy::read_prefix_marker(&p.text)
        .map(|m| hierarchy::canonical_marker(&p.text[..m.end_byte]).to_string()).unwrap_or_default())
        .map(|m| esc(&m)).collect();
    let classifications: Vec<String> = out.iter().map(|a| esc(classification(a.role, a.kind))).collect();
    let ambiguous: Vec<String> = out.iter().enumerate().filter(|(_, a)| a.role == Role::Ambiguous).map(|(i, _)| i.to_string()).collect();
    let confidence = out.iter().map(|a| a.confidence).min().unwrap_or(100);
    let diags: Vec<String> = out.iter().flat_map(|a| a.diagnostics.iter().map(diag)).map(|d| esc(&d)).collect();
    Ok(format!("{{\"levels\":[{}],\"markers\":[{}],\"classifications\":[{}],\"confidence\":{},\"ambiguous\":[{}],\"diagnostics\":[{}]}}", levels.join(","), markers.join(","), classifications.join(","), confidence, ambiguous.join(","), diags.join(",")))
}

fn main() {
    match run() {
        Ok(s) => { let _ = writeln!(std::io::stdout(), "{}", s); }
        Err(e) => { eprintln!("hierarchy: {}", e); std::process::exit(2); }
    }
}
