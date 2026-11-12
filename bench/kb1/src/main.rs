//! KB1: 같은 합성 말뭉치(IS-S1 bench.py, seed 7)로 kfind-core · Tantivy(2글자 n-gram) · SeekStorm 비교.
//! 측정 전용 도구다. 본 제품 크레이트의 의존성이 아니다.
//!
//! kb1 build <kfind|tantivy|seekstorm> <corpus.tsv> <색인 폴더>
//! kb1 query <kfind|tantivy|seekstorm> <색인 폴더> <검색어> <반복 수>
//! 결과는 JSON 한 줄로 표준 출력에 쓴다. 검색어마다 새 프로세스로 부르면 첫 검색이 "콜드"(프로세스·색인 열기 직후)다.
//!
//! 공통 의미(공정 비교용): 문장 하나 = 검색 단위. 비교형 = 소문자 + 글자·숫자만(공백·문장부호 제거).
//! 검색어를 공백으로 나눈 낱말(2글자 이상, 중복 제거)이 모두 같은 문장의 비교형에 부분 문자열로 있으면 일치.
//! Tantivy·SeekStorm은 비교형을 겹치는 2글자 조각으로 색인하고, 2글자 낱말은 조각 하나,
//! 3글자 이상은 조각들의 구문(연속 위치) 질의로 같은 부분 문자열 일치를 낸다.
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::Instant;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// (최대 작업 집합 = 최대 RSS, 최대 개인 커밋) MB.
#[cfg(windows)]
fn memory_mb() -> (f64, f64) {
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    // SAFETY: 0으로 채운 구조체는 유효하고 크기를 cb에 적어 넘긴다.
    let mut c: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    // SAFETY: 현재 프로세스 핸들과 방금 만든 구조체.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    if ok == 0 {
        return (0.0, 0.0);
    }
    (c.PeakWorkingSetSize as f64 / 1048576.0, c.PeakPagefileUsage as f64 / 1048576.0)
}
#[cfg(not(windows))]
fn memory_mb() -> (f64, f64) {
    (0.0, 0.0)
}

fn dir_size(p: &Path) -> u64 {
    let mut total = 0;
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            let Ok(m) = e.metadata() else { continue };
            total += if m.is_dir() { dir_size(&e.path()) } else { m.len() };
        }
    }
    total
}

fn fold(s: &str) -> String {
    s.chars().flat_map(|c| c.to_lowercase()).filter(|c| c.is_alphanumeric()).collect()
}

/// 공백으로 나눈 낱말 → 비교형, 2글자 미만 제외, 중복 제거(kfind-core 끔 모드와 같은 규칙).
fn parse_query(q: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in q.split_whitespace() {
        let f = fold(w);
        if f.chars().count() >= 2 && !out.contains(&f) {
            out.push(f);
        }
    }
    out
}

/// 겹치는 2글자 조각.
fn bigrams(s: &str) -> Vec<String> {
    let c: Vec<char> = s.chars().collect();
    c.windows(2).map(|w| w.iter().collect()).collect()
}

/// 말뭉치 한 줄: `문서번호\t위계순위\t문장`. 문장 번호는 줄 순서.
fn read_corpus(path: &Path, mut f: impl FnMut(u32, u32, u8, &str)) {
    let reader = BufReader::with_capacity(1 << 20, std::fs::File::open(path).expect("corpus"));
    for (sid, line) in reader.lines().enumerate() {
        let line = line.expect("line");
        let mut p = line.splitn(3, '\t');
        let doc: u32 = p.next().unwrap().parse().expect("doc");
        let level: u8 = p.next().unwrap().parse().expect("level");
        f(sid as u32, doc, level, p.next().unwrap_or(""));
    }
}

/// 문장 번호 → 문서 번호·위계(Tantivy·SeekStorm은 문장 = 문서라 문서 묶기를 밖에서 한다).
struct Meta {
    doc_of: Vec<u32>,
    level_of: Vec<u8>,
    ndocs: usize,
}
impl Meta {
    fn save(&self, p: &Path) {
        let mut out = Vec::with_capacity(self.doc_of.len() * 5 + 8);
        out.extend_from_slice(&(self.doc_of.len() as u64).to_le_bytes());
        for d in &self.doc_of {
            out.extend_from_slice(&d.to_le_bytes());
        }
        out.extend_from_slice(&self.level_of);
        std::fs::write(p, out).expect("meta");
    }
    fn load(p: &Path) -> Meta {
        let b = std::fs::read(p).expect("meta");
        let n = u64::from_le_bytes(b[..8].try_into().unwrap()) as usize;
        let doc_of: Vec<u32> = b[8..8 + n * 4].chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
        let level_of = b[8 + n * 4..8 + n * 5].to_vec();
        let ndocs = doc_of.iter().max().map(|m| *m as usize + 1).unwrap_or(0);
        Meta { doc_of, level_of, ndocs }
    }
}

/// 문서 순위(IS-S1 시제품·kfind-core 대조 규칙과 같음): 위계 높은 순 > 일치 문장 수 많은 순 > 문서 번호.
/// 반환: (전체 문서 수, 상위 N [(문서, 위계, 문장 수)]).
fn rank(meta: &Meta, sids: impl Iterator<Item = u32>, top: usize) -> (usize, Vec<(u32, u8, u32)>) {
    let mut hits: Vec<(u32, u8, u32)> = Vec::new();
    let mut slot = vec![u32::MAX; meta.ndocs];
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
            h.1 = h.1.min(lv);
        }
    }
    let total = hits.len();
    let order = |a: &(u32, u8, u32), b: &(u32, u8, u32)| a.1.cmp(&b.1).then(b.2.cmp(&a.2)).then(a.0.cmp(&b.0));
    if top < hits.len() {
        hits.select_nth_unstable_by(top, order);
        hits.truncate(top);
    }
    hits.sort_unstable_by(order);
    (total, hits)
}

fn stats(samples: &[f64]) -> serde_json::Value {
    let mut s = samples.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p = |q: f64| s[((s.len() as f64 * q).ceil() as usize).max(1) - 1];
    serde_json::json!({"n": s.len(), "p50_ms": p(0.5), "p95_ms": p(0.95), "min_ms": s[0], "max_ms": s[s.len() - 1]})
}

/// 한 질의 변형의 결과: 일치 문장 수, 문서 수(모르면 None), 상위 20 문서.
#[derive(Clone)]
struct Out {
    sentences: u64,
    docs: Option<usize>,
    top: Vec<(u32, u8, u32)>,
}
impl Out {
    fn json(&self) -> serde_json::Value {
        serde_json::json!({"sentences": self.sentences, "docs": self.docs, "top20": self.top.iter().map(|h| [h.0, h.1 as u32, h.2]).collect::<Vec<_>>()})
    }
}

/// 변형 이름과 실행 함수. 첫 변형의 첫 실행을 콜드로 잰다.
type Variant<'a> = (&'static str, Box<dyn FnMut() -> Out + 'a>);

fn run_variants(mut variants: Vec<Variant>, reps: usize, open_ms: f64, extra: impl FnOnce() -> serde_json::Value) {
    let t = Instant::now();
    let cold = (variants[0].1)();
    let cold_ms = ms(t);
    let mut rows = Vec::new();
    for (name, f) in variants.iter_mut() {
        let first = f();
        let mut s = Vec::with_capacity(reps);
        for _ in 0..reps {
            let t = Instant::now();
            let r = f();
            s.push(ms(t));
            std::hint::black_box(r);
        }
        rows.push(serde_json::json!({"variant": name, "result": first.json(), "warm": stats(&s)}));
    }
    let (ws, private) = memory_mb();
    let cold_variant = variants[0].0;
    drop(variants);
    let extra = extra();
    println!(
        "{}",
        serde_json::json!({"open_ms": open_ms, "cold_variant": cold_variant, "cold_ms": cold_ms, "cold_result": cold.json(),
            "variants": rows, "peak_working_set_mb": ws, "peak_private_mb": private, "extra": extra})
    );
}

// ───────────────────────── kfind-core ─────────────────────────
mod kf {
    use super::*;
    use kfind_core::{Index, SearchOptions, Sentence, SplitMode, Writer, WriterOptions};

    pub fn build(corpus: &Path, dir: &Path) -> serde_json::Value {
        let t = Instant::now();
        let mut w = Writer::create(dir, WriterOptions::default()).expect("writer");
        let (mut cur, mut sents): (Option<u32>, Vec<(u8, String)>) = (None, Vec::new());
        let flush = |w: &mut Writer, d: u32, sents: &[(u8, String)]| {
            let v: Vec<Sentence> = sents.iter().enumerate().map(|(i, (lv, t))| Sentence { para: i as u32, level: *lv, text: t }).collect();
            w.add_document(d as u64, 0, &v).expect("add");
        };
        read_corpus(corpus, |_, d, lv, text| {
            if cur != Some(d) {
                if let Some(c) = cur {
                    flush(&mut w, c, &sents);
                }
                sents.clear();
                cur = Some(d);
            }
            sents.push((lv, text.to_string()));
        });
        if let Some(c) = cur {
            flush(&mut w, c, &sents);
        }
        w.commit().expect("commit");
        let commit_s = t.elapsed().as_secs_f64();
        w.merge_all().expect("merge");
        serde_json::json!({"commit_s": commit_s, "segments": w.num_segments()})
    }

    pub fn query(dir: &Path, q: &str, reps: usize) {
        let t = Instant::now();
        let index = Index::open(dir).expect("open");
        let open_ms = ms(t);
        let opts = |top: usize, split: SplitMode| SearchOptions { top, split, time_limit: None, cancel: None, refs_per_doc: 3 };
        let run = |o: SearchOptions| {
            let r = index.search(q, &o).expect("search");
            Out { sentences: r.total_sentences as u64, docs: Some(r.total_docs as usize), top: r.hits.iter().map(|h| (h.doc_id as u32, h.level, h.count as u32)).collect() }
        };
        let variants: Vec<Variant> = vec![
            ("top20_default(dict)", Box::new(|| run(opts(20, SplitMode::DictLongest)))),
            ("top20_off", Box::new(|| run(opts(20, SplitMode::Off)))),
            ("full_off", Box::new(|| run(opts(0, SplitMode::Off)))),
        ];
        // 나누기 결과 기록: run_variants가 측정을 마친 뒤에 부른다(콜드 측정에 영향 없음).
        let extra = || {
            let r = index.search(q, &opts(20, SplitMode::DictLongest)).expect("search");
            let words: Vec<String> = r.words.iter().map(|w| match &w.pieces { Some(p) => format!("{}={}", w.text, p.join("+")), None => w.text.clone() }).collect();
            serde_json::json!({"segments": index.num_segments(), "dict_words": words.join(" ")})
        };
        run_variants(variants, reps, open_ms, extra);
    }
}

// ───────────────────────── Tantivy ─────────────────────────
mod tv {
    use super::*;
    use tantivy::collector::{Collector, Count, SegmentCollector};
    use tantivy::columnar::Column;
    use tantivy::query::{BooleanQuery, Occur, PhraseQuery, Query, TermQuery};
    use tantivy::schema::{Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, FAST};
    use tantivy::tokenizer::{Token, TokenStream, Tokenizer};
    use tantivy::{doc, DocId, Index, Score, SegmentOrdinal, SegmentReader, TantivyDocument, Term};

    /// 비교형 글을 겹치는 2글자 조각으로 자르는 토크나이저. 위치 = 시작 글자 번호.
    #[derive(Clone)]
    struct Bigram;
    struct BigramStream<'a> {
        text: &'a str,
        bounds: Vec<usize>,
        i: usize,
        token: Token,
    }
    impl Tokenizer for Bigram {
        type TokenStream<'a> = BigramStream<'a>;
        fn token_stream<'a>(&'a mut self, text: &'a str) -> BigramStream<'a> {
            let mut bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
            bounds.push(text.len());
            BigramStream { text, bounds, i: 0, token: Token::default() }
        }
    }
    impl TokenStream for BigramStream<'_> {
        fn advance(&mut self) -> bool {
            if self.i + 2 >= self.bounds.len() {
                return false;
            }
            let (a, b) = (self.bounds[self.i], self.bounds[self.i + 2]);
            self.token.text.clear();
            self.token.text.push_str(&self.text[a..b]);
            self.token.offset_from = a;
            self.token.offset_to = b;
            self.token.position = self.i;
            self.token.position_length = 1;
            self.i += 1;
            true
        }
        fn token(&self) -> &Token {
            &self.token
        }
        fn token_mut(&mut self) -> &mut Token {
            &mut self.token
        }
    }

    pub fn build(corpus: &Path, dir: &Path) -> serde_json::Value {
        let t = Instant::now();
        std::fs::create_dir_all(dir).expect("dir");
        let mut sb = Schema::builder();
        let bi = sb.add_text_field("bi", TextOptions::default().set_indexing_options(
            TextFieldIndexing::default().set_tokenizer("ko2").set_index_option(IndexRecordOption::WithFreqsAndPositions)));
        let sid_f = sb.add_u64_field("sid", FAST);
        let index = Index::create_in_dir(dir, sb.build()).expect("create");
        index.tokenizers().register("ko2", Bigram);
        // 기본 스레드 수(코어 수, 최대 8)와 쓰기 버퍼 200MB. 배경 합치기는 끄고 커밋 뒤 한 조각으로 합친다(kfind-core와 같은 최종 상태).
        let mut writer = index.writer::<TantivyDocument>(200_000_000).expect("writer");
        writer.set_merge_policy(Box::new(tantivy::indexer::NoMergePolicy));
        let mut meta = Meta { doc_of: Vec::new(), level_of: Vec::new(), ndocs: 0 };
        read_corpus(corpus, |sid, d, lv, text| {
            writer.add_document(doc!(bi => fold(text), sid_f => sid as u64)).expect("add");
            meta.doc_of.push(d);
            meta.level_of.push(lv);
        });
        writer.commit().expect("commit");
        let commit_s = t.elapsed().as_secs_f64();
        let ids = index.searchable_segment_ids().expect("ids");
        let before = ids.len();
        if ids.len() > 1 {
            writer.merge(&ids).wait().expect("merge");
        }
        writer.wait_merging_threads().expect("wait");
        meta.save(&dir.join("meta.bin"));
        serde_json::json!({"commit_s": commit_s, "segments_before_merge": before, "threads": "default"})
    }

    struct SidCollector;
    struct SidSegment {
        col: Column<u64>,
        out: Vec<u32>,
    }
    impl Collector for SidCollector {
        type Fruit = Vec<u32>;
        type Child = SidSegment;
        fn for_segment(&self, _: SegmentOrdinal, seg: &SegmentReader) -> tantivy::Result<SidSegment> {
            Ok(SidSegment { col: seg.fast_fields().u64("sid")?, out: Vec::new() })
        }
        fn requires_scoring(&self) -> bool {
            false
        }
        fn merge_fruits(&self, fruits: Vec<Vec<u32>>) -> tantivy::Result<Vec<u32>> {
            let mut all = fruits.concat();
            all.sort_unstable();
            Ok(all)
        }
    }
    impl SegmentCollector for SidSegment {
        type Fruit = Vec<u32>;
        fn collect(&mut self, doc: DocId, _: Score) {
            self.out.push(self.col.first(doc).unwrap_or(0) as u32);
        }
        fn harvest(self) -> Vec<u32> {
            self.out
        }
    }

    fn word_query(bi: Field, w: &str) -> Box<dyn Query> {
        let g = bigrams(w);
        if g.len() == 1 {
            Box::new(TermQuery::new(Term::from_field_text(bi, &g[0]), IndexRecordOption::Basic))
        } else {
            Box::new(PhraseQuery::new_with_offset(g.iter().enumerate().map(|(i, s)| (i, Term::from_field_text(bi, s))).collect()))
        }
    }

    fn build_query(bi: Field, words: &[String]) -> Option<Box<dyn Query>> {
        match words.len() {
            0 => None,
            1 => Some(word_query(bi, &words[0])),
            _ => Some(Box::new(BooleanQuery::new(words.iter().map(|w| (Occur::Must, word_query(bi, w))).collect()))),
        }
    }

    pub fn query(dir: &Path, q: &str, reps: usize, split: Option<Vec<String>>) {
        let t = Instant::now();
        let index = Index::open_in_dir(dir).expect("open");
        index.tokenizers().register("ko2", Bigram);
        let reader = index.reader_builder().reload_policy(tantivy::ReloadPolicy::Manual).try_into().expect("reader");
        let meta = Meta::load(&dir.join("meta.bin"));
        let bi = index.schema().get_field("bi").expect("bi");
        let open_ms = ms(t);
        let searcher = reader.searcher();
        let words = parse_query(q);
        let mk = |words: &[String]| {
            let words = words.to_vec();
            let searcher = searcher.clone();
            let meta = &meta;
            (
                {
                    let (s, w) = (searcher.clone(), words.clone());
                    move || -> Out {
                        let Some(qq) = build_query(bi, &w) else { return Out { sentences: 0, docs: Some(0), top: vec![] } };
                        let sids = s.search(qq.as_ref(), &SidCollector).expect("search");
                        let (docs, top) = rank(meta, sids.iter().copied(), 20);
                        Out { sentences: sids.len() as u64, docs: Some(docs), top }
                    }
                },
                move || -> Out {
                    let Some(qq) = build_query(bi, &words) else { return Out { sentences: 0, docs: None, top: vec![] } };
                    let n = searcher.search(qq.as_ref(), &Count).expect("search");
                    Out { sentences: n as u64, docs: None, top: vec![] }
                },
            )
        };
        let (full, count) = mk(&words);
        let mut variants: Vec<Variant> = vec![("full_rank_top20", Box::new(full)), ("count", Box::new(count))];
        if let Some(sw) = split {
            let (f2, _) = mk(&sw);
            variants.push(("split_full_rank_top20", Box::new(f2)));
        }
        let segs = searcher.segment_readers().len();
        run_variants(variants, reps, open_ms, move || serde_json::json!({"segments": segs, "words": words}));
    }
}

// ───────────────────────── SeekStorm ─────────────────────────
mod ss {
    use super::*;
    use seekstorm::commit::Commit;
    use seekstorm::index::{
        create_index, open_index, AccessType, Clustering, Close, DocumentCompression, FileType, FrequentwordType, IndexDocument, IndexMetaObject,
        LexicalSimilarity, NgramSet, StemmerType, StopwordType, TokenizerType,
    };
    use seekstorm::search::{QueryRewriting, QueryType, ResultType, Search, SearchMode};
    use seekstorm::vector::Inference;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio")
    }

    pub fn build(corpus: &Path, dir: &Path) -> serde_json::Value {
        let t = Instant::now();
        let rt = rt();
        let meta = rt.block_on(async {
            let schema = serde_json::from_str(r#"[{"field":"b","field_type":"Text","store":false,"index_lexical":true}]"#).unwrap();
            let im = IndexMetaObject {
                id: 0,
                name: "kb1".to_string(),
                lexical_similarity: LexicalSimilarity::Bm25f,
                tokenizer: TokenizerType::Whitespace,
                stemmer: StemmerType::None,
                stop_words: StopwordType::None,
                frequent_words: FrequentwordType::None,
                ngram_indexing: NgramSet::SingleTerm as u8,
                document_compression: DocumentCompression::Snappy,
                access_type: AccessType::Mmap,
                spelling_correction: None,
                query_completion: None,
                clustering: Clustering::None,
                inference: Inference::None,
                segment_number_bits: 11,
            };
            let index = create_index(dir, im, &schema, &Vec::new(), true, None).await.expect("create");
            let mut meta = Meta { doc_of: Vec::new(), level_of: Vec::new(), ndocs: 0 };
            let mut rows: Vec<(u32, u8, String)> = Vec::new();
            read_corpus(corpus, |_, d, lv, text| rows.push((d, lv, text.to_string())));
            // 문서 ID는 넣은 순서대로 0,1,2…(index_document가 전역 번호를 차례로 매김) → 문장 번호와 같다.
            for (d, lv, text) in rows {
                let mut doc = seekstorm::index::Document::new();
                doc.insert("b".to_string(), serde_json::Value::String(bigrams(&fold(&text)).join(" ")));
                index.index_document(doc, FileType::None).await;
                meta.doc_of.push(d);
                meta.level_of.push(lv);
            }
            index.commit().await;
            let shards = index.read().await.shard_count().await;
            index.close().await;
            (meta, shards)
        });
        let total_s = t.elapsed().as_secs_f64();
        meta.0.save(&dir.join("kb1-meta.bin"));
        serde_json::json!({"commit_close_s": total_s, "shards": meta.1})
    }

    /// 낱말 → SeekStorm 질의 문자열. 2글자는 낱말 하나, 3글자 이상은 조각 구문("…").
    /// SeekStorm 질의 해석기는 구문이 하나라도 있으면 질의 전체를 구문으로 본다. 그래서 3글자 이상 낱말은 1개일 때만 표현된다.
    fn to_query(words: &[String]) -> Option<String> {
        if words.is_empty() {
            return None;
        }
        if words.len() == 1 && words[0].chars().count() >= 3 {
            return Some(format!("\"{}\"", bigrams(&words[0]).join(" ")));
        }
        if words.iter().any(|w| w.chars().count() >= 3) {
            panic!("SeekStorm 질의로 표현 불가(여러 낱말 중 3글자 이상): {words:?}");
        }
        Some(words.iter().map(|w| format!("+{w}")).collect::<Vec<_>>().join(" "))
    }

    pub fn query(dir: &Path, q: &str, reps: usize, split: Option<Vec<String>>) {
        let rt = rt();
        let t = Instant::now();
        let index = rt.block_on(open_index(dir)).expect("open");
        let meta = Meta::load(&dir.join("kb1-meta.bin"));
        let open_ms = ms(t);
        let ntotal = meta.doc_of.len();
        let words = parse_query(q);
        let search = |qs: &str, len: usize, rtype: ResultType| {
            rt.block_on(index.search(qs.to_string(), None, QueryType::Intersection, SearchMode::Lexical, false, 0, len, rtype, false,
                Vec::new(), Vec::new(), Vec::new(), Vec::new(), QueryRewriting::SearchOnly))
        };
        let full = |words: &[String]| -> Out {
            let Some(qs) = to_query(words) else { return Out { sentences: 0, docs: Some(0), top: vec![] } };
            let r = search(&qs, ntotal, ResultType::TopkCount);
            let mut sids: Vec<u32> = r.results.iter().map(|x| x.doc_id as u32).collect();
            sids.sort_unstable();
            let (docs, top) = rank(&meta, sids.iter().copied(), 20);
            assert_eq!(sids.len(), r.result_count_total, "모든 결과를 받지 못함");
            Out { sentences: r.result_count_total as u64, docs: Some(docs), top }
        };
        let count = |words: &[String]| -> Out {
            let Some(qs) = to_query(words) else { return Out { sentences: 0, docs: None, top: vec![] } };
            let r = search(&qs, 0, ResultType::Count);
            Out { sentences: r.result_count_total as u64, docs: None, top: vec![] }
        };
        let top20 = |words: &[String]| -> Out {
            let Some(qs) = to_query(words) else { return Out { sentences: 0, docs: None, top: vec![] } };
            let r = search(&qs, 20, ResultType::TopkCount);
            Out { sentences: r.result_count_total as u64, docs: None, top: vec![] }
        };
        let w1 = words.clone();
        let w2 = words.clone();
        let w3 = words.clone();
        let mut variants: Vec<Variant> = vec![
            ("full_rank_top20", Box::new(move || full(&w1))),
            ("count", Box::new(move || count(&w2))),
            ("bm25_top20_sentences+count", Box::new(move || top20(&w3))),
        ];
        if let Some(sw) = split {
            variants.push(("split_full_rank_top20", Box::new(move || full(&sw))));
        }
        let shards = rt.block_on(async { index.read().await.shard_count().await });
        let qs = to_query(&words);
        run_variants(variants, reps, open_ms, move || serde_json::json!({"shards": shards, "query": qs, "words": words}));
        drop(index);
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match (a.get(1).map(String::as_str), a.get(2).map(String::as_str)) {
        (Some("build"), Some(eng)) if a.len() == 5 => {
            let (corpus, dir) = (Path::new(&a[3]), Path::new(&a[4]));
            let _ = std::fs::remove_dir_all(dir);
            let t = Instant::now();
            let extra = match eng {
                "kfind" => kf::build(corpus, dir),
                "tantivy" => tv::build(corpus, dir),
                "seekstorm" => ss::build(corpus, dir),
                _ => panic!("engine"),
            };
            let build_s = t.elapsed().as_secs_f64();
            let (ws, private) = memory_mb();
            println!(
                "{}",
                serde_json::json!({"engine": eng, "build_s": build_s, "bytes": dir_size(dir), "peak_working_set_mb": ws, "peak_private_mb": private, "extra": extra})
            );
        }
        (Some("query"), Some(eng)) if a.len() == 6 || a.len() == 7 => {
            let (dir, q, reps) = (Path::new(&a[3]), a[4].as_str(), a[5].parse().expect("reps"));
            // 선택: 7번째 인자 = 붙여 쓴 검색어를 나눈 낱말(공백 구분). kfind-core의 낱말 나누기 결과를 다른 엔진에 똑같이 준다.
            let split = a.get(6).map(|s| parse_query(s));
            match eng {
                "kfind" => kf::query(dir, q, reps),
                "tantivy" => tv::query(dir, q, reps, split),
                "seekstorm" => ss::query(dir, q, reps, split),
                _ => panic!("engine"),
            }
        }
        _ => {
            eprintln!("usage: kb1 build <kfind|tantivy|seekstorm> <corpus.tsv> <dir> | query <engine> <dir> <query> <reps> [split words]");
            std::process::exit(2);
        }
    }
}
