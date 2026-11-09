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
            Ok(quick_xml::events::Event::End(_)) => if stack.len()>1 {
                let node=stack.pop().unwrap();
                stack.last_mut().unwrap()["children"].as_array_mut().unwrap().push(node);
            },
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
    for name in names { let root=read_member(&mut z,&name)?;let normalized=normalize_tree(&root);let paras=crate::style_inventory::walk::walk(&normalized,"본문");sections.push(json!({"name":name,"paragraphs":paras,"root":normalized})); }
    if stage=="body_walk" { return Ok(json!({"sections":sections})); }
    if stage=="aggregate" { return Ok(aggregate(&sections,&defs)); }
    Err(format!("analyze stage not implemented: {stage}"))
}

fn aggregate(sections: &[Value], defs: &Value) -> Value {
    use std::collections::{BTreeMap, BTreeSet};
    let chars = defs.get("chars").cloned().unwrap_or(json!({}));
    let paras = defs.get("paras").cloned().unwrap_or(json!({}));
    let mut char_use: BTreeMap<String, Value> = BTreeMap::new();
    let mut para_use: BTreeMap<String, Value> = BTreeMap::new();
    let mut style_use: BTreeMap<String, usize> = BTreeMap::new();
    let mut borders: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut tables: Vec<Value> = Vec::new();
    let mut combos: Vec<Value> = Vec::new();
    let mut types: Vec<Value> = Vec::new();
    let mut effects: BTreeMap<String, Value> = BTreeMap::new();
    let mut shades: BTreeMap<String, Value> = BTreeMap::new();
    let mut page = Value::Null;
    let mut total = 0usize;
    for section in sections {
        let root = &section["root"];
        if page.is_null() {
            if let Some(pp) = find_local(root, "pagePr") {
                let margin = find_local(pp, "margin");
                let mut margins = serde_json::Map::new();
                for k in ["left","right","top","bottom","header","footer","gutter"] {
                    margins.insert(k.into(), margin.and_then(|m| find_local(m,k)).map(|x| crate::style_inventory::mm::mm(attr(x,"value"))).unwrap_or(0.0).into());
                }
                page=json!({"width_mm":crate::style_inventory::mm::mm(attr(pp,"width")),"height_mm":crate::style_inventory::mm::mm(attr(pp,"height")),"orientation":if attr(pp,"landscape")==Some("NARROWLY"){"가로"}else{"세로"},"margin_mm":if margin.is_some(){Value::Object(margins)}else{json!({})}});
            }
        }
        if let Some(items)=section["paragraphs"].as_array(){ for item in items {
            total+=1; let p=&item[0]; let ctx=item[1].as_str().unwrap_or("본문");
            let pid=attr(p,"paraPrIDRef").map(str::to_owned); let sid=attr(p,"styleIDRef").map(str::to_owned); let text=para_text(p);
            let pk=pid.clone().unwrap_or_else(||"null".into()); let sk=sid.clone().unwrap_or_else(||"null".into());
            let pu=para_use.entry(pk.clone()).or_insert_with(||json!({"count":0,"contexts":{},"sample":""}));pu["count"]=json!(pu["count"].as_u64().unwrap_or(0)+1);pu["contexts"][ctx]=json!(pu["contexts"][ctx].as_u64().unwrap_or(0)+1);if pu["sample"].as_str()==Some("")&&!text.trim().is_empty(){pu["sample"]=json!(text.trim().chars().take(60).collect::<String>());}
            *style_use.entry(sk).or_default()+=1;
            if let Some(b)=paras.get(&pk).and_then(|x|x.get("border_fill")).and_then(Value::as_str).filter(|x|!x.is_empty()){borders.entry(b.into()).or_default().insert("문단 테두리".into());}
            let runs=direct_runs(p); let sig=crate::style_inventory::char_signature::char_signature(&json!(runs),Some(&chars)); let seq=sig[0].clone();let main=sig[1].clone();
            for run in &runs { let rt=run_text(run);let trimmed=rt.trim();let cid=attr(run,"charPrIDRef").map(str::to_owned);let ck=cid.clone().unwrap_or_else(||"null".into());
                let cu=char_use.entry(ck.clone()).or_insert_with(||json!({"chars":0,"runs":0,"contexts":{},"sample":""})); cu["runs"]=json!(cu["runs"].as_u64().unwrap_or(0)+1);cu["chars"]=json!(cu["chars"].as_u64().unwrap_or(0)+trimmed.chars().count() as u64);
                if !trimmed.is_empty(){cu["contexts"][ctx]=json!(cu["contexts"][ctx].as_u64().unwrap_or(0)+trimmed.chars().count() as u64);if cu["sample"].as_str()==Some(""){cu["sample"]=json!(trimmed.chars().take(40).collect::<String>());}}
                if let Some(ch)=chars.get(&ck){if let Some(sh)=ch.get("shade").and_then(Value::as_str).filter(|s|!s.is_empty()){if !trimmed.is_empty(){let u=shades.entry(sh.into()).or_insert_with(||json!({"runs":0,"chars":0,"char_ids":[],"contexts":{},"samples":[]}));u["runs"]=json!(u["runs"].as_u64().unwrap_or(0)+1);u["chars"]=json!(u["chars"].as_u64().unwrap_or(0)+trimmed.chars().count() as u64);if !u["char_ids"].as_array().unwrap().contains(&json!(cid)){u["char_ids"].as_array_mut().unwrap().push(json!(cid));}u["contexts"][ctx]=json!(u["contexts"][ctx].as_u64().unwrap_or(0)+1);if u["samples"].as_array().unwrap().len()<3{u["samples"].as_array_mut().unwrap().push(json!({"part":trimmed.chars().take(20).collect::<String>(),"text":text.trim().chars().take(60).collect::<String>()}));}}}}
                let mut tbls=Vec::new();if let Some(k)=run.get("children").and_then(Value::as_array){for t in k.iter().filter(|n|local(n)=="tbl"){tbls.push(t);}}
                for tbl in tbls {let sig=crate::style_inventory::table_signature::table_signature(tbl,Some(&chars));let info=crate::style_inventory::table_info::table_info(tbl,Some(&chars));if let Some(found)=tables.iter_mut().find(|t|t["_sig"]==sig){found["count"]=json!(found["count"].as_u64().unwrap_or(0)+1);}else{let mut v=info;v["count"]=json!(1);v["context"]=json!(ctx);v["_sig"]=sig;tables.push(v);}if let Some(b)=attr(tbl,"borderFillIDRef"){borders.entry(b.into()).or_default().insert("표".into());}let mut cells=Vec::new();iter_local(tbl,"tc",&mut cells);for c in cells{if let Some(b)=attr(c,"borderFillIDRef"){borders.entry(b.into()).or_default().insert("표 셀".into());}}}
            }
            if ctx=="본문"&&!text.trim().is_empty(){let (marker,role)=crate::style_inventory::paragraph_style_type::leading_marker(&text);let key=json!([marker,pid,sid,seq]);if let Some(found)=combos.iter_mut().find(|x|x["_key"]==key){found["count"]=json!(found["count"].as_u64().unwrap_or(0)+1);if found["samples"].as_array().unwrap().len()<2{found["samples"].as_array_mut().unwrap().push(json!(text.trim().chars().take(80).collect::<String>()));}}else{combos.push(json!({"_key":key,"marker":if marker.is_empty(){"(없음)"}else{&marker},"role":role,"para":pid,"style":sid,"char_seq":seq,"main_char":main,"count":1,"samples":[text.trim().chars().take(80).collect::<String>()]}));}
                if let Some(typed)=crate::style_inventory::paragraph_style_type::paragraph_style_type(p,defs){let tk=typed[0].clone();let m=&typed[1];if let Some(t)=types.iter_mut().find(|x|x["_key"]==tk){t["count"]=json!(t["count"].as_u64().unwrap_or(0)+1);t["variants"]=json!(t["variants"].as_u64().unwrap_or(0)+1);}else{types.push(json!({"_key":tk,"marker":m["marker"],"role":m["role"],"main_char":m["main_char"],"count":1,"variants":1,"spacing":{"mode":m["spacing"],"min":m["spacing"],"max":m["spacing"]},"indent_mm":{"mode":m["indent_mm"],"min":m["indent_mm"],"max":m["indent_mm"]},"line":if m["line"].is_null(){Value::Null}else{json!({"mode":m["line"],"min":m["line"],"max":m["line"]})},"prev_pt":{"mode":m["prev_pt"],"min":m["prev_pt"],"max":m["prev_pt"]},"para":m["para"],"style":m["style"],"emphasis":if m["emphasis"]==true{1}else{0},"effects":{},"samples":[m["text"].as_str().unwrap_or("").chars().take(80).collect::<String>()]}));}}
            }
        }}
    }
    for (cid,c) in chars.as_object().into_iter().flat_map(|m|m.iter()){if let Some(b)=c.get("border_fill").and_then(Value::as_str){if char_use.get(cid).map_or(false,|u|u["chars"].as_u64().unwrap_or(0)>0){borders.entry(b.into()).or_default().insert("글자 테두리".into());}}}
    let mut used_fonts=BTreeMap::new();let mut used_chars=0;for (cid,u) in &char_use{if u["chars"].as_u64().unwrap_or(0)>0{used_chars+=1;if let Some(f)=chars.get(cid).and_then(|c|c["font"]["hangul"].as_str()){*used_fonts.entry(f.to_string()).or_insert(0usize)+=u["chars"].as_u64().unwrap_or(0) as usize;}}}
    let table_total=tables.iter().map(|t|t["count"].as_u64().unwrap_or(0) as usize).sum::<usize>();let table_types=tables.len();for t in &mut tables{t.as_object_mut().unwrap().remove("_sig");}
    for x in &mut combos{x.as_object_mut().unwrap().remove("_key");}
    let summary=json!({"paragraphs":total,"fonts_defined":defs["fonts"].as_object().map(|m|m.values().map(|x|x.as_object().map_or(0,|o|o.len())).sum::<usize>()).unwrap_or(0),"fonts_used":used_fonts.len(),"chars_defined":chars.as_object().map_or(0,|m|m.len()),"chars_used":used_chars,"paras_defined":paras.as_object().map_or(0,|m|m.len()),"paras_used":para_use.len(),"styles_defined":defs["styles"].as_object().map_or(0,|m|m.len()),"styles_used":style_use.keys().filter(|s|defs["styles"].get(*s).is_some()).count(),"border_fills_defined":defs["border_fills"].as_object().map_or(0,|m|m.len()),"border_fills_used":borders.len(),"tables":table_total,"table_types":table_types,"combos":combos.len(),"types":types.len(),"shade_colors":shades.len()});
    json!({"summary":summary,"page":page,"used_fonts":used_fonts,"char_usage":char_use.into_iter().filter(|(_,u)|u["chars"].as_u64().unwrap_or(0)>0).collect::<BTreeMap<_,_>>(),"para_usage":para_use,"style_usage":style_use,"border_fill_usage":borders.into_iter().map(|(k,v)|(k,json!(v.into_iter().collect::<Vec<_>>()))).collect::<BTreeMap<_,_>>(),"combos":combos,"types":types,"shade_colors":shades,"inline_effects":effects,"tables":tables})
}
fn attr<'a>(node:&'a Value,key:&str)->Option<&'a str>{node.get("attrib").and_then(|a|a.get(key)).and_then(Value::as_str)}
fn local(node:&Value)->&str{node.get("tag").and_then(Value::as_str).unwrap_or("").rsplit('}').next().unwrap_or("")}
fn direct_runs(node:&Value)->Vec<&Value>{node.get("children").and_then(Value::as_array).map(|a|a.iter().filter(|x|local(x)=="run").collect()).unwrap_or_default()}
fn run_text(run:&Value)->String{run.get("children").and_then(Value::as_array).map(|a|a.iter().filter(|x|local(x)=="t").map(|x|x.get("text").and_then(Value::as_str).unwrap_or("")).collect()).unwrap_or_default()}
fn para_text(para:&Value)->String{direct_runs(para).iter().map(|r|run_text(r)).collect()}
fn iter_local<'a>(node:&'a Value,name:&str,out:&mut Vec<&'a Value>){if local(node)==name{out.push(node)}if let Some(c)=node.get("children").and_then(Value::as_array){for x in c{iter_local(x,name,out)}}}
fn table_data(tbl:&Value,chars:&Value)->(Value,Value){
    use std::collections::BTreeMap;
    let mut cells=Vec::new();iter_local(tbl,"tc",&mut cells);let mut rows=Vec::new();iter_local(tbl,"tr",&mut rows);
    let mut cellfills:BTreeMap<String,usize>=BTreeMap::new();for c in &cells{*cellfills.entry(attr(c,"borderFillIDRef").unwrap_or("null").into()).or_insert(0)+=1;}
    let sz=find_local(tbl,"sz");let width=sz.and_then(|n|attr(n,"width")).and_then(|v|v.parse::<f64>().ok()).map(|v|(v/(7200.0/25.4)*10.0).round()/10.0);
    let mut first=String::new();if let Some(row)=rows.first(){let mut tcs=Vec::new();if let Some(cs)=row.get("children").and_then(Value::as_array){tcs.extend(cs.iter().filter(|x|local(x)=="tc"));}first=tcs.iter().map(|c|text_tree(c).trim().to_string()).collect::<Vec<_>>().join(" | ").chars().take(120).collect();}
    let info=json!({"rows":attr(tbl,"rowCnt").and_then(|x|x.parse::<usize>().ok()).filter(|x|*x>0).unwrap_or(rows.len()),"cols":attr(tbl,"colCnt").and_then(|x|x.parse::<usize>().ok()).unwrap_or(0),"width_mm":width,"table_border_fill":attr(tbl,"borderFillIDRef"),"cell_border_fills":cellfills,"cell_margin_mm":null,"header_chars":[],"body_chars":[],"first_row":first});
    let sig=json!([attr(tbl,"colCnt"),attr(tbl,"borderFillIDRef"),cellfills.keys().collect::<Vec<_>>(),null,width.map_or(false,|x|x>120.0)]);let _=chars;(sig,info)
}
fn find_local<'a>(node:&'a Value,name:&str)->Option<&'a Value>{if local(node)==name{return Some(node)}node.get("children")?.as_array()?.iter().find_map(|x|find_local(x,name))}
fn text_tree(node:&Value)->String{let mut s=node.get("text").and_then(Value::as_str).unwrap_or("").to_string();if let Some(c)=node.get("children").and_then(Value::as_array){for x in c{s.push_str(&text_tree(x));}}s}

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
