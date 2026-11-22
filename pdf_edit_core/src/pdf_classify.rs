//! PDF 사전 분류기 — Python `PdfEditService._classify`(docfit_app/pdf_edit_service.py)의 Rust 이식.
//!
//! 판정 순서와 오류 코드·한국어 메시지는 Python 원본과 동일하게 유지한다.
//! 차이가 날 수 있는 지점(pypdf strict 파서 대 lopdf 파서)은 `tests/classify_parity.rs`가
//! Python 판정 기대값(`tests/fixtures/classify/expected.json`)과 대조한다.
use lopdf::{Document, Object};
use std::path::Path;

/// 분류 거부 사유. `code`/`message`는 Python `ServiceError`와 같다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifyError {
    pub code: &'static str,
    pub message: &'static str,
}

fn err(code: &'static str, message: &'static str) -> ClassifyError {
    ClassifyError { code, message }
}

pub const MAX_PDF_BYTES: u64 = 1024 * 1024 * 1024;

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// 파일 크기 한도(1GB 초과 거부). 파일을 읽기 전에 쓸 수 있도록 분리.
pub fn check_size(len: u64) -> Result<(), ClassifyError> {
    if len > MAX_PDF_BYTES {
        return Err(err("PDF_TOO_LARGE", "1GB보다 큰 PDF는 편집할 수 없습니다."));
    }
    Ok(())
}

/// 파일 경로 분류. 통과하면 `Ok(())`.
pub fn classify_path(path: impl AsRef<Path>) -> Result<(), ClassifyError> {
    let raw = std::fs::read(path.as_ref())
        .map_err(|_| err("PDF_UNAVAILABLE", "선택한 PDF를 읽을 수 없습니다."))?;
    classify_bytes(&raw)
}

/// 바이트 분류(인메모리 파이프라인용).
pub fn classify_bytes(raw: &[u8]) -> Result<(), ClassifyError> {
    check_size(raw.len() as u64)?;
    let tail = &raw[raw.len().saturating_sub(4096)..];
    if !raw.starts_with(b"%PDF-") || !contains(tail, b"%%EOF") {
        return Err(err(
            "PDF_DAMAGED",
            "PDF 파일이 손상되었거나 PDF 형식이 아닙니다.",
        ));
    }
    if contains(raw, b"/Encrypt") && !contains(raw, b"/P") {
        return Err(err(
            "PDF_ENCRYPTED",
            "암호가 설정된 PDF는 현재 편집할 수 없습니다.",
        ));
    }
    let damaged = || err("PDF_DAMAGED", "PDF 구조를 읽을 수 없습니다. 손상된 PDF일 수 있습니다.");
    let mut doc = Document::load_mem(raw).map_err(|_| damaged())?;
    // pypdf 는 trailer 에 /Root 가 없으면 /Type /Catalog 객체를 찾아 복구한다. 같은 동작을 재현한다.
    if doc.trailer.get(b"Root").is_err() {
        let catalog = doc.objects.iter().find_map(|(id, o)| {
            (o.as_dict().ok()?.get(b"Type").ok()?.as_name().ok()? == b"Catalog").then_some(*id)
        });
        if let Some(id) = catalog {
            doc.trailer.set("Root", Object::Reference(id));
        }
    }
    let encrypt = doc.trailer.get(b"Encrypt").ok().cloned();
    if doc.is_encrypted() || encrypt.is_some() {
        let opens = encrypt
            .as_ref()
            .and_then(|e| resolve(&doc, e))
            .and_then(|e| e.as_dict().ok())
            .is_some_and(|d| empty_user_password_opens(&doc, d));
        if !opens {
            return Err(err(
                "PDF_ENCRYPTED",
                "열기 암호가 설정된 PDF는 현재 편집할 수 없습니다.",
            ));
        }
        if let Some(encrypt) = encrypt {
            let dict = match &encrypt {
                Object::Reference(id) => doc.get_object(*id).ok().and_then(|o| o.as_dict().ok()),
                other => other.as_dict().ok(),
            };
            let permissions = dict
                .and_then(|d| d.get(b"P").ok())
                .and_then(|p| match p {
                    Object::Reference(id) => doc.get_object(*id).ok(),
                    other => Some(other),
                })
                .and_then(|p| p.as_i64().ok());
            if permissions.is_some_and(|p| p != -1) {
                return Err(err(
                    "PDF_RESTRICTED",
                    "권한 제한이 설정된 PDF는 현재 편집할 수 없습니다.",
                ));
            }
        }
        return Err(err(
            "PDF_ENCRYPTED",
            "암호가 설정된 PDF는 현재 편집할 수 없습니다.",
        ));
    }
    let pages = doc.get_pages();
    if pages.is_empty() {
        return Err(err("PDF_DAMAGED", "페이지가 없는 손상 PDF입니다."));
    }
    for id in pages.values() {
        let Ok(page) = doc.get_object(*id).and_then(|o| o.as_dict()) else {
            continue;
        };
        let annots = match page.get(b"Annots") {
            Ok(Object::Reference(r)) => doc.get_object(*r).ok(),
            Ok(other) => Some(other),
            Err(_) => None,
        };
        let Some(Ok(list)) = annots.map(|a| a.as_array()) else {
            continue;
        };
        for annot in list {
            let annot = match annot {
                Object::Reference(r) => doc.get_object(*r).ok(),
                other => Some(other),
            };
            let is_sig = annot
                .and_then(|a| a.as_dict().ok())
                .and_then(|d| d.get(b"FT").ok())
                .and_then(|v| v.as_name().ok())
                == Some(b"Sig");
            if is_sig {
                return Err(err(
                    "PDF_SIGNED",
                    "전자서명된 PDF는 편집할 수 없습니다. 서명이 무효화됩니다.",
                ));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 열기 암호(빈 사용자 암호) 검증 — pypdf `reader.decrypt('')` 대응.
// lopdf 0.34 는 V1~2/R2~3(RC4)만 검증하므로 R2~R6(AES-128/256 포함)을 직접 계산한다.
// ---------------------------------------------------------------------------
use aes::cipher::{generic_array::GenericArray, BlockEncrypt, KeyInit};
use md5::{Digest as _, Md5};
use sha2::{Sha256, Sha384, Sha512};

const PAD: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut s: Vec<u8> = (0..=255).collect();
    let mut j = 0usize;
    for i in 0..256 {
        j = (j + s[i] as usize + key[i % key.len()] as usize) & 255;
        s.swap(i, j);
    }
    let (mut i, mut j) = (0usize, 0usize);
    data.iter()
        .map(|b| {
            i = (i + 1) & 255;
            j = (j + s[i] as usize) & 255;
            s.swap(i, j);
            b ^ s[(s[i] as usize + s[j] as usize) & 255]
        })
        .collect()
}

/// PDF 2.0 알고리즘 2.B (R6 해시). 빈 암호·빈 사용자 데이터 전용.
fn hash_r6(salt: &[u8]) -> Vec<u8> {
    let mut k = Sha256::digest(salt).to_vec();
    let mut round = 0usize;
    loop {
        let unit = k.clone(); // 암호(빈) + K + 사용자 데이터(빈)
        let k1: Vec<u8> = unit.iter().copied().cycle().take(unit.len() * 64).collect();
        let cipher = aes::Aes128::new(GenericArray::from_slice(&k[..16]));
        let mut prev = [0u8; 16];
        prev.copy_from_slice(&k[16..32]);
        let mut e = Vec::with_capacity(k1.len());
        for chunk in k1.chunks(16) {
            let mut block = [0u8; 16];
            for i in 0..16 {
                block[i] = chunk[i] ^ prev[i];
            }
            let mut b = GenericArray::clone_from_slice(&block);
            cipher.encrypt_block(&mut b);
            prev.copy_from_slice(&b);
            e.extend_from_slice(&b);
        }
        let m: u32 = e[..16].iter().map(|b| *b as u32).sum::<u32>() % 3;
        k = match m {
            0 => Sha256::digest(&e).to_vec(),
            1 => Sha384::digest(&e).to_vec(),
            _ => Sha512::digest(&e).to_vec(),
        };
        round += 1;
        if round >= 64 && (*e.last().unwrap() as usize) <= round - 32 {
            break;
        }
    }
    k.truncate(32);
    k
}

fn resolve<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a Object> {
    match o {
        Object::Reference(id) => doc.get_object(*id).ok(),
        other => Some(other),
    }
}

/// 빈 사용자 암호로 열 수 있는지. 판정 불가(필드 누락·미지원 리비전)이면 `false`.
fn empty_user_password_opens(doc: &Document, dict: &lopdf::Dictionary) -> bool {
    let int = |k: &[u8]| dict.get(k).ok().and_then(|o| resolve(doc, o)).and_then(|o| o.as_i64().ok());
    let bytes = |k: &[u8]| {
        dict.get(k)
            .ok()
            .and_then(|o| resolve(doc, o))
            .and_then(|o| o.as_str().ok())
            .map(|s| s.to_vec())
    };
    let (Some(r), Some(u)) = (int(b"R"), bytes(b"U")) else {
        return false;
    };
    match r {
        5 | 6 => {
            if u.len() < 48 {
                return false;
            }
            let (hash, vsalt) = (&u[..32], &u[32..40]);
            let got = if r == 5 { Sha256::digest(vsalt).to_vec() } else { hash_r6(vsalt) };
            got == hash
        }
        2..=4 => {
            let (Some(o), Some(p)) = (bytes(b"O"), int(b"P")) else {
                return false;
            };
            let id0 = doc
                .trailer
                .get(b"ID")
                .ok()
                .and_then(|o| resolve(doc, o))
                .and_then(|o| o.as_array().ok())
                .and_then(|a| a.first())
                .and_then(|o| resolve(doc, o))
                .and_then(|o| o.as_str().ok())
                .map(|s| s.to_vec())
                .unwrap_or_default();
            let key_len = if r == 2 { 5 } else { (int(b"Length").unwrap_or(40) / 8).clamp(1, 16) as usize };
            let mut buf = PAD.to_vec();
            buf.extend_from_slice(&o);
            buf.extend_from_slice(&(p as u32).to_le_bytes());
            buf.extend_from_slice(&id0);
            let metadata_encrypted = dict
                .get(b"EncryptMetadata")
                .ok()
                .and_then(|o| o.as_bool().ok())
                .unwrap_or(true);
            if r >= 4 && !metadata_encrypted {
                buf.extend_from_slice(&[0xFF; 4]);
            }
            let mut digest = Md5::digest(&buf).to_vec();
            if r >= 3 {
                for _ in 0..50 {
                    digest = Md5::digest(&digest[..key_len]).to_vec();
                }
            }
            let key = &digest[..key_len];
            if r == 2 {
                rc4(key, &PAD) == u[..u.len().min(32)]
            } else {
                let mut h = Md5::new();
                h.update(PAD);
                h.update(&id0);
                let mut enc = rc4(key, &h.finalize());
                for i in 1..=19u8 {
                    let k: Vec<u8> = key.iter().map(|b| b ^ i).collect();
                    enc = rc4(&k, &enc);
                }
                u.len() >= 16 && enc[..16] == u[..16]
            }
        }
        _ => false,
    }
}
