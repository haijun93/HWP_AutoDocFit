//! (B) 직접 구현: 비교형 글의 2·3글자 조각 → 문장 번호 roaring 비트맵. 조각 사전은 fst(조각 → 비트맵 위치).
//! 낱말 길이 2·3은 비트맵 하나가 곧 정확한 답, 4 이상은 3글자 조각 비트맵 교집합(후보) 뒤
//! 비교형 문장 원문(mmap)에서 memchr::memmem으로 확인한다.
use crate::common::{fold, read_corpus, Meta};
use crate::engine_a::cover_offsets;
use memmap2::Mmap;
use roaring::RoaringBitmap;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

pub fn build(corpus: &Path, out: &Path) -> std::io::Result<serde_json::Value> {
    std::fs::create_dir_all(out)?;
    let mut grams: HashMap<Vec<u8>, RoaringBitmap> = HashMap::new();
    let mut meta = Meta { doc_of: Vec::new(), level_of: Vec::new(), ndocs: 0 };
    let mut text = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(out.join("fold.bin"))?);
    let mut offsets: Vec<u8> = Vec::new();
    let mut pos: u64 = 0;
    let mut bounds: Vec<usize> = Vec::new();
    read_corpus(&corpus.join("corpus.tsv"), |sid, d, lv, raw| {
        let f = fold(raw);
        offsets.extend_from_slice(&pos.to_le_bytes());
        text.write_all(f.as_bytes()).expect("write");
        pos += f.len() as u64;
        bounds.clear();
        bounds.extend(f.char_indices().map(|(i, _)| i));
        bounds.push(f.len());
        for n in [2usize, 3] {
            for i in 0..bounds.len().saturating_sub(n) {
                let key = &f.as_bytes()[bounds[i]..bounds[i + n]];
                match grams.get_mut(key) {
                    // 같은 문장 안 중복 조각은 try_push가 거부한다(문장 번호가 늘어나는 순서로만 들어옴).
                    Some(bm) => { let _ = bm.try_push(sid); }
                    None => { let mut bm = RoaringBitmap::new(); bm.insert(sid); grams.insert(key.to_vec(), bm); }
                }
            }
        }
        meta.doc_of.push(d);
        meta.level_of.push(lv);
    })?;
    offsets.extend_from_slice(&pos.to_le_bytes());
    text.flush()?;
    std::fs::write(out.join("fold.idx"), &offsets)?;
    let mut keys: Vec<&Vec<u8>> = grams.keys().collect();
    keys.sort();
    let mut post = std::io::BufWriter::new(std::fs::File::create(out.join("post.bin"))?);
    let mut fst_b = fst::MapBuilder::new(std::io::BufWriter::new(std::fs::File::create(out.join("dict.fst"))?)).map_err(std::io::Error::other)?;
    let mut off: u64 = 0;
    for k in &keys {
        let bm = &grams[*k];
        fst_b.insert(k, off).map_err(std::io::Error::other)?;
        bm.serialize_into(&mut post)?;
        off += bm.serialized_size() as u64;
    }
    fst_b.finish().map_err(std::io::Error::other)?;
    post.flush()?;
    std::fs::write(out.join("meta.bin"), meta.to_bytes())?;
    Ok(serde_json::json!({"grams": keys.len(), "postings_bytes": off}))
}

pub struct EngineB {
    dict: fst::Map<Mmap>,
    post: Mmap,
    text: Mmap,
    idx: Mmap,
    pub meta: Meta,
}

fn map(p: &Path) -> std::io::Result<Mmap> {
    let f = std::fs::File::open(p)?;
    // SAFETY: 시험 중 색인 파일을 바꾸는 쪽이 없다(읽기 전용으로 연다).
    unsafe { Mmap::map(&f) }
}

impl EngineB {
    pub fn open(dir: &Path) -> std::io::Result<EngineB> {
        Ok(EngineB {
            dict: fst::Map::new(map(&dir.join("dict.fst"))?).map_err(std::io::Error::other)?,
            post: map(&dir.join("post.bin"))?,
            text: map(&dir.join("fold.bin"))?,
            idx: map(&dir.join("fold.idx"))?,
            meta: Meta::from_bytes(&std::fs::read(dir.join("meta.bin"))?),
        })
    }

    fn gram(&self, key: &str) -> RoaringBitmap {
        match self.dict.get(key) {
            Some(off) => RoaringBitmap::deserialize_unchecked_from(&self.post[off as usize..]).expect("posting"),
            None => RoaringBitmap::new(),
        }
    }

    fn sentence(&self, sid: u32) -> &[u8] {
        let i = sid as usize * 8;
        let a = u64::from_le_bytes(self.idx[i..i + 8].try_into().unwrap()) as usize;
        let b = u64::from_le_bytes(self.idx[i + 8..i + 16].try_into().unwrap()) as usize;
        &self.text[a..b]
    }

    /// 낱말 하나의 정확한 일치 문장. restrict가 있으면 그 안에서만(4글자 이상 확인 비용을 줄인다).
    pub fn word(&self, w: &str, restrict: Option<&RoaringBitmap>) -> RoaringBitmap {
        let chars: Vec<usize> = w.char_indices().map(|(i, _)| i).chain([w.len()]).collect();
        let n = chars.len() - 1;
        if n <= 3 {
            let mut bm = self.gram(w);
            if let Some(r) = restrict { bm &= r; }
            return bm;
        }
        let mut parts: Vec<RoaringBitmap> = cover_offsets(n).into_iter().map(|o| self.gram(&w[chars[o]..chars[o + 3]])).collect();
        parts.sort_by_key(|b| b.len());
        let mut cand = parts.swap_remove(0);
        if let Some(r) = restrict { cand &= r; }
        for p in &parts { cand &= p; }
        let finder = memchr::memmem::Finder::new(w.as_bytes());
        RoaringBitmap::from_sorted_iter(cand.iter().filter(|&s| finder.find(self.sentence(s)).is_some())).expect("sorted")
    }

    /// 2·3글자 낱말은 비트맵 교집합(작은 것부터), 4글자 이상은 그 교집합 안에서 확인.
    pub fn query(&self, words: &[String]) -> RoaringBitmap {
        if words.is_empty() { return RoaringBitmap::new(); }
        let (short, long): (Vec<&String>, Vec<&String>) = words.iter().partition(|w| w.chars().count() <= 3);
        let mut acc: Option<RoaringBitmap> = None;
        let mut bms: Vec<RoaringBitmap> = short.iter().map(|w| self.gram(w)).collect();
        bms.sort_by_key(|b| b.len());
        for b in bms {
            acc = Some(match acc { None => b, Some(mut a) => { a &= &b; a } });
        }
        for w in long {
            let r = self.word(w, acc.as_ref());
            acc = Some(r);
        }
        acc.unwrap_or_default()
    }
}
