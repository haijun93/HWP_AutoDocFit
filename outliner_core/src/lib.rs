use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io::Write, path::Path};

pub const MAX_DOCUMENT_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_BLOCKS: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutlinerError {
    DocumentTooLarge,
    TooManyBlocks,
    QueryTooShort,
    Invalid(String),
    Io(String),
}
impl OutlinerError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::DocumentTooLarge => "DOCUMENT_TOO_LARGE",
            Self::TooManyBlocks => "TOO_MANY_BLOCKS",
            Self::QueryTooShort => "QUERY_TOO_SHORT",
            Self::Invalid(_) => "INVALID_OPERATION",
            Self::Io(_) => "IO_ERROR",
        }
    }
}
impl std::fmt::Display for OutlinerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DocumentTooLarge => write!(f, "문서가 10 MiB 한도를 초과했습니다"),
            Self::TooManyBlocks => write!(f, "블록이 100000개 한도를 초과했습니다"),
            Self::QueryTooShort => write!(f, "검색어는 두 글자 이상이어야 합니다"),
            Self::Invalid(s) | Self::Io(s) => write!(f, "{s}"),
        }
    }
}
impl From<std::io::Error> for OutlinerError { fn from(e: std::io::Error) -> Self { Self::Io(e.to_string()) } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Block {
    pub id: u64,
    pub text: String,
    pub indent: usize,
    pub folded: bool,
    pub status: Option<String>,
    pub tags: Vec<String>,
    pub properties: BTreeMap<String, String>,
    pub page_links: Vec<String>,
    pub block_refs: Vec<String>,
    pub line_index: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Document {
    pub original: String,
    pub blocks: Vec<Block>,
    pub edited: bool,
    pub sequence: Vec<LineNode>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LineNode { Block(u64), Raw(String) }
fn newline_style(s: &str) -> &'static str { if s.contains("\r\n") { "\r\n" } else { "\n" } }
fn indent_width(prefix: &str) -> usize { prefix.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum() }
fn parse_block_line(line: &str, index: usize, id: u64) -> Option<Block> {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let indent = indent_width(&line[..line.len() - trimmed.len()]);
    let body = trimmed.strip_prefix("- ")?;
    let mut text = body.to_owned();
    let status = if let Some(v) = text.strip_prefix("TODO ") { text = v.into(); Some("TODO".into()) }
        else if let Some(v) = text.strip_prefix("DONE ") { text = v.into(); Some("DONE".into()) }
        else { None };
    let mut tags = Vec::new(); let mut properties = BTreeMap::new();
    let mut page_links = Vec::new(); let mut block_refs = Vec::new();
    let chars: Vec<char> = text.chars().collect(); let mut i=0;
    while i < chars.len() {
        if chars[i]=='#' { let start=i+1; let mut j=start; while j<chars.len() && (chars[j].is_alphanumeric() || chars[j]=='_' || chars[j]=='-') { j+=1; } if j>start { tags.push(chars[start..j].iter().collect()); i=j; continue; } }
        if i+1<chars.len() && chars[i]=='[' && chars[i+1]=='[' { let start=i+2; let mut j=start; while j+1<chars.len() && !(chars[j]==']' && chars[j+1]==']') { j+=1; } if j+1<chars.len() { page_links.push(chars[start..j].iter().collect()); i=j+2; continue; } }
        if i+1<chars.len() && chars[i]=='(' && chars[i+1]=='(' { let start=i+2; let mut j=start; while j+1<chars.len() && !(chars[j]==')' && chars[j+1]==')') { j+=1; } if j+1<chars.len() { block_refs.push(chars[start..j].iter().collect()); i=j+2; continue; } }
        i+=1;
    }
    for part in text.split_whitespace() { if let Some((k,v))=part.split_once("::") { if !k.is_empty() { properties.insert(k.into(),v.into()); } } }
    Some(Block { id, text, indent, folded:false, status, tags, properties, page_links, block_refs, line_index:index })
}
pub fn parse(input: &str) -> Result<Document, OutlinerError> {
    if input.len()>MAX_DOCUMENT_BYTES { return Err(OutlinerError::DocumentTooLarge); }
    let mut blocks=Vec::new(); let mut sequence=Vec::new();
    for (i,line) in input.split_terminator('\n').enumerate() { let line=line.strip_suffix('\r').unwrap_or(line); if let Some(b)=parse_block_line(line,i,blocks.len() as u64+1) { sequence.push(LineNode::Block(b.id)); blocks.push(b); if blocks.len()>MAX_BLOCKS { return Err(OutlinerError::TooManyBlocks); } } else { sequence.push(LineNode::Raw(line.into())); } }
    Ok(Document { original:input.into(), blocks, edited:false, sequence })
}
fn format_block(b:&Block)->String { let mut s=String::new(); s.push_str(&" ".repeat(b.indent)); s.push_str("- "); if let Some(st)=&b.status { s.push_str(st); s.push(' '); } s.push_str(&b.text); s }
pub fn serialize(doc:&Document)->String {
    if !doc.edited { return doc.original.clone(); }
    let nl=newline_style(&doc.original); let mut lines=Vec::new();
    for node in &doc.sequence { match node { LineNode::Raw(s)=>lines.push(s.clone()), LineNode::Block(id)=>if let Some(b)=doc.blocks.iter().find(|b|b.id==*id){lines.push(format_block(b));} } }
    lines.join(nl)
}
pub fn backlinks(doc:&Document, target:&str)->Vec<u64> { doc.blocks.iter().filter(|b|b.page_links.iter().any(|v|v==target)||b.block_refs.iter().any(|v|v==target)).map(|b|b.id).collect() }
pub fn search(doc:&Document, query:&str)->Result<Vec<u64>,OutlinerError>{ if query.chars().count()<2{return Err(OutlinerError::QueryTooShort)} let q=query.to_lowercase(); Ok(doc.blocks.iter().filter(|b|b.text.to_lowercase().contains(&q)).map(|b|b.id).collect()) }
fn block_index(doc:&Document,id:u64)->Result<usize,OutlinerError>{ doc.blocks.iter().position(|b|b.id==id).ok_or_else(||OutlinerError::Invalid(format!("블록을 찾을 수 없습니다: {id}"))) }
pub fn edit(doc:&mut Document, op:&str, id:u64, text:Option<&str>)->Result<(),OutlinerError>{ let i=block_index(doc,id)?; let seq_i=doc.sequence.iter().position(|n|*n==LineNode::Block(id)).ok_or_else(||OutlinerError::Invalid("블록 순서를 찾을 수 없습니다".into()))?; match op {
    "insert"=>{let value=text.ok_or_else(||OutlinerError::Invalid("text가 필요합니다".into()))?; let mut b=parse_block_line(&format!("{}- {value}"," ".repeat(doc.blocks[i].indent)),i,id+1).unwrap(); b.id=doc.blocks.iter().map(|x|x.id).max().unwrap_or(0)+1; let new_id=b.id; doc.blocks.insert(i+1,b); doc.sequence.insert(seq_i+1,LineNode::Block(new_id));},
    "delete"=>{doc.blocks.remove(i);doc.sequence.remove(seq_i);},
    "indent"=>{if i==0{return Err(OutlinerError::Invalid("첫 블록은 들여쓸 수 없습니다".into()))} doc.blocks[i].indent+=1;},
    "outdent"=>{if doc.blocks[i].indent==0{return Err(OutlinerError::Invalid("이미 최상위 블록입니다".into()))} doc.blocks[i].indent-=1;},
    "fold"=>doc.blocks[i].folded=true,
    "unfold"=>doc.blocks[i].folded=false,
    "up"=>{let prev=seq_i.checked_sub(1).ok_or_else(||OutlinerError::Invalid("위로 이동할 블록이 없습니다".into()))?; if !matches!(doc.sequence[prev],LineNode::Block(_)){return Err(OutlinerError::Invalid("보존 줄을 건너 이동할 수 없습니다".into()))} doc.sequence.swap(seq_i,prev);},
    "down"=>{if seq_i+1>=doc.sequence.len()||!matches!(doc.sequence[seq_i+1],LineNode::Block(_)){return Err(OutlinerError::Invalid("아래로 이동할 블록이 없습니다".into()))}doc.sequence.swap(seq_i,seq_i+1);},
    "split"=>{let value=text.ok_or_else(||OutlinerError::Invalid("분할 위치 text가 필요합니다".into()))?; let (left,right)={let source=&doc.blocks[i].text;let (l,r)=source.split_once(value).ok_or_else(||OutlinerError::Invalid("분할 문자열이 없습니다".into()))?;(l.to_owned(),r.to_owned())}; doc.blocks[i].text=left; let mut b=doc.blocks[i].clone(); b.id=doc.blocks.iter().map(|x|x.id).max().unwrap_or(0)+1; b.text=right; let new_id=b.id; doc.blocks.insert(i+1,b); doc.sequence.insert(seq_i+1,LineNode::Block(new_id));},
    "merge"=>{if seq_i+1>=doc.sequence.len()||!matches!(doc.sequence[seq_i+1],LineNode::Block(_)){return Err(OutlinerError::Invalid("병합할 다음 블록이 없습니다".into()))}let next_id=match doc.sequence[seq_i+1]{LineNode::Block(x)=>x,_=>unreachable!()};let j=block_index(doc,next_id)?;let next=doc.blocks.remove(j);doc.blocks[i].text.push_str(&next.text);doc.sequence.remove(seq_i+1);},
    "set_status"=>{let s=text.unwrap_or(""); if !["","TODO","DONE"].contains(&s){return Err(OutlinerError::Invalid("status는 TODO, DONE 또는 빈 문자열이어야 합니다".into()))} doc.blocks[i].status=if s.is_empty(){None}else{Some(s.into())};},
    _=>return Err(OutlinerError::Invalid(format!("지원하지 않는 작업: {op}"))),
} for (n,b) in doc.blocks.iter_mut().enumerate(){ b.line_index=n; } doc.edited=true; Ok(()) }
pub fn daily_page(date:&str)->Result<String,OutlinerError>{ if date.len()!=10||date.as_bytes()[4]!=b'-'||date.as_bytes()[7]!=b'-'||!date.chars().enumerate().all(|(i,c)|i==4||i==7||c.is_ascii_digit()){return Err(OutlinerError::Invalid("날짜는 YYYY-MM-DD여야 합니다".into()))} Ok(format!("{date}.md")) }
pub fn save_atomic(path:&Path, content:&str)->Result<(),OutlinerError>{ if content.len()>MAX_DOCUMENT_BYTES{return Err(OutlinerError::DocumentTooLarge)} let parent=path.parent().filter(|p|!p.as_os_str().is_empty()).unwrap_or(Path::new(".")); let name=path.file_name().and_then(|s|s.to_str()).unwrap_or("outliner.md"); let tmp=parent.join(format!(".{name}.outliner-tmp")); { let mut f=fs::File::create(&tmp)?; f.write_all(content.as_bytes())?; f.sync_all()?; } if let Err(e)=fs::rename(&tmp,path){let _=fs::remove_file(&tmp);return Err(OutlinerError::Io(e.to_string()))} Ok(()) }

#[cfg(test)] mod tests {
 use super::*;
 #[test] fn round_trip_preserves_empty_korean_deep_indent_and_unknown_lines(){ for s in ["","한글\n- 항목\n    - 깊은 항목\n일반 문단\n","\t- 탭 자식\n  - 공백 혼합\n```\n- 코드\n```\n"] {let d=parse(s).unwrap();assert_eq!(serialize(&d),s);} }
 #[test] fn extracts_links_tags_properties_backlinks_and_search(){let d=parse("- [[페이지]] #태그 kind:: task ((42))\n- unrelated\n").unwrap();let b=&d.blocks[0];assert_eq!(b.page_links,["페이지"]);assert_eq!(b.tags,["태그"]);assert!(b.properties.contains_key("kind"));assert_eq!(backlinks(&d,"페이지"),[1]);assert_eq!(backlinks(&d,"42"),[1]);assert_eq!(search(&d,"관계"),Ok(vec![]));assert_eq!(search(&d,"태"),Err(OutlinerError::QueryTooShort));}
 #[test] fn edit_operations_cover_insert_delete_indent_fold_move_split_merge(){let mut d=parse("- one\n- two\n- three\n").unwrap();edit(&mut d,"insert",1,Some("new")).unwrap();edit(&mut d,"indent",2,None).unwrap();edit(&mut d,"fold",2,None).unwrap();assert!(d.blocks.iter().find(|b|b.id==2).unwrap().folded);edit(&mut d,"up",3,None).unwrap();let split_id=d.blocks.iter().find(|b|b.text=="one").unwrap().id;edit(&mut d,"split",split_id,Some("n")).unwrap();edit(&mut d,"merge",split_id,None).unwrap();let tail=d.blocks.last().unwrap().id;edit(&mut d,"delete",tail,None).unwrap();assert!(!d.blocks.is_empty());assert!(d.edited);}
 #[test] fn rejects_limits_and_malformed_operations(){let too_big="x".repeat(MAX_DOCUMENT_BYTES+1);assert_eq!(parse(&too_big),Err(OutlinerError::DocumentTooLarge));let d=parse("- x\n").unwrap();assert_eq!(search(&d,"x"),Err(OutlinerError::QueryTooShort));assert!(daily_page("yesterday").is_err());let mut d=parse("- x\n").unwrap();assert!(edit(&mut d,"delete",999,None).is_err());}
}
