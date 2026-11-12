//! 조각(세그먼트) 파일: 한 번 쓰면 바뀌지 않는다. 임시 이름으로 다 쓴 뒤 rename으로 제자리에 놓고 mmap으로 읽는다.
//!
//! 파일 배치(모두 리틀 엔디언):
//! ```text
//! 머리글 216바이트
//!   0  magic "KFSEG\r\n\x1a"      8  형식 버전 u32     12  비교형 규칙 버전 u32
//!   16 조각 번호 u64              24 문장 수 u32       28  문서 수 u32
//!   32 본문 해시 u64(FNV-1a 64, 머리글 뒤 전체)        40  파일 길이 u64
//!   48 구역 표 10 × (위치 u64, 길이 u64)               208 머리글 해시 u64(0..208)
//! 구역
//!   POST     조각·낱말별 문장 번호 roaring 비트맵(표준 직렬화)을 이어 붙임
//!   DICT     fst: 열쇠 → POST 안 위치. 열쇠 = 2·3글자 조각의 UTF-8, 또는 0x00 + 낱말(어절) UTF-8
//!   TEXT     문장 비교형을 조각 글자 표 번호로 부호화해 이어 붙임(4글자 이상 낱말 확인용, `textcode` 참고)
//!   TEXT_OFF (문장 수 + 1) × u32: 문장의 TEXT 안 시작 위치
//!   PARA     문장 수 × u32: 문단 번호
//!   LEVEL    문장 수 × u8: 위계 순위(작을수록 높음)
//!   DOC_OF   문장 수 × u32: 조각 안 문서 번호
//!   DOCS     문서 수 × (문서 ID u64, 날짜 i64, 첫 문장 번호 u32, 문장 수 u32)
//!   ALPHA    글자 수 × (글자 u32, 빈도 u64): 번호 순서(빈도 많은 순). 합칠 때 빈도를 더해 새 표를 만든다.
//!   ALPHA_IX 글자 수 × (글자 u32, 번호 u32): 글자 오름차순(검색어 부호화용 이진 탐색, 열 때 표를 만들지 않는다)
//! ```
//! 형식 v1(KC1)과 다른 점: TEXT를 UTF-8 대신 조각 글자 표 부호로, TEXT_OFF를 u64 대신 u32로 쓴다(KB2, 디스크 축소).
//! 한 문서의 문장은 조각 안에서 이어져 있고, 문서 안 순번 = 문장 번호 - 첫 문장 번호.
use crate::fold::FOLD_RULE_VERSION;
use crate::textcode::{Alphabet, MAX_CODES, decode_codes, put_code};
use crate::{Error, FNV_SEED, Result, corrupt, fnv64};
use memmap2::Mmap;
use roaring::RoaringBitmap;
use std::collections::HashMap;
use std::fs::File;
use std::hash::{BuildHasherDefault, Hasher};
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const FORMAT_VERSION: u32 = 2;
const MAGIC: &[u8; 8] = b"KFSEG\r\n\x1a";
const HEADER_LEN: usize = 216;
const HEADER_HASH: usize = HEADER_LEN - 8;
const N_SECTIONS: usize = 10;
const POST: usize = 0;
const DICT: usize = 1;
const TEXT: usize = 2;
const TEXT_OFF: usize = 3;
const PARA: usize = 4;
const LEVEL: usize = 5;
const DOC_OF: usize = 6;
const DOCS: usize = 7;
const ALPHA: usize = 8;
const ALPHA_IX: usize = 9;
const DOC_REC: usize = 24;
const ALPHA_REC: usize = 12;
const ALPHA_IX_REC: usize = 8;
/// 낱말(어절) 사전에 넣는 낱말의 최대 길이(바이트). 더 긴 어절은 나누기 후보가 되지 않는다.
pub(crate) const MAX_WORD_BYTES: usize = 96;
/// 낱말 열쇠의 머리 바이트. 비교형 글에는 0x00이 없어 조각 열쇠와 겹치지 않는다.
pub(crate) const WORD_PREFIX: u8 = 0;

pub(crate) fn seg_file_name(id: u64) -> String {
    format!("seg-{id:08}.kfs")
}

/// 조각 열쇠: 낱말(어절) 사전 열쇠.
pub(crate) fn word_key(w: &str) -> Vec<u8> {
    let mut k = Vec::with_capacity(w.len() + 1);
    k.push(WORD_PREFIX);
    k.extend_from_slice(w.as_bytes());
    k
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DocRec {
    pub doc_id: u64,
    pub date: i64,
    pub first_sid: u32,
    pub n_sent: u32,
}

/// 아직 조각에 쓰지 않은 문서 하나(비교형으로 바꾼 상태).
pub(crate) struct PendingDoc {
    pub doc_id: u64,
    pub date: i64,
    /// 문장 비교형을 이어 붙인 글.
    pub text: String,
    /// 문장마다 끝 위치(text 안).
    pub sent_end: Vec<u32>,
    /// 낱말(어절) 시작 위치(text 안, 오름차순). 낱말은 다음 시작 또는 문장 끝에서 끝난다.
    pub tok_start: Vec<u32>,
    pub para: Vec<u32>,
    pub level: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SegMeta {
    pub id: u64,
    pub n_sent: u32,
    pub n_docs: u32,
    pub body_hash: u64,
}

#[derive(Default)]
struct Fx(u64);

impl Hasher for Fx {
    fn finish(&self) -> u64 {
        self.0 ^ (self.0 >> 32)
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.write_u64(*b as u64);
        }
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(5) ^ v).wrapping_mul(0x517cc1b727220a95);
    }
}

type FxMap<K, V> = HashMap<K, V, BuildHasherDefault<Fx>>;

/// 조각 파일을 순서대로 쓴다: 비트맵(열쇠 오름차순) → 문장 → 문서 → 머리글.
struct SegWriter {
    tmp: PathBuf,
    out: BufWriter<File>,
    hash: u64,
    pos: u64,
    sec: [(u64, u64); N_SECTIONS],
    dict: Option<fst::MapBuilder<Vec<u8>>>,
    text_off: Vec<u32>,
    para: Vec<u32>,
    level: Vec<u8>,
    doc_of: Vec<u32>,
    buf: Vec<u8>,
    alpha: Alphabet,
}

impl SegWriter {
    fn create(tmp: &Path, alpha: Alphabet) -> Result<SegWriter> {
        if alpha.chars.len() as u64 >= MAX_CODES as u64 {
            return Err(Error::Invalid("too many distinct characters in one segment".into()));
        }
        let mut out = BufWriter::with_capacity(1 << 20, File::create(tmp)?);
        out.write_all(&[0u8; HEADER_LEN])?;
        Ok(SegWriter {
            tmp: tmp.to_path_buf(),
            out,
            hash: FNV_SEED,
            pos: HEADER_LEN as u64,
            sec: [(0, 0); N_SECTIONS],
            dict: Some(fst::MapBuilder::memory()),
            text_off: Vec::new(),
            para: Vec::new(),
            level: Vec::new(),
            doc_of: Vec::new(),
            buf: Vec::new(),
            alpha,
        })
    }

    fn put(&mut self, bytes: &[u8]) -> Result<()> {
        self.out.write_all(bytes)?;
        self.hash = fnv64(self.hash, bytes);
        self.pos += bytes.len() as u64;
        Ok(())
    }

    /// 열쇠는 오름차순으로만 넣는다.
    fn posting(&mut self, key: &[u8], bm: &RoaringBitmap) -> Result<()> {
        if self.sec[POST].0 == 0 {
            self.sec[POST].0 = self.pos;
        }
        let off = self.pos - self.sec[POST].0;
        self.dict.as_mut().expect("postings after dict").insert(key, off).map_err(|e| Error::Corrupt(format!("fst insert: {e}")))?;
        self.buf.clear();
        bm.serialize_into(&mut self.buf)?;
        let buf = std::mem::take(&mut self.buf);
        self.put(&buf)?;
        self.buf = buf;
        Ok(())
    }

    fn end_postings(&mut self) -> Result<()> {
        if self.sec[POST].0 == 0 {
            self.sec[POST].0 = self.pos;
        }
        self.sec[POST].1 = self.pos - self.sec[POST].0;
        let dict = self.dict.take().expect("dict once").into_inner().map_err(|e| Error::Corrupt(format!("fst finish: {e}")))?;
        self.sec[DICT] = (self.pos, dict.len() as u64);
        self.put(&dict)?;
        self.sec[TEXT].0 = self.pos;
        Ok(())
    }

    /// 문장 하나(비교형 글)를 조각 글자 표로 부호화해 쓴다.
    fn sentence(&mut self, text: &str, para: u32, level: u8, doc_local: u32) -> Result<()> {
        let off = self.pos - self.sec[TEXT].0;
        if off >= u32::MAX as u64 {
            return Err(Error::Invalid("sentence text of one segment exceeds 4 GiB".into()));
        }
        self.text_off.push(off as u32);
        self.para.push(para);
        self.level.push(level);
        self.doc_of.push(doc_local);
        let mut buf = std::mem::take(&mut self.buf);
        buf.clear();
        if !self.alpha.encode_into(text, &mut buf) {
            return Err(Error::Invalid("character missing from the segment alphabet".into()));
        }
        self.put(&buf)?;
        self.buf = buf;
        Ok(())
    }

    fn finish(mut self, id: u64, docs: &[DocRec], final_path: &Path) -> Result<SegMeta> {
        let n_sent = self.text_off.len();
        if n_sent >= u32::MAX as usize || docs.len() >= u32::MAX as usize {
            return Err(Error::Invalid("too many sentences in one segment".into()));
        }
        self.sec[TEXT].1 = self.pos - self.sec[TEXT].0;
        if self.sec[TEXT].1 >= u32::MAX as u64 {
            return Err(Error::Invalid("sentence text of one segment exceeds 4 GiB".into()));
        }
        self.text_off.push(self.sec[TEXT].1 as u32);
        let section = |w: &mut SegWriter, idx: usize, bytes: &[u8]| -> Result<()> {
            w.sec[idx] = (w.pos, bytes.len() as u64);
            w.put(bytes)
        };
        let mut b: Vec<u8> = Vec::with_capacity((n_sent + 1) * 4);
        for v in &self.text_off {
            b.extend_from_slice(&v.to_le_bytes());
        }
        section(&mut self, TEXT_OFF, &b)?;
        b.clear();
        for v in &self.para {
            b.extend_from_slice(&v.to_le_bytes());
        }
        section(&mut self, PARA, &b)?;
        let level = std::mem::take(&mut self.level);
        section(&mut self, LEVEL, &level)?;
        b.clear();
        for v in &self.doc_of {
            b.extend_from_slice(&v.to_le_bytes());
        }
        section(&mut self, DOC_OF, &b)?;
        b.clear();
        for d in docs {
            b.extend_from_slice(&d.doc_id.to_le_bytes());
            b.extend_from_slice(&d.date.to_le_bytes());
            b.extend_from_slice(&d.first_sid.to_le_bytes());
            b.extend_from_slice(&d.n_sent.to_le_bytes());
        }
        section(&mut self, DOCS, &b)?;
        b.clear();
        for (c, n) in &self.alpha.chars {
            b.extend_from_slice(&(*c as u32).to_le_bytes());
            b.extend_from_slice(&n.to_le_bytes());
        }
        section(&mut self, ALPHA, &b)?;
        let mut ix: Vec<(u32, u32)> = self.alpha.chars.iter().enumerate().map(|(code, (c, _))| (*c as u32, code as u32)).collect();
        ix.sort_unstable();
        b.clear();
        for (c, code) in ix {
            b.extend_from_slice(&c.to_le_bytes());
            b.extend_from_slice(&code.to_le_bytes());
        }
        section(&mut self, ALPHA_IX, &b)?;

        let mut h = [0u8; HEADER_LEN];
        h[0..8].copy_from_slice(MAGIC);
        h[8..12].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        h[12..16].copy_from_slice(&FOLD_RULE_VERSION.to_le_bytes());
        h[16..24].copy_from_slice(&id.to_le_bytes());
        h[24..28].copy_from_slice(&(n_sent as u32).to_le_bytes());
        h[28..32].copy_from_slice(&(docs.len() as u32).to_le_bytes());
        h[32..40].copy_from_slice(&self.hash.to_le_bytes());
        h[40..48].copy_from_slice(&self.pos.to_le_bytes());
        for (i, (off, len)) in self.sec.iter().enumerate() {
            h[48 + i * 16..56 + i * 16].copy_from_slice(&off.to_le_bytes());
            h[56 + i * 16..64 + i * 16].copy_from_slice(&len.to_le_bytes());
        }
        let hh = fnv64(FNV_SEED, &h[..HEADER_HASH]);
        h[HEADER_HASH..HEADER_LEN].copy_from_slice(&hh.to_le_bytes());
        self.out.seek(SeekFrom::Start(0))?;
        self.out.write_all(&h)?;
        self.out.flush()?;
        let file = self.out.into_inner().map_err(|e| Error::Io(e.into_error()))?;
        file.sync_all()?;
        drop(file);
        crate::manifest::rename_retry(&self.tmp, final_path)?;
        Ok(SegMeta { id, n_sent: n_sent as u32, n_docs: docs.len() as u32, body_hash: self.hash })
    }
}

fn gram_key_bytes(packed: u64, out: &mut Vec<u8>) {
    out.clear();
    let mut buf = [0u8; 4];
    for shift in [0u32, 21, 42] {
        let cp = ((packed >> shift) & 0x1F_FFFF) as u32;
        if cp == 0 {
            break;
        }
        // 조각은 유효한 글자에서 만들었다.
        if let Some(c) = char::from_u32(cp) {
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        }
    }
}

/// 문서들로 새 조각 파일을 만든다. `dir/seg-<id>.kfs.tmp`에 쓰고 다 쓴 뒤 제 이름으로 바꾼다.
pub(crate) fn write_segment<'a>(dir: &Path, id: u64, docs: impl Iterator<Item = &'a PendingDoc> + Clone) -> Result<SegMeta> {
    let mut grams: FxMap<u64, RoaringBitmap> = FxMap::default();
    let mut words: HashMap<Box<[u8]>, RoaringBitmap> = HashMap::new();
    let mut cps: Vec<u64> = Vec::new();
    let mut char_counts: FxMap<u64, u64> = FxMap::default();
    let mut sid: u32 = 0;
    for d in docs.clone() {
        let mut start = 0usize;
        let mut tok = 0usize;
        for &end in &d.sent_end {
            let end = end as usize;
            let s = &d.text[start..end];
            cps.clear();
            cps.extend(s.chars().map(|c| c as u64));
            for &c in &cps {
                *char_counts.entry(c).or_default() += 1;
            }
            for i in 0..cps.len().saturating_sub(1) {
                let k2 = cps[i] | (cps[i + 1] << 21);
                // 같은 문장 안 중복 조각은 try_push가 거부한다(문장 번호가 늘어나는 순서로만 들어온다).
                let _ = grams.entry(k2).or_default().try_push(sid);
                if i + 2 < cps.len() {
                    let _ = grams.entry(k2 | (cps[i + 2] << 42)).or_default().try_push(sid);
                }
            }
            while tok < d.tok_start.len() && (d.tok_start[tok] as usize) < end {
                let a = d.tok_start[tok] as usize;
                let b = d.tok_start.get(tok + 1).map(|v| (*v as usize).min(end)).unwrap_or(end);
                tok += 1;
                let w = &d.text.as_bytes()[a..b];
                if w.len() <= MAX_WORD_BYTES && d.text[a..b].chars().nth(crate::query::MIN_WORD_CHARS - 1).is_some() {
                    match words.get_mut(w) {
                        Some(bm) => {
                            let _ = bm.try_push(sid);
                        }
                        None => {
                            let mut bm = RoaringBitmap::new();
                            let _ = bm.try_push(sid);
                            words.insert(w.into(), bm);
                        }
                    }
                }
            }
            start = end;
            sid = sid.checked_add(1).ok_or_else(|| Error::Invalid("too many sentences in one segment".into()))?;
        }
    }
    let mut entries: Vec<(Vec<u8>, RoaringBitmap)> = Vec::with_capacity(grams.len() + words.len());
    let mut key = Vec::new();
    for (packed, bm) in grams {
        gram_key_bytes(packed, &mut key);
        entries.push((key.clone(), bm));
    }
    for (w, bm) in words {
        let mut k = Vec::with_capacity(w.len() + 1);
        k.push(WORD_PREFIX);
        k.extend_from_slice(&w);
        entries.push((k, bm));
    }
    entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));

    let alpha = Alphabet::from_counts(char_counts.into_iter().filter_map(|(c, n)| Some((char::from_u32(c as u32)?, n))));
    let name = seg_file_name(id);
    let mut w = SegWriter::create(&dir.join(format!("{name}.tmp")), alpha)?;
    for (k, bm) in &entries {
        w.posting(k, bm)?;
    }
    drop(entries);
    w.end_postings()?;
    let mut recs: Vec<DocRec> = Vec::new();
    let mut sid: u32 = 0;
    for (local, d) in docs.enumerate() {
        recs.push(DocRec { doc_id: d.doc_id, date: d.date, first_sid: sid, n_sent: d.sent_end.len() as u32 });
        let mut start = 0usize;
        for (i, &end) in d.sent_end.iter().enumerate() {
            w.sentence(&d.text[start..end as usize], d.para[i], d.level[i], local as u32)?;
            start = end as usize;
            sid += 1;
        }
    }
    w.finish(id, &recs, &dir.join(name))
}

/// mmap 구역을 fst가 빌려 쓸 수 있게 감싼다.
struct Section {
    map: Arc<Mmap>,
    start: usize,
    len: usize,
}

impl AsRef<[u8]> for Section {
    fn as_ref(&self) -> &[u8] {
        &self.map[self.start..self.start + self.len]
    }
}

/// 읽기 전용으로 연 조각.
pub(crate) struct Segment {
    pub id: u64,
    pub n_sent: u32,
    pub n_docs: u32,
    pub body_hash: u64,
    map: Arc<Mmap>,
    dict: fst::Map<Section>,
    sec: [(usize, usize); N_SECTIONS],
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

fn u64_at(b: &[u8], i: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[i..i + 8]);
    u64::from_le_bytes(a)
}

impl Segment {
    pub(crate) fn open(path: &Path) -> Result<Segment> {
        let file = File::open(path)?;
        // SAFETY: 조각 파일은 제 이름을 얻은 뒤에는 누구도 내용을 바꾸지 않는다(새 내용은 항상 새 번호의 새 파일).
        // 지우기는 목록 파일에서 빠진 뒤에만 하고, Windows에서는 매핑 중인 파일을 지울 수 없어 다음 정리 때로 미뤄진다.
        let map = unsafe { Mmap::map(&file)? };
        let b: &[u8] = &map;
        if b.len() < HEADER_LEN || &b[0..8] != MAGIC {
            return corrupt(format!("{}: not a segment file", path.display()));
        }
        let format = u32_at(b, 8);
        if format != FORMAT_VERSION {
            return Err(Error::FormatVersion { found: format, expected: FORMAT_VERSION });
        }
        if fnv64(FNV_SEED, &b[..HEADER_HASH]) != u64_at(b, HEADER_HASH) {
            return corrupt(format!("{}: header checksum", path.display()));
        }
        let rule = u32_at(b, 12);
        if rule != FOLD_RULE_VERSION {
            return Err(Error::RuleVersion { found: rule, expected: FOLD_RULE_VERSION });
        }
        let id = u64_at(b, 16);
        let n_sent = u32_at(b, 24);
        let n_docs = u32_at(b, 28);
        let body_hash = u64_at(b, 32);
        if u64_at(b, 40) != b.len() as u64 {
            return corrupt(format!("{}: file length {} differs from header {}", path.display(), b.len(), u64_at(b, 40)));
        }
        let mut sec = [(0usize, 0usize); N_SECTIONS];
        for (i, s) in sec.iter_mut().enumerate() {
            let off = u64_at(b, 48 + i * 16);
            let len = u64_at(b, 56 + i * 16);
            if off < HEADER_LEN as u64 || off.checked_add(len).is_none_or(|e| e > b.len() as u64) {
                return corrupt(format!("{}: section {i} out of bounds", path.display()));
            }
            *s = (off as usize, len as usize);
        }
        let n = n_sent as usize;
        let n_chars = sec[ALPHA].1 / ALPHA_REC;
        let sizes_ok = sec[TEXT_OFF].1 == (n + 1) * 4
            && sec[PARA].1 == n * 4
            && sec[LEVEL].1 == n
            && sec[DOC_OF].1 == n * 4
            && sec[DOCS].1 == n_docs as usize * DOC_REC
            && sec[ALPHA].1 % ALPHA_REC == 0
            && sec[ALPHA_IX].1 == n_chars * ALPHA_IX_REC;
        if !sizes_ok || u32_at(b, sec[TEXT_OFF].0 + n * 4) as usize != sec[TEXT].1 {
            return corrupt(format!("{}: section sizes do not match counts", path.display()));
        }
        let map = Arc::new(map);
        let dict = fst::Map::new(Section { map: map.clone(), start: sec[DICT].0, len: sec[DICT].1 }).map_err(|e| Error::Corrupt(format!("{}: dictionary: {e}", path.display())))?;
        Ok(Segment { id, n_sent, n_docs, body_hash, map, dict, sec })
    }

    /// 본문 전체 해시를 다시 계산해 머리글과 비교한다(파일 크기에 비례해 느리다. 진단·시험용).
    pub(crate) fn verify(&self) -> Result<()> {
        if fnv64(FNV_SEED, &self.map[HEADER_LEN..]) != self.body_hash {
            return corrupt(format!("segment {}: body checksum", self.id));
        }
        Ok(())
    }

    fn section(&self, idx: usize) -> &[u8] {
        &self.map[self.sec[idx].0..self.sec[idx].0 + self.sec[idx].1]
    }

    /// 열쇠의 문장 비트맵. 사전에 없으면 None.
    pub(crate) fn posting(&self, key: &[u8]) -> Result<Option<RoaringBitmap>> {
        match self.dict.get(key) {
            None => Ok(None),
            Some(off) => self.posting_at(off).map(Some),
        }
    }

    fn posting_at(&self, off: u64) -> Result<RoaringBitmap> {
        let post = self.section(POST);
        if off >= post.len() as u64 {
            return corrupt(format!("segment {}: posting offset out of bounds", self.id));
        }
        // 검사 없는 읽기: 깨진 자료로는 틀린 결과나 패닉이 날 수 있지만(패닉은 경계에서 잡힌다) 메모리 안전은 유지된다.
        RoaringBitmap::deserialize_unchecked_from(&post[off as usize..]).map_err(|e| Error::Corrupt(format!("segment {}: posting: {e}", self.id)))
    }

    /// 문장의 비교형 글(조각 글자 표로 부호화한 바이트). 찾을 낱말은 `encode_word`로 같은 부호로 바꿔 비교한다.
    pub(crate) fn text(&self, sid: u32) -> Result<&[u8]> {
        if sid >= self.n_sent {
            return corrupt(format!("segment {}: sentence {sid} out of range", self.id));
        }
        let offs = self.section(TEXT_OFF);
        let a = u32_at(offs, sid as usize * 4) as usize;
        let b = u32_at(offs, sid as usize * 4 + 4) as usize;
        match self.section(TEXT).get(a..b) {
            Some(t) => Ok(t),
            None => corrupt(format!("segment {}: text range of sentence {sid}", self.id)),
        }
    }

    /// 낱말을 이 조각의 부호로 바꾼다. 조각에 없는 글자가 있으면 None(그 낱말은 이 조각 어느 문장에도 없다).
    pub(crate) fn encode_word(&self, w: &str) -> Option<Vec<u8>> {
        let ix = self.section(ALPHA_IX);
        let n = ix.len() / ALPHA_IX_REC;
        let mut out = Vec::with_capacity(w.len());
        for c in w.chars() {
            let c = c as u32;
            // 글자 오름차순 표에서 이진 탐색.
            let (mut lo, mut hi) = (0usize, n);
            let mut code = None;
            while lo < hi {
                let mid = (lo + hi) / 2;
                match u32_at(ix, mid * ALPHA_IX_REC).cmp(&c) {
                    std::cmp::Ordering::Less => lo = mid + 1,
                    std::cmp::Ordering::Greater => hi = mid,
                    std::cmp::Ordering::Equal => {
                        code = Some(u32_at(ix, mid * ALPHA_IX_REC + 4));
                        break;
                    }
                }
            }
            // 깨진 표의 큰 번호는 잘라 쓴다(틀린 결과는 날 수 있어도 패닉·메모리 문제는 없다).
            put_code(code?.min(MAX_CODES - 1), &mut out);
        }
        Some(out)
    }

    /// 문장의 비교형 글을 UTF-8로 되돌린다(합치기용).
    fn decode_text(&self, sid: u32, out: &mut String) -> Result<()> {
        out.clear();
        let alpha = self.section(ALPHA);
        let n = alpha.len() / ALPHA_REC;
        let ok = decode_codes(self.text(sid)?, |code| {
            let code = code as usize;
            if code >= n {
                return false;
            }
            match char::from_u32(u32_at(alpha, code * ALPHA_REC)) {
                Some(c) => {
                    out.push(c);
                    true
                }
                None => false,
            }
        });
        if ok.is_none() {
            return corrupt(format!("segment {}: text of sentence {sid}", self.id));
        }
        Ok(())
    }

    /// 조각 글자 표의 (글자, 빈도). 깨진 글자는 건너뛴다.
    fn alphabet_counts(&self) -> impl Iterator<Item = (char, u64)> + '_ {
        self.section(ALPHA).chunks_exact(ALPHA_REC).filter_map(|r| Some((char::from_u32(u32_at(r, 0))?, u64_at(r, 4))))
    }

    fn check_sid(&self, sid: u32) -> Result<usize> {
        if sid >= self.n_sent {
            return corrupt(format!("segment {}: sentence {sid} out of range", self.id));
        }
        Ok(sid as usize)
    }

    pub(crate) fn para(&self, sid: u32) -> Result<u32> {
        Ok(u32_at(self.section(PARA), self.check_sid(sid)? * 4))
    }

    /// 빠른 모으기용 표 묶음. 반복문에서 구역 찾기를 되풀이하지 않게 한 번에 꺼낸다.
    pub(crate) fn tables(&self) -> Tables<'_> {
        Tables { levels: self.section(LEVEL), doc_of: self.section(DOC_OF), docs: self.section(DOCS) }
    }

    pub(crate) fn level(&self, sid: u32) -> Result<u8> {
        Ok(self.section(LEVEL)[self.check_sid(sid)?])
    }

    /// 문장이 속한 조각 안 문서 번호.
    pub(crate) fn doc_of(&self, sid: u32) -> Result<u32> {
        let d = u32_at(self.section(DOC_OF), self.check_sid(sid)? * 4);
        if d >= self.n_docs {
            return corrupt(format!("segment {}: document {d} out of range", self.id));
        }
        Ok(d)
    }

    pub(crate) fn doc(&self, local: u32) -> Result<DocRec> {
        if local >= self.n_docs {
            return corrupt(format!("segment {}: document {local} out of range", self.id));
        }
        let b = self.section(DOCS);
        let i = local as usize * DOC_REC;
        let rec = DocRec { doc_id: u64_at(b, i), date: u64_at(b, i + 8) as i64, first_sid: u32_at(b, i + 16), n_sent: u32_at(b, i + 20) };
        if rec.first_sid.checked_add(rec.n_sent).is_none_or(|e| e > self.n_sent) {
            return corrupt(format!("segment {}: document {local} sentence range", self.id));
        }
        Ok(rec)
    }
}

/// 조각의 문장·문서 표를 날것으로 본 것(열 때 크기를 확인했다).
pub(crate) struct Tables<'a> {
    levels: &'a [u8],
    doc_of: &'a [u8],
    docs: &'a [u8],
}

impl Tables<'_> {
    /// 문장의 위계 순위. 범위를 벗어나면 None.
    #[inline]
    pub(crate) fn level(&self, sid: u32) -> Option<u8> {
        self.levels.get(sid as usize).copied()
    }

    /// 문장이 속한 조각 안 문서 번호. 범위를 벗어나면 None.
    #[inline]
    pub(crate) fn doc_of(&self, sid: u32) -> Option<u32> {
        let i = sid as usize * 4;
        let b = self.doc_of.get(i..i + 4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// 문서 기록. 범위를 벗어나면 None(문장 범위는 호출하는 쪽이 확인한다).
    #[inline]
    pub(crate) fn doc(&self, local: u32) -> Option<DocRec> {
        let i = local as usize * DOC_REC;
        let b = self.docs.get(i..i + DOC_REC)?;
        Some(DocRec { doc_id: u64_at(b, 0), date: u64_at(b, 8) as i64, first_sid: u32_at(b, 16), n_sent: u32_at(b, 20) })
    }
}

/// 합치기 결과에서 원본 조각 하나의 몫.
pub(crate) struct MergedSource {
    pub id: u64,
    /// 합칠 때 본 "지운 문장" 비트맵.
    pub deleted: RoaringBitmap,
    /// 이 조각의 살아 있는 문장이 새 조각에서 시작하는 번호.
    pub base: u32,
}

/// 여러 조각의 살아 있는 문장만 모아 새 조각 하나로 쓴다. 비트맵은 다시 만들지 않고 문장 번호만 옮겨 적는다.
/// 살아 있는 문장이 없으면 파일을 만들지 않고 None을 돌려준다.
pub(crate) fn merge_segments(dir: &Path, new_id: u64, sources: &[(Arc<Segment>, RoaringBitmap)]) -> Result<(Option<SegMeta>, Vec<MergedSource>)> {
    const GONE: u32 = u32::MAX;
    let mut remaps: Vec<Vec<u32>> = Vec::with_capacity(sources.len());
    let mut out_sources = Vec::with_capacity(sources.len());
    let mut next: u64 = 0;
    for (seg, deleted) in sources {
        let base = next;
        let mut remap = vec![GONE; seg.n_sent as usize];
        for (sid, slot) in remap.iter_mut().enumerate() {
            if !deleted.contains(sid as u32) {
                *slot = next as u32;
                next += 1;
            }
        }
        if next >= u32::MAX as u64 {
            return Err(Error::Invalid("too many sentences to merge into one segment".into()));
        }
        remaps.push(remap);
        out_sources.push(MergedSource { id: seg.id, deleted: deleted.clone(), base: base as u32 });
    }
    if next == 0 {
        return Ok((None, out_sources));
    }
    let mut counts: HashMap<char, u64> = HashMap::new();
    for (seg, _) in sources {
        for (c, n) in seg.alphabet_counts() {
            *counts.entry(c).or_default() += n;
        }
    }
    let name = seg_file_name(new_id);
    let mut w = SegWriter::create(&dir.join(format!("{name}.tmp")), Alphabet::from_counts(counts))?;
    {
        let mut op = fst::map::OpBuilder::new();
        for (seg, _) in sources {
            op = op.add(seg.dict.stream());
        }
        let mut union = op.union();
        let mut ivs: Vec<fst::map::IndexedValue> = Vec::new();
        use fst::Streamer;
        while let Some((key, values)) = union.next() {
            ivs.clear();
            ivs.extend_from_slice(values);
            ivs.sort_unstable_by_key(|iv| iv.index);
            let mut bm = RoaringBitmap::new();
            for iv in &ivs {
                let remap = &remaps[iv.index];
                let src = sources[iv.index].0.posting_at(iv.value)?;
                for sid in src {
                    match remap.get(sid as usize) {
                        Some(&n) if n != GONE => {
                            let _ = bm.try_push(n);
                        }
                        Some(_) => {}
                        None => return corrupt(format!("segment {}: posting sentence {sid} out of range", sources[iv.index].0.id)),
                    }
                }
            }
            if !bm.is_empty() {
                w.posting(key, &bm)?;
            }
        }
    }
    w.end_postings()?;
    let mut recs: Vec<DocRec> = Vec::new();
    let mut text = String::new();
    for (i, (seg, _)) in sources.iter().enumerate() {
        let remap = &remaps[i];
        for local in 0..seg.n_docs {
            let d = seg.doc(local)?;
            // 문서는 통째로 지워진다: 첫 문장이 지워졌으면 문서 전체가 지워진 것.
            if d.n_sent == 0 || remap[d.first_sid as usize] == GONE {
                continue;
            }
            let new_local = recs.len() as u32;
            recs.push(DocRec { doc_id: d.doc_id, date: d.date, first_sid: remap[d.first_sid as usize], n_sent: d.n_sent });
            for sid in d.first_sid..d.first_sid + d.n_sent {
                if remap[sid as usize] == GONE {
                    return corrupt(format!("segment {}: document {} is partly deleted", seg.id, d.doc_id));
                }
                seg.decode_text(sid, &mut text)?;
                w.sentence(&text, seg.para(sid)?, seg.level(sid)?, new_local)?;
            }
        }
    }
    let meta = w.finish(new_id, &recs, &dir.join(name))?;
    if meta.n_sent as u64 != next {
        return corrupt("merge wrote a different sentence count than planned");
    }
    Ok((Some(meta), out_sources))
}
