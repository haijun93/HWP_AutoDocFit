//! 독립 트레이 수명과 단축키. 시작·reload에서만 등록한다.
use crate::{settings,hotkey,util::wide};
use windows_sys::Win32::{Foundation::*,UI::{Shell::*,WindowsAndMessaging::*,Input::KeyboardAndMouse::*}};
pub const MESSAGE:u32=WM_APP+3;
pub struct Tray {pub ok:bool,pub error:String,pub settings:settings::Settings,failed:Option<String>,shown:bool}
impl Tray {
 pub fn new()->Self{Self{ok:false,error:String::new(),settings:Default::default(),failed:None,shown:false}}
 fn data(&self,hwnd:HWND)->NOTIFYICONDATAW{let mut n:NOTIFYICONDATAW=unsafe{std::mem::zeroed()};n.cbSize=std::mem::size_of_val(&n) as u32;n.hWnd=hwnd;n.uID=1;n}
 pub unsafe fn reload(&mut self,hwnd:HWND){unsafe{
  UnregisterHotKey(hwnd,1);self.settings=settings::Settings::load(&crate::paths::data_dir().join("settings.json"));
  let agent=std::env::var_os("KAGENT_TEST_ROOT").map(std::path::PathBuf::from).unwrap_or_else(||std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()).join("HwpAutoDocFit/kagent"));
  let h=&self.settings.hotkey;let parsed=hotkey::parse_hotkey(h);let same=settings::same_hotkey(h,&settings::agent_hotkey(&agent.join("settings.json")));
  self.ok=!h.is_empty()&&!same&&parsed.is_some_and(|(m,k)|RegisterHotKey(hwnd,1,m,k)!=0);
  self.error=if h.is_empty()||self.ok{String::new()}else if same{"몽땅클립보드와 같은 단축키입니다".into()}else{hotkey::hotkey_failure_body(h,parsed.is_some())};
  let (failed,notice)=hotkey::hotkey_failure_notice(self.failed.as_deref(),h,self.error.is_empty());self.failed=failed;
  let show=self.settings.tray||!self.error.is_empty();let mut n=self.data(hwnd);
  if show {n.uFlags=NIF_MESSAGE|NIF_ICON|NIF_TIP;n.uCallbackMessage=MESSAGE;n.hIcon=icon();copy(&mut n.szTip,&format!("가상 폴더 — {h}"));Shell_NotifyIconW(if self.shown{NIM_MODIFY}else{NIM_ADD},&n);DestroyIcon(n.hIcon);}else if self.shown{Shell_NotifyIconW(NIM_DELETE,&n);}self.shown=show;
  if notice&&!self.error.is_empty(){self.notice(hwnd,"가상 폴더 단축키를 쓸 수 없습니다",&self.error);}
 }}
 pub unsafe fn notice(&self,hwnd:HWND,title:&str,body:&str){unsafe{let mut n=self.data(hwnd);n.uFlags=NIF_INFO;n.dwInfoFlags=NIIF_WARNING;copy(&mut n.szInfoTitle,title);copy(&mut n.szInfo,body);Shell_NotifyIconW(NIM_MODIFY,&n);}}
 pub unsafe fn remove(&mut self,hwnd:HWND){unsafe{UnregisterHotKey(hwnd,1);if self.shown{Shell_NotifyIconW(NIM_DELETE,&self.data(hwnd));}self.shown=false;}}
}
fn copy<const N:usize>(dst:&mut [u16;N],s:&str){for(d,c)in dst.iter_mut().take(N-1).zip(s.encode_utf16()){*d=c;}}
unsafe fn icon()->HICON{unsafe{
 // 16x16 금색 폴더, kagent와 별개의 모양·색.
 let mut pixels=[0u8;16*16*4];for y in 3..13{for x in 1..15{if y>=5||x<8{let p=(y*16+x)*4;pixels[p..p+4].copy_from_slice(&[40,185,245,255]);}}}
 let mask=[0u8;32];CreateIcon(std::ptr::null_mut(),16,16,1,32,mask.as_ptr(),pixels.as_ptr())
}}
pub unsafe fn menu(hwnd:HWND)->u32{unsafe{let m=CreatePopupMenu();AppendMenuW(m,MF_STRING,1,wide("가상 폴더 열기").as_ptr());AppendMenuW(m,MF_STRING,2,wide("가상 폴더 끝내기").as_ptr());let mut p=POINT{x:0,y:0};GetCursorPos(&mut p);SetForegroundWindow(hwnd);let c=TrackPopupMenu(m,TPM_RETURNCMD|TPM_RIGHTBUTTON,p.x,p.y,0,hwnd,std::ptr::null());DestroyMenu(m);c as u32}}

#[cfg(test)] mod tests {
 use super::*;
 #[test]fn isolated_registration_conflict(){unsafe{
  // 시험 스레드에만 귀속되는 Ctrl+Alt+Shift+F24. 실제 기본 조합을 등록하지 않는다.
  let (mods,key)=crate::hotkey::parse_hotkey("Ctrl+Alt+Shift+F24").unwrap();
  assert_ne!((mods,key),crate::hotkey::parse_hotkey("Win+Shift+F").unwrap());
  assert_ne!(RegisterHotKey(std::ptr::null_mut(),0x4f51,mods,key),0);
  assert_eq!(RegisterHotKey(std::ptr::null_mut(),0x4f52,mods,key),0);
  let (s,once)=hotkey::hotkey_failure_notice(None,"Ctrl+Alt+Shift+F24",false);assert!(once);
  assert!(!hotkey::hotkey_failure_notice(s.as_deref(),"Ctrl+Alt+Shift+F24",false).1);
  assert_ne!(UnregisterHotKey(std::ptr::null_mut(),0x4f51),0);
 }}
 #[test]fn folder_icon_has_owned_handle(){unsafe{let i=icon();assert!(!i.is_null());assert_ne!(DestroyIcon(i),0);}}
}
