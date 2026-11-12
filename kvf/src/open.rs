//! 열기 요청 구성과 실행을 분리한다. 원본은 탐색기 선택에만 사용한다.
use std::path::Path;
use crate::vf_data::{self,Item,OpenTarget,OpenError};
use windows_sys::Win32::{Foundation::*,Storage::FileSystem::{INVALID_FILE_ATTRIBUTES,FILE_ATTRIBUTE_REPARSE_POINT},UI::{Shell::ShellExecuteW,WindowsAndMessaging::SW_SHOWNORMAL},System::SystemInformation::GetWindowsDirectoryW};
#[derive(Clone,Copy,Debug,PartialEq)]pub enum Action{Open,LocateCopy,LocateOrigin}
#[derive(Clone,Debug,PartialEq)]pub enum Request{Open(String),Select(String)}
pub fn explorer_args(path:&str)->Result<String,String>{
 if !Path::new(path).is_absolute()||path.chars().any(|c|c=='"'||c.is_control()){return Err("위치 경로가 올바르지 않습니다".into());}
 Ok(format!("/select,\"{path}\""))
}
fn reason(e:OpenError)->String{match e{OpenError::NoCopy=>"사본이 없습니다. 앱에서 한 번 열어 주세요",OpenError::Missing=>"자료가 옮겨졌거나 지워졌습니다",OpenError::Executable=>"실행 파일은 열 수 없습니다",OpenError::UnsafeUrl=>"http/https 링크만 열 수 있습니다",_=>"안전한 사본 경로가 아닙니다"}.into()}
/// exists와 safe_copy는 파일 시스템 관측값을 주입한다. 시험은 외부 앱을 실행하지 않는다.
pub fn request(root:&Path,item:&Item,action:Action,exists:impl Fn(&Path)->bool,safe_copy:impl Fn(&Path)->bool)->Result<Request,String>{
 if action==Action::LocateOrigin{
  if item.origin_path.is_empty()||!exists(Path::new(&item.origin_path)){return Err("원본 위치를 찾을 수 없습니다".into());}
  return explorer_args(&item.origin_path).map(Request::Select);
 }
 let target=vf_data::openable(root,item,exists(Path::new(&item.managed_path))).map_err(reason)?;
 match target{
  OpenTarget::Web(url) if action==Action::Open=>Ok(Request::Open(url)),
  OpenTarget::Web(_)=>Err("링크에는 사본 위치가 없습니다".into()),
  OpenTarget::Copy(path)=>{
   if !safe_copy(Path::new(&path)){return Err("링크·정션을 포함한 사본은 열 수 없습니다".into());}
   if action==Action::LocateCopy{explorer_args(&path).map(Request::Select)}else{Ok(Request::Open(path))}
  }
 }
}
pub fn safe_copy(root:&Path,path:&Path)->bool{unsafe{
 // 검사 직후 파일 교체까지 막는 것은 아니다. 사용자 폴더의 원자적 잠금은 범위 밖이다.
 let mut current=Some(path);while let Some(p)=current{
  let attrs=windows_sys::Win32::Storage::FileSystem::GetFileAttributesW(crate::util::wide(&p.to_string_lossy()).as_ptr());
  if !safe_attributes(attrs){return false;}
  if p==root{return true;}current=p.parent();
 }false
}}
fn safe_attributes(attrs:u32)->bool{attrs!=INVALID_FILE_ATTRIBUTES&&attrs&FILE_ATTRIBUTE_REPARSE_POINT==0}
pub fn execute(request:&Request,mut launch:impl FnMut(&str,Option<&str>)->Result<(),String>)->Result<(),String>{match request{Request::Open(path)=>launch(path,None),Request::Select(args)=>{
 let mut buf=[0u16;32768];let n=unsafe{GetWindowsDirectoryW(buf.as_mut_ptr(),buf.len()as u32)}as usize;
 if n==0||n>=buf.len(){return Err("Windows 폴더를 찾지 못했습니다".into());}
 let explorer=format!("{}\\explorer.exe",String::from_utf16_lossy(&buf[..n]));launch(&explorer,Some(args))
}}}
pub fn open(hwnd:HWND,root:&Path,item:&Item,action:Action)->Result<(),String>{
 let req=request(root,item,action,Path::exists,|p|safe_copy(root,p))?;
 execute(&req,|file,args|unsafe{let file=crate::util::wide(file);let args=args.map(crate::util::wide);let code=ShellExecuteW(hwnd,crate::util::wide("open").as_ptr(),file.as_ptr(),args.as_ref().map_or(std::ptr::null(),|v|v.as_ptr()),std::ptr::null(),SW_SHOWNORMAL)as isize;
  if code>32{Ok(())}else{Err(format!("열지 못했습니다 ({code})"))}
 })
}
#[cfg(test)]mod tests{
 use super::*;
 fn root()->std::path::PathBuf{std::path::PathBuf::from("E:/docfit-tmp/kvf-open-test")}
 fn item()->Item{Item{id:1,box_id:2,kind:"file".into(),source_path:"C:/source/a.txt".into(),origin_path:"C:/원본/a.txt".into(),managed_path:root().join("2/core/한글 파일.txt").to_string_lossy().into(),display_name:"파일".into(),space:"core".into(),created_at:0.0}}
 #[test]fn explorer_quotes_unicode_spaces_commas(){assert_eq!(explorer_args("C:\\한글 폴더\\a,b.txt").unwrap(),"/select,\"C:\\한글 폴더\\a,b.txt\"");}
 #[test]fn explorer_rejects_injection(){for p in ["relative.txt","C:\\a\"b.txt","C:\\a\nb.txt"]{assert!(explorer_args(p).is_err());}}
 #[test]fn opens_only_copy(){let i=item();assert_eq!(request(&root(),&i,Action::Open,|_|true,|_|true),Ok(Request::Open(i.managed_path)));}
 #[test]fn missing_copy_never_falls_back(){let mut i=item();i.managed_path.clear();assert!(request(&root(),&i,Action::Open,|_|true,|_|true).unwrap_err().contains("앱에서 한 번"));}
 #[test]fn blocked_extensions(){for ext in ["exe","CMD","ps1","lnk"]{let mut i=item();i.managed_path=root().join(format!("2/core/a.{ext}")).to_string_lossy().into();assert!(request(&root(),&i,Action::Open,|_|true,|_|true).is_err());}}
 #[test]fn distinct_locations(){let i=item();assert_eq!(request(&root(),&i,Action::LocateCopy,|_|true,|_|true),Ok(Request::Select(explorer_args(&i.managed_path).unwrap())));assert_eq!(request(&root(),&i,Action::LocateOrigin,|_|true,|_|false),Ok(Request::Select(explorer_args(&i.origin_path).unwrap())));}
 #[test]fn missing_and_reparse_refused(){assert!(request(&root(),&item(),Action::Open,|_|false,|_|true).is_err());assert!(request(&root(),&item(),Action::Open,|_|true,|_|false).is_err());assert!(request(&root(),&item(),Action::LocateOrigin,|_|false,|_|true).is_err());}
 #[test]fn reparse_attribute_is_rejected(){assert!(!safe_attributes(INVALID_FILE_ATTRIBUTES));assert!(!safe_attributes(FILE_ATTRIBUTE_REPARSE_POINT|0x10));assert!(safe_attributes(0x10));assert!(safe_attributes(0x80));}
 #[test]fn http_only(){let mut i=item();i.kind="link".into();i.source_path="https://example.com".into();assert_eq!(request(&root(),&i,Action::Open,|_|false,|_|false),Ok(Request::Open(i.source_path.clone())));i.source_path="file:///C:/a".into();assert!(request(&root(),&i,Action::Open,|_|true,|_|true).is_err());}
 #[test]fn launch_arguments_and_failure(){let i=item();let req=request(&root(),&i,Action::LocateCopy,|_|true,|_|true).unwrap();let mut calls=vec![];execute(&req,|file,args|{calls.push((file.to_string(),args.map(String::from)));Ok(())}).unwrap();assert_eq!(calls.len(),1);assert!(calls[0].0.ends_with("\\explorer.exe"));assert!(Path::new(&calls[0].0).is_file());assert_eq!(calls[0].1.as_deref(),Some(explorer_args(&i.managed_path).unwrap().as_str()));assert!(execute(&Request::Open(i.managed_path),|_,_|Err("거부".into())).is_err());}
 #[test]fn inspection_preserves_files(){let root=std::path::PathBuf::from(format!("E:/docfit-tmp/kvf-open-{}",std::process::id()));let path=root.join("2/core/한글 파일.txt");std::fs::create_dir_all(path.parent().unwrap()).unwrap();std::fs::write(&path,b"copy bytes").unwrap();let mut i=item();i.managed_path=path.to_string_lossy().into();let origin=root.join("origin.txt");std::fs::write(&origin,b"original bytes").unwrap();i.origin_path=origin.to_string_lossy().into();let origin_before=std::fs::read(&origin).unwrap();let before=std::fs::read(&path).unwrap();let req=request(&root,&i,Action::Open,Path::exists,|p|safe_copy(&root,p)).unwrap();execute(&req,|file,args|{assert_eq!(file,i.managed_path);assert_eq!(args,None);Ok(())}).unwrap();let location=request(&root,&i,Action::LocateOrigin,Path::exists,|_|true).unwrap();execute(&location,|_,args|{assert_eq!(args,Some(explorer_args(&i.origin_path).unwrap().as_str()));Ok(())}).unwrap();assert_eq!(std::fs::read(&origin).unwrap(),origin_before);assert_eq!(std::fs::read(&path).unwrap(),before);std::fs::remove_dir_all(root).unwrap();}
}
