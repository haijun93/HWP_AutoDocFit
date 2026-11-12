//! (A) Tantivy: 문장 하나 = Tantivy 문서 하나. 비교형(공백 제거) 글을 2글자 조각(`bi`, 위치 없음)과
//! 3글자 조각(`tri`, 위치 포함) 두 필드로 색인한다. 낱말 길이 2 → bi 용어, 3 → tri 용어,
//! 4 이상 → tri 조각 구문 질의(겹치게 덮는 위치)로 정확한 부분 문자열 일치를 낸다.
use crate::common::{fold, read_corpus, Meta};
use roaring::RoaringBitmap;
use std::path::Path;
use tantivy::collector::{Collector, SegmentCollector};
use tantivy::columnar::Column;
use tantivy::query::{BooleanQuery, Occur, PhraseQuery, Query, TermQuery};
use tantivy::schema::{Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, FAST};
use tantivy::tokenizer::{Token, TokenStream, Tokenizer};
use tantivy::{doc, DocId, Index, IndexReader, ReloadPolicy, Score, SegmentOrdinal, SegmentReader, TantivyDocument, Term};

/// 글자 n개씩 겹쳐 자르는 토크나이저(입력은 이미 비교형). 위치 = 시작 글자 번호.
#[derive(Clone)]
pub struct Ngram(pub usize);
pub struct NgramStream<'a> {
    text: &'a str,
    bounds: Vec<usize>,
    i: usize,
    n: usize,
    token: Token,
}
impl Tokenizer for Ngram {
    type TokenStream<'a> = NgramStream<'a>;
    fn token_stream<'a>(&'a mut self, text: &'a str) -> NgramStream<'a> {
        let mut bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
        bounds.push(text.len());
        NgramStream { text, bounds, i: 0, n: self.0, token: Token::default() }
    }
}
impl TokenStream for NgramStream<'_> {
    fn advance(&mut self) -> bool {
        if self.i + self.n >= self.bounds.len() { return false; }
        let (a, b) = (self.bounds[self.i], self.bounds[self.i + self.n]);
        self.token.text.clear();
        self.token.text.push_str(&self.text[a..b]);
        self.token.offset_from = a;
        self.token.offset_to = b;
        self.token.position = self.i;
        self.token.position_length = 1;
        self.i += 1;
        true
    }
    fn token(&self) -> &Token { &self.token }
    fn token_mut(&mut self) -> &mut Token { &mut self.token }
}

fn register(index: &Index) {
    index.tokenizers().register("bi2", Ngram(2));
    index.tokenizers().register("tri3", Ngram(3));
}

pub fn build(corpus: &Path, out: &Path, heap: usize, threads: usize) -> tantivy::Result<serde_json::Value> {
    std::fs::create_dir_all(out)?;
    let mut sb = Schema::builder();
    let bi = sb.add_text_field("bi", TextOptions::default().set_indexing_options(
        TextFieldIndexing::default().set_tokenizer("bi2").set_index_option(IndexRecordOption::Basic)));
    let tri = sb.add_text_field("tri", TextOptions::default().set_indexing_options(
        TextFieldIndexing::default().set_tokenizer("tri3").set_index_option(IndexRecordOption::WithFreqsAndPositions)));
    let sid_f = sb.add_u64_field("sid", FAST);
    let index = Index::create_in_dir(out, sb.build())?;
    register(&index);
    let mut writer = index.writer_with_num_threads::<TantivyDocument>(threads, heap)?;
    // 백그라운드 병합을 끄고 커밋 뒤 한 번에 한 조각으로 합친다(기본 정책이 먼저 합치면 아래 merge가 사라진 조각을 가리킴).
    writer.set_merge_policy(Box::new(tantivy::indexer::NoMergePolicy));
    let mut meta = Meta { doc_of: Vec::new(), level_of: Vec::new(), ndocs: 0 };
    let t0 = std::time::Instant::now();
    read_corpus(&corpus.join("corpus.tsv"), |sid, d, lv, text| {
        let f = fold(text);
        writer.add_document(doc!(bi => f.clone(), tri => f, sid_f => sid as u64)).expect("add");
        meta.doc_of.push(d);
        meta.level_of.push(lv);
    })?;
    writer.commit()?;
    let commit_s = t0.elapsed().as_secs_f64();
    let ids = index.searchable_segment_ids()?;
    let segments_before = ids.len();
    if ids.len() > 1 { writer.merge(&ids).wait()?; }
    writer.wait_merging_threads()?;
    std::fs::write(out.join("meta.bin"), meta.to_bytes())?;
    Ok(serde_json::json!({"commit_s": commit_s, "segments_before_merge": segments_before}))
}

pub struct EngineA {
    reader: IndexReader,
    bi: Field,
    tri: Field,
    pub meta: Meta,
}

struct SidCollector;
struct SidSegment { col: Column<u64>, out: Vec<u32> }
impl Collector for SidCollector {
    type Fruit = Vec<u32>;
    type Child = SidSegment;
    fn for_segment(&self, _: SegmentOrdinal, seg: &SegmentReader) -> tantivy::Result<SidSegment> {
        Ok(SidSegment { col: seg.fast_fields().u64("sid")?, out: Vec::new() })
    }
    fn requires_scoring(&self) -> bool { false }
    fn merge_fruits(&self, fruits: Vec<Vec<u32>>) -> tantivy::Result<Vec<u32>> {
        let mut all: Vec<u32> = if fruits.len() == 1 { fruits.into_iter().next().unwrap() } else { fruits.concat() };
        all.sort_unstable();
        Ok(all)
    }
}
impl SegmentCollector for SidSegment {
    type Fruit = Vec<u32>;
    fn collect(&mut self, doc: DocId, _: Score) {
        self.out.push(self.col.first(doc).unwrap_or(0) as u32);
    }
    fn harvest(self) -> Vec<u32> { self.out }
}

/// 낱말 길이 n(≥4)을 3글자 조각으로 덮는 시작 위치: 0,3,6,…와 마지막 n-3.
pub fn cover_offsets(n: usize) -> Vec<usize> {
    let mut v: Vec<usize> = (0..).step_by(3).take_while(|o| o + 3 <= n).collect();
    if *v.last().unwrap() != n - 3 { v.push(n - 3); }
    v
}

impl EngineA {
    pub fn open(dir: &Path) -> tantivy::Result<EngineA> {
        let index = Index::open_in_dir(dir)?;
        register(&index);
        let schema = index.schema();
        let reader = index.reader_builder().reload_policy(ReloadPolicy::Manual).try_into()?;
        let meta = Meta::from_bytes(&std::fs::read(dir.join("meta.bin"))?);
        Ok(EngineA { reader, bi: schema.get_field("bi")?, tri: schema.get_field("tri")?, meta })
    }

    fn word_query(&self, w: &str) -> Box<dyn Query> {
        let chars: Vec<usize> = w.char_indices().map(|(i, _)| i).chain([w.len()]).collect();
        let n = chars.len() - 1;
        match n {
            2 => Box::new(TermQuery::new(Term::from_field_text(self.bi, w), IndexRecordOption::Basic)),
            3 => Box::new(TermQuery::new(Term::from_field_text(self.tri, w), IndexRecordOption::Basic)),
            _ => Box::new(PhraseQuery::new_with_offset(cover_offsets(n).into_iter()
                .map(|o| (o, Term::from_field_text(self.tri, &w[chars[o]..chars[o + 3]]))).collect())),
        }
    }

    /// 모든 낱말이 같은 문장에 있는 문장 번호(오름차순).
    pub fn query(&self, words: &[String]) -> Vec<u32> {
        if words.is_empty() { return Vec::new(); }
        let q: Box<dyn Query> = if words.len() == 1 { self.word_query(&words[0]) } else {
            Box::new(BooleanQuery::new(words.iter().map(|w| (Occur::Must, self.word_query(w))).collect()))
        };
        self.reader.searcher().search(q.as_ref(), &SidCollector).expect("search")
    }

    /// 낱말 하나의 일치 문장 비트맵(재사용 모드용). Tantivy는 후보 제한을 받을 수 없어 restrict는 뒤에 교차.
    pub fn word(&self, w: &str, restrict: Option<&RoaringBitmap>) -> RoaringBitmap {
        let v = self.query(std::slice::from_ref(&w.to_string()));
        let mut bm = RoaringBitmap::from_sorted_iter(v).expect("sorted");
        if let Some(r) = restrict { bm &= r; }
        bm
    }
}
