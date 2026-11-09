use serde_json::{json, Value};
use std::{fs::File, io::Read, path::Path};
use zip::ZipArchive;

fn tree(node: &Value) -> Value {
    if let Some(obj)=node.as_object() {
        let mut out=serde_json::Map::new();
        for key in ["tag","text","tail","attrib","children"] { out.insert(key.into(),obj.get(key).cloned().unwrap_or(Value::Null)); }
        Value::Object(out)
    } else { node.clone() }
}
fn xml_tag(name: &str) -> String { name.to_string() }

fn parse(bytes: &[u8]) -> Result<Value, String> {
    let mut reader = quick_xml::Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut stack: Vec<Value> = Vec::new();
    let mut root = Value::Null;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) => {
                let tag = format!("{{http://www.hancom.co.kr/hwpml/2011/head}}{}", e.name().as_ref().split(':').last().unwrap_or(e.name().as_ref()));
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
                let tag = format!("{{http://www.hancom.co.kr/hwpml/2011/head}}{}", e.name().as_ref().split(':').last().unwrap_or(e.name().as_ref()));
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
            Ok(quick_xml::events::Event::End(_)) => if stack.len()>1 {let node=stack.pop().unwrap();stack.last_mut().unwrap()["children"].as_array_mut().unwrap().push(node)},
            Ok(quick_xml::events::Event::Text(e)) => {
                let text=quick_xml::escape::unescape(e.as_ref()).map_err(|err|err.to_string())?.into_owned();
                if !text.trim().is_empty(){if let Some(node)=stack.last_mut(){node["text"]=Value::String(text)}}
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
    for name in names { let root=tree(&read_member(&mut z,&name)?);sections.push(json!({"name":name,"tree":root,"paragraphs":crate::style_inventory::walk::walk(&root,"본문")})); }
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
