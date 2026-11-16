//! GDI 이중 버퍼·공용 frame·직접 키/IME 입력. Model은 event의 step 한 곳에서만 갱신한다.
use std::{cell::RefCell,ptr::{null,null_mut}};
use std::time::{SystemTime,UNIX_EPOCH};
use crate::{vf_state::{self,Model,View,Event,Effect,Caller,ReadResult,Context},vf_data::{self,Item,BoxRow,Snapshot},settings::Settings,util::wide,frame,vf_store};
use windows_sys::Win32::{Foundation::*,Graphics::Gdi::*,System::LibraryLoader::GetModuleHandleW,UI::{WindowsAndMessaging::*,Input::{KeyboardAndMouse::*,Ime::*}}};
const HEAD:i32=32;const TAB:i32=30;const ROW:i32=30;
const ADDRESS:i32=36;const TOOLBAR:i32=34;const STATUS:i32=26;const GUTTER:i32=6;
const SPACES:[Option<&str>;5]=[None,Some("core"),Some("basis"),Some("reference"),Some("temp")];
const LABELS:[&str;5]=["전체","핵심자료","기초자료","참고자료","임시저장소"];
#[derive(Default)]struct Ui{model:Model,hwnd:HWND,owner:HWND,boxes:Vec<BoxRow>,items:Vec<Item>,filtered:Vec<i64>,top:usize,committed:String,comp:String,pressed:Option<usize>,truncated:bool,drag_split:Option<crate::vf_layout::SplitHit>,active_panel:usize,address_edit:bool}
thread_local!{static UI:RefCell<Ui>=RefCell::new(Ui::default());}
fn with<R>(f:impl FnOnce(&Ui)->R)->R{UI.with(|s|f(&s.borrow()))}
fn edit<R>(f:impl FnOnce(&mut Ui)->R)->R{UI.with(|s|f(&mut s.borrow_mut()))}
pub fn snapshot()->(String,Option<i64>,usize,String){with(|s|(s.model.view.name().into(),s.model.view.context().and_then(|c|c.box_id),s.filtered.len(),match &s.model.view{View::Failed{error,..}=>error.clone(),_=>String::new()}))}
pub fn configure(owner:HWND,settings:&Settings){edit(|s|s.owner=owner);event(Event::SetSticky(settings.sticky));let h=with(|s|s.hwnd);if !h.is_null(){topmost(h);draw_later();}}
pub fn call(caller:Caller){event(Event::Call(caller));}
pub fn close(){event(Event::Close);}
pub fn destroy(){let h=with(|s|s.hwnd);if !h.is_null(){unsafe{DestroyWindow(h);}}}
fn notify(body:&str){let owner=with(|s|s.owner);crate::notice(owner,body);}
/// Model 대입은 이곳 한 군데뿐이다. Win32의 재진입 전에 RefCell 빌림을 끝낸다.
pub fn event(e:Event){
 if matches!(&e,Event::Close|Event::Escape|Event::Outside|Event::Minimize|Event::Call(_)|Event::Opened{..}){save();}
 let effects=edit(|s|{let(next,effects)=vf_state::step(&s.model,e);s.model=next;effects});
 for effect in effects{match effect{
  Effect::Show=>{if let Err(error)=ensure(){notify(&error);event(Event::Close);return;}let h=with(|s|s.hwnd);unsafe{ShowWindow(h,SW_SHOW);SetForegroundWindow(h);SetFocus(h);}},
  Effect::Foreground=>{let h=with(|s|s.hwnd);unsafe{SetForegroundWindow(h);SetFocus(h);}},
  Effect::Hide=>{save();let h=with(|s|s.hwnd);unsafe{ShowWindow(h,SW_HIDE);}},
  Effect::ClearItems=>edit(|s|{s.boxes=Vec::new();s.items=Vec::new();s.filtered=Vec::new();s.top=0;s.committed.clear();s.comp.clear();}),
  Effect::Read{seq,context}=>read(seq,context),
  Effect::Filter=>refilter(),Effect::Draw=>draw_later(),
  Effect::Notice(message)=>notify(&message),
  Effect::Open{seq,item_id}=>open_item(seq,item_id,crate::open::Action::Open),
  Effect::Locate{seq,item_id,origin}=>open_item(seq,item_id,if origin{crate::open::Action::LocateOrigin}else{crate::open::Action::LocateCopy}),
  Effect::PersistLayout(layout)=>{if let Err(error)=persist_layout(&layout){notify(&format!("패널 설정 저장 실패: {error}"));}},
  Effect::PersistVirtualOperation{seq,operation_id,kind}=>{let result=persist_virtual(&operation_id,&kind);event(Event::VirtualOperationCompleted{seq,result});},
  Effect::PersistVirtualHistory{seq,operation_id,redo}=>{let result=(||{let store=vf_store::Store::open(&store_path())?;if redo{store.redo(&operation_id)}else{store.undo(&operation_id)}})();event(Event::VirtualOperationCompleted{seq,result});},
 }}
 let(v,_,_,_)=snapshot();let _=crate::status(&v);
}
fn layout_path()->std::path::PathBuf{crate::paths::data_dir().join("settings.json")}
fn store_path()->std::path::PathBuf{crate::paths::data_dir().join("virtual.db")}
fn persist_layout(layout:&crate::vf_layout::Layout)->Result<(),String>{let p=layout_path();let mut value=Settings::load(&p).value();if let crate::json::Value::Obj(ref mut entries)=value{entries.retain(|(k,_)|!matches!(k.as_str(),"layout_version"|"panel_count"|"layout"|"active_panel"|"split"|"panels"));if let crate::json::Value::Obj(layout)=layout.value(){entries.extend(layout);}}crate::settings::write_changed(&p,&crate::json::to_string(&value)).map(|_|()).map_err(|e|e.to_string())}
fn load_layout()->crate::vf_layout::Layout{let p=layout_path();match std::fs::read_to_string(&p){Ok(text) if crate::json::parse(&text).and_then(|v|v.num("panel_count")).is_some()=>crate::vf_layout::Layout::parse(&text),Ok(text)=>crate::vf_layout::Layout::migrate_legacy(&text),Err(_)=>crate::vf_layout::Layout::default()}}
fn latest_operation_id()->String{vf_store::Store::open(&store_path()).ok().and_then(|s|s.journal().ok()).and_then(|v|v.first().map(|e|e.operation_id.split(":").next().unwrap_or(&e.operation_id).to_string())).unwrap_or_default()}fn persist_virtual(operation_id:&str,kind:&vf_state::VirtualOperationKind)->Result<(),String>{let store=vf_store::Store::open(&store_path())?;match kind{vf_state::VirtualOperationKind::Move{source_panel:_,target_panel,item_ids}=>store.move_many(operation_id,item_ids,target_panel),vf_state::VirtualOperationKind::Copy{source_panel:_,target_panel,item_ids}=>store.copy_many(operation_id,item_ids,target_panel,SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64)}}
fn read(seq:u64,mut c:Context){
 edit(|s|s.truncated=false);draw_later();let h=with(|s|s.hwnd);unsafe{UpdateWindow(h);}
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
 let h=CreateWindowExW(WS_EX_TOOLWINDOW,class.as_ptr(),wide("퀵가상폴더").as_ptr(),WS_POPUP|WS_THICKFRAME|WS_MAXIMIZEBOX,r[0],r[1],r[2],r[3],null_mut(),null_mut(),instance,null());
 if h.is_null(){return Err("목록 창 생성 실패".into());}edit(|s|{s.hwnd=h;s.model.explorer=load_layout();});frame::style_frame(h);topmost(h);Ok(())
 }}
pub fn window_title(name: &str) -> String { format!("퀵가상폴더 · {name} ▾") }
/// 테마 상태 전이(창 스레드): kagent 설정의 선호 + 시스템 → `ui_theme` 상태 하나. 색이 바뀌면 틀을 맞추고 다시 그린다.
/// 시작할 때·테마 메시지(시스템 변경, kagent의 DocFitThemeChanged 방송)·reload에서 부른다.
pub fn refresh_theme(){let change=ui_theme::refresh(crate::settings::agent_theme(&crate::paths::agent_settings()));let h=with(|s|s.hwnd);if change.look&&!h.is_null(){unsafe{frame::apply_theme(h);}draw_later();}}
fn place(saved:Option<[i32;4]>,mouse:[i32;2],work:RECT)->[i32;4]{let mut r=saved.unwrap_or([mouse[0]+12,mouse[1]+12,600,420]);r[2]=r[2].max(360).min((work.right-work.left).max(1));r[3]=r[3].max(160).min((work.bottom-work.top).max(1));r[0]=r[0].clamp(work.left,(work.right-r[2]).max(work.left));r[1]=r[1].clamp(work.top,(work.bottom-r[3]).max(work.top));r}
fn topmost(h:HWND){unsafe{SetWindowPos(h,if with(|s|s.model.sticky){HWND_TOPMOST}else{HWND_NOTOPMOST},0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE);}}
fn xy(l:isize)->(i32,i32){((l as u16)as i16 as i32,((l>>16)as u16)as i16 as i32)}
fn size(h:HWND)->RECT{unsafe{let mut r=std::mem::zeroed();GetClientRect(h,&mut r);r}}
fn list_y(h:HWND)->i32{frame::scale(h,HEAD+ADDRESS+TOOLBAR+TAB+if with(|s|s.model.view.context().is_some_and(|c|!c.query.is_empty())){ROW}else{0})}
fn row_at(h:HWND,l:isize)->Option<usize>{let (x,y)=xy(l);let rc=size(h);let c=with(|s|s.model.explorer.clone());let rects=c.panel_rects(rc.right,rc.bottom-frame::scale(h,STATUS)-list_y(h),frame::scale(h,GUTTER));let off=list_y(h);let panel=rects.iter().position(|r|x>=r.left&&x<r.right&&y>=off+r.top&&y<off+r.bottom)?;let panel_id=format!("p{}",panel+1);let changed=with(|s|s.model.explorer.active_panel!=panel_id);edit(|s|s.active_panel=panel);if changed{event(Event::ActivatePanel(panel_id));}with(|s|if c.panels.get(panel).is_some_and(|p|p.view=="icons"){let cw=frame::scale(h,120);let cols=(rects[panel].right-rects[panel].left).div_euclid(cw).max(1)as usize;let row=((y-off-rects[panel].top-frame::scale(h,ROW))/frame::scale(h,100)).max(0)as usize;Some(s.top+row*cols+((x-rects[panel].left)/cw).max(0)as usize)}else{frame::row_index(y,off+rects[panel].top+frame::scale(h,ROW),frame::scale(h,ROW),s.top,s.filtered.len())})}
fn button(h:HWND,x:i32,y:i32)->Option<usize>{let w=size(h).right;let bw=frame::scale(h,40);(y>=0&&y<frame::scale(h,HEAD)&&x>=w-4*bw&&x<w).then(||((w-1-x)/bw)as usize)}
fn choose(index:usize){let id=with(|s|s.filtered.get(index).copied());event(Event::Select(id));}
fn set_view_mode(icons:bool){let layout=edit(|s|{let id=s.model.explorer.active_panel.clone();if let Some(p)=s.model.explorer.panels.iter_mut().find(|p|p.id==id){p.view=if icons{"icons"}else{"details"}.into();}s.model.explorer.clone()});let _=persist_layout(&layout);draw_later();}
fn configure_panels(h:HWND,menu:bool){if !menu{return;}unsafe{let m=CreatePopupMenu();for(n,label)in [(1u8,"영역 1개"),(2,"좌우 2개"),(3,"좌1 + 우2"),(4,"2 × 2")]{AppendMenuW(m,MF_STRING|if with(|s|s.model.explorer.panel_count==n){MF_CHECKED}else{0},n as usize,wide(label).as_ptr());}let mut p=POINT{x:frame::scale(h,12),y:frame::scale(h,HEAD+ADDRESS+TOOLBAR)};ClientToScreen(h,&mut p);let cmd=TrackPopupMenu(m,TPM_RETURNCMD,p.x,p.y,0,h,null());DestroyMenu(m);if (1..=4).contains(&cmd){event(Event::SetPanelCount(cmd as u8));}}}
fn drag_split(h:HWND,x:i32,y:i32){let rc=size(h);let top=list_y(h);let gutter=frame::scale(h,GUTTER);let hit=with(|s|s.model.explorer.split_hit(rc.right,rc.bottom-frame::scale(h,STATUS)-top,gutter,x,y-top,frame::scale(h,8)));let Some(hit)=hit else{return};edit(|s|s.drag_split=Some(hit));unsafe{SetCapture(h);}}
fn update_split(h:HWND,x:i32,y:i32){let rc=size(h);let top=list_y(h);let gutter=frame::scale(h,GUTTER);let hh=(rc.bottom-frame::scale(h,STATUS)-top).max(1);let changed=edit(|s|{let Some(hit)=s.drag_split else{return false};let l=&mut s.model.explorer;let ratio=match hit{crate::vf_layout::SplitHit::Column(_)=>((x-gutter/2).clamp(1,(rc.right-1).max(1))as i64*10000/rc.right.max(1)as i64)as u16,crate::vf_layout::SplitHit::Row(_)=>((y-top-gutter/2).clamp(1,(hh-1).max(1))as i64*10000/hh as i64)as u16}.clamp(1,9999);match hit{crate::vf_layout::SplitHit::Column(_)=>l.column_splits=vec![ratio,10000-ratio],crate::vf_layout::SplitHit::Row(_)=>l.row_splits=vec![ratio,10000-ratio]};true});if changed{draw_later();}}
fn move_selection(h:HWND,delta:isize){let(rows,current,len)=with(|s|{let rows=((size(h).bottom-list_y(h))/frame::scale(h,ROW)).max(1)as usize;(rows,s.model.view.context().and_then(|c|c.selection).and_then(|id|s.filtered.iter().position(|v|*v==id)).unwrap_or(0),s.filtered.len())});if len==0{return;}let i=(current as isize+delta).clamp(0,len as isize-1)as usize;edit(|s|{if i<s.top{s.top=i;}if i>=s.top+rows{s.top=i+1-rows;}});choose(i);}
fn change_tab(delta:isize){let i=with(|s|SPACES.iter().position(|v|*v==s.model.view.context().and_then(|c|c.space.as_deref())).unwrap_or(0));edit(|s|{s.committed.clear();s.comp.clear();s.top=0;});event(Event::ChangeSpace(SPACES[(i as isize+delta).rem_euclid(5)as usize].map(String::from)));}
fn boxes_menu(h:HWND){unsafe{let m=CreatePopupMenu();with(|s|{for(i,b)in s.boxes.iter().enumerate(){AppendMenuW(m,MF_STRING,100+i,wide(&b.name).as_ptr());}});let mut pt=POINT{x:0,y:frame::scale(h,HEAD)};ClientToScreen(h,&mut pt);let n=TrackPopupMenu(m,TPM_RETURNCMD,pt.x,pt.y,0,h,null());DestroyMenu(m);if n>=100{let id=with(|s|s.boxes.get(n as usize-100).map(|b|b.id));if let Some(id)=id{event(Event::ChangeBox(id));}}}}
unsafe fn paint(h: HWND) {
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let dc = BeginPaint(h, &mut ps);
        let rc = size(h);
        if rc.right <= 0 || rc.bottom <= 0 {
            EndPaint(h, &ps);
            return;
        }

        let mem = CreateCompatibleDC(dc);
        let bmp = CreateCompatibleBitmap(dc, rc.right, rc.bottom);
        let old = SelectObject(mem, bmp);
        let font = CreateFontW(
            -frame::scale(h, 14), 0, 0, 0, 400, 0, 0, 0,
            DEFAULT_CHARSET as u32, 0, 0, 0, 0, wide("맑은 고딕").as_ptr(),
        );
        let oldfont = SelectObject(mem, font);
        SetBkMode(mem, TRANSPARENT as i32);

        let fill = |r: RECT, color| {
            let brush = CreateSolidBrush(color);
            FillRect(mem, &r, brush);
            DeleteObject(brush);
        };
        let text = |value: &str, mut r: RECT, flags| {
            let value = wide(value);
            DrawTextW(
                mem, value.as_ptr(), (value.len() - 1) as i32, &mut r,
                DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | flags,
            );
        };
        let palette = ui_theme::palette();
        fill(rc, palette.bg);
        SetTextColor(mem, palette.text);

        let head = frame::scale(h, HEAD);
        let address_h = frame::scale(h, ADDRESS);
        let toolbar_h = frame::scale(h, TOOLBAR);
        let tab_h = frame::scale(h, TAB);
        let row_h = frame::scale(h, ROW);
        let pad = frame::scale(h, 8);
        let button_w = frame::scale(h, 40);
        fill(RECT { bottom: head, ..rc }, palette.plain_head_bg);

        // Native title strip and window controls.
        let title_name = with(|s| {
            s.model.view.context().and_then(|c| c.box_id)
                .and_then(|id| s.boxes.iter().find(|b| b.id == id))
                .map(|b| b.name.clone()).unwrap_or_else(|| "오늘의 업무".into())
        });
        text(
            &window_title(&title_name),
            RECT { left: pad, top: 0, right: rc.right - frame::scale(h, 160), bottom: head },
            DT_END_ELLIPSIS,
        );
        let sticky = with(|s| s.model.sticky);
        for (index, label) in ["×", "□", "—", if sticky { "◆" } else { "◇" }].iter().enumerate() {
            let right = rc.right - index as i32 * button_w;
            text(*label, RECT { left: right - button_w, top: 0, right, bottom: head }, DT_CENTER);
        }

        let (address, active_space, icons) = with(|s| {
            let context = s.model.view.context();
            let panel = s.model.explorer.panels.iter()
                .find(|p| p.id == s.model.explorer.active_panel);
            let box_id = panel.and_then(|p| p.box_id).or_else(|| context.and_then(|c| c.box_id));
            let space = panel.and_then(|p| p.space.as_deref())
                .or_else(|| context.and_then(|c| c.space.as_deref()));
            let box_name = box_id.and_then(|id| s.boxes.iter().find(|b| b.id == id))
                .map(|b| b.name.as_str()).unwrap_or("오늘의 업무");
            let space_name = space.and_then(|v| SPACES.iter().position(|x| *x == Some(v)))
                .map(|i| LABELS[i]).unwrap_or("전체");
            let address = format!("상자 › {box_name} › 영역 › {space_name}");
            (address, space.map(str::to_owned), panel.is_some_and(|p| p.view == "icons"))
        });

        // Address row: the displayed path follows the active virtual panel.
        let address_top = head;
        let address_bottom = address_top + address_h;
        fill(RECT { left: 0, top: address_top, right: rc.right, bottom: address_bottom }, palette.bg);
        text("위치:", RECT { left: pad, top: address_top, right: frame::scale(h, 64), bottom: address_bottom }, DT_LEFT);
        text(
            &address,
            RECT { left: frame::scale(h, 64), top: address_top + frame::scale(h, 4), right: rc.right - frame::scale(h, 16), bottom: address_bottom - frame::scale(h, 4) },
            DT_END_ELLIPSIS,
        );

        // Toolbar buttons include the layout menu and both saved view modes.
        let toolbar_top = address_bottom;
        let toolbar_bottom = toolbar_top + toolbar_h;
        fill(RECT { left: 0, top: toolbar_top, right: rc.right, bottom: toolbar_bottom }, palette.plain_head_bg);
        for (index, label) in ["영역 ▾", "목록", "큰 아이콘", "새로고침"].iter().enumerate() {
            let left = pad + index as i32 * frame::scale(h, 106);
            let button = RECT {
                left, top: toolbar_top + frame::scale(h, 3),
                right: left + frame::scale(h, 104), bottom: toolbar_bottom - frame::scale(h, 3),
            };
            if (index == 1 && !icons) || (index == 2 && icons) {
                fill(button, palette.selected);
            }
            text(*label, button, DT_CENTER | DT_END_ELLIPSIS);
        }

        // Existing category tabs remain available below the explorer controls.
        let tab_top = toolbar_bottom;
        for (index, label) in LABELS.iter().enumerate() {
            let left = index as i32 * rc.right / LABELS.len() as i32;
            let right = (index as i32 + 1) * rc.right / LABELS.len() as i32;
            if SPACES[index] == active_space.as_deref() {
                fill(RECT { left, top: tab_top + tab_h - frame::scale(h, 3), right, bottom: tab_top + tab_h }, palette.accent_line);
            }
            text(*label, RECT { left, top: tab_top, right, bottom: tab_top + tab_h }, DT_CENTER | DT_END_ELLIPSIS);
        }

        let list_top = list_y(h);
        let (query, item_count, selected_id, message, truncated) = with(|s| {
            let context = s.model.view.context();
            let message = match &s.model.view {
                View::Loading(_) => "불러오는 중…".to_string(),
                View::Failed { error, .. } => error.clone(),
                View::Empty { reason, .. } => reason.clone(),
                _ if s.filtered.is_empty() => "표시할 항목이 없습니다. 검색어를 지우거나 F5로 새로고침하세요".into(),
                _ => String::new(),
            };
            (context.map(|c| c.query.clone()).unwrap_or_default(), s.filtered.len(),
             context.and_then(|c| c.selection), message, s.truncated)
        });
        if !query.is_empty() {
            text(
                &format!("검색: {query} · {item_count}개 · Esc로 지우기"),
                RECT { left: pad, top: tab_top + tab_h, right: rc.right - pad, bottom: list_top },
                DT_END_ELLIPSIS,
            );
        }
        if !message.is_empty() {
            text(&message, RECT { left: pad, top: list_top, right: rc.right - pad, bottom: list_top + row_h }, DT_END_ELLIPSIS);
        }

        // The same layout rectangles drive both panel rendering and split hit tests.
        with(|s| {
            let body_h = rc.bottom - frame::scale(h, STATUS) - list_top;
            let gutter = frame::scale(h, GUTTER);
            let layout = &s.model.explorer;
            let panel_rects = layout.panel_rects(rc.right, body_h, gutter);
            for split in layout.split_rects(rc.right, body_h, gutter) {
                fill(RECT { left: split.left, top: list_top + split.top, right: split.right, bottom: list_top + split.bottom }, palette.border);
            }
            for (panel_index, panel_rect) in panel_rects.iter().enumerate() {
                let panel = layout.panels.get(panel_index);
                let panel_id = format!("p{}", panel_index + 1);
                let panel_is_active = layout.active_panel == panel_id;
                let rect = RECT {
                    left: panel_rect.left, top: list_top + panel_rect.top,
                    right: panel_rect.right, bottom: list_top + panel_rect.bottom,
                };
                if panel_is_active { frame::own_border(mem, rect); }

                let box_id = panel.and_then(|p| p.box_id).or_else(|| if panel_is_active { s.model.view.context().and_then(|c| c.box_id) } else { None });
                let box_name = box_id.and_then(|id| s.boxes.iter().find(|b| b.id == id))
                    .map(|b| b.name.as_str()).unwrap_or("오늘의 업무");
                let panel_space = panel.and_then(|p| p.space.as_deref())
                    .or_else(|| if panel_is_active { s.model.view.context().and_then(|c| c.space.as_deref()) } else { None });
                let space_name = panel_space.and_then(|v| SPACES.iter().position(|x| *x == Some(v)))
                    .map(|i| LABELS[i]).unwrap_or("전체");
                text(
                    &format!("{box_name} › {space_name}"),
                    RECT { left: panel_rect.left + pad, top: rect.top + pad, right: panel_rect.right - pad, bottom: rect.top + row_h },
                    DT_END_ELLIPSIS,
                );

                let icons = panel.is_some_and(|p| p.view == "icons");
                if icons {
                    let cell_w = frame::scale(h, 120);
                    let cell_h = frame::scale(h, 100);
                    let columns = ((panel_rect.right - panel_rect.left) / cell_w).max(1);
                    let capacity = ((panel_rect.bottom - row_h).max(0) / cell_h) as usize * columns as usize;
                    for (index, id) in s.filtered.iter().enumerate().skip(s.top).take(capacity) {
                        if let Some(item) = s.items.iter().find(|item| item.id == *id) {
                            let offset = index - s.top;
                            let left = panel_rect.left + (offset as i32 % columns) * cell_w;
                            let top = list_top + panel_rect.top + row_h + (offset as i32 / columns) * cell_h;
                            let cell = RECT { left, top, right: (left + cell_w).min(panel_rect.right), bottom: (top + cell_h).min(rect.bottom) };
                            let selected = selected_id == Some(*id);
                            if selected { fill(cell, palette.selected); }
                            SetTextColor(mem, palette.row_text(selected, palette.text));
                            text("▣", RECT { left: cell.left, top: cell.top, right: cell.right, bottom: cell.top + frame::scale(h, 48) }, DT_CENTER);
                            text(&item.display_name, RECT { left: cell.left + pad, top: cell.top + frame::scale(h, 48), right: cell.right - pad, bottom: cell.bottom }, DT_CENTER | DT_END_ELLIPSIS);
                        }
                    }
                } else {
                    let capacity = ((panel_rect.bottom - row_h).max(0) / row_h) as usize;
                    for (index, id) in s.filtered.iter().enumerate().skip(s.top).take(capacity) {
                        if let Some(item) = s.items.iter().find(|item| item.id == *id) {
                            let top = list_top + panel_rect.top + row_h + (index - s.top) as i32 * row_h;
                            let row = RECT { left: panel_rect.left, top, right: panel_rect.right, bottom: top + row_h };
                            let selected = selected_id == Some(*id);
                            if selected { fill(row, palette.selected); }
                            SetTextColor(mem, palette.row_text(selected, palette.text));
                            let category = SPACES.iter().position(|space| *space == Some(item.space.as_str())).unwrap_or(0);
                            text(
                                &format!("[{}] {} · {}", item.kind, item.display_name, LABELS[category]),
                                RECT { left: row.left + pad, top: row.top, right: row.right - pad, bottom: row.bottom },
                                DT_END_ELLIPSIS,
                            );
                        }
                    }
                }
            }
        });

        SetTextColor(mem, palette.text);
        let status_top = rc.bottom - frame::scale(h, STATUS);
        fill(RECT { left: 0, top: status_top, right: rc.right, bottom: rc.bottom }, palette.plain_head_bg);
        let selected_count = usize::from(selected_id.is_some());
        let status_right = if truncated { rc.right - frame::scale(h, 220) } else { rc.right - pad };
        text(
            &format!("{item_count}개 항목 · {selected_count}개 선택 · {}", with(|s| format!("{}개 영역", s.model.explorer.panel_count))),
            RECT { left: pad, top: status_top, right: status_right, bottom: rc.bottom },
            DT_END_ELLIPSIS,
        );
        if truncated {
            text("5,000개 초과 항목은 표시하지 않습니다", RECT { left: rc.right - frame::scale(h, 220), top: status_top, right: rc.right - pad, bottom: rc.bottom }, DT_END_ELLIPSIS);
        }

        frame::own_border(mem, rc);
        BitBlt(dc, 0, 0, rc.right, rc.bottom, mem, 0, 0, SRCCOPY);
        SelectObject(mem, oldfont);
        DeleteObject(font);
        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
        EndPaint(h, &ps);
    }
}
unsafe extern "system" fn wndproc(h:HWND,msg:u32,w:usize,l:isize)->isize{unsafe{match msg{
 WM_PAINT=>{paint(h);0},WM_ERASEBKGND=>1,WM_NCCALCSIZE if w!=0=>0,
 WM_NCHITTEST=>{let(x,y)=xy(l);let mut p=POINT{x,y};ScreenToClient(h,&mut p);let r=size(h);frame::frame_hit(p.x,p.y,r.right,r.bottom,frame::scale(h,6),frame::scale(h,HEAD),p.x<frame::scale(h,260)||button(h,p.x,p.y).is_some(),IsZoomed(h)!=0,false).code()as isize},
 WM_NCLBUTTONDBLCLK if w==HTCAPTION as usize=>{ShowWindow(h,if IsZoomed(h)!=0{SW_RESTORE}else{SW_MAXIMIZE});0},
 WM_CLOSE=>{event(Event::Close);0},WM_ACTIVATE if (w&0xffff)as u32==WA_INACTIVE=>{let mut pid=0;GetWindowThreadProcessId(l as HWND,&mut pid);if pid!=std::process::id(){event(Event::Outside);}0},
 WM_GETMINMAXINFO=>{let p=&mut*(l as *mut MINMAXINFO);p.ptMinTrackSize=POINT{x:frame::scale(h,360),y:frame::scale(h,160)};0},
 WM_SIZE=>{draw_later();0},WM_EXITSIZEMOVE=>{save();0},WM_DESTROY=>{edit(|s|s.hwnd=null_mut());0},
 WM_LBUTTONDOWN=>{let(x,y)=xy(l);if y>=list_y(h){drag_split(h,x,y);}else{edit(|s|s.pressed=button(h,x,y));}0},
  WM_MOUSEMOVE=>{let(x,y)=xy(l);if with(|s|s.drag_split.is_some()){update_split(h,x,y);}else{let rc=size(h);let top=list_y(h);let hit=with(|s|s.model.explorer.split_hit(rc.right,rc.bottom-frame::scale(h,STATUS)-top,frame::scale(h,GUTTER),x,y-top,frame::scale(h,8)));if let Some(hit)=hit{let cursor=if matches!(hit,crate::vf_layout::SplitHit::Column(_)){32644usize}else{32645usize};SetCursor(LoadCursorW(null_mut(),cursor as *const u16));}else{SetCursor(LoadCursorW(null_mut(),IDC_ARROW));}}0},
  WM_LBUTTONUP=>{let(x,y)=xy(l);if with(|s|s.drag_split.is_some()){let layout=edit(|s|{s.drag_split.take().map(|_|s.model.explorer.clone())});ReleaseCapture();if let Some(layout)=layout{let _=persist_layout(&layout);}save();draw_later();return 0;}let pressed=edit(|s|s.pressed.take());let b=button(h,x,y);if b.is_some()||pressed.is_some(){if b==pressed{match b{Some(0)=>event(Event::Close),Some(1)=>{ShowWindow(h,if IsZoomed(h)!=0{SW_RESTORE}else{SW_MAXIMIZE});},Some(2)=>event(Event::Minimize),Some(3)=>{let on=with(|s|!s.model.sticky);event(Event::SetSticky(on));topmost(h);save();draw_later();},_=>{}}}}else if y<frame::scale(h,HEAD)&&x<frame::scale(h,260){boxes_menu(h);}else if y>=frame::scale(h,HEAD+ADDRESS)&&y<frame::scale(h,HEAD+ADDRESS+TOOLBAR){let idx=((x-frame::scale(h,8))/frame::scale(h,106)).clamp(0,3);match idx{0=>configure_panels(h,true),1=>set_view_mode(false),2=>set_view_mode(true),3=>event(Event::Refresh),_=>{}}draw_later();}else if y>=frame::scale(h,HEAD+ADDRESS+TOOLBAR)&&y<frame::scale(h,HEAD+ADDRESS+TOOLBAR+TAB){let i=(x*5/size(h).right.max(1)).clamp(0,4)as usize;edit(|s|{s.committed.clear();s.comp.clear();s.top=0;});event(Event::ChangeSpace(SPACES[i].map(String::from)));}else if let Some(i)=row_at(h,l){choose(i);}0},
 WM_LBUTTONDBLCLK=>{if let Some(i)=row_at(h,l){choose(i);event(Event::Open);}0},
 WM_RBUTTONUP=>{if let Some(i)=row_at(h,l){choose(i);item_menu(h,l);}0},
 WM_MOUSEWHEEL=>{move_selection(h,if ((w>>16)as u16)as i16>0{-3}else{3});0},
 WM_KEYDOWN=>{match w as u16{
 VK_ESCAPE=>{edit(|s|{s.committed.clear();s.comp.clear();});event(Event::Escape);},VK_F5=>event(Event::Refresh),VK_Z if GetKeyState(VK_CONTROL as i32)<0=>event(Event::RequestUndo{operation_id:latest_operation_id()}),VK_Y if GetKeyState(VK_CONTROL as i32)<0=>event(Event::RequestRedo{operation_id:latest_operation_id()}),
 VK_BACK=>{edit(|s|{s.committed.pop();s.comp.clear();});query();},VK_RETURN=>event(if GetKeyState(VK_CONTROL as i32)<0{Event::LocateCopy}else{Event::Open}),
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
 #[test]fn window_title_formats_correctly(){assert_eq!(window_title("상자 선택"),"퀵가상폴더 · 상자 선택 ▾");assert_eq!(window_title("업무"),"퀵가상폴더 · 업무 ▾");}
}

#[cfg(test)] mod integration_tests {
 use super::*;
 struct Isolated {root:std::path::PathBuf,old_root:Option<std::ffi::OsString>,old_library:Option<std::ffi::OsString>,mutex:HANDLE}
 impl Isolated {fn new()->Self{unsafe{
  let root=std::env::temp_dir().join(format!("kvf-window-test-{}",std::process::id()));std::fs::create_dir_all(&root).unwrap();
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
  assert!(self.root.starts_with(std::env::temp_dir()));let _=std::fs::remove_dir_all(&self.root);
 }}}
 #[test]fn isolated_window_events_and_rendering(){
  let fixture=Isolated::new();
  let db=crate::sqlite::Connection::open(fixture.root.join("library/library.db").to_str().unwrap()).unwrap();
  db.exec("CREATE TABLE box(id INTEGER PRIMARY KEY,name TEXT,description TEXT,created_at REAL,updated_at REAL); CREATE TABLE item(id INTEGER PRIMARY KEY,box_id INTEGER,kind TEXT,source_path TEXT,origin_path TEXT,managed_path TEXT,display_name TEXT,space TEXT,created_at REAL); INSERT INTO box VALUES(1,'시험 상자','',1,2),(2,'빈 상자','',1,1); INSERT INTO item VALUES(1,1,'file','','','','한글 보고','core',3),(2,1,'file','','','','한 자료','core',2),(3,1,'file','','','','영역 자료','basis',1)").unwrap();drop(db);
  let before_db=std::fs::read(fixture.root.join("library/library.db")).unwrap();
  configure(null_mut(),&Settings::default());assert!(with(|s|s.hwnd.is_null()));
  call(Caller::SecondRun);let h=with(|s|s.hwnd);assert!(!h.is_null());assert_eq!(snapshot().0,"list");assert_eq!(snapshot().2,3);
  let pinned=Settings{sticky:true,..Settings::default()};configure(null_mut(),&pinned);unsafe{assert_ne!(GetWindowLongPtrW(h,GWL_EXSTYLE)&WS_EX_TOPMOST as isize,0);}configure(null_mut(),&Settings::default());unsafe{assert_eq!(GetWindowLongPtrW(h,GWL_EXSTYLE)&WS_EX_TOPMOST as isize,0);}
  unsafe{SendMessageW(h,WM_ACTIVATE,WA_INACTIVE as usize,h as isize);}assert_eq!(snapshot().0,"list");
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
  assert_eq!(std::fs::read(fixture.root.join("library/library.db")).unwrap(),before_db);
  unsafe{crate::wndproc(null_mut(),WM_DESTROY,0,0);}assert!(!fixture.root.join("kvf/status.json").exists());
 }
}

fn open_item(seq:u64,id:i64,action:crate::open::Action){
 let item=with(|s|s.items.iter().find(|i|i.id==id).cloned());let h=with(|s|s.hwnd);
 let result=item.ok_or_else(||"자료가 옮겨졌거나 지워졌습니다".to_string()).and_then(|item|crate::open::open(h,&crate::paths::library_dir(),&item,action));
 event(Event::Opened{seq,result});
}
fn item_menu(h:HWND,l:isize){unsafe{
 let origin=with(|s|s.model.view.context().and_then(|c|c.selection).and_then(|id|s.items.iter().find(|i|i.id==id)).is_some_and(|i|!i.origin_path.is_empty()&&std::path::Path::new(&i.origin_path).exists()));
 let menu=CreatePopupMenu();AppendMenuW(menu,MF_STRING,1,wide("열기").as_ptr());AppendMenuW(menu,MF_STRING,2,wide("탐색기에서 위치 열기\tCtrl+Enter").as_ptr());if origin{AppendMenuW(menu,MF_STRING,3,wide("원본 위치 열기").as_ptr());}
 let(x,y)=xy(l);let mut p=POINT{x,y};ClientToScreen(h,&mut p);let command=TrackPopupMenu(menu,TPM_RETURNCMD|TPM_RIGHTBUTTON,p.x,p.y,0,h,null());DestroyMenu(menu);
 match command{1=>event(Event::Open),2=>event(Event::LocateCopy),3=>event(Event::LocateOrigin),_=>{}}
}}