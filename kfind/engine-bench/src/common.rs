//! 두 엔진이 함께 쓰는 규칙: 비교형(fold), 질의 해석, 문서 순위(Q4), 결과 지문, 메모리 측정.
use std::io::{BufRead, BufReader};
use std::path::Path;

/// 비교형: 소문자화 후 글자·숫자만 남긴다(공백·문장부호 제거, 계획 §2.4·Q5).
/// NFC 정규화는 하지 않는다(시제품: 합성 자료는 이미 NFC, 정규화 크레이트 라이선스 문제로 S5에서 결정).
pub fn fold(s: &str) -> String {
    s.chars().flat_map(|c| c.to_lowercase()).filter(|c| c.is_alphanumeric()).collect()
}

/// 질의를 공백으로 낱말로 나누고 비교형으로 바꾼 뒤 2글자 미만은 뺀다(Q8). 같은 낱말은 한 번만.
pub fn parse_query(q: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in q.split_whitespace() {
        let f = fold(w);
        if f.chars().count() >= 2 && !out.contains(&f) {
            out.push(f);
        }
    }
    out
}

/// 문장 번호 → 문서 번호·위계 순위. 문장 번호는 말뭉치 줄 순서(0부터).
pub struct Meta {
    pub doc_of: Vec<u32>,
    pub level_of: Vec<u8>,
    pub ndocs: usize,
}

impl Meta {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.doc_of.len() * 5 + 8);
        out.extend_from_slice(&(self.doc_of.len() as u64).to_le_bytes());
        for d in &self.doc_of { out.extend_from_slice(&d.to_le_bytes()); }
        out.extend_from_slice(&self.level_of);
        out
    }
    pub fn from_bytes(b: &[u8]) -> Meta {
        let n = u64::from_le_bytes(b[..8].try_into().unwrap()) as usize;
        let doc_of: Vec<u32> = b[8..8 + n * 4].chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
        let level_of = b[8 + n * 4..8 + n * 5].to_vec();
        let ndocs = doc_of.iter().max().map(|m| *m as usize + 1).unwrap_or(0);
        Meta { doc_of, level_of, ndocs }
    }
}

/// 말뭉치 한 줄: `문서번호\t위계순위\t문장 원문`.
pub fn read_corpus(path: &Path, mut f: impl FnMut(u32, u32, u8, &str)) -> std::io::Result<()> {
    let reader = BufReader::with_capacity(1 << 20, std::fs::File::open(path)?);
    let mut sid = 0u32;
    for line in reader.lines() {
        let line = line?;
        let mut parts = line.splitn(3, '\t');
        let doc: u32 = parts.next().unwrap().parse().expect("doc");
        let level: u8 = parts.next().unwrap().parse().expect("level");
        let text = parts.next().unwrap_or("");
        f(sid, doc, level, text);
        sid += 1;
    }
    Ok(())
}

/// 문서 순위 한 줄: (문서, 가장 높은 위계=가장 작은 순위 값, 일치 문장 수).
pub type DocHit = (u32, u8, u32);

/// Q4: 가장 높은 위계 > 일치 문장 수 > (날짜: 합성 자료는 모두 같아 생략) > 문서 번호.
fn order(a: &DocHit, b: &DocHit) -> std::cmp::Ordering {
    a.1.cmp(&b.1).then(b.2.cmp(&a.2)).then(a.0.cmp(&b.0))
}

/// 일치 문장(오름차순)을 문서별로 모아 순위를 낸다. `top`이 있으면 상위 N개만 정렬해 돌려준다.
pub fn rank(meta: &Meta, sids: impl Iterator<Item = u32>, top: Option<usize>) -> (Vec<DocHit>, usize) {
    let mut hits: Vec<DocHit> = Vec::new();
    let mut slot: Vec<u32> = vec![u32::MAX; meta.ndocs];
    for sid in sids {
        let d = meta.doc_of[sid as usize];
        let lv = meta.level_of[sid as usize];
        let s = slot[d as usize];
        if s == u32::MAX {
            slot[d as usize] = hits.len() as u32;
            hits.push((d, lv, 1));
        } else {
            let h = &mut hits[s as usize];
            h.2 += 1;
            if lv < h.1 { h.1 = lv; }
        }
    }
    let total = hits.len();
    match top {
        Some(k) if k < hits.len() => {
            hits.select_nth_unstable_by(k, order);
            hits.truncate(k);
            hits.sort_unstable_by(order);
        }
        _ => hits.sort_unstable_by(order),
    }
    (hits, total)
}

/// FNV-1a 64. Python 기준 구현과 같은 방식으로 결과 지문을 낸다.
pub fn fnv(words: impl Iterator<Item = u32>) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for w in words {
        for b in w.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

pub fn rank_hash(hits: &[DocHit]) -> u64 {
    fnv(hits.iter().flat_map(|h| [h.0, h.1 as u32, h.2]))
}

/// (최대 작업 집합, 최대 개인 커밋, 현재 개인 커밋) 바이트.
#[cfg(windows)]
pub fn memory() -> (u64, u64, u64) {
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut c: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    if ok == 0 { return (0, 0, 0); }
    (c.PeakWorkingSetSize as u64, c.PeakPagefileUsage as u64, c.PagefileUsage as u64)
}

#[cfg(not(windows))]
pub fn memory() -> (u64, u64, u64) { (0, 0, 0) }

pub fn dir_size(p: &Path) -> u64 {
    let mut total = 0;
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            let m = match e.metadata() { Ok(m) => m, Err(_) => continue };
            total += if m.is_dir() { dir_size(&e.path()) } else { m.len() };
        }
    }
    total
}

/// 중앙값과 nearest-rank p95(ms).
pub fn stats(samples: &[f64]) -> serde_json::Value {
    let mut s = samples.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p = |q: f64| s[((s.len() as f64 * q).ceil() as usize).max(1) - 1];
    serde_json::json!({"n": s.len(), "p50_ms": p(0.5), "p95_ms": p(0.95), "max_ms": s[s.len() - 1], "min_ms": s[0]})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fold_removes_spaces_and_punct() {
        assert_eq!(fold("예산 집행, (계획)."), "예산집행계획");
        assert_eq!(fold("ABC_d-e"), "abcde");
    }
    #[test]
    fn parse_drops_short_and_dupes() {
        assert_eq!(parse_query("예산 및 집행 예산"), vec!["예산", "집행"]);
        assert!(parse_query("ㅇ").is_empty());
    }
    #[test]
    fn rank_orders_level_then_count_then_doc() {
        let meta = Meta { doc_of: vec![0, 0, 1, 1, 1, 2], level_of: vec![8, 8, 8, 8, 8, 3], ndocs: 3 };
        let (all, n) = rank(&meta, [0, 1, 2, 3, 4, 5].into_iter(), None);
        assert_eq!(n, 3);
        assert_eq!(all, vec![(2, 3, 1), (1, 8, 3), (0, 8, 2)]);
        let (top, _) = rank(&meta, [0, 1, 2, 3, 4, 5].into_iter(), Some(1));
        assert_eq!(top, vec![(2, 3, 1)]);
    }
}
