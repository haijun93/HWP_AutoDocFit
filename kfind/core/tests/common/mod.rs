//! 시험 공용: Python 기준 구현이 만든 대조 표(tests/data/oracle_*.tsv) 읽기와 비교.
#![allow(dead_code)]
use kfind_core::{FNV_SEED, Index, SearchOptions, Sentence, SplitMode, Writer, fnv64};
use std::path::{Path, PathBuf};

pub struct Sent {
    pub para: u32,
    pub level: u8,
    pub text: String,
}

pub struct Doc {
    pub id: u64,
    pub date: i64,
    pub sents: Vec<Sent>,
}

pub struct Expect {
    pub mode: SplitMode,
    pub mode_name: String,
    pub query: String,
    pub words: String,
    pub dropped: String,
    pub sentences: u64,
    pub docs: usize,
    pub adjacent_docs: usize,
    pub sid_hash: String,
    pub rank_hash: String,
    pub top: String,
}

fn data(name: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// "C608 C0B0" → "예산". "-"는 빈 글.
pub fn from_hex(s: &str) -> String {
    if s == "-" {
        return String::new();
    }
    s.split(' ').map(|h| char::from_u32(u32::from_str_radix(h, 16).expect("hex")).expect("char")).collect()
}

pub fn rows(name: &str) -> Vec<Vec<String>> {
    data(name).lines().filter(|l| !l.starts_with('#') && !l.is_empty()).map(|l| l.split('\t').map(str::to_string).collect()).collect()
}

pub fn corpus(state: u32) -> Vec<Doc> {
    let mut docs: Vec<Doc> = Vec::new();
    for r in rows(&format!("oracle_corpus_{state}.tsv")) {
        let id: u64 = r[0].parse().expect("doc id");
        if docs.last().map(|d| d.id) != Some(id) {
            docs.push(Doc { id, date: r[1].parse().expect("date"), sents: Vec::new() });
        }
        let d = docs.last_mut().expect("doc");
        d.sents.push(Sent { para: r[2].parse().expect("para"), level: r[3].parse().expect("level"), text: from_hex(&r[4]) });
    }
    docs
}

pub fn expect(state: u32) -> Vec<Expect> {
    rows(&format!("oracle_expect_{state}.tsv"))
        .into_iter()
        .map(|r| Expect {
            mode: match r[0].as_str() {
                "off" => SplitMode::Off,
                "dict" => SplitMode::DictLongest,
                "bigram" => SplitMode::Bigram,
                other => panic!("mode {other}"),
            },
            mode_name: r[0].clone(),
            query: from_hex(&r[1]),
            words: r[2].clone(),
            dropped: r[3].clone(),
            sentences: r[4].parse().expect("sentences"),
            docs: r[5].parse().expect("docs"),
            adjacent_docs: r[6].parse().expect("adjacent"),
            sid_hash: r[7].clone(),
            rank_hash: r[8].clone(),
            top: r[9].clone(),
        })
        .collect()
}

/// 시험마다 빈 임시 폴더(cargo의 target/tmp 아래).
pub fn tmp_dir(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("kfind-core-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("tmp dir");
    p
}

pub fn add(w: &mut Writer, d: &Doc) {
    let sents: Vec<Sentence> = d.sents.iter().map(|s| Sentence { para: s.para, level: s.level, text: &s.text }).collect();
    w.add_document(d.id, d.date, &sents).expect("add_document");
}

pub fn hash_u64s(values: impl Iterator<Item = u64>) -> String {
    let mut h = FNV_SEED;
    for v in values {
        h = fnv64(h, &v.to_le_bytes());
    }
    format!("{h:016x}")
}

pub fn opts(mode: SplitMode, top: usize) -> SearchOptions {
    SearchOptions { top, split: mode, time_limit: None, cancel: None, refs_per_doc: 2, ..SearchOptions::default() }
}

/// `rank`(미리보기 없이 순위만)가 `search`와 같은 순위·합계를 내고, 문서별 미리보기를 나중에 만들어도
/// `search`의 미리보기와 같은지 본다(KB2). 대조 표의 순위 지문도 함께 본다. 확인한 줄 수를 돌려준다.
pub fn check_rank(index: &Index, expected: &[Expect], label: &str) -> usize {
    let strip = |hits: &[kfind_core::DocHit]| -> Vec<(u64, bool, u8, u32, i64)> { hits.iter().map(|h| (h.doc_id, h.adjacent, h.level, h.count, h.date)).collect() };
    for e in expected {
        let ctx = format!("[{label}] mode={} query={:?}", e.mode_name, e.query);
        for top in [0usize, 1, 20] {
            let full = index.search(&e.query, &opts(e.mode, top)).expect("search");
            let ranking = index.rank(&e.query, &opts(e.mode, top)).expect("rank");
            let r = &ranking.result;
            assert_eq!((r.total_sentences, r.total_docs, r.adjacent_docs, r.generation), (e.sentences, e.docs, e.adjacent_docs, index.generation()), "{ctx} top={top}: totals");
            assert_eq!(strip(&r.hits), strip(&full.hits), "{ctx} top={top}: order");
            assert!(r.hits.iter().all(|h| h.sentences.is_empty()), "{ctx}: rank builds no previews");
            for (i, h) in full.hits.iter().enumerate() {
                assert_eq!(ranking.sentences(index, i, 2).expect("sentences"), h.sentences, "{ctx} top={top}: preview of hit {i}");
            }
            assert!(ranking.sentences(index, r.hits.len(), 2).is_err(), "{ctx}: hit out of range");
            if top == 0 {
                let rank = hash_u64s(r.hits.iter().flat_map(|h| [h.doc_id, h.adjacent as u64, h.level as u64, h.count as u64]));
                assert_eq!(rank, e.rank_hash, "{ctx}: rank order fingerprint");
            }
        }
    }
    expected.len()
}

/// `count`(개수만)가 대조 표의 문장 수·문서 수, `search`의 낱말 해석과 같은지 본다(KB2). 확인한 줄 수를 돌려준다.
pub fn check_count(index: &Index, expected: &[Expect], label: &str) -> usize {
    for e in expected {
        let ctx = format!("[{label}] mode={} query={:?}", e.mode_name, e.query);
        let c = index.count(&e.query, &opts(e.mode, 20)).expect("count");
        assert_eq!((c.total_sentences, c.total_docs, c.generation), (e.sentences, e.docs, index.generation()), "{ctx}: counts");
        let s = index.search(&e.query, &opts(e.mode, 20)).expect("search");
        assert_eq!((&c.words, &c.dropped), (&s.words, &s.dropped), "{ctx}: words");
    }
    expected.len()
}

/// 색인 결과가 대조 표의 모든 줄과 같은지 본다. 확인한 줄 수를 돌려준다.
pub fn check_all(index: &Index, expected: &[Expect], label: &str) -> usize {
    for e in expected {
        let ctx = format!("[{label}] mode={} query={:?}", e.mode_name, e.query);
        let parsed = index.resolve(&e.query, e.mode).expect("resolve");
        let words: Vec<String> = parsed.words.iter().map(|w| match &w.pieces {
            Some(p) => format!("{}={}", w.text, p.join("+")),
            None => w.text.clone(),
        }).collect();
        let words = if words.is_empty() { "-".to_string() } else { words.join(" ") };
        assert_eq!(words, e.words, "{ctx}: words");
        let dropped = if parsed.dropped.is_empty() { "-".to_string() } else { parsed.dropped.join(" ") };
        assert_eq!(dropped, e.dropped, "{ctx}: dropped");

        let full = index.search(&e.query, &opts(e.mode, 0)).expect("search");
        assert_eq!(full.total_sentences, e.sentences, "{ctx}: sentences");
        assert_eq!(full.total_docs, e.docs, "{ctx}: docs");
        assert_eq!(full.hits.len(), e.docs, "{ctx}: hits");
        assert_eq!(full.adjacent_docs, e.adjacent_docs, "{ctx}: adjacent docs");
        let rank = hash_u64s(full.hits.iter().flat_map(|h| [h.doc_id, h.adjacent as u64, h.level as u64, h.count as u64]));
        assert_eq!(rank, e.rank_hash, "{ctx}: rank order");
        let top_of = |hits: &[kfind_core::DocHit]| -> String {
            let v: Vec<String> = hits.iter().take(20).map(|h| format!("{}:{}:{}:{}", h.doc_id, h.adjacent as u8, h.level, h.count)).collect();
            if v.is_empty() { "-".to_string() } else { v.join(";") }
        };
        assert_eq!(top_of(&full.hits), e.top, "{ctx}: top 20 of full");
        let top = index.search(&e.query, &opts(e.mode, 20)).expect("search top");
        assert_eq!(top_of(&top.hits), e.top, "{ctx}: top 20");
        assert_eq!((top.total_docs, top.total_sentences), (e.docs, e.sentences), "{ctx}: totals with top");

        let sents = index.match_sentences(&e.query, &opts(e.mode, 0)).expect("match_sentences");
        assert_eq!(sents.len() as u64, e.sentences, "{ctx}: sentence list");
        let sid = hash_u64s(sents.iter().flat_map(|m| [m.doc_id, m.sentence.ord as u64, m.sentence.para as u64, m.sentence.level as u64, m.sentence.adjacent as u64]));
        assert_eq!(sid, e.sid_hash, "{ctx}: sentence set");
        // 문서별 미리보기 문장은 그 문서의 일치 문장 앞 2개와 같아야 한다.
        for h in top.hits.iter().take(5) {
            let own: Vec<_> = sents.iter().filter(|m| m.doc_id == h.doc_id).map(|m| m.sentence.clone()).take(2).collect();
            assert_eq!(h.sentences, own, "{ctx}: preview sentences of {}", h.doc_id);
        }
    }
    expected.len()
}
