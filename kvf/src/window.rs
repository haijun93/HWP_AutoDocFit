//! GDI 이중 버퍼·공용 frame·직접 키/IME 입력. Model은 event의 step 한 곳에서만 갱신한다.
use std::{cell::RefCell,ptr::{null,null_mut}};
use crate::{vf_state::{self,Model,View,Event,Effect,Caller,ReadResult,Context},vf_data::{self,Item,BoxRow,Snapshot},settings::Settings,util::wide,frame};
use windows_sys::Win32::{Foundation::*,Graphics::Gdi::*,System::LibraryLoader::GetModuleHandleW,UI::{WindowsAndMessaging::*,Input::{KeyboardAndMouse::*,Ime::*}}};
const HEAD:i32=32;const TAB:i32=30;const ROW:i32=30;
const SPACES:[Option<&str>;5]=[None,Some("core"),Some("basis"),Some("reference"),Some("temp")];
const LABELS:[&str;5]=["전체","핵심자료","기초자료","참고자료","임시저장소"];
#[derive(Default)]struct Ui{model:Model,hwnd:HWND,owner:HWND,boxes:Vec<BoxRow>,items:Vec<Item>,filtered:Vec<i64>,top:usize,committed:String,comp:String,pressed:Option<usize>,truncated:bool}
thread_local!{static UI:RefCell<Ui>=RefCell::new(Ui::default());}
fn with<R>(f:impl FnOnce(&Ui)->R)->R{UI.with(|s|f(&s.borrow()))}
fn edit<R>(f:impl FnOnce(&mut Ui)->R)->R{UI.with(|s|f(&mut s.borrow_mut()))}
pub fn snapshot()->(String,Option<i64>,usize,String){with(|s|(s.model.view.name().into(),s.model.view.context().and_then(|c|c.box_id),s.filtered.len(),match &s.model.view{View::Failed{error,..}=>error.clone(),_=>String::new()}))}
pub fn configure(owner:HWND,settings:&Settings){edit(|s|s.owner=owner);event(Event::SetSticky(settings.sticky));}
pub fn call(caller:Caller){event(Event::Call(caller));}
pub fn close(){event(Event::Close);}
pub fn destroy(){let h=with(|s|s.hwnd);if !h.is_null(){unsafe{DestroyWindow(h);}}}
fn notify(body:&str){let owner=with(|s|s.owner);crate::notice(owner,body);}
/// 唯一의 Model 대입. Win32의 재진입 전에 RefCell 빌림을 끝낸다.
pub fn event(e:Event){
 if matches!(&e,Event::Close|Event::Escape|Event::Outside|Event::Minimize|Event::Call(_)){save();}
 let effects=edit(|s|{let(next,effects)=vf_state::step(&s.model,e);s.model=next;effects});
 for effect in effects{match effect{
  Effect::Show=>{if let Err(error)=ensure(){notify(&error);event(Event::Close);return;}let h=with(|s|s.hwnd);unsafe{ShowWindow(h,SW_SHOW);SetForegroundWindow(h);SetFocus(h);}},
  Effect::Foreground=>{let h=with(|s|s.hwnd);unsafe{SetForegroundWindow(h);SetFocus(h);}},
  Effect::Hide=>{save();let h=with(|s|s.hwnd);unsafe{ShowWindow(h,SW_HIDE);}},
  Effect::ClearItems=>edit(|s|{s.boxes.clear();s.items=Vec::new();s.filtered=Vec::new();s.top=0;s.committed.clear();s.comp.clear();}),
  Effect::Read{seq,context}=>read(seq,context),
  Effect::Filter=>refilter(),Effect::Draw=>draw_later(),
  Effect::Notice(message)=>notify(&message),
  Effect::Open{seq,..}=>event(Event::Opened{seq,result:Err("열기는 V7에서 연결합니다".into())}),
 }}
 let(v,_,_,_)=snapshot();let _=crate::status(&v);
}
fn read(seq:u64,mut c:Context){
 draw_later();let h=with(|s|s.hwnd);unsafe{UpdateWindow(h);}
 if c.box_id.is_none(){c.box_id=Settings::load(&crate::paths::data_dir().join("settings.json")).last_box;}
 let result=match vf_data::read(&crate::paths::library_dir(),c.box_id){
  Ok(Snapshot::Loaded{boxes,box_id,items,truncated})=>{
   let empty=items.is_empty();edit(|s|{s.boxes=boxes;s.items=items;s.truncated=truncated;s.committed=c.query.clone();s.comp.clear();s.top=0;});
   // 빈 상자의 ID를 유지해야 상자 메뉴에서 다른 상자로 바꿀 수 있다.
   if empty{ReadResult::EmptyBox{box_id,reason:"항목이 없습니다. 다른 상자를 고르거나 앱에서 추가한 뒤 F5로 다시 불러오세요".into()}}else{ReadResult::Items{box_id}}
  },
  Ok(Snapshot::Empty(reason))=>{edit(|s|{s.boxes.clear();s.items.clear();s.filtered.clear();});ReadResult::Empty(match reason{vf_data::EmptyReason::NoDatabase=>"자료가 없습니다. 앱에서 자료를 추가한 뒤 F5로 다시 불러오세요",_=>"상자가 없습니다. 앱에서 상자를 만든 뒤 F5로 다시 불러오세요"}.into())},
  Err(e)=>{edit(|s|{s.items.clear();s.filtered.clear();});ReadResult::Failed(format!("{e} · F5로 다시"))}
 };event(Event::Loaded{seq,result});
}
fn refilter(){edit(|s|{let c=s.model.view.context().cloned().unwrap_or_default();s.filtered=vf_data::filter(&s.items,c.space.as_deref(),&c.query).iter().map(|i|i.id).collect();s.top=s.top.min(s.filtered.len().saturating_sub(1));});
 let (sel,first)=with(|s|(s.model.view.context().and_then(|c|c.selection),s.filtered.first().copied()));
 if !with(|s|sel.is_some_and(|id|s.filtered.contains(&id))){event(Event::Select(first));}
}
fn draw_later(){let h=with(|s|s.hwnd);if !h.is_null(){unsafe{InvalidateRect(h,null(),0);}}}
fn query(){let q=with(|s|format!("{}{}",s.committed,s.comp));event(Event::Query(q));}
fn save(){let h=with(|s|s.hwnd);if h.is_null(){return;}let p=crate::paths::data_dir().join("settings.json");let mut settings=Settings::load(&p);settings.sticky=with(|s|s.model.sticky);settings.last_box=with(|s|s.model.view.context().and_then(|c|c.box_id)).or(settings.last_box);
 unsafe{if IsZoomed(h)==0{let mut r:RECT=std::mem::zeroed();if GetWindowRect(h,&mut r)!=0{settings.rect=Some([r.left,r.top,r.right-r.left,r.bottom-r.top]);}}}
 if let Err(e)=settings.save(&p){notify(&format!("창 설정 저장 실패: {e}"));}
}
fn ensure()->Result<(),String>{
 if !with(|s|s.hwnd).is_null(){return Ok(());}unsafe{
 let root=std::env::var("KVF_TEST_ROOT").ok();let class=wide(&crate::paths::object_name("HwpAutoDocFit.KvfWindow",root.as_deref()));let instance=GetModuleHandleW(null());
 let wc=WNDCLASSW{style:CS_DBLCLKS,lpfnWndProc:Some(wndproc),hInstance:instance,hCursor:LoadCursorW(null_mut(),IDC_ARROW),lpszClassName:class.as_ptr(),..std::mem::zeroed()};
 if RegisterClassW(&wc)==0&&GetLastError()!=ERROR_CLASS_ALREADY_EXISTS{return Err("목록 창 등록 실패".into());}
 let settings=Settings::load(&crate::paths::data_dir().join("settings.json"));let mut pt=POINT{x:0,y:0};GetCursorPos(&mut pt);
 let mut monitor:MONITORINFO=std::mem::zeroed();monitor.cbSize=std::mem::size_of_val(&monitor) as u32;GetMonitorInfoW(MonitorFromPoint(pt,MONITOR_DEFAULTTONEAREST),&mut monitor);
 let r=place(settings.rect,[pt.x,pt.y],monitor.rcWork);
 let h=CreateWindowExW(WS_EX_TOOLWINDOW,class.as_ptr(),wide("가상 폴더").as_ptr(),WS_POPUP|WS_THICKFRAME|WS_MAXIMIZEBOX,r[0],r[1],r[2],r[3],null_mut(),null_mut(),instance,null());
 if h.is_null(){return Err("목록 창 생성 실패".into());}edit(|s|s.hwnd=h);frame::style_frame(h);topmost(h);Ok(())
 }}
fn place(saved:Option<[i32;4]>,mouse:[i32;2],work:RECT)->[i32;4]{let mut r=saved.unwrap_or([mouse[0]+12,mouse[1]+12,600,420]);r[2]=r[2].max(360).min((work.right-work.left).max(1));r[3]=r[3].max(160).min((work.bottom-work.top).max(1));r[0]=r[0].clamp(work.left,(work.right-r[2]).max(work.left));r[1]=r[1].clamp(work.top,(work.bottom-r[3]).max(work.top));r}
fn topmost(h:HWND){unsafe{SetWindowPos(h,if with(|s|s.model.sticky){HWND_TOPMOST}else{HWND_NOTOPMOST},0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE);}}
fn xy(l:isize)->(i32,i32){((l as u16)as i16 as i32,((l>>16)as u16)as i16 as i32)}
fn size(h:HWND)->RECT{unsafe{let mut r=std::mem::zeroed();GetClientRect(h,&mut r);r}}
fn list_y(h:HWND)->i32{frame::scale(h,HEAD+TAB+if with(|s|s.model.view.context().is_some_and(|c|!c.query.is_empty())){ROW}else{0})}
fn row_at(h:HWND,l:isize)->Option<usize>{let (_,y)=xy(l);with(|s|frame::row_index(y,list_y(h),frame::scale(h,ROW),s.top,s.filtered.len()))}
fn button(h:HWND,x:i32,y:i32)->Option<usize>{let w=size(h).right;let bw=frame::scale(h,40);(y>=0&&y<frame::scale(h,HEAD)&&x>=w-4*bw&&x<w).then(||((w-1-x)/bw)as usize)}
fn choose(index:usize){let id=with(|s|s.filtered.get(index).copied());event(Event::Select(id));}
fn move_selection(h:HWND,delta:isize){let(rows,current,len)=with(|s|{let rows=((size(h).bottom-list_y(h))/frame::scale(h,ROW)).max(1)as usize;(rows,s.model.view.context().and_then(|c|c.selection).and_then(|id|s.filtered.iter().position(|v|*v==id)).unwrap_or(0),s.filtered.len())});if len==0{return;}let i=(current as isize+delta).clamp(0,len as isize-1)as usize;edit(|s|{if i<s.top{s.top=i;}if i>=s.top+rows{s.top=i+1-rows;}});choose(i);}
fn change_tab(delta:isize){let i=with(|s|SPACES.iter().position(|v|*v==s.model.view.context().and_then(|c|c.space.as_deref())).unwrap_or(0));edit(|s|{s.committed.clear();s.comp.clear();s.top=0;});event(Event::ChangeSpace(SPACES[(i as isize+delta).rem_euclid(5)as usize].map(String::from)));}
fn boxes_menu(h:HWND){unsafe{let m=CreatePopupMenu();with(|s|{for(i,b)in s.boxes.iter().enumerate(){AppendMenuW(m,MF_STRING,100+i,wide(&b.name).as_ptr());}});let mut pt=POINT{x:0,y:frame::scale(h,HEAD)};ClientToScreen(h,&mut pt);let n=TrackPopupMenu(m,TPM_RETURNCMD,pt.x,pt.y,0,h,null());DestroyMenu(m);if n>=100{let id=with(|s|s.boxes.get(n as usize-100).map(|b|b.id));if let Some(id)=id{event(Event::ChangeBox(id));}}}}
unsafe fn paint(h:HWND){unsafe{
 let mut ps:PAINTSTRUCT=std::mem::zeroed();let dc=BeginPaint(h,&mut ps);let rc=size(h);if rc.right<=0||rc.bottom<=0{EndPaint(h,&ps);return;}
 let mem=CreateCompatibleDC(dc);let bmp=CreateCompatibleBitmap(dc,rc.right,rc.bottom);let old=SelectObject(mem,bmp);let font=CreateFontW(-frame::scale(h,14),0,0,0,400,0,0,0,DEFAULT_CHARSET as u32,0,0,0,0,wide("맑은 고딕").as_ptr());let oldfont=SelectObject(mem,font);SetBkMode(mem,TRANSPARENT as i32);
 let fill=|r:RECT,color|{let b=CreateSolidBrush(color);FillRect(mem,&r,b);DeleteObject(b);};
 let text=|s:&str,mut r:RECT,flags|{let v=wide(s);DrawTextW(mem,v.as_ptr(),(v.len()-1)as i32,&mut r,DT_SINGLELINE|DT_VCENTER|DT_NOPREFIX|flags);};
 fill(rc,0x00ffffff);let head=frame::scale(h,HEAD);let tab=frame::scale(h,TAB);let row=frame::scale(h,ROW);let pad=frame::scale(h,8);fill(RECT{bottom:head,..rc},0x00f4f4f4);
 with(|s|{
 let c=s.model.view.context();let name=c.and_then(|c|c.box_id).and_then(|id|s.boxes.iter().find(|b|b.id==id)).map(|b|b.name.as_str()).unwrap_or("상자 선택");SetTextColor(mem,0x00222222);text(&format!("가상 폴더 · {name} ▾"),RECT{left:pad,top:0,right:rc.right-frame::scale(h,160),bottom:head},DT_END_ELLIPSIS);
 for(i,label)in ["✕","□","―",if s.model.sticky{"◆"}else{"◇"}].iter().enumerate(){let right=rc.right-i as i32*frame::scale(h,40);text(label,RECT{left:right-frame::scale(h,40),top:0,right,bottom:head},DT_CENTER);}
 for(i,label)in LABELS.iter().enumerate(){let l=i as i32*rc.right/5;let r=(i+1)as i32*rc.right/5;let selected=SPACES[i]==c.and_then(|c|c.space.as_deref());if selected{fill(RECT{left:l,top:head+tab-frame::scale(h,3),right:r,bottom:head+tab},0x00cc7733);}text(label,RECT{left:l,top:head,right:r,bottom:head+tab},DT_CENTER|DT_END_ELLIPSIS);}
 let y=list_y(h);if let Some(c)=c.filter(|c|!c.query.is_empty()){text(&format!("찾기: {} · {}개 · Esc 지우기",c.query,s.filtered.len()),RECT{left:pad,top:head+tab,right:rc.right-pad,bottom:y},DT_END_ELLIPSIS);}
 let message=match &s.model.view{View::Loading(_)=>"불러오는 중",View::Failed{error,..}=>error,View::Empty{reason,..}=>reason,_ if s.filtered.is_empty()=>"항목이 없습니다. 다른 상자·영역을 고르거나 F5로 다시 불러오세요",_=>""};
 if !message.is_empty(){text(message,RECT{left:pad,top:y,right:rc.right-pad,bottom:y+row},DT_END_ELLIPSIS);}
 for (n,id) in s.filtered.iter().enumerate().skip(s.top).take(((rc.bottom-y)/row).max(0)as usize){if let Some(i)=s.items.iter().find(|i|i.id==*id){let top=y+(n-s.top)as i32*row;let r=RECT{left:0,top,right:rc.right,bottom:top+row};if c.and_then(|c|c.selection)==Some(*id){fill(r,0x00f7e3cf);}text(&format!("[{}] {} · {}",i.kind,i.display_name,LABELS[SPACES.iter().position(|v|*v==Some(i.space.as_str())).unwrap_or(0)]),RECT{left:pad,right:rc.right-pad,..r},DT_END_ELLIPSIS);}}
 if s.truncated{text("5,000개까지만 보입니다. 앱에서 보세요",RECT{left:pad,top:rc.bottom-row,right:rc.right,bottom:rc.bottom},DT_END_ELLIPSIS);}
 });frame::own_border(mem,rc);BitBlt(dc,0,0,rc.right,rc.bottom,mem,0,0,SRCCOPY);SelectObject(mem,oldfont);DeleteObject(font);SelectObject(mem,old);DeleteObject(bmp);DeleteDC(mem);EndPaint(h,&ps);
}}
unsafe extern "system" fn wndproc(h:HWND,msg:u32,w:usize,l:isize)->isize{unsafe{match msg{
 WM_PAINT=>{paint(h);0},WM_ERASEBKGND=>1,WM_NCCALCSIZE if w!=0=>0,
 WM_NCHITTEST=>{let(x,y)=xy(l);let mut p=POINT{x,y};ScreenToClient(h,&mut p);let r=size(h);frame::frame_hit(p.x,p.y,r.right,r.bottom,frame::scale(h,6),frame::scale(h,HEAD),p.x<frame::scale(h,260)||button(h,p.x,p.y).is_some(),IsZoomed(h)!=0,false).code()as isize},
 WM_NCLBUTTONDBLCLK if w==HTCAPTION as usize=>{ShowWindow(h,if IsZoomed(h)!=0{SW_RESTORE}else{SW_MAXIMIZE});0},
 WM_CLOSE=>{event(Event::Close);0},WM_ACTIVATE if (w&0xffff)as u32==WA_INACTIVE=>{event(Event::Outside);0},
 WM_GETMINMAXINFO=>{let p=&mut*(l as *mut MINMAXINFO);p.ptMinTrackSize=POINT{x:frame::scale(h,360),y:frame::scale(h,160)};0},
 WM_SIZE=>{draw_later();0},WM_EXITSIZEMOVE=>{save();0},
 WM_LBUTTONDOWN=>{let(x,y)=xy(l);edit(|s|s.pressed=button(h,x,y));0},
 WM_LBUTTONUP=>{let(x,y)=xy(l);let pressed=edit(|s|s.pressed.take());let b=button(h,x,y);if b.is_some()||pressed.is_some(){if b==pressed{match b{Some(0)=>event(Event::Close),Some(1)=>{ShowWindow(h,if IsZoomed(h)!=0{SW_RESTORE}else{SW_MAXIMIZE});},Some(2)=>event(Event::Minimize),Some(3)=>{let on=with(|s|!s.model.sticky);event(Event::SetSticky(on));topmost(h);save();draw_later();},_=>{}}}}else if y<frame::scale(h,HEAD)&&x<frame::scale(h,260){boxes_menu(h);}else if y<frame::scale(h,HEAD+TAB){let i=(x*5/size(h).right.max(1)).clamp(0,4)as usize;edit(|s|{s.committed.clear();s.comp.clear();s.top=0;});event(Event::ChangeSpace(SPACES[i].map(String::from)));}else if let Some(i)=row_at(h,l){choose(i);}0},
 WM_LBUTTONDBLCLK=>{if let Some(i)=row_at(h,l){choose(i);event(Event::Open);}0},
 WM_MOUSEWHEEL=>{move_selection(h,if ((w>>16)as u16)as i16>0{-3}else{3});0},
 WM_KEYDOWN=>{match w as u16{
 VK_ESCAPE=>{edit(|s|{s.committed.clear();s.comp.clear();});event(Event::Escape);},VK_F5=>event(Event::Refresh),
 VK_BACK=>{edit(|s|{s.committed.pop();s.comp.clear();});query();},VK_RETURN=>event(Event::Open),
 VK_LEFT=>change_tab(-1),VK_RIGHT=>change_tab(1),VK_TAB=>change_tab(if GetKeyState(VK_SHIFT as i32)<0{-1}else{1}),
 VK_UP=>move_selection(h,-1),VK_DOWN=>move_selection(h,1),VK_PRIOR=>move_selection(h,-(((size(h).bottom-list_y(h))/frame::scale(h,ROW)).max(1)as isize)),VK_NEXT=>move_selection(h,((size(h).bottom-list_y(h))/frame::scale(h,ROW)).max(1)as isize),VK_HOME=>move_selection(h,-100000),VK_END=>move_selection(h,100000),_=>{}}
 0},
 WM_CHAR=>{if let Some(ch)=char::from_u32(w as u32).filter(|c|!c.is_control()){edit(|s|{if s.committed.chars().count()<256{s.committed.push(ch);}s.comp.clear();});query();}0},
 WM_IME_COMPOSITION=>{if l as u32&GCS_COMPSTR!=0{let imc=ImmGetContext(h);if !imc.is_null(){let n=ImmGetCompositionStringW(imc,GCS_COMPSTR,null_mut(),0).max(0)as usize;let mut buf=vec![0u16;n/2];if n>0{ImmGetCompositionStringW(imc,GCS_COMPSTR,buf.as_mut_ptr()as _,n as u32);}ImmReleaseContext(h,imc);edit(|s|s.comp=String::from_utf16_lossy(&buf));query();}}DefWindowProcW(h,msg,w,l)},
 WM_IME_ENDCOMPOSITION=>{edit(|s|s.comp.clear());query();DefWindowProcW(h,msg,w,l)},
 _=>DefWindowProcW(h,msg,w,l)
}}}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn saved_offscreen_clamps(){assert_eq!(place(Some([9999,-9999,600,420]),[0,0],RECT{left:0,top:0,right:1920,bottom:1080}),[1320,0,600,420]);}
 #[test]fn first_position_near_mouse(){assert_eq!(place(None,[100,100],RECT{left:0,top:0,right:1920,bottom:1080}),[112,112,600,420]);}
 #[test]fn tabs_match_data_spaces(){assert_eq!(SPACES,[None,Some("core"),Some("basis"),Some("reference"),Some("temp")]);}
}

#[cfg(test)] mod integration_tests {
 use super::*;
 struct Isolated {root:std::path::PathBuf,old_root:Option<std::ffi::OsString>,old_library:Option<std::ffi::OsString>,mutex:HANDLE}
 impl Isolated {fn new()->Self{unsafe{
  let root=std::path::PathBuf::from(format!("E:/docfit-tmp/kvf-window-test-{}",std::process::id()));std::fs::create_dir_all(&root).unwrap();
  let old_root=std::env::var_os("KVF_TEST_ROOT");let old_library=std::env::var_os("KLIB_TEST_ROOT");
  std::env::set_var("KVF_TEST_ROOT",root.join("kvf"));std::env::set_var("KLIB_TEST_ROOT",root.join("library"));std::fs::create_dir_all(root.join("library")).unwrap();
  let name=wide(&crate::paths::object_name("Local\\HwpAutoDocFit.kvf",Some(root.to_str().unwrap())));let mutex=windows_sys::Win32::System::Threading::CreateMutexW(null(),0,name.as_ptr());assert!(!mutex.is_null());
  Self{root,old_root,old_library,mutex}
 }}}
 impl Drop for Isolated{fn drop(&mut self){unsafe{
  close();destroy();CloseHandle(self.mutex);
  match &self.old_root{Some(v)=>std::env::set_var("KVF_TEST_ROOT",v),None=>std::env::remove_var("KVF_TEST_ROOT")};
  match &self.old_library{Some(v)=>std::env::set_var("KLIB_TEST_ROOT",v),None=>std::env::remove_var("KLIB_TEST_ROOT")};
  // 고정된 시험 디렉터리만 정리한다.
  assert!(self.root.starts_with("E:/docfit-tmp"));let _=std::fs::remove_dir_all(&self.root);
 }}}
 #[test]fn isolated_window_events_and_rendering(){
  let fixture=Isolated::new();
  let db=crate::sqlite::Connection::open(fixture.root.join("library/library.db").to_str().unwrap()).unwrap();
  db.exec("CREATE TABLE box(id INTEGER PRIMARY KEY,name TEXT,description TEXT,created_at REAL,updated_at REAL); CREATE TABLE item(id INTEGER PRIMARY KEY,box_id INTEGER,kind TEXT,source_path TEXT,origin_path TEXT,managed_path TEXT,display_name TEXT,space TEXT,created_at REAL); INSERT INTO box VALUES(1,'시험 상자','',1,2),(2,'빈 상자','',1,1); INSERT INTO item VALUES(1,1,'file','','','','한글 보고','core',3),(2,1,'file','','','','한 자료','core',2),(3,1,'file','','','','영역 자료','basis',1)").unwrap();drop(db);
  configure(null_mut(),&Settings::default());assert!(with(|s|s.hwnd.is_null()));
  call(Caller::SecondRun);let h=with(|s|s.hwnd);assert!(!h.is_null());assert_eq!(snapshot().0,"list");assert_eq!(snapshot().2,3);
  unsafe{assert_ne!(IsWindowVisible(h),0);UpdateWindow(h);SendMessageW(h,WM_KEYDOWN,VK_RIGHT as usize,0);}
  assert_eq!(snapshot().2,2);unsafe{SendMessageW(h,WM_KEYDOWN,VK_RIGHT as usize,0);}assert_eq!(snapshot().2,1);
  unsafe{SendMessageW(h,WM_KEYDOWN,VK_LEFT as usize,0);SendMessageW(h,WM_CHAR,'한' as usize,0);}assert_eq!(snapshot().2,2);
  // 실제 IME 관측 결과와 같은 입력 경로. IME 엔진의 키 합성 자체는 실기 확인 대상.
  edit(|s|s.comp="글".into());query();assert_eq!(snapshot().2,1);assert_eq!(with(|s|s.model.view.context().unwrap().query.clone()),"한글");
  unsafe{SendMessageW(h,WM_IME_ENDCOMPOSITION,0,0);SendMessageW(h,WM_CHAR,'글' as usize,0);UpdateWindow(h);}assert_eq!(snapshot().2,1);
  let context=with(|s|s.model.view.context().cloned().unwrap());event(Event::Minimize);assert_eq!(snapshot().0,"minimized");assert_eq!(snapshot().2,1);call(Caller::Tray);assert_eq!(snapshot().0,"search");assert_eq!(with(|s|s.model.view.context().cloned().unwrap()),context);
  event(Event::Escape);assert_eq!(snapshot().0,"list");event(Event::ChangeBox(2));assert_eq!(snapshot().0,"empty");assert_eq!(snapshot().1,Some(2));event(Event::ChangeBox(1));assert_eq!(snapshot().0,"list");
  event(Event::SetSticky(true));event(Event::Outside);assert_eq!(snapshot().0,"list");call(Caller::Hotkey);assert_eq!(snapshot().0,"hidden");assert_eq!(with(|s|s.items.len()),0);assert_eq!(snapshot().2,0);
  let status=std::fs::read_to_string(fixture.root.join("kvf/status.json")).unwrap();assert_eq!(crate::json::parse(&status).unwrap().str("view"),Some("hidden"));
 }
}
