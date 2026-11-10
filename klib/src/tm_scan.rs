//! Python docfit_core.timemachine.scan + rules의 단일 프로세스 메타데이터 스캔.
use std::{collections::HashSet, fs, path::Path, time::UNIX_EPOCH};
use crate::json::Value;

const DIRS: &[&str] = &["windows","program files","program files (x86)","programdata","$recycle.bin","system volume information","appdata","temp","packages","user data","cache","code cache","gpu cache","node_modules",".git",".venv","venv","__pycache__",".dart_tool",".gradle",".idea",".vscode"];
const NAMES: &[&str] = &["~$*","~wrl*.tmp","*.tmp","*.crdownload","*.part","thumbs.db","desktop.ini","*.asv","*.bak","*.hwp.tmp",".~lock.*","*(무결성검사).json",".rename-tmp-*"];
const EXTS: &[(&str,&str)] = &[(".hwp","hwp"),(".hwpx","hwp"),(".hwt","hwp"),(".doc","doc"),(".docx","doc"),(".txt","doc"),(".md","doc"),(".xls","sheet"),(".xlsx","sheet"),(".xlsm","sheet"),(".csv","sheet"),(".ppt","slide"),(".pptx","slide"),(".pdf","pdf"),(".jpg","image"),(".jpeg","image"),(".png","image"),(".gif","image"),(".bmp","image"),(".tif","image"),(".tiff","image"),(".webp","image")];

fn wildcard(p:&str,s:&str)->bool { let (p,s)=(p.as_bytes(),s.as_bytes()); let(mut i,mut j,mut star,mut mark)=(0,0,usize::MAX,0); while j<s.len(){ if i<p.len()&&(p[i]==b'?'||p[i]==s[j]){i+=1;j+=1}else if i<p.len()&&p[i]==b'*'{star=i;i+=1;mark=j}else if star!=usize::MAX{ i=star+1;mark+=1;j=mark}else{return false} } while i<p.len()&&p[i]==b'*'{i+=1} i==p.len() }
fn ext_kind(name:&str, selected:&HashSet<String>, extras:&HashSet<String>)->Option<String>{let ext=Path::new(name).extension()?.to_string_lossy().to_lowercase();let dotted=format!(".{ext}"); if !selected.is_empty()&&!selected.contains(&ext){return None} if let Some((_,k))=EXTS.iter().find(|(e,_)|*e==dotted){Some((*k).into())}else if extras.contains(&ext){Some("other".into())}else{None}}
fn excluded(name:&str)->bool {let base=Path::new(name).file_name().unwrap_or_default().to_string_lossy().to_lowercase();let v=base.trim_matches(|c:char|matches!(c,' '| '\t'|'\r'|'\n'|'\''|'"'|'‘'|'’'|'“'|'”'|'`'|'\u{00a0}'|'\u{200b}'|'\u{feff}'));v.starts_with("#ieeee")}
fn timestamp(t:std::io::Result<std::time::SystemTime>)->f64{t.ok().and_then(|x|x.duration_since(UNIX_EPOCH).ok()).map(|x|x.as_secs_f64()).unwrap_or(0.0)}
fn number(x:f64)->Value{Value::Num(x)}
pub fn run(root:&str, req:&Value)->Result<Value,String>{
 let root=Path::new(root).to_path_buf(); if !root.is_dir(){return Err("root is not a directory".into())}
 let selected:HashSet<String>=req.arr("extensions").unwrap_or(&[]).iter().filter_map(|v|if let Value::Str(s)=v{Some(s.trim_start_matches('.').to_lowercase())}else{None}).collect();
 let extras:HashSet<String>=req.arr("extra_extensions").unwrap_or(&[]).iter().filter_map(|v|if let Value::Str(s)=v{Some(s.trim_start_matches('.').to_lowercase())}else{None}).collect();
 let extra_dirs:HashSet<String>=req.arr("extra_dirs").unwrap_or(&[]).iter().filter_map(|v|if let Value::Str(s)=v{Some(s.to_lowercase())}else{None}).collect();
 let extra_names:Vec<String>=req.arr("extra_names").unwrap_or(&[]).iter().filter_map(|v|if let Value::Str(s)=v{Some(s.to_lowercase())}else{None}).collect();
 let include_hidden=req.bool("include_hidden").unwrap_or(false); let mode=req.str("time_mode").unwrap_or("max"); if mode!="max"&&mode!="mtime"{return Err("time_mode".into())}
 let mut stack=vec![root.clone()];let mut rows=Vec::new();let mut errors=Vec::new();let mut skipped=0usize;
 while let Some(dir)=stack.pop(){let rd=match fs::read_dir(&dir){Ok(x)=>x,Err(e)=>{errors.push(Value::Arr(vec![dir.to_string_lossy().into_owned().into(),format!("{}",if e.kind()==std::io::ErrorKind::PermissionDenied{"PermissionError"}else{"OSError"}).into()]));continue}};
 for ent in rd{let ent=match ent{Ok(x)=>x,Err(e)=>{errors.push(Value::Arr(vec![dir.to_string_lossy().into_owned().into(),if e.kind()==std::io::ErrorKind::PermissionDenied{"PermissionError"}else{"OSError"}.into()]));continue}};let path=ent.path();let name=ent.file_name().to_string_lossy().into_owned();let meta=match fs::symlink_metadata(&path){Ok(x)=>x,Err(e)=>{errors.push(Value::Arr(vec![path.to_string_lossy().into_owned().into(),if e.kind()==std::io::ErrorKind::PermissionDenied{"PermissionError"}else{"OSError"}.into()]));continue}};if meta.file_type().is_symlink(){continue}
 let attrs=attrs(&meta); if meta.is_dir(){if attrs&0x400!=0||DIRS.contains(&name.to_lowercase().as_str())||extra_dirs.contains(&name.to_lowercase())||(!include_hidden&&(attrs&6!=0||name.starts_with('.'))){skipped+=1}else{stack.push(path)}continue}
 if !meta.is_file(){continue}let kind=match ext_kind(&name,&selected,&extras){Some(k)=>k,None=>continue};let folded=name.to_lowercase();if excluded(&name)||NAMES.iter().copied().chain(extra_names.iter().map(String::as_str)).any(|p|wildcard(p,&folded))||(!include_hidden&&attrs&2!=0){continue}
 let mt=timestamp(meta.modified());let ct=timestamp(meta.created().or_else(|_|meta.modified()));let mut flags=Vec::new();if ct>mt+2.0{flags.push(Value::from("copied"))}else if (ct-mt).abs()<=2.0{flags.push(Value::from("new"))}let wt=if mode=="mtime"{mt}else{ct.max(mt)};
 rows.push(Value::obj(vec![("path",path.to_string_lossy().into_owned().into()),("root",root.to_string_lossy().into_owned().into()),("name",name.into()),("kind",kind.into()),("size",(meta.len().min(i64::MAX as u64) as i64).into()),("mtime",number(mt)),("ctime",number(ct)),("work_time",number(wt)),("flags",Value::Arr(flags)),("cloud",Value::Bool(attrs&0x00441000!=0))]));
 }}
 Ok(Value::obj(vec![("rows",Value::Arr(rows)),("errors",Value::Arr(errors)),("skipped_dirs",skipped.into())]))
}
#[cfg(windows)] fn attrs(m:&fs::Metadata)->u32{use std::os::windows::fs::MetadataExt;m.file_attributes()}
#[cfg(not(windows))] fn attrs(_: &fs::Metadata)->u32{0}
