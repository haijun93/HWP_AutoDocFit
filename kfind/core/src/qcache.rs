//! 질의 이력 캐시(KB4, 승인 설계 reports/IS-QCACHE-2026-11-12.md).
//!
//! 무엇을 담나: 해석한 질의(정규화한 낱말 + 나눈 조각)마다, 조각 파일마다 "일치 문장 비트맵"과 "붙어 있는 일치
//! 비트맵"을 **지운 문장을 빼기 전** 상태로 담는다. 최종 순위는 담지 않고 매번 다시 매긴다(순위 재계산).
//!
//! 열쇠: (정규화 질의, 조각 번호, 조각 본문 해시). 조각 파일은 한 번 쓰면 바뀌지 않으므로 같은 조각의 판정은 색인
//! 세대가 바뀌어도 그대로 맞다. 규칙 판(비교형 규칙·형식 버전)이 다르면 조각을 열 수 없어 섞이지 않는다.
//! 그래서 새 세대에서는
//! - 남아 있는 조각: 캐시에서 그대로 가져오고 그 세대의 지운 문장만 뺀다(삭제·수정된 문서는 빠진다),
//! - 새 조각(바뀐·새 문서): 그 조각만 판정한다(바뀐 문서만 재판정),
//! - 목록에서 빠진 조각(합쳐짐·비워짐): `retain_segments`가 버린다.
//!
//! 좁히기(N차): 새 질의의 일치가 앞 질의 일치의 부분집합임이 낱말만 보고 확실할 때(`implies`) 앞 결과 비트맵
//! 안에서만 판정한다. 그 밖에는 처음부터 판정한다.
//!
//! 상태(질의 하나): 없음 → 담김(조각 일부 또는 전부) → (조각이 목록에서 빠지면) 줄어듦 → 버림(상한·빈 질의).
//! 버림 규칙: 담은 바이트 합이 `max_bytes`를, 질의 수가 `max_queries`를 넘으면 가장 오래 안 쓴 질의부터 버린다.
//! 질의 하나가 `max_bytes`의 절반을 넘으면 담지 않는다.
use crate::query::QueryWord;
use roaring::RoaringBitmap;
use std::collections::HashMap;
use std::sync::Arc;

/// 조각 하나의 (일치 문장, 붙어 있는 일치 문장). 두 번째가 None이면 첫 번째와 같다.
pub(crate) type SegSets = (RoaringBitmap, Option<RoaringBitmap>);
/// (조각 번호, 조각 본문 해시).
pub(crate) type SegKey = (u64, u64);

#[derive(Clone, Debug)]
pub struct QueryCacheOptions {
    /// 담은 비트맵 바이트 합의 상한.
    pub max_bytes: usize,
    /// 담는 질의 수 상한.
    pub max_queries: usize,
}

impl Default for QueryCacheOptions {
    fn default() -> Self {
        QueryCacheOptions { max_bytes: 32 << 20, max_queries: 256 }
    }
}

/// 캐시 상태(화면·로그·에이전트가 읽는 값). 판정 수는 조각 단위로 센다.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueryCacheStats {
    /// 지금 담은 질의 수.
    pub queries: usize,
    /// 지금 담은 비트맵 바이트(추정).
    pub bytes: usize,
    /// 캐시에서 그대로 가져온 조각 판정 수.
    pub reused: u64,
    /// 앞 질의 결과 안에서만 판정한 조각 수(좁히기).
    pub narrowed: u64,
    /// 처음부터 판정한 조각 수.
    pub evaluated: u64,
    /// 상한 때문에 버린 질의 수.
    pub evicted: u64,
}

/// 조각 하나를 어떻게 판정할지.
pub(crate) enum Plan {
    /// 같은 질의·같은 조각의 판정이 있다.
    Reuse(Arc<SegSets>),
    /// 이 비트맵(앞 질의의 일치) 안에서만 판정한다.
    Narrow(Arc<SegSets>),
    /// 처음부터 판정한다.
    Fresh,
}

struct Entry {
    words: Vec<QueryWord>,
    segs: HashMap<SegKey, Arc<SegSets>>,
    bytes: usize,
    last_used: u64,
}

pub(crate) struct QueryCache {
    opts: QueryCacheOptions,
    entries: HashMap<String, Entry>,
    bytes: usize,
    clock: u64,
    stats: QueryCacheStats,
}

fn sets_bytes(s: &SegSets) -> usize {
    64 + s.0.serialized_size() + s.1.as_ref().map_or(0, |b| b.serialized_size())
}

/// 정규화 질의 열쇠: 낱말(조각 포함)을 정렬해 잇는다. 같은 문장 안 AND라 낱말 순서는 결과에 영향이 없다.
pub(crate) fn query_key(words: &[QueryWord]) -> String {
    let mut parts: Vec<String> = words
        .iter()
        .map(|w| match &w.pieces {
            Some(p) => format!("{}={}", w.text, p.join("+")),
            None => w.text.clone(),
        })
        .collect();
    parts.sort_unstable();
    parts.dedup();
    parts.join("\u{1}")
}

/// 낱말 n이 일치하는 문장이 모두 낱말 o에도 일치하는지(낱말만 보고 확실한 경우만 true).
/// 일치 = 붙어 있는 그대로(글 안의 부분 문자열) 또는, 조각이 있으면 조각이 모두 같은 문장에 있음.
fn word_implies(n: &QueryWord, o: &QueryWord) -> bool {
    // "글에 x가 들어 있으면 o에 일치": x가 o를 품거나, o의 조각을 모두 품으면.
    let covers = |x: &str| x.contains(o.text.as_str()) || o.pieces.as_ref().is_some_and(|op| op.iter().all(|q| x.contains(q.as_str())));
    if !covers(&n.text) {
        return false;
    }
    match &n.pieces {
        None => true,
        // n의 조각이 모두 있는 문장: 한 조각이 o를 품거나, o의 조각마다 그것을 품는 n의 조각이 있으면 o에 일치.
        Some(np) => np.iter().any(|p| p.contains(o.text.as_str())) || o.pieces.as_ref().is_some_and(|op| op.iter().all(|q| np.iter().any(|p| p.contains(q.as_str())))),
    }
}

/// 새 질의의 일치 ⊆ 앞 질의의 일치(앞 질의의 낱말마다 그것을 함의하는 새 낱말이 있다).
pub(crate) fn implies(new: &[QueryWord], old: &[QueryWord]) -> bool {
    !old.is_empty() && old.iter().all(|o| new.iter().any(|n| word_implies(n, o)))
}

impl QueryCache {
    pub(crate) fn new(opts: QueryCacheOptions) -> QueryCache {
        QueryCache { opts, entries: HashMap::new(), bytes: 0, clock: 0, stats: QueryCacheStats::default() }
    }

    pub(crate) fn stats(&self) -> QueryCacheStats {
        QueryCacheStats { queries: self.entries.len(), bytes: self.bytes, ..self.stats.clone() }
    }

    /// 조각마다 판정 방법을 정한다(`segs`와 같은 순서).
    pub(crate) fn plan(&mut self, key: &str, words: &[QueryWord], segs: &[SegKey]) -> Vec<Plan> {
        self.clock += 1;
        let clock = self.clock;
        let mut plans: Vec<Plan> = segs.iter().map(|_| Plan::Fresh).collect();
        if let Some(e) = self.entries.get_mut(key) {
            e.last_used = clock;
            for (p, sk) in plans.iter_mut().zip(segs) {
                if let Some(s) = e.segs.get(sk) {
                    *p = Plan::Reuse(s.clone());
                }
            }
        }
        if plans.iter().any(|p| matches!(p, Plan::Fresh)) {
            for (k, e) in self.entries.iter_mut() {
                if k == key || !implies(words, &e.words) {
                    continue;
                }
                let mut used = false;
                for (p, sk) in plans.iter_mut().zip(segs) {
                    let Some(s) = e.segs.get(sk) else { continue };
                    let better = match p {
                        Plan::Fresh => true,
                        Plan::Narrow(cur) => s.0.len() < cur.0.len(),
                        Plan::Reuse(_) => false,
                    };
                    if better {
                        *p = Plan::Narrow(s.clone());
                        used = true;
                    }
                }
                if used {
                    e.last_used = clock;
                }
            }
        }
        for p in &plans {
            match p {
                Plan::Reuse(_) => self.stats.reused += 1,
                Plan::Narrow(_) => self.stats.narrowed += 1,
                Plan::Fresh => self.stats.evaluated += 1,
            }
        }
        plans
    }

    /// 새로 판정한 조각들을 담고 상한을 지킨다.
    pub(crate) fn store(&mut self, key: &str, words: &[QueryWord], fresh: Vec<(SegKey, Arc<SegSets>)>) {
        if fresh.is_empty() {
            return;
        }
        let clock = self.clock;
        let e = self.entries.entry(key.to_string()).or_insert_with(|| Entry { words: words.to_vec(), segs: HashMap::new(), bytes: 0, last_used: clock });
        e.last_used = clock;
        for (sk, s) in fresh {
            let add = sets_bytes(&s);
            if let Some(old) = e.segs.insert(sk, s) {
                let sub = sets_bytes(&old);
                e.bytes -= sub;
                self.bytes -= sub;
            }
            e.bytes += add;
            self.bytes += add;
        }
        if e.bytes > self.opts.max_bytes / 2 {
            self.remove(key);
        }
        while self.bytes > self.opts.max_bytes || self.entries.len() > self.opts.max_queries {
            let Some(oldest) = self.entries.iter().min_by_key(|(_, e)| e.last_used).map(|(k, _)| k.clone()) else { break };
            self.remove(&oldest);
            self.stats.evicted += 1;
        }
    }

    fn remove(&mut self, key: &str) {
        if let Some(e) = self.entries.remove(key) {
            self.bytes -= e.bytes;
        }
    }

    /// 목록에 남은 조각의 판정만 남긴다(새 세대를 열 때). 조각이 하나도 남지 않은 질의는 버린다.
    pub(crate) fn retain_segments(&mut self, live: &[SegKey]) {
        let mut freed = 0usize;
        self.entries.retain(|_, e| {
            e.segs.retain(|sk, s| {
                let keep = live.contains(sk);
                if !keep {
                    let b = sets_bytes(s);
                    e.bytes -= b;
                    freed += b;
                }
                keep
            });
            !e.segs.is_empty()
        });
        self.bytes -= freed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(text: &str, pieces: Option<&[&str]>) -> QueryWord {
        QueryWord { text: text.into(), pieces: pieces.map(|p| p.iter().map(|s| s.to_string()).collect()) }
    }

    #[test]
    fn narrowing_rule() {
        // 낱말 추가(AND), 글자 추가.
        assert!(implies(&[w("예산", None), w("집행", None)], &[w("예산", None)]));
        assert!(implies(&[w("예산집", None)], &[w("예산", None)]));
        assert!(!implies(&[w("예산", None)], &[w("예산", None), w("집행", None)]));
        assert!(!implies(&[w("집행", None)], &[w("예산", None)]));
        // 조각이 생기면 넓어질 수 있다: "가나다" ⊄ "가나"+"다라".
        assert!(!implies(&[w("가나다라", Some(&["가나", "다라"]))], &[w("가나다", None)]));
        // 조각이 있어도 앞 낱말이 한 조각 안에 있으면 좁아진다.
        assert!(implies(&[w("예산집행", Some(&["예산", "집행"]))], &[w("예산", None)]));
        // 앞 낱말의 조각을 새 조각이 모두 품는다.
        assert!(implies(&[w("예산집행계획", Some(&["예산", "집행", "계획"]))], &[w("예산집행", Some(&["예산", "집행"]))]));
        assert!(!implies(&[w("예산집행계획", Some(&["예산", "집행계획"]))], &[w("예산집", None)]));
        assert!(!implies(&[w("예산", None)], &[]));
    }

    #[test]
    fn key_ignores_order_and_eviction_keeps_limits() {
        assert_eq!(query_key(&[w("예산", None), w("집행", None)]), query_key(&[w("집행", None), w("예산", None)]));
        assert_ne!(query_key(&[w("예산집행", None)]), query_key(&[w("예산집행", Some(&["예산", "집행"]))]));
        let mut bm = RoaringBitmap::new();
        bm.insert_range(0..1000);
        let one = sets_bytes(&(bm.clone(), None));
        let mut c = QueryCache::new(QueryCacheOptions { max_bytes: one * 3, max_queries: 10 });
        for (i, q) in ["가나", "나다", "다라", "라마"].iter().enumerate() {
            let words = [w(q, None)];
            let key = query_key(&words);
            assert!(matches!(c.plan(&key, &words, &[(1, 1)])[0], Plan::Fresh));
            c.store(&key, &words, vec![((1, 1), Arc::new((bm.clone(), None)))]);
            assert!(c.stats().bytes <= one * 3, "step {i}");
        }
        let s = c.stats();
        assert_eq!((s.queries, s.evicted), (3, 1));
        // 가장 오래 안 쓴 "가나"가 버려졌다.
        assert!(matches!(c.plan(&query_key(&[w("가나", None)]), &[w("가나", None)], &[(1, 1)])[0], Plan::Fresh));
        assert!(matches!(c.plan(&query_key(&[w("라마", None)]), &[w("라마", None)], &[(1, 1)])[0], Plan::Reuse(_)));
        c.retain_segments(&[(2, 2)]);
        assert_eq!((c.stats().queries, c.stats().bytes), (0, 0));
    }
}
