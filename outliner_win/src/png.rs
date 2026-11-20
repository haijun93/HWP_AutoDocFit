//! 외부 크레이트 없는 PNG 쓰기(압축 없는 저장 블록). `ktm/src/window_capture.rs` 와 같은 방식.
use std::path::Path;

fn crc_table() -> [u32; 256] {
    std::array::from_fn(|n| {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        c
    })
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8], table: &[u32; 256]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut crc = 0xFFFF_FFFFu32;
    for b in kind.iter().chain(data.iter()) {
        crc = table[((crc ^ *b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&(crc ^ 0xFFFF_FFFF).to_be_bytes());
}

/// `bgra`: 위에서 아래로의 32비트 화소(`width*height*4` 바이트).
pub fn encode(width: usize, height: usize, bgra: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(height * (width * 3 + 1));
    for y in 0..height {
        raw.push(0);
        for x in 0..width {
            let i = (y * width + x) * 4;
            raw.extend_from_slice(&[bgra[i + 2], bgra[i + 1], bgra[i]]);
        }
    }
    let mut z = vec![0x78, 0x01];
    let mut parts = raw.chunks(65535).peekable();
    while let Some(part) = parts.next() {
        z.push(if parts.peek().is_none() { 1 } else { 0 });
        z.extend_from_slice(&(part.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(part.len() as u16)).to_le_bytes());
        z.extend_from_slice(part);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &raw {
        a = (a + *byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let table = crc_table();
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr, &table);
    chunk(&mut out, b"IDAT", &z, &table);
    chunk(&mut out, b"IEND", &[], &table);
    out
}

pub fn write(path: &Path, width: usize, height: usize, bgra: &[u8]) -> Result<(), String> {
    std::fs::write(path, encode(width, height, bgra)).map_err(|e| format!("{}: {e}", path.display()))
}
