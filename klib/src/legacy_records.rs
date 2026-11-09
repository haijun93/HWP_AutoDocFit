//! Direct port of the HWPX branch of `extract_paragraphs_legacy`.
use crate::json::Value;
use quick_xml::events::Event;
use std::{collections::HashMap, fs::File, io::Read, path::Path};
use zip::ZipArchive;

#[derive(Default)]
struct Node { name: String, attrs: HashMap<String,String>, content: Vec<Content> }
enum Content { Text(String), Node(Node) }
fn local(bytes: &[u8]) -> String { String::from_utf8_lossy(bytes).rsplit(':').next().unwrap_or("").to_owned() }
fn parse(xml: &[u8]) -> Result<Node,String> {
 let mut r=quick_xml::Reader::from_reader(xml); r.config_mut().trim_text(false);
 let mut stack:Vec<Node>=Vec::new(); let mut root=Node::default();
 loop { match r.read_event() {
  Ok(Event::Start(e)) => { let mut n=Node{name:local(e.name().as_ref()),..Default::default()}; for a in e.attributes().with_checks(false).flatten(){ n.attrs.insert(local(a.key.as_ref()), a.decode_and_unescape_value(r.decoder()).map_err(|e|e.to_string())?.into_owned()); } stack.push(n); }
  Ok(Event::Empty(e)) => { let mut n=Node{name:local(e.name().as_ref()),..Default::default()}; for a in e.attributes().with_checks(false).flatten(){ n.attrs.insert(local(a.key.as_ref()),a.decode_and_unescape_value(r.decoder()).map_err(|e|e.to_string())?.into_owned()); } if let Some(p)=stack.last_mut(){p.content.push(Content::Node(n))}else{root.content.push(Content::Node(n))} }
  Ok(Event::Text(e)) => {let s=e.decode().map_err(|e|e.to_string())?.into_owned(); if let Some(n)=stack.last_mut(){n.content.push(Content::Text(s));}}
  Ok(Event::CData(e)) => {let s=e.decode().map_err(|e|e.to_string())?.into_owned(); if let Some(n)=stack.last_mut(){n.content.push(Content::Text(s));}}
  Ok(Event::GeneralRef(e)) => {let s=match e.resolve_char_ref(){Ok(Some(c))=>c.to_string(),_=>format!("&{};",e.decode().unwrap_or_default())}; if let Some(n)=stack.last_mut(){n.content.push(Content::Text(s));}}
  Ok(Event::End(_)) => {let n=stack.pop().ok_or("malformed XML")?; if let Some(p)=stack.last_mut(){p.content.push(Content::Node(n))}else{root.content.push(Content::Node(n))}}
  Ok(Event::Eof)=>break, Ok(_)=>{}, Err(e)=>return Err(e.to_string())
 }} Ok(root)
}
fn tag(n:&Node)->&str{&n.name}
fn children(n:&Node)->impl Iterator<Item=&Node>{n.content.iter().filter_map(|c|if let Content::Node(n)=c{Some(n)}else{None})}
fn collect(n:&Node, root:bool, out:&mut String){if !root && matches!(tag(n),"tbl"|"table"){return} for c in &n.content {match c {Content::Text(s)=>out.push_str(s),Content::Node(ch)=>{collect(ch,false,out); for t in &ch.content {if let Content::Text(s)=t {let _=s;}}}}} }
fn gather(n:&Node, root:bool, out:&mut String){if !root&&matches!(tag(n),"tbl"|"table"){return} for c in &n.content{match c{Content::Text(s)=>out.push_str(s),Content::Node(ch)=>{gather(ch,false,out);}}}}
fn walk(n:&Node, table:bool, styles:&HashMap<String,String>, out:&mut Vec<Value>){let t=tag(n); if matches!(t,"tc"|"cell"){for c in children(n){walk(c,true,styles,out)}return} if t=="p"{let mut s=String::new();gather(n,true,&mut s);let text=s.trim().to_owned();if !text.is_empty(){let mut style=n.attrs.get("styleIDRef").or(n.attrs.get("styleID")).or(n.attrs.get("style")).cloned();let mut outline=n.attrs.get("outlineLevel").or(n.attrs.get("outline_level")).cloned();for c in children(n){if matches!(tag(c),"pPr"|"p_pr"){style=style.or_else(||c.attrs.get("styleIDRef").or(c.attrs.get("styleID")).cloned());outline=outline.or_else(||c.attrs.get("outlineLevel").or(c.attrs.get("outline_level")).cloned());}} let level_rank=crate::json::Value::from(0i64); out.push(Value::obj(vec![("paragraph_index",(out.len() as i64).into()),("text",text.into()),("level","body".into()),("level_rank",level_rank),("is_table_cell",table.into()),("style_name",style.and_then(|x|styles.get(&x).cloned().or(Some(x))).map(Value::Str).unwrap_or(Value::Null)),("outline_level",outline.and_then(|x|x.parse::<i64>().ok()).map(Value::from).unwrap_or(Value::Null))]));} for c in children(n){walk(c,table,styles,out)}return} for c in children(n){walk(c,table,styles,out)} }
pub fn run(path:&Path)->Result<Vec<Value>,String>{let f=File::open(path).map_err(|e|e.to_string())?;let mut z=ZipArchive::new(f).map_err(|e|e.to_string())?;let mut names:Vec<String>=(0..z.len()).filter_map(|i|z.by_index(i).ok().and_then(|x|x.name().ok().map(|n|n.into_owned()))).collect();names.sort_by_key(|n|{let tail=n.split("section").nth(1).unwrap_or("");tail.chars().take_while(char::is_ascii_digit).collect::<String>().parse::<usize>().unwrap_or(0)});let mut styles=HashMap::new();if names.iter().any(|n|n=="Contents/header.xml"){let mut b=Vec::new();z.by_name("Contents/header.xml").map_err(|e|e.to_string())?.read_to_end(&mut b).map_err(|e|e.to_string())?;let h=parse(&b)?;fn scan(n:&Node,s:&mut HashMap<String,String>){if tag(n)=="style"{if let(Some(i),Some(v))=(n.attrs.get("id"),n.attrs.get("name")){s.insert(i.clone(),v.clone());}}for c in children(n){scan(c,s)}}scan(&h,&mut styles)}let mut out=Vec::new();for name in names.into_iter().filter(|n|n.starts_with("Contents/section")&&n.ends_with(".xml")){let mut b=Vec::new();z.by_name(&name).map_err(|e|e.to_string())?.read_to_end(&mut b).map_err(|e|e.to_string())?;walk(&parse(&b)?,false,&styles,&mut out)}Ok(out)}
