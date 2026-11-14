//! CRC-32C(Castagnoli). SSE4.2 명령이 있으면 그것을, 없으면 표 방식.

const POLY: u32 = 0x82F6_3B78;

fn table() -> &'static [u32; 256] {
    use std::sync::OnceLock;
    static T: OnceLock<[u32; 256]> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { (c >> 1) ^ POLY } else { c >> 1 };
            }
            *e = c;
        }
        t
    })
}

fn soft(mut c: u32, data: &[u8]) -> u32 {
    let t = table();
    for &b in data {
        c = t[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse4.2")]
unsafe fn hw(mut c: u32, data: &[u8]) -> u32 {
    use std::arch::x86_64::{_mm_crc32_u8, _mm_crc32_u64};
    let mut chunks = data.chunks_exact(8);
    let mut c64 = c as u64;
    for ch in &mut chunks {
        let v = u64::from_le_bytes(ch.try_into().unwrap());
        c64 = _mm_crc32_u64(c64, v);
    }
    c = c64 as u32;
    for &b in chunks.remainder() {
        c = _mm_crc32_u8(c, b);
    }
    c
}

pub fn crc32c(data: &[u8]) -> u32 {
    let c = !0u32;
    #[cfg(target_arch = "x86_64")]
    {
        if std::is_x86_feature_detected!("sse4.2") {
            // SAFETY: 위에서 sse4.2를 확인했다.
            return !unsafe { hw(c, data) };
        }
    }
    !soft(c, data)
}

#[cfg(test)]
mod tests {
    #[test]
    fn known_vector() {
        // RFC 3720 B.4: "123456789" → 0xE3069283
        assert_eq!(super::crc32c(b"123456789"), 0xE306_9283);
        assert_eq!(!super::soft(!0, b"123456789"), 0xE306_9283);
        let big: Vec<u8> = (0..10_000u32).map(|i| (i * 31 % 251) as u8).collect();
        assert_eq!(super::crc32c(&big), !super::soft(!0, &big));
    }
}
