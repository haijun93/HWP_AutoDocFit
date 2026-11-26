use serde_json::{json, Map, Value};
use std::{env, fs, io::{Read, Write}, path::{Path, PathBuf}};

pub const DEFAULTS_JSON: &str = include_str!("../../docs/settings_defaults.json");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsError { pub code: &'static str, pub message: String }
impl SettingsError { fn new(code: &'static str, message: impl Into<String>) -> Self { Self { code, message: message.into() } } }
impl std::fmt::Display for SettingsError { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}: {}", self.code, self.message) } }
impl std::error::Error for SettingsError {}

pub fn defaults() -> Value { serde_json::from_str(DEFAULTS_JSON).expect("embedded settings defaults") }

pub fn settings_path() -> PathBuf {
    let appdata = env::var_os("APPDATA").map(|p|PathBuf::from(p).join("HwpAutoDocFit"));
    let local = env::var_os("LOCALAPPDATA").map(|p|PathBuf::from(p).join("HwpAutoDocFit"));
    let home = env::var_os("USERPROFILE").or_else(|| env::var_os("HOME")).map(PathBuf::from);
    let root = appdata.or(local).or_else(|| home.map(|p| p.join(".hwp_auto_docfit"))).unwrap_or_else(env::temp_dir);
    root.join("settings.json")
}

pub fn read_json(path: &Path) -> Result<Value, SettingsError> {
    let mut bytes = Vec::new();
    match fs::File::open(path) {
        Ok(mut f) => { f.read_to_end(&mut bytes).map_err(|e| SettingsError::new("SETTINGS_READ_FAILED", e.to_string()))?; }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(json!({})),
        Err(e) => return Err(SettingsError::new("SETTINGS_READ_FAILED", e.to_string())),
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| SettingsError::new("SETTINGS_READ_FAILED", e.to_string()))?;
    if !value.is_object() { return Err(SettingsError::new("SETTINGS_READ_FAILED", "settings must be an object")); }
    Ok(value)
}

pub fn migrate(input: &Value) -> Value {
    let mut r = input.clone();
    if !r.is_object(){r=json!({});}
    let Some(obj) = r.as_object_mut() else { return json!({}); };
    obj.remove("diagram_editor_enabled");
    for (old, new) in [("all_linespacing", "all_exclude_linespacing"), ("spacing_page_fit", "spacing_exclude_pagefit"), ("spacing_reset_existing", "spacing_exclude_reset")] {
        if !obj.contains_key(new) { if let Some(Value::Bool(v)) = obj.get(old) { obj.insert(new.to_string(), Value::Bool(!v)); } }
        obj.remove(old);
    }
    if !obj.contains_key("spacing_exclude_reset") {
        if let Some(Value::Bool(v)) = obj.get("stage_choices").and_then(Value::as_object).and_then(|x| x.get("spacing")).and_then(Value::as_object).and_then(|x| x.get("reset_spacing")) { obj.insert("spacing_exclude_reset".into(), Value::Bool(!v)); }
    }
    for old_key in ["all_linespacing","spacing_page_fit","spacing_reset_existing"] { obj.remove(old_key); }
    obj.insert("card_options_rev".into(), json!(2));
    fn is_old(obj:&Map<String,Value>, key:&str)->bool { obj.get(key).and_then(|v| if let Some(s)=v.as_str(){s.parse::<i64>().ok()}else{v.as_i64()}).unwrap_or(1)<2 }
    let d = defaults();
    let label_revision_old=is_old(obj,"label_symbols_rev");
    if label_revision_old && obj.get("label_symbols").map(Value::is_object).unwrap_or(true) {
        let mut symbols = obj.get("label_symbols").and_then(Value::as_object).cloned().filter(|_|obj.get("label_symbols").is_some_and(Value::is_object)).or_else(|| d["label_symbols"].as_object().cloned()).unwrap_or_default();
        symbols.insert("*".into(), json!(false)); symbols.insert("**".into(), json!(false)); obj.insert("label_symbols".into(), Value::Object(symbols));
    }
    if is_old(obj,"report_files_rev") { for k in ["log_file","integrity_report_file","final_review_file"] { obj.insert(k.into(),json!(false)); } }
    if is_old(obj,"std_parspace_rev") {
        let prior=[("std_parspace_chapter","20"),("std_parspace_midtitle","15"),("std_parspace_box","15"),("std_parspace_circle","10"),("std_parspace_note","3")];
        let same=prior.iter().all(|(k,v)| obj.get(*k).and_then(Value::as_str).map(str::trim).map(|s|s==*v).unwrap_or(true));
        if same { for (k,_) in prior { obj.insert(k.into(),d[k].clone()); } obj.insert("std_parspace_dash".into(),d["std_parspace_dash"].clone()); }
    }
    if is_old(obj,"std_supplement_rev") { obj.insert("std_supplement_indent".into(),json!(true)); }
    if is_old(obj,"linespacing_min_rev") && obj.get("linespacing_min").map(|v|v.as_str().unwrap_or("160").trim()=="160").unwrap_or(true) { obj.insert("linespacing_min".into(),d["linespacing_min"].clone()); }
    for k in ["label_symbols_rev","report_files_rev","std_parspace_rev","std_supplement_rev","linespacing_min_rev"] { obj.insert(k.into(),json!(2)); }
    if let Some(fonts)=obj.get_mut("symbol_fonts").and_then(Value::as_object_mut) { for spec in fonts.values_mut().filter_map(Value::as_object_mut) { if spec.get("font")==Some(&json!("한컴돋음")) { spec.insert("font".into(),json!("한컴돋움")); } } }
    r
}

fn merge_for_snapshot(dst:&mut Map<String,Value>,src:&Map<String,Value>){for(k,v)in src{if let Some(Value::Object(existing))=dst.get_mut(k){if let Some(incoming)=v.as_object(){merge_for_snapshot(existing,incoming)}else{dst.insert(k.clone(),v.clone());}}else{dst.insert(k.clone(),v.clone());}}}
pub fn merged(input: &Value) -> Value {
    let migrated = migrate(input);
    let mut out = defaults();
    let label_revision_old=input.get("label_symbols_rev").and_then(Value::as_i64).unwrap_or(1)<2;
    if let (Some(dst), Some(src)) = (out.as_object_mut(), migrated.as_object()) { merge_for_snapshot(dst, src); }
    if let (Some(dst), Some(src)) = (out.as_object_mut(), input.as_object()) { for (k,v) in src { if dst.contains_key(k) && !v.is_object() {dst.insert(k.clone(),v.clone());} } }
    if !["kfind","legacy"].contains(&out["search_engine"].as_str().unwrap_or("")) { out["search_engine"] = json!("kfind"); }
    if let Some(in_obj)=input.as_object(){if let Some(out_obj)=out.as_object_mut(){for key in ["label_symbols","label_symbols_rev","report_files_rev","std_parspace_rev","std_supplement_rev","linespacing_min_rev"]{if !((key=="label_symbols"&&label_revision_old)||((key=="label_symbols_rev"||key=="report_files_rev")&&label_revision_old)){if let Some(v)=in_obj.get(key){out_obj.insert(key.into(),v.clone());}}}}}
    out
}

/// 화면에 노출되는 설정 잎(키, 종류 boolean|number|text|choice|list, 기본값). 설정 창이 키→컨트롤 매핑에 쓴다.
pub fn visible_schema() -> Vec<(String, String, Value)> {
    fn walk(prefix: String, value: &Value, excluded: &std::collections::HashSet<&str>, out: &mut Vec<(String,String,Value)>) {
        let Some(map)=value.as_object() else { out.push((prefix.clone(), String::new(), value.clone())); return; };
        for (k,v) in map { let key=if prefix.is_empty(){k.clone()}else{format!("{prefix}.{k}")}; if prefix.is_empty() && excluded.contains(k.as_str()){continue} walk(key,v,excluded,out); }
    }
    let excluded: std::collections::HashSet<&str>=["ui_tk_fallback","hierarchy_engine","abbreviations","abbreviation_source","active_format_profile","rust_rename_preview","rust_timemachine_scan","rust_search_hierarchy","theme_mode","menu_display_mode","kclip_settings_mode","outliner_ui","settings_ui","docview_ui","photoview_ui","conversion_ui","rename_ui","label_symbols_rev","report_files_rev","std_parspace_rev","std_supplement_rev"].into_iter().collect();
    let mut out=Vec::new(); walk(String::new(),&defaults(),&excluded,&mut out);
    let numeric=["punctuation_threshold","retry_body","retry_table","linespacing_min","linespacing_max","std_title_subtitle_pt"];
    for (key,kind,value) in &mut out { let root=key.split('.').next().unwrap_or(""); *kind=if *key=="search_engine" {"choice".into()} else if value.is_array(){"list".into()} else if value.is_boolean(){"boolean".into()} else if numeric.contains(&root)||root.starts_with("std_parspace_")||key.ends_with(".size"){"number".into()} else {"text".into()}; }
    out
}

fn decimal(s: &str) -> Option<f64> { let t=s.trim(); if t.is_empty(){return None} let n=t.parse::<f64>().ok()?; if n.is_finite(){Some(n)}else{None} }

/// 목록 키의 항목 종류(키 이름·기본값 `[]` 은 그대로, 표현만 목록). 규칙은 Python 쪽 정리 함수와 같다:
/// `hidden_tabs` = `application_service.clean_hidden_tabs`(정수 1~15, 12·13 제외, 정렬·중복 없음),
/// `library_excluded_folders` = `library_exclusions.read_exclusions`(빈 글 아닌 문자열).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind { Text, Folder, MenuNumber }

pub const LIST_MAX_ITEMS: usize = 1000;
pub const LIST_ITEM_MAX_CHARS: usize = 4096;

pub fn list_kind(key: &str) -> ListKind {
    match key { "hidden_tabs" => ListKind::MenuNumber, "library_excluded_folders" => ListKind::Folder, _ => ListKind::Text }
}

/// 숨길 메뉴 번호로 쓸 수 있는가(1~15, 필수 메뉴 12·13 제외).
pub fn is_hideable_menu(n: i64) -> bool { (1..=15).contains(&n) && n != 12 && n != 13 }

fn validate_list(key: &str, value: &Value) -> Result<(), SettingsError> {
    let bad = |what: &str| SettingsError::new("INVALID_INPUT", format!("{key} {what}"));
    let Some(items) = value.as_array() else { return Err(bad("type")); };
    if items.len() > LIST_MAX_ITEMS { return Err(bad("too many items")); }
    match list_kind(key) {
        ListKind::MenuNumber => {
            let mut prev = 0i64;
            for v in items {
                // Python `type(v) is int` 와 같게: 불리언·소수·글은 거부. 정렬·중복 없음만 허용(정리 결과와 같아야 한다).
                let Some(n) = v.as_i64() else { return Err(bad("item type")); };
                if !is_hideable_menu(n) { return Err(bad("item range")); }
                if n <= prev { return Err(bad("order or duplicate")); }
                prev = n;
            }
        }
        ListKind::Folder | ListKind::Text => {
            let mut seen = std::collections::HashSet::new();
            for v in items {
                let Some(s) = v.as_str() else { return Err(bad("item type")); };
                if s.trim().is_empty() { return Err(bad("blank item")); }
                if s.chars().count() > LIST_ITEM_MAX_CHARS || s.contains('\0') { return Err(bad("item string")); }
                if !seen.insert(s.trim().to_lowercase()) { return Err(bad("duplicate")); }
            }
        }
    }
    Ok(())
}

pub fn validate_patch(patch: &Value) -> Result<Value, SettingsError> {
    let map=patch.as_object().ok_or_else(|| SettingsError::new("INVALID_INPUT","patch must be object"))?;
    let schema=visible_schema(); let mut clean=Map::new();
    for (key,value) in map {
        let Some((_,kind,default))=schema.iter().find(|(k,_,_)|k==key) else {return Err(SettingsError::new("INVALID_INPUT","unknown setting"));};
        match kind.as_str() {
            "boolean" if !value.is_boolean()=>return Err(SettingsError::new("INVALID_INPUT",format!("{key} type"))),
            "boolean"=>{},
            "list"=>validate_list(key,value)?,
            _=>{
                let Some(s)=value.as_str() else{return Err(SettingsError::new("INVALID_INPUT",format!("{key} type")));};
                if s.chars().count()>4096 || s.contains('\0') { return Err(SettingsError::new("INVALID_INPUT",format!("{key} string"))); }
                if kind=="number" { let Some(n)=decimal(s) else{return Err(SettingsError::new("INVALID_INPUT",format!("{key} number")));}; if !(0.0..=1000.0).contains(&n) || ((key.ends_with(".size")||key=="std_title_subtitle_pt")&&n<=0.0) || (["retry_body","retry_table","punctuation_threshold","linespacing_min","linespacing_max"].contains(&key.as_str())&&n.fract()!=0.0) {return Err(SettingsError::new("INVALID_INPUT",format!("{key} range")));} }
                if kind=="choice" && !["kfind","legacy"].contains(&s) {return Err(SettingsError::new("INVALID_INPUT",format!("{key} choice")));}
                let _=default;
            }
        }
        clean.insert(key.clone(),value.clone());
    }
    Ok(Value::Object(clean))
}

#[cfg(windows)]
struct FileLock { file: fs::File }
#[cfg(windows)]
impl FileLock { fn acquire(path:&Path)->std::io::Result<Self>{ use std::os::windows::io::AsRawHandle; use windows_sys::Win32::{Foundation::HANDLE,Storage::FileSystem::{LockFileEx,LOCKFILE_EXCLUSIVE_LOCK},System::IO::OVERLAPPED}; let file=fs::OpenOptions::new().create(true).read(true).write(true).open(path)?; let mut ov:OVERLAPPED=unsafe{std::mem::zeroed()}; let h=file.as_raw_handle() as HANDLE; let ok=unsafe{LockFileEx(h,LOCKFILE_EXCLUSIVE_LOCK,0,1,0,&mut ov)}; if ok==0{return Err(std::io::Error::last_os_error())} Ok(Self{file}) } }
#[cfg(windows)] impl Drop for FileLock { fn drop(&mut self){ use std::os::windows::io::AsRawHandle; use windows_sys::Win32::{Foundation::HANDLE,Storage::FileSystem::UnlockFileEx,System::IO::OVERLAPPED}; let mut ov:OVERLAPPED=unsafe{std::mem::zeroed()}; unsafe{UnlockFileEx(self.file.as_raw_handle() as HANDLE,0,1,0,&mut ov)}; } }
#[cfg(unix)]
struct FileLock { file: fs::File }
#[cfg(unix)] impl FileLock { fn acquire(path:&Path)->std::io::Result<Self>{ use std::os::fd::AsRawFd; let file=fs::OpenOptions::new().create(true).read(true).write(true).open(path)?; if unsafe{libc::flock(file.as_raw_fd(),libc::LOCK_EX)}!=0{return Err(std::io::Error::last_os_error())} Ok(Self{file}) } }
#[cfg(unix)] impl Drop for FileLock { fn drop(&mut self){use std::os::fd::AsRawFd; unsafe{libc::flock(self.file.as_raw_fd(),libc::LOCK_UN)};} }

pub fn atomic_write(path:&Path, value:&Value) -> Result<(),SettingsError> {
    let parent=path.parent().unwrap_or(Path::new(".")); fs::create_dir_all(parent).map_err(|e|SettingsError::new("SETTINGS_WRITE_FAILED",e.to_string()))?;
    let lock_path=path.with_file_name(format!("{}.lock",path.file_name().unwrap_or_default().to_string_lossy()));
    let _lock=FileLock::acquire(&lock_path).map_err(|e|SettingsError::new("SETTINGS_WRITE_FAILED",e.to_string()))?;
    let bytes=serde_json::to_vec_pretty(value).map_err(|e|SettingsError::new("SETTINGS_WRITE_FAILED",e.to_string()))?;
    let nonce=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let temp=path.with_file_name(format!("{}.{}.{}.tmp",path.file_name().unwrap_or_default().to_string_lossy(),std::process::id(),nonce));
    let result=(||{let mut f=fs::OpenOptions::new().write(true).create_new(true).open(&temp)?; f.write_all(&bytes)?; f.flush()?; f.sync_all()?; drop(f); replace_file(&temp,path)?; Ok::<(),std::io::Error>(())})();
    let _=fs::remove_file(&temp);
    result.map_err(|e|SettingsError::new("SETTINGS_WRITE_FAILED",e.to_string()))
}

pub fn get(path:&Path)->Result<Value,SettingsError>{let v=read_json(path)?;Ok(merged(&v))}
pub fn set(path:&Path, patch:&Value)->Result<Value,SettingsError>{
    let mut public_patch=patch.clone();if let Some(m)=public_patch.as_object_mut(){m.remove("__revision");}
    let clean=validate_patch(&public_patch)?; let _lock=FileLock::acquire(&path.with_file_name(format!("{}.lock",path.file_name().unwrap_or_default().to_string_lossy()))).map_err(|e|SettingsError::new("SETTINGS_WRITE_FAILED",e.to_string()))?;
    let mut current=read_json(path)?; let original=migrate(&current); let mut migrated=original.clone(); let Some(obj)=migrated.as_object_mut() else {return Err(SettingsError::new("INVALID_SETTINGS","settings must be object"));};
    if let Some(expected)=patch.get("__revision").and_then(Value::as_str){let actual=revision(&current);if expected!=actual{return Err(SettingsError::new("SETTINGS_CHANGED","settings revision changed"));}}
    for (k,v) in clean.as_object().unwrap(){let mut parts=k.split('.').peekable(); let mut target=&mut *obj; while let Some(p)=parts.next(){if parts.peek().is_none(){target.insert(p.into(),v.clone());break} let entry=target.entry(p).or_insert_with(||json!({})); let Some(next)=entry.as_object_mut() else{return Err(SettingsError::new("INVALID_SETTINGS","stored setting is not object"));};target=next;} }
    let merged_value=merged(&migrated); let a=decimal(merged_value["linespacing_min"].as_str().unwrap_or("140")).ok_or_else(||SettingsError::new("INVALID_SETTINGS","invalid minimum"))?; let b=decimal(merged_value["linespacing_max"].as_str().unwrap_or("200")).ok_or_else(||SettingsError::new("INVALID_SETTINGS","invalid maximum"))?; if a>b{return Err(SettingsError::new("INVALID_INPUT","minimum exceeds maximum"));}
    let mut to_save=original.clone(); if let (Some(dst),Some(src))=(to_save.as_object_mut(),migrated.as_object()){let d=defaults();for(k,v)in src{if dst.contains_key(k)||!d.as_object().unwrap().contains_key(k){dst.insert(k.clone(),v.clone());}}}
    if let (Some(dst),Some(src))=(to_save.as_object_mut(),clean.as_object()){for(k,v)in src{let mut parts=k.split('.').peekable();let mut target=&mut *dst;while let Some(part)=parts.next(){if parts.peek().is_none(){target.insert(part.into(),v.clone());break}let entry=target.entry(part).or_insert_with(||json!({}));if let Some(next)=entry.as_object_mut(){target=next}else{break}}}}
    if current!=to_save { atomic_write_unlocked(path,&to_save)?; }
    current=to_save; Ok(merged(&current))
}
/// 문서뷰·사진뷰 썸네일 크기(`ui_thumb_sizes.<view>`)만 바꾼다(앱 `thumb_size_setting.write_thumb_size` 대응).
/// 설정 창이 다루지 않는 내부 키라 `set` 의 목록 검사 대신 같은 잠금·원자 저장을 쓴다.
pub fn set_thumb_size(path:&Path, view:&str, size:f64)->Result<(),SettingsError>{
    if !matches!(view,"doc_grid"|"photo_album")||!size.is_finite(){return Err(SettingsError::new("INVALID_INPUT","unknown thumbnail view"));}
    let _lock=FileLock::acquire(&path.with_file_name(format!("{}.lock",path.file_name().unwrap_or_default().to_string_lossy()))).map_err(|e|SettingsError::new("SETTINGS_WRITE_FAILED",e.to_string()))?;
    let mut current=read_json(path)?;
    let Some(root)=current.as_object_mut() else{return Err(SettingsError::new("INVALID_SETTINGS","settings must be object"));};
    let sizes=root.entry("ui_thumb_sizes").or_insert_with(||json!({}));
    if !sizes.is_object(){*sizes=json!({});}
    let value=json!(size.clamp(100.0,240.0));
    if sizes.get(view)==Some(&value){return Ok(());}
    sizes[view]=value;
    atomic_write_unlocked(path,&current)
}
pub fn revision(value:&Value)->String{
    fn sorted(v:&Value)->Value{match v{Value::Object(m)=>{let mut entries:Vec<_>=m.iter().collect();entries.sort_by(|a,b|a.0.cmp(b.0));let mut out=Map::new();for(k,v)in entries{out.insert(k.clone(),sorted(v));}Value::Object(out)},Value::Array(a)=>Value::Array(a.iter().map(sorted).collect()),_=>v.clone()}}
    use sha2::{Digest,Sha256};let bytes=serde_json::to_string(&sorted(value)).unwrap_or_default().into_bytes();let hash=Sha256::digest(bytes);hash.iter().map(|b|format!("{b:02x}")).collect()
}
fn replace_file(from:&Path,to:&Path)->std::io::Result<()> { #[cfg(windows)] { use std::{os::windows::ffi::OsStrExt, ptr}; use windows_sys::Win32::Storage::FileSystem::{MoveFileExW,MOVEFILE_REPLACE_EXISTING,MOVEFILE_WRITE_THROUGH}; let src:Vec<u16>=from.as_os_str().encode_wide().chain(Some(0)).collect(); let dst:Vec<u16>=to.as_os_str().encode_wide().chain(Some(0)).collect(); let ok=unsafe{MoveFileExW(src.as_ptr(),dst.as_ptr(),MOVEFILE_REPLACE_EXISTING|MOVEFILE_WRITE_THROUGH)}; if ok==0{return Err(std::io::Error::last_os_error())} let _=ptr::null::<u8>(); Ok(()) } #[cfg(not(windows))] {fs::rename(from,to)} }
fn atomic_write_unlocked(path:&Path,value:&Value)->Result<(),SettingsError>{let bytes=serde_json::to_vec_pretty(value).map_err(|e|SettingsError::new("SETTINGS_WRITE_FAILED",e.to_string()))?; let temp=path.with_file_name(format!("{}.{}.tmp",path.file_name().unwrap_or_default().to_string_lossy(),std::process::id())); let mut f=fs::OpenOptions::new().write(true).create(true).truncate(true).open(&temp).map_err(|e|SettingsError::new("SETTINGS_WRITE_FAILED",e.to_string()))?; f.write_all(&bytes).and_then(|_|f.sync_all()).map_err(|e|SettingsError::new("SETTINGS_WRITE_FAILED",e.to_string()))?;drop(f); replace_file(&temp,path).map_err(|e|SettingsError::new("SETTINGS_WRITE_FAILED",e.to_string()))?; Ok(())}

#[cfg(test)] mod tests {
    use super::*; use tempfile::tempdir; use std::{sync::Arc,thread};
    #[test] fn defaults_has_at_least_97_top_level_keys(){assert!(defaults().as_object().unwrap().len()>=97);}
    #[test] fn read_corrupt_is_error_and_missing_is_empty(){let d=tempdir().unwrap();let p=d.path().join("s.json");assert_eq!(read_json(&p).unwrap(),json!({}));fs::write(&p,"{").unwrap();assert_eq!(read_json(&p).unwrap_err().code,"SETTINGS_READ_FAILED");}
    #[test] fn preserves_unknown_and_migrates(){let v=json!({"custom":{"x":1},"all_linespacing":true});let r=merged(&v);assert_eq!(r["custom"]["x"],1);assert_eq!(r["all_exclude_linespacing"],false);}
    #[test] fn list_keys_are_visible_lists_with_empty_defaults(){let v=visible_schema();for key in ["hidden_tabs","library_excluded_folders"]{let (_,kind,d)=v.iter().find(|(k,_,_)|k==key).unwrap_or_else(||panic!("{key} not visible"));assert_eq!(kind,"list");assert_eq!(d,&json!([]));}}
    #[test] fn list_validation_matches_the_app_cleaning_rules(){
        let ok=|k:&str,v:Value|validate_patch(&json!({k:v})).is_ok();
        assert!(ok("hidden_tabs",json!([])));assert!(ok("hidden_tabs",json!([1,3,15])));
        for bad in [json!([12]),json!([13]),json!([0]),json!([16]),json!([3,1]),json!([3,3]),json!(["3"]),json!([true]),json!([1.5]),json!("3"),json!(null)]{assert!(!ok("hidden_tabs",bad.clone()),"{bad}");}
        assert!(ok("library_excluded_folders",json!(["C:/a","D:/b"])));
        for bad in [json!([""]),json!(["  "]),json!([1]),json!(["C:/a","c:/A"]),json!(["a\u{0}b"]),json!("x")]{assert!(!ok("library_excluded_folders",bad.clone()),"{bad}");}
        let many:Vec<String>=(0..=LIST_MAX_ITEMS).map(|i|format!("C:/f{i}")).collect();assert!(!ok("library_excluded_folders",json!(many)));
    }
    #[test] fn list_keys_roundtrip_and_keep_unknown_keys(){let d=tempdir().unwrap();let p=d.path().join("s.json");fs::write(&p,r#"{"custom":{"x":1},"hidden_tabs":[2]}"#).unwrap();
        let got=get(&p).unwrap();assert_eq!(got["hidden_tabs"],json!([2]));assert_eq!(got["library_excluded_folders"],json!([]));
        set(&p,&json!({"hidden_tabs":[2,5],"library_excluded_folders":["C:/x"]})).unwrap();let raw=read_json(&p).unwrap();
        assert_eq!(raw["hidden_tabs"],json!([2,5]));assert_eq!(raw["library_excluded_folders"],json!(["C:/x"]));assert_eq!(raw["custom"]["x"],1);
        assert_eq!(set(&p,&json!({"hidden_tabs":[13]})).unwrap_err().code,"INVALID_INPUT");}
    #[test] fn thumb_size_keeps_other_view_and_unknown_keys(){let d=tempdir().unwrap();let p=d.path().join("s.json");fs::write(&p,r#"{"custom":1,"ui_thumb_sizes":{"doc_grid":200.0}}"#).unwrap();
        set_thumb_size(&p,"photo_album",999.0).unwrap();let v=read_json(&p).unwrap();
        assert_eq!(v["ui_thumb_sizes"],json!({"doc_grid":200.0,"photo_album":240.0}));assert_eq!(v["custom"],1);
        assert!(set_thumb_size(&p,"other",120.0).is_err());assert!(set_thumb_size(&p,"doc_grid",f64::NAN).is_err());
        fs::write(&p,"{").unwrap();assert!(set_thumb_size(&p,"doc_grid",120.0).is_err());assert_eq!(fs::read_to_string(&p).unwrap(),"{");}
    #[test] fn large_file_roundtrip(){let d=tempdir().unwrap();let p=d.path().join("s.json");let v=json!({"big":"x".repeat(2*1024*1024)});atomic_write(&p,&v).unwrap();assert_eq!(read_json(&p).unwrap(),v);}
    #[test] fn concurrent_updates_do_not_lose_keys(){let d=tempdir().unwrap();let p=Arc::new(d.path().join("s.json"));let mut joins=vec![];for i in 0..8{let p=p.clone();joins.push(thread::spawn(move||{let _guard=FileLock::acquire(&p.with_file_name("s.json.lock")).unwrap();let mut v=read_json(&p).unwrap();v.as_object_mut().unwrap().insert(format!("k{i}"),json!(i));atomic_write_unlocked(&p,&v).unwrap();}));}for j in joins{j.join().unwrap();}let v=read_json(&p).unwrap();for i in 0..8{assert_eq!(v[format!("k{i}")],i);}}
}
