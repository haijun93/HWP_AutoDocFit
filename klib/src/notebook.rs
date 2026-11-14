//! SQLite-backed notebooks, citations, paragraph search, and deterministic transforms.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser};
use tantivy::schema::{IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value};
use tantivy::tokenizer::{Token, TokenStream, Tokenizer};
use tantivy::{doc, Index, IndexReader, ReloadPolicy, TantivyDocument};

#[derive(Clone, Debug)]
pub struct Source { pub id:i64, pub notebook_id:i64, pub kind:String, pub origin:String, pub display_name:String, pub status:String, pub reason:String, pub paragraph_count:i64, pub original_size:i64, pub original_mtime:i64 }
#[derive(Clone, Debug)]
pub struct Paragraph { pub source_id:i64, pub ordinal:i64, pub location:String, pub level:i64, pub kind:String, pub body:String }
#[derive(Clone, Debug)]
pub struct Hit { pub source_id:i64, pub ordinal:i64, pub display_name:String, pub location:String, pub score:f32, pub body:String }
#[derive(Clone, Debug)]
pub struct TransformItem { pub source_id:i64, pub ordinal:i64, pub location:String, pub level:i64, pub kind:String, pub text:String, pub score:f64 }

pub struct NotebookIndex { index:Index, reader:IndexReader, notebook:tantivy::schema::Field, source:tantivy::schema::Field, ordinal:tantivy::schema::Field, name:tantivy::schema::Field, body:tantivy::schema::Field, location:tantivy::schema::Field }

#[derive(Clone)] struct Bigram;
struct Stream { tokens:Vec<Token>, pos:usize }
impl Tokenizer for Bigram { type TokenStream<'a> = Stream; fn token_stream<'a>(&'a mut self, text:&'a str)->Stream {
    let chars:Vec<_>=text.char_indices().collect(); let mut tokens=Vec::new(); let mut run=Vec::new();
    let flush=|run:&mut Vec<(usize,char)>,tokens:&mut Vec<Token>| { if run.len()==1 { let (s,c)=run[0]; tokens.push(Token{offset_from:s,offset_to:s+c.len_utf8(),text:c.to_string(),position:0,position_length:1}); } else { for pair in run.windows(2) { let (s,a)=pair[0]; let (e,b)=pair[1]; tokens.push(Token{offset_from:s,offset_to:e+b.len_utf8(),text:format!("{a}{b}"),position:0,position_length:1}); } } run.clear(); };
    for (s,c) in chars { if c.is_alphanumeric() {run.push((s,c))} else {flush(&mut run,&mut tokens)} } flush(&mut run,&mut tokens);
    for (i,t) in tokens.iter_mut().enumerate(){t.position=i;} Stream{tokens,pos:0}
} }
impl TokenStream for Stream { fn advance(&mut self)->bool { if self.pos>=self.tokens.len(){false}else{self.pos+=1;true} } fn token(&self)->&Token{&self.tokens[self.pos-1]} fn token_mut(&mut self)->&mut Token{&mut self.tokens[self.pos-1]} }

impl NotebookIndex {
    pub fn open(path:impl AsRef<Path>)->tantivy::Result<Self>{ let path=path.as_ref(); std::fs::create_dir_all(path).map_err(|e|tantivy::TantivyError::InternalError(e.to_string()))?;
        let mut b=Schema::builder(); let notebook=b.add_u64_field("notebook",tantivy::schema::INDEXED|tantivy::schema::STORED); let source=b.add_u64_field("source",tantivy::schema::INDEXED|tantivy::schema::STORED); let ordinal=b.add_u64_field("ordinal",tantivy::schema::INDEXED|tantivy::schema::STORED);
        let text=TextOptions::default().set_indexing_options(TextFieldIndexing::default().set_index_option(IndexRecordOption::WithFreqsAndPositions).set_tokenizer("nb_bigram")).set_stored();
        let name=b.add_text_field("name",text.clone()); let body=b.add_text_field("body",text); let location=b.add_text_field("location",tantivy::schema::STRING|tantivy::schema::STORED); let schema=b.build();
        let index=if path.join("meta.json").exists(){Index::open_in_dir(path)?}else{Index::create_in_dir(path,schema)?}; index.tokenizers().register("nb_bigram",Bigram);
        let reader=index.reader_builder().reload_policy(ReloadPolicy::Manual).try_into()?; Ok(Self{index,reader,notebook,source,ordinal,name,body,location})
    }
    pub fn replace_source(&self,notebook_id:i64,source:&Source,paragraphs:&[Paragraph])->tantivy::Result<()> { let mut w=self.index.writer::<TantivyDocument>(50_000_000)?; w.delete_term(tantivy::Term::from_field_u64(self.source,source.id as u64));
        for p in paragraphs { w.add_document(doc!(self.notebook=>notebook_id as u64,self.source=>source.id as u64,self.ordinal=>p.ordinal as u64,self.name=>source.display_name.clone(),self.body=>p.body.clone(),self.location=>p.location.clone()))?; } w.commit()?; self.reader.reload()?; Ok(()) }
    pub fn remove_source(&self,source_id:i64)->tantivy::Result<()> {let mut w=self.index.writer::<TantivyDocument>(50_000_000)?;w.delete_term(tantivy::Term::from_field_u64(self.source,source_id as u64));w.commit()?;self.reader.reload()?;Ok(())}
    pub fn remove_notebook(&self,notebook_id:i64)->tantivy::Result<()> {let mut w=self.index.writer::<TantivyDocument>(50_000_000)?;w.delete_term(tantivy::Term::from_field_u64(self.notebook,notebook_id as u64));w.commit()?;self.reader.reload()?;Ok(())}
    pub fn search(&self,notebook_id:i64,query:&str,source_ids:Option<&[i64]>,limit:usize)->tantivy::Result<Vec<Hit>> { if query.trim().is_empty(){return Ok(Vec::new())} let parser=QueryParser::for_index(&self.index,vec![self.name,self.body]); let parsed=parser.parse_query(query).unwrap_or_else(|_|Box::new(tantivy::query::EmptyQuery)); let mut clauses:Vec<(Occur,Box<dyn Query>)>=vec![(Occur::Must,Box::new(tantivy::query::TermQuery::new(tantivy::Term::from_field_u64(self.notebook,notebook_id as u64),IndexRecordOption::Basic))),(Occur::Must,parsed)];
        if let Some(ids)=source_ids { if ids.is_empty(){return Ok(Vec::new())} let ors=ids.iter().map(|id|(Occur::Should,Box::new(tantivy::query::TermQuery::new(tantivy::Term::from_field_u64(self.source,*id as u64),IndexRecordOption::Basic)) as Box<dyn Query>)).collect(); clauses.push((Occur::Must,Box::new(BooleanQuery::new(ors)))); }
        let q=BooleanQuery::new(clauses); let searcher=self.reader.searcher(); let docs=searcher.search(&q,&TopDocs::with_limit(limit.max(1)).order_by_score())?; let mut out=Vec::new(); for(score,addr) in docs { let d=searcher.doc::<TantivyDocument>(addr)?; out.push(Hit{source_id:d.get_first(self.source).and_then(|v|v.as_u64()).unwrap_or(0) as i64,ordinal:d.get_first(self.ordinal).and_then(|v|v.as_u64()).unwrap_or(0) as i64,display_name:d.get_first(self.name).and_then(|v|v.as_str()).unwrap_or("").into(),location:d.get_first(self.location).and_then(|v|v.as_str()).unwrap_or("").into(),score,body:d.get_first(self.body).and_then(|v|v.as_str()).unwrap_or("").into()}); } Ok(out)
    }
}

/// 업무자료함 표(노트북·출처·문단·노트·인용·태그·다시 색인 준비)를 만든다. 시험·옮기기용 단독 연결에서는 main에 만든다.
pub fn init_schema(c:&Connection)->rusqlite::Result<()> { init_schema_in(c,"main") }
/// `schema`(main 또는 klib 연결에 붙인 `nb`) 안에 표를 만든다. 외래키 부모 표는 같은 DB 파일 안에 있다.
pub fn init_schema_in(c:&Connection,schema:&str)->rusqlite::Result<()> { let s=schema; c.execute_batch(&format!("PRAGMA foreign_keys=ON;
CREATE TABLE IF NOT EXISTS {s}.notebook(id INTEGER PRIMARY KEY AUTOINCREMENT,name TEXT NOT NULL,description TEXT NOT NULL DEFAULT '',created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS {s}.nb_source(id INTEGER PRIMARY KEY AUTOINCREMENT,notebook_id INTEGER NOT NULL REFERENCES notebook(id) ON DELETE CASCADE,kind TEXT NOT NULL,origin TEXT NOT NULL DEFAULT '',display_name TEXT NOT NULL,status TEXT NOT NULL DEFAULT 'pending',reason TEXT NOT NULL DEFAULT '',paragraph_count INTEGER NOT NULL DEFAULT 0,original_size INTEGER NOT NULL DEFAULT 0,original_mtime INTEGER NOT NULL DEFAULT 0,created_at INTEGER NOT NULL DEFAULT 0,updated_at INTEGER NOT NULL DEFAULT 0,UNIQUE(notebook_id,id));
CREATE TABLE IF NOT EXISTS {s}.nb_paragraph(source_id INTEGER NOT NULL REFERENCES nb_source(id) ON DELETE CASCADE,ordinal INTEGER NOT NULL,location TEXT NOT NULL DEFAULT '',level INTEGER NOT NULL DEFAULT 0,kind TEXT NOT NULL DEFAULT 'text',body TEXT NOT NULL,PRIMARY KEY(source_id,ordinal),UNIQUE(source_id,ordinal));
CREATE TABLE IF NOT EXISTS {s}.nb_note(id INTEGER PRIMARY KEY AUTOINCREMENT,notebook_id INTEGER NOT NULL REFERENCES notebook(id) ON DELETE CASCADE,title TEXT NOT NULL,body TEXT NOT NULL,updated_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS {s}.nb_citation(id INTEGER PRIMARY KEY AUTOINCREMENT,note_id INTEGER NOT NULL REFERENCES nb_note(id) ON DELETE CASCADE,source_id INTEGER NOT NULL REFERENCES nb_source(id) ON DELETE CASCADE,paragraph_ordinal INTEGER NOT NULL,quote TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS {s}.nb_note_tag(note_id INTEGER NOT NULL REFERENCES nb_note(id) ON DELETE CASCADE,tag TEXT NOT NULL,PRIMARY KEY(note_id,tag));
CREATE TABLE IF NOT EXISTS {s}.nb_refresh(source_id INTEGER PRIMARY KEY REFERENCES nb_source(id) ON DELETE CASCADE,original_size INTEGER NOT NULL,original_mtime INTEGER NOT NULL,started_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS {s}.nb_refresh_paragraph(source_id INTEGER NOT NULL REFERENCES nb_refresh(source_id) ON DELETE CASCADE,ordinal INTEGER NOT NULL,location TEXT NOT NULL DEFAULT '',level INTEGER NOT NULL DEFAULT 0,kind TEXT NOT NULL DEFAULT 'text',body TEXT NOT NULL,PRIMARY KEY(source_id,ordinal));
CREATE INDEX IF NOT EXISTS {s}.idx_nb_note_tag_tag ON nb_note_tag(tag);
CREATE INDEX IF NOT EXISTS {s}.idx_nb_source_notebook ON nb_source(notebook_id); CREATE INDEX IF NOT EXISTS {s}.idx_nb_paragraph_source ON nb_paragraph(source_id,ordinal); CREATE INDEX IF NOT EXISTS {s}.idx_nb_note_notebook ON nb_note(notebook_id); CREATE INDEX IF NOT EXISTS {s}.idx_nb_citation_note ON nb_citation(note_id); CREATE INDEX IF NOT EXISTS {s}.idx_nb_citation_source_paragraph ON nb_citation(source_id,paragraph_ordinal);"))?; migrate_item_link(c,s) }

/// nb_source.item_id(출처가 나온 자료함 항목 id): 기존 DB에는 없을 수 있어 없으면 만든다. 항목이 지워져도 출처는 남으므로 외래키는 두지 않는다.
fn migrate_item_link(c:&Connection,s:&str)->rusqlite::Result<()> {
    let has:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('nb_source',?1) WHERE name='item_id')",[s],|r|r.get(0))?;
    if !has { c.execute_batch(&format!("ALTER TABLE {s}.nb_source ADD COLUMN item_id INTEGER;"))?; }
    c.execute_batch(&format!("CREATE INDEX IF NOT EXISTS {s}.idx_nb_source_item ON nb_source(notebook_id,item_id);"))
}

/// 출처 색인 상태(nb_source.status). 화면·Python·에이전트가 같은 값을 읽는다.
///
/// ```text
/// pending ─추가 완료─▶ ready ─원본 바뀜(nb_source_check)─▶ stale ─다시 색인 시작─▶ refreshing ─완료─▶ ready
///                        │  ▲                              │                         └─실패─▶ failed ─다시 색인 시작─▶ refreshing
///                        └──┴─원본 없어짐─▶ missing ─원본 다시 보임─▶ stale | ready(크기·시각 같음)
/// ```
/// 불가능한 전이: pending→stale(아직 색인 전), refreshing→stale·missing(다시 색인 중에는 검사가 상태를 덮지 않음), ready→failed(실패는 다시 색인에서만).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceStatus { Pending, Ready, Stale, Refreshing, Failed, Missing }
impl SourceStatus {
    pub fn as_str(self)->&'static str { match self { Self::Pending=>"pending", Self::Ready=>"ready", Self::Stale=>"stale", Self::Refreshing=>"refreshing", Self::Failed=>"failed", Self::Missing=>"missing" } }
    pub fn parse(s:&str)->Option<Self> { Some(match s { "pending"=>Self::Pending, "ready"=>Self::Ready, "stale"=>Self::Stale, "refreshing"=>Self::Refreshing, "failed"=>Self::Failed, "missing"=>Self::Missing, _=>return None }) }
    pub fn can_go(self,to:Self)->bool { use SourceStatus::*; self==to || matches!((self,to),
        (Pending,Ready)|(Pending,Failed)|(Ready,Stale)|(Ready,Missing)|(Ready,Refreshing)|(Stale,Refreshing)|(Stale,Missing)|(Stale,Ready)
        |(Refreshing,Ready)|(Refreshing,Failed)|(Failed,Refreshing)|(Failed,Missing)|(Missing,Stale)|(Missing,Ready)|(Missing,Refreshing)) }
    /// 다시 색인할 대상인지(쉬는 시간 작업이 고른다).
    pub fn wants_refresh(self)->bool { matches!(self,Self::Stale|Self::Failed) }
}
/// 상태를 바꾼다. 허용되지 않은 전이는 바꾸지 않고 Err.
pub fn set_status(c:&Connection,source_id:i64,to:SourceStatus,reason:&str)->Result<(),String>{
    let from:String=c.query_row("SELECT status FROM nb_source WHERE id=?1",[source_id],|r|r.get(0)).map_err(|e|e.to_string())?;
    let from=SourceStatus::parse(&from).unwrap_or(SourceStatus::Ready);
    if !from.can_go(to){return Err(format!("출처 상태를 {}에서 {}로 바꿀 수 없습니다.",from.as_str(),to.as_str()))}
    c.execute("UPDATE nb_source SET status=?1,reason=?2,updated_at=?3 WHERE id=?4",params![to.as_str(),reason,now(),source_id]).map_err(|e|e.to_string())?;Ok(())
}

/// 다시 색인 결과: 문단 수와 인용 연결(그대로·옮김·찾지 못함).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RefreshReport { pub paragraphs:usize, pub citations:usize, pub kept:usize, pub relinked:usize, pub unresolved:usize }

/// 다시 색인 준비: 원본 크기·시각을 적고 준비 문단을 비운다(상태 refreshing).
pub fn refresh_begin(c:&Connection,source_id:i64,size:i64,mtime:i64)->Result<(),String>{
    let tx=c.unchecked_transaction().map_err(|e|e.to_string())?;
    set_status(&tx,source_id,SourceStatus::Refreshing,"")?;
    tx.execute("DELETE FROM nb_refresh WHERE source_id=?1",[source_id]).map_err(|e|e.to_string())?;
    tx.execute("INSERT INTO nb_refresh(source_id,original_size,original_mtime,started_at) VALUES(?1,?2,?3,?4)",params![source_id,size,mtime,now()]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e|e.to_string())
}
/// 준비 문단을 덧붙인다(번호는 이어서). 준비가 없으면 Err(순서가 어긋난 요청).
pub fn refresh_append(c:&Connection,source_id:i64,ps:&[Paragraph])->Result<usize,String>{
    let tx=c.unchecked_transaction().map_err(|e|e.to_string())?;
    let started:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM nb_refresh WHERE source_id=?1)",[source_id],|r|r.get(0)).map_err(|e|e.to_string())?;
    if !started{return Err("다시 색인을 시작하지 않은 출처입니다(part 0부터 보내 주세요).".into())}
    let start:i64=tx.query_row("SELECT COALESCE(MAX(ordinal)+1,0) FROM nb_refresh_paragraph WHERE source_id=?1",[source_id],|r|r.get(0)).map_err(|e|e.to_string())?;
    {let mut s=tx.prepare("INSERT INTO nb_refresh_paragraph(source_id,ordinal,location,level,kind,body) VALUES(?1,?2,?3,?4,?5,?6)").map_err(|e|e.to_string())?;
     for(i,p)in ps.iter().enumerate(){s.execute(params![source_id,start+i as i64,p.location,p.level,p.kind,p.body]).map_err(|e|e.to_string())?;}}
    let n:i64=tx.query_row("SELECT count(*) FROM nb_refresh_paragraph WHERE source_id=?1",[source_id],|r|r.get(0)).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e|e.to_string())?;Ok(n as usize)
}
/// 준비 문단으로 출처 문단을 한 번에 바꾼다(출처 번호 그대로). 인용은 같은 번호 문단에 인용 글이 있으면 그대로, 없으면
/// 인용 글이 든 가장 가까운 문단으로 옮기고, 어디에도 없으면 번호를 그대로 둔다(인용 글 사본은 노트에 남아 있다).
/// 문단이 하나도 없으면 바꾸지 않고 Err(옛 문단 유지).
pub fn refresh_commit(c:&Connection,source_id:i64)->Result<RefreshReport,String>{
    let e=|e:rusqlite::Error|e.to_string();
    let tx=c.unchecked_transaction().map_err(e)?;
    let meta:Option<(i64,i64)>=tx.query_row("SELECT original_size,original_mtime FROM nb_refresh WHERE source_id=?1",[source_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(e)?;
    let Some((size,mtime))=meta else{return Err("다시 색인을 시작하지 않은 출처입니다.".into())};
    let staged:Vec<Paragraph>={let mut s=tx.prepare("SELECT source_id,ordinal,location,level,kind,body FROM nb_refresh_paragraph WHERE source_id=?1 ORDER BY ordinal").map_err(e)?;
        s.query_map([source_id],|r|Ok(Paragraph{source_id:r.get(0)?,ordinal:r.get(1)?,location:r.get(2)?,level:r.get(3)?,kind:r.get(4)?,body:r.get(5)?})).map_err(e)?.collect::<rusqlite::Result<_>>().map_err(e)?};
    if staged.is_empty(){return Err("출처에서 본문 문단을 찾지 못했습니다.".into())}
    let citations:Vec<(i64,i64,String)>={let mut s=tx.prepare("SELECT id,paragraph_ordinal,quote FROM nb_citation WHERE source_id=?1 ORDER BY id").map_err(e)?;
        s.query_map([source_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(e)?.collect::<rusqlite::Result<_>>().map_err(e)?};
    let mut report=RefreshReport{paragraphs:staged.len(),citations:citations.len(),..Default::default()};
    for(id,ordinal,quote) in &citations{
        match relink(&staged,*ordinal,quote){
            Some(o) if o==*ordinal=>report.kept+=1,
            Some(o)=>{tx.execute("UPDATE nb_citation SET paragraph_ordinal=?1 WHERE id=?2",params![o,id]).map_err(e)?;report.relinked+=1}
            None=>report.unresolved+=1,
        }
    }
    tx.execute("DELETE FROM nb_paragraph WHERE source_id=?1",[source_id]).map_err(e)?;
    tx.execute("INSERT INTO nb_paragraph(source_id,ordinal,location,level,kind,body) SELECT source_id,ordinal,location,level,kind,body FROM nb_refresh_paragraph WHERE source_id=?1",[source_id]).map_err(e)?;
    tx.execute("DELETE FROM nb_refresh WHERE source_id=?1",[source_id]).map_err(e)?;
    set_status(&tx,source_id,SourceStatus::Ready,"")?;
    tx.execute("UPDATE nb_source SET paragraph_count=?1,original_size=?2,original_mtime=?3 WHERE id=?4",params![staged.len() as i64,size,mtime,source_id]).map_err(e)?;
    tx.commit().map_err(e)?;Ok(report)
}
/// 다시 색인 실패: 준비 문단을 버리고 옛 문단은 그대로 둔 채 failed로.
pub fn refresh_fail(c:&Connection,source_id:i64,reason:&str)->Result<(),String>{
    let tx=c.unchecked_transaction().map_err(|e|e.to_string())?;
    tx.execute("DELETE FROM nb_refresh WHERE source_id=?1",[source_id]).map_err(|e|e.to_string())?;
    let status:String=tx.query_row("SELECT status FROM nb_source WHERE id=?1",[source_id],|r|r.get(0)).map_err(|e|e.to_string())?;
    if status!="failed"{ if SourceStatus::parse(&status)!=Some(SourceStatus::Refreshing){set_status(&tx,source_id,SourceStatus::Refreshing,"")?;} set_status(&tx,source_id,SourceStatus::Failed,reason)?; }
    else { tx.execute("UPDATE nb_source SET reason=?1 WHERE id=?2",params![reason,source_id]).map_err(|e|e.to_string())?; }
    tx.commit().map_err(|e|e.to_string())
}
/// 인용 글이 든 문단 번호: 같은 번호 문단이 먼저, 아니면 가장 가까운 번호(같으면 앞 번호). 공백 차이는 무시한다.
fn relink(ps:&[Paragraph],ordinal:i64,quote:&str)->Option<i64>{
    let squash=|s:&str|s.split_whitespace().collect::<Vec<_>>().join(" ");
    let q=squash(quote); if q.is_empty(){return ps.iter().any(|p|p.ordinal==ordinal).then_some(ordinal)}
    let has=|p:&Paragraph|squash(&p.body).contains(&q);
    if ps.iter().any(|p|p.ordinal==ordinal&&has(p)){return Some(ordinal)}
    ps.iter().filter(|p|has(p)).min_by_key(|p|((p.ordinal-ordinal).abs(),p.ordinal)).map(|p|p.ordinal)
}
/// 원본 크기·시각(ms). 원본이 없으면 None.
pub fn file_signature(path:&Path)->Option<(i64,i64)>{let m=std::fs::metadata(path).ok()?;if !m.is_file(){return None}Some((m.len() as i64,m.modified().ok().and_then(|t|t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d|d.as_millis() as i64).unwrap_or(0)))}

/// 원본 변경 검사 한 줄.
#[derive(Clone, Debug, PartialEq)]
pub struct CheckRow { pub source_id:i64, pub notebook_id:i64, pub path:String, pub kind:String, pub status:SourceStatus }
/// 출처마다 원본 크기·시각을 저장값과 견줘 상태를 고친다(ready↔stale↔missing). 다시 색인 중(refreshing)인 출처는 건드리지 않는다.
/// 크기·시각을 모르는 옛 출처(둘 다 0)는 지금 값을 기준으로 적고 다시 색인하지 않는다.
/// `path_of`는 출처의 지금 원본 경로(자료함 항목과 연결된 출처는 항목의 지금 사본 경로)를 준다.
pub fn check_sources(c:&Connection,notebook_id:Option<i64>,path_of:impl Fn(&Source,Option<i64>)->String)->Result<Vec<CheckRow>,String>{
    let e=|e:rusqlite::Error|e.to_string();
    let rows:Vec<(Source,Option<i64>)>={let mut s=c.prepare("SELECT id,notebook_id,kind,origin,display_name,status,reason,paragraph_count,original_size,original_mtime,item_id FROM nb_source WHERE ?1 IS NULL OR notebook_id=?1 ORDER BY id").map_err(e)?;
        s.query_map([notebook_id],|r|Ok((Source{id:r.get(0)?,notebook_id:r.get(1)?,kind:r.get(2)?,origin:r.get(3)?,display_name:r.get(4)?,status:r.get(5)?,reason:r.get(6)?,paragraph_count:r.get(7)?,original_size:r.get(8)?,original_mtime:r.get(9)?},r.get(10)?))).map_err(e)?.collect::<rusqlite::Result<_>>().map_err(e)?};
    let mut out=Vec::new();
    for(s,item) in rows{
        let status=SourceStatus::parse(&s.status).unwrap_or(SourceStatus::Ready);
        if matches!(status,SourceStatus::Pending|SourceStatus::Refreshing){continue}
        let path=path_of(&s,item);
        let next=match file_signature(Path::new(&path)){
            None=>SourceStatus::Missing,
            Some((size,mtime)) if s.original_size==0&&s.original_mtime==0=>{c.execute("UPDATE nb_source SET original_size=?1,original_mtime=?2 WHERE id=?3",params![size,mtime,s.id]).map_err(e)?;if status==SourceStatus::Missing{SourceStatus::Ready}else{status}}
            Some((size,mtime)) if size==s.original_size&&mtime==s.original_mtime=>if matches!(status,SourceStatus::Missing|SourceStatus::Stale){SourceStatus::Ready}else{status},
            Some(_)=>if status==SourceStatus::Failed{status}else{SourceStatus::Stale},
        };
        if next!=status{set_status(c,s.id,next,if next==SourceStatus::Missing{"원본 파일을 찾을 수 없습니다."}else{""})?;}
        out.push(CheckRow{source_id:s.id,notebook_id:s.notebook_id,path,kind:s.kind,status:next});
    }
    Ok(out)
}
pub fn set_source_item(c:&Connection,source_id:i64,item_id:i64)->rusqlite::Result<()> {c.execute("UPDATE nb_source SET item_id=?1 WHERE id=?2",params![item_id,source_id])?;Ok(())}
pub fn source_for_item(c:&Connection,notebook_id:i64,item_id:i64)->rusqlite::Result<Option<i64>> {c.query_row("SELECT id FROM nb_source WHERE notebook_id=?1 AND item_id=?2 ORDER BY id LIMIT 1",params![notebook_id,item_id],|r|r.get(0)).optional()}
/// 출처의 원본 자료함 항목 id. 출처가 없으면 None, 항목과 연결되지 않은 출처는 Some(None).
pub fn source_item_id(c:&Connection,notebook_id:i64,source_id:i64)->rusqlite::Result<Option<Option<i64>>> {c.query_row("SELECT item_id FROM nb_source WHERE id=?1 AND notebook_id=?2",params![source_id,notebook_id],|r|r.get::<_,Option<i64>>(0)).optional()}

fn now()->i64{std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64}
pub fn create(c:&Connection,name:&str,description:&str)->rusqlite::Result<i64>{let t=now();c.execute("INSERT INTO notebook(name,description,created_at,updated_at) VALUES(?1,?2,?3,?3)",params![name,description,t])?;Ok(c.last_insert_rowid())}
pub fn update(c:&Connection,id:i64,name:&str,description:&str)->rusqlite::Result<bool>{Ok(c.execute("UPDATE notebook SET name=?1,description=?2,updated_at=?3 WHERE id=?4",params![name,description,now(),id])?!=0)}
pub fn delete(c:&Connection,id:i64)->rusqlite::Result<bool>{Ok(c.execute("DELETE FROM notebook WHERE id=?1",[id])?!=0)}
pub fn list(c:&Connection)->rusqlite::Result<Vec<(i64,String,String,i64,i64)>>{let mut s=c.prepare("SELECT id,name,description,created_at,updated_at FROM notebook ORDER BY updated_at DESC,id DESC")?;s.query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?.collect()}
pub fn add_source(c:&Connection,notebook_id:i64,kind:&str,origin:&str,name:&str)->rusqlite::Result<i64>{let t=now() as i64;c.execute("INSERT INTO nb_source(notebook_id,kind,origin,display_name,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5)",params![notebook_id,kind,origin,name,t])?;Ok(c.last_insert_rowid())}
pub fn sources(c:&Connection,notebook_id:i64)->rusqlite::Result<Vec<Source>>{let mut s=c.prepare("SELECT id,notebook_id,kind,origin,display_name,status,reason,paragraph_count,original_size,original_mtime FROM nb_source WHERE notebook_id=?1 ORDER BY id")?;s.query_map([notebook_id],|r|Ok(Source{id:r.get(0)?,notebook_id:r.get(1)?,kind:r.get(2)?,origin:r.get(3)?,display_name:r.get(4)?,status:r.get(5)?,reason:r.get(6)?,paragraph_count:r.get(7)?,original_size:r.get(8)?,original_mtime:r.get(9)?}))?.collect()}
pub fn source_paragraphs(c:&Connection,notebook_id:i64,source_id:i64)->rusqlite::Result<Vec<Paragraph>>{let owner:Option<i64>=c.query_row("SELECT notebook_id FROM nb_source WHERE id=?1",[source_id],|r|r.get(0)).optional()?;if owner!=Some(notebook_id){return Err(rusqlite::Error::QueryReturnedNoRows)}paragraphs(c,source_id)}
pub fn remove_source(c:&Connection,id:i64)->rusqlite::Result<bool>{Ok(c.execute("DELETE FROM nb_source WHERE id=?1",[id])?!=0)}
pub fn replace_paragraphs(c:&Connection,source_id:i64,ps:&[Paragraph])->rusqlite::Result<()> {let tx=c.unchecked_transaction()?;tx.execute("DELETE FROM nb_paragraph WHERE source_id=?1",[source_id])?;{let mut s=tx.prepare("INSERT INTO nb_paragraph(source_id,ordinal,location,level,kind,body) VALUES(?1,?2,?3,?4,?5,?6)")?;for p in ps{s.execute(params![source_id,p.ordinal,p.location,p.level,p.kind,p.body])?;}}tx.execute("UPDATE nb_source SET status='ready',reason='',paragraph_count=?1,updated_at=?2 WHERE id=?3",params![ps.len() as i64,now() as i64,source_id])?;tx.commit()}
/// 문단을 기존 문단 뒤에 덧붙인다(번호는 이어서 매긴다). 나눠 보낸 추출 결과를 모을 때 쓴다.
pub fn append_paragraphs(c:&Connection,source_id:i64,ps:&[Paragraph])->rusqlite::Result<i64>{let tx=c.unchecked_transaction()?;let start:i64=tx.query_row("SELECT COALESCE(MAX(ordinal)+1,0) FROM nb_paragraph WHERE source_id=?1",[source_id],|r|r.get(0))?;{let mut s=tx.prepare("INSERT INTO nb_paragraph(source_id,ordinal,location,level,kind,body) VALUES(?1,?2,?3,?4,?5,?6)")?;for(i,p)in ps.iter().enumerate(){s.execute(params![source_id,start+i as i64,p.location,p.level,p.kind,p.body])?;}}tx.execute("UPDATE nb_source SET status='ready',reason='',paragraph_count=(SELECT count(*) FROM nb_paragraph WHERE source_id=?1),updated_at=?2 WHERE id=?1",params![source_id,now()])?;tx.commit()?;Ok(start)}
pub fn paragraphs(c:&Connection,source_id:i64)->rusqlite::Result<Vec<Paragraph>>{let mut s=c.prepare("SELECT source_id,ordinal,location,level,kind,body FROM nb_paragraph WHERE source_id=?1 ORDER BY ordinal")?;s.query_map([source_id],|r|Ok(Paragraph{source_id:r.get(0)?,ordinal:r.get(1)?,location:r.get(2)?,level:r.get(3)?,kind:r.get(4)?,body:r.get(5)?}))?.collect()}
pub fn save_note(c:&Connection,id:Option<i64>,notebook_id:i64,title:&str,body:&str)->rusqlite::Result<i64>{let t=now();if let Some(id)=id{let changed=c.execute("UPDATE nb_note SET title=?1,body=?2,updated_at=?3 WHERE id=?4 AND notebook_id=?5",params![title,body,t,id,notebook_id])?;if changed==0{return Err(rusqlite::Error::QueryReturnedNoRows)}Ok(id)}else{c.execute("INSERT INTO nb_note(notebook_id,title,body,updated_at) VALUES(?1,?2,?3,?4)",params![notebook_id,title,body,t])?;Ok(c.last_insert_rowid())}}
/// 노트를 저장한다. `tags`가 Some이면 노트 태그를 그 목록으로 바꾸고, None이면 기존 태그를 그대로 둔다.
pub fn save_note_with_citations(c:&Connection,id:Option<i64>,notebook_id:i64,title:&str,body:&str,citations:&[(i64,i64,String)],tags:Option<&[String]>)->rusqlite::Result<i64>{let tx=c.unchecked_transaction()?;let note=save_note(&tx,id,notebook_id,title,body)?;if let Some(tags)=tags{set_note_tags(&tx,note,tags)?;}if id.is_some(){tx.execute("DELETE FROM nb_citation WHERE note_id=?1",[note])?;}for(source,ordinal,quote)in citations{let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM nb_note n JOIN nb_source s ON s.notebook_id=n.notebook_id JOIN nb_paragraph p ON p.source_id=s.id WHERE n.id=?1 AND s.id=?2 AND p.ordinal=?3)",params![note,source,ordinal],|r|r.get(0))?;if !valid{return Err(rusqlite::Error::InvalidQuery)}tx.execute("INSERT INTO nb_citation(note_id,source_id,paragraph_ordinal,quote) VALUES(?1,?2,?3,?4)",params![note,source,ordinal,quote])?;}tx.commit()?;Ok(note)}
pub fn save_citation(c:&Connection,note_id:i64,source_id:i64,ordinal:i64,quote:&str)->rusqlite::Result<i64>{
    let tx=c.unchecked_transaction()?;
    let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM nb_note n JOIN nb_source s ON s.notebook_id=n.notebook_id JOIN nb_paragraph p ON p.source_id=s.id WHERE n.id=?1 AND s.id=?2 AND p.ordinal=?3)",params![note_id,source_id,ordinal],|r|r.get(0))?;
    if !valid{return Err(rusqlite::Error::InvalidQuery)}
    tx.execute("INSERT INTO nb_citation(note_id,source_id,paragraph_ordinal,quote) VALUES(?1,?2,?3,?4)",params![note_id,source_id,ordinal,quote])?;
    let id=tx.last_insert_rowid(); tx.commit()?; Ok(id)
}
pub fn citations(c:&Connection,note_id:i64)->rusqlite::Result<Vec<(i64,i64,i64,String)>>{
    let mut s=c.prepare("SELECT source_id,paragraph_ordinal,id,quote FROM nb_citation WHERE note_id=?1 ORDER BY id")?;
    s.query_map([note_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?.collect()
}
/// 규칙 기반 변환(개요·표·숫자 문장·핵심어). 규칙 자체는 `nb_transform`에 있고, 여기서는 문단 형식만 맞춘다.
pub fn transform(items:&[Paragraph],kind:&str,limit:usize)->Result<Vec<TransformItem>,String>{
    use crate::nb_transform as nt;
    let to_para=|p:&Paragraph,text:&str|nt::Para{source_id:p.source_id.to_string(),seq:p.ordinal.max(0) as u64,page:None,level:(p.level>0).then(||p.level.min(u8::MAX as i64) as u8),text:text.to_string(),table:p.kind.eq_ignore_ascii_case("table")};
    let item=|p:&Paragraph,kind:&str,text:String,score:f64|TransformItem{source_id:p.source_id,ordinal:p.ordinal,location:p.location.clone(),level:p.level,kind:kind.to_string(),text,score};
    let find=|source:&str,seq:u64|items.iter().find(|p|p.source_id.to_string()==source&&p.ordinal.max(0) as u64==seq);
    let paras:Vec<nt::Para>=items.iter().map(|p|to_para(p,&p.body)).collect();
    let mut out=Vec::new();
    match kind{
        "outline"=>for o in nt::outline(&paras){if let Some(p)=find(&o.source_id,o.seq){out.push(item(p,&p.kind,o.text,p.level as f64))}},
        // 표 묶음마다 원문 문단을 차례로 낸다. score는 1부터 세는 표 묶음 번호.
        "tables"=>for(n,group) in nt::tables(&paras).into_iter().enumerate(){for seq in group.paragraph_seqs{if let Some(p)=find(&group.source_id,seq){out.push(item(p,"table",p.body.clone(),(n+1) as f64))}}},
        "numbers"=>{let sentences:Vec<(&Paragraph,nt::Para)>=items.iter().flat_map(|p|split_sentences(&p.body).into_iter().map(move|s|(p,s))).map(|(p,s)|(p,to_para(p,s))).collect();let only:Vec<nt::Para>=sentences.iter().map(|(_,s)|s.clone()).collect();let hits=nt::numbers(&only);let mut next=0;for hit in hits{while let Some((p,s))=sentences.get(next){next+=1;if s.source_id==hit.source_id&&s.seq==hit.seq&&s.text==hit.text{out.push(item(p,&p.kind,hit.text,1.0));break}}}},
        "keywords"=>for k in nt::keywords(&paras,limit){if let Some(p)=items.iter().find(|p|p.body.to_lowercase().contains(&k.term)){out.push(item(p,"keyword",k.term,k.count as f64))}},
        _=>return Err(format!("알 수 없는 변환 종류: {kind}"))
    }
    out.truncate(limit);Ok(out)
}
/// 문장 나누기: 마침표류 뒤가 공백·끝이고 앞이 숫자가 아닐 때만 끊는다(“2026. 11. 5.”, “3.5%”는 한 문장에 둔다). 줄바꿈은 항상 끊는다.
fn split_sentences(text:&str)->Vec<&str>{
    let mut out=Vec::new();let mut start=0;let chars:Vec<(usize,char)>=text.char_indices().collect();
    for(i,&(at,c)) in chars.iter().enumerate(){
        let end=at+c.len_utf8();
        let cut=c=='\n'||(matches!(c,'.'|'!'|'?'|'。'|'！'|'？')&&chars.get(i+1).is_none_or(|(_,n)|n.is_whitespace())&&!(c=='.'&&i>0&&chars[i-1].1.is_ascii_digit()));
        if cut{let s=text[start..end].trim();if !s.is_empty(){out.push(s)}start=end;}
    }
    let rest=text[start..].trim();if !rest.is_empty(){out.push(rest)}out
}
pub const MAX_TAGS_PER_NOTE:usize=20;
pub const MAX_TAG_CHARS:usize=30;
/// 태그 정리: 앞뒤 공백과 앞의 '#'을 떼고, 안쪽 공백은 '_'로 바꾸고, 30자로 자르고, 대소문자 무시 중복을 뺀다(처음 쓴 순서 유지, 최대 20개).
pub fn normalize_tags<S:AsRef<str>>(tags:&[S])->Vec<String>{
    let mut out:Vec<String>=Vec::new();
    for t in tags{
        let t=t.as_ref().trim().trim_start_matches('#').trim();
        if t.is_empty(){continue}
        let t:String=t.split_whitespace().collect::<Vec<_>>().join("_").chars().take(MAX_TAG_CHARS).collect();
        if out.iter().any(|o|o.to_lowercase()==t.to_lowercase()){continue}
        out.push(t);
        if out.len()>=MAX_TAGS_PER_NOTE{break}
    }
    out
}
pub fn set_note_tags<S:AsRef<str>>(c:&Connection,note_id:i64,tags:&[S])->rusqlite::Result<()>{
    c.execute("DELETE FROM nb_note_tag WHERE note_id=?1",[note_id])?;
    let mut s=c.prepare("INSERT OR IGNORE INTO nb_note_tag(note_id,tag) VALUES(?1,?2)")?;
    for t in normalize_tags(tags){s.execute(params![note_id,t])?;}
    Ok(())
}
/// 노트 태그(넣은 순서).
pub fn note_tags(c:&Connection,note_id:i64)->rusqlite::Result<Vec<String>>{let mut s=c.prepare("SELECT tag FROM nb_note_tag WHERE note_id=?1 ORDER BY rowid")?;s.query_map([note_id],|r|r.get(0))?.collect()}
/// 노트북의 태그와 노트 수(많은 순, 같으면 이름순).
pub fn notebook_tags(c:&Connection,notebook_id:i64)->rusqlite::Result<Vec<(String,i64)>>{let mut s=c.prepare("SELECT t.tag,count(*) FROM nb_note_tag t JOIN nb_note n ON n.id=t.note_id WHERE n.notebook_id=?1 GROUP BY t.tag ORDER BY count(*) DESC,t.tag")?;s.query_map([notebook_id],|r|Ok((r.get(0)?,r.get(1)?)))?.collect()}
/// 노트 목록(최근 고친 순). `tag`가 있으면 그 태그(대소문자 무시, 앞 '#' 무시)가 달린 노트만.
pub fn notes(c:&Connection,notebook_id:i64,tag:Option<&str>)->rusqlite::Result<Vec<(i64,String,String,i64)>>{
    let tag=tag.map(|t|t.trim().trim_start_matches('#').trim().to_string()).filter(|t|!t.is_empty());
    let map=|r:&rusqlite::Row|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?));
    match tag{
        Some(t)=>{let mut s=c.prepare("SELECT id,title,body,updated_at FROM nb_note n WHERE notebook_id=?1 AND EXISTS(SELECT 1 FROM nb_note_tag t WHERE t.note_id=n.id AND lower(t.tag)=lower(?2)) ORDER BY updated_at DESC,id DESC")?;s.query_map(params![notebook_id,t],map)?.collect()}
        None=>{let mut s=c.prepare("SELECT id,title,body,updated_at FROM nb_note WHERE notebook_id=?1 ORDER BY updated_at DESC,id DESC")?;s.query_map([notebook_id],map)?.collect()}
    }
}

/// 출처 비교 한 칸: 한 출처에서 찾은 근거 문단(점수순).
#[derive(Clone, Debug)]
pub struct CompareGroup { pub source:Source, pub hits:Vec<Hit> }
/// 같은 검색어로 출처마다 따로 근거를 찾아 나란히 놓는다. 출처마다 따로 찾으므로 점수가 낮은 출처도 빠지지 않는다.
/// 순서: 근거가 있는 출처(가장 높은 점수순) → 근거가 없는 출처(출처 순). `scope`가 Some이면 그 출처만.
pub fn compare_sources<E>(sources:&[Source],scope:Option<&[i64]>,per_source:usize,mut search:impl FnMut(i64,usize)->Result<Vec<Hit>,E>)->Result<Vec<CompareGroup>,E>{
    let mut groups=Vec::new();
    for s in sources{
        if scope.is_some_and(|ids|!ids.contains(&s.id)){continue}
        let mut hits:Vec<Hit>=search(s.id,per_source.max(1))?.into_iter().filter(|h|h.source_id==s.id).collect();
        hits.sort_by(|a,b|b.score.total_cmp(&a.score).then(a.ordinal.cmp(&b.ordinal)));
        hits.truncate(per_source.max(1));
        groups.push(CompareGroup{source:s.clone(),hits});
    }
    let best=|g:&CompareGroup|g.hits.first().map(|h|h.score).unwrap_or(f32::NEG_INFINITY);
    groups.sort_by(|a,b|best(b).total_cmp(&best(a)).then(a.source.id.cmp(&b.source.id)));
    Ok(groups)
}
/// 출처 비교를 Markdown 절로 만든다(출처마다 소제목, 근거는 인용 블록).
pub fn compare_md(query:&str,groups:&[CompareGroup])->String{
    let mut out=format!("## 출처 비교: {}\n",query.trim());
    for g in groups{
        out.push_str(&format!("\n### {}",g.source.display_name));
        if !g.source.origin.is_empty(){out.push_str(&format!(" ({})",g.source.origin))}
        out.push('\n');
        if g.hits.is_empty(){out.push_str("\n근거 문단 없음\n");continue}
        for h in &g.hits{
            let loc=if h.location.is_empty(){String::new()}else{format!(" · {}",h.location)};
            out.push_str(&format!("\n> {}\n> — 문단 {}{}\n",h.body.replace('\n',"\n> "),h.ordinal,loc));
        }
    }
    out
}

pub fn export_md(c:&Connection,notebook_id:i64,note_id:i64)->rusqlite::Result<Option<String>>{let note:Option<(String,String)>=c.query_row("SELECT title,body FROM nb_note WHERE id=?1 AND notebook_id=?2",params![note_id,notebook_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;let Some((title,body))=note else{return Ok(None)};let tags=note_tags(c,note_id)?;let tag_line=if tags.is_empty(){String::new()}else{format!("태그: {}\n\n",tags.iter().map(|t|format!("#{t}")).collect::<Vec<_>>().join(" "))};let mut out=format!("# {title}\n\n{tag_line}{body}");let mut s=c.prepare("SELECT c.quote,src.display_name,src.origin,c.paragraph_ordinal FROM nb_citation c JOIN nb_source src ON src.id=c.source_id WHERE c.note_id=?1 ORDER BY c.id")?;let rows=s.query_map([note_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?)))?;for row in rows{let(q,n,path,o)=row?;out.push_str(&format!("\n\n> {q}\n> — {n} ({path}), 문단 {o}"));}Ok(Some(out))}

pub fn source_by_id(c:&Connection,id:i64)->rusqlite::Result<Option<Source>>{c.query_row("SELECT id,notebook_id,kind,origin,display_name,status,reason,paragraph_count,original_size,original_mtime FROM nb_source WHERE id=?1",[id],|r|Ok(Source{id:r.get(0)?,notebook_id:r.get(1)?,kind:r.get(2)?,origin:r.get(3)?,display_name:r.get(4)?,status:r.get(5)?,reason:r.get(6)?,paragraph_count:r.get(7)?,original_size:r.get(8)?,original_mtime:r.get(9)?})).optional()}
pub fn index_root(db_path:&str)->PathBuf{Path::new(db_path).parent().unwrap_or(Path::new(".")).join("notebook_tantivy")}

#[cfg(test)] mod tests { use super::*; fn db()->Connection{let c=Connection::open_in_memory().unwrap();init_schema(&c).unwrap();c} fn p(s:i64,n:i64,l:i64,k:&str,b:&str)->Paragraph{Paragraph{source_id:s,ordinal:n,location:"1".into(),level:l,kind:k.into(),body:b.into()}}
#[test]fn source_scope_and_search_order(){let dir=std::env::temp_dir().join(format!("nb-index-{}",std::process::id()));let ix=NotebookIndex::open(&dir).unwrap();let s1=Source{id:1,notebook_id:1,kind:"txt".into(),origin:"".into(),display_name:"하나".into(),status:"ready".into(),reason:"".into(),paragraph_count:2,original_size:0,original_mtime:0};let s2=Source{id:2,..s1.clone()};ix.replace_source(1,&s1,&[p(1,0,0,"text","서울 정책 자료"),p(1,1,0,"text","서울 서울 정책")]).unwrap();ix.replace_source(1,&s2,&[p(2,0,0,"text","서울 예산 정책")]).unwrap();let hits=ix.search(1,"서울 정책",None,10).unwrap();assert!(hits.len()>=2);assert!(hits[0].score>=hits[1].score);assert!(ix.search(1,"서울 정책",Some(&[2]),10).unwrap().iter().all(|h|h.source_id==2));assert!(ix.search(2,"서울 정책",None,10).unwrap().is_empty());let _=std::fs::remove_dir_all(dir);}
#[test]fn transforms_four_kinds(){let ps=vec![p(1,0,2,"text","정책 개요 2025년 예산. 다음 문장."),p(1,1,0,"table","항목 금액"),p(1,2,0,"text","정책 정책 자료")];assert_eq!(transform(&ps,"outline",10).unwrap()[0].text,"정책 개요 2025년 예산. 다음 문장.");assert_eq!(transform(&ps,"tables",10).unwrap()[0].text,"항목 금액");assert_eq!(transform(&ps,"numbers",10).unwrap()[0].text,"정책 개요 2025년 예산.");let keywords=transform(&ps,"keywords",2).unwrap();assert_eq!(keywords[0].text,"정책");assert_eq!(keywords[0].score,3.0);assert!(keywords[0].source_id>0);}
#[test]fn transforms_keep_dates_in_one_sentence_and_group_table_rows(){let ps=vec![p(1,0,0,"text","신청서는 2026. 11. 5.까지 제출합니다. 증가율은 3.5%입니다. 안내 문장."),p(1,1,0,"table","구분\t금액"),p(1,2,0,"table","예산\t1,250만원"),p(1,3,0,"text","본문"),p(1,4,0,"text","[표] 일정|담당")];let n=transform(&ps,"numbers",10).unwrap();assert_eq!(n.iter().map(|x|x.text.as_str()).collect::<Vec<_>>(),vec!["신청서는 2026. 11. 5.까지 제출합니다.","증가율은 3.5%입니다.","예산\t1,250만원"]);let t=transform(&ps,"tables",10).unwrap();assert_eq!(t.iter().map(|x|(x.ordinal,x.score)).collect::<Vec<_>>(),vec![(1,1.0),(2,1.0),(4,2.0)]);assert_eq!(t[0].text,"구분\t금액");assert!(transform(&ps,"요약",10).is_err());}
#[test]fn append_continues_ordinals_and_count(){let c=db();let n=create(&c,"노트","").unwrap();let s=add_source(&c,n,"pdf","a.pdf","a").unwrap();replace_paragraphs(&c,s,&[p(s,0,1,"text","하나"),p(s,1,0,"text","둘")]).unwrap();assert_eq!(append_paragraphs(&c,s,&[p(s,0,0,"table","셋\t넷")]).unwrap(),2);let ps=paragraphs(&c,s).unwrap();assert_eq!(ps.iter().map(|p|(p.ordinal,p.kind.as_str())).collect::<Vec<_>>(),vec![(0,"text"),(1,"text"),(2,"table")]);assert_eq!(source_by_id(&c,s).unwrap().unwrap().paragraph_count,3);}
#[test]fn citation_snapshot_and_cascade(){let c=db();let n=create(&c,"노트","설명").unwrap();let s=add_source(&c,n,"txt","source.txt","원문").unwrap();replace_paragraphs(&c,s,&[p(s,0,0,"text","수정 전 원문 인용")]).unwrap();let note=save_note(&c,None,n,"제목","본문").unwrap();let quote="수정 전 원문 인용";save_citation(&c,note,s,0,quote).unwrap();replace_paragraphs(&c,s,&[p(s,0,0,"text","새 원문")]).unwrap();let saved:String=c.query_row("SELECT quote FROM nb_citation WHERE note_id=?1",[note],|r|r.get(0)).unwrap();assert_eq!(saved,quote);c.execute("UPDATE nb_source SET display_name='바뀐 원본' WHERE id=?1",[s]).unwrap();let md=export_md(&c,n,note).unwrap().unwrap();assert!(md.contains(quote));assert!(md.contains("source.txt"));assert!(export_md(&c,n+1,note).unwrap().is_none());delete(&c,n).unwrap();let count:i64=c.query_row("SELECT count(*) FROM nb_citation",[],|r|r.get(0)).unwrap();assert_eq!(count,0);}
#[test]fn tags_normalize_filter_export_and_cascade(){
    assert_eq!(normalize_tags(&["  #예산 ","예산","복지 정책","","#","EU","eu"]),vec!["예산","복지_정책","EU"]);
    assert_eq!(normalize_tags(&[&"가".repeat(40)])[0].chars().count(),MAX_TAG_CHARS);
    assert_eq!(normalize_tags(&(0..30).map(|i|format!("t{i}")).collect::<Vec<_>>()).len(),MAX_TAGS_PER_NOTE);
    let c=db();let n=create(&c,"노트","").unwrap();let other=create(&c,"다른","").unwrap();
    let a=save_note_with_citations(&c,None,n,"가","본문 가",&[],Some(&["예산".into(),"복지".into()])).unwrap();
    let b=save_note_with_citations(&c,None,n,"나","본문 나",&[],Some(&["#예산".into()])).unwrap();
    save_note_with_citations(&c,None,other,"다","",&[],Some(&["예산".into()])).unwrap();
    assert_eq!(note_tags(&c,a).unwrap(),vec!["예산","복지"]);
    assert_eq!(notebook_tags(&c,n).unwrap(),vec![("예산".to_string(),2),("복지".to_string(),1)]);
    let mut ids:Vec<i64>=notes(&c,n,Some("#예산")).unwrap().into_iter().map(|r|r.0).collect();ids.sort();assert_eq!(ids,vec![a,b]);
    assert_eq!(notes(&c,n,Some("복지")).unwrap().iter().map(|r|r.0).collect::<Vec<_>>(),vec![a]);
    assert!(notes(&c,n,Some("없음")).unwrap().is_empty());
    assert_eq!(notes(&c,n,Some("  ")).unwrap().len(),2);
    // 태그를 안 보내면(None) 기존 태그 유지, 빈 목록이면 지움.
    save_note_with_citations(&c,Some(a),n,"가2","본문",&[],None).unwrap();assert_eq!(note_tags(&c,a).unwrap().len(),2);
    let md=export_md(&c,n,a).unwrap().unwrap();assert!(md.starts_with("# 가2\n\n태그: #예산 #복지\n\n본문"),"{md}");
    save_note_with_citations(&c,Some(a),n,"가2","본문",&[],Some(&[])).unwrap();assert!(note_tags(&c,a).unwrap().is_empty());
    assert!(!export_md(&c,n,a).unwrap().unwrap().contains("태그:"));
    c.execute("DELETE FROM nb_note WHERE id=?1",[b]).unwrap();
    let left:i64=c.query_row("SELECT count(*) FROM nb_note_tag WHERE note_id=?1",[b],|r|r.get(0)).unwrap();assert_eq!(left,0);
}
#[test]fn tag_table_added_to_old_database_without_losing_notes(){
    let c=Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE notebook(id INTEGER PRIMARY KEY AUTOINCREMENT,name TEXT NOT NULL,description TEXT NOT NULL DEFAULT '',created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL);
CREATE TABLE nb_note(id INTEGER PRIMARY KEY AUTOINCREMENT,notebook_id INTEGER NOT NULL REFERENCES notebook(id) ON DELETE CASCADE,title TEXT NOT NULL,body TEXT NOT NULL,updated_at INTEGER NOT NULL);
INSERT INTO notebook(name,created_at,updated_at) VALUES('옛',1,1);INSERT INTO nb_note(notebook_id,title,body,updated_at) VALUES(1,'옛 노트','본문',1);").unwrap();
    init_schema(&c).unwrap();init_schema(&c).unwrap();
    assert_eq!(notes(&c,1,None).unwrap().len(),1);assert!(note_tags(&c,1).unwrap().is_empty());
    set_note_tags(&c,1,&["옛것"]).unwrap();assert_eq!(notes(&c,1,Some("옛것")).unwrap().len(),1);
}
#[test]fn source_status_transitions_are_explicit(){
    use SourceStatus::*;
    for(a,b) in [(Pending,Ready),(Ready,Stale),(Stale,Refreshing),(Refreshing,Ready),(Refreshing,Failed),(Failed,Refreshing),(Ready,Missing),(Missing,Stale),(Missing,Ready),(Stale,Ready)]{assert!(a.can_go(b),"{a:?}->{b:?}")}
    for(a,b) in [(Pending,Stale),(Refreshing,Stale),(Refreshing,Missing),(Ready,Failed),(Missing,Failed),(Stale,Pending)]{assert!(!a.can_go(b),"{a:?}->{b:?}")}
    for s in [Pending,Ready,Stale,Refreshing,Failed,Missing]{assert_eq!(SourceStatus::parse(s.as_str()),Some(s))}
    assert!(Stale.wants_refresh()&&Failed.wants_refresh()&&!Ready.wants_refresh()&&!Missing.wants_refresh());
    let c=db();let n=create(&c,"노트","").unwrap();let s=add_source(&c,n,"txt","a.txt","a").unwrap();
    assert!(set_status(&c,s,Stale,"").is_err(),"pending에서 stale로 갈 수 없다");
    replace_paragraphs(&c,s,&[p(s,0,0,"text","하나")]).unwrap();
    assert!(set_status(&c,s,Failed,"x").is_err());
    set_status(&c,s,Stale,"").unwrap();assert_eq!(source_by_id(&c,s).unwrap().unwrap().status,"stale");
}
#[test]fn refresh_keeps_source_id_and_relinks_citations(){
    let c=db();let n=create(&c,"노트","").unwrap();let s=add_source(&c,n,"txt","a.txt","a").unwrap();
    replace_paragraphs(&c,s,&[p(s,0,0,"text","머리말"),p(s,1,0,"text","복지 예산은 120억 원이다."),p(s,2,0,"text","도로 정비 계획")]).unwrap();
    let note=save_note_with_citations(&c,None,n,"메모","본문",&[(s,1,"복지 예산은 120억 원이다.".into()),(s,2,"도로 정비".into()),(s,0,"사라질 문장".into())],None).unwrap();
    // 앞에 문단이 하나 끼어 번호가 밀린 새 원본.
    refresh_begin(&c,s,10,20).unwrap();assert_eq!(source_by_id(&c,s).unwrap().unwrap().status,"refreshing");
    assert_eq!(refresh_append(&c,s,&[p(s,0,0,"text","새 머리말"),p(s,0,0,"text","추가 문단")]).unwrap(),2);
    assert_eq!(refresh_append(&c,s,&[p(s,0,0,"text","복지   예산은 120억 원이다. 덧붙임"),p(s,0,0,"text","도로 정비 계획")]).unwrap(),4);
    // 준비 중에는 옛 문단이 그대로다.
    assert_eq!(paragraphs(&c,s).unwrap().len(),3);
    let r=refresh_commit(&c,s).unwrap();
    assert_eq!(r,RefreshReport{paragraphs:4,citations:3,kept:0,relinked:2,unresolved:1});
    let src=source_by_id(&c,s).unwrap().unwrap();assert_eq!((src.id,src.status.as_str(),src.paragraph_count,src.original_size,src.original_mtime),(s,"ready",4,10,20));
    let cites=citations(&c,note).unwrap();assert_eq!(cites.iter().map(|x|(x.1,x.3.as_str())).collect::<Vec<_>>(),vec![(2,"복지 예산은 120억 원이다."),(3,"도로 정비"),(0,"사라질 문장")]);
    assert!(export_md(&c,n,note).unwrap().unwrap().contains("> 복지 예산은 120억 원이다."));
    // 준비 없이 덧붙이기·확정은 거절, 빈 준비로 확정하면 옛 문단 유지 후 실패 상태.
    assert!(refresh_append(&c,s,&[p(s,0,0,"text","x")]).is_err());assert!(refresh_commit(&c,s).is_err());
    refresh_begin(&c,s,1,1).unwrap();assert!(refresh_commit(&c,s).is_err());refresh_fail(&c,s,"본문 없음").unwrap();
    let src=source_by_id(&c,s).unwrap().unwrap();assert_eq!((src.status.as_str(),src.reason.as_str(),src.paragraph_count),("failed","본문 없음",4));
    assert_eq!(paragraphs(&c,s).unwrap().len(),4);
    let staged:i64=c.query_row("SELECT count(*) FROM nb_refresh",[],|r|r.get(0)).unwrap();assert_eq!(staged,0);
}
#[test]fn check_marks_stale_missing_and_baselines_unknown(){
    let dir=std::env::temp_dir().join(format!("nb-check-{}",std::process::id()));let _=std::fs::remove_dir_all(&dir);std::fs::create_dir_all(&dir).unwrap();
    let (a,b,u)=(dir.join("a.txt"),dir.join("b.txt"),dir.join("u.txt"));for f in [&a,&b,&u]{std::fs::write(f,"하나").unwrap();}
    let c=db();let n=create(&c,"노트","").unwrap();
    let mk=|path:&Path|{let s=add_source(&c,n,"txt",path.to_str().unwrap(),"x").unwrap();replace_paragraphs(&c,s,&[p(s,0,0,"text","하나")]).unwrap();s};
    let (sa,sb,su)=(mk(&a),mk(&b),mk(&u));
    for (s,f) in [(sa,&a),(sb,&b)]{let (size,mtime)=file_signature(f).unwrap();c.execute("UPDATE nb_source SET original_size=?1,original_mtime=?2 WHERE id=?3",params![size,mtime,s]).unwrap();}
    std::fs::write(&a,"하나 둘 셋").unwrap();std::fs::remove_file(&b).unwrap();
    let rows=check_sources(&c,Some(n),|s,_|s.origin.clone()).unwrap();
    assert_eq!(rows.iter().map(|r|(r.source_id,r.status)).collect::<Vec<_>>(),vec![(sa,SourceStatus::Stale),(sb,SourceStatus::Missing),(su,SourceStatus::Ready)]);
    assert_eq!(source_by_id(&c,su).unwrap().unwrap().original_size,"하나".len() as i64,"크기·시각을 모르던 출처는 기준만 잡는다");
    std::fs::write(&b,"하나").unwrap();c.execute("UPDATE nb_source SET original_mtime=1 WHERE id=?1",[sb]).unwrap();
    let rows=check_sources(&c,None,|s,_|s.origin.clone()).unwrap();
    assert_eq!(rows[1].status,SourceStatus::Stale,"다시 보인 원본은 크기·시각이 다르면 stale");
    let _=std::fs::remove_dir_all(dir);
}
#[test]fn compare_groups_each_source_side_by_side(){
    let dir=std::env::temp_dir().join(format!("nb-compare-{}",std::process::id()));let _=std::fs::remove_dir_all(&dir);let ix=NotebookIndex::open(&dir).unwrap();
    let src=|id:i64,name:&str|Source{id,notebook_id:1,kind:"txt".into(),origin:format!("C:/자료/{name}.txt"),display_name:name.into(),status:"ready".into(),reason:"".into(),paragraph_count:0,original_size:0,original_mtime:0};
    let (s1,s2,s3)=(src(1,"가"),src(2,"나"),src(3,"다"));
    ix.replace_source(1,&s1,&[p(1,0,0,"text","복지 예산 늘림"),p(1,1,0,"text","복지 예산 복지 예산 많이"),p(1,2,0,"text","복지 예산 셋째"),p(1,3,0,"text","도로")]).unwrap();
    ix.replace_source(1,&s2,&[p(2,0,0,"text","도로 정비"),p(2,1,0,"text","복지 한 번")]).unwrap();
    ix.replace_source(1,&s3,&[p(3,0,0,"text","무관한 문단")]).unwrap();
    let sources=vec![s1,s2,s3];
    let groups=compare_sources(&sources,None,2,|id,limit|ix.search(1,"복지 예산",Some(&[id]),limit)).unwrap();
    assert_eq!(groups.iter().map(|g|g.source.id).collect::<Vec<_>>(),vec![1,2,3]);
    assert_eq!(groups[0].hits.len(),2);assert_eq!(groups[0].hits[0].ordinal,1);assert!(groups[0].hits[0].score>=groups[0].hits[1].score);
    assert_eq!(groups[1].hits.iter().map(|h|h.ordinal).collect::<Vec<_>>(),vec![1]);
    assert!(groups[2].hits.is_empty());
    let scoped=compare_sources(&sources,Some(&[2,3]),5,|id,limit|ix.search(1,"복지 예산",Some(&[id]),limit)).unwrap();
    assert_eq!(scoped.iter().map(|g|g.source.id).collect::<Vec<_>>(),vec![2,3]);
    let md=compare_md(" 복지 예산 ",&groups);
    assert!(md.starts_with("## 출처 비교: 복지 예산\n"),"{md}");
    assert!(md.contains("### 가 (C:/자료/가.txt)"));assert!(md.contains("> 복지 예산 복지 예산 많이\n> — 문단 1"));
    assert!(md.contains("### 다 (C:/자료/다.txt)\n\n근거 문단 없음"));
    assert!(md.find("### 가").unwrap()<md.find("### 나").unwrap());
    let _=std::fs::remove_dir_all(dir);
}
}
