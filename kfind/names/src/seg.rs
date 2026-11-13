//! `names.seg`: 이름 색인 한 세대를 파일로 쓰고, 다른 프로세스가 메모리 매핑으로 복사 없이 읽는다.
//!
//! 공통 머리글(TM1 §1-4, 64바이트, 리틀 엔디언): 마법수 `KTMSEG01` · format_version u32 · kind u32(1 = names)
//! · rule_version u32(비교형 규칙 판) · 0 u32 · generation u64 · body_len u64 · 목차 오프셋 u64 · body_crc32c u32
//! · header_crc32c u32(앞 52바이트) · 0 8바이트. 본문은 8바이트 정렬 구획들, 끝에 목차(개수 u32·0 u32·{id u32, 0 u32, off u64, len u64}…).
//! 읽는 쪽은 모든 오프셋·길이·배열 관계(위치 배열 단조 증가, 부모 번호 범위, 하위 트리 끝)를 검사한 뒤 슬라이스를 만든다.
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::crc::crc32c;
use crate::index::{EXT_OTHER, ExtList, ExtTable, Names, NONE, View, Volume};
use crate::{Error, Result};

pub const MAGIC: &[u8; 8] = b"KTMSEG01";
pub const FORMAT_VERSION: u32 = 1;
pub const KIND_NAMES: u32 = 1;
const HEADER: usize = 64;

mod sec {
    pub const PARENT: u32 = 1;
    pub const FOLD_OFF: u32 = 2;
    pub const NAME_OFF: u32 = 3;
    pub const KEY: u32 = 4;
    pub const EXT: u32 = 5;
    pub const FLAGS: u32 = 6;
    pub const END: u32 = 7;
    pub const FOLD: u32 = 8;
    pub const NAME: u32 = 9;
    pub const EXT_BLOB: u32 = 10;
    pub const EXT_OFF: u32 = 11;
    pub const META: u32 = 12;
    pub const RECMAP: u32 = 13;
    pub const ALL: [u32; 13] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
}

fn bytes_of<T: Copy>(v: &[T]) -> &[u8] {
    // SAFETY: 정수 배열을 바이트로 본다(패딩 없는 기본 정수형만 넘긴다).
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

/// 색인을 `path`에 쓴다(같은 폴더 임시 파일 → 이름 바꾸기). 덧붙임이 있으면 먼저 기본판으로 다시 만든다.
pub fn write(names: &Names, path: &Path) -> Result<u64> {
    #[cfg(not(target_endian = "little"))]
    compile_error!("names.seg는 리틀 엔디언만 지원");
    let compacted;
    let n = if names.has_delta() {
        compacted = names.compact();
        &compacted
    } else {
        names
    };
    let mut ext_blob = Vec::new();
    let mut ext_off: Vec<u32> = vec![0];
    for e in &n.exts.names {
        ext_blob.extend_from_slice(e);
        ext_off.push(ext_blob.len() as u32);
    }
    let mut meta = Vec::new();
    meta.extend_from_slice(&(n.len() as u32).to_le_bytes());
    meta.extend_from_slice(&n.base_len.to_le_bytes());
    meta.extend_from_slice(&(n.vols.len() as u32).to_le_bytes());
    meta.extend_from_slice(&0u32.to_le_bytes());
    let mut recmap: Vec<u32> = Vec::new();
    for v in &n.vols {
        meta.extend_from_slice(&v.root_rec.to_le_bytes());
        meta.extend_from_slice(&v.journal_id.to_le_bytes());
        meta.extend_from_slice(&v.next_usn.to_le_bytes());
        meta.extend_from_slice(&(v.recmap.len() as u32).to_le_bytes());
        meta.extend_from_slice(&(v.name.len() as u32).to_le_bytes());
        meta.extend_from_slice(v.name.as_bytes());
        while meta.len() % 8 != 0 {
            meta.push(0);
        }
        recmap.extend_from_slice(&v.recmap);
    }
    let sections: Vec<(u32, &[u8])> = vec![
        (sec::PARENT, bytes_of(&n.parent)),
        (sec::FOLD_OFF, bytes_of(&n.fold_off)),
        (sec::NAME_OFF, bytes_of(&n.name_off)),
        (sec::KEY, bytes_of(&n.key)),
        (sec::EXT, bytes_of(&n.ext)),
        (sec::FLAGS, &n.flags),
        (sec::END, bytes_of(&n.end)),
        (sec::FOLD, &n.fold),
        (sec::NAME, &n.name),
        (sec::EXT_BLOB, &ext_blob),
        (sec::EXT_OFF, bytes_of(&ext_off)),
        (sec::META, &meta),
        (sec::RECMAP, bytes_of(&recmap)),
    ];
    let mut body: Vec<u8> = Vec::new();
    let mut toc: Vec<(u32, u64, u64)> = Vec::new();
    for (id, data) in &sections {
        while (HEADER + body.len()) % 8 != 0 {
            body.push(0);
        }
        toc.push((*id, (HEADER + body.len()) as u64, data.len() as u64));
        body.extend_from_slice(data);
    }
    while (HEADER + body.len()) % 8 != 0 {
        body.push(0);
    }
    let toc_off = (HEADER + body.len()) as u64;
    body.extend_from_slice(&(toc.len() as u32).to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes());
    for (id, off, len) in &toc {
        body.extend_from_slice(&id.to_le_bytes());
        body.extend_from_slice(&0u32.to_le_bytes());
        body.extend_from_slice(&off.to_le_bytes());
        body.extend_from_slice(&len.to_le_bytes());
    }
    let mut h = [0u8; HEADER];
    h[0..8].copy_from_slice(MAGIC);
    h[8..12].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
    h[12..16].copy_from_slice(&KIND_NAMES.to_le_bytes());
    h[16..20].copy_from_slice(&kfind_core::FOLD_RULE_VERSION.to_le_bytes());
    h[24..32].copy_from_slice(&n.generation.to_le_bytes());
    h[32..40].copy_from_slice(&(body.len() as u64).to_le_bytes());
    h[40..48].copy_from_slice(&toc_off.to_le_bytes());
    h[48..52].copy_from_slice(&crc32c(&body).to_le_bytes());
    let hc = crc32c(&h[0..52]);
    h[52..56].copy_from_slice(&hc.to_le_bytes());
    let tmp = path.with_extension("seg.tmp");
    {
        let mut f = File::create(&tmp)?;
        f.write_all(&h)?;
        f.write_all(&body)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok((HEADER + body.len()) as u64)
}

/// 메모리 매핑한 `names.seg`.
pub struct NamesSeg {
    map: memmap2::Mmap,
    secs: HashMap<u32, (usize, usize)>,
    generation: u64,
    n: usize,
    base_len: u32,
    vols: Vec<Volume>,
}

fn corrupt(s: &str) -> Error {
    Error::Corrupt(s.to_string())
}

fn cast<T: Copy>(b: &[u8]) -> Result<&[T]> {
    let sz = std::mem::size_of::<T>();
    if b.len() % sz != 0 || (b.as_ptr() as usize) % std::mem::align_of::<T>() != 0 {
        return Err(corrupt("section alignment"));
    }
    // SAFETY: 정렬·길이를 확인했고 T는 모든 비트 패턴이 유효한 정수형만 쓴다.
    Ok(unsafe { std::slice::from_raw_parts(b.as_ptr() as *const T, b.len() / sz) })
}

fn rd_u32(b: &[u8], o: usize) -> Result<u32> {
    b.get(o..o + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap())).ok_or_else(|| corrupt("short"))
}
fn rd_u64(b: &[u8], o: usize) -> Result<u64> {
    b.get(o..o + 8).map(|x| u64::from_le_bytes(x.try_into().unwrap())).ok_or_else(|| corrupt("short"))
}

impl NamesSeg {
    /// 열고 구조를 검사한다. `verify_body`면 본문 CRC까지 확인(파일 전체를 한 번 읽음).
    pub fn open(path: &Path, verify_body: bool) -> Result<NamesSeg> {
        let f = File::open(path)?;
        // SAFETY: 게시된 세그먼트는 다시 쓰지 않는다(TM1 §1-4 불변). 내용은 아래에서 모두 검사한다.
        let map = unsafe { memmap2::Mmap::map(&f)? };
        Self::from_map(map, verify_body)
    }

    fn from_map(map: memmap2::Mmap, verify_body: bool) -> Result<NamesSeg> {
        let b: &[u8] = &map;
        if b.len() < HEADER || &b[0..8] != MAGIC {
            return Err(corrupt("magic"));
        }
        if crc32c(&b[0..52]) != rd_u32(b, 52)? {
            return Err(corrupt("header crc"));
        }
        if rd_u32(b, 8)? != FORMAT_VERSION || rd_u32(b, 12)? != KIND_NAMES {
            return Err(corrupt("version/kind"));
        }
        if b[20..24] != [0; 4] || b[56..64] != [0; 8] {
            return Err(corrupt("header reserved"));
        }
        if rd_u32(b, 16)? != kfind_core::FOLD_RULE_VERSION {
            return Err(corrupt("fold rule version"));
        }
        let generation = rd_u64(b, 24)?;
        let body_len = rd_u64(b, 32)? as usize;
        if HEADER.checked_add(body_len) != Some(b.len()) {
            return Err(corrupt("body length"));
        }
        if verify_body && crc32c(&b[HEADER..]) != rd_u32(b, 48)? {
            return Err(corrupt("body crc"));
        }
        let toc = rd_u64(b, 40)? as usize;
        let cnt = rd_u32(b, toc)? as usize;
        if cnt > 64 {
            return Err(corrupt("toc count"));
        }
        let mut secs = HashMap::new();
        for k in 0..cnt {
            let e = toc + 8 + k * 24;
            let id = rd_u32(b, e)?;
            let off = rd_u64(b, e + 8)? as usize;
            let len = rd_u64(b, e + 16)? as usize;
            if off < HEADER || off.checked_add(len).is_none_or(|x| x > toc) {
                return Err(corrupt("section bounds"));
            }
            secs.insert(id, (off, len));
        }
        for id in sec::ALL {
            if !secs.contains_key(&id) {
                return Err(corrupt("missing section"));
            }
        }
        let mut s = NamesSeg { map, secs, generation, n: 0, base_len: 0, vols: Vec::new() };
        s.load_meta()?;
        s.validate()?;
        Ok(s)
    }

    fn raw(&self, id: u32) -> &[u8] {
        let (o, l) = self.secs[&id];
        &self.map[o..o + l]
    }

    fn load_meta(&mut self) -> Result<()> {
        let m = self.raw(sec::META).to_vec();
        let n = rd_u32(&m, 0)? as usize;
        let base_len = rd_u32(&m, 4)?;
        let nv = rd_u32(&m, 8)? as usize;
        if nv > 4096 {
            return Err(corrupt("volume count"));
        }
        let recmap: &[u32] = cast(self.raw(sec::RECMAP))?;
        let mut o = 16;
        let mut ro = 0usize;
        let mut vols = Vec::with_capacity(nv);
        for _ in 0..nv {
            let root_rec = rd_u64(&m, o)?;
            let journal_id = rd_u64(&m, o + 8)?;
            let next_usn = rd_u64(&m, o + 16)? as i64;
            let rl = rd_u32(&m, o + 24)? as usize;
            let nl = rd_u32(&m, o + 28)? as usize;
            let name = m.get(o + 32..o + 32 + nl).ok_or_else(|| corrupt("meta name"))?;
            let name = std::str::from_utf8(name).map_err(|_| corrupt("meta utf8"))?.to_string();
            o = (o + 32 + nl).div_ceil(8) * 8;
            let rm = recmap.get(ro..ro + rl).ok_or_else(|| corrupt("recmap"))?;
            if rm.iter().any(|&i| i != NONE && i as usize >= n) {
                return Err(corrupt("recmap range"));
            }
            ro += rl;
            vols.push(Volume { name, root_rec, recmap: rm.to_vec(), journal_id, next_usn });
        }
        if base_len as usize != n {
            return Err(corrupt("segment must be compacted"));
        }
        self.n = n;
        self.base_len = base_len;
        self.vols = vols;
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        let v = self.try_view()?;
        let n = self.n;
        if v.parent.len() != n
            || v.fold_off.len() != n + 1
            || v.name_off.len() != n + 1
            || v.key.len() != n
            || v.ext.len() != n
            || v.flags.len() != n
            || v.end.len() != n
        {
            return Err(corrupt("array lengths"));
        }
        if v.fold.first() != Some(&0) || v.fold_off[0] != 1 || v.fold_off[n] as usize != v.fold.len() {
            return Err(corrupt("fold arena"));
        }
        for i in 0..n {
            let (a, b) = (v.fold_off[i], v.fold_off[i + 1]);
            if b <= a || b as usize > v.fold.len() || v.fold[b as usize - 1] != 0 {
                return Err(corrupt("fold offsets"));
            }
            if v.name_off[i + 1] < v.name_off[i] {
                return Err(corrupt("name offsets"));
            }
            let p = v.parent[i];
            if p != NONE && p as usize >= i {
                return Err(corrupt("parent order"));
            }
            let e = v.end[i] as usize;
            if e <= i || e > n {
                return Err(corrupt("subtree end"));
            }
            let x = v.ext[i];
            if x != EXT_OTHER && x as usize >= v.exts.len().max(1) {
                return Err(corrupt("ext id"));
            }
        }
        if v.name_off[0] != 0 || v.name_off[n] as usize != v.name.len() {
            return Err(corrupt("name arena"));
        }
        std::str::from_utf8(v.name).map_err(|_| corrupt("name utf8"))?;
        let eo: &[u32] = cast(self.raw(sec::EXT_OFF))?;
        if eo.is_empty() || eo[0] != 0 || eo.windows(2).any(|w| w[1] < w[0]) || *eo.last().unwrap() as usize != self.raw(sec::EXT_BLOB).len()
        {
            return Err(corrupt("ext table"));
        }
        Ok(())
    }

    fn try_view(&self) -> Result<View<'_>> {
        Ok(View {
            parent: cast(self.raw(sec::PARENT))?,
            fold_off: cast(self.raw(sec::FOLD_OFF))?,
            name_off: cast(self.raw(sec::NAME_OFF))?,
            key: cast(self.raw(sec::KEY))?,
            ext: cast(self.raw(sec::EXT))?,
            flags: self.raw(sec::FLAGS),
            end: cast(self.raw(sec::END))?,
            fold: self.raw(sec::FOLD),
            name: self.raw(sec::NAME),
            exts: ExtList::Blob { blob: self.raw(sec::EXT_BLOB), off: cast(self.raw(sec::EXT_OFF))? },
            base_len: self.base_len,
            redirect: None,
            dirty: false,
        })
    }

    /// 질의용 보기(열 때 검사를 마쳤으므로 실패하지 않는다).
    pub fn view(&self) -> View<'_> {
        self.try_view().expect("validated at open")
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn len(&self) -> usize {
        self.n
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    pub fn file_len(&self) -> usize {
        self.map.len()
    }

    pub fn volumes(&self) -> &[Volume] {
        &self.vols
    }

    /// 소유판으로 읽어 들인다(서비스 재시작 스냅숏 → USN 따라잡기).
    pub fn to_names(&self) -> Names {
        let v = self.view();
        let mut exts = ExtTable::new();
        for i in 1..v.exts.len() {
            let e = v.exts.get(i).to_vec();
            exts.map.insert(e.clone(), i as u16);
            exts.names.push(e);
        }
        Names {
            parent: v.parent.to_vec(),
            fold_off: v.fold_off.to_vec(),
            name_off: v.name_off.to_vec(),
            key: v.key.to_vec(),
            ext: v.ext.to_vec(),
            flags: v.flags.to_vec(),
            end: v.end.to_vec(),
            fold: v.fold.to_vec(),
            name: v.name.to_vec(),
            exts,
            base_len: self.base_len,
            redirect: HashMap::new(),
            dirty: false,
            vols: self.vols.clone(),
            generation: self.generation,
            deleted: v.flags.iter().filter(|&&f| f & crate::flag::DELETED != 0).count(),
        }
    }
}
