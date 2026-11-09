use quick_xml::{events::Event, Reader};
use serde::Serialize;
use std::{ffi::{CStr, CString}, fs::File, io::Read, os::raw::c_char, panic::{catch_unwind, AssertUnwindSafe}, path::Path, sync::Mutex};
use zip::ZipArchive;

mod style_inventory {
    pub mod first;
    pub mod mm;
    pub mod tag;
    pub mod text_of;
    pub mod run_text;
    pub mod parse_fonts;
    pub mod lang_values;
    pub mod shade;
    pub mod parse_char;
}

const MAX_ARCHIVE: u64 = 128 * 1024 * 1024;
const MAX_ENTRY: u64 = 32 * 1024 * 1024;
const MAX_TOTAL: u64 = 128 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;

#[derive(Serialize)]
struct Report { version: u32, members: Vec<Member>, paragraphs: u64 }
#[derive(Serialize)]
struct Member { name: String, bytes: u64, elements: std::collections::BTreeMap<String,u64> }

fn parse_xml(name: &str, bytes: &[u8]) -> Result<Member,String> {
    let mut r=Reader::from_reader(bytes); r.config_mut().trim_text(true);
    let mut buf=Vec::new(); let mut counts=std::collections::BTreeMap::new();
    loop { match r.read_event_into(&mut buf) {
        Ok(Event::Start(e)) | Ok(Event::Empty(e)) => { let n=e.local_name().as_ref().to_owned(); let n=n.to_string(); *counts.entry(n).or_insert(0)+=1; },
        Ok(Event::DocType(_)) => return Err(format!("DOCTYPE forbidden: {name}")),
        Ok(Event::Eof)=>break,
        Err(e)=>return Err(format!("invalid XML {name}: {e}")), _=>{}
    } buf.clear(); }
    Ok(Member{name:name.into(),bytes:bytes.len() as u64,elements:counts})
}

fn analyze(path:&Path)->Result<String,String>{
 let f=File::open(path).map_err(|e|e.to_string())?; let size=f.metadata().map_err(|e|e.to_string())?.len();
 if size>MAX_ARCHIVE{return Err("archive size limit exceeded".into())}
 let mut z=ZipArchive::new(f).map_err(|e|e.to_string())?; if z.len()>MAX_ENTRIES{return Err("entry count limit exceeded".into())}
 let mut total=0u64;let mut members=Vec::new();let mut paragraphs=0;
 for i in 0..z.len(){let mut f=z.by_index(i).map_err(|e|e.to_string())?;let n=f.name().map_err(|e|e.to_string())?.to_string();
  if f.is_dir() || !(n=="Contents/header.xml" || (n.starts_with("Contents/section")&&n.ends_with(".xml"))){continue}
  if f.size()>MAX_ENTRY{return Err(format!("entry size limit exceeded: {n}"))} total=total.checked_add(f.size()).ok_or("size overflow")?;if total>MAX_TOTAL{return Err("expanded size limit exceeded".into())}
  let mut b=Vec::with_capacity(f.size() as usize);f.read_to_end(&mut b).map_err(|e|e.to_string())?;let m=parse_xml(&n,&b)?;if n.contains("section"){paragraphs+=*m.elements.get("p").unwrap_or(&0)} members.push(m);
 }
 if !members.iter().any(|m|m.name=="Contents/header.xml"){return Err("missing Contents/header.xml".into())}
 serde_json::to_string(&Report{version:1,members,paragraphs}).map_err(|e|e.to_string())
}

// Caller releases returned UTF-8 buffer with hwpx_free. Null on error; use hwpx_last_error.
static LAST_ERROR: Mutex<Option<CString>> = Mutex::new(None);
#[no_mangle] pub unsafe extern "C" fn hwpx_analyze(path:*const c_char)->*mut c_char {
 let result=catch_unwind(AssertUnwindSafe(||{if path.is_null(){return Err("null path".into())}let p=CStr::from_ptr(path).to_str().map_err(|e|e.to_string())?;analyze(Path::new(p))}));
 match result { Ok(Ok(s))=>CString::new(s).map(CString::into_raw).unwrap_or(std::ptr::null_mut()),Ok(Err(e))=>{*LAST_ERROR.lock().unwrap_or_else(|e|e.into_inner())=CString::new(e).ok();std::ptr::null_mut()},Err(_)=>{*LAST_ERROR.lock().unwrap_or_else(|e|e.into_inner())=CString::new("panic contained").ok();std::ptr::null_mut()} }
}
#[no_mangle] pub unsafe extern "C" fn hwpx_last_error()->*const c_char { LAST_ERROR.lock().unwrap_or_else(|e|e.into_inner()).as_ref().map_or(std::ptr::null(),|s|s.as_ptr()) }
#[no_mangle] pub unsafe extern "C" fn hwpx_free(p:*mut c_char){if !p.is_null(){drop(CString::from_raw(p));}}

fn call(fn_name: &str, json_in: &str) -> Result<String, String> {
    match fn_name {
        "_tag" => {
            let value: serde_json::Value = serde_json::from_str(json_in).map_err(|e| e.to_string())?;
            let tag = value.get("tag").and_then(serde_json::Value::as_str)
                .ok_or("_tag expects {\"tag\": string}")?;
            Ok(serde_json::to_string(style_inventory::tag::tag(tag)).unwrap())
        }
        "_first" => {
            let value: serde_json::Value = serde_json::from_str(json_in).map_err(|e| e.to_string())?;
            let name = value.get("name").and_then(serde_json::Value::as_str)
                .ok_or("_first expects {\"tree\": ..., \"name\": string}")?;
            let found = value.get("tree").and_then(|tree| style_inventory::first::first(tree, name));
            Ok(serde_json::to_string(&found).unwrap())
        }
        "_mm" => {
            let value: serde_json::Value = serde_json::from_str(json_in).map_err(|e| e.to_string())?;
            let input = match &value {
                serde_json::Value::Null => None,
                serde_json::Value::Number(n) => Some(n.to_string()),
                serde_json::Value::String(s) => Some(s.clone()),
                _ => None,
            };
            Ok(serde_json::to_string(&style_inventory::mm::mm(input.as_deref())).unwrap())
        }
        "_text_of" => {
            let value: serde_json::Value = serde_json::from_str(json_in).map_err(|e| e.to_string())?;
            let tree = value.get("tree").ok_or("_text_of expects {\"tree\": ...}")?;
            Ok(serde_json::to_string(&style_inventory::text_of::text_of(tree)).unwrap())
        }
        "_run_text" => {
            let value: serde_json::Value = serde_json::from_str(json_in).map_err(|e| e.to_string())?;
            let tree = value.get("tree").ok_or("_run_text expects {\"tree\": ...}")?;
            Ok(serde_json::to_string(&style_inventory::run_text::run_text(tree)).unwrap())
        }
        "_parse_fonts" => {
            let value: serde_json::Value = serde_json::from_str(json_in).map_err(|e| e.to_string())?;
            let tree = value.get("tree").ok_or("_parse_fonts expects {\"tree\": ...}")?;
            Ok(serde_json::to_string(&style_inventory::parse_fonts::parse_fonts(tree)).unwrap())
        }
        "_parse_char" => {
            let value: serde_json::Value = serde_json::from_str(json_in).map_err(|e| e.to_string())?;
            let tree = value.get("tree").ok_or("_parse_char expects tree")?;
            let fonts = value.get("fonts").unwrap_or(&serde_json::Value::Null);
            Ok(serde_json::to_string(&style_inventory::parse_char::parse_char(tree,fonts)).unwrap())
        }
        "_shade" => {
            let value: serde_json::Value = serde_json::from_str(json_in).map_err(|e| e.to_string())?;
            let shade = value.as_str();
            Ok(serde_json::to_string(&style_inventory::shade::shade(shade)).unwrap())
        }
        "_lang_values" => {
            let value: serde_json::Value = serde_json::from_str(json_in).map_err(|e| e.to_string())?;
            let tree = value.get("tree").ok_or("_lang_values expects {\"tree\": ..., \"cast\": ...}")?;
            let cast = value.get("cast").and_then(serde_json::Value::as_str).unwrap_or("int");
            Ok(serde_json::to_string(&style_inventory::lang_values::lang_values(tree, cast)?).unwrap())
        }
        _ => Err(format!("unknown function: {fn_name}")),
    }
}

#[no_mangle] pub unsafe extern "C" fn hwpx_call(fn_name:*const c_char, json_in:*const c_char)->*mut c_char {
    let result=catch_unwind(AssertUnwindSafe(||{
        if fn_name.is_null() || json_in.is_null() { return Err("null argument".into()); }
        let name=CStr::from_ptr(fn_name).to_str().map_err(|e|e.to_string())?;
        let input=CStr::from_ptr(json_in).to_str().map_err(|e|e.to_string())?;
        call(name,input)
    }));
    match result {
        Ok(Ok(s))=>CString::new(s).map(CString::into_raw).unwrap_or(std::ptr::null_mut()),
        Ok(Err(e))=>{*LAST_ERROR.lock().unwrap_or_else(|e|e.into_inner())=CString::new(e).ok();std::ptr::null_mut()},
        Err(_)=>{*LAST_ERROR.lock().unwrap_or_else(|e|e.into_inner())=CString::new("panic contained").ok();std::ptr::null_mut()}
    }
}

#[cfg(test)] mod tests { use super::*; #[test] fn rejects_doctype(){assert!(parse_xml("x",b"<!DOCTYPE x><x/>").is_err());} #[test] fn counts_namespaces(){let m=parse_xml("x",b"<hp:p xmlns:hp='u'><hp:run/></hp:p>").unwrap();assert_eq!(m.elements["p"],1);} }
