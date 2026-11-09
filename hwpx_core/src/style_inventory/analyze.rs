use serde_json::{json, Value};
use std::{fs::File, io::Read, path::Path};
use zip::ZipArchive;

fn tree(node: &Value) -> Value { node.clone() }

fn normalize_tree(node: &Value) -> Value {
    let mut out=tree(node);
    if let Some(obj)=out.as_object_mut(){
        if obj.get("text")==Some(&Value::Null){obj.remove("text");}
        if obj.get("tail")==Some(&Value::Null){obj.remove("tail");}
        if let Some(children)=obj.get_mut("children").and_then(Value::as_array_mut){for child in children.iter_mut(){*child=normalize_tree(child);}}
    }
    out
}
fn xml_tag(name: &str, namespace: &str) -> String { format!("{{{namespace}}}{}", name.split(':').last().unwrap_or(name)) }
fn ns_uri(prefix: &str) -> &'static str {
    match prefix {
        "ha" => "http://www.hancom.co.kr/hwpml/2011/app", "hc" => "http://www.hancom.co.kr/hwpml/2011/core",
        "hh" => "http://www.hancom.co.kr/hwpml/2011/head", "hhs" => "http://www.hancom.co.kr/hwpml/2011/history",
        "hm" => "http://www.hancom.co.kr/hwpml/2011/master-page", "hp" => "http://www.hancom.co.kr/hwpml/2011/paragraph",
        "hpf" => "http://www.hancom.co.kr/schema/2011/hpf", "hs" => "http://www.hancom.co.kr/hwpml/2011/section",
        "opf" => "http://www.idpf.org/2007/opf/", "dc" => "http://purl.org/dc/elements/1.1/",
        "epub" => "http://www.idpf.org/2007/ops", "config" => "urn:oasis:names:tc:opendocument:xmlns:config:1.0",
        _ => "http://www.hancom.co.kr/hwpml/2011/head",
    }
}

fn parse(bytes: &[u8]) -> Result<Value, String> {
    let mut reader = quick_xml::Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut stack: Vec<Value> = Vec::new();
    let mut root = Value::Null;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) => {
                let qname=e.name(); let prefix=qname.as_ref().split(':').next().unwrap_or(""); let ns=ns_uri(prefix);
                let tag = xml_tag(qname.as_ref(), ns);
                let mut attrib = serde_json::Map::new();
                for a in e.attributes().with_checks(false).flatten() {
                    let raw_key=a.key.as_ref();
                    if raw_key == "xmlns" || raw_key.starts_with("xmlns:") { continue; }
                    let key = raw_key.to_string();
                    let val = quick_xml::escape::unescape(a.value.as_ref()).map_err(|err| err.to_string())?.into_owned();
                    attrib.insert(key, Value::String(val));
                }
                let node=json!({"tag":tag,"attrib":attrib,"children":[]});
                if stack.is_empty(){root=node.clone();stack.push(node)}else{stack.push(node)}
            }
            Ok(quick_xml::events::Event::Empty(e)) => {
                let qname=e.name(); let prefix=qname.as_ref().split(':').next().unwrap_or(""); let ns=ns_uri(prefix);
                let tag = xml_tag(qname.as_ref(), ns);
                let mut attrib = serde_json::Map::new();
                for a in e.attributes().with_checks(false).flatten() {
                    let raw_key=a.key.as_ref();
                    if raw_key == "xmlns" || raw_key.starts_with("xmlns:") { continue; }
                    let key = raw_key.to_string();
                    let val = quick_xml::escape::unescape(a.value.as_ref()).map_err(|err| err.to_string())?.into_owned();
                    attrib.insert(key, Value::String(val));
                }
                let node=json!({"tag":tag,"attrib":attrib,"children":[]});
                if let Some(parent)=stack.last_mut(){parent["children"].as_array_mut().unwrap().push(node)}else{root=node}
            }
            Ok(quick_xml::events::Event::End(_)) => if stack.len()>1 {let node=stack.pop().unwrap();let mut node=normalize_tree(&node);if let Some(text)=node.get("text").and_then(Value::as_str){if let Some(parent)=stack.last_mut(){if let Some(children)=parent.get_mut("children").and_then(Value::as_array_mut){if let Some(prev)=children.last_mut(){prev["tail"]=Value::String(text.to_string());}}}}stack.last_mut().unwrap()["children"].as_array_mut().unwrap().push(node)},
            Ok(quick_xml::events::Event::Text(e)) => {
                let text=quick_xml::escape::unescape(e.as_ref()).map_err(|err|err.to_string())?.into_owned();
                if !text.trim().is_empty() { if let Some(node)=stack.last_mut(){node["text"]=Value::String(text);} }
            }
            Ok(quick_xml::events::Event::DocType(_)) => return Err("DOCTYPE forbidden".into()),
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => return Err(e.to_string()), _ => {}
        }
        buf.clear();
    }
    Ok(stack.into_iter().next().unwrap_or(root))
}

fn read_member(z: &mut ZipArchive<File>, name: &str) -> Result<Value,String> {
    let mut f=z.by_name(name).map_err(|e|e.to_string())?;let mut bytes=Vec::new();f.read_to_end(&mut bytes).map_err(|e|e.to_string())?;parse(&bytes)
}

pub fn analyze(path: &Path, stage: &str) -> Result<Value,String> {
    let file=File::open(path).map_err(|e|e.to_string())?;let mut z=ZipArchive::new(file).map_err(|e|e.to_string())?;
    let raw_header=read_member(&mut z,"Contents/header.xml")?;
    let header=with_python_order(&raw_header);
    let defs=crate::style_inventory::parse_header::parse_header(&header);
    if stage=="header" { return Ok(json!({"header":header,"definitions":defs})); }
    let mut names=(0..z.len()).filter_map(|i|z.by_index(i).ok().and_then(|f|f.name().ok().map(|n|n.into_owned())))
        .filter(|n|n.starts_with("Contents/section")&&n.ends_with(".xml")).collect::<Vec<_>>();
    names.sort_by_key(|n|n.trim_end_matches(".xml").trim_start_matches("Contents/section").parse::<u32>().unwrap_or(u32::MAX));
    let mut sections=Vec::new();
    for name in names { let root=read_member(&mut z,&name)?;let mut paras=crate::style_inventory::walk::walk(&root,"본문");if let Some(items)=paras.as_array_mut(){for item in items{item[0]=normalize_tree(&item[0]);}}sections.push(json!({"name":name,"paragraphs":paras})); }
    if stage=="body_walk" { return Ok(json!({"sections":sections})); }
    Err(format!("analyze stage not implemented: {stage}"))
}

fn with_python_order(node: &Value) -> Value {
    if let Some(obj)=node.as_object() {
        let mut out=serde_json::Map::new();
        for key in ["tag","text","tail","attrib","children"] { out.insert(key.into(),obj.get(key).cloned().unwrap_or(Value::Null)); }
        if let Some(attrib)=obj.get("attrib").and_then(Value::as_object) {
            let ordered=["version","secCnt","page","footnote","endnote","pic","tbl","equation","id","lang","face","type","name","engName","paraPrIDRef","charPrIDRef","nextStyleIDRef"];
            let mut attrs=serde_json::Map::new();
            for key in ordered { if let Some(value)=attrib.get(key){attrs.insert(key.into(),value.clone());} }
            for (key,value) in attrib { if !attrs.contains_key(key){attrs.insert(key.clone(),value.clone());} }
            out.insert("attrib".into(),Value::Object(attrs));
        }
        if let Some(children)=obj.get("children").and_then(Value::as_array){out.insert("children".into(),Value::Array(children.iter().map(with_python_order).collect()));}
        Value::Object(out)
    } else { node.clone() }
}

#[cfg(test)] mod tests { use super::*; #[test] fn parses_empty_element() { let v=parse(b"<x><y/></x>").unwrap(); assert_eq!(v["children"][0]["tag"],"y"); } }
