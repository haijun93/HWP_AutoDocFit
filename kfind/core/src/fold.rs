//! 비교형(fold) 규칙. Python 기준 구현 `tools/fold_ref.py`와 글자 단위로 같다.
//!
//! 비교형 = NFC → casefold → 글자·숫자만 남김. casefold와 "글자·숫자" 판정은 Rust 표준 라이브러리 대신
//! Python `str.casefold`·`str.isalnum`에서 뽑은 고정표(`fold_table.rs`)를 쓴다(두 언어의 유니코드 표 차이를 없앰).
use crate::fold_table::{CASEFOLD, KEEP};
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfc_quick};

pub use crate::fold_table::{FOLD_RULE_VERSION, UNICODE_VERSION};

#[inline]
fn keep(c: char) -> bool {
    let cp = c as u32;
    if cp < 0x80 {
        return c.is_ascii_alphanumeric();
    }
    if (0xAC00..=0xD7A3).contains(&cp) {
        return true;
    }
    KEEP.binary_search_by(|&(a, b)| {
        if cp < a {
            std::cmp::Ordering::Greater
        } else if cp > b {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    })
    .is_ok()
}

#[inline]
fn push_folded(c: char, out: &mut String) {
    let cp = c as u32;
    if cp < 0x80 {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        }
        return;
    }
    if (0xAC00..=0xD7A3).contains(&cp) {
        out.push(c);
        return;
    }
    match CASEFOLD.binary_search_by_key(&cp, |e| e.0) {
        Ok(i) => {
            for &f in &CASEFOLD[i].1 {
                // 0은 "없음". 생성기가 유효한 코드포인트만 넣는다.
                if let Some(fc) = char::from_u32(f).filter(|_| f != 0) {
                    if keep(fc) {
                        out.push(fc);
                    }
                }
            }
        }
        Err(_) => {
            if keep(c) {
                out.push(c);
            }
        }
    }
}

/// `s`의 비교형을 `out` 뒤에 붙인다.
pub fn fold_into(s: &str, out: &mut String) {
    if s.is_ascii() || is_nfc_quick(s.chars()) == IsNormalized::Yes {
        for c in s.chars() {
            push_folded(c, out);
        }
    } else {
        for c in s.nfc() {
            push_folded(c, out);
        }
    }
}

/// 비교형: NFC → casefold → 글자·숫자만.
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    fold_into(s, &mut out);
    out
}

/// 공백(유니코드 White_Space)으로 나눈 낱말의 비교형(빈 것 제외). `fold(s) == tokens(s).concat()`.
pub fn tokens(s: &str) -> Vec<String> {
    s.split(char::is_whitespace).map(fold).filter(|f| !f.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic() {
        assert_eq!(fold("예산 집행, (계획)."), "예산집행계획");
        assert_eq!(fold("ABC_d-e"), "abcde");
        assert_eq!(fold("\u{110b}\u{1168}\u{1109}\u{1161}\u{11ab}"), "예산");
        assert_eq!(fold("Straße"), "strasse");
    }

    #[test]
    fn tokens_concat_equals_fold() {
        for s in ["예산  집행\t계획\u{3000}수립", " a b ", "Caf\u{65}\u{301} X", ""] {
            assert_eq!(tokens(s).concat(), fold(s), "{s:?}");
        }
    }

    #[test]
    fn tables_sorted() {
        assert!(KEEP.windows(2).all(|w| w[0].1 < w[1].0));
        assert!(CASEFOLD.windows(2).all(|w| w[0].0 < w[1].0));
    }
}
