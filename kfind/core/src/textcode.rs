//! 조각 안 비교형 글 부호(형식 v2). 조각마다 글자를 빈도순으로 번호 매기고, 번호를 자기 동기 가변 길이 바이트로 쓴다.
//!
//! - 번호 0..128: 1바이트 `0x00..0x7F`
//! - 다음 48×64개: 2바이트 `0xC0..0xEF` + 이음 1바이트
//! - 다음 8×64²개: 3바이트 `0xF0..0xF7` + 이음 2바이트
//! - 나머지 8×64³개: 4바이트 `0xF8..0xFF` + 이음 3바이트
//! - 이음 바이트는 항상 `0x80..0xBF`.
//!
//! 머리 바이트와 이음 바이트가 겹치지 않으므로(UTF-8과 같은 원리) 부호화한 낱말을 부호화한 문장에서
//! 바이트 단위로 찾으면 글자 경계에서만 맞는다. 그래서 확인은 원래처럼 `memmem` 한 번으로 끝난다.
//! 한국어 문서는 흔한 음절 128개가 1바이트, 다음 3,072개가 2바이트라 UTF-8(음절 3바이트)보다 짧다.
use std::collections::HashMap;

const ONE: u32 = 128;
const TWO: u32 = ONE + 48 * 64;
const THREE: u32 = TWO + 8 * 64 * 64;
const FOUR: u32 = THREE + 8 * 64 * 64 * 64;

/// 부호화할 수 있는 글자 번호 수(유니코드 글자 수보다 많다).
pub(crate) const MAX_CODES: u32 = FOUR;

/// 번호 하나를 바이트로 쓴다. `code < MAX_CODES`.
#[inline]
pub(crate) fn put_code(code: u32, out: &mut Vec<u8>) {
    debug_assert!(code < MAX_CODES);
    if code < ONE {
        out.push(code as u8);
    } else if code < TWO {
        let j = code - ONE;
        out.extend_from_slice(&[0xC0 + (j >> 6) as u8, 0x80 | (j & 63) as u8]);
    } else if code < THREE {
        let j = code - TWO;
        out.extend_from_slice(&[0xF0 + (j >> 12) as u8, 0x80 | ((j >> 6) & 63) as u8, 0x80 | (j & 63) as u8]);
    } else {
        let j = code - THREE;
        out.extend_from_slice(&[0xF8 + (j >> 18) as u8, 0x80 | ((j >> 12) & 63) as u8, 0x80 | ((j >> 6) & 63) as u8, 0x80 | (j & 63) as u8]);
    }
}

/// 부호화한 글을 번호로 푼다. 형식이 맞지 않으면 None.
pub(crate) fn decode_codes(b: &[u8], mut f: impl FnMut(u32) -> bool) -> Option<()> {
    let mut i = 0;
    while i < b.len() {
        let lead = b[i];
        let (n, base, mut v) = match lead {
            0x00..=0x7F => (0, 0, lead as u32),
            0xC0..=0xEF => (1, ONE, (lead - 0xC0) as u32),
            0xF0..=0xF7 => (2, TWO, (lead - 0xF0) as u32),
            0xF8..=0xFF => (3, THREE, (lead - 0xF8) as u32),
            _ => return None,
        };
        let cont = b.get(i + 1..i + 1 + n)?;
        for &c in cont {
            if c & 0xC0 != 0x80 {
                return None;
            }
            v = (v << 6) | (c & 63) as u32;
        }
        if !f(base + v) {
            return None;
        }
        i += 1 + n;
    }
    Some(())
}

/// 조각 하나의 글자 표: 번호 → 글자(빈도 많은 순), 글자 → 번호.
pub(crate) struct Alphabet {
    /// (글자, 빈도) 번호 순서.
    pub chars: Vec<(char, u64)>,
    map: HashMap<char, u32>,
}

impl Alphabet {
    /// 글자 빈도에서 표를 만든다. 빈도 많은 순, 같으면 글자 순(결정적).
    pub(crate) fn from_counts(counts: impl IntoIterator<Item = (char, u64)>) -> Alphabet {
        let mut chars: Vec<(char, u64)> = counts.into_iter().collect();
        chars.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let map = chars.iter().enumerate().map(|(i, (c, _))| (*c, i as u32)).collect();
        Alphabet { chars, map }
    }

    /// 글을 부호화해 `out` 뒤에 붙인다. 표에 없는 글자가 있으면 false(쓰는 쪽에서는 일어나지 않는다).
    pub(crate) fn encode_into(&self, s: &str, out: &mut Vec<u8>) -> bool {
        for c in s.chars() {
            match self.map.get(&c) {
                Some(&code) => put_code(code, out),
                None => return false,
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(code: u32) {
        let mut b = Vec::new();
        put_code(code, &mut b);
        let mut got = Vec::new();
        decode_codes(&b, |c| {
            got.push(c);
            true
        })
        .unwrap();
        assert_eq!(got, [code], "code {code} bytes {b:x?}");
    }

    #[test]
    fn codes_roundtrip_at_every_width_boundary() {
        for c in [0, 1, 127, 128, 129, TWO - 1, TWO, TWO + 1, THREE - 1, THREE, THREE + 1, FOUR - 1] {
            roundtrip(c);
        }
        let mut b = Vec::new();
        put_code(127, &mut b);
        put_code(128, &mut b);
        put_code(TWO, &mut b);
        put_code(THREE, &mut b);
        assert_eq!(b.len(), 1 + 2 + 3 + 4);
    }

    #[test]
    fn unicode_fits() {
        assert!(MAX_CODES > 0x10FFFF);
    }

    #[test]
    fn bad_bytes_are_rejected() {
        assert!(decode_codes(&[0x80], |_| true).is_none(), "lone continuation");
        assert!(decode_codes(&[0xC0], |_| true).is_none(), "truncated");
        assert!(decode_codes(&[0xC0, 0x41], |_| true).is_none(), "bad continuation");
    }

    /// 자기 동기: 부호화한 낱말은 부호화한 글에서 글자 경계에서만 찾아진다.
    /// 모든 번호 너비를 섞은 글에서, 바이트 찾기 결과와 글자 단위 찾기 결과가 같아야 한다.
    #[test]
    fn byte_search_matches_char_search() {
        // 번호 너비가 섞이게: 0..300은 1·2바이트, 그 뒤 몇 개는 3·4바이트.
        let mut pool: Vec<char> = ('가'..='힣').take(300).collect();
        pool.extend(['😀', '𠀀', 'a', 'b']);
        // 304글자: 앞 128개는 1바이트, 나머지는 2바이트 번호(3·4바이트는 아래 시험이 따로 본다).
        let counts: Vec<(char, u64)> = pool.iter().enumerate().map(|(i, c)| (*c, 1000 - i as u64)).collect();
        let alpha_small = Alphabet::from_counts(counts);
        assert!(alpha_small.chars.len() > ONE as usize && alpha_small.chars.len() < TWO as usize);
        let mut seed = 12345u64;
        let mut rnd = |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        for _ in 0..200 {
            let text: String = (0..60).map(|_| pool[rnd(pool.len())]).collect();
            let chars: Vec<char> = text.chars().collect();
            let mut enc = Vec::new();
            assert!(alpha_small.encode_into(&text, &mut enc));
            for _ in 0..20 {
                let a = rnd(chars.len());
                let len = 1 + rnd(4);
                let w: String = if rnd(2) == 0 && a + len <= chars.len() { chars[a..a + len].iter().collect() } else { (0..len).map(|_| pool[rnd(pool.len())]).collect() };
                let mut ew = Vec::new();
                assert!(alpha_small.encode_into(&w, &mut ew));
                let by_bytes = memchr::memmem::find(&enc, &ew).is_some();
                assert_eq!(by_bytes, text.contains(&w), "text {text} word {w}");
            }
        }
    }

    /// 높은 번호(3·4바이트)가 섞여도 경계 밖에서 맞지 않는다.
    #[test]
    fn wide_codes_do_not_match_inside_other_codes() {
        let codes = [5, 130, TWO + 77, THREE + 4097, 64, TWO, THREE];
        let mut text = Vec::new();
        for c in codes {
            put_code(c, &mut text);
        }
        for (i, &c) in codes.iter().enumerate() {
            let mut w = Vec::new();
            put_code(c, &mut w);
            let at = memchr::memmem::find_iter(&text, &w).count();
            assert_eq!(at, codes.iter().filter(|x| **x == c).count(), "code {c} index {i}");
        }
    }
}
