//! 목록 파일(MANIFEST): 지금 유효한 조각들과 조각마다 "지운 문장" 비트맵. 색인에서 유일하게 교체되는 파일이다.
//! 임시 파일에 다 쓰고 디스크에 내린 뒤 rename으로 한 번에 바꾼다. 읽는 쪽은 옛 목록이나 새 목록 중 하나만 본다.
//!
//! 배치: magic "KFMAN\r\n\x1a" | 형식 버전 u32 | 규칙 버전 u32 | 세대 u64 | 다음 조각 번호 u64 | 조각 수 u32 |
//!       조각마다 (번호 u64, 문장 수 u32, 문서 수 u32, 본문 해시 u64, 비트맵 길이 u32, 비트맵) | FNV-1a 64(앞 전체)
use crate::fold::FOLD_RULE_VERSION;
use crate::segment::FORMAT_VERSION;
use crate::{Error, FNV_SEED, Result, corrupt, fnv64};
use roaring::RoaringBitmap;
use std::io::Write;
use std::path::Path;

pub(crate) const MANIFEST_NAME: &str = "MANIFEST.kfm";
pub(crate) const MANIFEST_TMP_PREFIX: &str = "MANIFEST.tmp-";
const MAGIC: &[u8; 8] = b"KFMAN\r\n\x1a";

#[derive(Clone)]
pub(crate) struct SegEntry {
    pub id: u64,
    pub n_sent: u32,
    pub n_docs: u32,
    pub body_hash: u64,
    /// 지운 문장(삭제·수정된 문서의 옛 문장).
    pub deleted: RoaringBitmap,
    /// 지운 문서(삭제·수정된 문서의 로컬 번호, tombstone).
    pub doc_deleted: RoaringBitmap,
}

#[derive(Clone, Default)]
pub(crate) struct Manifest {
    /// 커밋할 때마다 1씩 는다. 0 = 아직 아무것도 없음.
    pub generation: u64,
    pub next_seg: u64,
    pub segs: Vec<SegEntry>,
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        match self.b.get(self.pos..self.pos + n) {
            Some(s) => {
                self.pos += n;
                Ok(s)
            }
            None => corrupt("manifest: truncated"),
        }
    }
    fn u32(&mut self) -> Result<u32> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn u64(&mut self) -> Result<u64> {
        let mut a = [0u8; 8];
        a.copy_from_slice(self.take(8)?);
        Ok(u64::from_le_bytes(a))
    }
}

impl Manifest {
    pub(crate) fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut b = Vec::new();
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        b.extend_from_slice(&FOLD_RULE_VERSION.to_le_bytes());
        b.extend_from_slice(&self.generation.to_le_bytes());
        b.extend_from_slice(&self.next_seg.to_le_bytes());
        b.extend_from_slice(&(self.segs.len() as u32).to_le_bytes());
        for s in &self.segs {
            b.extend_from_slice(&s.id.to_le_bytes());
            b.extend_from_slice(&s.n_sent.to_le_bytes());
            b.extend_from_slice(&s.n_docs.to_le_bytes());
            b.extend_from_slice(&s.body_hash.to_le_bytes());
            b.extend_from_slice(&(s.deleted.serialized_size() as u32).to_le_bytes());
            s.deleted.serialize_into(&mut b)?;
            b.extend_from_slice(&(s.doc_deleted.serialized_size() as u32).to_le_bytes());
            s.doc_deleted.serialize_into(&mut b)?;
        }
        let h = fnv64(FNV_SEED, &b);
        b.extend_from_slice(&h.to_le_bytes());
        Ok(b)
    }

    pub(crate) fn from_bytes(b: &[u8]) -> Result<Manifest> {
        if b.len() < 16 || &b[0..8] != MAGIC {
            return corrupt("manifest: bad magic");
        }
        let mut r = Reader { b, pos: 8 };
        let format = r.u32()?;
        if format != FORMAT_VERSION {
            return Err(Error::FormatVersion { found: format, expected: FORMAT_VERSION });
        }
        if b.len() < 24 || fnv64(FNV_SEED, &b[..b.len() - 8]) != u64::from_le_bytes(b[b.len() - 8..].try_into().map_err(|_| Error::Corrupt("manifest: truncated".into()))?) {
            return corrupt("manifest: checksum");
        }
        let rule = r.u32()?;
        if rule != FOLD_RULE_VERSION {
            return Err(Error::RuleVersion { found: rule, expected: FOLD_RULE_VERSION });
        }
        let generation = r.u64()?;
        let next_seg = r.u64()?;
        let n = r.u32()? as usize;
        let mut segs = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            let id = r.u64()?;
            let n_sent = r.u32()?;
            let n_docs = r.u32()?;
            let body_hash = r.u64()?;
            let len = r.u32()? as usize;
            let deleted = RoaringBitmap::deserialize_from(r.take(len)?).map_err(|e| Error::Corrupt(format!("manifest: deleted bitmap: {e}")))?;
            let doc_len = r.u32()? as usize;
            let doc_deleted = RoaringBitmap::deserialize_from(r.take(doc_len)?).map_err(|e| Error::Corrupt(format!("manifest: doc_deleted bitmap: {e}")))?;
            if deleted.max().is_some_and(|m| m >= n_sent) || doc_deleted.max().is_some_and(|m| m >= n_docs) || id >= next_seg {
                return corrupt("manifest: segment entry out of range");
            }
            segs.push(SegEntry { id, n_sent, n_docs, body_hash, deleted, doc_deleted });
        }
        if r.pos != b.len() - 8 {
            return corrupt("manifest: trailing bytes");
        }
        Ok(Manifest { generation, next_seg, segs })
    }

    /// 목록 파일이 없으면 None(빈 색인).
    pub(crate) fn load(dir: &Path) -> Result<Option<Manifest>> {
        match std::fs::read(dir.join(MANIFEST_NAME)) {
            Ok(b) => Manifest::from_bytes(&b).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// 원자적 교체: 임시 파일에 쓰고 디스크에 내린 뒤 rename.
    pub(crate) fn save(&self, dir: &Path) -> Result<()> {
        let bytes = self.to_bytes()?;
        let tmp = dir.join(format!("{MANIFEST_TMP_PREFIX}{}-{}", std::process::id(), self.generation));
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&bytes)?;
            f.sync_all()?;
        }
        rename_retry(&tmp, &dir.join(MANIFEST_NAME))?;
        sync_dir(dir);
        Ok(())
    }
}

/// rename. Windows에서 다른 프로세스가 대상 파일을 잠깐 열고 있으면 거부될 수 있어 짧게 다시 시도한다.
pub(crate) fn rename_retry(from: &Path, to: &Path) -> Result<()> {
    let mut last = None;
    for attempt in 0..50u32 {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                last = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(2 + attempt as u64));
            }
            Err(e) => return Err(e.into()),
        }
    }
    Err(Error::Io(last.unwrap_or_else(|| std::io::Error::other("rename failed"))))
}

/// 폴더 항목(rename 결과)을 디스크에 내린다. Windows에는 해당 호출이 없어 파일 sync만으로 둔다.
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    {
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all();
        }
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_corruption() {
        let mut deleted = RoaringBitmap::new();
        deleted.insert_range(3..9);
        let mut doc_deleted = RoaringBitmap::new();
        doc_deleted.insert(0);
        let m = Manifest { generation: 7, next_seg: 5, segs: vec![SegEntry { id: 4, n_sent: 10, n_docs: 2, body_hash: 99, deleted, doc_deleted }] };
        let b = m.to_bytes().unwrap();
        let back = Manifest::from_bytes(&b).unwrap();
        assert_eq!((back.generation, back.next_seg, back.segs.len()), (7, 5, 1));
        assert_eq!(back.segs[0].deleted.len(), 6);
        for i in 0..b.len() {
            let mut bad = b.clone();
            bad[i] ^= 0x40;
            assert!(Manifest::from_bytes(&bad).is_err(), "flipped byte {i} accepted");
        }
        assert!(Manifest::from_bytes(&b[..b.len() - 3]).is_err());
    }
}
