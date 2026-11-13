//! kvf 설정만 읽고 쓴다. kagent 설정은 단축키 비교용 읽기 전용이다.
use crate::{json::{self,Value},hotkey};
use std::path::Path;
#[derive(Clone,Debug,PartialEq)]
pub struct Settings {pub hotkey:String,pub tray:bool,pub sticky:bool,pub rect:Option<[i32;4]>,pub last_box:Option<i64>}
impl Default for Settings {fn default()->Self{Self{hotkey:"Win+Shift+F".into(),tray:true,sticky:false,rect:None,last_box:None}}}
impl Settings {
 pub fn parse(text:&str)->Self {
  let mut s=Self::default();let Some(v)=json::parse(text) else{return s};
  if let Some(h)=v.str("hotkey").filter(|h|h.chars().count()<=40){s.hotkey=h.into();}
  s.tray=v.bool("tray").unwrap_or(true);s.sticky=v.bool("sticky").unwrap_or(false);
  s.last_box=v.num("last_box").filter(|n|n.fract()==0.0&&*n>0.0&&*n<9e15).map(|n|n as i64);
  let nums:Option<Vec<i32>>=["x","y","width","height"].iter().map(|k|v.num(k).filter(|n|n.fract()==0.0&&n.abs()<=100000.0).map(|n|n as i32)).collect();
  if let Some(n)=nums {if (360..=10000).contains(&n[2])&&(160..=10000).contains(&n[3]){s.rect=Some([n[0],n[1],n[2],n[3]]);}}
  s
 }
 pub fn load(path:&Path)->Self{std::fs::read_to_string(path).map(|s|Self::parse(&s)).unwrap_or_default()}
 pub fn value(&self)->Value {
  let mut v=vec![("hotkey",self.hotkey.clone().into()),("tray",self.tray.into()),("sticky",self.sticky.into()),("last_box",self.last_box.map(Value::from).unwrap_or(Value::Null))];
  if let Some(r)=self.rect{for (k,n) in ["x","y","width","height"].into_iter().zip(r){v.push((k,Value::from(n as i64)));}}
  Value::obj(v)
 }
 pub fn save(&self,path:&Path)->std::io::Result<bool>{write_changed(path,&json::to_string(&self.value()))}
}
pub fn write_changed(path:&Path,text:&str)->std::io::Result<bool>{
 if std::fs::read_to_string(path).ok().as_deref()==Some(text){return Ok(false)}
 if let Some(p)=path.parent(){std::fs::create_dir_all(p)?;}std::fs::write(path,text)?;Ok(true)
}
pub fn same_hotkey(a:&str,b:&str)->bool{match (hotkey::parse_hotkey(a),hotkey::parse_hotkey(b)){(Some(a),Some(b))=>a==b,_=>false}}
pub fn agent_hotkey(path:&Path)->String{std::fs::read_to_string(path).ok().and_then(|s|json::parse(&s)).and_then(|v|v.str("hotkey").map(String::from)).unwrap_or_else(||"Ctrl+Shift+V".into())}
/// kagent 설정의 화면 테마 선호("system"·"light"·"dark"). 파일·키가 없거나 다른 값이면 시스템 따라가기.
pub fn agent_theme(path:&Path)->ui_theme::ThemePref{std::fs::read_to_string(path).ok().and_then(|s|json::parse(&s)).and_then(|v|v.str("theme").map(ui_theme::ThemePref::parse)).unwrap_or(ui_theme::ThemePref::System)}
#[cfg(test)] mod tests{
 use super::*;
 #[test]fn defaults(){assert_eq!(Settings::parse("bad"),Settings::default());}
 #[test]fn roundtrip(){let s=Settings{hotkey:"Alt+F2".into(),tray:false,sticky:true,rect:Some([-300,10,600,400]),last_box:Some(2)};assert_eq!(Settings::parse(&json::to_string(&s.value())),s);}
 #[test]fn invalid_ranges(){let s=Settings::parse(r#"{"width":3,"height":999999,"last_box":-1}"#);assert_eq!(s.rect,None);assert_eq!(s.last_box,None);}
 #[test]fn long_hotkey(){assert_eq!(Settings::parse(&format!("{{\"hotkey\":\"{}\"}}","a".repeat(41))).hotkey,"Win+Shift+F");}
 #[test]fn disabled_hotkey(){assert_eq!(Settings::parse(r#"{"hotkey":""}"#).hotkey,"");}
 #[test]fn combinations(){assert!(same_hotkey("Shift+Ctrl+V","Ctrl+Shift+V"));assert!(!same_hotkey("",""));assert!(!same_hotkey("Win+Shift+F","Ctrl+Shift+V"));}
 #[test]fn missing_agent(){assert_eq!(agent_hotkey(Path::new("E:/docfit-tmp/absent-kvf-settings.json")),"Ctrl+Shift+V");}
 #[test]fn agent_theme_reads_kagent_settings(){
  let p=std::env::temp_dir().join(format!("kvf-agent-theme-{}.json",std::process::id()));
  assert_eq!(agent_theme(&p),ui_theme::ThemePref::System,"파일 없음");
  for (text,want) in [(r#"{"theme":"dark"}"#,ui_theme::ThemePref::Dark),(r#"{"theme":"light","hotkey":"Ctrl+Shift+V"}"#,ui_theme::ThemePref::Light),(r#"{"theme":"blue"}"#,ui_theme::ThemePref::System),(r#"{"hotkey":"Alt+V"}"#,ui_theme::ThemePref::System),("bad",ui_theme::ThemePref::System)]{std::fs::write(&p,text).unwrap();assert_eq!(agent_theme(&p),want,"{text}");}
  std::fs::remove_file(p).unwrap();
 }
 #[test]fn file_and_unchanged(){let p=std::path::PathBuf::from(format!("E:/docfit-tmp/kvf-settings-{}.json",std::process::id()));let s=Settings::default();assert!(s.save(&p).unwrap());assert!(!s.save(&p).unwrap());assert_eq!(Settings::load(&p),s);std::fs::remove_file(p).unwrap();}
}
