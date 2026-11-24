//! Tantivy paragraph index for the offline library.
//!
//! Morphology, synonyms, chosung and typo expansion are reserved in the schema;
//! this first stage implements the design's bigram fallback and query filters.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser};
use tantivy::schema::{Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value};
use tantivy::snippet::SnippetGenerator;
use tantivy::tokenizer::{Token, TokenStream, Tokenizer};
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy, Score, TantivyDocument};

#[path = "search_content_policy.rs"]
mod content_policy;
pub use content_policy::ContentScope;

// JSONL의 profile 요청에서만 수집한다. 일반 검색 응답·표준 출력에 진단을 섞지 않는다.
thread_local! {
    static PROFILE: std::cell::RefCell<Option<std::collections::BTreeMap<&'static str, f64>>> = const { std::cell::RefCell::new(None) };
}
pub(crate) fn profile_start(enabled: bool) {
    PROFILE.with(|p| *p.borrow_mut() = enabled.then(Default::default));
}
pub(crate) fn profile_values() -> std::collections::BTreeMap<&'static str, f64> {
    PROFILE.with(|p| p.borrow().clone().unwrap_or_default())
}
fn profile_add(name: &'static str, value: f64) {
    PROFILE.with(|p| { if let Some(p) = p.borrow_mut().as_mut() { *p.entry(name).or_default() += value; } });
}
fn profile_elapsed(name: &'static str, start: std::time::Instant) {
    profile_add(name, start.elapsed().as_secs_f64() * 1000.0);
}

const TOKENIZER: &str = "ko_bigram";
/// writer 힙 예산. writer는 처음 쓸 때 한 번만 만들고 계속 재사용한다.
const WRITER_HEAP: usize = 50_000_000;
/// 색인 스키마 버전. 스키마나 색인에 넣는 내용이 바뀌면 올린다. 색인 폴더의 표시와 다르면 열 때 옛 색인을 버리고 다시 만든다.
/// 1(표시 없음): 본문 필드를 모두 저장, `body_ko`·`body_bi` 중복. 2: 저장은 `body`만, `body_ko` 제거.
/// 3: hwpx·docx를 문단 단위로 나눠 위계 수준(`level`)과 함께 색인.
/// 4: 순위 계산용 FAST 필드. 5: 구조 위치 및 콘텐츠 지문·추출기 버전 저장.
pub const SCHEMA_VERSION: u32 = 5;
/// 색인이 온전히 만들어진 스키마 버전(다시 만들 때는 다 끝난 뒤에만 쓴다).
const VERSION_FILE: &str = "klib_schema_version";
/// 다시 만드는 중이라는 표시. 남아 있으면 중간에 죽은 것이므로 다음에 열 때 처음부터 다시 한다.
const PENDING_FILE: &str = "klib_rebuild_pending";
/// 다시 만드는 프로세스가 쥐는 잠금 파일(공유 없이 열어 둔다. 프로세스가 죽으면 저절로 풀린다).
const LOCK_FILE: &str = "klib_rebuild.lock";

#[derive(Clone)]
pub struct Paragraph {
    pub path: String,
    pub box_ids: Vec<String>,
    pub name: String,
    pub title: String,
    pub level: u8,
    pub body: String,
    pub ext: String,
    pub mtime: i64,
    pub page: Option<u64>,
    pub sheet: Option<String>,
    pub row: Option<u64>,
    pub content_hash: String,
    pub extractor_version: String,
}

#[derive(Clone, Debug, Default)]
pub struct SearchBlock {
    pub level: u8,
    pub text: String,
    pub ordinal: u64,
    pub node_id: String,
    pub parent_id: Option<String>,
    pub breadcrumb: Vec<String>,
    pub table: bool,
    pub row: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct SearchOptions {
    pub limit: usize,
}

#[derive(Clone, Debug, Default)]
pub struct SearchHit {
    pub path: String,
    pub name: String,
    pub title: String,
    pub score: Score,
    pub paragraph_count: usize,
    pub snippets: Vec<String>,
    pub page: Option<u64>,
    pub box_ids: Vec<String>,
    pub hit_pages: Vec<Option<u64>>,
    pub highlighted: Vec<String>,
    pub paragraph_index: u64,
    pub breadcrumb: Vec<String>,
    pub breadcrumb_text: String,
    pub node_id: String,
    pub parent_id: Option<String>,
    pub table: bool,
    pub row: Option<u64>,
    pub locations: Vec<SearchLocation>,
    pub level: u8,
}

#[derive(Clone, Debug)]
pub struct SearchLocation {
    pub paragraph_index: u64,
    pub page: Option<u64>,
    pub row: Option<u64>,
    pub table: bool,
    pub snippet: String,
    pub breadcrumb: Vec<String>,
    pub level: u8,
}

#[derive(Clone)]
struct Fields {
    doc_id: Field, path: Field, box_ids: Field, name: Field, name_cho: Field, title: Field,
    level: Field, body_bi: Field, body_jamo: Field,
    body_cho: Field, ext: Field, mtime: Field, page: Field, sheet: Field,
    row: Field, body: Field, notebook_id: Field, source_id: Field,
    paragraph_ordinal: Field, location: Field, node_id: Field, parent_id: Field,
    breadcrumb: Field, content_hash: Field, extractor_version: Field,
    is_table: Field,
}

pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    fields: Fields,
    path: PathBuf,
    synonyms: Mutex<SynonymCache>,
    content_cache: Mutex<content_policy::ContentCache>,
    /// 하나뿐인 writer와 묶음 상태. 잠금 순서: (DB 잠금 →) writer. 이 잠금을 쥔 채 DB·synonyms 잠금을 잡지 않는다.
    writer: Mutex<WriterState>,
    /// 옛 스키마 색인을 버려서 DB의 원본 목록으로 다시 채워야 하는 상태(`reindex::rebuild`가 끝내면 꺼진다).
    rebuild: AtomicBool,
    rebuild_lock: Mutex<Option<std::fs::File>>,
    /// writer 스레드 수(0이면 tantivy 기본). 업무자료함 색인은 1: 넣은 순서대로 한 세그먼트에 쌓여 같은 점수의 순서가 실행마다 같다.
    writer_threads: usize,
    /// 시험 전용: 옛(버전 1) 스키마로 연 색인의 `body_ko` 필드.
    #[cfg(test)]
    legacy_body_ko: Option<Field>,
}

#[derive(Default)]
struct WriterState {
    writer: Option<IndexWriter<TantivyDocument>>,
    /// 묶음을 연 스레드와 중첩 깊이. 다른 스레드의 쓰기는 묶음과 무관하게 바로 커밋한다.
    batch: Option<(std::thread::ThreadId, usize)>,
    /// 커밋하지 않은 변경이 있는지.
    dirty: bool,
}

/// 여러 건을 연달아 넣을 때 커밋을 묶음 끝으로 미루는 범위. 버려질 때(오류·패닉 포함) 커밋한다.
pub struct BatchGuard<'a> { index: &'a SearchIndex, active: bool, _not_send: std::marker::PhantomData<*const ()> }
impl BatchGuard<'_> {
    /// 묶음을 끝내고 커밋 결과를 돌려준다.
    pub fn finish(mut self) -> tantivy::Result<()> {
        if std::mem::replace(&mut self.active, false) { self.index.end_batch() } else { Ok(()) }
    }
}
impl Drop for BatchGuard<'_> {
    fn drop(&mut self) { if self.active { let _ = self.index.end_batch(); } }
}
impl Drop for SearchIndex {
    fn drop(&mut self) { let _ = self.flush(); }
}

#[derive(Default)]
struct SynonymCache { modified: Option<(SystemTime, u64)>, loaded: bool, file: Option<PathBuf>, groups: std::sync::Arc<Vec<Vec<String>>> }

#[derive(Clone)]
struct KoBigram;
struct BigramStream<'a> {
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    previous: Option<(usize,char)>,
    paired: bool,
    token: Token,
    position: usize,
}
impl Tokenizer for KoBigram {
    type TokenStream<'a> = BigramStream<'a>;
    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        BigramStream { chars:text.char_indices().peekable(),previous:None,paired:false,token:Token::default(),position:0 }
    }
}
impl TokenStream for BigramStream<'_> {
    fn advance(&mut self) -> bool {
        loop {
            let (start,a)=match self.previous.take().or_else(||self.chars.next()) { Some(v)=>v,None=>return false };
            if !a.is_alphanumeric() { self.paired=false; continue; }
            self.token.text.clear();
            self.token.text.push(a);
            self.token.offset_from=start;
            self.token.offset_to=start+a.len_utf8();
            if let Some(&(end,b))=self.chars.peek() {
                if b.is_alphanumeric() {
                    self.chars.next();
                    self.previous=Some((end,b));
                    self.paired=true;
                    self.token.text.push(b);
                    self.token.offset_to=end+b.len_utf8();
                } else {
                    if self.paired { self.paired=false; continue; }
                }
            } else if self.paired { self.paired=false; return false; }
            self.token.position=self.position;
            self.token.position_length=1;
            self.position+=1;
            return true;
        }
    }
    fn token(&self)->&Token { &self.token }
    fn token_mut(&mut self)->&mut Token { &mut self.token }
}

/// 검색용 글 필드. 결과에 보여 주려고 문서에서 되읽는 필드(`name`·`title`)만 저장한다.
fn text_options(tokenizer: &str, stored: bool) -> TextOptions {
    let indexing = TextFieldIndexing::default().set_index_option(IndexRecordOption::WithFreqsAndPositions)
        .set_tokenizer(tokenizer);
    let options = TextOptions::default().set_indexing_options(indexing);
    if stored { options.set_stored() } else { options }
}
fn schema() -> Schema {
    let mut b = Schema::builder();
    b.add_text_field("doc_id", tantivy::schema::STRING);
    // 순위 계산(문서 묶기·위계 수준·최신·쪽)에 쓰는 값은 FAST 필드로 둔다. 후보 문단(최대 4,000건)의 저장 문서를 풀지 않고 읽는다.
    b.add_text_field("path", tantivy::schema::STRING | tantivy::schema::FAST);
    b.add_text_field("box_ids", tantivy::schema::STRING | tantivy::schema::STORED);
    b.add_text_field("name", text_options(TOKENIZER, true));
    b.add_text_field("name_cho", text_options("default", false));
    b.add_text_field("title", text_options(TOKENIZER, true));
    b.add_u64_field("level", tantivy::schema::FAST);
    // 본문 검색 필드는 색인만 한다. 본문 글은 `body` 한 곳에만 저장한다.
    // (`body_ko`는 형태소 분석을 넣기 전까지 `body_bi`와 같은 색인이라 없앴다. 질의의 가중치는 `body_bi`에 합쳤다.)
    b.add_text_field("body_bi", text_options(TOKENIZER, false));
    b.add_text_field("body_jamo", text_options("default", false));
    b.add_text_field("body_cho", text_options("default", false));
    b.add_text_field("ext", tantivy::schema::STRING | tantivy::schema::FAST);
    b.add_i64_field("mtime", tantivy::schema::INDEXED | tantivy::schema::FAST);
    b.add_u64_field("page", tantivy::schema::FAST);
    b.add_text_field("sheet", tantivy::schema::STRING | tantivy::schema::STORED);
    b.add_u64_field("row", tantivy::schema::STORED);
    b.add_text_field("body", tantivy::schema::STORED);
    b.add_u64_field("notebook_id",tantivy::schema::INDEXED|tantivy::schema::STORED);
    b.add_u64_field("source_id",tantivy::schema::INDEXED|tantivy::schema::STORED);
    b.add_u64_field("paragraph_ordinal",tantivy::schema::INDEXED|tantivy::schema::STORED);
    b.add_text_field("location",tantivy::schema::STRING|tantivy::schema::STORED);
    b.add_text_field("node_id",tantivy::schema::STRING|tantivy::schema::STORED);
    b.add_text_field("parent_id",tantivy::schema::STRING|tantivy::schema::STORED);
    b.add_text_field("breadcrumb",tantivy::schema::STORED);
    b.add_text_field("content_hash",tantivy::schema::STRING|tantivy::schema::STORED);
    b.add_text_field("extractor_version",tantivy::schema::STRING|tantivy::schema::STORED);
    b.add_u64_field("is_table",tantivy::schema::STORED);
    b.build()
}
/// 필드를 이름으로 찾는다(스키마에 없는 이름이면 오류).
fn fields_of(schema: &Schema) -> tantivy::Result<Fields> {
    let f = |name: &str| schema.get_field(name);
    Ok(Fields {doc_id:f("doc_id")?,path:f("path")?,box_ids:f("box_ids")?,name:f("name")?,name_cho:f("name_cho")?,title:f("title")?,level:f("level")?,body_bi:f("body_bi")?,body_jamo:f("body_jamo")?,body_cho:f("body_cho")?,ext:f("ext")?,mtime:f("mtime")?,page:f("page")?,sheet:f("sheet")?,row:f("row")?,body:f("body")?,notebook_id:f("notebook_id")?,source_id:f("source_id")?,paragraph_ordinal:f("paragraph_ordinal")?,location:f("location")?,node_id:f("node_id")?,parent_id:f("parent_id")?,breadcrumb:f("breadcrumb")?,content_hash:f("content_hash")?,extractor_version:f("extractor_version")?,is_table:f("is_table")?})
}
/// 한 세그먼트의 FAST 열 묶음. 값이 하나도 없는 열은 세그먼트에 없을 수 있어 Option이다.
struct Columns {
    path: Option<tantivy::columnar::StrColumn>,
    ext: Option<tantivy::columnar::StrColumn>,
    /// 이 세그먼트 `ext` 사전에서 "nb"(노트북 문단)의 번호.
    notebook_ord: Option<u64>,
    level: Option<tantivy::columnar::Column<u64>>,
    mtime: Option<tantivy::columnar::Column<i64>>,
    page: Option<tantivy::columnar::Column<u64>>,
}
impl Columns {
    fn open(searcher: &tantivy::Searcher) -> tantivy::Result<Vec<Columns>> {
        searcher.segment_readers().iter().map(|reader| {
            let fast=reader.fast_fields();
            let ext=fast.str("ext")?;
            let notebook_ord=match &ext { Some(column)=>column.dictionary().term_ord("nb")?, None=>None };
            Ok(Columns {path:fast.str("path")?, ext, notebook_ord, level:fast.column_opt("level")?, mtime:fast.column_opt("mtime")?, page:fast.column_opt("page")?})
        }).collect()
    }
    fn path_ord(&self, doc: tantivy::DocId) -> Option<u64> { self.path.as_ref().and_then(|c|c.term_ords(doc).next()) }
    fn path(&self, doc: tantivy::DocId) -> tantivy::Result<String> {
        let mut out=String::new();
        if let (Some(column),Some(ord))=(&self.path,self.path_ord(doc)) { column.ord_to_str(ord,&mut out)?; }
        Ok(out)
    }
    fn is_notebook(&self, doc: tantivy::DocId) -> bool {
        match (&self.ext,self.notebook_ord) { (Some(column),Some(nb))=>column.term_ords(doc).next()==Some(nb), _=>false }
    }
    fn level(&self, doc: tantivy::DocId) -> u8 { self.level.as_ref().and_then(|c|c.first(doc)).unwrap_or(0).min(255) as u8 }
    fn recency(&self, doc: tantivy::DocId) -> i64 { self.mtime.as_ref().and_then(|c|c.first(doc)).unwrap_or(0) }
    fn page(&self, doc: tantivy::DocId) -> Option<u64> { self.page.as_ref().and_then(|c|c.first(doc)) }
}
fn io_error(e: std::io::Error) -> tantivy::TantivyError { tantivy::TantivyError::InternalError(e.to_string()) }
/// 다시 만들기 잠금을 잡는다. 다른 프로세스가 쥐고 있으면 None.
fn rebuild_lock(dir: &Path) -> Option<std::fs::File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new().write(true).create(true).truncate(false).share_mode(0).open(dir.join(LOCK_FILE)).ok()
    }
    #[cfg(not(windows))]
    {
        std::fs::OpenOptions::new().write(true).create(true).truncate(false).open(dir.join(LOCK_FILE)).ok()
    }
}
/// 색인 폴더 바로 아래의 색인 파일을 지운다. 사용자 동의어 파일과 하위 폴더(data)는 남긴다.
fn wipe_index_files(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if !entry.file_type()?.is_file() || [PENDING_FILE, LOCK_FILE, "synonyms.tsv"].iter().any(|keep| name.to_str() == Some(keep)) { continue; }
        std::fs::remove_file(entry.path())?;
    }
    Ok(())
}
/// 시험 전용: 버전 1(옛) 스키마. 자동 재색인·결과 동일성 시험에서 옛 색인을 만드는 데 쓴다.
#[cfg(test)]
fn legacy_schema() -> Schema {
    let mut b = Schema::builder();
    b.add_text_field("doc_id", tantivy::schema::STRING | tantivy::schema::STORED);
    b.add_text_field("path", tantivy::schema::STRING | tantivy::schema::STORED);
    b.add_text_field("box_ids", tantivy::schema::STRING | tantivy::schema::STORED);
    b.add_text_field("name", text_options(TOKENIZER, true));
    b.add_text_field("name_cho", text_options("default", true));
    b.add_text_field("title", text_options(TOKENIZER, true));
    b.add_u64_field("level", tantivy::schema::INDEXED | tantivy::schema::STORED);
    b.add_text_field("body_ko", text_options(TOKENIZER, true));
    b.add_text_field("body_bi", text_options(TOKENIZER, true));
    b.add_text_field("body_jamo", text_options("default", true));
    b.add_text_field("body_cho", text_options("default", true));
    b.add_text_field("ext", tantivy::schema::STRING | tantivy::schema::STORED);
    b.add_i64_field("mtime", tantivy::schema::INDEXED | tantivy::schema::STORED);
    b.add_u64_field("page", tantivy::schema::STORED);
    b.add_text_field("sheet", tantivy::schema::STRING | tantivy::schema::STORED);
    b.add_u64_field("row", tantivy::schema::STORED);
    b.add_text_field("body", tantivy::schema::STORED);
    b.add_u64_field("notebook_id",tantivy::schema::INDEXED|tantivy::schema::STORED);
    b.add_u64_field("source_id",tantivy::schema::INDEXED|tantivy::schema::STORED);
    b.add_u64_field("paragraph_ordinal",tantivy::schema::INDEXED|tantivy::schema::STORED);
    b.add_text_field("location",tantivy::schema::STRING|tantivy::schema::STORED);
    b.add_text_field("node_id",tantivy::schema::STRING|tantivy::schema::STORED);
    b.add_text_field("parent_id",tantivy::schema::STRING|tantivy::schema::STORED);
    b.add_text_field("breadcrumb",tantivy::schema::STORED);
    b.add_text_field("content_hash",tantivy::schema::STRING|tantivy::schema::STORED);
    b.add_text_field("extractor_version",tantivy::schema::STRING|tantivy::schema::STORED);
    b.add_u64_field("is_table",tantivy::schema::STORED);
    b.build()
}
fn id(path: &str) -> String { format!("{:x}", md5::compute(path.to_lowercase().as_bytes())) }

impl SearchIndex {
    fn state(&self) -> std::sync::MutexGuard<'_, WriterState> { self.writer.lock().unwrap_or_else(|e| e.into_inner()) }
    fn commit_locked(&self, st: &mut WriterState) -> tantivy::Result<()> {
        let result = (|| { if let Some(w) = st.writer.as_mut() { w.commit()?; } self.reader.reload() })();
        // 실패한 writer는 버린다(커밋 안 된 변경은 예전처럼 사라지고 다음 호출이 새 writer를 만든다).
        if result.is_err() { st.writer = None; }
        st.dirty = false;
        result
    }
    /// 재사용 writer로 변경을 넣는다. 묶음 밖이면 돌아오기 전에 커밋·reload해 바로 검색에 보인다.
    fn write<R>(&self, f: impl FnOnce(&mut IndexWriter<TantivyDocument>) -> tantivy::Result<R>) -> tantivy::Result<R> {
        let mut st = self.state();
        if st.writer.is_none() { st.writer = Some(if self.writer_threads == 0 { self.index.writer::<TantivyDocument>(WRITER_HEAP)? } else { self.index.writer_with_num_threads::<TantivyDocument>(self.writer_threads, WRITER_HEAP)? }); }
        let value = match f(st.writer.as_mut().expect("writer")) {
            Ok(value) => value,
            Err(e) => { st.writer = None; st.dirty = false; return Err(e); }
        };
        let batched = matches!(st.batch, Some((owner, _)) if owner == std::thread::current().id());
        if batched { st.dirty = true; } else { self.commit_locked(&mut st)?; }
        Ok(value)
    }
    /// 묶음을 연다. 같은 스레드에서 중첩할 수 있고, 가장 바깥 묶음이 끝날 때 한 번만 커밋한다.
    pub fn batch(&self) -> BatchGuard<'_> {
        let mut st = self.state();
        let me = std::thread::current().id();
        let active = match st.batch.as_mut() {
            Some((owner, depth)) if *owner == me => { *depth += 1; true }
            Some(_) => false,
            None => { st.batch = Some((me, 1)); true }
        };
        BatchGuard { index: self, active, _not_send: std::marker::PhantomData }
    }
    fn end_batch(&self) -> tantivy::Result<()> {
        let mut st = self.state();
        if let Some((_, depth)) = st.batch.as_mut() { if *depth > 1 { *depth -= 1; return Ok(()); } }
        st.batch = None;
        if st.dirty { self.commit_locked(&mut st) } else { Ok(()) }
    }
    /// 커밋하지 않은 변경이 있으면 커밋한다(종료 직전 등).
    pub fn flush(&self) -> tantivy::Result<()> {
        let mut st = self.state();
        if st.dirty { self.commit_locked(&mut st) } else { Ok(()) }
    }
    pub fn replace_notebook_source(&self, notebook_id:i64, source:&crate::notebook::Source, paragraphs:&[crate::notebook::Paragraph])->tantivy::Result<()> {
        self.write(|writer| {
        writer.delete_term(tantivy::Term::from_field_u64(self.fields.source_id,source.id as u64));
        for p in paragraphs {
            let path=format!("notebook/{notebook_id}/source/{}",source.id);
            let doc=doc!(self.fields.doc_id=>format!("{notebook_id}:{}:{}",source.id,p.ordinal),
                self.fields.path=>path,self.fields.box_ids=>format!("notebook:{notebook_id}"),
                self.fields.name=>source.display_name.clone(),self.fields.title=>source.display_name.clone(),
                self.fields.level=>p.level.max(0) as u64,
                self.fields.body_bi=>p.body.clone(),self.fields.body_jamo=>jamo_text(&p.body),
                self.fields.body_cho=>cho_text(&p.body),self.fields.ext=>"nb".to_string(),
                self.fields.mtime=>source.original_mtime,self.fields.body=>p.body.clone(),
                self.fields.notebook_id=>notebook_id as u64,self.fields.source_id=>source.id as u64,
                self.fields.paragraph_ordinal=>p.ordinal as u64,self.fields.location=>p.location.clone());
            writer.add_document(doc)?;
        }
        Ok(()) })
    }
    pub fn remove_notebook_source(&self,source_id:i64)->tantivy::Result<()> {
        self.write(|writer| { writer.delete_term(tantivy::Term::from_field_u64(self.fields.source_id,source_id as u64)); Ok(()) })
    }
    /// 업무자료함 문단(ext=nb)을 모두 지운다. 업무자료함을 별도 색인으로 옮긴 뒤 공용 색인을 정리할 때 쓴다.
    pub fn remove_all_notebook_docs(&self)->tantivy::Result<()> {
        self.write(|writer| { writer.delete_term(tantivy::Term::from_field_text(self.fields.ext,"nb")); Ok(()) })
    }
    /// 모든 문서를 지운다(별도 색인을 DB로 다시 채우기 전).
    pub fn clear(&self)->tantivy::Result<()> { self.write(|writer| { writer.delete_all_documents()?; Ok(()) }) }
    /// 지금 검색에 보이는 문서(문단) 수.
    pub fn doc_count(&self)->u64 { self.reader.searcher().num_docs() }
    /// 고유 파일 수(문단 수와 구분해 SQLite 상태 계약에 사용).
    pub fn file_count(&self)->u64 {
        let searcher=self.reader.searcher();
        let Ok(columns)=Columns::open(&searcher) else { return 0 };
        let mut paths=std::collections::HashSet::new();
        for (segment_ord, reader) in searcher.segment_readers().iter().enumerate() {
            for doc_id in 0..reader.max_doc() {
                if !reader.is_deleted(doc_id) && !columns[segment_ord].is_notebook(doc_id) {
                    if let Ok(path)=columns[segment_ord].path(doc_id) { paths.insert(path); }
                }
            }
        }
        paths.len() as u64
    }
    pub fn remove_notebook(&self,notebook_id:i64)->tantivy::Result<()> {
        self.write(|writer| { writer.delete_term(tantivy::Term::from_field_u64(self.fields.notebook_id,notebook_id as u64)); Ok(()) })
    }
    pub fn search_notebook(&self,notebook_id:i64,query:&str,source_ids:Option<&[i64]>,limit:usize)->tantivy::Result<Vec<crate::notebook::Hit>> {
        if query.trim().is_empty(){return Ok(Vec::new())}
        let mut parser=QueryParser::for_index(&self.index,vec![self.fields.name,self.fields.body_bi]);
        // 예전에는 같은 색인인 body_ko·body_bi를 둘 다 물어 점수가 두 배였다. 점수를 그대로 두려고 가중치로 합친다.
        parser.set_field_boost(self.fields.body_bi,2.0);
        let parsed=parser.parse_query(query).unwrap_or_else(|_|Box::new(tantivy::query::EmptyQuery));
        let mut clauses:Vec<(Occur,Box<dyn Query>)>=vec![
            (Occur::Must,Box::new(tantivy::query::TermQuery::new(tantivy::Term::from_field_u64(self.fields.notebook_id,notebook_id as u64),IndexRecordOption::Basic))),
            (Occur::Must,parsed)];
        if let Some(ids)=source_ids{
            if ids.is_empty(){return Ok(Vec::new())}
            let terms=ids.iter().map(|id|(Occur::Should,Box::new(tantivy::query::TermQuery::new(tantivy::Term::from_field_u64(self.fields.source_id,*id as u64),IndexRecordOption::Basic)) as Box<dyn Query>)).collect();
            clauses.push((Occur::Must,Box::new(BooleanQuery::new(terms))));
        }
        let q=BooleanQuery::new(clauses);let searcher=self.reader.searcher();
        let docs=searcher.search(&q,&TopDocs::with_limit(limit.max(1)).order_by_score())?;let mut hits=Vec::new();
        for(score,address) in docs{let d=searcher.doc::<TantivyDocument>(address)?;hits.push(crate::notebook::Hit{
            source_id:d.get_first(self.fields.source_id).and_then(|v|v.as_u64()).unwrap_or(0) as i64,
            ordinal:d.get_first(self.fields.paragraph_ordinal).and_then(|v|v.as_u64()).unwrap_or(0) as i64,
            display_name:d.get_first(self.fields.name).and_then(|v|v.as_str()).unwrap_or("").into(),
            location:d.get_first(self.fields.location).and_then(|v|v.as_str()).unwrap_or("").into(),
            score,body:d.get_first(self.fields.body).and_then(|v|v.as_str()).unwrap_or("").into()});}
        Ok(hits)
    }
    /// writer 스레드 수를 정해 연다(업무자료함 별도 색인은 1).
    pub fn open_or_create_with_threads(path: impl AsRef<Path>, threads: usize) -> tantivy::Result<Self> {
        let mut ix = Self::open_or_create(path)?;
        ix.writer_threads = threads;
        Ok(ix)
    }
    /// 색인을 연다. 색인 폴더의 스키마 버전 표시가 지금과 다르거나(옛 색인은 표시가 없다) 다시 만들다 만 표시가 있으면
    /// 옛 색인 파일을 버리고 빈 색인으로 시작하며 `needs_rebuild()`가 켜진다. 채우는 일은 `reindex::rebuild`가 한다.
    pub fn open_or_create(path: impl AsRef<Path>) -> tantivy::Result<Self> {
        let path=path.as_ref().to_path_buf();
        std::fs::create_dir_all(&path).map_err(io_error)?;
        let schema=schema();
        let current=std::fs::read_to_string(path.join(VERSION_FILE)).ok().and_then(|v|v.trim().parse::<u32>().ok())==Some(SCHEMA_VERSION);
        let mut lock=None;
        if path.join(PENDING_FILE).exists() || (path.join("meta.json").exists() && !current) {
            // 다른 프로세스가 다시 만드는 중이면 그 색인을 건드리지 않는다.
            lock=Some(rebuild_lock(&path).ok_or_else(|| tantivy::TantivyError::InternalError("다른 프로세스가 본문 색인을 다시 만드는 중입니다.".into()))?);
            // 표시를 먼저 남기고 지운다. 지우다 죽어도 다음에 열 때 다시 지운다.
            std::fs::write(path.join(PENDING_FILE), SCHEMA_VERSION.to_string()).map_err(io_error)?;
            let _=std::fs::remove_file(path.join(VERSION_FILE));
            wipe_index_files(&path).map_err(io_error)?;
        }
        let rebuild=lock.is_some();
        let index=if path.join("meta.json").exists() { Index::open_in_dir(&path)? } else {
            let index=Index::create_in_dir(&path, schema)?;
            // 새로 만든 빈 색인은 이미 지금 스키마다. 다시 만드는 중이면 다 채운 뒤(finish_rebuild)에 쓴다.
            if !rebuild { std::fs::write(path.join(VERSION_FILE), SCHEMA_VERSION.to_string()).map_err(io_error)?; }
            index
        };
        let fields=fields_of(&index.schema())?;
        index.tokenizers().register(TOKENIZER, KoBigram);
        let reader=index.reader_builder().reload_policy(ReloadPolicy::Manual).try_into()?;
        Ok(Self {index, reader, fields, path, synonyms: Mutex::new(SynonymCache::default()), content_cache: Mutex::new(Default::default()), writer: Mutex::new(WriterState::default()), rebuild: AtomicBool::new(rebuild), rebuild_lock: Mutex::new(lock), writer_threads: 0, #[cfg(test)] legacy_body_ko: None})
    }
    /// 시험 전용: 버전 1(옛) 스키마 색인을 버전 표시 없이 만든다(옛 프로그램이 남긴 색인 폴더와 같다).
    #[cfg(test)]
    pub(crate) fn open_legacy_for_test(path: impl AsRef<Path>) -> tantivy::Result<Self> {
        let path=path.as_ref().to_path_buf();
        std::fs::create_dir_all(&path).map_err(io_error)?;
        let index=Index::create_in_dir(&path, legacy_schema())?;
        let fields=fields_of(&index.schema())?;
        let legacy_body_ko=Some(index.schema().get_field("body_ko")?);
        index.tokenizers().register(TOKENIZER, KoBigram);
        let reader=index.reader_builder().reload_policy(ReloadPolicy::Manual).try_into()?;
        Ok(Self {index, reader, fields, path, synonyms: Mutex::new(SynonymCache::default()), content_cache: Mutex::new(Default::default()), writer: Mutex::new(WriterState::default()), rebuild: AtomicBool::new(false), rebuild_lock: Mutex::new(None), writer_threads: 0, legacy_body_ko})
    }
    /// 옛 색인을 버려서 원본으로 다시 채워야 하는지.
    pub fn needs_rebuild(&self) -> bool { self.rebuild.load(Ordering::SeqCst) }
    /// 다시 채우기가 끝났음을 기록한다: 남은 변경을 커밋한 뒤에 버전 표시를 쓰고 '다시 만드는 중' 표시와 잠금을 지운다.
    pub fn finish_rebuild(&self) -> tantivy::Result<()> {
        self.flush()?;
        if !self.needs_rebuild() { return Ok(()); }
        std::fs::write(self.path.join(VERSION_FILE), SCHEMA_VERSION.to_string()).map_err(io_error)?;
        let _=std::fs::remove_file(self.path.join(PENDING_FILE));
        *self.rebuild_lock.lock().unwrap_or_else(|e|e.into_inner())=None;
        let _=std::fs::remove_file(self.path.join(LOCK_FILE));
        self.rebuild.store(false,Ordering::SeqCst);
        Ok(())
    }
    pub fn index_path(&self) -> &Path { &self.path }
    /// 파일 원문 지문과 추출기 버전이 일치하면 재추출·재색인을 생략한다.
    pub fn is_current(&self, path: &str, hash: &str, extractor_version: &str, name: &str, box_ids: &[String]) -> bool {
        let query=tantivy::query::TermQuery::new(tantivy::Term::from_field_text(self.fields.path, path), IndexRecordOption::Basic);
        let searcher=self.reader.searcher();
        let Ok(rows)=searcher.search(&query,&TopDocs::with_limit(1).order_by_score()) else { return false };
        let Some((_, address))=rows.first() else { return false };
        let Ok(doc)=searcher.doc::<TantivyDocument>(*address) else { return false };
        let indexed_boxes:std::collections::HashSet<&str>=doc.get_all(self.fields.box_ids).filter_map(|v|v.as_str()).collect();
        let wanted_boxes:std::collections::HashSet<&str>=box_ids.iter().map(String::as_str).collect();
        doc.get_first(self.fields.content_hash).and_then(|v|v.as_str())==Some(hash)
            && doc.get_first(self.fields.extractor_version).and_then(|v|v.as_str())==Some(extractor_version)
            && doc.get_first(self.fields.name).and_then(|v|v.as_str())==Some(name)
            && indexed_boxes==wanted_boxes
    }
    /// 검색에 쓰이는 세그먼트 수(측정·점검용).
    pub fn segment_count(&self) -> usize { self.reader.searcher().segment_readers().len() }
    /// 본문을 줄 단위 문단으로 나눠 넣는다. 모든 문단의 위계 수준은 `p.level`이다.
    pub fn add_or_update(&self, p: &Paragraph) -> tantivy::Result<()> {
        let blocks: Vec<_> = p.body.split_inclusive(['\n','\r']).enumerate().map(|(i, text)| SearchBlock {
            level: p.level, text: text.trim().to_string(), ordinal: i as u64,
            node_id: format!("{}#{i}", id(&p.path)), ..SearchBlock::default()
        }).collect();
        self.put(p, blocks.iter(), false)
    }
    /// 문단마다 위계 수준이 다른 문서를 넣는다(수준, 글). `p.body`·`p.level`은 쓰지 않는다.
    /// 수준은 `hierarchy::search::aggregate`의 순위와 같다: 0은 본문, 1이 가장 위이고 클수록 아래.
    pub fn add_or_update_blocks(&self, p: &Paragraph, blocks: &[SearchBlock]) -> tantivy::Result<()> {
        self.put(p, blocks.iter(), false)
    }
    /// Store structural title and location metadata for the opt-in content search index.
    pub fn add_or_update_content_blocks(&self, p: &Paragraph, blocks: &[SearchBlock]) -> tantivy::Result<()> {
        self.put(p, blocks.iter(), true)
    }
    fn put<'a>(&self, p: &Paragraph, parts: impl Iterator<Item=&'a SearchBlock>, content_mode: bool) -> tantivy::Result<()> {
        self.write(|w| {
        w.delete_term(tantivy::Term::from_field_text(self.fields.path, &p.path));
        for block in parts {
            let text=block.text.trim(); if text.is_empty() { continue; }
            let crumb=block.breadcrumb.join("\u{1f}");
            let title=if content_mode && block.level>0 { text } else { p.title.as_str() };
            let mut d=doc!(self.fields.doc_id=>id(&p.path), self.fields.path=>p.path.clone(), self.fields.name=>p.name.clone(), self.fields.title=>title.to_string(), self.fields.level=>block.level as u64, self.fields.body_bi=>text.to_string(), self.fields.body_jamo=>jamo_text(text), self.fields.body_cho=>cho_text(text), self.fields.ext=>p.ext.clone(), self.fields.mtime=>p.mtime, self.fields.body=>text.to_string(), self.fields.paragraph_ordinal=>block.ordinal, self.fields.node_id=>block.node_id.clone(), self.fields.breadcrumb=>crumb, self.fields.content_hash=>p.content_hash.clone(), self.fields.extractor_version=>p.extractor_version.clone(), self.fields.is_table=>u64::from(block.table));
            if let Some(parent_id)=&block.parent_id { d.add_text(self.fields.parent_id,parent_id); }
            #[cfg(test)]
            if let Some(body_ko)=self.legacy_body_ko { d.add_text(body_ko,text); }
            for box_id in &p.box_ids { d.add_text(self.fields.box_ids, box_id); }
            d.add_text(self.fields.name_cho,cho_text(&p.name));
            if let Some(page)=p.page { d.add_u64(self.fields.page,page); }
            if let Some(sheet)=&p.sheet { d.add_text(self.fields.sheet,sheet); }
            if let Some(row)=block.row.or(p.row) { d.add_u64(self.fields.row,row); }
            w.add_document(d)?;
        }
        Ok(()) })
    }
    pub fn delete_path(&self, path: &str) -> tantivy::Result<()> {
        self.write(|w| { w.delete_term(tantivy::Term::from_field_text(self.fields.path,path)); Ok(()) })
    }
    pub fn search_box(&self, box_id: i64, query: &str, limit: usize) -> tantivy::Result<Vec<SearchHit>> {
        // 기존 오타 확장 기준과 집계 후보 수를 유지한다.
        let limit=limit.max(1).min(200);
        if !self.index.schema().get_field_entry(self.fields.box_ids).is_indexed() {
            let id=box_id.to_string();
            return Ok(self.search(query,SearchOptions{limit:limit.saturating_mul(20)})?.into_iter()
                .filter(|hit|hit.box_ids.iter().any(|value|value==&id)).take(limit).collect());
        }
        let mut hits=self.search_inner(query, SearchOptions { limit: limit.saturating_mul(20) }, Some(box_id))?;
        hits.truncate(limit);
        Ok(hits)
    }
    pub fn search(&self, query: &str, options: SearchOptions) -> tantivy::Result<Vec<SearchHit>> {
        self.search_inner(query, options, None)
    }
    fn search_inner(&self, query: &str, options: SearchOptions, box_id: Option<i64>) -> tantivy::Result<Vec<SearchHit>> {
        self.search_scoped(query, options, box_id, None)
    }
    /// opt-in JSONL 검색: 후보 수와 검색 의미는 유지하고 범위·페이지 선택 뒤 복원한다.
    pub fn search_content(&self, query: &str, options: SearchOptions, scope: &ContentScope<'_>) -> tantivy::Result<Vec<SearchHit>> {
        self.search_scoped(query, options, None, Some(scope))
    }
    fn search_scoped(&self, query: &str, options: SearchOptions, box_id: Option<i64>, scope: Option<&ContentScope<'_>>) -> tantivy::Result<Vec<SearchHit>> {
        let phase = std::time::Instant::now();
        let parsed=parse_directives(query);
        // 본문 가중치 1.4 = 예전 body_ko 1.0 + body_bi 0.4(두 필드는 같은 색인이었으므로 점수·순서가 그대로다).
        let fields=[(self.fields.name,3.0),(self.fields.title,2.5),(self.fields.body_bi,1.4)];
        let mut parser=QueryParser::for_index(&self.index, fields.iter().map(|(f,_)|*f).collect());
        for (f,w) in fields { parser.set_field_boost(f,w); }
        let groups=self.load_synonyms();
        let mut synonym_terms=Vec::new();
        for term in parsed.text.split_whitespace() {
            if let Some(group)=groups.iter().find(|g|g.iter().any(|s|s.eq_ignore_ascii_case(term))) {
                synonym_terms.extend(group.iter().filter(|s|!s.eq_ignore_ascii_case(term)).cloned());
            }
        }
        if scope.is_some() { parser.set_conjunction_by_default(); }
        let base=parser.parse_query(&parsed.text).unwrap_or_else(|_| Box::new(tantivy::query::EmptyQuery));
        let mut query_clauses: Vec<(Occur, Box<dyn Query>)>=vec![(Occur::Should,base)];
        if !synonym_terms.is_empty() {
            let syn=parser.parse_query(&synonym_terms.join(" ")).unwrap_or_else(|_| Box::new(tantivy::query::EmptyQuery));
            query_clauses.push((Occur::Should,Box::new(tantivy::query::BoostQuery::new(syn,0.8))));
        }
        let mut q: Box<dyn Query>=Box::new(BooleanQuery::new(query_clauses));
        if is_chosung_query(&parsed.text) {
            let cho=cho_text(&parsed.text);
            let cq=Box::new(tantivy::query::RegexQuery::from_pattern(&format!("{}.*",regex::escape(&cho)),self.fields.body_cho)?);
            let nq=Box::new(tantivy::query::RegexQuery::from_pattern(&format!("{}.*",regex::escape(&cho)),self.fields.name_cho)?);
            let cho_clauses: Vec<(Occur, Box<dyn Query>)>=vec![(Occur::Should, Box::new(tantivy::query::BoostQuery::new(cq,1.0))),(Occur::Should,Box::new(tantivy::query::BoostQuery::new(nq,3.0))),(Occur::Should,q)];
            q=Box::new(BooleanQuery::new(cho_clauses));
        }
        let searcher=self.reader.searcher();
        let candidate_cap=if scope.is_some() { 10000 } else { 4000 };
        let candidate_limit=options.limit.max(1).saturating_mul(20).min(candidate_cap);
        // 여러 낱말 AND와 따옴표 구문은 오타 확장 시 OR로 풀지 않는다.
        let fuzzy=if (scope.is_none() || parsed.text.split_whitespace().count() <= 1) && !parsed.text.contains('"') {
            fuzzy_query(&parsed.text,self.fields.body_jamo)
        } else { None };
        let probe_limit=if parsed.filters.is_empty() && box_id.is_none() { candidate_limit.max(options.limit.max(1)) } else { options.limit.max(1) };
        let initial=if fuzzy.is_some() { Some(searcher.search(&q,&TopDocs::with_limit(probe_limit).order_by_score())?) } else { None };
        let mut expanded=false;
        if initial.as_ref().is_some_and(|top|top.len()<options.limit.max(1)) {
            if let Some(fuzzy)=fuzzy {
                q=Box::new(BooleanQuery::new(vec![(Occur::Should,q),(Occur::Should,Box::new(tantivy::query::BoostQuery::new(fuzzy,0.5)))]));
                expanded=true;
            }
        }
        // 순위 계산에 쓰는 값은 FAST 열에서 읽는다. 저장 문서는 결과로 나갈 문서만 푼다.
        let columns=Columns::open(&searcher)?;
        let mut reusable=initial.is_some() && !expanded && parsed.filters.is_empty() && box_id.is_none();
        if reusable {
            reusable=!initial.as_ref().unwrap().iter().take(candidate_limit).any(|&(_,addr)|columns[addr.segment_ord as usize].is_notebook(addr.doc_id));
        }
        let mut clauses=vec![
            (Occur::Must,q),
            (Occur::MustNot,Box::new(tantivy::query::TermQuery::new(tantivy::Term::from_field_text(self.fields.ext,"nb"),IndexRecordOption::Basic)) as Box<dyn Query>),
        ];
        if let Some(box_id)=box_id {
            clauses.push((Occur::Must,Box::new(tantivy::query::ConstScoreQuery::new(Box::new(tantivy::query::TermQuery::new(tantivy::Term::from_field_text(self.fields.box_ids,&box_id.to_string()),IndexRecordOption::Basic)),0.0))));
        }
        for filter in parsed.filters {
            if let Some((field,value))=filter.strip_prefix("ext:").map(|v|(self.fields.ext,v)) {
                clauses.push((Occur::Must, Box::new(tantivy::query::TermQuery::new(tantivy::Term::from_field_text(field,value),IndexRecordOption::Basic))));
            } else if let Some((field,value))=filter.strip_prefix("path:").map(|v|(self.fields.path,v)) {
                clauses.push((Occur::Must, Box::new(tantivy::query::RegexQuery::from_pattern(&format!(".*{}.*",regex::escape(value)),field)?)));
            // 날짜 필터는 역색인 범위 질의로 고정한다. mtime이 FAST 필드라 RangeQuery에 맡기면 열 기반 경로를 타는데,
            // 그 경로는 세그먼트 전체가 범위 안일 때 점수 가산이 달라 세그먼트 배치에 따라 순위가 흔들린다.
            } else if let Some(value)=filter.strip_prefix("after:").and_then(parse_date_epoch) {
                clauses.push((Occur::Must, Box::new(tantivy::query::InvertedIndexRangeQuery::new(std::ops::Bound::Excluded(tantivy::Term::from_field_i64(self.fields.mtime,value)), std::ops::Bound::Unbounded))));
            } else if let Some(value)=filter.strip_prefix("before:").and_then(parse_date_epoch) {
                clauses.push((Occur::Must, Box::new(tantivy::query::InvertedIndexRangeQuery::new(std::ops::Bound::Unbounded, std::ops::Bound::Excluded(tantivy::Term::from_field_i64(self.fields.mtime,value))))));
            }
        }
        // 콘텐츠 모드에서는 문서 범위를 후보 상한보다 먼저 적용한다. 폴더별 검색에서
        // 전역 상위 후보를 먼저 자르면 뒤쪽 폴더 문서가 결과에서 빠질 수 있다.
        if let Some(scope) = scope {
            if !scope.roots.is_empty() {
                let roots: Result<Vec<(Occur, Box<dyn Query>)>, tantivy::TantivyError> = scope.roots.iter().map(|root| {
                    let pattern = format!(r"(?i)^{}([/\\\\].*)?$", regex::escape(root));
                    Ok((Occur::Should, Box::new(tantivy::query::RegexQuery::from_pattern(&pattern, self.fields.path)?) as Box<dyn Query>))
                }).collect();
                clauses.push((Occur::Must, Box::new(BooleanQuery::new(roots?))));
            }
            if let Some(allowed) = scope.allowed {
                let terms = allowed.iter().map(|path| tantivy::Term::from_field_text(self.fields.path, path));
                clauses.push((Occur::Must, Box::new(tantivy::query::TermSetQuery::new(terms))));
            }
        }
        q=Box::new(BooleanQuery::new(clauses));
        let top=if reusable { initial.unwrap().into_iter().take(candidate_limit).collect() } else { searcher.search(&q,&TopDocs::with_limit(candidate_limit).order_by_score())? };
        profile_elapsed("tantivy_ms", phase);
        profile_add("candidate_paragraphs", top.len() as f64);
        let phase = std::time::Instant::now();
        // 후보: (문서 대표 번호, 점수, 위계 수준, 수정 시각, 쪽, 주소). 같은 경로의 문단은 가장 먼저 나온(점수가 가장 높은) 문단을 대표로 묶는다.
        // 경로 글자열은 세그먼트의 사전 번호마다 한 번만 푼다.
        let mut by_ord: HashMap<(u32,u64),usize> = HashMap::new();
        let mut by_path: HashMap<String,usize> = HashMap::new();
        let mut candidates: Vec<(usize, Score, u8, i64, Option<u64>, tantivy::DocAddress)> = Vec::with_capacity(top.len());
        for (i,(score,addr)) in top.into_iter().enumerate() {
            let column=&columns[addr.segment_ord as usize];
            let document=match column.path_ord(addr.doc_id) {
                Some(ord)=>match by_ord.get(&(addr.segment_ord,ord)) {
                    Some(&document)=>document,
                    None=>{ let path=if scope.is_some() { self.content_path(&searcher,&columns,addr)? } else { column.path(addr.doc_id)? }; let document=*by_path.entry(path).or_insert(i); by_ord.insert((addr.segment_ord,ord),document); document }
                },
                None=>*by_path.entry(String::new()).or_insert(i),
            };
            candidates.push((document,score,column.level(addr.doc_id),column.recency(addr.doc_id),column.page(addr.doc_id),addr));
        }
        let compact: Vec<_> = candidates.iter().map(|c| hierarchy::search::Candidate {document:c.0,score:c.1,hierarchy_rank:c.2,recency:c.3}).collect();
        let ranked=hierarchy::search::aggregate(&compact, if scope.is_some() { usize::MAX } else { options.limit.max(1) });
        profile_elapsed("aggregate_ms", phase);
        let ranked=if let Some(scope)=scope { content_policy::select(&searcher, self.fields.body_bi, &parsed.text, &candidates, &by_path, ranked, scope)? } else { ranked };
        if scope.is_some_and(|s| !s.hydrate) {
            return ranked.into_iter().map(|row| {
                let c=&candidates[row.document];
                Ok(SearchHit { path: self.content_path(&searcher,&columns,c.5)?, score:row.best_score,
                    paragraph_count:row.mentions, level:row.best_rank, ..SearchHit::default() })
            }).collect();
        }
        let phase = std::time::Instant::now();
        let mut groups: HashMap<usize,SearchHit> = HashMap::new();
        let ranked_len=ranked.len();
        let mut generator=SnippetGenerator::create(&searcher,q.as_ref(),self.fields.body_bi)?;
        for row in &ranked {
            let c=&candidates[row.document];
            let d=if scope.is_some() { self.content_document(&searcher,c.5)? } else { std::sync::Arc::new(searcher.doc::<TantivyDocument>(c.5)?) };
            let mut hit=SearchHit{path:if scope.is_some() { self.content_path(&searcher,&columns,c.5)? } else { columns[c.5.segment_ord as usize].path(c.5.doc_id)? },
                name:d.get_first(self.fields.name).and_then(|v|v.as_str()).unwrap_or("").to_string(),
                title:d.get_first(self.fields.title).and_then(|v|v.as_str()).unwrap_or("").to_string(),
                score:row.best_score,paragraph_count:row.mentions,snippets:Vec::new(),page:c.4,
                box_ids:d.get_all(self.fields.box_ids).filter_map(|v|v.as_str().map(str::to_string)).collect(),hit_pages:Vec::new(),highlighted:Vec::new(),
                paragraph_index:if scope.is_some() { d.get_first(self.fields.paragraph_ordinal).and_then(|v|v.as_u64()).unwrap_or(0) } else { 0 },
                breadcrumb:if scope.is_some() { d.get_first(self.fields.breadcrumb).and_then(|v|v.as_str()).unwrap_or("").split('\u{1f}').filter(|s|!s.is_empty()).map(str::to_string).collect() } else { Vec::new() },
                breadcrumb_text:if scope.is_some() { d.get_first(self.fields.breadcrumb).and_then(|v|v.as_str()).unwrap_or("").replace('\u{1f}'," > ") } else { String::new() },
                node_id:if scope.is_some() { d.get_first(self.fields.node_id).and_then(|v|v.as_str()).unwrap_or("").to_string() } else { String::new() },
                parent_id:if scope.is_some() { d.get_first(self.fields.parent_id).and_then(|v|v.as_str()).map(str::to_string) } else { None },
                table:scope.is_some() && d.get_first(self.fields.is_table).and_then(|v|v.as_u64()).unwrap_or(0)!=0,
                row:if scope.is_some() { d.get_first(self.fields.row).and_then(|v|v.as_u64()) } else { None }, locations:Vec::new(), level:row.best_rank};
            let body=d.get_first(self.fields.body).and_then(|v|v.as_str()).unwrap_or("");
            let snippet_phase=std::time::Instant::now();
            let snippet=generator.snippet(body);
            profile_elapsed("snippet_ms",snippet_phase);
            hit.highlighted.extend(snippet.highlighted().iter().map(|range|snippet.fragment()[range.clone()].to_string()));
            hit.snippets.push(snippet.fragment().to_string());
            groups.insert(row.document,hit);
        }
        for c in &candidates {
            if let Some(hit) = groups.get_mut(&c.0) {
                hit.hit_pages.push(c.4);
                let d=if scope.is_some() { self.content_document(&searcher,c.5)? } else { std::sync::Arc::new(searcher.doc::<TantivyDocument>(c.5)?) };
                let breadcrumb_phase=std::time::Instant::now();
                let breadcrumb:Vec<String>=d.get_first(self.fields.breadcrumb).and_then(|v|v.as_str()).unwrap_or("").split('\u{1f}').filter(|s|!s.is_empty()).map(str::to_string).collect();
                profile_elapsed("breadcrumb_ms",breadcrumb_phase);
                let body=d.get_first(self.fields.body).and_then(|v|v.as_str()).unwrap_or("");
                let snippet_phase=std::time::Instant::now();
                let snippet=generator.snippet(body);
                profile_elapsed("snippet_ms",snippet_phase);
                if scope.is_some() { hit.locations.push(SearchLocation {
                    paragraph_index:d.get_first(self.fields.paragraph_ordinal).and_then(|v|v.as_u64()).unwrap_or(0),
                    page:c.4, row:d.get_first(self.fields.row).and_then(|v|v.as_u64()),
                    table:d.get_first(self.fields.is_table).and_then(|v|v.as_u64()).unwrap_or(0)!=0,
                    snippet:snippet.fragment().to_string(), breadcrumb, level:columns[c.5.segment_ord as usize].level(c.5.doc_id),
                }); }
            }
        }
        profile_elapsed("stored_breadcrumb_snippet_ms", phase);
        profile_add("hydrated_documents", ranked_len as f64);
        profile_add("locations", groups.values().map(|h| h.locations.len()).sum::<usize>() as f64);
        let mut out: Vec<_>=Vec::with_capacity(ranked_len);
        // Search-policy output order comes from hierarchy::search::aggregate.
        for row in ranked { if let Some(hit)=groups.remove(&row.document) { out.push(hit); } }
        Ok(out)
    }

    #[cfg(test)]
    fn reference_search(&self, query: &str, options: SearchOptions) -> tantivy::Result<Vec<SearchHit>> {
        let parsed=parse_directives(query);
        // 옛 색인에서는 옛 질의(body_ko 1.0 + body_bi 0.4) 그대로 묻는다.
        let fields=match self.legacy_body_ko {
            Some(body_ko)=>vec![(self.fields.name,3.0),(self.fields.title,2.5),(body_ko,1.0),(self.fields.body_bi,0.4)],
            None=>vec![(self.fields.name,3.0),(self.fields.title,2.5),(self.fields.body_bi,1.4)],
        };
        let mut parser=QueryParser::for_index(&self.index, fields.iter().map(|(f,_)|*f).collect());
        for (f,w) in fields { parser.set_field_boost(f,w); }
        let groups=self.load_synonyms();
        let mut synonym_terms=Vec::new();
        for term in parsed.text.split_whitespace() {
            if let Some(group)=groups.iter().find(|g|g.iter().any(|s|s.eq_ignore_ascii_case(term))) {
                synonym_terms.extend(group.iter().filter(|s|!s.eq_ignore_ascii_case(term)).cloned());
            }
        }
        let base=parser.parse_query(&parsed.text).unwrap_or_else(|_| Box::new(tantivy::query::EmptyQuery));
        let mut query_clauses: Vec<(Occur, Box<dyn Query>)>=vec![(Occur::Should,base)];
        if !synonym_terms.is_empty() {
            let syn=parser.parse_query(&synonym_terms.join(" ")).unwrap_or_else(|_| Box::new(tantivy::query::EmptyQuery));
            query_clauses.push((Occur::Should,Box::new(tantivy::query::BoostQuery::new(syn,0.8))));
        }
        let mut q: Box<dyn Query>=Box::new(BooleanQuery::new(query_clauses));
        if is_chosung_query(&parsed.text) {
            let cho=cho_text(&parsed.text);
            let cq=Box::new(tantivy::query::RegexQuery::from_pattern(&format!("{}.*",regex::escape(&cho)),self.fields.body_cho)?);
            let nq=Box::new(tantivy::query::RegexQuery::from_pattern(&format!("{}.*",regex::escape(&cho)),self.fields.name_cho)?);
            let cho_clauses: Vec<(Occur, Box<dyn Query>)>=vec![(Occur::Should, Box::new(tantivy::query::BoostQuery::new(cq,1.0))),(Occur::Should,Box::new(tantivy::query::BoostQuery::new(nq,3.0))),(Occur::Should,q)];
            q=Box::new(BooleanQuery::new(cho_clauses));
        }
        let searcher=self.reader.searcher();
        if searcher.search(&q,&TopDocs::with_limit(options.limit.max(1)).order_by_score())?.len()<options.limit.max(1) {
            if let Some(fuzzy)=fuzzy_query(&parsed.text,self.fields.body_jamo) {
                q=Box::new(BooleanQuery::new(vec![(Occur::Should,q),(Occur::Should,Box::new(tantivy::query::BoostQuery::new(fuzzy,0.5)))]));
            }
        }
        let mut clauses=vec![
            (Occur::Must,q),
            (Occur::MustNot,Box::new(tantivy::query::TermQuery::new(tantivy::Term::from_field_text(self.fields.ext,"nb"),IndexRecordOption::Basic)) as Box<dyn Query>),
        ];
        for filter in parsed.filters {
            if let Some((field,value))=filter.strip_prefix("ext:").map(|v|(self.fields.ext,v)) {
                clauses.push((Occur::Must, Box::new(tantivy::query::TermQuery::new(tantivy::Term::from_field_text(field,value),IndexRecordOption::Basic))));
            } else if let Some((field,value))=filter.strip_prefix("path:").map(|v|(self.fields.path,v)) {
                clauses.push((Occur::Must, Box::new(tantivy::query::RegexQuery::from_pattern(&format!(".*{}.*",regex::escape(value)),field)?)));
            } else if let Some(value)=filter.strip_prefix("after:").and_then(parse_date_epoch) {
                clauses.push((Occur::Must, Box::new(tantivy::query::InvertedIndexRangeQuery::new(std::ops::Bound::Excluded(tantivy::Term::from_field_i64(self.fields.mtime,value)), std::ops::Bound::Unbounded))));
            } else if let Some(value)=filter.strip_prefix("before:").and_then(parse_date_epoch) {
                clauses.push((Occur::Must, Box::new(tantivy::query::InvertedIndexRangeQuery::new(std::ops::Bound::Unbounded, std::ops::Bound::Excluded(tantivy::Term::from_field_i64(self.fields.mtime,value))))));
            }
        }
        q=Box::new(BooleanQuery::new(clauses));
        let top=searcher.search(&q,&TopDocs::with_limit(options.limit.max(1).saturating_mul(20).min(10000)).order_by_score())?;
        // First pass stores only compact metadata, not paragraph bodies or snippets.
        // 옛(버전 1) 색인은 저장 필드에서, 지금 색인은 FAST 열에서 문단마다 따로 읽는다(묶음·캐시 없는 기준 구현).
        let columns=if self.legacy_body_ko.is_none() { Some(Columns::open(&searcher)?) } else { None };
        let mut candidates: Vec<(String, Score, u8, i64, Option<u64>, String, String, Vec<String>, tantivy::DocAddress)> = Vec::with_capacity(top.len());
        for (score,addr) in top {
            let d=searcher.doc::<TantivyDocument>(addr)?;
            let (path,page,level,recency)=match &columns {
                Some(columns)=>{ let c=&columns[addr.segment_ord as usize]; (c.path(addr.doc_id)?,c.page(addr.doc_id),c.level(addr.doc_id),c.recency(addr.doc_id)) }
                None=>(d.get_first(self.fields.path).and_then(|v|v.as_str()).unwrap_or("").to_string(),
                    d.get_first(self.fields.page).and_then(|v|v.as_u64()),
                    d.get_first(self.fields.level).and_then(|v|v.as_u64()).unwrap_or(0).min(255) as u8,
                    d.get_first(self.fields.mtime).and_then(|v|v.as_i64()).unwrap_or(0)),
            };
            let name=d.get_first(self.fields.name).and_then(|v|v.as_str()).unwrap_or("").to_string();
            let title=d.get_first(self.fields.title).and_then(|v|v.as_str()).unwrap_or("").to_string();
            let box_ids=d.get_all(self.fields.box_ids).filter_map(|v|v.as_str().map(str::to_string)).collect();
            candidates.push((path,score,level,recency,page,name,title,box_ids,addr));
        }
        // Use the first (highest-scoring) paragraph as each path's representative.
        let mut documents = HashMap::new();
        let compact: Vec<_> = candidates.iter().enumerate().map(|(i,c)| {
            let document = *documents.entry(c.0.as_str()).or_insert(i);
            hierarchy::search::Candidate {document,score:c.1,hierarchy_rank:c.2,recency:c.3}
        }).collect();
        let ranked=hierarchy::search::aggregate(&compact, options.limit.max(1));
        let mut groups: HashMap<usize,SearchHit> = HashMap::new();
        let ranked_len=ranked.len();
        let mut generator=SnippetGenerator::create(&searcher,q.as_ref(),self.fields.body_bi)?;
        for row in &ranked {
            let c=&candidates[row.document];
            let hit=groups.entry(row.document).or_insert_with(||SearchHit{path:c.0.clone(),name:c.5.clone(),title:c.6.clone(),score:row.best_score,paragraph_count:row.mentions,snippets:Vec::new(),page:c.4,box_ids:c.7.clone(),hit_pages:Vec::new(),highlighted:Vec::new(),paragraph_index:0,breadcrumb:Vec::new(),breadcrumb_text:String::new(),node_id:String::new(),parent_id:None,table:false,row:None,locations:Vec::new(),level:row.best_rank});
            let d=searcher.doc::<TantivyDocument>(c.8)?;
            let body=d.get_first(self.fields.body).and_then(|v|v.as_str()).unwrap_or("");
            let snippet=generator.snippet(body);
            if hit.snippets.is_empty() {
                hit.highlighted.extend(snippet.highlighted().iter().map(|range|snippet.fragment()[range.clone()].to_string()));
                hit.snippets.push(snippet.fragment().to_string());
            }
        }
        for (c, candidate) in candidates.iter().zip(&compact) {
            if let Some(hit) = groups.get_mut(&candidate.document) { hit.hit_pages.push(c.4); }
        }
        let mut out: Vec<_>=Vec::with_capacity(ranked_len);
        // Search-policy output order comes from hierarchy::search::aggregate.
        for row in ranked { if let Some(hit)=groups.remove(&row.document) { out.push(hit); } }
        Ok(out)
    }

    fn load_synonyms(&self) -> std::sync::Arc<Vec<Vec<String>>> {
        let mut cache=self.synonyms.lock().unwrap_or_else(|e|e.into_inner());
        if cache.loaded { if let Some(f)=cache.file.clone() { if let Some(m)=std::fs::metadata(&f).ok().and_then(|m|m.modified().ok().map(|t|(t,m.len()))) { if Some(m)==cache.modified { return cache.groups.clone(); } } } }
        drop(cache);
        let executable_data=std::env::current_exe().ok().and_then(|p|p.parent().map(|d|d.join("data/synonyms.tsv")));
        let candidates=[executable_data.unwrap_or_default(),self.path.join("data/synonyms.tsv"), self.path.join("synonyms.tsv"), self.path.parent().unwrap_or(&self.path).join("data/synonyms.tsv")];
        let file=candidates.iter().find(|p|p.exists()).cloned().unwrap_or_else(||self.path.join("data/synonyms.tsv"));
        let modified=std::fs::metadata(&file).ok().and_then(|m|m.modified().ok().map(|t|(t,m.len())));
        let mut cache=self.synonyms.lock().unwrap_or_else(|e|e.into_inner());
        if !cache.loaded || modified!=cache.modified {
            let contents=std::fs::read_to_string(&file).unwrap_or_else(|_|include_str!("../data/synonyms.tsv").to_string());
            let mut groups=contents.lines().filter_map(|line| {let row:Vec<String>=line.trim_start_matches('\u{feff}').split('\t').map(str::trim).filter(|x|!x.is_empty()&&!x.starts_with('#')).map(str::to_string).collect();(row.len()>1).then_some(row)}).collect::<Vec<_>>();
            if groups.is_empty() { groups=vec![vec!["동주민센터".into(),"행정복지센터".into()],vec!["TF".into(),"전담팀".into()],vec!["주민센터".into(),"행정센터".into()]]; }
            cache.modified=modified; cache.loaded=true; cache.file=Some(file.clone()); cache.groups=std::sync::Arc::new(groups);
        }
        cache.groups.clone()
    }
}

fn cho_char(c: char) -> char {
    const CHO: [char;19]=['ㄱ','ㄲ','ㄴ','ㄷ','ㄸ','ㄹ','ㅁ','ㅂ','ㅃ','ㅅ','ㅆ','ㅇ','ㅈ','ㅉ','ㅊ','ㅋ','ㅌ','ㅍ','ㅎ'];
    let n=c as u32;
    if (0xAC00..=0xD7A3).contains(&n) { CHO[((n-0xAC00)/588) as usize] } else { c }
}
fn cho_text(s:&str)->String { s.chars().map(cho_char).collect() }
fn is_chosung_query(s:&str)->bool { !s.is_empty() && s.chars().all(|c| matches!(c,'ㄱ'..='ㅎ'|'ㄲ'|'ㄸ'|'ㅃ'|'ㅆ'|'ㅉ')) }
fn jamo_char(c:char, out:&mut String) {
    const C:[char;19]=['ㄱ','ㄲ','ㄴ','ㄷ','ㄸ','ㄹ','ㅁ','ㅂ','ㅃ','ㅅ','ㅆ','ㅇ','ㅈ','ㅉ','ㅊ','ㅋ','ㅌ','ㅍ','ㅎ'];
    const V:[char;21]=['ㅏ','ㅐ','ㅑ','ㅒ','ㅓ','ㅔ','ㅕ','ㅖ','ㅗ','ㅘ','ㅙ','ㅚ','ㅛ','ㅜ','ㅝ','ㅞ','ㅟ','ㅠ','ㅡ','ㅢ','ㅣ'];
    const T:[char;28]=[' ','ㄱ','ㄲ','ㄳ','ㄴ','ㄵ','ㄶ','ㄷ','ㄹ','ㄺ','ㄻ','ㄼ','ㄽ','ㄾ','ㄿ','ㅀ','ㅁ','ㅂ','ㅄ','ㅅ','ㅆ','ㅇ','ㅈ','ㅊ','ㅋ','ㅌ','ㅍ','ㅎ'];
    let n=c as u32;
    if (0xAC00..=0xD7A3).contains(&n) { let x=n-0xAC00; out.push(C[(x/588) as usize]); out.push(V[((x%588)/28) as usize]); if x%28!=0 { out.push(T[(x%28) as usize]); } } else { out.push(c); }
}
fn jamo_text(s:&str)->String { { let mut out=String::with_capacity(s.len()); for c in s.chars() { jamo_char(c,&mut out); } out.to_lowercase() } }
fn fuzzy_query(query:&str, field:Field)->Option<Box<dyn Query>> {
    let terms:Vec<String>=query.split_whitespace().map(|s|jamo_text(s).replace(" ","")).filter(|s|s.chars().count()>4).collect();
    let mut clauses=Vec::new();
    for term in terms { clauses.push((Occur::Should,Box::new(tantivy::query::FuzzyTermQuery::new(tantivy::Term::from_field_text(field,&term),1,true)) as Box<dyn Query>)); }
    if clauses.is_empty(){None}else{Some(Box::new(BooleanQuery::new(clauses)))}
}

struct Parsed { text:String, filters:Vec<String> }
fn parse_date_epoch(s: &str) -> Option<i64> {
    let mut parts=s.split('-');
    let (y,m,d)=(parts.next()?.parse::<i64>().ok()?,parts.next()?.parse::<i64>().ok()?,parts.next()?.parse::<i64>().ok()?);
    if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) { return None; }
    let y=y-(m<=2) as i64; let era=y.div_euclid(400); let yoe=y-era*400;
    let mp=m+if m>2 {-3} else {9}; let doy=(153*mp+2)/5+d-1; let doe=yoe*365+yoe/4-yoe/100+doy;
    Some((era*146097+doe-719468)*86_400_000)
}
fn parse_directives(input:&str)->Parsed {
    let mut text=String::new(); let mut filters=Vec::new();
    for word in input.split_whitespace() { if ["ext:","path:","after:","before:"].iter().any(|p|word.starts_with(p)) {filters.push(word.to_string())} else {if !text.is_empty(){text.push(' ')} text.push_str(word)} }
    Parsed{text,filters}
}
#[cfg(test)]
mod tests {
 use super::*;
#[derive(Clone)]
struct ReferenceBigram;
struct ReferenceBigramStream { tokens: Vec<Token>, cursor: usize }
impl Tokenizer for ReferenceBigram {
    type TokenStream<'a> = ReferenceBigramStream;
    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let mut tokens = Vec::new();
        let mut run = Vec::new();
        let flush = |run: &mut Vec<(usize, char)>, tokens: &mut Vec<Token>| {
            if run.len() == 1 {
                let (offset, ch) = run[0];
                tokens.push(Token { offset_from: offset, offset_to: offset + ch.len_utf8(), text: ch.to_string(), position: 0, position_length: 1 });
            } else {
                for pair in run.windows(2) {
                    let (start, a) = pair[0]; let (end, b) = pair[1];
                    tokens.push(Token { offset_from: start, offset_to: end + b.len_utf8(), text: format!("{a}{b}"), position: 0, position_length: 1 });
                }
            }
            run.clear();
        };
        for (offset, ch) in chars {
            if ch.is_alphanumeric() { run.push((offset, ch)); } else { flush(&mut run, &mut tokens); }
        }
        flush(&mut run, &mut tokens);
        for (position, token) in tokens.iter_mut().enumerate() { token.position = position; }
        ReferenceBigramStream { tokens, cursor: 0 }
    }
}
impl TokenStream for ReferenceBigramStream {
    fn advance(&mut self) -> bool { if self.cursor >= self.tokens.len() { false } else { self.cursor += 1; true } }
    fn token(&self) -> &Token { &self.tokens[self.cursor - 1] }
    fn token_mut(&mut self) -> &mut Token { &mut self.tokens[self.cursor - 1] }
}

fn reference_jamo_char(c:char)->String {
    const C:[char;19]=['ㄱ','ㄲ','ㄴ','ㄷ','ㄸ','ㄹ','ㅁ','ㅂ','ㅃ','ㅅ','ㅆ','ㅇ','ㅈ','ㅉ','ㅊ','ㅋ','ㅌ','ㅍ','ㅎ'];
    const V:[char;21]=['ㅏ','ㅐ','ㅑ','ㅒ','ㅓ','ㅔ','ㅕ','ㅖ','ㅗ','ㅘ','ㅙ','ㅚ','ㅛ','ㅜ','ㅝ','ㅞ','ㅟ','ㅠ','ㅡ','ㅢ','ㅣ'];
    const T:[char;28]=[' ','ㄱ','ㄲ','ㄳ','ㄴ','ㄵ','ㄶ','ㄷ','ㄹ','ㄺ','ㄻ','ㄼ','ㄽ','ㄾ','ㄿ','ㅀ','ㅁ','ㅂ','ㅄ','ㅅ','ㅆ','ㅇ','ㅈ','ㅊ','ㅋ','ㅌ','ㅍ','ㅎ'];
    let n=c as u32;
    if (0xAC00..=0xD7A3).contains(&n) { let x=n-0xAC00; format!("{}{}{}",C[(x/588) as usize],V[((x%588)/28) as usize],T[(x%28) as usize]).replace(" ","") } else { c.to_string() }
}
fn reference_jamo_text(s:&str)->String { s.chars().map(reference_jamo_char).collect::<String>().to_lowercase() }
 #[test] fn tokenization_matches_original() {
    let all_hangul:String=(0xAC00..=0xD7A3).map(|n|char::from_u32(n).unwrap()).collect();
    for text in ["", "가", "가나다 라 마바사", "서울 예산! 정책,자료", "ABC abc 123 9", "😀가나🚀a1💖", " ㄱㅏ 한글 English 2026\n끝.", "İΣ ΟΣ", &all_hangul] {
        let mut new_tokenizer=KoBigram; let mut old_tokenizer=ReferenceBigram;
        let mut new=new_tokenizer.token_stream(text); let mut old=old_tokenizer.token_stream(text);
        loop { let a=new.advance(); let b=old.advance(); assert_eq!(a,b,"{text:?}"); if !a { break; }
            let a=new.token(); let b=old.token();
            assert_eq!((&a.text,a.position,a.position_length,a.offset_from,a.offset_to),(&b.text,b.position,b.position_length,b.offset_from,b.offset_to));
        }
        assert_eq!(jamo_text(text),reference_jamo_text(text));
    }
 }
 fn temp()->PathBuf { use std::sync::atomic::{AtomicU64,Ordering}; static N:AtomicU64=AtomicU64::new(0); let p=std::env::temp_dir().join(format!("klib-search-{}-{}",std::process::id(),N.fetch_add(1,Ordering::Relaxed))); std::fs::create_dir_all(&p).unwrap();p }
 fn dir_size(p:&Path)->u64 { std::fs::read_dir(p).map(|d|d.filter_map(Result::ok).map(|e|{let m=e.metadata().unwrap(); if m.is_dir(){dir_size(&e.path())}else{m.len()}}).sum()).unwrap_or(0) }
 fn paragraph(path:&str,body:&str)->Paragraph {Paragraph{path:path.into(),box_ids:vec!["box-a".into()],name:"\u{d55c}\u{ad6d}\u{c5b4} \u{bb38}\u{c11c}".into(),title:"\u{ac80}\u{c0c9} \u{c81c}\u{baa9}".into(),level:2,body:body.into(),ext:"txt".into(),mtime:100,page:Some(1),sheet:None,row:None,content_hash:String::new(),extractor_version:String::new()}}
 #[test] fn keyword_queries_require_all_terms_and_phrases_are_contiguous(){
    let dir=temp(); let ix=SearchIndex::open_or_create(&dir).unwrap(); let batch=ix.batch();
    ix.add_or_update(&paragraph("both.txt","행정 절차 지원 정책")).unwrap();
    ix.add_or_update(&paragraph("one.txt","행정 업무")).unwrap();
    ix.add_or_update(&paragraph("spaced.txt","행정 절차 민원 지원")).unwrap();
    batch.finish().unwrap();
    let roots:Vec<String>=Vec::new();
    let scope=ContentScope{roots:&roots,allowed:None,start:None,end:None,offset:0,limit:20,hydrate:true};
    let and=ix.search_content("행정 정책",SearchOptions{limit:20},&scope).unwrap();
    assert_eq!(and.iter().map(|h|h.path.as_str()).collect::<Vec<_>>(),["both.txt"]);
    let phrase=ix.search_content("\"행정 절차 지원\"",SearchOptions{limit:20},&scope).unwrap();
    assert_eq!(phrase.iter().map(|h|h.path.as_str()).collect::<Vec<_>>(),["both.txt"]);
    assert_eq!(ix.file_count(),3);
    drop(ix); let _=std::fs::remove_dir_all(dir);
 }
 #[test] fn content_fingerprint_and_structure_are_persisted(){
    let dir=temp(); let ix=SearchIndex::open_or_create(&dir).unwrap();
    let mut p=paragraph("structured.md","ignored"); p.content_hash="sha256-a".into(); p.extractor_version="srch5-1".into();
    let blocks=vec![SearchBlock{level:1,text:"제목 검색어".into(),ordinal:7,node_id:"node-7".into(),parent_id:None,breadcrumb:vec!["제목 검색어".into()],table:false,row:None},
        SearchBlock{level:0,text:"본문 검색어".into(),ordinal:8,node_id:"node-8".into(),parent_id:Some("node-7".into()),breadcrumb:vec!["제목 검색어".into()],table:true,row:Some(2)}];
    ix.add_or_update_content_blocks(&p,&blocks).unwrap();
    assert!(ix.is_current("structured.md","sha256-a","srch5-1","한국어 문서", &["box-a".into()]));
    assert!(!ix.is_current("structured.md","sha256-b","srch5-1","한국어 문서", &["box-a".into()]));
    let roots:Vec<String>=Vec::new();
    let scope=ContentScope{roots:&roots,allowed:None,start:None,end:None,offset:0,limit:10,hydrate:true};
    let hit=ix.search_content("본문 검색어",SearchOptions{limit:10},&scope).unwrap().remove(0);
    assert_eq!(hit.paragraph_index,8); assert_eq!(hit.node_id,"node-8");
    assert_eq!(hit.parent_id.as_deref(),Some("node-7")); assert_eq!(hit.breadcrumb_text,"제목 검색어");
    assert!(hit.table); assert_eq!(hit.row,Some(2)); assert!(!hit.locations.is_empty());
    let _=std::fs::remove_dir_all(dir);
 }
 #[test] fn schema_has_reserved_fields(){let s=schema();for n in ["doc_id","path","box_ids","name","name_cho","title","level","body_bi","body_jamo","body_cho","ext","mtime","page","sheet","row","body"]{assert!(s.get_field(n).is_ok());}
    // 저장은 결과에 되읽는 필드만. 본문 글은 body 한 곳에만 저장하고, body_bi와 같던 body_ko는 없다.
    assert!(s.get_field("body_ko").is_err());
    for n in ["body_bi","body_jamo","body_cho","name_cho","doc_id"]{assert!(!s.get_field_entry(s.get_field(n).unwrap()).is_stored(),"{n}");}
    for n in ["body","name","title"]{assert!(s.get_field_entry(s.get_field(n).unwrap()).is_stored(),"{n}");}
    // 순위 계산용 값은 FAST 필드이고 저장 문서에는 넣지 않는다.
    for n in ["path","level","mtime","page","ext"]{let e=s.get_field_entry(s.get_field(n).unwrap());assert!(e.is_fast()&&!e.is_stored(),"{n}");}}
 #[test] fn bigrams_preserve_korean(){let mut tokenizer=KoBigram;let mut s=tokenizer.token_stream("\u{d55c}\u{ad6d}\u{c5b4} \u{bb38}\u{c11c}");let mut v=vec![];while s.advance(){v.push(s.token().text.clone())}assert_eq!(v,["\u{d55c}\u{ad6d}","\u{ad6d}\u{c5b4}","\u{bb38}\u{c11c}"]);}
 #[test] fn bigram_single_char(){let mut tokenizer=KoBigram;let mut s=tokenizer.token_stream("\u{ac00}");assert!(s.advance());assert_eq!(s.token().text,"\u{ac00}");}
 #[test] fn bigram_splits_punctuation(){let mut tokenizer=KoBigram;let mut s=tokenizer.token_stream("\u{ac00}\u{b098},\u{b2e4}\u{b77c}");let mut v=vec![];while s.advance(){v.push(s.token().text.clone())}assert_eq!(v,["\u{ac00}\u{b098}","\u{b2e4}\u{b77c}"]);}
 #[test] fn tokenizer_utf8_offsets(){let mut tokenizer=KoBigram;let mut s=tokenizer.token_stream("\u{ac00}\u{b098}\u{b2e4}");s.advance();assert_eq!(&"\u{ac00}\u{b098}\u{b2e4}"[s.token().offset_from..s.token().offset_to],"\u{ac00}\u{b098}");}
 #[test] fn directives_extract_filters(){let p=parse_directives("\u{d55c}\u{ad6d}\u{c5b4} ext:txt path:abc");assert_eq!(p.text,"\u{d55c}\u{ad6d}\u{c5b4}");assert_eq!(p.filters.len(),2);}
 #[test] fn add_and_search(){let dir=temp();let ix=SearchIndex::open_or_create(&dir).unwrap();ix.add_or_update(&paragraph("a.txt","\u{d55c}\u{ad6d}\u{c5b4} \u{ac80}\u{c0c9} \u{bb38}\u{b2e8}")).unwrap();assert_eq!(ix.search("\u{d55c}\u{ad6d}\u{c5b4}",SearchOptions{limit:10}).unwrap().len(),1);let _=std::fs::remove_dir_all(dir);}
 #[test] fn search_groups_matching_paragraphs_by_path_before_limit(){
    let dir=temp(); let ix=SearchIndex::open_or_create(&dir).unwrap();
    ix.add_or_update(&paragraph("multi.txt","\u{c11c}\u{c6b8} budget\n\u{c11c}\u{c6b8} plan\n\u{c11c}\u{c6b8} policy")).unwrap();
    ix.add_or_update(&paragraph("other.txt","\u{c11c}\u{c6b8} data")).unwrap();
    let hits=ix.search("\u{c11c}\u{c6b8}",SearchOptions{limit:2}).unwrap();
    assert_eq!(hits.len(),2);
    let multi=hits.iter().find(|h|h.path=="multi.txt").unwrap();
    assert_eq!(multi.paragraph_count,3); assert_eq!(multi.hit_pages,vec![Some(1);3]);
    assert_eq!(multi.snippets.len(),1); assert!(!multi.snippets[0].is_empty());
    assert_eq!(multi.page,Some(1)); assert_eq!(multi.box_ids,vec!["box-a"]);
    assert_eq!(hits.iter().filter(|h|h.path=="other.txt").count(),1);
    let limited=ix.search("\u{c11c}\u{c6b8}",SearchOptions{limit:1}).unwrap();
    assert_eq!(limited.len(),1);
    assert_eq!(limited[0].paragraph_count,hits.iter().find(|h|h.path==limited[0].path).unwrap().paragraph_count);
    drop(ix); let _=std::fs::remove_dir_all(dir);
 }
 #[test] fn same_file_three_paragraphs_fold_to_single_result_with_three_mentions(){
    let dir=temp(); let ix=SearchIndex::open_or_create(&dir).unwrap();
    ix.add_or_update(&paragraph("report.txt", "예산 분석 보고서\n예산 심의 일정\n예산 집행 내역")).unwrap();
    ix.add_or_update(&paragraph("summary.txt", "예산 요약")).unwrap();
    let hits = ix.search("예산", SearchOptions { limit: 10 }).unwrap();
    assert_eq!(hits.len(), 2, "총 2개 파일이어야 함");
    let report_hit = hits.iter().find(|h| h.path == "report.txt").expect("report.txt hit 존재");
    assert_eq!(report_hit.paragraph_count, 3, "같은 파일 내 3개 문단 일치 시 mentions(paragraph_count) == 3이어야 함");
    assert_eq!(report_hit.hit_pages.len(), 3, "3개 문단의 쪽 번호가 모여야 함");
    assert_eq!(hits.iter().filter(|h| h.path == "report.txt").count(), 1, "동일 파일은 단 1건으로 묶여야 함");
    drop(ix); let _ = std::fs::remove_dir_all(dir);
 }
 #[test] fn hierarchy_aggregation_keeps_top_twenty_order_and_snippets(){
    let dir=temp(); let ix=SearchIndex::open_or_create(&dir).unwrap();
    for n in 0..25 { let mut p=paragraph(&format!("doc-{n:02}.txt"),&format!("서울 예산 정책 자료 {n}")); p.level=if n<20 {1}else{0}; p.mtime=n as i64; ix.add_or_update(&p).unwrap(); }
    let hits=ix.search("서울",SearchOptions{limit:20}).unwrap();
    assert_eq!(hits.len(),20); assert!(hits.iter().all(|h|h.paragraph_count==1 && !h.snippets[0].is_empty()));
    assert!(hits.iter().all(|h|h.path.starts_with("doc-")));
    let _=std::fs::remove_dir_all(dir);
 }
 #[test] fn notebook_index_reuses_search_index_with_exact_scope_and_rank(){let dir=temp();let ix=SearchIndex::open_or_create(&dir).unwrap();let source=|id|crate::notebook::Source{id,notebook_id:7,kind:"txt".into(),origin:"".into(),display_name:format!("자료{id}"),status:"ready".into(),reason:"".into(),paragraph_count:1,original_size:0,original_mtime:0,item_id:None};let a=source(31);let b=source(32);ix.replace_notebook_source(7,&a,&[crate::notebook::Paragraph{source_id:31,ordinal:0,location:"1".into(),level:0,kind:"text".into(),body:"서울 예산".into()}]).unwrap();ix.replace_notebook_source(7,&b,&[crate::notebook::Paragraph{source_id:32,ordinal:0,location:"2".into(),level:0,kind:"text".into(),body:"서울 서울 예산 예산".into()}]).unwrap();let all=ix.search_notebook(7,"서울 예산",None,10).unwrap();assert_eq!(all.len(),2);assert!(all[0].score>=all[1].score);assert!(ix.search_notebook(7,"서울 예산",Some(&[31]),10).unwrap().iter().all(|h|h.source_id==31));assert!(ix.search_notebook(8,"서울 예산",None,10).unwrap().is_empty());ix.remove_notebook(7).unwrap();assert!(ix.search_notebook(7,"서울",None,10).unwrap().is_empty());let _=std::fs::remove_dir_all(dir);}
 /// 측정용: `cargo test --release bench_many_files -- --ignored --nocapture`
 #[test] fn search_matches_original_order_scores_and_snippets() {
    let dir=temp(); let ix=SearchIndex::open_or_create(&dir).unwrap();
    { let batch=ix.batch();
      for n in 0..35 {
        let mut p=paragraph(&format!("rank-{n:02}.txt"),if n%3==0 {"서울 예산 정책\n서울 행정복지센터"} else {"서울 운영 계획"});
        p.level=(n%5) as u8; p.mtime=n*86_400_000; p.box_ids=vec![(n%2).to_string()];
        ix.add_or_update(&p).unwrap();
      }
      batch.finish().unwrap();
    }
    for include_notebook in [false,true] {
    if include_notebook { let mut notebook=paragraph("notebook.txt","서울 예산"); notebook.ext="nb".into(); ix.add_or_update(&notebook).unwrap(); }
    for query in ["서울", "서울 예산", "서을 예산", "ㅅㅇ", "TF", "없는검색어", "서울 ext:txt", "서울 path:rank-1", "서울 after:1970-01-02", "서울 before:1970-01-20"] {
      for limit in [1,3,20,50] {
        assert_eq!(format!("{:?}",ix.search(query,SearchOptions{limit}).unwrap()),format!("{:?}",ix.reference_search(query,SearchOptions{limit}).unwrap()),"query={query} limit={limit}");
      }
    }
    }
    let expected:Vec<_>=ix.reference_search("서울",SearchOptions{limit:200}).unwrap().into_iter().filter(|h|h.box_ids.iter().any(|id|id=="1")).collect();
    assert_eq!(format!("{:?}",ix.search_box(1,"서울",200).unwrap()),format!("{expected:?}"));
    assert!(ix.search_box(99,"서울",20).unwrap().is_empty());
    drop(ix); let _=std::fs::remove_dir_all(dir);
 }
 #[test] #[ignore] fn bench_many_files(){
    let n:usize=std::env::var("KLIB_BENCH_N").ok().and_then(|v|v.parse().ok()).unwrap_or(200);
    let body=|i:usize|(0..20).map(|l|format!("서울 예산 정책 자료 {i} 문단 {l} 행정복지센터 운영 계획")).collect::<Vec<_>>().join("
");
    let dir=temp(); let ix=SearchIndex::open_or_create(&dir).unwrap();
    let t=std::time::Instant::now();
    for i in 0..n { ix.add_or_update(&paragraph(&format!("single-{i:04}.txt"),&body(i))).unwrap(); }
    let single=t.elapsed();
    assert_eq!(ix.search("ext:txt 서울",SearchOptions{limit:n}).unwrap().len(),n);
    println!("BENCH single n={n} ms={} segments={}",single.as_millis(),ix.segment_count());
    drop(ix); let _=std::fs::remove_dir_all(dir);
    let dir=temp(); let ix=SearchIndex::open_or_create(&dir).unwrap();
    let t=std::time::Instant::now();
    { let b=ix.batch(); for i in 0..n { ix.add_or_update(&paragraph(&format!("batch-{i:04}.txt"),&body(i))).unwrap(); } b.finish().unwrap(); }
    let batch=t.elapsed();
    assert_eq!(ix.search("ext:txt 서울",SearchOptions{limit:n}).unwrap().len(),n);
    println!("BENCH batch n={n} ms={} segments={}",batch.as_millis(),ix.segment_count());
    let t=std::time::Instant::now();
    for _ in 0..100 { std::hint::black_box(ix.search("서울 예산",SearchOptions{limit:20}).unwrap()); }
    println!("BENCH search repeats=100 ms={}",t.elapsed().as_millis());
    drop(ix); println!("BENCH index_bytes n={n} bytes={}",dir_size(&dir)); let _=std::fs::remove_dir_all(dir);
 }
 #[test] fn single_writes_are_visible_immediately_with_reused_writer(){
    let dir=temp(); let ix=SearchIndex::open_or_create(&dir).unwrap(); let count=|q:&str|ix.search(q,SearchOptions{limit:50}).unwrap().len();
    for n in 0..5 { ix.add_or_update(&paragraph(&format!("s-{n}.txt"),"서울 예산")).unwrap(); assert_eq!(count("서울"),n+1); }
    ix.add_or_update(&paragraph("s-0.txt","부산 계획")).unwrap(); assert_eq!(count("서울"),4); assert_eq!(count("부산"),1);
    ix.delete_path("s-1.txt").unwrap(); assert_eq!(count("서울"),3);
    drop(ix); let again=SearchIndex::open_or_create(&dir).unwrap(); assert_eq!(again.search("서울",SearchOptions{limit:50}).unwrap().len(),3);
    drop(again); let _=std::fs::remove_dir_all(dir);
 }
 #[test] fn batch_commits_once_at_the_end(){
    let dir=temp(); let ix=SearchIndex::open_or_create(&dir).unwrap(); let count=|q:&str|ix.search(q,SearchOptions{limit:50}).unwrap().len();
    ix.add_or_update(&paragraph("old.txt","서울 예산")).unwrap();
    { let outer=ix.batch();
      for n in 0..10 { ix.add_or_update(&paragraph(&format!("b-{n}.txt"),"서울 예산")).unwrap(); }
      { let _inner=ix.batch(); ix.delete_path("old.txt").unwrap(); ix.add_or_update(&paragraph("b-0.txt","부산 계획")).unwrap(); }
      assert_eq!(count("서울"),1); assert_eq!(count("부산"),0);
      outer.finish().unwrap(); }
    assert_eq!(count("서울"),9); assert_eq!(count("부산"),1);
    ix.add_or_update(&paragraph("after.txt","서울 예산")).unwrap(); assert_eq!(count("서울"),10);
    let _=std::fs::remove_dir_all(dir);
 }
 #[test] fn dropped_batch_and_index_drop_do_not_lose_writes(){
    let dir=temp(); let ix=SearchIndex::open_or_create(&dir).unwrap();
    { let _b=ix.batch(); ix.add_or_update(&paragraph("d-1.txt","서울 예산")).unwrap(); }
    assert_eq!(ix.search("서울",SearchOptions{limit:50}).unwrap().len(),1);
    let b=ix.batch(); ix.add_or_update(&paragraph("d-2.txt","서울 예산")).unwrap(); std::mem::forget(b);
    drop(ix); let again=SearchIndex::open_or_create(&dir).unwrap(); assert_eq!(again.search("서울",SearchOptions{limit:50}).unwrap().len(),2);
    drop(again); let _=std::fs::remove_dir_all(dir);
 }
 #[test] fn other_thread_write_is_not_deferred_by_batch(){
    let dir=temp(); let ix=SearchIndex::open_or_create(&dir).unwrap();
    let b=ix.batch(); ix.add_or_update(&paragraph("t-1.txt","서울 예산")).unwrap();
    std::thread::scope(|s|{ s.spawn(||{ ix.add_or_update(&paragraph("t-2.txt","서울 예산")).unwrap(); assert_eq!(ix.search("서울",SearchOptions{limit:50}).unwrap().len(),2); }); });
    b.finish().unwrap(); assert_eq!(ix.search("서울",SearchOptions{limit:50}).unwrap().len(),2);
    let _=std::fs::remove_dir_all(dir);
 }
 #[test] fn fresh_index_is_current_and_reopens_without_rebuild(){
    let dir=temp(); let ix=SearchIndex::open_or_create(&dir).unwrap();
    assert!(!ix.needs_rebuild()); assert_eq!(std::fs::read_to_string(dir.join(VERSION_FILE)).unwrap(),SCHEMA_VERSION.to_string());
    ix.add_or_update(&paragraph("f.txt","서울 예산")).unwrap(); drop(ix);
    let again=SearchIndex::open_or_create(&dir).unwrap(); assert!(!again.needs_rebuild());
    assert_eq!(again.search("서울",SearchOptions{limit:5}).unwrap().len(),1);
    drop(again); let _=std::fs::remove_dir_all(dir);
 }
 #[test] fn old_schema_index_is_discarded_and_marked_only_after_rebuild(){
    let dir=temp();
    { let old=SearchIndex::open_legacy_for_test(&dir).unwrap(); old.add_or_update(&paragraph("old.txt","서울 예산")).unwrap(); assert_eq!(old.reference_search("서울",SearchOptions{limit:5}).unwrap().len(),1); }
    std::fs::write(dir.join("synonyms.tsv"),"가\t나\n").unwrap();
    assert!(!dir.join(VERSION_FILE).exists());
    let ix=SearchIndex::open_or_create(&dir).unwrap();
    assert!(ix.needs_rebuild()); assert!(dir.join(PENDING_FILE).exists()); assert!(!dir.join(VERSION_FILE).exists());
    assert!(dir.join("synonyms.tsv").exists(),"사용자 동의어 파일은 남긴다");
    assert!(ix.index.schema().get_field("body_ko").is_err());
    assert!(ix.search("서울",SearchOptions{limit:5}).unwrap().is_empty());
    // 다시 만드는 동안 다른 곳에서 열면 건드리지 않고 오류로 알린다.
    assert!(SearchIndex::open_or_create(&dir).is_err());
    // 채우다 죽은 경우: 버전 표시가 없으므로 다음에 열 때 처음부터 다시 한다.
    ix.add_or_update(&paragraph("half.txt","서울 예산")).unwrap(); drop(ix);
    let ix=SearchIndex::open_or_create(&dir).unwrap();
    assert!(ix.needs_rebuild()); assert!(ix.search("서울",SearchOptions{limit:5}).unwrap().is_empty());
    ix.add_or_update(&paragraph("new.txt","서울 예산")).unwrap(); ix.finish_rebuild().unwrap();
    assert!(!ix.needs_rebuild()); assert!(!dir.join(PENDING_FILE).exists());
    assert_eq!(std::fs::read_to_string(dir.join(VERSION_FILE)).unwrap(),SCHEMA_VERSION.to_string()); drop(ix);
    let ix=SearchIndex::open_or_create(&dir).unwrap(); assert!(!ix.needs_rebuild());
    assert_eq!(ix.search("서울",SearchOptions{limit:5}).unwrap().iter().map(|h|h.path.as_str()).collect::<Vec<_>>(),["new.txt"]);
    drop(ix); let _=std::fs::remove_dir_all(dir);
 }
 /// 스키마 변경 전후 비교: 같은 자료를 옛 스키마 색인(옛 질의)과 새 색인에 넣고 같은 질의의 결과 문서·순서·발췌가 같은지 본다.
 /// 위계 수준은 모두 같게(본문) 둔다. 후보 자르기(limit×20)에 걸리지 않게 문단 수를 60개 아래로 둔다
 /// (점수가 같은 문단 중 무엇이 잘리는지는 세그먼트 배치에 달려 있어 색인마다 다를 수 있다).
 #[test] fn schema_change_keeps_result_set_and_order_for_flat_levels(){
    let (old_dir,new_dir)=(temp(),temp());
    let old=SearchIndex::open_legacy_for_test(&old_dir).unwrap(); let new=SearchIndex::open_or_create(&new_dir).unwrap();
    let bodies=["서울 예산 정책\n서울 행정복지센터 운영","서울 운영 계획","부산 예산 편성 지침과 서울 사례","행정복지센터 TF 구성\n동주민센터 예산","마포구 주민 참여 예산 제도 안내"];
    for ix in [&old,&new] { let batch=ix.batch();
      for n in 0..40i64 { let mut p=paragraph(&format!("flat-{n:02}.txt"),bodies[(n%5) as usize]); p.level=0; p.mtime=n*86_400_000; p.name=format!("자료 {n} {}",if n%7==0 {"예산서"} else {"문서"}); p.page=Some((n%4) as u64+1); p.box_ids=vec![(n%2).to_string()]; ix.add_or_update(&p).unwrap(); }
      batch.finish().unwrap(); }
    let mut compared=0;
    for query in ["서울","서울 예산","서을 예산","예산","ㅅㅇ","TF","행정복지센터","없는검색어","서울 ext:txt","서울 path:flat-1","예산 after:1970-01-10","예산 before:1970-01-20"] {
      for limit in [3,20,50] {
        let before=old.reference_search(query,SearchOptions{limit}).unwrap(); let after=new.search(query,SearchOptions{limit}).unwrap();
        let strip=|hits:&[SearchHit]|hits.iter().map(|h|{let mut h=h.clone(); h.score=0.0; format!("{h:?}")}).collect::<Vec<_>>();
        assert_eq!(strip(&after),strip(&before),"query={query} limit={limit}");
        for (a,b) in after.iter().zip(&before) { assert!((a.score-b.score).abs()<=b.score.abs()*1e-5,"query={query} limit={limit} {} != {}",a.score,b.score); }
        compared+=after.len();
      }
    }
    assert!(compared>200,"비교한 결과가 너무 적다: {compared}");
    for box_id in [0,1] { let expected:Vec<_>=old.reference_search("예산",SearchOptions{limit:200}).unwrap().into_iter().filter(|h|h.box_ids.iter().any(|id|*id==box_id.to_string())).map(|h|h.path).collect();
      assert_eq!(new.search_box(box_id,"예산",200).unwrap().into_iter().map(|h|h.path).collect::<Vec<_>>(),expected); }
    drop(old); drop(new); let _=std::fs::remove_dir_all(old_dir); let _=std::fs::remove_dir_all(new_dir);
 }
 /// FAST 열 읽기: 쪽이 없는 문단은 None, 쪽이 있으면 그 값. 따로 커밋해 세그먼트가 갈린 색인에서도 경로별로 하나로 묶인다.
 #[test] fn fast_columns_handle_missing_pages_and_multiple_segments(){
    let dir=temp(); let ix=SearchIndex::open_or_create(&dir).unwrap();
    let mut no_page=paragraph("no-page.txt","서울 예산\n서울 계획"); no_page.page=None; no_page.level=0; ix.add_or_update(&no_page).unwrap();
    assert_eq!(ix.search("서울",SearchOptions{limit:5}).unwrap()[0].hit_pages,vec![None,None]);
    for n in 0..6u64 { let mut p=paragraph(&format!("seg-{n}.txt"),"서울 예산\n서울 계획\n서울 정책"); p.page=Some(n+1); p.mtime=n as i64; ix.add_or_update(&p).unwrap(); }
    assert!(ix.segment_count()>1,"세그먼트가 여럿이어야 한다");
    let hits=ix.search("서울",SearchOptions{limit:20}).unwrap();
    assert_eq!(hits.len(),7);
    for h in &hits { if h.path=="no-page.txt" { assert_eq!((h.page,h.paragraph_count,&h.hit_pages),(None,2,&vec![None,None])); } else { let n:u64=h.path[4..5].parse().unwrap(); assert_eq!((h.page,h.paragraph_count,&h.hit_pages),(Some(n+1),3,&vec![Some(n+1);3]),"{}",h.path); } }
    assert_eq!(format!("{hits:?}"),format!("{:?}",ix.reference_search("서울",SearchOptions{limit:20}).unwrap()));
    drop(ix); let _=std::fs::remove_dir_all(dir);
 }
 #[test] fn chosung_and_fuzzy_helpers(){assert_eq!(cho_text("\u{d589}\u{c815}\u{bcf5}\u{c9c0}\u{c13c}\u{d130}"),"\u{314E}\u{3148}\u{3142}\u{3148}\u{3145}\u{314C}");assert!(is_chosung_query("\u{314E}\u{3148}\u{3142}\u{3148}"));assert!(fuzzy_query("\u{d589}\u{c815}\u{bcf5}\u{c9c0}\u{c13c}\u{d130}",fields_of(&schema()).unwrap().body_jamo).is_some());}
}
