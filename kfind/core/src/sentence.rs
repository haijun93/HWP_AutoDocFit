//! 문장 나누기(규칙 판 `SENTENCE_RULE_VERSION`). 문단 하나를 문장 구간들로 나눈다.
//!
//! Python 기준 구현 `tools/sentence_ref.py`와 결과가 같아야 한다(대조 표 `tests/data/sentence_cases.tsv`).
//! 규칙을 바꾸면 양쪽 판 번호를 함께 올리고 표를 다시 만든다.
//!
//! 규칙:
//! 1. 줄바꿈(`\n` `\r` U+000B U+000C U+0085 U+2028 U+2029, `\r\n`은 하나)은 항상 경계.
//! 2. 끝맺음 글자(`.` `?` `!` `。` `．` `？` `！`) 묶음 + 바로 뒤 닫는 글자(따옴표·괄호) 묶음 다음이 공백이거나 글 끝이면 그 뒤가 경계.
//! 3. 묶음이 마침표(`.` `．`)뿐일 때 예외(경계 아님): ① 마침표 2개 이상(말줄임)
//!    ② 마침표 하나이고 바로 앞이 아라비아 숫자("2026. 11. 12.", "1. 개요")
//!    ③ 마침표 하나이고 그 줄 처음부터가 목록 표지(앞 공백·여는 괄호 하나 뺀 한글 음절 1자, 영문자 1~4자, 로마 숫자 글자 1~4자).
//! 4. 문장은 앞뒤 공백(유니코드 White_Space)을 뺀 구간이고 빈 문장은 버린다. 기호만 있는 문장은 남긴다.
//! 5. 위치는 문단 안 코드포인트(Python 문자열 색인과 같음)와 UTF-8 바이트 둘 다 준다.

/// 문장 나누기 규칙 판. 바뀌면 문장 색인을 다시 만들어야 한다.
pub const SENTENCE_RULE_VERSION: u32 = 1;

/// 문단 안 문장 하나의 위치.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SentenceSpan {
    /// 시작 위치(코드포인트).
    pub start: u32,
    /// 길이(코드포인트).
    pub len: u32,
    /// 시작 바이트(UTF-8).
    pub byte_start: usize,
    /// 끝 바이트(UTF-8, 포함 안 함).
    pub byte_end: usize,
}

impl SentenceSpan {
    /// 문단 원문에서 이 문장 글을 잘라 낸다(`para`는 나눌 때 준 그 글이어야 한다).
    pub fn text<'a>(&self, para: &'a str) -> &'a str {
        &para[self.byte_start..self.byte_end]
    }
}

fn is_line_break(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

fn is_period(c: char) -> bool {
    matches!(c, '.' | '\u{FF0E}')
}

fn is_terminator(c: char) -> bool {
    matches!(c, '.' | '?' | '!' | '\u{3002}' | '\u{FF0E}' | '\u{FF1F}' | '\u{FF01}')
}

fn is_closer(c: char) -> bool {
    matches!(
        c,
        '"' | '\'' | ')' | ']' | '\u{2019}' | '\u{201D}' | '\u{300D}' | '\u{300F}' | '\u{3009}' | '\u{300B}' | '\u{3011}' | '\u{FF09}' | '\u{FF3D}'
    )
}

fn is_opener(c: char) -> bool {
    matches!(c, '(' | '[' | '\u{FF08}' | '\u{FF3B}')
}

/// `chars[line..end]`가 목록 표지(마침표 앞 부분)인가.
fn is_marker(chars: &[(usize, char)], line: usize, end: usize) -> bool {
    let mut i = line;
    while i < end && chars[i].1.is_whitespace() {
        i += 1;
    }
    if i < end && is_opener(chars[i].1) {
        i += 1;
    }
    let body = &chars[i..end];
    let n = body.len();
    if n == 0 {
        return false;
    }
    if n == 1 && ('\u{AC00}'..='\u{D7A3}').contains(&body[0].1) {
        return true;
    }
    if n <= 4 && body.iter().all(|&(_, c)| c.is_ascii_alphabetic()) {
        return true;
    }
    n <= 4 && body.iter().all(|&(_, c)| ('\u{2160}'..='\u{217F}').contains(&c))
}

/// 문단 → 문장 구간들(앞에서부터 차례로, 겹치지 않음).
pub fn split_sentences(text: &str) -> Vec<SentenceSpan> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    let byte_at = |i: usize| if i < n { chars[i].0 } else { text.len() };
    let mut out = Vec::new();
    let mut emit = |mut a: usize, mut b: usize| {
        while a < b && chars[a].1.is_whitespace() {
            a += 1;
        }
        while b > a && chars[b - 1].1.is_whitespace() {
            b -= 1;
        }
        if a < b {
            out.push(SentenceSpan { start: a as u32, len: (b - a) as u32, byte_start: byte_at(a), byte_end: byte_at(b) });
        }
    };
    let (mut seg, mut line, mut i) = (0usize, 0usize, 0usize);
    while i < n {
        let c = chars[i].1;
        if is_line_break(c) {
            emit(seg, i);
            let mut j = i + 1;
            if c == '\r' && j < n && chars[j].1 == '\n' {
                j += 1;
            }
            (seg, line, i) = (j, j, j);
            continue;
        }
        if is_terminator(c) {
            let mut j = i;
            while j < n && is_terminator(chars[j].1) {
                j += 1;
            }
            let mut k = j;
            while k < n && is_closer(chars[k].1) {
                k += 1;
            }
            if k == n || chars[k].1.is_whitespace() {
                let exempt = chars[i..j].iter().all(|&(_, r)| is_period(r))
                    && (j - i >= 2 || (i > 0 && chars[i - 1].1.is_ascii_digit()) || is_marker(&chars, line, i));
                if !exempt {
                    emit(seg, k);
                    seg = k;
                }
            }
            i = k;
            continue;
        }
        i += 1;
    }
    emit(seg, n);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(s: &str) -> Vec<&str> {
        split_sentences(s).iter().map(|sp| sp.text(s)).collect()
    }

    #[test]
    fn basic_rules() {
        assert_eq!(texts("예산을 집행한다. 계획을 세운다."), ["예산을 집행한다.", "계획을 세운다."]);
        assert_eq!(texts("2026. 11. 12. 회의를 연다. 참석."), ["2026. 11. 12. 회의를 연다.", "참석."]);
        assert_eq!(texts("가. 사업 개요\r\n나. 목적"), ["가. 사업 개요", "나. 목적"]);
        assert_eq!(texts("검토 중... 결과"), ["검토 중... 결과"]);
        assert_eq!(texts("그는 “좋다.” 고 했다."), ["그는 “좋다.”", "고 했다."]);
        assert!(split_sentences("  \n\r\n ").is_empty());
    }

    #[test]
    fn spans_are_codepoints_and_bytes() {
        let s = "😀 이모지. 다음";
        let sp = split_sentences(s);
        assert_eq!((sp[1].start, sp[1].len), (7, 2));
        assert_eq!(sp[1].text(s), "다음");
        assert_eq!(&s[sp[0].byte_start..sp[0].byte_end], "😀 이모지.");
    }
}
